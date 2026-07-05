//! `NotifyReport` — the Charging Station streams the *actual* device-model
//! report data that a [`GetBaseReport`] or [`GetReport`] request asked for.
//!
//! Ports `ocpp.v201.call.NotifyReport` / `ocpp.v201.call_result.NotifyReport`.
//! It is the asynchronous data half of the device-model **report** flow: the
//! station sends one or more `NotifyReport.req` messages, paged via `seq_no` /
//! `tbc`, each correlated back to the triggering request by `request_id`. The
//! near-twin of `NotifyMonitoringReport` (same paged-carrier shape) but its
//! payload embeds the report graph ([`ReportDataType`] →
//! [`VariableAttributeType`] / [`VariableCharacteristicsType`]) rather than the
//! monitoring graph. The response is empty.
//!
//! [`GetBaseReport`]: super::GetBaseReportRequest
//! [`GetReport`]: super::GetReportRequest
//! [`ReportDataType`]: ocpp_types::v201::ReportDataType
//! [`VariableAttributeType`]: ocpp_types::v201::VariableAttributeType
//! [`VariableCharacteristicsType`]: ocpp_types::v201::VariableCharacteristicsType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, ReportDataType};
use serde::{Deserialize, Serialize};

/// `NotifyReport.req` — a single (possibly partial) page of device-model report
/// data sent by the Charging Station.
///
/// Ports `ocpp.v201.call.NotifyReport`. `request_id` echoes the
/// `GetBaseReport` / `GetReport` that triggered the report; `seq_no` numbers the
/// pages (first is 0) and `tbc` ("to be continued") is `true` while more pages
/// follow. `report_data` carries the actual data and, per the schema, holds at
/// least one item when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyReportRequest {
    /// The id of the `GetBaseReport` / `GetReport` request that requested this
    /// report.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Timestamp (RFC 3339 / ISO 8601) of the moment this message was generated
    /// at the Charging Station.
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    /// The report data for this page; absent when the page carries none. The
    /// schema requires at least one item when present.
    #[serde(rename = "reportData", skip_serializing_if = "Option::is_none")]
    pub report_data: Option<Vec<ReportDataType>>,
    /// "To be continued" — `true` when another page follows in an upcoming
    /// `NotifyReport`. Absent means the default `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbc: Option<bool>,
    /// Sequence number of this message; the first page starts at 0.
    #[serde(rename = "seqNo")]
    pub seq_no: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyReportRequest {
    const ACTION_NAME: &'static str = "NotifyReport";
    type Response = NotifyReportResponse;
}

/// `NotifyReport.conf` — the CSMS's empty acknowledgement.
///
/// Ports `ocpp.v201.call_result.NotifyReport`. It carries no fields beyond the
/// optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NotifyReportResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyReportResponse {
    const ACTION_NAME: &'static str = "NotifyReportResponse";
    type Response = Self;
}

