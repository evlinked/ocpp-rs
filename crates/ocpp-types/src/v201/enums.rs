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

/// Mutability of a variable attribute, reported by a `NotifyReport`'s
/// [`VariableAttributeType`](crate::v201::VariableAttributeType).
///
/// Ports `MutabilityEnumType` (`ocpp/v201/enums.py`). When omitted on the wire
/// the 2.0.1 default is `ReadWrite`. Wire values are PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutabilityEnumType {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

/// Data type of a variable, reported by a `NotifyReport`'s
/// [`VariableCharacteristicsType`](crate::v201::VariableCharacteristicsType).
///
/// Ports `DataEnumType` (`ocpp/v201/enums.py`). The scalar members carry
/// lower-case wire spellings (`string` / `decimal` / `integer` / `dateTime` /
/// `boolean`); the list members are PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataEnumType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "decimal")]
    Decimal,
    #[serde(rename = "integer")]
    Integer,
    #[serde(rename = "dateTime")]
    DateTime,
    #[serde(rename = "boolean")]
    Boolean,
    OptionList,
    SequenceList,
    MemberList,
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

/// Which message a `TriggerMessage.req` asks the Charging Station to send next.
///
/// Ports `MessageTriggerEnumType` (`ocpp/v201/enums.py`). Each variant names a
/// message (or certificate-signing flow) the CSMS can prompt the station to emit
/// proactively. Wire values are PascalCase and identical to the bundled OCPP
/// 2.0.1 FINAL JSON Schema's `enum`, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageTriggerEnumType {
    BootNotification,
    LogStatusNotification,
    FirmwareStatusNotification,
    Heartbeat,
    MeterValues,
    SignChargingStationCertificate,
    SignV2GCertificate,
    StatusNotification,
    TransactionEvent,
    SignCombinedCertificate,
    PublishFirmwareStatusNotification,
}

/// Whether the Charging Station will honor a `TriggerMessage.req`, reported in
/// `TriggerMessage.conf`.
///
/// Ports `TriggerMessageStatusEnumType` (`ocpp/v201/enums.py`). `NotImplemented`
/// means the requested message is recognized but the station does not support
/// triggering it. Wire values are PascalCase, identical to the bundled OCPP
/// 2.0.1 FINAL JSON Schema's `enum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMessageStatusEnumType {
    Accepted,
    Rejected,
    NotImplemented,
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

/// Outcome of a `DataTransfer` vendor exchange: whether the receiver accepted
/// the transfer, and if not, why.
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

/// Purpose of a [`super::ChargingProfileType`]: where in the station's profile
/// stack it applies.
///
/// Ports `ChargingProfilePurposeEnumType` (`ocpp/v201/enums.py`). Wire values
/// are the verbatim schema strings; all four are idiomatic Rust identifiers, so
/// no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingProfilePurposeEnumType {
    ChargingStationExternalConstraints,
    ChargingStationMaxProfile,
    TxDefaultProfile,
    TxProfile,
}

/// Kind of a [`super::ChargingProfileType`] schedule: absolute, relative to the
/// transaction start, or recurring.
///
/// Ports `ChargingProfileKindEnumType` (`ocpp/v201/enums.py`). Wire values are
/// PascalCase, verbatim variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingProfileKindEnumType {
    Absolute,
    Recurring,
    Relative,
}

/// Recurrence period of a `Recurring` [`super::ChargingProfileType`].
///
/// Ports `RecurrencyKindEnumType` (`ocpp/v201/enums.py`). Wire values are
/// PascalCase, verbatim variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecurrencyKindEnumType {
    Daily,
    Weekly,
}

/// Unit in which a [`super::ChargingScheduleType`]'s limits are expressed:
/// watts or amperes.
///
/// Ports `ChargingRateUnitEnumType` (`ocpp/v201/enums.py`). The single-letter
/// wire values `"W"` / `"A"` are valid Rust identifiers and serialize verbatim,
/// so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingRateUnitEnumType {
    /// Watts.
    W,
    /// Amperes.
    A,
}

/// Kind of cost carried by a [`super::CostType`] in a sales tariff.
///
/// Ports `CostKindEnumType` (`ocpp/v201/enums.py`). Wire values are PascalCase,
/// verbatim variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostKindEnumType {
    CarbonDioxideEmission,
    RelativePricePercentage,
    RenewableGenerationPercentage,
}

/// Outcome of a `ReserveNow` request: whether the Charging Station accepted the
/// reservation, and if not, why.
///
/// Ports `ReserveNowStatusEnumType` (`ocpp/v201/enums.py`). The reference
/// dataclass enum and the FINAL JSON Schema agree exactly on these five values.
/// All wire values are PascalCase, verbatim variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReserveNowStatusEnumType {
    /// The reservation has been made.
    Accepted,
    /// The reservation could not be made because the EVSE/connector is faulted.
    Faulted,
    /// The reservation could not be made because the EVSE/connector is occupied.
    Occupied,
    /// The reservation has been rejected (generic refusal).
    Rejected,
    /// The reservation could not be made because the EVSE/connector is
    /// unavailable.
    Unavailable,
}

