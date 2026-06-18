//! End-to-end CS→CP TriggerMessage test (Issue #61).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the M5 `TriggerMessage` command (OCPP 1.6J §4.x)
//! through the `OcppServer::trigger_message` helper, asserting the CP not only
//! answers `TriggerMessageStatus` faithfully but **actually sends** the
//! requested message off the inbound-CALL path:
//!
//!   1. `trigger_message(BootNotification)` → `Accepted`, and the CSMS then
//!      observes a fresh `BootNotification` CALL from the CP.
//!   2. `trigger_message(StatusNotification, Some(1))` → `Accepted`, and the
//!      CSMS observes a `StatusNotification` scoped to connector 1.
//!   3. `trigger_message(DiagnosticsStatusNotification)` → `NotImplemented`
//!      (a message this CP cannot produce), and nothing is sent.
//!
//! Rust counterpart of the Python reference's central system driving
//! `TriggerMessage`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against the `@on('TriggerMessage')` charge point.

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
use ocpp_types::common::{AuthorizationStatus, IdTagInfo, ReadingContext};
use ocpp_types::v16j::{MessageTrigger, RemoteStartStopStatus, TriggerMessageStatus};
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Bound on how long a triggered message may take to reach the CSMS before the
/// test gives up. Generous so a loaded CI box doesn't flake.
const TRIGGER_TIMEOUT: Duration = Duration::from_secs(5);

/// A CSMS dispatcher that records the `BootNotification` and
/// `StatusNotification` CALLs the CP sends, so the test can assert a
/// `TriggerMessage` actually produced the requested message (not just that it
/// was acknowledged).
fn recording_csms_dispatcher(
    boot_tx: mpsc::UnboundedSender<()>,
    status_tx: mpsc::UnboundedSender<u32>,
) -> ActionDispatcher {
    let mut d = ActionDispatcher::new();

    {
        let boot_tx = boot_tx.clone();
        d.on(move |_req: BootNotificationRequest| {
            let boot_tx = boot_tx.clone();
            async move {
                let _ = boot_tx.send(());
                Ok(BootNotificationResponse {
                    current_time: chrono::Utc::now(),
                    // A long interval keeps stray heartbeats from racing the asserts.
                    interval: 300,
                    status: RegistrationStatus::Accepted,
                })
            }
        });
    }
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
                let _ = status_tx.send(req.connector_id);
                Ok(StatusNotificationResponse {})
            }
        });
    }
    d.on(|_req: AuthorizeRequest| async move {
        Ok(AuthorizeResponse {
            id_tag_info: IdTagInfo {
                status: AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: None,
            },
        })
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
        connector_count: 1,
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        ..ChargePointConfig::default()
    }
}

/// Discard whatever the boot handshake already recorded (its `BootNotification`
/// and the per-connector `StatusNotification`s), so later assertions see only
/// the messages a `TriggerMessage` produced. `connect()` has already awaited
/// those CALLs' responses, so they are present and the drain is race-free.
fn drain<T>(rx: &mut mpsc::UnboundedReceiver<T>) {
    while rx.try_recv().is_ok() {}
}

