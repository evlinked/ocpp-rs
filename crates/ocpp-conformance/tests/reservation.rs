//! End-to-end conformance for ReserveNow / CancelReservation (Issue #71).
//!
//! `ReserveNow` (OCPP 1.6J §5.14) reserves a connector for an `idTag` until an
//! `expiryDate`; `CancelReservation` (§5.4) clears a reservation by
//! `reservationId`. These tests drive both from a real CSMS over a loopback to
//! a real charge point and assert the faithful status semantics *and* the CP
//! side effect (connector → `Reserved` / `Available`).
//!
//! Rust counterpart of the Python reference's `ReserveNow` / `CancelReservation`
//! ([`ocpp/v16/call.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/call.py),
//! [`ocpp/v16/enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/enums.py)).

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

const TXN_ID: i32 = 99;
const SIDE_EFFECT_TIMEOUT: Duration = Duration::from_secs(5);

fn accepted() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// A minimal CSMS dispatcher that lets the CP boot and (for the consume test)
/// start a transaction. ReserveNow/CancelReservation flow CSMS→CP, so the
/// dispatcher needs no reservation handlers.
fn csms_dispatcher() -> ActionDispatcher {
    let mut d = ActionDispatcher::new();
    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: chrono::Utc::now(),
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

async fn start_csms() -> (OcppServer, SocketAddr) {
    let handler = Arc::new(DispatchHandler::new(Arc::new(csms_dispatcher())));
    let (mut server, _events) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr)
}

/// A CSMS dispatcher that records every `StatusNotification` (connectorId,
/// status) the CP sends, so a test can assert the reservation transitions
/// (`Reserved` / `Available`, Issue #80) actually reach the CSMS rather than
/// only changing the connector's local state.
fn recording_csms_dispatcher(
    status_tx: mpsc::UnboundedSender<(u32, ChargePointStatus)>,
) -> ActionDispatcher {
    let mut d = csms_dispatcher();
    // Override the no-op StatusNotification handler with a recording one.
    d.on(move |req: StatusNotificationRequest| {
        let status_tx = status_tx.clone();
        async move {
            let _ = status_tx.send((req.connector_id, req.status));
            Ok(StatusNotificationResponse {})
        }
    });
    d
}

async fn start_recording_csms(dispatcher: ActionDispatcher) -> (OcppServer, SocketAddr) {
    let handler = Arc::new(DispatchHandler::new(Arc::new(dispatcher)));
    let (mut server, _events) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr)
}

/// Pull `StatusNotification`s until one matches `(connector, want)`, discarding
/// the earlier lifecycle notifications (e.g. the boot-time `Available`). Fails
/// the test if none arrives before [`SIDE_EFFECT_TIMEOUT`].
async fn recv_status_notification(
    rx: &mut mpsc::UnboundedReceiver<(u32, ChargePointStatus)>,
    connector: u32,
    want: ChargePointStatus,
) {
    let poll = async {
        loop {
            let (cid, status) = rx.recv().await.expect("status channel open");
            if cid == connector && status == want {
                return;
            }
        }
    };
    timeout(SIDE_EFFECT_TIMEOUT, poll)
        .await
        .unwrap_or_else(|_| {
            panic!("CSMS did not observe StatusNotification({connector}, {want:?}) in time")
        });
}

fn cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        auto_reconnect: false,
        meter_values_interval: 3600,
        ..ChargePointConfig::default()
    }
}

async fn status_of(cp: &ChargePoint, connector: ConnectorId) -> ChargePointStatus {
    cp.get_connector(connector)
        .await
        .expect("connector exists")
        .status()
        .await
}