/// The connector type an EVSE exposes, used by `ReserveNow` to scope a
/// reservation to a particular plug standard.
///
/// Ports `ConnectorEnumType` (`ocpp/v201/enums.py`). Wire values are matched to
/// the **FINAL JSON Schema `enum` verbatim**; the `c…`/`s…`/`w…`-prefixed and
/// hyphenated tokens are not idiomatic Rust identifiers, so they carry explicit
/// `#[serde(rename)]`. The six PascalCase tokens (`Other…`, `Pan`,
/// `Undetermined`, `Unknown`) need no rename.
///
/// Note: the reference *dataclass* additionally defines `cChaoJi` and `cGBT`,
/// but these are **absent from the FINAL JSON Schema** — so they are omitted
/// here to keep serde and schema validation in agreement (both reject them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorEnumType {
    /// Combined Charging System 1 (captive cabled) a.k.a. Combo 1.
    #[serde(rename = "cCCS1")]
    Ccs1,
    /// Combined Charging System 2 (captive cabled) a.k.a. Combo 2.
    #[serde(rename = "cCCS2")]
    Ccs2,
    /// JARI G105-1993 (captive cabled) a.k.a. CHAdeMO.
    #[serde(rename = "cG105")]
    Cg105,
    /// Tesla Connector (captive cabled).
    #[serde(rename = "cTesla")]
    Ctesla,
    /// IEC62196-2 Type 1 connector (captive cabled) a.k.a. J1772.
    #[serde(rename = "cType1")]
    Ctype1,
    /// IEC62196-2 Type 2 connector (captive cabled) a.k.a. Mennekes.
    #[serde(rename = "cType2")]
    Ctype2,
    /// 16A 1 phase IEC60309 socket.
    #[serde(rename = "s309-1P-16A")]
    S3091P16A,
    /// 32A 1 phase IEC60309 socket.
    #[serde(rename = "s309-1P-32A")]
    S3091P32A,
    /// 16A 3 phase IEC60309 socket.
    #[serde(rename = "s309-3P-16A")]
    S3093P16A,
    /// 32A 3 phase IEC60309 socket.
    #[serde(rename = "s309-3P-32A")]
    S3093P32A,
    /// UK domestic socket a.k.a. 13Amp (BS1361).
    #[serde(rename = "sBS1361")]
    Sbs1361,
    /// Schuko socket (CEE 7/7).
    #[serde(rename = "sCEE-7-7")]
    Scee77,
    /// IEC62196-2 Type 2 socket (Mennekes).
    #[serde(rename = "sType2")]
    Stype2,
    /// IEC62196-2 Type 3 socket (Scame).
    #[serde(rename = "sType3")]
    Stype3,
    /// Other single-phase (domestic) socket, max 16A.
    Other1PhMax16A,
    /// Other single-phase (domestic) socket, over 16A.
    Other1PhOver16A,
    /// Other three-phase socket.
    Other3Ph,
    /// Pantograph connector.
    Pan,
    /// Wireless inductive charging.
    #[serde(rename = "wInductive")]
    Winductive,
    /// Wireless resonant charging.
    #[serde(rename = "wResonant")]
    Wresonant,
    /// Yet to be determined (e.g. before plugged in).
    Undetermined,
    /// Unknown / not supported.
    Unknown,
}

/// Whether the CSMS succeeded in cancelling a previously made reservation,
/// reported in `CancelReservation.conf`.
///
/// Ports `CancelReservationStatusEnumType` (`ocpp/v201/enums.py`). `Accepted`
/// if the reservation was cancelled, otherwise `Rejected` (e.g. no reservation
/// with that id exists). Wire values are PascalCase (`"Accepted"`,
/// `"Rejected"`); the reference dataclass enum and the FINAL JSON Schema agree
/// exactly on these two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelReservationStatusEnumType {
    Accepted,
    Rejected,
}

/// Whether a `SendLocalList.req` carries a full replacement of the Local
/// Authorization List or a differential update applied on top of the current
/// one.
///
/// Ports `UpdateEnumType` (`ocpp/v201/enums.py`). For a `Differential` update
/// each entry whose `idTokenInfo` is absent removes that token from the list;
/// for a `Full` update the list (which may be absent/empty for a clear)
/// replaces the station's list entirely. Wire values are PascalCase
/// (`"Differential"`, `"Full"`); the reference dataclass enum and the FINAL
/// JSON Schema agree exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateEnumType {
    Differential,
    Full,
}

/// Whether the Charging Station successfully received and applied the Local
/// Authorization List update from a `SendLocalList.req`, reported in
/// `SendLocalList.conf`.
///
/// Ports `SendLocalListStatusEnumType` (`ocpp/v201/enums.py`).
/// `VersionMismatch` means a differential update was rejected because its
/// `versionNumber` is not exactly one higher than the list the station holds.
/// Wire values are PascalCase (`"Accepted"`, `"Failed"`, `"VersionMismatch"`);
/// the reference dataclass enum and the FINAL JSON Schema agree exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendLocalListStatusEnumType {
    Accepted,
    Failed,
    VersionMismatch,
}

