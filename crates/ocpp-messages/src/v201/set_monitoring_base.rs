//! `SetMonitoringBase` — the CSMS tells a Charging Station which set of
//! pre-configured variable monitors to activate: `All` monitors, the
//! `FactoryDefault` set, or only the `HardWiredOnly` monitors.
//!
//! Ports `ocpp.v201.call.SetMonitoringBase` /
//! `ocpp.v201.call_result.SetMonitoringBase`. The station answers
//! synchronously with a [`GenericDeviceModelStatusEnumType`] — the same shared
//! response status used across the device-model family (`GetReport`,
//! `GetMonitoringReport`, `SetMonitoringBase`).
//!
//! [`GenericDeviceModelStatusEnumType`]: ocpp_types::v201::GenericDeviceModelStatusEnumType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, GenericDeviceModelStatusEnumType, MonitorBaseEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `SetMonitoringBase.req` — sent by the CSMS to set the active monitoring base
/// on a Charging Station.
///
/// Ports `ocpp.v201.call.SetMonitoringBase`. `monitoring_base` selects which
/// pre-configured set of variable monitors becomes active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMonitoringBaseRequest {
    /// Which monitoring base to activate: `All`, `FactoryDefault`, or
    /// `HardWiredOnly`.
    #[serde(rename = "monitoringBase")]
    pub monitoring_base: MonitorBaseEnumType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetMonitoringBaseRequest {
    const ACTION_NAME: &'static str = "SetMonitoringBase";
    type Response = SetMonitoringBaseResponse;
}

/// `SetMonitoringBase.conf` — the Charging Station's synchronous
/// acknowledgement.
///
/// Ports `ocpp.v201.call_result.SetMonitoringBase`. `status` reports whether the
/// station accepted the new monitoring base.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMonitoringBaseResponse {
    /// Whether the station accepted the requested monitoring base (`Accepted`,
    /// `Rejected`, `NotSupported`, or `EmptyResultSet`).
    pub status: GenericDeviceModelStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetMonitoringBaseResponse {
    const ACTION_NAME: &'static str = "SetMonitoringBaseResponse";
    type Response = Self;
}

impl OcppResponse for SetMonitoringBaseResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = SetMonitoringBaseRequest {
            monitoring_base: MonitorBaseEnumType::All,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`.
        assert_eq!(value, json!({ "monitoringBase": "All" }));
        let parsed: SetMonitoringBaseRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn monitoring_base_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(MonitorBaseEnumType::All).unwrap(),
            json!("All")
        );
        assert_eq!(
            serde_json::to_value(MonitorBaseEnumType::FactoryDefault).unwrap(),
            json!("FactoryDefault")
        );
        assert_eq!(
            serde_json::to_value(MonitorBaseEnumType::HardWiredOnly).unwrap(),
            json!("HardWiredOnly")
        );
    }

    #[test]
    fn request_missing_monitoring_base_fails() {
        let err = serde_json::from_value::<SetMonitoringBaseRequest>(json!({})).unwrap_err();
        assert!(err.to_string().contains("monitoringBase"));
    }

    #[test]
    fn request_rejects_unknown_monitoring_base() {
        let err = serde_json::from_value::<SetMonitoringBaseRequest>(
            json!({ "monitoringBase": "SomeMonitors" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("SomeMonitors") || err.to_string().contains("variant"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = SetMonitoringBaseResponse {
            status: GenericDeviceModelStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: SetMonitoringBaseResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = SetMonitoringBaseResponse {
            status: GenericDeviceModelStatusEnumType::Rejected,
            status_info: Some(StatusInfoType {
                reason_code: "NotEnabled".to_string(),
                additional_info: None,
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("Rejected"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("NotEnabled"));
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err = serde_json::from_value::<SetMonitoringBaseResponse>(json!({ "status": "Maybe" }))
            .unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(SetMonitoringBaseRequest::ACTION_NAME, "SetMonitoringBase");
        assert_eq!(
            SetMonitoringBaseResponse::ACTION_NAME,
            "SetMonitoringBaseResponse"
        );
    }
}
