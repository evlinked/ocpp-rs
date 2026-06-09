//! OCPP 1.6J specific types and enums

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Charge point status enumeration for OCPP 1.6J
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChargePointStatus {
    /// Available for new transaction
    Available,
    /// Preparing for transaction
    Preparing,
    /// Charging in progress
    Charging,
    /// SuspendedEV - charging suspended by EV
    SuspendedEV,
    /// SuspendedEVSE - charging suspended by EVSE
    SuspendedEVSE,
    /// Transaction finished, ready to start new
    Finishing,
    /// Reserved for specific user
    Reserved,
    /// Out of order
    Faulted,
    /// Unavailable due to local action
    Unavailable,
}

impl std::fmt::Display for ChargePointStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Available => "Available",
            Self::Preparing => "Preparing",
            Self::Charging => "Charging",
            Self::SuspendedEV => "SuspendedEV",
            Self::SuspendedEVSE => "SuspendedEVSE",
            Self::Finishing => "Finishing",
            Self::Reserved => "Reserved",
            Self::Faulted => "Faulted",
            Self::Unavailable => "Unavailable",
        };
        write!(f, "{}", s)
    }
}

/// Error code enumeration for OCPP 1.6J
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChargePointErrorCode {
    /// Connector failure
    ConnectorLockFailure,
    /// EV communication failure
    EVCommunicationError,
    /// Ground failure
    GroundFailure,
    /// High temperature
    HighTemperature,
    /// Internal error
    InternalError,
    /// Local list conflict
    LocalListConflict,
    /// No error
    NoError,
    /// Other error
    OtherError,
    /// Over current failure
    OverCurrentFailure,
    /// Over voltage
    OverVoltage,
    /// Power meter failure
    PowerMeterFailure,
    /// Power switch failure
    PowerSwitchFailure,
    /// Reader failure
    ReaderFailure,
    /// Reset failure
    ResetFailure,
    /// Under voltage
    UnderVoltage,
    /// Weak signal
    WeakSignal,
}

/// Charge point vendor information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargePointVendorInfo {
    /// Charge point vendor name
    #[serde(rename = "chargePointVendor")]
    pub charge_point_vendor: String,
    /// Charge point model
    #[serde(rename = "chargePointModel")]
    pub charge_point_model: String,
    /// Charge point serial number (optional)
    #[serde(
        rename = "chargePointSerialNumber",
        skip_serializing_if = "Option::is_none"
    )]
    pub charge_point_serial_number: Option<String>,
    /// Charge box serial number (optional)
    #[serde(
        rename = "chargeBoxSerialNumber",
        skip_serializing_if = "Option::is_none"
    )]
    pub charge_box_serial_number: Option<String>,
    /// Firmware version (optional)
    #[serde(rename = "firmwareVersion", skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    /// ICCID of the modem (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iccid: Option<String>,
    /// IMSI of the modem (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imsi: Option<String>,
    /// Meter type (optional)
    #[serde(rename = "meterType", skip_serializing_if = "Option::is_none")]
    pub meter_type: Option<String>,
    /// Meter serial number (optional)
    #[serde(rename = "meterSerialNumber", skip_serializing_if = "Option::is_none")]
    pub meter_serial_number: Option<String>,
}

/// Diagnostics status enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DiagnosticsStatus {
    /// Diagnostics idle
    Idle,
    /// Diagnostics uploaded
    Uploaded,
    /// Upload failed
    UploadFailed,
    /// Uploading diagnostics
    Uploading,
}

/// Firmware status enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FirmwareStatus {
    /// Firmware downloaded
    Downloaded,
    /// Download failed
    DownloadFailed,
    /// Downloading firmware
    Downloading,
    /// Firmware idle
    Idle,
    /// Installation failed
    InstallationFailed,
    /// Installing firmware
    Installing,
    /// Firmware installed
    Installed,
}

/// Remote start/stop status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RemoteStartStopStatus {
    /// Request accepted
    Accepted,
    /// Request rejected
    Rejected,
}

/// Reservation status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ReservationStatus {
    /// Reservation accepted
    Accepted,
    /// Connector faulted
    Faulted,
    /// Connector occupied
    Occupied,
    /// Reservation rejected
    Rejected,
    /// Connector unavailable
    Unavailable,
}

/// Cancel reservation status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CancelReservationStatus {
    /// Cancellation accepted
    Accepted,
    /// Cancellation rejected
    Rejected,
}

/// Unlock status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum UnlockStatus {
    /// Unlock successful
    Unlocked,
    /// Unlock failed
    UnlockFailed,
    /// Not supported
    NotSupported,
}

/// Configuration status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ConfigurationStatus {
    /// Configuration accepted
    Accepted,
    /// Configuration rejected
    Rejected,
    /// Reboot required for configuration to take effect
    RebootRequired,
    /// Configuration not supported
    NotSupported,
}

/// Update type for firmware updates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum UpdateType {
    /// Differential update
    Differential,
    /// Full update
    Full,
}

