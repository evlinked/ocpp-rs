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

/// One entry in a `SetVariables` request: the value to write to a single
/// component-variable attribute.
///
/// Ports `SetVariableDataType`. `attribute_value`, `component`, and `variable`
/// are required; `attribute_type` defaults to [`AttributeEnumType::Actual`]
/// when omitted. The write-path counterpart to [`GetVariableDataType`], which
/// carries no value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVariableDataType {
    /// Value to assign to the attribute (max length 1000 per schema).
    #[serde(rename = "attributeValue")]
    pub attribute_value: String,
    /// Component for which the variable is set.
    pub component: ComponentType,
    /// Variable whose attribute value is set.
    pub variable: VariableType,
    /// Attribute to write; absent means `Actual`.
    #[serde(rename = "attributeType", skip_serializing_if = "Option::is_none")]
    pub attribute_type: Option<AttributeEnumType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// One entry in a `SetVariables` response: the outcome of writing a single
/// component-variable attribute.
///
/// Ports `SetVariableResultType`. `attribute_status`, `component`, and
/// `variable` are required; `attribute_status_info` carries detail about a
/// non-`Accepted` status. The write-path counterpart to
/// [`GetVariableResultType`] — it echoes no value, only a status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVariableResultType {
    /// Result status of setting the variable.
    #[serde(rename = "attributeStatus")]
    pub attribute_status: SetVariableStatusEnumType,
    /// Component for which the variable was set.
    pub component: ComponentType,
    /// Variable whose attribute value was set.
    pub variable: VariableType,
    /// Attribute that was written; absent means `Actual`.
    #[serde(rename = "attributeType", skip_serializing_if = "Option::is_none")]
    pub attribute_type: Option<AttributeEnumType>,
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

/// A single entry in a Local Authorization List: the identifier to authorize
/// plus, optionally, the cached status to associate with it.
///
/// Ports `AuthorizationData`. Carried by `SendLocalList.req`. Only `idToken` is
/// required; in a differential update an entry whose `idTokenInfo` is absent
/// signals removal of that token from the list (reusing the already-ported
/// [`IdTokenType`] / [`IdTokenInfoType`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizationData {
    /// The identifier this entry authorizes.
    #[serde(rename = "idToken")]
    pub id_token: IdTokenType,
    /// Cached authorization status for the token. Absent in a differential
    /// update means "remove this token from the list".
    #[serde(rename = "idTokenInfo", skip_serializing_if = "Option::is_none")]
    pub id_token_info: Option<IdTokenInfoType>,
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

/// A cryptographically signed version of a meter reading, carried inside a
/// [`SampledValueType`].
///
/// Ports `SignedMeterValueType`. All four fields are required by the schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedMeterValueType {
    /// Base64-encoded signed data, which may contain more than the meter value
    /// (timestamps, customer reference, …) (max length 2500).
    #[serde(rename = "signedMeterData")]
    pub signed_meter_data: String,
    /// Method used to create the digital signature (max length 50).
    #[serde(rename = "signingMethod")]
    pub signing_method: String,
    /// Method used to encode the meter values before signing (max length 50).
    #[serde(rename = "encodingMethod")]
    pub encoding_method: String,
    /// Base64-encoded public key (max length 2500).
    #[serde(rename = "publicKey")]
    pub public_key: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// The unit and decimal multiplier qualifying a [`SampledValueType`] value.
///
/// Ports `UnitOfMeasureType`. Both fields are optional; when absent the value
/// defaults to `"Wh"` with multiplier `0`. `unit` is modelled as a free
/// `String` because the reference allows either a standardized unit or a
/// custom one (`Union[StandardizedUnitsOfMeasureType, str]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitOfMeasureType {
    /// Unit of the value (default `"Wh"`; max length 20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Exponent to base 10 applied to the value (default `0`). A multiplier of
    /// `3` means the value is scaled by 10³.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A single sampled measurement within a [`MeterValueType`].
///
/// Ports `SampledValueType`. Only `value` is required; with every optional
/// field absent the reading is interpreted as active-import energy in Wh, per
/// the spec's documented defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampledValueType {
    /// The measured value.
    pub value: f64,
    /// What the reading represents (start/end/sample/…); default
    /// `Sample.Periodic`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ReadingContextEnumType>,
    /// Kind of measurement; default `Energy.Active.Import.Register`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurand: Option<MeasurandEnumType>,
    /// Electrical phase the value applies to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<PhaseEnumType>,
    /// Where the value was sampled; default `Outlet`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationEnumType>,
    /// Signed representation of this meter value, if provided.
    #[serde(rename = "signedMeterValue", skip_serializing_if = "Option::is_none")]
    pub signed_meter_value: Option<SignedMeterValueType>,
    /// Unit and multiplier qualifying `value`, if provided.
    #[serde(rename = "unitOfMeasure", skip_serializing_if = "Option::is_none")]
    pub unit_of_measure: Option<UnitOfMeasureType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A collection of one or more [`SampledValueType`]s captured at the same
