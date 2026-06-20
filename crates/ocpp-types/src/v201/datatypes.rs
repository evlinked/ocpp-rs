//! OCPP 2.0.1 shared `*Type` datatypes.
//!
//! Ports the datatype dataclasses from the OCPP 2.0.1 specification
//! (mobilityhouse/ocpp `ocpp/v201/datatypes.py`). Mirrors the conventions of
//! [`crate::v16j`]: serde with explicit camelCase renames and
//! `skip_serializing_if` on every optional field so absent values never appear
//! on the wire.
//!
//! Enum references resolve through the sibling [`super::enums`] module; the
//! whole set is re-exported from [`super`] so the public path stays
//! `ocpp_types::v201::*`.

use serde::{Deserialize, Serialize};

use super::enums::*;

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