/// Poll the connector until it reaches `want`, or fail after the timeout. The
/// CSMS-initiated reserve/cancel resolves the CALLRESULT before the CP records
/// the status, so polling avoids a racy bare assert.
async fn wait_for_status(cp: &ChargePoint, connector: ConnectorId, want: ChargePointStatus) {
    let poll = async {
        loop {
            if status_of(cp, connector).await == want {
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
async fn reserve_then_cancel_lifecycle() {
    let cp_id = "CP_RESERVE_01";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(server.is_cp_connected(cp_id), "CSMS must route to the CP");

    let connector = ConnectorId::new(1).unwrap();
    let expiry = chrono::Utc::now() + chrono::Duration::hours(1);

    // 1. Reserve the free connector 1 → Accepted, connector → Reserved (§5.14).
    let status = server
        .reserve_now(cp_id, 1, "RES_TAG", expiry, 1001, None)
        .await
        .expect("reserve_now resolves");
    assert_eq!(
        status,
        ReservationStatus::Accepted,
        "a free connector accepts a reservation"
    );
    wait_for_status(&cp, connector, ChargePointStatus::Reserved).await;

    // 2. Reserve the now-busy (Reserved) connector → Occupied.
    let status = server
        .reserve_now(cp_id, 1, "OTHER_TAG", expiry, 1002, None)
        .await
        .expect("reserve_now resolves");
    assert_eq!(
        status,
        ReservationStatus::Occupied,
        "a reserved connector reports Occupied"
    );

    // 3. Reserve an unknown connector id → Rejected.
    let status = server
        .reserve_now(cp_id, 7, "RES_TAG", expiry, 1003, None)
        .await
        .expect("reserve_now resolves");
    assert_eq!(
        status,
        ReservationStatus::Rejected,
        "an unknown connector id is rejected"
    );

    // 4. Cancel the held reservation → Accepted, connector → Available (§5.4).
    let status = server
        .cancel_reservation(cp_id, 1001)
        .await
        .expect("cancel_reservation resolves");
    assert_eq!(
        status,
        CancelReservationStatus::Accepted,
        "cancelling a held reservation is accepted"
    );
    wait_for_status(&cp, connector, ChargePointStatus::Available).await;

    // 5. Cancel an unknown reservation id → Rejected.
    let status = server
        .cancel_reservation(cp_id, 4242)
        .await
        .expect("cancel_reservation resolves");
    assert_eq!(
        status,
        CancelReservationStatus::Rejected,
        "cancelling an unknown reservation id is rejected"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// Issue #80: a back office watching connector availability should see a
/// connector flip to `Reserved` the moment a reservation is held, and back to
/// `Available` when it is cancelled — not on some later, unrelated status
/// event. Asserts the CSMS *observes* both `StatusNotification` CALLs, which
/// the CP sends off the inbound-CALL path via the `RemoteCommand` consumer.
#[tokio::test]
async fn reserve_and_cancel_emit_status_notifications() {
    let cp_id = "CP_RESERVE_NOTIFY";
    let (status_tx, mut status_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_recording_csms(recording_csms_dispatcher(status_tx)).await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(server.is_cp_connected(cp_id), "CSMS must route to the CP");

    let expiry = chrono::Utc::now() + chrono::Duration::hours(1);

    // ReserveNow(Accepted) → CSMS observes StatusNotification(1, Reserved).
    // (The boot-time Available for connector 1 is drained by the recv loop.)
    let status = server
        .reserve_now(cp_id, 1, "RES_TAG", expiry, 5001, None)
        .await
        .expect("reserve_now resolves");
    assert_eq!(status, ReservationStatus::Accepted);
    recv_status_notification(&mut status_rx, 1, ChargePointStatus::Reserved).await;

    // CancelReservation(Accepted) → CSMS observes StatusNotification(1, Available).
    let status = server
        .cancel_reservation(cp_id, 5001)
        .await
        .expect("cancel_reservation resolves");
    assert_eq!(status, CancelReservationStatus::Accepted);
    recv_status_notification(&mut status_rx, 1, ChargePointStatus::Available).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn start_consumes_reservation() {
    let cp_id = "CP_RESERVE_02";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");

    let connector = ConnectorId::new(1).unwrap();
    let expiry = chrono::Utc::now() + chrono::Duration::hours(1);

    // Reserve connector 1 for RES_TAG.
    let status = server
        .reserve_now(cp_id, 1, "RES_TAG", expiry, 2001, None)
        .await
        .expect("reserve_now resolves");
    assert_eq!(status, ReservationStatus::Accepted);
    wait_for_status(&cp, connector, ChargePointStatus::Reserved).await;

    // A RemoteStart with the reserving idTag starts a transaction on the
    // reserved connector — the connector ends up Charging and the reservation
    // is consumed.
    let rs = server
        .remote_start_transaction(cp_id, "RES_TAG", Some(1))
        .await
        .expect("remote start resolves");
    assert_eq!(rs, RemoteStartStopStatus::Accepted);
    wait_for_status(&cp, connector, ChargePointStatus::Charging).await;

    // Because the start consumed it, cancelling that reservation id → Rejected.
    let status = server
        .cancel_reservation(cp_id, 2001)
        .await
        .expect("cancel_reservation resolves");
    assert_eq!(
        status,
        CancelReservationStatus::Rejected,
        "a reservation consumed by a start can no longer be cancelled"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
