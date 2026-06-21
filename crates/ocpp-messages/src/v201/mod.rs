//! OCPP 2.0.1 message definitions.
//!
//! Ports the CALL / CALLRESULT payload structs from mobilityhouse/ocpp
//! (`ocpp/v201/call.py`, `ocpp/v201/call_result.py`), built on the shared
//! datatypes in [`ocpp_types::v201`]. Mirrors the [`crate::v16j`] module: each
//! request/response implements [`OcppAction`](crate::OcppAction) /
//! [`OcppResponse`](crate::OcppResponse) so it slots into the same framing and
//! dispatch machinery.
//!
//! This is the foundation for **M7 — OCPP 2.0.1** and currently covers the core
//! lifecycle messages `BootNotification`, `Heartbeat`, `StatusNotification`,
//! `Authorize`, the `GetVariables`/`SetVariables` device-model read/write pair,
//! `TransactionEvent`, the standalone `MeterValues` push, the `Reset` remote
//! command, and the `RequestStartTransaction` remote-start command.
//!
//! ## Layout
//!
//! Each message lives in its own submodule and is re-exported here, so the
//! public path is unchanged (`ocpp_messages::v201::BootNotificationRequest`,
//! …). Adding a new 2.0.1 message is a new file plus one `mod` + one `pub use`
//! line below — it no longer grows a single monolithic file, which previously
//! made concurrent v201 PRs conflict by construction (see #124).

mod authorize;
mod boot_notification;
mod get_local_list_version;
mod get_variables;
mod heartbeat;
mod meter_values;
mod request_start_transaction;
mod request_stop_transaction;
mod reset;
mod set_variables;
mod status_notification;
mod transaction_event;

pub use authorize::{AuthorizeRequest, AuthorizeResponse};
pub use boot_notification::{BootNotificationRequest, BootNotificationResponse};
pub use get_local_list_version::{GetLocalListVersionRequest, GetLocalListVersionResponse};
pub use get_variables::{GetVariablesRequest, GetVariablesResponse};
pub use heartbeat::{HeartbeatRequest, HeartbeatResponse};
pub use meter_values::{MeterValuesRequest, MeterValuesResponse};
pub use request_start_transaction::{
    RequestStartTransactionRequest, RequestStartTransactionResponse,
};
pub use request_stop_transaction::{RequestStopTransactionRequest, RequestStopTransactionResponse};
pub use reset::{ResetRequest, ResetResponse};
pub use set_variables::{SetVariablesRequest, SetVariablesResponse};
pub use status_notification::{StatusNotificationRequest, StatusNotificationResponse};
pub use transaction_event::{TransactionEventRequest, TransactionEventResponse};

#[cfg(test)]
mod tests {
    use super::*;
    // The message structs are re-exported via `super::*`; pull the trait that
    // provides `ACTION_NAME` and the shared 2.0.1 datatypes/enums in directly,
    // since they now live in the per-message submodules rather than at module
    // scope. A glob keeps this stable as new messages add datatypes.
    use crate::OcppAction;
    use ocpp_types::v201::*;
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
            certificate: None,
            iso15118_certificate_hash_data: None,
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
            certificate_status: None,
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
            certificate: None,
            iso15118_certificate_hash_data: None,
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
            certificate_status: None,
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

