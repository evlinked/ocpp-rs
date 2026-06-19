//! End-to-end conformance test for `UnlockConnector` (OCPP 1.6J §5.21, Issue #88).
//!
//! Exercises the faithful unlock behavior the CP grew in place of the old
//! always-`Unlocked` stub:
//!
//! - an idle, valid connector → `Unlocked` (cable released, no OCPP side effect);
//! - a connector with an active transaction → `Unlocked`, and the CP actually
//!   stops that transaction (`StopTransaction`, reason `UnlockCommand`) and frees
//!   the connector (→ `Available`);
//! - an unknown / out-of-range `connectorId` (incl. 0) → `UnlockFailed` (the CP
//!   cannot unlock a connector it does not have; the spec response has no
//!   "Rejected");
//! - a CP whose lock is uncontrollable → `NotSupported`;
//! - a CP whose lock mechanically fails → `UnlockFailed`.
//!
//! Rust counterpart of the Python reference's `@on('UnlockConnector')`
//! ([`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py)).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig, UnlockConnectorOutcome};
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
use ocpp_types::v16j::{ChargePointStatus, RemoteStartStopStatus, UnlockStatus};
use ocpp_types::ConnectorId;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// The transaction id the CSMS hands out for a started transaction.
const TXN_ID: i32 = 77;

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

/// A CSMS dispatcher that records the `StopTransaction` CALLs the CP sends, so a
/// test can assert the unlock side effect actually stopped the transaction.
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

fn cp_config(addr: SocketAddr, id: &str, outcome: UnlockConnectorOutcome) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        unlock_connector_outcome: outcome,
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

/// Boot a charge point against `addr` and assert the CSMS can route CALLs to it.
async fn boot_cp(
    server: &OcppServer,
    addr: SocketAddr,
    id: &str,
    outcome: UnlockConnectorOutcome,
) -> ChargePoint {
    let cp = ChargePoint::new(cp_config(addr, id, outcome)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(id),
        "the CSMS must be able to route CALLs to the booted CP"
    );
    cp
}

#[tokio::test]
async fn unlock_idle_connector_reports_unlocked() {
    let cp_id = "CP_UNLOCK_IDLE_01";
    let (stop_tx, _stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(stop_tx)).await;
    let cp = boot_cp(&server, addr, cp_id, UnlockConnectorOutcome::Unlock).await;

    let connector = ConnectorId::new(1).unwrap();
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Available,
        "connector starts out idle"
    );

    // An idle, valid connector unlocks its cable (§5.21) — purely local, the
    // connector stays Available.
    let status = server
        .unlock_connector(cp_id, 1)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::Unlocked,
        "an idle valid connector unlocks"
    );
    assert_eq!(
        cp.get_connector(connector).await.unwrap().status().await,
        ChargePointStatus::Available,
        "an idle connector stays Available after an unlock"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn unlock_connector_with_active_transaction_stops_it() {
    let cp_id = "CP_UNLOCK_TXN_01";
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(stop_tx)).await;
    let cp = boot_cp(&server, addr, cp_id, UnlockConnectorOutcome::Unlock).await;

    let connector = ConnectorId::new(1).unwrap();

    // Bring connector 1 into a charging transaction via RemoteStart (§5.11).
    let status = server
        .remote_start_transaction(cp_id, "TAG_UNLOCK", Some(1))
        .await
        .expect("remote start resolves");
    assert_eq!(status, RemoteStartStopStatus::Accepted);
    wait_for_status(&cp, connector, ChargePointStatus::Charging).await;

    // UnlockConnector on the charging connector → Unlocked (§5.21).
    let status = server
        .unlock_connector(cp_id, 1)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::Unlocked,
        "a controllable lock unlocks even with an active transaction"
    );

    // SIDE EFFECT: the CP must stop the transaction with reason UnlockCommand.
    let observed = timeout(SIDE_EFFECT_TIMEOUT, stop_rx.recv())
        .await
        .expect("CSMS observes a StopTransaction after an unlock")
        .expect("stop channel open");
    assert_eq!(
        observed.transaction_id, TXN_ID,
        "stopped the transaction on the unlocked connector"
    );
    assert_eq!(
        observed.reason,
        Some(Reason::UnlockCommand),
        "an unlock-triggered stop reports reason UnlockCommand"
    );

    // ...and the connector is free again.
    wait_for_status(&cp, connector, ChargePointStatus::Available).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn unlock_unknown_or_out_of_range_connector_fails() {
    let cp_id = "CP_UNLOCK_UNKNOWN_01";
    let (stop_tx, _stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(stop_tx)).await;
    // connector_count is 1, so connectors 0 and 9 are not present.
    let cp = boot_cp(&server, addr, cp_id, UnlockConnectorOutcome::Unlock).await;

    // connectorId 0 is not a chargeable connector.
    let status = server
        .unlock_connector(cp_id, 0)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::UnlockFailed,
        "connectorId 0 cannot be unlocked"
    );

    // A connector this CP does not have.
    let status = server
        .unlock_connector(cp_id, 9)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::UnlockFailed,
        "an unknown connector cannot be unlocked"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn unlock_reports_not_supported_when_lock_uncontrollable() {
    let cp_id = "CP_UNLOCK_NOTSUP_01";
    let (stop_tx, _stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(stop_tx)).await;
    let cp = boot_cp(&server, addr, cp_id, UnlockConnectorOutcome::NotSupported).await;

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
async fn unlock_reports_failed_on_mechanical_fault() {
    let cp_id = "CP_UNLOCK_FAILED_01";
    let (stop_tx, _stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(stop_tx)).await;
    let cp = boot_cp(&server, addr, cp_id, UnlockConnectorOutcome::UnlockFailed).await;

    let status = server
        .unlock_connector(cp_id, 1)
        .await
        .expect("unlock resolves");
    assert_eq!(
        status,
        UnlockStatus::UnlockFailed,
        "a mechanical unlock fault reports UnlockFailed"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