/// instant, as reported in `MeterValues` and `TransactionEvent`.
///
/// Ports `MeterValueType`. Both `timestamp` and a non-empty `sampledValue`
/// list are required by the schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterValueType {
    /// Time at which all of `sampledValue` were sampled (RFC 3339 / ISO 8601).
    pub timestamp: String,
    /// The sampled values; the schema requires at least one.
    #[serde(rename = "sampledValue")]
    pub sampled_value: Vec<SampledValueType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A single cost component of a [`ConsumptionCostType`] block in a sales
/// tariff.
///
/// Ports `CostType`. `costKind` and `amount` are required; `amountMultiplier`
/// (a power-of-ten exponent applied to `amount`, range −3..3) is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostType {
    /// The kind of cost this amount represents.
    #[serde(rename = "costKind")]
    pub cost_kind: CostKindEnumType,
    /// The cost amount, scaled by `amountMultiplier` when present.
    pub amount: i32,
    /// Power-of-ten multiplier applied to `amount` (e.g. `-2` → amount/100).
    #[serde(rename = "amountMultiplier", skip_serializing_if = "Option::is_none")]
    pub amount_multiplier: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// The cost(s) associated with a consumption band of a [`SalesTariffEntryType`].
///
/// Ports `ConsumptionCostType`. Both `startValue` and a non-empty `cost` list
/// are required by the schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsumptionCostType {
    /// Lower bound of the consumption range this cost applies to.
    #[serde(rename = "startValue")]
    pub start_value: f64,
    /// The cost components for this consumption band; at least one is required.
    pub cost: Vec<CostType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A time interval expressed relative to the start of the parent schedule, used
/// by a [`SalesTariffEntryType`].
///
/// Ports `RelativeTimeIntervalType`. `start` (seconds from schedule start) is
/// required; `duration` (seconds) is optional and, when absent, means the
/// interval runs until the next entry begins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelativeTimeIntervalType {
    /// Offset in seconds from the start of the schedule.
    pub start: i32,
    /// Duration of the interval in seconds, if bounded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// One entry of a [`SalesTariffType`], pricing a relative time interval.
///
/// Ports `SalesTariffEntryType`. Only `relativeTimeInterval` is required;
/// `ePriceLevel` (a price tier index, 0 = cheapest) and `consumptionCost`
/// (consumption-banded costs) are optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SalesTariffEntryType {
    /// The time interval, relative to the schedule start, this entry prices.
    #[serde(rename = "relativeTimeInterval")]
    pub relative_time_interval: RelativeTimeIntervalType,
    /// Price level index for this entry (lower = cheaper), if used.
    #[serde(rename = "ePriceLevel", skip_serializing_if = "Option::is_none")]
    pub e_price_level: Option<i32>,
    /// Consumption-banded costs for this entry, if any.
    #[serde(rename = "consumptionCost", skip_serializing_if = "Option::is_none")]
    pub consumption_cost: Option<Vec<ConsumptionCostType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A sales tariff attached to a [`ChargingScheduleType`], conveying time- and
/// consumption-based pricing to the EV (ISO 15118).
///
/// Ports `SalesTariffType`. `id` and a non-empty `salesTariffEntry` list are
/// required; `salesTariffDescription` (≤32 chars) and `numEPriceLevels` are
/// optional metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SalesTariffType {
    /// Identifier of this sales tariff, unique within the schedule.
    pub id: i32,
    /// The tariff entries; the schema requires at least one.
    #[serde(rename = "salesTariffEntry")]
    pub sales_tariff_entry: Vec<SalesTariffEntryType>,
    /// Human-readable description of the tariff (≤32 chars), if provided.
    #[serde(
        rename = "salesTariffDescription",
        skip_serializing_if = "Option::is_none"
    )]
    pub sales_tariff_description: Option<String>,
    /// Number of distinct price levels used across all entries, if provided.
    #[serde(rename = "numEPriceLevels", skip_serializing_if = "Option::is_none")]
    pub num_e_price_levels: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// One period of a [`ChargingScheduleType`]: a charging limit that takes effect
