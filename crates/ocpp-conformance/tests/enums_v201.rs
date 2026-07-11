//! OCPP 2.0.1 enum wire-string conformance suite.
//!
//! A faithful port of the mobilityhouse/ocpp reference's
//! [`tests/v201/test_v201_enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v201/test_v201_enums.py),
//! which pins the 2.0.1 enum members that are most exposed to acronym / mixed-
//! case rename drift (`ConnectorEnumType`, `DataEnumType`,
//! `TxStartStopPointEnumType`). This is the 2.0.1 analog of the 1.6J
//! `enums_v16.rs` suite (#265) and completes the conformance coverage the 2.0.1
//! side was missing (#268): before this file, `grep -c 'assert_eq!'
//! crates/ocpp-types/src/v201/enums.rs` was `0` across 88 v201 enums.
//!
//! The v201 enums live in `ocpp-types::v201`. Their wire format comes entirely
//! from per-variant `#[serde(rename = "…")]` (there is no blanket
//! `rename_all`), so a single wrong rename silently breaks interop with a
//! spec-conformant 2.0.1 CSMS/CP. Each test pins the reference wire strings via
//! a **deserialize → re-serialize round-trip** keyed on the wire string, which
//! catches both a *missing* variant (deserialize fails) and *rename drift*
//! (re-serialize differs) without hand-naming Rust variants.
//!
//! ## Divergence from the reference dataclass — pinned, not dropped
//!
//! Two reference *dataclass* members are intentionally **absent** from the Rust
//! enums because they are absent from the OCPP 2.0.1 **FINAL JSON Schemas** that
//! `SchemaValidator::v201()` enforces — modelling them would let serde accept a
//! value the message layer then rejects:
//!
//! - `ConnectorEnumType::cChaoJi` / `cGBT` — not in `ReserveNowRequest.json`'s
//!   22-member `ConnectorEnumType` enum (verified against the bundled schema).
//! - `DataEnumType::passwordString` — not in the FINAL 8-member `DataEnumType`.
//!
//! Rather than silently drop the reference's assertions for these, the suite
//! **pins the divergence**: [`reject`] asserts each is refused by serde, so the
//! behavior is documented and regression-guarded. If a future OCPP errata folds
//! these into the FINAL schema, the `reject` calls fail loudly and flag the
//! model + schema for update together.
//!
//! ## Sweep of the remaining v201 enums (#274)
//!
//! Beyond the reference's hand-picked high-risk set, #274 sweeps every
//! `*EnumType` this crate models, split into domain slices. Slice 1 (#297)
//! covered the security / certificate / ISO-15118 / network-transport domain.
//! **Slice 2 (this change, #298)** covers the command/operation-status +
//! transaction/registration lifecycle domain — the reply-status vocabularies a
//! CSMS reads off every command CALLRESULT, plus the request-side lifecycle
//! discriminators. Each swept enum is cross-checked against the bundled FINAL
//! schema named in its test's doc comment; divergences are pinned with
//! [`reject`], never dropped.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, json, to_value};

use ocpp_types::v201::{
    BootReasonEnumType, CancelReservationStatusEnumType, ChangeAvailabilityStatusEnumType,
    ChargingStateEnumType, ClearCacheStatusEnumType, ClearChargingProfileStatusEnumType,
    ClearMessageStatusEnumType, ConnectorEnumType, ConnectorStatusEnumType,
    CustomerInformationStatusEnumType, DataEnumType, DataTransferStatusEnumType,
    DisplayMessageStatusEnumType, GenericStatusEnumType, GetChargingProfileStatusEnumType,
    GetDisplayMessagesStatusEnumType, LocationEnumType, MeasurandEnumType,
    NotifyEVChargingNeedsStatusEnumType, OperationalStatusEnumType, PhaseEnumType,
    ReadingContextEnumType, ReasonEnumType, RegistrationStatusEnumType,
    RequestStartStopStatusEnumType, ReservationUpdateStatusEnumType, ReserveNowStatusEnumType,
    ResetEnumType, ResetStatusEnumType, SendLocalListStatusEnumType, TransactionEventEnumType,
    TriggerMessageStatusEnumType, TriggerReasonEnumType, TxStartStopPointEnumType,
    UnlockStatusEnumType,
};