    mod transaction_event {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            AuthorizationStatusEnumType, ChargingStateEnumType, IdTokenEnumType, IdTokenInfoType,
            IdTokenType, LocationEnumType, MeasurandEnumType, MessageContentType,
            MessageFormatEnumType, MeterValueType, PhaseEnumType, ReadingContextEnumType,
            ReasonEnumType, SampledValueType, TransactionType, UnitOfMeasureType,
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
                evse: Some(EvseType {
                    id: 1,
                    connector_id: Some(1),
                    custom_data: None,
                }),
                meter_value: None,
                id_token: Some(IdTokenType {
                    id_token: "045918E24B5380".to_string(),
                    kind: IdTokenEnumType::Iso14443,
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
                meter_value: None,
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
                meter_value: None,
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
        fn updated_event_with_meter_value_matches_wire_json_and_validates() {
            // An `Updated` event carrying a periodic meter reading: the value
            // plus several qualifying fields and a per-phase current sample.
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
                offline: None,
                number_of_phases_used: None,
                cable_max_current: None,
                reservation_id: None,
                evse: None,
                meter_value: Some(vec![MeterValueType {
                    timestamp: "2022-01-01T10:05:00Z".to_string(),
                    sampled_value: vec![
                        SampledValueType {
                            value: 1234.5,
                            context: Some(ReadingContextEnumType::SamplePeriodic),
                            measurand: Some(MeasurandEnumType::EnergyActiveImportRegister),
                            phase: None,
                            location: None,
                            signed_meter_value: None,
                            unit_of_measure: Some(UnitOfMeasureType {
                                unit: Some("Wh".to_string()),
                                multiplier: None,
                                custom_data: None,
                            }),
                            custom_data: None,
                        },
                        SampledValueType {
                            value: 16.0,
                            context: Some(ReadingContextEnumType::SamplePeriodic),
                            measurand: Some(MeasurandEnumType::CurrentImport),
                            phase: Some(PhaseEnumType::L1N),
                            location: Some(LocationEnumType::Outlet),
                            signed_meter_value: None,
                            unit_of_measure: None,
                            custom_data: None,
                        },
                    ],
                    custom_data: None,
                }]),
                id_token: None,
                custom_data: None,
            };
            let expected = json!({
                "eventType": "Updated",
                "timestamp": "2022-01-01T10:05:00Z",
                "triggerReason": "MeterValuePeriodic",
                "seqNo": 1,
                "transactionInfo": {
                    "transactionId": "tx-42",
                    "chargingState": "Charging",
                    "timeSpentCharging": 300
                },
                "meterValue": [{
                    "timestamp": "2022-01-01T10:05:00Z",
                    "sampledValue": [
                        {
                            "value": 1234.5,
                            "context": "Sample.Periodic",
                            "measurand": "Energy.Active.Import.Register",
                            "unitOfMeasure": { "unit": "Wh" }
                        },
                        {
                            "value": 16.0,
                            "context": "Sample.Periodic",
                            "measurand": "Current.Import",
                            "phase": "L1-N",
                            "location": "Outlet"
                        }
                    ]
                }]
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: TransactionEventRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            // The dotted/hyphenated measurement enums and nested meter objects
            // all satisfy the bundled 2.0.1 schema.
            assert!(SchemaValidator::v201()
                .validate_call("TransactionEvent", &expected)
                .is_ok());
        }

        #[test]
        fn meter_value_requires_non_empty_sampled_value() {
            // The schema sets `minItems: 1` on `sampledValue`; an empty list
            // must be rejected even though the Rust type would allow it.
            let bad = json!({
                "eventType": "Updated",
                "timestamp": "2022-01-01T10:05:00Z",
                "triggerReason": "MeterValuePeriodic",
                "seqNo": 1,
                "transactionInfo": { "transactionId": "tx-42" },
                "meterValue": [{
                    "timestamp": "2022-01-01T10:05:00Z",
                    "sampledValue": []
                }]
            });
            assert!(SchemaValidator::v201()
                .validate_call("TransactionEvent", &bad)
                .is_err());
        }

        #[test]
        fn response_empty_is_object_and_full_round_trips() {
            let v = SchemaValidator::v201();
            let empty = TransactionEventResponse::default();
            assert_eq!(serde_json::to_value(&empty).unwrap(), json!({}));
            assert!(v
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

    mod reset {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{ResetEnumType, ResetStatusEnumType, StatusInfoType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // The CSMS asks the whole station to reset immediately — only the
            // required `type` is present (named `kind` in Rust).
            let req = ResetRequest {
                kind: ResetEnumType::Immediate,
                evse_id: None,
                custom_data: None,
            };
            let expected = json!({ "type": "Immediate" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: ResetRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("Reset", &expected)
                .is_ok());
        }

        #[test]
        fn request_targets_single_evse_and_validates() {
            let req = ResetRequest {
                kind: ResetEnumType::OnIdle,
                evse_id: Some(2),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire, json!({ "type": "OnIdle", "evseId": 2 }));
            assert!(SchemaValidator::v201()
                .validate_call("Reset", &wire)
                .is_ok());
            let back: ResetRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = ResetResponse {
                status: ResetStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("Reset", &expected)
                .is_ok());
            let back: ResetResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_scheduled_with_status_info_round_trips() {
            let resp = ResetResponse {
                status: ResetStatusEnumType::Scheduled,
                status_info: Some(StatusInfoType {
                    reason_code: "TxOngoing".to_string(),
                    additional_info: Some("reset deferred until idle".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Scheduled"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("TxOngoing"));
            assert!(SchemaValidator::v201()
                .validate_call_result("Reset", &wire)
                .is_ok());
            let back: ResetResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(ResetRequest::ACTION_NAME, "Reset");
            assert_eq!(ResetResponse::ACTION_NAME, "ResetResponse");
        }

        #[test]
        fn schema_rejects_missing_required_type() {
            assert!(SchemaValidator::v201()
                .validate_call("Reset", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_unknown_reset_type() {
            // `Hard` is a 1.6J value, not a member of the 2.0.1 ResetEnumType.
            let bad = json!({ "type": "Hard" });
            assert!(SchemaValidator::v201()
                .validate_call("Reset", &bad)
                .is_err());
            assert!(serde_json::from_value::<ResetRequest>(bad).is_err());
        }
    }

    /// `RequestStartTransaction` — the 2.0.1 remote-start command (#132). The
    /// optional `chargingProfile` field is deferred (#136); the bundled schema
    /// still validates one when present, asserted below for forward-compat.
    mod request_start_transaction {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            IdTokenEnumType, IdTokenType, RequestStartStopStatusEnumType, StatusInfoType,
        };

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the two required fields: idToken + remoteStartId.
            let req = RequestStartTransactionRequest {
                id_token: IdTokenType {
                    id_token: "045918E24B6D80".to_string(),
                    kind: IdTokenEnumType::Iso14443,
                    additional_info: None,
                    custom_data: None,
                },
                remote_start_id: 42,
                evse_id: None,
                group_id_token: None,
                custom_data: None,
            };
            let expected = json!({
                "idToken": { "idToken": "045918E24B6D80", "type": "ISO14443" },
                "remoteStartId": 42
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: RequestStartTransactionRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("RequestStartTransaction", &expected)
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_all_optionals_and_validates() {
            let req = RequestStartTransactionRequest {
                id_token: IdTokenType {
                    id_token: "DEADBEEF".to_string(),
                    kind: IdTokenEnumType::EMaid,
                    additional_info: None,
                    custom_data: None,
                },
                remote_start_id: 7,
                evse_id: Some(1),
                group_id_token: Some(IdTokenType {
                    id_token: "PARENT01".to_string(),
                    kind: IdTokenEnumType::Central,
                    additional_info: None,
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["evseId"], json!(1));
            assert_eq!(wire["groupIdToken"]["idToken"], json!("PARENT01"));
            assert!(SchemaValidator::v201()
                .validate_call("RequestStartTransaction", &wire)
                .is_ok());
            let back: RequestStartTransactionRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_omits_optional_fields_when_absent() {
            let req = RequestStartTransactionRequest {
                id_token: IdTokenType {
                    id_token: "abc".to_string(),
                    kind: IdTokenEnumType::Iso14443,
                    additional_info: None,
                    custom_data: None,
                },
                remote_start_id: 1,
                evse_id: None,
                group_id_token: None,
                custom_data: None,
            };
            let obj = serde_json::to_value(&req).unwrap();
            let obj = obj.as_object().unwrap();
            assert!(!obj.contains_key("evseId"));
            assert!(!obj.contains_key("groupIdToken"));
            assert!(!obj.contains_key("customData"));
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = RequestStartTransactionResponse {
                status: RequestStartStopStatusEnumType::Accepted,
                status_info: None,
                transaction_id: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("RequestStartTransaction", &expected)
                .is_ok());
            let back: RequestStartTransactionResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejected_with_status_info_and_transaction_id_round_trips() {
            // The station had already started a transaction (cable plugged in
            // first), so it reports that transactionId alongside the status.
            let resp = RequestStartTransactionResponse {
                status: RequestStartStopStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "AlreadyStarted".to_string(),
                    additional_info: Some("cable plugged in first".to_string()),
                    custom_data: None,
                }),
                transaction_id: Some("tx-99".to_string()),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Rejected"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("AlreadyStarted"));
            assert_eq!(wire["transactionId"], json!("tx-99"));
            assert!(SchemaValidator::v201()
                .validate_call_result("RequestStartTransaction", &wire)
                .is_ok());
            let back: RequestStartTransactionResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            assert_eq!(
                serde_json::to_value(RequestStartStopStatusEnumType::Accepted).unwrap(),
                json!("Accepted")
            );
            assert_eq!(
                serde_json::to_value(RequestStartStopStatusEnumType::Rejected).unwrap(),
                json!("Rejected")
            );
            assert!(
                serde_json::from_value::<RequestStartStopStatusEnumType>(json!("Scheduled"))
                    .is_err()
            );
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                RequestStartTransactionRequest::ACTION_NAME,
                "RequestStartTransaction"
            );
            assert_eq!(
                RequestStartTransactionResponse::ACTION_NAME,
                "RequestStartTransactionResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_required_fields() {
            // `remoteStartId` is required alongside `idToken`.
            let bad = json!({
                "idToken": { "idToken": "abc", "type": "ISO14443" }
            });
            assert!(SchemaValidator::v201()
                .validate_call("RequestStartTransaction", &bad)
                .is_err());
            // And one missing `idToken`.
            let bad = json!({ "remoteStartId": 1 });
            assert!(SchemaValidator::v201()
                .validate_call("RequestStartTransaction", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status_and_additional_properties() {
            assert!(SchemaValidator::v201()
                .validate_call_result("RequestStartTransaction", &json!({}))
                .is_err());
            let bad = json!({ "status": "Accepted", "unexpected": true });
            assert!(SchemaValidator::v201()
                .validate_call_result("RequestStartTransaction", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_unknown_status_value() {
            // `Scheduled` is a ResetStatusEnumType value, not valid here.
            let bad = json!({ "status": "Scheduled" });
            assert!(SchemaValidator::v201()
                .validate_call_result("RequestStartTransaction", &bad)
                .is_err());
        }

        #[test]
        fn schema_still_validates_a_charging_profile_when_present() {
            // Forward-compat for the deferred `chargingProfile` field (#136):
            // a peer may already send one, and the bundled schema must accept a
            // well-formed TxProfile. Constructed as raw JSON since the Rust
            // struct does not yet carry the field.
            let req = json!({
                "idToken": { "idToken": "abc", "type": "ISO14443" },
                "remoteStartId": 1,
                "chargingProfile": {
                    "id": 1,
                    "stackLevel": 0,
                    "chargingProfilePurpose": "TxProfile",
                    "chargingProfileKind": "Absolute",
                    "transactionId": "tx-1",
                    "chargingSchedule": [{
                        "id": 1,
                        "chargingRateUnit": "A",
                        "chargingSchedulePeriod": [
                            { "startPeriod": 0, "limit": 16.0 }
                        ]
                    }]
                }
            });
            assert!(SchemaValidator::v201()
                .validate_call("RequestStartTransaction", &req)
                .is_ok());
        }
    }

    mod set_variables {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            AttributeEnumType, ComponentType, EvseType, SetVariableDataType, SetVariableResultType,
            SetVariableStatusEnumType, StatusInfoType, VariableType,
        };

        #[test]
        fn request_matches_reference_wire_json_and_validates() {
            // Set HeartbeatInterval to 300 on the OCPPCommCtrlr component.
            let req = SetVariablesRequest {
                set_variable_data: vec![SetVariableDataType {
                    attribute_value: "300".to_string(),
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
                    attribute_type: None,
                    custom_data: None,
                }],
                custom_data: None,
            };
            let expected = json!({
                "setVariableData": [{
                    "attributeValue": "300",
                    "component": { "name": "OCPPCommCtrlr" },
                    "variable": { "name": "HeartbeatInterval" }
                }]
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: SetVariablesRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            // The CALL payload satisfies the bundled 2.0.1 schema.
            assert!(SchemaValidator::v201()
                .validate_call("SetVariables", &expected)
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_all_optionals() {
            let req = SetVariablesRequest {
                set_variable_data: vec![SetVariableDataType {
                    attribute_value: "true".to_string(),
                    component: ComponentType {
                        name: "AuthCtrlr".to_string(),
                        instance: Some("Main".to_string()),
                        evse: Some(EvseType {
                            id: 1,
                            connector_id: Some(1),
                            custom_data: None,
                        }),
                        custom_data: None,
                    },
                    variable: VariableType {
                        name: "Enabled".to_string(),
                        instance: None,
                        custom_data: None,
                    },
                    attribute_type: Some(AttributeEnumType::Target),
                    custom_data: None,
                }],
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["setVariableData"][0]["attributeType"], json!("Target"));
            assert_eq!(
                wire["setVariableData"][0]["component"]["evse"]["connectorId"],
                json!(1)
            );
            assert!(SchemaValidator::v201()
                .validate_call("SetVariables", &wire)
                .is_ok());
            let back: SetVariablesRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_matches_reference_wire_json_and_validates() {
            let resp = SetVariablesResponse {
                set_variable_result: vec![SetVariableResultType {
                    attribute_status: SetVariableStatusEnumType::Accepted,
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
                    attribute_type: None,
                    attribute_status_info: None,
                    custom_data: None,
                }],
                custom_data: None,
            };
            let expected = json!({
                "setVariableResult": [{
                    "attributeStatus": "Accepted",
                    "component": { "name": "OCPPCommCtrlr" },
                    "variable": { "name": "HeartbeatInterval" }
                }]
            });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            let back: SetVariablesResponse = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, resp);
            assert!(SchemaValidator::v201()
                .validate_call_result("SetVariables", &expected)
                .is_ok());
        }

        #[test]
        fn response_reboot_required_carries_status_info_and_validates() {
            let resp = SetVariablesResponse {
                set_variable_result: vec![SetVariableResultType {
                    attribute_status: SetVariableStatusEnumType::RebootRequired,
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
                    attribute_status_info: Some(StatusInfoType {
                        reason_code: "Queued".to_string(),
                        additional_info: Some("applies after reboot".to_string()),
                        custom_data: None,
                    }),
                    custom_data: None,
                }],
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(
                wire["setVariableResult"][0]["attributeStatus"],
                json!("RebootRequired")
            );
            assert_eq!(
                wire["setVariableResult"][0]["attributeStatusInfo"]["reasonCode"],
                json!("Queued")
            );
            assert!(SchemaValidator::v201()
                .validate_call_result("SetVariables", &wire)
                .is_ok());
            let back: SetVariablesResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(SetVariablesRequest::ACTION_NAME, "SetVariables");
            assert_eq!(SetVariablesResponse::ACTION_NAME, "SetVariablesResponse");
        }

        #[test]
        fn schema_rejects_request_missing_attribute_value() {
            // `attributeValue` is required on each SetVariableData entry.
            let bad = json!({
                "setVariableData": [{
                    "component": { "name": "OCPPCommCtrlr" },
                    "variable": { "name": "HeartbeatInterval" }
                }]
            });
            assert!(SchemaValidator::v201()
                .validate_call("SetVariables", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_empty_data_array() {
            // The schema requires `minItems: 1` on `setVariableData`.
            let bad = json!({ "setVariableData": [] });
            assert!(SchemaValidator::v201()
                .validate_call("SetVariables", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let bad = json!({
                "setVariableData": [{
                    "attributeValue": "300",
                    "component": { "name": "OCPPCommCtrlr" },
                    "variable": { "name": "HeartbeatInterval" }
                }],
                "unexpected": true
            });
            assert!(SchemaValidator::v201()
                .validate_call("SetVariables", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_unknown_status_value() {
            let bad = json!({
                "setVariableResult": [{
                    "attributeStatus": "Maybe",
                    "component": { "name": "OCPPCommCtrlr" },
                    "variable": { "name": "HeartbeatInterval" }
                }]
            });
            assert!(SchemaValidator::v201()
                .validate_call_result("SetVariables", &bad)
                .is_err());
            // And serde rejects the unknown enum value too.
            assert!(serde_json::from_value::<SetVariableStatusEnumType>(json!("Maybe")).is_err());
        }
    }

    /// ISO 15118 plug-and-charge certificate path on `Authorize`
    /// (issue #117): the request's `certificate` / `iso15118CertificateHashData`
    /// and the response's `certificateStatus`.
    mod authorize_certificate {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            AuthorizeCertificateStatusEnumType, HashAlgorithmEnumType, IdTokenEnumType,
            IdTokenType, OCSPRequestDataType,
        };

        fn sample_ocsp() -> OCSPRequestDataType {
            OCSPRequestDataType {
                hash_algorithm: HashAlgorithmEnumType::Sha256,
                issuer_name_hash: "a4f8...".to_string(),
                issuer_key_hash: "b91c...".to_string(),
                serial_number: "12AB34CD".to_string(),
                responder_url: "https://ocsp.example.com".to_string(),
                custom_data: None,
            }
        }

        #[test]
        fn request_with_certificate_path_matches_wire_json_and_validates() {
            let req = AuthorizeRequest {
                id_token: IdTokenType {
                    id_token: "DEADBEEF".to_string(),
                    kind: IdTokenEnumType::EMaid,
                    additional_info: None,
                    custom_data: None,
                },
                certificate: Some(
                    "-----BEGIN CERTIFICATE-----\nMII...\n-----END CERTIFICATE-----".to_string(),
                ),
                iso15118_certificate_hash_data: Some(vec![sample_ocsp()]),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(
                wire["iso15118CertificateHashData"][0]["hashAlgorithm"],
                json!("SHA256")
            );
            assert_eq!(
                wire["iso15118CertificateHashData"][0]["responderURL"],
                json!("https://ocsp.example.com")
            );
            assert!(wire["certificate"].is_string());
            assert!(SchemaValidator::v201()
                .validate_call("Authorize", &wire)
                .is_ok());
            let back: AuthorizeRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_omits_certificate_fields_when_absent() {
            let req = AuthorizeRequest {
                id_token: IdTokenType {
                    id_token: "DEADBEEF".to_string(),
                    kind: IdTokenEnumType::Iso14443,
                    additional_info: None,
                    custom_data: None,
                },
                certificate: None,
                iso15118_certificate_hash_data: None,
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            let obj = wire.as_object().unwrap();
            assert!(!obj.contains_key("certificate"));
            assert!(!obj.contains_key("iso15118CertificateHashData"));
        }

        #[test]
        fn response_with_certificate_status_round_trips_and_validates() {
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
                certificate_status: Some(AuthorizeCertificateStatusEnumType::CertChainError),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["certificateStatus"], json!("CertChainError"));
            assert!(SchemaValidator::v201()
                .validate_call_result("Authorize", &wire)
                .is_ok());
            let back: AuthorizeResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn hash_algorithm_enum_serializes_to_wire_values() {
            assert_eq!(
                serde_json::to_value(HashAlgorithmEnumType::Sha256).unwrap(),
                json!("SHA256")
            );
            assert_eq!(
                serde_json::to_value(HashAlgorithmEnumType::Sha384).unwrap(),
                json!("SHA384")
            );
            assert_eq!(
                serde_json::to_value(HashAlgorithmEnumType::Sha512).unwrap(),
                json!("SHA512")
            );
            // Unknown values are rejected.
            assert!(serde_json::from_value::<HashAlgorithmEnumType>(json!("MD5")).is_err());
            assert!(serde_json::from_value::<HashAlgorithmEnumType>(json!("sha256")).is_err());
        }

        #[test]
        fn certificate_status_enum_serializes_to_wire_values() {
            for (variant, wire) in [
                (AuthorizeCertificateStatusEnumType::Accepted, "Accepted"),
                (
                    AuthorizeCertificateStatusEnumType::SignatureError,
                    "SignatureError",
                ),
                (
                    AuthorizeCertificateStatusEnumType::CertificateExpired,
                    "CertificateExpired",
                ),
                (
                    AuthorizeCertificateStatusEnumType::CertificateRevoked,
                    "CertificateRevoked",
                ),
                (
                    AuthorizeCertificateStatusEnumType::NoCertificateAvailable,
                    "NoCertificateAvailable",
                ),
                (
                    AuthorizeCertificateStatusEnumType::CertChainError,
                    "CertChainError",
                ),
                (
                    AuthorizeCertificateStatusEnumType::ContractCancelled,
                    "ContractCancelled",
                ),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            }
            assert!(
                serde_json::from_value::<AuthorizeCertificateStatusEnumType>(json!("Bogus"))
                    .is_err()
            );
        }

        #[test]
        fn schema_rejects_ocsp_entry_missing_required_field() {
            // `serialNumber` is required on every OCSPRequestDataType entry.
            let bad = json!({
                "idToken": { "idToken": "DEADBEEF", "type": "eMAID" },
                "iso15118CertificateHashData": [{
                    "hashAlgorithm": "SHA256",
                    "issuerNameHash": "a4f8",
                    "issuerKeyHash": "b91c",
                    "responderURL": "https://ocsp.example.com"
                }]
            });
            assert!(SchemaValidator::v201()
                .validate_call("Authorize", &bad)
                .is_err());
            assert!(serde_json::from_value::<AuthorizeRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_more_than_four_hash_data_entries() {
            // The schema caps `iso15118CertificateHashData` at maxItems = 4.
            let entry = json!({
                "hashAlgorithm": "SHA256",
                "issuerNameHash": "a4f8",
                "issuerKeyHash": "b91c",
                "serialNumber": "12AB",
                "responderURL": "https://ocsp.example.com"
            });
            let bad = json!({
                "idToken": { "idToken": "DEADBEEF", "type": "eMAID" },
                "iso15118CertificateHashData": [
                    entry, entry, entry, entry, entry
                ]
            });
            assert!(SchemaValidator::v201()
                .validate_call("Authorize", &bad)
                .is_err());
        }
    }

    /// Standalone `MeterValues` push (issue #141): the request wraps the
    /// existing `MeterValueType` tree; the response is an empty ack.
    mod meter_values {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            CustomDataType, MeasurandEnumType, MeterValueType, ReadingContextEnumType,
            SampledValueType, UnitOfMeasureType,
        };

        fn sample_meter_value() -> MeterValueType {
            MeterValueType {
                timestamp: "2022-01-01T10:05:00Z".to_string(),
                sampled_value: vec![SampledValueType {
                    value: 1234.5,
                    context: Some(ReadingContextEnumType::SamplePeriodic),
                    measurand: Some(MeasurandEnumType::EnergyActiveImportRegister),
                    phase: None,
                    location: None,
                    signed_meter_value: None,
                    unit_of_measure: Some(UnitOfMeasureType {
                        unit: Some("Wh".to_string()),
                        multiplier: None,
                        custom_data: None,
                    }),
                    custom_data: None,
                }],
                custom_data: None,
            }
        }

        #[test]
        fn request_matches_reference_wire_json_and_validates() {
            let req = MeterValuesRequest {
                evse_id: 1,
                meter_value: vec![sample_meter_value()],
                custom_data: None,
            };
            let expected = json!({
                "evseId": 1,
                "meterValue": [{
                    "timestamp": "2022-01-01T10:05:00Z",
                    "sampledValue": [{
                        "value": 1234.5,
                        "context": "Sample.Periodic",
                        "measurand": "Energy.Active.Import.Register",
                        "unitOfMeasure": { "unit": "Wh" }
                    }]
                }]
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            // `customData` is omitted when `None`.
            assert!(!expected.as_object().unwrap().contains_key("customData"));
            let back: MeterValuesRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("MeterValues", &expected)
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = MeterValuesRequest {
                evse_id: 0, // 0 = the station's main power meter.
                meter_value: vec![sample_meter_value()],
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["evseId"], json!(0));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("MeterValues", &wire)
                .is_ok());
            let back: MeterValuesRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_empty_is_object_and_full_round_trips() {
            let empty = MeterValuesResponse::default();
            assert_eq!(serde_json::to_value(&empty).unwrap(), json!({}));
            assert!(SchemaValidator::v201()
                .validate_call_result("MeterValues", &json!({}))
                .is_ok());

            let full = MeterValuesResponse {
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&full).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("MeterValues", &wire)
                .is_ok());
            let back: MeterValuesResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, full);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(MeterValuesRequest::ACTION_NAME, "MeterValues");
            assert_eq!(MeterValuesResponse::ACTION_NAME, "MeterValuesResponse");
        }

        #[test]
        fn schema_rejects_missing_required_fields() {
            let v = SchemaValidator::v201();
            // `evseId` and `meterValue` are both required.
            assert!(v
                .validate_call("MeterValues", &json!({ "meterValue": [] }))
                .is_err());
            assert!(v
                .validate_call("MeterValues", &json!({ "evseId": 1 }))
                .is_err());
        }

        #[test]
        fn schema_rejects_empty_meter_value_array() {
            // `meterValue` has `minItems: 1`.
            let bad = json!({ "evseId": 1, "meterValue": [] });
            assert!(SchemaValidator::v201()
                .validate_call("MeterValues", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_empty_sampled_value_array() {
            // Each `MeterValueType.sampledValue` also has `minItems: 1`.
            let bad = json!({
                "evseId": 1,
                "meterValue": [{
                    "timestamp": "2022-01-01T10:05:00Z",
                    "sampledValue": []
                }]
            });
            assert!(SchemaValidator::v201()
                .validate_call("MeterValues", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let bad = json!({
                "evseId": 1,
                "meterValue": [{
                    "timestamp": "2022-01-01T10:05:00Z",
                    "sampledValue": [{ "value": 1.0 }]
                }],
                "unexpected": true
            });
            assert!(SchemaValidator::v201()
                .validate_call("MeterValues", &bad)
                .is_err());
        }
    }

    /// `GetLocalListVersion` (issue #145): the smallest 2.0.1 query — an empty
    /// request, a single integer (`versionNumber`) out. No new enums/datatypes.
    mod get_local_list_version {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::CustomDataType;

        #[test]
        fn request_is_empty_object_on_wire_and_validates() {
            // GetLocalListVersion.req has no payload fields, so it serializes to
            // an empty object.
            let req = GetLocalListVersionRequest::default();
            assert_eq!(serde_json::to_value(&req).unwrap(), json!({}));
            let back: GetLocalListVersionRequest = serde_json::from_value(json!({})).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("GetLocalListVersion", &json!({}))
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = GetLocalListVersionRequest {
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("GetLocalListVersion", &wire)
                .is_ok());
            let back: GetLocalListVersionRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_matches_reference_wire_json_and_validates() {
            let resp = GetLocalListVersionResponse {
                version_number: 42,
                custom_data: None,
            };
            let expected = json!({ "versionNumber": 42 });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            // `customData` is omitted when `None`.
            assert!(!expected.as_object().unwrap().contains_key("customData"));
            assert!(SchemaValidator::v201()
                .validate_call_result("GetLocalListVersion", &expected)
                .is_ok());
            let back: GetLocalListVersionResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_round_trips_with_custom_data_and_sentinel_version() {
            // `versionNumber` of 0 means no local list is installed; -1 is the
            // documented "unknown" sentinel — both are plain integers on the
            // wire and must round-trip.
            for version in [0, -1] {
                let resp = GetLocalListVersionResponse {
                    version_number: version,
                    custom_data: Some(CustomDataType {
                        vendor_id: "com.example".to_string(),
                        extra: Default::default(),
                    }),
                };
                let wire = serde_json::to_value(&resp).unwrap();
                assert_eq!(wire["versionNumber"], json!(version));
                assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
                assert!(SchemaValidator::v201()
                    .validate_call_result("GetLocalListVersion", &wire)
                    .is_ok());
                let back: GetLocalListVersionResponse = serde_json::from_value(wire).unwrap();
                assert_eq!(back, resp);
            }
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                GetLocalListVersionRequest::ACTION_NAME,
                "GetLocalListVersion"
            );
            assert_eq!(
                GetLocalListVersionResponse::ACTION_NAME,
                "GetLocalListVersionResponse"
            );
        }

        #[test]
        fn schema_rejects_response_missing_version_number() {
            // `versionNumber` is the sole required field.
            assert!(SchemaValidator::v201()
                .validate_call_result("GetLocalListVersion", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_response_wrong_version_number_type() {
            // `versionNumber` must be an integer, not a string.
            let bad = json!({ "versionNumber": "42" });
            assert!(SchemaValidator::v201()
                .validate_call_result("GetLocalListVersion", &bad)
                .is_err());
            // And serde refuses to coerce the string into the i32 field.
            assert!(serde_json::from_value::<GetLocalListVersionResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            // The request carries no fields, so any extra key is rejected.
            let bad_req = json!({ "unexpected": true });
            assert!(SchemaValidator::v201()
                .validate_call("GetLocalListVersion", &bad_req)
                .is_err());
            // The response only permits `versionNumber` + `customData`.
            let bad_resp = json!({ "versionNumber": 1, "unexpected": true });
            assert!(SchemaValidator::v201()
                .validate_call_result("GetLocalListVersion", &bad_resp)
                .is_err());
        }
    }
}
