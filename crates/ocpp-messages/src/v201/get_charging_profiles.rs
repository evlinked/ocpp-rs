//! `GetChargingProfiles` — the CSMS asks a Charging Station to report which
//! charging profiles it currently has installed.
//!
//! Ports `ocpp.v201.call.GetChargingProfiles` /
//! `ocpp.v201.call_result.GetChargingProfiles`. It is the query **trigger** of
//! the OCPP 2.0.1 charging-profile report flow: the CSMS narrows the query with
//! a [`ChargingProfileCriterionType`] (purpose, stack level, profile ids, limit
//! sources) and optionally an `evse_id`; the station answers synchronously with
//! a [`GetChargingProfileStatusEnumType`] (`Accepted` when it has matches,
//! `NoProfiles` when it has none) and then streams the matching profiles
//! asynchronously via one or more `ReportChargingProfiles.req`, correlated back
//! to this request by `request_id`.
//!
//! [`ChargingProfileCriterionType`]: ocpp_types::v201::ChargingProfileCriterionType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ChargingProfileCriterionType, CustomDataType, GetChargingProfileStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetChargingProfiles.req` — sent by the CSMS to enumerate the charging
/// profiles installed on a station.
///
/// Ports `ocpp.v201.call.GetChargingProfiles`. `request_id` correlates the
/// asynchronous `ReportChargingProfiles` report(s) back to this query, and
/// `charging_profile` carries the filter criteria. `evse_id` optionally scopes
/// the query to a single EVSE — `0` means only profiles installed on the
/// Charging Station itself (the grid connection); when omitted, all installed
/// profiles are reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetChargingProfilesRequest {
    /// The id of this request, echoed back by the station on each
    /// `ReportChargingProfiles` report so the CSMS can correlate them.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// The criteria narrowing which installed profiles the station reports.
    #[serde(rename = "chargingProfile")]
    pub charging_profile: ChargingProfileCriterionType,
    /// Restrict the report to profiles installed on this EVSE; `0` means the
    /// Charging Station itself, and `None` means all EVSEs.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<i32>,
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
    /// Whether the station has charging profiles matching the request criteria.
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

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{ChargingLimitSourceEnumType, ChargingProfilePurposeEnumType};
    use serde_json::json;

    fn full_criterion() -> ChargingProfileCriterionType {
        ChargingProfileCriterionType {
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            stack_level: Some(3),
            charging_profile_id: Some(vec![1, 2, 3]),
            charging_limit_source: Some(vec![
                ChargingLimitSourceEnumType::Ems,
                ChargingLimitSourceEnumType::Cso,
            ]),
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        // Only `requestId` + `chargingProfile` are required; an empty criterion
        // (match everything) and absent `evseId` / `customData` stay off the wire.
        let req = GetChargingProfilesRequest {
            request_id: 42,
            charging_profile: ChargingProfileCriterionType {
                charging_profile_purpose: None,
                stack_level: None,
                charging_profile_id: None,
                charging_limit_source: None,
                custom_data: None,
            },
            evse_id: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value, json!({ "requestId": 42, "chargingProfile": {} }));
        let parsed: GetChargingProfilesRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_full_criterion() {
        let req = GetChargingProfilesRequest {
            request_id: 7,
            charging_profile: full_criterion(),
            evse_id: Some(0),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(
            value["chargingProfile"]["chargingProfilePurpose"],
            json!("TxDefaultProfile")
        );
        assert_eq!(value["chargingProfile"]["stackLevel"], json!(3));
        assert_eq!(
            value["chargingProfile"]["chargingProfileId"],
            json!([1, 2, 3])
        );
        assert_eq!(
            value["chargingProfile"]["chargingLimitSource"],
            json!(["EMS", "CSO"])
        );
        assert_eq!(value["evseId"], json!(0));
        let parsed: GetChargingProfilesRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let value = serde_json::to_value(GetChargingProfilesRequest {
            request_id: -3,
            charging_profile: full_criterion(),
            evse_id: Some(2),
            custom_data: None,
        })
        .unwrap();
        assert!(value["requestId"].is_i64());
        assert!(value["evseId"].is_i64());
        assert!(value["chargingProfile"]["stackLevel"].is_i64());
        assert!(value["chargingProfile"]["chargingProfileId"].is_array());
        assert!(value["chargingProfile"]["chargingProfileId"][0].is_i64());
        assert!(value["chargingProfile"]["chargingLimitSource"].is_array());
    }

    #[test]
    fn response_round_trips() {
        let resp = GetChargingProfilesResponse {
            status: GetChargingProfileStatusEnumType::NoProfiles,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "NoProfiles" }));
        let parsed: GetChargingProfilesResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn status_enum_serializes_to_exact_wire_values() {
        for (variant, wire) in [
            (GetChargingProfileStatusEnumType::Accepted, "Accepted"),
            (GetChargingProfileStatusEnumType::NoProfiles, "NoProfiles"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: GetChargingProfileStatusEnumType =
                serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn request_missing_required_field_fails() {
        // `chargingProfile` is required.
        let err = serde_json::from_value::<GetChargingProfilesRequest>(json!({ "requestId": 1 }))
            .unwrap_err();
        assert!(err.to_string().contains("chargingProfile"));
    }

    #[test]
    fn request_rejects_unknown_charging_limit_source() {
        let err = serde_json::from_value::<GetChargingProfilesRequest>(json!({
            "requestId": 1,
            "chargingProfile": { "chargingLimitSource": ["Nope"] }
        }))
        .unwrap_err();
        assert!(err.to_string().contains("Nope") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            GetChargingProfilesRequest::ACTION_NAME,
            "GetChargingProfiles"
        );
        assert_eq!(
            GetChargingProfilesResponse::ACTION_NAME,
            "GetChargingProfilesResponse"
        );
    }
}
