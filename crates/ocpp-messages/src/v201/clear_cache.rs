//! `ClearCache` — the CSMS asks a Charging Station to wipe its local
//! authorization cache.
//!
//! Ports `ocpp.v201.call.ClearCache` / `ocpp.v201.call_result.ClearCache`. This
//! is the 2.0.1 successor to 1.6J `ClearCache`: one of the smallest command
//! messages — an empty request, a single status out.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{ClearCacheStatusEnumType, CustomDataType, StatusInfoType};
use serde::{Deserialize, Serialize};

/// `ClearCache.req` — sent by the CSMS to wipe the Charging Station's local
/// authorization cache.
///
/// Ports `ocpp.v201.call.ClearCache`. The request carries no fields beyond the
/// optional vendor extension, so it serializes to `{}` on the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClearCacheRequest {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearCacheRequest {
    const ACTION_NAME: &'static str = "ClearCache";
    type Response = ClearCacheResponse;
}

/// `ClearCache.conf` — the Charging Station's reply, stating whether it cleared
/// the cache.
///
/// Ports `ocpp.v201.call_result.ClearCache`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearCacheResponse {
    /// `Accepted` if the Charging Station executed the request, otherwise
    /// `Rejected`.
    pub status: ClearCacheStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearCacheResponse {
    const ACTION_NAME: &'static str = "ClearCacheResponse";
    type Response = Self;
}

impl OcppResponse for ClearCacheResponse {}
