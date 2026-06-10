//! OCPP 1.6J message definitions
//!
//! This module contains all message types defined in the OCPP 1.6J specification,
//! organized by functional profiles.

use crate::{OcppAction, OcppResponse};
use chrono::{DateTime, Utc};
use ocpp_types::{common::*, v16j::*, IdToken};
use serde::{Deserialize, Serialize};

// =============================================================================
// Core Profile Messages
// =============================================================================

/// Authorize request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeRequest {
    /// The identifier that needs to be authorized
    #[serde(rename = "idTag")]
    pub id_tag: IdToken,
}

impl OcppAction for AuthorizeRequest {
    const ACTION_NAME: &'static str = "Authorize";
    type Response = AuthorizeResponse;
}

/// Authorize response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeResponse {
    /// Authorization information
    #[serde(rename = "idTagInfo")]
    pub id_tag_info: IdTagInfo,
}

impl OcppAction for AuthorizeResponse {
    const ACTION_NAME: &'static str = "AuthorizeResponse";
    type Response = Self;
}

impl OcppResponse for AuthorizeResponse {}

/// BootNotification request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootNotificationRequest {
    /// Charge point vendor identification
    #[serde(rename = "chargePointVendor")]
    pub charge_point_vendor: String,
    /// Charge point model identification
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
    /// ICCID of the modem's SIM card (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iccid: Option<String>,
    /// IMSI of the modem's SIM card (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imsi: Option<String>,
    /// Meter type (optional)
    #[serde(rename = "meterType", skip_serializing_if = "Option::is_none")]
    pub meter_type: Option<String>,
    /// Meter serial number (optional)
    #[serde(rename = "meterSerialNumber", skip_serializing_if = "Option::is_none")]
    pub meter_serial_number: Option<String>,
}

impl OcppAction for BootNotificationRequest {
    const ACTION_NAME: &'static str = "BootNotification";
    type Response = BootNotificationResponse;
}

/// BootNotification response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootNotificationResponse {
    /// Current time at central system
    #[serde(rename = "currentTime")]
    pub current_time: DateTime<Utc>,
    /// Heartbeat interval in seconds
    pub interval: i32,
    /// Registration status
    pub status: RegistrationStatus,
}

impl OcppAction for BootNotificationResponse {
    const ACTION_NAME: &'static str = "BootNotificationResponse";
    type Response = Self;
}

impl OcppResponse for BootNotificationResponse {}

/// Registration status for BootNotification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RegistrationStatus {
    /// Charge point is accepted by central system
    Accepted,
    /// Charge point is not yet accepted
    Pending,
    /// Charge point is rejected by central system
    Rejected,
}

/// Heartbeat request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatRequest {}

impl OcppAction for HeartbeatRequest {
    const ACTION_NAME: &'static str = "Heartbeat";
    type Response = HeartbeatResponse;
}

/// Heartbeat response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// Current time at central system
    #[serde(rename = "currentTime")]
    pub current_time: DateTime<Utc>,
}

impl OcppAction for HeartbeatResponse {
    const ACTION_NAME: &'static str = "HeartbeatResponse";
    type Response = Self;
}

impl OcppResponse for HeartbeatResponse {}

/// MeterValues request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterValuesRequest {
    /// Connector ID
    #[serde(rename = "connectorId")]
    pub connector_id: u32,
    /// Transaction ID (optional)
    #[serde(rename = "transactionId", skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<i32>,
    /// Meter values
    #[serde(rename = "meterValue")]
    pub meter_values: Vec<MeterValue>,
}

impl OcppAction for MeterValuesRequest {
    const ACTION_NAME: &'static str = "MeterValues";
    type Response = MeterValuesResponse;
}

/// MeterValues response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterValuesResponse {}

impl OcppAction for MeterValuesResponse {
    const ACTION_NAME: &'static str = "MeterValuesResponse";
    type Response = Self;
}

impl OcppResponse for MeterValuesResponse {}

