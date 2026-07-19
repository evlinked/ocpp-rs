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
//! `*EnumType` this crate models, split into domain slices:
//!
//! - **Slice 1** (#297) — security / certificate / ISO-15118 /
//!   network-transport / identity domain.
//! - **Slice 2** (#298, this change) — command/operation-status +
//!   transaction/registration lifecycle domain: the reply-status vocabularies
//!   a CSMS reads off every command CALLRESULT, plus the request-side
//!   lifecycle discriminators (`BootReason`, `TransactionEvent`, `Reset`,
//!   `OperationalStatus`).
//! - **Slice 3** (#301) — device-model / monitoring / variables / messaging
//!   domain.
//! - **Slice 4** (#302) — smart-charging + firmware/log domain.
//!
//! With all four slices landed, every one of the 89 `*EnumType`s this crate
//! models is pinned. Every swept enum's wire strings are verified against both
//! `ocpp/v201/enums.py` and the bundled FINAL
//! `crates/ocpp-messages/schemas/v201/*.json`; each divergence is pinned with
//! [`reject`] (or a documented `pin`), never dropped.
//!
//! ## Recommendation catalogs (#357)
//!
//! Beyond the FINAL-schema `*EnumType`s, this suite also pins the reference's
//! standardized-name *catalogs* that back **open** wire strings rather than
//! closed schema `enum`s — the v201 analog of the 1.6J `ConfigurationKey`
//! sweep (#350). These are pinned as vocabularies (`pin` only, no `reject`,
//! since an unlisted value is a valid open-string value, not a rejected one):
//!
//! - `SecurityEventType` (#357) — the 20 OCPP 2.0.1 Part 2 Appendix-1 security
//!   event names, backing the open `SecurityEventNotification.type` field.
//! - `ControllerComponentName` / `PhysicalComponentName` /
//!   `StandardizedVariableName` (#359 slice 1) — the OCPP 2.0.1 Part 2
//!   Appendix-3 device-model name catalogs, backing the open
//!   `ComponentType.name` / `VariableType.name` fields.
//! - the device-model controller variable-/instance-name catalogs of Part 2
//!   Appendix 3 (#359), backing the open `ComponentType.name` /
//!   `VariableType.name` / `ComponentType.instance` device-model fields.
//!   Slice 2a, controllers A–L (#361):
//!   `AlignedDataCtrlrVariableName`, `AuthCacheCtrlrVariableName`,
//!   `AuthCtrlrVariableName`, `CHAdeMOCtrlrVariableName`,
//!   `ClockCtrlrVariableName`, `CustomizationCtrlrVariableName`,
//!   `DeviceDataCtrlrVariableName`, `DeviceDataCtrlrInstanceName`,
//!   `DisplayMessageCtrlrVariableName`, `ISO15118CtrlrVariableName`,
//!   `LocalAuthListCtrlrVariableName`.
//! - slice 2b, controllers M–T (#362) — the Appendix-3 standardized
//!   variable/instance names of each controller component M–T, backing the
//!   same open `VariableType.name` / `ComponentType.instance` fields.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, json, to_value};

use ocpp_types::v201::{
    APNAuthenticationEnumType, AttributeEnumType, AuthorizationStatusEnumType,
    AuthorizeCertificateStatusEnumType, BootReasonEnumType, CancelReservationStatusEnumType,
    CertificateActionEnumType, CertificateSignedStatusEnumType, CertificateSigningUseEnumType,
    ChangeAvailabilityStatusEnumType, ChargingLimitSourceEnumType, ChargingProfileKindEnumType,
    ChargingProfilePurposeEnumType, ChargingProfileStatusEnumType, ChargingRateUnitEnumType,
    ChargingStateEnumType, ClearCacheStatusEnumType, ClearChargingProfileStatusEnumType,
    ClearMessageStatusEnumType, ClearMonitoringStatusEnumType, ComponentCriterionEnumType,
    ConnectorEnumType, ConnectorStatusEnumType, ControllerComponentName, CostKindEnumType,
    CustomerInformationStatusEnumType, DataEnumType, DataTransferStatusEnumType,
    DeleteCertificateStatusEnumType, DisplayMessageStatusEnumType, EnergyTransferModeEnumType,
    EventNotificationEnumType, EventTriggerEnumType, FirmwareStatusEnumType,
    GenericDeviceModelStatusEnumType, GenericStatusEnumType, GetCertificateIdUseEnumType,
    GetCertificateStatusEnumType, GetChargingProfileStatusEnumType,
    GetDisplayMessagesStatusEnumType, GetInstalledCertificateStatusEnumType,
    GetVariableStatusEnumType, HashAlgorithmEnumType, IdTokenEnumType,
    InstallCertificateStatusEnumType, InstallCertificateUseEnumType,
    Iso15118EVCertificateStatusEnumType, LocationEnumType, LogEnumType, LogStatusEnumType,
    MeasurandEnumType, MessageFormatEnumType, MessagePriorityEnumType, MessageStateEnumType,
    MessageTriggerEnumType, MonitorBaseEnumType, MonitorEnumType, MonitoringCriterionEnumType,
    MonitoringCtrlrInstanceName, MonitoringCtrlrVariableName, MutabilityEnumType,
    NotifyEVChargingNeedsStatusEnumType, OCPPCommCtrlrInstanceName, OCPPCommCtrlrVariableName,
    OCPPInterfaceEnumType, OCPPTransportEnumType, OCPPVersionEnumType, OperationalStatusEnumType,
    PhaseEnumType, PhysicalComponentName, PublishFirmwareStatusEnumType, ReadingContextEnumType,
    ReasonEnumType, RecurrencyKindEnumType, RegistrationStatusEnumType, ReportBaseEnumType,
    RequestStartStopStatusEnumType, ReservationCtrlrVariableName, ReservationUpdateStatusEnumType,
    ReserveNowStatusEnumType, ResetEnumType, ResetStatusEnumType, SampledDataCtrlrVariableName,
    SecurityCtrlrVariableName, SecurityEventType, SendLocalListStatusEnumType,
    SetMonitoringStatusEnumType, SetNetworkProfileStatusEnumType, SetVariableStatusEnumType,
    SmartChargingCtrlrInstanceName, SmartChargingCtrlrVariableName, StandardizedVariableName,
    TariffCostCtrlrInstanceName, TariffCostCtrlrVariableName, TransactionEventEnumType,
    TriggerMessageStatusEnumType, TriggerReasonEnumType, TxCtrlrVariableName,
    TxStartStopPointEnumType, UnlockStatusEnumType, UnpublishFirmwareStatusEnumType,
    UpdateEnumType, UpdateFirmwareStatusEnumType, UploadLogStatusEnumType, VPNEnumType,
};
use ocpp_types::v201::{
    AcDcConverterVariableName, AcPhaseSelectorVariableName, AccessBarrierVariableName,
    ActuatorVariableName, AirCoolingSystemVariableName, AlignedDataCtrlrVariableName,
    AreaVentilationVariableName, AuthCacheCtrlrVariableName, AuthCtrlrVariableName,
    BayOccupancySensorVariableName, BeaconLightingVariableName, CHAdeMOCtrlrVariableName,
    CableBreakawaySensorVariableName, CaseAccessSensorVariableName, ChargingStationVariableName,
    ChargingStatusIndicatorVariableName, ClockCtrlrVariableName, CustomizationCtrlrVariableName,
    DeviceDataCtrlrInstanceName, DeviceDataCtrlrVariableName, DisplayMessageCtrlrVariableName,
    ISO15118CtrlrVariableName, LocalAuthListCtrlrVariableName,
};
// Device-model physical-component variable-name catalogs (Appendix 3) — slice 3b.
use ocpp_types::v201::{
    CPPWMControllerVariableName, ChargingStateVariableName, ConnectedEVVariableName,
    ConnectorHolsterReleaseVariableName, ConnectorHolsterSensorVariableName,
    ConnectorPlugRetentionLockVariableName, ConnectorProtectionReleaseVariableName,
    ConnectorVariableName, ControlMeteringVariableName, ControllerVariableName,
};
// Device-model physical-component variable-name catalogs (Appendix 3) — slice 3c.
use ocpp_types::v201::{
    DataLinkVariableName, DisplayVariableName, DistributionPanelVariableName,
    ELVSupplyVariableName, EVRetentionLockVariableName, EVSEVariableName,
    ElectricalFeedVariableName, EmergencyStopSensorVariableName, EnvironmentalLightingVariableName,
};
use ocpp_types::v201::{StandardizedUnitsOfMeasureType, StatusInfoReasonType};

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

// Sweep of the remaining v201 `*EnumType`s (issue #274) — slice 1: the
// security / certificate / ISO-15118 / network-transport / identity domain.
//
// Every enum below is a FINAL-schema field. Each expected wire string is
// verified against BOTH `ocpp/v201/enums.py` (the reference dataclass) AND the
// bundled `crates/ocpp-messages/schemas/v201/*.json` (the FINAL schema
// `SchemaValidator::v201()` enforces). For all 20, model == reference ==
// schema, so — unlike `ConnectorEnumType` / `DataEnumType` above — this slice
// pins no `reject` divergences. The schema file named in each doc comment is
// the FINAL source cross-checked (an enum may recur in several schemas with an
// identical member list; one representative is named).
// ---------------------------------------------------------------------------

