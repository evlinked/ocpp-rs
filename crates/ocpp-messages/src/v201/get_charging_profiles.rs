//! `GetChargingProfiles` — the CSMS asks a Charging Station to report which
//! charging profiles it currently has installed.
//!
//! Ports `ocpp.v201.call.GetChargingProfiles` /
//! `ocpp.v201.call_result.GetChargingProfiles`. It is the query **trigger** of
//! the OCPP 2.0.1 charging-profile report flow: the CSMS narrows the query with
//! a [`ChargingProfileCriterionType`] (purpose, stack level, profile ids, limit
//! sources) and optionally an `evse_id`; the station answers synchronously with
//! a [`GetChargingProfileStatusEnumType`] (`Accepted` when it has matching
//! profiles, `NoProfiles` when it does not) and then streams the actual profile
//! data asynchronously via one or more `ReportChargingProfiles.req`, correlated
//! back to this request by `request_id`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ChargingProfileCriterionType, CustomDataType, GetChargingProfileStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetChargingProfiles.req` — sent by the CSMS to enumerate the charging
/// profiles installed on a station.
///
/// Ports `ocpp.v201.call.GetChargingProfiles`. `request_id` correlates the
/// asynchronous `ReportChargingProfiles` report(s) back to this query and
/// `charging_profile` narrows which profiles the station reports (an empty `{}`
/// criterion reports every installed profile). The optional `evse_id` restricts
/// the report to a single EVSE: `0` targets only the station-wide profiles (the
/// grid connection), and omitting it reports profiles on every EVSE.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetChargingProfilesRequest {
    /// The id of this request, echoed back by the station on each
    /// `ReportChargingProfiles` report so the CSMS can correlate them.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Restrict the report to a single EVSE; `0` targets only the station-wide
    /// profiles. Omitted means "report profiles on every EVSE".
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<i32>,
    /// The criterion selecting which installed profiles to report; an empty
    /// `{}` criterion matches every installed profile.
    #[serde(rename = "chargingProfile")]
    pub charging_profile: ChargingProfileCriterionType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetChargingProfilesRequest {
    const ACTION_NAME: &'static str = "GetChargingProfiles";
    type Response = GetChargingProfilesResponse;
}

/// `GetChargingProfiles.conf` — the station's synchronous acknowledgement of the
/// query.
///
/// Ports `ocpp.v201.call_result.GetChargingProfiles`. `status` is the only
/// required field: `Accepted` means the station has one or more matching
/// profiles (streamed afterwards via `ReportChargingProfiles`), `NoProfiles`
/// means it has none. This response carries no profile data itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetChargingProfilesResponse {
    /// Whether the station has charging profiles matching the request criterion.
    pub status: GetChargingProfileStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetChargingProfilesResponse {
    const ACTION_NAME: &'static str = "GetChargingProfilesResponse";
    type Response = Self;
}

impl OcppResponse for GetChargingProfilesResponse {}