/// at `startPeriod` and holds until the next period begins.
///
/// Ports `ChargingSchedulePeriodType`. `startPeriod` (seconds from the schedule
/// start) and `limit` (in the schedule's `chargingRateUnit`) are required;
/// `numberPhases` and `phaseToUse` refine which phases the limit applies to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingSchedulePeriodType {
    /// Offset in seconds from the schedule start at which this limit applies.
    #[serde(rename = "startPeriod")]
    pub start_period: i32,
    /// The charging rate limit, in the schedule's `chargingRateUnit`.
    pub limit: f64,
    /// Number of phases the limit applies to (1–3); default 3 when absent.
    #[serde(rename = "numberPhases", skip_serializing_if = "Option::is_none")]
    pub number_phases: Option<i32>,
    /// Which phase to use for a single-phase limit on a 3-phase connection.
    #[serde(rename = "phaseToUse", skip_serializing_if = "Option::is_none")]
    pub phase_to_use: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A charging schedule within a [`ChargingProfileType`]: an ordered set of
/// limit periods in a given rate unit, optionally with a sales tariff.
///
/// Ports `ChargingScheduleType`. `id`, `chargingRateUnit` and a non-empty
/// `chargingSchedulePeriod` list are required; `startSchedule`, `duration`,
/// `minChargingRate` and `salesTariff` are optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingScheduleType {
    /// Identifier of this schedule, unique within the profile.
    pub id: i32,
    /// Unit the period limits are expressed in (watts or amperes).
    #[serde(rename = "chargingRateUnit")]
    pub charging_rate_unit: ChargingRateUnitEnumType,
    /// The limit periods; the schema requires at least one.
    #[serde(rename = "chargingSchedulePeriod")]
    pub charging_schedule_period: Vec<ChargingSchedulePeriodType>,
    /// Absolute start of the schedule (RFC 3339), for `Absolute`/`Recurring`
    /// profiles.
    #[serde(rename = "startSchedule", skip_serializing_if = "Option::is_none")]
    pub start_schedule: Option<String>,
    /// Duration of the schedule in seconds, if bounded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
    /// Minimum charging rate the EV may always draw, in `chargingRateUnit`.
    #[serde(rename = "minChargingRate", skip_serializing_if = "Option::is_none")]
    pub min_charging_rate: Option<f64>,
    /// Optional sales tariff conveyed alongside the schedule.
    #[serde(rename = "salesTariff", skip_serializing_if = "Option::is_none")]
    pub sales_tariff: Option<SalesTariffType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A charging profile: the station's limit on charging power/current over time,
/// carried in `RequestStartTransaction` / `SetChargingProfile`.
///
/// Ports `ChargingProfileType`. `id`, `stackLevel`, `chargingProfilePurpose`,
/// `chargingProfileKind` and a non-empty `chargingSchedule` list are required;
/// `recurrencyKind`, `validFrom`/`validTo` and `transactionId` are optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingProfileType {
    /// Identifier of this profile.
    pub id: i32,
    /// Priority within the profile stack; higher levels override lower ones.
    #[serde(rename = "stackLevel")]
    pub stack_level: i32,
    /// Where in the station's stack this profile applies.
    #[serde(rename = "chargingProfilePurpose")]
    pub charging_profile_purpose: ChargingProfilePurposeEnumType,
    /// Whether the schedule is absolute, relative, or recurring.
    #[serde(rename = "chargingProfileKind")]
    pub charging_profile_kind: ChargingProfileKindEnumType,
    /// The schedule(s) making up the profile; the schema requires at least one.
    #[serde(rename = "chargingSchedule")]
    pub charging_schedule: Vec<ChargingScheduleType>,
    /// Recurrence period for a `Recurring` profile, if applicable.
    #[serde(rename = "recurrencyKind", skip_serializing_if = "Option::is_none")]
    pub recurrency_kind: Option<RecurrencyKindEnumType>,
    /// Point in time the profile becomes valid (RFC 3339), if bounded.
    #[serde(rename = "validFrom", skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    /// Point in time the profile stops being valid (RFC 3339), if bounded.
    #[serde(rename = "validTo", skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    /// Transaction this profile is tied to, for `TxProfile` purposes.
    #[serde(rename = "transactionId", skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A component together with (optionally) a specific variable within it, used to
/// narrow a `GetReport` request to a subset of the device model.
///
/// Ports `ComponentVariableType`. Only `component` is required; when `variable`
/// is omitted the whole component is referenced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentVariableType {
    /// Component that is referenced.
    pub component: ComponentType,
    /// Variable within the component; absent means the whole component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable: Option<VariableType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A single active monitor configured on a component-variable, carried by
