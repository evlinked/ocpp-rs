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
    /// List of keys to retrieve (optional).
    ///
    /// Omitted from the wire when `None` — the OCPP 1.6J `GetConfiguration`
    /// schema types `key` as an array, so serializing it as `null` would fail
    /// schema validation. Matches the "absent optional field" convention used
    /// by every other optional field and the Python reference.
    #[serde(skip_serializing_if = "Option::is_none")]
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

// =============================================================================
// Smart Charging Profile Messages
// =============================================================================

/// SetChargingProfile request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetChargingProfileRequest {
    /// Connector to which the charging profile applies (0 = all connectors)
    #[serde(rename = "connectorId")]
    pub connector_id: i32,
    /// Charging profile to set
    #[serde(rename = "csChargingProfiles")]
    pub cs_charging_profiles: ChargingProfile,
}

impl OcppAction for SetChargingProfileRequest {
    const ACTION_NAME: &'static str = "SetChargingProfile";
    type Response = SetChargingProfileResponse;
}

/// SetChargingProfile response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetChargingProfileResponse {
    /// Acceptance status of the profile
    pub status: ChargingProfileStatus,
}

impl OcppAction for SetChargingProfileResponse {
    const ACTION_NAME: &'static str = "SetChargingProfileResponse";
    type Response = Self;
}

impl OcppResponse for SetChargingProfileResponse {}

/// ClearChargingProfile request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearChargingProfileRequest {
    /// ID of the charging profile to clear (optional — clears all matching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    /// Connector whose profile should be cleared (optional)
    #[serde(rename = "connectorId", skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<i32>,
    /// Purpose of profiles to clear (optional)
    #[serde(
        rename = "chargingProfilePurpose",
        skip_serializing_if = "Option::is_none"
    )]
    pub charging_profile_purpose: Option<ChargingProfilePurposeType>,
    /// Stack level of profiles to clear (optional)
    #[serde(rename = "stackLevel", skip_serializing_if = "Option::is_none")]
    pub stack_level: Option<i32>,
}

impl OcppAction for ClearChargingProfileRequest {
    const ACTION_NAME: &'static str = "ClearChargingProfile";
    type Response = ClearChargingProfileResponse;
}

/// ClearChargingProfile response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearChargingProfileResponse {
    /// Whether a matching profile was found and cleared
    pub status: ClearChargingProfileStatus,
}

impl OcppAction for ClearChargingProfileResponse {
    const ACTION_NAME: &'static str = "ClearChargingProfileResponse";
    type Response = Self;
}

impl OcppResponse for ClearChargingProfileResponse {}

/// GetCompositeSchedule request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCompositeScheduleRequest {
    /// Connector for which to calculate the schedule (0 = entire charge point)
    #[serde(rename = "connectorId")]
    pub connector_id: i32,
    /// Length of the schedule in seconds
    pub duration: i32,
    /// Desired unit of measure in the response (optional)
    #[serde(rename = "chargingRateUnit", skip_serializing_if = "Option::is_none")]
    pub charging_rate_unit: Option<ChargingRateUnitType>,
}

impl OcppAction for GetCompositeScheduleRequest {
    const ACTION_NAME: &'static str = "GetCompositeSchedule";
    type Response = GetCompositeScheduleResponse;
}

/// GetCompositeSchedule response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCompositeScheduleResponse {
    /// Whether the CSMS was able to process the request
    pub status: GetCompositeScheduleStatus,
    /// Connector for which the schedule is returned (optional)
    #[serde(rename = "connectorId", skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<i32>,
    /// Start time of the schedule (optional)
    #[serde(rename = "scheduleStart", skip_serializing_if = "Option::is_none")]
    pub schedule_start: Option<DateTime<Utc>>,
    /// Composite charging schedule (optional)
    #[serde(rename = "chargingSchedule", skip_serializing_if = "Option::is_none")]
    pub charging_schedule: Option<ChargingSchedule>,
}

impl OcppAction for GetCompositeScheduleResponse {
    const ACTION_NAME: &'static str = "GetCompositeScheduleResponse";
    type Response = Self;
}

impl OcppResponse for GetCompositeScheduleResponse {}

// =============================================================================
// Reservation Profile Messages
// =============================================================================

