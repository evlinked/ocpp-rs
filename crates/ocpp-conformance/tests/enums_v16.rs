//! OCPP 1.6J enum wire-string conformance suite.
//!
//! A faithful port of the mobilityhouse/ocpp reference's
//! [`tests/v16/test_v16_enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v16/test_v16_enums.py),
//! which pins every 1.6J enum member to its exact on-the-wire string
//! (e.g. `ChargePointStatus.suspended_evse == "SuspendedEVSE"`,
//! `Measurand.soc == "SoC"`, `Phase.l1_n == "L1-N"`, `UnitOfMeasure.kwh == "kWh"`).
//!
//! On the Rust side these enums live in `ocpp-types` (`common`, `v16j`) and
//! `ocpp-messages` (`v16j::RegistrationStatus`). Their wire format is produced
//! by `#[serde(rename_all = "PascalCase")]` plus per-variant
//! `#[serde(rename = "…")]` for the dotted/acronym cases. Almost none of those
//! strings were asserted before this suite: the in-crate unit tests round-trip
//! *values* through serde without pinning the *string*, so an acronym/dotted
//! rename could silently drift (`SuspendedEVSE` → `SuspendedEvse`,
//! `Energy.Active.Import.Register` → `EnergyActiveImportRegister`) and break
//! interop with a spec-conformant CSMS/CP undetected.
//!
//! Each test mirrors one `test_v16_enums.py` function and pins the reference
//! wire strings via a **deserialize → re-serialize round-trip** keyed on the
//! wire string. This catches both a *missing* variant (deserialize fails) and
//! *rename drift* (re-serialize differs), without hand-naming Rust variants.
//!
//! ## Known gaps (documented, not silently dropped)
//!
//! - **`ConfigurationKey`** — the reference's ~50-member config-key enum is not
//!   modelled as a Rust enum (config keys are handled as plain strings), so it
//!   has no enum round-trip to port. Tracked for a follow-up.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, json, to_value};

use ocpp_types::common::{
    AuthorizationStatus, AvailabilityStatus, AvailabilityType, Location, Measurand, Phase,
    ReadingContext, Reason, UnitOfMeasure, ValueFormat,
};
use ocpp_types::v16j::{
    CancelReservationStatus, ChargePointErrorCode, ChargePointStatus, ChargingProfileKindType,
    ChargingProfilePurposeType, ChargingProfileStatus, ChargingRateUnitType, ClearCacheStatus,
    ClearChargingProfileStatus, ConfigurationStatus, DataTransferStatus, DiagnosticsStatus,
    FirmwareStatus, GetCompositeScheduleStatus, MessageTrigger, RecurrencyKindType,
    RemoteStartStopStatus, ReservationStatus, ResetStatus, ResetType, TriggerMessageStatus,
    UnlockStatus, UpdateStatus, UpdateType,
};

use ocpp_messages::v16j::RegistrationStatus;

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

fn type_name<T>() -> &'static str {
    std::any::type_name::<T>()
}

#[test]
fn test_authorization_status() {
    pin_all::<AuthorizationStatus>(&["Accepted", "Blocked", "Expired", "Invalid", "ConcurrentTx"]);
}

#[test]
fn test_availability_status() {
    pin_all::<AvailabilityStatus>(&["Accepted", "Rejected", "Scheduled"]);
}

#[test]
fn test_availability_type() {
    pin_all::<AvailabilityType>(&["Inoperative", "Operative"]);
}

#[test]
fn test_cancel_reservation_status() {
    pin_all::<CancelReservationStatus>(&["Accepted", "Rejected"]);
}

#[test]
fn test_charge_point_error_code() {
    pin_all::<ChargePointErrorCode>(&[
        "ConnectorLockFailure",
        "EVCommunicationError",
        "GroundFailure",
        "HighTemperature",
        "InternalError",
        "LocalListConflict",
        "NoError",
        "OtherError",
        "OverCurrentFailure",
        "OverVoltage",
        "PowerMeterFailure",
        "PowerSwitchFailure",
        "ReaderFailure",
        "ResetFailure",
        "UnderVoltage",
        "WeakSignal",
    ]);
}

#[test]
fn test_charge_point_status() {
    pin_all::<ChargePointStatus>(&[
        "Available",
        "Preparing",
        "Charging",
        "SuspendedEVSE",
        "SuspendedEV",
        "Finishing",
        "Reserved",
        "Unavailable",
        "Faulted",
    ]);
}

#[test]
fn test_charging_profile_kind_type() {
    pin_all::<ChargingProfileKindType>(&["Absolute", "Recurring", "Relative"]);
}

#[test]
fn test_charging_profile_purpose_type() {
    pin_all::<ChargingProfilePurposeType>(&[
        "ChargePointMaxProfile",
        "TxDefaultProfile",
        "TxProfile",
    ]);
}

#[test]
fn test_charging_profile_status() {
    pin_all::<ChargingProfileStatus>(&["Accepted", "Rejected", "NotSupported"]);
}

#[test]
fn test_charging_rate_unit() {
    pin_all::<ChargingRateUnitType>(&["W", "A"]);
}

#[test]
fn test_clear_cache_status() {
    pin_all::<ClearCacheStatus>(&["Accepted", "Rejected"]);
}

