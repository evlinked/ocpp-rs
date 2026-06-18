//! End-to-end conformance for `UnlockConnector` (OCPP 1.6J §5.21, Issue #88).
//!
//! Exercises both the command/response leg — the faithful [`UnlockStatus`] the
//! CP reports for each connector state / lock capability — and the *side
//! effect*: an `Unlocked` on a connector with a transaction in progress must
//! stop that transaction (`StopTransaction`, reason `UnlockCommand`) and free
//! the connector back to `Available`.
//!
//! Rust counterpart of the Python reference's `@on('UnlockConnector')`
//! ([`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py))
//! and `UnlockStatus` ([`ocpp/v16/enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/enums.py)).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig, UnlockOutcome};
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
use ocpp_types::v16j::{ChargePointStatus, UnlockStatus};
use ocpp_types::ConnectorId;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// The transaction id the CSMS hands out (matches the reference example CSMS).
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

/// What the CSMS observed for a `StopTransaction` CALL the CP sent.
#[derive(Debug)]
struct StopObserved {
    transaction_id: i32,
    reason: Option<Reason>,
}

/// A CSMS dispatcher that records the `StopTransaction` CALLs the CP sends, so
/// the test can assert the unlock side effect actually fired (or, for the
/// failure outcomes, that it did *not*).
fn recording_csms_dispatcher(stop_tx: mpsc::UnboundedSender<StopObserved>) -> ActionDispatcher {
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

fn cp_config(addr: SocketAddr, id: &str, unlock_outcome: UnlockOutcome) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        unlock_outcome,
        ..ChargePointConfig::default()
    }
}

/// Poll the connector until it reaches `want`, or fail after `SIDE_EFFECT_TIMEOUT`.
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

/// Boot a CP against a fresh CSMS and return both, plus the StopTransaction
/// observation channel.
async fn boot(
    cp_id: &str,
    unlock_outcome: UnlockOutcome,
) -> (
    OcppServer,
    ChargePoint,
    mpsc::UnboundedReceiver<StopObserved>,
) {
    let (stop_tx, stop_rx) = mpsc::unbounded_channel();
    let (server, addr) = start_csms(recording_csms_dispatcher(stop_tx)).await;
    let cp = ChargePoint::new(cp_config(addr, cp_id, unlock_outcome)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );
    (server, cp, stop_rx)
}

#[tokio::test]
async fn unlock_idle_connector_reports_unlocked() {
    let cp_id = "CP_UNLOCK_IDLE_01";
    let (mut server, cp, _stop_rx) = boot(cp_id, UnlockOutcome::Unlock).await;

    let connector = ConnectorId::new(1).unwrap();
    let status = server
        .unlock_connector(cp_id, 1)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::Unlocked,
        "an idle, valid connector unlocks"
    );

    // The connector was already free and stays free.
    wait_for_status(&cp, connector, ChargePointStatus::Available).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn unlock_connector_with_active_transaction_stops_it() {
    let cp_id = "CP_UNLOCK_TXN_01";
    let (mut server, cp, mut stop_rx) = boot(cp_id, UnlockOutcome::Unlock).await;

    let connector = ConnectorId::new(1).unwrap();

    // Drive a real transaction onto connector 1 (→ Charging).
    let txn = cp
        .start_transaction(connector, "TAG_UNLOCK", 0)
        .await
        .expect("start transaction");
    assert_eq!(txn, TXN_ID, "CSMS assigns the fixed transaction id");
    wait_for_status(&cp, connector, ChargePointStatus::Charging).await;

    // UnlockConnector on the busy connector → Unlocked.
    let status = server
        .unlock_connector(cp_id, 1)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::Unlocked,
        "the connector unlocks even with a transaction running"
    );

    // SIDE EFFECT: the CP must stop the transaction with reason UnlockCommand.
    let observed = timeout(SIDE_EFFECT_TIMEOUT, stop_rx.recv())
        .await
        .expect("CSMS observes a StopTransaction after the unlock")
        .expect("stop channel open");
    assert_eq!(observed.transaction_id, TXN_ID, "stopped the running txn");
    assert_eq!(
        observed.reason,
        Some(Reason::UnlockCommand),
        "an unlock-initiated stop reports reason UnlockCommand"
    );

    // ...and the connector is freed back to Available.
    wait_for_status(&cp, connector, ChargePointStatus::Available).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn unlock_unknown_connector_reports_unlock_failed() {
    let cp_id = "CP_UNLOCK_UNKNOWN_01";
    // connector_count = 1, so connector 9 does not exist.
    let (mut server, cp, _stop_rx) = boot(cp_id, UnlockOutcome::Unlock).await;

    let status = server
        .unlock_connector(cp_id, 9)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::UnlockFailed,
        "an unknown connector cannot be unlocked → UnlockFailed (not a false Unlocked)"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn unlock_not_supported_outcome_reports_not_supported() {
    let cp_id = "CP_UNLOCK_NOSUP_01";
    let (mut server, cp, _stop_rx) = boot(cp_id, UnlockOutcome::NotSupported).await;

    let status = server
        .unlock_connector(cp_id, 1)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::NotSupported,
        "a connector with no controllable lock reports NotSupported"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn unlock_fail_outcome_leaves_transaction_running() {
    let cp_id = "CP_UNLOCK_FAIL_01";
    let (mut server, cp, mut stop_rx) = boot(cp_id, UnlockOutcome::Fail).await;

    let connector = ConnectorId::new(1).unwrap();
    cp.start_transaction(connector, "TAG_UNLOCK", 0)
        .await
        .expect("start transaction");
    wait_for_status(&cp, connector, ChargePointStatus::Charging).await;

    // A mechanically stuck lock fails to release.
    let status = server
        .unlock_connector(cp_id, 1)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::UnlockFailed,
        "a stuck lock reports UnlockFailed"
    );

    // The cable stays latched: no StopTransaction is sent and the connector
    // keeps charging. A short wait confirms the absence of a side effect.
    let no_stop = timeout(Duration::from_millis(500), stop_rx.recv()).await;
    assert!(
        no_stop.is_err(),
        "a failed unlock must not stop the transaction"
    );
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Charging,
        "the connector keeps charging after a failed unlock"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
