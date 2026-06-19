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
//! `StatusNotification`, and the transaction model message `TransactionEvent`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    BootReasonEnumType, ChargingStationType, ConnectorStatusEnumType, CustomDataType, EVSEType,
    IdTokenInfoType, IdTokenType, MessageContentType, RegistrationStatusEnumType, StatusInfoType,
    TransactionEventEnumType, TransactionType, TriggerReasonEnumType,
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

/// `TransactionEvent.req` — the unified 2.0.1 transaction message that replaces
/// the 1.6J `StartTransaction` / `StopTransaction` / `MeterValues` triad.
///
/// Ports `ocpp.v201.call.TransactionEvent`. A transaction is reported as a
/// sequence of events: one `Started`, zero or more `Updated`, and one `Ended`
/// (see [`TransactionEventEnumType`]).
///
/// **Scope:** this slice omits the optional `meterValue` field (the
/// `MeterValueType` / `SampledValueType` sub-objects and their measurement
/// enums); it is deferred to a follow-up (tracked on the issue). The bundled
/// schema still validates `meterValue` when present, so adding it later is
/// purely additive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionEventRequest {
    /// Which event in the transaction's lifecycle this is.
    #[serde(rename = "eventType")]
    pub event_type: TransactionEventEnumType,
    /// The time at which the event occurred (RFC 3339 / ISO 8601).
    pub timestamp: String,
    /// What triggered this event.
    #[serde(rename = "triggerReason")]
    pub trigger_reason: TriggerReasonEnumType,
    /// Sequence number, incrementing per event within the transaction so the
    /// CSMS can detect gaps and order events received out of sequence.
    #[serde(rename = "seqNo")]
    pub seq_no: i32,
    /// State of the transaction this event belongs to.
    #[serde(rename = "transactionInfo")]
    pub transaction_info: TransactionType,
    /// Whether the Charging Station was offline when the event occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline: Option<bool>,
    /// Number of electrical phases used, if relevant.
    #[serde(rename = "numberOfPhasesUsed", skip_serializing_if = "Option::is_none")]
    pub number_of_phases_used: Option<i32>,
    /// Maximum current of the cable in amperes, if reported.
    #[serde(rename = "cableMaxCurrent", skip_serializing_if = "Option::is_none")]
    pub cable_max_current: Option<i32>,
    /// Reservation this transaction terminated, if any.
    #[serde(rename = "reservationId", skip_serializing_if = "Option::is_none")]
    pub reservation_id: Option<i32>,
    /// The EVSE (and optionally connector) for which the event is reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evse: Option<EVSEType>,
    /// The identifier that authorized the transaction.
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<IdTokenType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for TransactionEventRequest {
    const ACTION_NAME: &'static str = "TransactionEvent";
    type Response = TransactionEventResponse;
}

/// `TransactionEvent.conf` — the CSMS's reply.
///
/// Ports `ocpp.v201.call_result.TransactionEvent`. Every field is optional, so
/// an empty acknowledgement serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TransactionEventResponse {
    /// Running total cost of the transaction in the configured currency.
    #[serde(rename = "totalCost", skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    /// Charging priority granted to this transaction (-9..=9).
    #[serde(rename = "chargingPriority", skip_serializing_if = "Option::is_none")]
    pub charging_priority: Option<i32>,
    /// Updated authorization status for the transaction's identifier.
    #[serde(rename = "idTokenInfo", skip_serializing_if = "Option::is_none")]
    pub id_token_info: Option<IdTokenInfoType>,
    /// Personal message to display on the Charging Station.
    #[serde(
        rename = "updatedPersonalMessage",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_personal_message: Option<MessageContentType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for TransactionEventResponse {
    const ACTION_NAME: &'static str = "TransactionEventResponse";
    type Response = Self;
}