/// `APNAuthenticationEnumType` — all-caps acronyms (`CHAP`, `PAP`, `AUTO`) that
/// must serialize verbatim, never title-cased. Schema: `SetNetworkProfile.json`.
#[test]
fn test_apn_authentication() {
    pin_all::<APNAuthenticationEnumType>(&["CHAP", "NONE", "PAP", "AUTO"]);
}

/// `AuthorizationStatusEnumType` — the id-token authorization vocabulary; the
/// embedded-acronym `NotAllowedTypeEVSE` and the `ConcurrentTx` case are the
/// drift risk. Schema: `AuthorizeResponse.json` (also `TransactionEventResponse`,
/// `SendLocalList`).
#[test]
fn test_authorization_status() {
    pin_all::<AuthorizationStatusEnumType>(&[
        "Accepted",
        "Blocked",
        "ConcurrentTx",
        "Expired",
        "Invalid",
        "NoCredit",
        "NotAllowedTypeEVSE",
        "NotAtThisLocation",
        "NotAtThisTime",
        "Unknown",
    ]);
}

/// `AuthorizeCertificateStatusEnumType` — ISO-15118 contract-certificate status;
/// `CertChainError` (abbreviated) and `SignatureError` must serialize verbatim.
/// Schema: `AuthorizeResponse.json`.
#[test]
fn test_authorize_certificate_status() {
    pin_all::<AuthorizeCertificateStatusEnumType>(&[
        "Accepted",
        "SignatureError",
        "CertificateExpired",
        "CertificateRevoked",
        "NoCertificateAvailable",
        "CertChainError",
        "ContractCancelled",
    ]);
}

/// `CertificateActionEnumType` — Install/Update for the ISO-15118 EV certificate
/// flow. Schema: `Get15118EVCertificate.json`.
#[test]
fn test_certificate_action() {
    pin_all::<CertificateActionEnumType>(&["Install", "Update"]);
}

/// `CertificateSignedStatusEnumType` — Accepted/Rejected on a signed cert.
/// Schema: `CertificateSignedResponse.json`.
#[test]
fn test_certificate_signed_status() {
    pin_all::<CertificateSignedStatusEnumType>(&["Accepted", "Rejected"]);
}

/// `CertificateSigningUseEnumType` — the `V2GCertificate` acronym case must
/// serialize verbatim. Schema: `CertificateSigned.json` (also
/// `SignCertificate.json`).
#[test]
fn test_certificate_signing_use() {
    pin_all::<CertificateSigningUseEnumType>(&["ChargingStationCertificate", "V2GCertificate"]);
}

/// `DeleteCertificateStatusEnumType` — Accepted/Failed/NotFound. Schema:
/// `DeleteCertificateResponse.json`.
#[test]
fn test_delete_certificate_status() {
    pin_all::<DeleteCertificateStatusEnumType>(&["Accepted", "Failed", "NotFound"]);
}

/// `GetCertificateIdUseEnumType` — root-certificate selector with the `V2G` /
/// `MO` / `CSMS` acronym prefixes. Schema: `GetInstalledCertificateIds.json`.
#[test]
fn test_get_certificate_id_use() {
    pin_all::<GetCertificateIdUseEnumType>(&[
        "V2GRootCertificate",
        "MORootCertificate",
        "CSMSRootCertificate",
        "V2GCertificateChain",
        "ManufacturerRootCertificate",
    ]);
}

/// `GetCertificateStatusEnumType` — Accepted/Failed on an OCSP status fetch.
/// Schema: `GetCertificateStatusResponse.json`.
#[test]
fn test_get_certificate_status() {
    pin_all::<GetCertificateStatusEnumType>(&["Accepted", "Failed"]);
}

/// `GetInstalledCertificateStatusEnumType` — Accepted/NotFound. Schema:
/// `GetInstalledCertificateIdsResponse.json`.
#[test]
fn test_get_installed_certificate_status() {
    pin_all::<GetInstalledCertificateStatusEnumType>(&["Accepted", "NotFound"]);
}

/// `HashAlgorithmEnumType` — the digit-suffixed `SHA256` / `SHA384` / `SHA512`
/// must serialize verbatim (a `rename_all` mistake would lower-case them).
/// Schema: `Authorize.json` (recurs across the certificate-hash schemas).
#[test]
fn test_hash_algorithm() {
    pin_all::<HashAlgorithmEnumType>(&["SHA256", "SHA384", "SHA512"]);
}

/// `IdTokenEnumType` — the id-token medium; the acronym / digit cases
/// (`eMAID`, `ISO14443`, `ISO15693`) are exactly the rename risk. Schema:
/// `Authorize.json` (recurs in every schema carrying an `IdTokenType`).
#[test]
fn test_id_token() {
    pin_all::<IdTokenEnumType>(&[
        "Central",
        "eMAID",
        "ISO14443",
        "ISO15693",
        "KeyCode",
        "Local",
        "MacAddress",
        "NoAuthorization",
    ]);
}

/// `InstallCertificateStatusEnumType` — Accepted/Rejected/Failed. Schema:
/// `InstallCertificateResponse.json`.
#[test]
fn test_install_certificate_status() {
    pin_all::<InstallCertificateStatusEnumType>(&["Accepted", "Rejected", "Failed"]);
}

/// `InstallCertificateUseEnumType` — root-certificate selector (the
/// `GetCertificateIdUse` set minus `V2GCertificateChain`). Schema:
/// `InstallCertificate.json`.
#[test]
fn test_install_certificate_use() {
    pin_all::<InstallCertificateUseEnumType>(&[
        "V2GRootCertificate",
        "MORootCertificate",
        "CSMSRootCertificate",
        "ManufacturerRootCertificate",
    ]);
}

/// `Iso15118EVCertificateStatusEnumType` — Accepted/Failed on the 15118 EV
/// certificate exchange. Schema: `Get15118EVCertificateResponse.json`.
#[test]
fn test_iso15118_ev_certificate_status() {
    pin_all::<Iso15118EVCertificateStatusEnumType>(&["Accepted", "Failed"]);
}

/// `OCPPInterfaceEnumType` — digit-suffixed network interfaces (`Wired0`…
/// `Wireless3`); an off-by-one rename would silently mis-route a network
/// profile. Schema: `SetNetworkProfile.json`.
#[test]
fn test_ocpp_interface() {
    pin_all::<OCPPInterfaceEnumType>(&[
        "Wired0",
        "Wired1",
        "Wired2",
        "Wired3",
        "Wireless0",
        "Wireless1",
        "Wireless2",
        "Wireless3",
    ]);
}

/// `OCPPTransportEnumType` — the all-caps `JSON` / `SOAP`. Schema:
/// `SetNetworkProfile.json`.
#[test]
fn test_ocpp_transport() {
    pin_all::<OCPPTransportEnumType>(&["JSON", "SOAP"]);
}

/// `OCPPVersionEnumType` — the digit-packed `OCPP12`…`OCPP20` (note: the FINAL
/// 2.0.1 schema uses no dotted form and stops at `OCPP20`). Schema:
/// `SetNetworkProfile.json`.
#[test]
fn test_ocpp_version() {
    pin_all::<OCPPVersionEnumType>(&["OCPP12", "OCPP15", "OCPP16", "OCPP20"]);
}

/// `VPNEnumType` — mixed-case / acronym VPN types (`IKEv2`, `IPSec`, `L2TP`,
/// `PPTP`) that must serialize verbatim. Schema: `SetNetworkProfile.json`.
#[test]
fn test_vpn() {
    pin_all::<VPNEnumType>(&["IKEv2", "IPSec", "L2TP", "PPTP"]);
}

/// `EnergyTransferModeEnumType` — the underscore-joined `AC_single_phase` /
/// `AC_two_phase` / `AC_three_phase` and bare `DC`; serde must keep the
/// underscores (a `rename_all = "camelCase"` slip would drop them). Schema:
/// `NotifyEVChargingNeeds.json`.
#[test]
fn test_energy_transfer_mode() {
    pin_all::<EnergyTransferModeEnumType>(&[
        "DC",
        "AC_single_phase",
        "AC_two_phase",
        "AC_three_phase",
    ]);
}

// ---------------------------------------------------------------------------
// Sweep of the remaining v201 `*EnumType`s (issue #274) — slice 3: the
// device-model / monitoring / variables / messaging domain. These vocabularies
// drive `GetVariables`/`SetVariables`, `SetVariableMonitoring`,
// `NotifyReport`/`NotifyEvent`, `GetBaseReport`/`GetReport`, and the
// `SetDisplayMessage` family.
//
// As in slice 1, every schema-backed enum's wire strings are cross-checked
// against BOTH `ocpp/v201/enums.py` and the bundled
// `crates/ocpp-messages/schemas/v201/*.json`. For all of these except
// `MessageStateEnumType` (below), model == reference == FINAL schema.
//
// ## Divergence pinned, not dropped — `MessageStateEnumType`
//
// The FINAL 2.0.1 `MessageStateEnumType` (bundled `GetDisplayMessages.json`,
// `SetDisplayMessage.json`, `NotifyDisplayMessages.json`) defines **four**
// members — `Charging`, `Faulted`, `Idle`, `Unavailable` — and this crate
// models all four. The reference *dataclass* (`ocpp/v201/enums.py`) omits
// `Unavailable`, so it lags the FINAL schema here. This is the mirror image of
// slice 1's `ConnectorEnumType` reject cases (member the reference has but the
// schema drops): here the schema has a member the reference drops. Rather than
// port only the reference's three, `test_message_state` pins all four — a
// value the crate must accept because `SchemaValidator::v201()` accepts it —
// and documents why the fourth is present.
//
// `MonitorBaseEnumType` is *not* a payload-schema field — it is a device-model
// config vocabulary (like `TxStartStopPointEnumType` above) — so it is verified
// against `ocpp/v201/enums.py` only, with no bundled-schema cross-check.
// ---------------------------------------------------------------------------