/// StartTransaction request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartTransactionRequest {
    /// Connector ID
    #[serde(rename = "connectorId")]
    pub connector_id: u32,
    /// ID tag that started the transaction
    #[serde(rename = "idTag")]
    pub id_tag: IdToken,
    /// Meter start value in Wh
    #[serde(rename = "meterStart")]
    pub meter_start: i32,
    /// Timestamp when transaction started
    pub timestamp: DateTime<Utc>,
    /// Optional reservation ID
    #[serde(rename = "reservationId", skip_serializing_if = "Option::is_none")]
    pub reservation_id: Option<i32>,
}

impl OcppAction for StartTransactionRequest {
    const ACTION_NAME: &'static str = "StartTransaction";
    type Response = StartTransactionResponse;
}

/// StartTransaction response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartTransactionResponse {
    /// ID tag information
    #[serde(rename = "idTagInfo")]
    pub id_tag_info: IdTagInfo,
    /// Unique transaction ID
    #[serde(rename = "transactionId")]
    pub transaction_id: i32,
}

impl OcppAction for StartTransactionResponse {
    const ACTION_NAME: &'static str = "StartTransactionResponse";
    type Response = Self;
}

impl OcppResponse for StartTransactionResponse {}

/// StatusNotification request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusNotificationRequest {
    /// Connector ID
    #[serde(rename = "connectorId")]
    pub connector_id: u32,
    /// Error code
    #[serde(rename = "errorCode")]
    pub error_code: ChargePointErrorCode,
    /// Additional information about the error (optional)
    pub info: Option<String>,
    /// Current status
    pub status: ChargePointStatus,
    /// Timestamp of status change (optional)
    pub timestamp: Option<DateTime<Utc>>,
    /// Vendor-specific error code (optional)
    #[serde(rename = "vendorErrorCode", skip_serializing_if = "Option::is_none")]
    pub vendor_error_code: Option<String>,
    /// Vendor ID (optional)
    #[serde(rename = "vendorId", skip_serializing_if = "Option::is_none")]
    pub vendor_id: Option<String>,
}

impl OcppAction for StatusNotificationRequest {
    const ACTION_NAME: &'static str = "StatusNotification";
    type Response = StatusNotificationResponse;
}

/// StatusNotification response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusNotificationResponse {}

impl OcppAction for StatusNotificationResponse {
    const ACTION_NAME: &'static str = "StatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for StatusNotificationResponse {}

/// StopTransaction request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StopTransactionRequest {
    /// ID tag that stopped the transaction (optional)
    #[serde(rename = "idTag", skip_serializing_if = "Option::is_none")]
    pub id_tag: Option<IdToken>,
    /// Meter stop value in Wh
    #[serde(rename = "meterStop")]
    pub meter_stop: i32,
    /// Timestamp when transaction stopped
    pub timestamp: DateTime<Utc>,
    /// Transaction ID
    #[serde(rename = "transactionId")]
    pub transaction_id: i32,
    /// Reason for stopping (optional)
    pub reason: Option<Reason>,
    /// Transaction data (optional)
    #[serde(rename = "transactionData", skip_serializing_if = "Option::is_none")]
    pub transaction_data: Option<Vec<MeterValue>>,
}

impl OcppAction for StopTransactionRequest {
    const ACTION_NAME: &'static str = "StopTransaction";
    type Response = StopTransactionResponse;
}

/// StopTransaction response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StopTransactionResponse {
    /// ID tag information (optional)
    #[serde(rename = "idTagInfo", skip_serializing_if = "Option::is_none")]
    pub id_tag_info: Option<IdTagInfo>,
}

impl OcppAction for StopTransactionResponse {
    const ACTION_NAME: &'static str = "StopTransactionResponse";
    type Response = Self;
}

impl OcppResponse for StopTransactionResponse {}

// =============================================================================
// Remote Control Messages
// =============================================================================

/// ChangeAvailability request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeAvailabilityRequest {
    /// Connector ID (0 for entire charge point)
    #[serde(rename = "connectorId")]
    pub connector_id: u32,
    /// Availability type
    #[serde(rename = "type")]
    pub availability_type: AvailabilityType,
}

impl OcppAction for ChangeAvailabilityRequest {
    const ACTION_NAME: &'static str = "ChangeAvailability";
    type Response = ChangeAvailabilityResponse;
}

