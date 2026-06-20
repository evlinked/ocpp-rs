//! `Heartbeat` — keeps the connection alive and learns the CSMS's current time.
//!
//! Ports `ocpp.v201.call.Heartbeat` / `ocpp.v201.call_result.Heartbeat`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::CustomDataType;
use serde::{Deserialize, Serialize};

/// `Heartbeat.req` — sent by a Charging Station to keep the connection alive
/// and to learn the CSMS's current time.
///
/// Ports `ocpp.v201.call.Heartbeat`. The request carries no fields beyond the
/// optional vendor extension, so it serializes to `{}` on the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for HeartbeatRequest {
    const ACTION_NAME: &'static str = "Heartbeat";
    type Response = HeartbeatResponse;
}

/// `Heartbeat.conf` — the CSMS's reply, carrying its current time.
///
/// Ports `ocpp.v201.call_result.Heartbeat`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// The CSMS's current time (RFC 3339 / ISO 8601).
    #[serde(rename = "currentTime")]
    pub current_time: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for HeartbeatResponse {
    const ACTION_NAME: &'static str = "HeartbeatResponse";
    type Response = Self;
}

impl OcppResponse for HeartbeatResponse {}
