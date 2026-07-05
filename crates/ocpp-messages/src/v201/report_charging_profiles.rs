//! `ReportChargingProfiles` — the Charging Station streams the *actual*
//! charging-profile data that a [`GetChargingProfiles`] request asked for.
//!
//! Ports `ocpp.v201.call.ReportChargingProfiles` /
//! `ocpp.v201.call_result.ReportChargingProfiles`. It is the asynchronous data
//! half of the OCPP 2.0.1 charging-profile **report** flow: after a station
//! answers `GetChargingProfiles` synchronously with `Accepted`, it sends one or
//! more `ReportChargingProfiles.req` messages — paged via `tbc` — each
//! correlated back to the triggering request by `request_id` and tagged with
//! the `charging_limit_source` the profiles came from. Structurally the
//! near-twin of [`NotifyDisplayMessages`] (paged carrier, empty response) but
//! its payload embeds the already-ported [`ChargingProfileType`] tree rather
//! than display messages. The response is empty.
//!
//! [`GetChargingProfiles`]: https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py
//! [`NotifyDisplayMessages`]: super::NotifyDisplayMessagesRequest
//! [`ChargingProfileType`]: ocpp_types::v201::ChargingProfileType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{ChargingLimitSourceEnumType, ChargingProfileType, CustomDataType};
use serde::{Deserialize, Serialize};

/// `ReportChargingProfiles.req` — a single (possibly partial) page of the
/// charging profiles a station currently has installed.
///
/// Ports `ocpp.v201.call.ReportChargingProfiles`. `request_id` echoes the
/// `GetChargingProfiles` that triggered the report so the CSMS can correlate the
/// pages; `tbc` ("to be continued") is `true` while more pages follow. Every
/// field except `tbc` and `custom_data` is required, and `charging_profile`
/// holds at least one item (schema `minItems: 1`). `evse_id` `0` means the
/// profiles carry an overall limit for the whole Charging Station rather than a
/// single EVSE.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportChargingProfilesRequest {
    /// The id of the `GetChargingProfiles` request that requested this report.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Source that installed the reported profiles (`EMS` / `Other` / `SO` /
    /// `CSO`).
    #[serde(rename = "chargingLimitSource")]
    pub charging_limit_source: ChargingLimitSourceEnumType,
    /// The reported charging profiles for this page; the schema requires at
    /// least one item.
    #[serde(rename = "chargingProfile")]
    pub charging_profile: Vec<ChargingProfileType>,
    /// The EVSE the profiles apply to (`0` = station-wide / overall limit).
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// "To be continued" — `true` when another page follows in an upcoming
    /// `ReportChargingProfiles`. Absent means the default `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbc: Option<bool>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ReportChargingProfilesRequest {
    const ACTION_NAME: &'static str = "ReportChargingProfiles";
    type Response = ReportChargingProfilesResponse;
}

/// `ReportChargingProfiles.conf` — the CSMS's empty acknowledgement.
///
/// Ports `ocpp.v201.call_result.ReportChargingProfiles`. It carries no fields
/// beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReportChargingProfilesResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ReportChargingProfilesResponse {
    const ACTION_NAME: &'static str = "ReportChargingProfilesResponse";
    type Response = Self;
}

impl OcppResponse for ReportChargingProfilesResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        ChargingProfileKindEnumType, ChargingProfilePurposeEnumType, ChargingRateUnitEnumType,
        ChargingSchedulePeriodType, ChargingScheduleType,
    };
    use serde_json::json;

    /// The smallest valid profile: the five required fields, one schedule with
    /// one period. Mirrors the reused [`ChargingProfileType`] shape.
    fn minimal_profile() -> ChargingProfileType {
        ChargingProfileType {
            id: 1,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeEnumType::TxDefaultProfile,
            charging_profile_kind: ChargingProfileKindEnumType::Absolute,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: ChargingRateUnitEnumType::A,
                charging_schedule_period: vec![ChargingSchedulePeriodType {
                    start_period: 0,
                    limit: 16.0,
                    number_phases: None,
                    phase_to_use: None,
                    custom_data: None,
                }],
                start_schedule: None,
                duration: None,
                min_charging_rate: None,
                sales_tariff: None,
                custom_data: None,
            }],
            recurrency_kind: None,
            valid_from: None,
            valid_to: None,
            transaction_id: None,
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = ReportChargingProfilesRequest {
            request_id: 42,
            charging_limit_source: ChargingLimitSourceEnumType::Cso,
            charging_profile: vec![minimal_profile()],
            evse_id: 1,
            tbc: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional `tbc` / `customData` stay off the wire.
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("tbc"));
        assert!(!obj.contains_key("customData"));
        assert_eq!(value["requestId"], json!(42));
        assert_eq!(value["chargingLimitSource"], json!("CSO"));
        assert_eq!(value["evseId"], json!(1));
        assert_eq!(value["chargingProfile"][0]["id"], json!(1));
        let parsed: ReportChargingProfilesRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_multi_page() {
        // A `tbc: true` page announcing more profiles follow.
        let req = ReportChargingProfilesRequest {
            request_id: 7,
            charging_limit_source: ChargingLimitSourceEnumType::Ems,
            charging_profile: vec![minimal_profile()],
            evse_id: 0,
            tbc: Some(true),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["tbc"], json!(true));
        assert_eq!(value["evseId"], json!(0));
        assert_eq!(value["chargingLimitSource"], json!("EMS"));
        let parsed: ReportChargingProfilesRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let value = serde_json::to_value(ReportChargingProfilesRequest {
            request_id: -3,
            charging_limit_source: ChargingLimitSourceEnumType::So,
            charging_profile: vec![minimal_profile()],
            evse_id: 2,
            tbc: Some(false),
            custom_data: None,
        })
        .unwrap();
        assert!(value["requestId"].is_i64());
        assert!(value["evseId"].is_i64());
        assert!(value["tbc"].is_boolean());
        assert!(value["chargingProfile"].is_array());
        assert!(value["chargingProfile"][0]["stackLevel"].is_i64());
        assert_eq!(value["chargingLimitSource"], json!("SO"));
    }

    #[test]
    fn response_round_trips_empty() {
        let resp = ReportChargingProfilesResponse::default();
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({}));
        let parsed: ReportChargingProfilesResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn request_missing_required_field_fails() {
        // `chargingProfile` is required.
        let err = serde_json::from_value::<ReportChargingProfilesRequest>(json!({
            "requestId": 1,
            "chargingLimitSource": "CSO",
            "evseId": 0
        }))
        .unwrap_err();
        assert!(err.to_string().contains("chargingProfile"));
    }

    #[test]
    fn request_rejects_unknown_charging_limit_source() {
        let err = serde_json::from_value::<ReportChargingProfilesRequest>(json!({
            "requestId": 1,
            "chargingLimitSource": "Nope",
            "chargingProfile": [],
            "evseId": 0
        }))
        .unwrap_err();
        assert!(err.to_string().contains("Nope") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            ReportChargingProfilesRequest::ACTION_NAME,
            "ReportChargingProfiles"
        );
        assert_eq!(
            ReportChargingProfilesResponse::ACTION_NAME,
            "ReportChargingProfilesResponse"
        );
    }
}
