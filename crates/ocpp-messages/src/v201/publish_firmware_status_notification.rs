//! `PublishFirmwareStatusNotification` — a Local Controller reports progress of
//! a firmware *publish* (driven by `PublishFirmware`) back to the CSMS.
//!
//! Ports `ocpp.v201.call.PublishFirmwareStatusNotification` /
//! `ocpp.v201.call_result.PublishFirmwareStatusNotification`. The request
//! carries a single [`PublishFirmwareStatusEnumType`], an optional list of
//! download `location` URIs, and an optional `requestId` correlating it to the
//! triggering `PublishFirmwareRequest`; the response is empty (only the
//! optional vendor extension), so it serializes to `{}`. Mirrors
//! [`crate::v201::FirmwareStatusNotificationRequest`], adding the `location`
//! list.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, PublishFirmwareStatusEnumType};
use serde::{Deserialize, Serialize};

/// `PublishFirmwareStatusNotification.req` — progress report sent by the Local
/// Controller while a firmware publish proceeds.
///
/// Ports `ocpp.v201.call.PublishFirmwareStatusNotification`. `request_id`
/// echoes the id from the `PublishFirmwareRequest` that started the publish; it
/// is absent when the notification was produced by a `TriggerMessageRequest`
/// with no publish ongoing. `location` lists the URIs the published image can
/// be downloaded from; per the spec it is required only when `status` is
/// [`PublishFirmwareStatusEnumType::Published`], which is enforced at the
/// application layer rather than the schema, so the field stays optional here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishFirmwareStatusNotificationRequest {
    /// The current stage of the firmware publish lifecycle.
    pub status: PublishFirmwareStatusEnumType,
    /// The URIs from which the published firmware image can be downloaded.
    /// Present (non-empty) when `status` is `Published`; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Vec<String>>,
    /// The `requestId` provided in the `PublishFirmwareRequest` that started
    /// this publish, when applicable.
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for PublishFirmwareStatusNotificationRequest {
    const ACTION_NAME: &'static str = "PublishFirmwareStatusNotification";
    type Response = PublishFirmwareStatusNotificationResponse;
}

/// `PublishFirmwareStatusNotification.conf` — the CSMS's acknowledgement.
///
/// Ports `ocpp.v201.call_result.PublishFirmwareStatusNotification`. The
/// response carries no fields beyond the optional vendor extension, so it
/// serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PublishFirmwareStatusNotificationResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for PublishFirmwareStatusNotificationResponse {
    const ACTION_NAME: &'static str = "PublishFirmwareStatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for PublishFirmwareStatusNotificationResponse {}
