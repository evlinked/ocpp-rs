//! `GetTransactionStatus` — the CSMS asks the Charging Station whether a given
//! transaction is still ongoing and whether it still has queued messages
//! pending delivery (used to coordinate drain/shutdown).
//!
//! Ports `ocpp.v201.call.GetTransactionStatus` /
//! `ocpp.v201.call_result.GetTransactionStatus`. The request carries an optional
//! `transactionId` (absent → station-wide message-queue state); the response
//! reports the required `messagesInQueue` flag plus an optional
//! `ongoingIndicator`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::CustomDataType;
use serde::{Deserialize, Serialize};

/// `GetTransactionStatus.req` — sent by the CSMS to query the status of a
/// transaction (or, when `transaction_id` is absent, the station's overall
/// message-queue state).
///
/// Ports `ocpp.v201.call.GetTransactionStatus`. With no fields set it
/// serializes to `{}` on the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GetTransactionStatusRequest {
    /// The id of the transaction whose status is requested. When omitted, the
    /// query concerns the station's queued-message state as a whole.
    #[serde(rename = "transactionId", skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetTransactionStatusRequest {
    const ACTION_NAME: &'static str = "GetTransactionStatus";
    type Response = GetTransactionStatusResponse;
}

/// `GetTransactionStatus.conf` — the Charging Station's reply.
///
/// Ports `ocpp.v201.call_result.GetTransactionStatus`. `messages_in_queue`
/// reports whether messages still await delivery; `ongoing_indicator` (when
/// present) reports whether the queried transaction is still active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetTransactionStatusResponse {
    /// Whether there are still messages to be delivered for the transaction.
    #[serde(rename = "messagesInQueue")]
    pub messages_in_queue: bool,
    /// Whether the transaction is still ongoing. Absent when the station does
    /// not report it (e.g. a station-wide query with no specific transaction).
    #[serde(rename = "ongoingIndicator", skip_serializing_if = "Option::is_none")]
    pub ongoing_indicator: Option<bool>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetTransactionStatusResponse {
    const ACTION_NAME: &'static str = "GetTransactionStatusResponse";
    type Response = Self;
}

impl OcppResponse for GetTransactionStatusResponse {}