/// [`MonitoringDataType`] inside a `NotifyMonitoringReport`.
///
/// Ports `VariableMonitoringType`. Every field except `custom_data` is
/// required. `value` is a threshold/delta magnitude for
/// [`MonitorEnumType::UpperThreshold`] / [`MonitorEnumType::LowerThreshold`] /
/// [`MonitorEnumType::Delta`], and an interval in seconds for
/// [`MonitorEnumType::Periodic`] / [`MonitorEnumType::PeriodicClockAligned`].
/// `severity` runs 0 (highest) to 9 (lowest).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableMonitoringType {
    /// Identifies the monitor.
    pub id: i32,
    /// Whether the monitor is only active while a transaction is ongoing on a
    /// component relevant to this transaction.
    pub transaction: bool,
    /// Threshold/delta magnitude, or the interval in seconds for periodic
    /// monitors.
    pub value: f64,
    /// The kind of monitor this is.
    #[serde(rename = "type")]
    pub kind: MonitorEnumType,
    /// Severity assigned to events this monitor triggers (0 = highest, 9 =
    /// lowest).
    pub severity: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// The set of monitors configured on one component-variable, one entry in a
/// `NotifyMonitoringReport`'s `monitor` list.
///
/// Ports `MonitoringDataType`. `component`, `variable`, and
/// `variable_monitoring` are required; the schema also caps
/// `variable_monitoring` at a minimum of one item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitoringDataType {
    /// Component the monitored variable belongs to.
    pub component: ComponentType,
    /// The monitored variable.
    pub variable: VariableType,
    /// The monitors active on this component-variable. The schema requires at
    /// least one item.
    #[serde(rename = "variableMonitoring")]
    pub variable_monitoring: Vec<VariableMonitoringType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// One monitor to install via a `SetVariableMonitoring` request, targeting a
/// single component-variable.
///
/// Ports `SetMonitoringDataType`. `value`, `kind` (`type`), `severity`,
/// `component`, and `variable` are required; `id` is supplied only to *replace*
/// an existing monitor (the Charging Station assigns ids for new monitors), and
/// `transaction` defaults to `false`. `value` is a threshold/delta magnitude for
/// [`MonitorEnumType::UpperThreshold`] / [`MonitorEnumType::LowerThreshold`] /
/// [`MonitorEnumType::Delta`], or an interval in seconds for
/// [`MonitorEnumType::Periodic`] / [`MonitorEnumType::PeriodicClockAligned`].
/// `severity` runs 0 (highest) to 9 (lowest).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMonitoringDataType {
    /// Set only to replace an existing monitor; omitted for new monitors, whose
    /// id the Charging Station assigns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    /// Whether the monitor is only active while a transaction is ongoing on a
    /// component relevant to this transaction. Defaults to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<bool>,
    /// Threshold/delta magnitude, or the interval in seconds for periodic
    /// monitors.
    pub value: f64,
    /// The kind of monitor to install.
    #[serde(rename = "type")]
    pub kind: MonitorEnumType,
    /// Severity assigned to events this monitor triggers (0 = highest, 9 =
    /// lowest).
    pub severity: i32,
    /// Component the monitored variable belongs to.
    pub component: ComponentType,
    /// The variable to monitor.
    pub variable: VariableType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// The per-monitor outcome of a `SetVariableMonitoring` request, one entry in
/// the response's `setMonitoringResult` list.
///
/// Ports `SetMonitoringResultType`. `status`, `kind` (`type`), `severity`,
/// `component`, and `variable` are required; `id` is returned only when
/// `status` is [`SetMonitoringStatusEnumType::Accepted`], and `status_info`
/// carries optional detail. The echoed `kind` / `severity` / `component` /
/// `variable` correlate each result back to its requested
/// [`SetMonitoringDataType`] entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMonitoringResultType {
    /// The id assigned to the monitor; returned only when `status` is
    /// [`SetMonitoringStatusEnumType::Accepted`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    /// Whether the monitor was installed, or why it was rejected.
    pub status: SetMonitoringStatusEnumType,
    /// The kind of monitor this result refers to (echoed from the request).
    #[serde(rename = "type")]
    pub kind: MonitorEnumType,
    /// Component the monitored variable belongs to (echoed from the request).
    pub component: ComponentType,
    /// The monitored variable (echoed from the request).
    pub variable: VariableType,
    /// Severity of the monitor this result refers to (echoed from the request).
    pub severity: i32,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}