/// Progress of a firmware installation, reported in
/// `FirmwareStatusNotification.req` while an `UpdateFirmware` flow proceeds.
///
/// Ports `FirmwareStatusEnumType` (`ocpp/v201/enums.py`). The values cover the
/// full download → install lifecycle, the terminal failure states, and the
/// `SignatureVerified` / `InvalidSignature` outcomes of the firmware-image
/// signature check. Every wire value is PascalCase and identical between the
/// reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no
/// `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirmwareStatusEnumType {
    Downloaded,
    DownloadFailed,
    Downloading,
    DownloadScheduled,
    DownloadPaused,
    Idle,
    InstallationFailed,
    Installing,
    Installed,
    InstallRebooting,
    InstallScheduled,
    InstallVerificationFailed,
    InvalidSignature,
    SignatureVerified,
}

/// Whether the Charging Station accepted an installed charging profile,
/// reported in `SetChargingProfile.conf`.
///
/// Ports `ChargingProfileStatusEnumType` (`ocpp/v201/enums.py`). `Accepted` if
/// the station was able to process the profile, otherwise `Rejected`. As the
/// FINAL schema notes, `Accepted` does not guarantee the schedule will be
/// followed to the letter — other local constraints may still apply. Wire
/// values are PascalCase (`"Accepted"`, `"Rejected"`); the reference dataclass
/// enum and the bundled OCPP 2.0.1 FINAL JSON Schema agree exactly on these two
/// values, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingProfileStatusEnumType {
    Accepted,
    Rejected,
}

/// Progress of a diagnostics/security log upload, reported in
/// `LogStatusNotification.req` while a `GetLog` flow proceeds.
///
/// Ports `UploadLogStatusEnumType` (`ocpp/v201/enums.py`). The values cover the
/// idle/uploading/uploaded lifecycle, the terminal `UploadFailure`, the
/// `BadMessage` / `NotSupportedOperation` / `PermissionDenied` rejections, and
/// `AcceptedCanceled` (a new upload request supersedes one already running).
/// Every wire value is PascalCase and identical between the reference dataclass
/// enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]`
/// is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadLogStatusEnumType {
    BadMessage,
    Idle,
    NotSupportedOperation,
    PermissionDenied,
    Uploaded,
    UploadFailure,
    Uploading,
    AcceptedCanceled,
}

/// Why a previously-made reservation is no longer valid, reported by the
/// Charging Station in `ReservationStatusUpdate.req`.
///
/// Ports `ReservationUpdateStatusEnumType` (`ocpp/v201/enums.py`): `Expired`
/// (the reservation passed its `expiryDateTime`) or `Removed` (it was dropped
/// for another reason, e.g. the connector became unavailable). Both wire values
/// are PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReservationUpdateStatusEnumType {
    Expired,
    Removed,
}

/// Where an externally-imposed charging limit originated, reported by the
/// Charging Station in `ClearedChargingLimit.req` (and `NotifyChargingLimit`).
///
/// Ports `ChargingLimitSourceEnumType` (`ocpp/v201/enums.py`): `EMS` (an Energy
/// Management System), `Other`, `SO` (a System Operator) or `CSO` (the Charging
/// Station Operator). The acronym variants carry a `#[serde(rename)]` so the
/// Rust identifier stays idiomatic while the wire value matches the OCPP 2.0.1
/// FINAL JSON Schema verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingLimitSourceEnumType {
    #[serde(rename = "EMS")]
    Ems,
    Other,
    #[serde(rename = "SO")]
    So,
    #[serde(rename = "CSO")]
    Cso,
}

/// Progress of a firmware *publish* to a Local Controller's local cache,
/// reported in `PublishFirmwareStatusNotification.req` while a
/// `PublishFirmware` flow proceeds.
///
/// Ports `PublishFirmwareStatusEnumType` (`ocpp/v201/enums.py`). Mirrors
/// [`FirmwareStatusEnumType`] but for the publish-to-local-cache flow: the
/// download lifecycle (`Idle` → `DownloadScheduled` → `Downloading` →
/// `Downloaded`), the terminal `Published`, the failure/pause states
/// (`DownloadFailed`, `DownloadPaused`, `PublishFailed`), and the checksum
/// outcomes (`InvalidChecksum`, `ChecksumVerified`). Every wire value is
/// PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublishFirmwareStatusEnumType {
    Idle,
    DownloadScheduled,
    Downloading,
    Downloaded,
    Published,
    DownloadFailed,
    DownloadPaused,
    InvalidChecksum,
    ChecksumVerified,
    PublishFailed,
}

/// Outcome of an `UnpublishFirmware` request: whether the Local Controller
/// removed the previously-published firmware image from its local cache.
///
/// Ports `UnpublishFirmwareStatusEnumType` (`ocpp/v201/enums.py`). `NoFirmware`
/// means no image matched the requested checksum; `DownloadOngoing` means a
/// publish is still in progress and cannot be torn down yet; `Unpublished` is
/// the success terminal. Every wire value is PascalCase and identical between
/// the reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema,
/// so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnpublishFirmwareStatusEnumType {
    DownloadOngoing,
    NoFirmware,
    Unpublished,
}

