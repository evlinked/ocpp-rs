//! OCPP 1.6J specific types and enums

use crate::common::IdTagInfo;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Firmware status enumeration (core + security extension variants)
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
    // --- Security Extension variants (OCPP 1.6J Security Annex) ---
    /// Signature verification failed
    SignatureError,
    /// Signing certificate has expired
    CertificateExpired,
    /// Signing certificate has been revoked
    CertificateRevoked,
    /// Installed firmware failed verification
    InstallVerificationFailed,
    /// Signature is invalid
    InvalidSignature,
    /// Certificate not yet valid
    NotYetValid,
    /// Signature was revoked
    RevokedSignatureError,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Local authorization list update status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum UpdateStatus {
    /// Update accepted
    Accepted,
    /// Update failed
    Failed,
    /// Local list management not supported
    NotSupported,
    /// List version mismatch
    VersionMismatch,
}

/// Entry in a local authorization list update
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizationData {
    /// The identifier to which this authorization applies
    #[serde(rename = "idTag")]
    pub id_tag: String,
    /// Authorization status info; absent in a differential update means deauthorize
    #[serde(rename = "idTagInfo", skip_serializing_if = "Option::is_none")]
    pub id_tag_info: Option<IdTagInfo>,
}

// =============================================================================
// Security Extension Types (OCPP 1.6J Security Annex)
// =============================================================================

/// Hash algorithm used to generate the CertificateHashData fields
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HashAlgorithmType {
    /// SHA-256
    #[serde(rename = "SHA256")]
    Sha256,
    /// SHA-384
    #[serde(rename = "SHA384")]
    Sha384,
    /// SHA-512
    #[serde(rename = "SHA512")]
    Sha512,
}

/// Which certificate(s) the CSMS is referring to
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CertificateUse {
    /// Central System root certificate
    CentralSystemRootCertificate,
    /// Manufacturer root certificate
    ManufacturerRootCertificate,
}

/// Status returned in response to a CertificateSigned request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CertificateSignedStatus {
    /// Certificate is valid and accepted
    Accepted,
    /// Certificate is invalid or rejected
    Rejected,
}

/// Status returned in response to a DeleteCertificate request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DeleteCertificateStatus {
    /// Certificate deleted successfully
    Accepted,
    /// Failed to delete certificate
    Failed,
    /// Certificate not found
    NotFound,
}

/// Status returned in response to a GetInstalledCertificateIds request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GetInstalledCertificatesStatus {
    /// One or more certificates found
    Accepted,
    /// No matching certificate found
    NotFound,
}

/// Status returned in response to an InstallCertificate request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum InstallCertificateStatus {
    /// Certificate installed successfully
    Accepted,
    /// Certificate rejected
    Rejected,
    /// Failed to install certificate
    Failed,
}

/// Generic two-value status used where only Accepted/Rejected is needed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GenericStatus {
    /// Accepted
    Accepted,
    /// Rejected
    Rejected,
}

/// Type of log file requested by the CSMS
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LogType {
    /// Diagnostics log
    DiagnosticsLog,
    /// Security log
    SecurityLog,
}

/// Upload status reported by LogStatusNotification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UploadLogStatus {
    /// A badly formatted packet or other protocol incompatibility
    BadMessage,
    /// The Charge Point is not uploading a log file; idle
    Idle,
    /// The server does not support the operation
    NotSupportedOperation,
    /// Insufficient permissions to perform the operation
    PermissionDenied,
    /// File uploaded successfully
    Uploaded,
    /// File upload failed
    UploadFailure,
    /// Uploading
    Uploading,
}

/// Status returned in response to a SignedUpdateFirmware request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum UpdateFirmwareStatus {
    /// Accepted; firmware update will be attempted
    Accepted,
    /// Rejected
    Rejected,
    /// Accepted but the update has been cancelled
    AcceptedCanceled,
    /// Certificate is invalid
    InvalidCertificate,
    /// Certificate has been revoked
    RevokedCertificate,
}

/// Hash data of an X.509 certificate for identification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertificateHashData {
    /// Hash algorithm used to generate the hash values
    #[serde(rename = "hashAlgorithm")]
    pub hash_algorithm: HashAlgorithmType,
    /// Hash of the issuer's Distinguished Name (DN)
    #[serde(rename = "issuerNameHash")]
    pub issuer_name_hash: String,
    /// Hash of the DER encoding of the issuer's public key
    #[serde(rename = "issuerKeyHash")]
    pub issuer_key_hash: String,
    /// Serial number of the certificate
    #[serde(rename = "serialNumber")]
    pub serial_number: String,
}

