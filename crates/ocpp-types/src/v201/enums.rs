//! OCPP 2.0.1 enumerations.
//!
//! Ports the `*EnumType` definitions from the OCPP 2.0.1 specification
//! (mobilityhouse/ocpp `ocpp/v201/enums.py`). Wire values are the verbatim
//! spec strings; variants whose spec spelling is not an idiomatic Rust
//! identifier (acronyms, `eMAID`, …) carry an explicit `#[serde(rename)]`.
//!
//! Re-exported from [`super`] so the public path stays `ocpp_types::v201::*`.

use serde::{Deserialize, Serialize};

/// Reason the Charging Station sends a `BootNotification` to the CSMS.
///
/// Ports `BootReasonEnumType` (`ocpp/v201/enums.py`). Wire values are
/// PascalCase, e.g. `"PowerUp"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootReasonEnumType {
    ApplicationReset,
    FirmwareUpdate,
    LocalReset,
    PowerUp,
    RemoteReset,
    ScheduledReset,
    Triggered,
    Unknown,
    Watchdog,
}

/// Result of a registration in response to a `BootNotification`.
///
/// Ports `RegistrationStatusEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistrationStatusEnumType {
    Accepted,
    Pending,
    Rejected,
}

/// Current status of a connector, reported in a `StatusNotification`.
///
/// Ports `ConnectorStatusEnumType` (`ocpp/v201/enums.py`). The 2.0.1 set is the
/// schema's five values — `Available`, `Occupied`, `Reserved`, `Unavailable`,
/// `Faulted` — a different (smaller) vocabulary than the 1.6J
/// `ChargePointStatus`. Wire values are PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorStatusEnumType {
    Available,
    Occupied,
    Reserved,
    Unavailable,
    Faulted,
}

/// Enumeration of possible `idToken` types.
///
/// Ports `IdTokenEnumType` (`ocpp/v201/enums.py`). Wire values are the verbatim
/// spec strings; several are not idiomatic Rust identifiers (`eMAID`, the
/// `ISO*` acronyms), so those variants carry an explicit `#[serde(rename)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdTokenEnumType {
    Central,
    #[serde(rename = "eMAID")]
    EMaid,
    #[serde(rename = "ISO14443")]
    Iso14443,
    #[serde(rename = "ISO15693")]
    Iso15693,
    KeyCode,
    Local,
    MacAddress,
    NoAuthorization,
}

/// Current authorization status of an `idToken`.
///
/// Ports `AuthorizationStatusEnumType` (`ocpp/v201/enums.py`). A richer set than
/// the 1.6J `AuthorizationStatus` (which has only `Accepted`/`Blocked`/
/// `Expired`/`Invalid`/`ConcurrentTx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationStatusEnumType {
    Accepted,
    Blocked,
    ConcurrentTx,
    Expired,
    Invalid,
    NoCredit,
    #[serde(rename = "NotAllowedTypeEVSE")]
    NotAllowedTypeEvse,
    NotAtThisLocation,
    NotAtThisTime,
    Unknown,
}

/// Format of a message to be displayed on a Charging Station.
///
/// Ports `MessageFormatEnumType` (`ocpp/v201/enums.py`). All four wire values
/// are all-caps acronyms, so each variant is renamed from its idiomatic Rust
/// spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageFormatEnumType {
    #[serde(rename = "ASCII")]
    Ascii,
    #[serde(rename = "HTML")]
    Html,
    #[serde(rename = "URI")]
    Uri,
    #[serde(rename = "UTF8")]
    Utf8,
}

/// Which attribute of a variable a request reads or a result reports.
///
/// Ports `AttributeEnumType` (`ocpp/v201/enums.py`). When omitted on the wire
/// the 2.0.1 default is `Actual`. Wire values are PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeEnumType {
    Actual,
    Target,
    MinSet,
    MaxSet,
}

/// Result of reading a single component-variable attribute.
///
/// Ports `GetVariableStatusEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GetVariableStatusEnumType {
    Accepted,
    Rejected,
    UnknownComponent,
    UnknownVariable,
    NotSupportedAttributeType,
}

