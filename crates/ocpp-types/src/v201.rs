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
//! `StatusNotification`) need. Subsequent 2.0.1 messages extend it.

use serde::{Deserialize, Serialize};

// =============================================================================
// Enumerations
// =============================================================================

/// Reason the Charging Station sends a `BootNotification` to the CSMS.
///
/// Ports `BootReasonEnumType` (`ocpp/v201/enums.py`). Wire values are
/// PascalCase, e.g. `"PowerUp"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootReasonEnumType {
    ApplicationReset,
    FirmwareUpdate,
    LocalReset,
    PowerUp,
    RemoteReset,
    ScheduledReset,
    Triggered,
    Unknown,
    Watchdog,
}

/// Result of a registration in response to a `BootNotification`.
///
/// Ports `RegistrationStatusEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistrationStatusEnumType {
    Accepted,
    Pending,
    Rejected,
}

/// Current status of a connector, reported in a `StatusNotification`.
///
/// Ports `ConnectorStatusEnumType` (`ocpp/v201/enums.py`). The 2.0.1 set is the
/// schema's five values — `Available`, `Occupied`, `Reserved`, `Unavailable`,
/// `Faulted` — a different (smaller) vocabulary than the 1.6J
/// `ChargePointStatus`. Wire values are PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorStatusEnumType {
    Available,
    Occupied,
    Reserved,
    Unavailable,
    Faulted,
}

/// Type of a `TransactionEvent` message.
///
/// Ports `TransactionEventEnumType` (`ocpp/v201/enums.py`). A transaction is a
/// sequence of one `Started`, zero or more `Updated`, and one `Ended` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionEventEnumType {
    Ended,
    Started,
    Updated,
}

/// Reason that triggered a `TransactionEvent`.
///
/// Ports `TriggerReasonEnumType` (`ocpp/v201/enums.py`). Wire values are
/// PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerReasonEnumType {
    Authorized,
    CablePluggedIn,
    ChargingRateChanged,
    ChargingStateChanged,
    Deauthorized,
    EnergyLimitReached,
    EVCommunicationLost,
    EVConnectTimeout,
    MeterValueClock,
    MeterValuePeriodic,
    TimeLimitReached,
    Trigger,
    UnlockCommand,
    StopAuthorized,
    EVDeparted,
    EVDetected,
    RemoteStop,
    RemoteStart,
    AbnormalCondition,
    SignedDataReceived,
    ResetCommand,
}

/// Current charging state of an EVSE during a transaction.
///
/// Ports `ChargingStateEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingStateEnumType {
    Charging,
    EVConnected,
    SuspendedEV,
    SuspendedEVSE,
    Idle,
}

/// Reason a transaction was stopped, reported on the `Ended` event.
///
/// Ports `ReasonEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasonEnumType {
    DeAuthorized,
    EmergencyStop,
    EnergyLimitReached,
    EVDisconnected,
    GroundFault,
    ImmediateReset,
    Local,
    LocalOutOfCredit,
    MasterPass,
    Other,
    OvercurrentFault,
    PowerLoss,
    PowerQuality,
    Reboot,
    Remote,
    SOCLimitReached,
    StoppedByEV,
    TimeLimitReached,
    Timeout,
}

/// Type of identifier used to authorize a charging session.
///
/// Ports `IdTokenEnumType` (`ocpp/v201/enums.py`). Three wire values are not
/// valid Rust identifiers in their spec form, so they carry explicit serde
/// renames (`eMAID`, `ISO14443`, `ISO15693`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdTokenEnumType {
    Central,
    #[serde(rename = "eMAID")]
    EMaid,
    #[serde(rename = "ISO14443")]
    Iso14443,
    #[serde(rename = "ISO15693")]
    Iso15693,
    KeyCode,
    Local,
    MacAddress,
    NoAuthorization,
}

/// Status of an identifier's authorization, returned in an `IdTokenInfoType`.
///
/// Ports `AuthorizationStatusEnumType` (`ocpp/v201/enums.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationStatusEnumType {
    Accepted,
    Blocked,
    ConcurrentTx,
    Expired,
    Invalid,
    NoCredit,
    #[serde(rename = "NotAllowedTypeEVSE")]
    NotAllowedTypeEvse,
    NotAtThisLocation,
    NotAtThisTime,
    Unknown,
}