/// `AttributeEnumType` — the variable-attribute selector; the acronym-suffixed
/// `MinSet` / `MaxSet` must serialize verbatim. Schema: `GetVariables.json`
/// (recurs across the variable schemas).
#[test]
fn test_attribute() {
    pin_all::<AttributeEnumType>(&["Actual", "Target", "MinSet", "MaxSet"]);
}

/// `ClearMonitoringStatusEnumType` — per-item result of `ClearVariableMonitoring`;
/// `NotFound` must not be title-/snake-cased. Schema:
/// `ClearVariableMonitoringResponse.json`.
#[test]
fn test_clear_monitoring_status() {
    pin_all::<ClearMonitoringStatusEnumType>(&["Accepted", "Rejected", "NotFound"]);
}

/// `ComponentCriterionEnumType` — the `GetReport` component filter. Schema:
/// `GetReport.json`.
#[test]
fn test_component_criterion() {
    pin_all::<ComponentCriterionEnumType>(&["Active", "Available", "Enabled", "Problem"]);
}

/// `EventNotificationEnumType` — the `NotifyEvent` origin; the compound
/// `HardWiredNotification` / `PreconfiguredMonitor` / `CustomMonitor` cases are
/// the drift risk. Schema: `NotifyEvent.json`.
#[test]
fn test_event_notification() {
    pin_all::<EventNotificationEnumType>(&[
        "HardWiredNotification",
        "HardWiredMonitor",
        "PreconfiguredMonitor",
        "CustomMonitor",
    ]);
}

/// `EventTriggerEnumType` — what fired a `NotifyEvent`. Schema: `NotifyEvent.json`.
#[test]
fn test_event_trigger() {
    pin_all::<EventTriggerEnumType>(&["Alerting", "Delta", "Periodic"]);
}

/// `GenericDeviceModelStatusEnumType` — the shared reply status of the
/// device-model report commands (`GetBaseReport`, `GetReport`,
/// `GetMonitoringReport`, `SetMonitoringBase`, `SetMonitoringLevel`); the
/// compound `NotSupported` / `EmptyResultSet` are the drift risk. Schema:
/// `GetBaseReportResponse.json` (recurs across the report responses).
#[test]
fn test_generic_device_model_status() {
    pin_all::<GenericDeviceModelStatusEnumType>(&[
        "Accepted",
        "Rejected",
        "NotSupported",
        "EmptyResultSet",
    ]);
}

/// `GetVariableStatusEnumType` — per-item result of `GetVariables`; the compound
/// `UnknownComponent` / `UnknownVariable` / `NotSupportedAttributeType` are the
/// drift risk. Schema: `GetVariablesResponse.json`.
#[test]
fn test_get_variable_status() {
    pin_all::<GetVariableStatusEnumType>(&[
        "Accepted",
        "Rejected",
        "UnknownComponent",
        "UnknownVariable",
        "NotSupportedAttributeType",
    ]);
}

/// `MessageFormatEnumType` — the display-message content format; every member is
/// an all-caps acronym (`ASCII`, `HTML`, `URI`, `UTF8`) that a stray
/// `rename_all` would lower-case. Schema: `AuthorizeResponse.json` (recurs
/// across the display-message schemas via `MessageContentType`).
#[test]
fn test_message_format() {
    pin_all::<MessageFormatEnumType>(&["ASCII", "HTML", "URI", "UTF8"]);
}

/// `MessagePriorityEnumType` — display-message priority; the compound
/// `AlwaysFront` / `InFront` / `NormalCycle` are the drift risk. Schema:
/// `GetDisplayMessages.json` (also `SetDisplayMessage.json`,
/// `NotifyDisplayMessages.json`).
#[test]
fn test_message_priority() {
    pin_all::<MessagePriorityEnumType>(&["AlwaysFront", "InFront", "NormalCycle"]);
}

/// `MessageStateEnumType` — display-message activation state. The FINAL schema
/// carries **four** members; the reference dataclass omits `Unavailable`, so
/// this suite pins the crate's schema-faithful superset (see the section header
/// above for why `Unavailable` is present). Schema: `GetDisplayMessages.json`
/// (also `SetDisplayMessage.json`, `NotifyDisplayMessages.json`).
#[test]
fn test_message_state() {
    pin_all::<MessageStateEnumType>(&["Charging", "Faulted", "Idle", "Unavailable"]);
}

/// `MessageTriggerEnumType` — which message `TriggerMessage` requests; the
/// acronym cases (`SignV2GCertificate`, `LogStatusNotification`,
/// `PublishFirmwareStatusNotification`) must serialize verbatim. Schema:
/// `TriggerMessage.json`.
#[test]
fn test_message_trigger() {
    pin_all::<MessageTriggerEnumType>(&[
        "BootNotification",
        "LogStatusNotification",
        "FirmwareStatusNotification",
        "Heartbeat",
        "MeterValues",
        "SignChargingStationCertificate",
        "SignV2GCertificate",
        "StatusNotification",
        "TransactionEvent",
        "SignCombinedCertificate",
        "PublishFirmwareStatusNotification",
    ]);
}

/// `MonitorEnumType` — the kind of a variable monitor; `PeriodicClockAligned`
/// (compound) and `UpperThreshold` / `LowerThreshold` are the drift risk.
/// Schema: `NotifyMonitoringReport.json` (also `SetVariableMonitoring.json`).
#[test]
fn test_monitor() {
    pin_all::<MonitorEnumType>(&[
        "UpperThreshold",
        "LowerThreshold",
        "Delta",
        "Periodic",
        "PeriodicClockAligned",
    ]);
}

/// `MonitorBaseEnumType` — the `SetMonitoringBase` preset. A device-model config
/// vocabulary, **not** a payload-schema field, so verified against
/// `ocpp/v201/enums.py` only (no bundled-schema cross-check); the compound
/// `FactoryDefault` / `HardWiredOnly` are the drift risk.
#[test]
fn test_monitor_base() {
    pin_all::<MonitorBaseEnumType>(&["All", "FactoryDefault", "HardWiredOnly"]);
}

/// `MonitoringCriterionEnumType` — the `GetMonitoringReport` filter; every
/// member is a compound `*Monitoring` token. Schema: `GetMonitoringReport.json`.
#[test]
fn test_monitoring_criterion() {
    pin_all::<MonitoringCriterionEnumType>(&[
        "ThresholdMonitoring",
        "DeltaMonitoring",
        "PeriodicMonitoring",
    ]);
}

/// `MutabilityEnumType` — a reported variable's mutability; the compound
/// `ReadOnly` / `WriteOnly` / `ReadWrite` are the drift risk. Schema:
/// `NotifyReport.json`.
#[test]
fn test_mutability() {
    pin_all::<MutabilityEnumType>(&["ReadOnly", "WriteOnly", "ReadWrite"]);
}

/// `ReportBaseEnumType` — the `GetBaseReport` scope; the compound
/// `ConfigurationInventory` / `FullInventory` / `SummaryInventory` are the drift
/// risk. Schema: `GetBaseReport.json`.
#[test]
fn test_report_base() {
    pin_all::<ReportBaseEnumType>(&[
        "ConfigurationInventory",
        "FullInventory",
        "SummaryInventory",
    ]);
}

/// `SetMonitoringStatusEnumType` — per-item result of `SetVariableMonitoring`;
/// the compound `UnknownComponent` / `UnknownVariable` / `UnsupportedMonitorType`
/// and `Duplicate` are the drift risk. Schema:
/// `SetVariableMonitoringResponse.json`.
#[test]
fn test_set_monitoring_status() {
    pin_all::<SetMonitoringStatusEnumType>(&[
        "Accepted",
        "UnknownComponent",
        "UnknownVariable",
        "UnsupportedMonitorType",
        "Rejected",
        "Duplicate",
    ]);
}

/// `SetNetworkProfileStatusEnumType` — reply status of `SetNetworkProfile`.
/// Schema: `SetNetworkProfileResponse.json`.
#[test]
fn test_set_network_profile_status() {
    pin_all::<SetNetworkProfileStatusEnumType>(&["Accepted", "Rejected", "Failed"]);
}

/// `SetVariableStatusEnumType` — per-item result of `SetVariables`; the compound
/// `NotSupportedAttributeType` and `RebootRequired` are the drift risk. Schema:
/// `SetVariablesResponse.json`.
#[test]
fn test_set_variable_status() {
    pin_all::<SetVariableStatusEnumType>(&[
        "Accepted",
        "Rejected",
        "UnknownComponent",
        "UnknownVariable",
        "NotSupportedAttributeType",
        "RebootRequired",
    ]);
}

