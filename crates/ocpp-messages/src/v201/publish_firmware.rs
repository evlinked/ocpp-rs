//! `PublishFirmware` — the CSMS tells a Local Controller to download a firmware
//! image once and cache it locally.
//!
//! Ports `ocpp.v201.call.PublishFirmware` / `ocpp.v201.call_result.PublishFirmware`.
//! The Local Controller downloads the image from `location`, verifies it against
//! `checksum`, and caches it so the chargers behind it can later fetch it over
//! the LAN instead of each pulling it from the CSMS over the WAN. It is the
//! trigger for the firmware-publish family:
//! [`crate::v201::PublishFirmwareStatusNotificationRequest`] reports progress,
//! correlated back by `requestId`. The response is a synchronous
//! [`GenericStatusEnumType`] acknowledgement.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, GenericStatusEnumType, StatusInfoType};
use serde::{Deserialize, Serialize};

/// `PublishFirmware.req` — sent by the CSMS to have a Local Controller download
/// and cache a firmware image.
///
/// Ports `ocpp.v201.call.PublishFirmware`. `retries` / `retryInterval` are
/// optional download-retry tuning; when absent the Local Controller decides how
/// many times and how long to wait between attempts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishFirmwareRequest {
    /// URI the Local Controller downloads the firmware image from (schema
    /// `maxLength: 512`).
    pub location: String,
    /// How many times to retry the download before giving up. Absent leaves the
    /// count to the Local Controller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i32>,
    /// MD5 checksum over the entire firmware file, as a 32-char hex string
    /// (schema `maxLength: 32`).
    pub checksum: String,
    /// Correlation id echoed back on the asynchronous
    /// `PublishFirmwareStatusNotification` messages.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Seconds to wait between download retries. Absent leaves the interval to
    /// the Local Controller.
    #[serde(rename = "retryInterval", skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for PublishFirmwareRequest {
    const ACTION_NAME: &'static str = "PublishFirmware";
    type Response = PublishFirmwareResponse;
}

/// `PublishFirmware.conf` — the Local Controller's synchronous acknowledgement
/// of whether it accepted the publish request.
///
/// Ports `ocpp.v201.call_result.PublishFirmware`. The actual download/publish
/// progress is reported asynchronously via `PublishFirmwareStatusNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishFirmwareResponse {
    /// Whether the Local Controller accepted the publish request.
    pub status: GenericStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for PublishFirmwareResponse {
    const ACTION_NAME: &'static str = "PublishFirmwareResponse";
    type Response = Self;
}

impl OcppResponse for PublishFirmwareResponse {}
