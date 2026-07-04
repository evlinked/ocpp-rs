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
mod cancel_reservation;
mod change_availability;
mod clear_cache;
mod clear_charging_profile;
mod clear_display_message;
mod clear_variable_monitoring;
mod cleared_charging_limit;
mod cost_updated;
mod data_transfer;
mod firmware_status_notification;
mod get_base_report;
mod get_composite_schedule;
mod get_local_list_version;
mod get_monitoring_report;
mod get_report;
mod get_transaction_status;
mod get_variables;
mod heartbeat;
mod log_status_notification;
mod meter_values;
mod notify_monitoring_report;
mod publish_firmware;
mod publish_firmware_status_notification;
mod request_start_transaction;
mod request_stop_transaction;
mod reservation_status_update;
mod reserve_now;
mod reset;
mod security_event_notification;
mod send_local_list;
mod set_charging_profile;
mod set_monitoring_base;
mod set_monitoring_level;
mod set_variable_monitoring;
mod set_variables;
mod status_notification;
mod transaction_event;
mod trigger_message;
mod unlock_connector;
mod unpublish_firmware;
mod update_firmware;

pub use authorize::{AuthorizeRequest, AuthorizeResponse};
pub use boot_notification::{BootNotificationRequest, BootNotificationResponse};
pub use cancel_reservation::{CancelReservationRequest, CancelReservationResponse};
pub use change_availability::{ChangeAvailabilityRequest, ChangeAvailabilityResponse};
pub use clear_cache::{ClearCacheRequest, ClearCacheResponse};
pub use clear_charging_profile::{ClearChargingProfileRequest, ClearChargingProfileResponse};
pub use clear_display_message::{ClearDisplayMessageRequest, ClearDisplayMessageResponse};
pub use clear_variable_monitoring::{
    ClearVariableMonitoringRequest, ClearVariableMonitoringResponse,
};
pub use cleared_charging_limit::{ClearedChargingLimitRequest, ClearedChargingLimitResponse};
pub use cost_updated::{CostUpdatedRequest, CostUpdatedResponse};
pub use data_transfer::{DataTransferRequest, DataTransferResponse};
pub use firmware_status_notification::{
    FirmwareStatusNotificationRequest, FirmwareStatusNotificationResponse,
};
pub use get_base_report::{GetBaseReportRequest, GetBaseReportResponse};
pub use get_composite_schedule::{GetCompositeScheduleRequest, GetCompositeScheduleResponse};
pub use get_local_list_version::{GetLocalListVersionRequest, GetLocalListVersionResponse};
pub use get_monitoring_report::{GetMonitoringReportRequest, GetMonitoringReportResponse};
pub use get_report::{GetReportRequest, GetReportResponse};
pub use get_transaction_status::{GetTransactionStatusRequest, GetTransactionStatusResponse};
pub use get_variables::{GetVariablesRequest, GetVariablesResponse};
pub use heartbeat::{HeartbeatRequest, HeartbeatResponse};
pub use log_status_notification::{LogStatusNotificationRequest, LogStatusNotificationResponse};
pub use meter_values::{MeterValuesRequest, MeterValuesResponse};
pub use notify_monitoring_report::{NotifyMonitoringReportRequest, NotifyMonitoringReportResponse};
pub use publish_firmware::{PublishFirmwareRequest, PublishFirmwareResponse};
pub use publish_firmware_status_notification::{
    PublishFirmwareStatusNotificationRequest, PublishFirmwareStatusNotificationResponse,
};
pub use request_start_transaction::{
    RequestStartTransactionRequest, RequestStartTransactionResponse,
};
pub use request_stop_transaction::{RequestStopTransactionRequest, RequestStopTransactionResponse};
pub use reservation_status_update::{
    ReservationStatusUpdateRequest, ReservationStatusUpdateResponse,
};
pub use reserve_now::{ReserveNowRequest, ReserveNowResponse};
pub use reset::{ResetRequest, ResetResponse};
pub use security_event_notification::{
    SecurityEventNotificationRequest, SecurityEventNotificationResponse,
};
pub use send_local_list::{SendLocalListRequest, SendLocalListResponse};
pub use set_charging_profile::{SetChargingProfileRequest, SetChargingProfileResponse};
pub use set_monitoring_base::{SetMonitoringBaseRequest, SetMonitoringBaseResponse};
pub use set_monitoring_level::{SetMonitoringLevelRequest, SetMonitoringLevelResponse};
pub use set_variable_monitoring::{SetVariableMonitoringRequest, SetVariableMonitoringResponse};
pub use set_variables::{SetVariablesRequest, SetVariablesResponse};
pub use status_notification::{StatusNotificationRequest, StatusNotificationResponse};
pub use transaction_event::{TransactionEventRequest, TransactionEventResponse};
pub use trigger_message::{TriggerMessageRequest, TriggerMessageResponse};
pub use unlock_connector::{UnlockConnectorRequest, UnlockConnectorResponse};
pub use unpublish_firmware::{UnpublishFirmwareRequest, UnpublishFirmwareResponse};
pub use update_firmware::{UpdateFirmwareRequest, UpdateFirmwareResponse};

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

    mod clear_cache {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{ClearCacheStatusEnumType, CustomDataType, StatusInfoType};

        #[test]
        fn empty_request_serializes_to_object_and_validates() {
            // No vendor extension — the request carries no fields, so it is `{}`
            // on the wire.
            let req = ClearCacheRequest::default();
            let expected = json!({});
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: ClearCacheRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("ClearCache", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_custom_data_round_trips() {
            let req = ClearCacheRequest {
                custom_data: Some(CustomDataType {
                    vendor_id: "ACME".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("ACME"));
            assert!(SchemaValidator::v201()
                .validate_call("ClearCache", &wire)
                .is_ok());
            let back: ClearCacheRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = ClearCacheResponse {
                status: ClearCacheStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearCache", &expected)
                .is_ok());
            let back: ClearCacheResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejected_with_status_info_round_trips() {
            let resp = ClearCacheResponse {
                status: ClearCacheStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "NotSupported".to_string(),
                    additional_info: Some("no local cache".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Rejected"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotSupported"));
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearCache", &wire)
                .is_ok());
            let back: ClearCacheResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(ClearCacheRequest::ACTION_NAME, "ClearCache");
            assert_eq!(ClearCacheResponse::ACTION_NAME, "ClearCacheResponse");
        }

        #[test]
        fn schema_rejects_missing_required_status() {
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearCache", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_unknown_status() {
            // `Scheduled` is a ResetStatusEnumType value, not valid here.
            let bad = json!({ "status": "Scheduled" });
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearCache", &bad)
                .is_err());
            assert!(serde_json::from_value::<ClearCacheResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let bad = json!({ "status": "Accepted", "bogus": true });
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearCache", &bad)
                .is_err());
        }
    }

    /// `ClearChargingProfile` — the 2.0.1 clear-charging-profile command (#202).
    /// Teardown counterpart to `SetChargingProfile`; the request has no required
    /// fields (an empty `{}` means "clear all"), reuses
    /// `ChargingProfilePurposeEnumType` in its filter, and the response returns a
    /// two-value `ClearChargingProfileStatusEnumType` plus optional
    /// `StatusInfoType`.
    mod clear_charging_profile {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            ChargingProfilePurposeEnumType, ClearChargingProfileStatusEnumType,
            ClearChargingProfileType, CustomDataType, StatusInfoType,
        };

        #[test]
        fn empty_request_means_clear_all_and_validates() {
            // No fields set → `{}` on the wire, meaning "clear every profile".
            let req = ClearChargingProfileRequest::default();
            let expected = json!({});
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: ClearChargingProfileRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("ClearChargingProfile", &expected)
                .is_ok());
        }

        #[test]
        fn request_by_profile_id_matches_wire_json_and_validates() {
            let req = ClearChargingProfileRequest {
                charging_profile_id: Some(7),
                charging_profile_criteria: None,
                custom_data: None,
            };
            let expected = json!({ "chargingProfileId": 7 });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: ClearChargingProfileRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("ClearChargingProfile", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_full_criteria_matches_wire_json_and_validates() {
            let req = ClearChargingProfileRequest {
                charging_profile_id: None,
                charging_profile_criteria: Some(ClearChargingProfileType {
                    evse_id: Some(1),
                    charging_profile_purpose: Some(
                        ChargingProfilePurposeEnumType::TxDefaultProfile,
                    ),
                    stack_level: Some(3),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let expected = json!({
                "chargingProfileCriteria": {
                    "evseId": 1,
                    "chargingProfilePurpose": "TxDefaultProfile",
                    "stackLevel": 3
                }
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: ClearChargingProfileRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("ClearChargingProfile", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_empty_criteria_object_validates() {
            // An empty criteria object is valid (no required fields) and matches
            // "every profile on the station wide overall limit for evseId 0".
            let req = ClearChargingProfileRequest {
                charging_profile_id: None,
                charging_profile_criteria: Some(ClearChargingProfileType {
                    evse_id: Some(0),
                    charging_profile_purpose: None,
                    stack_level: None,
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire, json!({ "chargingProfileCriteria": { "evseId": 0 } }));
            assert!(SchemaValidator::v201()
                .validate_call("ClearChargingProfile", &wire)
                .is_ok());
            // A truly empty criteria object also validates.
            assert!(SchemaValidator::v201()
                .validate_call(
                    "ClearChargingProfile",
                    &json!({ "chargingProfileCriteria": {} })
                )
                .is_ok());
            let back: ClearChargingProfileRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = ClearChargingProfileRequest {
                charging_profile_id: Some(42),
                charging_profile_criteria: None,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("ClearChargingProfile", &wire)
                .is_ok());
            let back: ClearChargingProfileRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = ClearChargingProfileResponse {
                status: ClearChargingProfileStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearChargingProfile", &expected)
                .is_ok());
            let back: ClearChargingProfileResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_unknown_with_status_info_round_trips() {
            let resp = ClearChargingProfileResponse {
                status: ClearChargingProfileStatusEnumType::Unknown,
                status_info: Some(StatusInfoType {
                    reason_code: "NoProfile".to_string(),
                    additional_info: Some("no profile matched the criteria".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Unknown"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NoProfile"));
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearChargingProfile", &wire)
                .is_ok());
            let back: ClearChargingProfileResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            assert_eq!(
                serde_json::to_value(ClearChargingProfileStatusEnumType::Accepted).unwrap(),
                json!("Accepted")
            );
            assert_eq!(
                serde_json::to_value(ClearChargingProfileStatusEnumType::Unknown).unwrap(),
                json!("Unknown")
            );
            // `Rejected` is a `ChargingProfileStatusEnumType` value, not valid here.
            assert!(
                serde_json::from_value::<ClearChargingProfileStatusEnumType>(json!("Rejected"))
                    .is_err()
            );
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                ClearChargingProfileRequest::ACTION_NAME,
                "ClearChargingProfile"
            );
            assert_eq!(
                ClearChargingProfileResponse::ACTION_NAME,
                "ClearChargingProfileResponse"
            );
        }

        #[test]
        fn schema_rejects_response_missing_status() {
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearChargingProfile", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_unknown_status() {
            let bad = json!({ "status": "Rejected" });
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearChargingProfile", &bad)
                .is_err());
            assert!(serde_json::from_value::<ClearChargingProfileResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_non_integer_profile_id() {
            let bad = json!({ "chargingProfileId": "7" });
            assert!(SchemaValidator::v201()
                .validate_call("ClearChargingProfile", &bad)
                .is_err());
            assert!(serde_json::from_value::<ClearChargingProfileRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            // On the request itself.
            assert!(v
                .validate_call(
                    "ClearChargingProfile",
                    &json!({ "chargingProfileId": 1, "bogus": true })
                )
                .is_err());
            // Inside the criteria object.
            assert!(v
                .validate_call(
                    "ClearChargingProfile",
                    &json!({ "chargingProfileCriteria": { "bogus": true } })
                )
                .is_err());
            // On the response.
            assert!(v
                .validate_call_result(
                    "ClearChargingProfile",
                    &json!({ "status": "Accepted", "bogus": true })
                )
                .is_err());
        }
    }

    mod cancel_reservation {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CancelReservationStatusEnumType, CustomDataType, StatusInfoType};

        #[test]
        fn request_matches_wire_json_and_validates() {
            let req = CancelReservationRequest {
                reservation_id: 42,
                custom_data: None,
            };
            let expected = json!({ "reservationId": 42 });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("CancelReservation", &expected)
                .is_ok());
            let back: CancelReservationRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_with_custom_data_round_trips() {
            let req = CancelReservationRequest {
                reservation_id: -1,
                custom_data: Some(CustomDataType {
                    vendor_id: "ACME".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["reservationId"], json!(-1));
            assert_eq!(wire["customData"]["vendorId"], json!("ACME"));
            assert!(SchemaValidator::v201()
                .validate_call("CancelReservation", &wire)
                .is_ok());
            let back: CancelReservationRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = CancelReservationResponse {
                status: CancelReservationStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("CancelReservation", &expected)
                .is_ok());
            let back: CancelReservationResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejected_with_status_info_round_trips() {
            let resp = CancelReservationResponse {
                status: CancelReservationStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "NoReservation".to_string(),
                    additional_info: Some("unknown reservationId".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Rejected"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NoReservation"));
            assert!(SchemaValidator::v201()
                .validate_call_result("CancelReservation", &wire)
                .is_ok());
            let back: CancelReservationResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(CancelReservationRequest::ACTION_NAME, "CancelReservation");
            assert_eq!(
                CancelReservationResponse::ACTION_NAME,
                "CancelReservationResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_reservation_id() {
            assert!(SchemaValidator::v201()
                .validate_call("CancelReservation", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_non_integer_reservation_id() {
            let bad = json!({ "reservationId": "42" });
            assert!(SchemaValidator::v201()
                .validate_call("CancelReservation", &bad)
                .is_err());
            assert!(serde_json::from_value::<CancelReservationRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status() {
            assert!(SchemaValidator::v201()
                .validate_call_result("CancelReservation", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_unknown_status() {
            // `Scheduled` is a ResetStatusEnumType value, not valid here.
            let bad = json!({ "status": "Scheduled" });
            assert!(SchemaValidator::v201()
                .validate_call_result("CancelReservation", &bad)
                .is_err());
            assert!(serde_json::from_value::<CancelReservationResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            assert!(SchemaValidator::v201()
                .validate_call(
                    "CancelReservation",
                    &json!({ "reservationId": 1, "bogus": true })
                )
                .is_err());
            assert!(SchemaValidator::v201()
                .validate_call_result(
                    "CancelReservation",
                    &json!({ "status": "Accepted", "bogus": true })
                )
                .is_err());
        }
    }

    /// `ChangeAvailability` — the 2.0.1 operational-status command (#146).
    /// Targets the whole station when `evse` is omitted, or a single EVSE when
    /// present; reuses `EvseType` and `StatusInfoType`.
    mod change_availability {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            ChangeAvailabilityStatusEnumType, EvseType, OperationalStatusEnumType, StatusInfoType,
        };

        #[test]
        fn request_targets_whole_station_and_validates() {
            // No `evse` → the change applies to the entire Charging Station.
            let req = ChangeAvailabilityRequest {
                operational_status: OperationalStatusEnumType::Inoperative,
                evse: None,
                custom_data: None,
            };
            let expected = json!({ "operationalStatus": "Inoperative" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: ChangeAvailabilityRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("ChangeAvailability", &expected)
                .is_ok());
        }

        #[test]
        fn request_targets_single_evse_and_validates() {
            let req = ChangeAvailabilityRequest {
                operational_status: OperationalStatusEnumType::Operative,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: Some(2),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(
                wire,
                json!({
                    "operationalStatus": "Operative",
                    "evse": { "id": 1, "connectorId": 2 }
                })
            );
            assert!(SchemaValidator::v201()
                .validate_call("ChangeAvailability", &wire)
                .is_ok());
            let back: ChangeAvailabilityRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = ChangeAvailabilityResponse {
                status: ChangeAvailabilityStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("ChangeAvailability", &expected)
                .is_ok());
            let back: ChangeAvailabilityResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_scheduled_with_status_info_round_trips() {
            // `Scheduled` is rejected by the reference dataclass enum but valid
            // per the FINAL schema — the change is deferred until idle.
            let resp = ChangeAvailabilityResponse {
                status: ChangeAvailabilityStatusEnumType::Scheduled,
                status_info: Some(StatusInfoType {
                    reason_code: "TxOngoing".to_string(),
                    additional_info: Some("change deferred until idle".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Scheduled"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("TxOngoing"));
            assert!(SchemaValidator::v201()
                .validate_call_result("ChangeAvailability", &wire)
                .is_ok());
            let back: ChangeAvailabilityResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(ChangeAvailabilityRequest::ACTION_NAME, "ChangeAvailability");
            assert_eq!(
                ChangeAvailabilityResponse::ACTION_NAME,
                "ChangeAvailabilityResponse"
            );
        }

        #[test]
        fn operational_status_serializes_pascal_case() {
            assert_eq!(
                serde_json::to_value(OperationalStatusEnumType::Inoperative).unwrap(),
                json!("Inoperative")
            );
            assert_eq!(
                serde_json::to_value(OperationalStatusEnumType::Operative).unwrap(),
                json!("Operative")
            );
        }

        #[test]
        fn schema_and_serde_reject_unknown_operational_status() {
            // `Scheduled` is a response status, not a valid operationalStatus.
            let bad = json!({ "operationalStatus": "Scheduled" });
            assert!(SchemaValidator::v201()
                .validate_call("ChangeAvailability", &bad)
                .is_err());
            assert!(serde_json::from_value::<ChangeAvailabilityRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_missing_required_fields() {
            assert!(SchemaValidator::v201()
                .validate_call("ChangeAvailability", &json!({}))
                .is_err());
            assert!(SchemaValidator::v201()
                .validate_call_result("ChangeAvailability", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let bad = json!({ "operationalStatus": "Operative", "bogusExtra": true });
            assert!(SchemaValidator::v201()
                .validate_call("ChangeAvailability", &bad)
                .is_err());
        }
    }

    /// `UnlockConnector` — the 2.0.1 connector-unlock command (#147). Two
    /// required ids in, an [`UnlockStatusEnumType`] out; reuses `StatusInfoType`.
    mod unlock_connector {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CustomDataType, StatusInfoType, UnlockStatusEnumType};

        #[test]
        fn request_matches_wire_json_and_validates() {
            let req = UnlockConnectorRequest {
                evse_id: 1,
                connector_id: 2,
                custom_data: None,
            };
            let expected = json!({ "evseId": 1, "connectorId": 2 });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: UnlockConnectorRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("UnlockConnector", &expected)
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = UnlockConnectorRequest {
                evse_id: 3,
                connector_id: 1,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("UnlockConnector", &wire)
                .is_ok());
            let back: UnlockConnectorRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = UnlockConnectorResponse {
                status: UnlockStatusEnumType::Unlocked,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Unlocked" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("UnlockConnector", &expected)
                .is_ok());
            let back: UnlockConnectorResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_failed_with_status_info_round_trips() {
            let resp = UnlockConnectorResponse {
                status: UnlockStatusEnumType::UnlockFailed,
                status_info: Some(StatusInfoType {
                    reason_code: "Jammed".to_string(),
                    additional_info: Some("connector lock motor stalled".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("UnlockFailed"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("Jammed"));
            assert!(SchemaValidator::v201()
                .validate_call_result("UnlockConnector", &wire)
                .is_ok());
            let back: UnlockConnectorResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            for (variant, wire) in [
                (UnlockStatusEnumType::Unlocked, "Unlocked"),
                (UnlockStatusEnumType::UnlockFailed, "UnlockFailed"),
                (
                    UnlockStatusEnumType::OngoingAuthorizedTransaction,
                    "OngoingAuthorizedTransaction",
                ),
                (UnlockStatusEnumType::UnknownConnector, "UnknownConnector"),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            }
            // `NotSupported` is a 1.6J UnlockStatus value, not valid in 2.0.1.
            assert!(serde_json::from_value::<UnlockStatusEnumType>(json!("NotSupported")).is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(UnlockConnectorRequest::ACTION_NAME, "UnlockConnector");
            assert_eq!(
                UnlockConnectorResponse::ACTION_NAME,
                "UnlockConnectorResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_required_ids() {
            let v = SchemaValidator::v201();
            // Missing connectorId.
            assert!(v
                .validate_call("UnlockConnector", &json!({ "evseId": 1 }))
                .is_err());
            // Missing evseId.
            assert!(v
                .validate_call("UnlockConnector", &json!({ "connectorId": 1 }))
                .is_err());
        }

        #[test]
        fn schema_rejects_non_integer_ids() {
            assert!(SchemaValidator::v201()
                .validate_call(
                    "UnlockConnector",
                    &json!({ "evseId": "1", "connectorId": 2 })
                )
                .is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status_and_unknown_value() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("UnlockConnector", &json!({}))
                .is_err());
            assert!(v
                .validate_call_result("UnlockConnector", &json!({ "status": "Maybe" }))
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "UnlockConnector",
                    &json!({ "evseId": 1, "connectorId": 2, "bogusExtra": true })
                )
                .is_err());
            assert!(v
                .validate_call_result(
                    "UnlockConnector",
                    &json!({ "status": "Unlocked", "bogusExtra": true })
                )
                .is_err());
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
                charging_profile: None,
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
                charging_profile: None,
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
                charging_profile: None,
                custom_data: None,
            };
            let obj = serde_json::to_value(&req).unwrap();
            let obj = obj.as_object().unwrap();
            assert!(!obj.contains_key("evseId"));
            assert!(!obj.contains_key("groupIdToken"));
            assert!(!obj.contains_key("chargingProfile"));
            assert!(!obj.contains_key("customData"));
        }

        #[test]
        fn request_with_charging_profile_round_trips_and_validates() {
            use ocpp_types::v201::{
                ChargingProfileKindEnumType, ChargingProfilePurposeEnumType, ChargingProfileType,
                ChargingRateUnitEnumType, ChargingSchedulePeriodType, ChargingScheduleType,
            };
            // A TxProfile carrying a single absolute schedule with one period.
            let req = RequestStartTransactionRequest {
                id_token: IdTokenType {
                    id_token: "045918E24B6D80".to_string(),
                    kind: IdTokenEnumType::Iso14443,
                    additional_info: None,
                    custom_data: None,
                },
                remote_start_id: 99,
                evse_id: None,
                group_id_token: None,
                charging_profile: Some(ChargingProfileType {
                    id: 1,
                    stack_level: 0,
                    charging_profile_purpose: ChargingProfilePurposeEnumType::TxProfile,
                    charging_profile_kind: ChargingProfileKindEnumType::Absolute,
                    charging_schedule: vec![ChargingScheduleType {
                        id: 10,
                        charging_rate_unit: ChargingRateUnitEnumType::A,
                        charging_schedule_period: vec![ChargingSchedulePeriodType {
                            start_period: 0,
                            limit: 16.0,
                            number_phases: Some(3),
                            phase_to_use: None,
                            custom_data: None,
                        }],
                        start_schedule: None,
                        duration: None,
                        min_charging_rate: None,
                        sales_tariff: None,
                        custom_data: None,
                    }],
                    recurrency_kind: None,
                    valid_from: None,
                    valid_to: None,
                    transaction_id: None,
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            // The nested profile is present and shaped per the 2.0.1 wire JSON.
            assert_eq!(
                wire["chargingProfile"]["chargingProfilePurpose"],
                json!("TxProfile")
            );
            assert_eq!(
                wire["chargingProfile"]["chargingProfileKind"],
                json!("Absolute")
            );
            assert_eq!(
                wire["chargingProfile"]["chargingSchedule"][0]["chargingRateUnit"],
                json!("A")
            );
            assert_eq!(
                wire["chargingProfile"]["chargingSchedule"][0]["chargingSchedulePeriod"][0]
                    ["limit"],
                json!(16.0)
            );
            // A populated chargingProfile validates against the bundled schema.
            assert!(SchemaValidator::v201()
                .validate_call("RequestStartTransaction", &wire)
                .is_ok());
            // Full round-trip back to the typed struct.
            let back: RequestStartTransactionRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
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
            // A well-formed `chargingProfile` (#136) must pass schema
            // validation independently of the Rust struct. Constructed as raw
            // JSON so this guards the bundled schema directly, complementing the
            // typed round-trip in `request_with_charging_profile_*`.
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

    /// `GetTransactionStatus` — the 2.0.1 transaction-status / queued-message
    /// query (#164). Optional `transactionId` in; required `messagesInQueue`
    /// bool plus optional `ongoingIndicator` bool out. No new enums/datatypes.
    mod get_transaction_status {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::CustomDataType;

        #[test]
        fn request_empty_is_empty_object_on_wire_and_validates() {
            // With no transactionId the request queries station-wide state and
            // serializes to an empty object.
            let req = GetTransactionStatusRequest::default();
            assert_eq!(serde_json::to_value(&req).unwrap(), json!({}));
            let back: GetTransactionStatusRequest = serde_json::from_value(json!({})).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("GetTransactionStatus", &json!({}))
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_transaction_id() {
            let req = GetTransactionStatusRequest {
                transaction_id: Some("txn-42".to_string()),
                custom_data: None,
            };
            let expected = json!({ "transactionId": "txn-42" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("GetTransactionStatus", &expected)
                .is_ok());
            let back: GetTransactionStatusRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = GetTransactionStatusRequest {
                transaction_id: Some("txn-7".to_string()),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["transactionId"], json!("txn-7"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("GetTransactionStatus", &wire)
                .is_ok());
            let back: GetTransactionStatusRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            // `messagesInQueue` is the sole required field; `ongoingIndicator`
            // is omitted when `None`.
            let resp = GetTransactionStatusResponse {
                messages_in_queue: false,
                ongoing_indicator: None,
                custom_data: None,
            };
            let expected = json!({ "messagesInQueue": false });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(!expected
                .as_object()
                .unwrap()
                .contains_key("ongoingIndicator"));
            assert!(SchemaValidator::v201()
                .validate_call_result("GetTransactionStatus", &expected)
                .is_ok());
            let back: GetTransactionStatusResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_round_trips_with_both_booleans_and_custom_data() {
            for (queued, ongoing) in [(true, true), (false, false), (true, false)] {
                let resp = GetTransactionStatusResponse {
                    messages_in_queue: queued,
                    ongoing_indicator: Some(ongoing),
                    custom_data: Some(CustomDataType {
                        vendor_id: "com.example".to_string(),
                        extra: Default::default(),
                    }),
                };
                let wire = serde_json::to_value(&resp).unwrap();
                assert_eq!(wire["messagesInQueue"], json!(queued));
                assert_eq!(wire["ongoingIndicator"], json!(ongoing));
                assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
                assert!(SchemaValidator::v201()
                    .validate_call_result("GetTransactionStatus", &wire)
                    .is_ok());
                let back: GetTransactionStatusResponse = serde_json::from_value(wire).unwrap();
                assert_eq!(back, resp);
            }
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                GetTransactionStatusRequest::ACTION_NAME,
                "GetTransactionStatus"
            );
            assert_eq!(
                GetTransactionStatusResponse::ACTION_NAME,
                "GetTransactionStatusResponse"
            );
        }

        #[test]
        fn schema_rejects_response_missing_messages_in_queue() {
            // `messagesInQueue` is required.
            assert!(SchemaValidator::v201()
                .validate_call_result("GetTransactionStatus", &json!({}))
                .is_err());
            // ...even when only the optional field is present.
            assert!(SchemaValidator::v201()
                .validate_call_result("GetTransactionStatus", &json!({ "ongoingIndicator": true }))
                .is_err());
        }

        #[test]
        fn schema_rejects_wrong_field_types() {
            // `messagesInQueue` must be a bool, not a string.
            let bad_queue = json!({ "messagesInQueue": "true" });
            assert!(SchemaValidator::v201()
                .validate_call_result("GetTransactionStatus", &bad_queue)
                .is_err());
            assert!(serde_json::from_value::<GetTransactionStatusResponse>(bad_queue).is_err());
            // `ongoingIndicator` must be a bool when present.
            assert!(SchemaValidator::v201()
                .validate_call_result(
                    "GetTransactionStatus",
                    &json!({ "messagesInQueue": true, "ongoingIndicator": 1 })
                )
                .is_err());
            // `transactionId` must be a string on the request.
            assert!(SchemaValidator::v201()
                .validate_call("GetTransactionStatus", &json!({ "transactionId": 42 }))
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let bad_req = json!({ "unexpected": true });
            assert!(SchemaValidator::v201()
                .validate_call("GetTransactionStatus", &bad_req)
                .is_err());
            let bad_resp = json!({ "messagesInQueue": true, "unexpected": true });
            assert!(SchemaValidator::v201()
                .validate_call_result("GetTransactionStatus", &bad_resp)
                .is_err());
        }
    }

    /// `DataTransfer` — the 2.0.1 bidirectional vendor escape hatch (#154).
    /// First v201 message to carry a free-form `serde_json::Value` `data`
    /// field; reuses `StatusInfoType` and the new `DataTransferStatusEnumType`.
    mod data_transfer {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{DataTransferStatusEnumType, StatusInfoType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the required vendorId; everything else stays off the wire.
            let req = DataTransferRequest {
                vendor_id: "ACME".to_string(),
                message_id: None,
                data: None,
                custom_data: None,
            };
            let expected = json!({ "vendorId": "ACME" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("DataTransfer", &expected)
                .is_ok());
            let back: DataTransferRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_round_trips_with_all_optionals_and_validates() {
            let req = DataTransferRequest {
                vendor_id: "ACME".to_string(),
                message_id: Some("diag.run".to_string()),
                data: Some(json!({ "level": 3, "tags": ["a", "b"] })),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(
                wire,
                json!({
                    "vendorId": "ACME",
                    "messageId": "diag.run",
                    "data": { "level": 3, "tags": ["a", "b"] }
                })
            );
            assert!(SchemaValidator::v201()
                .validate_call("DataTransfer", &wire)
                .is_ok());
            let back: DataTransferRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn data_round_trips_arbitrary_json_without_loss() {
            // Object, array, string, number, bool, and null all survive a
            // serialize → deserialize round-trip unchanged. A JSON `null`
            // *inside* a composite (the array below) is preserved; a bare
            // top-level `null` is the one shape that collapses — see
            // `bare_null_data_collapses_to_none` for that documented edge.
            for data in [
                json!({ "nested": { "k": [1, 2, 3] } }),
                json!([true, null, "x", 1.5]),
                json!("plain string"),
                json!(-7),
                json!(false),
            ] {
                let req = DataTransferRequest {
                    vendor_id: "ACME".to_string(),
                    message_id: None,
                    data: Some(data.clone()),
                    custom_data: None,
                };
                let wire = serde_json::to_value(&req).unwrap();
                assert_eq!(wire["data"], data);
                let back: DataTransferRequest = serde_json::from_value(wire).unwrap();
                assert_eq!(back, req);
            }
        }

        #[test]
        fn bare_null_data_collapses_to_none() {
            // `Some(Value::Null)` serializes to an explicit `"data": null`, but
            // serde maps a JSON `null` back to `None` for an `Option` field, so
            // the two are indistinguishable after a read. That is fine for
            // OCPP: sending `data: null` and omitting `data` are semantically
            // equivalent. Composite payloads carrying inner nulls are
            // unaffected (covered above).
            let req = DataTransferRequest {
                vendor_id: "ACME".to_string(),
                message_id: None,
                data: Some(json!(null)),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["data"], json!(null));
            let back: DataTransferRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back.data, None);
        }

        #[test]
        fn request_omits_data_field_when_none() {
            // `None` data must stay off the wire, distinct from `Some(null)`.
            let absent = DataTransferRequest {
                vendor_id: "ACME".to_string(),
                message_id: None,
                data: None,
                custom_data: None,
            };
            assert!(serde_json::to_value(&absent).unwrap().get("data").is_none());
            // `Some(Value::Null)` *does* appear on the wire as an explicit null.
            let explicit_null = DataTransferRequest {
                data: Some(json!(null)),
                ..absent.clone()
            };
            assert_eq!(
                serde_json::to_value(&explicit_null).unwrap()["data"],
                json!(null)
            );
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = DataTransferResponse {
                status: DataTransferStatusEnumType::Accepted,
                status_info: None,
                data: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("DataTransfer", &expected)
                .is_ok());
            let back: DataTransferResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejected_with_status_info_and_data_round_trips() {
            let resp = DataTransferResponse {
                status: DataTransferStatusEnumType::UnknownMessageId,
                status_info: Some(StatusInfoType {
                    reason_code: "NotSupported".to_string(),
                    additional_info: Some("unknown messageId".to_string()),
                    custom_data: None,
                }),
                data: Some(json!({ "echo": "diag.run" })),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("UnknownMessageId"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotSupported"));
            assert_eq!(wire["data"], json!({ "echo": "diag.run" }));
            assert!(SchemaValidator::v201()
                .validate_call_result("DataTransfer", &wire)
                .is_ok());
            let back: DataTransferResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            for (variant, wire) in [
                (DataTransferStatusEnumType::Accepted, "Accepted"),
                (DataTransferStatusEnumType::Rejected, "Rejected"),
                (
                    DataTransferStatusEnumType::UnknownMessageId,
                    "UnknownMessageId",
                ),
                (
                    DataTransferStatusEnumType::UnknownVendorId,
                    "UnknownVendorId",
                ),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            }
            assert!(serde_json::from_value::<DataTransferStatusEnumType>(json!("Bogus")).is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(DataTransferRequest::ACTION_NAME, "DataTransfer");
            assert_eq!(DataTransferResponse::ACTION_NAME, "DataTransferResponse");
        }

        #[test]
        fn schema_rejects_request_missing_vendor_id() {
            // `vendorId` is required even when other fields are supplied.
            let bad = json!({ "messageId": "x", "data": { "k": "v" } });
            assert!(SchemaValidator::v201()
                .validate_call("DataTransfer", &bad)
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_unknown_status() {
            let bad = json!({ "status": "Bogus" });
            assert!(SchemaValidator::v201()
                .validate_call_result("DataTransfer", &bad)
                .is_err());
            assert!(serde_json::from_value::<DataTransferResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties_at_root() {
            // The freedom is inside `data`; the message root stays closed.
            let bad_req = json!({ "vendorId": "ACME", "bogusExtra": true });
            assert!(SchemaValidator::v201()
                .validate_call("DataTransfer", &bad_req)
                .is_err());
            let bad_resp = json!({ "status": "Accepted", "bogusExtra": true });
            assert!(SchemaValidator::v201()
                .validate_call_result("DataTransfer", &bad_resp)
                .is_err());
        }
    }

    /// `TriggerMessage` — the 2.0.1 message-trigger command (#152). A required
    /// [`MessageTriggerEnumType`] in (optionally scoped to one `evse`), a
    /// [`TriggerMessageStatusEnumType`] out; reuses `EvseType`/`StatusInfoType`.
    mod trigger_message {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            CustomDataType, EvseType, MessageTriggerEnumType, StatusInfoType,
            TriggerMessageStatusEnumType,
        };

        #[test]
        fn request_targets_whole_station_and_validates() {
            // No `evse` → trigger a fresh BootNotification from the whole station.
            let req = TriggerMessageRequest {
                requested_message: MessageTriggerEnumType::BootNotification,
                evse: None,
                custom_data: None,
            };
            let expected = json!({ "requestedMessage": "BootNotification" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            // `evse`/`customData` are omitted when `None`.
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("evse"));
            assert!(!obj.contains_key("customData"));
            let back: TriggerMessageRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("TriggerMessage", &expected)
                .is_ok());
        }

        #[test]
        fn request_scoped_to_single_evse_and_validates() {
            let req = TriggerMessageRequest {
                requested_message: MessageTriggerEnumType::StatusNotification,
                evse: Some(EvseType {
                    id: 1,
                    connector_id: Some(2),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(
                wire,
                json!({
                    "requestedMessage": "StatusNotification",
                    "evse": { "id": 1, "connectorId": 2 }
                })
            );
            assert!(SchemaValidator::v201()
                .validate_call("TriggerMessage", &wire)
                .is_ok());
            let back: TriggerMessageRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = TriggerMessageRequest {
                requested_message: MessageTriggerEnumType::MeterValues,
                evse: None,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("TriggerMessage", &wire)
                .is_ok());
            let back: TriggerMessageRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = TriggerMessageResponse {
                status: TriggerMessageStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("TriggerMessage", &expected)
                .is_ok());
            let back: TriggerMessageResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_not_implemented_with_status_info_round_trips() {
            let resp = TriggerMessageResponse {
                status: TriggerMessageStatusEnumType::NotImplemented,
                status_info: Some(StatusInfoType {
                    reason_code: "NotSupported".to_string(),
                    additional_info: Some("station cannot trigger this message".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("NotImplemented"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotSupported"));
            assert!(SchemaValidator::v201()
                .validate_call_result("TriggerMessage", &wire)
                .is_ok());
            let back: TriggerMessageResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            for (variant, wire) in [
                (TriggerMessageStatusEnumType::Accepted, "Accepted"),
                (TriggerMessageStatusEnumType::Rejected, "Rejected"),
                (
                    TriggerMessageStatusEnumType::NotImplemented,
                    "NotImplemented",
                ),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            }
            // `NotSupported` is a 1.6J TriggerMessageStatus value, not valid in
            // 2.0.1 (the spec renamed it `NotImplemented`).
            assert!(
                serde_json::from_value::<TriggerMessageStatusEnumType>(json!("NotSupported"))
                    .is_err()
            );
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(TriggerMessageRequest::ACTION_NAME, "TriggerMessage");
            assert_eq!(
                TriggerMessageResponse::ACTION_NAME,
                "TriggerMessageResponse"
            );
        }

        #[test]
        fn schema_rejects_missing_required_fields() {
            let v = SchemaValidator::v201();
            // `requestedMessage` is the sole required request field.
            assert!(v.validate_call("TriggerMessage", &json!({})).is_err());
            // `status` is required on the response.
            assert!(v
                .validate_call_result("TriggerMessage", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_unknown_requested_message() {
            // `DiagnosticsStatusNotification` is a 1.6J trigger, dropped in 2.0.1.
            let bad = json!({ "requestedMessage": "DiagnosticsStatusNotification" });
            assert!(SchemaValidator::v201()
                .validate_call("TriggerMessage", &bad)
                .is_err());
            assert!(serde_json::from_value::<TriggerMessageRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "TriggerMessage",
                    &json!({ "requestedMessage": "Heartbeat", "bogusExtra": true })
                )
                .is_err());
            assert!(v
                .validate_call_result(
                    "TriggerMessage",
                    &json!({ "status": "Accepted", "bogusExtra": true })
                )
                .is_err());
        }
    }

    /// `ReserveNow` — the 2.0.1 reservation-create command (#158), companion to
    /// `CancelReservation`. Required `id`/`expiryDateTime`/`idToken` in, a
    /// [`ReserveNowStatusEnumType`] out; reuses `IdTokenType`/`StatusInfoType`,
    /// the only new surface being `ReserveNowStatusEnumType` and the
    /// `#[serde(rename)]`-heavy [`ConnectorEnumType`].
    mod reserve_now {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            ConnectorEnumType, CustomDataType, IdTokenEnumType, IdTokenType,
            ReserveNowStatusEnumType, StatusInfoType,
        };

        fn sample_id_token() -> IdTokenType {
            IdTokenType {
                id_token: "045918E24B6D80".to_string(),
                kind: IdTokenEnumType::Iso14443,
                additional_info: None,
                custom_data: None,
            }
        }

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the three required fields: id, expiryDateTime, idToken.
            let req = ReserveNowRequest {
                id: 42,
                expiry_date_time: "2024-01-01T12:00:00Z".to_string(),
                id_token: sample_id_token(),
                connector_type: None,
                evse_id: None,
                group_id_token: None,
                custom_data: None,
            };
            let expected = json!({
                "id": 42,
                "expiryDateTime": "2024-01-01T12:00:00Z",
                "idToken": { "idToken": "045918E24B6D80", "type": "ISO14443" }
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            // Optional fields stay off the wire when `None`.
            let obj = expected.as_object().unwrap();
            for key in ["connectorType", "evseId", "groupIdToken", "customData"] {
                assert!(!obj.contains_key(key));
            }
            let back: ReserveNowRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("ReserveNow", &expected)
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_all_optionals_and_validates() {
            // groupIdToken reuses the nested IdTokenType; connectorType exercises
            // a `#[serde(rename)]` value.
            let req = ReserveNowRequest {
                id: 7,
                expiry_date_time: "2024-06-01T08:30:00Z".to_string(),
                id_token: sample_id_token(),
                connector_type: Some(ConnectorEnumType::Ccs2),
                evse_id: Some(1),
                group_id_token: Some(IdTokenType {
                    id_token: "PARENT01".to_string(),
                    kind: IdTokenEnumType::Central,
                    additional_info: None,
                    custom_data: None,
                }),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["connectorType"], json!("cCCS2"));
            assert_eq!(wire["evseId"], json!(1));
            assert_eq!(wire["groupIdToken"]["idToken"], json!("PARENT01"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("ReserveNow", &wire)
                .is_ok());
            let back: ReserveNowRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = ReserveNowResponse {
                status: ReserveNowStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            // `statusInfo` is omitted when `None`.
            assert!(!expected.as_object().unwrap().contains_key("statusInfo"));
            assert!(SchemaValidator::v201()
                .validate_call_result("ReserveNow", &expected)
                .is_ok());
            let back: ReserveNowResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_occupied_with_status_info_round_trips() {
            let resp = ReserveNowResponse {
                status: ReserveNowStatusEnumType::Occupied,
                status_info: Some(StatusInfoType {
                    reason_code: "InUse".to_string(),
                    additional_info: Some("connector currently charging".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Occupied"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("InUse"));
            assert!(SchemaValidator::v201()
                .validate_call_result("ReserveNow", &wire)
                .is_ok());
            let back: ReserveNowResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            for (variant, wire) in [
                (ReserveNowStatusEnumType::Accepted, "Accepted"),
                (ReserveNowStatusEnumType::Faulted, "Faulted"),
                (ReserveNowStatusEnumType::Occupied, "Occupied"),
                (ReserveNowStatusEnumType::Rejected, "Rejected"),
                (ReserveNowStatusEnumType::Unavailable, "Unavailable"),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            }
            // `Scheduled` belongs to other status enums, not ReserveNow.
            assert!(
                serde_json::from_value::<ReserveNowStatusEnumType>(json!("Scheduled")).is_err()
            );
        }

        #[test]
        fn connector_enum_serializes_to_exact_wire_values() {
            // Every member round-trips to its FINAL-schema wire spelling,
            // including the `#[serde(rename)]` tokens and the bare PascalCase
            // ones. Each value is also accepted by the bundled schema.
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (ConnectorEnumType::Ccs1, "cCCS1"),
                (ConnectorEnumType::Ccs2, "cCCS2"),
                (ConnectorEnumType::Cg105, "cG105"),
                (ConnectorEnumType::Ctesla, "cTesla"),
                (ConnectorEnumType::Ctype1, "cType1"),
                (ConnectorEnumType::Ctype2, "cType2"),
                (ConnectorEnumType::S3091P16A, "s309-1P-16A"),
                (ConnectorEnumType::S3091P32A, "s309-1P-32A"),
                (ConnectorEnumType::S3093P16A, "s309-3P-16A"),
                (ConnectorEnumType::S3093P32A, "s309-3P-32A"),
                (ConnectorEnumType::Sbs1361, "sBS1361"),
                (ConnectorEnumType::Scee77, "sCEE-7-7"),
                (ConnectorEnumType::Stype2, "sType2"),
                (ConnectorEnumType::Stype3, "sType3"),
                (ConnectorEnumType::Other1PhMax16A, "Other1PhMax16A"),
                (ConnectorEnumType::Other1PhOver16A, "Other1PhOver16A"),
                (ConnectorEnumType::Other3Ph, "Other3Ph"),
                (ConnectorEnumType::Pan, "Pan"),
                (ConnectorEnumType::Winductive, "wInductive"),
                (ConnectorEnumType::Wresonant, "wResonant"),
                (ConnectorEnumType::Undetermined, "Undetermined"),
                (ConnectorEnumType::Unknown, "Unknown"),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
                // Round-trips back from the wire string.
                assert_eq!(
                    serde_json::from_value::<ConnectorEnumType>(json!(wire)).unwrap(),
                    variant
                );
                // And the schema accepts a request carrying it.
                let req = json!({
                    "id": 1,
                    "expiryDateTime": "2024-01-01T12:00:00Z",
                    "idToken": { "idToken": "abc", "type": "ISO14443" },
                    "connectorType": wire
                });
                assert!(v.validate_call("ReserveNow", &req).is_ok());
            }
        }

        #[test]
        fn connector_enum_rejects_dataclass_only_and_unknown_values() {
            // `cChaoJi` / `cGBT` exist in the reference dataclass but were
            // dropped from the FINAL schema — both serde and schema reject them,
            // staying in agreement.
            let v = SchemaValidator::v201();
            for bad in ["cChaoJi", "cGBT", "Bogus", "ccs1"] {
                assert!(serde_json::from_value::<ConnectorEnumType>(json!(bad)).is_err());
                let req = json!({
                    "id": 1,
                    "expiryDateTime": "2024-01-01T12:00:00Z",
                    "idToken": { "idToken": "abc", "type": "ISO14443" },
                    "connectorType": bad
                });
                assert!(v.validate_call("ReserveNow", &req).is_err());
            }
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(ReserveNowRequest::ACTION_NAME, "ReserveNow");
            assert_eq!(ReserveNowResponse::ACTION_NAME, "ReserveNowResponse");
        }

        #[test]
        fn schema_rejects_request_missing_required_fields() {
            let v = SchemaValidator::v201();
            let full = json!({
                "id": 1,
                "expiryDateTime": "2024-01-01T12:00:00Z",
                "idToken": { "idToken": "abc", "type": "ISO14443" }
            });
            assert!(v.validate_call("ReserveNow", &full).is_ok());
            // Drop each required field in turn.
            for missing in ["id", "expiryDateTime", "idToken"] {
                let mut bad = full.clone();
                bad.as_object_mut().unwrap().remove(missing);
                assert!(
                    v.validate_call("ReserveNow", &bad).is_err(),
                    "expected rejection when `{missing}` is absent"
                );
            }
        }

        #[test]
        fn schema_and_serde_reject_non_integer_id() {
            let bad = json!({
                "id": "42",
                "expiryDateTime": "2024-01-01T12:00:00Z",
                "idToken": { "idToken": "abc", "type": "ISO14443" }
            });
            assert!(SchemaValidator::v201()
                .validate_call("ReserveNow", &bad)
                .is_err());
            assert!(serde_json::from_value::<ReserveNowRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_or_unknown_status() {
            let v = SchemaValidator::v201();
            assert!(v.validate_call_result("ReserveNow", &json!({})).is_err());
            let bad = json!({ "status": "Scheduled" });
            assert!(v.validate_call_result("ReserveNow", &bad).is_err());
            assert!(serde_json::from_value::<ReserveNowResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({
                "id": 1,
                "expiryDateTime": "2024-01-01T12:00:00Z",
                "idToken": { "idToken": "abc", "type": "ISO14443" },
                "bogusExtra": true
            });
            assert!(v.validate_call("ReserveNow", &bad_req).is_err());
            let bad_resp = json!({ "status": "Accepted", "bogusExtra": true });
            assert!(v.validate_call_result("ReserveNow", &bad_resp).is_err());
        }
    }

    /// `SendLocalList` — the 2.0.1 local-authorization-list write path (#159),
    /// companion to `GetLocalListVersion` (#148) and `ClearCache` (#149). A
    /// `versionNumber` + `updateType` in, an optional list of `AuthorizationData`
    /// (reusing `IdTokenType`/`IdTokenInfoType`), a `SendLocalListStatusEnumType`
    /// out; new surface is `AuthorizationData` and two enums.
    mod send_local_list {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            AuthorizationData, AuthorizationStatusEnumType, CustomDataType, IdTokenEnumType,
            IdTokenInfoType, IdTokenType, SendLocalListStatusEnumType, StatusInfoType,
            UpdateEnumType,
        };

        /// A `Full` update carrying one accepted token matches the exact wire
        /// JSON and validates against the bundled schema.
        #[test]
        fn full_update_matches_wire_json_and_validates() {
            let req = SendLocalListRequest {
                version_number: 3,
                update_type: UpdateEnumType::Full,
                local_authorization_list: Some(vec![AuthorizationData {
                    id_token: IdTokenType {
                        id_token: "045918E24B6D80".to_string(),
                        kind: IdTokenEnumType::Iso14443,
                        additional_info: None,
                        custom_data: None,
                    },
                    id_token_info: Some(IdTokenInfoType {
                        status: AuthorizationStatusEnumType::Accepted,
                        cache_expiry_date_time: None,
                        charging_priority: None,
                        language1: None,
                        evse_id: None,
                        language2: None,
                        group_id_token: None,
                        personal_message: None,
                        custom_data: None,
                    }),
                    custom_data: None,
                }]),
                custom_data: None,
            };
            let expected = json!({
                "versionNumber": 3,
                "updateType": "Full",
                "localAuthorizationList": [{
                    "idToken": { "idToken": "045918E24B6D80", "type": "ISO14443" },
                    "idTokenInfo": { "status": "Accepted" }
                }]
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: SendLocalListRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("SendLocalList", &expected)
                .is_ok());
        }

        /// A `Differential` update whose entry omits `idTokenInfo` (a token
        /// removal) round-trips and validates — `idTokenInfo` stays absent on
        /// the wire.
        #[test]
        fn differential_update_removal_entry_round_trips_and_validates() {
            let req = SendLocalListRequest {
                version_number: 4,
                update_type: UpdateEnumType::Differential,
                local_authorization_list: Some(vec![AuthorizationData {
                    id_token: IdTokenType {
                        id_token: "abc".to_string(),
                        kind: IdTokenEnumType::Central,
                        additional_info: None,
                        custom_data: None,
                    },
                    id_token_info: None,
                    custom_data: None,
                }]),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["updateType"], json!("Differential"));
            // Removal entry: idTokenInfo must be absent.
            assert!(!wire["localAuthorizationList"][0]
                .as_object()
                .unwrap()
                .contains_key("idTokenInfo"));
            assert!(SchemaValidator::v201()
                .validate_call("SendLocalList", &wire)
                .is_ok());
            let back: SendLocalListRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        /// A `Full` update with no list (clear the station's list) — the
        /// `localAuthorizationList` field is omitted entirely and validates.
        #[test]
        fn request_without_list_matches_wire_json_and_validates() {
            let req = SendLocalListRequest {
                version_number: 0,
                update_type: UpdateEnumType::Full,
                local_authorization_list: None,
                custom_data: None,
            };
            let expected = json!({ "versionNumber": 0, "updateType": "Full" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("SendLocalList", &expected)
                .is_ok());
            let back: SendLocalListRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = SendLocalListResponse {
                status: SendLocalListStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("SendLocalList", &expected)
                .is_ok());
            let back: SendLocalListResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_version_mismatch_with_status_info_round_trips() {
            let resp = SendLocalListResponse {
                status: SendLocalListStatusEnumType::VersionMismatch,
                status_info: Some(StatusInfoType {
                    reason_code: "VersionMismatch".to_string(),
                    additional_info: Some("differential not contiguous".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("VersionMismatch"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("VersionMismatch"));
            assert!(SchemaValidator::v201()
                .validate_call_result("SendLocalList", &wire)
                .is_ok());
            let back: SendLocalListResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = SendLocalListRequest {
                version_number: 1,
                update_type: UpdateEnumType::Full,
                local_authorization_list: None,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("SendLocalList", &wire)
                .is_ok());
            let back: SendLocalListRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(SendLocalListRequest::ACTION_NAME, "SendLocalList");
            assert_eq!(SendLocalListResponse::ACTION_NAME, "SendLocalListResponse");
        }

        #[test]
        fn schema_rejects_request_missing_required_fields() {
            let v = SchemaValidator::v201();
            // Missing updateType.
            assert!(v
                .validate_call("SendLocalList", &json!({ "versionNumber": 1 }))
                .is_err());
            // Missing versionNumber.
            assert!(v
                .validate_call("SendLocalList", &json!({ "updateType": "Full" }))
                .is_err());
        }

        #[test]
        fn schema_rejects_empty_authorization_list() {
            // The schema sets `minItems: 1` on `localAuthorizationList`; an
            // empty list is rejected even though the Rust `Vec` allows it.
            let bad = json!({
                "versionNumber": 1,
                "updateType": "Full",
                "localAuthorizationList": []
            });
            assert!(SchemaValidator::v201()
                .validate_call("SendLocalList", &bad)
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_unknown_update_type() {
            let bad = json!({ "versionNumber": 1, "updateType": "Partial" });
            assert!(SchemaValidator::v201()
                .validate_call("SendLocalList", &bad)
                .is_err());
            assert!(serde_json::from_value::<SendLocalListRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_or_unknown_status() {
            let v = SchemaValidator::v201();
            assert!(v.validate_call_result("SendLocalList", &json!({})).is_err());
            let bad = json!({ "status": "Scheduled" });
            assert!(v.validate_call_result("SendLocalList", &bad).is_err());
            assert!(serde_json::from_value::<SendLocalListResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({
                "versionNumber": 1,
                "updateType": "Full",
                "bogusExtra": true
            });
            assert!(v.validate_call("SendLocalList", &bad_req).is_err());
            let bad_resp = json!({ "status": "Accepted", "bogusExtra": true });
            assert!(v.validate_call_result("SendLocalList", &bad_resp).is_err());
        }
    }

    mod firmware_status_notification {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CustomDataType, FirmwareStatusEnumType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the required `status`; `requestId` / `customData` stay off the
            // wire when `None`.
            let req = FirmwareStatusNotificationRequest {
                status: FirmwareStatusEnumType::Downloaded,
                request_id: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Downloaded" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            for key in ["requestId", "customData"] {
                assert!(!obj.contains_key(key));
            }
            let back: FirmwareStatusNotificationRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("FirmwareStatusNotification", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_request_id_round_trips_and_validates() {
            let req = FirmwareStatusNotificationRequest {
                status: FirmwareStatusEnumType::Installing,
                request_id: Some(1234),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["status"], json!("Installing"));
            assert_eq!(wire["requestId"], json!(1234));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("FirmwareStatusNotification", &wire)
                .is_ok());
            let back: FirmwareStatusNotificationRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_empty_is_object_and_validates() {
            let resp = FirmwareStatusNotificationResponse::default();
            assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
            assert!(SchemaValidator::v201()
                .validate_call_result("FirmwareStatusNotification", &json!({}))
                .is_ok());
            let back: FirmwareStatusNotificationResponse =
                serde_json::from_value(json!({})).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_exact_wire_values_and_schema_accepts() {
            // Every one of the 14 members round-trips to its FINAL-schema wire
            // spelling, and the bundled schema accepts a request carrying it.
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (FirmwareStatusEnumType::Downloaded, "Downloaded"),
                (FirmwareStatusEnumType::DownloadFailed, "DownloadFailed"),
                (FirmwareStatusEnumType::Downloading, "Downloading"),
                (
                    FirmwareStatusEnumType::DownloadScheduled,
                    "DownloadScheduled",
                ),
                (FirmwareStatusEnumType::DownloadPaused, "DownloadPaused"),
                (FirmwareStatusEnumType::Idle, "Idle"),
                (
                    FirmwareStatusEnumType::InstallationFailed,
                    "InstallationFailed",
                ),
                (FirmwareStatusEnumType::Installing, "Installing"),
                (FirmwareStatusEnumType::Installed, "Installed"),
                (FirmwareStatusEnumType::InstallRebooting, "InstallRebooting"),
                (FirmwareStatusEnumType::InstallScheduled, "InstallScheduled"),
                (
                    FirmwareStatusEnumType::InstallVerificationFailed,
                    "InstallVerificationFailed",
                ),
                (FirmwareStatusEnumType::InvalidSignature, "InvalidSignature"),
                (
                    FirmwareStatusEnumType::SignatureVerified,
                    "SignatureVerified",
                ),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
                assert_eq!(
                    serde_json::from_value::<FirmwareStatusEnumType>(json!(wire)).unwrap(),
                    variant
                );
                assert!(v
                    .validate_call("FirmwareStatusNotification", &json!({ "status": wire }))
                    .is_ok());
            }
        }

        #[test]
        fn enum_and_schema_reject_unknown_status() {
            let v = SchemaValidator::v201();
            for bad in ["downloaded", "Bogus", "Install"] {
                assert!(serde_json::from_value::<FirmwareStatusEnumType>(json!(bad)).is_err());
                assert!(v
                    .validate_call("FirmwareStatusNotification", &json!({ "status": bad }))
                    .is_err());
            }
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                FirmwareStatusNotificationRequest::ACTION_NAME,
                "FirmwareStatusNotification"
            );
            assert_eq!(
                FirmwareStatusNotificationResponse::ACTION_NAME,
                "FirmwareStatusNotificationResponse"
            );
        }

        #[test]
        fn schema_rejects_missing_status_and_non_integer_request_id() {
            let v = SchemaValidator::v201();
            // `status` is required.
            assert!(v
                .validate_call("FirmwareStatusNotification", &json!({}))
                .is_err());
            // `requestId` must be an integer — both schema and serde reject a string.
            let bad = json!({ "status": "Downloaded", "requestId": "1" });
            assert!(v.validate_call("FirmwareStatusNotification", &bad).is_err());
            assert!(serde_json::from_value::<FirmwareStatusNotificationRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({ "status": "Downloaded", "bogusExtra": true });
            assert!(v
                .validate_call("FirmwareStatusNotification", &bad_req)
                .is_err());
            let bad_resp = json!({ "bogusExtra": true });
            assert!(v
                .validate_call_result("FirmwareStatusNotification", &bad_resp)
                .is_err());
        }
    }

    mod security_event_notification {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::CustomDataType;

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the required `type` + `timestamp`; `techInfo` / `customData`
            // stay off the wire when `None`. The renamed `event_type` field
            // serializes to the wire key `"type"`.
            let req = SecurityEventNotificationRequest {
                event_type: "SettingSystemTime".to_string(),
                timestamp: "2026-06-22T03:00:00Z".to_string(),
                tech_info: None,
                custom_data: None,
            };
            let expected = json!({
                "type": "SettingSystemTime",
                "timestamp": "2026-06-22T03:00:00Z"
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            for key in ["techInfo", "customData"] {
                assert!(!obj.contains_key(key));
            }
            let back: SecurityEventNotificationRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("SecurityEventNotification", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_all_optionals_round_trips_and_validates() {
            let req = SecurityEventNotificationRequest {
                event_type: "InvalidFirmwareSignature".to_string(),
                timestamp: "2026-06-22T03:01:02Z".to_string(),
                tech_info: Some("signature mismatch on slot 2".to_string()),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["type"], json!("InvalidFirmwareSignature"));
            assert_eq!(wire["timestamp"], json!("2026-06-22T03:01:02Z"));
            assert_eq!(wire["techInfo"], json!("signature mismatch on slot 2"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("SecurityEventNotification", &wire)
                .is_ok());
            let back: SecurityEventNotificationRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_empty_is_object_and_validates() {
            let resp = SecurityEventNotificationResponse::default();
            assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
            assert!(SchemaValidator::v201()
                .validate_call_result("SecurityEventNotification", &json!({}))
                .is_ok());
            let back: SecurityEventNotificationResponse =
                serde_json::from_value(json!({})).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                SecurityEventNotificationRequest::ACTION_NAME,
                "SecurityEventNotification"
            );
            assert_eq!(
                SecurityEventNotificationResponse::ACTION_NAME,
                "SecurityEventNotificationResponse"
            );
        }

        #[test]
        fn schema_rejects_missing_required_and_wrong_types() {
            let v = SchemaValidator::v201();
            // `type` and `timestamp` are both required.
            assert!(v
                .validate_call("SecurityEventNotification", &json!({}))
                .is_err());
            assert!(v
                .validate_call("SecurityEventNotification", &json!({ "type": "Reboot" }))
                .is_err());
            assert!(v
                .validate_call(
                    "SecurityEventNotification",
                    &json!({ "timestamp": "2026-06-22T03:00:00Z" })
                )
                .is_err());
            // `type` must be a string; `timestamp` must be a date-time string.
            assert!(v
                .validate_call(
                    "SecurityEventNotification",
                    &json!({ "type": 42, "timestamp": "2026-06-22T03:00:00Z" })
                )
                .is_err());
            assert!(v
                .validate_call(
                    "SecurityEventNotification",
                    &json!({ "type": "Reboot", "timestamp": "not-a-date" })
                )
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({
                "type": "Reboot",
                "timestamp": "2026-06-22T03:00:00Z",
                "bogusExtra": true
            });
            assert!(v
                .validate_call("SecurityEventNotification", &bad_req)
                .is_err());
            let bad_resp = json!({ "bogusExtra": true });
            assert!(v
                .validate_call_result("SecurityEventNotification", &bad_resp)
                .is_err());
        }
    }

    mod set_charging_profile {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            ChargingProfileKindEnumType, ChargingProfilePurposeEnumType,
            ChargingProfileStatusEnumType, ChargingProfileType, ChargingRateUnitEnumType,
            ChargingSchedulePeriodType, ChargingScheduleType, ConsumptionCostType,
            CostKindEnumType, CostType, CustomDataType, RecurrencyKindEnumType,
            RelativeTimeIntervalType, SalesTariffEntryType, SalesTariffType, StatusInfoType,
        };

        /// The smallest valid profile: the five required fields, one schedule
        /// with one period.
        fn minimal_profile() -> ChargingProfileType {
            ChargingProfileType {
                id: 1,
                stack_level: 0,
                charging_profile_purpose: ChargingProfilePurposeEnumType::TxDefaultProfile,
                charging_profile_kind: ChargingProfileKindEnumType::Absolute,
                charging_schedule: vec![ChargingScheduleType {
                    id: 1,
                    charging_rate_unit: ChargingRateUnitEnumType::A,
                    charging_schedule_period: vec![ChargingSchedulePeriodType {
                        start_period: 0,
                        limit: 16.0,
                        number_phases: None,
                        phase_to_use: None,
                        custom_data: None,
                    }],
                    start_schedule: None,
                    duration: None,
                    min_charging_rate: None,
                    sales_tariff: None,
                    custom_data: None,
                }],
                recurrency_kind: None,
                valid_from: None,
                valid_to: None,
                transaction_id: None,
                custom_data: None,
            }
        }

        /// A recurring profile carrying a full sales-tariff tree, exercising the
        /// reused [`ChargingProfileType`] through the new message.
        fn full_profile() -> ChargingProfileType {
            ChargingProfileType {
                id: 5,
                stack_level: 1,
                charging_profile_purpose: ChargingProfilePurposeEnumType::TxDefaultProfile,
                charging_profile_kind: ChargingProfileKindEnumType::Recurring,
                charging_schedule: vec![ChargingScheduleType {
                    id: 2,
                    charging_rate_unit: ChargingRateUnitEnumType::W,
                    charging_schedule_period: vec![ChargingSchedulePeriodType {
                        start_period: 0,
                        limit: 11000.0,
                        number_phases: Some(3),
                        phase_to_use: Some(1),
                        custom_data: None,
                    }],
                    start_schedule: Some("2022-01-01T00:00:00Z".to_string()),
                    duration: Some(86400),
                    min_charging_rate: Some(1380.0),
                    sales_tariff: Some(SalesTariffType {
                        id: 3,
                        sales_tariff_entry: vec![SalesTariffEntryType {
                            relative_time_interval: RelativeTimeIntervalType {
                                start: 0,
                                duration: Some(3600),
                                custom_data: None,
                            },
                            e_price_level: Some(1),
                            consumption_cost: Some(vec![ConsumptionCostType {
                                start_value: 0.0,
                                cost: vec![CostType {
                                    cost_kind: CostKindEnumType::RelativePricePercentage,
                                    amount: 25,
                                    amount_multiplier: Some(-1),
                                    custom_data: None,
                                }],
                                custom_data: None,
                            }]),
                            custom_data: None,
                        }],
                        sales_tariff_description: Some("peak".to_string()),
                        num_e_price_levels: Some(2),
                        custom_data: None,
                    }),
                    custom_data: None,
                }],
                recurrency_kind: Some(RecurrencyKindEnumType::Daily),
                valid_from: Some("2022-01-01T00:00:00Z".to_string()),
                valid_to: Some("2022-12-31T23:59:59Z".to_string()),
                transaction_id: None,
                custom_data: None,
            }
        }

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            let req = SetChargingProfileRequest {
                evse_id: 1,
                charging_profile: minimal_profile(),
                custom_data: None,
            };
            let expected = json!({
                "evseId": 1,
                "chargingProfile": {
                    "id": 1,
                    "stackLevel": 0,
                    "chargingProfilePurpose": "TxDefaultProfile",
                    "chargingProfileKind": "Absolute",
                    "chargingSchedule": [{
                        "id": 1,
                        "chargingRateUnit": "A",
                        "chargingSchedulePeriod": [{ "startPeriod": 0, "limit": 16.0 }]
                    }]
                }
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            // `customData` stays off the wire when `None`.
            assert!(!expected.as_object().unwrap().contains_key("customData"));
            let back: SetChargingProfileRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("SetChargingProfile", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_full_profile_tree_round_trips_and_validates() {
            let req = SetChargingProfileRequest {
                evse_id: 0,
                charging_profile: full_profile(),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            // Spot-check a deeply-nested value made it onto the wire with renames.
            assert_eq!(
                wire["chargingProfile"]["chargingSchedule"][0]["salesTariff"]["salesTariffEntry"]
                    [0]["consumptionCost"][0]["cost"][0]["costKind"],
                json!("RelativePricePercentage")
            );
            assert!(SchemaValidator::v201()
                .validate_call("SetChargingProfile", &wire)
                .is_ok());
            let back: SetChargingProfileRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_with_custom_data_round_trips() {
            let req = SetChargingProfileRequest {
                evse_id: 2,
                charging_profile: minimal_profile(),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("SetChargingProfile", &wire)
                .is_ok());
            let back: SetChargingProfileRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = SetChargingProfileResponse {
                status: ChargingProfileStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            // `statusInfo` is omitted when `None`.
            assert!(!expected.as_object().unwrap().contains_key("statusInfo"));
            assert!(SchemaValidator::v201()
                .validate_call_result("SetChargingProfile", &expected)
                .is_ok());
            let back: SetChargingProfileResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejected_with_status_info_round_trips() {
            let resp = SetChargingProfileResponse {
                status: ChargingProfileStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "InvalidProfile".to_string(),
                    additional_info: Some("stack level conflict".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Rejected"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("InvalidProfile"));
            assert!(SchemaValidator::v201()
                .validate_call_result("SetChargingProfile", &wire)
                .is_ok());
            let back: SetChargingProfileResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            for (variant, wire) in [
                (ChargingProfileStatusEnumType::Accepted, "Accepted"),
                (ChargingProfileStatusEnumType::Rejected, "Rejected"),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
                assert_eq!(
                    serde_json::from_value::<ChargingProfileStatusEnumType>(json!(wire)).unwrap(),
                    variant
                );
            }
            // `Scheduled` belongs to other status enums, not SetChargingProfile.
            assert!(
                serde_json::from_value::<ChargingProfileStatusEnumType>(json!("Scheduled"))
                    .is_err()
            );
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(SetChargingProfileRequest::ACTION_NAME, "SetChargingProfile");
            assert_eq!(
                SetChargingProfileResponse::ACTION_NAME,
                "SetChargingProfileResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_required_fields() {
            let v = SchemaValidator::v201();
            let req = SetChargingProfileRequest {
                evse_id: 1,
                charging_profile: minimal_profile(),
                custom_data: None,
            };
            let full = serde_json::to_value(&req).unwrap();
            assert!(v.validate_call("SetChargingProfile", &full).is_ok());
            for missing in ["evseId", "chargingProfile"] {
                let mut bad = full.clone();
                bad.as_object_mut().unwrap().remove(missing);
                assert!(
                    v.validate_call("SetChargingProfile", &bad).is_err(),
                    "expected rejection when `{missing}` is absent"
                );
            }
        }

        #[test]
        fn schema_and_serde_reject_non_integer_evse_id() {
            let bad = json!({
                "evseId": "1",
                "chargingProfile": serde_json::to_value(minimal_profile()).unwrap()
            });
            assert!(SchemaValidator::v201()
                .validate_call("SetChargingProfile", &bad)
                .is_err());
            assert!(serde_json::from_value::<SetChargingProfileRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_profile_with_empty_schedule() {
            // `chargingSchedule` has `minItems: 1` — an empty list is rejected by
            // the schema even though the Rust `Vec` permits it.
            let mut profile = serde_json::to_value(minimal_profile()).unwrap();
            profile["chargingSchedule"] = json!([]);
            let bad = json!({ "evseId": 1, "chargingProfile": profile });
            assert!(SchemaValidator::v201()
                .validate_call("SetChargingProfile", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_profile_missing_required_field() {
            // Drop a required field of the nested ChargingProfileType.
            let mut profile = serde_json::to_value(minimal_profile()).unwrap();
            profile
                .as_object_mut()
                .unwrap()
                .remove("chargingProfileKind");
            let bad = json!({ "evseId": 1, "chargingProfile": profile });
            assert!(SchemaValidator::v201()
                .validate_call("SetChargingProfile", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_response_missing_or_unknown_status() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("SetChargingProfile", &json!({}))
                .is_err());
            let bad = json!({ "status": "Scheduled" });
            assert!(v.validate_call_result("SetChargingProfile", &bad).is_err());
            assert!(serde_json::from_value::<SetChargingProfileResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({
                "evseId": 1,
                "chargingProfile": serde_json::to_value(minimal_profile()).unwrap(),
                "bogusExtra": true
            });
            assert!(v.validate_call("SetChargingProfile", &bad_req).is_err());
            let bad_resp = json!({ "status": "Accepted", "bogusExtra": true });
            assert!(v
                .validate_call_result("SetChargingProfile", &bad_resp)
                .is_err());
        }
    }

    mod log_status_notification {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CustomDataType, UploadLogStatusEnumType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the required `status`; `requestId` / `customData` stay off the
            // wire when `None`.
            let req = LogStatusNotificationRequest {
                status: UploadLogStatusEnumType::Idle,
                request_id: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Idle" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            for key in ["requestId", "customData"] {
                assert!(!obj.contains_key(key));
            }
            let back: LogStatusNotificationRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("LogStatusNotification", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_request_id_round_trips_and_validates() {
            let req = LogStatusNotificationRequest {
                status: UploadLogStatusEnumType::Uploading,
                request_id: Some(1234),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["status"], json!("Uploading"));
            assert_eq!(wire["requestId"], json!(1234));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("LogStatusNotification", &wire)
                .is_ok());
            let back: LogStatusNotificationRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_empty_is_object_and_validates() {
            let resp = LogStatusNotificationResponse::default();
            assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
            assert!(SchemaValidator::v201()
                .validate_call_result("LogStatusNotification", &json!({}))
                .is_ok());
            let back: LogStatusNotificationResponse = serde_json::from_value(json!({})).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_exact_wire_values_and_schema_accepts() {
            // Every one of the 8 members round-trips to its FINAL-schema wire
            // spelling, and the bundled schema accepts a request carrying it.
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (UploadLogStatusEnumType::BadMessage, "BadMessage"),
                (UploadLogStatusEnumType::Idle, "Idle"),
                (
                    UploadLogStatusEnumType::NotSupportedOperation,
                    "NotSupportedOperation",
                ),
                (
                    UploadLogStatusEnumType::PermissionDenied,
                    "PermissionDenied",
                ),
                (UploadLogStatusEnumType::Uploaded, "Uploaded"),
                (UploadLogStatusEnumType::UploadFailure, "UploadFailure"),
                (UploadLogStatusEnumType::Uploading, "Uploading"),
                (
                    UploadLogStatusEnumType::AcceptedCanceled,
                    "AcceptedCanceled",
                ),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
                assert_eq!(
                    serde_json::from_value::<UploadLogStatusEnumType>(json!(wire)).unwrap(),
                    variant
                );
                assert!(v
                    .validate_call("LogStatusNotification", &json!({ "status": wire }))
                    .is_ok());
            }
        }

        #[test]
        fn enum_and_schema_reject_unknown_status() {
            let v = SchemaValidator::v201();
            for bad in ["idle", "Bogus", "Upload"] {
                assert!(serde_json::from_value::<UploadLogStatusEnumType>(json!(bad)).is_err());
                assert!(v
                    .validate_call("LogStatusNotification", &json!({ "status": bad }))
                    .is_err());
            }
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                LogStatusNotificationRequest::ACTION_NAME,
                "LogStatusNotification"
            );
            assert_eq!(
                LogStatusNotificationResponse::ACTION_NAME,
                "LogStatusNotificationResponse"
            );
        }

        #[test]
        fn schema_rejects_missing_status_and_non_integer_request_id() {
            let v = SchemaValidator::v201();
            // `status` is required.
            assert!(v
                .validate_call("LogStatusNotification", &json!({}))
                .is_err());
            // `requestId` must be an integer — both schema and serde reject a string.
            let bad = json!({ "status": "Idle", "requestId": "1" });
            assert!(v.validate_call("LogStatusNotification", &bad).is_err());
            assert!(serde_json::from_value::<LogStatusNotificationRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({ "status": "Idle", "bogusExtra": true });
            assert!(v.validate_call("LogStatusNotification", &bad_req).is_err());
            let bad_resp = json!({ "bogusExtra": true });
            assert!(v
                .validate_call_result("LogStatusNotification", &bad_resp)
                .is_err());
        }
    }

    mod cost_updated {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::CustomDataType;

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Both `totalCost` and `transactionId` are required; `customData`
            // stays off the wire when `None`. `0.0` is exactly representable, so
            // derived `PartialEq` round-trips reliably.
            let req = CostUpdatedRequest {
                total_cost: 0.0,
                transaction_id: "tx-001".to_string(),
                custom_data: None,
            };
            let expected = json!({ "totalCost": 0.0, "transactionId": "tx-001" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(!expected.as_object().unwrap().contains_key("customData"));
            let back: CostUpdatedRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("CostUpdated", &expected)
                .is_ok());
        }

        #[test]
        fn total_cost_round_trips_as_number_integral_and_fractional() {
            let v = SchemaValidator::v201();
            // Integral and fractional values, both exactly representable.
            for (cost, expected_json) in [(12.0_f64, json!(12.0)), (12.5_f64, json!(12.5))] {
                let req = CostUpdatedRequest {
                    total_cost: cost,
                    transaction_id: "tx-42".to_string(),
                    custom_data: None,
                };
                let wire = serde_json::to_value(&req).unwrap();
                // Serializes as a JSON number, not a string.
                assert!(wire["totalCost"].is_number());
                assert_eq!(wire["totalCost"], expected_json);
                assert_eq!(wire["transactionId"], json!("tx-42"));
                assert!(v.validate_call("CostUpdated", &wire).is_ok());
                let back: CostUpdatedRequest = serde_json::from_value(wire).unwrap();
                assert_eq!(back, req);
            }
        }

        #[test]
        fn request_with_custom_data_round_trips_and_validates() {
            let req = CostUpdatedRequest {
                total_cost: 3.25,
                transaction_id: "tx-7".to_string(),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("CostUpdated", &wire)
                .is_ok());
            let back: CostUpdatedRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_empty_is_object_and_validates() {
            let resp = CostUpdatedResponse::default();
            assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
            assert!(SchemaValidator::v201()
                .validate_call_result("CostUpdated", &json!({}))
                .is_ok());
            let back: CostUpdatedResponse = serde_json::from_value(json!({})).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(CostUpdatedRequest::ACTION_NAME, "CostUpdated");
            assert_eq!(CostUpdatedResponse::ACTION_NAME, "CostUpdatedResponse");
        }

        #[test]
        fn schema_rejects_missing_required_fields() {
            let v = SchemaValidator::v201();
            // Both fields required.
            assert!(v.validate_call("CostUpdated", &json!({})).is_err());
            assert!(v
                .validate_call("CostUpdated", &json!({ "totalCost": 1.0 }))
                .is_err());
            assert!(v
                .validate_call("CostUpdated", &json!({ "transactionId": "tx-1" }))
                .is_err());
        }

        #[test]
        fn schema_rejects_wrong_types() {
            let v = SchemaValidator::v201();
            // `totalCost` must be a number, not a string.
            let bad_cost = json!({ "totalCost": "1.0", "transactionId": "tx-1" });
            assert!(v.validate_call("CostUpdated", &bad_cost).is_err());
            assert!(serde_json::from_value::<CostUpdatedRequest>(bad_cost).is_err());
            // `transactionId` must be a string, not a number.
            let bad_id = json!({ "totalCost": 1.0, "transactionId": 1 });
            assert!(v.validate_call("CostUpdated", &bad_id).is_err());
            assert!(serde_json::from_value::<CostUpdatedRequest>(bad_id).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({ "totalCost": 1.0, "transactionId": "tx-1", "bogusExtra": true });
            assert!(v.validate_call("CostUpdated", &bad_req).is_err());
            let bad_resp = json!({ "bogusExtra": true });
            assert!(v.validate_call_result("CostUpdated", &bad_resp).is_err());
        }
    }

    /// `PublishFirmwareStatusNotification` — the 2.0.1 publish-firmware progress
    /// report (#179). Mirrors `FirmwareStatusNotification` (status enum + empty
    /// response) but adds the optional `location` URI list.
    mod publish_firmware_status_notification {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CustomDataType, PublishFirmwareStatusEnumType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the required `status`; `location` / `requestId` / `customData`
            // stay off the wire when `None`.
            let req = PublishFirmwareStatusNotificationRequest {
                status: PublishFirmwareStatusEnumType::Idle,
                location: None,
                request_id: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Idle" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            for key in ["location", "requestId", "customData"] {
                assert!(!obj.contains_key(key));
            }
            let back: PublishFirmwareStatusNotificationRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("PublishFirmwareStatusNotification", &expected)
                .is_ok());
        }

        #[test]
        fn published_request_with_location_round_trips_and_validates() {
            // A `Published` notification carries the download URIs plus the
            // correlating `requestId` and a vendor extension.
            let req = PublishFirmwareStatusNotificationRequest {
                status: PublishFirmwareStatusEnumType::Published,
                location: Some(vec![
                    "http://lc.example/fw/image.bin".to_string(),
                    "ftp://lc.example/fw/image.bin".to_string(),
                ]),
                request_id: Some(1234),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let expected = json!({
                "status": "Published",
                "location": [
                    "http://lc.example/fw/image.bin",
                    "ftp://lc.example/fw/image.bin"
                ],
                "requestId": 1234,
                "customData": { "vendorId": "com.example" }
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("PublishFirmwareStatusNotification", &expected)
                .is_ok());
            let back: PublishFirmwareStatusNotificationRequest =
                serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_empty_is_object_and_validates() {
            let resp = PublishFirmwareStatusNotificationResponse::default();
            assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
            assert!(SchemaValidator::v201()
                .validate_call_result("PublishFirmwareStatusNotification", &json!({}))
                .is_ok());
            let back: PublishFirmwareStatusNotificationResponse =
                serde_json::from_value(json!({})).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_with_custom_data_round_trips() {
            let resp = PublishFirmwareStatusNotificationResponse {
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("PublishFirmwareStatusNotification", &wire)
                .is_ok());
            let back: PublishFirmwareStatusNotificationResponse =
                serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_exact_wire_values_and_schema_accepts() {
            // Every one of the 10 members round-trips to its FINAL-schema wire
            // spelling, and the bundled schema accepts a request carrying it.
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (PublishFirmwareStatusEnumType::Idle, "Idle"),
                (
                    PublishFirmwareStatusEnumType::DownloadScheduled,
                    "DownloadScheduled",
                ),
                (PublishFirmwareStatusEnumType::Downloading, "Downloading"),
                (PublishFirmwareStatusEnumType::Downloaded, "Downloaded"),
                (PublishFirmwareStatusEnumType::Published, "Published"),
                (
                    PublishFirmwareStatusEnumType::DownloadFailed,
                    "DownloadFailed",
                ),
                (
                    PublishFirmwareStatusEnumType::DownloadPaused,
                    "DownloadPaused",
                ),
                (
                    PublishFirmwareStatusEnumType::InvalidChecksum,
                    "InvalidChecksum",
                ),
                (
                    PublishFirmwareStatusEnumType::ChecksumVerified,
                    "ChecksumVerified",
                ),
                (
                    PublishFirmwareStatusEnumType::PublishFailed,
                    "PublishFailed",
                ),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
                assert_eq!(
                    serde_json::from_value::<PublishFirmwareStatusEnumType>(json!(wire)).unwrap(),
                    variant
                );
                assert!(v
                    .validate_call(
                        "PublishFirmwareStatusNotification",
                        &json!({ "status": wire })
                    )
                    .is_ok());
            }
        }

        #[test]
        fn enum_and_schema_reject_unknown_status() {
            // `Installed` is a FirmwareStatusEnumType value, not valid here.
            let v = SchemaValidator::v201();
            for bad in ["published", "Installed", "Bogus"] {
                assert!(
                    serde_json::from_value::<PublishFirmwareStatusEnumType>(json!(bad)).is_err()
                );
                assert!(v
                    .validate_call(
                        "PublishFirmwareStatusNotification",
                        &json!({ "status": bad })
                    )
                    .is_err());
            }
        }

        #[test]
        fn schema_rejects_missing_status_and_non_array_location() {
            let v = SchemaValidator::v201();
            // `status` is required.
            assert!(v
                .validate_call("PublishFirmwareStatusNotification", &json!({}))
                .is_err());
            // `location` must be an array of strings — a bare string is rejected.
            let bad = json!({ "status": "Published", "location": "http://x" });
            assert!(v
                .validate_call("PublishFirmwareStatusNotification", &bad)
                .is_err());
            assert!(
                serde_json::from_value::<PublishFirmwareStatusNotificationRequest>(bad).is_err()
            );
        }

        #[test]
        fn schema_rejects_empty_location_array() {
            // The schema sets `minItems: 1` on `location`; an empty list is
            // rejected even though the Rust `Vec` would allow it.
            let bad = json!({ "status": "Published", "location": [] });
            assert!(SchemaValidator::v201()
                .validate_call("PublishFirmwareStatusNotification", &bad)
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({ "status": "Idle", "bogusExtra": true });
            assert!(v
                .validate_call("PublishFirmwareStatusNotification", &bad_req)
                .is_err());
            let bad_resp = json!({ "bogusExtra": true });
            assert!(v
                .validate_call_result("PublishFirmwareStatusNotification", &bad_resp)
                .is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                PublishFirmwareStatusNotificationRequest::ACTION_NAME,
                "PublishFirmwareStatusNotification"
            );
            assert_eq!(
                PublishFirmwareStatusNotificationResponse::ACTION_NAME,
                "PublishFirmwareStatusNotificationResponse"
            );
        }
    }

    mod publish_firmware {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CustomDataType, GenericStatusEnumType, StatusInfoType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the three required fields — optional retry tuning omitted.
            let req = PublishFirmwareRequest {
                location: "https://lc.example/fw/v2.bin".to_string(),
                retries: None,
                checksum: "5f4dcc3b5aa765d61d8327deb882cf99".to_string(),
                request_id: 42,
                retry_interval: None,
                custom_data: None,
            };
            let expected = json!({
                "location": "https://lc.example/fw/v2.bin",
                "checksum": "5f4dcc3b5aa765d61d8327deb882cf99",
                "requestId": 42
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: PublishFirmwareRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("PublishFirmware", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_all_optionals_round_trips_and_validates() {
            let req = PublishFirmwareRequest {
                location: "https://lc.example/fw/v2.bin".to_string(),
                retries: Some(3),
                checksum: "5f4dcc3b5aa765d61d8327deb882cf99".to_string(),
                request_id: 7,
                retry_interval: Some(30),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["retries"], json!(3));
            assert_eq!(wire["retryInterval"], json!(30));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("PublishFirmware", &wire)
                .is_ok());
            let back: PublishFirmwareRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = PublishFirmwareResponse {
                status: GenericStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("PublishFirmware", &expected)
                .is_ok());
            let back: PublishFirmwareResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejected_with_status_info_and_custom_data_round_trips() {
            let resp = PublishFirmwareResponse {
                status: GenericStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "NotSupported".to_string(),
                    additional_info: Some("local caching disabled".to_string()),
                    custom_data: None,
                }),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Rejected"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotSupported"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("PublishFirmware", &wire)
                .is_ok());
            let back: PublishFirmwareResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn generic_status_serializes_to_wire_values_and_rejects_unknown() {
            assert_eq!(
                serde_json::to_value(GenericStatusEnumType::Accepted).unwrap(),
                json!("Accepted")
            );
            assert_eq!(
                serde_json::to_value(GenericStatusEnumType::Rejected).unwrap(),
                json!("Rejected")
            );
            // `Scheduled` belongs to other status enums, not this generic one.
            assert!(serde_json::from_value::<GenericStatusEnumType>(json!("Scheduled")).is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(PublishFirmwareRequest::ACTION_NAME, "PublishFirmware");
            assert_eq!(
                PublishFirmwareResponse::ACTION_NAME,
                "PublishFirmwareResponse"
            );
        }

        #[test]
        fn schema_rejects_missing_required_request_fields() {
            let v = SchemaValidator::v201();
            let full = json!({
                "location": "https://lc.example/fw.bin",
                "checksum": "5f4dcc3b5aa765d61d8327deb882cf99",
                "requestId": 1
            });
            // Drop each required field in turn.
            for missing in ["location", "checksum", "requestId"] {
                let mut bad = full.clone();
                bad.as_object_mut().unwrap().remove(missing);
                assert!(
                    v.validate_call("PublishFirmware", &bad).is_err(),
                    "missing {missing} should fail"
                );
            }
        }

        #[test]
        fn schema_and_serde_reject_non_integer_request_id() {
            let bad = json!({
                "location": "https://lc.example/fw.bin",
                "checksum": "5f4dcc3b5aa765d61d8327deb882cf99",
                "requestId": "1"
            });
            assert!(SchemaValidator::v201()
                .validate_call("PublishFirmware", &bad)
                .is_err());
            assert!(serde_json::from_value::<PublishFirmwareRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status_and_unknown_value() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("PublishFirmware", &json!({}))
                .is_err());
            let bad = json!({ "status": "Maybe" });
            assert!(v.validate_call_result("PublishFirmware", &bad).is_err());
            assert!(serde_json::from_value::<PublishFirmwareResponse>(bad).is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({
                "location": "https://lc.example/fw.bin",
                "checksum": "5f4dcc3b5aa765d61d8327deb882cf99",
                "requestId": 1,
                "bogusExtra": true
            });
            assert!(v.validate_call("PublishFirmware", &bad_req).is_err());
            let bad_resp = json!({ "status": "Accepted", "bogusExtra": true });
            assert!(v
                .validate_call_result("PublishFirmware", &bad_resp)
                .is_err());
        }
    }

    mod get_base_report {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::CustomDataType;

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Both `requestId` and `reportBase` are required; `customData`
            // stays off the wire when `None`.
            let req = GetBaseReportRequest {
                request_id: 42,
                report_base: ReportBaseEnumType::FullInventory,
                custom_data: None,
            };
            let expected = json!({ "requestId": 42, "reportBase": "FullInventory" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(!expected.as_object().unwrap().contains_key("customData"));
            // `requestId` is a JSON integer, not a string.
            assert!(expected["requestId"].is_i64());
            let back: GetBaseReportRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("GetBaseReport", &expected)
                .is_ok());
        }

        #[test]
        fn request_report_base_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (
                    ReportBaseEnumType::ConfigurationInventory,
                    "ConfigurationInventory",
                ),
                (ReportBaseEnumType::FullInventory, "FullInventory"),
                (ReportBaseEnumType::SummaryInventory, "SummaryInventory"),
            ] {
                let req = GetBaseReportRequest {
                    request_id: 1,
                    report_base: variant,
                    custom_data: None,
                };
                let value = serde_json::to_value(&req).unwrap();
                assert_eq!(value["reportBase"], json!(wire));
                assert!(v.validate_call("GetBaseReport", &value).is_ok());
                let back: GetBaseReportRequest = serde_json::from_value(value).unwrap();
                assert_eq!(back, req);
            }
        }

        #[test]
        fn request_with_custom_data_round_trips_and_validates() {
            let req = GetBaseReportRequest {
                request_id: 7,
                report_base: ReportBaseEnumType::SummaryInventory,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("GetBaseReport", &wire)
                .is_ok());
            let back: GetBaseReportRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            // Only `status` is required; `statusInfo` / `customData` stay off
            // the wire when `None`.
            let resp = GetBaseReportResponse {
                status: GenericDeviceModelStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("statusInfo"));
            assert!(!obj.contains_key("customData"));
            let back: GetBaseReportResponse = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, resp);
            assert!(SchemaValidator::v201()
                .validate_call_result("GetBaseReport", &expected)
                .is_ok());
        }

        #[test]
        fn response_status_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (GenericDeviceModelStatusEnumType::Accepted, "Accepted"),
                (GenericDeviceModelStatusEnumType::Rejected, "Rejected"),
                (
                    GenericDeviceModelStatusEnumType::NotSupported,
                    "NotSupported",
                ),
                (
                    GenericDeviceModelStatusEnumType::EmptyResultSet,
                    "EmptyResultSet",
                ),
            ] {
                let resp = GetBaseReportResponse {
                    status: variant,
                    status_info: None,
                    custom_data: None,
                };
                let value = serde_json::to_value(&resp).unwrap();
                assert_eq!(value["status"], json!(wire));
                assert!(v.validate_call_result("GetBaseReport", &value).is_ok());
                let back: GetBaseReportResponse = serde_json::from_value(value).unwrap();
                assert_eq!(back, resp);
            }
        }

        #[test]
        fn response_with_status_info_and_custom_data_round_trips_and_validates() {
            let resp = GetBaseReportResponse {
                status: GenericDeviceModelStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "NotEnabled".to_string(),
                    additional_info: Some("Device model reporting disabled".to_string()),
                    custom_data: None,
                }),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotEnabled"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("GetBaseReport", &wire)
                .is_ok());
            let back: GetBaseReportResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(GetBaseReportRequest::ACTION_NAME, "GetBaseReport");
            assert_eq!(GetBaseReportResponse::ACTION_NAME, "GetBaseReportResponse");
        }
    }

    mod unpublish_firmware {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CustomDataType, UnpublishFirmwareStatusEnumType};

        #[test]
        fn request_matches_wire_json_and_validates() {
            let req = UnpublishFirmwareRequest {
                checksum: "8b3a8e1c9e2f4a6b0d1c3e5f7a9b0d2f".to_string(),
                custom_data: None,
            };
            let expected = json!({ "checksum": "8b3a8e1c9e2f4a6b0d1c3e5f7a9b0d2f" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let back: UnpublishFirmwareRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("UnpublishFirmware", &expected)
                .is_ok());
        }

        #[test]
        fn request_round_trips_with_custom_data() {
            let req = UnpublishFirmwareRequest {
                checksum: "deadbeef".to_string(),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("UnpublishFirmware", &wire)
                .is_ok());
            let back: UnpublishFirmwareRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = UnpublishFirmwareResponse {
                status: UnpublishFirmwareStatusEnumType::Unpublished,
                custom_data: None,
            };
            let expected = json!({ "status": "Unpublished" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("UnpublishFirmware", &expected)
                .is_ok());
            let back: UnpublishFirmwareResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_no_firmware_round_trips_with_custom_data() {
            let resp = UnpublishFirmwareResponse {
                status: UnpublishFirmwareStatusEnumType::NoFirmware,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("NoFirmware"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("UnpublishFirmware", &wire)
                .is_ok());
            let back: UnpublishFirmwareResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            for (variant, wire) in [
                (
                    UnpublishFirmwareStatusEnumType::DownloadOngoing,
                    "DownloadOngoing",
                ),
                (UnpublishFirmwareStatusEnumType::NoFirmware, "NoFirmware"),
                (UnpublishFirmwareStatusEnumType::Unpublished, "Unpublished"),
            ] {
                assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            }
            // `Published` is a PublishFirmwareStatusEnumType value, not valid here.
            assert!(
                serde_json::from_value::<UnpublishFirmwareStatusEnumType>(json!("Published"))
                    .is_err()
            );
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(UnpublishFirmwareRequest::ACTION_NAME, "UnpublishFirmware");
            assert_eq!(
                UnpublishFirmwareResponse::ACTION_NAME,
                "UnpublishFirmwareResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_checksum() {
            assert!(SchemaValidator::v201()
                .validate_call("UnpublishFirmware", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_and_serde_reject_non_string_checksum() {
            let bad = json!({ "checksum": 12345 });
            assert!(SchemaValidator::v201()
                .validate_call("UnpublishFirmware", &bad)
                .is_err());
            assert!(serde_json::from_value::<UnpublishFirmwareRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status_and_unknown_value() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("UnpublishFirmware", &json!({}))
                .is_err());
            assert!(v
                .validate_call_result("UnpublishFirmware", &json!({ "status": "Published" }))
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "UnpublishFirmware",
                    &json!({ "checksum": "abc", "bogusExtra": true })
                )
                .is_err());
            assert!(v
                .validate_call_result(
                    "UnpublishFirmware",
                    &json!({ "status": "Unpublished", "bogusExtra": true })
                )
                .is_err());
        }
    }

    mod get_monitoring_report {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{ComponentType, CustomDataType, MonitoringCriterionEnumType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only `requestId` is required; the optional filters and
            // `customData` stay off the wire when `None`.
            let req = GetMonitoringReportRequest {
                component_variable: None,
                request_id: 42,
                monitoring_criteria: None,
                custom_data: None,
            };
            let expected = json!({ "requestId": 42 });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("componentVariable"));
            assert!(!obj.contains_key("monitoringCriteria"));
            assert!(!obj.contains_key("customData"));
            // `requestId` is a JSON integer, not a string.
            assert!(expected["requestId"].is_i64());
            let back: GetMonitoringReportRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("GetMonitoringReport", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_filters_round_trips_and_validates() {
            let req = GetMonitoringReportRequest {
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
                monitoring_criteria: Some(vec![
                    MonitoringCriterionEnumType::ThresholdMonitoring,
                    MonitoringCriterionEnumType::DeltaMonitoring,
                    MonitoringCriterionEnumType::PeriodicMonitoring,
                ]),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(
                wire["componentVariable"][0]["component"]["name"],
                json!("EVSE")
            );
            assert_eq!(
                wire["monitoringCriteria"],
                json!([
                    "ThresholdMonitoring",
                    "DeltaMonitoring",
                    "PeriodicMonitoring"
                ])
            );
            assert!(SchemaValidator::v201()
                .validate_call("GetMonitoringReport", &wire)
                .is_ok());
            let back: GetMonitoringReportRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_monitoring_criteria_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (
                    MonitoringCriterionEnumType::ThresholdMonitoring,
                    "ThresholdMonitoring",
                ),
                (
                    MonitoringCriterionEnumType::DeltaMonitoring,
                    "DeltaMonitoring",
                ),
                (
                    MonitoringCriterionEnumType::PeriodicMonitoring,
                    "PeriodicMonitoring",
                ),
            ] {
                let req = GetMonitoringReportRequest {
                    component_variable: None,
                    request_id: 1,
                    monitoring_criteria: Some(vec![variant]),
                    custom_data: None,
                };
                let value = serde_json::to_value(&req).unwrap();
                assert_eq!(value["monitoringCriteria"], json!([wire]));
                assert!(v.validate_call("GetMonitoringReport", &value).is_ok());
                let back: GetMonitoringReportRequest = serde_json::from_value(value).unwrap();
                assert_eq!(back, req);
            }
        }

        #[test]
        fn request_rejects_missing_request_id() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "GetMonitoringReport",
                    &json!({ "monitoringCriteria": ["DeltaMonitoring"] })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_wrong_request_id_type() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call("GetMonitoringReport", &json!({ "requestId": "42" }))
                .is_err());
        }

        #[test]
        fn request_rejects_empty_component_variable() {
            // Schema requires `minItems: 1` when the filter is present.
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "GetMonitoringReport",
                    &json!({ "requestId": 1, "componentVariable": [] })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_empty_monitoring_criteria() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "GetMonitoringReport",
                    &json!({ "requestId": 1, "monitoringCriteria": [] })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_oversized_monitoring_criteria() {
            // Schema caps `monitoringCriteria` at `maxItems: 3`; four entries
            // (even valid ones) must fail.
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "GetMonitoringReport",
                    &json!({
                        "requestId": 1,
                        "monitoringCriteria": [
                            "ThresholdMonitoring",
                            "DeltaMonitoring",
                            "PeriodicMonitoring",
                            "ThresholdMonitoring"
                        ]
                    })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_unknown_criterion() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "GetMonitoringReport",
                    &json!({ "requestId": 1, "monitoringCriteria": ["HourlyMonitoring"] })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "GetMonitoringReport",
                    &json!({ "requestId": 1, "unexpected": true })
                )
                .is_err());
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            // Only `status` is required; `statusInfo` / `customData` stay off
            // the wire when `None`.
            let resp = GetMonitoringReportResponse {
                status: GenericDeviceModelStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("statusInfo"));
            assert!(!obj.contains_key("customData"));
            let back: GetMonitoringReportResponse =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, resp);
            assert!(SchemaValidator::v201()
                .validate_call_result("GetMonitoringReport", &expected)
                .is_ok());
        }

        #[test]
        fn response_status_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (GenericDeviceModelStatusEnumType::Accepted, "Accepted"),
                (GenericDeviceModelStatusEnumType::Rejected, "Rejected"),
                (
                    GenericDeviceModelStatusEnumType::NotSupported,
                    "NotSupported",
                ),
                (
                    GenericDeviceModelStatusEnumType::EmptyResultSet,
                    "EmptyResultSet",
                ),
            ] {
                let resp = GetMonitoringReportResponse {
                    status: variant,
                    status_info: None,
                    custom_data: None,
                };
                let value = serde_json::to_value(&resp).unwrap();
                assert_eq!(value["status"], json!(wire));
                assert!(v
                    .validate_call_result("GetMonitoringReport", &value)
                    .is_ok());
                let back: GetMonitoringReportResponse = serde_json::from_value(value).unwrap();
                assert_eq!(back, resp);
            }
        }

        #[test]
        fn response_with_status_info_and_custom_data_round_trips_and_validates() {
            let resp = GetMonitoringReportResponse {
                status: GenericDeviceModelStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "NotEnabled".to_string(),
                    additional_info: Some("Monitoring reporting disabled".to_string()),
                    custom_data: None,
                }),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotEnabled"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("GetMonitoringReport", &wire)
                .is_ok());
            let back: GetMonitoringReportResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejects_missing_status() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("GetMonitoringReport", &json!({}))
                .is_err());
        }

        #[test]
        fn response_rejects_unknown_status() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("GetMonitoringReport", &json!({ "status": "Maybe" }))
                .is_err());
        }

        #[test]
        fn response_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result(
                    "GetMonitoringReport",
                    &json!({ "status": "Accepted", "unexpected": true })
                )
                .is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                GetMonitoringReportRequest::ACTION_NAME,
                "GetMonitoringReport"
            );
            assert_eq!(
                GetMonitoringReportResponse::ACTION_NAME,
                "GetMonitoringReportResponse"
            );
        }
    }

    mod notify_monitoring_report {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            ComponentType, CustomDataType, MonitorEnumType, MonitoringDataType,
            VariableMonitoringType, VariableType,
        };

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
        fn request_minimal_matches_wire_json_and_validates() {
            // Only `requestId` / `seqNo` / `generatedAt` are required; the
            // optional `monitor` / `tbc` / `customData` stay off the wire.
            let req = NotifyMonitoringReportRequest {
                monitor: None,
                request_id: 42,
                tbc: None,
                seq_no: 0,
                generated_at: "2022-01-01T10:00:00Z".to_string(),
                custom_data: None,
            };
            let expected = json!({
                "requestId": 42,
                "seqNo": 0,
                "generatedAt": "2022-01-01T10:00:00Z"
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("monitor"));
            assert!(!obj.contains_key("tbc"));
            assert!(!obj.contains_key("customData"));
            assert!(expected["requestId"].is_i64());
            assert!(expected["seqNo"].is_i64());
            let back: NotifyMonitoringReportRequest =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("NotifyMonitoringReport", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_monitor_round_trips_and_validates() {
            let req = NotifyMonitoringReportRequest {
                monitor: Some(vec![sample_monitor()]),
                request_id: 7,
                tbc: Some(true),
                seq_no: 3,
                generated_at: "2022-01-01T10:05:00Z".to_string(),
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["monitor"][0]["component"]["name"], json!("EVSE"));
            assert_eq!(
                wire["monitor"][0]["variableMonitoring"][0]["type"],
                json!("UpperThreshold")
            );
            assert_eq!(wire["tbc"], json!(true));
            assert!(SchemaValidator::v201()
                .validate_call("NotifyMonitoringReport", &wire)
                .is_ok());
            let back: NotifyMonitoringReportRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_monitor_kind_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
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
                let mut monitor = sample_monitor();
                monitor.variable_monitoring[0].kind = variant;
                let req = NotifyMonitoringReportRequest {
                    monitor: Some(vec![monitor]),
                    request_id: 1,
                    tbc: None,
                    seq_no: 0,
                    generated_at: "2022-01-01T10:00:00Z".to_string(),
                    custom_data: None,
                };
                let value = serde_json::to_value(&req).unwrap();
                assert_eq!(
                    value["monitor"][0]["variableMonitoring"][0]["type"],
                    json!(wire)
                );
                assert!(v.validate_call("NotifyMonitoringReport", &value).is_ok());
                let back: NotifyMonitoringReportRequest = serde_json::from_value(value).unwrap();
                assert_eq!(back, req);
            }
        }

        #[test]
        fn request_rejects_missing_seq_no() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({ "requestId": 1, "generatedAt": "2022-01-01T10:00:00Z" })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_missing_generated_at() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({ "requestId": 1, "seqNo": 0 })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_wrong_request_id_type() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({ "requestId": "1", "seqNo": 0, "generatedAt": "2022-01-01T10:00:00Z" })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_empty_monitor_array() {
            // Schema requires `minItems: 1` when `monitor` is present.
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({
                        "requestId": 1, "seqNo": 0,
                        "generatedAt": "2022-01-01T10:00:00Z",
                        "monitor": []
                    })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_empty_variable_monitoring_array() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({
                        "requestId": 1, "seqNo": 0,
                        "generatedAt": "2022-01-01T10:00:00Z",
                        "monitor": [{
                            "component": { "name": "EVSE" },
                            "variable": { "name": "Temperature" },
                            "variableMonitoring": []
                        }]
                    })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_missing_required_variable_monitoring_field() {
            // `severity` is required inside VariableMonitoringType.
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({
                        "requestId": 1, "seqNo": 0,
                        "generatedAt": "2022-01-01T10:00:00Z",
                        "monitor": [{
                            "component": { "name": "EVSE" },
                            "variable": { "name": "Temperature" },
                            "variableMonitoring": [{
                                "id": 1, "transaction": false,
                                "value": 80.0, "type": "UpperThreshold"
                            }]
                        }]
                    })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_unknown_monitor_kind() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({
                        "requestId": 1, "seqNo": 0,
                        "generatedAt": "2022-01-01T10:00:00Z",
                        "monitor": [{
                            "component": { "name": "EVSE" },
                            "variable": { "name": "Temperature" },
                            "variableMonitoring": [{
                                "id": 1, "transaction": false, "value": 80.0,
                                "type": "Hourly", "severity": 5
                            }]
                        }]
                    })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "NotifyMonitoringReport",
                    &json!({
                        "requestId": 1, "seqNo": 0,
                        "generatedAt": "2022-01-01T10:00:00Z",
                        "unexpected": true
                    })
                )
                .is_err());
        }

        #[test]
        fn response_empty_matches_wire_json_and_validates() {
            let resp = NotifyMonitoringReportResponse::default();
            let expected = json!({});
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            let back: NotifyMonitoringReportResponse =
                serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, resp);
            assert!(SchemaValidator::v201()
                .validate_call_result("NotifyMonitoringReport", &expected)
                .is_ok());
        }

        #[test]
        fn response_with_custom_data_round_trips_and_validates() {
            let resp = NotifyMonitoringReportResponse {
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("NotifyMonitoringReport", &wire)
                .is_ok());
            let back: NotifyMonitoringReportResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("NotifyMonitoringReport", &json!({ "unexpected": true }))
                .is_err());
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

    mod set_monitoring_base {
        use super::*;
        use crate::schema_validation::SchemaValidator;

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // `monitoringBase` is the only required field; `customData` stays
            // off the wire when `None`.
            let req = SetMonitoringBaseRequest {
                monitoring_base: MonitorBaseEnumType::All,
                custom_data: None,
            };
            let expected = json!({ "monitoringBase": "All" });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(!expected.as_object().unwrap().contains_key("customData"));
            let back: SetMonitoringBaseRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("SetMonitoringBase", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_custom_data_round_trips_and_validates() {
            let req = SetMonitoringBaseRequest {
                monitoring_base: MonitorBaseEnumType::FactoryDefault,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["monitoringBase"], json!("FactoryDefault"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("SetMonitoringBase", &wire)
                .is_ok());
            let back: SetMonitoringBaseRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_monitoring_base_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (MonitorBaseEnumType::All, "All"),
                (MonitorBaseEnumType::FactoryDefault, "FactoryDefault"),
                (MonitorBaseEnumType::HardWiredOnly, "HardWiredOnly"),
            ] {
                let req = SetMonitoringBaseRequest {
                    monitoring_base: variant,
                    custom_data: None,
                };
                let value = serde_json::to_value(&req).unwrap();
                assert_eq!(value["monitoringBase"], json!(wire));
                assert!(v.validate_call("SetMonitoringBase", &value).is_ok());
                let back: SetMonitoringBaseRequest = serde_json::from_value(value).unwrap();
                assert_eq!(back, req);
            }
        }

        #[test]
        fn request_rejects_missing_monitoring_base() {
            let v = SchemaValidator::v201();
            assert!(v.validate_call("SetMonitoringBase", &json!({})).is_err());
        }

        #[test]
        fn request_rejects_wrong_monitoring_base_type() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call("SetMonitoringBase", &json!({ "monitoringBase": 1 }))
                .is_err());
        }

        #[test]
        fn request_rejects_unknown_monitoring_base() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "SetMonitoringBase",
                    &json!({ "monitoringBase": "SomeMonitors" })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "SetMonitoringBase",
                    &json!({ "monitoringBase": "All", "unexpected": true })
                )
                .is_err());
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            // Only `status` is required; `statusInfo` / `customData` stay off
            // the wire when `None`.
            let resp = SetMonitoringBaseResponse {
                status: GenericDeviceModelStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("statusInfo"));
            assert!(!obj.contains_key("customData"));
            let back: SetMonitoringBaseResponse = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, resp);
            assert!(SchemaValidator::v201()
                .validate_call_result("SetMonitoringBase", &expected)
                .is_ok());
        }

        #[test]
        fn response_status_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (GenericDeviceModelStatusEnumType::Accepted, "Accepted"),
                (GenericDeviceModelStatusEnumType::Rejected, "Rejected"),
                (
                    GenericDeviceModelStatusEnumType::NotSupported,
                    "NotSupported",
                ),
                (
                    GenericDeviceModelStatusEnumType::EmptyResultSet,
                    "EmptyResultSet",
                ),
            ] {
                let resp = SetMonitoringBaseResponse {
                    status: variant,
                    status_info: None,
                    custom_data: None,
                };
                let value = serde_json::to_value(&resp).unwrap();
                assert_eq!(value["status"], json!(wire));
                assert!(v.validate_call_result("SetMonitoringBase", &value).is_ok());
                let back: SetMonitoringBaseResponse = serde_json::from_value(value).unwrap();
                assert_eq!(back, resp);
            }
        }

        #[test]
        fn response_with_status_info_and_custom_data_round_trips_and_validates() {
            let resp = SetMonitoringBaseResponse {
                status: GenericDeviceModelStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "NotEnabled".to_string(),
                    additional_info: Some("Monitoring base not supported".to_string()),
                    custom_data: None,
                }),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotEnabled"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("SetMonitoringBase", &wire)
                .is_ok());
            let back: SetMonitoringBaseResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_rejects_missing_status() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("SetMonitoringBase", &json!({}))
                .is_err());
        }

        #[test]
        fn response_rejects_unknown_status() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("SetMonitoringBase", &json!({ "status": "Maybe" }))
                .is_err());
        }

        #[test]
        fn response_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result(
                    "SetMonitoringBase",
                    &json!({ "status": "Accepted", "unexpected": true })
                )
                .is_err());
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

    mod clear_variable_monitoring {
        use super::*;
        use crate::schema_validation::SchemaValidator;

        #[test]
        fn request_round_trips_and_validates() {
            let req = ClearVariableMonitoringRequest {
                id: vec![1, 2, 3],
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire, json!({ "id": [1, 2, 3] }));
            assert!(SchemaValidator::v201()
                .validate_call("ClearVariableMonitoring", &wire)
                .is_ok());
            let back: ClearVariableMonitoringRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_round_trips_and_validates() {
            let resp = ClearVariableMonitoringResponse {
                clear_monitoring_result: vec![
                    ClearMonitoringResultType {
                        status: ClearMonitoringStatusEnumType::Accepted,
                        id: 1,
                        status_info: None,
                        custom_data: None,
                    },
                    ClearMonitoringResultType {
                        status: ClearMonitoringStatusEnumType::Rejected,
                        id: 2,
                        status_info: Some(StatusInfoType {
                            reason_code: "InUse".to_string(),
                            additional_info: Some("monitor busy".to_string()),
                            custom_data: None,
                        }),
                        custom_data: None,
                    },
                ],
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(
                wire["clearMonitoringResult"][0]["status"],
                json!("Accepted")
            );
            assert_eq!(
                wire["clearMonitoringResult"][1]["status"],
                json!("Rejected")
            );
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearVariableMonitoring", &wire)
                .is_ok());
            let back: ClearVariableMonitoringResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn all_status_wire_spellings_round_trip_and_validate() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (ClearMonitoringStatusEnumType::Accepted, "Accepted"),
                (ClearMonitoringStatusEnumType::Rejected, "Rejected"),
                (ClearMonitoringStatusEnumType::NotFound, "NotFound"),
            ] {
                let resp = ClearVariableMonitoringResponse {
                    clear_monitoring_result: vec![ClearMonitoringResultType {
                        status: variant,
                        id: 5,
                        status_info: None,
                        custom_data: None,
                    }],
                    custom_data: None,
                };
                let value = serde_json::to_value(&resp).unwrap();
                assert_eq!(value["clearMonitoringResult"][0]["status"], json!(wire));
                assert!(v
                    .validate_call_result("ClearVariableMonitoring", &value)
                    .is_ok());
            }
        }

        #[test]
        fn request_rejects_missing_id() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call("ClearVariableMonitoring", &json!({}))
                .is_err());
        }

        #[test]
        fn request_rejects_empty_id_array() {
            // Schema enforces `minItems: 1` even though serde accepts an empty Vec.
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call("ClearVariableMonitoring", &json!({ "id": [] }))
                .is_err());
        }

        #[test]
        fn request_rejects_non_integer_id() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call("ClearVariableMonitoring", &json!({ "id": ["oops"] }))
                .is_err());
        }

        #[test]
        fn request_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call(
                    "ClearVariableMonitoring",
                    &json!({ "id": [1], "unexpected": true })
                )
                .is_err());
        }

        #[test]
        fn response_rejects_missing_result_list() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("ClearVariableMonitoring", &json!({}))
                .is_err());
        }

        #[test]
        fn response_rejects_empty_result_list() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result(
                    "ClearVariableMonitoring",
                    &json!({ "clearMonitoringResult": [] })
                )
                .is_err());
        }

        #[test]
        fn response_rejects_result_missing_required_fields() {
            let v = SchemaValidator::v201();
            // Missing `id`.
            assert!(v
                .validate_call_result(
                    "ClearVariableMonitoring",
                    &json!({ "clearMonitoringResult": [{ "status": "Accepted" }] })
                )
                .is_err());
            // Missing `status`.
            assert!(v
                .validate_call_result(
                    "ClearVariableMonitoring",
                    &json!({ "clearMonitoringResult": [{ "id": 1 }] })
                )
                .is_err());
        }

        #[test]
        fn response_rejects_unknown_status() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result(
                    "ClearVariableMonitoring",
                    &json!({ "clearMonitoringResult": [{ "status": "Maybe", "id": 1 }] })
                )
                .is_err());
        }

        #[test]
        fn response_rejects_additional_properties_on_result() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result(
                    "ClearVariableMonitoring",
                    &json!({
                        "clearMonitoringResult": [
                            { "status": "Accepted", "id": 1, "unexpected": true }
                        ]
                    })
                )
                .is_err());
        }
    }

    mod set_variable_monitoring {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            ComponentType, MonitorEnumType, SetMonitoringDataType, SetMonitoringResultType,
            SetMonitoringStatusEnumType, VariableType,
        };

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
        fn request_round_trips_and_validates() {
            let req = SetVariableMonitoringRequest {
                set_monitoring_data: vec![sample_data()],
                custom_data: None,
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(
                wire["setMonitoringData"][0]["type"],
                json!("UpperThreshold")
            );
            assert!(wire["setMonitoringData"][0]["value"].is_number());
            assert!(SchemaValidator::v201()
                .validate_call("SetVariableMonitoring", &wire)
                .is_ok());
            let back: SetVariableMonitoringRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_with_id_and_transaction_validates() {
            let mut data = sample_data();
            data.id = Some(9);
            data.transaction = Some(true);
            let req = SetVariableMonitoringRequest {
                set_monitoring_data: vec![data],
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["setMonitoringData"][0]["id"], json!(9));
            assert_eq!(wire["setMonitoringData"][0]["transaction"], json!(true));
            assert!(SchemaValidator::v201()
                .validate_call("SetVariableMonitoring", &wire)
                .is_ok());
        }

        #[test]
        fn request_rejects_missing_set_monitoring_data() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call("SetVariableMonitoring", &json!({}))
                .is_err());
        }

        #[test]
        fn request_rejects_empty_set_monitoring_data() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call("SetVariableMonitoring", &json!({ "setMonitoringData": [] }))
                .is_err());
        }

        #[test]
        fn request_rejects_data_entry_missing_required_fields() {
            let v = SchemaValidator::v201();
            // Missing `value`.
            assert!(v
                .validate_call(
                    "SetVariableMonitoring",
                    &json!({ "setMonitoringData": [{
                        "type": "UpperThreshold",
                        "severity": 5,
                        "component": { "name": "EVSE" },
                        "variable": { "name": "Temperature" }
                    }] })
                )
                .is_err());
            // Missing `component`.
            assert!(v
                .validate_call(
                    "SetVariableMonitoring",
                    &json!({ "setMonitoringData": [{
                        "value": 1.0,
                        "type": "UpperThreshold",
                        "severity": 5,
                        "variable": { "name": "Temperature" }
                    }] })
                )
                .is_err());
        }

        #[test]
        fn request_rejects_wrong_value_type() {
            let v = SchemaValidator::v201();
            let mut wire = serde_json::to_value(SetVariableMonitoringRequest {
                set_monitoring_data: vec![sample_data()],
                custom_data: None,
            })
            .unwrap();
            wire["setMonitoringData"][0]["value"] = json!("hot");
            assert!(v.validate_call("SetVariableMonitoring", &wire).is_err());
        }

        #[test]
        fn request_rejects_unknown_monitor_type() {
            let v = SchemaValidator::v201();
            let mut wire = serde_json::to_value(SetVariableMonitoringRequest {
                set_monitoring_data: vec![sample_data()],
                custom_data: None,
            })
            .unwrap();
            wire["setMonitoringData"][0]["type"] = json!("Sideways");
            assert!(v.validate_call("SetVariableMonitoring", &wire).is_err());
        }

        #[test]
        fn request_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            // On the request object.
            assert!(v
                .validate_call(
                    "SetVariableMonitoring",
                    &json!({ "setMonitoringData": [], "unexpected": true })
                )
                .is_err());
            // On the nested data object.
            let mut wire = serde_json::to_value(SetVariableMonitoringRequest {
                set_monitoring_data: vec![sample_data()],
                custom_data: None,
            })
            .unwrap();
            wire["setMonitoringData"][0]["unexpected"] = json!(true);
            assert!(v.validate_call("SetVariableMonitoring", &wire).is_err());
        }

        #[test]
        fn response_round_trips_and_validates() {
            let resp = SetVariableMonitoringResponse {
                set_monitoring_result: vec![sample_result()],
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["setMonitoringResult"][0]["status"], json!("Accepted"));
            assert!(SchemaValidator::v201()
                .validate_call_result("SetVariableMonitoring", &wire)
                .is_ok());
            let back: SetVariableMonitoringResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_result_status_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
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
                let mut result = sample_result();
                result.status = variant;
                let resp = SetVariableMonitoringResponse {
                    set_monitoring_result: vec![result],
                    custom_data: None,
                };
                let value = serde_json::to_value(&resp).unwrap();
                assert_eq!(value["setMonitoringResult"][0]["status"], json!(wire));
                assert!(v
                    .validate_call_result("SetVariableMonitoring", &value)
                    .is_ok());
                let back: SetVariableMonitoringResponse = serde_json::from_value(value).unwrap();
                assert_eq!(back, resp);
            }
        }

        #[test]
        fn response_rejects_missing_set_monitoring_result() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result("SetVariableMonitoring", &json!({}))
                .is_err());
        }

        #[test]
        fn response_rejects_empty_set_monitoring_result() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result(
                    "SetVariableMonitoring",
                    &json!({ "setMonitoringResult": [] })
                )
                .is_err());
        }

        #[test]
        fn response_rejects_result_missing_required_fields() {
            let v = SchemaValidator::v201();
            // Missing `status`.
            assert!(v
                .validate_call_result(
                    "SetVariableMonitoring",
                    &json!({ "setMonitoringResult": [{
                        "type": "UpperThreshold",
                        "severity": 5,
                        "component": { "name": "EVSE" },
                        "variable": { "name": "Temperature" }
                    }] })
                )
                .is_err());
            // Missing `severity` (required on the result object).
            assert!(v
                .validate_call_result(
                    "SetVariableMonitoring",
                    &json!({ "setMonitoringResult": [{
                        "status": "Accepted",
                        "type": "UpperThreshold",
                        "component": { "name": "EVSE" },
                        "variable": { "name": "Temperature" }
                    }] })
                )
                .is_err());
        }

        #[test]
        fn response_rejects_unknown_status() {
            let v = SchemaValidator::v201();
            assert!(v
                .validate_call_result(
                    "SetVariableMonitoring",
                    &json!({ "setMonitoringResult": [{
                        "status": "Maybe",
                        "type": "UpperThreshold",
                        "severity": 5,
                        "component": { "name": "EVSE" },
                        "variable": { "name": "Temperature" }
                    }] })
                )
                .is_err());
        }

        #[test]
        fn response_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let mut wire = serde_json::to_value(SetVariableMonitoringResponse {
                set_monitoring_result: vec![sample_result()],
                custom_data: None,
            })
            .unwrap();
            wire["setMonitoringResult"][0]["unexpected"] = json!(true);
            assert!(v
                .validate_call_result("SetVariableMonitoring", &wire)
                .is_err());
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

    mod update_firmware {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            CustomDataType, FirmwareType, StatusInfoType, UpdateFirmwareStatusEnumType,
        };

        fn minimal_firmware() -> FirmwareType {
            FirmwareType {
                location: "https://csms.example/fw/v2.bin".to_string(),
                retrieve_date_time: "2026-07-03T00:00:00Z".to_string(),
                install_date_time: None,
                signing_certificate: None,
                signature: None,
                custom_data: None,
            }
        }

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            // Only the two required fields — `requestId` plus a minimal
            // `firmware` (`location` + `retrieveDateTime`); everything else off
            // the wire.
            let req = UpdateFirmwareRequest {
                retries: None,
                retry_interval: None,
                request_id: 42,
                firmware: minimal_firmware(),
                custom_data: None,
            };
            let expected = json!({
                "requestId": 42,
                "firmware": {
                    "location": "https://csms.example/fw/v2.bin",
                    "retrieveDateTime": "2026-07-03T00:00:00Z"
                }
            });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("retries"));
            assert!(!obj.contains_key("retryInterval"));
            assert!(!obj.contains_key("customData"));
            // `requestId` is a JSON integer, not a string.
            assert!(expected["requestId"].is_i64());
            let back: UpdateFirmwareRequest = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(back, req);
            assert!(SchemaValidator::v201()
                .validate_call("UpdateFirmware", &expected)
                .is_ok());
        }

        #[test]
        fn request_with_all_optionals_round_trips_and_validates() {
            let req = UpdateFirmwareRequest {
                retries: Some(3),
                retry_interval: Some(30),
                request_id: 7,
                firmware: FirmwareType {
                    location: "https://csms.example/fw/v2.bin".to_string(),
                    retrieve_date_time: "2026-07-03T00:00:00Z".to_string(),
                    install_date_time: Some("2026-07-03T02:00:00Z".to_string()),
                    signing_certificate: Some(
                        "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----".to_string(),
                    ),
                    signature: Some("YmFzZTY0c2ln".to_string()),
                    custom_data: Some(CustomDataType {
                        vendor_id: "com.example.fw".to_string(),
                        extra: Default::default(),
                    }),
                },
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["retries"], json!(3));
            assert_eq!(wire["retryInterval"], json!(30));
            assert_eq!(
                wire["firmware"]["installDateTime"],
                json!("2026-07-03T02:00:00Z")
            );
            assert_eq!(wire["firmware"]["signature"], json!("YmFzZTY0c2ln"));
            assert_eq!(
                wire["firmware"]["customData"]["vendorId"],
                json!("com.example.fw")
            );
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("UpdateFirmware", &wire)
                .is_ok());
            let back: UpdateFirmwareRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = UpdateFirmwareResponse {
                status: UpdateFirmwareStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            let obj = expected.as_object().unwrap();
            assert!(!obj.contains_key("statusInfo"));
            assert!(!obj.contains_key("customData"));
            assert!(SchemaValidator::v201()
                .validate_call_result("UpdateFirmware", &expected)
                .is_ok());
            let back: UpdateFirmwareResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_status_round_trips_all_wire_spellings() {
            let v = SchemaValidator::v201();
            for (variant, wire) in [
                (UpdateFirmwareStatusEnumType::Accepted, "Accepted"),
                (UpdateFirmwareStatusEnumType::Rejected, "Rejected"),
                (
                    UpdateFirmwareStatusEnumType::AcceptedCanceled,
                    "AcceptedCanceled",
                ),
                (
                    UpdateFirmwareStatusEnumType::InvalidCertificate,
                    "InvalidCertificate",
                ),
                (
                    UpdateFirmwareStatusEnumType::RevokedCertificate,
                    "RevokedCertificate",
                ),
            ] {
                let resp = UpdateFirmwareResponse {
                    status: variant,
                    status_info: None,
                    custom_data: None,
                };
                let value = serde_json::to_value(&resp).unwrap();
                assert_eq!(value["status"], json!(wire));
                assert!(v.validate_call_result("UpdateFirmware", &value).is_ok());
                let back: UpdateFirmwareResponse = serde_json::from_value(value).unwrap();
                assert_eq!(back, resp);
            }
        }

        #[test]
        fn response_with_status_info_and_custom_data_round_trips() {
            let resp = UpdateFirmwareResponse {
                status: UpdateFirmwareStatusEnumType::InvalidCertificate,
                status_info: Some(StatusInfoType {
                    reason_code: "BadSignature".to_string(),
                    additional_info: Some("firmware signing certificate not trusted".to_string()),
                    custom_data: None,
                }),
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("InvalidCertificate"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("BadSignature"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call_result("UpdateFirmware", &wire)
                .is_ok());
            let back: UpdateFirmwareResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn update_firmware_status_serializes_and_rejects_unknown() {
            assert_eq!(
                serde_json::to_value(UpdateFirmwareStatusEnumType::AcceptedCanceled).unwrap(),
                json!("AcceptedCanceled")
            );
            // Unknown / mis-spelled values are rejected by both serde and schema.
            assert!(
                serde_json::from_value::<UpdateFirmwareStatusEnumType>(json!("Canceled")).is_err()
            );
            assert!(SchemaValidator::v201()
                .validate_call_result("UpdateFirmware", &json!({ "status": "Canceled" }))
                .is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(UpdateFirmwareRequest::ACTION_NAME, "UpdateFirmware");
            assert_eq!(
                UpdateFirmwareResponse::ACTION_NAME,
                "UpdateFirmwareResponse"
            );
        }

        #[test]
        fn schema_rejects_missing_required_request_fields() {
            let v = SchemaValidator::v201();
            let full = json!({
                "requestId": 1,
                "firmware": {
                    "location": "https://csms.example/fw.bin",
                    "retrieveDateTime": "2026-07-03T00:00:00Z"
                }
            });
            for missing in ["requestId", "firmware"] {
                let mut bad = full.clone();
                bad.as_object_mut().unwrap().remove(missing);
                assert!(
                    v.validate_call("UpdateFirmware", &bad).is_err(),
                    "missing {missing} should fail"
                );
            }
        }

        #[test]
        fn schema_rejects_firmware_missing_required_fields() {
            let v = SchemaValidator::v201();
            for missing in ["location", "retrieveDateTime"] {
                let mut firmware = json!({
                    "location": "https://csms.example/fw.bin",
                    "retrieveDateTime": "2026-07-03T00:00:00Z"
                });
                firmware.as_object_mut().unwrap().remove(missing);
                let bad = json!({ "requestId": 1, "firmware": firmware });
                assert!(
                    v.validate_call("UpdateFirmware", &bad).is_err(),
                    "firmware missing {missing} should fail"
                );
            }
        }

        #[test]
        fn schema_and_serde_reject_non_integer_request_id() {
            let bad = json!({
                "requestId": "1",
                "firmware": {
                    "location": "https://csms.example/fw.bin",
                    "retrieveDateTime": "2026-07-03T00:00:00Z"
                }
            });
            assert!(SchemaValidator::v201()
                .validate_call("UpdateFirmware", &bad)
                .is_err());
            assert!(serde_json::from_value::<UpdateFirmwareRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status() {
            assert!(SchemaValidator::v201()
                .validate_call_result("UpdateFirmware", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            let bad_req = json!({
                "requestId": 1,
                "firmware": {
                    "location": "https://csms.example/fw.bin",
                    "retrieveDateTime": "2026-07-03T00:00:00Z"
                },
                "bogusExtra": true
            });
            assert!(v.validate_call("UpdateFirmware", &bad_req).is_err());
            let bad_firmware = json!({
                "requestId": 1,
                "firmware": {
                    "location": "https://csms.example/fw.bin",
                    "retrieveDateTime": "2026-07-03T00:00:00Z",
                    "bogusExtra": true
                }
            });
            assert!(v.validate_call("UpdateFirmware", &bad_firmware).is_err());
            let bad_resp = json!({ "status": "Accepted", "bogusExtra": true });
            assert!(v.validate_call_result("UpdateFirmware", &bad_resp).is_err());
        }
    }

    /// `GetCompositeSchedule` — the 2.0.1 composite-schedule query command
    /// (#206). Read/query side of the smart-charging family: the request names
    /// a `duration` and `evseId` (both required, `chargingRateUnit` optional);
    /// the response returns a two-value `GenericStatusEnumType` plus, when
    /// `Accepted`, a `CompositeScheduleType` (a non-empty `chargingSchedulePeriod`
    /// list) and optional `StatusInfoType`.
    mod get_composite_schedule {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{
            ChargingRateUnitEnumType, ChargingSchedulePeriodType, CompositeScheduleType,
            CustomDataType, GenericStatusEnumType, StatusInfoType,
        };

        fn sample_schedule() -> CompositeScheduleType {
            CompositeScheduleType {
                evse_id: 1,
                duration: 3600,
                schedule_start: "2026-07-03T00:00:00Z".to_string(),
                charging_rate_unit: ChargingRateUnitEnumType::A,
                charging_schedule_period: vec![ChargingSchedulePeriodType {
                    start_period: 0,
                    limit: 16.0,
                    number_phases: Some(3),
                    phase_to_use: None,
                    custom_data: None,
                }],
                custom_data: None,
            }
        }

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            let req = GetCompositeScheduleRequest {
                duration: 3600,
                charging_rate_unit: None,
                evse_id: 0,
                custom_data: None,
            };
            let expected = json!({ "duration": 3600, "evseId": 0 });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("GetCompositeSchedule", &expected)
                .is_ok());
            let back: GetCompositeScheduleRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_with_rate_unit_and_custom_data_round_trips() {
            let req = GetCompositeScheduleRequest {
                duration: 900,
                charging_rate_unit: Some(ChargingRateUnitEnumType::W),
                evse_id: 2,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["duration"], json!(900));
            assert_eq!(wire["evseId"], json!(2));
            assert_eq!(wire["chargingRateUnit"], json!("W"));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("GetCompositeSchedule", &wire)
                .is_ok());
            let back: GetCompositeScheduleRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = GetCompositeScheduleResponse {
                status: GenericStatusEnumType::Rejected,
                schedule: None,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Rejected" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("GetCompositeSchedule", &expected)
                .is_ok());
            let back: GetCompositeScheduleResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_accepted_with_schedule_round_trips_and_validates() {
            let resp = GetCompositeScheduleResponse {
                status: GenericStatusEnumType::Accepted,
                schedule: Some(sample_schedule()),
                status_info: None,
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Accepted"));
            assert_eq!(wire["schedule"]["evseId"], json!(1));
            assert_eq!(
                wire["schedule"]["scheduleStart"],
                json!("2026-07-03T00:00:00Z")
            );
            assert_eq!(wire["schedule"]["chargingRateUnit"], json!("A"));
            assert_eq!(
                wire["schedule"]["chargingSchedulePeriod"][0]["limit"],
                json!(16.0)
            );
            assert!(SchemaValidator::v201()
                .validate_call_result("GetCompositeSchedule", &wire)
                .is_ok());
            let back: GetCompositeScheduleResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_with_status_info_round_trips() {
            let resp = GetCompositeScheduleResponse {
                status: GenericStatusEnumType::Rejected,
                schedule: None,
                status_info: Some(StatusInfoType {
                    reason_code: "NotSupported".to_string(),
                    additional_info: Some("no profiles installed".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NotSupported"));
            assert!(SchemaValidator::v201()
                .validate_call_result("GetCompositeSchedule", &wire)
                .is_ok());
            let back: GetCompositeScheduleResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            assert_eq!(
                serde_json::to_value(GenericStatusEnumType::Accepted).unwrap(),
                json!("Accepted")
            );
            // `Scheduled` is not a `GenericStatusEnumType` value.
            let bad = json!({ "status": "Scheduled" });
            assert!(SchemaValidator::v201()
                .validate_call_result("GetCompositeSchedule", &bad)
                .is_err());
            assert!(serde_json::from_value::<GetCompositeScheduleResponse>(bad).is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                GetCompositeScheduleRequest::ACTION_NAME,
                "GetCompositeSchedule"
            );
            assert_eq!(
                GetCompositeScheduleResponse::ACTION_NAME,
                "GetCompositeScheduleResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_required_fields() {
            let v = SchemaValidator::v201();
            // Missing `evseId`.
            assert!(v
                .validate_call("GetCompositeSchedule", &json!({ "duration": 60 }))
                .is_err());
            // Missing `duration`.
            assert!(v
                .validate_call("GetCompositeSchedule", &json!({ "evseId": 0 }))
                .is_err());
        }

        #[test]
        fn schema_rejects_non_integer_duration() {
            let bad = json!({ "duration": "60", "evseId": 0 });
            assert!(SchemaValidator::v201()
                .validate_call("GetCompositeSchedule", &bad)
                .is_err());
            assert!(serde_json::from_value::<GetCompositeScheduleRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_unknown_charging_rate_unit() {
            let bad = json!({ "duration": 60, "evseId": 0, "chargingRateUnit": "kW" });
            assert!(SchemaValidator::v201()
                .validate_call("GetCompositeSchedule", &bad)
                .is_err());
            assert!(serde_json::from_value::<GetCompositeScheduleRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status() {
            assert!(SchemaValidator::v201()
                .validate_call_result("GetCompositeSchedule", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_schedule_missing_required_and_empty_periods() {
            let v = SchemaValidator::v201();
            // Full, valid schedule as the baseline.
            let good = serde_json::to_value(sample_schedule()).unwrap();
            assert!(v
                .validate_call_result(
                    "GetCompositeSchedule",
                    &json!({ "status": "Accepted", "schedule": good })
                )
                .is_ok());
            // Each required field, dropped in turn, must fail.
            for field in [
                "evseId",
                "duration",
                "scheduleStart",
                "chargingRateUnit",
                "chargingSchedulePeriod",
            ] {
                let mut sched = serde_json::to_value(sample_schedule()).unwrap();
                sched.as_object_mut().unwrap().remove(field);
                assert!(
                    v.validate_call_result(
                        "GetCompositeSchedule",
                        &json!({ "status": "Accepted", "schedule": sched })
                    )
                    .is_err(),
                    "schedule missing `{field}` should be rejected"
                );
            }
            // `chargingSchedulePeriod` must be non-empty (schema minItems: 1).
            let mut empty = serde_json::to_value(sample_schedule()).unwrap();
            empty["chargingSchedulePeriod"] = json!([]);
            assert!(v
                .validate_call_result(
                    "GetCompositeSchedule",
                    &json!({ "status": "Accepted", "schedule": empty })
                )
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            // On the request.
            assert!(v
                .validate_call(
                    "GetCompositeSchedule",
                    &json!({ "duration": 60, "evseId": 0, "bogus": true })
                )
                .is_err());
            // On the response.
            assert!(v
                .validate_call_result(
                    "GetCompositeSchedule",
                    &json!({ "status": "Accepted", "bogus": true })
                )
                .is_err());
            // Inside the schedule object.
            let mut sched = serde_json::to_value(sample_schedule()).unwrap();
            sched["bogus"] = json!(true);
            assert!(v
                .validate_call_result(
                    "GetCompositeSchedule",
                    &json!({ "status": "Accepted", "schedule": sched })
                )
                .is_err());
        }
    }

    mod set_monitoring_level {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{CustomDataType, GenericStatusEnumType, StatusInfoType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            let req = SetMonitoringLevelRequest {
                severity: 5,
                custom_data: None,
            };
            let expected = json!({ "severity": 5 });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("SetMonitoringLevel", &expected)
                .is_ok());
            let back: SetMonitoringLevelRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_boundary_severities_and_custom_data_round_trip() {
            for severity in [0, 9] {
                let req = SetMonitoringLevelRequest {
                    severity,
                    custom_data: Some(CustomDataType {
                        vendor_id: "com.example".to_string(),
                        extra: Default::default(),
                    }),
                };
                let wire = serde_json::to_value(&req).unwrap();
                assert_eq!(wire["severity"], json!(severity));
                assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
                assert!(SchemaValidator::v201()
                    .validate_call("SetMonitoringLevel", &wire)
                    .is_ok());
                let back: SetMonitoringLevelRequest = serde_json::from_value(wire).unwrap();
                assert_eq!(back, req);
            }
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = SetMonitoringLevelResponse {
                status: GenericStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("SetMonitoringLevel", &expected)
                .is_ok());
            let back: SetMonitoringLevelResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_with_status_info_round_trips_and_validates() {
            let resp = SetMonitoringLevelResponse {
                status: GenericStatusEnumType::Rejected,
                status_info: Some(StatusInfoType {
                    reason_code: "OutOfRange".to_string(),
                    additional_info: Some("severity must be 0-9".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Rejected"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("OutOfRange"));
            assert!(SchemaValidator::v201()
                .validate_call_result("SetMonitoringLevel", &wire)
                .is_ok());
            let back: SetMonitoringLevelResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            assert_eq!(
                serde_json::to_value(GenericStatusEnumType::Accepted).unwrap(),
                json!("Accepted")
            );
            assert_eq!(
                serde_json::to_value(GenericStatusEnumType::Rejected).unwrap(),
                json!("Rejected")
            );
            // `Scheduled` is not a `GenericStatusEnumType` value.
            let bad = json!({ "status": "Scheduled" });
            assert!(SchemaValidator::v201()
                .validate_call_result("SetMonitoringLevel", &bad)
                .is_err());
            assert!(serde_json::from_value::<SetMonitoringLevelResponse>(bad).is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(SetMonitoringLevelRequest::ACTION_NAME, "SetMonitoringLevel");
            assert_eq!(
                SetMonitoringLevelResponse::ACTION_NAME,
                "SetMonitoringLevelResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_severity() {
            assert!(SchemaValidator::v201()
                .validate_call("SetMonitoringLevel", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_non_integer_severity() {
            let bad = json!({ "severity": "5" });
            assert!(SchemaValidator::v201()
                .validate_call("SetMonitoringLevel", &bad)
                .is_err());
            assert!(serde_json::from_value::<SetMonitoringLevelRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status() {
            assert!(SchemaValidator::v201()
                .validate_call_result("SetMonitoringLevel", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            // On the request.
            assert!(v
                .validate_call(
                    "SetMonitoringLevel",
                    &json!({ "severity": 5, "bogus": true })
                )
                .is_err());
            // On the response.
            assert!(v
                .validate_call_result(
                    "SetMonitoringLevel",
                    &json!({ "status": "Accepted", "bogus": true })
                )
                .is_err());
        }
    }

    mod clear_display_message {
        use super::*;
        use crate::schema_validation::SchemaValidator;
        use ocpp_types::v201::{ClearMessageStatusEnumType, CustomDataType, StatusInfoType};

        #[test]
        fn request_minimal_matches_wire_json_and_validates() {
            let req = ClearDisplayMessageRequest {
                id: 42,
                custom_data: None,
            };
            let expected = json!({ "id": 42 });
            assert_eq!(serde_json::to_value(&req).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call("ClearDisplayMessage", &expected)
                .is_ok());
            let back: ClearDisplayMessageRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn request_with_custom_data_round_trips_and_validates() {
            let req = ClearDisplayMessageRequest {
                id: 7,
                custom_data: Some(CustomDataType {
                    vendor_id: "com.example".to_string(),
                    extra: Default::default(),
                }),
            };
            let wire = serde_json::to_value(&req).unwrap();
            assert_eq!(wire["id"], json!(7));
            assert_eq!(wire["customData"]["vendorId"], json!("com.example"));
            assert!(SchemaValidator::v201()
                .validate_call("ClearDisplayMessage", &wire)
                .is_ok());
            let back: ClearDisplayMessageRequest = serde_json::from_value(wire).unwrap();
            assert_eq!(back, req);
        }

        #[test]
        fn response_minimal_matches_wire_json_and_validates() {
            let resp = ClearDisplayMessageResponse {
                status: ClearMessageStatusEnumType::Accepted,
                status_info: None,
                custom_data: None,
            };
            let expected = json!({ "status": "Accepted" });
            assert_eq!(serde_json::to_value(&resp).unwrap(), expected);
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearDisplayMessage", &expected)
                .is_ok());
            let back: ClearDisplayMessageResponse = serde_json::from_value(expected).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn response_with_status_info_round_trips_and_validates() {
            let resp = ClearDisplayMessageResponse {
                status: ClearMessageStatusEnumType::Unknown,
                status_info: Some(StatusInfoType {
                    reason_code: "NoSuchMessage".to_string(),
                    additional_info: Some("no display message with that id".to_string()),
                    custom_data: None,
                }),
                custom_data: None,
            };
            let wire = serde_json::to_value(&resp).unwrap();
            assert_eq!(wire["status"], json!("Unknown"));
            assert_eq!(wire["statusInfo"]["reasonCode"], json!("NoSuchMessage"));
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearDisplayMessage", &wire)
                .is_ok());
            let back: ClearDisplayMessageResponse = serde_json::from_value(wire).unwrap();
            assert_eq!(back, resp);
        }

        #[test]
        fn status_enum_serializes_to_wire_values_and_rejects_unknown() {
            assert_eq!(
                serde_json::to_value(ClearMessageStatusEnumType::Accepted).unwrap(),
                json!("Accepted")
            );
            assert_eq!(
                serde_json::to_value(ClearMessageStatusEnumType::Unknown).unwrap(),
                json!("Unknown")
            );
            // `Rejected` is not a `ClearMessageStatusEnumType` value.
            let bad = json!({ "status": "Rejected" });
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearDisplayMessage", &bad)
                .is_err());
            assert!(serde_json::from_value::<ClearDisplayMessageResponse>(bad).is_err());
        }

        #[test]
        fn action_names_are_stable() {
            assert_eq!(
                ClearDisplayMessageRequest::ACTION_NAME,
                "ClearDisplayMessage"
            );
            assert_eq!(
                ClearDisplayMessageResponse::ACTION_NAME,
                "ClearDisplayMessageResponse"
            );
        }

        #[test]
        fn schema_rejects_request_missing_id() {
            assert!(SchemaValidator::v201()
                .validate_call("ClearDisplayMessage", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_non_integer_id() {
            let bad = json!({ "id": "42" });
            assert!(SchemaValidator::v201()
                .validate_call("ClearDisplayMessage", &bad)
                .is_err());
            assert!(serde_json::from_value::<ClearDisplayMessageRequest>(bad).is_err());
        }

        #[test]
        fn schema_rejects_response_missing_status() {
            assert!(SchemaValidator::v201()
                .validate_call_result("ClearDisplayMessage", &json!({}))
                .is_err());
        }

        #[test]
        fn schema_rejects_additional_properties() {
            let v = SchemaValidator::v201();
            // On the request.
            assert!(v
                .validate_call("ClearDisplayMessage", &json!({ "id": 42, "bogus": true }))
                .is_err());
            // On the response.
            assert!(v
                .validate_call_result(
                    "ClearDisplayMessage",
                    &json!({ "status": "Accepted", "bogus": true })
                )
                .is_err());
        }
    }
}
