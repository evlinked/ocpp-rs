//! OCPP 2.0.1 shared datatypes and enumerations.
//!
//! This module ports the enums and `*Type` datatypes from the OCPP 2.0.1
//! specification (mobilityhouse/ocpp `ocpp/v201/enums.py` and
//! `ocpp/v201/datatypes.py`), mirroring the conventions of [`crate::v16j`]:
//! serde with explicit camelCase renames and `skip_serializing_if` on every
//! optional field so absent values never appear on the wire.
//!
//! It is the foundation slice for **M7 — OCPP 2.0.1**; today it carries what
//! the core lifecycle messages (`BootNotification`, `Heartbeat`,
//! `StatusNotification`, `Authorize`) need. Subsequent 2.0.1 messages extend it.

use serde::{Deserialize, Serialize};

// =============================================================================
// Enumerations
// =============================================================================

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

/// Hash algorithm used for the OCSP request data in the ISO 15118
/// plug-and-charge certificate path.
///
/// Ports `HashAlgorithmEnumType` (`ocpp/v201/enums.py`). All wire values are
/// all-caps acronyms, so each variant is renamed from its idiomatic Rust
/// spelling. Used by [`OCSPRequestDataType`].
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

// =============================================================================
// Datatypes
// =============================================================================

/// Open-ended vendor extension object carried by virtually every 2.0.1
/// message and datatype.
///
/// Ports `CustomDataType`. The schema requires `vendorId` and explicitly
/// permits arbitrary additional properties (it is the one type that does *not*
/// get `additionalProperties: false`), so extra keys are preserved verbatim
/// via `#[serde(flatten)]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomDataType {
    /// Vendor identification (max length 255 per schema).
    #[serde(rename = "vendorId")]
    pub vendor_id: String,
    /// Any additional vendor-specific properties.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// More information about the status returned in a response.
///
/// Ports `StatusInfoType`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusInfoType {
    /// Predefined, vendor-agnostic code describing the reason (max length 20).
    #[serde(rename = "reasonCode")]
    pub reason_code: String,
    /// Additional human-readable text (max length 512).
    #[serde(rename = "additionalInfo", skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Parameters of the wireless communication module of a Charging Station.
///
/// Ports `ModemType`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModemType {
    /// ICCID of the modem's SIM card (max length 20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iccid: Option<String>,
    /// IMSI of the modem's SIM card (max length 20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imsi: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// The physical system (Charging Station) where an EV can be charged.
///
/// Ports `ChargingStationType`. Only `model` and `vendorName` are required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingStationType {
    /// Vendor identification (not necessarily unique; max length 50).
    #[serde(rename = "vendorName")]
    pub vendor_name: String,
    /// Model of the Charging Station (max length 20).
    pub model: String,
    /// Vendor-specific device serial number (max length 25).
    #[serde(rename = "serialNumber", skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Firmware version running on the Charging Station (max length 50).
    #[serde(rename = "firmwareVersion", skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    /// Wireless-modem parameters, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modem: Option<ModemType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

// =============================================================================
// Device model: components, variables, and the GetVariables enums
// =============================================================================

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

/// Electric Vehicle Supply Equipment — an EVSE (and optionally a connector
/// within it) that scopes a [`ComponentType`].
///
/// Ports `EVSEType`. Only `id` is required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvseType {
    /// EVSE identifier within the Charging Station (≥ 1).
    pub id: i32,
    /// Connector within the EVSE, if the component is connector-scoped.
    #[serde(rename = "connectorId", skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A case-insensitive additional identifier, paired with its type, used to
/// support multiple forms of identifiers alongside the primary `idToken`.
///
/// Ports `AdditionalInfoType`. Both fields are required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionalInfoType {
    /// The additional identifier (max length 36).
    #[serde(rename = "additionalIdToken")]
    pub additional_id_token: String,
    /// The type of the additional identifier; a custom, party-agreed string
    /// (max length 50) — *not* an [`IdTokenEnumType`].
    #[serde(rename = "type")]
    pub kind: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A physical or logical component of the device model.
