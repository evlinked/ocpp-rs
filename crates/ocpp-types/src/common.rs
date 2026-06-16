//! Common types and utilities used across OCPP implementations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generic meter value with unit and context information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterValue {
    /// Timestamp when meter value was sampled
    pub timestamp: DateTime<Utc>,
    /// Collection of sampled values
    #[serde(rename = "sampledValue")]
    pub sampled_values: Vec<SampledValue>,
}

/// Individual sampled value from a meter reading
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampledValue {
    /// Value as string (may contain numeric or other data)
    pub value: String,
    /// Context of the reading (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ReadingContext>,
    /// Format of the value (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ValueFormat>,
    /// What was measured (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurand: Option<Measurand>,
    /// Phase of the electrical system (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
    /// Location of measurement (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Unit of measurement (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<UnitOfMeasure>,
}

/// Context in which a meter value was taken.
///
/// Serialized using the dotted OCPP 1.6J wire values (e.g. `"Sample.Periodic"`),
/// matching the `context` enum in the bundled `MeterValues.json` and
/// `StopTransaction.json` schemas. A previous `#[serde(rename_all = "PascalCase")]`
/// emitted undotted values like `"SamplePeriodic"`, which a spec-conformant CSMS
/// rejects with `FormationViolation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReadingContext {
    /// Value taken at the start of an interruption.
    #[serde(rename = "Interruption.Begin")]
    InterruptionBegin,
    /// Value taken when resuming after an interruption.
    #[serde(rename = "Interruption.End")]
    InterruptionEnd,
    /// Value taken at a clock-aligned interval.
    #[serde(rename = "Sample.Clock")]
    SampleClock,
    /// Value taken as a periodic sample during a transaction.
    #[serde(rename = "Sample.Periodic")]
    SamplePeriodic,
    /// Value taken at the start of a transaction.
    #[serde(rename = "Transaction.Begin")]
    TransactionBegin,
    /// Value taken at the end of a transaction.
    #[serde(rename = "Transaction.End")]
    TransactionEnd,
    /// Value taken in response to a `TriggerMessage` request.
    #[serde(rename = "Trigger")]
    Trigger,
    /// Value taken for some other or unspecified reason.
    Other,
}

/// Format of the sampled value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ValueFormat {
    /// Raw value
    Raw,
    /// Signed integer
    SignedData,
}

/// Type of measurement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Measurand {
    /// Energy imported
    #[serde(rename = "Energy.Active.Import.Register")]
    EnergyActiveImportRegister,
    /// Energy exported
    #[serde(rename = "Energy.Active.Export.Register")]
    EnergyActiveExportRegister,
    /// Reactive energy imported
    #[serde(rename = "Energy.Reactive.Import.Register")]
    EnergyReactiveImportRegister,
    /// Reactive energy exported
    #[serde(rename = "Energy.Reactive.Export.Register")]
    EnergyReactiveExportRegister,
    /// Apparent energy imported
    #[serde(rename = "Energy.Active.Import.Interval")]
    EnergyActiveImportInterval,
    /// Apparent energy exported
    #[serde(rename = "Energy.Active.Export.Interval")]
    EnergyActiveExportInterval,
    /// Active power imported
    #[serde(rename = "Power.Active.Import")]
    PowerActiveImport,
    /// Active power exported
    #[serde(rename = "Power.Active.Export")]
    PowerActiveExport,
    /// Reactive power imported
    #[serde(rename = "Power.Reactive.Import")]
    PowerReactiveImport,
    /// Reactive power exported
    #[serde(rename = "Power.Reactive.Export")]
    PowerReactiveExport,
    /// Power factor
    #[serde(rename = "Power.Factor")]
    PowerFactor,
    /// Current imported
    #[serde(rename = "Current.Import")]
    CurrentImport,
    /// Current exported
    #[serde(rename = "Current.Export")]
    CurrentExport,
    /// Offered current
    #[serde(rename = "Current.Offered")]
    CurrentOffered,
    /// Voltage
    Voltage,
    /// Frequency
    Frequency,
    /// Temperature
    Temperature,
    /// State of charge
    SoC,
    /// RPM
    RPM,
}