// ---------------------------------------------------------------------------
// Sweep of the remaining v201 `*EnumType`s (issue #274) — slice 4 (final): the
// smart-charging (charging-profile / charging-schedule) and firmware / log
// domains. These vocabularies drive `SetChargingProfile` / `GetChargingProfiles`
// / `ClearChargingProfile` / `ReportChargingProfiles` / `GetCompositeSchedule` /
// `NotifyChargingLimit`, and the firmware-update (`UpdateFirmware`,
// `FirmwareStatusNotification`, `PublishFirmware` / `UnpublishFirmware`) and
// diagnostics-log (`GetLog`, `LogStatusNotification`) flows.
//
// With this slice all 89 v201 `*EnumType`s are pinned and #274 closes.
//
// As in slices 1 and 3, every enum below is cross-checked against BOTH
// `ocpp/v201/enums.py` (the reference dataclass) AND the bundled FINAL
// `crates/ocpp-messages/schemas/v201/*.json` (what `SchemaValidator::v201()`
// enforces). For 14 of the 15, model == reference == FINAL schema.
//
// ## Divergence pinned, not dropped — `ChargingProfilePurposeEnumType`
//
// The FINAL 2.0.1 `ChargingProfilePurposeEnumType` (bundled
// `SetChargingProfile.json`, `GetChargingProfiles.json`,
// `ClearChargingProfile.json`, `ReportChargingProfiles.json`,
// `RequestStartTransaction.json` — all five agree) defines **four** members —
// `ChargingStationExternalConstraints`, `ChargingStationMaxProfile`,
// `TxDefaultProfile`, `TxProfile` — and this crate models all four. The
// reference *dataclass* omits `ChargingStationExternalConstraints`, so it lags
// the FINAL schema here. This is the same shape as slice 3's
// `MessageStateEnumType`: the schema carries a member the reference drops.
// Rather than port only the reference's three, `test_charging_profile_purpose`
// pins all four — a value the crate must accept because `SchemaValidator::v201()`
// accepts it — and documents why the fourth is present. No `reject` cases in
// this slice: no reference member is absent from the FINAL schema.
// ---------------------------------------------------------------------------

/// `ChargingLimitSourceEnumType` — the origin of an external charging limit; the
/// all-caps acronyms (`EMS`, `SO`, `CSO`) carry `#[serde(rename)]` overrides and
/// must serialize verbatim, never title-cased. Schema:
/// `ClearedChargingLimit.json` (also `NotifyChargingLimit.json`,
/// `ReportChargingProfiles.json`).
#[test]
fn test_charging_limit_source() {
    pin_all::<ChargingLimitSourceEnumType>(&["EMS", "Other", "SO", "CSO"]);
}

/// `ChargingProfileKindEnumType` — whether a profile's schedule is `Absolute`,
/// `Recurring`, or `Relative` to the transaction start. Schema:
/// `ReportChargingProfiles.json` (recurs across the charging-profile schemas via
/// `ChargingProfileType`).
#[test]
fn test_charging_profile_kind() {
    pin_all::<ChargingProfileKindEnumType>(&["Absolute", "Recurring", "Relative"]);
}

/// `ChargingProfilePurposeEnumType` — the role a charging profile plays. The
/// FINAL schema carries **four** members; the reference dataclass omits
/// `ChargingStationExternalConstraints`, so this suite pins the crate's
/// schema-faithful superset (see the section header for why the fourth member is
/// present). Schema: `ClearChargingProfile.json` (also `SetChargingProfile.json`,
/// `GetChargingProfiles.json`, `ReportChargingProfiles.json`,
/// `RequestStartTransaction.json`).
#[test]
fn test_charging_profile_purpose() {
    pin_all::<ChargingProfilePurposeEnumType>(&[
        "ChargingStationExternalConstraints",
        "ChargingStationMaxProfile",
        "TxDefaultProfile",
        "TxProfile",
    ]);
}

/// `ChargingProfileStatusEnumType` — the `SetChargingProfile` reply status.
/// Schema: `SetChargingProfileResponse.json`.
#[test]
fn test_charging_profile_status() {
    pin_all::<ChargingProfileStatusEnumType>(&["Accepted", "Rejected"]);
}

/// `ChargingRateUnitEnumType` — the unit of a charging-schedule limit: the
/// single-letter wire values `W` (watts) / `A` (amperes). These are valid Rust
/// identifiers modelled with no rename, so the round-trip catches a wrong
/// variant name or a stray `rename_all` that would lower-case them. Schema:
/// `GetCompositeSchedule.json` (recurs across the charging-schedule schemas).
#[test]
fn test_charging_rate_unit() {
    pin_all::<ChargingRateUnitEnumType>(&["W", "A"]);
}

/// `CostKindEnumType` — the kind of cost in a sales-tariff cost entry; the
/// compound `CarbonDioxideEmission` / `RelativePricePercentage` /
/// `RenewableGenerationPercentage` are the drift risk. Schema:
/// `NotifyChargingLimit.json` (recurs wherever a `SalesTariff` / `CostType`
/// appears, e.g. `SetChargingProfile.json`).
#[test]
fn test_cost_kind() {
    pin_all::<CostKindEnumType>(&[
        "CarbonDioxideEmission",
        "RelativePricePercentage",
        "RenewableGenerationPercentage",
    ]);
}

/// `RecurrencyKindEnumType` — the recurrence period of a `Recurring` profile:
/// `Daily` / `Weekly`. Schema: `ReportChargingProfiles.json` (recurs via
/// `ChargingProfileType`).
#[test]
fn test_recurrency_kind() {
    pin_all::<RecurrencyKindEnumType>(&["Daily", "Weekly"]);
}

/// `FirmwareStatusEnumType` — the firmware-update lifecycle reported in
/// `FirmwareStatusNotification.req`; the compound download / install states
/// (`DownloadScheduled`, `InstallRebooting`, `InstallVerificationFailed`,
/// `SignatureVerified`) are the drift risk. Schema:
/// `FirmwareStatusNotification.json`.
#[test]
fn test_firmware_status() {
    pin_all::<FirmwareStatusEnumType>(&[
        "Downloaded",
        "DownloadFailed",
        "Downloading",
        "DownloadScheduled",
        "DownloadPaused",
        "Idle",
        "InstallationFailed",
        "Installing",
        "Installed",
        "InstallRebooting",
        "InstallScheduled",
        "InstallVerificationFailed",
        "InvalidSignature",
        "SignatureVerified",
    ]);
}

/// `LogEnumType` — which log a `GetLog` request asks for: `DiagnosticsLog` or
/// `SecurityLog`. Schema: `GetLog.json`.
#[test]
fn test_log() {
    pin_all::<LogEnumType>(&["DiagnosticsLog", "SecurityLog"]);
}

/// `LogStatusEnumType` — the synchronous `GetLog` accept/reject ack;
/// `AcceptedCanceled` (a running upload was canceled to serve this one) must
/// serialize verbatim. Schema: `GetLogResponse.json`.
#[test]
fn test_log_status() {
    pin_all::<LogStatusEnumType>(&["Accepted", "Rejected", "AcceptedCanceled"]);
}

/// `PublishFirmwareStatusEnumType` — the publish-to-local-cache lifecycle
/// reported in `PublishFirmwareStatusNotification.req`; the compound
/// `DownloadScheduled` / `InvalidChecksum` / `ChecksumVerified` / `PublishFailed`
/// are the drift risk. Schema: `PublishFirmwareStatusNotification.json`.
#[test]
fn test_publish_firmware_status() {
    pin_all::<PublishFirmwareStatusEnumType>(&[
        "Idle",
        "DownloadScheduled",
        "Downloading",
        "Downloaded",
        "Published",
        "DownloadFailed",
        "DownloadPaused",
        "InvalidChecksum",
        "ChecksumVerified",
        "PublishFailed",
    ]);
}

/// `UnpublishFirmwareStatusEnumType` — the `UnpublishFirmware` reply:
/// `DownloadOngoing` / `NoFirmware` / `Unpublished`. Schema:
/// `UnpublishFirmwareResponse.json`.
#[test]
fn test_unpublish_firmware_status() {
    pin_all::<UnpublishFirmwareStatusEnumType>(&["DownloadOngoing", "NoFirmware", "Unpublished"]);
}

/// `UpdateEnumType` — the `SendLocalList` update mode: `Differential` or `Full`.
/// Schema: `SendLocalList.json`.
#[test]
fn test_update() {
    pin_all::<UpdateEnumType>(&["Differential", "Full"]);
}

/// `UpdateFirmwareStatusEnumType` — the `UpdateFirmware` reply; the compound
/// `AcceptedCanceled` / `InvalidCertificate` / `RevokedCertificate` are the drift
/// risk. Schema: `UpdateFirmwareResponse.json`.
#[test]
fn test_update_firmware_status() {
    pin_all::<UpdateFirmwareStatusEnumType>(&[
        "Accepted",
        "Rejected",
        "AcceptedCanceled",
        "InvalidCertificate",
        "RevokedCertificate",
    ]);
}

/// `UploadLogStatusEnumType` — the log-upload *progress* reported in
/// `LogStatusNotification.req` while a `GetLog` flow proceeds; the compound
/// `BadMessage` / `NotSupportedOperation` / `PermissionDenied` / `UploadFailure`
/// / `AcceptedCanceled` are the drift risk. Distinct from [`LogStatusEnumType`],
/// the synchronous `GetLog` ack. Schema: `LogStatusNotification.json`.
#[test]
fn test_upload_log_status() {
    pin_all::<UploadLogStatusEnumType>(&[
        "BadMessage",
        "Idle",
        "NotSupportedOperation",
        "PermissionDenied",
        "Uploaded",
        "UploadFailure",
        "Uploading",
        "AcceptedCanceled",
    ]);
}

