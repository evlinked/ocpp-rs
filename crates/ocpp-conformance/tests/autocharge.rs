//! End-to-end conformance for CP-side AutoCharge (Issue #72).
//!
//! AutoCharge lets a driver start a session with no RFID/app: on plug-in the
//! charge point derives an `idTag` from the EV's MAC / EVCCID and drives the
//! normal `Authorize` + `StartTransaction` flow; on unplug it stops the
//! transaction with reason `EVDisconnected`. AutoCharge is a de-facto industry
//! extension (not part of the OCPP spec) with no `mobilityhouse/ocpp`
//! reference — it is built on the already-ported, standard 1.6 primitives
//! `Authorize` (§4.1), `StartTransaction` (§4.8) and `StopTransaction` (§4.10).
//!
//! These tests exercise the *side effect* on a real CP <-> CSMS loopback:
//! plugging in an AutoCharge-enabled connector with an enrolled EV must make
//! the CSMS observe an `Authorize` then a `StartTransaction`, both with the
//! MAC-derived `idTag`; unplugging must make the CSMS observe a
//! `StopTransaction` with reason `EVDisconnected`. A connector with AutoCharge
//! *disabled* must stay inert on plug-in.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{AutoChargeConfig, ChargePoint, ChargePointConfig};
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
use tokio::sync::mpsc;
use tokio::time::timeout;

/// The transaction id the CSMS hands out.
const TXN_ID: i32 = 7;

/// Generous bound so a loaded CI box doesn't flake on the async side effect.
const SIDE_EFFECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The EV MAC the simulator presents, and the `idTag` the CP must derive from
/// it (12 uppercase hex, separators stripped — see `ocpp_types::autocharge`).
const EV_MAC: &str = "aa:bb:cc:dd:ee:ff";
const DERIVED_ID_TAG: &str = "AABBCCDDEEFF";

fn accepted() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// What the CSMS observed for an `Authorize` CALL.
#[derive(Debug)]
struct AuthorizeObserved {
    id_tag: String,
}

/// What the CSMS observed for a `StartTransaction` CALL.
#[derive(Debug)]
struct StartObserved {
    connector_id: u32,
    id_tag: String,
}

/// What the CSMS observed for a `StopTransaction` CALL.
#[derive(Debug)]
struct StopObserved {
    transaction_id: i32,
    reason: Option<Reason>,
}

/// A CSMS dispatcher that records the `Authorize` / `StartTransaction` /
/// `StopTransaction` CALLs the CP sends, so the test can assert the AutoCharge
/// side effects actually fired.
fn recording_csms_dispatcher(
    auth_tx: mpsc::UnboundedSender<AuthorizeObserved>,
    start_tx: mpsc::UnboundedSender<StartObserved>,
    stop_tx: mpsc::UnboundedSender<StopObserved>,
) -> ActionDispatcher {
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
    {
        let auth_tx = auth_tx.clone();
        d.on(move |req: AuthorizeRequest| {
            let auth_tx = auth_tx.clone();
            async move {
                let _ = auth_tx.send(AuthorizeObserved { id_tag: req.id_tag });
                Ok(AuthorizeResponse {
                    id_tag_info: accepted(),
                })
            }
        });
    }
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

fn cp_config(addr: SocketAddr, id: &str, auto_charge: AutoChargeConfig) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        auto_reconnect: false,
        // Keep the periodic sampler quiet so it doesn't interleave with asserts.
        meter_values_interval: 3600,
        auto_charge,
        ..ChargePointConfig::default()
    }
}

/// Poll the connector until it reaches `want`, or fail after the timeout.
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
async fn autocharge_plug_in_starts_and_unplug_stops() {
    let cp_id = "CP_AUTOCHARGE_01";
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel();
    let (start_tx, mut start_rx) = mpsc::unbounded_channel();
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) =
        start_csms(recording_csms_dispatcher(auth_tx, start_tx, stop_tx)).await;

    let cp = ChargePoint::new(cp_config(
        addr,
        cp_id,
        AutoChargeConfig {
            enabled: true,
            id_tag_prefix: None,
        },
    ))
    .expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(server.is_cp_connected(cp_id), "CSMS must route to the CP");

    let connector = ConnectorId::new(1).unwrap();
    // Enroll the EV on connector 1.
    cp.set_ev_identifier(connector, EV_MAC).await;

    // 1. Plug in: AutoCharge derives the idTag and authorizes it (§4.1).
    cp.plug_in(connector).await.expect("plug in");
    let authorized = timeout(SIDE_EFFECT_TIMEOUT, auth_rx.recv())
        .await
        .expect("CSMS observes an Authorize after plug-in")
        .expect("authorize channel open");
    assert_eq!(
        authorized.id_tag, DERIVED_ID_TAG,
        "Authorize carries the MAC-derived idTag"
    );

    // 2. ...then starts the transaction with the same idTag (§4.8).
    let started = timeout(SIDE_EFFECT_TIMEOUT, start_rx.recv())
        .await
        .expect("CSMS observes a StartTransaction after plug-in")
        .expect("start channel open");
    assert_eq!(started.connector_id, 1, "started on the plugged connector");
    assert_eq!(
        started.id_tag, DERIVED_ID_TAG,
        "StartTransaction carries the MAC-derived idTag"
    );
    wait_for_status(&cp, connector, ChargePointStatus::Charging).await;

    // 3. Unplug: AutoCharge stops the transaction with reason EVDisconnected.
    cp.plug_out(connector).await.expect("plug out");
    let stopped = timeout(SIDE_EFFECT_TIMEOUT, stop_rx.recv())
        .await
        .expect("CSMS observes a StopTransaction after unplug")
        .expect("stop channel open");
    assert_eq!(stopped.transaction_id, TXN_ID, "stopped the running txn");
    assert_eq!(
        stopped.reason,
        Some(Reason::EVDisconnected),
        "an unplug-driven stop reports reason EVDisconnected"
    );
    wait_for_status(&cp, connector, ChargePointStatus::Available).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn autocharge_disabled_plug_in_is_inert() {
    let cp_id = "CP_AUTOCHARGE_OFF_01";
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel();
    let (start_tx, _start_rx) = mpsc::unbounded_channel();
    let (stop_tx, _stop_rx) = mpsc::unbounded_channel();
    let (mut server, addr) =
        start_csms(recording_csms_dispatcher(auth_tx, start_tx, stop_tx)).await;

    // AutoCharge disabled (the default).
    let cp = ChargePoint::new(cp_config(addr, cp_id, AutoChargeConfig::default()))
        .expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");

    let connector = ConnectorId::new(1).unwrap();
    // Even with an EV enrolled, a disabled CP must not auto-start.
    cp.set_ev_identifier(connector, EV_MAC).await;
    cp.plug_in(connector).await.expect("plug in");

    // The connector still moves to Preparing (cable plugged) but no Authorize
    // is sent and the connector never reaches Charging.
    wait_for_status(&cp, connector, ChargePointStatus::Preparing).await;
    assert!(
        timeout(Duration::from_millis(300), auth_rx.recv())
            .await
            .is_err(),
        "a disabled CP must not send an Authorize on plug-in"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
