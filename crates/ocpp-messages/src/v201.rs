//! OCPP 2.0.1 message definitions.
//!
//! Ports the CALL / CALLRESULT payload structs from mobilityhouse/ocpp
//! (`ocpp/v201/call.py`, `ocpp/v201/call_result.py`), built on the shared
//! datatypes in [`ocpp_types::v201`]. Mirrors the [`crate::v16j`] module: each
//! request/response implements [`OcppAction`] / [`OcppResponse`] so it slots
//! into the same framing and dispatch machinery.
//!
//! This is the foundation slice for **M7 — OCPP 2.0.1** and currently covers
//! the core lifecycle messages `BootNotification`, `Heartbeat`,
//! `StatusNotification`, and the `GetVariables` device-model read (its
//! `SetVariables` counterpart is a planned follow-up).

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    BootReasonEnumType, ChargingStationType, ConnectorStatusEnumType, CustomDataType,
    GetVariableDataType, GetVariableResultType, RegistrationStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `BootNotification.req` — sent by a Charging Station to the CSMS on boot.
///
/// Ports `ocpp.v201.call.BootNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootNotificationRequest {
    /// Identity and capabilities of the booting Charging Station.
    #[serde(rename = "chargingStation")]
    pub charging_station: ChargingStationType,
    /// Why the Charging Station is sending this message.
    pub reason: BootReasonEnumType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for BootNotificationRequest {
    const ACTION_NAME: &'static str = "BootNotification";
    type Response = BootNotificationResponse;
}

/// `BootNotification.conf` — the CSMS's reply.
///
/// Ports `ocpp.v201.call_result.BootNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootNotificationResponse {
    /// The CSMS's current time (RFC 3339 / ISO 8601).
    #[serde(rename = "currentTime")]
    pub current_time: String,
    /// Heartbeat interval in seconds when `status` is `Accepted`; otherwise the
    /// minimum wait before the next `BootNotification`.
    pub interval: i32,
    /// Whether the Charging Station was accepted by the CSMS.
    pub status: RegistrationStatusEnumType,
    /// Optional detail about the registration result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for BootNotificationResponse {
    const ACTION_NAME: &'static str = "BootNotificationResponse";
    type Response = Self;
}

impl OcppResponse for BootNotificationResponse {}

/// `Heartbeat.req` — sent by a Charging Station to keep the connection alive
/// and to learn the CSMS's current time.
///
/// Ports `ocpp.v201.call.Heartbeat`. The request carries no fields beyond the
/// optional vendor extension, so it serializes to `{}` on the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for HeartbeatRequest {
    const ACTION_NAME: &'static str = "Heartbeat";
    type Response = HeartbeatResponse;
}

/// `Heartbeat.conf` — the CSMS's reply, carrying its current time.
///
/// Ports `ocpp.v201.call_result.Heartbeat`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// The CSMS's current time (RFC 3339 / ISO 8601).
    #[serde(rename = "currentTime")]
    pub current_time: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for HeartbeatResponse {
    const ACTION_NAME: &'static str = "HeartbeatResponse";
    type Response = Self;
}

impl OcppResponse for HeartbeatResponse {}

/// `StatusNotification.req` — reports the status of a single connector.
///
/// Ports `ocpp.v201.call.StatusNotification`. Unlike 1.6J, status is reported
/// per `(evseId, connectorId)` pair using [`ConnectorStatusEnumType`], and
/// there is no `errorCode` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusNotificationRequest {
    /// The time for which the status is reported (RFC 3339 / ISO 8601).
    pub timestamp: String,
    /// The reported status of the connector.
    #[serde(rename = "connectorStatus")]
    pub connector_status: ConnectorStatusEnumType,
    /// The id of the EVSE to which the connector belongs.
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// The id of the connector within the EVSE.
    #[serde(rename = "connectorId")]
    pub connector_id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for StatusNotificationRequest {
    const ACTION_NAME: &'static str = "StatusNotification";
    type Response = StatusNotificationResponse;
}

/// `StatusNotification.conf` — the CSMS's acknowledgement.
///
/// Ports `ocpp.v201.call_result.StatusNotification`. The response carries no
/// fields beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StatusNotificationResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for StatusNotificationResponse {
    const ACTION_NAME: &'static str = "StatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for StatusNotificationResponse {}