/// Whether a request was accepted, used as the generic Accepted/Rejected result
/// status shared by several 2.0.1 messages (e.g. `PublishFirmware`).
///
/// Ports `GenericStatusEnumType` (`ocpp/v201/enums.py`). Both wire values are
/// PascalCase (`"Accepted"`, `"Rejected"`) and identical between the reference
/// dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no
/// `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenericStatusEnumType {
    Accepted,
    Rejected,
}

/// Which slice of the device model a `GetBaseReport` request asks the Charging
/// Station to report: the writable configuration only, the full component /
/// variable inventory, or a summary.
///
/// Ports `ReportBaseEnumType` (`ocpp/v201/enums.py`). Every wire value is
/// PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportBaseEnumType {
    ConfigurationInventory,
    FullInventory,
    SummaryInventory,
}

/// Whether the Charging Station can honour a device-model report or monitoring
/// request (`GetBaseReport`, and — reused as the shared response status — the
/// `GetReport` / `GetMonitoringReport` / `SetMonitoringBase` / `SetMonitoringLevel`
/// / `SetNetworkProfile` family).
///
/// Ports `GenericDeviceModelStatusEnumType` (`ocpp/v201/enums.py`).
/// `EmptyResultSet` reports that the request was accepted but matched no
/// components/variables. Every wire value is PascalCase and identical between
/// the reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema,
/// so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenericDeviceModelStatusEnumType {
    Accepted,
    Rejected,
    NotSupported,
    EmptyResultSet,
}

/// Criterion selecting which components a `GetReport` request asks the Charging
/// Station to include: only those that are currently `Active`, `Available`,
/// `Enabled`, or in a `Problem` state.
///
/// Ports `ComponentCriterionEnumType` (`ocpp/v201/enums.py`). Every wire value
/// is PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentCriterionEnumType {
    Active,
    Available,
    Enabled,
    Problem,
}

/// Criterion selecting which components a `GetMonitoringReport` request asks the
/// Charging Station to include, by the kind of monitor configured on them:
/// `ThresholdMonitoring`, `DeltaMonitoring`, or `PeriodicMonitoring`.
///
/// Ports `MonitoringCriterionEnumType` (`ocpp/v201/enums.py`). Every wire value
/// is PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitoringCriterionEnumType {
    ThresholdMonitoring,
    DeltaMonitoring,
    PeriodicMonitoring,
}

/// The kind of variable monitor a `VariableMonitoringType` describes: a fixed
/// `UpperThreshold` / `LowerThreshold`, a `Delta` (change-since-last-report),
/// or a `Periodic` / `PeriodicClockAligned` interval.
///
/// Ports `MonitorEnumType` (`ocpp/v201/enums.py`). Carried by
/// `NotifyMonitoringReport` (and `SetVariableMonitoring`). Every wire value is
/// PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorEnumType {
    UpperThreshold,
    LowerThreshold,
    Delta,
    Periodic,
    PeriodicClockAligned,
}

/// Which kind of monitor triggered a device-model event carried by a
/// `NotifyEvent` request: a threshold `Alerting` monitor, a `Delta` monitor, or
/// a `Periodic` monitor.
///
/// Ports `EventTriggerEnumType` (`ocpp/v201/enums.py`). Every wire value is
/// PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventTriggerEnumType {
    Alerting,
    Delta,
    Periodic,
}

/// The type of monitor that produced a device-model event carried by a
/// `NotifyEvent` request: a `HardWired` notification or monitor built into the
/// station, a `Preconfigured` monitor, or a `Custom` monitor set up by the
/// CSMS.
///
/// Ports `EventNotificationEnumType` (`ocpp/v201/enums.py`). Every wire value is
/// PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventNotificationEnumType {
    HardWiredNotification,
    HardWiredMonitor,
    PreconfiguredMonitor,
    CustomMonitor,
}

/// Which set of pre-configured variable monitors a `SetMonitoringBase` request
/// activates on the Charging Station: `All` monitors, the `FactoryDefault` set,
/// or only the `HardWiredOnly` monitors.
///
/// Ports `MonitorBaseEnumType` (`ocpp/v201/enums.py`). Every wire value is
/// PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorBaseEnumType {
    All,
    FactoryDefault,
    HardWiredOnly,
}

/// Per-monitor result of a `ClearVariableMonitoring` request: the monitor was
/// cleared (`Accepted`), the station refused to clear it (`Rejected`), or no
/// monitor with the requested id exists (`NotFound`).
///
/// Ports `ClearMonitoringStatusEnumType` (`ocpp/v201/enums.py`). Every wire
/// value is PascalCase and identical between the reference dataclass enum and
/// the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearMonitoringStatusEnumType {
    Accepted,
    Rejected,
    NotFound,
}

