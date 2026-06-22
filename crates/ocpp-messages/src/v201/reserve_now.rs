//! `ReserveNow` — the CSMS asks a Charging Station to hold an EVSE/connector for
//! a driver until an expiry time.
//!
//! Ports `ocpp.v201.call.ReserveNow` / `ocpp.v201.call_result.ReserveNow`. This
//! is the 2.0.1 successor to 1.6J `ReserveNow` and the explicit companion to
//! [`CancelReservation`](super) — the other half of the reservation pair. It
//! reuses the already-ported [`IdTokenType`] (from `Authorize`) and
//! [`StatusInfoType`]; the only genuinely new surface is the
//! [`ConnectorEnumType`] and [`ReserveNowStatusEnumType`] enums.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ConnectorEnumType, CustomDataType, IdTokenType, ReserveNowStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `ReserveNow.req` — sent by the CSMS to reserve an EVSE (or the whole Charging
/// Station, when `evse_id` is omitted) for a specific `idToken` until
/// `expiry_date_time`.
///
/// Ports `ocpp.v201.call.ReserveNow`. `connector_type` narrows the reservation
/// to a particular plug standard; `group_id_token` reserves on behalf of a token
/// group. All three plus `evse_id` are optional and omitted from the wire when
/// absent. Timestamps follow the crate-wide convention of an ISO-8601 `String`
/// (mirroring `BootNotificationResponse::current_time`, transaction
/// timestamps, …) rather than a typed `DateTime`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReserveNowRequest {
    /// Id of the reservation.
    pub id: i32,
    /// Date and time at which the reservation expires (ISO-8601).
    #[serde(rename = "expiryDateTime")]
    pub expiry_date_time: String,
    /// The identifier the reservation is held for.
    #[serde(rename = "idToken")]
    pub id_token: IdTokenType,
    /// The connector type to reserve. Omit to leave the connector unconstrained.
    #[serde(rename = "connectorType", skip_serializing_if = "Option::is_none")]
    pub connector_type: Option<ConnectorEnumType>,
    /// The EVSE to reserve. Omit to reserve at the Charging Station level.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<i32>,
    /// A group identifier the reservation may also be used by.
    #[serde(rename = "groupIdToken", skip_serializing_if = "Option::is_none")]
    pub group_id_token: Option<IdTokenType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ReserveNowRequest {
    const ACTION_NAME: &'static str = "ReserveNow";
    type Response = ReserveNowResponse;
}

/// `ReserveNow.conf` — the Charging Station's reply, stating whether the
/// reservation was made.
///
/// Ports `ocpp.v201.call_result.ReserveNow`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReserveNowResponse {
    /// Whether the reservation succeeded (`Accepted`) or why it did not
    /// (`Faulted` / `Occupied` / `Rejected` / `Unavailable`).
    pub status: ReserveNowStatusEnumType,
    /// Optional detail about the result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ReserveNowResponse {
    const ACTION_NAME: &'static str = "ReserveNowResponse";
    type Response = Self;
}

impl OcppResponse for ReserveNowResponse {}