/// Assert that `wire` deserializes into `T` and re-serializes back to the
/// identical string — the reference's `Enum.member == "Wire"` assertion,
/// expressed as a byte-for-byte serde round-trip.
fn pin<T: DeserializeOwned + Serialize>(wire: &str) {
    let value: T = from_value(json!(wire)).unwrap_or_else(|e| {
        panic!(
            "{} does not model wire string {wire:?}: {e}",
            type_name::<T>()
        )
    });
    let back = to_value(&value).unwrap();
    assert_eq!(
        back,
        json!(wire),
        "{} round-trips {wire:?} to a different string",
        type_name::<T>()
    );
}

/// Pin every wire string in `wires` for enum `T`.
fn pin_all<T: DeserializeOwned + Serialize>(wires: &[&str]) {
    for w in wires {
        pin::<T>(w);
    }
}

/// Assert that `wire` is **not** a member of `T` — deserialization must fail.
///
/// Used to pin a deliberate divergence from the reference dataclass: a member
/// the Python side defines but the OCPP 2.0.1 FINAL JSON Schema (and therefore
/// this crate) does not.
fn reject<T: DeserializeOwned>(wire: &str) {
    let parsed: Result<T, _> = from_value(json!(wire));
    assert!(
        parsed.is_err(),
        "{} unexpectedly accepts {wire:?} — the FINAL 2.0.1 schema does not \
         define it; if an errata added it, add the variant AND the schema enum \
         together and update this test",
        type_name::<T>()
    );
}

fn type_name<T>() -> &'static str {
    std::any::type_name::<T>()
}

// ---------------------------------------------------------------------------
// Faithful port of tests/v201/test_v201_enums.py
// ---------------------------------------------------------------------------

/// Ports `test_connector_type`. Pins the 22 members of the FINAL-schema
/// `ConnectorEnumType` (`ReserveNowRequest.json`), including every prefixed /
/// hyphenated acronym case: `cCCS1`, `s309-1P-16A`, `sCEE-7-7`, `wInductive`,
/// `Other1PhMax16A`. The reference's `cChaoJi` / `cGBT` are pinned as
/// *rejected* — see [`reject`] and the module docs.
#[test]
fn test_connector_type() {
    pin_all::<ConnectorEnumType>(&[
        "cCCS1",
        "cCCS2",
        "cG105",
        "cTesla",
        "cType1",
        "cType2",
        "s309-1P-16A",
        "s309-1P-32A",
        "s309-3P-16A",
        "s309-3P-32A",
        "sBS1361",
        "sCEE-7-7",
        "sType2",
        "sType3",
        "Other1PhMax16A",
        "Other1PhOver16A",
        "Other3Ph",
        "Pan",
        "wInductive",
        "wResonant",
        "Undetermined",
        "Unknown",
    ]);

    // Reference dataclass members absent from the FINAL 2.0.1 schema.
    reject::<ConnectorEnumType>("cChaoJi");
    reject::<ConnectorEnumType>("cGBT");
}

/// Ports `test_data_type`. Pins the 8 members of the FINAL-schema
/// `DataEnumType`, including the lower-case scalars (`string`, `dateTime`) and
/// the PascalCase list kinds (`OptionList`). The reference's `passwordString`
/// is pinned as *rejected* — see [`reject`] and the module docs.
#[test]
fn test_data_type() {
    pin_all::<DataEnumType>(&[
        "string",
        "decimal",
        "integer",
        "dateTime",
        "boolean",
        "OptionList",
        "SequenceList",
        "MemberList",
    ]);

    // Reference dataclass member absent from the FINAL 2.0.1 schema.
    reject::<DataEnumType>("passwordString");
}