/// `GetVariables.req` — sent by the CSMS to read one or more
/// component-variable attributes from a Charging Station.
///
/// Ports `ocpp.v201.call.GetVariables`. The 2.0.1 device-model replacement for
/// 1.6J `GetConfiguration`: instead of flat string keys, each entry names a
/// `component`/`variable` pair (see [`GetVariableDataType`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetVariablesRequest {
    /// The variables (and attributes) to read. Per the schema at least one
    /// entry must be present.
    #[serde(rename = "getVariableData")]
    pub get_variable_data: Vec<GetVariableDataType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetVariablesRequest {
    const ACTION_NAME: &'static str = "GetVariables";
    type Response = GetVariablesResponse;
}

/// `GetVariables.conf` — the Charging Station's reply, one result per requested
/// variable (order corresponds to the request).
///
/// Ports `ocpp.v201.call_result.GetVariables`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetVariablesResponse {
    /// One result per requested variable.
    #[serde(rename = "getVariableResult")]
    pub get_variable_result: Vec<GetVariableResultType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetVariablesResponse {
    const ACTION_NAME: &'static str = "GetVariablesResponse";
    type Response = Self;
}

impl OcppResponse for GetVariablesResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::ModemType;
    use serde_json::json;

    #[test]
    fn boot_request_matches_reference_wire_json() {
        // Ported from tests/v201/conftest.py + test_v201_charge_point.py.
        let req = BootNotificationRequest {
            charging_station: ChargingStationType {
                vendor_name: "ICU Eve Mini".to_string(),
                model: "ICU Eve Mini".to_string(),
                serial_number: None,
                firmware_version: Some("#1:3.4.0-2990#N:217H;1.0-223".to_string()),
                modem: None,
                custom_data: None,
            },
            reason: BootReasonEnumType::PowerUp,
            custom_data: None,
        };
        let expected = json!({
            "chargingStation": {
                "vendorName": "ICU Eve Mini",
                "model": "ICU Eve Mini",
                "firmwareVersion": "#1:3.4.0-2990#N:217H;1.0-223"
            },
            "reason": "PowerUp"
        });
        assert_eq!(serde_json::to_value(&req).unwrap(), expected);
        // Deserialization is faithful (round-trip).
        let back: BootNotificationRequest = serde_json::from_value(expected).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn boot_response_matches_reference_wire_json() {
        // Ported from tests/v201/conftest.py mock_base_central_system.
        let resp = BootNotificationResponse {
            current_time: "2018-05-29T17:37:05.495259".to_string(),
            interval: 350,
            status: RegistrationStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let expected = json!({
            "currentTime": "2018-05-29T17:37:05.495259",
            "interval": 350,
            "status": "Accepted"
        });
        assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
        let back: BootNotificationResponse = serde_json::from_value(expected).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(BootNotificationRequest::ACTION_NAME, "BootNotification");
        assert_eq!(
            BootNotificationResponse::ACTION_NAME,
            "BootNotificationResponse"
        );
    }

    #[test]
    fn full_request_round_trips_with_all_optionals() {
        let req = BootNotificationRequest {
            charging_station: ChargingStationType {
                vendor_name: "Vendor".to_string(),
                model: "Model".to_string(),
                serial_number: Some("SN-1".to_string()),
                firmware_version: Some("1.0".to_string()),
                modem: Some(ModemType {
                    iccid: Some("89000000".to_string()),
                    imsi: Some("26201".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            },
            reason: BootReasonEnumType::RemoteReset,
            custom_data: None,
        };
        let wire = serde_json::to_value(&req).unwrap();
        let back: BootNotificationRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn response_with_status_info_round_trips() {
        let resp = BootNotificationResponse {
            current_time: "2018-05-29T17:37:05Z".to_string(),
            interval: 60,
            status: RegistrationStatusEnumType::Pending,
            status_info: Some(StatusInfoType {
                reason_code: "PendingConfig".to_string(),
                additional_info: Some("awaiting provisioning".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&resp).unwrap();
        assert_eq!(wire["statusInfo"]["reasonCode"], json!("PendingConfig"));
        let back: BootNotificationResponse = serde_json::from_value(wire).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn heartbeat_request_is_empty_object_on_wire() {
        // Ported from tests/v201/test_charge_point.py — Heartbeat.req has no
        // payload fields, so it serializes to an empty object.
        let req = HeartbeatRequest::default();
        assert_eq!(serde_json::to_value(&req).unwrap(), json!({}));
        let back: HeartbeatRequest = serde_json::from_value(json!({})).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn heartbeat_response_matches_reference_wire_json() {
        let resp = HeartbeatResponse {
            current_time: "2020-01-01T00:00:00Z".to_string(),
            custom_data: None,
        };
        assert_eq!(
            serde_json::to_value(&resp).unwrap(),
            json!({ "currentTime": "2020-01-01T00:00:00Z" })
        );
        let back: HeartbeatResponse =
            serde_json::from_value(json!({ "currentTime": "2020-01-01T00:00:00Z" })).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn status_notification_request_matches_reference_wire_json() {
        let req = StatusNotificationRequest {
            timestamp: "2020-01-01T00:00:00Z".to_string(),
            connector_status: ConnectorStatusEnumType::Available,
            evse_id: 1,
            connector_id: 2,
            custom_data: None,
        };
        let expected = json!({
            "timestamp": "2020-01-01T00:00:00Z",
            "connectorStatus": "Available",
            "evseId": 1,
            "connectorId": 2
        });
        assert_eq!(serde_json::to_value(&req).unwrap(), expected);
        let back: StatusNotificationRequest = serde_json::from_value(expected).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn status_notification_response_is_empty_object_on_wire() {
        let resp = StatusNotificationResponse::default();
        assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
        let back: StatusNotificationResponse = serde_json::from_value(json!({})).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn new_v201_action_names_are_stable() {
        assert_eq!(HeartbeatRequest::ACTION_NAME, "Heartbeat");
        assert_eq!(HeartbeatResponse::ACTION_NAME, "HeartbeatResponse");
        assert_eq!(StatusNotificationRequest::ACTION_NAME, "StatusNotification");
        assert_eq!(
            StatusNotificationResponse::ACTION_NAME,
            "StatusNotificationResponse"
        );
    }

    #[test]
    fn status_notification_round_trips_with_custom_data() {
        let req = StatusNotificationRequest {
            timestamp: "2020-01-01T00:00:00Z".to_string(),
            connector_status: ConnectorStatusEnumType::Faulted,
            evse_id: 0,
            connector_id: 1,
            custom_data: Some(CustomDataType {
                vendor_id: "com.example".to_string(),
                extra: Default::default(),
            }),
        };
        let wire = serde_json::to_value(&req).unwrap();
        assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
        let back: StatusNotificationRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn get_variables_request_round_trips() {
        use ocpp_types::v201::{ComponentType, EvseType, VariableType};

        let req = GetVariablesRequest {
            get_variable_data: vec![GetVariableDataType {
                component: ComponentType {
                    name: "SampledDataCtrlr".to_string(),
                    instance: None,
                    evse: Some(EvseType {
                        id: 1,
                        connector_id: None,
                        custom_data: None,
                    }),
                    custom_data: None,
                },
                variable: VariableType {
                    name: "TxEndedMeasurands".to_string(),
                    instance: None,
                    custom_data: None,
                },
                attribute_type: None,
                custom_data: None,
            }],
            custom_data: None,
        };
        let expected = json!({
            "getVariableData": [{
                "component": { "name": "SampledDataCtrlr", "evse": { "id": 1 } },
                "variable": { "name": "TxEndedMeasurands" }
            }]
        });
        assert_eq!(serde_json::to_value(&req).unwrap(), expected);
        let back: GetVariablesRequest = serde_json::from_value(expected).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn get_variables_response_round_trips() {
        use ocpp_types::v201::{
            AttributeEnumType, ComponentType, GetVariableStatusEnumType, VariableType,
        };

        let resp = GetVariablesResponse {
            get_variable_result: vec![GetVariableResultType {
                attribute_status: GetVariableStatusEnumType::Accepted,
                component: ComponentType {
                    name: "OCPPCommCtrlr".to_string(),
                    instance: None,
                    evse: None,
                    custom_data: None,
                },
                variable: VariableType {
                    name: "HeartbeatInterval".to_string(),
                    instance: None,
                    custom_data: None,
                },
                attribute_type: Some(AttributeEnumType::Actual),
                attribute_value: Some("300".to_string()),
                attribute_status_info: None,
                custom_data: None,
            }],
            custom_data: None,
        };
        let expected = json!({
            "getVariableResult": [{
                "attributeStatus": "Accepted",
                "component": { "name": "OCPPCommCtrlr" },
                "variable": { "name": "HeartbeatInterval" },
                "attributeType": "Actual",
                "attributeValue": "300"
            }]
        });
        assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
        let back: GetVariablesResponse = serde_json::from_value(expected).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn get_variables_action_names() {
        assert_eq!(GetVariablesRequest::ACTION_NAME, "GetVariables");
        assert_eq!(GetVariablesResponse::ACTION_NAME, "GetVariablesResponse");
    }
}