/// Per-monitor result of a `SetVariableMonitoring` request: `Accepted` (the
/// monitor was installed, and its assigned `id` is returned), or one of the
/// rejection reasons — the component/variable is unknown, the monitor type is
/// unsupported for that variable, the request was `Rejected`, or it would
/// create a `Duplicate` monitor.
///
/// Ports `SetMonitoringStatusEnumType` (`ocpp/v201/enums.py`). Every wire value
/// is PascalCase and identical between the reference dataclass enum and the
/// bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetMonitoringStatusEnumType {
    Accepted,
    UnknownComponent,
    UnknownVariable,
    UnsupportedMonitorType,
    Rejected,
    Duplicate,
}

/// Whether the Charging Station was able to execute a `ClearChargingProfile`
/// request, reported in `ClearChargingProfile.conf`.
///
/// Ports `ClearChargingProfileStatusEnumType` (`ocpp/v201/enums.py`). `Accepted`
/// means at least one charging profile matched the request's criteria and was
/// cleared; `Unknown` means nothing matched, so no profile was removed. Both
/// wire values are PascalCase and identical between the reference dataclass enum
/// and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is
/// needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearChargingProfileStatusEnumType {
    Accepted,
    Unknown,
}

/// Whether the Charging Station accepted an `UpdateFirmware` request, reported in
/// `UpdateFirmware.conf`.
///
/// Ports `UpdateFirmwareStatusEnumType` (`ocpp/v201/enums.py`). `Accepted` means
/// the station will download and install the firmware; `Rejected` declines the
/// request; `AcceptedCanceled` accepts the new request while canceling a
/// firmware update already in progress; `InvalidCertificate` /
/// `RevokedCertificate` reject the request because the firmware's signing
/// certificate is invalid or has been revoked. Every wire value is PascalCase
/// and identical between the reference dataclass enum and the bundled OCPP 2.0.1
/// FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateFirmwareStatusEnumType {
    Accepted,
    Rejected,
    AcceptedCanceled,
    InvalidCertificate,
    RevokedCertificate,
}

/// Which certificate a signing request (or the signed chain returned for it) is
/// for, used by the certificate-provisioning flow.
///
/// Ports `CertificateSigningUseEnumType` (`ocpp/v201/enums.py`). `SignCertificate`
/// carries this to select the certificate the submitted CSR is for; the paired
/// `CertificateSigned` reuses it to say which certificate the signed chain
/// installs. When omitted the request applies to both the ISO 15118 connection
/// and the Charging-Station-to-CSMS connection. Both wire values are PascalCase
/// (`"ChargingStationCertificate"`, `"V2GCertificate"`) and identical between the
/// reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no
/// `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateSigningUseEnumType {
    ChargingStationCertificate,
    V2GCertificate,
}

/// Whether the Charging Station was able to remove a display message in response
/// to a `ClearDisplayMessage` request, reported in `ClearDisplayMessage.conf`.
///
/// Ports `ClearMessageStatusEnumType` (`ocpp/v201/enums.py`). `Accepted` means
/// the message matching the requested id was found and removed; `Unknown` means
/// no message with that id existed on the station, so nothing was removed. Both
/// wire values are PascalCase and identical between the reference dataclass enum
/// and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is
/// needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearMessageStatusEnumType {
    Accepted,
    Unknown,
}

/// With what priority a display message should be shown on a Charging Station.
///
/// Ports `MessagePriorityEnumType` (`ocpp/v201/enums.py`). Carried by
/// [`MessageInfoType`](crate::v201::MessageInfoType) and used by
/// `SetDisplayMessage` / `NotifyDisplayMessages`. All three wire values are
/// PascalCase and identical between the reference dataclass enum and the bundled
/// OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessagePriorityEnumType {
    AlwaysFront,
    InFront,
    NormalCycle,
}

/// During which Charging Station state a display message should be shown.
///
/// Ports `MessageStateEnumType` (`ocpp/v201/enums.py`). Optional on
/// [`MessageInfoType`](crate::v201::MessageInfoType); when omitted the message
/// is shown in any state. All four wire values are PascalCase and identical
/// between the reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON
/// Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStateEnumType {
    Charging,
    Faulted,
    Idle,
    Unavailable,
}

/// Whether a Charging Station accepted a display message installed via
/// `SetDisplayMessage`, and if not, why.
///
/// Ports `DisplayMessageStatusEnumType` (`ocpp/v201/enums.py`). Returned by
/// `SetDisplayMessage.conf`. All six wire values are PascalCase and identical
/// between the reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON
/// Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMessageStatusEnumType {
    Accepted,
    NotSupportedMessageFormat,
    Rejected,
    NotSupportedPriority,
    NotSupportedState,
    UnknownTransaction,
}

/// Whether a Charging Station holds any display messages matching a
/// `GetDisplayMessages` query.
///
/// Ports `GetDisplayMessagesStatusEnumType` (`ocpp/v201/enums.py`). Returned
/// synchronously by `GetDisplayMessages.conf` to acknowledge the query: the
/// station answers `Accepted` when it has one or more matching messages (which
/// it then streams asynchronously via `NotifyDisplayMessages`), or `Unknown`
/// when it has none. Both wire values are PascalCase and identical between the
/// reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no
/// `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GetDisplayMessagesStatusEnumType {
    Accepted,
    Unknown,
}