///
/// Ports `ComponentType`. Only `name` is required; `instance` disambiguates
/// multiple instances of the same component and `evse` scopes it to a
/// particular EVSE/connector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentType {
    /// Name of the component (case-insensitive; max length 50).
    pub name: String,
    /// Name of the instance in case the component exists as multiple instances
    /// (max length 50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// EVSE the component belongs to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evse: Option<EvseType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A case-insensitive identifier used for authorization, plus the type of
/// authorization it represents.
///
/// Ports `IdTokenType`. `id_token` and `kind` are required; the schema also
/// caps `additional_info` at a minimum of one item when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdTokenType {
    /// The identifier itself — case-insensitive; may hold an RFID tag's hidden
    /// id, a UUID, etc. (max length 36).
    #[serde(rename = "idToken")]
    pub id_token: String,
    /// The kind of identifier this is.
    #[serde(rename = "type")]
    pub kind: IdTokenEnumType,
    /// Additional identifiers (e.g. a linked parent token). Omitted entirely
    /// when absent; the schema requires at least one item when present.
    #[serde(rename = "additionalInfo", skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<Vec<AdditionalInfoType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// OCSP request data identifying a single certificate to be checked along the
/// ISO 15118 plug-and-charge path, carried by an `Authorize` request's
/// `iso15118CertificateHashData`.
///
/// Ports `OCSPRequestDataType`. Every field except `custom_data` is required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OCSPRequestDataType {
    /// Hash algorithm used for `issuer_name_hash` and `issuer_key_hash`.
    #[serde(rename = "hashAlgorithm")]
    pub hash_algorithm: HashAlgorithmEnumType,
    /// Hashed value of the issuer Distinguished Name (max length 128).
    #[serde(rename = "issuerNameHash")]
    pub issuer_name_hash: String,
    /// Hashed value of the issuer's public key (max length 128).
    #[serde(rename = "issuerKeyHash")]
    pub issuer_key_hash: String,
    /// Serial number of the certificate (max length 40).
    #[serde(rename = "serialNumber")]
    pub serial_number: String,
    /// Case-insensitive responder URL of the OCSP server (max length 512).
    #[serde(rename = "responderURL")]
    pub responder_url: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Message details for a message to be displayed on a Charging Station.
///
/// Ports `MessageContentType`. `format` and `content` are required. Carried by
/// [`IdTokenInfoType::personal_message`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageContentType {
    /// Format of the message contents.
    pub format: MessageFormatEnumType,
    /// The message contents (max length 512).
    pub content: String,
    /// Message language identifier (RFC 5646 code; max length 8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Reference key to a variable within a [`ComponentType`].
///
/// Ports `VariableType`. Only `name` is required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableType {
    /// Name of the variable (case-insensitive; max length 50).
    pub name: String,
    /// Name of the instance in case the variable exists as multiple instances
    /// (max length 50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// One entry in a `GetVariables` request: which attribute of which
/// component-variable to read.
///
/// Ports `GetVariableDataType`. `component` and `variable` are required;
/// `attribute_type` defaults to [`AttributeEnumType::Actual`] when omitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetVariableDataType {
    /// Component for which the variable is requested.
    pub component: ComponentType,
    /// Variable for which the attribute value is requested.
    pub variable: VariableType,
    /// Attribute to read; absent means `Actual`.
    #[serde(rename = "attributeType", skip_serializing_if = "Option::is_none")]
    pub attribute_type: Option<AttributeEnumType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// One entry in a `GetVariables` response: the outcome of reading a single