impl OcppResponse for NotifyReportResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        AttributeEnumType, ComponentType, DataEnumType, MutabilityEnumType, VariableAttributeType,
        VariableCharacteristicsType, VariableType,
    };
    use serde_json::json;

    fn sample_report_data() -> ReportDataType {
        ReportDataType {
            component: ComponentType {
                name: "EVSE".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "AvailabilityState".to_string(),
                instance: None,
                custom_data: None,
            },
            variable_attribute: vec![VariableAttributeType {
                kind: Some(AttributeEnumType::Actual),
                value: Some("Available".to_string()),
                mutability: Some(MutabilityEnumType::ReadOnly),
                persistent: Some(true),
                constant: Some(false),
                custom_data: None,
            }],
            variable_characteristics: Some(VariableCharacteristicsType {
                unit: None,
                data_type: DataEnumType::OptionList,
                min_limit: None,
                max_limit: None,
                values_list: Some("Available,Occupied,Faulted".to_string()),
                supports_monitoring: true,
                custom_data: None,
            }),
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = NotifyReportRequest {
            request_id: 42,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            report_data: None,
            tbc: None,
            seq_no: 0,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional `reportData` / `tbc` / `customData` stay off the wire.
        assert_eq!(
            value,
            json!({
                "requestId": 42,
                "generatedAt": "2022-01-01T10:00:00Z",
                "seqNo": 0
            })
        );
        let parsed: NotifyReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_report_data() {
        let req = NotifyReportRequest {
            request_id: 7,
            generated_at: "2022-01-01T10:05:00Z".to_string(),
            report_data: Some(vec![sample_report_data()]),
            tbc: Some(true),
            seq_no: 3,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["reportData"][0]["component"]["name"], json!("EVSE"));
        assert_eq!(
            value["reportData"][0]["variable"]["name"],
            json!("AvailabilityState")
        );
        let attr = &value["reportData"][0]["variableAttribute"][0];
        assert_eq!(attr["type"], json!("Actual"));
        assert_eq!(attr["value"], json!("Available"));
        assert_eq!(attr["mutability"], json!("ReadOnly"));
        let chars = &value["reportData"][0]["variableCharacteristics"];
        assert_eq!(chars["dataType"], json!("OptionList"));
        assert_eq!(chars["supportsMonitoring"], json!(true));
        assert_eq!(value["tbc"], json!(true));
        let parsed: NotifyReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let value = serde_json::to_value(NotifyReportRequest {
            request_id: -3,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            report_data: Some(vec![sample_report_data()]),
            tbc: Some(false),
            seq_no: 12,
            custom_data: None,
        })
        .unwrap();
        assert!(value["requestId"].is_i64());
        assert!(value["seqNo"].is_i64());
        assert!(value["tbc"].is_boolean());
        assert!(value["generatedAt"].is_string());
        let attr = &value["reportData"][0]["variableAttribute"][0];
        assert!(attr["persistent"].is_boolean());
        assert!(attr["constant"].is_boolean());
        let chars = &value["reportData"][0]["variableCharacteristics"];
        assert!(chars["supportsMonitoring"].is_boolean());
    }

    #[test]
    fn optional_attribute_fields_omitted_when_none() {
        // A minimal VariableAttribute (all fields None) serializes to `{}`.
        let req = NotifyReportRequest {
            request_id: 1,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            report_data: Some(vec![ReportDataType {
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
                variable_attribute: vec![VariableAttributeType {
                    kind: None,
                    value: None,
                    mutability: None,
                    persistent: None,
                    constant: None,
                    custom_data: None,
                }],
                variable_characteristics: None,
                custom_data: None,
            }]),
            tbc: None,
            seq_no: 0,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["reportData"][0]["variableAttribute"][0], json!({}));
        assert!(value["reportData"][0]
            .get("variableCharacteristics")
            .is_none());
        let parsed: NotifyReportRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn mutability_enum_serializes_to_exact_wire_values() {
        for (variant, wire) in [
            (MutabilityEnumType::ReadOnly, "ReadOnly"),
            (MutabilityEnumType::WriteOnly, "WriteOnly"),
            (MutabilityEnumType::ReadWrite, "ReadWrite"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: MutabilityEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn data_enum_serializes_to_exact_wire_values() {
        // The scalar members are lower-case on the wire; the list members are
        // PascalCase.
        for (variant, wire) in [
            (DataEnumType::String, "string"),
            (DataEnumType::Decimal, "decimal"),
            (DataEnumType::Integer, "integer"),
            (DataEnumType::DateTime, "dateTime"),
            (DataEnumType::Boolean, "boolean"),
            (DataEnumType::OptionList, "OptionList"),
            (DataEnumType::SequenceList, "SequenceList"),
            (DataEnumType::MemberList, "MemberList"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: DataEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // Unknown value rejected.
        assert!(serde_json::from_value::<DataEnumType>(json!("String")).is_err());
    }

    #[test]
    fn response_round_trips_empty() {
        let resp = NotifyReportResponse::default();
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({}));
        let parsed: NotifyReportResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn request_missing_required_field_fails() {
        // `seqNo` is required.
        let err = serde_json::from_value::<NotifyReportRequest>(
            json!({ "requestId": 1, "generatedAt": "2022-01-01T10:00:00Z" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("seqNo"));
    }

    #[test]
    fn request_rejects_unknown_data_type() {
        let err = serde_json::from_value::<NotifyReportRequest>(json!({
            "requestId": 1,
            "seqNo": 0,
            "generatedAt": "2022-01-01T10:00:00Z",
            "reportData": [{
                "component": { "name": "EVSE" },
                "variable": { "name": "Temperature" },
                "variableAttribute": [{}],
                "variableCharacteristics": { "dataType": "float", "supportsMonitoring": true }
            }]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("float") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(NotifyReportRequest::ACTION_NAME, "NotifyReport");
        assert_eq!(NotifyReportResponse::ACTION_NAME, "NotifyReportResponse");
    }
}