/// ReserveNow request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReserveNowRequest {
    /// Connector to reserve (0 = no specific connector)
    #[serde(rename = "connectorId")]
    pub connector_id: i32,
    /// Expiry date and time of the reservation
    #[serde(rename = "expiryDate")]
    pub expiry_date: DateTime<Utc>,
    /// Identifier to be used for the reservation
    #[serde(rename = "idTag")]
    pub id_tag: IdToken,
    /// Unique reservation ID
    #[serde(rename = "reservationId")]
    pub reservation_id: i32,
    /// Parent identifier (optional)
    #[serde(rename = "parentIdTag", skip_serializing_if = "Option::is_none")]
    pub parent_id_tag: Option<String>,
}

impl OcppAction for ReserveNowRequest {
    const ACTION_NAME: &'static str = "ReserveNow";
    type Response = ReserveNowResponse;
}

/// ReserveNow response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReserveNowResponse {
    /// Acceptance status of the reservation request
    pub status: ReservationStatus,
}

impl OcppAction for ReserveNowResponse {
    const ACTION_NAME: &'static str = "ReserveNowResponse";
    type Response = Self;
}

impl OcppResponse for ReserveNowResponse {}

/// CancelReservation request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelReservationRequest {
    /// ID of the reservation to cancel
    #[serde(rename = "reservationId")]
    pub reservation_id: i32,
}

impl OcppAction for CancelReservationRequest {
    const ACTION_NAME: &'static str = "CancelReservation";
    type Response = CancelReservationResponse;
}

/// CancelReservation response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelReservationResponse {
    /// Whether the cancellation was accepted
    pub status: CancelReservationStatus,
}

impl OcppAction for CancelReservationResponse {
    const ACTION_NAME: &'static str = "CancelReservationResponse";
    type Response = Self;
}

impl OcppResponse for CancelReservationResponse {}

// =============================================================================
// Remote Trigger Profile Messages
// =============================================================================

/// TriggerMessage request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerMessageRequest {
    /// Type of message to trigger
    #[serde(rename = "requestedMessage")]
    pub requested_message: MessageTrigger,
    /// Connector for which the message should be triggered (optional)
    #[serde(rename = "connectorId", skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<i32>,
}

impl OcppAction for TriggerMessageRequest {
    const ACTION_NAME: &'static str = "TriggerMessage";
    type Response = TriggerMessageResponse;
}

/// TriggerMessage response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerMessageResponse {
    /// Whether the trigger was accepted
    pub status: TriggerMessageStatus,
}

impl OcppAction for TriggerMessageResponse {
    const ACTION_NAME: &'static str = "TriggerMessageResponse";
    type Response = Self;
}

impl OcppResponse for TriggerMessageResponse {}

// =============================================================================
// Local Authorization List Profile Messages
// =============================================================================

/// GetLocalListVersion request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetLocalListVersionRequest {}

impl OcppAction for GetLocalListVersionRequest {
    const ACTION_NAME: &'static str = "GetLocalListVersion";
    type Response = GetLocalListVersionResponse;
}

/// GetLocalListVersion response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetLocalListVersionResponse {
    /// Version number of the local authorization list
    #[serde(rename = "listVersion")]
    pub list_version: i32,
}

impl OcppAction for GetLocalListVersionResponse {
    const ACTION_NAME: &'static str = "GetLocalListVersionResponse";
    type Response = Self;
}

impl OcppResponse for GetLocalListVersionResponse {}

/// SendLocalList request message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendLocalListRequest {
    /// Version number of the list after this update
    #[serde(rename = "listVersion")]
    pub list_version: i32,
    /// Whether to apply a full or differential update
    #[serde(rename = "updateType")]
    pub update_type: UpdateType,
    /// Authorization list entries (empty for Full update means clear the list)
    #[serde(rename = "localAuthorizationList", default)]
    pub local_authorization_list: Vec<AuthorizationData>,
}

impl OcppAction for SendLocalListRequest {
    const ACTION_NAME: &'static str = "SendLocalList";
    type Response = SendLocalListResponse;
}

/// SendLocalList response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendLocalListResponse {
    /// Acceptance status of the update
    pub status: UpdateStatus,
}

impl OcppAction for SendLocalListResponse {
    const ACTION_NAME: &'static str = "SendLocalListResponse";
    type Response = Self;
}

impl OcppResponse for SendLocalListResponse {}

