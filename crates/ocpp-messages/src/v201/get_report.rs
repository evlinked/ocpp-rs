//! `GetReport` — the CSMS asks a Charging Station for a *filtered* snapshot of
//! its device model: narrowed by a list of component/variable references and/or
//! by component criteria (`Active` / `Available` / `Enabled` / `Problem`).
//!
//! Ports `ocpp.v201.call.GetReport` / `ocpp.v201.call_result.GetReport`. Like
//! [`GetBaseReport`](super::GetBaseReportRequest), the station acknowledges
//! synchronously with a [`GenericDeviceModelStatusEnumType`] and then streams
//! the actual data asynchronously via later `NotifyReport` messages, correlated
//! back to this request by `requestId`.
//!
//! [`GenericDeviceModelStatusEnumType`]: ocpp_types::v201::GenericDeviceModelStatusEnumType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ComponentCriterionEnumType, ComponentVariableType, CustomDataType,
    GenericDeviceModelStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetReport.req` — sent by the CSMS to request a filtered device-model report.
///
/// Ports `ocpp.v201.call.GetReport`. `request_id` is the correlation id the
/// station echoes back on the asynchronous `NotifyReport` messages;
/// `component_variable` narrows the report to specific component-variables and
/// `component_criteria` narrows it by component state. Both filters are
/// optional; when present the schema requires `component_variable` to hold at
/// least one item and `component_criteria` between one and four.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetReportRequest {
    /// Component-variables to report on; absent means no such filter. The schema
    /// requires at least one item when present.
    #[serde(rename = "componentVariable", skip_serializing_if = "Option::is_none")]
    pub component_variable: Option<Vec<ComponentVariableType>>,
    /// The id of the request, echoed by the station on the asynchronous
    /// `NotifyReport` messages that carry the actual report data.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Criteria selecting which components to report on; absent means no such
    /// filter. The schema requires one to four items when present.
    #[serde(rename = "componentCriteria", skip_serializing_if = "Option::is_none")]
    pub component_criteria: Option<Vec<ComponentCriterionEnumType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetReportRequest {
    const ACTION_NAME: &'static str = "GetReport";
    type Response = GetReportResponse;
}

/// `GetReport.conf` — the Charging Station's synchronous acknowledgement.
///
/// Ports `ocpp.v201.call_result.GetReport`. `status` reports whether the station
/// will produce the report; the actual data follows asynchronously in
/// `NotifyReport` messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetReportResponse {
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

impl OcppAction for GetReportResponse {
    const ACTION_NAME: &'static str = "GetReportResponse";
    type Response = Self;
}

impl OcppResponse for GetReportResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::ComponentType;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = GetReportRequest {
            component_variable: None,
            request_id: 42,
            component_criteria: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional filters and `customData` are omitted when `None`.
        assert_eq!(value, json!({ "requestId": 42 }));
        let parsed: GetReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_filters() {
        let req = GetReportRequest {
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
            component_criteria: Some(vec![
                ComponentCriterionEnumType::Active,
                ComponentCriterionEnumType::Problem,
            ]),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(
            value,
            json!({
                "componentVariable": [{ "component": { "name": "EVSE" } }],
                "requestId": 7,
                "componentCriteria": ["Active", "Problem"]
            })
        );
        let parsed: GetReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_id_round_trips_as_integer() {
        let value = serde_json::to_value(GetReportRequest {
            component_variable: None,
            request_id: -3,
            component_criteria: None,
            custom_data: None,
        })
        .unwrap();
        assert_eq!(value["requestId"], json!(-3));
        assert!(value["requestId"].is_i64());
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = GetReportResponse {
            status: GenericDeviceModelStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: GetReportResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = GetReportResponse {
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
    fn component_criterion_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(ComponentCriterionEnumType::Active).unwrap(),
            json!("Active")
        );
        assert_eq!(
            serde_json::to_value(ComponentCriterionEnumType::Available).unwrap(),
            json!("Available")
        );
        assert_eq!(
            serde_json::to_value(ComponentCriterionEnumType::Enabled).unwrap(),
            json!("Enabled")
        );
        assert_eq!(
            serde_json::to_value(ComponentCriterionEnumType::Problem).unwrap(),
            json!("Problem")
        );
    }

    #[test]
    fn request_missing_request_id_fails() {
        let err =
            serde_json::from_value::<GetReportRequest>(json!({ "componentCriteria": ["Active"] }))
                .unwrap_err();
        assert!(err.to_string().contains("requestId"));
    }

    #[test]
    fn request_rejects_unknown_criterion() {
        let err = serde_json::from_value::<GetReportRequest>(
            json!({ "requestId": 1, "componentCriteria": ["Broken"] }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Broken") || err.to_string().contains("variant"));
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err =
            serde_json::from_value::<GetReportResponse>(json!({ "status": "Maybe" })).unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(GetReportRequest::ACTION_NAME, "GetReport");
        assert_eq!(GetReportResponse::ACTION_NAME, "GetReportResponse");
    }
}
