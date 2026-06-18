//! End-to-end CS→CP UpdateFirmware test (Issue #70).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the M5 `UpdateFirmware` command (OCPP 1.6J §4.x,
//! firmware-management profile) through the `OcppServer::update_firmware`
//! helper, asserting the CP not only answers the (empty) `UpdateFirmware.conf`
//! but **actually runs** the simulated update off the inbound-CALL path,
//! emitting `FirmwareStatusNotification(Downloading)` → `Downloaded` →
//! `Installing` → `Installed`:
//!
//!   1. `update_firmware(...)` resolves with the empty conf, and the CSMS then
//!      observes the full `Downloading → Downloaded → Installing → Installed`
//!      progression in order.
//!   2. `trigger_message(FirmwareStatusNotification)` → `Accepted`, and the
//!      CSMS observes a notification carrying the CP's *current* status
//!      (`Installed`) — closing the deferred half of Issue #65.
//!
//! Rust counterpart of the Python reference's central system driving
//! `UpdateFirmware`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against the `@on('UpdateFirmware')` charge point.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, FirmwareStatusNotificationRequest,
    FirmwareStatusNotificationResponse, HeartbeatRequest, HeartbeatResponse, RegistrationStatus,
    StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::v16j::{FirmwareStatus, MessageTrigger, TriggerMessageStatus};
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Bound on how long a notification may take to reach the CSMS before the test
/// gives up. Generous so a loaded CI box doesn't flake.
const FW_TIMEOUT: Duration = Duration::from_secs(5);

/// A CSMS dispatcher that records every `FirmwareStatusNotification` status the
/// CP sends, so the test can assert the update state machine actually ran.
fn recording_csms_dispatcher(fw_tx: mpsc::UnboundedSender<FirmwareStatus>) -> ActionDispatcher {
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
        let fw_tx = fw_tx.clone();
        d.on(move |req: FirmwareStatusNotificationRequest| {
            let fw_tx = fw_tx.clone();
            async move {
                let _ = fw_tx.send(req.status);
                Ok(FirmwareStatusNotificationResponse {})
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

/// Pull notifications until one matches `expected`, failing the test if none
/// arrives before [`FW_TIMEOUT`].
async fn recv_status(rx: &mut mpsc::UnboundedReceiver<FirmwareStatus>, expected: FirmwareStatus) {
    loop {
        let status = timeout(FW_TIMEOUT, rx.recv())
            .await
            .expect("CSMS observes a FirmwareStatusNotification")
            .expect("firmware channel open");
        if status == expected {
            return;
        }
    }
}

#[tokio::test]
async fn csms_update_firmware_drives_update_state_machine() {
    let cp_id = "CP_FW_01";
    let (fw_tx, mut fw_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(fw_tx)).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // 1. UpdateFirmware resolves (empty conf per spec), and the CP runs the
    //    update off the inbound-CALL path.
    server
        .update_firmware(
            cp_id,
            "ftp://example.test/firmware.bin",
            chrono::Utc::now(),
            None,
            None,
        )
        .await
        .expect("update_firmware resolves");

    // The simulated update reports the full happy-path progression, in order.
    recv_status(&mut fw_rx, FirmwareStatus::Downloading).await;
    recv_status(&mut fw_rx, FirmwareStatus::Downloaded).await;
    recv_status(&mut fw_rx, FirmwareStatus::Installing).await;
    recv_status(&mut fw_rx, FirmwareStatus::Installed).await;

    // 2. TriggerMessage(FirmwareStatusNotification) reports the *current* status
    //    (Installed) without re-running the update.
    let status = server
        .trigger_message(cp_id, MessageTrigger::FirmwareStatusNotification, None)
        .await
        .expect("trigger_message(Firmware) resolves");
    assert_eq!(
        status,
        TriggerMessageStatus::Accepted,
        "the CP now supports a FirmwareStatusNotification trigger"
    );
    recv_status(&mut fw_rx, FirmwareStatus::Installed).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
