//! `StatusNotification` — reports the status of a single connector.
//!
//! Ports `ocpp.v201.call.StatusNotification` /
//! `ocpp.v201.call_result.StatusNotification`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{ConnectorStatusEnumType, CustomDataType};
use serde::{Deserialize, Serialize};

/// `StatusNotification.req` — reports the status of a single connector.
///
/// Ports `ocpp.v201.call.StatusNotification`. Unlike 1.6J, status is reported
/// per `(evseId, connectorId)` pair using [`ConnectorStatusEnumType`], and
/// there is no `errorCode` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusNotificationRequest {
    /// The time for which the status is reported (RFC 3339 / ISO 8601).
    pub timestamp: String,
    /// The reported status of the connector.
    #[serde(rename = "connectorStatus")]
    pub connector_status: ConnectorStatusEnumType,
    /// The id of the EVSE to which the connector belongs.
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// The id of the connector within the EVSE.
    #[serde(rename = "connectorId")]
    pub connector_id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for StatusNotificationRequest {
    const ACTION_NAME: &'static str = "StatusNotification";
    type Response = StatusNotificationResponse;
}

/// `StatusNotification.conf` — the CSMS's acknowledgement.
///
/// Ports `ocpp.v201.call_result.StatusNotification`. The response carries no
/// fields beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StatusNotificationResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for StatusNotificationResponse {
    const ACTION_NAME: &'static str = "StatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for StatusNotificationResponse {}
