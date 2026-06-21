//! `ChangeAvailability` — the CSMS asks a Charging Station (or a single EVSE) to
//! go Operative or Inoperative.
//!
//! Ports `ocpp.v201.call.ChangeAvailability` /
//! `ocpp.v201.call_result.ChangeAvailability`. This is the 2.0.1 successor to
//! 1.6J `ChangeAvailability`: the request carries an
//! [`OperationalStatusEnumType`] and, when `evse` is omitted, targets the whole
//! Charging Station rather than a single EVSE/connector.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ChangeAvailabilityStatusEnumType, CustomDataType, EvseType, OperationalStatusEnumType,
    StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `ChangeAvailability.req` — sent by the CSMS to make a Charging Station or a
/// single EVSE Operative or Inoperative.
///
/// Ports `ocpp.v201.call.ChangeAvailability`. When `evse` is `None` the command
/// targets the entire Charging Station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeAvailabilityRequest {
    /// The availability change to perform (`Operative` / `Inoperative`).
    #[serde(rename = "operationalStatus")]
    pub operational_status: OperationalStatusEnumType,
    /// The specific EVSE (and optionally connector) to target. Omit to target
    /// the whole Charging Station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evse: Option<EvseType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ChangeAvailabilityRequest {
    const ACTION_NAME: &'static str = "ChangeAvailability";
    type Response = ChangeAvailabilityResponse;
}

/// `ChangeAvailability.conf` — the Charging Station's reply, stating whether it
/// can perform the availability change.
///
/// Ports `ocpp.v201.call_result.ChangeAvailability`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeAvailabilityResponse {
    /// Whether the Charging Station can perform the change (`Accepted`,
    /// `Rejected`, or `Scheduled` when deferred).
    pub status: ChangeAvailabilityStatusEnumType,
    /// Optional detail about the result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ChangeAvailabilityResponse {
    const ACTION_NAME: &'static str = "ChangeAvailabilityResponse";
    type Response = Self;
}

impl OcppResponse for ChangeAvailabilityResponse {}
