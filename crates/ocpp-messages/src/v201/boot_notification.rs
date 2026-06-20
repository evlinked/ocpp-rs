//! `BootNotification` — sent by a Charging Station to the CSMS on boot.
//!
//! Ports `ocpp.v201.call.BootNotification` / `ocpp.v201.call_result.BootNotification`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    BootReasonEnumType, ChargingStationType, CustomDataType, RegistrationStatusEnumType,
    StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `BootNotification.req` — sent by a Charging Station to the CSMS on boot.
///
/// Ports `ocpp.v201.call.BootNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootNotificationRequest {
    /// Identity and capabilities of the booting Charging Station.
    #[serde(rename = "chargingStation")]
    pub charging_station: ChargingStationType,
    /// Why the Charging Station is sending this message.
    pub reason: BootReasonEnumType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for BootNotificationRequest {
    const ACTION_NAME: &'static str = "BootNotification";
    type Response = BootNotificationResponse;
}

/// `BootNotification.conf` — the CSMS's reply.
///
/// Ports `ocpp.v201.call_result.BootNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootNotificationResponse {
    /// The CSMS's current time (RFC 3339 / ISO 8601).
    #[serde(rename = "currentTime")]
    pub current_time: String,
    /// Heartbeat interval in seconds when `status` is `Accepted`; otherwise the
    /// minimum wait before the next `BootNotification`.
    pub interval: i32,
    /// Whether the Charging Station was accepted by the CSMS.
    pub status: RegistrationStatusEnumType,
    /// Optional detail about the registration result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for BootNotificationResponse {
    const ACTION_NAME: &'static str = "BootNotificationResponse";
    type Response = Self;
}

impl OcppResponse for BootNotificationResponse {}