/// ChangeAvailability response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeAvailabilityResponse {
    /// Status of the availability change
    pub status: AvailabilityStatus,
}

impl OcppAction for ChangeAvailabilityResponse {
    const ACTION_NAME: &'static str = "ChangeAvailabilityResponse";
    type Response = Self;
}

impl OcppResponse for ChangeAvailabilityResponse {}

/// ChangeConfiguration request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeConfigurationRequest {
    /// Configuration key
    pub key: String,
    /// Configuration value
    pub value: String,
}

impl OcppAction for ChangeConfigurationRequest {
    const ACTION_NAME: &'static str = "ChangeConfiguration";
    type Response = ChangeConfigurationResponse;
}

/// ChangeConfiguration response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeConfigurationResponse {
    /// Status of configuration change
    pub status: ConfigurationStatus,
}

impl OcppAction for ChangeConfigurationResponse {
    const ACTION_NAME: &'static str = "ChangeConfigurationResponse";
    type Response = Self;
}

impl OcppResponse for ChangeConfigurationResponse {}

/// ClearCache request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearCacheRequest {}

impl OcppAction for ClearCacheRequest {
    const ACTION_NAME: &'static str = "ClearCache";
    type Response = ClearCacheResponse;
}

/// ClearCache response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearCacheResponse {
    /// Status of cache clearing
    pub status: ClearCacheStatus,
}

impl OcppAction for ClearCacheResponse {
    const ACTION_NAME: &'static str = "ClearCacheResponse";
    type Response = Self;
}

impl OcppResponse for ClearCacheResponse {}

/// DataTransfer request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataTransferRequest {
    /// Vendor identifier
    #[serde(rename = "vendorId")]
    pub vendor_id: String,
    /// Message identifier (optional)
    #[serde(rename = "messageId", skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Data (optional)
    pub data: Option<String>,
}

impl OcppAction for DataTransferRequest {
    const ACTION_NAME: &'static str = "DataTransfer";
    type Response = DataTransferResponse;
}

/// DataTransfer response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataTransferResponse {
    /// Status of data transfer
    pub status: DataTransferStatus,
    /// Response data (optional)
    pub data: Option<String>,
}

impl OcppAction for DataTransferResponse {
    const ACTION_NAME: &'static str = "DataTransferResponse";
    type Response = Self;
}

impl OcppResponse for DataTransferResponse {}

/// GetConfiguration request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetConfigurationRequest {
    /// List of keys to retrieve (optional)
    pub key: Option<Vec<String>>,
}

impl OcppAction for GetConfigurationRequest {
    const ACTION_NAME: &'static str = "GetConfiguration";
    type Response = GetConfigurationResponse;
}

/// GetConfiguration response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetConfigurationResponse {
    /// Configuration key-value pairs (optional)
    #[serde(rename = "configurationKey", skip_serializing_if = "Option::is_none")]
    pub configuration_keys: Option<Vec<KeyValue>>,
    /// Unknown keys (optional)
    #[serde(rename = "unknownKey", skip_serializing_if = "Option::is_none")]
    pub unknown_keys: Option<Vec<String>>,
}

impl OcppAction for GetConfigurationResponse {
    const ACTION_NAME: &'static str = "GetConfigurationResponse";
    type Response = Self;
}

impl OcppResponse for GetConfigurationResponse {}

/// RemoteStartTransaction request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteStartTransactionRequest {
    /// Connector ID (optional)
    #[serde(rename = "connectorId", skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<u32>,
    /// ID tag
    #[serde(rename = "idTag")]
    pub id_tag: IdToken,
    /// Charging profile (optional)
    #[serde(rename = "chargingProfile", skip_serializing_if = "Option::is_none")]
    pub charging_profile: Option<ChargingProfile>,
}

impl OcppAction for RemoteStartTransactionRequest {
    const ACTION_NAME: &'static str = "RemoteStartTransaction";
    type Response = RemoteStartTransactionResponse;
}

/// RemoteStartTransaction response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteStartTransactionResponse {
    /// Status of remote start
    pub status: RemoteStartStopStatus,
}

impl OcppAction for RemoteStartTransactionResponse {
    const ACTION_NAME: &'static str = "RemoteStartTransactionResponse";
    type Response = Self;
}

