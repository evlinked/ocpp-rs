//! End-to-end: a real [`ChargePoint`] boots against an in-process CSMS backed
//! by the default central-system handler set (Issue #40).
//!
//! This wires the two halves the M2/M4 milestones care about: the
//! `central_system_dispatcher()` "batteries included" responders
//! (`crates/ocpp-transport/src/central_system.rs`) and the charge-point boot
//! sequence (`ChargePoint::connect`). It proves that a CP connecting to a CSMS
//! that only has the default handlers completes its BootNotification handshake
//! and ends up `Accepted` — i.e. the defaults are genuinely spec-valid and the
//! routing introduced in #39 carries the frames end to end.
//!
//! The 2.0.1 twin (Issue #418) mirrors this for a `for_version(V201)` CP against
//! a `central_system_service_v201` CSMS, exercising the version-aware runtime
//! (2.0.1 `BootNotification` / `StatusNotification`, response normalization)
//! landed in #419 end to end.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::RegistrationStatus;
use ocpp_messages::v201::{
    BootNotificationRequest as V201BootNotificationRequest,
    BootNotificationResponse as V201BootNotificationResponse, ResetRequest as V201ResetRequest,
    TransactionEventRequest, TransactionEventResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{
    central_system_dispatcher, central_system_dispatcher_with, central_system_service_v201,
    CentralSystemConfig, CentralSystemConfigV201, DispatchHandler, TransportConfig,
};
use ocpp_types::common::Reason;
use ocpp_types::v201::{
    BootReasonEnumType, ReasonEnumType, RegistrationStatusEnumType, ResetEnumType,
    ResetStatusEnumType, TransactionEventEnumType, TriggerReasonEnumType,
};
use ocpp_types::{ConnectorId, OcppVersion};

/// Start an in-process CSMS whose dispatcher is built from `dispatcher` and
/// return it alongside the bound address (random free port).
async fn start_csms(dispatcher: ocpp_messages::ActionDispatcher) -> (OcppServer, SocketAddr) {
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
        // Keep the test deterministic: no background reconnect storm.
        auto_reconnect: false,
        ..ChargePointConfig::default()
    }
}

