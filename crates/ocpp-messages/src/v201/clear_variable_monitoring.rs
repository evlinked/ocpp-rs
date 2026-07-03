//! `ClearVariableMonitoring` — the CSMS tells a Charging Station to **remove a
//! set of previously-installed variable monitors** by their monitor ids.
//!
//! Ports `ocpp.v201.call.ClearVariableMonitoring` /
//! `ocpp.v201.call_result.ClearVariableMonitoring`. The station answers with a
//! per-id result list ([`ClearMonitoringResultType`]) reporting, for each
//! requested monitor, whether it was cleared (`Accepted`), refused
//! (`Rejected`), or unknown (`NotFound`). It is the teardown counterpart to
//! `SetVariableMonitoring` and part of the device-model **monitoring** family
//! (`SetMonitoringBase`, `GetMonitoringReport` → `NotifyMonitoringReport`).
//!
//! [`ClearMonitoringResultType`]: ocpp_types::v201::ClearMonitoringResultType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{ClearMonitoringResultType, CustomDataType};
use serde::{Deserialize, Serialize};

/// `ClearVariableMonitoring.req` — sent by the CSMS to clear one or more
/// variable monitors on a Charging Station.
///
/// Ports `ocpp.v201.call.ClearVariableMonitoring`. `id` lists the monitor ids
/// to clear; per the schema it holds at least one item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearVariableMonitoringRequest {
    /// The ids of the monitors to clear. The schema requires at least one item.
    pub id: Vec<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearVariableMonitoringRequest {
    const ACTION_NAME: &'static str = "ClearVariableMonitoring";
    type Response = ClearVariableMonitoringResponse;
}

/// `ClearVariableMonitoring.conf` — the Charging Station's per-id result list.
///
/// Ports `ocpp.v201.call_result.ClearVariableMonitoring`.
/// `clear_monitoring_result` reports the outcome for each requested monitor;
/// per the schema it holds at least one item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearVariableMonitoringResponse {
    /// The per-monitor clear results. The schema requires at least one item.
    #[serde(rename = "clearMonitoringResult")]
    pub clear_monitoring_result: Vec<ClearMonitoringResultType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearVariableMonitoringResponse {
    const ACTION_NAME: &'static str = "ClearVariableMonitoringResponse";
    type Response = Self;
}

impl OcppResponse for ClearVariableMonitoringResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{ClearMonitoringStatusEnumType, StatusInfoType};
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = ClearVariableMonitoringRequest {
            id: vec![1, 2, 3],
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`.
        assert_eq!(value, json!({ "id": [1, 2, 3] }));
        let parsed: ClearVariableMonitoringRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_ids_round_trip_as_integers() {
        let req = ClearVariableMonitoringRequest {
            id: vec![7],
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert!(value["id"][0].is_i64());
    }

    #[test]
    fn request_missing_id_fails() {
        let err = serde_json::from_value::<ClearVariableMonitoringRequest>(json!({})).unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = ClearVariableMonitoringResponse {
            clear_monitoring_result: vec![ClearMonitoringResultType {
                status: ClearMonitoringStatusEnumType::Accepted,
                id: 1,
                status_info: None,
                custom_data: None,
            }],
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(
            value,
            json!({ "clearMonitoringResult": [{ "status": "Accepted", "id": 1 }] })
        );
        let parsed: ClearVariableMonitoringResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info_and_omits_it_when_none() {
        let resp = ClearVariableMonitoringResponse {
            clear_monitoring_result: vec![
                ClearMonitoringResultType {
                    status: ClearMonitoringStatusEnumType::Rejected,
                    id: 4,
                    status_info: Some(StatusInfoType {
                        reason_code: "InUse".to_string(),
                        additional_info: None,
                        custom_data: None,
                    }),
                    custom_data: None,
                },
                ClearMonitoringResultType {
                    status: ClearMonitoringStatusEnumType::NotFound,
                    id: 99,
                    status_info: None,
                    custom_data: None,
                },
            ],
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(
            value["clearMonitoringResult"][0]["statusInfo"]["reasonCode"],
            json!("InUse")
        );
        // The second result has no `statusInfo`.
        assert!(value["clearMonitoringResult"][1]
            .as_object()
            .unwrap()
            .get("statusInfo")
            .is_none());
        let parsed: ClearVariableMonitoringResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn clear_monitoring_status_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(ClearMonitoringStatusEnumType::Accepted).unwrap(),
            json!("Accepted")
        );
        assert_eq!(
            serde_json::to_value(ClearMonitoringStatusEnumType::Rejected).unwrap(),
            json!("Rejected")
        );
        assert_eq!(
            serde_json::to_value(ClearMonitoringStatusEnumType::NotFound).unwrap(),
            json!("NotFound")
        );
    }

    #[test]
    fn result_rejects_unknown_status() {
        let err = serde_json::from_value::<ClearMonitoringResultType>(
            json!({ "status": "Maybe", "id": 1 }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            ClearVariableMonitoringRequest::ACTION_NAME,
            "ClearVariableMonitoring"
        );
        assert_eq!(
            ClearVariableMonitoringResponse::ACTION_NAME,
            "ClearVariableMonitoringResponse"
        );
    }
}