/// Ports `test_tx_start_stop_point`. Pins all 6 members of
/// `TxStartStopPointEnumType` — a device-model config vocabulary (`TxCtrlr`
/// `TxStartPoint`/`TxStopPoint`), not a payload-schema field — including the
/// embedded-acronym cases `EVConnected` and `PowerPathClosed`.
#[test]
fn test_tx_start_stop_point() {
    pin_all::<TxStartStopPointEnumType>(&[
        "Authorized",
        "DataSigned",
        "EnergyTransfer",
        "EVConnected",
        "ParkingBayOccupancy",
        "PowerPathClosed",
    ]);
}

// ---------------------------------------------------------------------------
// Beyond the reference: other high-acronym / dotted / hyphenated v201 enums
// this crate models. Mirrors how enums_v16.rs (#265) went past the 1.6J
// reference's strict set. Wire strings verified against ocpp/v201/enums.py.
// ---------------------------------------------------------------------------

/// `MeasurandEnumType` — 25 dotted members plus the bare `Frequency`, `SoC`,
/// `Voltage`. The dotted renames (`Energy.Active.Import.Register`,
/// `Power.Offered`) are exactly the drift risk this suite guards.
#[test]
fn test_measurand() {
    pin_all::<MeasurandEnumType>(&[
        "Current.Export",
        "Current.Import",
        "Current.Offered",
        "Energy.Active.Export.Register",
        "Energy.Active.Import.Register",
        "Energy.Reactive.Export.Register",
        "Energy.Reactive.Import.Register",
        "Energy.Active.Export.Interval",
        "Energy.Active.Import.Interval",
        "Energy.Active.Net",
        "Energy.Reactive.Export.Interval",
        "Energy.Reactive.Import.Interval",
        "Energy.Reactive.Net",
        "Energy.Apparent.Net",
        "Energy.Apparent.Import",
        "Energy.Apparent.Export",
        "Frequency",
        "Power.Active.Export",
        "Power.Active.Import",
        "Power.Factor",
        "Power.Offered",
        "Power.Reactive.Export",
        "Power.Reactive.Import",
        "SoC",
        "Voltage",
    ]);
}

/// `PhaseEnumType` — the hyphenated phase-to-neutral / phase-to-phase renames
/// (`L1-N`, `L3-L1`) are the drift risk.
#[test]
fn test_phase() {
    pin_all::<PhaseEnumType>(&[
        "L1", "L2", "L3", "N", "L1-N", "L2-N", "L3-N", "L1-L2", "L2-L3", "L3-L1",
    ]);
}

/// `ReadingContextEnumType` — dotted renames (`Interruption.Begin`,
/// `Sample.Periodic`, `Transaction.End`).
#[test]
fn test_reading_context() {
    pin_all::<ReadingContextEnumType>(&[
        "Interruption.Begin",
        "Interruption.End",
        "Other",
        "Sample.Clock",
        "Sample.Periodic",
        "Transaction.Begin",
        "Transaction.End",
        "Trigger",
    ]);
}

/// `LocationEnumType` — `EV` is renamed away from a PascalCase default.
#[test]
fn test_location() {
    pin_all::<LocationEnumType>(&["Body", "Cable", "EV", "Inlet", "Outlet"]);
}

/// `ReasonEnumType` — mixed-case acronyms (`EVDisconnected`, `SOCLimitReached`,
/// `StoppedByEV`) with no rename must serialize verbatim.
#[test]
fn test_reason() {
    pin_all::<ReasonEnumType>(&[
        "DeAuthorized",
        "EmergencyStop",
        "EnergyLimitReached",
        "EVDisconnected",
        "GroundFault",
        "ImmediateReset",
        "Local",
        "LocalOutOfCredit",
        "MasterPass",
        "Other",
        "OvercurrentFault",
        "PowerLoss",
        "PowerQuality",
        "Reboot",
        "Remote",
        "SOCLimitReached",
        "StoppedByEV",
        "TimeLimitReached",
        "Timeout",
    ]);
}