/// Parameters that describe the log file to be uploaded
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogParameters {
    /// URL of the location at which to store the log file
    #[serde(rename = "remoteLocation")]
    pub remote_location: String,
    /// Lower bound of the timestamp range; no lower bound if absent
    #[serde(rename = "oldestTimestamp", skip_serializing_if = "Option::is_none")]
    pub oldest_timestamp: Option<DateTime<Utc>>,
    /// Upper bound of the timestamp range; no upper bound if absent
    #[serde(rename = "latestTimestamp", skip_serializing_if = "Option::is_none")]
    pub latest_timestamp: Option<DateTime<Utc>>,
}

/// Firmware image information for a signed firmware update
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirmwareType {
    /// URI pointing to the location of the firmware
    pub location: String,
    /// Date and time at which the Charge Point must retrieve the firmware
    #[serde(rename = "retrieveDateTime")]
    pub retrieve_date_time: DateTime<Utc>,
    /// Certificate with which the firmware was signed
    #[serde(rename = "signingCertificate")]
    pub signing_certificate: String,
    /// Date and time at which the Charge Point must install the firmware (optional)
    #[serde(rename = "installDateTime", skip_serializing_if = "Option::is_none")]
    pub install_date_time: Option<DateTime<Utc>>,
    /// Base64-encoded firmware signature (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
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

impl std::fmt::Display for ChargePointStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChargePointStatus::Available => write!(f, "Available"),
            ChargePointStatus::Preparing => write!(f, "Preparing"),
            ChargePointStatus::Charging => write!(f, "Charging"),
            ChargePointStatus::SuspendedEV => write!(f, "SuspendedEV"),
            ChargePointStatus::SuspendedEVSE => write!(f, "SuspendedEVSE"),
            ChargePointStatus::Finishing => write!(f, "Finishing"),
            ChargePointStatus::Reserved => write!(f, "Reserved"),
            ChargePointStatus::Faulted => write!(f, "Faulted"),
            ChargePointStatus::Unavailable => write!(f, "Unavailable"),
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

    #[test]
    fn test_update_status_serialization() {
        let cases = [
            (UpdateStatus::Accepted, "\"Accepted\""),
            (UpdateStatus::Failed, "\"Failed\""),
            (UpdateStatus::NotSupported, "\"NotSupported\""),
            (UpdateStatus::VersionMismatch, "\"VersionMismatch\""),
        ];
        for (status, expected) in &cases {
            let json = serde_json::to_string(status).unwrap();
            assert_eq!(&json, expected);
            let deserialized: UpdateStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, &deserialized);
        }
    }

    #[test]
    fn test_authorization_data_serialization() {
        use crate::common::{AuthorizationStatus, IdTagInfo};

        let with_info = AuthorizationData {
            id_tag: "RFID001".to_string(),
            id_tag_info: Some(IdTagInfo {
                status: AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: None,
            }),
        };
        let json = serde_json::to_string(&with_info).unwrap();
        assert!(json.contains("idTag"));
        assert!(json.contains("idTagInfo"));
        let deserialized: AuthorizationData = serde_json::from_str(&json).unwrap();
        assert_eq!(with_info, deserialized);

        let without_info = AuthorizationData {
            id_tag: "RFID002".to_string(),
            id_tag_info: None,
        };
        let json = serde_json::to_string(&without_info).unwrap();
        assert!(!json.contains("idTagInfo"));
        let deserialized: AuthorizationData = serde_json::from_str(&json).unwrap();
        assert_eq!(without_info, deserialized);
    }

    #[test]
    fn test_firmware_status_security_variants() {
        let cases = [
            (FirmwareStatus::SignatureError, "\"SignatureError\""),
            (FirmwareStatus::CertificateExpired, "\"CertificateExpired\""),
            (FirmwareStatus::CertificateRevoked, "\"CertificateRevoked\""),
            (
                FirmwareStatus::InstallVerificationFailed,
                "\"InstallVerificationFailed\"",
            ),
            (FirmwareStatus::InvalidSignature, "\"InvalidSignature\""),
            (FirmwareStatus::NotYetValid, "\"NotYetValid\""),
            (
                FirmwareStatus::RevokedSignatureError,
                "\"RevokedSignatureError\"",
            ),
        ];
        for (status, expected) in &cases {
            let json = serde_json::to_string(status).unwrap();
            assert_eq!(&json, expected);
            let deserialized: FirmwareStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, &deserialized);
        }
    }

    #[test]
    fn test_certificate_hash_data() {
        let data = CertificateHashData {
            hash_algorithm: HashAlgorithmType::Sha256,
            issuer_name_hash: "abc123".to_string(),
            issuer_key_hash: "def456".to_string(),
            serial_number: "0001".to_string(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("hashAlgorithm"));
        assert!(json.contains("SHA256"));
        assert!(json.contains("issuerNameHash"));
        assert!(json.contains("issuerKeyHash"));
        assert!(json.contains("serialNumber"));
        let deserialized: CertificateHashData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, deserialized);
    }

    #[test]
    fn test_log_parameters() {
        let params = LogParameters {
            remote_location: "ftp://example.com/logs/".to_string(),
            oldest_timestamp: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
            latest_timestamp: None,
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("remoteLocation"));
        assert!(json.contains("oldestTimestamp"));
        assert!(!json.contains("latestTimestamp"));
        let deserialized: LogParameters = serde_json::from_str(&json).unwrap();
        assert_eq!(params, deserialized);

        let minimal = LogParameters {
            remote_location: "ftp://example.com/".to_string(),
            oldest_timestamp: None,
            latest_timestamp: None,
        };
        let json = serde_json::to_string(&minimal).unwrap();
        assert!(!json.contains("oldestTimestamp"));
        assert!(!json.contains("latestTimestamp"));
        let deserialized: LogParameters = serde_json::from_str(&json).unwrap();
        assert_eq!(minimal, deserialized);
    }

    #[test]
    fn test_firmware_type() {
        let firmware = FirmwareType {
            location: "https://example.com/firmware.bin".to_string(),
            retrieve_date_time: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            signing_certificate: "-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----"
                .to_string(),
            install_date_time: Some(Utc.with_ymd_and_hms(2024, 6, 1, 14, 0, 0).unwrap()),
            signature: Some("base64signature==".to_string()),
        };
        let json = serde_json::to_string(&firmware).unwrap();
        assert!(json.contains("retrieveDateTime"));
        assert!(json.contains("signingCertificate"));
        assert!(json.contains("installDateTime"));
        assert!(json.contains("signature"));
        let deserialized: FirmwareType = serde_json::from_str(&json).unwrap();
        assert_eq!(firmware, deserialized);

        let minimal = FirmwareType {
            location: "https://example.com/fw.bin".to_string(),
            retrieve_date_time: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            signing_certificate: "CERT".to_string(),
            install_date_time: None,
            signature: None,
        };
        let json = serde_json::to_string(&minimal).unwrap();
        assert!(!json.contains("installDateTime"));
        assert!(!json.contains("signature"));
        let deserialized: FirmwareType = serde_json::from_str(&json).unwrap();
        assert_eq!(minimal, deserialized);
    }

    #[test]
    fn test_security_enums() {
        assert_eq!(
            serde_json::to_string(&CertificateUse::CentralSystemRootCertificate).unwrap(),
            "\"CentralSystemRootCertificate\""
        );
        assert_eq!(
            serde_json::to_string(&CertificateSignedStatus::Accepted).unwrap(),
            "\"Accepted\""
        );
        assert_eq!(
            serde_json::to_string(&DeleteCertificateStatus::NotFound).unwrap(),
            "\"NotFound\""
        );
        assert_eq!(
            serde_json::to_string(&GetInstalledCertificatesStatus::NotFound).unwrap(),
            "\"NotFound\""
        );
        assert_eq!(
            serde_json::to_string(&InstallCertificateStatus::Failed).unwrap(),
            "\"Failed\""
        );
        assert_eq!(
            serde_json::to_string(&GenericStatus::Accepted).unwrap(),
            "\"Accepted\""
        );
        assert_eq!(
            serde_json::to_string(&LogType::SecurityLog).unwrap(),
            "\"SecurityLog\""
        );
        assert_eq!(
            serde_json::to_string(&UpdateFirmwareStatus::RevokedCertificate).unwrap(),
            "\"RevokedCertificate\""
        );
    }
}
