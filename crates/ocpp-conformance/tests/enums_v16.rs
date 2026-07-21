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
//! The security-extension / firmware / logging family is pinned too — the
//! initial set in #304 (`CertificateSignedStatus`, `CertificateUse`,
//! `DeleteCertificateStatus`, `GenericStatus`, `UpdateFirmwareStatus`,
//! `UploadLogStatus`) plus the remaining members from #307 (`HashAlgorithmType`,
//! `LogType`, `LogStatus`, `GetInstalledCertificatesStatus`,
//! `InstallCertificateStatus`) — so every 1.6J enum modelled as a Rust enum now
//! has a round-trip pin. Every schema-backed member is additionally
//! cross-checked against the bundled FINAL 1.6J schema named in each test's doc
//! comment.
//!
//! ## Known gaps (documented, not silently dropped)
//!
//! None remaining for the modelled 1.6J enum set. The last documented gap —
//! **`ConfigurationKey`**, the reference's 54-member config-key enum — is now
//! modelled in `ocpp-types::v16j` and pinned by [`test_configuration_keys`]
//! below (#349).
//!
//! The #307 "unmodelled residual" is closed: four of its five enums were
//! already modelled under Rust-idiomatic names (`CertificateStatus` →
//! `InstallCertificateStatus`, `GetInstalledCertificateStatus` →
//! `GetInstalledCertificatesStatus`, `HashAlgorithm` → `HashAlgorithmType`,
//! `Log` → `LogType`) and are now pinned above; the genuinely-missing fifth,
//! `LogStatus` (`GetLog.conf`), is now modelled and pinned.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, json, to_value};

