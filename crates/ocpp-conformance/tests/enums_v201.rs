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
//! ## Sweep of the remaining v201 enums (issue #274)
//!
//! Beyond the three reference-pinned enums and the dotted/hyphenated extras
//! below, this suite sweeps the remaining `*EnumType`s in
//! `ocpp-types::v201::enums` so a stray rename in any of them fails a test.
//! Slice 1 covers the **security / certificate / ISO-15118 / network-transport /
//! identity** domain; slice 3 (also in this file) covers the **device-model /
//! monitoring / variables / messaging** domain; the remaining slices cover the
//! command-status/lifecycle (slice 2, #298) and firmware/log (slice 4) domains.
//! Every swept enum's wire strings are verified against both `ocpp/v201/enums.py`
//! and the bundled FINAL `crates/ocpp-messages/schemas/v201/*.json`.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, json, to_value};

use ocpp_types::v201::{
    APNAuthenticationEnumType, AttributeEnumType, AuthorizationStatusEnumType,
    AuthorizeCertificateStatusEnumType, CertificateActionEnumType, CertificateSignedStatusEnumType,
    CertificateSigningUseEnumType, ChargingStateEnumType, ClearMonitoringStatusEnumType,
    ComponentCriterionEnumType, ConnectorEnumType, ConnectorStatusEnumType, DataEnumType,
    DeleteCertificateStatusEnumType, EnergyTransferModeEnumType, EventNotificationEnumType,
    EventTriggerEnumType, GenericDeviceModelStatusEnumType, GetCertificateIdUseEnumType,
    GetCertificateStatusEnumType, GetInstalledCertificateStatusEnumType, GetVariableStatusEnumType,
    HashAlgorithmEnumType, IdTokenEnumType, InstallCertificateStatusEnumType,
    InstallCertificateUseEnumType, Iso15118EVCertificateStatusEnumType, LocationEnumType,
    MeasurandEnumType, MessageFormatEnumType, MessagePriorityEnumType, MessageStateEnumType,
    MessageTriggerEnumType, MonitorBaseEnumType, MonitorEnumType, MonitoringCriterionEnumType,
    MutabilityEnumType, OCPPInterfaceEnumType, OCPPTransportEnumType, OCPPVersionEnumType,
    PhaseEnumType, ReadingContextEnumType, ReasonEnumType, ReportBaseEnumType,
    SetMonitoringStatusEnumType, SetNetworkProfileStatusEnumType, SetVariableStatusEnumType,
    TriggerReasonEnumType, TxStartStopPointEnumType, VPNEnumType,
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
