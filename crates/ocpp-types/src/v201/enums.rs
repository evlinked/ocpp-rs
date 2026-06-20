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
