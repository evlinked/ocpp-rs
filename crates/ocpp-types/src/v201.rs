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

/// The kind of identifier carried by an [`IdTokenType`].
///
/// Ports `IdTokenEnumType` (`ocpp/v201/enums.py`). Wire values are *not*
/// uniformly PascalCase — `eMAID` is camelCase and `ISO14443`/`ISO15693` are
/// all-caps — so each variant maps to its exact spec spelling rather than a
/// blanket `rename_all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdTokenEnumType {
    Central,
    #[serde(rename = "eMAID")]
    EMAID,
    ISO14443,
    ISO15693,
    KeyCode,
    Local,
    MacAddress,
    NoAuthorization,
}

/// Current authorization status of an idToken, as decided by the CSMS.
///
/// Ports `AuthorizationStatusEnumType` (`ocpp/v201/enums.py`). The 2.0.1 set is
/// considerably richer than the 1.6J `AuthorizationStatus`. Wire values are
/// PascalCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationStatusEnumType {
    Accepted,
    Blocked,
    ConcurrentTx,
    Expired,
    Invalid,
    NoCredit,
    NotAllowedTypeEVSE,
    NotAtThisLocation,
    NotAtThisTime,
    Unknown,
}

/// Format of a message to be displayed on a Charging Station.
///
/// Ports `MessageFormatEnumType` (`ocpp/v201/enums.py`). Wire values are the
/// exact all-caps tokens `ASCII`, `HTML`, `URI`, `UTF8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageFormatEnumType {
    ASCII,
    HTML,
    URI,
    UTF8,
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

/// An additional identifier nested inside an [`IdTokenType`], supporting
/// multiple forms of identifier for a single authorization.
///
/// Ports `AdditionalInfoType`. Both `additionalIdToken` and `type` are
/// required by the schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionalInfoType {
    /// The additional IdToken (max length 36).
    #[serde(rename = "additionalIdToken")]
    pub additional_id_token: String,
    /// The type of `additionalIdToken`; a custom, agreed-upon string (max
    /// length 50). Renamed from the reserved word `type`.
    #[serde(rename = "type")]
    pub additional_token_type: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// A case-insensitive identifier used for authorization, together with the
/// kind of identifier it is.
///
/// Ports `IdTokenType`. The 2.0.1 replacement for the 1.6J bare `idTag`
/// string: a `{ idToken, type }` pair, optionally carrying nested
/// [`AdditionalInfoType`] entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdTokenType {
    /// The identifier itself; case-insensitive (max length 36).
    #[serde(rename = "idToken")]
    pub id_token: String,
    /// The kind of identifier. Renamed from the reserved word `type`.
    #[serde(rename = "type")]
    pub id_token_type: IdTokenEnumType,
    /// Optional additional identifiers (schema requires at least one entry when
    /// present).
    #[serde(rename = "additionalInfo", skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<Vec<AdditionalInfoType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Message details to be displayed on a Charging Station.
///
/// Ports `MessageContentType`. Nested by [`IdTokenInfoType::personal_message`]
/// (and, in later slices, several other 2.0.1 messages).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageContentType {
    /// Format of the message content.
    pub format: MessageFormatEnumType,
    /// The message contents (max length 512).
    pub content: String,
    /// Message language identifier, an RFC 5646 code (max length 8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