/// component-variable attribute.
///
/// Ports `GetVariableResultType`. `attribute_status`, `component`, and
/// `variable` are required; `attribute_value` is present only when the read
/// was `Accepted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetVariableResultType {
    /// Result status of getting the variable.
    #[serde(rename = "attributeStatus")]
    pub attribute_status: GetVariableStatusEnumType,
    /// Component for which the variable was requested.
    pub component: ComponentType,
    /// Variable for which the attribute value was requested.
    pub variable: VariableType,
    /// Attribute that was read; absent means `Actual`.
    #[serde(rename = "attributeType", skip_serializing_if = "Option::is_none")]
    pub attribute_type: Option<AttributeEnumType>,
    /// Value of the attribute (max length 2500); only meaningful when
    /// `attribute_status` is `Accepted`.
    #[serde(rename = "attributeValue", skip_serializing_if = "Option::is_none")]
    pub attribute_value: Option<String>,
    /// Detail about the `attribute_status`.
    #[serde(
        rename = "attributeStatusInfo",
        skip_serializing_if = "Option::is_none"
    )]
    pub attribute_status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Status information about an identifier, returned in an `AuthorizeResponse`
/// (and reused by the 2.0.1 transaction model).
///
/// Ports `IdTokenInfoType`. Only `status` is required; `cache_expiry_date_time`
/// is for caching only and (per the spec) should not by itself stop an active
/// charging session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdTokenInfoType {
    /// Current status of the identifier.
    pub status: AuthorizationStatusEnumType,
    /// Date/time after which the cached token must be considered invalid
    /// (RFC 3339 / ISO 8601). Caching hint only.
    #[serde(
        rename = "cacheExpiryDateTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_expiry_date_time: Option<String>,
    /// Business priority, -9..=9 (default 0); higher is more important.
    #[serde(rename = "chargingPriority", skip_serializing_if = "Option::is_none")]
    pub charging_priority: Option<i32>,
    /// Preferred UI language of the identifier's user (RFC 5646; max length 8).
    #[serde(rename = "language1", skip_serializing_if = "Option::is_none")]
    pub language1: Option<String>,
    /// EVSE ids the token is restricted to; absent means the whole station.
    /// The schema requires at least one item when present.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<Vec<i32>>,
    /// Second preferred UI language (RFC 5646; max length 8).
    #[serde(rename = "language2", skip_serializing_if = "Option::is_none")]
    pub language2: Option<String>,
    /// A parent/group token this identifier belongs to.
    #[serde(rename = "groupIdToken", skip_serializing_if = "Option::is_none")]
    pub group_id_token: Option<IdTokenType>,
    /// A personal message to display for this identifier.
    #[serde(rename = "personalMessage", skip_serializing_if = "Option::is_none")]
    pub personal_message: Option<MessageContentType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
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

