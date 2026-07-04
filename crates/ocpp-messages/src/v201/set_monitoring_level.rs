//! `SetMonitoringLevel` — the CSMS sets the **severity threshold** at or above
//! which a Charging Station reports monitoring events. Monitors whose
//! configured severity is *at or below* the requested number are reported (via
//! `NotifyEvent` / `NotifyMonitoringReport`); the rest are suppressed. Lower
//! numbers are higher severity: `0` (Danger) … `9` (Debug).
//!
//! Ports `ocpp.v201.call.SetMonitoringLevel` /
//! `ocpp.v201.call_result.SetMonitoringLevel`. The station answers
//! synchronously with a shared [`GenericStatusEnumType`] (`Accepted` /
//! `Rejected`). This is the reporting-level control of the device-model
//! monitoring family, completing it alongside `SetMonitoringBase`,
//! `SetVariableMonitoring`, `ClearVariableMonitoring`, and
//! `NotifyMonitoringReport`.
//!
//! [`GenericStatusEnumType`]: ocpp_types::v201::GenericStatusEnumType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, GenericStatusEnumType, StatusInfoType};
use serde::{Deserialize, Serialize};

/// `SetMonitoringLevel.req` — sent by the CSMS to set the monitoring reporting
/// severity threshold on a Charging Station.
///
/// Ports `ocpp.v201.call.SetMonitoringLevel`. The station reports monitoring
/// events whose configured severity is at or below `severity` (lower number =
/// higher severity; range `0`–`9`, documented but not schema-constrained beyond
/// `integer`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMonitoringLevelRequest {
    /// Severity threshold: the station only reports events with a severity
    /// number lower than or equal to this value. `0` (Danger) … `9` (Debug).
    pub severity: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetMonitoringLevelRequest {
    const ACTION_NAME: &'static str = "SetMonitoringLevel";
    type Response = SetMonitoringLevelResponse;
}

/// `SetMonitoringLevel.conf` — the Charging Station's synchronous
/// acknowledgement.
///
/// Ports `ocpp.v201.call_result.SetMonitoringLevel`. `status` reports whether
/// the station accepted the requested monitoring level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMonitoringLevelResponse {
    /// Whether the station accepted the requested monitoring level (`Accepted`
    /// or `Rejected`).
    pub status: GenericStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetMonitoringLevelResponse {
    const ACTION_NAME: &'static str = "SetMonitoringLevelResponse";
    type Response = Self;
}

impl OcppResponse for SetMonitoringLevelResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = SetMonitoringLevelRequest {
            severity: 5,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`.
        assert_eq!(value, json!({ "severity": 5 }));
        let parsed: SetMonitoringLevelRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn severity_round_trips_as_integer() {
        let req = SetMonitoringLevelRequest {
            severity: 0,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["severity"], json!(0));
        assert!(value["severity"].is_i64());
    }

    #[test]
    fn request_missing_severity_fails() {
        let err = serde_json::from_value::<SetMonitoringLevelRequest>(json!({})).unwrap_err();
        assert!(err.to_string().contains("severity"));
    }

    #[test]
    fn request_rejects_non_integer_severity() {
        // A non-integer `severity` fails to deserialize (serde reports the
        // expected `i32` type).
        let err =
            serde_json::from_value::<SetMonitoringLevelRequest>(json!({ "severity": "high" }))
                .unwrap_err();
        assert!(err.to_string().contains("i32") || err.to_string().contains("integer"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = SetMonitoringLevelResponse {
            status: GenericStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: SetMonitoringLevelResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = SetMonitoringLevelResponse {
            status: GenericStatusEnumType::Rejected,
            status_info: Some(StatusInfoType {
                reason_code: "OutOfRange".to_string(),
                additional_info: None,
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("Rejected"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("OutOfRange"));
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
        let err =
            serde_json::from_value::<SetMonitoringLevelResponse>(json!({ "status": "Maybe" }))
                .unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(SetMonitoringLevelRequest::ACTION_NAME, "SetMonitoringLevel");
        assert_eq!(
            SetMonitoringLevelResponse::ACTION_NAME,
            "SetMonitoringLevelResponse"
        );
    }
}
