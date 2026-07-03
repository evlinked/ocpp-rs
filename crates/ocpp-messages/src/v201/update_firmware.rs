//! `UpdateFirmware` — the CSMS tells a Charging Station to download and install
//! a firmware image.
//!
//! Ports `ocpp.v201.call.UpdateFirmware` / `ocpp.v201.call_result.UpdateFirmware`.
//! The request carries a [`FirmwareType`] (download `location`, retrieve/install
//! timestamps, and an optional signing certificate + signature) plus a
//! `requestId` that correlates the whole rollout to the progress reports the
//! station streams back via [`crate::v201::FirmwareStatusNotificationRequest`].
//! It is the install/trigger counterpart to that asynchronous progress half, and
//! is distinct from the publish-to-Local-Controller family
//! ([`crate::v201::PublishFirmwareRequest`]). The response is a single
//! [`UpdateFirmwareStatusEnumType`] acknowledgement.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, FirmwareType, StatusInfoType, UpdateFirmwareStatusEnumType,
};
use serde::{Deserialize, Serialize};

/// `UpdateFirmware.req` — sent by the CSMS to have a Charging Station download
/// and install a firmware image.
///
/// Ports `ocpp.v201.call.UpdateFirmware`. `retries` / `retryInterval` are
/// optional download-retry tuning; when absent the Charging Station decides how
/// many times and how long to wait between attempts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateFirmwareRequest {
    /// How many times to retry the download before giving up. Absent leaves the
    /// count to the Charging Station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i32>,
    /// Seconds to wait between download retries. Absent leaves the interval to
    /// the Charging Station.
    #[serde(rename = "retryInterval", skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<i32>,
    /// Correlation id echoed back on the asynchronous
    /// `FirmwareStatusNotification` messages that report install progress.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// The firmware image to download and install.
    pub firmware: FirmwareType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for UpdateFirmwareRequest {
    const ACTION_NAME: &'static str = "UpdateFirmware";
    type Response = UpdateFirmwareResponse;
}

/// `UpdateFirmware.conf` — the Charging Station's synchronous acknowledgement of
/// whether it accepted the firmware-update request.
///
/// Ports `ocpp.v201.call_result.UpdateFirmware`. The actual download/install
/// progress is reported asynchronously via `FirmwareStatusNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateFirmwareResponse {
    /// Whether the Charging Station accepted the firmware-update request.
    pub status: UpdateFirmwareStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for UpdateFirmwareResponse {
    const ACTION_NAME: &'static str = "UpdateFirmwareResponse";
    type Response = Self;
}

impl OcppResponse for UpdateFirmwareResponse {}
