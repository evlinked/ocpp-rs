//! `GetMonitoringReport` — the CSMS asks a Charging Station for a *filtered*
//! snapshot of its variable-monitoring configuration: narrowed by a list of
//! component/variable references and/or by monitoring criteria
//! (`ThresholdMonitoring` / `DeltaMonitoring` / `PeriodicMonitoring`).
//!
//! Ports `ocpp.v201.call.GetMonitoringReport` /
//! `ocpp.v201.call_result.GetMonitoringReport`. Like [`GetReport`] and
//! [`GetBaseReport`](super::GetBaseReportRequest), the station acknowledges
//! synchronously with a [`GenericDeviceModelStatusEnumType`] and then streams
//! the actual data asynchronously via later `NotifyMonitoringReport` messages,
//! correlated back to this request by `requestId`.
//!
//! [`GetReport`]: super::GetReportRequest
//! [`GenericDeviceModelStatusEnumType`]: ocpp_types::v201::GenericDeviceModelStatusEnumType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ComponentVariableType, CustomDataType, GenericDeviceModelStatusEnumType,
    MonitoringCriterionEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetMonitoringReport.req` — sent by the CSMS to request a filtered snapshot
/// of a Charging Station's variable-monitoring configuration.
///
/// Ports `ocpp.v201.call.GetMonitoringReport`. `request_id` is the correlation
/// id the station echoes back on the asynchronous `NotifyMonitoringReport`
/// messages; `component_variable` narrows the report to specific
/// component-variables and `monitoring_criteria` narrows it by the kind of
/// monitor configured. Both filters are optional; when present the schema
/// requires `component_variable` to hold at least one item and
/// `monitoring_criteria` between one and three.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetMonitoringReportRequest {
    /// Component-variables to report monitoring on; absent means no such
    /// filter. The schema requires at least one item when present.
    #[serde(rename = "componentVariable", skip_serializing_if = "Option::is_none")]
    pub component_variable: Option<Vec<ComponentVariableType>>,
    /// The id of the request, echoed by the station on the asynchronous
    /// `NotifyMonitoringReport` messages that carry the actual monitoring data.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Criteria selecting which components to report on, by the kind of monitor
    /// configured; absent means no such filter. The schema requires one to
    /// three items when present.
    #[serde(rename = "monitoringCriteria", skip_serializing_if = "Option::is_none")]
    pub monitoring_criteria: Option<Vec<MonitoringCriterionEnumType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetMonitoringReportRequest {
    const ACTION_NAME: &'static str = "GetMonitoringReport";
    type Response = GetMonitoringReportResponse;
}

/// `GetMonitoringReport.conf` — the Charging Station's synchronous
/// acknowledgement.
///
/// Ports `ocpp.v201.call_result.GetMonitoringReport`. `status` reports whether
/// the station will produce the report; the actual data follows asynchronously
/// in `NotifyMonitoringReport` messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetMonitoringReportResponse {
    /// Whether the station can produce the requested report (`Accepted`,
    /// `Rejected`, `NotSupported`, or `EmptyResultSet` when nothing matched).
    pub status: GenericDeviceModelStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetMonitoringReportResponse {
    const ACTION_NAME: &'static str = "GetMonitoringReportResponse";
    type Response = Self;
}

impl OcppResponse for GetMonitoringReportResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::ComponentType;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = GetMonitoringReportRequest {
            component_variable: None,
            request_id: 42,
            monitoring_criteria: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional filters and `customData` are omitted when `None`.
        assert_eq!(value, json!({ "requestId": 42 }));
        let parsed: GetMonitoringReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_filters() {
        let req = GetMonitoringReportRequest {
            component_variable: Some(vec![ComponentVariableType {
                component: ComponentType {
                    name: "EVSE".to_string(),
                    instance: None,
                    evse: None,
                    custom_data: None,
                },
                variable: None,
                custom_data: None,
            }]),
            request_id: 7,
            monitoring_criteria: Some(vec![
                MonitoringCriterionEnumType::ThresholdMonitoring,
                MonitoringCriterionEnumType::PeriodicMonitoring,
            ]),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(
            value,
            json!({
                "componentVariable": [{ "component": { "name": "EVSE" } }],
                "requestId": 7,
                "monitoringCriteria": ["ThresholdMonitoring", "PeriodicMonitoring"]
            })
        );
        let parsed: GetMonitoringReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_id_round_trips_as_integer() {
        let value = serde_json::to_value(GetMonitoringReportRequest {
            component_variable: None,
            request_id: -3,
            monitoring_criteria: None,
            custom_data: None,
        })
        .unwrap();
        assert_eq!(value["requestId"], json!(-3));
        assert!(value["requestId"].is_i64());
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = GetMonitoringReportResponse {
            status: GenericDeviceModelStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: GetMonitoringReportResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = GetMonitoringReportResponse {
            status: GenericDeviceModelStatusEnumType::EmptyResultSet,
            status_info: Some(StatusInfoType {
                reason_code: "NoData".to_string(),
                additional_info: None,
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("EmptyResultSet"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("NoData"));
    }

    #[test]
    fn monitoring_criterion_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(MonitoringCriterionEnumType::ThresholdMonitoring).unwrap(),
            json!("ThresholdMonitoring")
        );
        assert_eq!(
            serde_json::to_value(MonitoringCriterionEnumType::DeltaMonitoring).unwrap(),
            json!("DeltaMonitoring")
        );
        assert_eq!(
            serde_json::to_value(MonitoringCriterionEnumType::PeriodicMonitoring).unwrap(),
            json!("PeriodicMonitoring")
        );
    }

    #[test]
    fn request_missing_request_id_fails() {
        let err = serde_json::from_value::<GetMonitoringReportRequest>(
            json!({ "monitoringCriteria": ["DeltaMonitoring"] }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("requestId"));
    }

    #[test]
    fn request_rejects_unknown_criterion() {
        let err = serde_json::from_value::<GetMonitoringReportRequest>(
            json!({ "requestId": 1, "monitoringCriteria": ["HourlyMonitoring"] }),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("HourlyMonitoring") || err.to_string().contains("variant")
        );
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err =
            serde_json::from_value::<GetMonitoringReportResponse>(json!({ "status": "Maybe" }))
                .unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            GetMonitoringReportRequest::ACTION_NAME,
            "GetMonitoringReport"
        );
        assert_eq!(
            GetMonitoringReportResponse::ACTION_NAME,
            "GetMonitoringReportResponse"
        );
    }
}
