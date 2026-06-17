//! End-to-end side-effect test for RemoteStart/RemoteStopTransaction (Issue #55).
//!
//! The command/response leg (faithful `RemoteStartStopStatus`) is covered by
//! `remote_commands.rs`. This test exercises the *side effect*: an `Accepted`
//! `RemoteStartTransaction` must make the CP actually start charging (drive its
//! local `StartTransaction`, connector → `Charging`), and an `Accepted`
//! `RemoteStopTransaction` must end the matching transaction (`StopTransaction`,
//! reason `Remote`, connector → `Available`) — OCPP 1.6J §5.11–5.12.
//!
//! Rust counterpart of the Python reference's `@on`/`@after('RemoteStartTransaction')`
//! split ([`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py)):
//! the `@on` handler returns the status, the `@after` effect performs the start.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

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
use ocpp_types::common::{AuthorizationStatus, IdTagInfo, Reason};
use ocpp_types::v16j::{ChargePointStatus, RemoteStartStopStatus};
use ocpp_types::ConnectorId;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// The transaction id the CSMS hands out, matching the fixed id the Python
/// reference's example CSMS returns.
const TXN_ID: i32 = 42;

/// Bound on how long a queued side effect may take to reach the CSMS before the
/// test gives up. Generous so a loaded CI box doesn't flake.
const SIDE_EFFECT_TIMEOUT: Duration = Duration::from_secs(5);

fn accepted() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// What the CSMS observed for a `StartTransaction` CALL the CP sent.
#[derive(Debug)]
struct StartObserved {
    connector_id: u32,
    id_tag: String,
}

/// What the CSMS observed for a `StopTransaction` CALL the CP sent.
#[derive(Debug)]
struct StopObserved {
    transaction_id: i32,
    reason: Option<Reason>,
}

/// A CSMS dispatcher that records the `StartTransaction` / `StopTransaction`
/// CALLs the CP sends, so the test can assert the remote-command side effects
/// actually fired (not just that the command was acknowledged).
fn recording_csms_dispatcher(
    start_tx: mpsc::UnboundedSender<StartObserved>,
    stop_tx: mpsc::UnboundedSender<StopObserved>,
) -> ActionDispatcher {
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
    {
        let start_tx = start_tx.clone();
        d.on(move |req: StartTransactionRequest| {
            let start_tx = start_tx.clone();
            async move {
                let _ = start_tx.send(StartObserved {
                    connector_id: req.connector_id,
                    id_tag: req.id_tag,
                });
                Ok(StartTransactionResponse {
                    id_tag_info: accepted(),
                    transaction_id: TXN_ID,
                })
            }
        });
    }
    d.on(|_req: MeterValuesRequest| async move { Ok(MeterValuesResponse {}) });
    {
        let stop_tx = stop_tx.clone();
        d.on(move |req: StopTransactionRequest| {
            let stop_tx = stop_tx.clone();
            async move {
                let _ = stop_tx.send(StopObserved {
                    transaction_id: req.transaction_id,
                    reason: req.reason,
                });
                Ok(StopTransactionResponse { id_tag_info: None })
            }
        });
    }

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

/// Poll the connector until it reaches `want`, or fail after `SIDE_EFFECT_TIMEOUT`.
///
/// The side effect runs asynchronously on the CP's command-consumer task, so the
/// connector transition lands a beat after the CSMS observes the CALL. Polling
/// avoids both a racy bare assert and an arbitrary fixed sleep.
async fn wait_for_status(cp: &ChargePoint, connector: ConnectorId, want: ChargePointStatus) {
    let poll = async {
        loop {
            let status = cp
                .get_connector(connector)
                .await
                .expect("connector exists")
                .status()
                .await;
            if status == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    timeout(SIDE_EFFECT_TIMEOUT, poll)
        .await
        .unwrap_or_else(|_| panic!("connector did not reach {want:?} in time"));
}

#[tokio::test]
async fn accepted_remote_start_then_stop_drive_local_transaction() {
    let cp_id = "CP_REMOTE_SIDE_EFFECT_01";
    let (start_tx, mut start_rx) = mpsc::unbounded_channel();
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(start_tx, stop_tx)).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    let connector = ConnectorId::new(1).unwrap();
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Available,
        "connector starts out free"
    );

    // 1. RemoteStart the free connector 1 → Accepted (§5.11).
    let status = server
        .remote_start_transaction(cp_id, "TAG_001", Some(1))
        .await
        .expect("remote start resolves");
    assert_eq!(
        status,
        RemoteStartStopStatus::Accepted,
        "free connector accepts a remote start"
    );

    // 2. SIDE EFFECT: the CP must actually start a transaction — the CSMS sees a
    //    StartTransaction for connector 1 with the requested id tag.
    let observed = timeout(SIDE_EFFECT_TIMEOUT, start_rx.recv())
        .await
        .expect("CSMS observes a StartTransaction after an accepted remote start")
        .expect("start channel open");
    assert_eq!(
        observed.connector_id, 1,
        "started on the requested connector"
    );
    assert_eq!(
        observed.id_tag, "TAG_001",
        "started with the requested id tag"
    );

    // ...and the connector ends up Charging.
    wait_for_status(&cp, connector, ChargePointStatus::Charging).await;

    // 3. RemoteStop the now-running transaction → Accepted (§5.12).
    let status = server
        .remote_stop_transaction(cp_id, TXN_ID)
        .await
        .expect("remote stop resolves");
    assert_eq!(
        status,
        RemoteStartStopStatus::Accepted,
        "the CP accepts a stop for the transaction it is running"
    );

    // 4. SIDE EFFECT: the CP must actually end the transaction — the CSMS sees a
    //    StopTransaction for TXN_ID with reason Remote.
    let observed = timeout(SIDE_EFFECT_TIMEOUT, stop_rx.recv())
        .await
        .expect("CSMS observes a StopTransaction after an accepted remote stop")
        .expect("stop channel open");
    assert_eq!(
        observed.transaction_id, TXN_ID,
        "stopped the running transaction"
    );
    assert_eq!(
        observed.reason,
        Some(Reason::Remote),
        "a remote-initiated stop reports reason Remote"
    );

    // ...and the connector is free again.
    wait_for_status(&cp, connector, ChargePointStatus::Available).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