/// Data transfer status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DataTransferStatus {
    /// Transfer accepted
    Accepted,
    /// Transfer rejected
    Rejected,
    /// Unknown message ID
    UnknownMessageId,
    /// Unknown vendor ID
    UnknownVendorId,
}

/// Reset type enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ResetType {
    /// Hard reset (reboot)
    Hard,
    /// Soft reset (restart software)
    Soft,
}

/// Reset status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ResetStatus {
    /// Reset accepted
    Accepted,
    /// Reset rejected
    Rejected,
}

/// Clear cache status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ClearCacheStatus {
    /// Cache cleared
    Accepted,
    /// Cache clear rejected
    Rejected,
}

/// Charging profile purpose
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChargingProfilePurposeType {
    /// Charge point maximum power
    ChargePointMaxProfile,
    /// Transaction-specific profile
    TxDefaultProfile,
    /// Transaction profile
    TxProfile,
}

/// Charging profile kind
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChargingProfileKindType {
    /// Absolute power limits
    Absolute,
    /// Recurring schedule
    Recurring,
    /// Relative power limits
    Relative,
}

/// Recurrency kind for charging profiles
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RecurrencyKindType {
    /// Daily recurrence
    Daily,
    /// Weekly recurrence
    Weekly,
}

/// Charging schedule period
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingSchedulePeriod {
    /// Start period offset in seconds from start of schedule
    #[serde(rename = "startPeriod")]
    pub start_period: i32,
    /// Power limit in Amperes
    pub limit: f64,
    /// Number of phases (optional)
    #[serde(rename = "numberPhases", skip_serializing_if = "Option::is_none")]
    pub number_phases: Option<i32>,
}

/// Charging schedule
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingSchedule {
    /// Duration in seconds (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
    /// Start schedule timestamp (optional)
    #[serde(rename = "startSchedule", skip_serializing_if = "Option::is_none")]
    pub start_schedule: Option<DateTime<Utc>>,
    /// Charging rate unit
    #[serde(rename = "chargingRateUnit")]
    pub charging_rate_unit: ChargingRateUnitType,
    /// Charging schedule periods
    #[serde(rename = "chargingSchedulePeriod")]
    pub charging_schedule_period: Vec<ChargingSchedulePeriod>,
    /// Minimum charging rate (optional)
    #[serde(rename = "minChargingRate", skip_serializing_if = "Option::is_none")]
    pub min_charging_rate: Option<f64>,
}

/// Charging rate unit
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChargingRateUnitType {
    /// Watts
    W,
    /// Amperes
    A,
}

/// Charging profile
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingProfile {
    /// Unique identifier
    #[serde(rename = "chargingProfileId")]
    pub charging_profile_id: i32,
    /// Transaction ID (for TxProfile only, optional)
    #[serde(rename = "transactionId", skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<i32>,
    /// Stack level (for priority)
    #[serde(rename = "stackLevel")]
    pub stack_level: i32,
    /// Purpose of the profile
    #[serde(rename = "chargingProfilePurpose")]
    pub charging_profile_purpose: ChargingProfilePurposeType,
    /// Kind of profile
    #[serde(rename = "chargingProfileKind")]
    pub charging_profile_kind: ChargingProfileKindType,
    /// Recurrency kind (optional)
    #[serde(rename = "recurrencyKind", skip_serializing_if = "Option::is_none")]
    pub recurrency_kind: Option<RecurrencyKindType>,
    /// Valid from timestamp (optional)
    #[serde(rename = "validFrom", skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    /// Valid to timestamp (optional)
    #[serde(rename = "validTo", skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<DateTime<Utc>>,
    /// Charging schedule
    #[serde(rename = "chargingSchedule")]
    pub charging_schedule: ChargingSchedule,
}

/// Charging profile status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChargingProfileStatus {
    /// Profile accepted
    Accepted,
    /// Profile rejected
    Rejected,
    /// Not supported
    NotSupported,
}

/// Clear charging profile status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ClearChargingProfileStatus {
    /// Clearing accepted
    Accepted,
    /// Unknown profile
    Unknown,
}

/// Get composite schedule status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GetCompositeScheduleStatus {
    /// Schedule accepted
    Accepted,
    /// Schedule rejected
    Rejected,
}

/// Trigger message request type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MessageTrigger {
    /// BootNotification
    BootNotification,
    /// DiagnosticsStatusNotification
    DiagnosticsStatusNotification,
    /// FirmwareStatusNotification
    FirmwareStatusNotification,
    /// Heartbeat
    Heartbeat,
    /// MeterValues
    MeterValues,
    /// StatusNotification
    StatusNotification,
}

/// Trigger message status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum TriggerMessageStatus {
    /// Trigger accepted
    Accepted,
    /// Trigger rejected
    Rejected,
    /// Not implemented
    NotImplemented,
}