/// `ConnectorStatusEnumType` — the 2.0.1 connector-status vocabulary (distinct
/// from and smaller than the 1.6J `ChargePointStatus`).
#[test]
fn test_connector_status() {
    pin_all::<ConnectorStatusEnumType>(&[
        "Available",
        "Occupied",
        "Reserved",
        "Unavailable",
        "Faulted",
    ]);
}

/// `TriggerReasonEnumType` — the acronym cases (`EVCommunicationLost`,
/// `EVConnectTimeout`, `EVDeparted`, `EVDetected`) must serialize verbatim.
#[test]
fn test_trigger_reason() {
    pin_all::<TriggerReasonEnumType>(&[
        "Authorized",
        "CablePluggedIn",
        "ChargingRateChanged",
        "ChargingStateChanged",
        "Deauthorized",
        "EnergyLimitReached",
        "EVCommunicationLost",
        "EVConnectTimeout",
        "MeterValueClock",
        "MeterValuePeriodic",
        "TimeLimitReached",
        "Trigger",
        "UnlockCommand",
        "StopAuthorized",
        "EVDeparted",
        "EVDetected",
        "RemoteStop",
        "RemoteStart",
        "AbnormalCondition",
        "SignedDataReceived",
        "ResetCommand",
    ]);
}

/// `ChargingStateEnumType` — `EVConnected`, `SuspendedEV`, `SuspendedEVSE`
/// carry embedded acronyms and must serialize verbatim.
#[test]
fn test_charging_state() {
    pin_all::<ChargingStateEnumType>(&[
        "Charging",
        "EVConnected",
        "SuspendedEV",
        "SuspendedEVSE",
        "Idle",
    ]);
}

// ---------------------------------------------------------------------------
// Slice 2 of #274 — command/operation-status + transaction/registration
// lifecycle enums. These are the reply-status vocabularies a CSMS reads off
// every command CALLRESULT, plus the request-side lifecycle discriminators
// (`BootReason`, `TransactionEvent`, `Reset`, `OperationalStatus`).
//
// Every wire string below was verified against `ocpp/v201/enums.py` (source of
// truth) AND cross-checked against the named bundled FINAL schema in
// `crates/ocpp-messages/schemas/v201/*.json` (the vocabulary
// `SchemaValidator::v201()` actually enforces). For all 24, model == reference
// == FINAL schema — so, like slice 1, this slice pins no `reject` divergences.
// Each test's doc comment names the FINAL schema it was cross-checked against.
// ---------------------------------------------------------------------------

/// `BootReasonEnumType` — the `reason` discriminator a Charging Station sends in
/// every `BootNotification`. Cross-checked against `BootNotification.json`.
#[test]
fn test_boot_reason() {
    pin_all::<BootReasonEnumType>(&[
        "ApplicationReset",
        "FirmwareUpdate",
        "LocalReset",
        "PowerUp",
        "RemoteReset",
        "ScheduledReset",
        "Triggered",
        "Unknown",
        "Watchdog",
    ]);
}

/// `RegistrationStatusEnumType` — the CSMS's boot verdict. `Pending` gates the
/// station into the provisioning state machine, so a rename would strand it.
/// Cross-checked against `BootNotificationResponse.json`.
#[test]
fn test_registration_status() {
    pin_all::<RegistrationStatusEnumType>(&["Accepted", "Pending", "Rejected"]);
}

/// `CancelReservationStatusEnumType`. Cross-checked against
/// `CancelReservationResponse.json`.
#[test]
fn test_cancel_reservation_status() {
    pin_all::<CancelReservationStatusEnumType>(&["Accepted", "Rejected"]);
}

