//! End-to-end CS→CP GetDiagnostics test (Issue #69).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the M5 `GetDiagnostics` command (OCPP 1.6J §4.x,
//! firmware-management profile) through the `OcppServer::get_diagnostics`
//! helper, asserting the CP not only answers with a `fileName` but **actually
//! runs** the simulated upload off the inbound-CALL path, emitting
//! `DiagnosticsStatusNotification(Uploading)` then `Uploaded`:
//!
//!   1. `get_diagnostics(...)` → `Accepted` with a non-empty `fileName`, and the
//!      CSMS then observes `Uploading` followed by `Uploaded`.
//!   2. `trigger_message(DiagnosticsStatusNotification)` → `Accepted`, and the
//!      CSMS observes a notification carrying the CP's *current* status
//!      (`Uploaded`) — closing the deferred half of Issue #65.
//!
//! Rust counterpart of the Python reference's central system driving
//! `GetDiagnostics`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against the `@on('GetDiagnostics')` charge point.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, DiagnosticsStatusNotificationRequest,
    DiagnosticsStatusNotificationResponse, HeartbeatRequest, HeartbeatResponse, RegistrationStatus,
    StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::v16j::{DiagnosticsStatus, MessageTrigger, TriggerMessageStatus};
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Bound on how long a notification may take to reach the CSMS before the test
/// gives up. Generous so a loaded CI box doesn't flake.
const DIAG_TIMEOUT: Duration = Duration::from_secs(5);

/// A CSMS dispatcher that records every `DiagnosticsStatusNotification` status
/// the CP sends, so the test can assert the upload state machine actually ran.
fn recording_csms_dispatcher(
    diag_tx: mpsc::UnboundedSender<DiagnosticsStatus>,
) -> ActionDispatcher {
    let mut d = ActionDispatcher::new();

    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: chrono::Utc::now(),
            // A long interval keeps stray heartbeats from racing the asserts.
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
        let diag_tx = diag_tx.clone();
        d.on(move |req: DiagnosticsStatusNotificationRequest| {
            let diag_tx = diag_tx.clone();
            async move {
                let _ = diag_tx.send(req.status);
                Ok(DiagnosticsStatusNotificationResponse {})
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
    cp_config_with_fault(addr, id, false)
}

/// `cp_config`, but with the opt-in diagnostics-upload fault injection toggled
/// so a test can drive the `Uploading → UploadFailed` branch (Issue #83).
fn cp_config_with_fault(addr: SocketAddr, id: &str, fail_upload: bool) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        diagnostics_upload_should_fail: fail_upload,
        ..ChargePointConfig::default()
    }
}

/// Pull notifications until one matches `expected`, failing the test if none
/// arrives before [`DIAG_TIMEOUT`].
async fn recv_status(
    rx: &mut mpsc::UnboundedReceiver<DiagnosticsStatus>,
    expected: DiagnosticsStatus,
) {
    loop {
        let status = timeout(DIAG_TIMEOUT, rx.recv())
            .await
            .expect("CSMS observes a DiagnosticsStatusNotification")
            .expect("diagnostics channel open");
        if status == expected {
            return;
        }
    }
}

#[tokio::test]
async fn csms_get_diagnostics_drives_upload_state_machine() {
    let cp_id = "CP_DIAG_01";
    let (diag_tx, mut diag_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(diag_tx)).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // 1. GetDiagnostics → Accepted with a file name, and the CP runs the upload.
    let resp = server
        .get_diagnostics(cp_id, "ftp://example.test/diag", None, None, None, None)
        .await
        .expect("get_diagnostics resolves");
    let file_name = resp
        .file_name
        .expect("an accepted GetDiagnostics returns the file name the CP will upload");
    assert!(
        !file_name.is_empty(),
        "the returned diagnostics file name must be non-empty"
    );

    // The simulated upload reports Uploading then Uploaded, in order.
    recv_status(&mut diag_rx, DiagnosticsStatus::Uploading).await;
    recv_status(&mut diag_rx, DiagnosticsStatus::Uploaded).await;

    // 2. TriggerMessage(DiagnosticsStatusNotification) reports the *current*
    //    status (Uploaded) without re-running the upload.
    let status = server
        .trigger_message(cp_id, MessageTrigger::DiagnosticsStatusNotification, None)
        .await
        .expect("trigger_message(Diagnostics) resolves");
    assert_eq!(
        status,
        TriggerMessageStatus::Accepted,
        "the CP now supports a DiagnosticsStatusNotification trigger"
    );
    recv_status(&mut diag_rx, DiagnosticsStatus::Uploaded).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// Issue #83 — the diagnostics simulator can model a *failed* upload, not just
/// the happy path. With the opt-in `diagnostics_upload_should_fail` knob set, a
/// `GetDiagnostics` drives `Uploading → UploadFailed`, and the failed status is
/// retained so a later `TriggerMessage(DiagnosticsStatusNotification)` reports
/// `UploadFailed`.
#[tokio::test]
async fn csms_get_diagnostics_upload_failure_is_observable() {
    let cp_id = "CP_DIAG_FAIL_01";
    let (diag_tx, mut diag_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(diag_tx)).await;

    // A charge point with fault injection on: its diagnostics upload will fail.
    let cp = ChargePoint::new(cp_config_with_fault(addr, cp_id, true)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // GetDiagnostics is still Accepted — the upload only fails partway through.
    let resp = server
        .get_diagnostics(cp_id, "ftp://example.test/diag", None, None, None, None)
        .await
        .expect("get_diagnostics resolves");
    assert!(
        resp.file_name.is_some_and(|n| !n.is_empty()),
        "an accepted GetDiagnostics returns a non-empty file name even when the upload will fail"
    );

    // The simulated upload reports Uploading then UploadFailed, in order.
    recv_status(&mut diag_rx, DiagnosticsStatus::Uploading).await;
    recv_status(&mut diag_rx, DiagnosticsStatus::UploadFailed).await;

    // The failed status is retained: a trigger reports the *current* status
    // (UploadFailed) without re-running the upload.
    let status = server
        .trigger_message(cp_id, MessageTrigger::DiagnosticsStatusNotification, None)
        .await
        .expect("trigger_message(Diagnostics) resolves");
    assert_eq!(
        status,
        TriggerMessageStatus::Accepted,
        "the CP supports a DiagnosticsStatusNotification trigger after a failed upload"
    );
    recv_status(&mut diag_rx, DiagnosticsStatus::UploadFailed).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