/// Phase of electrical system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    /// Phase 1
    #[serde(rename = "L1")]
    L1,
    /// Phase 2
    #[serde(rename = "L2")]
    L2,
    /// Phase 3
    #[serde(rename = "L3")]
    L3,
    /// Neutral
    #[serde(rename = "N")]
    N,
    /// Line 1 to Neutral
    #[serde(rename = "L1-N")]
    L1N,
    /// Line 2 to Neutral
    #[serde(rename = "L2-N")]
    L2N,
    /// Line 3 to Neutral
    #[serde(rename = "L3-N")]
    L3N,
    /// Line 1 to Line 2
    #[serde(rename = "L1-L2")]
    L1L2,
    /// Line 2 to Line 3
    #[serde(rename = "L2-L3")]
    L2L3,
    /// Line 3 to Line 1
    #[serde(rename = "L3-L1")]
    L3L1,
}

/// Location of the measurement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Location {
    /// Cable
    Cable,
    /// EV (Electric Vehicle)
    EV,
    /// Inlet
    Inlet,
    /// Outlet
    Outlet,
    /// Body (of charge point)
    Body,
}

/// Unit of measurement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnitOfMeasure {
    /// Watt hours
    #[serde(rename = "Wh")]
    Wh,
    /// Kilowatt hours
    #[serde(rename = "kWh")]
    KWh,
    /// Volt ampere reactive hours
    #[serde(rename = "varh")]
    Varh,
    /// Kilovolt ampere reactive hours
    #[serde(rename = "kvarh")]
    Kvarh,
    /// Watts
    #[serde(rename = "W")]
    W,
    /// Kilowatts
    #[serde(rename = "kW")]
    KW,
    /// Volt ampere reactive
    #[serde(rename = "var")]
    Var,
    /// Kilovolt ampere reactive
    #[serde(rename = "kvar")]
    Kvar,
    /// Volt amperes
    #[serde(rename = "VA")]
    VA,
    /// Kilovolt amperes
    #[serde(rename = "kVA")]
    KVA,
    /// Amperes
    #[serde(rename = "A")]
    A,
    /// Volts
    #[serde(rename = "V")]
    V,
    /// Celsius
    Celsius,
    /// Fahrenheit
    Fahrenheit,
    /// Kelvin
    K,
    /// Percent (0-100)
    Percent,
}

/// Generic key-value configuration pair
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyValue {
    /// Configuration key name
    pub key: String,
    /// Configuration value (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Whether the value is read-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
}

/// Id tag info containing authorization data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdTagInfo {
    /// Authorization status
    pub status: AuthorizationStatus,
    /// Parent id tag (optional)
    #[serde(rename = "parentIdTag", skip_serializing_if = "Option::is_none")]
    pub parent_id_tag: Option<String>,
    /// Expiry date (optional)
    #[serde(rename = "expiryDate", skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<DateTime<Utc>>,
}

/// Authorization status for id tags
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AuthorizationStatus {
    /// Identifier is allowed for charging
    Accepted,
    /// Identifier has been blocked
    Blocked,
    /// Identifier has expired
    Expired,
    /// Identifier is invalid
    Invalid,
    /// Identifier is valid but EV driver doesn't allow concurrent transactions
    ConcurrentTx,
}

/// Reason for stopping a transaction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Reason {
    /// Emergency stop button was used
    EmergencyStop,
    /// EV disconnected
    EVDisconnected,
    /// Hard reset of charge point
    HardReset,
    /// Local action at charge point
    Local,
    /// Other reason
    Other,
    /// Power failure
    PowerLoss,
    /// Reboot of charge point
    Reboot,
    /// Remote action
    Remote,
    /// Soft reset of charge point
    SoftReset,
    /// Unlock connector button was used
    UnlockCommand,
    /// De-authorized by id tag
    DeAuthorized,
}

/// Availability status of a connector or charge point
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AvailabilityStatus {
    /// Accepted and scheduled
    Accepted,
    /// Rejected
    Rejected,
    /// Scheduled for later execution
    Scheduled,
}

