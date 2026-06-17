//! End-to-end Reset (Soft/Hard) test — CSMS-initiated `Reset` driving a real
//! charge-point side effect (Issue #53, M5 "Commands & control").
//!
//! Ports the OCPP 1.6J Reset use case (§5.13) and the `@on('Reset')` /
//! `@after('Reset')` split from the Python reference's
//! [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py):
//! the CSMS calls [`OcppServer::reset`], the CP acknowledges with a
//! `ResetStatus`, and then actually carries out the reset — gracefully stopping
//! any active transaction (reason `SoftReset` / `HardReset`) and re-running the
//! boot handshake.
//!
//! Like `full_session.rs`, this wires `OcppServer` and `ChargePoint` together
//! in-process over a real loopback WebSocket so it exercises the full
//! command/response + side-effect path, not just unit-level dispatch.

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
use ocpp_types::v16j::{ChargePointStatus, ResetStatus, ResetType};
use ocpp_types::ConnectorId;
use tokio::sync::Mutex;

/// Ordered log of every action the CSMS received, by `ACTION_NAME`. A
/// `StopTransaction` entry is annotated with its `reason` so the test can prove
/// the CP attributed the stop to the reset.
type FrameLog = Arc<Mutex<Vec<String>>>;

const TXN_ID: i32 = 7;

fn accepted() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// Build a CSMS dispatcher that records every action it receives (in arrival
/// order) and replies with an accepting response. `BootNotification` advertises
/// a long heartbeat interval so background heartbeats don't crowd the log, and
/// `StopTransaction` records the stop reason alongside the action name.
fn recording_dispatcher(log: FrameLog) -> ActionDispatcher {
    let mut d = ActionDispatcher::new();

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

    {
        let l = log.clone();
        d.on(move |_req: BootNotificationRequest| {
            let l = l.clone();
            async move {
                l.lock().await.push("BootNotification".to_string());
                Ok(BootNotificationResponse {
                    current_time: chrono::Utc::now(),
                    // 3600s: a heartbeat won't tick inside the test window.
                    interval: 3600,
                    status: RegistrationStatus::Accepted,
                })
            }
        });
    }

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

    {
        let l = log.clone();
        d.on(move |req: StopTransactionRequest| {
            let l = l.clone();
            async move {
                // Annotate with the reason so the test can assert the stop was
                // attributed to the (soft) reset, per OCPP 1.6J §5.13.
                l.lock()
                    .await
                    .push(format!("StopTransaction:{:?}", req.reason));
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
        // Long sampler interval: no periodic MeterValues racing the assertions.
        meter_values_interval: 3600,
        auto_reconnect: false,
        ..ChargePointConfig::default()
    }
}

/// Count how many times `action` appears in the log.
async fn count(log: &FrameLog, action: &str) -> usize {
    log.lock()
        .await
        .iter()
        .filter(|a| a.as_str() == action)
        .count()
}

/// Poll until `pred(log)` holds or `timeout` elapses; returns whether it held.
async fn wait_until<F>(log: &FrameLog, timeout: Duration, pred: F) -> bool
where
    F: Fn(&[String]) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if pred(&log.lock().await) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A Soft reset during an active transaction must stop that transaction with
/// reason `SoftReset` and then re-boot on the same connection — the CSMS sees a
/// `StopTransaction:Some(SoftReset)` followed by a second `BootNotification`.
#[tokio::test]
async fn soft_reset_stops_transaction_and_reboots() {
    let log: FrameLog = Arc::new(Mutex::new(Vec::new()));
    let (mut server, addr) = start_csms(recording_dispatcher(log.clone())).await;

    let cp = ChargePoint::new(cp_config(addr, "CP_RESET_SOFT")).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert_eq!(
        count(&log, "BootNotification").await,
        1,
        "one boot on connect"
    );

    // Start a transaction so the reset has something to gracefully stop.
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "TAG_001", 0)
        .await
        .expect("start transaction");
    assert_eq!(txn_id, TXN_ID);
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Charging,
        "connector charging before the reset"
    );

    // CSMS initiates the Soft reset; the CP acknowledges immediately.
    let status = server
        .reset("CP_RESET_SOFT", ResetType::Soft)
        .await
        .expect("reset resolves");
    assert_eq!(status, ResetStatus::Accepted, "CP accepts the reset");

    // The side effect runs out-of-band: the active transaction is stopped with
    // reason SoftReset, then the CP re-boots.
    assert!(
        wait_until(&log, Duration::from_secs(10), |frames| {
            frames
                .iter()
                .any(|a| a == "StopTransaction:Some(SoftReset)")
                && frames
                    .iter()
                    .filter(|a| a.as_str() == "BootNotification")
                    .count()
                    >= 2
        })
        .await,
        "expected a SoftReset StopTransaction and a second BootNotification; saw {:?}",
        log.lock().await
    );

    // Ordering: the reboot's BootNotification follows the reset-driven stop.
    let frames = log.lock().await.clone();
    let stop = frames
        .iter()
        .position(|a| a == "StopTransaction:Some(SoftReset)")
        .expect("SoftReset stop present");
    let second_boot = frames
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "BootNotification")
        .nth(1)
        .map(|(i, _)| i)
        .expect("second boot present");
    assert!(
        stop < second_boot,
        "stop precedes the re-boot; saw {frames:?}"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// A Hard reset tears the session down and reconnects from scratch: the CSMS
/// observes a second `BootNotification` after the reset is accepted.
#[tokio::test]
async fn hard_reset_reconnects_and_reboots() {
    let log: FrameLog = Arc::new(Mutex::new(Vec::new()));
    let (mut server, addr) = start_csms(recording_dispatcher(log.clone())).await;

    let cp = ChargePoint::new(cp_config(addr, "CP_RESET_HARD")).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert_eq!(
        count(&log, "BootNotification").await,
        1,
        "one boot on connect"
    );

    let status = server
        .reset("CP_RESET_HARD", ResetType::Hard)
        .await
        .expect("reset resolves");
    assert_eq!(status, ResetStatus::Accepted, "CP accepts the reset");

    assert!(
        wait_until(&log, Duration::from_secs(10), |frames| {
            frames
                .iter()
                .filter(|a| a.as_str() == "BootNotification")
                .count()
                >= 2
        })
        .await,
        "expected a second BootNotification after the hard reset; saw {:?}",
        log.lock().await
    );
    assert!(
        cp.is_connected().await,
        "CP reconnected after the hard reset"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
