//! `UnpublishFirmware` — the CSMS tells a Local Controller to drop a
//! previously-published firmware image from its local cache.
//!
//! Ports `ocpp.v201.call.UnpublishFirmware` /
//! `ocpp.v201.call_result.UnpublishFirmware`. It is the teardown counterpart to
//! the firmware-publish family whose trigger is
//! [`crate::v201::PublishFirmwareRequest`]: the image is identified by the same
//! MD5 `checksum` used when it was published, and the Local Controller answers
//! synchronously with an [`UnpublishFirmwareStatusEnumType`] saying whether it
//! removed the image, found none matching, or is still mid-publish.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, UnpublishFirmwareStatusEnumType};
use serde::{Deserialize, Serialize};

/// `UnpublishFirmware.req` — sent by the CSMS to have a Local Controller remove
/// a cached firmware image identified by its checksum.
///
/// Ports `ocpp.v201.call.UnpublishFirmware`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnpublishFirmwareRequest {
    /// MD5 checksum over the entire firmware file, as a 32-char hex string
    /// (schema `maxLength: 32`). Identifies which cached image to remove.
    pub checksum: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for UnpublishFirmwareRequest {
    const ACTION_NAME: &'static str = "UnpublishFirmware";
    type Response = UnpublishFirmwareResponse;
}

/// `UnpublishFirmware.conf` — the Local Controller's synchronous report of
/// whether it unpublished the requested image.
///
/// Ports `ocpp.v201.call_result.UnpublishFirmware`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnpublishFirmwareResponse {
    /// Whether the image was removed, no matching image was found, or a publish
    /// is still ongoing.
    pub status: UnpublishFirmwareStatusEnumType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for UnpublishFirmwareResponse {
    const ACTION_NAME: &'static str = "UnpublishFirmwareResponse";
    type Response = Self;
}

impl OcppResponse for UnpublishFirmwareResponse {}
