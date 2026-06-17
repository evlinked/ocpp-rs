//! End-to-end CP ↔ CSMS session test — the full M4 lifecycle, in-process
//! (Issue #29, the M4 "CP simulator" capstone).
//!
//! This is the Rust port of the integration flow demonstrated by the Python
//! reference's [`tests/test_charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point.py)
//! and [`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py):
//! start a CSMS, connect a charge point, run a complete OCPP 1.6J conversation
//! (boot → heartbeat → status → authorize → transaction → meter values → stop)
//! and assert the CSMS observed every expected action, in order.
//!
//! Unlike the per-component tests in `ocpp-cp/tests`, this wires `OcppServer`
//! and `ChargePoint` together in the same process over a real loopback
//! WebSocket, so it exercises framing, routing and the CP session lifecycle as
//! a whole — the definition of "M4 done".

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
use ocpp_types::v16j::ChargePointStatus;
use ocpp_types::ConnectorId;
use tokio::sync::Mutex;

/// Ordered log of every action the CSMS received, by `ACTION_NAME`. Shared
/// between the dispatcher handlers and the test body.
type FrameLog = Arc<Mutex<Vec<String>>>;

/// The transaction ID the recording CSMS hands out for every StartTransaction,
/// mirroring the fixed `transaction_id` the Python reference's example CSMS
/// returns.
const TXN_ID: i32 = 42;

fn accepted() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// Build a CSMS dispatcher that records the name of every action it receives
/// (in arrival order) and replies with a realistic, accepting response.
///
/// Every handler the CP can drive in this flow is registered, so the CSMS never
/// trips over an unknown action. The BootNotification advertises a 1-second
/// heartbeat interval so a `Heartbeat` reliably ticks within the test window.
fn recording_dispatcher(log: FrameLog) -> ActionDispatcher {
    let mut d = ActionDispatcher::new();

    // Records `action` into the shared log; one helper keeps each handler terse.
    macro_rules! record {
        ($log:expr, $action:literal) => {{
            let l = $log.clone();
            move || {
                let l = l.clone();
                async move {
                    l.lock().await.push($action.to_string());
                }
            }
        }};
    }

    let rec = record!(log, "BootNotification");
    d.on(move |_req: BootNotificationRequest| {
        let rec = rec.clone();
        async move {
            rec().await;
            Ok(BootNotificationResponse {
                current_time: chrono::Utc::now(),
                interval: 1,
                status: RegistrationStatus::Accepted,
            })
        }
    });

    let rec = record!(log, "Heartbeat");
    d.on(move |_req: HeartbeatRequest| {
        let rec = rec.clone();
        async move {
            rec().await;
            Ok(HeartbeatResponse {
                current_time: chrono::Utc::now(),
            })
        }
    });

    let rec = record!(log, "StatusNotification");
    d.on(move |_req: StatusNotificationRequest| {
        let rec = rec.clone();
        async move {
            rec().await;
            Ok(StatusNotificationResponse {})
        }
    });

    let rec = record!(log, "Authorize");
    d.on(move |_req: AuthorizeRequest| {
        let rec = rec.clone();
        async move {
            rec().await;
            Ok(AuthorizeResponse {
                id_tag_info: accepted(),
            })
        }
    });

    let rec = record!(log, "StartTransaction");
    d.on(move |_req: StartTransactionRequest| {
        let rec = rec.clone();
        async move {
            rec().await;
            Ok(StartTransactionResponse {
                id_tag_info: accepted(),
                transaction_id: TXN_ID,
            })
        }
    });

    let rec = record!(log, "MeterValues");
    d.on(move |_req: MeterValuesRequest| {
        let rec = rec.clone();
        async move {
            rec().await;
            Ok(MeterValuesResponse {})
        }
    });

    let rec = record!(log, "StopTransaction");
    d.on(move |_req: StopTransactionRequest| {
        let rec = rec.clone();
        async move {
            rec().await;
            Ok(StopTransactionResponse { id_tag_info: None })
        }
    });

    d
}

