//! End-to-end CS→CP ReserveNow / CancelReservation test (Issue #71).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the M5 reservation commands (OCPP 1.6J §5.14 ReserveNow,
//! §5.4 CancelReservation) through the `OcppServer::reserve_now` /
//! `cancel_reservation` helpers, asserting the CP both answers the right status
//! *and* drives the matching connector-state side effect off the inbound-CALL
//! path (a `StatusNotification` reporting the new connector status):
//!
//!   1. `reserve_now(connector 1, free)` → `Accepted`; the connector becomes
//!      `Reserved` and the CSMS observes `StatusNotification(1, Reserved)`.
//!   2. `reserve_now(connector 2, busy)` → `Occupied` (connector 2 is mid
//!      transaction), and nothing changes.
//!   3. `cancel_reservation(held id)` → `Accepted`; connector 1 returns to
//!      `Available` and the CSMS observes `StatusNotification(1, Available)`.
//!   4. `cancel_reservation(unknown id)` → `Rejected`.
//!
//! Rust counterpart of the Python reference's central system driving
//! `ReserveNow` / `CancelReservation`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against the default `@on('ReserveNow')` / `@on('CancelReservation')` charge point.

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
use ocpp_types::common::{AuthorizationStatus, IdTagInfo};
use ocpp_types::v16j::{
    CancelReservationStatus, ChargePointStatus, RemoteStartStopStatus, ReservationStatus,
};
use ocpp_types::ConnectorId;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// The transaction id the CSMS hands out for the busy-connector setup.
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

/// A `StatusNotification` CALL the CSMS observed from the CP.
#[derive(Debug, Clone)]
struct StatusObserved {
    connector_id: u32,
    status: ChargePointStatus,
}

/// A CSMS dispatcher that records every `StatusNotification` the CP sends, so the
/// test can assert the reservation side effects actually fired (not just that the
/// command was acknowledged). It also answers the handshake and the
/// transaction CALLs needed to drive a connector into a genuinely busy state.
fn recording_csms_dispatcher(status_tx: mpsc::UnboundedSender<StatusObserved>) -> ActionDispatcher {
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
    {
        let status_tx = status_tx.clone();
        d.on(move |req: StatusNotificationRequest| {
            let status_tx = status_tx.clone();
            async move {
                let _ = status_tx.send(StatusObserved {
                    connector_id: req.connector_id,
                    status: req.status,
                });
                Ok(StatusNotificationResponse {})
            }
        });
    }
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
        // Two connectors: one stays free (reservable), one is driven busy.
        connector_count: 2,
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        ..ChargePointConfig::default()
    }
}

/// Poll the connector until it reaches `want`, or fail after `SIDE_EFFECT_TIMEOUT`.
///
/// The reservation side effect runs asynchronously on the CP's command-consumer
/// task, so the connector transition lands a beat after the CALLRESULT. Polling
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

/// Wait for the CSMS to observe a `StatusNotification(connector, status)`,
/// skipping any unrelated reports, or fail after `SIDE_EFFECT_TIMEOUT`.
async fn expect_status(
    rx: &mut mpsc::UnboundedReceiver<StatusObserved>,
    connector_id: u32,
    status: ChargePointStatus,
) {
    let poll = async {
        loop {
            let observed = rx.recv().await.expect("status channel open");
            if observed.connector_id == connector_id && observed.status == status {
                return;
            }
        }
    };
    timeout(SIDE_EFFECT_TIMEOUT, poll)
        .await
        .unwrap_or_else(|_| {
            panic!("CSMS never observed StatusNotification({connector_id}, {status:?})")
        });
}

#[tokio::test]
async fn csms_reserve_now_and_cancel_reservation_drive_connector_state() {
    let cp_id = "CP_RESERVE_01";
    let (status_tx, mut status_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(status_tx)).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    let connector1 = ConnectorId::new(1).unwrap();
    let connector2 = ConnectorId::new(2).unwrap();

    // 1. Reserve the free connector 1 → Accepted (§5.14).
    let res_id_1 = 1001;
    let status = server
        .reserve_now(
            cp_id,
            1,
            "TAG-RES-01",
            res_id_1,
            chrono::Utc::now() + chrono::Duration::hours(1),
            None,
        )
        .await
        .expect("reserve_now resolves");
    assert_eq!(
        status,
        ReservationStatus::Accepted,
        "a free connector accepts a reservation"
    );

    // SIDE EFFECT: connector 1 becomes Reserved and the CSMS is told.
    wait_for_status(&cp, connector1, ChargePointStatus::Reserved).await;
    expect_status(&mut status_rx, 1, ChargePointStatus::Reserved).await;

    // 2. Drive connector 2 genuinely busy via an accepted remote start, then try
    //    to reserve it → Occupied (§5.14).
    let remote = server
        .remote_start_transaction(cp_id, "TAG-DRIVE-02", Some(2))
        .await
        .expect("remote start resolves");
    assert_eq!(remote, RemoteStartStopStatus::Accepted);
    wait_for_status(&cp, connector2, ChargePointStatus::Charging).await;

    let status = server
        .reserve_now(
            cp_id,
            2,
            "TAG-RES-02",
            1002,
            chrono::Utc::now() + chrono::Duration::hours(1),
            None,
        )
        .await
        .expect("reserve_now resolves");
    assert_eq!(
        status,
        ReservationStatus::Occupied,
        "a connector with an active transaction is Occupied"
    );
    assert_eq!(
        cp.get_connector(connector2).await.unwrap().status().await,
        ChargePointStatus::Charging,
        "a rejected reservation must not disturb the busy connector"
    );

    // 3. Cancel the held reservation on connector 1 → Accepted (§5.4).
    let status = server
        .cancel_reservation(cp_id, res_id_1)
        .await
        .expect("cancel_reservation resolves");
    assert_eq!(
        status,
        CancelReservationStatus::Accepted,
        "a held reservation can be cancelled"
    );

    // SIDE EFFECT: connector 1 returns to Available and the CSMS is told.
    wait_for_status(&cp, connector1, ChargePointStatus::Available).await;
    expect_status(&mut status_rx, 1, ChargePointStatus::Available).await;

    // 4. Cancel an unknown reservation id → Rejected (§5.4).
    let status = server
        .cancel_reservation(cp_id, 9999)
        .await
        .expect("cancel_reservation resolves");
    assert_eq!(
        status,
        CancelReservationStatus::Rejected,
        "cancelling an unknown reservation id is Rejected"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