/// Whether a Charging Station accepted the signed certificate chain the CSMS
/// delivered via `CertificateSigned`.
///
/// Ports `CertificateSignedStatusEnumType` (`ocpp/v201/enums.py`). Returned by
/// `CertificateSigned.conf` to report whether the station installed the chain.
/// The two wire values are PascalCase (`"Accepted"`, `"Rejected"`) and identical
/// between the reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON
/// Schema, so no `#[serde(rename)]` is needed. Its members coincide with
/// [`GenericStatusEnumType`], but the schema defines it as its own type, so it is
/// kept distinct here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateSignedStatusEnumType {
    Accepted,
    Rejected,
}

/// Which root (CA) certificate an `InstallCertificate` request installs into the
/// Charging Station's trust store.
///
/// Ports `InstallCertificateUseEnumType` (`ocpp/v201/enums.py`). This selects the
/// trust anchor being installed — the V2G root, an MO (Mobility Operator) root,
/// the CSMS root, or a manufacturer root. It is the certificate-*management*
/// counterpart to [`CertificateSigningUseEnumType`], which selects the station's
/// *own* certificate during provisioning. All four wire values are PascalCase
/// (`"V2GRootCertificate"`, `"MORootCertificate"`, `"CSMSRootCertificate"`,
/// `"ManufacturerRootCertificate"`) and identical between the reference dataclass
/// enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is
/// needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallCertificateUseEnumType {
    V2GRootCertificate,
    MORootCertificate,
    CSMSRootCertificate,
    ManufacturerRootCertificate,
}

/// Whether a Charging Station installed the root certificate the CSMS delivered
/// via `InstallCertificate`.
///
/// Ports `InstallCertificateStatusEnumType` (`ocpp/v201/enums.py`). Returned by
/// `InstallCertificate.conf`: `Accepted` if the certificate was installed,
/// `Rejected` if the station refused it, `Failed` if installation was attempted
/// but did not complete. The three wire values are PascalCase (`"Accepted"`,
/// `"Rejected"`, `"Failed"`) and identical between the reference dataclass enum
/// and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is
/// needed. Unlike the two-member [`GenericStatusEnumType`], it carries a distinct
/// third `Failed` member, so it is kept as its own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallCertificateStatusEnumType {
    Accepted,
    Rejected,
    Failed,
}

/// Whether a Charging Station could remove a previously installed certificate
/// requested by a `DeleteCertificate` command.
///
/// Ports `DeleteCertificateStatusEnumType` (`ocpp/v201/enums.py`). Returned by
/// `DeleteCertificate.conf`: `Accepted` means the certificate matching the
/// supplied [`super::CertificateHashDataType`] was found and removed; `NotFound`
/// means no installed certificate matched the hash, so nothing was removed;
/// `Failed` means a match existed but the station could not remove it. All three
/// wire values are PascalCase (`"Accepted"`, `"Failed"`, `"NotFound"`) and
/// identical between the reference dataclass enum and the bundled OCPP 2.0.1
/// FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteCertificateStatusEnumType {
    Accepted,
    Failed,
    NotFound,
}

/// Which kind of installed certificate a `GetInstalledCertificateIds` request
/// asks the Charging Station to enumerate.
///
/// Ports `GetCertificateIdUseEnumType` (`ocpp/v201/enums.py`). It is a superset
/// of [`InstallCertificateUseEnumType`]: the same four trust-anchor roots plus a
/// fifth member, `V2GCertificateChain`, which selects the station's installed
/// V2G leaf-certificate chain rather than a root. Because of that extra member it
/// is kept as its own type and **not** aliased to `InstallCertificateUseEnumType`.
/// All five wire values are PascalCase (`"V2GRootCertificate"`,
/// `"MORootCertificate"`, `"CSMSRootCertificate"`, `"V2GCertificateChain"`,
/// `"ManufacturerRootCertificate"`) and identical between the reference dataclass
/// enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is
/// needed. Carried by `GetInstalledCertificateIds` requests and by
/// [`super::CertificateHashDataChainType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GetCertificateIdUseEnumType {
    V2GRootCertificate,
    MORootCertificate,
    CSMSRootCertificate,
    V2GCertificateChain,
    ManufacturerRootCertificate,
}

/// Whether a Charging Station could process a `GetInstalledCertificateIds`
/// request.
///
/// Ports `GetInstalledCertificateStatusEnumType` (`ocpp/v201/enums.py`). Returned
/// by `GetInstalledCertificateIds.conf`: `Accepted` when the station returns the
/// (possibly empty) set of matching installed certificates, `NotFound` when no
/// certificate of the requested type(s) is installed. Both wire values are
/// PascalCase (`"Accepted"`, `"NotFound"`) and identical between the reference
/// dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no
/// `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GetInstalledCertificateStatusEnumType {
    Accepted,
    NotFound,
}

