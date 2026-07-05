//! `NotifyChargingLimit` — the Charging Station reports that an external actor
//! (an EMS, system operator, or the CSO) has imposed a charging limit on it.
//!
//! Ports `ocpp.v201.call.NotifyChargingLimit` /
//! `ocpp.v201.call_result.NotifyChargingLimit`. This is the *notify* counterpart
//! to [`ClearedChargingLimit`](super::cleared_charging_limit): the pair frames
//! the lifecycle of an externally-set limit (imposed → cleared). A
//! [`ChargingLimitType`] plus an optional target `evseId` and the resulting
//! [`ChargingScheduleType`]s go in; an empty response comes out.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{ChargingLimitType, ChargingScheduleType, CustomDataType};
use serde::{Deserialize, Serialize};

/// `NotifyChargingLimit.req` — sent by the Charging Station when an external
/// charging limit has been imposed on it.
///
/// Ports `ocpp.v201.call.NotifyChargingLimit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyChargingLimitRequest {
    /// The limit that was imposed and its source.
    #[serde(rename = "chargingLimit")]
    pub charging_limit: ChargingLimitType,
    /// The EVSE the limit applies to. Per the spec `evseId` must be `> 0`;
    /// absent means the limit applies to the whole charging station.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<i32>,
    /// The resulting schedule(s), present when the station can express the
    /// limit as one or more charging schedules. The schema requires at least
    /// one entry when the field is present.
    #[serde(rename = "chargingSchedule", skip_serializing_if = "Option::is_none")]
    pub charging_schedule: Option<Vec<ChargingScheduleType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyChargingLimitRequest {
    const ACTION_NAME: &'static str = "NotifyChargingLimit";
    type Response = NotifyChargingLimitResponse;
}

/// `NotifyChargingLimit.conf` — the CSMS acknowledgement. The 2.0.1 schema
/// carries no fields beyond the optional vendor extension, so it serializes to
/// an empty object `{}`.
///
/// Ports `ocpp.v201.call_result.NotifyChargingLimit`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NotifyChargingLimitResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyChargingLimitResponse {
    const ACTION_NAME: &'static str = "NotifyChargingLimitResponse";
    type Response = Self;
}

impl OcppResponse for NotifyChargingLimitResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{ChargingLimitSourceEnumType, ChargingRateUnitEnumType};
    use serde_json::json;

    fn sample_charging_limit() -> ChargingLimitType {
        ChargingLimitType {
            charging_limit_source: ChargingLimitSourceEnumType::Ems,
            is_grid_critical: Some(true),
            custom_data: None,
        }
    }

    fn sample_schedule() -> ChargingScheduleType {
        ChargingScheduleType {
            id: 1,
            charging_rate_unit: ChargingRateUnitEnumType::A,
            charging_schedule_period: vec![ocpp_types::v201::ChargingSchedulePeriodType {
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
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = NotifyChargingLimitRequest {
            charging_limit: ChargingLimitType {
                charging_limit_source: ChargingLimitSourceEnumType::So,
                is_grid_critical: None,
                custom_data: None,
            },
            evse_id: None,
            charging_schedule: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Only `chargingLimit` is present; `isGridCritical`, `evseId`,
        // `chargingSchedule` and `customData` are all omitted when `None`.
        assert_eq!(
            value,
            json!({ "chargingLimit": { "chargingLimitSource": "SO" } })
        );
        let parsed: NotifyChargingLimitRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_full() {
        let req = NotifyChargingLimitRequest {
            charging_limit: sample_charging_limit(),
            evse_id: Some(2),
            charging_schedule: Some(vec![sample_schedule()]),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["chargingLimit"]["chargingLimitSource"], json!("EMS"));
        assert_eq!(value["chargingLimit"]["isGridCritical"], json!(true));
        assert_eq!(value["evseId"], json!(2));
        assert_eq!(value["chargingSchedule"][0]["id"], json!(1));
        assert_eq!(value["chargingSchedule"][0]["chargingRateUnit"], json!("A"));
        let parsed: NotifyChargingLimitRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn evse_id_round_trips_as_integer() {
        let req = NotifyChargingLimitRequest {
            charging_limit: sample_charging_limit(),
            evse_id: Some(7),
            charging_schedule: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert!(value["evseId"].is_i64());
        assert_eq!(value["evseId"], json!(7));
    }

    #[test]
    fn request_serializes_custom_data() {
        let req = NotifyChargingLimitRequest {
            charging_limit: sample_charging_limit(),
            evse_id: None,
            charging_schedule: None,
            custom_data: Some(CustomDataType {
                vendor_id: "ACME".to_string(),
                extra: Default::default(),
            }),
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["customData"]["vendorId"], json!("ACME"));
    }

    #[test]
    fn charging_limit_source_wire_values() {
        for (variant, wire) in [
            (ChargingLimitSourceEnumType::Ems, "EMS"),
            (ChargingLimitSourceEnumType::Other, "Other"),
            (ChargingLimitSourceEnumType::So, "SO"),
            (ChargingLimitSourceEnumType::Cso, "CSO"),
        ] {
            let req = NotifyChargingLimitRequest {
                charging_limit: ChargingLimitType {
                    charging_limit_source: variant,
                    is_grid_critical: None,
                    custom_data: None,
                },
                evse_id: None,
                charging_schedule: None,
                custom_data: None,
            };
            let value = serde_json::to_value(&req).unwrap();
            assert_eq!(value["chargingLimit"]["chargingLimitSource"], json!(wire));
        }
    }

    #[test]
    fn request_missing_charging_limit_fails() {
        let err = serde_json::from_value::<NotifyChargingLimitRequest>(json!({ "evseId": 1 }))
            .unwrap_err();
        assert!(err.to_string().contains("chargingLimit"));
    }

    #[test]
    fn request_rejects_unknown_source() {
        let err = serde_json::from_value::<NotifyChargingLimitRequest>(
            json!({ "chargingLimit": { "chargingLimitSource": "DSO" } }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("DSO") || err.to_string().contains("variant"));
    }

    #[test]
    fn response_is_empty_object_on_wire() {
        let resp = NotifyChargingLimitResponse::default();
        assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
        let parsed: NotifyChargingLimitResponse = serde_json::from_value(json!({})).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            NotifyChargingLimitRequest::ACTION_NAME,
            "NotifyChargingLimit"
        );
        assert_eq!(
            NotifyChargingLimitResponse::ACTION_NAME,
            "NotifyChargingLimitResponse"
        );
    }
}
