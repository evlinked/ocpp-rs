//! `TriggerMessage` — the CSMS asks a Charging Station to proactively send a
//! specific message *now* (e.g. a fresh `BootNotification`, `StatusNotification`,
//! or `MeterValues`), optionally scoped to a single EVSE.
//!
//! Ports `ocpp.v201.call.TriggerMessage` /
//! `ocpp.v201.call_result.TriggerMessage`. This is the 2.0.1 successor to 1.6J
//! `TriggerMessage`: the request carries a [`MessageTriggerEnumType`] and, when
//! `evse` is omitted, targets the whole Charging Station rather than a single
//! EVSE. Reuses the shared [`EvseType`] and [`StatusInfoType`]; only the two
//! enums are new.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, EvseType, MessageTriggerEnumType, StatusInfoType, TriggerMessageStatusEnumType,
};
use serde::{Deserialize, Serialize};

/// `TriggerMessage.req` — sent by the CSMS to prompt a Charging Station to emit
/// a specific message immediately.
///
/// Ports `ocpp.v201.call.TriggerMessage`. When `evse` is `None` the trigger
/// applies to the whole Charging Station; when present it scopes the request to
/// that EVSE (e.g. a per-EVSE `StatusNotification`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerMessageRequest {
    /// The message the Charging Station should send next.
    #[serde(rename = "requestedMessage")]
    pub requested_message: MessageTriggerEnumType,
    /// The specific EVSE (and optionally connector) to scope the trigger to.
    /// Omit to target the whole Charging Station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evse: Option<EvseType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for TriggerMessageRequest {
    const ACTION_NAME: &'static str = "TriggerMessage";
    type Response = TriggerMessageResponse;
}

/// `TriggerMessage.conf` — the Charging Station's reply, stating whether it will
/// send the requested message.
///
/// Ports `ocpp.v201.call_result.TriggerMessage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerMessageResponse {
    /// Whether the station will send the message (`Accepted`), refuses
    /// (`Rejected`), or recognizes but cannot trigger it (`NotImplemented`).
    pub status: TriggerMessageStatusEnumType,
    /// Optional detail about the result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for TriggerMessageResponse {
    const ACTION_NAME: &'static str = "TriggerMessageResponse";
    type Response = Self;
}

impl OcppResponse for TriggerMessageResponse {}