/// Result of writing a single component-variable attribute.
///
/// Ports `SetVariableStatusEnumType` (`ocpp/v201/enums.py`). The write-path
/// counterpart to [`GetVariableStatusEnumType`]: the same statuses plus
/// `RebootRequired` (the value was accepted but only takes effect after a
/// reboot). Wire values are PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetVariableStatusEnumType {
    Accepted,
    Rejected,
    UnknownComponent,
    UnknownVariable,
    NotSupportedAttributeType,
    RebootRequired,
}

/// Type of a `TransactionEvent` message.
///
/// Ports `TransactionEventEnumType` (`ocpp/v201/enums.py`). A transaction is a
/// sequence of one `Started`, zero or more `Updated`, and one `Ended` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionEventEnumType {
    Ended,
    Started,
    Updated,
}

/// Reason that triggered a `TransactionEvent`.
///
/// Ports `TriggerReasonEnumType` (`ocpp/v201/enums.py`). Wire values are
/// PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerReasonEnumType {
    Authorized,
    CablePluggedIn,
    ChargingRateChanged,
    ChargingStateChanged,
    Deauthorized,
    EnergyLimitReached,
    EVCommunicationLost,
    EVConnectTimeout,
    MeterValueClock,
    MeterValuePeriodic,
    TimeLimitReached,
    Trigger,
    UnlockCommand,
    StopAuthorized,
    EVDeparted,
    EVDetected,
    RemoteStop,
    RemoteStart,
    AbnormalCondition,
    SignedDataReceived,
    ResetCommand,
}

/// Current charging state of an EVSE during a transaction.
///
/// Ports `ChargingStateEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingStateEnumType {
    Charging,
    EVConnected,
    SuspendedEV,
    SuspendedEVSE,
    Idle,
}

/// Reason a transaction was stopped, reported on the `Ended` event.
///
/// Ports `ReasonEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasonEnumType {
    DeAuthorized,
    EmergencyStop,
    EnergyLimitReached,
    EVDisconnected,
    GroundFault,
    ImmediateReset,
    Local,
    LocalOutOfCredit,
    MasterPass,
    Other,
    OvercurrentFault,
    PowerLoss,
    PowerQuality,
    Reboot,
    Remote,
    SOCLimitReached,
    StoppedByEV,
    TimeLimitReached,
    Timeout,
}

/// Type of reset requested by `Reset.req`.
///
/// Ports `ResetEnumType` (`ocpp/v201/enums.py`). Wire values are PascalCase
/// (`"Immediate"`, `"OnIdle"`). The 2.0.1 replacement for the 1.6J
/// `Hard`/`Soft` distinction: `Immediate` resets at once, `OnIdle` defers the
/// reset until any ongoing transaction has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetEnumType {
    Immediate,
    OnIdle,
}

/// Result of a `Reset.req`, reported in `Reset.conf`.
///
/// Ports `ResetStatusEnumType` (`ocpp/v201/enums.py`). `Scheduled` is returned
/// when the Charging Station accepts the reset but will defer it (e.g. an
/// `OnIdle` reset while a transaction is in progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetStatusEnumType {
    Accepted,
    Rejected,
    Scheduled,
}

/// Whether a Charging Station executed a `ClearCache.req` and wiped its local
/// authorization cache.
///
/// Ports `ClearCacheStatusEnumType` (`ocpp/v201/enums.py`). `Accepted` if the
/// Charging Station executed the request, otherwise `Rejected`. Wire values are
/// PascalCase (`"Accepted"`, `"Rejected"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearCacheStatusEnumType {
    Accepted,
    Rejected,
}

/// The availability change a `ChangeAvailability.req` asks the Charging Station
/// (or a single EVSE) to perform.
///
/// Ports `OperationalStatusEnumType` (`ocpp/v201/enums.py`). Wire values are
/// PascalCase (`"Inoperative"`, `"Operative"`); any other value is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationalStatusEnumType {
    Inoperative,
    Operative,
}

