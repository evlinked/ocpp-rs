//! `NotifyEVChargingSchedule` — the Charging Station reports the charging
//! schedule an EV has communicated to it (via ISO 15118), so the CSMS's
//! smart-charging logic can reconcile it against the profiles it has installed.
//!
//! Ports `ocpp.v201.call.NotifyEVChargingSchedule` /
//! `ocpp.v201.call_result.NotifyEVChargingSchedule`. The station sends the EV's
//! [`ChargingScheduleType`] anchored to a `timeBase` for a given `evseId`; the
//! CSMS acks synchronously with a shared [`GenericStatusEnumType`] (`Accepted` /
//! `Rejected`) that reports only whether it could process the message — it
//! implies no approval of the schedule itself. A reuse-only carrier: no new
//! datatypes and no new enums.
//!
//! [`ChargingScheduleType`]: ocpp_types::v201::ChargingScheduleType
//! [`GenericStatusEnumType`]: ocpp_types::v201::GenericStatusEnumType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ChargingScheduleType, CustomDataType, GenericStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `NotifyEVChargingSchedule.req` — sent by the Charging Station to report the
/// EV's charging schedule to the CSMS.
///
/// Ports `ocpp.v201.call.NotifyEVChargingSchedule`. The periods in
/// `chargingSchedule` are relative to `timeBase`; `evseId` identifies the EVSE
/// the schedule applies to (per the spec it must be `> 0`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyEVChargingScheduleRequest {
    /// Point in time the periods in `chargingSchedule` are relative to
    /// (RFC 3339 date-time).
    #[serde(rename = "timeBase")]
    pub time_base: String,
    /// The schedule the EV communicated to the station.
    #[serde(rename = "chargingSchedule")]
    pub charging_schedule: ChargingScheduleType,
    /// The EVSE the schedule applies to. Per the spec `evseId` must be `> 0`.
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyEVChargingScheduleRequest {
    const ACTION_NAME: &'static str = "NotifyEVChargingSchedule";
    type Response = NotifyEVChargingScheduleResponse;
}

/// `NotifyEVChargingSchedule.conf` — the CSMS's synchronous acknowledgement.
///
/// Ports `ocpp.v201.call_result.NotifyEVChargingSchedule`. `status` reports only
/// whether the CSMS could process the message; per the spec it does **not**
/// imply approval of the charging schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyEVChargingScheduleResponse {
    /// Whether the CSMS processed the message (`Accepted` / `Rejected`); implies
    /// no approval of the schedule.
    pub status: GenericStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyEVChargingScheduleResponse {
    const ACTION_NAME: &'static str = "NotifyEVChargingScheduleResponse";
    type Response = Self;
}

impl OcppResponse for NotifyEVChargingScheduleResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{ChargingRateUnitEnumType, ChargingSchedulePeriodType};
    use serde_json::json;

    fn sample_schedule() -> ChargingScheduleType {
        ChargingScheduleType {
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
        }
    }

    #[test]
    fn request_round_trips() {
        let req = NotifyEVChargingScheduleRequest {
            time_base: "2022-01-01T10:00:00Z".to_string(),
            charging_schedule: sample_schedule(),
            evse_id: 1,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`; the three required fields are all
        // present.
        assert_eq!(value["timeBase"], json!("2022-01-01T10:00:00Z"));
        assert_eq!(value["evseId"], json!(1));
        assert_eq!(value["chargingSchedule"]["id"], json!(1));
        assert_eq!(value["chargingSchedule"]["chargingRateUnit"], json!("A"));
        assert_eq!(
            value["chargingSchedule"]["chargingSchedulePeriod"][0]["startPeriod"],
            json!(0)
        );
        assert!(!value.as_object().unwrap().contains_key("customData"));
        let parsed: NotifyEVChargingScheduleRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn evse_id_round_trips_as_integer() {
        let req = NotifyEVChargingScheduleRequest {
            time_base: "2022-01-01T10:00:00Z".to_string(),
            charging_schedule: sample_schedule(),
            evse_id: 3,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert!(value["evseId"].is_i64());
        assert_eq!(value["evseId"], json!(3));
    }

    #[test]
    fn request_serializes_custom_data() {
        let req = NotifyEVChargingScheduleRequest {
            time_base: "2022-01-01T10:00:00Z".to_string(),
            charging_schedule: sample_schedule(),
            evse_id: 1,
            custom_data: Some(CustomDataType {
                vendor_id: "ACME".to_string(),
                extra: Default::default(),
            }),
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["customData"]["vendorId"], json!("ACME"));
    }

    #[test]
    fn request_missing_charging_schedule_fails() {
        let err = serde_json::from_value::<NotifyEVChargingScheduleRequest>(
            json!({ "timeBase": "2022-01-01T10:00:00Z", "evseId": 1 }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("chargingSchedule"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = NotifyEVChargingScheduleResponse {
            status: GenericStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: NotifyEVChargingScheduleResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = NotifyEVChargingScheduleResponse {
            status: GenericStatusEnumType::Rejected,
            status_info: Some(StatusInfoType {
                reason_code: "InternalError".to_string(),
                additional_info: None,
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("Rejected"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("InternalError"));
    }

    #[test]
    fn status_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(GenericStatusEnumType::Accepted).unwrap(),
            json!("Accepted")
        );
        assert_eq!(
            serde_json::to_value(GenericStatusEnumType::Rejected).unwrap(),
            json!("Rejected")
        );
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err = serde_json::from_value::<NotifyEVChargingScheduleResponse>(
            json!({ "status": "Maybe" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            NotifyEVChargingScheduleRequest::ACTION_NAME,
            "NotifyEVChargingSchedule"
        );
        assert_eq!(
            NotifyEVChargingScheduleResponse::ACTION_NAME,
            "NotifyEVChargingScheduleResponse"
        );
    }
}