/// State of an ongoing or finished transaction.
///
/// Ports `TransactionType`. Only `transactionId` is required; the remaining
/// fields describe the charging state and (on the `Ended` event) the stop
/// reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionType {
    /// Unique identifier of the transaction (max length 36).
    #[serde(rename = "transactionId")]
    pub transaction_id: String,
    /// Current charging state.
    #[serde(rename = "chargingState", skip_serializing_if = "Option::is_none")]
    pub charging_state: Option<ChargingStateEnumType>,
    /// Cumulative seconds the EV has actually been charging (excludes pauses).
    #[serde(rename = "timeSpentCharging", skip_serializing_if = "Option::is_none")]
    pub time_spent_charging: Option<i32>,
    /// Why the transaction was stopped (present on the `Ended` event).
    #[serde(rename = "stoppedReason", skip_serializing_if = "Option::is_none")]
    pub stopped_reason: Option<ReasonEnumType>,
    /// `RequestStartTransaction` id that started this transaction remotely.
    #[serde(rename = "remoteStartId", skip_serializing_if = "Option::is_none")]
    pub remote_start_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn boot_reason_serializes_pascal_case() {
        assert_eq!(
            serde_json::to_value(BootReasonEnumType::PowerUp).unwrap(),
            json!("PowerUp")
        );
        assert_eq!(
            serde_json::to_value(BootReasonEnumType::ApplicationReset).unwrap(),
            json!("ApplicationReset")
        );
        let parsed: BootReasonEnumType = serde_json::from_value(json!("Watchdog")).unwrap();
        assert_eq!(parsed, BootReasonEnumType::Watchdog);
    }

    #[test]
    fn registration_status_round_trips() {
        for (variant, wire) in [
            (RegistrationStatusEnumType::Accepted, "Accepted"),
            (RegistrationStatusEnumType::Pending, "Pending"),
            (RegistrationStatusEnumType::Rejected, "Rejected"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: RegistrationStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn unknown_enum_value_is_rejected() {
        let err = serde_json::from_value::<RegistrationStatusEnumType>(json!("Bogus"));
        assert!(err.is_err());
    }

    #[test]
    fn connector_status_serializes_pascal_case() {
        for (variant, wire) in [
            (ConnectorStatusEnumType::Available, "Available"),
            (ConnectorStatusEnumType::Occupied, "Occupied"),
            (ConnectorStatusEnumType::Reserved, "Reserved"),
            (ConnectorStatusEnumType::Unavailable, "Unavailable"),
            (ConnectorStatusEnumType::Faulted, "Faulted"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: ConnectorStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // The 1.6J-only states are not part of the 2.0.1 vocabulary.
        assert!(serde_json::from_value::<ConnectorStatusEnumType>(json!("Charging")).is_err());
    }

    #[test]
    fn charging_station_omits_none_optionals() {
        let cs = ChargingStationType {
            vendor_name: "ICU Eve Mini".to_string(),
            model: "ICU Eve Mini".to_string(),
            serial_number: None,
            firmware_version: Some("#1:3.4.0-2990#N:217H;1.0-223".to_string()),
            modem: None,
            custom_data: None,
        };
        // Matches the Python reference fixture (tests/v201/test_v201_charge_point.py):
        // only the three present fields, in camelCase, no nulls.
        assert_eq!(
            serde_json::to_value(&cs).unwrap(),
            json!({
                "vendorName": "ICU Eve Mini",
                "model": "ICU Eve Mini",
                "firmwareVersion": "#1:3.4.0-2990#N:217H;1.0-223"
            })
        );
    }

    #[test]
    fn modem_round_trips_through_charging_station() {
        let cs = ChargingStationType {
            vendor_name: "Vendor".to_string(),
            model: "Model".to_string(),
            serial_number: Some("SN-1".to_string()),
            firmware_version: None,
            modem: Some(ModemType {
                iccid: Some("89000000".to_string()),
                imsi: Some("26201".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&cs).unwrap();
        assert_eq!(wire["modem"]["iccid"], json!("89000000"));
        let back: ChargingStationType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, cs);
    }

    #[test]
    fn custom_data_preserves_extra_properties() {
        let value = json!({ "vendorId": "com.example", "foo": 1, "bar": ["a", "b"] });
        let cd: CustomDataType = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(cd.vendor_id, "com.example");
        assert_eq!(cd.extra.get("foo"), Some(&json!(1)));
        // Round-trips back to the same object, extras intact.
        assert_eq!(serde_json::to_value(&cd).unwrap(), value);
    }

    #[test]
    fn status_info_omits_none_optionals() {
        let si = StatusInfoType {
            reason_code: "Booted".to_string(),
            additional_info: None,
            custom_data: None,
        };
        assert_eq!(
            serde_json::to_value(&si).unwrap(),
            json!({ "reasonCode": "Booted" })
        );
    }

    #[test]
    fn attribute_enum_serializes_pascal_case() {
        for (variant, wire) in [
            (AttributeEnumType::Actual, "Actual"),
            (AttributeEnumType::Target, "Target"),
            (AttributeEnumType::MinSet, "MinSet"),
            (AttributeEnumType::MaxSet, "MaxSet"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: AttributeEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn id_token_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (IdTokenEnumType::Central, "Central"),
            (IdTokenEnumType::EMaid, "eMAID"),
            (IdTokenEnumType::Iso14443, "ISO14443"),
            (IdTokenEnumType::Iso15693, "ISO15693"),
            (IdTokenEnumType::KeyCode, "KeyCode"),
            (IdTokenEnumType::Local, "Local"),
            (IdTokenEnumType::MacAddress, "MacAddress"),
            (IdTokenEnumType::NoAuthorization, "NoAuthorization"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: IdTokenEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // Unknown / mis-cased values are rejected.
        assert!(serde_json::from_value::<IdTokenEnumType>(json!("emaid")).is_err());
        assert!(serde_json::from_value::<IdTokenEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn authorization_status_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (AuthorizationStatusEnumType::Accepted, "Accepted"),
            (AuthorizationStatusEnumType::Blocked, "Blocked"),
            (AuthorizationStatusEnumType::ConcurrentTx, "ConcurrentTx"),
            (AuthorizationStatusEnumType::Expired, "Expired"),
            (AuthorizationStatusEnumType::Invalid, "Invalid"),
            (AuthorizationStatusEnumType::NoCredit, "NoCredit"),
            (
                AuthorizationStatusEnumType::NotAllowedTypeEvse,
                "NotAllowedTypeEVSE",
            ),
            (
                AuthorizationStatusEnumType::NotAtThisLocation,
                "NotAtThisLocation",
            ),
            (AuthorizationStatusEnumType::NotAtThisTime, "NotAtThisTime"),
            (AuthorizationStatusEnumType::Unknown, "Unknown"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: AuthorizationStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        assert!(serde_json::from_value::<AuthorizationStatusEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn message_format_enum_round_trips() {
        for (variant, wire) in [
            (MessageFormatEnumType::Ascii, "ASCII"),
            (MessageFormatEnumType::Html, "HTML"),
            (MessageFormatEnumType::Uri, "URI"),
            (MessageFormatEnumType::Utf8, "UTF8"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: MessageFormatEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn get_variable_status_serializes_pascal_case() {
        for (variant, wire) in [
            (GetVariableStatusEnumType::Accepted, "Accepted"),
            (GetVariableStatusEnumType::Rejected, "Rejected"),
            (
                GetVariableStatusEnumType::UnknownComponent,
                "UnknownComponent",
            ),
            (
                GetVariableStatusEnumType::UnknownVariable,
                "UnknownVariable",
            ),
            (
                GetVariableStatusEnumType::NotSupportedAttributeType,
                "NotSupportedAttributeType",
            ),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: GetVariableStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn enums_reject_unknown_wire_values() {
        assert!(serde_json::from_value::<AttributeEnumType>(json!("Bogus")).is_err());
        assert!(serde_json::from_value::<GetVariableStatusEnumType>(json!("Nope")).is_err());
    }

    #[test]
    fn component_omits_none_optionals() {
        let c = ComponentType {
            name: "EVSE".to_string(),
            instance: None,
            evse: None,
            custom_data: None,
        };
        assert_eq!(serde_json::to_value(&c).unwrap(), json!({ "name": "EVSE" }));
    }

    #[test]
    fn evse_round_trips_with_connector() {
        let evse = EvseType {
            id: 1,
            connector_id: Some(2),
            custom_data: None,
        };
        let wire = serde_json::to_value(&evse).unwrap();
        assert_eq!(wire, json!({ "id": 1, "connectorId": 2 }));
        let back: EvseType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, evse);
    }

    #[test]
    fn get_variable_data_defaults_attribute_type_to_absent() {
        let data = GetVariableDataType {
            component: ComponentType {
                name: "SampledDataCtrlr".to_string(),
                instance: None,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            },
            variable: VariableType {
                name: "TxEndedMeasurands".to_string(),
                instance: None,
                custom_data: None,
            },
            attribute_type: None,
            custom_data: None,
        };
        let expected = json!({
            "component": { "name": "SampledDataCtrlr", "evse": { "id": 1 } },
            "variable": { "name": "TxEndedMeasurands" }
        });
        assert_eq!(serde_json::to_value(&data).unwrap(), expected);
        let back: GetVariableDataType = serde_json::from_value(expected).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn get_variable_result_round_trips() {
        let result = GetVariableResultType {
            attribute_status: GetVariableStatusEnumType::Accepted,
            component: ComponentType {
                name: "OCPPCommCtrlr".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "HeartbeatInterval".to_string(),
                instance: None,
                custom_data: None,
            },
            attribute_type: Some(AttributeEnumType::Actual),
            attribute_value: Some("300".to_string()),
            attribute_status_info: None,
            custom_data: None,
        };
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["attributeStatus"], json!("Accepted"));
        assert_eq!(wire["attributeValue"], json!("300"));
        assert_eq!(wire["attributeType"], json!("Actual"));
        let back: GetVariableResultType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, result);
    }

    #[test]
    fn id_token_minimal_matches_wire_json() {
        // Reference: tests/v201/conftest.py — a bare RFID token.
        let token = IdTokenType {
            id_token: "045918E24B6D80".to_string(),
            kind: IdTokenEnumType::Iso14443,
            additional_info: None,
            custom_data: None,
        };
        let expected = json!({
            "idToken": "045918E24B6D80",
            "type": "ISO14443"
        });
        assert_eq!(serde_json::to_value(&token).unwrap(), expected);
        let back: IdTokenType = serde_json::from_value(expected).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn id_token_with_additional_info_round_trips() {
        let token = IdTokenType {
            id_token: "primary".to_string(),
            kind: IdTokenEnumType::Central,
            additional_info: Some(vec![AdditionalInfoType {
                additional_id_token: "linked".to_string(),
                kind: "VendorScheme".to_string(),
                custom_data: None,
            }]),
            custom_data: None,
        };
        let wire = serde_json::to_value(&token).unwrap();
        assert_eq!(
            wire["additionalInfo"][0]["additionalIdToken"],
            json!("linked")
        );
        assert_eq!(wire["additionalInfo"][0]["type"], json!("VendorScheme"));
        let back: IdTokenType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn id_token_info_minimal_matches_wire_json() {
        let info = IdTokenInfoType {
            status: AuthorizationStatusEnumType::Accepted,
            cache_expiry_date_time: None,
            charging_priority: None,
            language1: None,
            evse_id: None,
            language2: None,
            group_id_token: None,
            personal_message: None,
            custom_data: None,
        };
        // Only the required `status` field appears — no nulls.
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            json!({ "status": "Accepted" })
        );
    }

    #[test]
    fn id_token_info_full_round_trips_with_nested_objects() {
        let info = IdTokenInfoType {
            status: AuthorizationStatusEnumType::Accepted,
            cache_expiry_date_time: Some("2030-01-01T00:00:00Z".to_string()),
            charging_priority: Some(5),
            language1: Some("en".to_string()),
            evse_id: Some(vec![1, 2]),
            language2: Some("nl".to_string()),
            group_id_token: Some(IdTokenType {
                id_token: "group-1".to_string(),
                kind: IdTokenEnumType::Central,
                additional_info: None,
                custom_data: None,
            }),
            personal_message: Some(MessageContentType {
                format: MessageFormatEnumType::Utf8,
                content: "Welcome".to_string(),
                language: Some("en".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&info).unwrap();
        assert_eq!(wire["chargingPriority"], json!(5));
        assert_eq!(wire["groupIdToken"]["idToken"], json!("group-1"));
        assert_eq!(wire["personalMessage"]["format"], json!("UTF8"));
        assert_eq!(wire["evseId"], json!([1, 2]));
        let back: IdTokenInfoType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, info);
    }

    #[test]
    fn transaction_event_enums_serialize_pascal_case() {
        assert_eq!(
            serde_json::to_value(TransactionEventEnumType::Started).unwrap(),
            json!("Started")
        );
        assert_eq!(
            serde_json::to_value(TriggerReasonEnumType::EVCommunicationLost).unwrap(),
            json!("EVCommunicationLost")
        );
        assert_eq!(
            serde_json::to_value(ChargingStateEnumType::SuspendedEVSE).unwrap(),
            json!("SuspendedEVSE")
        );
        assert_eq!(
            serde_json::to_value(ReasonEnumType::SOCLimitReached).unwrap(),
            json!("SOCLimitReached")
        );
    }

    #[test]
    fn transaction_type_omits_none_optionals() {
        let tx = TransactionType {
            transaction_id: "tx-001".to_string(),
            charging_state: None,
            time_spent_charging: None,
            stopped_reason: None,
            remote_start_id: None,
            custom_data: None,
        };
        assert_eq!(
            serde_json::to_value(&tx).unwrap(),
            json!({ "transactionId": "tx-001" })
        );
    }
}