// =============================================================================
// Security Extension Messages (OCPP 1.6J Security Annex)
// =============================================================================

/// CertificateSigned request — CSMS sends a signed certificate to the Charge Point
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertificateSignedRequest {
    /// The signed PEM encoded X.509 certificate (chain) (max 10 000 chars)
    #[serde(rename = "certificateChain")]
    pub certificate_chain: String,
}

impl OcppAction for CertificateSignedRequest {
    const ACTION_NAME: &'static str = "CertificateSigned";
    type Response = CertificateSignedResponse;
}

/// CertificateSigned response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertificateSignedResponse {
    /// Whether the certificate was accepted
    pub status: CertificateSignedStatus,
}

impl OcppAction for CertificateSignedResponse {
    const ACTION_NAME: &'static str = "CertificateSignedResponse";
    type Response = Self;
}

impl OcppResponse for CertificateSignedResponse {}

/// DeleteCertificate request — CSMS instructs the Charge Point to delete an installed certificate
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteCertificateRequest {
    /// Hash data identifying the certificate to delete
    #[serde(rename = "certificateHashData")]
    pub certificate_hash_data: CertificateHashData,
}

impl OcppAction for DeleteCertificateRequest {
    const ACTION_NAME: &'static str = "DeleteCertificate";
    type Response = DeleteCertificateResponse;
}

/// DeleteCertificate response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteCertificateResponse {
    /// Outcome of the deletion request
    pub status: DeleteCertificateStatus,
}

impl OcppAction for DeleteCertificateResponse {
    const ACTION_NAME: &'static str = "DeleteCertificateResponse";
    type Response = Self;
}

impl OcppResponse for DeleteCertificateResponse {}

/// ExtendedTriggerMessage request — CSMS requests the Charge Point to send a specific message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtendedTriggerMessageRequest {
    /// The type of message to be triggered
    #[serde(rename = "requestedMessage")]
    pub requested_message: MessageTrigger,
    /// Connector for which the message should be triggered (optional)
    #[serde(rename = "connectorId", skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<i32>,
}

impl OcppAction for ExtendedTriggerMessageRequest {
    const ACTION_NAME: &'static str = "ExtendedTriggerMessage";
    type Response = ExtendedTriggerMessageResponse;
}

/// ExtendedTriggerMessage response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtendedTriggerMessageResponse {
    /// Whether the trigger request was accepted
    pub status: TriggerMessageStatus,
}

impl OcppAction for ExtendedTriggerMessageResponse {
    const ACTION_NAME: &'static str = "ExtendedTriggerMessageResponse";
    type Response = Self;
}

impl OcppResponse for ExtendedTriggerMessageResponse {}

/// GetInstalledCertificateIds request — CSMS queries the certificates installed on the Charge Point
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetInstalledCertificateIdsRequest {
    /// Which certificate type to retrieve
    #[serde(rename = "certificateType")]
    pub certificate_type: CertificateUse,
}

impl OcppAction for GetInstalledCertificateIdsRequest {
    const ACTION_NAME: &'static str = "GetInstalledCertificateIds";
    type Response = GetInstalledCertificateIdsResponse;
}

/// GetInstalledCertificateIds response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetInstalledCertificateIdsResponse {
    /// Whether one or more certificates were found
    pub status: GetInstalledCertificatesStatus,
    /// Hash data of each installed certificate (absent when status is NotFound)
    #[serde(
        rename = "certificateHashDataChain",
        skip_serializing_if = "Option::is_none"
    )]
    pub certificate_hash_data_chain: Option<Vec<CertificateHashData>>,
}

impl OcppAction for GetInstalledCertificateIdsResponse {
    const ACTION_NAME: &'static str = "GetInstalledCertificateIdsResponse";
    type Response = Self;
}

impl OcppResponse for GetInstalledCertificateIdsResponse {}

/// GetLog request — CSMS requests the Charge Point to upload a log file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetLogRequest {
    /// Type of log file to upload
    #[serde(rename = "logType")]
    pub log_type: LogType,
    /// Request ID used to correlate the LogStatusNotification
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Parameters describing the log file
    pub log: LogParameters,
    /// Number of upload retries (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i32>,
    /// Interval in seconds between retries (optional)
    #[serde(rename = "retryInterval", skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<i32>,
}