/// Format of the `content` of a `MessageContentType`.
///
/// Ports `MessageFormatEnumType` (`ocpp/v201/enums.py`). All four wire values
/// are all-caps acronyms, so they carry explicit serde renames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageFormatEnumType {
    #[serde(rename = "ASCII")]
    Ascii,
    #[serde(rename = "HTML")]
    Html,
    #[serde(rename = "URI")]
    Uri,
    #[serde(rename = "UTF8")]
    Utf8,
}

// =============================================================================
// Datatypes
// =============================================================================

/// Open-ended vendor extension object carried by virtually every 2.0.1
/// message and datatype.
///
/// Ports `CustomDataType`. The schema requires `vendorId` and explicitly
/// permits arbitrary additional properties (it is the one type that does *not*
/// get `additionalProperties: false`), so extra keys are preserved verbatim
/// via `#[serde(flatten)]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomDataType {
    /// Vendor identification (max length 255 per schema).
    #[serde(rename = "vendorId")]
    pub vendor_id: String,
    /// Any additional vendor-specific properties.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// More information about the status returned in a response.
///
/// Ports `StatusInfoType`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusInfoType {
    /// Predefined, vendor-agnostic code describing the reason (max length 20).
    #[serde(rename = "reasonCode")]
    pub reason_code: String,
    /// Additional human-readable text (max length 512).
    #[serde(rename = "additionalInfo", skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Parameters of the wireless communication module of a Charging Station.
///
/// Ports `ModemType`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModemType {
    /// ICCID of the modem's SIM card (max length 20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iccid: Option<String>,
    /// IMSI of the modem's SIM card (max length 20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imsi: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// The physical system (Charging Station) where an EV can be charged.
///
/// Ports `ChargingStationType`. Only `model` and `vendorName` are required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingStationType {
    /// Vendor identification (not necessarily unique; max length 50).
    #[serde(rename = "vendorName")]
    pub vendor_name: String,
    /// Model of the Charging Station (max length 20).
    pub model: String,
    /// Vendor-specific device serial number (max length 25).
    #[serde(rename = "serialNumber", skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Firmware version running on the Charging Station (max length 50).
    #[serde(rename = "firmwareVersion", skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    /// Wireless-modem parameters, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modem: Option<ModemType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Electric Vehicle Supply Equipment — a single physical EVSE, optionally
/// narrowed to one of its connectors.
///
/// Ports `EVSEType`. Only `id` is required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvseType {
    /// EVSE identifier within the Charging Station (≥ 1).
    pub id: i32,
    /// Connector within the EVSE, if the message refers to a specific one.
    #[serde(rename = "connectorId", skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// One additional identifier carried alongside a primary [`IdTokenType`].
///
/// Ports `AdditionalInfoType`. Both fields are required by the schema; `type`
/// here is a free-form string (unlike [`IdTokenType::kind`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionalInfoType {
    /// The additional identifier (max length 36).
    #[serde(rename = "additionalIdToken")]
    pub additional_id_token: String,
    /// Type of the additional identifier (max length 50).
    #[serde(rename = "type")]
    pub kind: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A case-insensitive authorization identifier and its type.
///
/// Ports `IdTokenType`. Required fields are `idToken` and `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdTokenType {
    /// The identifier value (max length 36). May be empty for
    /// [`IdTokenEnumType::NoAuthorization`].
    #[serde(rename = "idToken")]
    pub id_token: String,
    /// How to interpret `id_token`.
    #[serde(rename = "type")]
    pub kind: IdTokenEnumType,
    /// Additional identifiers carried with this token.
    #[serde(rename = "additionalInfo", skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<Vec<AdditionalInfoType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// State of an ongoing or finished transaction.
///
/// Ports `TransactionType`. Only `transactionId` is required; the remaining
/// fields describe the charging state and (on the `Ended` event) the stop
/// reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionType {
    /// Unique identifier of the transaction (max length 36).
    #[serde(rename = "transactionId")]
    pub transaction_id: String,
    /// Current charging state.
    #[serde(rename = "chargingState", skip_serializing_if = "Option::is_none")]
    pub charging_state: Option<ChargingStateEnumType>,
    /// Cumulative seconds the EV has actually been charging (excludes pauses).
    #[serde(rename = "timeSpentCharging", skip_serializing_if = "Option::is_none")]
    pub time_spent_charging: Option<i32>,
    /// Why the transaction was stopped (present on the `Ended` event).
    #[serde(rename = "stoppedReason", skip_serializing_if = "Option::is_none")]
    pub stopped_reason: Option<ReasonEnumType>,
    /// `RequestStartTransaction` id that started this transaction remotely.
    #[serde(rename = "remoteStartId", skip_serializing_if = "Option::is_none")]
    pub remote_start_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Message to be displayed on a Charging Station.