impl OcppResponse for RemoteStartTransactionResponse {}

/// RemoteStopTransaction request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteStopTransactionRequest {
    /// Transaction ID
    #[serde(rename = "transactionId")]
    pub transaction_id: i32,
}

impl OcppAction for RemoteStopTransactionRequest {
    const ACTION_NAME: &'static str = "RemoteStopTransaction";
    type Response = RemoteStopTransactionResponse;
}

/// RemoteStopTransaction response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteStopTransactionResponse {
    /// Status of remote stop
    pub status: RemoteStartStopStatus,
}

impl OcppAction for RemoteStopTransactionResponse {
    const ACTION_NAME: &'static str = "RemoteStopTransactionResponse";
    type Response = Self;
}

impl OcppResponse for RemoteStopTransactionResponse {}

/// Reset request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResetRequest {
    /// Reset type
    #[serde(rename = "type")]
    pub reset_type: ResetType,
}

impl OcppAction for ResetRequest {
    const ACTION_NAME: &'static str = "Reset";
    type Response = ResetResponse;
}

/// Reset response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResetResponse {
    /// Status of reset
    pub status: ResetStatus,
}

impl OcppAction for ResetResponse {
    const ACTION_NAME: &'static str = "ResetResponse";
    type Response = Self;
}

impl OcppResponse for ResetResponse {}

/// UnlockConnector request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnlockConnectorRequest {
    /// Connector ID
    #[serde(rename = "connectorId")]
    pub connector_id: u32,
}

impl OcppAction for UnlockConnectorRequest {
    const ACTION_NAME: &'static str = "UnlockConnector";
    type Response = UnlockConnectorResponse;
}

/// UnlockConnector response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnlockConnectorResponse {
    /// Status of unlock
    pub status: UnlockStatus,
}

impl OcppAction for UnlockConnectorResponse {
    const ACTION_NAME: &'static str = "UnlockConnectorResponse";
    type Response = Self;
}

impl OcppResponse for UnlockConnectorResponse {}

// =============================================================================
// Firmware Management Profile Messages
// =============================================================================

/// GetDiagnostics request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetDiagnosticsRequest {
    /// Location (URL) where diagnostics should be uploaded
    pub location: String,
    /// Number of retries (optional)
    pub retries: Option<i32>,
    /// Retry interval in seconds (optional)
    #[serde(rename = "retryInterval", skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<i32>,
    /// Start time for diagnostics (optional)
    #[serde(rename = "startTime", skip_serializing_if = "Option::is_none")]
    pub start_time: Option<DateTime<Utc>>,
    /// Stop time for diagnostics (optional)
    #[serde(rename = "stopTime", skip_serializing_if = "Option::is_none")]
    pub stop_time: Option<DateTime<Utc>>,
}

impl OcppAction for GetDiagnosticsRequest {
    const ACTION_NAME: &'static str = "GetDiagnostics";
    type Response = GetDiagnosticsResponse;
}

/// GetDiagnostics response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetDiagnosticsResponse {
    /// Filename of diagnostics (optional)
    #[serde(rename = "fileName", skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

impl OcppAction for GetDiagnosticsResponse {
    const ACTION_NAME: &'static str = "GetDiagnosticsResponse";
    type Response = Self;
}

impl OcppResponse for GetDiagnosticsResponse {}

/// DiagnosticsStatusNotification request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticsStatusNotificationRequest {
    /// Status of diagnostics upload
    pub status: DiagnosticsStatus,
}

impl OcppAction for DiagnosticsStatusNotificationRequest {
    const ACTION_NAME: &'static str = "DiagnosticsStatusNotification";
    type Response = DiagnosticsStatusNotificationResponse;
}

/// DiagnosticsStatusNotification response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticsStatusNotificationResponse {}

impl OcppAction for DiagnosticsStatusNotificationResponse {
    const ACTION_NAME: &'static str = "DiagnosticsStatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for DiagnosticsStatusNotificationResponse {}

/// UpdateFirmware request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateFirmwareRequest {
    /// Location (URL) of firmware
    pub location: String,
    /// Number of retries (optional)
    pub retries: Option<i32>,
    /// Retrieve date and time
    #[serde(rename = "retrieveDate")]
    pub retrieve_date: DateTime<Utc>,
    /// Retry interval in seconds (optional)
    #[serde(rename = "retryInterval", skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<i32>,
}

