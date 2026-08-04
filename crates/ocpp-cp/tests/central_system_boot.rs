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

use ocpp_cp::{ChargePoint, ChargePointConfig, UnlockConnectorOutcome};
use ocpp_messages::v16j::RegistrationStatus;
use ocpp_messages::v201::{
    BootNotificationRequest as V201BootNotificationRequest,
    BootNotificationResponse as V201BootNotificationResponse,
    ChangeAvailabilityRequest as V201ChangeAvailabilityRequest,
    MeterValuesRequest as V201MeterValuesRequest, MeterValuesResponse as V201MeterValuesResponse,
    RequestStartTransactionRequest as V201RequestStartTransactionRequest,
    ResetRequest as V201ResetRequest, StatusNotificationRequest as V201StatusNotificationRequest,
    StatusNotificationResponse as V201StatusNotificationResponse, TransactionEventRequest,
    TransactionEventResponse, TriggerMessageRequest as V201TriggerMessageRequest,
    UnlockConnectorRequest as V201UnlockConnectorRequest,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{
    central_system_dispatcher, central_system_dispatcher_with, central_system_service_v201,
    CentralSystemConfig, CentralSystemConfigV201, DispatchHandler, TransportConfig,
};
use ocpp_types::common::Reason;
use ocpp_types::v201::{
    BootReasonEnumType, ChangeAvailabilityStatusEnumType, ChargingProfileKindEnumType,
    ChargingProfilePurposeEnumType, ChargingProfileType, ChargingRateUnitEnumType,
    ChargingSchedulePeriodType, ChargingScheduleType, ConnectorStatusEnumType, EvseType,
    IdTokenEnumType, IdTokenType, MessageTriggerEnumType, OperationalStatusEnumType,
    ReadingContextEnumType, ReasonEnumType, RegistrationStatusEnumType,
    RequestStartStopStatusEnumType, ResetEnumType, ResetStatusEnumType, TransactionEventEnumType,
    TriggerMessageStatusEnumType, TriggerReasonEnumType, UnlockStatusEnumType,
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

// ---------------------------------------------------------------------------
// OCPP 2.0.1 TriggerMessage (Issue #433, slice 5b): a CSMS sends a
// `TriggerMessage` CALL against a `for_version(V201)` CP over a real socket,
// exercising the version-aware inbound dispatcher wired to the slice-5a decision
// logic (`crates/ocpp-cp/src/v201_command.rs`, #434). A green run proves an
// inbound 2.0.1 `TriggerMessage.req` routes to the v201 handler, returns a
// schema-valid `TriggerMessage.conf`, and — for an accepted trigger — actually
// emits the requested message after the CALLRESULT is flushed.
// ---------------------------------------------------------------------------

/// Shared, ordered log of every 2.0.1 `MeterValues` the CSMS received.
type MeterValuesLog = Arc<Mutex<Vec<V201MeterValuesRequest>>>;

/// Start an in-process 2.0.1 CSMS that both counts `BootNotification`s and
/// records every standalone `MeterValues` it receives (on top of the default
/// 2.0.1 lifecycle responders). Returns the server, its address, the boot count,
/// and the meter-values log — enough to observe every message a `TriggerMessage`
/// side effect can emit.
async fn start_v201_csms_recording_triggers() -> (OcppServer, SocketAddr, BootCount, MeterValuesLog)
{
    let boots: BootCount = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let meter_values: MeterValuesLog = Arc::new(Mutex::new(Vec::new()));
    let boots_for_handler = boots.clone();
    let mv_for_handler = meter_values.clone();
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
        let meter_values = mv_for_handler;
        dispatcher.on(move |req: V201MeterValuesRequest| {
            let meter_values = meter_values.clone();
            async move {
                meter_values
                    .lock()
                    .expect("meter values log mutex not poisoned")
                    .push(req);
                Ok(V201MeterValuesResponse::default())
            }
        });
    })
    .await;
    (server, addr, boots, meter_values)
}