/// Whether a Charging Station was able to retrieve the OCSP status of a
/// certificate on the CSMS's behalf.
///
/// Ports `GetCertificateStatusEnumType` (`ocpp/v201/enums.py`). Returned by
/// `GetCertificateStatus.conf`: `Accepted` means the CSMS obtained the OCSP
/// response (carried back in `ocspResult`), `Failed` means it could not. Both
/// wire values are PascalCase (`"Accepted"`, `"Failed"`) and identical between
/// the reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so
/// no `#[serde(rename)]` is needed. Distinct from the two-member
/// [`GenericStatusEnumType`] (`Accepted` / `Rejected`): it pairs `Accepted` with
/// `Failed`, not `Rejected`, so it is kept as its own type. Used by
/// `GetCertificateStatus.conf` to report the OCSP lookup outcome for a
/// [`super::OCSPRequestDataType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GetCertificateStatusEnumType {
    Accepted,
    Failed,
}

/// Whether an ISO 15118 contract certificate needs to be freshly installed or
/// an existing one updated.
///
/// Ports `CertificateActionEnumType` (`ocpp/v201/enums.py`). Carried by
/// `Get15118EVCertificate.req` in its `action` field: the Charging Station
/// relays the EV's raw EXI `CertificateInstallationReq` (`Install`) or
/// certificate-update request (`Update`) up to the CSMS. Both wire values are
/// PascalCase (`"Install"`, `"Update"`) and identical between the reference
/// dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no
/// `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateActionEnumType {
    Install,
    Update,
}

/// Whether the CSMS could process a relayed ISO 15118 EV certificate request.
///
/// Ports `Iso15118EVCertificateStatusEnumType` (`ocpp/v201/enums.py`). Returned
/// by `Get15118EVCertificate.conf`: `Accepted` means the CSMS produced the EXI
/// `CertificateInstallationRes` (carried back in `exiResponse`), `Failed` means
/// it could not. Both wire values are PascalCase (`"Accepted"`, `"Failed"`) and
/// identical between the reference dataclass enum and the bundled OCPP 2.0.1
/// FINAL JSON Schema, so no `#[serde(rename)]` is needed. Like
/// [`GetCertificateStatusEnumType`] it pairs `Accepted` with `Failed` (not
/// `Rejected`), but it reports the outcome of the plug-and-charge certificate
/// exchange rather than an OCSP lookup, so it is kept as its own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Iso15118EVCertificateStatusEnumType {
    Accepted,
    Failed,
}

/// The kind of log a Charging Station should collect and upload in response to
/// a `GetLog.req`.
///
/// Ports `LogEnumType` (`ocpp/v201/enums.py`): `DiagnosticsLog` (the general
/// diagnostics log) or `SecurityLog` (the security event log). Both wire values
/// are PascalCase (`"DiagnosticsLog"`, `"SecurityLog"`) and identical between
/// the reference dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so
/// no `#[serde(rename)]` is needed. Carried by `GetLog.req` in its `logType`
/// field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogEnumType {
    DiagnosticsLog,
    SecurityLog,
}

/// Whether a Charging Station accepted a `GetLog.req`, i.e. whether it will
/// collect and upload the requested log.
///
/// Ports `LogStatusEnumType` (`ocpp/v201/enums.py`): `Accepted` (the upload will
/// proceed), `Rejected` (the station declined), or `AcceptedCanceled` (accepted,
/// but a log upload already in progress was canceled to serve this one). All
/// three wire values are PascalCase and identical between the reference dataclass
/// enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is
/// needed. Returned by `GetLog.conf`.
///
/// Distinct from [`UploadLogStatusEnumType`], which reports the *progress* of an
/// upload already underway in `LogStatusNotification.req`; this enum is the
/// synchronous accept/reject ack of the triggering `GetLog.req`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogStatusEnumType {
    Accepted,
    Rejected,
    AcceptedCanceled,
}

/// Whether a Charging Station can honour a `GetChargingProfiles` query and will
/// stream the matching profiles back.
///
/// Ports `GetChargingProfileStatusEnumType` (`ocpp/v201/enums.py`). Returned
/// synchronously by `GetChargingProfiles.conf`: the station answers `Accepted`
/// when it has one or more profiles matching the criterion (which it then
/// streams asynchronously via `ReportChargingProfiles`, correlated by
/// `requestId`), or `NoProfiles` when nothing matches. Both wire values are
/// PascalCase and identical between the reference dataclass enum and the bundled
/// OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GetChargingProfileStatusEnumType {
    Accepted,
    NoProfiles,
}

/// Whether a Charging Station accepted a `CustomerInformation` request to report
/// and/or clear stored customer data.
///
/// Ports `CustomerInformationStatusEnumType` (`ocpp/v201/enums.py`). Returned
/// synchronously by `CustomerInformation.conf`: `Accepted` acknowledges the
/// command (any requested report data then arrives asynchronously via
/// `NotifyCustomerInformation`), `Rejected` refuses it, and `Invalid` signals
/// that the request itself was malformed (e.g. no usable customer selector).
/// All three wire values are PascalCase and identical between the reference
/// dataclass enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no
/// `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomerInformationStatusEnumType {
    Accepted,
    Rejected,
    Invalid,
}

