//! `ClearChargingProfile` — the CSMS removes installed charging profiles from a
//! Charging Station, the teardown counterpart to `SetChargingProfile`.
//!
//! Ports `ocpp.v201.call.ClearChargingProfile` /
//! `ocpp.v201.call_result.ClearChargingProfile`. The request either names a
//! single profile by `charging_profile_id`, or supplies a
//! [`ClearChargingProfileType`] *filter* (`evseId` / `chargingProfilePurpose` /
//! `stackLevel`) matching a set of profiles — or is entirely empty (`{}`),
//! meaning "clear every profile". Nothing in the request is required. The
//! station replies with a single [`ClearChargingProfileStatusEnumType`]:
//! `Accepted` if it cleared at least one matching profile, `Unknown` if nothing
//! matched.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ClearChargingProfileStatusEnumType, ClearChargingProfileType, CustomDataType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `ClearChargingProfile.req` — sent by the CSMS to remove installed charging
/// profiles.
///
/// Ports `ocpp.v201.call.ClearChargingProfile`. Both fields are optional: set
/// `charging_profile_id` to clear one specific profile, or
/// `charging_profile_criteria` to clear every profile matching the filter.
/// Leaving both `None` (an empty `{}` on the wire) clears **all** profiles.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClearChargingProfileRequest {
    /// Id of a single charging profile to clear.
    #[serde(rename = "chargingProfileId", skip_serializing_if = "Option::is_none")]
    pub charging_profile_id: Option<i32>,
    /// Filter selecting which profiles to clear; omitted means "match all".
    #[serde(
        rename = "chargingProfileCriteria",
        skip_serializing_if = "Option::is_none"
    )]
    pub charging_profile_criteria: Option<ClearChargingProfileType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearChargingProfileRequest {
    const ACTION_NAME: &'static str = "ClearChargingProfile";
    type Response = ClearChargingProfileResponse;
}

/// `ClearChargingProfile.conf` — the Charging Station's reply, stating whether
/// any profile was cleared.
///
/// Ports `ocpp.v201.call_result.ClearChargingProfile`. `status` is `Accepted`
/// when at least one profile matched the request and was removed, or `Unknown`
/// when nothing matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearChargingProfileResponse {
    /// Whether a matching profile was cleared (`Accepted`) or none matched
    /// (`Unknown`).
    pub status: ClearChargingProfileStatusEnumType,
    /// Optional detail about the result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearChargingProfileResponse {
    const ACTION_NAME: &'static str = "ClearChargingProfileResponse";
    type Response = Self;
}

impl OcppResponse for ClearChargingProfileResponse {}