/// Start an in-process CSMS using `dispatcher`; returns it with its bound addr
/// (a random free loopback port).
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
        // One physical connector keeps the boot announcement deterministic
        // (connector 0 for the CP plus connector 1).
        connector_count: 1,
        // Sample meter values every second so a `MeterValues` frame lands
        // quickly inside the transaction window.
        meter_values_interval: 1,
        // Deterministic: no background reconnect storm racing the assertions.
        auto_reconnect: false,
        ..ChargePointConfig::default()
    }
}

/// Poll the frame log until `action` appears or `timeout` elapses. Returns
/// `true` if it appeared. Avoids a fixed `sleep` so the test is both fast and
/// robust to scheduling jitter under load.
async fn wait_for(log: &FrameLog, action: &str, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if log.lock().await.iter().any(|a| a == action) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Index of the first occurrence of `action` in `frames`; panics with a clear
/// message if absent, so an ordering assertion failure pinpoints the cause.
fn first_index(frames: &[String], action: &str) -> usize {
    frames
        .iter()
        .position(|a| a == action)
        .unwrap_or_else(|| panic!("expected the CSMS to receive a {action}; saw {frames:?}"))
}

#[tokio::test]
async fn full_cp_session_boot_to_transaction() {
    let log: FrameLog = Arc::new(Mutex::new(Vec::new()));
    let (mut server, addr) = start_csms(recording_dispatcher(log.clone())).await;

    // 1. Connect — connect() runs the BootNotification handshake internally and
    //    only returns Ok once the CSMS has Accepted the charge point.
    let cp = ChargePoint::new(cp_config(addr, "CP_E2E_01")).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert_eq!(
        cp.registration_status().await,
        RegistrationStatus::Accepted,
        "default CSMS must accept the BootNotification"
    );

    // 2. A heartbeat must tick at the CSMS-advertised 1s interval.
    assert!(
        wait_for(&log, "Heartbeat", Duration::from_secs(10)).await,
        "CP must send a Heartbeat at the advertised interval"
    );

    // 3. Authorize an id-tag, then run a transaction on connector 1.
    let connector = ConnectorId::new(1).unwrap();
    let id_tag = cp.authorize("TAG_001").await.expect("authorize");
    assert_eq!(
        id_tag.status,
        AuthorizationStatus::Accepted,
        "CSMS authorizes the id-tag"
    );

    let txn_id = cp
        .start_transaction(connector, "TAG_001", 0)
        .await
        .expect("start transaction");
    assert_eq!(txn_id, TXN_ID, "CP adopts the CSMS-assigned transaction id");
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Charging,
        "connector is Charging once the transaction starts"
    );

    // 4. At least one periodic MeterValues frame must arrive mid-transaction.
    assert!(
        wait_for(&log, "MeterValues", Duration::from_secs(10)).await,
        "CP must send periodic MeterValues during the transaction"
    );

    // 5. Stop the transaction; the connector returns to Available.
    cp.stop_transaction(txn_id, 100, Reason::Local)
        .await
        .expect("stop transaction");
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Available,
        "connector is Available again after the transaction stops"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");

    // 6. The CSMS must have observed every expected action type, in order.
    let frames = log.lock().await.clone();
    for action in [
        "BootNotification",
        "Heartbeat",
        "StatusNotification",
        "Authorize",
        "StartTransaction",
        "MeterValues",
        "StopTransaction",
    ] {
        assert!(
            frames.iter().any(|a| a == action),
            "CSMS should have received a {action}; saw {frames:?}"
        );
    }

    // Relative ordering of the lifecycle milestones. BootNotification opens the
    // session and is followed by the connector-0/1 Available announcements; the
    // transaction then runs authorize → start → meter values → stop.
    let boot = first_index(&frames, "BootNotification");
    let first_status = first_index(&frames, "StatusNotification");
    let authorize = first_index(&frames, "Authorize");
    let start = first_index(&frames, "StartTransaction");
    let meter = first_index(&frames, "MeterValues");
    let stop = first_index(&frames, "StopTransaction");

    assert!(
        boot < first_status,
        "boot precedes the status announcements"
    );
    assert!(
        first_status < authorize,
        "boot-time status announcements precede Authorize"
    );
    assert!(authorize < start, "Authorize precedes StartTransaction");
    assert!(start < meter, "StartTransaction precedes MeterValues");
    assert!(meter < stop, "MeterValues precede StopTransaction");
}