/// Poll `meter_values` until it holds at least `target` entries, or panic after
/// ~5s. Used to wait for a triggered `MeterValues` without a fixed sleep.
async fn wait_for_meter_values(meter_values: &MeterValuesLog, target: usize) {
    for _ in 0..250 {
        if meter_values
            .lock()
            .expect("meter values log mutex not poisoned")
            .len()
            >= target
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "timed out waiting for {target} MeterValues (last saw {})",
        meter_values
            .lock()
            .expect("meter values log mutex not poisoned")
            .len()
    );
}

#[tokio::test]
async fn v201_trigger_boot_notification_is_accepted_and_reboots() {
    let (mut server, addr, boots) = start_v201_csms_counting_boots().await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_TRIG_BOOT")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    // connect() returns only after the first boot handshake completed.
    assert_eq!(
        boots.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "exactly one BootNotification before the trigger"
    );

    // CSMS -> CP: TriggerMessage(BootNotification). The pure decision (slice 5a)
    // returns Accepted, and the wiring enqueues the emission off the CALL path.
    let resp = server
        .call::<V201TriggerMessageRequest>(
            "CP201_TRIG_BOOT",
            V201TriggerMessageRequest {
                requested_message: MessageTriggerEnumType::BootNotification,
                evse: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS TriggerMessage call round-trips");
    assert_eq!(
        resp.status,
        TriggerMessageStatusEnumType::Accepted,
        "BootNotification is a message the v201 CP emits, so the trigger is Accepted"
    );

    // The accepted trigger emits a second BootNotification after the CALLRESULT
    // is flushed — proof the side-effect fired off the CALL path.
    wait_for_boot_count(&boots, 2).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_trigger_meter_values_is_accepted_and_emits_meter_values() {
    let (mut server, addr, _boots, meter_values) = start_v201_csms_recording_triggers().await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_TRIG_MV")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: TriggerMessage(MeterValues) scoped to EVSE 1. 2.0.1 keeps a
    // standalone MeterValues message, so the CP answers Accepted and emits the
    // EVSE's current reading in its own CALL — no transaction needed. Scoping to
    // one EVSE makes the emit count deterministic regardless of connector count.
    let resp = server
        .call::<V201TriggerMessageRequest>(
            "CP201_TRIG_MV",
            V201TriggerMessageRequest {
                requested_message: MessageTriggerEnumType::MeterValues,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS TriggerMessage call round-trips");
    assert_eq!(resp.status, TriggerMessageStatusEnumType::Accepted);

    // The EVSE-1-scoped trigger emits exactly one MeterValues after the
    // CALLRESULT. Waited on, not slept.
    wait_for_meter_values(&meter_values, 1).await;
    let recorded = meter_values
        .lock()
        .expect("meter values log mutex not poisoned")
        .clone();
    assert_eq!(
        recorded.len(),
        1,
        "EVSE-1-scoped trigger -> one MeterValues"
    );
    let mv = &recorded[0];
    assert_eq!(
        mv.evse_id, 1,
        "the reading is reported for the targeted EVSE 1"
    );
    let sample = &mv.meter_value[0].sampled_value[0];
    assert_eq!(
        sample.context,
        Some(ReadingContextEnumType::Trigger),
        "a triggered reading is tagged ReadingContext::Trigger"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_trigger_unsupported_message_is_not_implemented_and_emits_nothing() {
    let (mut server, addr, boots, meter_values) = start_v201_csms_recording_triggers().await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_TRIG_NIMP")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    assert_eq!(boots.load(std::sync::atomic::Ordering::SeqCst), 1);

    // CSMS -> CP: TriggerMessage(FirmwareStatusNotification). The simulator has no
    // firmware state machine on the v201 path, so slice 5a classifies it
    // NotImplemented and the wiring enqueues nothing.
    let resp = server
        .call::<V201TriggerMessageRequest>(
            "CP201_TRIG_NIMP",
            V201TriggerMessageRequest {
                requested_message: MessageTriggerEnumType::FirmwareStatusNotification,
                evse: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS TriggerMessage call round-trips");
    assert_eq!(
        resp.status,
        TriggerMessageStatusEnumType::NotImplemented,
        "a message the simulator can't produce is NotImplemented"
    );

    // Absence is hard to assert directly, so drive a *supported* trigger after it
    // as a deterministic happens-after barrier: once the MeterValues emitted by
    // the second trigger arrives, the first (NotImplemented) trigger has fully
    // run and must have emitted nothing — the meter-values log holds exactly the
    // one MeterValues and no extra BootNotification slipped in. Scope to EVSE 1 so
    // the barrier emits exactly one MeterValues.
    let resp = server
        .call::<V201TriggerMessageRequest>(
            "CP201_TRIG_NIMP",
            V201TriggerMessageRequest {
                requested_message: MessageTriggerEnumType::MeterValues,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS TriggerMessage call round-trips");
    assert_eq!(resp.status, TriggerMessageStatusEnumType::Accepted);
    wait_for_meter_values(&meter_values, 1).await;

    assert_eq!(
        meter_values
            .lock()
            .expect("meter values log mutex not poisoned")
            .len(),
        1,
        "the NotImplemented trigger emitted no MeterValues; only the supported one did"
    );
    assert_eq!(
        boots.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the NotImplemented trigger emitted no BootNotification"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 ChangeAvailability (Issue #436, slice 6b): a CSMS takes a
// `for_version(V201)` CP (or a single EVSE) Operative / Inoperative over a real
// socket, exercising the version-aware inbound dispatcher wired to the slice-6a
// decision logic (`crates/ocpp-cp/src/v201_command.rs`, #435). A green run
// proves an inbound 2.0.1 `ChangeAvailability.req` routes to the v201 handler,
// returns a schema-valid `ChangeAvailability.conf`, and actually applies the
// availability transition — immediately when idle, or deferred to the moment the
// in-flight transaction ends when scheduled.
// ---------------------------------------------------------------------------

/// Shared, ordered log of every 2.0.1 `StatusNotification` the CSMS received.
type StatusLog = Arc<Mutex<Vec<V201StatusNotificationRequest>>>;

/// Start an in-process 2.0.1 CSMS that records every `StatusNotification` it
/// receives (on top of the default 2.0.1 lifecycle responders, whose default
/// `TransactionEvent` empty-ack still runs so a charging session works). Returns
/// the server, its address, and the shared log — enough to observe an
/// availability change reflected as a `StatusNotification`.
async fn start_v201_csms_recording_status() -> (OcppServer, SocketAddr, StatusLog) {
    let log: StatusLog = Arc::new(Mutex::new(Vec::new()));
    let log_for_handler = log.clone();
    let (server, addr) = start_v201_csms(move |dispatcher| {
        let log = log_for_handler;
        dispatcher.on(move |req: V201StatusNotificationRequest| {
            let log = log.clone();
            async move {
                log.lock().expect("status log mutex not poisoned").push(req);
                Ok(V201StatusNotificationResponse::default())
            }
        });
    })
    .await;
    (server, addr, log)
}

/// Count `StatusNotification`s recorded for `evse_id` reporting `status`.
fn status_count(log: &StatusLog, evse_id: i32, status: ConnectorStatusEnumType) -> usize {
    log.lock()
        .expect("status log mutex not poisoned")
        .iter()
        .filter(|s| s.evse_id == evse_id && s.connector_status == status)
        .count()
}

/// Poll `log` until it holds at least `target` `StatusNotification`s for
/// `evse_id` reporting `status`, or panic after ~5s. Used to wait for an applied
/// availability change without a fixed sleep.
async fn wait_for_status_count(
    log: &StatusLog,
    evse_id: i32,
    status: ConnectorStatusEnumType,
    target: usize,
) {
    for _ in 0..250 {
        if status_count(log, evse_id, status) >= target {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "timed out waiting for {target} StatusNotification({status:?}) on evse {evse_id} \
         (last saw {})",
        status_count(log, evse_id, status)
    );
}

#[tokio::test]
async fn v201_change_availability_inoperative_then_operative_round_trips_over_the_wire() {
    let (mut server, addr, status_log) = start_v201_csms_recording_status().await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_AVAIL_RT")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    // Boot announces every connector Available; wait for EVSE 1's so we don't
    // race the announce with the change below.
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Available, 1).await;

    // CSMS -> CP: take EVSE 1 Inoperative while idle. The slice-6a decision
    // returns Accepted, and the wiring applies the change off the CALL path.
    let resp = server
        .call::<V201ChangeAvailabilityRequest>(
            "CP201_AVAIL_RT",
            V201ChangeAvailabilityRequest {
                operational_status: OperationalStatusEnumType::Inoperative,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS ChangeAvailability call round-trips");
    assert_eq!(
        resp.status,
        ChangeAvailabilityStatusEnumType::Accepted,
        "an availability change on an idle EVSE is Accepted"
    );

    // The accepted change flips EVSE 1 to Unavailable and emits the reflecting
    // StatusNotification after the CALLRESULT — proof the side effect fired off
    // the CALL path.
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Unavailable, 1).await;

    // CSMS -> CP: bring EVSE 1 back Operative. Idle -> Accepted, and the change
    // restores Available (a *second* Available for EVSE 1, distinct from the boot
    // announce).
    let resp = server
        .call::<V201ChangeAvailabilityRequest>(
            "CP201_AVAIL_RT",
            V201ChangeAvailabilityRequest {
                operational_status: OperationalStatusEnumType::Operative,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS ChangeAvailability call round-trips");
    assert_eq!(resp.status, ChangeAvailabilityStatusEnumType::Accepted);
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Available, 2).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_change_availability_mid_transaction_is_scheduled_and_applied_on_stop() {
    let (mut server, addr, status_log) = start_v201_csms_recording_status().await;

    // A long meter interval so the periodic sampler does not tick during the test
    // (its TransactionEvents are irrelevant to the status log either way).
    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_AVAIL_SCHED")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Available, 1).await;

    // Start a transaction on connector 1 (EVSE 1) so the station is busy.
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "RFID-CAFE", 0)
        .await
        .expect("v201 start_transaction");

    // CSMS -> CP: take EVSE 1 Inoperative mid-transaction. The slice-6a decision
    // returns Scheduled (accept but defer): the paying session must not be cut off.
    let resp = server
        .call::<V201ChangeAvailabilityRequest>(
            "CP201_AVAIL_SCHED",
            V201ChangeAvailabilityRequest {
                operational_status: OperationalStatusEnumType::Inoperative,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS ChangeAvailability call round-trips");
    assert_eq!(
        resp.status,
        ChangeAvailabilityStatusEnumType::Scheduled,
        "an availability change mid-transaction is deferred, not applied"
    );

    // While charging, nothing arms an apply — no code path can emit Unavailable
    // for EVSE 1 until the transaction ends, so this is deterministic, not a race.
    assert_eq!(
        status_count(&status_log, 1, ConnectorStatusEnumType::Unavailable),
        0,
        "the live session must not be taken Unavailable while charging"
    );

    // End the transaction. The connector returns to Available (stop path), then
    // the deferred change is carried out and flips it Unavailable.
    cp.stop_transaction(txn_id, 0, Reason::EVDisconnected)
        .await
        .expect("v201 stop_transaction");
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Unavailable, 1).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_change_availability_for_an_unknown_evse_is_rejected() {
    let (mut server, addr, status_log) = start_v201_csms_recording_status().await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_AVAIL_REJ")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Available, 1).await;

    // CSMS -> CP: target an EVSE that does not exist on this CP (the default
    // config has 2 connectors, so EVSE 99 is out of range). The station cannot
    // apply the change, so it answers Rejected — the capability outcome the
    // slice-6a policy never produces on its own — and emits nothing.
    let resp = server
        .call::<V201ChangeAvailabilityRequest>(
            "CP201_AVAIL_REJ",
            V201ChangeAvailabilityRequest {
                operational_status: OperationalStatusEnumType::Inoperative,
                evse: Some(EvseType {
                    id: 99,
                    connector_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS ChangeAvailability call round-trips");
    assert_eq!(
        resp.status,
        ChangeAvailabilityStatusEnumType::Rejected,
        "an unknown / out-of-range EVSE target is Rejected"
    );
    // No connector ever reports Unavailable for a rejected change.
    assert_eq!(
        status_count(&status_log, 99, ConnectorStatusEnumType::Unavailable),
        0
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_change_availability_whole_station_takes_every_connector_inoperative() {
    let (mut server, addr, status_log) = start_v201_csms_recording_status().await;

    // The default v201 config exposes 2 connectors (EVSE 1 and EVSE 2), so an
    // `evse`-less whole-station change must apply to both.
    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_AVAIL_ALL")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Available, 1).await;
    wait_for_status_count(&status_log, 2, ConnectorStatusEnumType::Available, 1).await;

    // CSMS -> CP: whole-station Inoperative (no `evse`). Idle -> Accepted, and the
    // wiring fans the change out to every connector.
    let resp = server
        .call::<V201ChangeAvailabilityRequest>(
            "CP201_AVAIL_ALL",
            V201ChangeAvailabilityRequest {
                operational_status: OperationalStatusEnumType::Inoperative,
                evse: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS ChangeAvailability call round-trips");
    assert_eq!(
        resp.status,
        ChangeAvailabilityStatusEnumType::Accepted,
        "a whole-station change on an idle station is Accepted"
    );

    // Both connectors flip to Unavailable after the CALLRESULT.
    wait_for_status_count(&status_log, 1, ConnectorStatusEnumType::Unavailable, 1).await;
    wait_for_status_count(&status_log, 2, ConnectorStatusEnumType::Unavailable, 1).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 RequestStartTransaction (Issue #442, slice 7b): the 2.0.1 successor
// to 1.6J `RemoteStartTransaction`. An inbound `RequestStartTransaction.req`
// routes to the pure slice-7a decision (`v201_request_start_status`) and, on
// `Accepted`, actually begins a transaction on the targeted EVSE off the CALL
// path — observed via a `TransactionEvent(Started)` on the recording CSMS. A busy
// or unknown EVSE is `Rejected` and starts nothing.
// ---------------------------------------------------------------------------

/// Count the `Started` events currently in the transaction log.
fn started_count(log: &TxnLog) -> usize {
    log.lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .filter(|e| e.event_type == TransactionEventEnumType::Started)
        .count()
}

/// A CSMS-central id token, as a remote start would carry.
fn central_id_token(id: &str) -> IdTokenType {
    IdTokenType {
        id_token: id.to_string(),
        kind: IdTokenEnumType::Central,
        additional_info: None,
        custom_data: None,
    }
}

#[tokio::test]
async fn v201_request_start_transaction_accepted_starts_a_transaction() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    // A long meter interval so the only event we assert on is the Started one the
    // remote start produces (the periodic Updated sampler stays quiet).
    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTART")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: remotely start a session on the free EVSE 1. The slice-7a
    // decision returns Accepted, and the wiring queues the local StartTransaction
    // off the CALL path.
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 42,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Accepted,
        "a remote start on a free EVSE is Accepted"
    );

    // The accepted request actually begins a transaction after the CALLRESULT is
    // flushed — proof the side effect fired off the CALL path — targeting EVSE 1.
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let started = log
        .lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .find(|e| e.event_type == TransactionEventEnumType::Started)
        .cloned()
        .expect("a Started event was recorded");
    assert_eq!(
        started.evse.as_ref().map(|e| e.id),
        Some(1),
        "the started transaction targets EVSE 1"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_request_start_transaction_missing_evse_id_defaults_to_evse_1() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTART_DEFAULT")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: remote start with NO `evseId`. The handler defaults a missing
    // target to EVSE 1 (mirroring the 1.6J `connector_id.unwrap_or(1)`), so it is
    // Accepted and starts on EVSE 1.
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART_DEFAULT",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 7,
                evse_id: None,
                group_id_token: None,
                charging_profile: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Accepted,
        "a remote start with no evseId defaults to EVSE 1 and is Accepted"
    );

    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let started = log
        .lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .find(|e| e.event_type == TransactionEventEnumType::Started)
        .cloned()
        .expect("a Started event was recorded");
    assert_eq!(
        started.evse.as_ref().map(|e| e.id),
        Some(1),
        "a missing evseId defaults the started transaction to EVSE 1"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_request_start_transaction_for_a_busy_evse_is_rejected() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTART_BUSY")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Occupy EVSE 1 with a live local transaction, so it is no longer free to
    // charge. Waiting on the Started event guarantees the connector has flipped to
    // Charging before the remote start below reads its chargeability.
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "RFID-LOCAL", 0)
        .await
        .expect("v201 start_transaction");
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    assert_eq!(started_count(&log), 1, "exactly one transaction is running");

    // CSMS -> CP: remote start targeting the busy EVSE 1. A busy EVSE is not free
    // to charge, so the decision is Rejected.
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART_BUSY",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 1,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Rejected,
        "a remote start on a busy EVSE is Rejected"
    );

    // A Rejected request queues no StartTransaction, so no second session begins.
    // Give any erroneously-queued start time to land before asserting the negative.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        started_count(&log),
        1,
        "a rejected remote start begins no new transaction"
    );

    cp.stop_transaction(txn_id, 0, Reason::EVDisconnected)
        .await
        .expect("v201 stop_transaction");
    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_request_start_transaction_for_an_unknown_evse_is_rejected() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_REQSTART_UNKNOWN"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: target an EVSE that does not exist on this CP (the default config
    // has 2 connectors, so EVSE 99 is out of range). The station cannot start a
    // session there, so it answers Rejected and begins nothing.
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART_UNKNOWN",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 2,
                evse_id: Some(99),
                group_id_token: None,
                charging_profile: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Rejected,
        "an unknown / out-of-range EVSE target is Rejected"
    );

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        started_count(&log),
        0,
        "an unknown EVSE begins no transaction"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// Count the `Ended` TransactionEvents recorded so far — used to assert a refused
/// unlock leaves a live transaction running (no stop was queued).
fn ended_count(log: &TxnLog) -> usize {
    log.lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .filter(|e| e.event_type == TransactionEventEnumType::Ended)
        .count()
}

/// A 2.0.1 config whose connector lock reports `outcome`, so the wired
/// `UnlockConnector` handler can be exercised against each mechanical result.
fn v201_unlock_cp_config(
    addr: SocketAddr,
    id: &str,
    outcome: UnlockConnectorOutcome,
) -> ChargePointConfig {
    ChargePointConfig {
        unlock_connector_outcome: outcome,
        ..v201_cp_config(addr, id)
    }
}

#[tokio::test]
async fn v201_unlock_connector_idle_connector_is_unlocked() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_UNLOCK_OK")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: unlock the idle EVSE 1 / connector 1. With the default `Unlock`
    // lock capability and no live transaction, the pure decision releases the
    // cable → `Unlocked`.
    let resp = server
        .call::<V201UnlockConnectorRequest>(
            "CP201_UNLOCK_OK",
            V201UnlockConnectorRequest {
                evse_id: 1,
                connector_id: 1,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS UnlockConnector call round-trips");
    assert_eq!(
        resp.status,
        UnlockStatusEnumType::Unlocked,
        "an idle connector with a controllable lock is Unlocked"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_unlock_connector_reports_the_mechanical_unlock_failure() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    // A CP whose lock will not release mechanically. 2.0.1 has no `NotSupported`
    // status, so both `UnlockFailed` and `NotSupported` lock capabilities fold to
    // `UnlockFailed`; this exercises the `UnlockFailed` capability.
    let cp = ChargePoint::new(v201_unlock_cp_config(
        addr,
        "CP201_UNLOCK_FAIL",
        UnlockConnectorOutcome::UnlockFailed,
    ))
    .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    let resp = server
        .call::<V201UnlockConnectorRequest>(
            "CP201_UNLOCK_FAIL",
            V201UnlockConnectorRequest {
                evse_id: 1,
                connector_id: 1,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS UnlockConnector call round-trips");
    assert_eq!(
        resp.status,
        UnlockStatusEnumType::UnlockFailed,
        "a connector whose lock will not release reports UnlockFailed"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_unlock_connector_with_a_live_transaction_is_refused_and_does_not_stop_it() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    // A long meter interval so the only events on the log are the ones the test
    // drives — no periodic `Updated` noise, and crucially no spurious `Ended`.
    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_UNLOCK_BUSY")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Occupy EVSE 1 with a live local transaction. Waiting on the Started event
    // guarantees the transaction is registered before the unlock below reads it.
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "RFID-LOCAL", 0)
        .await
        .expect("v201 start_transaction");
    wait_for_event(&log, TransactionEventEnumType::Started).await;

    // CSMS -> CP: unlock the connector that still has an authorized session. 2.0.1
    // refuses to release the cable → `OngoingAuthorizedTransaction`, and (unlike
    // 1.6J) does not stop the transaction first.
    let resp = server
        .call::<V201UnlockConnectorRequest>(
            "CP201_UNLOCK_BUSY",
            V201UnlockConnectorRequest {
                evse_id: 1,
                connector_id: 1,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS UnlockConnector call round-trips");
    assert_eq!(
        resp.status,
        UnlockStatusEnumType::OngoingAuthorizedTransaction,
        "unlocking a connector with a live authorized transaction is refused"
    );

    // The refusal must not have stopped the transaction. Give any erroneously
    // queued stop time to land before asserting the negative.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        ended_count(&log),
        0,
        "a refused unlock leaves the transaction running (no Ended event)"
    );

    // The transaction is still stoppable through the normal path — proof it was
    // genuinely left alive.
    cp.stop_transaction(txn_id, 0, Reason::EVDisconnected)
        .await
        .expect("the still-live transaction stops normally");
    wait_for_event(&log, TransactionEventEnumType::Ended).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_unlock_connector_for_an_unknown_evse_is_unknown_connector() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_UNLOCK_UNKNOWN_EVSE"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: target an EVSE this CP does not have (the default config has 2
    // connectors, so EVSE 99 is out of range) → `UnknownConnector`.
    let resp = server
        .call::<V201UnlockConnectorRequest>(
            "CP201_UNLOCK_UNKNOWN_EVSE",
            V201UnlockConnectorRequest {
                evse_id: 99,
                connector_id: 1,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS UnlockConnector call round-trips");
    assert_eq!(
        resp.status,
        UnlockStatusEnumType::UnknownConnector,
        "an out-of-range EVSE target is UnknownConnector"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_unlock_connector_for_a_nonexistent_connector_within_an_evse_is_unknown_connector() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_UNLOCK_UNKNOWN_CONN"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: EVSE 1 exists, but on the flat single-connector-EVSE topology it
    // has exactly one connector (connectorId 1). Connector 2 within EVSE 1 does
    // not exist → `UnknownConnector`.
    let resp = server
        .call::<V201UnlockConnectorRequest>(
            "CP201_UNLOCK_UNKNOWN_CONN",
            V201UnlockConnectorRequest {
                evse_id: 1,
                connector_id: 2,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS UnlockConnector call round-trips");
    assert_eq!(
        resp.status,
        UnlockStatusEnumType::UnknownConnector,
        "a connectorId with no matching connector within the EVSE is UnknownConnector"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// RequestStartTransaction — acting on the request's `remoteStartId` and
// `chargingProfile` (Issue #445, slice 7c). Slice 7b accepted these fields on
// the wire but did not yet act on them; this slice (a) echoes `remoteStartId`
// onto the started transaction's TransactionEvent(Started) for CSMS
// correlation, and (b) guards `chargingProfile.chargingProfilePurpose` — only a
// `TxProfile` is permitted on a RequestStartTransaction.
// ---------------------------------------------------------------------------

/// A minimal schema-valid `ChargingProfileType` of the given purpose, bounding
/// the session to a single flat power limit. Enough to exercise the handler's
/// purpose guard on the wire.
fn charging_profile(purpose: ChargingProfilePurposeEnumType) -> ChargingProfileType {
    ChargingProfileType {
        id: 1,
        stack_level: 0,
        charging_profile_purpose: purpose,
        charging_profile_kind: ChargingProfileKindEnumType::Relative,
        charging_schedule: vec![ChargingScheduleType {
            id: 1,
            charging_rate_unit: ChargingRateUnitEnumType::W,
            charging_schedule_period: vec![ChargingSchedulePeriodType {
                start_period: 0,
                limit: 11_000.0,
                number_phases: None,
                phase_to_use: None,
                custom_data: None,
            }],
            start_schedule: None,
            duration: None,
            min_charging_rate: None,
            sales_tariff: None,
            custom_data: None,
        }],
        recurrency_kind: None,
        valid_from: None,
        valid_to: None,
        transaction_id: None,
        custom_data: None,
    }
}

#[tokio::test]
async fn v201_request_start_transaction_carries_remote_start_id_to_the_started_event() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTART_CORRELATE")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: remote start on the free EVSE 1 carrying a distinctive
    // remoteStartId the back office will use to correlate the resulting session.
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART_CORRELATE",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 4242,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(resp.status, RequestStartStopStatusEnumType::Accepted);

    // The started transaction's TransactionEvent(Started) must echo the
    // remoteStartId in transactionInfo.remoteStartId (2.0.1's correlation
    // mechanism, replacing 1.6J's synchronous conf transactionId).
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let started = log
        .lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .find(|e| e.event_type == TransactionEventEnumType::Started)
        .cloned()
        .expect("a Started event was recorded");
    assert_eq!(
        started.transaction_info.remote_start_id,
        Some(4242),
        "the Started event correlates back to the RequestStartTransaction's remoteStartId"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_request_start_transaction_with_a_valid_txprofile_is_accepted() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTART_TXPROFILE")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: remote start bounding the session with a TxProfile — the one
    // profile purpose 2.0.1 permits on a RequestStartTransaction. It is accepted
    // and the transaction starts (installing/enforcing the schedule is the
    // slice-7d follow-up; the profile is validated here, not yet enforced).
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART_TXPROFILE",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 5,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: Some(charging_profile(ChargingProfilePurposeEnumType::TxProfile)),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Accepted,
        "a valid TxProfile is accepted"
    );

    wait_for_event(&log, TransactionEventEnumType::Started).await;
    assert_eq!(
        started_count(&log),
        1,
        "a TxProfile-bounded remote start begins the transaction"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_request_start_transaction_with_a_non_txprofile_charging_profile_is_rejected() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTART_BADPROFILE")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: remote start on the free EVSE 1, but the attached profile is a
    // TxDefaultProfile — not a TxProfile. 2.0.1 permits only a TxProfile on a
    // RequestStartTransaction, so the station rejects the request with an
    // explanatory statusInfo and starts nothing (even though the EVSE is free).
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART_BADPROFILE",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 6,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: Some(charging_profile(
                    ChargingProfilePurposeEnumType::TxDefaultProfile,
                )),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Rejected,
        "a non-TxProfile chargingProfile is Rejected"
    );
    let info = resp
        .status_info
        .as_ref()
        .expect("the rejection carries an explanatory statusInfo");
    assert_eq!(info.reason_code, "InvalidProfile");

    // A rejected request queues no StartTransaction — nothing starts on the free
    // EVSE. Give any erroneously-queued start time to land before the negative.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        started_count(&log),
        0,
        "a rejected malformed-profile start begins no transaction"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