impl OcppResponse for TransactionEventResponse {}

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

    // --- TransactionEvent -------------------------------------------------

    mod transaction_event {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            AuthorizationStatusEnumType, ChargingStateEnumType, IdTokenEnumType, IdTokenInfoType,
            IdTokenType, MessageContentType, MessageFormatEnumType, ReasonEnumType,
            TransactionType,
        };

        #[test]
        fn started_event_matches_wire_json_and_validates() {
            // A `Started` event: EV plugged in and authorized.
            let req = TransactionEventRequest {
                event_type: TransactionEventEnumType::Started,
                timestamp: "2022-01-01T10:00:00Z".to_string(),
                trigger_reason: TriggerReasonEnumType::Authorized,
                seq_no: 0,
                transaction_info: TransactionType {
                    transaction_id: "tx-42".to_string(),
                    charging_state: Some(ChargingStateEnumType::EVConnected),
                    time_spent_charging: None,
                    stopped_reason: None,
                    remote_start_id: None,
                    custom_data: None,
                },
                offline: None,
                number_of_phases_used: None,
                cable_max_current: None,
                reservation_id: None,
                evse: Some(EVSEType {
                    id: 1,
                    connector_id: Some(1),
                    custom_data: None,
                }),
                id_token: Some(IdTokenType {
                    id_token: "045918E24B5380".to_string(),
                    type_: IdTokenEnumType::Iso14443,
                    additional_info: None,
                    custom_data: None,
                }),
                custom_data: None,
            };
            let expected = json!({
                "eventType": "Started",
                "timestamp": "2022-01-01T10:00:00Z",
                "triggerReason": "Authorized",
                "seqNo": 0,
                "transactionInfo": {
                    "transactionId": "tx-42",
                    "chargingState": "EVConnected"
                },
                "evse": { "id": 1, "connectorId": 1 },
                "idToken": { "idToken": "045918E24B5380", "type": "ISO14443" }
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: TransactionEventRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            // The CALL payload satisfies the bundled 2.0.1 schema.
            assert!(SchemaValidator::v201()
                .validate_call("TransactionEvent", &expected)
                .is_ok());
        }

        #[test]
        fn updated_event_round_trips() {
            let req = TransactionEventRequest {
                event_type: TransactionEventEnumType::Updated,
                timestamp: "2022-01-01T10:05:00Z".to_string(),
                trigger_reason: TriggerReasonEnumType::MeterValuePeriodic,
                seq_no: 1,
                transaction_info: TransactionType {
                    transaction_id: "tx-42".to_string(),
                    charging_state: Some(ChargingStateEnumType::Charging),
                    time_spent_charging: Some(300),
                    stopped_reason: None,
                    remote_start_id: None,
                    custom_data: None,
                },
                offline: Some(false),
                number_of_phases_used: Some(3),
                cable_max_current: Some(32),
                reservation_id: None,
                evse: None,
                id_token: None,
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["transactionInfo"]["timeSpentCharging"], json!(300));
            assert_eq!(wire["numberOfPhasesUsed"], json!(3));
            assert!(SchemaValidator::v201()
                .validate_call("TransactionEvent", &wire)
                .is_ok());
            let back: TransactionEventRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn ended_event_carries_stopped_reason_and_validates() {
            let req = TransactionEventRequest {
                event_type: TransactionEventEnumType::Ended,
                timestamp: "2022-01-01T11:00:00Z".to_string(),
                trigger_reason: TriggerReasonEnumType::StopAuthorized,
                seq_no: 2,
                transaction_info: TransactionType {
                    transaction_id: "tx-42".to_string(),
                    charging_state: Some(ChargingStateEnumType::Idle),
                    time_spent_charging: Some(3600),
                    stopped_reason: Some(ReasonEnumType::Local),
                    remote_start_id: None,
                    custom_data: None,
                },
                offline: None,
                number_of_phases_used: None,
                cable_max_current: None,
                reservation_id: None,
                evse: None,
                id_token: None,
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["transactionInfo"]["stoppedReason"], json!("Local"));
            assert!(SchemaValidator::v201()
                .validate_call("TransactionEvent", &wire)
                .is_ok());
            let back: TransactionEventRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_empty_is_object_and_full_round_trips() {
            let empty = TransactionEventResponse::default();
            assert_eq!(serde_json::to_value(&empty).unwrap(), json!({}));
            assert!(SchemaValidator::v201()
                .validate_call_result("TransactionEvent", &json!({}))
                .is_ok());

            let full = TransactionEventResponse {
                total_cost: Some(12.5),
                charging_priority: Some(1),
                id_token_info: Some(IdTokenInfoType {
                    status: AuthorizationStatusEnumType::Accepted,
                    cache_expiry_date_time: None,
                    charging_priority: None,
                    language1: None,
                    evse_id: None,
                    group_id_token: None,
                    language2: None,
                    personal_message: None,
                    custom_data: None,
                }),
                updated_personal_message: Some(MessageContentType {
                    format: MessageFormatEnumType::Utf8,
                    content: "Charging complete".to_string(),
                    language: Some("en".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&full).unwrap();
            assert_eq!(wire["idTokenInfo"]["status"], json!("Accepted"));
            assert_eq!(wire["updatedPersonalMessage"]["format"], json!("UTF8"));
            assert!(SchemaValidator::v201()
                .validate_call_result("TransactionEvent", &wire)
                .is_ok());
            let back: TransactionEventResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, full);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(TransactionEventRequest::ACTION_NAME, "TransactionEvent");
            assert_eq!(
                TransactionEventResponse::ACTION_NAME,
                "TransactionEventResponse"
            );
        }

        #[test]
        fn schema_rejects_missing_required_field() {
            // `seqNo` is required by the schema.
            let bad = json!({
                "eventType": "Started",
                "timestamp": "2022-01-01T10:00:00Z",
                "triggerReason": "Authorized",
                "transactionInfo": { "transactionId": "tx-42" }
            });
            assert!(SchemaValidator::v201()
                .validate_call("TransactionEvent", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_unknown_enum_value() {
            let bad = json!({
                "eventType": "Resumed",
                "timestamp": "2022-01-01T10:00:00Z",
                "triggerReason": "Authorized",
                "seqNo": 0,
                "transactionInfo": { "transactionId": "tx-42" }
            });
            assert!(SchemaValidator::v201()
                .validate_call("TransactionEvent", &bad)
                .is_err());
            // And serde rejects it too.
            assert!(serde_json::from_value::<TransactionEventRequest>(bad).is_err());
        }
    }
}
