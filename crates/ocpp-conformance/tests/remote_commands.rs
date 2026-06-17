//! End-to-end CS→CP command test for RemoteStart/RemoteStopTransaction (Issue #49).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the two M5 remote commands through the
//! `OcppServer::remote_start_transaction` / `remote_stop_transaction` helpers,
//! asserting the CP answers with faithful `RemoteStartStopStatus` semantics
//! (OCPP 1.6J §5.11–5.12).
//!
//! Rust counterpart of the Python reference's central-system driving of these
//! commands ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against an `@on('RemoteStartTransaction')` charge point
//! ([`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py)).
//!
//! Note: this exercises the *command/response* leg. Having an `Accepted`
//! RemoteStart actually drive the CP's local `StartTransaction` (and RemoteStop
//! end it) is the follow-up slice noted on Issue #49.

use std::net::SocketAddr;
use std::sync::Arc;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    AuthorizeRequest, AuthorizeResponse, BootNotificationRequest, BootNotificationResponse,
    HeartbeatRequest, HeartbeatResponse, MeterValuesRequest, MeterValuesResponse,
    RegistrationStatus, StartTransactionRequest, StartTransactionResponse,
    StatusNotificationRequest, StatusNotificationResponse, StopTransactionRequest,
    StopTransactionResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::common::{AuthorizationStatus, IdTagInfo};
use ocpp_types::v16j::{ChargePointStatus, RemoteStartStopStatus};
use ocpp_types::ConnectorId;

/// The transaction id the CSMS hands out for StartTransaction, matching the
/// fixed id the Python reference's example CSMS returns.
const TXN_ID: i32 = 42;

fn accepted() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// A CSMS dispatcher that accepts every CP-originated action the boot +
/// transaction flow needs, so the CP can reach the states this test drives.
fn csms_dispatcher() -> ActionDispatcher {
    let mut d = ActionDispatcher::new();

    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: chrono::Utc::now(),
            // A long interval keeps stray heartbeats from racing the assertions.
            interval: 300,
            status: RegistrationStatus::Accepted,
        })
    });
    d.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: chrono::Utc::now(),
        })
    });
    d.on(|_req: StatusNotificationRequest| async move { Ok(StatusNotificationResponse {}) });
    d.on(|_req: AuthorizeRequest| async move {
        Ok(AuthorizeResponse {
            id_tag_info: accepted(),
        })
    });
    d.on(|_req: StartTransactionRequest| async move {
        Ok(StartTransactionResponse {
            id_tag_info: accepted(),
            transaction_id: TXN_ID,
        })
    });
    d.on(|_req: MeterValuesRequest| async move { Ok(MeterValuesResponse {}) });
    d.on(|_req: StopTransactionRequest| async move {
        Ok(StopTransactionResponse { id_tag_info: None })
    });

    d
}

async fn start_csms(dispatcher: ActionDispatcher) -> (OcppServer, SocketAddr) {
    let handler = Arc::new(DispatchHandler::new(Arc::new(dispatcher)));
    let (mut server, _events) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr)
}

fn cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        ..ChargePointConfig::default()
    }
}

#[tokio::test]
async fn csms_drives_remote_start_and_stop_with_faithful_status() {
    let cp_id = "CP_REMOTE_01";
    let (mut server, addr) = start_csms(csms_dispatcher()).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    let connector = ConnectorId::new(1).unwrap();

    // 1. RemoteStart on the free connector 1 → Accepted (§5.11).
    let status = server
        .remote_start_transaction(cp_id, "TAG_001", Some(1))
        .await
        .expect("remote start resolves");
    assert_eq!(
        status,
        RemoteStartStopStatus::Accepted,
        "free connector accepts a remote start"
    );

    // 2. Start a transaction locally so connector 1 is now busy and the CP has a
    //    known active transaction id to stop later.
    let txn_id = cp
        .start_transaction(connector, "TAG_001", 0)
        .await
        .expect("start transaction");
    assert_eq!(txn_id, TXN_ID, "CP adopts the CSMS-assigned transaction id");
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Charging,
        "connector charges once the transaction starts"
    );

    // 3. RemoteStart on the now-busy connector → Rejected (§5.11).
    let status = server
        .remote_start_transaction(cp_id, "TAG_001", Some(1))
        .await
        .expect("remote start resolves");
    assert_eq!(
        status,
        RemoteStartStopStatus::Rejected,
        "a busy connector rejects a remote start"
    );

    // 4. RemoteStop the known transaction → Accepted (§5.12).
    let status = server
        .remote_stop_transaction(cp_id, txn_id)
        .await
        .expect("remote stop resolves");
    assert_eq!(
        status,
        RemoteStartStopStatus::Accepted,
        "the CP accepts a stop for a transaction it is running"
    );

    // 5. RemoteStop an unknown transaction id → Rejected (§5.12).
    let status = server
        .remote_stop_transaction(cp_id, 9999)
        .await
        .expect("remote stop resolves");
    assert_eq!(
        status,
        RemoteStartStopStatus::Rejected,
        "the CP rejects a stop for an unknown transaction id"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
