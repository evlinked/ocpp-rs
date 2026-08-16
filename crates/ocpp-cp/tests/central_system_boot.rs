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

use ocpp_cp::v201_station_ceiling::CeilingKind;
use ocpp_cp::{ChargePoint, ChargePointConfig, UnlockConnectorOutcome};
use ocpp_messages::v16j::RegistrationStatus;
use ocpp_messages::v201::{
    BootNotificationRequest as V201BootNotificationRequest,
    BootNotificationResponse as V201BootNotificationResponse,
    ChangeAvailabilityRequest as V201ChangeAvailabilityRequest,
    ClearChargingProfileRequest as V201ClearChargingProfileRequest,
    DataTransferRequest as V201DataTransferRequest,
    GetBaseReportRequest as V201GetBaseReportRequest,
    GetBaseReportResponse as V201GetBaseReportResponse,
    GetChargingProfilesRequest as V201GetChargingProfilesRequest,
    GetChargingProfilesResponse as V201GetChargingProfilesResponse,
    GetVariablesRequest as V201GetVariablesRequest,
    GetVariablesResponse as V201GetVariablesResponse, MeterValuesRequest as V201MeterValuesRequest,
    MeterValuesResponse as V201MeterValuesResponse, NotifyReportRequest as V201NotifyReportRequest,
    NotifyReportResponse as V201NotifyReportResponse,
    ReportChargingProfilesRequest as V201ReportChargingProfilesRequest,
    ReportChargingProfilesResponse as V201ReportChargingProfilesResponse,
    RequestStartTransactionRequest as V201RequestStartTransactionRequest,
    RequestStopTransactionRequest as V201RequestStopTransactionRequest,
    ResetRequest as V201ResetRequest, SetChargingProfileRequest as V201SetChargingProfileRequest,
    SetVariablesRequest as V201SetVariablesRequest,
    SetVariablesResponse as V201SetVariablesResponse,
    StatusNotificationRequest as V201StatusNotificationRequest,
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
    AttributeEnumType, BootReasonEnumType, ChangeAvailabilityStatusEnumType,
    ChargingProfileCriterionType, ChargingProfileKindEnumType, ChargingProfilePurposeEnumType,
    ChargingProfileStatusEnumType, ChargingProfileType, ChargingRateUnitEnumType,
    ChargingSchedulePeriodType, ChargingScheduleType, ClearChargingProfileStatusEnumType,
    ClearChargingProfileType, ComponentType, ConnectorStatusEnumType, DataTransferStatusEnumType,
    EvseType, GenericDeviceModelStatusEnumType, GetChargingProfileStatusEnumType,
    GetVariableDataType, GetVariableStatusEnumType, IdTokenEnumType, IdTokenType,
    MeasurandEnumType, MessageTriggerEnumType, MutabilityEnumType, OperationalStatusEnumType,
    ReadingContextEnumType, ReasonEnumType, RegistrationStatusEnumType, ReportBaseEnumType,
    RequestStartStopStatusEnumType, ResetEnumType, ResetStatusEnumType, SetVariableDataType,
    SetVariableStatusEnumType, TransactionEventEnumType, TriggerMessageStatusEnumType,
    TriggerReasonEnumType, UnlockStatusEnumType, VariableType,
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
async fn v201_unlock_connector_stops_a_deauthorized_transaction_then_unlocks() {
    // The other half of the 2.0.1 policy: a live transaction that has been
    // *deauthorized* (the driver re-presented their card / the app revoked
    // authorization) is no longer refused. An inbound `UnlockConnector` stops it
    // first (reason `UnlockCommand`) and releases the cable — the 2.0.1 analogue
    // of the 1.6J stop-then-unlock, reachable only once the session is
    // deauthorized (a still-authorized session stays refused, above).
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    // A long meter interval so the only `Ended` on the log is the unlock-triggered
    // stop — no periodic `Updated`/`Ended` noise to race the assertion.
    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_UNLOCK_DEAUTH")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Occupy EVSE 1, wait for the Started so the session is registered, then
    // deauthorize it: the cable stays latched but the driver is no longer
    // authorized.
    let connector = ConnectorId::new(1).unwrap();
    cp.start_transaction(connector, "RFID-DEAUTH", 0)
        .await
        .expect("v201 start_transaction");
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    assert_eq!(
        cp.deauthorize("RFID-DEAUTH").await,
        1,
        "the one live session started by this idTag is deauthorized"
    );

    // CSMS -> CP: unlock the now-deauthorized connector → the station stops the
    // transaction (reason `UnlockCommand`) and releases the cable (`Unlocked`).
    let resp = server
        .call::<V201UnlockConnectorRequest>(
            "CP201_UNLOCK_DEAUTH",
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
        "a deauthorized transaction is stopped and the cable released"
    );

    // The stop lands off the CALL path (CALLRESULT flushed first); wait for the
    // `Ended` it emits, then assert it names the transaction and the trigger.
    wait_for_event(&log, TransactionEventEnumType::Ended).await;
    {
        let events = log.lock().expect("txn log mutex not poisoned");
        let ended = events
            .iter()
            .rfind(|e| e.event_type == TransactionEventEnumType::Ended)
            .expect("an Ended event was recorded");
        assert_eq!(
            ended.trigger_reason,
            TriggerReasonEnumType::UnlockCommand,
            "the unlock-triggered stop reports triggerReason = UnlockCommand"
        );
        assert_eq!(
            ended.transaction_info.transaction_id, "1",
            "the Ended names the transaction that was unlocked"
        );
    }

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
    // and the transaction starts. (This case asserts acceptance; the install of
    // the profile is asserted by
    // `v201_request_start_installs_the_txprofile_and_threads_group_id_token`;
    // enforcing the schedule is the follow-up.)
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

// RequestStartTransaction — slice 7d (Issue #450): a valid TxProfile is now
// *installed* against the started transaction's EVSE (observable via
// `installed_tx_profile`) and the request's `groupIdToken` is threaded onto the
// session's auth context (observable via `transaction_group_id_token`), both for
// the lifetime of the transaction. Stopping the transaction clears the installed
// profile (a TxProfile is transaction-scoped). Enforcing the schedule is a
// follow-up; this asserts install + threading + teardown.
#[tokio::test]
async fn v201_request_start_installs_the_txprofile_and_threads_group_id_token() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTART_INSTALL")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // No profile installed and no session before the remote start.
    assert_eq!(
        cp.installed_tx_profile(1).await,
        None,
        "no TxProfile is installed on an idle EVSE"
    );

    // CSMS -> CP: remote start on the free EVSE 1, bounding it with a TxProfile
    // and naming a parent/group token (a fleet card the driver token belongs to).
    let profile = charging_profile(ChargingProfilePurposeEnumType::TxProfile);
    let group_token = central_id_token("GROUP-FLEET-01");
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_REQSTART_INSTALL",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 77,
                evse_id: Some(1),
                group_id_token: Some(group_token.clone()),
                charging_profile: Some(profile.clone()),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(resp.status, RequestStartStopStatusEnumType::Accepted);

    // Wait for the Started event: it guarantees the transaction actually opened
    // (the install + group-token threading happen in `open_transaction`, on the
    // same path, before this event is sent) and hands us the station-minted id.
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let txn_id_str = recorded_transaction_id(&log, TransactionEventEnumType::Started);
    let txn_id: i32 = txn_id_str.parse().expect("station-minted id is decimal");

    // The TxProfile the request carried is installed against EVSE 1, byte-for-byte
    // (round-tripped over the wire), rather than parsed and dropped.
    assert_eq!(
        cp.installed_tx_profile(1).await,
        Some(profile),
        "the accepted RequestStartTransaction installs its TxProfile against the targeted EVSE"
    );
    // The groupIdToken rides on the started transaction's auth context.
    assert_eq!(
        cp.transaction_group_id_token(txn_id).await,
        Some(group_token),
        "the request's groupIdToken is threaded onto the started transaction"
    );

    // CSMS -> CP: stop the live transaction. A TxProfile is transaction-scoped, so
    // ending the transaction clears the installed profile in lockstep.
    let stop = server
        .call::<V201RequestStopTransactionRequest>(
            "CP201_REQSTART_INSTALL",
            V201RequestStopTransactionRequest {
                transaction_id: txn_id_str,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStopTransaction call round-trips");
    assert_eq!(stop.status, RequestStartStopStatusEnumType::Accepted);

    wait_for_event(&log, TransactionEventEnumType::Ended).await;
    assert_eq!(
        cp.installed_tx_profile(1).await,
        None,
        "ending the transaction clears its transaction-scoped TxProfile"
    );
    assert_eq!(
        cp.transaction_group_id_token(txn_id).await,
        None,
        "the session (and its group token) is gone once the transaction ends"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// SetChargingProfile — Issue #469: the direct CSMS→CS command installs a
// transaction-scoped TxProfile straight into the v201 store the metering
// resolver reads, out-of-band from a remote start. This exercises the full
// handler contract end to end: rejected when there is no transaction to bind
// to, accepted once a session is live (observable via `installed_tx_profile`),
// a station-ceiling purpose Accepted into a *separate* ceiling store without
// disturbing the TxProfile store (Issue #511), a second TxProfile *replacing*
// the first, and teardown clearing it when the transaction ends. (A
// `TxDefaultProfile` is likewise Accepted into its own store — Issue #471 —
// covered by the `ocpp-cp` unit/wire tests; this test keeps its focus on the
// transaction-scoped `TxProfile` store's isolation from the other purposes.)
#[tokio::test]
async fn v201_set_charging_profile_installs_replaces_and_rejects_faithfully() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        // Long interval: this test asserts store state, not metering ticks.
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_SETPROFILE")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Two distinct TxProfiles (different limits) so a replace is observable, and a
    // station-ceiling profile the simulator now honors as a cap (Issue #511:
    // ChargingStationMaxProfile / ChargingStationExternalConstraints).
    let profile_a = tx_profile_limited_w(6_000.0);
    let profile_b = tx_profile_limited_w(3_000.0);
    let ceiling_profile =
        charging_profile(ChargingProfilePurposeEnumType::ChargingStationMaxProfile);

    // (1) Before any transaction: a TxProfile has nothing to bind to → Rejected
    // with `NoTransaction`, and the store stays empty.
    let resp = server
        .call::<V201SetChargingProfileRequest>(
            "CP201_SETPROFILE",
            V201SetChargingProfileRequest {
                evse_id: 1,
                charging_profile: profile_a.clone(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS SetChargingProfile call round-trips");
    assert_eq!(
        resp.status,
        ChargingProfileStatusEnumType::Rejected,
        "a TxProfile with no ongoing transaction is Rejected"
    );
    assert_eq!(
        resp.status_info.as_ref().map(|i| i.reason_code.as_str()),
        Some("NoTransaction"),
        "the rejection explains there is no transaction to bind to"
    );
    assert_eq!(
        cp.installed_tx_profile(1).await,
        None,
        "a rejected SetChargingProfile installs nothing"
    );

    // Bring EVSE 1 up with a *profile-less* remote start, so any installed profile
    // afterwards can only have come from SetChargingProfile.
    let start = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_SETPROFILE",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 99,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(start.status, RequestStartStopStatusEnumType::Accepted);
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let txn_id_str = recorded_transaction_id(&log, TransactionEventEnumType::Started);
    assert_eq!(
        cp.installed_tx_profile(1).await,
        None,
        "the profile-less start installs no TxProfile"
    );

    // (2) A valid TxProfile for the live transaction → Accepted, installed against
    // EVSE 1 byte-for-byte (round-tripped over the wire).
    let resp = server
        .call::<V201SetChargingProfileRequest>(
            "CP201_SETPROFILE",
            V201SetChargingProfileRequest {
                evse_id: 1,
                charging_profile: profile_a.clone(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS SetChargingProfile call round-trips");
    assert_eq!(resp.status, ChargingProfileStatusEnumType::Accepted);
    assert!(
        resp.status_info.is_none(),
        "an accepted install carries no reason"
    );
    assert_eq!(
        cp.installed_tx_profile(1).await,
        Some(profile_a.clone()),
        "an accepted SetChargingProfile installs its TxProfile against the EVSE"
    );

    // (3) A station-ceiling purpose → Accepted, installed into the *ceiling* store
    // (Issue #511), leaving the transaction-scoped TxProfile store untouched
    // (profile_a still installed).
    let resp = server
        .call::<V201SetChargingProfileRequest>(
            "CP201_SETPROFILE",
            V201SetChargingProfileRequest {
                evse_id: 1,
                charging_profile: ceiling_profile.clone(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS SetChargingProfile call round-trips");
    assert_eq!(resp.status, ChargingProfileStatusEnumType::Accepted);
    assert!(
        resp.status_info.is_none(),
        "an accepted station-ceiling install carries no reason"
    );
    assert_eq!(
        cp.installed_station_ceiling(CeilingKind::Max, 1).await,
        Some(ceiling_profile),
        "the station ceiling lands in the ceiling store against its EVSE"
    );
    assert_eq!(
        cp.installed_tx_profile(1).await,
        Some(profile_a),
        "installing a station ceiling leaves the transaction-scoped TxProfile untouched"
    );

    // (4) A second TxProfile *replaces* the first (the store holds one per EVSE).
    let resp = server
        .call::<V201SetChargingProfileRequest>(
            "CP201_SETPROFILE",
            V201SetChargingProfileRequest {
                evse_id: 1,
                charging_profile: profile_b.clone(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS SetChargingProfile call round-trips");
    assert_eq!(resp.status, ChargingProfileStatusEnumType::Accepted);
    assert_eq!(
        cp.installed_tx_profile(1).await,
        Some(profile_b),
        "a second SetChargingProfile replaces, not stacks, the installed TxProfile"
    );

    // (5) Ending the transaction clears the transaction-scoped profile.
    let stop = server
        .call::<V201RequestStopTransactionRequest>(
            "CP201_SETPROFILE",
            V201RequestStopTransactionRequest {
                transaction_id: txn_id_str,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStopTransaction call round-trips");
    assert_eq!(stop.status, RequestStartStopStatusEnumType::Accepted);
    wait_for_event(&log, TransactionEventEnumType::Ended).await;
    assert_eq!(
        cp.installed_tx_profile(1).await,
        None,
        "ending the transaction clears its transaction-scoped TxProfile"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ClearChargingProfile — Issue #474: the teardown counterpart to
// SetChargingProfile removes an installed TxProfile mid-session, without ending
// the transaction. This drives the full V201 handler over the wire: install via
// SetChargingProfile, confirm a *non-matching* selector returns Unknown and
// leaves the store intact, then confirm a matching selector returns Accepted and
// lifts the bound (the profile is gone from `installed_tx_profile`) — all while
// the transaction stays live.
#[tokio::test]
async fn v201_clear_charging_profile_removes_the_installed_txprofile_over_the_wire() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        // Long interval: this test asserts store state, not metering ticks.
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_CLEARPROFILE")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Bring EVSE 1 up with a profile-less remote start, then install a TxProfile
    // via SetChargingProfile — so the only path a profile could be installed by is
    // the one this test then clears.
    let start = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_CLEARPROFILE",
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
    assert_eq!(start.status, RequestStartStopStatusEnumType::Accepted);
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let txn_id_str = recorded_transaction_id(&log, TransactionEventEnumType::Started);

    let profile = tx_profile_limited_w(6_000.0); // id 1, stack_level 0
    let set = server
        .call::<V201SetChargingProfileRequest>(
            "CP201_CLEARPROFILE",
            V201SetChargingProfileRequest {
                evse_id: 1,
                charging_profile: profile.clone(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS SetChargingProfile call round-trips");
    assert_eq!(set.status, ChargingProfileStatusEnumType::Accepted);
    assert_eq!(
        cp.installed_tx_profile(1).await,
        Some(profile),
        "the TxProfile is installed and ready to be cleared"
    );

    // (1) A selector that matches nothing installed → Unknown, store untouched.
    // EVSE 2 holds no profile, so an evseId=2 criterion clears nothing.
    let miss = server
        .call::<V201ClearChargingProfileRequest>(
            "CP201_CLEARPROFILE",
            V201ClearChargingProfileRequest {
                charging_profile_id: None,
                charging_profile_criteria: Some(ClearChargingProfileType {
                    evse_id: Some(2),
                    charging_profile_purpose: None,
                    stack_level: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS ClearChargingProfile call round-trips");
    assert_eq!(
        miss.status,
        ClearChargingProfileStatusEnumType::Unknown,
        "a selector matching no installed profile returns Unknown"
    );
    assert!(
        cp.installed_tx_profile(1).await.is_some(),
        "a non-matching ClearChargingProfile leaves the store untouched"
    );

    // (2) A matching selector (by chargingProfileId) → Accepted, profile removed,
    // the transaction still live (this did not stop it).
    let hit = server
        .call::<V201ClearChargingProfileRequest>(
            "CP201_CLEARPROFILE",
            V201ClearChargingProfileRequest {
                charging_profile_id: Some(1),
                charging_profile_criteria: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS ClearChargingProfile call round-trips");
    assert_eq!(
        hit.status,
        ClearChargingProfileStatusEnumType::Accepted,
        "a selector matching the installed profile returns Accepted"
    );
    assert_eq!(
        cp.installed_tx_profile(1).await,
        None,
        "the matched ClearChargingProfile lifts the bound: the TxProfile is gone"
    );

    // The transaction is still live — clearing a profile does not end it. A second
    // clear of the now-empty store is Unknown.
    let again = server
        .call::<V201ClearChargingProfileRequest>(
            "CP201_CLEARPROFILE",
            V201ClearChargingProfileRequest::default(),
        )
        .await
        .expect("CSMS ClearChargingProfile call round-trips");
    assert_eq!(
        again.status,
        ClearChargingProfileStatusEnumType::Unknown,
        "an empty-store clear matches nothing"
    );

    // Teardown: the transaction still exists to be stopped, proving the clear left
    // the session alone.
    let stop = server
        .call::<V201RequestStopTransactionRequest>(
            "CP201_CLEARPROFILE",
            V201RequestStopTransactionRequest {
                transaction_id: txn_id_str,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStopTransaction call round-trips");
    assert_eq!(
        stop.status,
        RequestStartStopStatusEnumType::Accepted,
        "the transaction outlived the ClearChargingProfile and is stoppable"
    );
    wait_for_event(&log, TransactionEventEnumType::Ended).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// RequestStartTransaction — slice 7e (Issue #455): the installed TxProfile is
// now *binding*. When its chargingSchedule limit is tighter than the connector's
// natural rate, the periodic TransactionEvent(Updated) surfaces the bounded
// power as a Power.Active.Import sample alongside the energy reading. A
// profile-less session (or one whose limit is above the natural rate) is
// unchanged — energy only.
// ---------------------------------------------------------------------------

/// A schema-valid `TxProfile` bounding the session to a single flat watt limit.
fn tx_profile_limited_w(limit_w: f64) -> ChargingProfileType {
    ChargingProfileType {
        id: 1,
        stack_level: 0,
        charging_profile_purpose: ChargingProfilePurposeEnumType::TxProfile,
        charging_profile_kind: ChargingProfileKindEnumType::Relative,
        charging_schedule: vec![ChargingScheduleType {
            id: 1,
            charging_rate_unit: ChargingRateUnitEnumType::W,
            charging_schedule_period: vec![ChargingSchedulePeriodType {
                start_period: 0,
                limit: limit_w,
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

/// The first `Power.Active.Import` value on the first recorded `Updated` event,
/// or `None` if the sampler has emitted no `Updated` carrying one yet.
fn recorded_bounded_power(log: &TxnLog) -> Option<f64> {
    log.lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .find(|e| e.event_type == TransactionEventEnumType::Updated)
        .and_then(|e| e.meter_value.as_ref())
        .and_then(|mv| mv.first())
        .and_then(|m| {
            m.sampled_value
                .iter()
                .find(|s| s.measurand == Some(MeasurandEnumType::PowerActiveImport))
        })
        .map(|s| s.value)
}

#[tokio::test]
async fn v201_periodic_update_reflects_the_installed_txprofile_limit() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    // A 1s meter interval so the periodic Updated sampler ticks promptly; the
    // test waits on the recorded log rather than sleeping a fixed duration.
    let config = ChargePointConfig {
        meter_values_interval: 1,
        ..v201_cp_config(addr, "CP201_ENFORCE")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: remote start bounding the session to 3 680 W — tighter than the
    // connector's 7 360 W natural rate, so the limit is binding.
    let profile = tx_profile_limited_w(3_680.0);
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_ENFORCE",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 55,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: Some(profile),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Accepted,
        "a valid TxProfile-bounded remote start is Accepted"
    );

    // Wait for the background sampler to emit at least one periodic Updated — the
    // positive is waited, never slept.
    wait_for_event(&log, TransactionEventEnumType::Updated).await;

    // The periodic reading surfaces the profile-bounded power (3 680 W) as a
    // Power.Active.Import sample beside the energy one.
    assert_eq!(
        recorded_bounded_power(&log),
        Some(3_680.0),
        "the Updated reflects the installed TxProfile's binding limit"
    );

    // Every Updated carries two samples: the energy reading plus the bound. The
    // energy sample is preserved, first, and unchanged in kind.
    {
        let events = log.lock().expect("txn log mutex not poisoned");
        let updated = events
            .iter()
            .find(|e| e.event_type == TransactionEventEnumType::Updated)
            .expect("an Updated was recorded");
        let samples = &updated.meter_value.as_ref().expect("meterValue")[0].sampled_value;
        assert_eq!(samples.len(), 2, "energy + bounded-power");
        assert_eq!(
            samples[0].measurand,
            Some(MeasurandEnumType::EnergyActiveImportRegister),
            "the energy sample is still first"
        );
    }

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_periodic_update_is_unchanged_without_a_binding_profile() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 1,
        ..v201_cp_config(addr, "CP201_NOENFORCE")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: remote start with a TxProfile whose 11 kW limit is *looser*
    // than the 7.36 kW connector — not binding, so the reading must be unchanged.
    let profile = tx_profile_limited_w(11_000.0);
    let resp = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_NOENFORCE",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 56,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: Some(profile),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(resp.status, RequestStartStopStatusEnumType::Accepted);

    wait_for_event(&log, TransactionEventEnumType::Updated).await;

    // No Power.Active.Import sample: an above-natural-rate profile does not bend
    // the reading, and the Updated stays the single energy sample.
    assert_eq!(
        recorded_bounded_power(&log),
        None,
        "a profile above the natural rate adds no bounded-power sample"
    );
    {
        let events = log.lock().expect("txn log mutex not poisoned");
        let updated = events
            .iter()
            .find(|e| e.event_type == TransactionEventEnumType::Updated)
            .expect("an Updated was recorded");
        let samples = &updated.meter_value.as_ref().expect("meterValue")[0].sampled_value;
        assert_eq!(samples.len(), 1, "energy only — unchanged");
        assert_eq!(
            samples[0].measurand,
            Some(MeasurandEnumType::EnergyActiveImportRegister)
        );
    }

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// Issue #471: a station-wide `TxDefaultProfile` (installed by SetChargingProfile
// with evseId=0) is *applied* in the metering path when no `TxProfile` is in
// force — the profile-less session below is bounded by the default. This proves
// acceptance criterion #1 ("a TxDefaultProfile install is Accepted and applied
// when no TxProfile is in force on the EVSE") over the real sampler, not just the
// composed-schedule readout.
#[tokio::test]
async fn v201_periodic_update_reflects_a_txdefaultprofile_when_no_txprofile_is_in_force() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 1,
        ..v201_cp_config(addr, "CP201_DEFAULT")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // CSMS -> CP: install a station-wide (evseId=0) TxDefaultProfile bounding to
    // 3 680 W — tighter than the connector's 7 360 W natural rate — with no
    // transaction live. A default is Accepted regardless of transaction state.
    let mut default_profile = tx_profile_limited_w(3_680.0);
    default_profile.charging_profile_purpose = ChargingProfilePurposeEnumType::TxDefaultProfile;
    let resp = server
        .call::<V201SetChargingProfileRequest>(
            "CP201_DEFAULT",
            V201SetChargingProfileRequest {
                evse_id: 0,
                charging_profile: default_profile,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS SetChargingProfile call round-trips");
    assert_eq!(
        resp.status,
        ChargingProfileStatusEnumType::Accepted,
        "a station-wide TxDefaultProfile is Accepted with no live transaction"
    );

    // CSMS -> CP: a *profile-less* remote start on EVSE 1 — nothing binds it but
    // the station-wide default.
    let start = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_DEFAULT",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 71,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(start.status, RequestStartStopStatusEnumType::Accepted);
    assert_eq!(
        cp.installed_tx_profile(1).await,
        None,
        "the profile-less start installs no TxProfile — only the default is in force"
    );

    wait_for_event(&log, TransactionEventEnumType::Updated).await;

    // The periodic reading is bounded by the station-wide default (3 680 W),
    // exactly as a TxProfile of the same limit would bind it.
    assert_eq!(
        recorded_bounded_power(&log),
        Some(3_680.0),
        "the Updated reflects the station-wide TxDefaultProfile's binding limit"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// RequestStopTransaction (Issue #452, slice 9): wiring the 2.0.1 successor to
// 1.6J `RemoteStopTransaction` through the live inbound dispatcher. The CSMS
// names a running transaction by its `transactionId`; the station answers
// Accepted iff that id matches a live transaction and then ends it off the
// inbound-CALL path, emitting a `TransactionEvent(Ended)` with `stoppedReason`
// = `Remote` / `triggerReason` = `RemoteStop`. An unknown id, or an idle
// station, is Rejected and ends nothing.
// ---------------------------------------------------------------------------

/// The `transactionId` string the station put on its first recorded event of
/// `event_type` (e.g. the `Started` event). Panics if no such event exists yet.
fn recorded_transaction_id(log: &TxnLog, event_type: TransactionEventEnumType) -> String {
    log.lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .find(|e| e.event_type == event_type)
        .map(|e| e.transaction_info.transaction_id.clone())
        .unwrap_or_else(|| panic!("no {event_type:?} event recorded yet"))
}

/// Count the recorded events of `event_type`.
fn event_count(log: &TxnLog, event_type: TransactionEventEnumType) -> usize {
    log.lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .filter(|e| e.event_type == event_type)
        .count()
}

#[tokio::test]
async fn v201_request_stop_transaction_accepted_stops_the_live_transaction() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    // Long meter interval so the only events are Started and Ended — keeps the
    // Ended assertion unambiguous (no periodic Updated noise).
    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTOP")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // Bring a transaction up on EVSE 1 and learn the station-minted id from the
    // Started event (waiting on it guarantees the transaction is live before the
    // remote stop reads the transaction table).
    let connector = ConnectorId::new(1).unwrap();
    cp.start_transaction(connector, "RFID-CAFE", 0)
        .await
        .expect("v201 start_transaction");
    wait_for_event(&log, TransactionEventEnumType::Started).await;
    let txn_id = recorded_transaction_id(&log, TransactionEventEnumType::Started);

    // CSMS -> CP: stop the live transaction by its id. The decision is Accepted
    // and the wiring queues the stop off the CALL path.
    let resp = server
        .call::<V201RequestStopTransactionRequest>(
            "CP201_REQSTOP",
            V201RequestStopTransactionRequest {
                transaction_id: txn_id.clone(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStopTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Accepted,
        "stopping a live transaction by its id is Accepted"
    );

    // The accepted stop actually ends the transaction after the CALLRESULT is
    // flushed — proof the side effect fired off the CALL path.
    wait_for_event(&log, TransactionEventEnumType::Ended).await;
    let ended = log
        .lock()
        .expect("txn log mutex not poisoned")
        .iter()
        .find(|e| e.event_type == TransactionEventEnumType::Ended)
        .cloned()
        .expect("an Ended event was recorded");
    assert_eq!(
        ended.transaction_info.transaction_id, txn_id,
        "the Ended event names the stopped transaction"
    );
    assert_eq!(
        ended.transaction_info.stopped_reason,
        Some(ReasonEnumType::Remote),
        "a remote stop ends the transaction with stoppedReason = Remote"
    );
    assert_eq!(
        ended.trigger_reason,
        TriggerReasonEnumType::RemoteStop,
        "a remote stop carries triggerReason = RemoteStop"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_request_stop_transaction_for_an_unknown_id_is_rejected() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_REQSTOP_UNKNOWN")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // A transaction is live, but the CSMS names a *different*, non-existent id.
    let connector = ConnectorId::new(1).unwrap();
    let txn_id = cp
        .start_transaction(connector, "RFID-CAFE", 0)
        .await
        .expect("v201 start_transaction");
    wait_for_event(&log, TransactionEventEnumType::Started).await;

    let resp = server
        .call::<V201RequestStopTransactionRequest>(
            "CP201_REQSTOP_UNKNOWN",
            V201RequestStopTransactionRequest {
                transaction_id: "999999".to_string(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStopTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Rejected,
        "an unknown transactionId is Rejected"
    );

    // A rejected stop ends nothing: give any erroneously-queued stop time to land
    // before asserting the negative, then confirm the transaction is untouched.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        event_count(&log, TransactionEventEnumType::Ended),
        0,
        "a rejected remote stop emits no Ended event"
    );
    // The live transaction survived and can still be stopped normally.
    cp.stop_transaction(txn_id, 0, Reason::EVDisconnected)
        .await
        .expect("the untouched transaction still stops normally");
    wait_for_event(&log, TransactionEventEnumType::Ended).await;

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_request_stop_transaction_when_idle_is_rejected() {
    let (mut server, addr, log) = start_v201_csms_recording_txns().await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_REQSTOP_IDLE"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // No transaction is running, so there is nothing to stop.
    let resp = server
        .call::<V201RequestStopTransactionRequest>(
            "CP201_REQSTOP_IDLE",
            V201RequestStopTransactionRequest {
                transaction_id: "1".to_string(),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStopTransaction call round-trips");
    assert_eq!(
        resp.status,
        RequestStartStopStatusEnumType::Rejected,
        "a stop on an idle station is Rejected"
    );

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        event_count(&log, TransactionEventEnumType::Ended),
        0,
        "an idle-station stop emits no Ended event"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 `GetVariables` (Issue #457): the device-model read seam. The CSMS
// reads component-variable attributes from the CP's seeded standard profile
// over a real socket, and the CP answers one `GetVariableResultType` per
// requested entry — in request order, echoing the requested component/variable
// — with the right per-entry status. This is the 2.0.1 replacement for the
// 1.6J `GetConfiguration` path, exercised end to end.
// ---------------------------------------------------------------------------

fn v201_component(name: &str) -> ComponentType {
    ComponentType {
        name: name.to_string(),
        instance: None,
        evse: None,
        custom_data: None,
    }
}

fn v201_variable(name: &str) -> VariableType {
    VariableType {
        name: name.to_string(),
        instance: None,
        custom_data: None,
    }
}

fn get_variable_data(component: &str, variable: &str) -> GetVariableDataType {
    GetVariableDataType {
        component: v201_component(component),
        variable: v201_variable(variable),
        attribute_type: None,
        custom_data: None,
    }
}

#[tokio::test]
async fn v201_get_variables_reads_the_device_model_with_per_entry_status() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_GETVARS")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    assert!(server.is_cp_connected("CP201_GETVARS"));

    // Four entries spanning every outcome:
    //   0: seeded, Actual (default attributeType)         -> Accepted "300"
    //   1: unknown component                              -> UnknownComponent
    //   2: known component, unknown variable              -> UnknownVariable
    //   3: seeded variable, unsupported attributeType     -> NotSupportedAttributeType
    let mut not_supported = get_variable_data("OCPPCommCtrlr", "HeartbeatInterval");
    not_supported.attribute_type = Some(AttributeEnumType::Target);
    let resp: V201GetVariablesResponse = server
        .call::<V201GetVariablesRequest>(
            "CP201_GETVARS",
            V201GetVariablesRequest {
                get_variable_data: vec![
                    get_variable_data("OCPPCommCtrlr", "HeartbeatInterval"),
                    get_variable_data("NoSuchCtrlr", "HeartbeatInterval"),
                    get_variable_data("OCPPCommCtrlr", "NoSuchVariable"),
                    not_supported,
                ],
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetVariables call round-trips");

    // One result per requested entry, in request order.
    assert_eq!(resp.get_variable_result.len(), 4);

    let accepted = &resp.get_variable_result[0];
    assert_eq!(
        accepted.attribute_status,
        GetVariableStatusEnumType::Accepted
    );
    assert_eq!(accepted.attribute_value.as_deref(), Some("300"));
    // The result echoes back the requested component/variable.
    assert_eq!(accepted.component.name, "OCPPCommCtrlr");
    assert_eq!(accepted.variable.name, "HeartbeatInterval");

    assert_eq!(
        resp.get_variable_result[1].attribute_status,
        GetVariableStatusEnumType::UnknownComponent
    );
    assert_eq!(resp.get_variable_result[1].attribute_value, None);

    assert_eq!(
        resp.get_variable_result[2].attribute_status,
        GetVariableStatusEnumType::UnknownVariable
    );
    assert_eq!(resp.get_variable_result[2].attribute_value, None);

    let ns = &resp.get_variable_result[3];
    assert_eq!(
        ns.attribute_status,
        GetVariableStatusEnumType::NotSupportedAttributeType
    );
    assert_eq!(ns.attribute_value, None);
    // The unsupported attributeType is echoed back verbatim.
    assert_eq!(ns.attribute_type, Some(AttributeEnumType::Target));

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 `SetVariables` (Issue #458): the device-model write seam. The CSMS
// writes component-variable attributes into the CP's device model over a real
// socket, and the CP answers one `SetVariableResultType` per requested entry —
// in request order, echoing the requested component/variable — with the right
// per-entry status. A subsequent `GetVariables` proves the round-trip: an
// accepted write is read back, while a rejected (read-only) write leaves the
// stored value untouched. The 2.0.1 replacement for the 1.6J
// `ChangeConfiguration` path, exercised end to end.
// ---------------------------------------------------------------------------

fn set_variable_data(component: &str, variable: &str, value: &str) -> SetVariableDataType {
    SetVariableDataType {
        attribute_value: value.to_string(),
        component: v201_component(component),
        variable: v201_variable(variable),
        attribute_type: None,
        custom_data: None,
    }
}

#[tokio::test]
async fn v201_set_variables_writes_the_device_model_and_round_trips_via_get() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_SETVARS")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    assert!(server.is_cp_connected("CP201_SETVARS"));

    // Three entries spanning distinct write outcomes:
    //   0: writable variable                     -> Accepted (new value stored)
    //   1: read-only capability constant         -> Rejected (value unchanged)
    //   2: unknown component                     -> UnknownComponent
    let set_resp: V201SetVariablesResponse = server
        .call::<V201SetVariablesRequest>(
            "CP201_SETVARS",
            V201SetVariablesRequest {
                set_variable_data: vec![
                    set_variable_data("OCPPCommCtrlr", "HeartbeatInterval", "600"),
                    set_variable_data("SecurityCtrlr", "MaxCertificateChainSize", "9"),
                    set_variable_data("NoSuchCtrlr", "HeartbeatInterval", "1"),
                ],
                custom_data: None,
            },
        )
        .await
        .expect("CSMS SetVariables call round-trips");

    // One result per requested entry, in request order.
    assert_eq!(set_resp.set_variable_result.len(), 3);

    let accepted = &set_resp.set_variable_result[0];
    assert_eq!(
        accepted.attribute_status,
        SetVariableStatusEnumType::Accepted
    );
    // The result echoes back the requested component/variable.
    assert_eq!(accepted.component.name, "OCPPCommCtrlr");
    assert_eq!(accepted.variable.name, "HeartbeatInterval");

    assert_eq!(
        set_resp.set_variable_result[1].attribute_status,
        SetVariableStatusEnumType::Rejected
    );
    assert_eq!(
        set_resp.set_variable_result[2].attribute_status,
        SetVariableStatusEnumType::UnknownComponent
    );

    // Round-trip: read the two written-to variables back over the same socket.
    //   - the accepted write is now visible ("600"),
    //   - the rejected (read-only) write left the seed value ("3") intact.
    let get_resp: V201GetVariablesResponse = server
        .call::<V201GetVariablesRequest>(
            "CP201_SETVARS",
            V201GetVariablesRequest {
                get_variable_data: vec![
                    get_variable_data("OCPPCommCtrlr", "HeartbeatInterval"),
                    get_variable_data("SecurityCtrlr", "MaxCertificateChainSize"),
                ],
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetVariables call round-trips");

    assert_eq!(
        get_resp.get_variable_result[0].attribute_status,
        GetVariableStatusEnumType::Accepted
    );
    assert_eq!(
        get_resp.get_variable_result[0].attribute_value.as_deref(),
        Some("600"),
        "the accepted SetVariables write must be read back"
    );
    assert_eq!(
        get_resp.get_variable_result[1].attribute_value.as_deref(),
        Some("3"),
        "a rejected write must leave the read-only value unchanged"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 `GetBaseReport` -> `NotifyReport` (Issue #461): the device-model
// **report** seam, completing the read (`GetVariables`) / write
// (`SetVariables`) / report triad over the same device model. Unlike the
// synchronous read/write, `GetBaseReport` is a two-part exchange: the station
// acknowledges with a `GenericDeviceModelStatusEnumType` and then streams the
// inventory back asynchronously as `NotifyReport` CALL(s), correlated by
// `requestId` — the same ack-then-side-effect discipline as `TriggerMessage`.
// Exercised end to end over a real socket: the CSMS asks, the CP acks, and the
// CP's follow-up `NotifyReport` is observed off the CALL path.
// ---------------------------------------------------------------------------

type ReportLog = Arc<Mutex<Vec<V201NotifyReportRequest>>>;

/// Start an in-process 2.0.1 CSMS that records every `NotifyReport` it receives
/// (on top of the default 2.0.1 lifecycle responders), so a `GetBaseReport`
/// side effect can be observed end to end.
async fn start_v201_csms_recording_reports() -> (OcppServer, SocketAddr, ReportLog) {
    let reports: ReportLog = Arc::new(Mutex::new(Vec::new()));
    let reports_for_handler = reports.clone();
    let (server, addr) = start_v201_csms(move |dispatcher| {
        let reports = reports_for_handler;
        dispatcher.on(move |req: V201NotifyReportRequest| {
            let reports = reports.clone();
            async move {
                reports
                    .lock()
                    .expect("report log mutex not poisoned")
                    .push(req);
                Ok(V201NotifyReportResponse::default())
            }
        });
    })
    .await;
    (server, addr, reports)
}

/// Poll `reports` until it holds at least `target` entries, or panic after ~5s.
/// Waits for a streamed `NotifyReport` without a fixed sleep.
async fn wait_for_reports(reports: &ReportLog, target: usize) {
    for _ in 0..250 {
        if reports.lock().expect("report log mutex not poisoned").len() >= target {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "timed out waiting for {target} NotifyReport(s) (last saw {})",
        reports.lock().expect("report log mutex not poisoned").len()
    );
}

#[tokio::test]
async fn v201_get_base_report_full_inventory_is_accepted_and_streams_notify_report() {
    let (mut server, addr, reports) = start_v201_csms_recording_reports().await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_BASEREPORT"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    assert!(server.is_cp_connected("CP201_BASEREPORT"));

    // CSMS -> CP: GetBaseReport(FullInventory). The station acks synchronously...
    let resp: V201GetBaseReportResponse = server
        .call::<V201GetBaseReportRequest>(
            "CP201_BASEREPORT",
            V201GetBaseReportRequest {
                request_id: 77,
                report_base: ReportBaseEnumType::FullInventory,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetBaseReport call round-trips");
    assert_eq!(
        resp.status,
        GenericDeviceModelStatusEnumType::Accepted,
        "a non-empty full inventory is Accepted"
    );

    // ...then streams the inventory as a NotifyReport off the CALL path.
    wait_for_reports(&reports, 1).await;
    let recorded = reports
        .lock()
        .expect("report log mutex not poisoned")
        .clone();
    assert_eq!(recorded.len(), 1, "FullInventory streams exactly one page");
    let report = &recorded[0];
    assert_eq!(
        report.request_id, 77,
        "NotifyReport echoes the GetBaseReport requestId"
    );
    assert_eq!(report.seq_no, 0, "the single page is seqNo 0");
    assert!(
        !report.tbc.unwrap_or(false),
        "a single page is not 'to be continued'"
    );

    let data = report
        .report_data
        .as_ref()
        .expect("an Accepted report carries reportData");

    // The report reproduces the CSMS-visible casing, not the normalized key.
    let heartbeat = data
        .iter()
        .find(|d| d.component.name == "OCPPCommCtrlr" && d.variable.name == "HeartbeatInterval")
        .expect("full inventory reports the heartbeat interval in display casing");
    let attr = &heartbeat.variable_attribute[0];
    assert_eq!(attr.value.as_deref(), Some("300"));
    assert_eq!(attr.mutability, Some(MutabilityEnumType::ReadWrite));

    // Full inventory includes the read-only capability constant, reported ReadOnly.
    let read_only = data
        .iter()
        .find(|d| d.variable.name == "MaxCertificateChainSize")
        .expect("full inventory includes read-only variables");
    assert_eq!(
        read_only.variable_attribute[0].mutability,
        Some(MutabilityEnumType::ReadOnly)
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_get_base_report_summary_inventory_is_empty_result_set_and_streams_nothing() {
    let (mut server, addr, reports) = start_v201_csms_recording_reports().await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_BASEREPORT_SUM"))
        .expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // SummaryInventory on a freshly-booted simulator has nothing noteworthy, so
    // the station acks EmptyResultSet and streams no NotifyReport.
    let summary: V201GetBaseReportResponse = server
        .call::<V201GetBaseReportRequest>(
            "CP201_BASEREPORT_SUM",
            V201GetBaseReportRequest {
                request_id: 78,
                report_base: ReportBaseEnumType::SummaryInventory,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetBaseReport(Summary) round-trips");
    assert_eq!(
        summary.status,
        GenericDeviceModelStatusEnumType::EmptyResultSet,
        "an empty summary is EmptyResultSet, not Accepted"
    );

    // Prove the summary emitted nothing *deterministically* (no fixed sleep): a
    // subsequent FullInventory does stream a report, and because the command
    // channel is FIFO, the first — and only — report we ever see must be that
    // FullInventory's (requestId 79), never the summary's (78).
    let full: V201GetBaseReportResponse = server
        .call::<V201GetBaseReportRequest>(
            "CP201_BASEREPORT_SUM",
            V201GetBaseReportRequest {
                request_id: 79,
                report_base: ReportBaseEnumType::FullInventory,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetBaseReport(Full) round-trips");
    assert_eq!(full.status, GenericDeviceModelStatusEnumType::Accepted);

    wait_for_reports(&reports, 1).await;
    let recorded = reports
        .lock()
        .expect("report log mutex not poisoned")
        .clone();
    assert_eq!(
        recorded.len(),
        1,
        "only the FullInventory streamed a report; the summary streamed none"
    );
    assert_eq!(
        recorded[0].request_id, 79,
        "the one report is the FullInventory's (79), never the empty summary's (78)"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 CSMS -> CP `DataTransfer` (Issue #470): the vendor-extension escape
// hatch on the V201 dispatcher arm. A `for_version(V201)` CP routes an inbound
// 2.0.1 `DataTransfer` through the *same* shared registry the 1.6J CP uses
// (`register_data_transfer_handler`), so a registered `(vendorId, messageId)`
// handler runs and its free-form JSON `data` round-trips over the wire, while an
// unregistered vendor resolves to the faithful `UnknownVendorId`. A green run
// proves the version-gated registration and the `v201_data_transfer` adapter
// carry the 2.0.1 frame end to end.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn v201_data_transfer_routes_through_the_shared_registry_over_the_wire() {
    use ocpp_messages::v16j::{
        DataTransferRequest as V16jDataTransferRequest,
        DataTransferResponse as V16jDataTransferResponse,
    };
    use ocpp_types::v16j::DataTransferStatus as V16jDataTransferStatus;

    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_DATATRANSFER"))
        .expect("build v201 charge point");

    // Register an echo handler for one (vendorId, messageId) on the shared
    // registry — exactly the 1.6J registration API, observed by the V201 arm.
    // The handler sees the request's `data` as the JSON *text* of the 2.0.1
    // `Value`, so echoing it verbatim round-trips the structured payload.
    cp.register_data_transfer_handler(
        "com.evlinked",
        Some("Echo".to_string()),
        |req: &V16jDataTransferRequest| V16jDataTransferResponse {
            status: V16jDataTransferStatus::Accepted,
            data: req.data.clone(),
        },
    );

    cp.connect().await.expect("v201 connect + boot sequence");

    // (1) A registered (vendorId, messageId) with a structured JSON payload →
    // Accepted, and the object round-trips over the wire unchanged.
    let payload = serde_json::json!({ "soc": 80, "phases": [1, 2, 3] });
    let resp = server
        .call::<V201DataTransferRequest>(
            "CP201_DATATRANSFER",
            V201DataTransferRequest {
                vendor_id: "com.evlinked".to_string(),
                message_id: Some("Echo".to_string()),
                data: Some(payload.clone()),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS DataTransfer call round-trips");
    assert_eq!(
        resp.status,
        DataTransferStatusEnumType::Accepted,
        "a registered (vendorId, messageId) is Accepted"
    );
    assert_eq!(
        resp.data,
        Some(payload),
        "the free-form JSON data round-trips over the wire without loss"
    );

    // (2) An unregistered vendor → the faithful UnknownVendorId, no handler run.
    let resp = server
        .call::<V201DataTransferRequest>(
            "CP201_DATATRANSFER",
            V201DataTransferRequest {
                vendor_id: "com.stranger".to_string(),
                message_id: Some("Echo".to_string()),
                data: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS DataTransfer call round-trips");
    assert_eq!(
        resp.status,
        DataTransferStatusEnumType::UnknownVendorId,
        "an unimplemented vendor resolves to UnknownVendorId"
    );

    // (3) A known vendor but an unregistered messageId → UnknownMessageId.
    let resp = server
        .call::<V201DataTransferRequest>(
            "CP201_DATATRANSFER",
            V201DataTransferRequest {
                vendor_id: "com.evlinked".to_string(),
                message_id: Some("Nope".to_string()),
                data: None,
                custom_data: None,
            },
        )
        .await
        .expect("CSMS DataTransfer call round-trips");
    assert_eq!(
        resp.status,
        DataTransferStatusEnumType::UnknownMessageId,
        "a known vendor with an unregistered messageId is UnknownMessageId"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

// ---------------------------------------------------------------------------
// OCPP 2.0.1 CSMS -> CP `GetChargingProfiles` → `ReportChargingProfiles`
// (Issue #476): the query half of the smart-charging report flow. A
// `for_version(V201)` CP answers a `GetChargingProfiles` synchronously with
// `Accepted` / `NoProfiles`, then — on `Accepted` — streams the matching
// installed profiles asynchronously as one or more `ReportChargingProfiles`
// CALLs, correlated by `requestId`, off the inbound-CALL path (reusing the
// `GetBaseReport → NotifyReport` seam from #462). A green run proves the
// version-gated handler, the pure selector + page builder in `v201_command`,
// and the async report stream carry the 2.0.1 flow end to end.
// ---------------------------------------------------------------------------

type ProfileReportLog = Arc<Mutex<Vec<V201ReportChargingProfilesRequest>>>;

/// Start an in-process 2.0.1 CSMS that records every `ReportChargingProfiles` it
/// receives (on top of the default 2.0.1 lifecycle responders), so a
/// `GetChargingProfiles` side effect can be observed end to end.
async fn start_v201_csms_recording_charging_profile_reports(
) -> (OcppServer, SocketAddr, ProfileReportLog) {
    let reports: ProfileReportLog = Arc::new(Mutex::new(Vec::new()));
    let reports_for_handler = reports.clone();
    let (server, addr) = start_v201_csms(move |dispatcher| {
        let reports = reports_for_handler;
        dispatcher.on(move |req: V201ReportChargingProfilesRequest| {
            let reports = reports.clone();
            async move {
                reports
                    .lock()
                    .expect("profile report log mutex not poisoned")
                    .push(req);
                Ok(V201ReportChargingProfilesResponse::default())
            }
        });
    })
    .await;
    (server, addr, reports)
}

/// Poll `reports` until it holds at least `target` entries, or panic after ~5s.
/// Waits for a streamed `ReportChargingProfiles` without a fixed sleep.
async fn wait_for_profile_reports(reports: &ProfileReportLog, target: usize) {
    for _ in 0..250 {
        if reports
            .lock()
            .expect("profile report log mutex not poisoned")
            .len()
            >= target
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "timed out waiting for {target} ReportChargingProfiles CALL(s) (last saw {})",
        reports
            .lock()
            .expect("profile report log mutex not poisoned")
            .len()
    );
}

/// Poll until the CP has a `TxProfile` installed on `evse_id`, or panic after
/// ~5s. The install happens in `open_transaction` on the command-consumer path
/// (queued off the synchronous `RequestStartTransaction.conf`), so tests wait for
/// it deterministically rather than with a fixed sleep.
async fn wait_for_installed_profile(cp: &ChargePoint, evse_id: i32) {
    for _ in 0..250 {
        if cp.installed_tx_profile(evse_id).await.is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for a TxProfile to be installed on EVSE {evse_id}");
}

#[tokio::test]
async fn v201_get_charging_profiles_accepts_and_streams_report_then_no_profiles_streams_nothing() {
    let (mut server, addr, reports) = start_v201_csms_recording_charging_profile_reports().await;

    // A long meter interval keeps periodic TransactionEvents from cluttering the
    // command stream while we observe the report side effect.
    let config = ChargePointConfig {
        meter_values_interval: 3600,
        ..v201_cp_config(addr, "CP201_GETPROFILES")
    };
    let cp = ChargePoint::new(config).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");
    assert!(server.is_cp_connected("CP201_GETPROFILES"));

    // Install a TxProfile on EVSE 1 by remotely starting a transaction that
    // carries it (slice 7d). This is the only profile the query should later find.
    let profile = charging_profile(ChargingProfilePurposeEnumType::TxProfile);
    let start = server
        .call::<V201RequestStartTransactionRequest>(
            "CP201_GETPROFILES",
            V201RequestStartTransactionRequest {
                id_token: central_id_token("RFID-CAFE"),
                remote_start_id: 1,
                evse_id: Some(1),
                group_id_token: None,
                charging_profile: Some(profile.clone()),
                custom_data: None,
            },
        )
        .await
        .expect("CSMS RequestStartTransaction call round-trips");
    assert_eq!(start.status, RequestStartStopStatusEnumType::Accepted);
    // The install is queued off the CALLRESULT; wait until it lands before querying.
    wait_for_installed_profile(&cp, 1).await;

    // CSMS -> CP: GetChargingProfiles with an empty criterion (report every
    // installed profile). The station acks Accepted synchronously...
    let resp: V201GetChargingProfilesResponse = server
        .call::<V201GetChargingProfilesRequest>(
            "CP201_GETPROFILES",
            V201GetChargingProfilesRequest {
                request_id: 55,
                evse_id: None,
                charging_profile: ChargingProfileCriterionType {
                    charging_profile_purpose: None,
                    stack_level: None,
                    charging_profile_id: None,
                    charging_limit_source: None,
                    custom_data: None,
                },
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetChargingProfiles call round-trips");
    assert_eq!(
        resp.status,
        GetChargingProfileStatusEnumType::Accepted,
        "a query matching the installed TxProfile is Accepted"
    );

    // ...then streams the matching profile as a ReportChargingProfiles CALL.
    wait_for_profile_reports(&reports, 1).await;
    let first = reports
        .lock()
        .expect("profile report log mutex not poisoned")
        .clone();
    assert_eq!(
        first.len(),
        1,
        "one installed profile streams exactly one page"
    );
    let report = &first[0];
    assert_eq!(
        report.request_id, 55,
        "the ReportChargingProfiles echoes the GetChargingProfiles requestId"
    );
    assert_eq!(
        report.evse_id, 1,
        "the report names the EVSE the profile is on"
    );
    assert_eq!(
        report.charging_profile.len(),
        1,
        "the page carries the installed profile"
    );
    assert_eq!(
        report.charging_profile[0].id, profile.id,
        "the reported profile is the one installed, round-tripped over the wire"
    );
    assert!(
        !report.tbc.unwrap_or(false),
        "a single page is not 'to be continued'"
    );

    // CSMS -> CP: a GetChargingProfiles whose criterion matches nothing installed
    // (the store holds a TxProfile, not a TxDefaultProfile) is NoProfiles...
    let none: V201GetChargingProfilesResponse = server
        .call::<V201GetChargingProfilesRequest>(
            "CP201_GETPROFILES",
            V201GetChargingProfilesRequest {
                request_id: 56,
                evse_id: None,
                charging_profile: ChargingProfileCriterionType {
                    charging_profile_purpose: Some(
                        ChargingProfilePurposeEnumType::TxDefaultProfile,
                    ),
                    stack_level: None,
                    charging_profile_id: None,
                    charging_limit_source: None,
                    custom_data: None,
                },
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetChargingProfiles(non-matching) call round-trips");
    assert_eq!(
        none.status,
        GetChargingProfileStatusEnumType::NoProfiles,
        "a query matching nothing installed is NoProfiles"
    );

    // Prove the NoProfiles query streamed nothing *deterministically* (no fixed
    // sleep): a subsequent matching query does stream a report, and because the
    // command channel is FIFO, the second — and last — report we ever see must be
    // that matching query's (requestId 57), never the NoProfiles one's (56).
    let again: V201GetChargingProfilesResponse = server
        .call::<V201GetChargingProfilesRequest>(
            "CP201_GETPROFILES",
            V201GetChargingProfilesRequest {
                request_id: 57,
                evse_id: Some(1),
                charging_profile: ChargingProfileCriterionType {
                    charging_profile_purpose: None,
                    stack_level: None,
                    charging_profile_id: None,
                    charging_limit_source: None,
                    custom_data: None,
                },
                custom_data: None,
            },
        )
        .await
        .expect("CSMS GetChargingProfiles(rematch) call round-trips");
    assert_eq!(again.status, GetChargingProfileStatusEnumType::Accepted);

    wait_for_profile_reports(&reports, 2).await;
    let all = reports
        .lock()
        .expect("profile report log mutex not poisoned")
        .clone();
    assert_eq!(
        all.len(),
        2,
        "only the two matching queries streamed reports; the NoProfiles one streamed none"
    );
    assert_eq!(
        all[1].request_id, 57,
        "the second report is the rematch's (57), never the NoProfiles query's (56)"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