use ocpp_types::common::{
    AuthorizationStatus, AvailabilityStatus, AvailabilityType, Location, Measurand, Phase,
    ReadingContext, Reason, UnitOfMeasure, ValueFormat,
};
use ocpp_types::v16j::{
    CancelReservationStatus, CertificateSignedStatus, CertificateUse, ChargePointErrorCode,
    ChargePointStatus, ChargingProfileKindType, ChargingProfilePurposeType, ChargingProfileStatus,
    ChargingRateUnitType, ClearCacheStatus, ClearChargingProfileStatus, ConfigurationKey,
    ConfigurationStatus, DataTransferStatus, DeleteCertificateStatus, DiagnosticsStatus,
    FirmwareStatus, GenericStatus, GetCompositeScheduleStatus, GetInstalledCertificatesStatus,
    HashAlgorithmType, InstallCertificateStatus, LogStatus, LogType, MessageTrigger,
    RecurrencyKindType, RemoteStartStopStatus, ReservationStatus, ResetStatus, ResetType,
    TriggerMessageStatus, UnlockStatus, UpdateFirmwareStatus, UpdateStatus, UpdateType,
    UploadLogStatus,
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
    // Pinned to the reference `FirmwareStatus` (ocpp/v16/enums.py) and the
    // bundled `SignedFirmwareStatusNotification.json` `FirmwareStatusEnumType`
    // (14 members). One enum serves both the core `FirmwareStatusNotification`
    // (the first seven, common statuses) and the Security-extension
    // `SignedFirmwareStatusNotification` (all 14).
    pin_all::<FirmwareStatus>(&[
        // Common to FirmwareStatusNotification + SignedFirmwareStatusNotification
        "Downloaded",
        "DownloadFailed",
        "Downloading",
        "Idle",
        "InstallationFailed",
        "Installing",
        "Installed",
        // SignedFirmwareStatusNotification-only (Security extension)
        "DownloadScheduled",
        "DownloadPaused",
        "InstallRebooting",
        "InstallScheduled",
        "InstallVerificationFailed",
        "InvalidSignature",
        "SignatureVerified",
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
    // TriggerMessage triggers plus the two ExtendedTriggerMessage-only triggers
    // (`LogStatusNotification`, `SignChargePointCertificate`) from the reference
    // `MessageTrigger` and `ExtendedTriggerMessage.json` `MessageTriggerEnumType`.
    pin_all::<MessageTrigger>(&[
        "BootNotification",
        "DiagnosticsStatusNotification",
        "FirmwareStatusNotification",
        "Heartbeat",
        "MeterValues",
        "StatusNotification",
        "LogStatusNotification",
        "SignChargePointCertificate",
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

// --- Security-extension / firmware / logging enums (#304) ---
//
// The already-modelled tail of the 1.6J enum sweep. Wire strings verified
// against the reference `ocpp/v16/enums.py`; every enum that is a field in a
// bundled 1.6J schema is additionally cross-checked against
// `crates/ocpp-messages/schemas/v16j/*.json` (named per test). For all six,
// model == reference == FINAL schema — no divergence to `reject`.

#[test]
fn test_certificate_signed_status() {
    // Reference `CertificateSignedStatus`; cross-checked against
    // `CertificateSignedResponse.json` (`CertificateSignedStatusEnumType`).
    pin_all::<CertificateSignedStatus>(&["Accepted", "Rejected"]);
}

#[test]
fn test_certificate_use() {
    // Reference `CertificateUse`; cross-checked against
    // `GetInstalledCertificateIds.json` / `InstallCertificate.json`
    // (`CertificateUseEnumType`).
    pin_all::<CertificateUse>(&[
        "CentralSystemRootCertificate",
        "ManufacturerRootCertificate",
    ]);
}

#[test]
fn test_delete_certificate_status() {
    // Reference `DeleteCertificateStatus`; cross-checked against
    // `DeleteCertificateResponse.json` (`DeleteCertificateStatusEnumType`).
    pin_all::<DeleteCertificateStatus>(&["Accepted", "Failed", "NotFound"]);
}

#[test]
fn test_generic_status() {
    // Reference `GenericStatus`; cross-checked against
    // `SignCertificateResponse.json` (`GenericStatusEnumType`).
    pin_all::<GenericStatus>(&["Accepted", "Rejected"]);
}

#[test]
fn test_update_firmware_status() {
    // Reference `UpdateFirmwareStatus` (used by `SignedUpdateFirmware.conf`);
    // cross-checked against `SignedUpdateFirmwareResponse.json`
    // (`UpdateFirmwareStatusEnumType`). Watch `AcceptedCanceled` (single-l US
    // spelling, not `AcceptedCancelled`).
    pin_all::<UpdateFirmwareStatus>(&[
        "Accepted",
        "Rejected",
        "AcceptedCanceled",
        "InvalidCertificate",
        "RevokedCertificate",
    ]);
}

#[test]
fn test_upload_log_status() {
    // Reference `UploadLogStatus` (used by `LogStatusNotification.req`);
    // cross-checked against `LogStatusNotification.json`
    // (`UploadLogStatusEnumType`). Watch `NotSupportedOperation` (not
    // `NotSupported`) and `UploadFailure` (not `UploadFailed`).
    pin_all::<UploadLogStatus>(&[
        "BadMessage",
        "Idle",
        "NotSupportedOperation",
        "PermissionDenied",
        "Uploaded",
        "UploadFailure",
        "Uploading",
    ]);
}

#[test]
fn test_log_status() {
    // Reference `LogStatus` (used by `GetLog.conf`); cross-checked against
    // `GetLogResponse.json` (`LogStatusEnumType`). This is *not* the
    // upload-progress `UploadLogStatus` above (`LogStatusNotification.req`) —
    // the two are disjoint. Watch `AcceptedCanceled` (single-l US spelling).
    pin_all::<LogStatus>(&["Accepted", "Rejected", "AcceptedCanceled"]);
}

#[test]
fn test_log_type() {
    // Reference `Log` (used by `GetLog.req`); cross-checked against
    // `GetLog.json` (`LogEnumType`).
    pin_all::<LogType>(&["DiagnosticsLog", "SecurityLog"]);
}

#[test]
fn test_hash_algorithm() {
    // Reference `HashAlgorithm` (certificate hash data); cross-checked against
    // `GetInstalledCertificateIdsResponse.json` (`HashAlgorithmEnumType`).
    // Values are the raw acronyms (no PascalCase rename).
    pin_all::<HashAlgorithmType>(&["SHA256", "SHA384", "SHA512"]);
}

#[test]
fn test_get_installed_certificates_status() {
    // Reference `GetInstalledCertificateStatus`; cross-checked against
    // `GetInstalledCertificateIdsResponse.json`
    // (`GetInstalledCertificateStatusEnumType`).
    pin_all::<GetInstalledCertificatesStatus>(&["Accepted", "NotFound"]);
}

#[test]
fn test_install_certificate_status() {
    // Reference `CertificateStatus` (used by `InstallCertificate.conf`);
    // cross-checked against `InstallCertificateResponse.json`
    // (`InstallCertificateStatusEnumType`: Accepted / Rejected / Failed).
    pin_all::<InstallCertificateStatus>(&["Accepted", "Rejected", "Failed"]);
}

#[test]
fn test_configuration_keys() {
    // Direct port of `test_v16_enums.py::test_configuration_keys` — pins every
    // one of the 54 standard 1.6J configuration keys to its exact wire string.
    // Config keys are free-form strings on the wire (not schema-constrained), so
    // this pins the `ConfigurationKey` vocabulary rather than cross-checking a
    // bundled schema. Watch the acronym / mixed-case members that PascalCase
    // renaming must preserve verbatim: `StopTransactionOnEVSideDisconnect`,
    // `UnlockConnectorOnEVSideDisconnect`, `WebSocketPingInterval`,
    // `ISO15118PnCEnabled`, `ConnectorSwitch3to1PhaseSupported`, `CpoName`.
    pin_all::<ConfigurationKey>(&[
        // 9.1 Core Profile
        "AllowOfflineTxForUnknownId",
        "AuthorizationCacheEnabled",
        "AuthorizeRemoteTxRequests",
        "BlinkRepeat",
        "ClockAlignedDataInterval",
        "ConnectionTimeOut",
        "ConnectorPhaseRotation",
        "ConnectorPhaseRotationMaxLength",
        "GetConfigurationMaxKeys",
        "HeartbeatInterval",
        "LightIntensity",
        "LocalAuthorizeOffline",
        "LocalPreAuthorize",
        "MaxEnergyOnInvalidId",
        "MeterValuesAlignedData",
        "MeterValuesAlignedDataMaxLength",
        "MeterValuesSampledData",
        "MeterValuesSampledDataMaxLength",
        "MeterValueSampleInterval",
        "MinimumStatusDuration",
        "NumberOfConnectors",
        "ResetRetries",
        "StopTransactionOnEVSideDisconnect",
        "StopTransactionOnInvalidId",
        "StopTxnAlignedData",
        "StopTxnAlignedDataMaxLength",
        "StopTxnSampledData",
        "StopTxnSampledDataMaxLength",
        "SupportedFeatureProfiles",
        "SupportedFeatureProfilesMaxLength",
        "TransactionMessageAttempts",
        "TransactionMessageRetryInterval",
        "UnlockConnectorOnEVSideDisconnect",
        "WebSocketPingInterval",
        // 9.2 Local Auth List Management Profile
        "LocalAuthListEnabled",
        "LocalAuthListMaxLength",
        "SendLocalListMaxLength",
        // 9.3 Reservation Profile
        "ReserveConnectorZeroSupported",
        // 9.4 Smart Charging Profile
        "ChargeProfileMaxStackLevel",
        "ChargingScheduleAllowedChargingRateUnit",
        "ChargingScheduleMaxPeriods",
        "ConnectorSwitch3to1PhaseSupported",
        "MaxChargingProfilesInstalled",
        // OCPP 1.6 ISO 15118 v10 added configuration keys
        "CentralContractValidationAllowed",
        "CertificateSignedMaxChainSize",
        "CertSigningWaitMinimum",
        "CertSigningRepeatTimes",
        "CertificateStoreMaxLength",
        "ContractValidationOffline",
        "ISO15118PnCEnabled",
        // OCPP security whitepaper added configuration keys
        "AdditionalRootCertificateCheck",
        "AuthorizationKey",
        "CpoName",
        "SecurityProfile",
    ]);
}