/// Status information about an identifier, returned by the CSMS in an
/// `Authorize` response (and reused by the 2.0.1 transaction model).
///
/// Ports `IdTokenInfoType`. Only `status` is required; `cacheExpiryDateTime`
/// is advisory (used for caching, not for stopping an in-progress charge).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdTokenInfoType {
    /// Current authorization status of the identifier.
    pub status: AuthorizationStatusEnumType,
    /// Date/time after which the token must be considered invalid (RFC 3339).
    #[serde(
        rename = "cacheExpiryDateTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_expiry_date_time: Option<String>,
    /// Business priority, from -9 (lowest) to 9 (highest); default 0.
    #[serde(rename = "chargingPriority", skip_serializing_if = "Option::is_none")]
    pub charging_priority: Option<i32>,
    /// Preferred UI language, an RFC 5646 code (max length 8).
    #[serde(rename = "language1", skip_serializing_if = "Option::is_none")]
    pub language1: Option<String>,
    /// EVSE ids the token is restricted to (schema requires at least one entry
    /// when present); absent means valid for the whole Charging Station.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<Vec<i32>>,
    /// Second preferred UI language, an RFC 5646 code (max length 8).
    #[serde(rename = "language2", skip_serializing_if = "Option::is_none")]
    pub language2: Option<String>,
    /// Group/parent token this identifier belongs to.
    #[serde(rename = "groupIdToken", skip_serializing_if = "Option::is_none")]
    pub group_id_token: Option<IdTokenType>,
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
    fn id_token_enum_serializes_exact_spec_spellings() {
        // The non-PascalCase members are the interesting ones: `eMAID` is
        // camelCase, `ISO14443`/`ISO15693` are all-caps.
        for (variant, wire) in [
            (IdTokenEnumType::Central, "Central"),
            (IdTokenEnumType::EMAID, "eMAID"),
            (IdTokenEnumType::ISO14443, "ISO14443"),
            (IdTokenEnumType::ISO15693, "ISO15693"),
            (IdTokenEnumType::KeyCode, "KeyCode"),
            (IdTokenEnumType::Local, "Local"),
            (IdTokenEnumType::MacAddress, "MacAddress"),
            (IdTokenEnumType::NoAuthorization, "NoAuthorization"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: IdTokenEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
        // The 1.6J-spelled "EMAID" is not the 2.0.1 wire value.
        assert!(serde_json::from_value::<IdTokenEnumType>(json!("EMAID")).is_err());
    }

    #[test]
    fn authorization_status_round_trips_and_rejects_unknown() {
        for (variant, wire) in [
            (AuthorizationStatusEnumType::Accepted, "Accepted"),
            (AuthorizationStatusEnumType::Blocked, "Blocked"),
            (AuthorizationStatusEnumType::ConcurrentTx, "ConcurrentTx"),
            (AuthorizationStatusEnumType::Expired, "Expired"),
            (AuthorizationStatusEnumType::Invalid, "Invalid"),
            (AuthorizationStatusEnumType::NoCredit, "NoCredit"),
            (
                AuthorizationStatusEnumType::NotAllowedTypeEVSE,
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
    fn message_format_serializes_all_caps() {
        for (variant, wire) in [
            (MessageFormatEnumType::ASCII, "ASCII"),
            (MessageFormatEnumType::HTML, "HTML"),
            (MessageFormatEnumType::URI, "URI"),
            (MessageFormatEnumType::UTF8, "UTF8"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: MessageFormatEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn id_token_minimal_omits_optionals_and_renames_type() {
        let token = IdTokenType {
            id_token: "DEADBEEF".to_string(),
            id_token_type: IdTokenEnumType::ISO14443,
            additional_info: None,
            custom_data: None,
        };
        assert_eq!(
            serde_json::to_value(&token).unwrap(),
            json!({ "idToken": "DEADBEEF", "type": "ISO14443" })
        );
        let back: IdTokenType =
            serde_json::from_value(json!({ "idToken": "DEADBEEF", "type": "ISO14443" })).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn id_token_round_trips_with_additional_info() {
        let token = IdTokenType {
            id_token: "045918D2".to_string(),
            id_token_type: IdTokenEnumType::ISO15693,
            additional_info: Some(vec![AdditionalInfoType {
                additional_id_token: "ABC".to_string(),
                additional_token_type: "vendorX".to_string(),
                custom_data: None,
            }]),
            custom_data: None,
        };
        let wire = serde_json::to_value(&token).unwrap();
        assert_eq!(wire["additionalInfo"][0]["additionalIdToken"], json!("ABC"));
        assert_eq!(wire["additionalInfo"][0]["type"], json!("vendorX"));
        let back: IdTokenType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn id_token_info_minimal_is_status_only() {
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
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            json!({ "status": "Accepted" })
        );
    }

    #[test]
    fn id_token_info_round_trips_with_nested_objects() {
        let info = IdTokenInfoType {
            status: AuthorizationStatusEnumType::Accepted,
            cache_expiry_date_time: Some("2025-01-01T00:00:00Z".to_string()),
            charging_priority: Some(5),
            language1: Some("en".to_string()),
            evse_id: Some(vec![1, 2]),
            language2: Some("nl".to_string()),
            group_id_token: Some(IdTokenType {
                id_token: "GROUP-1".to_string(),
                id_token_type: IdTokenEnumType::Central,
                additional_info: None,
                custom_data: None,
            }),
            personal_message: Some(MessageContentType {
                format: MessageFormatEnumType::UTF8,
                content: "Welcome".to_string(),
                language: Some("en".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let wire = serde_json::to_value(&info).unwrap();
        assert_eq!(wire["groupIdToken"]["idToken"], json!("GROUP-1"));
        assert_eq!(wire["personalMessage"]["format"], json!("UTF8"));
        assert_eq!(wire["evseId"], json!([1, 2]));
        let back: IdTokenInfoType = serde_json::from_value(wire).unwrap();
        assert_eq!(back, info);
    }
}