impl OcppAction for GetLogRequest {
    const ACTION_NAME: &'static str = "GetLog";
    type Response = GetLogResponse;
}

/// GetLog response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetLogResponse {
    /// Upload status
    pub status: UploadLogStatus,
    /// Filename of the log (optional)
    #[serde(rename = "filename", skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

impl OcppAction for GetLogResponse {
    const ACTION_NAME: &'static str = "GetLogResponse";
    type Response = Self;
}

impl OcppResponse for GetLogResponse {}

/// InstallCertificate request — CSMS instructs the Charge Point to install a CA certificate
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallCertificateRequest {
    /// Type of certificate to install
    #[serde(rename = "certificateType")]
    pub certificate_type: CertificateUse,
    /// PEM encoded X.509 certificate
    pub certificate: String,
}

impl OcppAction for InstallCertificateRequest {
    const ACTION_NAME: &'static str = "InstallCertificate";
    type Response = InstallCertificateResponse;
}

/// InstallCertificate response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallCertificateResponse {
    /// Outcome of the installation request
    pub status: InstallCertificateStatus,
}

impl OcppAction for InstallCertificateResponse {
    const ACTION_NAME: &'static str = "InstallCertificateResponse";
    type Response = Self;
}

impl OcppResponse for InstallCertificateResponse {}

/// LogStatusNotification request — Charge Point reports the status of a log upload
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogStatusNotificationRequest {
    /// Current upload status
    pub status: UploadLogStatus,
    /// The request ID from the original GetLog request
    #[serde(rename = "requestId")]
    pub request_id: i32,
}

impl OcppAction for LogStatusNotificationRequest {
    const ACTION_NAME: &'static str = "LogStatusNotification";
    type Response = LogStatusNotificationResponse;
}

/// LogStatusNotification response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogStatusNotificationResponse {}

impl OcppAction for LogStatusNotificationResponse {
    const ACTION_NAME: &'static str = "LogStatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for LogStatusNotificationResponse {}

/// SecurityEventNotification request — Charge Point reports a security event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityEventNotificationRequest {
    /// The security event type string
    #[serde(rename = "type")]
    pub event_type: String,
    /// Timestamp of the security event
    pub timestamp: DateTime<Utc>,
    /// Additional technical information (optional)
    #[serde(rename = "techInfo", skip_serializing_if = "Option::is_none")]
    pub tech_info: Option<String>,
}

impl OcppAction for SecurityEventNotificationRequest {
    const ACTION_NAME: &'static str = "SecurityEventNotification";
    type Response = SecurityEventNotificationResponse;
}

/// SecurityEventNotification response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityEventNotificationResponse {}

impl OcppAction for SecurityEventNotificationResponse {
    const ACTION_NAME: &'static str = "SecurityEventNotificationResponse";
    type Response = Self;
}

impl OcppResponse for SecurityEventNotificationResponse {}

/// SignCertificate request — Charge Point sends a CSR for the CSMS to sign
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignCertificateRequest {
    /// PEM encoded Certificate Signing Request (CSR)
    pub csr: String,
}

impl OcppAction for SignCertificateRequest {
    const ACTION_NAME: &'static str = "SignCertificate";
    type Response = SignCertificateResponse;
}

/// SignCertificate response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignCertificateResponse {
    /// Whether the CSR was accepted
    pub status: GenericStatus,
}

impl OcppAction for SignCertificateResponse {
    const ACTION_NAME: &'static str = "SignCertificateResponse";
    type Response = Self;
}

impl OcppResponse for SignCertificateResponse {}

/// SignedFirmwareStatusNotification request — Charge Point reports status of a signed firmware update
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedFirmwareStatusNotificationRequest {
    /// Current firmware update status (including security-extension variants)
    pub status: FirmwareStatus,
    /// The request ID from the original SignedUpdateFirmware request
    #[serde(rename = "requestId")]
    pub request_id: i32,
}

impl OcppAction for SignedFirmwareStatusNotificationRequest {
    const ACTION_NAME: &'static str = "SignedFirmwareStatusNotification";
    type Response = SignedFirmwareStatusNotificationResponse;
}

/// SignedFirmwareStatusNotification response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedFirmwareStatusNotificationResponse {}

