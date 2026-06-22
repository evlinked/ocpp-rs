//! OCPP 2.0.1 shared datatypes and enumerations.
//!
//! This module ports the enums and `*Type` datatypes from the OCPP 2.0.1
//! specification (mobilityhouse/ocpp `ocpp/v201/enums.py` and
//! `ocpp/v201/datatypes.py`), mirroring the conventions of [`crate::v16j`]:
//! serde with explicit camelCase renames and `skip_serializing_if` on every
//! optional field so absent values never appear on the wire.
//!
//! It is the foundation slice for **M7 — OCPP 2.0.1**; today it carries what
//! the core lifecycle messages (`BootNotification`, `Heartbeat`,
//! `StatusNotification`, `Authorize`) need. Subsequent 2.0.1 messages extend it.
//!
//! The definitions are split by *kind* — `enums` for the `*EnumType`s and
//! `datatypes` for the shared struct datatypes — and re-exported here so the
//! public path stays `ocpp_types::v201::*`. New messages add enum variants and
//! datatypes to the two well-separated files instead of one monolith.

mod datatypes;
mod enums;

pub use datatypes::*;
pub use enums::*;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn boot_reason_serializes_pascal_case() {
        assert_eq!(
            serde_json::to_value(BootReasonEnumType::PowerUp).unwrap(),
            json!("PowerUp")
        );
        assert_eq!(
            serde_json::to_value(BootReasonEnumType::ApplicationReset).unwrap(),
            json!("ApplicationReset")
        );
        let parsed: BootReasonEnumType = serde_json::from_value(json!("Watchdog")).unwrap();
        assert_eq!(parsed, BootReasonEnumType::Watchdog);
    }

    #[test]
    fn registration_status_round_trips() {
        for (variant, wire) in [
            (RegistrationStatusEnumType::Accepted, "Accepted"),
            (RegistrationStatusEnumType::Pending, "Pending"),
            (RegistrationStatusEnumType::Rejected, "Rejected"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: RegistrationStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn unknown_enum_value_is_rejected() {
        let err = serde_json::from_value::<RegistrationStatusEnumType>(json!("Bogus"));
        assert!(err.is_err());
    }

    #[test]
    fn connector_status_serializes_pascal_case() {
        for (variant, wire) in [
            (ConnectorStatusEnumType::Available, "Available"),
            (ConnectorStatusEnumType::Occupied, "Occupied"),
            (ConnectorStatusEnumType::Reserved, "Reserved"),
            (ConnectorStatusEnumType::Unavailable, "Unavailable"),
            (ConnectorStatusEnumType::Faulted, "Faulted"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: ConnectorStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // The 1.6J-only states are not part of the 2.0.1 vocabulary.
        assert!(serde_json::from_value::<ConnectorStatusEnumType>(json!("Charging")).is_err());
    }

    #[test]
    fn charging_station_omits_none_optionals() {
        let cs = ChargingStationType {
            vendor_name: "ICU Eve Mini".to_string(),
            model: "ICU Eve Mini".to_string(),
            serial_number: None,
            firmware_version: Some("#1:3.4.0-2990#N:217H;1.0-223".to_string()),
            modem: None,
            custom_data: None,
        };
        // Matches the Python reference fixture (tests/v201/test_v201_charge_point.py):
        // only the three present fields, in camelCase, no nulls.
        assert_eq!(
            serde_json::to_value(&cs).unwrap(),
            json!({
                "vendorName": "ICU Eve Mini",
                "model": "ICU Eve Mini",
                "firmwareVersion": "#1:3.4.0-2990#N:217H;1.0-223"
            })
        );
    }

    #[test]
    fn modem_round_trips_through_charging_station() {
        let cs = ChargingStationType {
            vendor_name: "Vendor".to_string(),
            model: "Model".to_string(),
            serial_number: Some("SN-1".to_string()),
            firmware_version: None,
            modem: Some(ModemType {
                iccid: Some("89000000".to_string()),
                imsi: Some("26201".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&cs).unwrap();
        assert_eq!(wire["modem"]["iccid"], json!("89000000"));
        let back: ChargingStationType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, cs);
    }

    #[test]
    fn custom_data_preserves_extra_properties() {
        let value = json!({ "vendorId": "com.example", "foo": 1, "bar": ["a", "b"] });
        let cd: CustomDataType = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(cd.vendor_id, "com.example");
        assert_eq!(cd.extra.get("foo"), Some(&json!(1)));
        // Round-trips back to the same object, extras intact.
        assert_eq!(serde_json::to_value(&cd).unwrap(), value);
    }

    #[test]
    fn status_info_omits_none_optionals() {
        let si = StatusInfoType {
            reason_code: "Booted".to_string(),
            additional_info: None,
            custom_data: None,
        };
        assert_eq!(
            serde_json::to_value(&si).unwrap(),
            json!({ "reasonCode": "Booted" })
        );
    }

    #[test]
    fn attribute_enum_serializes_pascal_case() {
        for (variant, wire) in [
            (AttributeEnumType::Actual, "Actual"),
            (AttributeEnumType::Target, "Target"),
            (AttributeEnumType::MinSet, "MinSet"),
            (AttributeEnumType::MaxSet, "MaxSet"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: AttributeEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn id_token_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (IdTokenEnumType::Central, "Central"),
            (IdTokenEnumType::EMaid, "eMAID"),
            (IdTokenEnumType::Iso14443, "ISO14443"),
            (IdTokenEnumType::Iso15693, "ISO15693"),
            (IdTokenEnumType::KeyCode, "KeyCode"),
            (IdTokenEnumType::Local, "Local"),
            (IdTokenEnumType::MacAddress, "MacAddress"),
            (IdTokenEnumType::NoAuthorization, "NoAuthorization"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: IdTokenEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // Unknown / mis-cased values are rejected.
        assert!(serde_json::from_value::<IdTokenEnumType>(json!("emaid")).is_err());
        assert!(serde_json::from_value::<IdTokenEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn authorization_status_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (AuthorizationStatusEnumType::Accepted, "Accepted"),
            (AuthorizationStatusEnumType::Blocked, "Blocked"),
            (AuthorizationStatusEnumType::ConcurrentTx, "ConcurrentTx"),
            (AuthorizationStatusEnumType::Expired, "Expired"),
            (AuthorizationStatusEnumType::Invalid, "Invalid"),
            (AuthorizationStatusEnumType::NoCredit, "NoCredit"),
            (
                AuthorizationStatusEnumType::NotAllowedTypeEvse,
                "NotAllowedTypeEVSE",
            ),
            (
                AuthorizationStatusEnumType::NotAtThisLocation,
                "NotAtThisLocation",
            ),
            (AuthorizationStatusEnumType::NotAtThisTime, "NotAtThisTime"),
            (AuthorizationStatusEnumType::Unknown, "Unknown"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: AuthorizationStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        assert!(serde_json::from_value::<AuthorizationStatusEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn message_format_enum_round_trips() {
        for (variant, wire) in [
            (MessageFormatEnumType::Ascii, "ASCII"),
            (MessageFormatEnumType::Html, "HTML"),
            (MessageFormatEnumType::Uri, "URI"),
            (MessageFormatEnumType::Utf8, "UTF8"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: MessageFormatEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn get_variable_status_serializes_pascal_case() {
        for (variant, wire) in [
            (GetVariableStatusEnumType::Accepted, "Accepted"),
            (GetVariableStatusEnumType::Rejected, "Rejected"),
            (
                GetVariableStatusEnumType::UnknownComponent,
                "UnknownComponent",
            ),
            (
                GetVariableStatusEnumType::UnknownVariable,
                "UnknownVariable",
            ),
            (
                GetVariableStatusEnumType::NotSupportedAttributeType,
                "NotSupportedAttributeType",
            ),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: GetVariableStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn enums_reject_unknown_wire_values() {
        assert!(serde_json::from_value::<AttributeEnumType>(json!("Bogus")).is_err());
        assert!(serde_json::from_value::<GetVariableStatusEnumType>(json!("Nope")).is_err());
    }

    #[test]
    fn change_availability_enums_serialize_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(OperationalStatusEnumType::Inoperative).unwrap(),
            json!("Inoperative")
        );
        assert_eq!(
            serde_json::to_value(OperationalStatusEnumType::Operative).unwrap(),
            json!("Operative")
        );
        for (variant, wire) in [
            (ChangeAvailabilityStatusEnumType::Accepted, "Accepted"),
            (ChangeAvailabilityStatusEnumType::Rejected, "Rejected"),
            (ChangeAvailabilityStatusEnumType::Scheduled, "Scheduled"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<ChangeAvailabilityStatusEnumType>(json!(wire)).unwrap(),
                variant
            );
        }
        // The two enums share no overlap on the wire: `Scheduled` is response-only.
        assert!(serde_json::from_value::<OperationalStatusEnumType>(json!("Scheduled")).is_err());
    }

    #[test]
    fn trigger_message_enums_serialize_exact_wire_values() {
        // The full trigger set round-trips to its exact PascalCase wire value.
        for (variant, wire) in [
            (MessageTriggerEnumType::BootNotification, "BootNotification"),
            (
                MessageTriggerEnumType::LogStatusNotification,
                "LogStatusNotification",
            ),
            (
                MessageTriggerEnumType::FirmwareStatusNotification,
                "FirmwareStatusNotification",
            ),
            (MessageTriggerEnumType::Heartbeat, "Heartbeat"),
            (MessageTriggerEnumType::MeterValues, "MeterValues"),
            (
                MessageTriggerEnumType::SignChargingStationCertificate,
                "SignChargingStationCertificate",
            ),
            (
                MessageTriggerEnumType::SignV2GCertificate,
                "SignV2GCertificate",
            ),
            (
                MessageTriggerEnumType::StatusNotification,
                "StatusNotification",
            ),
            (MessageTriggerEnumType::TransactionEvent, "TransactionEvent"),
            (
                MessageTriggerEnumType::SignCombinedCertificate,
                "SignCombinedCertificate",
            ),
            (
                MessageTriggerEnumType::PublishFirmwareStatusNotification,
                "PublishFirmwareStatusNotification",
            ),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<MessageTriggerEnumType>(json!(wire)).unwrap(),
                variant
            );
        }
        for (variant, wire) in [
            (TriggerMessageStatusEnumType::Accepted, "Accepted"),
            (TriggerMessageStatusEnumType::Rejected, "Rejected"),
            (
                TriggerMessageStatusEnumType::NotImplemented,
                "NotImplemented",
            ),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<TriggerMessageStatusEnumType>(json!(wire)).unwrap(),
                variant
            );
        }
        // `Resumed` is a TransactionEvent trigger reason, not a message trigger.
        assert!(serde_json::from_value::<MessageTriggerEnumType>(json!("Resumed")).is_err());
        // `NotImplemented` is response-only; not a valid requestedMessage.
        assert!(serde_json::from_value::<MessageTriggerEnumType>(json!("NotImplemented")).is_err());
    }

    #[test]
    fn component_omits_none_optionals() {
        let c = ComponentType {
            name: "EVSE".to_string(),
            instance: None,
            evse: None,
            custom_data: None,
        };
        assert_eq!(serde_json::to_value(&c).unwrap(), json!({ "name": "EVSE" }));
    }

    #[test]
    fn evse_round_trips_with_connector() {
        let evse = EvseType {
            id: 1,
            connector_id: Some(2),
            custom_data: None,
        };
        let wire = serde_json::to_value(&evse).unwrap();
        assert_eq!(wire, json!({ "id": 1, "connectorId": 2 }));
        let back: EvseType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, evse);
    }

    #[test]
    fn get_variable_data_defaults_attribute_type_to_absent() {
        let data = GetVariableDataType {
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
        };
        let expected = json!({
            "component": { "name": "SampledDataCtrlr", "evse": { "id": 1 } },
            "variable": { "name": "TxEndedMeasurands" }
        });
        assert_eq!(serde_json::to_value(&data).unwrap(), expected);
        let back: GetVariableDataType = serde_json::from_value(expected).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn get_variable_result_round_trips() {
        let result = GetVariableResultType {
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
        };
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["attributeStatus"], json!("Accepted"));
        assert_eq!(wire["attributeValue"], json!("300"));
        assert_eq!(wire["attributeType"], json!("Actual"));
        let back: GetVariableResultType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, result);
    }

    #[test]
    fn id_token_minimal_matches_wire_json() {
        // Reference: tests/v201/conftest.py — a bare RFID token.
        let token = IdTokenType {
            id_token: "045918E24B6D80".to_string(),
            kind: IdTokenEnumType::Iso14443,
            additional_info: None,
            custom_data: None,
        };
        let expected = json!({
            "idToken": "045918E24B6D80",
            "type": "ISO14443"
        });
        assert_eq!(serde_json::to_value(&token).unwrap(), expected);
        let back: IdTokenType = serde_json::from_value(expected).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn id_token_with_additional_info_round_trips() {
        let token = IdTokenType {
            id_token: "primary".to_string(),
            kind: IdTokenEnumType::Central,
            additional_info: Some(vec![AdditionalInfoType {
                additional_id_token: "linked".to_string(),
                kind: "VendorScheme".to_string(),
                custom_data: None,
            }]),
            custom_data: None,
        };
        let wire = serde_json::to_value(&token).unwrap();
        assert_eq!(
            wire["additionalInfo"][0]["additionalIdToken"],
            json!("linked")
        );
        assert_eq!(wire["additionalInfo"][0]["type"], json!("VendorScheme"));
        let back: IdTokenType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn id_token_info_minimal_matches_wire_json() {
        let info = IdTokenInfoType {
            status: AuthorizationStatusEnumType::Accepted,
            cache_expiry_date_time: None,
            charging_priority: None,
            language1: None,
            evse_id: None,
            language2: None,
            group_id_token: None,
            personal_message: None,
            custom_data: None,
        };
        // Only the required `status` field appears — no nulls.
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            json!({ "status": "Accepted" })
        );
    }

    #[test]
    fn id_token_info_full_round_trips_with_nested_objects() {
        let info = IdTokenInfoType {
            status: AuthorizationStatusEnumType::Accepted,
            cache_expiry_date_time: Some("2030-01-01T00:00:00Z".to_string()),
            charging_priority: Some(5),
            language1: Some("en".to_string()),
            evse_id: Some(vec![1, 2]),
            language2: Some("nl".to_string()),
            group_id_token: Some(IdTokenType {
                id_token: "group-1".to_string(),
                kind: IdTokenEnumType::Central,
                additional_info: None,
                custom_data: None,
            }),
            personal_message: Some(MessageContentType {
                format: MessageFormatEnumType::Utf8,
                content: "Welcome".to_string(),
                language: Some("en".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&info).unwrap();
        assert_eq!(wire["chargingPriority"], json!(5));
        assert_eq!(wire["groupIdToken"]["idToken"], json!("group-1"));
        assert_eq!(wire["personalMessage"]["format"], json!("UTF8"));
        assert_eq!(wire["evseId"], json!([1, 2]));
        let back: IdTokenInfoType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, info);
    }

    #[test]
    fn authorization_data_omits_id_token_info_when_absent() {
        // A removal entry in a differential SendLocalList update: only the
        // required `idToken` appears — no `idTokenInfo`, no nulls.
        let entry = AuthorizationData {
            id_token: IdTokenType {
                id_token: "045918E24B6D80".to_string(),
                kind: IdTokenEnumType::Iso14443,
                additional_info: None,
                custom_data: None,
            },
            id_token_info: None,
            custom_data: None,
        };
        let expected = json!({
            "idToken": { "idToken": "045918E24B6D80", "type": "ISO14443" }
        });
        assert_eq!(serde_json::to_value(&entry).unwrap(), expected);
        let back: AuthorizationData = serde_json::from_value(expected).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn authorization_data_round_trips_with_id_token_info() {
        let entry = AuthorizationData {
            id_token: IdTokenType {
                id_token: "group-1".to_string(),
                kind: IdTokenEnumType::Central,
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
        };
        let wire = serde_json::to_value(&entry).unwrap();
        assert_eq!(wire["idTokenInfo"]["status"], json!("Accepted"));
        let back: AuthorizationData = serde_json::from_value(wire).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn send_local_list_enums_serialize_exact_wire_values() {
        for (variant, wire) in [
            (UpdateEnumType::Differential, "Differential"),
            (UpdateEnumType::Full, "Full"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<UpdateEnumType>(json!(wire)).unwrap(),
                variant
            );
        }
        for (variant, wire) in [
            (SendLocalListStatusEnumType::Accepted, "Accepted"),
            (SendLocalListStatusEnumType::Failed, "Failed"),
            (
                SendLocalListStatusEnumType::VersionMismatch,
                "VersionMismatch",
            ),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<SendLocalListStatusEnumType>(json!(wire)).unwrap(),
                variant
            );
        }
        assert!(serde_json::from_value::<UpdateEnumType>(json!("Partial")).is_err());
        assert!(serde_json::from_value::<SendLocalListStatusEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn transaction_event_enums_serialize_pascal_case() {
        assert_eq!(
            serde_json::to_value(TransactionEventEnumType::Started).unwrap(),
            json!("Started")
        );
        assert_eq!(
            serde_json::to_value(TriggerReasonEnumType::EVCommunicationLost).unwrap(),
            json!("EVCommunicationLost")
        );
        assert_eq!(
            serde_json::to_value(ChargingStateEnumType::SuspendedEVSE).unwrap(),
            json!("SuspendedEVSE")
        );
        assert_eq!(
            serde_json::to_value(ReasonEnumType::SOCLimitReached).unwrap(),
            json!("SOCLimitReached")
        );
    }

    #[test]
    fn transaction_type_omits_none_optionals() {
        let tx = TransactionType {
            transaction_id: "tx-001".to_string(),
            charging_state: None,
            time_spent_charging: None,
            stopped_reason: None,
            remote_start_id: None,
            custom_data: None,
        };
        assert_eq!(
            serde_json::to_value(&tx).unwrap(),
            json!({ "transactionId": "tx-001" })
        );
    }

    #[test]
    fn reset_enum_round_trips() {
        for (variant, wire) in [
            (ResetEnumType::Immediate, "Immediate"),
            (ResetEnumType::OnIdle, "OnIdle"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: ResetEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // The 1.6J vocabulary (`Hard`/`Soft`) is not part of the 2.0.1 enum.
        assert!(serde_json::from_value::<ResetEnumType>(json!("Hard")).is_err());
    }

    #[test]
    fn reset_status_enum_round_trips() {
        for (variant, wire) in [
            (ResetStatusEnumType::Accepted, "Accepted"),
            (ResetStatusEnumType::Rejected, "Rejected"),
            (ResetStatusEnumType::Scheduled, "Scheduled"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: ResetStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        assert!(serde_json::from_value::<ResetStatusEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn set_variable_status_serializes_pascal_case() {
        for (variant, wire) in [
            (SetVariableStatusEnumType::Accepted, "Accepted"),
            (SetVariableStatusEnumType::Rejected, "Rejected"),
            (
                SetVariableStatusEnumType::UnknownComponent,
                "UnknownComponent",
            ),
            (
                SetVariableStatusEnumType::UnknownVariable,
                "UnknownVariable",
            ),
            (
                SetVariableStatusEnumType::NotSupportedAttributeType,
                "NotSupportedAttributeType",
            ),
            (SetVariableStatusEnumType::RebootRequired, "RebootRequired"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: SetVariableStatusEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // `RebootRequired` is write-path only; the read-path status enum never
        // carries it, and unknown values are rejected.
        assert!(serde_json::from_value::<SetVariableStatusEnumType>(json!("Bogus")).is_err());
        assert!(
            serde_json::from_value::<GetVariableStatusEnumType>(json!("RebootRequired")).is_err()
        );
    }

    #[test]
    fn set_variable_data_omits_none_optionals() {
        let data = SetVariableDataType {
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
        };
        let expected = json!({
            "attributeValue": "300",
            "component": { "name": "OCPPCommCtrlr" },
            "variable": { "name": "HeartbeatInterval" }
        });
        assert_eq!(serde_json::to_value(&data).unwrap(), expected);
        let back: SetVariableDataType = serde_json::from_value(expected).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn set_variable_result_round_trips_with_status_info() {
        let result = SetVariableResultType {
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
        };
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["attributeStatus"], json!("RebootRequired"));
        assert_eq!(wire["attributeType"], json!("Actual"));
        assert_eq!(wire["attributeStatusInfo"]["reasonCode"], json!("Queued"));
        // The result echoes no value back, unlike its read-path counterpart.
        assert!(wire.get("attributeValue").is_none());
        let back: SetVariableResultType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, result);
    }

    #[test]
    fn reading_context_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (
                ReadingContextEnumType::InterruptionBegin,
                "Interruption.Begin",
            ),
            (ReadingContextEnumType::InterruptionEnd, "Interruption.End"),
            (ReadingContextEnumType::Other, "Other"),
            (ReadingContextEnumType::SampleClock, "Sample.Clock"),
            (ReadingContextEnumType::SamplePeriodic, "Sample.Periodic"),
            (
                ReadingContextEnumType::TransactionBegin,
                "Transaction.Begin",
            ),
            (ReadingContextEnumType::TransactionEnd, "Transaction.End"),
            (ReadingContextEnumType::Trigger, "Trigger"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: ReadingContextEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // The dotted spelling is significant — the bare segment is not a value.
        assert!(serde_json::from_value::<ReadingContextEnumType>(json!("Periodic")).is_err());
        assert!(serde_json::from_value::<ReadingContextEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn measurand_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (MeasurandEnumType::CurrentExport, "Current.Export"),
            (MeasurandEnumType::CurrentImport, "Current.Import"),
            (
                MeasurandEnumType::EnergyActiveImportRegister,
                "Energy.Active.Import.Register",
            ),
            (MeasurandEnumType::EnergyApparentNet, "Energy.Apparent.Net"),
            (MeasurandEnumType::Frequency, "Frequency"),
            (MeasurandEnumType::PowerActiveImport, "Power.Active.Import"),
            (MeasurandEnumType::PowerFactor, "Power.Factor"),
            (MeasurandEnumType::SoC, "SoC"),
            (MeasurandEnumType::Voltage, "Voltage"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: MeasurandEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // Case matters: `SoC` is the only correct spelling.
        assert!(serde_json::from_value::<MeasurandEnumType>(json!("Soc")).is_err());
        assert!(serde_json::from_value::<MeasurandEnumType>(json!("Bogus")).is_err());
    }

    #[test]
    fn phase_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (PhaseEnumType::L1, "L1"),
            (PhaseEnumType::L2, "L2"),
            (PhaseEnumType::L3, "L3"),
            (PhaseEnumType::N, "N"),
            (PhaseEnumType::L1N, "L1-N"),
            (PhaseEnumType::L2N, "L2-N"),
            (PhaseEnumType::L3N, "L3-N"),
            (PhaseEnumType::L1L2, "L1-L2"),
            (PhaseEnumType::L2L3, "L2-L3"),
            (PhaseEnumType::L3L1, "L3-L1"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: PhaseEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        assert!(serde_json::from_value::<PhaseEnumType>(json!("L1N")).is_err());
    }

    #[test]
    fn location_enum_serializes_exact_wire_values() {
        for (variant, wire) in [
            (LocationEnumType::Body, "Body"),
            (LocationEnumType::Cable, "Cable"),
            (LocationEnumType::Ev, "EV"),
            (LocationEnumType::Inlet, "Inlet"),
            (LocationEnumType::Outlet, "Outlet"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: LocationEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // `EV` is upper-case on the wire; the Rust spelling is not accepted.
        assert!(serde_json::from_value::<LocationEnumType>(json!("Ev")).is_err());
    }

    #[test]
    fn sampled_value_minimal_is_just_value() {
        // With every optional field absent, only `value` appears — the spec's
        // "active import energy in Wh" default reading.
        let sv = SampledValueType {
            value: 1234.5,
            context: None,
            measurand: None,
            phase: None,
            location: None,
            signed_meter_value: None,
            unit_of_measure: None,
            custom_data: None,
        };
        assert_eq!(
            serde_json::to_value(&sv).unwrap(),
            json!({ "value": 1234.5 })
        );
        let back: SampledValueType = serde_json::from_value(json!({ "value": 1234.5 })).unwrap();
        assert_eq!(back, sv);
    }

    #[test]
    fn sampled_value_full_round_trips_with_nested_objects() {
        let sv = SampledValueType {
            value: 230.0,
            context: Some(ReadingContextEnumType::TransactionEnd),
            measurand: Some(MeasurandEnumType::Voltage),
            phase: Some(PhaseEnumType::L2N),
            location: Some(LocationEnumType::Ev),
            signed_meter_value: Some(SignedMeterValueType {
                signed_meter_data: "c2lnbmVk".to_string(),
                signing_method: "ECDSA".to_string(),
                encoding_method: "DLMS Message".to_string(),
                public_key: "cHVibGlj".to_string(),
                custom_data: None,
            }),
            unit_of_measure: Some(UnitOfMeasureType {
                unit: Some("V".to_string()),
                multiplier: Some(0),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&sv).unwrap();
        assert_eq!(wire["context"], json!("Transaction.End"));
        assert_eq!(wire["phase"], json!("L2-N"));
        assert_eq!(wire["location"], json!("EV"));
        assert_eq!(wire["signedMeterValue"]["signingMethod"], json!("ECDSA"));
        assert_eq!(wire["unitOfMeasure"]["multiplier"], json!(0));
        let back: SampledValueType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, sv);
    }

    #[test]
    fn meter_value_matches_wire_json() {
        let mv = MeterValueType {
            timestamp: "2022-01-01T10:05:00Z".to_string(),
            sampled_value: vec![SampledValueType {
                value: 1234.5,
                context: None,
                measurand: Some(MeasurandEnumType::EnergyActiveImportRegister),
                phase: None,
                location: None,
                signed_meter_value: None,
                unit_of_measure: None,
                custom_data: None,
            }],
            custom_data: None,
        };
        let expected = json!({
            "timestamp": "2022-01-01T10:05:00Z",
            "sampledValue": [{
                "value": 1234.5,
                "measurand": "Energy.Active.Import.Register"
            }]
        });
        assert_eq!(serde_json::to_value(&mv).unwrap(), expected);
        let back: MeterValueType = serde_json::from_value(expected).unwrap();
        assert_eq!(back, mv);
    }

    #[test]
    fn unit_of_measure_omits_none_optionals() {
        let uom = UnitOfMeasureType {
            unit: None,
            multiplier: None,
            custom_data: None,
        };
        // Both fields default on the wire, so an all-absent unit serializes to
        // an empty object.
        assert_eq!(serde_json::to_value(&uom).unwrap(), json!({}));
    }

    #[test]
    fn charging_profile_enums_serialize_to_exact_wire_values() {
        for (value, wire) in [
            (
                ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
                "ChargingStationExternalConstraints",
            ),
            (
                ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
                "ChargingStationMaxProfile",
            ),
            (
                ChargingProfilePurposeEnumType::TxDefaultProfile,
                "TxDefaultProfile",
            ),
            (ChargingProfilePurposeEnumType::TxProfile, "TxProfile"),
        ] {
            assert_eq!(serde_json::to_value(value).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<ChargingProfilePurposeEnumType>(json!(wire)).unwrap(),
                value
            );
        }
        assert_eq!(
            serde_json::to_value(ChargingProfileKindEnumType::Recurring).unwrap(),
            json!("Recurring")
        );
        assert_eq!(
            serde_json::to_value(RecurrencyKindEnumType::Weekly).unwrap(),
            json!("Weekly")
        );
        // Single-letter rate units serialize verbatim.
        assert_eq!(
            serde_json::to_value(ChargingRateUnitEnumType::W).unwrap(),
            json!("W")
        );
        assert_eq!(
            serde_json::to_value(ChargingRateUnitEnumType::A).unwrap(),
            json!("A")
        );
        assert_eq!(
            serde_json::to_value(CostKindEnumType::RenewableGenerationPercentage).unwrap(),
            json!("RenewableGenerationPercentage")
        );
    }

    #[test]
    fn charging_profile_enums_reject_unknown_values() {
        assert!(serde_json::from_value::<ChargingProfilePurposeEnumType>(json!("Bogus")).is_err());
        assert!(serde_json::from_value::<ChargingProfileKindEnumType>(json!("Sometimes")).is_err());
        assert!(serde_json::from_value::<RecurrencyKindEnumType>(json!("Monthly")).is_err());
        assert!(serde_json::from_value::<ChargingRateUnitEnumType>(json!("kW")).is_err());
        assert!(serde_json::from_value::<CostKindEnumType>(json!("Free")).is_err());
    }

    #[test]
    fn charging_schedule_period_omits_none_optionals() {
        let period = ChargingSchedulePeriodType {
            start_period: 0,
            limit: 32.0,
            number_phases: None,
            phase_to_use: None,
            custom_data: None,
        };
        // Only the two required fields appear on the wire.
        assert_eq!(
            serde_json::to_value(&period).unwrap(),
            json!({ "startPeriod": 0, "limit": 32.0 })
        );
    }

    #[test]
    fn charging_profile_full_tree_round_trips() {
        // A recurring profile whose schedule carries a full sales-tariff tree,
        // exercising every nested datatype and optional field.
        let profile = ChargingProfileType {
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
        };
        let wire = serde_json::to_value(&profile).unwrap();
        // Spot-check the deeply-nested cost made it onto the wire with renames.
        assert_eq!(
            wire["chargingSchedule"][0]["salesTariff"]["salesTariffEntry"][0]["consumptionCost"][0]
                ["cost"][0]["costKind"],
            json!("RelativePricePercentage")
        );
        // transactionId is None, so it must be absent.
        assert!(!wire.as_object().unwrap().contains_key("transactionId"));
        let back: ChargingProfileType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, profile);
    }
}
