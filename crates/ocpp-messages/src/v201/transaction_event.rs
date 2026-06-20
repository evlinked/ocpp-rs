//! `TransactionEvent` — the unified 2.0.1 transaction message.
//!
//! Ports `ocpp.v201.call.TransactionEvent` /
//! `ocpp.v201.call_result.TransactionEvent`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, EvseType, IdTokenInfoType, IdTokenType, MessageContentType,
    TransactionEventEnumType, TransactionType, TriggerReasonEnumType,
};
use serde::{Deserialize, Serialize};

/// `TransactionEvent.req` — the unified 2.0.1 transaction message that replaces
/// the 1.6J `StartTransaction` / `StopTransaction` / `MeterValues` triad.
///
/// Ports `ocpp.v201.call.TransactionEvent`. A transaction is reported as a
/// sequence of events: one `Started`, zero or more `Updated`, and one `Ended`
/// (see [`TransactionEventEnumType`]).
///
/// **Scope:** this slice omits the optional `meterValue` field (the
/// `MeterValueType` / `SampledValueType` sub-objects and their measurement
/// enums); it is deferred to a follow-up (tracked on the issue). The bundled
/// schema still validates `meterValue` when present, so adding it later is
/// purely additive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionEventRequest {
    /// Which event in the transaction's lifecycle this is.
    #[serde(rename = "eventType")]
    pub event_type: TransactionEventEnumType,
    /// The time at which the event occurred (RFC 3339 / ISO 8601).
    pub timestamp: String,
    /// What triggered this event.
    #[serde(rename = "triggerReason")]
    pub trigger_reason: TriggerReasonEnumType,
    /// Sequence number, incrementing per event within the transaction so the
    /// CSMS can detect gaps and order events received out of sequence.
    #[serde(rename = "seqNo")]
    pub seq_no: i32,
    /// State of the transaction this event belongs to.
    #[serde(rename = "transactionInfo")]
    pub transaction_info: TransactionType,
    /// Whether the Charging Station was offline when the event occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline: Option<bool>,
    /// Number of electrical phases used, if relevant.
    #[serde(rename = "numberOfPhasesUsed", skip_serializing_if = "Option::is_none")]
    pub number_of_phases_used: Option<i32>,
    /// Maximum current of the cable in amperes, if reported.
    #[serde(rename = "cableMaxCurrent", skip_serializing_if = "Option::is_none")]
    pub cable_max_current: Option<i32>,
    /// Reservation this transaction terminated, if any.
    #[serde(rename = "reservationId", skip_serializing_if = "Option::is_none")]
    pub reservation_id: Option<i32>,
    /// The EVSE (and optionally connector) for which the event is reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evse: Option<EvseType>,
    /// The identifier that authorized the transaction.
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<IdTokenType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for TransactionEventRequest {
    const ACTION_NAME: &'static str = "TransactionEvent";
    type Response = TransactionEventResponse;
}

/// `TransactionEvent.conf` — the CSMS's reply.
///
/// Ports `ocpp.v201.call_result.TransactionEvent`. Every field is optional, so
/// an empty acknowledgement serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TransactionEventResponse {
    /// Running total cost of the transaction in the configured currency.
    #[serde(rename = "totalCost", skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    /// Charging priority granted to this transaction (-9..=9).
    #[serde(rename = "chargingPriority", skip_serializing_if = "Option::is_none")]
    pub charging_priority: Option<i32>,
    /// Updated authorization status for the transaction's identifier.
    #[serde(rename = "idTokenInfo", skip_serializing_if = "Option::is_none")]
    pub id_token_info: Option<IdTokenInfoType>,
    /// Personal message to display on the Charging Station.
    #[serde(
        rename = "updatedPersonalMessage",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_personal_message: Option<MessageContentType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for TransactionEventResponse {
    const ACTION_NAME: &'static str = "TransactionEventResponse";
    type Response = Self;
}

impl OcppResponse for TransactionEventResponse {}
