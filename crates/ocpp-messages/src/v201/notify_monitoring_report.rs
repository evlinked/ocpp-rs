//! `NotifyMonitoringReport` — the Charging Station streams the *actual*
//! variable-monitoring data that a [`GetMonitoringReport`] request asked for.
//!
//! Ports `ocpp.v201.call.NotifyMonitoringReport` /
//! `ocpp.v201.call_result.NotifyMonitoringReport`. It is the asynchronous data
//! half of the device-model **monitoring** flow: the station sends one or more
//! `NotifyMonitoringReport.req` messages, paged via `seq_no` / `tbc`, each
//! correlated back to the triggering request by `request_id`. The near-twin of
//! `NotifyReport` (same paged-carrier shape) but its payload embeds the
//! monitoring graph ([`MonitoringDataType`] → [`VariableMonitoringType`])
//! rather than the report graph. The response is empty.
//!
//! [`GetMonitoringReport`]: super::GetMonitoringReportRequest
//! [`MonitoringDataType`]: ocpp_types::v201::MonitoringDataType
//! [`VariableMonitoringType`]: ocpp_types::v201::VariableMonitoringType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, MonitoringDataType};
use serde::{Deserialize, Serialize};

/// `NotifyMonitoringReport.req` — a single (possibly partial) page of variable
/// monitoring data sent by the Charging Station.
///
/// Ports `ocpp.v201.call.NotifyMonitoringReport`. `request_id` echoes the
/// `GetMonitoringReport` that triggered the report; `seq_no` numbers the pages
/// (first is 0) and `tbc` ("to be continued") is `true` while more pages
/// follow. `monitor` carries the actual data and, per the schema, holds at
/// least one item when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyMonitoringReportRequest {
    /// The monitoring data for this page; absent when the page carries none.
    /// The schema requires at least one item when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<Vec<MonitoringDataType>>,
    /// The id of the `GetMonitoringReport` request that requested this report.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// "To be continued" — `true` when another page follows in an upcoming
    /// `NotifyMonitoringReport`. Absent means the default `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbc: Option<bool>,
    /// Sequence number of this message; the first page starts at 0.
    #[serde(rename = "seqNo")]
    pub seq_no: i32,
    /// Timestamp (RFC 3339 / ISO 8601) of the moment this message was generated
    /// at the Charging Station.
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyMonitoringReportRequest {
    const ACTION_NAME: &'static str = "NotifyMonitoringReport";
    type Response = NotifyMonitoringReportResponse;
}

/// `NotifyMonitoringReport.conf` — the CSMS's empty acknowledgement.
///
/// Ports `ocpp.v201.call_result.NotifyMonitoringReport`. It carries no fields
/// beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NotifyMonitoringReportResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyMonitoringReportResponse {
    const ACTION_NAME: &'static str = "NotifyMonitoringReportResponse";
    type Response = Self;
}

impl OcppResponse for NotifyMonitoringReportResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{ComponentType, MonitorEnumType, VariableMonitoringType, VariableType};
    use serde_json::json;

    fn sample_monitor() -> MonitoringDataType {
        MonitoringDataType {
            component: ComponentType {
                name: "EVSE".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "Temperature".to_string(),
                instance: None,
                custom_data: None,
            },
            variable_monitoring: vec![VariableMonitoringType {
                id: 1,
                transaction: false,
                value: 80.0,
                kind: MonitorEnumType::UpperThreshold,
                severity: 5,
                custom_data: None,
            }],
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = NotifyMonitoringReportRequest {
            monitor: None,
            request_id: 42,
            tbc: None,
            seq_no: 0,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional `monitor` / `tbc` / `customData` stay off the wire.
        assert_eq!(
            value,
            json!({
                "requestId": 42,
                "seqNo": 0,
                "generatedAt": "2022-01-01T10:00:00Z"
            })
        );
        let parsed: NotifyMonitoringReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_monitor() {
        let req = NotifyMonitoringReportRequest {
            monitor: Some(vec![sample_monitor()]),
            request_id: 7,
            tbc: Some(true),
            seq_no: 3,
            generated_at: "2022-01-01T10:05:00Z".to_string(),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["monitor"][0]["component"]["name"], json!("EVSE"));
        assert_eq!(
            value["monitor"][0]["variable"]["name"],
            json!("Temperature")
        );
        assert_eq!(
            value["monitor"][0]["variableMonitoring"][0]["type"],
            json!("UpperThreshold")
        );
        assert_eq!(value["tbc"], json!(true));
        let parsed: NotifyMonitoringReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let value = serde_json::to_value(NotifyMonitoringReportRequest {
            monitor: Some(vec![sample_monitor()]),
            request_id: -3,
            tbc: Some(false),
            seq_no: 12,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            custom_data: None,
        })
        .unwrap();
        assert!(value["requestId"].is_i64());
        assert!(value["seqNo"].is_i64());
        assert!(value["tbc"].is_boolean());
        assert!(value["generatedAt"].is_string());
        let vm = &value["monitor"][0]["variableMonitoring"][0];
        assert!(vm["id"].is_i64());
        assert!(vm["transaction"].is_boolean());
        assert!(vm["value"].is_number());
        assert!(vm["severity"].is_i64());
    }

    #[test]
    fn monitor_enum_serializes_to_exact_wire_values() {
        for (variant, wire) in [
            (MonitorEnumType::UpperThreshold, "UpperThreshold"),
            (MonitorEnumType::LowerThreshold, "LowerThreshold"),
            (MonitorEnumType::Delta, "Delta"),
            (MonitorEnumType::Periodic, "Periodic"),
            (
                MonitorEnumType::PeriodicClockAligned,
                "PeriodicClockAligned",
            ),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: MonitorEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn response_round_trips_empty() {
        let resp = NotifyMonitoringReportResponse::default();
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({}));
        let parsed: NotifyMonitoringReportResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn request_missing_required_field_fails() {
        // `seqNo` is required.
        let err = serde_json::from_value::<NotifyMonitoringReportRequest>(
            json!({ "requestId": 1, "generatedAt": "2022-01-01T10:00:00Z" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("seqNo"));
    }

    #[test]
    fn request_rejects_unknown_monitor_kind() {
        let err = serde_json::from_value::<NotifyMonitoringReportRequest>(json!({
            "requestId": 1,
            "seqNo": 0,
            "generatedAt": "2022-01-01T10:00:00Z",
            "monitor": [{
                "component": { "name": "EVSE" },
                "variable": { "name": "Temperature" },
                "variableMonitoring": [{
                    "id": 1, "transaction": false, "value": 80.0,
                    "type": "Hourly", "severity": 5
                }]
            }]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("Hourly") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            NotifyMonitoringReportRequest::ACTION_NAME,
            "NotifyMonitoringReport"
        );
        assert_eq!(
            NotifyMonitoringReportResponse::ACTION_NAME,
            "NotifyMonitoringReportResponse"
        );
    }
}
