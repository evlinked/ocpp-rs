//! `GetLocalListVersion` — asks the Charging Station which version of its Local
//! Authorization List it currently holds.
//!
//! Ports `ocpp.v201.call.GetLocalListVersion` /
//! `ocpp.v201.call_result.GetLocalListVersion`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::CustomDataType;
use serde::{Deserialize, Serialize};

/// `GetLocalListVersion.req` — sent by the CSMS to learn the version number of
/// the Local Authorization List currently stored on the Charging Station.
///
/// Ports `ocpp.v201.call.GetLocalListVersion`. The request carries no fields
/// beyond the optional vendor extension, so it serializes to `{}` on the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GetLocalListVersionRequest {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetLocalListVersionRequest {
    const ACTION_NAME: &'static str = "GetLocalListVersion";
    type Response = GetLocalListVersionResponse;
}

/// `GetLocalListVersion.conf` — the Charging Station's reply, carrying the
/// current version number of its Local Authorization List.
///
/// Ports `ocpp.v201.call_result.GetLocalListVersion`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetLocalListVersionResponse {
    /// The current version number of the Local Authorization List in the
    /// Charging Station.
    #[serde(rename = "versionNumber")]
    pub version_number: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetLocalListVersionResponse {
    const ACTION_NAME: &'static str = "GetLocalListVersionResponse";
    type Response = Self;
}

impl OcppResponse for GetLocalListVersionResponse {}