/// Type of availability change
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AvailabilityType {
    /// Make available for new transactions
    Operative,
    /// Stop accepting new transactions
    Inoperative,
}

/// Generic vendor-specific data container
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorSpecificData {
    /// Vendor identifier
    #[serde(rename = "vendorId")]
    pub vendor_id: String,
    /// Vendor-specific data
    pub data: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_meter_value_serialization() {
        let meter_value = MeterValue {
            timestamp: DateTime::from_timestamp(1640995200, 0).unwrap(),
            sampled_values: vec![SampledValue {
                value: "1234.5".to_string(),
                context: Some(ReadingContext::TransactionBegin),
                format: Some(ValueFormat::Raw),
                measurand: Some(Measurand::EnergyActiveImportRegister),
                phase: Some(Phase::L1),
                location: Some(Location::Outlet),
                unit: Some(UnitOfMeasure::KWh),
            }],
        };

        let json = serde_json::to_string(&meter_value).unwrap();
        let deserialized: MeterValue = serde_json::from_str(&json).unwrap();
        assert_eq!(meter_value, deserialized);
    }

    #[test]
    fn reading_context_serializes_with_dotted_ocpp_values() {
        // OCPP 1.6J MeterValues.json / StopTransaction.json define the `context`
        // enum with dotted values. Assert the wire form matches the schema (a
        // plain round-trip would pass even with the wrong, undotted encoding).
        let cases = [
            (ReadingContext::InterruptionBegin, "\"Interruption.Begin\""),
            (ReadingContext::InterruptionEnd, "\"Interruption.End\""),
            (ReadingContext::SampleClock, "\"Sample.Clock\""),
            (ReadingContext::SamplePeriodic, "\"Sample.Periodic\""),
            (ReadingContext::TransactionBegin, "\"Transaction.Begin\""),
            (ReadingContext::TransactionEnd, "\"Transaction.End\""),
            (ReadingContext::Trigger, "\"Trigger\""),
            (ReadingContext::Other, "\"Other\""),
        ];
        for (ctx, wire) in cases {
            let json = serde_json::to_string(&ctx).unwrap();
            assert_eq!(json, wire, "wrong wire encoding for {ctx:?}");
            let back: ReadingContext = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ctx, "round-trip mismatch for {ctx:?}");
        }
    }

    #[test]
    fn test_authorization_status_serialization() {
        let status = AuthorizationStatus::Accepted;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Accepted\"");

        let deserialized: AuthorizationStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_id_tag_info() {
        let info = IdTagInfo {
            status: AuthorizationStatus::Accepted,
            parent_id_tag: Some("PARENT123".to_string()),
            expiry_date: Some(DateTime::from_timestamp(1640995200, 0).unwrap()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: IdTagInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_key_value_optional_fields() {
        let kv = KeyValue {
            key: "TestKey".to_string(),
            value: None,
            readonly: None,
        };

        let json = serde_json::to_string(&kv).unwrap();
        // Should not include null fields
        assert!(!json.contains("value"));
        assert!(!json.contains("readonly"));
    }

    #[test]
    fn test_measurand_complex_names() {
        let measurand = Measurand::EnergyActiveImportRegister;
        let json = serde_json::to_string(&measurand).unwrap();
        assert_eq!(json, "\"Energy.Active.Import.Register\"");

        let deserialized: Measurand = serde_json::from_str(&json).unwrap();
        assert_eq!(measurand, deserialized);
    }

    #[test]
    fn test_phase_complex_names() {
        let phase = Phase::L1N;
        let json = serde_json::to_string(&phase).unwrap();
        assert_eq!(json, "\"L1-N\"");
    }

    #[test]
    fn test_vendor_specific_data() {
        let mut data = HashMap::new();
        data.insert(
            "custom_field".to_string(),
            serde_json::Value::String("test_value".to_string()),
        );

        let vendor_data = VendorSpecificData {
            vendor_id: "TestVendor".to_string(),
            data,
        };

        let json = serde_json::to_string(&vendor_data).unwrap();
        let deserialized: VendorSpecificData = serde_json::from_str(&json).unwrap();
        assert_eq!(vendor_data, deserialized);
    }
}
