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
}
