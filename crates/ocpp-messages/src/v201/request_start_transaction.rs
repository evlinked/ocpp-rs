//! `RequestStartTransaction` — the CSMS asks a Charging Station to start a
//! transaction (the 2.0.1 successor to 1.6J `RemoteStartTransaction`).
//!
//! Ports `ocpp.v201.call.RequestStartTransaction` /
//! `ocpp.v201.call_result.RequestStartTransaction`. The CSMS identifies the
//! driver via an [`IdTokenType`] and a `remoteStartId` it can later correlate
//! with the resulting transaction; it may target a specific `evseId` and supply
//! a `groupIdToken`.
//!
//! The reference's optional `charging_profile: Option<ChargingProfileType>`
//! field is **deferred** to a follow-up (#136): it pulls in the sizeable
//! `ChargingProfileType` → `ChargingScheduleType` datatype tree. Omitting it is
//! wire-correct — the field is optional — and the bundled
//! `RequestStartTransaction.json` schema still validates a `chargingProfile`
//! when a peer sends one, so adding the Rust field later is purely additive.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, IdTokenType, RequestStartStopStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `RequestStartTransaction.req` — sent by the CSMS to remotely start a
/// transaction.
///
/// Ports `ocpp.v201.call.RequestStartTransaction` (minus the deferred
/// `chargingProfile`, see module docs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestStartTransactionRequest {
    /// The driver/token the transaction is started for.
    #[serde(rename = "idToken")]
    pub id_token: IdTokenType,
    /// CSMS-assigned id echoed back so the station's later `TransactionEvent`
    /// can be correlated with this remote-start request.
    #[serde(rename = "remoteStartId")]
    pub remote_start_id: i32,
    /// Optional id of the EVSE on which to start; absent means the station
    /// chooses.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<i32>,
    /// Optional group/parent token associated with the driver token.
    #[serde(rename = "groupIdToken", skip_serializing_if = "Option::is_none")]
    pub group_id_token: Option<IdTokenType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for RequestStartTransactionRequest {
    const ACTION_NAME: &'static str = "RequestStartTransaction";
    type Response = RequestStartTransactionResponse;
}

/// `RequestStartTransaction.conf` — the Charging Station's reply, stating
/// whether it will start the transaction.
///
/// Ports `ocpp.v201.call_result.RequestStartTransaction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestStartTransactionResponse {
    /// Whether the station accepts the remote-start request.
    pub status: RequestStartStopStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Set when the transaction had already started (e.g. the cable was plugged
    /// in first); carries the id of that already-running transaction.
    #[serde(rename = "transactionId", skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for RequestStartTransactionResponse {
    const ACTION_NAME: &'static str = "RequestStartTransactionResponse";
    type Response = Self;
}

impl OcppResponse for RequestStartTransactionResponse {}
