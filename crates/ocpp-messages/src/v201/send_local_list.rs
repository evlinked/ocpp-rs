//! `SendLocalList` — the CSMS pushes a (full or differential) Local
//! Authorization List to the Charging Station.
//!
//! Ports `ocpp.v201.call.SendLocalList` / `ocpp.v201.call_result.SendLocalList`.
//! This is the write companion to `GetLocalListVersion` (reads the station's
//! current list version) and `ClearCache` (wipes the authorization cache). The
//! request carries a `versionNumber`, an `updateType` (`Full`/`Differential`),
//! and an optional `localAuthorizationList` of [`AuthorizationData`] entries —
//! which reuse the already-ported [`IdTokenType`](ocpp_types::v201::IdTokenType)
//! / [`IdTokenInfoType`](ocpp_types::v201::IdTokenInfoType).

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    AuthorizationData, CustomDataType, SendLocalListStatusEnumType, StatusInfoType, UpdateEnumType,
};
use serde::{Deserialize, Serialize};

/// `SendLocalList.req` — sent by the CSMS to install or update the Charging
/// Station's Local Authorization List.
///
/// Ports `ocpp.v201.call.SendLocalList`. For a `Full` update `versionNumber` is
/// the version of the complete list; for a `Differential` update it is the
/// resulting version after the changes are applied. The
/// `local_authorization_list` is omitted entirely when absent (a `Full` update
/// with no entries clears the list); per the FINAL schema, when present it must
/// contain at least one entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendLocalListRequest {
    /// Version number of the list (after the update has been applied).
    #[serde(rename = "versionNumber")]
    pub version_number: i32,
    /// Whether this is a full replacement or a differential update.
    #[serde(rename = "updateType")]
    pub update_type: UpdateEnumType,
    /// The authorization entries to install. Omitted on the wire when absent;
    /// when present the schema requires at least one entry.
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

/// `SendLocalList.conf` — the Charging Station's verdict on the update.
///
/// Ports `ocpp.v201.call_result.SendLocalList`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendLocalListResponse {
    /// Whether the station received and applied the list update.
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