/// `SecurityEventType` — the 20 standardized security-event names of OCPP 2.0.1
/// Part 2, Appendix 1 (v1.3), reported by `SecurityEventNotification`.
///
/// Unlike every other enum in this suite, `SecurityEventType` is **not** a
/// FINAL-schema `enum`: the message's `type` field is an open `string`
/// (`maxLength: 50`), and this enum is an available-but-not-forced recommendation
/// vocabulary (the v201 analog of the 1.6J `ConfigurationKey`, #350). So this
/// pins the *catalog* wire spellings rather than cross-checking a schema `enum`,
/// and there are deliberately no [`reject`] calls — an unlisted event name is a
/// valid open-string value, not a rejected one. The drift risks are the
/// acronym / compound spellings: `InvalidTLSVersion`, `InvalidTLSCipherSuite`,
/// `FailedToAuthenticateAtCsms`, `CsmsFailedToAuthenticate`.
#[test]
fn test_security_event_type() {
    pin_all::<SecurityEventType>(&[
        "FirmwareUpdated",
        "FailedToAuthenticateAtCsms",
        "CsmsFailedToAuthenticate",
        "SettingSystemTime",
        "StartupOfTheDevice",
        "ResetOrReboot",
        "SecurityLogWasCleared",
        "ReconfigurationOfSecurityParameters",
        "MemoryExhaustion",
        "InvalidMessages",
        "AttemptedReplayAttacks",
        "TamperDetectionActivated",
        "InvalidFirmwareSignature",
        "InvalidFirmwareSigningCertificate",
        "InvalidCsmsCertificate",
        "InvalidChargingStationCertificate",
        "InvalidTLSVersion",
        "InvalidTLSCipherSuite",
        "MaintenanceLoginAccepted",
        "MaintenanceLoginFailed",
    ]);
}

// ---------------------------------------------------------------------------
// Device-model controller variable-/instance-name catalogs (#359, Appendix 3)
//
// Open recommendation vocabularies backing the device model's open
// `ComponentType.name` / `VariableType.name` string fields — pinned as catalogs
// (`pin_all` only, no `reject`, since an unlisted name is a valid open-string
// value). Slice 2, controllers A–L. Each list is verbatim from the reference's
// `*CtrlrVariableName` / `*CtrlrInstanceName` `StrEnum`s in
// `ocpp/v201/enums.py`. Drift risk is the acronym / parenthesized spellings:
// `CHAdeMOProtocolNumber`, `SelftestActive(Set)`, `SeccId`, `PnCEnabled`,
// `V2GCertificateInstallationEnabled`.
// ---------------------------------------------------------------------------

/// Ports `AlignedDataCtrlrVariableName` (8 members).
#[test]
fn test_aligned_data_ctrlr_variable_name() {
    pin_all::<AlignedDataCtrlrVariableName>(&[
        "Available",
        "Enabled",
        "Interval",
        "Measurands",
        "SendDuringIdle",
        "SignReadings",
        "TxEndedInterval",
        "TxEndedMeasurands",
    ]);
}

/// Ports `AuthCacheCtrlrVariableName` (6 members).
#[test]
fn test_auth_cache_ctrlr_variable_name() {
    pin_all::<AuthCacheCtrlrVariableName>(&[
        "Available",
        "Enabled",
        "LifeTime",
        "Policy",
        "Storage",
        "DisablePostAuthorize",
    ]);
}

/// Ports `AuthCtrlrVariableName` (8 members).
#[test]
fn test_auth_ctrlr_variable_name() {
    pin_all::<AuthCtrlrVariableName>(&[
        "AdditionalInfoItemsPerMessage",
        "AuthorizeRemoteStart",
        "Enabled",
        "LocalAuthorizeOffline",
        "LocalPreAuthorize",
        "MasterPassGroupId",
        "OfflineTxForUnknownIdEnabled",
        "DisableRemoteAuthorization",
    ]);
}

/// Ports `CHAdeMOCtrlrVariableName` (13 members). Pins the acronym
/// `CHAdeMOProtocolNumber` and the parenthesized `SelftestActive(Set)`, which
/// carries a `#[serde(rename)]` on the Rust side.
#[test]
fn test_chademo_ctrlr_variable_name() {
    pin_all::<CHAdeMOCtrlrVariableName>(&[
        "Enabled",
        "Active",
        "Complete",
        "Tripped",
        "Problem",
        "SelftestActive",
        "SelftestActive(Set)",
        "CHAdeMOProtocolNumber",
        "VehicleStatus",
        "DynamicControl",
        "HighCurrentControl",
        "HighVoltageControl",
        "AutoManufacturerCode",
    ]);
}

/// Ports `ClockCtrlrVariableName` (8 members).
#[test]
fn test_clock_ctrlr_variable_name() {
    pin_all::<ClockCtrlrVariableName>(&[
        "DateTime",
        "NextTimeOffsetTransitionDateTime",
        "NtpServerUri",
        "NtpSource",
        "TimeAdjustmentReportingThreshold",
        "TimeOffset",
        "TimeSource",
        "TimeZone",
    ]);
}

/// Ports `CustomizationCtrlrVariableName` (1 member).
#[test]
fn test_customization_ctrlr_variable_name() {
    pin_all::<CustomizationCtrlrVariableName>(&["CustomImplementationEnabled"]);
}

/// Ports `DeviceDataCtrlrVariableName` (5 members).
#[test]
fn test_device_data_ctrlr_variable_name() {
    pin_all::<DeviceDataCtrlrVariableName>(&[
        "BytesPerMessage",
        "ConfigurationValueSize",
        "ItemsPerMessage",
        "ReportingValueSize",
        "ValueSize",
    ]);
}

/// Ports `DeviceDataCtrlrInstanceName` (3 members).
#[test]
fn test_device_data_ctrlr_instance_name() {
    pin_all::<DeviceDataCtrlrInstanceName>(&["GetReport", "GetVariables", "SetVariables"]);
}

/// Ports `DisplayMessageCtrlrVariableName` (6 members).
#[test]
fn test_display_message_ctrlr_variable_name() {
    pin_all::<DisplayMessageCtrlrVariableName>(&[
        "Available",
        "DisplayMessages",
        "Enabled",
        "PersonalMessageSize",
        "SupportedFormats",
        "SupportedPriorities",
    ]);
}

/// Ports `ISO15118CtrlrVariableName` (18 members). Pins the acronym /
/// compound spellings `SeccId`, `PnCEnabled`,
/// `V2GCertificateInstallationEnabled`, and the parenthesized
/// `SelftestActive(Set)` (`#[serde(rename)]` on the Rust side).
#[test]
fn test_iso15118_ctrlr_variable_name() {
    pin_all::<ISO15118CtrlrVariableName>(&[
        "Active",
        "Enabled",
        "CentralContractValidationAllowed",
        "Complete",
        "ContractValidationOffline",
        "SeccId",
        "SelftestActive",
        "SelftestActive(Set)",
        "MaxScheduleEntries",
        "RequestedEnergyTransferMode",
        "RequestMeteringReceipt",
        "CountryName",
        "OrganizationName",
        "PnCEnabled",
        "Problem",
        "Tripped",
        "V2GCertificateInstallationEnabled",
        "ContractCertificateInstallationEnabled",
    ]);
}

/// Ports `LocalAuthListCtrlrVariableName` (7 members).
#[test]
fn test_local_auth_list_ctrlr_variable_name() {
    pin_all::<LocalAuthListCtrlrVariableName>(&[
        "Available",
        "BytesPerMessage",
        "Enabled",
        "Entries",
        "ItemsPerMessage",
        "Storage",
        "DisablePostAuthorize",
    ]);
}

/// `ControllerComponentName` — the 18 standardized *logical* (controller)
/// component names of the OCPP 2.0.1 device model (Part 2 Appendix 3.1 v1.3),
/// backing the open `ComponentType.name` field.
///
/// Like [`SecurityEventType`], this is an open recommendation vocabulary, not a
/// FINAL-schema `enum` — `ComponentType.name` is an open `string`, so this pins
/// the *catalog* wire spellings with no [`reject`] calls. The drift risks are
/// the acronym spellings that must serialize verbatim: `CHAdeMOCtrlr`,
/// `ISO15118Ctrlr`, `OCPPCommCtrlr`.
#[test]
fn test_controller_component_name() {
    pin_all::<ControllerComponentName>(&[
        "AlignedDataCtrlr",
        "AuthCacheCtrlr",
        "AuthCtrlr",
        "CHAdeMOCtrlr",
        "ClockCtrlr",
        "CustomizationCtrlr",
        "DeviceDataCtrlr",
        "DisplayMessageCtrlr",
        "ISO15118Ctrlr",
        "LocalAuthListCtrlr",
        "MonitoringCtrlr",
        "OCPPCommCtrlr",
        "ReservationCtrlr",
        "SampledDataCtrlr",
        "SecurityCtrlr",
        "SmartChargingCtrlr",
        "TariffCostCtrlr",
        "TxCtrlr",
    ]);
}

