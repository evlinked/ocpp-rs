//! `SetVariableMonitoring` — the CSMS installs a set of variable monitors
//! (threshold / delta / periodic) on a Charging Station, each targeting a
//! component-variable with a severity, and the station replies with a
//! per-monitor result reporting whether each was accepted (returning the
//! assigned monitor id) or rejected with a reason.
//!
//! Ports `ocpp.v201.call.SetVariableMonitoring` /
//! `ocpp.v201.call_result.SetVariableMonitoring`. It is the install counterpart
//! to `ClearVariableMonitoring` and part of the device-model **monitoring**
//! family, alongside `GetMonitoringReport` / `NotifyMonitoringReport` /
//! `SetMonitoringBase`. The request/response carry the shared
//! [`SetMonitoringDataType`] / [`SetMonitoringResultType`] datatypes.
//!
//! [`SetMonitoringDataType`]: ocpp_types::v201::SetMonitoringDataType
//! [`SetMonitoringResultType`]: ocpp_types::v201::SetMonitoringResultType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, SetMonitoringDataType, SetMonitoringResultType};
use serde::{Deserialize, Serialize};

/// `SetVariableMonitoring.req` — sent by the CSMS to install one or more
/// variable monitors on a Charging Station.
///
/// Ports `ocpp.v201.call.SetVariableMonitoring`. `set_monitoring_data` carries
/// the monitors to install; the schema requires at least one entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVariableMonitoringRequest {
    /// The monitors to install. The schema requires at least one item.
    #[serde(rename = "setMonitoringData")]
    pub set_monitoring_data: Vec<SetMonitoringDataType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetVariableMonitoringRequest {
    const ACTION_NAME: &'static str = "SetVariableMonitoring";
    type Response = SetVariableMonitoringResponse;
}

/// `SetVariableMonitoring.conf` — the Charging Station's per-monitor result.
///
/// Ports `ocpp.v201.call_result.SetVariableMonitoring`. `set_monitoring_result`
/// reports, for each requested monitor, whether it was accepted (returning the
/// assigned id) or rejected with a reason; the schema requires at least one
/// item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVariableMonitoringResponse {
    /// The per-monitor outcomes. The schema requires at least one item.
    #[serde(rename = "setMonitoringResult")]
    pub set_monitoring_result: Vec<SetMonitoringResultType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetVariableMonitoringResponse {
    const ACTION_NAME: &'static str = "SetVariableMonitoringResponse";
    type Response = Self;
}

impl OcppResponse for SetVariableMonitoringResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        ComponentType, MonitorEnumType, SetMonitoringStatusEnumType, StatusInfoType, VariableType,
    };
    use serde_json::json;

    fn sample_data() -> SetMonitoringDataType {
        SetMonitoringDataType {
            id: None,
            transaction: None,
            value: 80.0,
            kind: MonitorEnumType::UpperThreshold,
            severity: 5,
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
            custom_data: None,
        }
    }

    fn sample_result() -> SetMonitoringResultType {
        SetMonitoringResultType {
            id: Some(1),
            status: SetMonitoringStatusEnumType::Accepted,
            kind: MonitorEnumType::UpperThreshold,
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
            severity: 5,
            status_info: None,
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = SetVariableMonitoringRequest {
            set_monitoring_data: vec![sample_data()],
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`; the data entry omits `id` /
        // `transaction` and serializes `value` as a number.
        let entry = &value["setMonitoringData"][0];
        assert_eq!(entry["value"], json!(80.0));
        assert!(entry["value"].is_number());
        assert_eq!(entry["type"], json!("UpperThreshold"));
        assert_eq!(entry["severity"], json!(5));
        assert_eq!(entry["component"]["name"], json!("EVSE"));
        assert_eq!(entry["variable"]["name"], json!("Temperature"));
        let obj = entry.as_object().unwrap();
        assert!(!obj.contains_key("id"));
        assert!(!obj.contains_key("transaction"));
        assert!(!value.as_object().unwrap().contains_key("customData"));
        let parsed: SetVariableMonitoringRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_id_and_transaction() {
        let mut data = sample_data();
        data.id = Some(42);
        data.transaction = Some(true);
        let req = SetVariableMonitoringRequest {
            set_monitoring_data: vec![data],
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        let entry = &value["setMonitoringData"][0];
        assert_eq!(entry["id"], json!(42));
        assert_eq!(entry["transaction"], json!(true));
        let parsed: SetVariableMonitoringRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = SetVariableMonitoringResponse {
            set_monitoring_result: vec![sample_result()],
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        let entry = &value["setMonitoringResult"][0];
        assert_eq!(entry["status"], json!("Accepted"));
        assert_eq!(entry["id"], json!(1));
        assert_eq!(entry["type"], json!("UpperThreshold"));
        assert_eq!(entry["severity"], json!(5));
        assert!(!entry.as_object().unwrap().contains_key("statusInfo"));
        let parsed: SetVariableMonitoringResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info_and_omits_id_on_rejection() {
        let mut result = sample_result();
        result.id = None;
        result.status = SetMonitoringStatusEnumType::Rejected;
        result.status_info = Some(StatusInfoType {
            reason_code: "OutOfRange".to_string(),
            additional_info: None,
            custom_data: None,
        });
        let resp = SetVariableMonitoringResponse {
            set_monitoring_result: vec![result],
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        let entry = &value["setMonitoringResult"][0];
        assert_eq!(entry["status"], json!("Rejected"));
        assert_eq!(entry["statusInfo"]["reasonCode"], json!("OutOfRange"));
        assert!(!entry.as_object().unwrap().contains_key("id"));
        let parsed: SetVariableMonitoringResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn status_serializes_to_exact_wire_values() {
        for (variant, wire) in [
            (SetMonitoringStatusEnumType::Accepted, "Accepted"),
            (
                SetMonitoringStatusEnumType::UnknownComponent,
                "UnknownComponent",
            ),
            (
                SetMonitoringStatusEnumType::UnknownVariable,
                "UnknownVariable",
            ),
            (
                SetMonitoringStatusEnumType::UnsupportedMonitorType,
                "UnsupportedMonitorType",
            ),
            (SetMonitoringStatusEnumType::Rejected, "Rejected"),
            (SetMonitoringStatusEnumType::Duplicate, "Duplicate"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
        }
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err = serde_json::from_value::<SetVariableMonitoringResponse>(json!({
            "setMonitoringResult": [{
                "status": "Maybe",
                "type": "UpperThreshold",
                "component": { "name": "EVSE" },
                "variable": { "name": "Temperature" },
                "severity": 5
            }]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            SetVariableMonitoringRequest::ACTION_NAME,
            "SetVariableMonitoring"
        );
        assert_eq!(
            SetVariableMonitoringResponse::ACTION_NAME,
            "SetVariableMonitoringResponse"
        );
    }
}