impl OcppAction for UpdateFirmwareRequest {
    const ACTION_NAME: &'static str = "UpdateFirmware";
    type Response = UpdateFirmwareResponse;
}

/// UpdateFirmware response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateFirmwareResponse {}

impl OcppAction for UpdateFirmwareResponse {
    const ACTION_NAME: &'static str = "UpdateFirmwareResponse";
    type Response = Self;
}

impl OcppResponse for UpdateFirmwareResponse {}

/// FirmwareStatusNotification request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirmwareStatusNotificationRequest {
    /// Status of firmware update
    pub status: FirmwareStatus,
}

impl OcppAction for FirmwareStatusNotificationRequest {
    const ACTION_NAME: &'static str = "FirmwareStatusNotification";
    type Response = FirmwareStatusNotificationResponse;
}

/// FirmwareStatusNotification response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirmwareStatusNotificationResponse {}

impl OcppAction for FirmwareStatusNotificationResponse {
    const ACTION_NAME: &'static str = "FirmwareStatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for FirmwareStatusNotificationResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_authorize_request_serialization() {
        let request = AuthorizeRequest {
            id_tag: "TAG123".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: AuthorizeRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request, deserialized);
        assert!(json.contains("idTag"));
        assert!(json.contains("TAG123"));
    }

    #[test]
    fn test_boot_notification_request() {
        let request = BootNotificationRequest {
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

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: BootNotificationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request, deserialized);

        // Check that None fields are not included
        assert!(!json.contains("chargeBoxSerialNumber"));
        assert!(!json.contains("iccid"));
    }

    #[test]
    fn test_start_transaction_request() {
        let request = StartTransactionRequest {
            connector_id: 1,
            id_tag: "USER123".to_string(),
            meter_start: 12345,
            timestamp: DateTime::from_timestamp(1640995200, 0).unwrap(),
            reservation_id: Some(456),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: StartTransactionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_status_notification_request() {
        let request = StatusNotificationRequest {
            connector_id: 1,
            error_code: ChargePointErrorCode::NoError,
            info: Some("Additional info".to_string()),
            status: ChargePointStatus::Available,
            timestamp: Some(DateTime::from_timestamp(1640995200, 0).unwrap()),
            vendor_error_code: None,
            vendor_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: StatusNotificationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_remote_start_transaction_request() {
        let request = RemoteStartTransactionRequest {
            connector_id: Some(1),
            id_tag: "REMOTE123".to_string(),
            charging_profile: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RemoteStartTransactionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_registration_status_serialization() {
        let status = RegistrationStatus::Accepted;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Accepted\"");

        let deserialized: RegistrationStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_action_names() {
        assert_eq!(AuthorizeRequest::ACTION_NAME, "Authorize");
        assert_eq!(BootNotificationRequest::ACTION_NAME, "BootNotification");
        assert_eq!(HeartbeatRequest::ACTION_NAME, "Heartbeat");
        assert_eq!(StartTransactionRequest::ACTION_NAME, "StartTransaction");
        assert_eq!(StopTransactionRequest::ACTION_NAME, "StopTransaction");
        assert_eq!(StatusNotificationRequest::ACTION_NAME, "StatusNotification");
        assert_eq!(MeterValuesRequest::ACTION_NAME, "MeterValues");
        assert_eq!(
            RemoteStartTransactionRequest::ACTION_NAME,
            "RemoteStartTransaction"
        );
        assert_eq!(
            RemoteStopTransactionRequest::ACTION_NAME,
            "RemoteStopTransaction"
        );
        assert_eq!(ResetRequest::ACTION_NAME, "Reset");
    }

    #[test]
    fn test_heartbeat_messages() {
        let request = HeartbeatRequest {};
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: HeartbeatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let response = HeartbeatResponse {
            current_time: DateTime::from_timestamp(1640995200, 0).unwrap(),
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: HeartbeatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }
}