/// `PhysicalComponentName` — the 56 standardized *physical* component names of
/// the OCPP 2.0.1 device model (Part 2 Appendix 3.2 v1.3), backing the open
/// `ComponentType.name` field.
///
/// Open recommendation vocabulary (no [`reject`]). The drift risks are the
/// acronym / compound spellings that must serialize verbatim: `AcDcConverter`,
/// `CPPWMController`, `ELVSupply`, `EVSE`, `RCD`, `RCDRecloser`, `UIInput`.
#[test]
fn test_physical_component_name() {
    pin_all::<PhysicalComponentName>(&[
        "AccessBarrier",
        "AcDcConverter",
        "AcPhaseSelector",
        "Actuator",
        "AirCoolingSystem",
        "AreaVentilation",
        "BayOccupancySensor",
        "BeaconLighting",
        "CableBreakawaySensor",
        "CaseAccessSensor",
        "ChargingStation",
        "ChargingStatusIndicator",
        "ConnectedEV",
        "Connector",
        "ConnectorHolsterRelease",
        "ConnectorHolsterSensor",
        "ConnectorPlugRetentionLock",
        "ConnectorProtectionRelease",
        "Controller",
        "ControlMetering",
        "CPPWMController",
        "DataLink",
        "Display",
        "DistributionPanel",
        "ElectricalFeed",
        "ELVSupply",
        "EmergencyStopSensor",
        "EnvironmentalLighting",
        "EVRetentionLock",
        "EVSE",
        "ExternalTemperatureSensor",
        "FiscalMetering",
        "FloodSensor",
        "GroundIsolationProtection",
        "Heater",
        "HumiditySensor",
        "LightSensor",
        "LiquidCoolingSystem",
        "LocalAvailabilitySensor",
        "LocalController",
        "LocalEnergyStorage",
        "OverCurrentProtection",
        "OverCurrentProtectionRecloser",
        "PowerContactor",
        "RCD",
        "RCDRecloser",
        "RealTimeClock",
        "ShockSensor",
        "SpacesCountSignage",
        "Switch",
        "TemperatureSensor",
        "TiltSensor",
        "TokenReader",
        "UIInput",
        "UpstreamProtectionTrigger",
        "VehicleIdSensor",
    ]);
}

/// `StandardizedVariableName` — the 91 standardized (component-non-specific)
/// variable names of the OCPP 2.0.1 device model (Part 2 Appendix 3 v1.3),
/// backing the open `VariableType.name` field.
///
/// Open recommendation vocabulary (no [`reject`]). The drift risks are the
/// acronym spellings that must serialize verbatim: `ACCurrent`, `ACVoltage`,
/// `DCCurrent`, `DCVoltage`, `ICCID`, `IMSI`, `ISO15118EvseId`, `SeccId`.
#[test]
fn test_standardized_variable_name() {
    pin_all::<StandardizedVariableName>(&[
        "ACCurrent",
        "Active",
        "ACVoltage",
        "AllowReset",
        "Angle",
        "Attempts",
        "AvailabilityState",
        "Available",
        "Certificate",
        "ChargeProtocol",
        "ChargingCompleteBulk",
        "ChargingCompleteFull",
        "ChargingTime",
        "Color",
        "Complete",
        "ConnectedTime",
        "ConnectorType",
        "Count",
        "Currency",
        "CurrentImbalance",
        "DataText",
        "DateTime",
        "DCCurrent",
        "DCVoltage",
        "DepartureTime",
        "ECVariant",
        "Enabled",
        "Energy",
        "EnergyCapacity",
        "EnergyExport",
        "EnergyExportRegister",
        "EnergyImport",
        "EnergyImportRegister",
        "Entries",
        "EvseId",
        "Fallback",
        "FanSpeed",
        "FirmwareVersion",
        "Force",
        "Formats",
        "Frequency",
        "FuseRating",
        "Height",
        "Humidity",
        "Hysteresis",
        "ICCID",
        "Impedance",
        "IMSI",
        "Interval",
        "ISO15118EvseId",
        "Length",
        "Light",
        "Manufacturer",
        "Message",
        "MinimumStatusDuration",
        "Mode",
        "Model",
        "NetworkAddress",
        "Operated",
        "OperatingTimes",
        "Overload",
        "Percent",
        "PhaseRotation",
        "PostChargingTime",
        "Power",
        "Problem",
        "Protecting",
        "RemainingTimeBulk",
        "RemainingTimeFull",
        "SeccId",
        "SerialNumber",
        "SignalStrength",
        "State",
        "StateOfCharge",
        "StateOfChargeBulk",
        "Storage",
        "SupplyPhases",
        "Suspending",
        "Suspension",
        "Temperature",
        "Time",
        "TimeOffset",
        "Timeout",
        "Token",
        "TokenType",
        "Tries",
        "Tripped",
        "VehicleId",
        "VersionDate",
        "VersionNumber",
        "VoltageImbalance",
    ]);
}

// ---------------------------------------------------------------------------
// Device-model physical-component variable-name catalogs (#359 slice 3a)
//
// The per-physical-component `*VariableName` sets `AccessBarrier` …
// `ChargingStatusIndicator` from OCPP 2.0.1 Part 2 Appendix 3, ported from
// `ocpp/v201/enums.py`. Open recommendation vocabularies (no [`reject`]): the
// `ComponentType.name` / `VariableType.name` wire fields stay `String`. Each
// test pins every reference wire string byte-for-byte; the drift risks are the
// acronym spellings (`DCCurrent`, `ACCurrent`, `ECVariant`) and the
// parenthesized `"…(Set)"` / `"…(MaxLimit)"` renames.
// ---------------------------------------------------------------------------

/// Ports `AccessBarrierVariableName` (3 members).
#[test]
fn test_access_barrier_variable_name() {
    pin_all::<AccessBarrierVariableName>(&["Enabled", "Active", "Problem"]);
}

/// Ports `AcDcConverterVariableName` (9 members). Pins the `DCCurrent` /
/// `DCVoltage` acronym spellings.
#[test]
fn test_ac_dc_converter_variable_name() {
    pin_all::<AcDcConverterVariableName>(&[
        "DCCurrent",
        "DCVoltage",
        "Enabled",
        "FanSpeed",
        "Overload",
        "Power",
        "Problem",
        "Temperature",
        "Tripped",
    ]);
}

/// Ports `AcPhaseSelectorVariableName` (4 members).
#[test]
fn test_ac_phase_selector_variable_name() {
    pin_all::<AcPhaseSelectorVariableName>(&["Active", "Enabled", "PhaseRotation", "Problem"]);
}

/// Ports `ActuatorVariableName` (4 members).
#[test]
fn test_actuator_variable_name() {
    pin_all::<ActuatorVariableName>(&["Active", "Enabled", "Problem", "State"]);
}

/// Ports `AirCoolingSystemVariableName` (4 members).
#[test]
fn test_air_cooling_system_variable_name() {
    pin_all::<AirCoolingSystemVariableName>(&["Active", "Enabled", "Problem", "FanSpeed"]);
}

/// Ports `AreaVentilationVariableName` (4 members).
#[test]
fn test_area_ventilation_variable_name() {
    pin_all::<AreaVentilationVariableName>(&["Active", "Enabled", "Problem", "FanSpeed"]);
}

/// Ports `BayOccupancySensorVariableName` (3 members).
#[test]
fn test_bay_occupancy_sensor_variable_name() {
    pin_all::<BayOccupancySensorVariableName>(&["Active", "Enabled", "Percent"]);
}

/// Ports `BeaconLightingVariableName` (8 members). Pins the settable
/// `"Enabled(Set)"` / `"Percent(Set)"` renames.
#[test]
fn test_beacon_lighting_variable_name() {
    pin_all::<BeaconLightingVariableName>(&[
        "Active",
        "Color",
        "Enabled",
        "Enabled(Set)",
        "Percent",
        "Percent(Set)",
        "Power",
        "Problem",
    ]);
}

/// Ports `CableBreakawaySensorVariableName` (3 members).
#[test]
fn test_cable_breakaway_sensor_variable_name() {
    pin_all::<CableBreakawaySensorVariableName>(&["Active", "Enabled", "Tripped"]);
}

/// Ports `CaseAccessSensorVariableName` (5 members). Pins the settable
/// `"Enabled(Set)"` rename.
#[test]
fn test_case_access_sensor_variable_name() {
    pin_all::<CaseAccessSensorVariableName>(&[
        "Active",
        "Enabled",
        "Enabled(Set)",
        "Problem",
        "Tripped",
    ]);
}

/// Ports `ChargingStationVariableName` (23 members). Pins the `ACCurrent` /
/// `ACVoltage` / `ECVariant` acronym spellings and the `"…(MaxLimit)"` renames.
#[test]
fn test_charging_station_variable_name() {
    pin_all::<ChargingStationVariableName>(&[
        "ACCurrent",
        "ACVoltage",
        "ACVoltage(MaxLimit)",
        "AllowNewSessionsPendingFirmwareUpdate",
        "Available",
        "AvailabilityState",
        "ChargeProtocol",
        "CurrentImbalance",
        "ECVariant",
        "Enabled",
        "Model",
        "OperatingTimes",
        "Overload",
        "PhaseRotation",
        "Power",
        "Power(MaxLimit)",
        "Problem",
        "SerialNumber",
        "SupplyPhases",
        "SupplyPhases(MaxLimit)",
        "Tripped",
        "VendorName",
        "VoltageImbalance",
    ]);
}

/// Ports `ChargingStatusIndicatorVariableName` (2 members).
#[test]
fn test_charging_status_indicator_variable_name() {
    pin_all::<ChargingStatusIndicatorVariableName>(&["Active", "Color"]);
}