/// Mode of energy transfer an EV requests over ISO 15118, reported inside a
/// [`ChargingNeedsType`](crate::v201::ChargingNeedsType).
///
/// Ports `EnergyTransferModeEnumType` (`ocpp/v201/enums.py`). The wire values
/// carry underscores and an uppercase `DC`/`AC` prefix that are not idiomatic
/// Rust identifiers, so every variant carries an explicit `#[serde(rename)]` to
/// the verbatim OCPP 2.0.1 FINAL JSON Schema spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnergyTransferModeEnumType {
    /// DC charging.
    #[serde(rename = "DC")]
    Dc,
    /// AC single phase charging.
    #[serde(rename = "AC_single_phase")]
    AcSinglePhase,
    /// AC two phase charging.
    #[serde(rename = "AC_two_phase")]
    AcTwoPhase,
    /// AC three phase charging.
    #[serde(rename = "AC_three_phase")]
    AcThreePhase,
}

/// Whether the CSMS could process a `NotifyEVChargingNeeds` message.
///
/// Ports `NotifyEVChargingNeedsStatusEnumType` (`ocpp/v201/enums.py`). Returned
/// synchronously by `NotifyEVChargingNeeds.conf`: `Accepted` means an
/// SASchedule will be provided momentarily, `Rejected` that the service is not
/// available, and `Processing` that the CSMS is still gathering information to
/// build the schedule. Per the spec the status reports only whether the message
/// was processed — it does **not** imply the EV's charging needs can be met with
/// the current charging profile. All three wire values are PascalCase and
/// identical between the reference dataclass enum and the bundled OCPP 2.0.1
/// FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotifyEVChargingNeedsStatusEnumType {
    Accepted,
    Rejected,
    Processing,
}

/// OCPP version a Charging Station uses over a given network connection,
/// carried by a `SetNetworkProfile`'s
/// [`NetworkConnectionProfileType`](crate::v201::NetworkConnectionProfileType).
///
/// Ports `OCPPVersionEnumType` (`ocpp/v201/enums.py`). Every wire value is an
/// all-caps `OCPP<n>` token, so each variant is renamed from its idiomatic Rust
/// spelling. Note the 2.0.1 schema tops out at `OCPP20` (there is no
/// `OCPP201` value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OCPPVersionEnumType {
    #[serde(rename = "OCPP12")]
    Ocpp12,
    #[serde(rename = "OCPP15")]
    Ocpp15,
    #[serde(rename = "OCPP16")]
    Ocpp16,
    #[serde(rename = "OCPP20")]
    Ocpp20,
}

/// Transport protocol a Charging Station uses over a given network connection.
///
/// Ports `OCPPTransportEnumType` (`ocpp/v201/enums.py`). Both wire values are
/// all-caps acronyms, so each variant is renamed from its idiomatic Rust
/// spelling. `SOAP` is not used in OCPP 2.0 but the enum retains it for
/// cross-version compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OCPPTransportEnumType {
    #[serde(rename = "JSON")]
    Json,
    #[serde(rename = "SOAP")]
    Soap,
}

/// Physical/logical network interface a Charging Station uses for a given
/// network connection.
///
/// Ports `OCPPInterfaceEnumType` (`ocpp/v201/enums.py`). Every wire value is an
/// idiomatic Rust identifier (`Wired0`…`Wired3`, `Wireless0`…`Wireless3`) and
/// identical between the reference dataclass enum and the bundled OCPP 2.0.1
/// FINAL JSON Schema, so no `#[serde(rename)]` is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OCPPInterfaceEnumType {
    Wired0,
    Wired1,
    Wired2,
    Wired3,
    Wireless0,
    Wireless1,
    Wireless2,
    Wireless3,
}

/// Authentication method used on a cellular data connection, carried by a
/// `SetNetworkProfile`'s [`APNType`](crate::v201::APNType).
///
/// Ports `APNAuthenticationEnumType` (`ocpp/v201/enums.py`). All four wire
/// values are all-caps, so each variant is renamed from its idiomatic Rust
/// spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum APNAuthenticationEnumType {
    #[serde(rename = "CHAP")]
    Chap,
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "PAP")]
    Pap,
    #[serde(rename = "AUTO")]
    Auto,
}

/// VPN tunnelling protocol, carried by a `SetNetworkProfile`'s
/// [`VPNType`](crate::v201::VPNType).
///
/// Ports `VPNEnumType` (`ocpp/v201/enums.py`). Every wire value carries
/// non-idiomatic casing (`IKEv2`, `IPSec`, `L2TP`, `PPTP`), so each variant is
/// renamed from its idiomatic Rust spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VPNEnumType {
    #[serde(rename = "IKEv2")]
    Ikev2,
    #[serde(rename = "IPSec")]
    Ipsec,
    #[serde(rename = "L2TP")]
    L2tp,
    #[serde(rename = "PPTP")]
    Pptp,
}

/// Result of a `SetNetworkProfile` request, reported by the Charging Station.
///
/// Ports `SetNetworkProfileStatusEnumType` (`ocpp/v201/enums.py`). All three
/// wire values are PascalCase and identical between the reference dataclass
/// enum and the bundled OCPP 2.0.1 FINAL JSON Schema, so no `#[serde(rename)]`
/// is needed. `Failed` reports the station accepted the message but could not
/// apply the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetNetworkProfileStatusEnumType {
    Accepted,
    Rejected,
    Failed,
}