/// Whether the Charging Station can perform the availability change requested
/// in `ChangeAvailability.req`, reported in `ChangeAvailability.conf`.
///
/// Ports `ChangeAvailabilityStatusEnumType` (`ocpp/v201/enums.py`). `Scheduled`
/// is returned when the station accepts the change but defers it (e.g. until an
/// in-progress transaction ends). Note: the reference *dataclass* enum lists
/// only `Accepted`/`Rejected`, but the bundled OCPP 2.0.1 FINAL JSON Schema —
/// the authority for the wire — also defines `Scheduled`, so it is included
/// here. Wire values are PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeAvailabilityStatusEnumType {
    Accepted,
    Rejected,
    Scheduled,
}

/// Whether a Charging Station accepts a request to remotely start or stop a
/// transaction.
///
/// Ports `RequestStartStopStatusEnumType` (`ocpp/v201/enums.py`). Shared by the
/// `RequestStartTransaction` and `RequestStopTransaction` command replies; the
/// 2.0.1 successor to the 1.6J `RemoteStartStopStatus`. Wire values are
/// PascalCase (`"Accepted"`, `"Rejected"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestStartStopStatusEnumType {
    Accepted,
    Rejected,
}

/// Whether a Charging Station managed to unlock a connector in response to an
/// `UnlockConnector.req`.
///
/// Ports `UnlockStatusEnumType` (`ocpp/v201/enums.py`). `OngoingAuthorizedTransaction`
/// means the connector is in use by an authorized transaction and so cannot be
/// unlocked; `UnknownConnector` means the requested EVSE/connector does not
/// exist. Wire values are PascalCase, identical to the bundled OCPP 2.0.1 FINAL
/// JSON Schema's `enum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnlockStatusEnumType {
    Unlocked,
    UnlockFailed,
    OngoingAuthorizedTransaction,
    UnknownConnector,
}

/// Hash algorithm used for the OCSP request data in the ISO 15118
/// plug-and-charge certificate path.
///
/// Ports `HashAlgorithmEnumType` (`ocpp/v201/enums.py`). All wire values are
/// all-caps acronyms, so each variant is renamed from its idiomatic Rust
/// spelling. Used by [`super::OCSPRequestDataType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashAlgorithmEnumType {
    #[serde(rename = "SHA256")]
    Sha256,
    #[serde(rename = "SHA384")]
    Sha384,
    #[serde(rename = "SHA512")]
    Sha512,
}

/// Outcome of validating the ISO 15118 contract certificate presented in an
/// `Authorize` request, returned in the `AuthorizeResponse`.
///
/// Ports `AuthorizeCertificateStatusEnumType` (`ocpp/v201/enums.py`). Wire
/// values are PascalCase. `Accepted` means the certificate is valid; every
/// other value is a distinct rejection reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizeCertificateStatusEnumType {
    Accepted,
    SignatureError,
    CertificateExpired,
    CertificateRevoked,
    NoCertificateAvailable,
    CertChainError,
    ContractCancelled,
}

/// Type of detail value carried in a [`super::SampledValueType`]: where in a
/// transaction's lifecycle, or under what circumstances, the value was read.
///
/// Ports `ReadingContextEnumType` (`ocpp/v201/enums.py`). Several wire values
/// are dotted (`"Sample.Periodic"`, the spec default) and so carry an explicit
/// `#[serde(rename)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadingContextEnumType {
    #[serde(rename = "Interruption.Begin")]
    InterruptionBegin,
    #[serde(rename = "Interruption.End")]
    InterruptionEnd,
    Other,
    #[serde(rename = "Sample.Clock")]
    SampleClock,
    #[serde(rename = "Sample.Periodic")]
    SamplePeriodic,
    #[serde(rename = "Transaction.Begin")]
    TransactionBegin,
    #[serde(rename = "Transaction.End")]
    TransactionEnd,
    Trigger,
}