// ---------------------------------------------------------------------------
// #359 slice 2b — per-controller variable-/instance-name catalogs, M–T.
//
// Open recommendation vocabularies (Part 2 Appendix 3 v1.3) backing the *open*
// `VariableType.name` / `ComponentType.instance` fields — pinned as catalogs
// (`pin_all` only, no `reject`, since an unlisted name is a valid open-string
// value). Verified byte-for-byte against the `*Ctrlr*Name` `StrEnum`s in
// `ocpp/v201/enums.py`. 12 classes, 79 members.
// ---------------------------------------------------------------------------

/// `MonitoringCtrlrVariableName` — the 9 standardized `MonitoringCtrlr`
/// variable names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_monitoring_ctrlr_variable_name() {
    pin_all::<MonitoringCtrlrVariableName>(&[
        "Available",
        "BytesPerMessage",
        "Enabled",
        "ItemsPerMessage",
        "OfflineQueuingSeverity",
        "MonitoringBase",
        "MonitoringLevel",
        "ActiveMonitoringBase",
        "ActiveMonitoringLevel",
    ]);
}

/// `MonitoringCtrlrInstanceName` — the 2 standardized `MonitoringCtrlr`
/// instance names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_monitoring_ctrlr_instance_name() {
    pin_all::<MonitoringCtrlrInstanceName>(&["ClearVariableMonitoring", "SetVariableMonitoring"]);
}

/// `OCPPCommCtrlrVariableName` — the 19 standardized `OCPPCommCtrlr` variable
/// names (open recommendation vocabulary, no [`reject`]). Drift risks are the
/// acronym spellings `UnlockOnEVSideDisconnect`, `WebSocketPingInterval`.
#[test]
fn test_ocpp_comm_ctrlr_variable_name() {
    pin_all::<OCPPCommCtrlrVariableName>(&[
        "ActiveNetworkProfile",
        "FileTransferProtocols",
        "HeartbeatInterval",
        "MessageTimeout",
        "MessageAttemptInterval",
        "MessageAttempts",
        "MinimumStatusDuration",
        "NetworkConfigurationPriority",
        "NetworkProfileConnectionAttempts",
        "OfflineThreshold",
        "PublicKeyWithSignedMeterValue",
        "QueueAllMessages",
        "ResetRetries",
        "RetryBackOffRandomRange",
        "RetryBackOffRepeatTimes",
        "RetryBackOffWaitMinimum",
        "UnlockOnEVSideDisconnect",
        "WebSocketPingInterval",
        "FieldLength",
    ]);
}

/// `OCPPCommCtrlrInstanceName` — the 2 standardized `OCPPCommCtrlr` instance
/// names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_ocpp_comm_ctrlr_instance_name() {
    pin_all::<OCPPCommCtrlrInstanceName>(&["Default", "TransactionEvent"]);
}

/// `ReservationCtrlrVariableName` — the 3 standardized `ReservationCtrlr`
/// variable names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_reservation_ctrlr_variable_name() {
    pin_all::<ReservationCtrlrVariableName>(&["Available", "Enabled", "NonEvseSpecific"]);
}

/// `SampledDataCtrlrVariableName` — the 9 standardized `SampledDataCtrlr`
/// variable names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_sampled_data_ctrlr_variable_name() {
    pin_all::<SampledDataCtrlrVariableName>(&[
        "Available",
        "Enabled",
        "SignReadings",
        "TxEndedInterval",
        "TxEndedMeasurands",
        "TxStartedMeasurands",
        "TxUpdatedInterval",
        "TxUpdatedMeasurands",
        "RegisterValuesWithoutPhases",
    ]);
}

/// `SecurityCtrlrVariableName` — the 9 standardized `SecurityCtrlr` variable
/// names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_security_ctrlr_variable_name() {
    pin_all::<SecurityCtrlrVariableName>(&[
        "AdditionalRootCertificateCheck",
        "BasicAuthPassword",
        "CertificateEntries",
        "CertSigningRepeatTimes",
        "CertSigningWaitMinimum",
        "Identity",
        "MaxCertificateChainSize",
        "OrganizationName",
        "SecurityProfile",
    ]);
}

/// `SmartChargingCtrlrVariableName` — the 11 standardized `SmartChargingCtrlr`
/// variable names (open recommendation vocabulary, no [`reject`]). Drift risks
/// are the acronym `ACPhaseSwitchingSupported` and the digit-bearing
/// `Phases3to1`.
#[test]
fn test_smart_charging_ctrlr_variable_name() {
    pin_all::<SmartChargingCtrlrVariableName>(&[
        "ACPhaseSwitchingSupported",
        "Available",
        "Enabled",
        "Entries",
        "ExternalControlSignalsEnabled",
        "LimitChangeSignificance",
        "NotifyChargingLimitWithSchedules",
        "PeriodsPerSchedule",
        "Phases3to1",
        "ProfileStackLevel",
        "RateUnit",
    ]);
}

/// `SmartChargingCtrlrInstanceName` — the single standardized
/// `SmartChargingCtrlr` instance name (open recommendation vocabulary, no
/// [`reject`]).
#[test]
fn test_smart_charging_ctrlr_instance_name() {
    pin_all::<SmartChargingCtrlrInstanceName>(&["ChargingProfiles"]);
}

/// `TariffCostCtrlrVariableName` — the 5 standardized `TariffCostCtrlr`
/// variable names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_tariff_cost_ctrlr_variable_name() {
    pin_all::<TariffCostCtrlrVariableName>(&[
        "Available",
        "Currency",
        "Enabled",
        "TariffFallbackMessage",
        "TotalCostFallbackMessage",
    ]);
}

/// `TariffCostCtrlrInstanceName` — the 2 standardized `TariffCostCtrlr`
/// instance names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_tariff_cost_ctrlr_instance_name() {
    pin_all::<TariffCostCtrlrInstanceName>(&["Tariff", "Cost"]);
}

/// `TxCtrlrVariableName` — the 7 standardized `TxCtrlr` variable names (open
/// recommendation vocabulary, no [`reject`]). Drift risks are the acronym
/// spellings `EVConnectionTimeOut`, `StopTxOnEVSideDisconnect`.
#[test]
fn test_tx_ctrlr_variable_name() {
    pin_all::<TxCtrlrVariableName>(&[
        "EVConnectionTimeOut",
        "MaxEnergyOnInvalidId",
        "StopTxOnEVSideDisconnect",
        "StopTxOnInvalidId",
        "TxBeforeAcceptedEnabled",
        "TxStartPoint",
        "TxStopPoint",
    ]);
}

// --- Device-model physical-component variable-name catalogs (Appendix 3) — 3b ---
// Open recommendation vocabularies, no `reject` — an unlisted name is a valid
// open-string value. Wire strings verified byte-for-byte against
// `ocpp/v201/enums.py` (`ConnectedEV` … `CPPWMController`, 98 members).

/// `ConnectedEVVariableName` — the 35 standardized `ConnectedEV` variable names
/// (open recommendation vocabulary, no [`reject`]). Drift risks are the dotted
/// measurement spellings (`ACCurrent.minSet`, `DCVoltage.target`, `Power.maxSet`,
/// `StateOfCharge.actual`, …) and the `ProtocolSupportedByEV` acronym.
#[test]
fn test_connected_ev_variable_name() {
    pin_all::<ConnectedEVVariableName>(&[
        "Available",
        "VehicleId",
        "ProtocolAgreed",
        "ProtocolSupportedByEV",
        "ACCurrent.minSet",
        "ACCurrent.maxSet",
        "ACVoltage.maxSet",
        "DCCurrent.minSet",
        "DCCurrent.maxSet",
        "DCCurrent.target",
        "DCVoltage.minSet",
        "DCVoltage.maxSet",
        "DCVoltage.target",
        "Power.maxSet",
        "EnergyCapacity",
        "EnergyImport.target",
        "DepartureTime",
        "RemainingTimeBulk",
        "RemainingTimeFull.maxSet",
        "RemainingTimeFull.actual",
        "StateOfChargeBulk",
        "StateOfCharge.maxSet",
        "StateOfCharge.actual",
        "ChargingCompleteBulk",
        "ChargingCompleteFull",
        "BatteryOvervoltage",
        "BatteryUndervoltage",
        "ChargingCurrentDeviation",
        "BatteryTemperature",
        "VoltageDeviation",
        "ChargingSystemError",
        "VehicleShiftPosition",
        "VehicleChargingEnabled",
        "ChargingSystemIncompatibility",
        "ChargerConnectorLockFault",
    ]);
}

/// `ChargingStateVariableName` — the 10 standardized `ChargingState` variable
/// names (open recommendation vocabulary, no [`reject`]).
#[test]
fn test_charging_state_variable_name() {
    pin_all::<ChargingStateVariableName>(&[
        "BatteryOvervoltage",
        "BatteryUndervoltage",
        "ChargingCurrentDeviation",
        "BatteryTemperature",
        "VoltageDeviation",
        "ChargingSystemError",
        "VehicleShiftPosition",
        "VehicleChargingEnabled",
        "ChargingSystemIncompatibility",
        "ChargerConnectorLockFault",
    ]);
}

/// `ConnectorVariableName` — the 10 standardized `Connector` variable names
/// (open recommendation vocabulary, no [`reject`]). Drift risk is the
/// parenthesized `SupplyPhases(MaxLimit)`.
#[test]
fn test_connector_variable_name() {
    pin_all::<ConnectorVariableName>(&[
        "AvailabilityState",
        "Available",
        "ChargeProtocol",
        "ConnectorType",
        "Enabled",
        "PhaseRotation",
        "Problem",
        "SupplyPhases",
        "SupplyPhases(MaxLimit)",
        "Tripped",
    ]);
}