#[tokio::test]
async fn csms_trigger_message_drives_cp_to_send_requested_messages() {
    let cp_id = "CP_TRIGGER_01";
    let (boot_tx, mut boot_rx) = mpsc::unbounded_channel();
    let (status_tx, mut status_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_csms_dispatcher(boot_tx, status_tx)).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // Drop the boot-time BootNotification / StatusNotifications so the asserts
    // below observe only what a TriggerMessage produces.
    drain(&mut boot_rx);
    drain(&mut status_rx);

    // 1. Trigger a BootNotification → Accepted, and the CP actually sends one.
    let status = server
        .trigger_message(cp_id, MessageTrigger::BootNotification, None)
        .await
        .expect("trigger_message(BootNotification) resolves");
    assert_eq!(
        status,
        TriggerMessageStatus::Accepted,
        "the CP supports a BootNotification trigger"
    );
    timeout(TRIGGER_TIMEOUT, boot_rx.recv())
        .await
        .expect("CSMS observes a triggered BootNotification")
        .expect("boot channel open");

    // 2. Trigger a connector-scoped StatusNotification → Accepted, observed for
    //    connector 1.
    let status = server
        .trigger_message(cp_id, MessageTrigger::StatusNotification, Some(1))
        .await
        .expect("trigger_message(StatusNotification) resolves");
    assert_eq!(
        status,
        TriggerMessageStatus::Accepted,
        "the CP supports a StatusNotification trigger"
    );
    let connector_id = timeout(TRIGGER_TIMEOUT, status_rx.recv())
        .await
        .expect("CSMS observes a triggered StatusNotification")
        .expect("status channel open");
    assert_eq!(
        connector_id, 1,
        "the StatusNotification is scoped to the requested connector"
    );

    // 3. Trigger an unsupported message → NotImplemented, and nothing is sent.
    let status = server
        .trigger_message(cp_id, MessageTrigger::DiagnosticsStatusNotification, None)
        .await
        .expect("trigger_message(Diagnostics) resolves");
    assert_eq!(
        status,
        TriggerMessageStatus::NotImplemented,
        "the CP reports NotImplemented for a message it cannot produce"
    );
    // No BootNotification or StatusNotification should follow a NotImplemented.
    assert!(
        timeout(Duration::from_millis(300), boot_rx.recv())
            .await
            .is_err(),
        "an unsupported trigger must not produce a BootNotification"
    );
    assert!(
        timeout(Duration::from_millis(300), status_rx.recv())
            .await
            .is_err(),
        "an unsupported trigger must not produce a StatusNotification"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// Transaction id the recording CSMS hands back for any `StartTransaction`, so
/// the test can assert a triggered `MeterValues` attaches the in-flight id.
const TRIGGERED_TX_ID: i32 = 4242;

/// What the CSMS recorded from a `MeterValues` CALL: which connector, the
/// optional in-flight transaction id, and the `ReadingContext` of the first
/// sample — which lets the test tell a *triggered* read (`Trigger`) apart from
/// the periodic `Transaction.Begin` snapshot the sampler emits at charge start.
#[derive(Debug)]
struct MeterValuesRecord {
    connector_id: u32,
    transaction_id: Option<i32>,
    context: Option<ReadingContext>,
}

fn accepted_id_tag() -> IdTagInfo {
    IdTagInfo {
        status: AuthorizationStatus::Accepted,
        parent_id_tag: None,
        expiry_date: None,
    }
}

/// A CSMS dispatcher that supports a full `StartTransaction` round-trip and
/// records every `MeterValues` CALL, so the test can drive an on-demand
/// `MeterValues` trigger and inspect exactly what the CP sent.
fn recording_meter_values_dispatcher(
    mv_tx: mpsc::UnboundedSender<MeterValuesRecord>,
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
    d.on(|_req: AuthorizeRequest| async move {
        Ok(AuthorizeResponse {
            id_tag_info: accepted_id_tag(),
        })
    });
    d.on(|_req: StartTransactionRequest| async move {
        Ok(StartTransactionResponse {
            id_tag_info: accepted_id_tag(),
            transaction_id: TRIGGERED_TX_ID,
        })
    });
    d.on(|_req: StopTransactionRequest| async move {
        Ok(StopTransactionResponse { id_tag_info: None })
    });
    {
        let mv_tx = mv_tx.clone();
        d.on(move |req: MeterValuesRequest| {
            let mv_tx = mv_tx.clone();
            async move {
                let context = req
                    .meter_values
                    .first()
                    .and_then(|mv| mv.sampled_values.first())
                    .and_then(|sv| sv.context.clone());
                let _ = mv_tx.send(MeterValuesRecord {
                    connector_id: req.connector_id,
                    transaction_id: req.transaction_id,
                    context,
                });
                Ok(MeterValuesResponse {})
            }
        });
    }

    d
}

/// Pull `MeterValues` records until one carries `context`, skipping the rest
/// (e.g. the sampler's `Transaction.Begin` snapshot). Fails the test if no
/// matching frame arrives before [`TRIGGER_TIMEOUT`].
async fn recv_meter_values_with_context(
    rx: &mut mpsc::UnboundedReceiver<MeterValuesRecord>,
    context: ReadingContext,
) -> MeterValuesRecord {
    loop {
        let rec = timeout(TRIGGER_TIMEOUT, rx.recv())
            .await
            .expect("CSMS observes a MeterValues frame")
            .expect("meter-values channel open");
        if rec.context.as_ref() == Some(&context) {
            return rec;
        }
    }
}

#[tokio::test]
async fn csms_trigger_meter_values_reports_current_reading() {
    let cp_id = "CP_TRIGGER_MV";
    let (mv_tx, mut mv_rx) = mpsc::unbounded_channel();
    let (mut server, addr) = start_csms(recording_meter_values_dispatcher(mv_tx)).await;

    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // 1. Idle connector: a triggered MeterValues reports the standing meter
    //    register with ReadingContext::Trigger and no transaction id.
    let status = server
        .trigger_message(cp_id, MessageTrigger::MeterValues, Some(1))
        .await
        .expect("trigger_message(MeterValues) resolves");
    assert_eq!(
        status,
        TriggerMessageStatus::Accepted,
        "the CP now supports an on-demand MeterValues trigger"
    );
    let idle = recv_meter_values_with_context(&mut mv_rx, ReadingContext::Trigger).await;
    assert_eq!(
        idle.connector_id, 1,
        "the triggered MeterValues is scoped to the requested connector"
    );
    assert_eq!(
        idle.transaction_id, None,
        "an idle connector reports its meter with no transaction id"
    );

    // 2. Start a charge remotely; once the sampler's Transaction.Begin snapshot
    //    confirms the transaction is live, a triggered MeterValues attaches the
    //    in-flight transaction id.
    let start = server
        .remote_start_transaction(cp_id, "TAG_MV", Some(1))
        .await
        .expect("remote_start_transaction resolves");
    assert_eq!(
        start,
        RemoteStartStopStatus::Accepted,
        "a free connector accepts the remote start"
    );
    let begin = recv_meter_values_with_context(&mut mv_rx, ReadingContext::TransactionBegin).await;
    assert_eq!(
        begin.transaction_id,
        Some(TRIGGERED_TX_ID),
        "the begin snapshot carries the new transaction id"
    );

    let status = server
        .trigger_message(cp_id, MessageTrigger::MeterValues, Some(1))
        .await
        .expect("trigger_message(MeterValues) resolves");
    assert_eq!(status, TriggerMessageStatus::Accepted);
    let active = recv_meter_values_with_context(&mut mv_rx, ReadingContext::Trigger).await;
    assert_eq!(active.connector_id, 1);
    assert_eq!(
        active.transaction_id,
        Some(TRIGGERED_TX_ID),
        "a charging connector attaches the in-flight transaction id to the triggered MeterValues"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
