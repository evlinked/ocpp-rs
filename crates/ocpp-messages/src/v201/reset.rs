//! `Reset` — the CSMS asks a Charging Station (or a single EVSE) to reset.
//!
//! Ports `ocpp.v201.call.Reset` / `ocpp.v201.call_result.Reset`. This is the
//! 2.0.1 successor to 1.6J `Reset`: instead of a `Hard`/`Soft` distinction the
//! request carries a [`ResetEnumType`] (`Immediate` / `OnIdle`) and may target a
//! specific `evseId` rather than the whole station.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, ResetEnumType, ResetStatusEnumType, StatusInfoType};
use serde::{Deserialize, Serialize};

/// `Reset.req` — sent by the CSMS to reset a Charging Station or a single EVSE.
///
/// Ports `ocpp.v201.call.Reset`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResetRequest {
    /// The kind of reset to perform. Named `kind` because `type` is a Rust
    /// keyword; serialized as `"type"` to match the wire format.
    #[serde(rename = "type")]
    pub kind: ResetEnumType,
    /// Optional ID of a specific EVSE to reset, instead of the entire Charging
    /// Station.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ResetRequest {
    const ACTION_NAME: &'static str = "Reset";
    type Response = ResetResponse;
}

/// `Reset.conf` — the Charging Station's reply, stating whether it can perform
/// the reset.
///
/// Ports `ocpp.v201.call_result.Reset`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResetResponse {
    /// Whether the Charging Station can perform the reset (`Accepted`,
    /// `Rejected`, or `Scheduled` when deferred).
    pub status: ResetStatusEnumType,
    /// Optional detail about the reset result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ResetResponse {
    const ACTION_NAME: &'static str = "ResetResponse";
    type Response = Self;
}

impl OcppResponse for ResetResponse {}
