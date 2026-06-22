//! `SendLocalList` — the CSMS pushes a (full or differential) Local
//! Authorization List to a Charging Station.
//!
//! Ports `ocpp.v201.call.SendLocalList` /
//! `ocpp.v201.call_result.SendLocalList`. The write-path companion to
//! [`GetLocalListVersion`](super::GetLocalListVersionRequest) (read the current
//! version) and [`ClearCache`](super::ClearCacheRequest) (wipe the cache).

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    AuthorizationData, CustomDataType, SendLocalListStatusEnumType, StatusInfoType, UpdateEnumType,
};
use serde::{Deserialize, Serialize};

/// `SendLocalList.req` — sent by the CSMS to install or update the Local
/// Authorization List on a Charging Station.
///
/// Ports `ocpp.v201.call.SendLocalList`. `version_number` carries the version
/// of the full list (for a `Full` update) or of the list *after* the update is
/// applied (for a `Differential` update). `local_authorization_list` is
/// optional — a `Full` update with no list clears the station's list, and a
/// differential update may add, change, or (via entries without `idTokenInfo`)
/// remove tokens. The schema requires at least one entry when the list is
/// present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendLocalListRequest {
    /// Version number of the list (full list, or the post-update version for a
    /// differential update).
    #[serde(rename = "versionNumber")]
    pub version_number: i32,
    /// Whether this is a full replacement or a differential update.
    #[serde(rename = "updateType")]
    pub update_type: UpdateEnumType,
    /// The authorization entries. Omitted entirely when absent; the schema
    /// requires at least one item when present.
    #[serde(
        rename = "localAuthorizationList",
        skip_serializing_if = "Option::is_none"
    )]
    pub local_authorization_list: Option<Vec<AuthorizationData>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SendLocalListRequest {
    const ACTION_NAME: &'static str = "SendLocalList";
    type Response = SendLocalListResponse;
}

/// `SendLocalList.conf` — the Charging Station's reply, stating whether it
/// received and applied the update.
///
/// Ports `ocpp.v201.call_result.SendLocalList`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendLocalListResponse {
    /// Whether the update was `Accepted`, `Failed`, or rejected with a
    /// `VersionMismatch`.
    pub status: SendLocalListStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SendLocalListResponse {
    const ACTION_NAME: &'static str = "SendLocalListResponse";
    type Response = Self;
}

impl OcppResponse for SendLocalListResponse {}