#[tokio::test]
async fn charge_point_boots_against_default_central_system() {
    let (mut server, addr) = start_csms(central_system_dispatcher()).await;

    let cp = ChargePoint::new(cp_config(addr, "CP_BOOT_01")).expect("build charge point");

    // connect() runs the boot sequence (BootNotification -> Accepted) internally
    // and only returns Ok once the CSMS has accepted the charge point.
    cp.connect().await.expect("connect + boot sequence");

    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert_eq!(
        cp.registration_status().await,
        RegistrationStatus::Accepted,
        "default CSMS must accept the BootNotification"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn charge_point_boot_rejected_when_csms_configured_to_reject() {
    // A CSMS that always Rejects, with a short retry interval so the CP's
    // bounded retry loop gives up quickly instead of sleeping for the default.
    let dispatcher = central_system_dispatcher_with(CentralSystemConfig {
        boot_interval: 1,
        registration_status: RegistrationStatus::Rejected,
    });
    let (mut server, addr) = start_csms(dispatcher).await;

    // max_boot_retries: 0 -> a single attempt, no waiting between retries.
    let config = ChargePointConfig {
        max_boot_retries: 0,
        ..cp_config(addr, "CP_BOOT_REJECT")
    };
    let cp = ChargePoint::new(config).expect("build charge point");

    // Boot is rejected, so connect() surfaces the failure rather than hanging.
    let result = tokio::time::timeout(Duration::from_secs(10), cp.connect()).await;
    let connect_result = result.expect("connect should resolve, not hang");
    assert!(
        connect_result.is_err(),
        "connect must fail when the CSMS rejects the BootNotification"
    );
    assert_eq!(
        cp.registration_status().await,
        RegistrationStatus::Rejected,
        "registration status should reflect the rejection"
    );

    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 twin (Issue #418): the same boot handshake, but a
// `for_version(V201)` ChargePoint against a `central_system_service_v201` CSMS.
// Both directions are `SchemaValidator::v201()`-backed, so a green `connect()`
// proves the CP negotiated `ocpp2.0.1`, emitted a schema-valid 2.0.1
// `BootNotification`, parsed the 2.0.1 `BootNotificationResponse`, and announced
// its connectors with the 2.0.1 `StatusNotification` shape — end to end. This is
// the regression guard for the version-aware runtime landed in #419, which had
// no CP<->CSMS integration coverage of its own.
// ---------------------------------------------------------------------------

/// Start an in-process 2.0.1 CSMS (`central_system_service_v201`, both
/// directions `SchemaValidator::v201()`-backed) and return it with its bound
/// address (random free port). `customize` runs against the default lifecycle
/// dispatcher before it is shared into the handler; pass `|_| {}` for the
/// pure-defaults CSMS.
async fn start_v201_csms(
    customize: impl FnOnce(&mut ActionDispatcher),
) -> (OcppServer, SocketAddr) {
    let (mut server, _events) = central_system_service_v201(
        CentralSystemConfigV201::default(),
        TransportConfig::default(),
        customize,
    );
    server
        .start("127.0.0.1:0")
        .await
        .expect("v201 server start");
    let addr = server.local_addr().expect("v201 server local addr");
    (server, addr)
}

/// A `for_version(V201)` config pointed at `addr` (so it offers `ocpp2.0.1` and
/// speaks the 2.0.1 runtime), with background reconnect disabled to keep the
/// test deterministic.
fn v201_cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        auto_reconnect: false,
        ..ChargePointConfig::for_version(OcppVersion::V201)
    }
}

#[tokio::test]
async fn v201_charge_point_boots_against_default_v201_central_system() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_BOOT")).expect("build v201 charge point");

    // connect() drives the full 2.0.1 boot handshake internally — negotiate
    // `ocpp2.0.1`, send the v201 `BootNotification`, process the v201
    // `BootNotificationResponse` (Accepted -> start heartbeat), then announce
    // every connector as Available via the 2.0.1 `StatusNotification` shape. It
    // only returns Ok once the CSMS has accepted the CP *and* acknowledged the
    // connector announcements, so a green connect exercises the whole path.
    cp.connect().await.expect("v201 connect + boot sequence");

    assert!(
        cp.is_connected().await,
        "v201 CP should be connected after boot"
    );
    assert_eq!(
        cp.registration_status().await,
        // boot_sequence normalizes the 2.0.1 `RegistrationStatusEnumType` onto
        // the canonical 1.6J `RegistrationStatus`, so Accepted reads through.
        RegistrationStatus::Accepted,
        "default v201 CSMS must accept the 2.0.1 BootNotification"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_boot_notification_carries_spec_shape_over_the_wire() {
    // Capture the `BootNotification` the CSMS actually receives, so we assert the
    // *runtime* emits the correct 2.0.1 wire shape end to end — not just the
    // slice-1 builder in isolation. The request reaching this handler has already
    // passed the server's inbound `SchemaValidator::v201()`, so it is schema-valid
    // by construction; here we pin the identity mapping and the boot reason.
    let received: Arc<Mutex<Option<V201BootNotificationRequest>>> = Arc::new(Mutex::new(None));

    let received_for_handler = received.clone();
    let (mut server, addr) = start_v201_csms(move |dispatcher| {
        let received = received_for_handler;
        dispatcher.on(move |req: V201BootNotificationRequest| {
            let received = received.clone();
            async move {
                *received.lock().expect("capture mutex not poisoned") = Some(req);
                Ok(V201BootNotificationResponse {
                    // Mirror the default handler's timestamp format so the
                    // server's outbound v201 validation is satisfied.
                    current_time: chrono::Utc::now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    interval: 300,
                    status: RegistrationStatusEnumType::Accepted,
                    status_info: None,
                    custom_data: None,
                })
            }
        });
    })
    .await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_SHAPE")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // connect() only returns after the boot CALLRESULT, which the capturing
    // handler produces *after* storing the request, so this is populated with no
    // timing dependency (no sleep / poll).
    let boot = received
        .lock()
        .expect("capture mutex not poisoned")
        .clone()
        .expect("CSMS should have received a 2.0.1 BootNotification");

    assert_eq!(
        boot.reason,
        BootReasonEnumType::PowerUp,
        "a fresh-boot simulator announces reason = PowerUp"
    );
    // Identity maps from the default 1.6J `ChargePointVendorInfo` onto the 2.0.1
    // `ChargingStationType` (see `ChargePointConfig::v201_boot_notification_request`).
    assert_eq!(boot.charging_station.vendor_name, "OCPP-RS");
    assert_eq!(boot.charging_station.model, "Simulator");
    assert_eq!(
        boot.charging_station.serial_number.as_deref(),
        Some("SIM001")
    );
    assert_eq!(
        boot.charging_station.firmware_version.as_deref(),
        Some("1.0.0")
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 CSMS -> CP `Reset` (Issue #428, slice 4b): the CSMS initiates a
// `Reset` CALL against a `for_version(V201)` CP over a real socket, exercising
// the version-aware inbound dispatcher wired to the slice-4a decision logic
// (`crates/ocpp-cp/src/v201_command.rs`, #427). A green run proves an inbound
// 2.0.1 `Reset.req` routes to the v201 handler, returns a schema-valid
// `Reset.conf`, and — for an accepted reset — actually drives the reset
// side-effect through `perform_reset`.
// ---------------------------------------------------------------------------

/// Shared counter of `BootNotification`s the CSMS has received.
type BootCount = Arc<std::sync::atomic::AtomicUsize>;

/// Start an in-process 2.0.1 CSMS that counts every `BootNotification` it
/// receives (still replying `Accepted`), on top of the default 2.0.1 lifecycle
/// responders. Returns the server, its address, and the shared count. The count
/// lets a test observe the reset side-effect: a soft reset re-runs the boot
/// sequence, so a second `BootNotification` arrives on the same socket.
async fn start_v201_csms_counting_boots() -> (OcppServer, SocketAddr, BootCount) {
    let boots: BootCount = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let boots_for_handler = boots.clone();
    let (server, addr) = start_v201_csms(move |dispatcher| {
        let boots = boots_for_handler;
        dispatcher.on(move |_req: V201BootNotificationRequest| {
            let boots = boots.clone();
            async move {
                boots.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(V201BootNotificationResponse {
                    current_time: chrono::Utc::now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    interval: 300,
                    status: RegistrationStatusEnumType::Accepted,
                    status_info: None,
                    custom_data: None,
                })
            }
        });
    })
    .await;
    (server, addr, boots)
}

/// Poll `boots` until it reaches at least `target`, or panic after ~5s. Used to
/// wait for the reset's re-boot without a fixed sleep.
async fn wait_for_boot_count(boots: &BootCount, target: usize) {
    for _ in 0..250 {
        if boots.load(std::sync::atomic::Ordering::SeqCst) >= target {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "timed out waiting for boot count to reach {target} (last saw {})",
        boots.load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[tokio::test]
async fn v201_reset_onidle_while_idle_is_accepted_and_reboots() {
    let (mut server, addr, boots) = start_v201_csms_counting_boots().await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_RESET_IDLE"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    // connect() returns only after the first boot handshake completed.
    assert_eq!(
        boots.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "exactly one BootNotification before the reset"
    );
    assert!(server.is_cp_connected("CP201_RESET_IDLE"));

    // CSMS -> CP: an `OnIdle` reset while the station is idle. The pure decision
    // (slice 4a) returns `Accepted`, and the wiring drives `perform_reset` off
    // the CALL path. `server.call` returns the typed `Reset.conf`.
    let resp = server
        .call::<V201ResetRequest>(
            "CP201_RESET_IDLE",
            V201ResetRequest {
                kind: ResetEnumType::OnIdle,
                evse_id: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS Reset call round-trips");
    assert_eq!(
        resp.status,
        ResetStatusEnumType::Accepted,
        "an OnIdle reset on an idle station is Accepted"
    );

    // The accepted reset maps `OnIdle -> Soft`, so `perform_reset` re-runs the
    // boot sequence in place: a second BootNotification proves the side-effect
    // fired (the CALLRESULT is flushed first, so this happens after the call).
    wait_for_boot_count(&boots, 2).await;
    assert!(
        cp.is_connected().await,
        "a soft reset reboots in place, so the CP stays connected"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// OCPP 2.0.1 deferred `Reset(OnIdle)` (Issue #431, slice 4c): an `OnIdle` reset
// received mid-transaction is answered `Scheduled` (slice 4b) and must then be
// carried out automatically the moment the in-flight transaction ends — the
// station reboots on idle, never mid-charge. This extends the slice-4b
// "does not interrupt" scenario with the deferred-reboot tail.
#[tokio::test]
async fn v201_reset_onidle_scheduled_then_reboots_once_the_transaction_ends() {
    // A CSMS that records TransactionEvents (so a v201 charging session runs) and
    // counts boots (so we can observe both that the deferred reset does NOT reboot
    // while charging, and that it DOES once the transaction ends).
    let boots: BootCount = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let log: TxnLog = Arc::new(Mutex::new(Vec::new()));
    let boots_for_handler = boots.clone();
    let log_for_handler = log.clone();
    let (mut server, addr) = start_v201_csms(move |dispatcher| {
        let boots = boots_for_handler;
        dispatcher.on(move |_req: V201BootNotificationRequest| {
            let boots = boots.clone();
            async move {
                boots.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(V201BootNotificationResponse {
                    current_time: chrono::Utc::now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    interval: 300,
                    status: RegistrationStatusEnumType::Accepted,
                    status_info: None,
                    custom_data: None,
                })
            }
        });
        let log = log_for_handler;
        dispatcher.on(move |req: TransactionEventRequest| {
            let log = log.clone();
            async move {
                log.lock().expect("txn log mutex not poisoned").push(req);
                Ok(TransactionEventResponse::default())
            }
        });
    })
    .await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_RESET_BUSY"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Start a transaction so the station is no longer idle.
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "RFID-CAFE", 1000)
        .await
        .expect("v201 start_transaction");
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let boots_before = boots.load(std::sync::atomic::Ordering::SeqCst);

    // CSMS -> CP: an `OnIdle` reset while a transaction is in progress. The pure
    // decision returns `Scheduled` (accept but defer until idle); the wiring
    // queues no side-effect, so the live session is untouched.
    let resp = server
        .call::<V201ResetRequest>(
            "CP201_RESET_BUSY",
            V201ResetRequest {
                kind: ResetEnumType::OnIdle,
                evse_id: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS Reset call round-trips");
    assert_eq!(
        resp.status,
        ResetStatusEnumType::Scheduled,
        "an OnIdle reset during a transaction is Scheduled (deferred)"
    );

    // The Scheduled reset must not have rebooted the station or torn down the
    // transaction: stopping it still succeeds (it is still active), which it
    // would not if the reset had force-stopped it.
    assert_eq!(
        boots.load(std::sync::atomic::Ordering::SeqCst),
        boots_before,
        "a Scheduled reset does not reboot while the transaction is live"
    );

    // End the transaction normally. This is the station's idle transition, and it
    // is where the armed deferred reset fires: `stop_transaction` re-enqueues the
    // mapped `Reset` on the command channel once `active_transactions` drains.
    cp.stop_transaction(txn_id, 2000, Reason::EVDisconnected)
        .await
        .expect("the still-live transaction stops normally after a Scheduled reset");

    // The deferred reset now carries out: `OnIdle -> Soft`, so `perform_reset`
    // re-runs the boot sequence in place and a second BootNotification lands on
    // the same socket (waited on, no fixed sleep). This proves the `Scheduled`
    // reset was not merely acknowledged but actually deferred and then executed.
    wait_for_boot_count(&boots, boots_before + 1).await;
    assert!(
        cp.is_connected().await,
        "the deferred soft reset reboots in place, so the CP stays connected"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// OCPP 2.0.1 deferred reset — negative / exactly-once guards (Issue #431, slice
// 4c). Two properties the transaction-stop hot path must hold:
//   1. A plain transaction stop with NO armed deferred reset must not reboot the
//      station (guard against a false trigger on every stop).
//   2. The deferred reset, once fired, is single-shot: a *second* transaction on
//      the same idle-armed station does not re-trigger the already-consumed reset.
#[tokio::test]
async fn v201_transaction_stop_without_armed_reset_does_not_reboot() {
    let boots: BootCount = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let log: TxnLog = Arc::new(Mutex::new(Vec::new()));
    let boots_for_handler = boots.clone();
    let log_for_handler = log.clone();
    let (mut server, addr) = start_v201_csms(move |dispatcher| {
        let boots = boots_for_handler;
        dispatcher.on(move |_req: V201BootNotificationRequest| {
            let boots = boots.clone();
            async move {
                boots.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(V201BootNotificationResponse {
                    current_time: chrono::Utc::now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    interval: 300,
                    status: RegistrationStatusEnumType::Accepted,
                    status_info: None,
                    custom_data: None,
                })
            }
        });
        let log = log_for_handler;
        dispatcher.on(move |req: TransactionEventRequest| {
            let log = log.clone();
            async move {
                log.lock().expect("txn log mutex not poisoned").push(req);
                Ok(TransactionEventResponse::default())
            }
        });
    })
    .await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_NO_RESET")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    assert_eq!(
        boots.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "exactly one BootNotification after connect"
    );

    // A full transaction with NO reset command in between.
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "RFID-CAFE", 1000)
        .await
        .expect("v201 start_transaction");
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    cp.stop_transaction(txn_id, 2000, Reason::EVDisconnected)
        .await
        .expect("v201 stop_transaction");
    wait_for_event(&log, TransactionEventEnumType::Ended).await;

    // Now drive a real `OnIdle` reset on the (idle) station as a deterministic
    // happens-after barrier: it is `Accepted` and reboots, taking the count to 2.
    // Had the plain stop above spuriously armed/fired a reset, that reboot would
    // have queued *first*, pushing the eventual count past 2 — so asserting the
    // count settles at exactly 2 catches a false trigger without a bare sleep.
    let resp = server
        .call::<V201ResetRequest>(
            "CP201_NO_RESET",
            V201ResetRequest {
                kind: ResetEnumType::OnIdle,
                evse_id: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS Reset call round-trips");
    assert_eq!(
        resp.status,
        ResetStatusEnumType::Accepted,
        "an OnIdle reset on the now-idle station is Accepted"
    );
    wait_for_boot_count(&boots, 2).await;

    // Give any erroneously-queued extra reboot time to land, then assert exactly
    // one reboot happened: the plain stop rebooted nothing, and the explicit reset
    // fired exactly once.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        boots.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "only the explicit OnIdle reset reboots; a plain stop triggers nothing"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 charging session (Issue #423, slice 3b): a `for_version(V201)` CP
// runs a full start -> periodic -> stop session against a
// `SchemaValidator::v201()`-backed CSMS that captures every `TransactionEvent`.
// The unified 2.0.1 `TransactionEvent` message replaces the 1.6J
// `StartTransaction` / `MeterValues` / `StopTransaction` triad, so a green run
// proves the live transactional loop speaks 2.0.1 end to end: the three events
// arrive in order, `seqNo` is strictly increasing from 0, and the
// station-chosen `transactionId` is stable across all of them.
// ---------------------------------------------------------------------------

/// Shared, ordered log of every `TransactionEvent` the CSMS received.
type TxnLog = Arc<Mutex<Vec<TransactionEventRequest>>>;

/// Start an in-process 2.0.1 CSMS that records every `TransactionEvent` it
/// receives (replacing the default empty-ack handler with a recording one that
/// still returns the empty `{}` ack), on top of the default 2.0.1 lifecycle
/// responders. Returns the server, its address, and the shared log.
async fn start_v201_csms_recording_txns() -> (OcppServer, SocketAddr, TxnLog) {
    let log: TxnLog = Arc::new(Mutex::new(Vec::new()));
    let log_for_handler = log.clone();
    let (server, addr) = start_v201_csms(move |dispatcher| {
        let log = log_for_handler;
        dispatcher.on(move |req: TransactionEventRequest| {
            let log = log.clone();
            async move {
                log.lock().expect("txn log mutex not poisoned").push(req);
                // Empty ack, exactly like the default 2.0.1 CSMS handler — no
                // cost / authorization policy, so `idTokenInfo` is absent and the
                // CP treats the start as an implicit accept.
                Ok(TransactionEventResponse::default())
            }
        });
    })
    .await;
    (server, addr, log)
}

/// Poll `log` until it contains at least one event of `event_type`, or panic
/// after ~5s. Used to wait for the background meter sampler to emit its first
/// `Updated` before stopping the transaction, without a fixed sleep.
async fn wait_for_event(log: &TxnLog, event_type: TransactionEventEnumType) {
    for _ in 0..250 {
        {
            let events = log.lock().expect("txn log mutex not poisoned");
            if events.iter().any(|e| e.event_type == event_type) {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for a {event_type:?} TransactionEvent");
}

#[tokio::test]
async fn v201_charging_session_emits_transaction_events_in_order() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    // A 1s meter interval so the periodic `Updated` sampler ticks promptly; the
    // test waits on the recorded log rather than sleeping a fixed duration.
    let config = ChargePointConfig {
        meter_values_interval: 1,
        ..v201_cp_config(addr, "CP201_TXN")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Start a transaction on connector 1 -> emits TransactionEvent(Started).
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "RFID-CAFE", 1000)
        .await
        .expect("v201 start_transaction");
    // The station mints the first transactionId as 1 (rendered "1" on the wire).
    assert_eq!(txn_id, 1, "V201 station mints the transaction id");

    // Wait for the background sampler to emit at least one periodic `Updated`.
    wait_for_event(&log, TransactionEventEnumType::Updated).await;

    // Stop the transaction (EV unplugged) -> emits TransactionEvent(Ended).
    cp.stop_transaction(txn_id, 2000, Reason::EVDisconnected)
        .await
        .expect("v201 stop_transaction");

    let events = log.lock().expect("txn log mutex not poisoned").clone();

    // At minimum: Started, >=1 Updated, Ended.
    assert!(
        events.len() >= 3,
        "expected at least Started + Updated + Ended, got {} events",
        events.len()
    );

    let first = &events[0];
    let last = &events[events.len() - 1];
    assert_eq!(
        first.event_type,
        TransactionEventEnumType::Started,
        "first event must be Started"
    );
    assert_eq!(
        last.event_type,
        TransactionEventEnumType::Ended,
        "last event must be Ended"
    );
    // Everything between the first and last is an Updated.
    for mid in &events[1..events.len() - 1] {
        assert_eq!(
            mid.event_type,
            TransactionEventEnumType::Updated,
            "mid-session events must be Updated"
        );
    }

    // seqNo strictly increasing, starting at 0 on Started.
    assert_eq!(first.seq_no, 0, "Started carries seqNo 0");
    for pair in events.windows(2) {
        assert!(
            pair[1].seq_no > pair[0].seq_no,
            "seqNo must strictly increase: {} then {}",
            pair[0].seq_no,
            pair[1].seq_no
        );
    }

    // transactionId is stable across every event of the session.
    for e in &events {
        assert_eq!(
            e.transaction_info.transaction_id, "1",
            "transactionId must be stable across all events"
        );
        // Each event reports the same EVSE the 1.6J connector maps onto.
        let evse = e.evse.as_ref().expect("every event carries evse");
        assert_eq!(evse.id, 1, "connector 1 maps to evse 1");
        assert_eq!(evse.connector_id, Some(1));
    }

    // The Started event authorizes with the presented idToken; the Ended event
    // carries the mapped stop reason and trigger for an EV unplug.
    let started_token = first
        .id_token
        .as_ref()
        .expect("Started carries the authorizing idToken");
    assert_eq!(started_token.id_token, "RFID-CAFE");
    assert_eq!(
        last.trigger_reason,
        TriggerReasonEnumType::EVDeparted,
        "EV unplug maps to an EVDeparted trigger"
    );
    assert_eq!(
        last.transaction_info.stopped_reason,
        Some(ReasonEnumType::EVDisconnected),
        "EV unplug maps to an EVDisconnected stopped-reason"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
