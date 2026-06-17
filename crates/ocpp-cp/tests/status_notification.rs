//! StatusNotification lifecycle tests (Issue #28).
//!
//! These run a real [`ChargePoint`] against an in-process CSMS whose dispatcher
//! records every `StatusNotification` CALL it receives, so we can assert the
//! exact connector-state sequence the CP emits during boot and the transaction
//! lifecycle.
//!
//! The CSMS reuses the default central-system responders
//! (`central_system_dispatcher_with`, Issue #40) for BootNotification/Heartbeat
//! and overrides `StatusNotification` with a recorder; `Authorize`,
//! `StartTransaction` and `StopTransaction` get minimal accepting handlers so
//! the CP's transaction methods can complete.

use std::net::SocketAddr;
use std::sync::Arc;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    AuthorizeRequest, AuthorizeResponse, StartTransactionRequest, StartTransactionResponse,
    StatusNotificationRequest, StatusNotificationResponse, StopTransactionRequest,
    StopTransactionResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{
    central_system_dispatcher_with, CentralSystemConfig, DispatchHandler, TransportConfig,
};
use ocpp_types::common::{AuthorizationStatus, IdTagInfo, Reason};
use ocpp_types::v16j::{ChargePointErrorCode, ChargePointStatus};
use ocpp_types::ConnectorId;
use tokio::sync::Mutex;

/// Shared log of every `StatusNotification` CALL the CSMS received, in order.
type Recorder = Arc<Mutex<Vec<StatusNotificationRequest>>>;

fn accepted() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// Build a CSMS dispatcher that records `StatusNotification` frames and accepts
/// transactions, on top of the default boot/heartbeat responders.
fn recording_dispatcher(recorder: Recorder) -> ActionDispatcher {
    let mut d = central_system_dispatcher_with(CentralSystemConfig::default());

    // Override the default (empty) StatusNotification handler with one that
    // records the full request before replying. Registered after the defaults,
    // so it replaces the boot-trio's StatusNotification responder.
    let rec = recorder.clone();
    d.on(move |req: StatusNotificationRequest| {
        let rec = rec.clone();
        async move {
            rec.lock().await.push(req);
            Ok(StatusNotificationResponse {})
        }
    });

    d.on(|_req: AuthorizeRequest| async move {
        Ok(AuthorizeResponse {
            id_tag_info: accepted(),
        })
    });

    d.on(|_req: StartTransactionRequest| async move {
        Ok(StartTransactionResponse {
            id_tag_info: accepted(),
            transaction_id: 42,
        })
    });

    d.on(|_req: StopTransactionRequest| async move {
        Ok(StopTransactionResponse { id_tag_info: None })
    });

    d
}

/// Start an in-process CSMS using `dispatcher`; returns it with its bound addr.
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
        // One physical connector keeps the boot announcement deterministic:
        // connector 0 (the CP) plus connector 1.
        connector_count: 1,
        auto_reconnect: false,
        ..ChargePointConfig::default()
    }
}

/// Connect a CP to a fresh recording CSMS and return both plus the recorder.
/// The recorder is cleared after boot so callers see only post-boot frames.
async fn connected_cp(id: &str) -> (OcppServer, ChargePoint, Recorder) {
    let recorder: Recorder = Arc::new(Mutex::new(Vec::new()));
    let (server, addr) = start_csms(recording_dispatcher(recorder.clone())).await;
    let cp = ChargePoint::new(cp_config(addr, id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    (server, cp, recorder)
}

/// Convenience: snapshot recorded frames as `(connector_id, status)` pairs.
async fn pairs(recorder: &Recorder) -> Vec<(u32, ChargePointStatus)> {
    recorder
        .lock()
        .await
        .iter()
        .map(|f| (f.connector_id, f.status))
        .collect()
}

#[tokio::test]
async fn status_notification_sent_after_boot_accepted() {
    let recorder: Recorder = Arc::new(Mutex::new(Vec::new()));
    let (_server, addr) = start_csms(recording_dispatcher(recorder.clone())).await;
    let cp = ChargePoint::new(cp_config(addr, "CP_SN_BOOT")).expect("build charge point");

    cp.connect().await.expect("connect + boot sequence");

    // Boot announces connector 0 (the CP itself) then connector 1, both
    // Available/NoError.
    let frames = pairs(&recorder).await;
    assert_eq!(
        frames,
        vec![
            (0, ChargePointStatus::Available),
            (1, ChargePointStatus::Available),
        ],
        "boot must announce connector 0 and each configured connector as Available"
    );
    for f in recorder.lock().await.iter() {
        assert_eq!(
            f.error_code,
            ChargePointErrorCode::NoError,
            "boot announcements use NoError"
        );
    }
}

#[tokio::test]
async fn status_notification_sent_on_start_transaction() {
    let (_server, cp, recorder) = connected_cp("CP_SN_START").await;
    recorder.lock().await.clear(); // drop the boot announcements

    let connector = ConnectorId::new(1).unwrap();
    cp.start_transaction(connector, "TAG_START", 0)
        .await
        .expect("start transaction");

    assert_eq!(
        pairs(&recorder).await,
        vec![
            (1, ChargePointStatus::Preparing),
            (1, ChargePointStatus::Charging),
        ],
        "start_transaction must emit Preparing then Charging on the connector"
    );
}

#[tokio::test]
async fn status_notification_sent_on_stop_transaction() {
    let (_server, cp, recorder) = connected_cp("CP_SN_STOP").await;

    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "TAG_STOP", 0)
        .await
        .expect("start transaction");
    recorder.lock().await.clear(); // drop boot + start frames

    cp.stop_transaction(txn_id, 100, Reason::Local)
        .await
        .expect("stop transaction");

    assert_eq!(
        pairs(&recorder).await,
        vec![
            (1, ChargePointStatus::Finishing),
            (1, ChargePointStatus::Available),
        ],
        "stop_transaction must emit Finishing then Available on the connector"
    );
}

#[tokio::test]
async fn send_status_notification_custom_error_code() {
    let (_server, cp, recorder) = connected_cp("CP_SN_FAULT").await;
    recorder.lock().await.clear();

    cp.send_status_notification(
        1,
        ChargePointStatus::Faulted,
        ChargePointErrorCode::GroundFailure,
    )
    .await
    .expect("send status notification");

    let frames = recorder.lock().await;
    assert_eq!(frames.len(), 1, "exactly one StatusNotification expected");
    let frame = &frames[0];
    assert_eq!(frame.connector_id, 1);
    assert_eq!(frame.status, ChargePointStatus::Faulted);
    assert_eq!(
        frame.error_code,
        ChargePointErrorCode::GroundFailure,
        "the caller-supplied error code must be forwarded verbatim"
    );
    assert!(
        frame.timestamp.is_some(),
        "StatusNotification must carry a timestamp"
    );
}