///
/// Ports `MessageContentType`. Required fields are `format` and `content`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageContentType {
    /// Format of `content`.
    pub format: MessageFormatEnumType,
    /// The message text (max length 512).
    pub content: String,
    /// Message language as an RFC 5646 tag (max length 8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Status information about an authorization identifier.
///
/// Ports `IdTokenInfoType`. Only `status` is required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdTokenInfoType {
    /// Authorization status of the identifier.
    pub status: AuthorizationStatusEnumType,
    /// When the cached authorization expires (RFC 3339 / ISO 8601).
    #[serde(
        rename = "cacheExpiryDateTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_expiry_date_time: Option<String>,
    /// Priority of this identifier relative to others (-9..=9).
    #[serde(rename = "chargingPriority", skip_serializing_if = "Option::is_none")]
    pub charging_priority: Option<i32>,
    /// Preferred user-interface language (RFC 5646 tag, max length 8).
    #[serde(rename = "language1", skip_serializing_if = "Option::is_none")]
    pub language1: Option<String>,
    /// EVSEs this identifier is allowed to charge at; empty/absent means all.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<Vec<i32>>,
    /// Group this identifier belongs to (for concurrent-tx checks).
    #[serde(rename = "groupIdToken", skip_serializing_if = "Option::is_none")]
    pub group_id_token: Option<IdTokenType>,
    /// Second-preference user-interface language (max length 8).
    #[serde(rename = "language2", skip_serializing_if = "Option::is_none")]
    pub language2: Option<String>,
    /// Personal message to display for this identifier.
    #[serde(rename = "personalMessage", skip_serializing_if = "Option::is_none")]
    pub personal_message: Option<MessageContentType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

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
    fn id_token_enum_renames_non_identifier_values() {
        // The three values that are not valid Rust identifiers carry renames.
        assert_eq!(
            serde_json::to_value(IdTokenEnumType::EMaid).unwrap(),
            json!("eMAID")
        );
        assert_eq!(
            serde_json::to_value(IdTokenEnumType::Iso14443).unwrap(),
            json!("ISO14443")
        );
        let back: IdTokenEnumType = serde_json::from_value(json!("ISO15693")).unwrap();
        assert_eq!(back, IdTokenEnumType::Iso15693);
        // A plain value still round-trips.
        assert_eq!(
            serde_json::to_value(IdTokenEnumType::Central).unwrap(),
            json!("Central")
        );
    }

    #[test]
    fn message_format_enum_renames_all_caps_values() {
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
    fn id_token_round_trips_with_additional_info() {
        let token = IdTokenType {
            id_token: "045918E24B5380".to_string(),
            kind: IdTokenEnumType::Iso14443,
            additional_info: Some(vec![AdditionalInfoType {
                additional_id_token: "VID:0815".to_string(),
                kind: "vendorId".to_string(),
                custom_data: None,
            }]),
            custom_data: None,
        };
        let wire = serde_json::to_value(&token).unwrap();
        assert_eq!(wire["type"], json!("ISO14443"));
        assert_eq!(wire["additionalInfo"][0]["type"], json!("vendorId"));
        let back: IdTokenType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn evse_omits_none_connector() {
        let evse = EvseType {
            id: 1,
            connector_id: None,
            custom_data: None,
        };
        assert_eq!(serde_json::to_value(&evse).unwrap(), json!({ "id": 1 }));
    }

    #[test]
    fn id_token_info_round_trips_with_personal_message() {
        let info = IdTokenInfoType {
            status: AuthorizationStatusEnumType::Accepted,
            cache_expiry_date_time: None,
            charging_priority: None,
            language1: Some("en".to_string()),
            evse_id: Some(vec![1, 2]),
            group_id_token: None,
            language2: None,
            personal_message: Some(MessageContentType {
                format: MessageFormatEnumType::Utf8,
                content: "Welcome".to_string(),
                language: Some("en".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&info).unwrap();
        assert_eq!(wire["status"], json!("Accepted"));
        assert_eq!(wire["personalMessage"]["format"], json!("UTF8"));
        let back: IdTokenInfoType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, info);
    }
}