/// `ChangeAvailabilityStatusEnumType` — `Scheduled` defers the availability
/// change until the transaction ends. Cross-checked against
/// `ChangeAvailabilityResponse.json`.
#[test]
fn test_change_availability_status() {
    pin_all::<ChangeAvailabilityStatusEnumType>(&["Accepted", "Rejected", "Scheduled"]);
}

/// `OperationalStatusEnumType` — the request-side target state of a
/// `ChangeAvailability`. Cross-checked against `ChangeAvailability.json`.
#[test]
fn test_operational_status() {
    pin_all::<OperationalStatusEnumType>(&["Inoperative", "Operative"]);
}

/// `ClearCacheStatusEnumType`. Cross-checked against `ClearCacheResponse.json`.
#[test]
fn test_clear_cache_status() {
    pin_all::<ClearCacheStatusEnumType>(&["Accepted", "Rejected"]);
}

/// `ClearChargingProfileStatusEnumType` — `Unknown` (not `Rejected`) is the
/// no-match verdict here, a per-command divergence worth pinning. Cross-checked
/// against `ClearChargingProfileResponse.json`.
#[test]
fn test_clear_charging_profile_status() {
    pin_all::<ClearChargingProfileStatusEnumType>(&["Accepted", "Unknown"]);
}

/// `ClearMessageStatusEnumType` — the `ClearDisplayMessage` verdict; likewise
/// `Unknown` rather than `Rejected`. Cross-checked against
/// `ClearDisplayMessageResponse.json`.
#[test]
fn test_clear_message_status() {
    pin_all::<ClearMessageStatusEnumType>(&["Accepted", "Unknown"]);
}

/// `CustomerInformationStatusEnumType` — carries `Invalid` beyond the usual
/// accept/reject pair. Cross-checked against `CustomerInformationResponse.json`.
#[test]
fn test_customer_information_status() {
    pin_all::<CustomerInformationStatusEnumType>(&["Accepted", "Rejected", "Invalid"]);
}

/// `DataTransferStatusEnumType` — the concatenated-acronym verdicts
/// `UnknownMessageId` / `UnknownVendorId` are the rename risk. Cross-checked
/// against `DataTransferResponse.json`.
#[test]
fn test_data_transfer_status() {
    pin_all::<DataTransferStatusEnumType>(&[
        "Accepted",
        "Rejected",
        "UnknownMessageId",
        "UnknownVendorId",
    ]);
}

/// `DisplayMessageStatusEnumType` — the `NotSupported*` family plus
/// `UnknownTransaction` are all embedded-word strings that must serialize
/// verbatim. Cross-checked against `SetDisplayMessageResponse.json`.
#[test]
fn test_display_message_status() {
    pin_all::<DisplayMessageStatusEnumType>(&[
        "Accepted",
        "NotSupportedMessageFormat",
        "Rejected",
        "NotSupportedPriority",
        "NotSupportedState",
        "UnknownTransaction",
    ]);
}

/// `GenericStatusEnumType` — the shared accept/reject verdict reused by
/// `GetCompositeSchedule`, `NotifyEVChargingSchedule`, `PublishFirmware`,
/// `SetMonitoringLevel`, and `SignCertificate`. Cross-checked against
/// `GetCompositeScheduleResponse.json`.
#[test]
fn test_generic_status() {
    pin_all::<GenericStatusEnumType>(&["Accepted", "Rejected"]);
}

/// `GetChargingProfileStatusEnumType` — `NoProfiles` is the empty-result
/// verdict. Cross-checked against `GetChargingProfilesResponse.json`.
#[test]
fn test_get_charging_profile_status() {
    pin_all::<GetChargingProfileStatusEnumType>(&["Accepted", "NoProfiles"]);
}