impl std::fmt::Display for ChargePointErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChargePointErrorCode::ConnectorLockFailure => write!(f, "ConnectorLockFailure"),
            ChargePointErrorCode::EVCommunicationError => write!(f, "EVCommunicationError"),
            ChargePointErrorCode::GroundFailure => write!(f, "GroundFailure"),
            ChargePointErrorCode::HighTemperature => write!(f, "HighTemperature"),
            ChargePointErrorCode::InternalError => write!(f, "InternalError"),
            ChargePointErrorCode::LocalListConflict => write!(f, "LocalListConflict"),
            ChargePointErrorCode::NoError => write!(f, "NoError"),
            ChargePointErrorCode::OtherError => write!(f, "OtherError"),
            ChargePointErrorCode::OverCurrentFailure => write!(f, "OverCurrentFailure"),
            ChargePointErrorCode::OverVoltage => write!(f, "OverVoltage"),
            ChargePointErrorCode::PowerMeterFailure => write!(f, "PowerMeterFailure"),
            ChargePointErrorCode::PowerSwitchFailure => write!(f, "PowerSwitchFailure"),
            ChargePointErrorCode::ReaderFailure => write!(f, "ReaderFailure"),
            ChargePointErrorCode::ResetFailure => write!(f, "ResetFailure"),
            ChargePointErrorCode::UnderVoltage => write!(f, "UnderVoltage"),
            ChargePointErrorCode::WeakSignal => write!(f, "WeakSignal"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_charge_point_status_serialization() {
        let status = ChargePointStatus::Available;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Available\"");

        let deserialized: ChargePointStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_charge_point_vendor_info() {
        let info = ChargePointVendorInfo {
            charge_point_vendor: "TestVendor".to_string(),
            charge_point_model: "TestModel".to_string(),
            charge_point_serial_number: Some("SN123456".to_string()),
            charge_box_serial_number: None,
            firmware_version: Some("1.0.0".to_string()),
            iccid: None,
            imsi: None,
            meter_type: None,
            meter_serial_number: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ChargePointVendorInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, deserialized);

        // Check that None fields are not included in JSON
        assert!(!json.contains("chargeBoxSerialNumber"));
        assert!(!json.contains("iccid"));
    }

    #[test]
    fn test_charging_schedule_period() {
        let period = ChargingSchedulePeriod {
            start_period: 0,
            limit: 32.0,
            number_phases: Some(3),
        };

        let json = serde_json::to_string(&period).unwrap();
        let deserialized: ChargingSchedulePeriod = serde_json::from_str(&json).unwrap();
        assert_eq!(period, deserialized);
    }

    #[test]
    fn test_charging_schedule() {
        let schedule = ChargingSchedule {
            duration: Some(3600),
            start_schedule: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
            charging_rate_unit: ChargingRateUnitType::A,
            charging_schedule_period: vec![ChargingSchedulePeriod {
                start_period: 0,
                limit: 16.0,
                number_phases: None,
            }],
            min_charging_rate: Some(6.0),
        };

        let json = serde_json::to_string(&schedule).unwrap();
        let deserialized: ChargingSchedule = serde_json::from_str(&json).unwrap();
        assert_eq!(schedule, deserialized);
    }

    #[test]
    fn test_charging_profile() {
        let profile = ChargingProfile {
            charging_profile_id: 1,
            transaction_id: None,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeType::TxDefaultProfile,
            charging_profile_kind: ChargingProfileKindType::Absolute,
            recurrency_kind: None,
            valid_from: None,
            valid_to: None,
            charging_schedule: ChargingSchedule {
                duration: None,
                start_schedule: None,
                charging_rate_unit: ChargingRateUnitType::A,
                charging_schedule_period: vec![ChargingSchedulePeriod {
                    start_period: 0,
                    limit: 32.0,
                    number_phases: Some(3),
                }],
                min_charging_rate: None,
            },
        };

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: ChargingProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, deserialized);
    }

    #[test]
    fn test_error_code_serialization() {
        let error = ChargePointErrorCode::NoError;
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(json, "\"NoError\"");

        let internal_error = ChargePointErrorCode::InternalError;
        let json = serde_json::to_string(&internal_error).unwrap();
        assert_eq!(json, "\"InternalError\"");
    }

    #[test]
    fn test_enum_completeness() {
        // Test that all enum variants can be serialized/deserialized
        let statuses = vec![
            ChargePointStatus::Available,
            ChargePointStatus::Preparing,
            ChargePointStatus::Charging,
            ChargePointStatus::SuspendedEV,
            ChargePointStatus::SuspendedEVSE,
            ChargePointStatus::Finishing,
            ChargePointStatus::Reserved,
            ChargePointStatus::Faulted,
            ChargePointStatus::Unavailable,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let _deserialized: ChargePointStatus = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_message_trigger_enum() {
        let trigger = MessageTrigger::Heartbeat;
        let json = serde_json::to_string(&trigger).unwrap();
        assert_eq!(json, "\"Heartbeat\"");

        let deserialized: MessageTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(trigger, deserialized);
    }
}