/// `ConnectorHolsterReleaseVariableName` — the 4 standardized
/// `ConnectorHolsterRelease` variable names (open recommendation vocabulary, no
/// [`reject`]).
#[test]
fn test_connector_holster_release_variable_name() {
    pin_all::<ConnectorHolsterReleaseVariableName>(&["Enabled", "Active", "Problem", "State"]);
}

/// `ConnectorHolsterSensorVariableName` — the 3 standardized
/// `ConnectorHolsterSensor` variable names (open recommendation vocabulary, no
/// [`reject`]).
#[test]
fn test_connector_holster_sensor_variable_name() {
    pin_all::<ConnectorHolsterSensorVariableName>(&["Enabled", "Active", "Problem"]);
}

/// `ConnectorPlugRetentionLockVariableName` — the 7 standardized
/// `ConnectorPlugRetentionLock` variable names (open recommendation vocabulary,
/// no [`reject`]). Drift risks are the parenthesized `Tries(SetLimit)` /
/// `Tries(MaxLimit)`.
#[test]
fn test_connector_plug_retention_lock_variable_name() {
    pin_all::<ConnectorPlugRetentionLockVariableName>(&[
        "Enabled",
        "Active",
        "Problem",
        "Tripped",
        "Tries",
        "Tries(SetLimit)",
        "Tries(MaxLimit)",
    ]);
}

/// `ConnectorProtectionReleaseVariableName` — the 4 standardized
/// `ConnectorProtectionRelease` variable names (open recommendation vocabulary,
/// no [`reject`]).
#[test]
fn test_connector_protection_release_variable_name() {
    pin_all::<ConnectorProtectionReleaseVariableName>(&["Enabled", "Active", "Problem", "Tripped"]);
}

/// `ControllerVariableName` — the 13 standardized `Controller` variable names
/// (open recommendation vocabulary, no [`reject`]). Drift risks are the
/// bracketed `Interval[Heartbeat]`, the parenthesized `SelftestActive(Set)`,
/// and the `ECVariant` acronym.
#[test]
fn test_controller_variable_name() {
    pin_all::<ControllerVariableName>(&[
        "Active",
        "ECVariant",
        "FirmwareVersion",
        "Interval[Heartbeat]",
        "Manufacturer",
        "MaxMsgElements",
        "Model",
        "Problem",
        "SelftestActive",
        "SelftestActive(Set)",
        "SerialNumber",
        "VersionDate",
        "VersionNumber",
    ]);
}

/// `ControlMeteringVariableName` — the 4 standardized `ControlMetering`
/// variable names (open recommendation vocabulary, no [`reject`]). Drift risks
/// are the `ACCurrent` / `DCCurrent` / `DCVoltage` acronyms.
#[test]
fn test_control_metering_variable_name() {
    pin_all::<ControlMeteringVariableName>(&["Power", "ACCurrent", "DCCurrent", "DCVoltage"]);
}

/// `CPPWMControllerVariableName` — the 8 standardized `CPPWMController` variable
/// names (open recommendation vocabulary, no [`reject`]). Drift risks are the
/// `DCVoltage` acronym and the parenthesized `SelftestActive(Set)`.
#[test]
fn test_cppwm_controller_variable_name() {
    pin_all::<CPPWMControllerVariableName>(&[
        "Active",
        "DCVoltage",
        "Enabled",
        "Percentage",
        "Problem",
        "SelftestActive",
        "SelftestActive(Set)",
        "State",
    ]);
}

// Device-model physical-component variable-name catalogs (#363 slice 3c):
// DataLink → EVSE. Faithful `pin_all` ports of the reference `*VariableName`
// StrEnums; wire strings verified byte-for-byte against `ocpp/v201/enums.py`.

#[test]
fn test_data_link_variable_name() {
    pin_all::<DataLinkVariableName>(&[
        "Active",
        "Complete",
        "Enabled",
        "Fallback",
        "ICCID",
        "IMSI",
        "NetworkAddress",
        "Problem",
        "SignalStrength",
    ]);
}

#[test]
fn test_display_variable_name() {
    pin_all::<DisplayVariableName>(&[
        "Color",
        "Count[HeightInChars]",
        "Count[WidthInChars]",
        "DataText[Visible]",
        "Enabled",
        "Problem",
        "State",
    ]);
}

#[test]
fn test_distribution_panel_variable_name() {
    pin_all::<DistributionPanelVariableName>(&[
        "ChargingStation",
        "DistributionPanel",
        "Fuse",
        "InstanceName",
    ]);
}

#[test]
fn test_electrical_feed_variable_name() {
    pin_all::<ElectricalFeedVariableName>(&[
        "ACVoltage",
        "Active",
        "DCVoltage",
        "Enabled",
        "Energy",
        "PhaseRotation",
        "Power",
        "PowerType",
        "Problem",
        "SupplyPhases",
    ]);
}

#[test]
fn test_elv_supply_variable_name() {
    pin_all::<ELVSupplyVariableName>(&[
        "EnergyImportRegister",
        "Fallback",
        "Fallback(MaxLimit)",
        "Power",
        "Power(MaxLimit)",
        "StateOfCharge",
        "Time",
    ]);
}

#[test]
fn test_emergency_stop_sensor_variable_name() {
    pin_all::<EmergencyStopSensorVariableName>(&["Enabled", "Active", "Tripped"]);
}

#[test]
fn test_environmental_lighting_variable_name() {
    pin_all::<EnvironmentalLightingVariableName>(&[
        "Active",
        "Color",
        "Enabled",
        "Enabled(Set)",
        "Percent",
        "Percent(Set)",
        "Power",
        "Problem",
    ]);
}

#[test]
fn test_ev_retention_lock_variable_name() {
    pin_all::<EVRetentionLockVariableName>(&["Active", "Complete", "Enabled", "Problem"]);
}

#[test]
fn test_evse_variable_name() {
    pin_all::<EVSEVariableName>(&[
        "ACCurrent",
        "ACVoltage",
        "Available",
        "AvailabilityState",
        "AllowReset",
        "ChargeProtocol",
        "ChargingTime",
        "Count[ChargingProfiles](MaxLimit)",
        "Count[ChargingProfiles]",
        "CurrentImbalance",
        "DCCurrent",
        "DCVoltage",
        "Enabled",
        "EvseId",
        "ISO15118EvseId",
        "Overload",
        "PhaseRotation",
        "PostChargingTime",
        "Power",
        "Problem",
        "SupplyPhases",
        "Tripped",
        "VoltageImbalance",
    ]);
}

/// Ports the `StatusInfoReasonType` members (`ocpp/v201/enums.py`, Appendix 5
/// v1.3). Pins all 43 standardized `StatusInfo.reasonCode` values, including
/// the acronym-cased `CSNotAccepted`, `InvalidCSR`, `InvalidURL`,
/// `InvalidMessageSeq`, `UnknownEvse` — a wrong rename would silently break
/// interop with a spec-conformant CSMS reading `reasonCode`.
#[test]
fn test_status_info_reason_type() {
    pin_all::<StatusInfoReasonType>(&[
        "CSNotAccepted",
        "DuplicateProfile",
        "DuplicateRequestId",
        "FixedCable",
        "FwUpdateInProgress",
        "InternalError",
        "InvalidCertificate",
        "InvalidCSR",
        "InvalidIdToken",
        "InvalidMessageSeq",
        "InvalidProfile",
        "InvalidSchedule",
        "InvalidStackLevel",
        "InvalidURL",
        "InvalidValue",
        "MissingDeviceModelInfo",
        "MissingParam",
        "NoCable",
        "NoError",
        "NotEnabled",
        "NotFound",
        "OutOfMemory",
        "OutOfStorage",
        "ReadOnly",
        "TooLargeElement",
        "TooManyElements",
        "TxInProgress",
        "TxNotFound",
        "TxStarted",
        "UnknownConnectorId",
        "UnknownConnectorType",
        "UnknownEvse",
        "UnknownTxId",
        "Unspecified",
        "UnsupportedParam",
        "UnsupportedRateUnit",
        "UnsupportedRequest",
        "ValueOutOfRange",
        "ValuePositiveOnly",
        "ValueTooHigh",
        "ValueTooLow",
        "ValueZeroNotAllowed",
        "WriteOnly",
    ]);
}

/// Ports the `StandardizedUnitsOfMeasureType` members (`ocpp/v201/enums.py`).
/// Pins all 33 allowable `unit` symbols, dominated by lower-/mixed-case
/// spellings (`dB`, `dBm`, `lx`, `m`, `ms2`, `kPa`, `kVA`, `kWh`, `var`,
/// `kvarh`, …) whose `#[serde(rename)]` a typo would silently break.
#[test]
fn test_standardized_units_of_measure_type() {
    pin_all::<StandardizedUnitsOfMeasureType>(&[
        "ASU",
        "B",
        "dB",
        "dBm",
        "Deg",
        "Hz",
        "lx",
        "m",
        "ms2",
        "N",
        "Ohm",
        "kPa",
        "Percent",
        "RH",
        "RPM",
        "s",
        "VA",
        "kVA",
        "VAh",
        "kVAh",
        "var",
        "kvar",
        "varh",
        "kvarh",
        "Wh",
        "kWh",
        "W",
        "kW",
        "A",
        "V",
        "Celsius",
        "Fahrenheit",
        "K",
    ]);
}
