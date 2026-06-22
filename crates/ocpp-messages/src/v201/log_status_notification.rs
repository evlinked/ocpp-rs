//! `LogStatusNotification` — the Charging Station reports progress of a
//! diagnostics/security log upload (driven by `GetLog`) back to the CSMS.
//!
//! Ports `ocpp.v201.call.LogStatusNotification` /
//! `ocpp.v201.call_result.LogStatusNotification`. The request carries a single
//! [`UploadLogStatusEnumType`] plus an optional `requestId` correlating it to
//! the triggering `GetLogRequest`; the response is empty (only the optional
//! vendor extension), so it serializes to `{}`. Directly mirrors
//! [`crate::v201::FirmwareStatusNotificationRequest`].

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, UploadLogStatusEnumType};
use serde::{Deserialize, Serialize};

/// `LogStatusNotification.req` — progress report sent by the Charging Station
/// while a log upload proceeds.
///
/// Ports `ocpp.v201.call.LogStatusNotification`. `request_id` echoes the id
/// from the `GetLogRequest` that started the upload; it is absent when the
/// notification was produced by a `TriggerMessageRequest` with no log upload
/// ongoing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogStatusNotificationRequest {
    /// The current stage of the log-upload lifecycle.
    pub status: UploadLogStatusEnumType,
    /// The `requestId` provided in the `GetLogRequest` that started this
    /// upload, when applicable.
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for LogStatusNotificationRequest {
    const ACTION_NAME: &'static str = "LogStatusNotification";
    type Response = LogStatusNotificationResponse;
}

/// `LogStatusNotification.conf` — the CSMS's acknowledgement.
///
/// Ports `ocpp.v201.call_result.LogStatusNotification`. The response carries no
/// fields beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LogStatusNotificationResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for LogStatusNotificationResponse {
    const ACTION_NAME: &'static str = "LogStatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for LogStatusNotificationResponse {}