impl OcppAction for SignedFirmwareStatusNotificationResponse {
    const ACTION_NAME: &'static str = "SignedFirmwareStatusNotificationResponse";
    type Response = Self;
}

impl OcppResponse for SignedFirmwareStatusNotificationResponse {}

/// SignedUpdateFirmware request — CSMS initiates a signed firmware update on the Charge Point
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedUpdateFirmwareRequest {
    /// Identifies this request; referenced in SignedFirmwareStatusNotification
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Firmware image to install
    pub firmware: FirmwareType,
    /// Number of download/install retries (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i32>,
    /// Interval in seconds between retries (optional)
    #[serde(rename = "retryInterval", skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<i32>,
}

impl OcppAction for SignedUpdateFirmwareRequest {
    const ACTION_NAME: &'static str = "SignedUpdateFirmware";
    type Response = SignedUpdateFirmwareResponse;
}

/// SignedUpdateFirmware response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedUpdateFirmwareResponse {
    /// Whether the request was accepted
    pub status: UpdateFirmwareStatus,
}

impl OcppAction for SignedUpdateFirmwareResponse {
    const ACTION_NAME: &'static str = "SignedUpdateFirmwareResponse";
    type Response = Self;
}

impl OcppResponse for SignedUpdateFirmwareResponse {}

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

    #[test]
    fn test_set_charging_profile_request() {
        let request = SetChargingProfileRequest {
            connector_id: 1,
            cs_charging_profiles: ChargingProfile {
                charging_profile_id: 42,
                transaction_id: None,
                stack_level: 0,
                charging_profile_purpose: ChargingProfilePurposeType::TxDefaultProfile,
                charging_profile_kind: ChargingProfileKindType::Absolute,
                recurrency_kind: None,
                valid_from: None,
                valid_to: None,
                charging_schedule: ChargingSchedule {
                    duration: Some(3600),
                    start_schedule: None,
                    charging_rate_unit: ChargingRateUnitType::A,
                    charging_schedule_period: vec![ChargingSchedulePeriod {
                        start_period: 0,
                        limit: 16.0,
                        number_phases: Some(3),
                    }],
                    min_charging_rate: None,
                },
            },
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("connectorId"));
        assert!(json.contains("csChargingProfiles"));
        let deserialized: SetChargingProfileRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let response = SetChargingProfileResponse {
            status: ChargingProfileStatus::Accepted,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Accepted"));
        let deserialized: SetChargingProfileResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_clear_charging_profile_request() {
        let request = ClearChargingProfileRequest {
            id: Some(42),
            connector_id: None,
            charging_profile_purpose: Some(ChargingProfilePurposeType::TxDefaultProfile),
            stack_level: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("connectorId"));
        assert!(!json.contains("stackLevel"));
        let deserialized: ClearChargingProfileRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let all_none = ClearChargingProfileRequest {
            id: None,
            connector_id: None,
            charging_profile_purpose: None,
            stack_level: None,
        };
        let json = serde_json::to_string(&all_none).unwrap();
        assert_eq!(json, "{}");

        let response = ClearChargingProfileResponse {
            status: ClearChargingProfileStatus::Accepted,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ClearChargingProfileResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_get_composite_schedule_request() {
        let request = GetCompositeScheduleRequest {
            connector_id: 1,
            duration: 86400,
            charging_rate_unit: Some(ChargingRateUnitType::W),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("chargingRateUnit"));
        let deserialized: GetCompositeScheduleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let response = GetCompositeScheduleResponse {
            status: GetCompositeScheduleStatus::Accepted,
            connector_id: Some(1),
            schedule_start: Some(DateTime::from_timestamp(1640995200, 0).unwrap()),
            charging_schedule: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("chargingSchedule"));
        let deserialized: GetCompositeScheduleResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);

        let rejected = GetCompositeScheduleResponse {
            status: GetCompositeScheduleStatus::Rejected,
            connector_id: None,
            schedule_start: None,
            charging_schedule: None,
        };
        let json = serde_json::to_string(&rejected).unwrap();
        assert_eq!(json, r#"{"status":"Rejected"}"#);
    }

    #[test]
    fn test_reserve_now_request() {
        let request = ReserveNowRequest {
            connector_id: 1,
            expiry_date: DateTime::from_timestamp(1640995200, 0).unwrap(),
            id_tag: "RFID001".to_string(),
            reservation_id: 99,
            parent_id_tag: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("connectorId"));
        assert!(json.contains("expiryDate"));
        assert!(json.contains("reservationId"));
        assert!(!json.contains("parentIdTag"));
        let deserialized: ReserveNowRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let with_parent = ReserveNowRequest {
            connector_id: 1,
            expiry_date: DateTime::from_timestamp(1640995200, 0).unwrap(),
            id_tag: "RFID001".to_string(),
            reservation_id: 100,
            parent_id_tag: Some("GROUP01".to_string()),
        };
        let json = serde_json::to_string(&with_parent).unwrap();
        assert!(json.contains("parentIdTag"));
        let deserialized: ReserveNowRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(with_parent, deserialized);

        let response = ReserveNowResponse {
            status: ReservationStatus::Accepted,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ReserveNowResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_cancel_reservation_request() {
        let request = CancelReservationRequest { reservation_id: 42 };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("reservationId"));
        let deserialized: CancelReservationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let response = CancelReservationResponse {
            status: CancelReservationStatus::Accepted,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: CancelReservationResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);

        let rejected = CancelReservationResponse {
            status: CancelReservationStatus::Rejected,
        };
        let json = serde_json::to_string(&rejected).unwrap();
        assert_eq!(json, r#"{"status":"Rejected"}"#);
    }

    #[test]
    fn test_trigger_message_request() {
        let request = TriggerMessageRequest {
            requested_message: MessageTrigger::Heartbeat,
            connector_id: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("requestedMessage"));
        assert!(!json.contains("connectorId"));
        let deserialized: TriggerMessageRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let with_connector = TriggerMessageRequest {
            requested_message: MessageTrigger::StatusNotification,
            connector_id: Some(2),
        };
        let json = serde_json::to_string(&with_connector).unwrap();
        assert!(json.contains("connectorId"));
        let deserialized: TriggerMessageRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(with_connector, deserialized);

        let response = TriggerMessageResponse {
            status: TriggerMessageStatus::Accepted,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: TriggerMessageResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_get_local_list_version() {
        let request = GetLocalListVersionRequest {};
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, "{}");
        let deserialized: GetLocalListVersionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let response = GetLocalListVersionResponse { list_version: 5 };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("listVersion"));
        assert!(json.contains('5'));
        let deserialized: GetLocalListVersionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_send_local_list_request() {
        use ocpp_types::common::{AuthorizationStatus, IdTagInfo};

        let request = SendLocalListRequest {
            list_version: 3,
            update_type: UpdateType::Full,
            local_authorization_list: vec![
                AuthorizationData {
                    id_tag: "RFID001".to_string(),
                    id_tag_info: Some(IdTagInfo {
                        status: AuthorizationStatus::Accepted,
                        parent_id_tag: None,
                        expiry_date: None,
                    }),
                },
                AuthorizationData {
                    id_tag: "RFID002".to_string(),
                    id_tag_info: None,
                },
            ],
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("listVersion"));
        assert!(json.contains("updateType"));
        assert!(json.contains("localAuthorizationList"));
        assert!(json.contains("RFID001"));
        let deserialized: SendLocalListRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);

        let empty_full = SendLocalListRequest {
            list_version: 1,
            update_type: UpdateType::Full,
            local_authorization_list: vec![],
        };
        let json = serde_json::to_string(&empty_full).unwrap();
        let deserialized: SendLocalListRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(empty_full, deserialized);

        let response = SendLocalListResponse {
            status: UpdateStatus::Accepted,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: SendLocalListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_new_action_names() {
        assert_eq!(SetChargingProfileRequest::ACTION_NAME, "SetChargingProfile");
        assert_eq!(
            ClearChargingProfileRequest::ACTION_NAME,
            "ClearChargingProfile"
        );
        assert_eq!(
            GetCompositeScheduleRequest::ACTION_NAME,
            "GetCompositeSchedule"
        );
        assert_eq!(ReserveNowRequest::ACTION_NAME, "ReserveNow");
        assert_eq!(CancelReservationRequest::ACTION_NAME, "CancelReservation");
        assert_eq!(TriggerMessageRequest::ACTION_NAME, "TriggerMessage");
        assert_eq!(
            GetLocalListVersionRequest::ACTION_NAME,
            "GetLocalListVersion"
        );
        assert_eq!(SendLocalListRequest::ACTION_NAME, "SendLocalList");
    }

    // ----- Security Extension Tests -----

    #[test]
    fn test_certificate_signed() {
        let req = CertificateSignedRequest {
            certificate_chain:
                "-----BEGIN CERTIFICATE-----\nMIIBIjAN...\n-----END CERTIFICATE-----".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("certificateChain"));
        let d: CertificateSignedRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = CertificateSignedResponse {
            status: CertificateSignedStatus::Accepted,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"Accepted"}"#);
        let d: CertificateSignedResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, d);
    }

    #[test]
    fn test_delete_certificate() {
        let req = DeleteCertificateRequest {
            certificate_hash_data: CertificateHashData {
                hash_algorithm: HashAlgorithmType::Sha256,
                issuer_name_hash: "aabb".to_string(),
                issuer_key_hash: "ccdd".to_string(),
                serial_number: "0001".to_string(),
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("certificateHashData"));
        let d: DeleteCertificateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = DeleteCertificateResponse {
            status: DeleteCertificateStatus::Accepted,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let d: DeleteCertificateResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, d);
    }

    #[test]
    fn test_extended_trigger_message() {
        let req = ExtendedTriggerMessageRequest {
            requested_message: MessageTrigger::Heartbeat,
            connector_id: Some(1),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("requestedMessage"));
        assert!(json.contains("connectorId"));
        let d: ExtendedTriggerMessageRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let no_connector = ExtendedTriggerMessageRequest {
            requested_message: MessageTrigger::BootNotification,
            connector_id: None,
        };
        let json = serde_json::to_string(&no_connector).unwrap();
        assert!(!json.contains("connectorId"));

        let resp = ExtendedTriggerMessageResponse {
            status: TriggerMessageStatus::Accepted,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let d: ExtendedTriggerMessageResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, d);
    }

    #[test]
    fn test_get_installed_certificate_ids() {
        let req = GetInstalledCertificateIdsRequest {
            certificate_type: CertificateUse::CentralSystemRootCertificate,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("certificateType"));
        assert!(json.contains("CentralSystemRootCertificate"));
        let d: GetInstalledCertificateIdsRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp_with_chain = GetInstalledCertificateIdsResponse {
            status: GetInstalledCertificatesStatus::Accepted,
            certificate_hash_data_chain: Some(vec![CertificateHashData {
                hash_algorithm: HashAlgorithmType::Sha256,
                issuer_name_hash: "h1".to_string(),
                issuer_key_hash: "h2".to_string(),
                serial_number: "01".to_string(),
            }]),
        };
        let json = serde_json::to_string(&resp_with_chain).unwrap();
        assert!(json.contains("certificateHashDataChain"));
        let d: GetInstalledCertificateIdsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp_with_chain, d);

        let resp_not_found = GetInstalledCertificateIdsResponse {
            status: GetInstalledCertificatesStatus::NotFound,
            certificate_hash_data_chain: None,
        };
        let json = serde_json::to_string(&resp_not_found).unwrap();
        assert!(!json.contains("certificateHashDataChain"));
        let d: GetInstalledCertificateIdsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp_not_found, d);
    }

    #[test]
    fn test_get_log() {
        let req = GetLogRequest {
            log_type: LogType::SecurityLog,
            request_id: 42,
            log: LogParameters {
                remote_location: "ftp://logs.example.com/".to_string(),
                oldest_timestamp: None,
                latest_timestamp: None,
            },
            retries: Some(3),
            retry_interval: Some(60),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("logType"));
        assert!(json.contains("requestId"));
        assert!(json.contains("retries"));
        assert!(json.contains("retryInterval"));
        let d: GetLogRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = GetLogResponse {
            status: UploadLogStatus::Uploading,
            filename: Some("security.log".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("filename"));
        let d: GetLogResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, d);

        let resp_no_file = GetLogResponse {
            status: UploadLogStatus::Idle,
            filename: None,
        };
        let json = serde_json::to_string(&resp_no_file).unwrap();
        assert!(!json.contains("filename"));
    }

    #[test]
    fn test_install_certificate() {
        let req = InstallCertificateRequest {
            certificate_type: CertificateUse::ManufacturerRootCertificate,
            certificate: "-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("certificateType"));
        assert!(json.contains("ManufacturerRootCertificate"));
        let d: InstallCertificateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = InstallCertificateResponse {
            status: InstallCertificateStatus::Accepted,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let d: InstallCertificateResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, d);
    }

    #[test]
    fn test_log_status_notification() {
        let req = LogStatusNotificationRequest {
            status: UploadLogStatus::Uploaded,
            request_id: 7,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("requestId"));
        let d: LogStatusNotificationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = LogStatusNotificationResponse {};
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_security_event_notification() {
        let req = SecurityEventNotificationRequest {
            event_type: "TamperingDetected".to_string(),
            timestamp: DateTime::from_timestamp(1640995200, 0).unwrap(),
            tech_info: Some("Connector 1 tamper switch triggered".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\""));
        assert!(json.contains("techInfo"));
        let d: SecurityEventNotificationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let no_info = SecurityEventNotificationRequest {
            event_type: "InvalidFirmwareSignature".to_string(),
            timestamp: DateTime::from_timestamp(1640995200, 0).unwrap(),
            tech_info: None,
        };
        let json = serde_json::to_string(&no_info).unwrap();
        assert!(!json.contains("techInfo"));

        let resp = SecurityEventNotificationResponse {};
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_sign_certificate() {
        let req = SignCertificateRequest {
            csr: "-----BEGIN CERTIFICATE REQUEST-----\n...\n-----END CERTIFICATE REQUEST-----"
                .to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("csr"));
        let d: SignCertificateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = SignCertificateResponse {
            status: GenericStatus::Accepted,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"Accepted"}"#);
        let d: SignCertificateResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, d);
    }

    #[test]
    fn test_signed_firmware_status_notification() {
        let req = SignedFirmwareStatusNotificationRequest {
            status: FirmwareStatus::SignatureError,
            request_id: 99,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("SignatureError"));
        assert!(json.contains("requestId"));
        let d: SignedFirmwareStatusNotificationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = SignedFirmwareStatusNotificationResponse {};
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_signed_update_firmware() {
        let req = SignedUpdateFirmwareRequest {
            request_id: 1,
            firmware: FirmwareType {
                location: "https://fw.example.com/v2.bin".to_string(),
                retrieve_date_time: DateTime::from_timestamp(1640995200, 0).unwrap(),
                signing_certificate: "-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----"
                    .to_string(),
                install_date_time: None,
                signature: Some("base64sig==".to_string()),
            },
            retries: None,
            retry_interval: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("requestId"));
        assert!(json.contains("firmware"));
        assert!(json.contains("retrieveDateTime"));
        assert!(json.contains("signingCertificate"));
        assert!(!json.contains("retries"));
        assert!(!json.contains("retryInterval"));
        let d: SignedUpdateFirmwareRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, d);

        let resp = SignedUpdateFirmwareResponse {
            status: UpdateFirmwareStatus::Accepted,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"Accepted"}"#);
        let d: SignedUpdateFirmwareResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, d);
    }

    #[test]
    fn test_security_action_names() {
        assert_eq!(CertificateSignedRequest::ACTION_NAME, "CertificateSigned");
        assert_eq!(DeleteCertificateRequest::ACTION_NAME, "DeleteCertificate");
        assert_eq!(
            ExtendedTriggerMessageRequest::ACTION_NAME,
            "ExtendedTriggerMessage"
        );
        assert_eq!(
            GetInstalledCertificateIdsRequest::ACTION_NAME,
            "GetInstalledCertificateIds"
        );
        assert_eq!(GetLogRequest::ACTION_NAME, "GetLog");
        assert_eq!(InstallCertificateRequest::ACTION_NAME, "InstallCertificate");
        assert_eq!(
            LogStatusNotificationRequest::ACTION_NAME,
            "LogStatusNotification"
        );
        assert_eq!(
            SecurityEventNotificationRequest::ACTION_NAME,
            "SecurityEventNotification"
        );
        assert_eq!(SignCertificateRequest::ACTION_NAME, "SignCertificate");
        assert_eq!(
            SignedFirmwareStatusNotificationRequest::ACTION_NAME,
            "SignedFirmwareStatusNotification"
        );
        assert_eq!(
            SignedUpdateFirmwareRequest::ACTION_NAME,
            "SignedUpdateFirmware"
        );
    }
}