/// `GetDisplayMessagesStatusEnumType`. Cross-checked against
/// `GetDisplayMessagesResponse.json`.
#[test]
fn test_get_display_messages_status() {
    pin_all::<GetDisplayMessagesStatusEnumType>(&["Accepted", "Unknown"]);
}

/// `NotifyEVChargingNeedsStatusEnumType` — `Processing` is the deferred verdict.
/// Cross-checked against `NotifyEVChargingNeedsResponse.json`.
#[test]
fn test_notify_ev_charging_needs_status() {
    pin_all::<NotifyEVChargingNeedsStatusEnumType>(&["Accepted", "Rejected", "Processing"]);
}

/// `RequestStartStopStatusEnumType` — the shared verdict for
/// `RequestStartTransaction` and `RequestStopTransaction`. Cross-checked against
/// `RequestStartTransactionResponse.json`.
#[test]
fn test_request_start_stop_status() {
    pin_all::<RequestStartStopStatusEnumType>(&["Accepted", "Rejected"]);
}

/// `ReservationUpdateStatusEnumType` — the request-side status a station reports
/// in `ReservationStatusUpdate`. Cross-checked against
/// `ReservationStatusUpdate.json`.
#[test]
fn test_reservation_update_status() {
    pin_all::<ReservationUpdateStatusEnumType>(&["Expired", "Removed"]);
}

/// `ReserveNowStatusEnumType` — the connector-availability verdicts (`Faulted`,
/// `Occupied`, `Unavailable`) beyond accept/reject. Cross-checked against
/// `ReserveNowResponse.json`.
#[test]
fn test_reserve_now_status() {
    pin_all::<ReserveNowStatusEnumType>(&[
        "Accepted",
        "Faulted",
        "Occupied",
        "Rejected",
        "Unavailable",
    ]);
}

/// `ResetEnumType` — the request-side reset kind; `OnIdle` defers until the
/// transaction ends. Cross-checked against `Reset.json`.
#[test]
fn test_reset_type() {
    pin_all::<ResetEnumType>(&["Immediate", "OnIdle"]);
}

/// `ResetStatusEnumType` — the reset verdict; `Scheduled` mirrors an `OnIdle`
/// request. Cross-checked against `ResetResponse.json`.
#[test]
fn test_reset_status() {
    pin_all::<ResetStatusEnumType>(&["Accepted", "Rejected", "Scheduled"]);
}

/// `SendLocalListStatusEnumType` — `VersionMismatch` guards the local-auth-list
/// versioning protocol. Cross-checked against `SendLocalListResponse.json`.
#[test]
fn test_send_local_list_status() {
    pin_all::<SendLocalListStatusEnumType>(&["Accepted", "Failed", "VersionMismatch"]);
}

/// `TransactionEventEnumType` — the `eventType` discriminator on every
/// `TransactionEvent` CALL (`Started` / `Updated` / `Ended` drive the CSMS
/// transaction state machine). Cross-checked against `TransactionEvent.json`.
#[test]
fn test_transaction_event() {
    pin_all::<TransactionEventEnumType>(&["Ended", "Started", "Updated"]);
}

/// `TriggerMessageStatusEnumType` — `NotImplemented` is distinct from
/// `Rejected` (the CSMS asked for a message the station cannot emit).
/// Cross-checked against `TriggerMessageResponse.json`.
#[test]
fn test_trigger_message_status() {
    pin_all::<TriggerMessageStatusEnumType>(&["Accepted", "Rejected", "NotImplemented"]);
}

/// `UnlockStatusEnumType` — the multi-word verdicts `UnlockFailed`,
/// `OngoingAuthorizedTransaction`, `UnknownConnector` must serialize verbatim.
/// Cross-checked against `UnlockConnectorResponse.json`.
#[test]
fn test_unlock_status() {
    pin_all::<UnlockStatusEnumType>(&[
        "Unlocked",
        "UnlockFailed",
        "OngoingAuthorizedTransaction",
        "UnknownConnector",
    ]);
}