/// The kind of measurement a [`super::SampledValueType`] reports.
///
/// Ports `MeasurandEnumType` (`ocpp/v201/enums.py`). The default when the field
/// is absent is `Energy.Active.Import.Register`. Most wire values are dotted
/// and carry an explicit `#[serde(rename)]`; `Frequency`, `Voltage` and `SoC`
/// match their Rust spelling directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeasurandEnumType {
    #[serde(rename = "Current.Export")]
    CurrentExport,
    #[serde(rename = "Current.Import")]
    CurrentImport,
    #[serde(rename = "Current.Offered")]
    CurrentOffered,
    #[serde(rename = "Energy.Active.Export.Register")]
    EnergyActiveExportRegister,
    #[serde(rename = "Energy.Active.Import.Register")]
    EnergyActiveImportRegister,
    #[serde(rename = "Energy.Reactive.Export.Register")]
    EnergyReactiveExportRegister,
    #[serde(rename = "Energy.Reactive.Import.Register")]
    EnergyReactiveImportRegister,
    #[serde(rename = "Energy.Active.Export.Interval")]
    EnergyActiveExportInterval,
    #[serde(rename = "Energy.Active.Import.Interval")]
    EnergyActiveImportInterval,
    #[serde(rename = "Energy.Active.Net")]
    EnergyActiveNet,
    #[serde(rename = "Energy.Reactive.Export.Interval")]
    EnergyReactiveExportInterval,
    #[serde(rename = "Energy.Reactive.Import.Interval")]
    EnergyReactiveImportInterval,
    #[serde(rename = "Energy.Reactive.Net")]
    EnergyReactiveNet,
    #[serde(rename = "Energy.Apparent.Net")]
    EnergyApparentNet,
    #[serde(rename = "Energy.Apparent.Import")]
    EnergyApparentImport,
    #[serde(rename = "Energy.Apparent.Export")]
    EnergyApparentExport,
    Frequency,
    #[serde(rename = "Power.Active.Export")]
    PowerActiveExport,
    #[serde(rename = "Power.Active.Import")]
    PowerActiveImport,
    #[serde(rename = "Power.Factor")]
    PowerFactor,
    #[serde(rename = "Power.Offered")]
    PowerOffered,
    #[serde(rename = "Power.Reactive.Export")]
    PowerReactiveExport,
    #[serde(rename = "Power.Reactive.Import")]
    PowerReactiveImport,
    SoC,
    Voltage,
}

/// Electrical phase a [`super::SampledValueType`] applies to; absent means the
/// measured value is an overall value.
///
/// Ports `PhaseEnumType` (`ocpp/v201/enums.py`). The hyphenated phase-to-phase
/// and phase-to-neutral values (`"L1-N"`, `"L1-L2"`, …) carry an explicit
/// `#[serde(rename)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseEnumType {
    L1,
    L2,
    L3,
    N,
    #[serde(rename = "L1-N")]
    L1N,
    #[serde(rename = "L2-N")]
    L2N,
    #[serde(rename = "L3-N")]
    L3N,
    #[serde(rename = "L1-L2")]
    L1L2,
    #[serde(rename = "L2-L3")]
    L2L3,
    #[serde(rename = "L3-L1")]
    L3L1,
}

/// Where a [`super::SampledValueType`] was sampled; the spec default is
/// `Outlet`.
///
/// Ports `LocationEnumType` (`ocpp/v201/enums.py`). All wire values are
/// PascalCase except `EV`, which carries an explicit `#[serde(rename)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationEnumType {
    Body,
    Cable,
    #[serde(rename = "EV")]
    Ev,
    Inlet,
    Outlet,
}

/// Outcome of a [`super::DataTransferType`]-style vendor exchange: whether the
/// receiver accepted the `DataTransfer`, and if not, why.
///
/// Ports `DataTransferStatusEnumType` (`ocpp/v201/enums.py`). `Accepted` /
/// `Rejected` mirror the generic outcome; `UnknownMessageId` and
/// `UnknownVendorId` let the receiver signal *which* part of the request it
/// did not recognise. Wire values are PascalCase, verbatim variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataTransferStatusEnumType {
    Accepted,
    Rejected,
    UnknownMessageId,
    UnknownVendorId,
}