#[test]
fn test_clear_charging_profile_status() {
    pin_all::<ClearChargingProfileStatus>(&["Accepted", "Unknown"]);
}

#[test]
fn test_configuration_status() {
    pin_all::<ConfigurationStatus>(&["Accepted", "Rejected", "RebootRequired", "NotSupported"]);
}

#[test]
fn test_data_transfer_status() {
    pin_all::<DataTransferStatus>(&[
        "Accepted",
        "Rejected",
        "UnknownMessageId",
        "UnknownVendorId",
    ]);
}

#[test]
fn test_diagnostics_status() {
    pin_all::<DiagnosticsStatus>(&["Idle", "Uploaded", "UploadFailed", "Uploading"]);
}

#[test]
fn test_firmware_status() {
    // The reference pins these seven core 1.6 statuses. Rust's `FirmwareStatus`
    // additionally models the Security-extension errata variants
    // (`SignatureError`, `CertificateExpired`, …); a superset is allowed —
    // this suite only asserts the reference members are present and correct.
    pin_all::<FirmwareStatus>(&[
        "Downloaded",
        "DownloadFailed",
        "Downloading",
        "Idle",
        "InstallationFailed",
        "Installing",
        "Installed",
    ]);
}

#[test]
fn test_get_composite_schedule_status() {
    pin_all::<GetCompositeScheduleStatus>(&["Accepted", "Rejected"]);
}

#[test]
fn test_location() {
    pin_all::<Location>(&["Inlet", "Outlet", "Body", "Cable", "EV"]);
}

#[test]
fn test_measurand() {
    pin_all::<Measurand>(&[
        "Energy.Active.Export.Register",
        "Energy.Active.Import.Register",
        "Energy.Reactive.Export.Register",
        "Energy.Reactive.Import.Register",
        "Energy.Active.Export.Interval",
        "Energy.Active.Import.Interval",
        "Energy.Reactive.Export.Interval",
        "Energy.Reactive.Import.Interval",
        "Frequency",
        "Power.Active.Export",
        "Power.Active.Import",
        "Power.Factor",
        "Power.Offered",
        "Power.Reactive.Export",
        "Power.Reactive.Import",
        "Current.Export",
        "Current.Import",
        "Current.Offered",
        "RPM",
        "SoC",
        "Voltage",
        "Temperature",
    ]);
}

#[test]
fn test_message_trigger() {
    pin_all::<MessageTrigger>(&[
        "BootNotification",
        "DiagnosticsStatusNotification",
        "FirmwareStatusNotification",
        "Heartbeat",
        "MeterValues",
        "StatusNotification",
    ]);
}

#[test]
fn test_phase() {
    pin_all::<Phase>(&[
        "L1", "L2", "L3", "N", "L1-N", "L2-N", "L3-N", "L1-L2", "L2-L3", "L3-L1",
    ]);
}

#[test]
fn test_reading_context() {
    pin_all::<ReadingContext>(&[
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

#[test]
fn test_reason() {
    pin_all::<Reason>(&[
        "EmergencyStop",
        "EVDisconnected",
        "HardReset",
        "Local",
        "Other",
        "PowerLoss",
        "Reboot",
        "Remote",
        "SoftReset",
        "UnlockCommand",
        "DeAuthorized",
    ]);
}

#[test]
fn test_recurrency_kind() {
    pin_all::<RecurrencyKindType>(&["Daily", "Weekly"]);
}

#[test]
fn test_registration_status() {
    pin_all::<RegistrationStatus>(&["Accepted", "Pending", "Rejected"]);
}

#[test]
fn test_remote_start_stop_status() {
    pin_all::<RemoteStartStopStatus>(&["Accepted", "Rejected"]);
}

#[test]
fn test_reservation_status() {
    pin_all::<ReservationStatus>(&["Accepted", "Faulted", "Occupied", "Rejected", "Unavailable"]);
}

#[test]
fn test_reset_status() {
    pin_all::<ResetStatus>(&["Accepted", "Rejected"]);
}

#[test]
fn test_reset_type() {
    pin_all::<ResetType>(&["Hard", "Soft"]);
}

#[test]
fn test_trigger_message_status() {
    pin_all::<TriggerMessageStatus>(&["Accepted", "Rejected", "NotImplemented"]);
}

#[test]
fn test_unit_of_measure() {
    pin_all::<UnitOfMeasure>(&[
        "Wh",
        "kWh",
        "varh",
        "kvarh",
        "W",
        "kW",
        "VA",
        "kVA",
        "var",
        "kvar",
        "A",
        "V",
        "Celsius",
        "Fahrenheit",
        "K",
        "Percent",
    ]);
}

#[test]
fn test_unlock_status() {
    pin_all::<UnlockStatus>(&["Unlocked", "UnlockFailed", "NotSupported"]);
}

#[test]
fn test_update_status() {
    pin_all::<UpdateStatus>(&["Accepted", "Failed", "NotSupported", "VersionMismatch"]);
}

#[test]
fn test_update_type() {
    pin_all::<UpdateType>(&["Differential", "Full"]);
}

#[test]
fn test_value_format() {
    pin_all::<ValueFormat>(&["Raw", "SignedData"]);
}
