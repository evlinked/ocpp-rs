//! `FirmwareStatusNotification` — the Charging Station reports progress of a
//! firmware update (driven by `UpdateFirmware`) back to the CSMS.
//!
//! Ports `ocpp.v201.call.FirmwareStatusNotification` /
//! `ocpp.v201.call_result.FirmwareStatusNotification`. The request carries a
//! single [`FirmwareStatusEnumType`] plus an optional `requestId` correlating it
//! to the triggering `UpdateFirmwareRequest`; the response is empty (only the
//! optional vendor extension), so it serializes to `{}`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, FirmwareStatusEnumType};
use serde::{Deserialize, Serialize};

/// `FirmwareStatusNotification.req` — progress report sent by the Charging
/// Station while a firmware update proceeds.
///
/// Ports `ocpp.v201.call.FirmwareStatusNotification`. `request_id` echoes the
/// id from the `UpdateFirmwareRequest` that started the update; it is absent
/// when the notification was produced by a `TriggerMessageRequest` with no
/// firmware update ongoing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirmwareStatusNotificationRequest {
    /// The current stage of the firmware download/install lifecycle.
    pub status: FirmwareStatusEnumType,
    /// The `requestId` provided in the `UpdateFirmwareRequest` that started
    /// this update, when applicable.
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for FirmwareStatusNotificationRequest {
    const ACTION_NAME: &'static str = "FirmwareStatusNotification";
    type Response = FirmwareStatusNotificationResponse;
}

/// `FirmwareStatusNotification.conf` — the CSMS's acknowledgement.
///
/// Ports `ocpp.v201.call_result.FirmwareStatusNotification`. The response
/// carries no fields beyond the optional vendor extension, so it serializes to
/// `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FirmwareStatusNotificationResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for FirmwareStatusNotificationResponse {
    const ACTION_NAME: &'static str = "FirmwareStatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for FirmwareStatusNotificationResponse {}
