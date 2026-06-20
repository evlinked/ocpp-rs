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
//! `StatusNotification`, and `Authorize`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    BootReasonEnumType, ChargingStationType, ConnectorStatusEnumType, CustomDataType,
    IdTokenInfoType, IdTokenType, RegistrationStatusEnumType, StatusInfoType,
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

/// `Authorize.req` — a Charging Station asks the CSMS whether an `idToken` is
/// authorized to start/stop charging.
///
/// Ports `ocpp.v201.call.Authorize`. Unlike 1.6J (a bare `idTag` string), 2.0.1
/// carries the richer [`IdTokenType`].
///
/// **Deferred:** the ISO 15118 plug-and-charge certificate path — the request's
/// optional `certificate` (PEM) and `iso15118CertificateHashData`
/// (`OCSPRequestDataType` list) — is not yet modelled here; it is tracked as a
/// follow-up. The bundled `Authorize.json` schema still validates those fields
/// when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeRequest {
    /// The identifier being authorized.
    #[serde(rename = "idToken")]
    pub id_token: IdTokenType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for AuthorizeRequest {
    const ACTION_NAME: &'static str = "Authorize";
    type Response = AuthorizeResponse;
}

/// `Authorize.conf` — the CSMS's authorization decision.
///
/// Ports `ocpp.v201.call_result.Authorize`. The [`IdTokenInfoType`] payload is
/// reused by the 2.0.1 transaction model.
///
/// **Deferred:** the optional `certificateStatus`
/// (`AuthorizeCertificateStatusEnumType`) field, part of the same ISO 15118
/// certificate path as the request-side certificate fields, is not yet
/// modelled. The bundled schema still validates it when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeResponse {
    /// Status information about the identifier.
    #[serde(rename = "idTokenInfo")]
    pub id_token_info: IdTokenInfoType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for AuthorizeResponse {
    const ACTION_NAME: &'static str = "AuthorizeResponse";
    type Response = Self;
}

impl OcppResponse for AuthorizeResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{AuthorizationStatusEnumType, IdTokenEnumType, ModemType};
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
        assert_eq!(AuthorizeRequest::ACTION_NAME, "Authorize");
        assert_eq!(AuthorizeResponse::ACTION_NAME, "AuthorizeResponse");
    }

    #[test]
    fn authorize_request_matches_reference_wire_json() {
        // Reference: tests/v201/conftest.py — Authorize.req with a bare RFID
        // idToken and nothing else.
        let req = AuthorizeRequest {
            id_token: IdTokenType {
                id_token: "045918E24B6D80".to_string(),
                kind: IdTokenEnumType::Iso14443,
                additional_info: None,
                custom_data: None,
            },
            custom_data: None,
        };
        let expected = json!({
            "idToken": {
                "idToken": "045918E24B6D80",
                "type": "ISO14443"
            }
        });
        assert_eq!(serde_json::to_value(&req).unwrap(), expected);
        let back: AuthorizeRequest = serde_json::from_value(expected).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn authorize_response_matches_reference_wire_json() {
        // Reference: tests/v201/conftest.py — Authorize.conf, status Accepted.
        let resp = AuthorizeResponse {
            id_token_info: IdTokenInfoType {
                status: AuthorizationStatusEnumType::Accepted,
                cache_expiry_date_time: None,
                charging_priority: None,
                language1: None,
                evse_id: None,
                language2: None,
                group_id_token: None,
                personal_message: None,
                custom_data: None,
            },
            custom_data: None,
        };
        let expected = json!({
            "idTokenInfo": { "status": "Accepted" }
        });
        assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
        let back: AuthorizeResponse = serde_json::from_value(expected).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn authorize_round_trips_with_all_optionals() {
        let req = AuthorizeRequest {
            id_token: IdTokenType {
                id_token: "abc".to_string(),
                kind: IdTokenEnumType::EMaid,
                additional_info: None,
                custom_data: None,
            },
            custom_data: Some(CustomDataType {
                vendor_id: "com.example".to_string(),
                extra: Default::default(),
            }),
        };
        let wire = serde_json::to_value(&req).unwrap();
        assert_eq!(wire["idToken"]["type"], json!("eMAID"));
        assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
        let back: AuthorizeRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(back, req);

        let resp = AuthorizeResponse {
            id_token_info: IdTokenInfoType {
                status: AuthorizationStatusEnumType::Blocked,
                cache_expiry_date_time: Some("2030-01-01T00:00:00Z".to_string()),
                charging_priority: None,
                language1: Some("en".to_string()),
                evse_id: None,
                language2: None,
                group_id_token: None,
                personal_message: None,
                custom_data: None,
            },
            custom_data: None,
        };
        let wire = serde_json::to_value(&resp).unwrap();
        assert_eq!(wire["idTokenInfo"]["status"], json!("Blocked"));
        let back: AuthorizeResponse = serde_json::from_value(wire).unwrap();
        assert_eq!(back, resp);
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
}
