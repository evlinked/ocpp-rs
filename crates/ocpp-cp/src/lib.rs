//! # OCPP Charge Point Implementation
//!
//! This crate provides a comprehensive charge point implementation that supports:
//! - Full connector state management with all OCPP 1.6J states
//! - Transaction lifecycle management
//! - Status notifications and meter values
//! - WebSocket connection to Central System
//! - Real-world charging scenarios simulation

pub mod auth_cache;
pub mod charging_profiles;
pub mod composite;
pub mod connector;
pub mod data_transfer;
pub mod error;
pub mod local_list;
pub mod message_handler;
pub mod meter_sampler;
pub mod state_machine;
pub mod transaction;
pub mod v201_certificate_store;
pub mod v201_charging_profiles;
pub mod v201_command;
pub mod v201_cost;
pub mod v201_customer_information;
pub mod v201_data_transfer;
pub mod v201_device_model;
pub mod v201_display_message;
pub mod v201_firmware_update;
pub mod v201_log_upload;
pub mod v201_network_profile;
pub mod v201_publish_firmware;
pub mod v201_station_ceiling;
pub mod v201_transaction;
pub mod v201_tx_default_profile;

use anyhow::Result;
use auth_cache::AuthCache;
use charging_profiles::ChargingProfileStore;
use connector::{Connector, ConnectorConfig};
use data_transfer::DataTransferRegistry;
use error::ChargePointError;
use local_list::LocalAuthList;
use message_handler::ConfigurationStore;
use ocpp_messages::v16j::{
    AuthorizeRequest, BootNotificationRequest, BootNotificationResponse, CancelReservationRequest,
    CancelReservationResponse, ChangeAvailabilityRequest, ChangeAvailabilityResponse,
    ChangeConfigurationRequest, ChangeConfigurationResponse, ClearCacheRequest, ClearCacheResponse,
    ClearChargingProfileRequest, ClearChargingProfileResponse, DataTransferRequest,
    DataTransferResponse, DiagnosticsStatusNotificationRequest, FirmwareStatusNotificationRequest,
    GetCompositeScheduleRequest, GetCompositeScheduleResponse, GetConfigurationRequest,
    GetConfigurationResponse, GetDiagnosticsRequest, GetDiagnosticsResponse,
    GetLocalListVersionRequest, GetLocalListVersionResponse, HeartbeatRequest, MeterValuesRequest,
    RegistrationStatus, RemoteStartTransactionRequest, RemoteStartTransactionResponse,
    RemoteStopTransactionRequest, RemoteStopTransactionResponse, ReserveNowRequest,
    ReserveNowResponse, ResetRequest, ResetResponse, SendLocalListRequest, SendLocalListResponse,
    SetChargingProfileRequest, SetChargingProfileResponse, StartTransactionRequest,
    StatusNotificationRequest, StopTransactionRequest, TriggerMessageRequest,
    TriggerMessageResponse, UnlockConnectorRequest, UnlockConnectorResponse, UpdateFirmwareRequest,
    UpdateFirmwareResponse,
};
use ocpp_messages::{
    ActionDispatcher, CallMessage, Message, MessageType, OcppAction, SchemaValidator,
};
use ocpp_transport::client::WebSocketClient;
use ocpp_transport::{
    MessageHandler as TransportMessageHandler, Transport, TransportConfig, TransportEvent,
};
use ocpp_types::common::{
    AuthorizationStatus, AvailabilityStatus, IdTagInfo, KeyValue, Measurand, ReadingContext, Reason,
};
use ocpp_types::v16j::{
    CancelReservationStatus, ChargePointErrorCode, ChargePointStatus, ChargePointVendorInfo,
    ChargingProfileStatus, ClearCacheStatus, ConfigurationStatus, DiagnosticsStatus,
    FirmwareStatus, GetCompositeScheduleStatus, MessageTrigger, RemoteStartStopStatus,
    ReservationStatus, ResetStatus, ResetType, TriggerMessageStatus, UnlockStatus,
};
use ocpp_types::{
    CallErrorCode, CallErrorMessage, CallResultMessage, ConnectorId, OcppError, OcppResult,
    OcppVersion,
};
use v201_certificate_store::V201CertificateStore;
use v201_charging_profiles::V201TxProfileStore;
use v201_cost::V201CostStore;
use v201_customer_information::V201CustomerInformationStore;
use v201_device_model::V201DeviceModel;
use v201_display_message::V201DisplayMessageStore;
use v201_firmware_update::V201FirmwareUpdateStore;
use v201_log_upload::V201LogUploadStore;
use v201_network_profile::V201NetworkProfileStore;
use v201_publish_firmware::V201PublishFirmwareStore;
use v201_station_ceiling::{CeilingKind, V201StationCeilingStore};
use v201_tx_default_profile::V201TxDefaultProfileStore;
// 2.0.1 provisioning message + enum types used by the version-aware runtime
// (slice 2). Aliased to avoid clashing with the unqualified 1.6J names imported
// above (`StatusNotificationRequest`, `RegistrationStatus`).
use ocpp_messages::v201::{
    CancelReservationRequest as V201CancelReservationRequest,
    CertificateSignedRequest as V201CertificateSignedRequest,
    ChangeAvailabilityRequest as V201ChangeAvailabilityRequest,
    ClearCacheRequest as V201ClearCacheRequest, ClearCacheResponse as V201ClearCacheResponse,
    ClearChargingProfileRequest as V201ClearChargingProfileRequest,
    ClearDisplayMessageRequest as V201ClearDisplayMessageRequest,
    ClearVariableMonitoringRequest as V201ClearVariableMonitoringRequest,
    ClearVariableMonitoringResponse as V201ClearVariableMonitoringResponse,
    CostUpdatedRequest as V201CostUpdatedRequest,
    CustomerInformationRequest as V201CustomerInformationRequest,
    DataTransferRequest as V201DataTransferRequest,
    DeleteCertificateRequest as V201DeleteCertificateRequest,
    GetBaseReportRequest as V201GetBaseReportRequest,
    GetBaseReportResponse as V201GetBaseReportResponse,
    GetChargingProfilesRequest as V201GetChargingProfilesRequest,
    GetCompositeScheduleRequest as V201GetCompositeScheduleRequest,
    GetCompositeScheduleResponse as V201GetCompositeScheduleResponse,
    GetDisplayMessagesRequest as V201GetDisplayMessagesRequest,
    GetInstalledCertificateIdsRequest as V201GetInstalledCertificateIdsRequest,
    GetLocalListVersionRequest as V201GetLocalListVersionRequest,
    GetLocalListVersionResponse as V201GetLocalListVersionResponse,
    GetLogRequest as V201GetLogRequest,
    GetMonitoringReportRequest as V201GetMonitoringReportRequest,
    GetReportRequest as V201GetReportRequest, GetReportResponse as V201GetReportResponse,
    GetTransactionStatusRequest as V201GetTransactionStatusRequest,
    GetVariablesRequest as V201GetVariablesRequest,
    GetVariablesResponse as V201GetVariablesResponse,
    InstallCertificateRequest as V201InstallCertificateRequest,
    MeterValuesRequest as V201MeterValuesRequest,
    NotifyMonitoringReportRequest as V201NotifyMonitoringReportRequest,
    NotifyReportRequest as V201NotifyReportRequest,
    PublishFirmwareRequest as V201PublishFirmwareRequest,
    RequestStartTransactionRequest as V201RequestStartTransactionRequest,
    RequestStopTransactionRequest as V201RequestStopTransactionRequest,
    ReserveNowRequest as V201ReserveNowRequest, ResetRequest as V201ResetRequest,
    SendLocalListRequest as V201SendLocalListRequest,
    SendLocalListResponse as V201SendLocalListResponse,
    SetChargingProfileRequest as V201SetChargingProfileRequest,
    SetDisplayMessageRequest as V201SetDisplayMessageRequest,
    SetMonitoringBaseRequest as V201SetMonitoringBaseRequest,
    SetMonitoringLevelRequest as V201SetMonitoringLevelRequest,
    SetNetworkProfileRequest as V201SetNetworkProfileRequest,
    SetVariableMonitoringRequest as V201SetVariableMonitoringRequest,
    SetVariableMonitoringResponse as V201SetVariableMonitoringResponse,
    SetVariablesRequest as V201SetVariablesRequest,
    SetVariablesResponse as V201SetVariablesResponse,
    StatusNotificationRequest as V201StatusNotificationRequest,
    TriggerMessageRequest as V201TriggerMessageRequest,
    UnlockConnectorRequest as V201UnlockConnectorRequest,
    UpdateFirmwareRequest as V201UpdateFirmwareRequest,
};
use ocpp_types::v201::{
    AttributeEnumType, AuthorizationStatusEnumType, CertificateSignedStatusEnumType,
    ChangeAvailabilityStatusEnumType, ChargingProfilePurposeEnumType,
    ChargingProfileStatusEnumType, ChargingProfileType, ConnectorStatusEnumType,
    CustomerInformationStatusEnumType, DeleteCertificateStatusEnumType,
    DisplayMessageStatusEnumType, FirmwareStatusEnumType, GenericDeviceModelStatusEnumType,
    GenericStatusEnumType, GetVariableResultType, IdTokenType as V201IdTokenType,
    InstallCertificateStatusEnumType, InstallCertificateUseEnumType, LogStatusEnumType,
    MessageInfoType, MessageTriggerEnumType, MonitoringDataType, NetworkConnectionProfileType,
    OperationalStatusEnumType, PublishFirmwareStatusEnumType, RegistrationStatusEnumType,
    ReportDataType, RequestStartStopStatusEnumType, ReservationUpdateStatusEnumType,
    ReserveNowStatusEnumType, ResetStatusEnumType, SetNetworkProfileStatusEnumType,
    SetVariableResultType, StatusInfoType, TriggerMessageStatusEnumType, UnlockStatusEnumType,
    UpdateFirmwareStatusEnumType, UploadLogStatusEnumType,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;
use v201_command::ConnectorReservState;

/// Opt-in outcome of the simulated firmware update (`UpdateFirmware`, OCPP
/// 1.6J §4.x and the OCPP 2.0.1 `FirmwareStatusNotification` flow, #534).
/// Selects which branch of the firmware state machine the simulator follows so
/// a CSMS can be tested against failed rollouts, not just successful ones.
///
/// Firmware updates have two distinct failure points — the download phase and
/// the install phase — so unlike the single-failure diagnostics upload
/// ([`ChargePointConfig::diagnostics_upload_should_fail`]) this is an enum
/// rather than a `bool`. The terminal/resting statuses (`DownloadFailed`,
/// `InstallationFailed`) are the faithful 1.6J `FirmwareStatus` values and are
/// retained by the CP so a subsequent
/// `TriggerMessage(FirmwareStatusNotification)` reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FirmwareUpdateOutcome {
    /// Happy path: `Downloading → Downloaded → Installing → Installed`.
    #[default]
    Succeed,
    /// Download phase fails: `Downloading → DownloadFailed` (no install).
    DownloadFailed,
    /// Install phase fails: `Downloading → Downloaded → Installing →
    /// InstallationFailed`.
    InstallationFailed,
}

/// Opt-in outcome of `UnlockConnector` (OCPP 1.6J §5.21), modeling whether this
/// charge point's connector locks are controllable. Lets a CSMS / back office be
/// exercised against all three `UnlockStatus` values, not just the happy path.
///
/// Global to the CP — every connector behaves the same; a per-connector lock
/// capability is a future refinement. The happy path
/// ([`UnlockConnectorOutcome::Unlock`]) is the default so existing behavior is
/// unchanged. Independent of the unknown/out-of-range-connector case, which is
/// always `UnlockFailed` regardless of this knob (the CP cannot unlock a
/// connector it does not have).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UnlockConnectorOutcome {
    /// Happy path: the lock is controllable. A valid connector unlocks
    /// (`Unlocked`); any active transaction on it is stopped first (reason
    /// `UnlockCommand`) and the connector freed.
    #[default]
    Unlock,
    /// Mechanical fault: the lock will not release → `UnlockFailed`.
    UnlockFailed,
    /// The connector has no controllable lock → `NotSupported`.
    NotSupported,
}

/// Default OCPP version for a Charge Point config — 1.6J, the version the
/// simulator's runtime speaks today. Used by `#[serde(default)]` on
/// [`ChargePointConfig::protocol_version`] so older serialized configs load.
fn default_protocol_version() -> OcppVersion {
    OcppVersion::V16J
}

/// The WebSocket subprotocol identifier(s) a Charge Point speaking `version`
/// offers to the CSMS in the `Sec-WebSocket-Protocol` handshake header.
///
/// Ports the `subprotocols=[…]` argument passed to `websockets.connect` in the
/// mobilityhouse/ocpp reference client examples: `examples/v16/charge_point.py`
/// offers `ocpp1.6`, `examples/v201/charge_point.py` offers `ocpp2.0.1`. A CP
/// offers exactly the one version it speaks (unlike the CSMS/server side, which
/// may *accept* several); this keeps the negotiated protocol honest.
pub fn subprotocols_for(version: OcppVersion) -> Vec<String> {
    match version {
        OcppVersion::V16J => vec!["ocpp1.6".to_string()],
        OcppVersion::V201 => vec!["ocpp2.0.1".to_string()],
    }
}

/// Normalize a 2.0.1 `RegistrationStatusEnumType` onto the 1.6J
/// [`RegistrationStatus`] the runtime uses as its canonical registration state.
///
/// Both enums carry exactly `Accepted` / `Pending` / `Rejected` (the 2.0.1 set
/// is unchanged from 1.6J), so this is a total, lossless 1:1 mapping. It lets
/// [`ChargePoint::boot_sequence`] drive one retry loop over both versions
/// instead of duplicating the Accepted/Pending/Rejected control flow.
fn v201_registration_status_to_canonical(status: RegistrationStatusEnumType) -> RegistrationStatus {
    match status {
        RegistrationStatusEnumType::Accepted => RegistrationStatus::Accepted,
        RegistrationStatusEnumType::Pending => RegistrationStatus::Pending,
        RegistrationStatusEnumType::Rejected => RegistrationStatus::Rejected,
    }
}

/// Map a 1.6J [`ChargePointStatus`] onto the reduced 2.0.1
/// [`ConnectorStatusEnumType`] used in a 2.0.1 `StatusNotification`.
///
/// 2.0.1 collapses the 1.6J connector-status set (9 values) down to 5
/// (`Available`, `Occupied`, `Reserved`, `Unavailable`, `Faulted`): the four
/// "a vehicle is connected and a session is in some phase" states
/// (`Preparing`, `Charging`, `SuspendedEV`, `SuspendedEVSE`) plus `Finishing`
/// all report as `Occupied`. `Available` / `Reserved` / `Faulted` /
/// `Unavailable` carry across unchanged. This is a total mapping, so the 2.0.1
/// `StatusNotification` path can report any status the 1.6J connector model
/// tracks.
fn charge_point_status_to_v201(status: ChargePointStatus) -> ConnectorStatusEnumType {
    match status {
        ChargePointStatus::Available => ConnectorStatusEnumType::Available,
        ChargePointStatus::Preparing
        | ChargePointStatus::Charging
        | ChargePointStatus::SuspendedEV
        | ChargePointStatus::SuspendedEVSE
        | ChargePointStatus::Finishing => ConnectorStatusEnumType::Occupied,
        ChargePointStatus::Reserved => ConnectorStatusEnumType::Reserved,
        ChargePointStatus::Faulted => ConnectorStatusEnumType::Faulted,
        ChargePointStatus::Unavailable => ConnectorStatusEnumType::Unavailable,
    }
}

/// Map a 2.0.1 [`OperationalStatusEnumType`] onto the connector
/// [`ChargePointStatus`] the simulator applies when it carries out a
/// `ChangeAvailability` (slice 6b): `Operative → Available`, `Inoperative →
/// Unavailable`. The simulator has no separate operative-state field — the
/// connector's own status *is* its operative state, and an inoperative connector
/// reports `Unavailable` — so this is the single point that turns a 2.0.1
/// availability target into the state the CP sets and announces.
///
/// Total over both `OperationalStatusEnumType` variants, so a future spec-added
/// operational status is a compile error here rather than a silent default.
fn operational_status_to_cp_status(target: OperationalStatusEnumType) -> ChargePointStatus {
    match target {
        OperationalStatusEnumType::Operative => ChargePointStatus::Available,
        OperationalStatusEnumType::Inoperative => ChargePointStatus::Unavailable,
    }
}

/// The current time as an RFC 3339 / ISO 8601 string, the wire form the 2.0.1
/// `TransactionEvent` (and `StatusNotification`) timestamps use. Centralized so
/// every 2.0.1 event stamps its `timestamp` the same way the schema's
/// `date-time` format expects.
fn v201_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Version-normalized view of a `BootNotification` response, so
/// [`ChargePoint::boot_sequence`]'s retry loop is written once and works for
/// both 1.6J and 2.0.1. The 1.6J and 2.0.1 responses carry the same three
/// load-bearing fields (`status`, `interval`, `currentTime`); this is the
/// common shape the loop reads.
struct BootOutcome {
    /// Registration decision, normalized onto the canonical 1.6J enum.
    status: RegistrationStatus,
    /// Heartbeat interval (Accepted/Pending) or minimum retry wait (Rejected).
    interval: i32,
    /// The CSMS's current time, reported to consumers on `Accepted`.
    current_time: chrono::DateTime<chrono::Utc>,
}

/// Charge point configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChargePointConfig {
    /// Charge point identifier
    pub charge_point_id: String,
    /// Central system WebSocket URL
    pub central_system_url: String,
    /// OCPP protocol version the Charge Point speaks. Selects which WebSocket
    /// subprotocol is offered in the handshake (see [`subprotocols_for`]) and
    /// which provisioning payloads are built (see
    /// [`ChargePointConfig::v201_boot_notification_request`]).
    ///
    /// Defaults to [`OcppVersion::V16J`], which leaves all existing behavior
    /// unchanged. **Note:** the live message loop (boot handshake, heartbeat,
    /// `StatusNotification`, transactions) currently speaks 1.6J regardless of
    /// this field — end-to-end 2.0.1 runtime wiring is the slice-2 follow-up.
    /// Setting [`OcppVersion::V201`] today changes the *offered subprotocol* to
    /// `ocpp2.0.1` and unlocks the 2.0.1 provisioning builders, but does not yet
    /// re-route the runtime; treat it as opt-in to an in-progress feature.
    ///
    /// `#[serde(default)]` so a persisted config predating this field still
    /// deserializes (to [`OcppVersion::V16J`]) rather than failing to load.
    #[serde(default = "default_protocol_version")]
    pub protocol_version: OcppVersion,
    /// Charge point vendor information
    pub vendor_info: ChargePointVendorInfo,
    /// Number of connectors
    pub connector_count: u32,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Meter values sample interval in seconds
    pub meter_values_interval: u64,
    /// Measurands sampled in each periodic `MeterValues` frame during a
    /// transaction. Defaults to `[Energy.Active.Import.Register]`, matching the
    /// OCPP default for the `MeterValuesSampledData` configuration key.
    pub meter_value_measurands: Vec<Measurand>,
    /// Connection retry interval in seconds
    pub connection_retry_interval: u64,
    /// Maximum connection retry attempts
    pub max_connection_retries: u32,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    /// Timeout for individual OCPP CALL/CALLRESULT round-trips in seconds.
    /// Matches the Python reference default of 30 s (charge_point.py).
    pub call_timeout: u64,
    /// Maximum BootNotification retries before returning `OcppError::BootRejected`.
    /// Mirrors the retry loop in `charge_point.py`; default 3.
    pub max_boot_retries: u32,
    /// Validate every incoming CALL and outgoing CALLRESULT against the bundled
    /// OCPP 1.6J JSON Schemas before dispatch/deserialization. Defaults to
    /// `true`, matching the Python reference which always runs `_validate()`.
    /// Set to `false` to opt out (e.g. for fuzzing or non-conformant peers).
    pub validate_payloads: bool,
    /// Fallback time-to-live, in seconds, for authorization cache entries that
    /// the CSMS returns without an explicit `idTagInfo.expiryDate`. Defaults to
    /// 24 hours. See [`auth_cache::AuthCache`] and OCPP 1.6J §3.1.
    pub auth_cache_ttl: u64,
    /// When the CSMS is unreachable (an `Authorize` CALL times out), accept a
    /// stale-but-previously-`Accepted` cached entry instead of failing safe.
    /// Defaults to `false` (fail-safe: an unreachable CSMS yields `Invalid`).
    pub offline_auth_stale_ok: bool,
    /// Whether [`ChargePoint::authorize`] consults the CSMS-managed Local
    /// Authorization List before the Authorization Cache and a CSMS round-trip
    /// (OCPP 1.6J §4.1.3, the `LocalAuthListEnabled` standard configuration
    /// key). Defaults to `true`, matching the spec default and the fact that the
    /// CP implements the Local Authorization List Management profile. Set to
    /// `false` to make the list management-only (populated by `SendLocalList`
    /// but never used for offline authorization decisions).
    pub local_auth_list_enabled: bool,
    /// Fault injection: when `true`, the simulated diagnostics upload
    /// (`GetDiagnostics`, OCPP 1.6J §4.x) takes the failure branch —
    /// `Uploading → UploadFailed` instead of `Uploading → Uploaded` — so a
    /// CSMS / back office can be exercised against a diagnostics upload that
    /// fails, not just one that succeeds. Defaults to `false` (happy path);
    /// the failure path is strictly opt-in so existing behavior is unchanged.
    pub diagnostics_upload_should_fail: bool,
    /// Fault injection: when `true`, the simulated OCPP 2.0.1 log upload
    /// (`GetLog`, Part 2, security profile) takes the failure branch — its async
    /// `LogStatusNotification` stream closes `Uploading → UploadFailure` instead
    /// of `Uploading → Uploaded` — so a CSMS / back office can be exercised
    /// against a log upload that fails, not just one that succeeds. Defaults to
    /// `false` (happy path); the failure path is strictly opt-in so existing
    /// behavior is unchanged. A `GetLog` superseded by a newer request still
    /// reports `AcceptedCanceled` regardless of this flag — a canceled upload
    /// never "fails".
    pub log_upload_should_fail: bool,
    /// Fault injection for the simulated firmware update (`UpdateFirmware`) —
    /// shared by both the OCPP 1.6J §4.x and the OCPP 2.0.1 (Part 2, firmware
    /// management, Issue #534) `FirmwareStatusNotification` flows, since a given
    /// charge point speaks one protocol version at a time. Defaults to
    /// [`FirmwareUpdateOutcome::Succeed`] (happy path); set
    /// [`FirmwareUpdateOutcome::DownloadFailed`] or
    /// [`FirmwareUpdateOutcome::InstallationFailed`] to drive the simulator
    /// down the corresponding error branch, so a CSMS / back office can be
    /// exercised against a firmware rollout that fails, not just one that
    /// succeeds. The failure path is strictly opt-in so existing behavior is
    /// unchanged.
    pub firmware_update_outcome: FirmwareUpdateOutcome,
    /// Connector-lock behavior for `UnlockConnector` (OCPP 1.6J §5.21). Defaults
    /// to [`UnlockConnectorOutcome::Unlock`] (happy path: a valid connector
    /// unlocks, stopping any active transaction first). Set
    /// [`UnlockConnectorOutcome::NotSupported`] to model a connector with no
    /// controllable lock, or [`UnlockConnectorOutcome::UnlockFailed`] to model a
    /// mechanical unlock failure, so a CSMS can be exercised against all three
    /// `UnlockStatus` outcomes. The failure paths are strictly opt-in so existing
    /// behavior is unchanged.
    pub unlock_connector_outcome: UnlockConnectorOutcome,
    /// Maximum number of entries the CP's Local Authorization List may hold —
    /// the capacity enforced by [`local_list::LocalAuthList`] and reported for
    /// the read-only `LocalAuthListMaxLength` configuration key (OCPP 1.6J §9).
    /// A `SendLocalList` whose resulting list would exceed this is rejected with
    /// `UpdateStatus::Failed`. Defaults to
    /// [`local_list::DEFAULT_LOCAL_AUTH_LIST_MAX_LENGTH`].
    pub local_auth_list_max_length: usize,
    /// Transport configuration (not serialized; uses Default on deserialization)
    #[serde(skip)]
    pub transport_config: TransportConfig,
}

impl Default for ChargePointConfig {
    fn default() -> Self {
        Self {
            charge_point_id: "CP001".to_string(),
            central_system_url: "ws://localhost:8080".to_string(),
            protocol_version: OcppVersion::V16J,
            vendor_info: ChargePointVendorInfo {
                charge_point_vendor: "OCPP-RS".to_string(),
                charge_point_model: "Simulator".to_string(),
                charge_point_serial_number: Some("SIM001".to_string()),
                charge_box_serial_number: Some("CB001".to_string()),
                firmware_version: Some("1.0.0".to_string()),
                iccid: None,
                imsi: None,
                meter_type: Some("Energy".to_string()),
                meter_serial_number: Some("MT001".to_string()),
            },
            connector_count: 2,
            heartbeat_interval: 300,   // 5 minutes
            meter_values_interval: 60, // 1 minute
            meter_value_measurands: vec![Measurand::EnergyActiveImportRegister],
            connection_retry_interval: 30, // 30 seconds
            max_connection_retries: 10,
            auto_reconnect: true,
            call_timeout: 30, // 30 seconds, matches Python reference default
            max_boot_retries: 3,
            validate_payloads: true,
            auth_cache_ttl: 24 * 60 * 60, // 24 hours
            offline_auth_stale_ok: false,
            local_auth_list_enabled: true,
            diagnostics_upload_should_fail: false,
            log_upload_should_fail: false,
            firmware_update_outcome: FirmwareUpdateOutcome::Succeed,
            unlock_connector_outcome: UnlockConnectorOutcome::Unlock,
            local_auth_list_max_length: local_list::DEFAULT_LOCAL_AUTH_LIST_MAX_LENGTH,
            // Offer exactly the subprotocol matching `protocol_version` rather
            // than the transport default (which lists both `ocpp1.6` and
            // `ocpp2.0.1` for the *server* side) — a CP must only offer a
            // version it can actually speak, otherwise a 2.0.1-only CSMS could
            // negotiate a version this client does not talk. The offered set is
            // derived from `protocol_version` via `subprotocols_for`, so the
            // handshake stays honest as the CP learns 2.0.1.
            transport_config: TransportConfig {
                sub_protocols: subprotocols_for(OcppVersion::V16J),
                ..TransportConfig::default()
            },
        }
    }
}

impl ChargePointConfig {
    /// A [`Default`] configuration adjusted to speak `version`: sets both
    /// `protocol_version` and the offered `transport_config.sub_protocols`
    /// consistently from `version`, so the two never disagree.
    ///
    /// `ChargePointConfig::for_version(OcppVersion::V16J)` is exactly
    /// [`Default`]; `for_version(OcppVersion::V201)` additionally offers
    /// `ocpp2.0.1` in the handshake. Prefer this over setting `protocol_version`
    /// by hand, which would leave `sub_protocols` at the default `ocpp1.6`.
    pub fn for_version(version: OcppVersion) -> Self {
        let mut config = Self {
            protocol_version: version,
            ..Self::default()
        };
        config.transport_config.sub_protocols = subprotocols_for(version);
        config
    }

    /// Build the OCPP 2.0.1 `BootNotification` request this Charge Point sends
    /// to announce itself, derived from its configured identity.
    ///
    /// Faithful to the reference `examples/v201/charge_point.py`, which sends
    /// `BootNotification(charging_station={"model": …, "vendor_name": …},
    /// reason="PowerUp")`. The 1.6J [`ChargePointVendorInfo`] maps onto the
    /// 2.0.1 [`ChargingStationType`](ocpp_types::v201::ChargingStationType):
    /// `charge_point_vendor → vendorName`, `charge_point_model → model`, and the
    /// optional `charge_point_serial_number` / `firmware_version` carry across
    /// unchanged. `reason` is [`PowerUp`](ocpp_types::v201::BootReasonEnumType::PowerUp),
    /// matching a fresh-boot simulator.
    ///
    /// This is a pure builder: it constructs the payload but does not send it.
    /// Wiring it into the live boot handshake (so a `V201` CP actually
    /// round-trips this against a CSMS) is the slice-2 runtime follow-up.
    pub fn v201_boot_notification_request(&self) -> ocpp_messages::v201::BootNotificationRequest {
        use ocpp_types::v201::{BootReasonEnumType, ChargingStationType};

        let vendor = &self.vendor_info;
        ocpp_messages::v201::BootNotificationRequest {
            charging_station: ChargingStationType {
                vendor_name: vendor.charge_point_vendor.clone(),
                model: vendor.charge_point_model.clone(),
                serial_number: vendor.charge_point_serial_number.clone(),
                firmware_version: vendor.firmware_version.clone(),
                modem: None,
                custom_data: None,
            },
            reason: BootReasonEnumType::PowerUp,
            custom_data: None,
        }
    }
}

/// Charge point events
#[derive(Debug, Clone)]
pub enum ChargePointEvent {
    /// Charge point started
    Started,
    /// Connected to central system
    Connected,
    /// Disconnected from central system
    Disconnected { reason: String },
    /// Boot notification accepted
    BootNotificationAccepted {
        current_time: chrono::DateTime<chrono::Utc>,
        interval: i32,
    },
    /// Connector status changed
    ConnectorStatusChanged {
        connector_id: ConnectorId,
        old_status: ChargePointStatus,
        new_status: ChargePointStatus,
    },
    /// Transaction started
    TransactionStarted {
        connector_id: ConnectorId,
        transaction_id: i32,
        id_tag: String,
    },
    /// Transaction stopped
    TransactionStopped {
        connector_id: ConnectorId,
        transaction_id: i32,
        reason: String,
    },
    /// A CSMS-initiated `Reset` was accepted and is being carried out
    /// (OCPP 1.6J §5.13). Emitted by the command-consumer task as the reset
    /// side effect begins, after the `Reset` CALLRESULT has been returned.
    ResetRequested { reset_type: ResetType },
    /// Error occurred
    Error { error: ChargePointError },
}

/// Internal command queued by an inbound CALL handler for asynchronous,
/// out-of-band execution by the command-consumer task spawned in
/// [`ChargePoint::connect`].
///
/// The dispatcher's `@on` handlers are `'static` closures that cannot reach
/// `&ChargePoint`, and invoking a side effect inline would re-enter the
/// WebSocket (send an outbound CALL + await its CALLRESULT) while the receive
/// loop is mid-dispatch — a deadlock. Channelling the work decouples the side
/// effect from the response path so the CALLRESULT is flushed first.
#[derive(Debug, Clone)]
enum RemoteCommand {
    /// Perform a CSMS-initiated Reset (OCPP 1.6J §5.13).
    Reset { reset_type: ResetType },
    /// Drive the local `StartTransaction` for an `Accepted`
    /// `RemoteStartTransaction` (OCPP 1.6J §5.11) or `RequestStartTransaction`
    /// (OCPP 2.0.1 Part 2).
    ///
    /// `remote_start_id` carries the 2.0.1 `RequestStartTransaction.remoteStartId`
    /// so the started transaction's `TransactionEvent(Started)` can echo it for
    /// CSMS correlation. It is `None` on the 1.6J path (1.6J has no
    /// `remoteStartId` — its correlation is the synchronous `transactionId` in
    /// `RemoteStartTransaction.conf`).
    ///
    /// `charging_profile` carries the 2.0.1 `RequestStartTransaction.chargingProfile`
    /// (a validated `TxProfile`) to install against the started transaction, and
    /// `group_id_token` its optional `groupIdToken`, threaded onto the session's
    /// auth context (slice 7d, Issue #450). Both are `None` on the 1.6J path and
    /// whenever the request omitted them.
    StartTransaction {
        connector_id: ConnectorId,
        id_tag: String,
        remote_start_id: Option<i32>,
        // Boxed: a 2.0.1 `ChargingProfileType` is a large value (nested
        // schedules), and boxing it keeps this rarely-large command variant from
        // bloating every `RemoteCommand` (clippy::large_enum_variant).
        charging_profile: Option<Box<ChargingProfileType>>,
        group_id_token: Option<V201IdTokenType>,
    },
    /// End the matching transaction for an `Accepted` `RemoteStopTransaction`
    /// (OCPP 1.6J §5.12).
    StopTransaction { transaction_id: i32 },
    /// Send a CSMS-requested message proactively for an `Accepted`
    /// `TriggerMessage` (OCPP 1.6J §4.x). `connector_id` scopes connector-
    /// specific messages (e.g. `StatusNotification`); `None` means all
    /// connectors / not connector-specific.
    TriggerMessage {
        requested_message: MessageTrigger,
        connector_id: Option<i32>,
    },
    /// Send a CSMS-requested message proactively for an `Accepted` OCPP 2.0.1
    /// `TriggerMessage` (OCPP 2.0.1 Part 2). The 2.0.1 twin of
    /// [`TriggerMessage`](RemoteCommand::TriggerMessage): `evse_id` scopes
    /// EVSE-specific messages (`StatusNotification`, `MeterValues`,
    /// `TransactionEvent`) and `None` targets the whole Charging Station. Queued
    /// off the inbound-CALL path for the same reason as the 1.6J variant — the
    /// triggered outbound CALL must not re-enter the receive loop mid-dispatch.
    V201TriggerMessage {
        requested_message: MessageTriggerEnumType,
        evse_id: Option<i32>,
    },
    /// Stream the device-model report a CSMS asked for with an `Accepted` OCPP
    /// 2.0.1 `GetBaseReport` (Part 2). The synchronous `GetBaseReport.conf` only
    /// acknowledges; the actual inventory follows as a `NotifyReport` CALL,
    /// correlated back by `request_id`. `report_data` is the already-computed
    /// report snapshot (taken under the device-model read lock on the CALL path,
    /// so the send touches no shared state). Queued off the inbound-CALL path for
    /// the same reason as the other side effects — the outbound `NotifyReport`
    /// CALL must not re-enter the receive loop mid-dispatch.
    V201NotifyReport {
        request_id: i32,
        report_data: Vec<ReportDataType>,
    },
    /// Stream the *variable-monitoring* snapshot a CSMS asked for with an
    /// `Accepted` OCPP 2.0.1 `GetMonitoringReport` (Part 2, D07). The monitoring
    /// twin of [`V201NotifyReport`](Self::V201NotifyReport): the synchronous
    /// `GetMonitoringReport.conf` only acknowledges; the installed monitors follow
    /// as a `NotifyMonitoringReport` CALL, correlated back by `request_id`.
    /// `monitor_data` is the already-computed snapshot (taken under the
    /// device-model read lock on the CALL path, so the send touches no shared
    /// state). Queued off the inbound-CALL path for the same reason as the other
    /// side effects — the outbound `NotifyMonitoringReport` CALL must not re-enter
    /// the receive loop mid-dispatch.
    V201NotifyMonitoringReport {
        request_id: i32,
        monitor_data: Vec<MonitoringDataType>,
    },
    /// Stream the installed charging profiles a CSMS asked for with an `Accepted`
    /// OCPP 2.0.1 `GetChargingProfiles` (Part 2). The synchronous
    /// `GetChargingProfiles.conf` only reports `Accepted` / `NoProfiles`; the
    /// actual profiles follow as one or more `ReportChargingProfiles` CALLs,
    /// correlated back by `request_id`. `profiles` is the already-resolved
    /// `(evse_id, profile)` match set (snapshotted off the store lock on the CALL
    /// path, so the send touches no shared state), paged into per-EVSE CALLs by
    /// the consumer. Queued off the inbound-CALL path for the same reason as the
    /// other side effects — the outbound `ReportChargingProfiles` CALLs must not
    /// re-enter the receive loop mid-dispatch. Mirrors the `GetBaseReport →
    /// NotifyReport` seam (#462).
    V201ReportChargingProfiles {
        request_id: i32,
        profiles: Vec<(i32, ChargingProfileType)>,
    },
    /// Stream the installed display messages a CSMS asked for with an `Accepted`
    /// OCPP 2.0.1 `GetDisplayMessages` (Part 2, E05–E08). The synchronous
    /// `GetDisplayMessages.conf` only reports `Accepted` / `Unknown`; the actual
    /// messages follow as one or more `NotifyDisplayMessages` CALLs, correlated
    /// back by `request_id`. `messages` is the already-resolved match set
    /// (snapshotted off the display-message store lock on the CALL path, so the
    /// send touches no shared state), paged into per-message CALLs by the
    /// consumer. Queued off the inbound-CALL path for the same reason as the
    /// other side effects — the outbound `NotifyDisplayMessages` CALLs must not
    /// re-enter the receive loop mid-dispatch. The display-message twin of
    /// [`V201ReportChargingProfiles`](Self::V201ReportChargingProfiles).
    V201NotifyDisplayMessages {
        request_id: i32,
        messages: Vec<MessageInfoType>,
    },
    /// Run the simulated async log-upload state machine for an `Accepted` /
    /// `AcceptedCanceled` OCPP 2.0.1 `GetLog` (Part 2, security profile, Issue
    /// #526). The synchronous `GetLog.conf` only acks; the station then reports
    /// upload progress asynchronously via `LogStatusNotification.req`, correlated
    /// by `request_id` — this drives that stream (`Uploading` → a terminal
    /// `Uploaded` / `UploadFailure` / `AcceptedCanceled`) and, on settling as the
    /// still-current upload, clears the `V201LogUploadStore` back to idle. Queued
    /// off the inbound-CALL path for the same reason as the other side effects —
    /// the outbound `LogStatusNotification` CALLs must not re-enter the receive
    /// loop mid-dispatch. The `GetLog` twin of the 1.6J
    /// [`GetDiagnostics`](Self::GetDiagnostics) upload flow.
    V201LogUpload { request_id: i32 },
    /// Run the simulated async firmware-update state machine for an `Accepted`
    /// (fresh) / `AcceptedCanceled` (supersede) OCPP 2.0.1 `UpdateFirmware`
    /// (Part 2, firmware management, L01–L03, Issue #534). The synchronous
    /// `UpdateFirmware.conf` only acks; the station then reports fetch/install
    /// progress asynchronously via `FirmwareStatusNotification.req`, correlated
    /// by `request_id` — this drives that stream (`Downloading` → `Downloaded` →
    /// `Installing` → a terminal `Installed`, or the opt-in `DownloadFailed` /
    /// `InstallationFailed` fault branches) and, on settling as the still-current
    /// rollout, compare-and-clears the `V201FirmwareUpdateStore` back to idle.
    /// Queued off the inbound-CALL path for the same reason as the other side
    /// effects — the outbound `FirmwareStatusNotification` CALLs must not re-enter
    /// the receive loop mid-dispatch. The 2.0.1 twin of the 1.6J
    /// [`UpdateFirmware`](Self::UpdateFirmware) progress flow, and the firmware
    /// sibling of [`V201LogUpload`](Self::V201LogUpload).
    V201FirmwareUpdate { request_id: i32 },
    /// Stream the simulated customer-data report an `Accepted`
    /// `CustomerInformation(report: true)` asked for, as one or more
    /// `NotifyCustomerInformation.req` pages (OCPP 2.0.1 Part 2, N-series — data
    /// privacy / GDPR, Issue #537). The synchronous `CustomerInformation.conf`
    /// only acks; when the request set `report: true` and was accepted, the
    /// station then streams the stored data back asynchronously, correlated by
    /// `request_id` — this drives that paged stream (`seqNo` from 0, `tbc` on
    /// every page but the last) and, once it settles, clears the
    /// `V201CustomerInformationStore` in-flight marker for `request_id`. Queued
    /// off the inbound-CALL path for the same reason as the other side effects —
    /// the outbound `NotifyCustomerInformation` CALLs must not re-enter the
    /// receive loop mid-dispatch. The flat-text privacy twin of the
    /// [`V201NotifyReport`](Self::V201NotifyReport) paged carrier.
    V201CustomerInformationReport { request_id: i32 },
    /// Run the simulated async firmware-publish progress stream for an
    /// `Accepted` OCPP 2.0.1 `PublishFirmware` (Part 2, firmware management —
    /// the Local-Controller firmware-cache trigger, Issue #540). The synchronous
    /// `PublishFirmware.conf` only acks; the station then reports
    /// download/publish progress asynchronously via
    /// `PublishFirmwareStatusNotification.req`, correlated by `request_id` — this
    /// drives that stream (`Idle` → `DownloadScheduled` → `Downloading` →
    /// `Downloaded` → a terminal `Published` carrying the cached image's download
    /// `location` URIs) and, once it settles, clears the
    /// `V201PublishFirmwareStore` in-flight marker for `request_id`. Queued off
    /// the inbound-CALL path for the same reason as the other side effects — the
    /// outbound `PublishFirmwareStatusNotification` CALLs must not re-enter the
    /// receive loop mid-dispatch. The firmware-publish sibling of
    /// [`V201FirmwareUpdate`](Self::V201FirmwareUpdate).
    V201PublishFirmwareStatus { request_id: i32 },
    /// Report to the CSMS that a previously-held OCPP 2.0.1 reservation is no
    /// longer valid, via a `ReservationStatusUpdate.req` CALL (Part 2,
    /// reservation — the CP→CSMS half that closes the loop `ReserveNow` /
    /// `CancelReservation` open, Issue #546). Queued — never sent inline — for the
    /// same reason as the other side effects: the outbound CALL must not re-enter
    /// the receive loop mid-dispatch (`CancelReservation`) and must run off the
    /// auto-expiry timer task rather than blocking it (`Expired`).
    ///
    /// `status` is [`Expired`](ReservationUpdateStatusEnumType::Expired) when the
    /// reservation's `expiryDateTime` passed and the auto-expiry timer freed the
    /// slot, or [`Removed`](ReservationUpdateStatusEnumType::Removed) when a
    /// CSMS-initiated `CancelReservation` tore down a still-held reservation. Only
    /// ever queued on the `V201` arms — the message does not exist in 1.6J, so a
    /// 1.6J CP's reservation teardown queues none. There is no in-flight store to
    /// clear (a single fire-and-forget notification, not a stream), so a
    /// consumer-gone send simply drops best-effort like
    /// [`EmitConnectorStatus`](Self::EmitConnectorStatus).
    V201ReservationStatusUpdate {
        reservation_id: i32,
        status: ReservationUpdateStatusEnumType,
    },
    /// Run the simulated diagnostics-upload state machine for an `Accepted`
    /// `GetDiagnostics` (OCPP 1.6J §4.x, firmware-management profile). Emits
    /// `DiagnosticsStatusNotification(Uploading)` then `Uploaded`.
    GetDiagnostics,
    /// Run the simulated firmware-update state machine for an `Accepted`
    /// `UpdateFirmware` (OCPP 1.6J §4.x, firmware-management profile). Emits
    /// `FirmwareStatusNotification(Downloading)` → `Downloaded` → `Installing`
    /// → `Installed`.
    UpdateFirmware,
    /// Emit a `StatusNotification` CALL for a connector whose status changed as
    /// a local side effect of an inbound command — currently the `Reserved` /
    /// `Available` transitions behind an `Accepted` `ReserveNow` /
    /// `CancelReservation` (OCPP 1.6J §5.14/§5.4, Issue #80). Queued rather than
    /// sent inline for the same reason as the other side effects: the outbound
    /// CALL must not re-enter the receive loop mid-dispatch.
    EmitConnectorStatus {
        connector_id: ConnectorId,
        status: ChargePointStatus,
    },
    /// Stop the active transaction on a connector being unlocked by an `Accepted`
    /// `UnlockConnector` (OCPP 1.6J §5.21). Per the spec the CP stops an ongoing
    /// transaction (`StopTransaction`, reason `UnlockCommand`) before releasing
    /// the cable; `stop_transaction` also frees the connector (→ `Available`) and
    /// emits the `StatusNotification`. Queued off the inbound-CALL path like the
    /// other side effects so the `UnlockConnector` CALLRESULT is flushed before
    /// the outbound `StopTransaction` CALL (no receive-loop re-entrancy).
    UnlockConnector {
        connector_id: ConnectorId,
        transaction_id: i32,
    },
    /// Apply a 2.0.1 `ChangeAvailability` transition to a single connector as a
    /// real side effect (OCPP 2.0.1 Part 2, `ChangeAvailability`, slice 6b). Flips
    /// the connector's operative state (`Operative → Available`, `Inoperative →
    /// Unavailable`) and emits the reflecting `StatusNotification`. Enqueued off
    /// the inbound-CALL path — by the `Accepted` handler now, or by
    /// [`stop_transaction`](ChargePoint::stop_transaction) when a `Scheduled`
    /// change is carried out once the station goes idle — so the CALLRESULT is
    /// flushed before the outbound `StatusNotification` and the receive loop never
    /// re-enters itself. One command is enqueued per targeted connector (a
    /// whole-station change targets every connector).
    V201ApplyAvailability {
        connector_id: ConnectorId,
        target: OperationalStatusEnumType,
    },
}

/// A 2.0.1 `ChangeAvailability` accepted-but-deferred while a transaction was in
/// progress (slice 6b, Issue #436). Recorded by the V201 `ChangeAvailability`
/// handler when it answers [`Scheduled`](ChangeAvailabilityStatusEnumType::Scheduled)
/// and carried out by [`stop_transaction`](ChargePoint::stop_transaction) once
/// the station next goes idle — the availability twin of the deferred
/// `Reset(OnIdle)` slot [`pending_v201_reset`](ChargePoint::pending_v201_reset).
#[derive(Debug, Clone, Copy)]
struct PendingAvailability {
    /// Targeted EVSE id; `None` targets the whole Charging Station (every
    /// connector).
    evse_id: Option<i32>,
    /// The availability the station will apply once idle.
    target: OperationalStatusEnumType,
}

/// Whether this charge point can proactively produce `message` on a
/// `TriggerMessage` request (OCPP 1.6J §4.x).
///
/// `BootNotification`, `Heartbeat`, `StatusNotification`, on-demand
/// `MeterValues`, `DiagnosticsStatusNotification` (the CP tracks a
/// diagnostics-upload status from `GetDiagnostics`), and
/// `FirmwareStatusNotification` (the CP tracks a firmware-update status from
/// `UpdateFirmware`) are wired. The two `ExtendedTriggerMessage`-only triggers
/// (`LogStatusNotification`, `SignChargePointCertificate`) are recognized but
/// unimplemented by the simulator, so they report `NotImplemented`. The
/// `matches!` gate means any future variant likewise defaults to
/// `NotImplemented` until it grows a state machine.
fn trigger_message_supported(message: &MessageTrigger) -> bool {
    matches!(
        message,
        MessageTrigger::BootNotification
            | MessageTrigger::Heartbeat
            | MessageTrigger::StatusNotification
            | MessageTrigger::MeterValues
            | MessageTrigger::DiagnosticsStatusNotification
            | MessageTrigger::FirmwareStatusNotification
    )
}

/// How long the simulated diagnostics upload "takes" between the
/// `Uploading` and `Uploaded` `DiagnosticsStatusNotification`s. Short so the
/// simulator stays responsive; the CP has no real archive to upload.
const DIAGNOSTICS_UPLOAD_DURATION: Duration = Duration::from_millis(200);

/// How long the simulated OCPP 2.0.1 log upload "takes" between the `Uploading`
/// and terminal `LogStatusNotification`s (`GetLog`, Part 2, security profile).
/// Short so the simulator stays responsive; the CP has no real archive to
/// upload. Mirrors [`DIAGNOSTICS_UPLOAD_DURATION`], the 1.6J analog.
const LOG_UPLOAD_DURATION: Duration = Duration::from_millis(200);

/// How long each simulated firmware-update step "takes" between consecutive
/// `FirmwareStatusNotification`s (`Downloading` → `Downloaded` → `Installing`
/// → `Installed`). Short so the simulator stays responsive; the CP has no real
/// firmware image to download or install.
const FIRMWARE_UPDATE_STEP_DURATION: Duration = Duration::from_millis(150);

/// Charge point event handler trait
#[async_trait::async_trait]
pub trait ChargePointEventHandler: Send + Sync {
    /// Handle charge point event
    async fn handle_event(&self, event: ChargePointEvent);
}

// ---------------------------------------------------------------------------
// Internal bridge: passed to WebSocketClient as the TransportMessageHandler.
// Holds clones of the shared state needed for dispatch and event handling.
// ---------------------------------------------------------------------------

struct CpHandler {
    dispatcher: Arc<RwLock<ActionDispatcher>>,
    event_sender: mpsc::UnboundedSender<ChargePointEvent>,
    is_connected: Arc<RwLock<bool>>,
}

#[async_trait::async_trait]
impl TransportMessageHandler for CpHandler {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        let call = match message {
            Message::Call(c) => c,
            // CALLRESULT and CALLERROR are resolved by PendingCallMap in the
            // transport recv loop before handle_message is ever invoked.
            Message::CallResult(_) | Message::CallError(_) => return Ok(None),
        };

        let unique_id = call.unique_id.clone();
        match self.dispatcher.read().await.dispatch(&call).await {
            Ok(payload) => {
                let result = CallResultMessage::new(unique_id, payload).map_err(|e| {
                    OcppError::Internal {
                        message: e.to_string(),
                    }
                })?;
                Ok(Some(Message::CallResult(result)))
            }
            Err(e) => {
                let (code, description) = ocpp_error_to_callerror_code(&e);
                let err_msg = CallErrorMessage::new(unique_id, code, description, None);
                Ok(Some(Message::CallError(err_msg)))
            }
        }
    }

    async fn handle_event(&self, event: TransportEvent) {
        match event {
            TransportEvent::Connected { .. } => {
                info!("Transport connected");
            }
            TransportEvent::Disconnected { reason, .. } => {
                warn!("Transport disconnected: {}", reason);
                *self.is_connected.write().await = false;
                let _ = self
                    .event_sender
                    .send(ChargePointEvent::Disconnected { reason });
            }
            TransportEvent::Error { error, .. } => {
                error!("Transport error: {}", error);
                let _ = self.event_sender.send(ChargePointEvent::Error {
                    error: ChargePointError::TransportError(error.to_string()),
                });
            }
            _ => {}
        }
    }
}

/// Convert an `OcppError` to the appropriate OCPP CALLERROR code + description.
fn ocpp_error_to_callerror_code(e: &OcppError) -> (CallErrorCode, String) {
    match e {
        OcppError::NotSupported { feature } => (
            CallErrorCode::NotSupported,
            format!("Action '{}' not supported", feature),
        ),
        // JSON-Schema failures carry the dominant failing keyword, mapped to a
        // keyword-granular CALLERROR code per `_validate_payload()` in
        // `ocpp/messages.py` (`type`/`maxLength` → TypeConstraintViolation,
        // `required` → ProtocolError, else → FormationViolation).
        OcppError::SchemaViolation {
            keyword, message, ..
        } => (keyword.call_error_code(), message.clone()),
        // Manual (non-schema) validation failures keep the default bucket.
        OcppError::ValidationError { message } => {
            (CallErrorCode::FormationViolation, message.clone())
        }
        OcppError::Json { message } => (CallErrorCode::FormationViolation, message.clone()),
        OcppError::ProtocolViolation { message } => (CallErrorCode::ProtocolError, message.clone()),
        _ => (CallErrorCode::InternalError, e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// ChargePoint
// ---------------------------------------------------------------------------

/// Per-transaction state a `V201` charge point carries across the three
/// `TransactionEvent`s of one charging session (slice 3b, Issue #423).
///
/// The 1.6J transactional loop is stateless between `StartTransaction` and
/// `StopTransaction` — the integer `transactionId` and the connector are all it
/// needs, and both live in [`ChargePoint::active_transactions`]. The unified
/// 2.0.1 `TransactionEvent` message needs two extra things that must stay
/// constant across `Started` / `Updated` / `Ended`:
///
/// * the authorizing `idToken` — echoed on the `Ended` event so a CSMS that
///   authorizes stops can match it, but not available from the 1.6J
///   `StopTransaction` inputs (which carry no `idTag`), so it is captured at
///   start; and
/// * a monotonic `seqNo` — shared between the periodic meter sampler (which
///   emits `Updated` events off a background task) and `stop_transaction`
///   (which emits the final `Ended`), so an `Arc<AtomicI32>` hands out
///   strictly increasing values with no coordination beyond the atomic.
#[derive(Debug, Clone)]
struct V201Session {
    /// The `idTag` that authorized the session (its 2.0.1 `idToken.idToken`).
    id_tag: String,
    /// Next `seqNo` to hand out. `Started` used `0`; the sampler and the
    /// `Ended` event each `fetch_add(1)` to claim the next value, so `seqNo`
    /// is strictly increasing across the whole transaction.
    next_seq_no: Arc<AtomicI32>,
    /// Whether the session's `idToken` is still authorized. `true` at start
    /// (a transaction only opens on an accepted authorization) and flipped to
    /// `false` by [`ChargePoint::deauthorize`] when the driver/app stops
    /// authorizing the session (the 2.0.1 deauthorization event).
    ///
    /// This is the "authorized-vs-stoppable" bit `UnlockConnector` consults:
    /// per OCPP 2.0.1 the station refuses to release the cable
    /// (`OngoingAuthorizedTransaction`) only while the session is *still
    /// authorized*; once deauthorized the cable may be released, stopping the
    /// transaction first (reason `UnlockCommand`). An explicit per-session flag
    /// — not the volatile auth cache — because a remotely-started transaction
    /// may hold no cache entry and cache *expiry* is not *deauthorization*.
    authorized: Arc<AtomicBool>,
    /// The optional `groupIdToken` a `RequestStartTransaction` carried — the
    /// parent/group token the driver `idToken` belongs to (a fleet card, a
    /// household account) that a CSMS may use to group or co-authorize sessions
    /// (OCPP 2.0.1 Part 2, `RequestStartTransaction.groupIdToken`).
    ///
    /// Captured onto the session's auth context at start (slice 7d, Issue #450)
    /// so it travels with the live transaction rather than being read off the
    /// wire and dropped. `None` for a locally-started transaction and whenever
    /// the request omitted it. Read back via
    /// [`ChargePoint::transaction_group_id_token`].
    group_id_token: Option<V201IdTokenType>,
}

/// Main charge point implementation
///
/// `ChargePoint` is cheap to [`Clone`]: every field is either an [`Arc`] over
/// shared state or itself trivially cloneable, so a clone is a new handle onto
/// the *same* connectors, client, transactions and event stream. This lets the
/// command-consumer task (see [`ChargePoint::connect`]) own a handle and drive
/// side effects such as `Reset` without a separate handle type.
#[derive(Clone)]
pub struct ChargePoint {
    /// Configuration
    config: ChargePointConfig,
    /// Connectors
    connectors: Arc<RwLock<HashMap<ConnectorId, Connector>>>,
    /// WebSocket client
    client: Arc<RwLock<Option<WebSocketClient>>>,
    /// Type-safe action dispatcher — routes incoming CALL messages.
    ///
    /// Ports `create_route_map()` / `_handle_call()` from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
    /// Default handlers for all 9 OCPP 1.6J Core Profile actions are
    /// registered in `new()`; callers can override or extend via `on()`.
    dispatcher: Arc<RwLock<ActionDispatcher>>,
    /// Configuration key-value store (shared with default ChangeConfiguration /
    /// GetConfiguration handlers).
    config_store: Arc<RwLock<ConfigurationStore>>,
    /// Event sender
    event_sender: mpsc::UnboundedSender<ChargePointEvent>,
    /// Event receiver
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<ChargePointEvent>>>>,
    /// Registration status
    registration_status: Arc<RwLock<RegistrationStatus>>,
    /// Connection state
    is_connected: Arc<RwLock<bool>>,
    /// Heartbeat task handle
    heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Maps CSMS-assigned transaction ID → connector ID for stop_transaction lookup.
    ///
    /// In 2.0.1 the *station* chooses the `transactionId` (the
    /// `TransactionEventResponse` carries none), so a `V201` CP mints the id
    /// from [`next_v201_transaction_id`](Self::next_v201_transaction_id) rather
    /// than reading it off the CSMS reply; the map is keyed the same either way.
    active_transactions: Arc<RwLock<HashMap<i32, ConnectorId>>>,
    /// Monotonic allocator for station-chosen 2.0.1 `transactionId`s (slice 3b,
    /// Issue #423). Unused on the 1.6J path (there the CSMS assigns the id). A
    /// `V201` `start_transaction` `fetch_add(1)`s to mint the next id and renders
    /// it as its decimal string for the wire (`SessionRef::transaction_id`). A CP
    /// is a single protocol version for its whole life, so these never collide
    /// with any CSMS-assigned 1.6J id.
    next_v201_transaction_id: Arc<AtomicI32>,
    /// Deferred 2.0.1 `Reset(OnIdle)` awaiting the station going idle (slice 4c,
    /// Issue #431). The V201 `Reset` handler records the mapped [`ResetType`] here
    /// when it answers `Scheduled` (an `OnIdle` reset received mid-transaction);
    /// [`stop_transaction`](Self::stop_transaction) takes and fires it exactly
    /// once when [`active_transactions`](Self::active_transactions) drains empty,
    /// so the station reboots the moment the last session ends — never mid-charge.
    /// [`perform_reset`](Self::perform_reset) clears it up front, so an immediate
    /// reset supersedes a pending deferred one instead of double-firing. `None`
    /// on the 1.6J path and whenever no deferred reset is armed.
    pending_v201_reset: Arc<RwLock<Option<ResetType>>>,
    /// Deferred 2.0.1 `ChangeAvailability` awaiting the station going idle (slice
    /// 6b, Issue #436). The V201 `ChangeAvailability` handler records the target
    /// (and its optional `evse` scope) here when it answers `Scheduled` (an
    /// availability change received mid-transaction); [`stop_transaction`](Self::stop_transaction)
    /// takes and applies it exactly once when [`active_transactions`](Self::active_transactions)
    /// drains empty, so the change lands the moment the last session ends — never
    /// cutting off a paying driver. Drained in the same `active_transactions`-guarded
    /// block as [`pending_v201_reset`](Self::pending_v201_reset), preserving the
    /// `active_transactions → pending_*` lock order. `None` on the 1.6J path and
    /// whenever no deferred change is armed.
    pending_v201_availability: Arc<RwLock<Option<PendingAvailability>>>,
    /// Per-transaction 2.0.1 session state, keyed by the same station-chosen
    /// `transactionId` as [`active_transactions`](Self::active_transactions).
    /// Populated only on the `V201` path: inserted by `start_transaction`
    /// alongside the `active_transactions` entry and removed by
    /// `stop_transaction` in lockstep. See [`V201Session`].
    v201_sessions: Arc<RwLock<HashMap<i32, V201Session>>>,
    /// v201-typed `TxProfile`s installed by an accepted `RequestStartTransaction`,
    /// keyed by EVSE id (slice 7d, Issue #450). Populated by `open_transaction`
    /// atomically with the [`V201Session`] the profile bounds and cleared by
    /// `close_transaction` in lockstep, so an installed profile never outlives
    /// its transaction. A distinct, v201-typed store rather than the 1.6J
    /// [`charging_profiles`](Self::charging_profiles) (which is typed on
    /// `v16j::ChargingProfile`). Read back via
    /// [`installed_tx_profile`](Self::installed_tx_profile). Empty on the 1.6J path.
    v201_tx_profiles: Arc<V201TxProfileStore>,
    /// v201 `TxDefaultProfile` store (Issue #471): the default charging schedule
    /// applied to an EVSE whenever no [`v201_tx_profiles`](Self::v201_tx_profiles)
    /// entry is in force. Installed out-of-band by a `SetChargingProfile` and, as
    /// station configuration rather than a transaction-scoped profile,
    /// **persists across transactions** — so it is *not* touched by
    /// `close_transaction`. Keyed by EVSE id, with key `0` the station-wide
    /// default (the schema's `evseId = 0` "applies to each individual evse" rule);
    /// read back via [`installed_tx_default_profile`](Self::installed_tx_default_profile).
    /// Consulted as the fallback by the metering resolver and `GetCompositeSchedule`.
    v201_tx_default_profiles: Arc<V201TxDefaultProfileStore>,
    /// v201 station-ceiling store (Issue #511): the `ChargingStationMaxProfile` /
    /// `ChargingStationExternalConstraints` profiles that **cap** — rather than
    /// substitute for — the resolved [`v201_tx_profiles`](Self::v201_tx_profiles) /
    /// [`v201_tx_default_profiles`](Self::v201_tx_default_profiles) limit. Installed
    /// out-of-band by a `SetChargingProfile`, keyed by `(kind, evseId)` with
    /// `evseId = 0` the whole-station ceiling; station configuration that persists
    /// across transactions (not touched by `close_transaction`). The metering
    /// resolver and `GetCompositeSchedule` apply it as `min(resolved, ceiling…)`;
    /// read back via [`installed_station_ceiling`](Self::installed_station_ceiling).
    /// Empty on the 1.6J path.
    v201_station_ceilings: Arc<V201StationCeilingStore>,
    /// v201 display messages installed by `SetDisplayMessage` (OCPP 2.0.1 Part 2,
    /// E05–E08, Issue #505), keyed by `MessageInfoType.id`. The foundational store
    /// of the display-message family: a same-id re-install upserts, a future
    /// `ClearDisplayMessage` removes by id, and a future `GetDisplayMessages`
    /// enumerates it — one source of truth for all three. Populated by the
    /// `SetDisplayMessage` handler (only once its pure decision returns `Accepted`).
    /// Empty on the 1.6J path (the handler is V201-only).
    v201_display_messages: Arc<V201DisplayMessageStore>,
    /// v201 root/CA trust anchors installed by `InstallCertificate` (OCPP 2.0.1
    /// Part 2, A02 / M03–M05, Issue #518), keyed by `InstallCertificateUseEnumType`.
    /// The foundational store of the certificate-*management* family: an install
    /// under a use upserts (rotates), and future `GetInstalledCertificateIds`
    /// (enumerate) / `DeleteCertificate` (remove) handlers read/mutate it — one
    /// source of truth for all three. Populated by the `InstallCertificate`
    /// handler only once its pure decision returns `Accepted`. Empty on the 1.6J
    /// path (the handler is V201-only; 1.6 has no per-use trust model here).
    v201_certificates: Arc<V201CertificateStore>,
    /// v201 network connection profiles installed by `SetNetworkProfile` (OCPP
    /// 2.0.1 Part 2, provisioning, B09/B10, Issue #528), keyed by the numbered
    /// `configurationSlot` each occupies. A same-slot re-provision upserts
    /// (last-writer-wins); a distinct slot stores independently. Populated by the
    /// V201-only `SetNetworkProfile` handler once its pure decision returns
    /// `Accepted`; read back via [`network_profile`](Self::network_profile) /
    /// [`configured_network_slots`](Self::configured_network_slots). Empty on the
    /// 1.6J path (the handler is V201-only; 1.6 has no equivalent command). A
    /// self-contained configuration store with no async follow-up.
    v201_network_profiles: Arc<V201NetworkProfileStore>,
    /// The single in-flight `GetLog` upload the station is serving (OCPP 2.0.1
    /// Part 2, security profile, Issue #517), by its `requestId`. A station
    /// uploads one log at a time, so the V201-only `GetLog` handler records the
    /// accepted request here to answer a later `GetLog` deterministically: a retry
    /// of the same `requestId` is idempotent, a different one supersedes
    /// (`AcceptedCanceled`). Idle on the 1.6J path (the handler is V201-only). See
    /// [`V201LogUploadStore`].
    v201_log_uploads: Arc<V201LogUploadStore>,
    /// The single in-flight `UpdateFirmware` rollout the station is serving (OCPP
    /// 2.0.1 Part 2, firmware management, L01–L03, Issue #532), by its `requestId`.
    /// A station runs one firmware update at a time, so the V201-only
    /// `UpdateFirmware` handler records the accepted request here to answer a later
    /// `UpdateFirmware` deterministically: a retry of the same `requestId` is
    /// idempotent, a different one supersedes (`AcceptedCanceled`). Idle on the 1.6J
    /// path (which keeps the empty-conf `UpdateFirmware` handler and its own
    /// firmware state machine). See [`V201FirmwareUpdateStore`].
    v201_firmware_updates: Arc<V201FirmwareUpdateStore>,
    /// The set of in-flight `CustomerInformation` report streams the station is
    /// serving (OCPP 2.0.1 Part 2, N-series — data privacy, Issue #537), by
    /// `requestId`. The V201-only `CustomerInformation` handler records an
    /// accepted *reporting* request here so a retry of the same `requestId` does
    /// not launch a second, duplicate `NotifyCustomerInformation` stream; the
    /// async consumer clears the marker once the stream settles. Unlike the
    /// single-slot log/firmware trackers this holds a *set* — customer-info
    /// reports are independent per id, with no supersede. Empty on the 1.6J path
    /// (the handler is V201-only). See [`V201CustomerInformationStore`].
    v201_customer_information_reports: Arc<V201CustomerInformationStore>,
    /// The set of in-flight `PublishFirmware` progress streams the station is
    /// driving as a Local Controller (OCPP 2.0.1 Part 2, firmware management,
    /// Issue #540), by `requestId`. The V201-only `PublishFirmware` handler
    /// records an accepted request here so a retry of the same `requestId` does
    /// not launch a second, duplicate `PublishFirmwareStatusNotification` stream;
    /// the async consumer clears the marker once the stream settles. Like the
    /// customer-information tracker (and unlike the single-slot log/firmware-
    /// update trackers) this holds a *set* — firmware publishes are independent
    /// per id, with no supersede. Empty on the 1.6J path (the handler is
    /// V201-only). See [`V201PublishFirmwareStore`].
    v201_publish_firmwares: Arc<V201PublishFirmwareStore>,
    /// Latest running total cost the CSMS has pushed per transaction id via the
    /// 2.0.1 `CostUpdated` message (Issue #502), keyed by the wire
    /// `transactionId` string. Upserted by the V201-only `CostUpdated` handler
    /// and read back via [`recorded_transaction_cost`](Self::recorded_transaction_cost).
    /// A cost is recorded unconditionally (OCPP defines no rejection for
    /// `CostUpdated`), so an entry may exist for a `transactionId` not in
    /// [`active_transactions`](Self::active_transactions). Empty on the 1.6J path
    /// (which has no `CostUpdated` handler). See [`V201CostStore`].
    v201_costs: Arc<V201CostStore>,
    /// Maps an active reservation ID → the connector it holds (OCPP 1.6J §5.14).
    /// Populated by the default `ReserveNow` handler, consulted/cleared by
    /// `CancelReservation`, and emptied for a connector when a transaction
    /// starts on it (a start consumes the reservation).
    reservations: Arc<RwLock<HashMap<i32, ConnectorId>>>,
    /// Per-reservation auto-expiry timer tasks, keyed by `reservationId` (OCPP
    /// 1.6J §5.14, Issue #85). Armed by the default `ReserveNow` handler when a
    /// reservation is `Accepted`; each task sleeps until the `expiryDate` and
    /// then frees the connector (`Reserved → Available`), drops the
    /// `reservationId`, and emits a `StatusNotification(Available)` off the
    /// inbound-CALL path. Disarmed (aborted) when the reservation is cancelled,
    /// consumed by a start, or superseded. Finished handles are pruned on each
    /// arm and all are aborted by [`stop`](Self::stop) so the map never grows
    /// unbounded and tasks don't leak.
    expiry_timers: Arc<RwLock<HashMap<i32, tokio::task::JoinHandle<()>>>>,
    /// Shared schema validator (when `config.validate_payloads`). Backs both
    /// the dispatcher's incoming-CALL validation and `call()`'s CALLRESULT
    /// validation. `None` when validation is disabled.
    validator: Option<Arc<SchemaValidator>>,
    /// Per-transaction periodic metering sampler tasks, keyed by the transaction
    /// ID. Each active transaction has its own task (connectors charge
    /// concurrently); cancelled on `stop_transaction`/`stop`. The task emits the
    /// 1.6J `MeterValues` frame or the 2.0.1 `TransactionEvent(Updated)`
    /// depending on `config.protocol_version`, but the handle map is
    /// version-agnostic.
    meter_sampler_handles: Arc<RwLock<HashMap<i32, tokio::task::JoinHandle<()>>>>,
    /// Authorization ID-tag cache (Issue #23). Shared with the default
    /// `ClearCache` handler so a CSMS `ClearCache` command empties it. Backs the
    /// cache-first behavior of [`ChargePoint::authorize`].
    auth_cache: Arc<AuthCache>,
    /// Sender half of the [`RemoteCommand`] channel, retained on the
    /// `ChargePoint` so methods that run *off* the inbound-CALL path (currently
    /// [`stop_transaction`](Self::stop_transaction), firing a deferred
    /// `Reset(OnIdle)` — slice 4c) can queue a side effect for the same
    /// command-consumer task the dispatcher closures feed. Cloned from the sender
    /// handed to [`build_default_dispatcher`](Self::build_default_dispatcher), so
    /// every clone points at the one channel drained by the single consumer.
    command_sender: mpsc::UnboundedSender<RemoteCommand>,
    /// Receiver for [`RemoteCommand`]s queued by inbound CALL handlers (e.g. the
    /// default `Reset` handler). Taken exactly once by the first
    /// [`ChargePoint::connect`] to spawn the single long-lived command-consumer
    /// task; `None` thereafter, so a reconnect (e.g. a Hard reset) does not spawn
    /// a second consumer. The sender half lives inside the dispatcher closures,
    /// which the `ChargePoint` keeps alive, so the channel stays open.
    command_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<RemoteCommand>>>>,
    /// Join handle for the command-consumer task. The consumer owns a
    /// `ChargePoint` clone (and thus, transitively, a `RemoteCommand` sender via
    /// the dispatcher), so the channel never closes on its own — the task is
    /// aborted by [`stop`](Self::stop) to break that cycle and free the shared
    /// state, exactly like the heartbeat task.
    command_consumer: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Current diagnostics-upload status (OCPP 1.6J §4.x). Starts `Idle`;
    /// driven through `Uploading` → `Uploaded` by the simulated upload a
    /// `GetDiagnostics` kicks off. Read on demand by
    /// `TriggerMessage(DiagnosticsStatusNotification)` to report the latest
    /// status without re-running the upload.
    diagnostics_status: Arc<RwLock<DiagnosticsStatus>>,
    /// Current firmware-update status (OCPP 1.6J §4.x). Starts `Idle`; driven
    /// through `Downloading` → `Downloaded` → `Installing` → `Installed` by the
    /// simulated update an `UpdateFirmware` kicks off. Read on demand by
    /// `TriggerMessage(FirmwareStatusNotification)` to report the latest status
    /// without re-running the update.
    firmware_status: Arc<RwLock<FirmwareStatus>>,
    /// Installed Smart Charging profiles (OCPP 1.6J §5.16 / §5.2). Shared with
    /// the default `SetChargingProfile` / `ClearChargingProfile` handlers, which
    /// install and clear profiles per the spec's stacking rules. Read by the
    /// `GetCompositeSchedule` follow-up (Issue #95) to compute the effective
    /// schedule.
    charging_profiles: Arc<ChargingProfileStore>,
    /// Vendor-scoped routing table for inbound `DataTransfer` requests (OCPP
    /// 1.6J §6.x, Issue #101). Shared with the default `DataTransfer` handler;
    /// embedders opt into vendors/messages via
    /// [`ChargePoint::register_data_transfer_handler`]. Empty by default, so an
    /// unimplemented vendor faithfully resolves to `UnknownVendorId`.
    data_transfer: Arc<DataTransferRegistry>,
    /// CSMS-managed, versioned Local Authorization List (OCPP 1.6J §5.x, Issue
    /// #93). Shared with the default `GetLocalListVersion` / `SendLocalList`
    /// handlers; the CSMS queries the version and pushes `Full`/`Differential`
    /// updates. Empty at version `0` by default.
    local_list: Arc<LocalAuthList>,
}

impl ChargePoint {
    /// Create a new charge point with default `@on` handlers for all OCPP 1.6J
    /// Core Profile actions.
    ///
    /// After construction, callers may override any default handler or add new
    /// ones via [`ChargePoint::on`] / [`ChargePoint::after`] before calling
    /// [`ChargePoint::start`].
    pub fn new(config: ChargePointConfig) -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let mut connectors = HashMap::new();
        for i in 1..=config.connector_count {
            let connector_id = ConnectorId::new(i)?;
            let connector_config = ConnectorConfig {
                connector_id,
                connector_type: "Type2".to_string(),
                max_amperage: 32.0,
                max_voltage: 230.0,
                max_power: 7360.0, // 32A * 230V
                phases: 1,
                energy_meter_serial: Some(format!("EM{:03}", i)),
            };
            connectors.insert(connector_id, Connector::new(connector_config)?);
        }

        // Build the shared validator once and back both the dispatcher (incoming
        // CALLs) and `call()` (outgoing CALLs + their CALLRESULTs) with it.
        // Mirrors `ocpp/charge_point.py`, which always runs `_validate()`.
        //
        // The validator is version-aware: a `V201`-configured CP validates
        // against the 2.0.1 schema set so its outgoing 2.0.1 `BootNotification`
        // /`StatusNotification` (which have entirely different shapes from their
        // 1.6J namesakes) are checked against the *right* schema. `V16J` (the
        // default) resolves to `SchemaValidator::v16j()` exactly as before, so
        // existing behavior is byte-for-byte unchanged.
        let validator = if config.validate_payloads {
            Some(Arc::new(match config.protocol_version {
                OcppVersion::V16J => SchemaValidator::v16j(),
                OcppVersion::V201 => SchemaValidator::v201(),
            }))
        } else {
            None
        };

        let auth_cache = Arc::new(AuthCache::new(Duration::from_secs(config.auth_cache_ttl)));

        let charging_profiles = Arc::new(ChargingProfileStore::new());

        let data_transfer = Arc::new(DataTransferRegistry::new());

        let local_list = Arc::new(LocalAuthList::with_max_length(
            config.local_auth_list_max_length,
        ));

        let (command_sender, command_receiver) = mpsc::unbounded_channel();

        // Slice 4c (#431): the V201 `Reset` handler records a deferred
        // `Reset(OnIdle)` here; `stop_transaction` drains it when the station goes
        // idle. Shared into the dispatcher (writer) and kept on the CP (reader).
        let pending_v201_reset: Arc<RwLock<Option<ResetType>>> = Arc::new(RwLock::new(None));

        // Slice 6b (#436): the V201 `ChangeAvailability` handler records a deferred
        // availability change here; `stop_transaction` drains it when the station
        // goes idle. Shared into the dispatcher (writer) and kept on the CP (reader),
        // exactly like `pending_v201_reset`.
        let pending_v201_availability: Arc<RwLock<Option<PendingAvailability>>> =
            Arc::new(RwLock::new(None));

        // Shared into the dispatcher (the V201 `UnlockConnector` handler reads a
        // live session's `authorized` bit) and kept on the CP (the metering
        // sampler, `stop_transaction`, and `deauthorize` own it).
        let v201_sessions: Arc<RwLock<HashMap<i32, V201Session>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Slice 7d (#450): v201-typed TxProfile store, kept on the CP (installed
        // by `open_transaction`, cleared by `close_transaction`, read by
        // `installed_tx_profile`). The `RequestStartTransaction` install path only
        // *queues* the profile via `RemoteCommand::StartTransaction` — that install
        // happens on the `&self` transaction-open path — but the direct
        // `SetChargingProfile` command (#469) installs straight into this store
        // from the dispatcher, so it is shared into the default dispatcher too.
        let v201_tx_profiles = Arc::new(V201TxProfileStore::new());
        // The display-message store the V201 `SetDisplayMessage` handler upserts
        // into (Issue #505). Shared into the default dispatcher so the handler can
        // install straight from the CALL path.
        let v201_display_messages = Arc::new(V201DisplayMessageStore::new());
        // The trust-anchor store the V201 `InstallCertificate` handler installs
        // into (Issue #518). Shared into the default dispatcher so the handler can
        // install straight from the CALL path; the foundational store the
        // certificate-management follow-ups (`GetInstalledCertificateIds` /
        // `DeleteCertificate`) will read/mutate.
        let v201_certificates = Arc::new(V201CertificateStore::new());

        // The network-profile store the V201 `SetNetworkProfile` handler upserts
        // into (Issue #528). Shared into the default dispatcher so the handler can
        // store straight from the CALL path; keyed by `configurationSlot`, a
        // self-contained configuration store with no async follow-up.
        let v201_network_profiles = Arc::new(V201NetworkProfileStore::new());

        // The in-flight `GetLog` upload tracker the V201 `GetLog` handler records
        // into (Issue #517). Shared into the default dispatcher so the handler can
        // read/set the single in-flight `requestId` straight from the CALL path;
        // idle on the 1.6J path (the handler is V201-only).
        let v201_log_uploads = Arc::new(V201LogUploadStore::new());

        // The in-flight `UpdateFirmware` rollout tracker the V201 `UpdateFirmware`
        // handler records into (Issue #532). Shared into the default dispatcher so
        // the handler can read/set the single in-flight `requestId` straight from
        // the CALL path; idle on the 1.6J path (which keeps the empty-conf handler).
        let v201_firmware_updates = Arc::new(V201FirmwareUpdateStore::new());

        // The in-flight `CustomerInformation` report-stream tracker the V201
        // `CustomerInformation` handler records into (Issue #537). Shared into the
        // default dispatcher so the handler can dedup a retry straight from the
        // CALL path; empty on the 1.6J path (the handler is V201-only).
        let v201_customer_information_reports = Arc::new(V201CustomerInformationStore::new());

        // The in-flight `PublishFirmware` progress-stream tracker the V201
        // `PublishFirmware` handler records into (Issue #540). Shared into the
        // default dispatcher so the handler can dedup a retry straight from the
        // CALL path; empty on the 1.6J path (the handler is V201-only).
        let v201_publish_firmwares = Arc::new(V201PublishFirmwareStore::new());

        // Issue #471: v201 `TxDefaultProfile` store, shared into the default
        // dispatcher (the `SetChargingProfile` command installs a default into it)
        // and kept on the CP (the metering sampler and `GetCompositeSchedule` read
        // it as the fallback when no `TxProfile` is in force; `installed_tx_default_profile`
        // reads it back). Persists across transactions — station configuration,
        // not a transaction-scoped profile — so nothing clears it on close.
        let v201_tx_default_profiles = Arc::new(V201TxDefaultProfileStore::new());

        // Issue #511: v201 station-ceiling store (ChargingStationMaxProfile /
        // ChargingStationExternalConstraints), shared into the default dispatcher
        // (the `SetChargingProfile` command installs a ceiling into it) and kept on
        // the CP (the metering sampler and `GetCompositeSchedule` cap the resolved
        // limit by it; `installed_station_ceiling` reads it back). Like the
        // TxDefaultProfile store it is station configuration that persists across
        // transactions, so nothing clears it on close.
        let v201_station_ceilings = Arc::new(V201StationCeilingStore::new());

        // Running-cost store the 2.0.1 `CostUpdated` handler upserts into
        // (Issue #502). Shared into the default dispatcher so the V201 arm can
        // record an inbound cost; read back via `recorded_transaction_cost`.
        let v201_costs = Arc::new(V201CostStore::new());

        // Wrap the shared state the default handlers need *before* building the
        // dispatcher: RemoteStart/RemoteStop consult live connector status and
        // the active-transaction map to answer Accepted/Rejected faithfully.
        let connectors = Arc::new(RwLock::new(connectors));
        let active_transactions = Arc::new(RwLock::new(HashMap::new()));
        let reservations = Arc::new(RwLock::new(HashMap::new()));
        let expiry_timers = Arc::new(RwLock::new(HashMap::new()));

        // Report the CP's actual Local Authorization List capacity for the
        // read-only `LocalAuthListMaxLength` key, so the value a CSMS reads via
        // GetConfiguration matches what `local_list` actually enforces.
        let config_store = {
            let mut store = ConfigurationStore::new();
            store.set_readonly(
                "LocalAuthListMaxLength",
                config.local_auth_list_max_length.to_string(),
            );
            Arc::new(RwLock::new(store))
        };
        // The 2.0.1 device model the `GetVariables` handler reads. Seeded with a
        // representative standard profile; station-wide (no per-EVSE variables)
        // for now. Behind an `RwLock` so a future `SetVariables` write seam
        // (#458) can share and mutate it.
        let v201_device_model = Arc::new(RwLock::new(V201DeviceModel::with_standard_profile()));
        let mut dispatcher = Self::build_default_dispatcher(
            config.protocol_version,
            config_store.clone(),
            v201_device_model.clone(),
            auth_cache.clone(),
            command_sender.clone(),
            connectors.clone(),
            active_transactions.clone(),
            v201_sessions.clone(),
            reservations.clone(),
            charging_profiles.clone(),
            v201_tx_profiles.clone(),
            v201_tx_default_profiles.clone(),
            v201_station_ceilings.clone(),
            v201_display_messages.clone(),
            v201_certificates.clone(),
            v201_network_profiles.clone(),
            v201_log_uploads.clone(),
            v201_firmware_updates.clone(),
            v201_customer_information_reports.clone(),
            v201_publish_firmwares.clone(),
            v201_costs.clone(),
            expiry_timers.clone(),
            config.unlock_connector_outcome,
            data_transfer.clone(),
            local_list.clone(),
            pending_v201_reset.clone(),
            pending_v201_availability.clone(),
        );
        if let Some(v) = &validator {
            dispatcher = dispatcher.with_validator(v.clone());
        }
        let dispatcher = Arc::new(RwLock::new(dispatcher));

        Ok(Self {
            config,
            connectors,
            client: Arc::new(RwLock::new(None)),
            dispatcher,
            config_store,
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
            registration_status: Arc::new(RwLock::new(RegistrationStatus::Rejected)),
            is_connected: Arc::new(RwLock::new(false)),
            heartbeat_handle: Arc::new(RwLock::new(None)),
            active_transactions,
            pending_v201_reset,
            pending_v201_availability,
            command_sender,
            next_v201_transaction_id: Arc::new(AtomicI32::new(1)),
            v201_sessions,
            v201_tx_profiles,
            v201_tx_default_profiles,
            v201_station_ceilings,
            v201_display_messages,
            v201_certificates,
            v201_network_profiles,
            v201_log_uploads,
            v201_firmware_updates,
            v201_customer_information_reports,
            v201_publish_firmwares,
            v201_costs,
            reservations,
            expiry_timers,
            validator,
            meter_sampler_handles: Arc::new(RwLock::new(HashMap::new())),
            auth_cache,
            command_receiver: Arc::new(RwLock::new(Some(command_receiver))),
            command_consumer: Arc::new(RwLock::new(None)),
            diagnostics_status: Arc::new(RwLock::new(DiagnosticsStatus::Idle)),
            firmware_status: Arc::new(RwLock::new(FirmwareStatus::Idle)),
            charging_profiles,
            data_transfer,
            local_list,
        })
    }

    /// Build the default `ActionDispatcher` pre-populated with handlers for
    /// all 9 OCPP 1.6J Core Profile actions.
    ///
    /// Ports the default `@on` handler registrations from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
    /// and [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    // Threads the handful of shared-state handles the default handlers need into
    // a single private constructor; the argument count grows with the feature
    // set (reservations, expiry timers, unlock outcome) rather than indicating a
    // design smell, so the lint is intentionally allowed here.
    #[allow(clippy::too_many_arguments)]
    fn build_default_dispatcher(
        protocol_version: OcppVersion,
        config_store: Arc<RwLock<ConfigurationStore>>,
        v201_device_model: Arc<RwLock<V201DeviceModel>>,
        auth_cache: Arc<AuthCache>,
        command_sender: mpsc::UnboundedSender<RemoteCommand>,
        connectors: Arc<RwLock<HashMap<ConnectorId, Connector>>>,
        active_transactions: Arc<RwLock<HashMap<i32, ConnectorId>>>,
        v201_sessions: Arc<RwLock<HashMap<i32, V201Session>>>,
        reservations: Arc<RwLock<HashMap<i32, ConnectorId>>>,
        charging_profiles: Arc<ChargingProfileStore>,
        v201_tx_profiles: Arc<V201TxProfileStore>,
        v201_tx_default_profiles: Arc<V201TxDefaultProfileStore>,
        v201_station_ceilings: Arc<V201StationCeilingStore>,
        v201_display_messages: Arc<V201DisplayMessageStore>,
        v201_certificates: Arc<V201CertificateStore>,
        v201_network_profiles: Arc<V201NetworkProfileStore>,
        v201_log_uploads: Arc<V201LogUploadStore>,
        v201_firmware_updates: Arc<V201FirmwareUpdateStore>,
        v201_customer_information_reports: Arc<V201CustomerInformationStore>,
        v201_publish_firmwares: Arc<V201PublishFirmwareStore>,
        v201_costs: Arc<V201CostStore>,
        expiry_timers: Arc<RwLock<HashMap<i32, tokio::task::JoinHandle<()>>>>,
        unlock_outcome: UnlockConnectorOutcome,
        data_transfer: Arc<DataTransferRegistry>,
        local_list: Arc<LocalAuthList>,
        pending_v201_reset: Arc<RwLock<Option<ResetType>>>,
        pending_v201_availability: Arc<RwLock<Option<PendingAvailability>>>,
    ) -> ActionDispatcher {
        let mut d = ActionDispatcher::new();

        // ChangeAvailability — take the Charging Station (or a single EVSE)
        // Operative / Inoperative. Both OCPP versions name the action
        // `"ChangeAvailability"`, so exactly one handler is registered per
        // `protocol_version` (the same discipline as the Reset / TriggerMessage
        // splits below); the negotiated subprotocol and the version-aware inbound
        // validator keep the wire on a single dialect, so the other version's
        // request shape never reaches this dispatcher.
        match protocol_version {
            // OCPP 1.6J §5.4. Unchanged: always accept (Issue #21 tracks real
            // availability-state tracking on the 1.6J path).
            OcppVersion::V16J => {
                d.on(|_req: ChangeAvailabilityRequest| async move {
                    Ok(ChangeAvailabilityResponse {
                        status: AvailabilityStatus::Accepted,
                    })
                });
            }
            // OCPP 2.0.1 (Part 2, `ChangeAvailability`). Ports
            // `ocpp.v201.call.ChangeAvailability`: the request carries an
            // `OperationalStatusEnumType` and, when `evse` is omitted, targets the
            // whole Charging Station. The pure Accepted/Scheduled policy lives in
            // `v201_command` (slice 6a, #435); this is the runtime wiring (slice
            // 6b, #436). Same off-CALL-path side-effect discipline and
            // capability-`Rejected` outcome as the Reset / TriggerMessage handlers.
            OcppVersion::V201 => {
                let command_sender = command_sender.clone();
                let active_transactions = active_transactions.clone();
                let connectors = connectors.clone();
                let pending_v201_availability = pending_v201_availability.clone();
                d.on(move |req: V201ChangeAvailabilityRequest| {
                    let command_sender = command_sender.clone();
                    let active_transactions = active_transactions.clone();
                    let connectors = connectors.clone();
                    let pending_v201_availability = pending_v201_availability.clone();
                    async move {
                        let target = req.operational_status;
                        let evse_id = req.evse.as_ref().map(|e| e.id);

                        // Resolve the targeted connectors. `evse` present → that
                        // single connector, but only if it is a real connector on
                        // this CP (an unknown / out-of-range EVSE — including a 0 or
                        // negative id — resolves to no target and is `Rejected`
                        // below). `evse` absent → the whole station (every
                        // connector). The simulator's flat topology maps EVSE id to
                        // the same-valued connector (the slice-2 StatusNotification
                        // convention).
                        let targets: Vec<ConnectorId> = match evse_id {
                            Some(id) => {
                                let cid = u32::try_from(id)
                                    .ok()
                                    .and_then(|v| ConnectorId::new(v).ok());
                                match cid {
                                    Some(cid) if connectors.read().await.contains_key(&cid) => {
                                        vec![cid]
                                    }
                                    _ => Vec::new(),
                                }
                            }
                            None => {
                                let mut ids: Vec<ConnectorId> =
                                    connectors.read().await.keys().copied().collect();
                                ids.sort_by_key(ConnectorId::value);
                                ids
                            }
                        };

                        // Unknown / out-of-range EVSE target → the station cannot
                        // apply the change: `Rejected` (a capability outcome, which
                        // the slice-6a policy never produces on its own).
                        if targets.is_empty() {
                            return Ok(v201_command::v201_change_availability_response(
                                ChangeAvailabilityStatusEnumType::Rejected,
                                None,
                            ));
                        }

                        // A transaction "in progress on the targeted scope" gates
                        // Accepted vs Scheduled: whole-station when `evse` is
                        // omitted, else only the targeted connector.
                        let transaction_in_progress = {
                            let active = active_transactions.read().await;
                            match evse_id {
                                None => !active.is_empty(),
                                Some(_) => active.values().any(|c| targets.contains(c)),
                            }
                        };

                        let mut status = v201_command::v201_change_availability_status(
                            target,
                            transaction_in_progress,
                        );

                        match status {
                            // Idle: apply now — one apply command per targeted
                            // connector, off the CALL path. If the consumer has gone
                            // away (CP shutting down) we cannot honor it, so
                            // downgrade to `Rejected` rather than accept-and-drop —
                            // the same capability outcome the Reset handler reports
                            // on a failed channel send.
                            ChangeAvailabilityStatusEnumType::Accepted => {
                                for cid in &targets {
                                    if command_sender
                                        .send(RemoteCommand::V201ApplyAvailability {
                                            connector_id: *cid,
                                            target,
                                        })
                                        .is_err()
                                    {
                                        status = ChangeAvailabilityStatusEnumType::Rejected;
                                        break;
                                    }
                                }
                            }
                            // Busy: accept but defer. Arm the pending change (last
                            // write wins, like the deferred reset); no side effect is
                            // queued now, so the live session is never interrupted.
                            // `stop_transaction` applies it once the station next
                            // goes idle (slice 6b tail).
                            ChangeAvailabilityStatusEnumType::Scheduled => {
                                *pending_v201_availability.write().await =
                                    Some(PendingAvailability { evse_id, target });
                            }
                            // Never produced by the slice-6a policy function; a
                            // capability `Rejected` is only reached via the failed
                            // enqueue / unknown-EVSE paths above.
                            ChangeAvailabilityStatusEnumType::Rejected => {}
                        }

                        Ok(v201_command::v201_change_availability_response(
                            status, None,
                        ))
                    }
                });
            }
        }

        // ChangeConfiguration — write to ConfigurationStore
        {
            let cs = config_store.clone();
            d.on(move |req: ChangeConfigurationRequest| {
                let cs = cs.clone();
                async move {
                    let mut store = cs.write().await;
                    let status = match store.set(&req.key, req.value) {
                        Ok(()) => ConfigurationStatus::Accepted,
                        Err(e) if e.contains("read-only") => ConfigurationStatus::Rejected,
                        Err(_) => ConfigurationStatus::NotSupported,
                    };
                    Ok(ChangeConfigurationResponse { status })
                }
            });
        }

        // GetConfiguration — read from ConfigurationStore
        {
            let cs = config_store.clone();
            d.on(move |req: GetConfigurationRequest| {
                let cs = cs.clone();
                async move {
                    let store = cs.read().await;
                    let (configuration_keys, unknown_keys) = if let Some(keys) = req.key {
                        let mut cfg_keys = Vec::new();
                        let mut unknown = Vec::new();
                        for key in keys {
                            if let Some(value) = store.get(&key) {
                                cfg_keys.push(KeyValue {
                                    key: key.clone(),
                                    readonly: Some(store.is_readonly(&key)),
                                    value: Some(value.clone()),
                                });
                            } else {
                                unknown.push(key);
                            }
                        }
                        (
                            Some(cfg_keys),
                            if unknown.is_empty() {
                                None
                            } else {
                                Some(unknown)
                            },
                        )
                    } else {
                        let cfg_keys = store
                            .keys()
                            .iter()
                            .map(|(k, v)| KeyValue {
                                key: k.clone(),
                                readonly: Some(store.is_readonly(k)),
                                value: Some(v.clone()),
                            })
                            .collect();
                        (Some(cfg_keys), None)
                    };
                    Ok(GetConfigurationResponse {
                        configuration_keys,
                        unknown_keys,
                    })
                }
            });
        }

        // GetVariables (OCPP 2.0.1 only) — read one or more component-variable
        // attributes from the device model. The 2.0.1 replacement for 1.6J
        // `GetConfiguration`: instead of flat keys, each entry names a
        // `component`/`variable` pair (see `v201_device_model`). Registered only
        // on the V201 arm — "GetVariables" has no 1.6J twin, and the negotiated
        // subprotocol + version-aware inbound validator keep a 1.6J CP from ever
        // seeing this action. Read-only: no side effects, so nothing is deferred
        // off the CALL path. Ports `ocpp.v201.call.GetVariables`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            d.on(move |req: V201GetVariablesRequest| {
                let device_model = device_model.clone();
                async move {
                    let model = device_model.read().await;
                    // One result per requested entry, in request order. Each
                    // result echoes the CSMS's original (un-normalized)
                    // `component` / `variable` / `attributeType`, as the schema
                    // requires; an omitted `attributeType` resolves to `Actual`
                    // for the lookup but is echoed back as `None`.
                    let get_variable_result: Vec<GetVariableResultType> = req
                        .get_variable_data
                        .iter()
                        .map(|data| {
                            let attribute =
                                data.attribute_type.unwrap_or(AttributeEnumType::Actual);
                            let (attribute_status, attribute_value) =
                                model.get(&data.component, &data.variable, attribute);
                            GetVariableResultType {
                                attribute_status,
                                component: data.component.clone(),
                                variable: data.variable.clone(),
                                attribute_type: data.attribute_type,
                                attribute_value,
                                attribute_status_info: None,
                                custom_data: None,
                            }
                        })
                        .collect();
                    Ok(V201GetVariablesResponse {
                        get_variable_result,
                        custom_data: None,
                    })
                }
            });
        }

        // SetVariables (OCPP 2.0.1 only) — write one or more component-variable
        // attributes into the device model. The 2.0.1 replacement for 1.6J
        // `ChangeConfiguration`, and the write counterpart to `GetVariables`
        // above: it shares the same `V201DeviceModel` store (behind the write
        // lock here). Registered only on the V201 arm — "SetVariables" has no
        // 1.6J twin, and the negotiated subprotocol + version-aware inbound
        // validator keep a 1.6J CP from ever seeing this action.
        //
        // Trust boundary: each `attributeValue` is an untrusted, schema-bounded
        // string stored verbatim (no parse/exec), so there are no panics on wire
        // input. The write is applied on the CALL path (a device-model store
        // update is cheap and must be visible to the CALLRESULT and to any
        // subsequent `GetVariables`), serialized by the model's write lock.
        // Ports `ocpp.v201.call.SetVariables`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            d.on(move |req: V201SetVariablesRequest| {
                let device_model = device_model.clone();
                async move {
                    let mut model = device_model.write().await;
                    // One result per requested entry, in request order. Each
                    // result echoes the CSMS's original (un-normalized)
                    // `component` / `variable` / `attributeType`; an omitted
                    // `attributeType` resolves to `Actual` for the write but is
                    // echoed back as `None`, mirroring the read seam.
                    let set_variable_result: Vec<SetVariableResultType> = req
                        .set_variable_data
                        .iter()
                        .map(|data| {
                            let attribute =
                                data.attribute_type.unwrap_or(AttributeEnumType::Actual);
                            let attribute_status = model.set(
                                &data.component,
                                &data.variable,
                                attribute,
                                &data.attribute_value,
                            );
                            SetVariableResultType {
                                attribute_status,
                                component: data.component.clone(),
                                variable: data.variable.clone(),
                                attribute_type: data.attribute_type,
                                attribute_status_info: None,
                                custom_data: None,
                            }
                        })
                        .collect();
                    Ok(V201SetVariablesResponse {
                        set_variable_result,
                        custom_data: None,
                    })
                }
            });
        }

        // SetVariableMonitoring (OCPP 2.0.1 only) — install (or replace) variable
        // monitors (threshold / delta / periodic) on the device model, the write
        // counterpart to the `GetMonitoringReport` read seam below and the monitor
        // sibling of `SetVariables` above. It shares the same `V201DeviceModel`
        // store (behind the write lock here); each accepted monitor becomes
        // visible to a subsequent `GetMonitoringReport` via `monitoring_snapshot`,
        // which streams it as a `NotifyMonitoringReport`. Registered only on the
        // V201 arm — "SetVariableMonitoring" has no 1.6J twin.
        //
        // Trust boundary: each `SetMonitoringData` entry is schema-bounded and
        // stored verbatim (no parse/exec, no indexing into it); an install against
        // an unknown component/variable is a modeled rejection status, never a
        // panic, and repeat installs are idempotent (an identical monitor is
        // reported `Duplicate`, not double-installed). The write is applied on the
        // CALL path (cheap, and must be visible to the CALLRESULT and any
        // subsequent `GetMonitoringReport`), serialized by the model's write lock.
        // Ports `ocpp.v201.call.SetVariableMonitoring` →
        // `ocpp.v201.call_result.SetVariableMonitoring`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            d.on(move |req: V201SetVariableMonitoringRequest| {
                let device_model = device_model.clone();
                async move {
                    let set_monitoring_result = device_model
                        .write()
                        .await
                        .install_monitors(&req.set_monitoring_data);
                    Ok(V201SetVariableMonitoringResponse {
                        set_monitoring_result,
                        custom_data: None,
                    })
                }
            });
        }

        // ClearVariableMonitoring (OCPP 2.0.1 only) — remove previously-installed
        // variable monitors by id, the teardown counterpart to
        // `SetVariableMonitoring` above over the same `V201DeviceModel` store. A
        // cleared monitor stops appearing in `monitoring_snapshot`, so a
        // subsequent `GetMonitoringReport` no longer streams it. Registered only
        // on the V201 arm — "ClearVariableMonitoring" has no 1.6J twin.
        //
        // Trust boundary: `req.id` is a schema-bounded `Vec<i32>`; each id is used
        // only as a `HashMap` key (no parse, no indexing), so an unknown or
        // out-of-range id is a modeled `NotFound`, never a panic. Like the install
        // handler this is a pure read-and-remove with **no queued side effect** —
        // the write is applied on the CALL path (must be visible to the CALLRESULT
        // and any later `GetMonitoringReport`), serialized by the model's write
        // lock. Ports `ocpp.v201.call.ClearVariableMonitoring` →
        // `ocpp.v201.call_result.ClearVariableMonitoring`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            d.on(move |req: V201ClearVariableMonitoringRequest| {
                let device_model = device_model.clone();
                async move {
                    let clear_monitoring_result =
                        device_model.write().await.clear_monitors(&req.id);
                    Ok(V201ClearVariableMonitoringResponse {
                        clear_monitoring_result,
                        custom_data: None,
                    })
                }
            });
        }

        // SetMonitoringLevel (OCPP 2.0.1 only) — set the **reporting severity
        // threshold** for the device-model monitoring family (#494/#495/#499).
        // Where `SetVariableMonitoring` installs monitors and
        // `ClearVariableMonitoring` removes them, this setter governs *at what
        // severity* installed monitors report events: the station reports monitors
        // whose `severity` is at or below the active level (lower = more severe;
        // `0` Danger … `9` Debug). The level is stored on the shared
        // `V201DeviceModel` (behind the write lock) and readable via
        // `active_monitoring_level()`. Registered only on the V201 arm —
        // "SetMonitoringLevel" has no 1.6J twin.
        //
        // Trust boundary: `req.severity` is a schema-bounded `i32`, but the `0..=9`
        // range is **not** schema-constrained (the payload only pins `integer`),
        // so the station validates it here. An in-range value is stored and
        // answered `Accepted`; any out-of-range `i32` (negative, `> 9`,
        // `i32::MIN` / `i32::MAX`) leaves the stored level unchanged and is
        // answered `Rejected` with a `StatusInfoType` reason — it is only
        // range-compared, never parsed or indexed, so it cannot panic or overflow
        // on wire input. The write is applied on the CALL path (cheap, and must be
        // visible to the CALLRESULT), serialized by the model's write lock;
        // nothing is queued.
        //
        // Threshold is recorded, not yet **enforced**: there is no live
        // `NotifyEvent` emitter on the simulator to gate, so no installed monitor's
        // reporting changes today. The active level is stored and readable so a
        // future emitter can honor it (follow-up on #500) — mirroring how the
        // monitor store landed before the report seam that reads it. Ports
        // `ocpp.v201.call.SetMonitoringLevel` →
        // `ocpp.v201.call_result.SetMonitoringLevel`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            d.on(move |req: V201SetMonitoringLevelRequest| {
                let device_model = device_model.clone();
                async move {
                    let accepted = device_model
                        .write()
                        .await
                        .set_monitoring_level(req.severity);
                    let (status, status_info) = if accepted {
                        (GenericStatusEnumType::Accepted, None)
                    } else {
                        (
                            GenericStatusEnumType::Rejected,
                            Some(StatusInfoType {
                                reason_code: "OutOfRange".to_string(),
                                additional_info: Some(
                                    "severity must be in 0..=9 (0 Danger … 9 Debug)".to_string(),
                                ),
                                custom_data: None,
                            }),
                        )
                    };
                    Ok(v201_command::v201_set_monitoring_level_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // SetDisplayMessage (OCPP 2.0.1 only) — install (or replace) a message on
        // the station's display (Issue #505, OCPP 2.0.1 Part 2 E05–E08). The
        // foundational slice of the display-message family: the `V201DisplayMessageStore`
        // it upserts into is the source of truth a future `GetDisplayMessages`
        // (query) and `ClearDisplayMessage` (remove-by-id) will read/mutate.
        // Registered only on the V201 arm — "SetDisplayMessage" has no 1.6J twin.
        //
        // A pure read-decide-answer with a single side effect (the upsert), no
        // queued command. The decision
        // (`v201_command::v201_set_display_message_status`) reads the live
        // `active_transactions` set — resolving its ids to their decimal spellings
        // exactly as the `GetTransactionStatus` / `RequestStopTransaction` handlers
        // do — so a message bound to a `transactionId` the station is not running
        // answers `UnknownTransaction` (with a `statusInfo`) and is **not** stored;
        // any other schema-valid `MessageInfoType` answers `Accepted` and is
        // upserted by `message.id` (a same-id re-install replaces, never
        // duplicates). The other `DisplayMessageStatusEnumType` variants
        // (NotSupported* / Rejected) are documented modeled seams the simulator
        // does not produce (a simulated display renders any schema-valid
        // format/priority/state).
        //
        // Trust boundary: `MessageInfoType`'s format / priority / state are typed
        // enums (an unknown wire value fails deserialization → CALLERROR before the
        // handler), and the bound `transactionId` is exact-string-matched, never
        // parsed — no wire value can panic. The `active_transactions` read lock is
        // released before the store write lock is taken (no lock held across the
        // decision), so the two never interleave. Ports
        // `ocpp.v201.call.SetDisplayMessage` →
        // `ocpp.v201.call_result.SetDisplayMessage`.
        if matches!(protocol_version, OcppVersion::V201) {
            let active_transactions = active_transactions.clone();
            let display_messages = v201_display_messages.clone();
            d.on(move |req: V201SetDisplayMessageRequest| {
                let active_transactions = active_transactions.clone();
                let display_messages = display_messages.clone();
                async move {
                    // Resolve the live transaction ids to their decimal spellings,
                    // then drop the read lock before touching the store.
                    let live_ids: Vec<String> = active_transactions
                        .read()
                        .await
                        .keys()
                        .map(ToString::to_string)
                        .collect();
                    let live_id_strs: Vec<&str> = live_ids.iter().map(String::as_str).collect();

                    let (status, status_info) =
                        v201_command::v201_set_display_message_status(&req.message, &live_id_strs);

                    // Install only an accepted message; a non-accept leaves the
                    // store unchanged (the same shape as SetMonitoringLevel's
                    // out-of-range rejection).
                    if matches!(status, DisplayMessageStatusEnumType::Accepted) {
                        display_messages.install(req.message).await;
                    }

                    Ok(v201_command::v201_set_display_message_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // InstallCertificate (OCPP 2.0.1 only) — the CSMS installs a root/CA
        // certificate into the station's trust store (Part 2, A02 / M03–M05,
        // Issue #518). The **write** side of the certificate-*management* family
        // (`InstallCertificate` / `DeleteCertificate` / `GetInstalledCertificateIds`),
        // distinct from the provisioning pair (`SignCertificate` / `CertificateSigned`
        // — the station's own certificate). Registered only on the V201 arm — 1.6
        // has no per-use trust model here.
        //
        // A pure decide-and-answer with at most one store side effect (the upsert
        // of an accepted anchor), no queued command. The decision
        // (`v201_command::v201_install_certificate_status`) is a lightweight
        // predicate on the PEM string — **no X.509 parse** (a documented simulator
        // boundary): empty / non-PEM → `Rejected`, PEM-armored but empty-bodied →
        // `Failed`, a well-formed PEM → `Accepted` and upserted into the
        // `V201CertificateStore` keyed by `certificateType` (a re-install under the
        // same use rotates the anchor, never duplicates). A non-`Accepted` outcome
        // leaves the store unchanged and carries a `statusInfo` reason (the same
        // "a non-accept leaves state unchanged" shape as `SetDisplayMessage`'s
        // refusal).
        //
        // Trust boundary: `certificate` is attacker-influenced CSMS input, treated
        // as an opaque bounded string — inspected, never parsed/unwrapped, so no
        // wire value (empty, garbage, very long, control chars) can panic;
        // `certificateType` is a closed enum (an unknown wire value fails
        // deserialization → CALLERROR before the handler). Ports
        // `ocpp.v201.call.InstallCertificate` →
        // `ocpp.v201.call_result.InstallCertificate`.
        if matches!(protocol_version, OcppVersion::V201) {
            let certificates = v201_certificates.clone();
            d.on(move |req: V201InstallCertificateRequest| {
                let certificates = certificates.clone();
                async move {
                    let status = v201_command::v201_install_certificate_status(&req.certificate);
                    let status_info = match status {
                        InstallCertificateStatusEnumType::Accepted => {
                            // Install only an accepted anchor; a re-install under
                            // the same use rotates it in place.
                            certificates
                                .install(req.certificate_type, req.certificate)
                                .await;
                            None
                        }
                        InstallCertificateStatusEnumType::Rejected => Some(StatusInfoType {
                            reason_code: "InvalidCertificate".to_string(),
                            additional_info: Some(
                                "certificate is empty or not a PEM-encoded certificate".to_string(),
                            ),
                            custom_data: None,
                        }),
                        InstallCertificateStatusEnumType::Failed => Some(StatusInfoType {
                            reason_code: "InstallationFailed".to_string(),
                            additional_info: Some(
                                "certificate is PEM-armored but carries no usable key material"
                                    .to_string(),
                            ),
                            custom_data: None,
                        }),
                    };
                    Ok(v201_command::v201_install_certificate_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // SetNetworkProfile (OCPP 2.0.1 only) — the CSMS provisions the
        // connectivity settings a station uses to reach it (the OCPP
        // transport/version, message timeout, security profile, network
        // interface, and cellular/VPN bearer), writing a
        // `NetworkConnectionProfileType` into a numbered `configurationSlot`
        // (Part 2, provisioning, B09/B10, Issue #528). Registered only on the
        // V201 arm — "SetNetworkProfile" has no 1.6J twin.
        //
        // A pure decide-and-answer with at most one store side effect (the upsert
        // of an accepted profile), no queued command — the same shape as
        // `SetDisplayMessage` / `InstallCertificate`. The decision
        // (`v201_command::v201_set_network_profile_decision`) is a lightweight
        // predicate on the profile — **no real network dial** (a documented
        // simulator boundary): a profile whose `ocppCsmsUrl` is blank names no
        // reachable CSMS → `Rejected` (nothing stored, a `statusInfo` reason
        // attached); a well-formed profile → `Accepted` and upserted into the
        // `V201NetworkProfileStore` keyed by `configurationSlot` (a re-provision
        // of the same slot rotates it in place, last-writer-wins, never a
        // duplicate). `Failed` — the spec's "accepted but could not apply" runtime
        // arm — is a documented unproduced seam (see the decision's docs).
        //
        // Trust boundary: `connection_data` is attacker-influenced CSMS input.
        // It is only inspected (`ocppCsmsUrl` presence) and stored — never dialed,
        // parsed, or unwrapped — so no wire value (blank, garbage, very long,
        // control chars) can panic; over-length string fields inside
        // `NetworkConnectionProfileType` are refused at the schema layer (→
        // CALLERROR) before the handler runs. `configuration_slot` is only used as
        // a `HashMap` key (hashed/compared), never indexed into a `Vec`, so
        // extreme values (`i32::MIN`/`MAX`) cannot panic. The snapshot-free
        // upsert needs no cross-lock: a repeated `SetNetworkProfile` for the same
        // slot deterministically last-writer-wins, a valid outcome. Ports
        // `ocpp.v201.call.SetNetworkProfile` →
        // `ocpp.v201.call_result.SetNetworkProfile`.
        if matches!(protocol_version, OcppVersion::V201) {
            let network_profiles = v201_network_profiles.clone();
            d.on(move |req: V201SetNetworkProfileRequest| {
                let network_profiles = network_profiles.clone();
                async move {
                    let status = v201_command::v201_set_network_profile_decision(&req);
                    let status_info = match status {
                        SetNetworkProfileStatusEnumType::Accepted => {
                            // Store only an accepted profile; a re-provision of the
                            // same slot rotates it in place.
                            network_profiles
                                .upsert(req.configuration_slot, req.connection_data)
                                .await;
                            None
                        }
                        SetNetworkProfileStatusEnumType::Rejected => Some(StatusInfoType {
                            reason_code: "InvalidProfile".to_string(),
                            additional_info: Some(
                                "connectionData.ocppCsmsUrl is empty; the profile names no \
                                 reachable CSMS"
                                    .to_string(),
                            ),
                            custom_data: None,
                        }),
                        SetNetworkProfileStatusEnumType::Failed => Some(StatusInfoType {
                            reason_code: "ApplicationFailed".to_string(),
                            additional_info: Some(
                                "profile accepted but the station could not apply it".to_string(),
                            ),
                            custom_data: None,
                        }),
                    };
                    Ok(v201_command::v201_set_network_profile_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // GetLog (OCPP 2.0.1 only) — the CSMS asks the station to collect a
        // diagnostics or security log and upload it to a remote location (Part 2,
        // security profile, Issue #517). The station *synchronously* acks with a
        // `LogStatusEnumType` and, when it will upload, the `filename` it will
        // produce; upload progress is then reported asynchronously via
        // `LogStatusNotification.req` (#526), driven off the command queue by the
        // `V201LogUpload` command this handler enqueues. Registered only on the
        // V201 arm — "GetLog" has no 1.6J twin.
        //
        // A station uploads one log at a time, so this handler reads the single
        // in-flight `requestId` from the `V201LogUploadStore`, lets the pure
        // decision (`v201_command::v201_get_log_decision`) resolve the status +
        // synthesized `filename`, then — on an accept — records the request as the
        // new in-flight upload. The decision: idle → `Accepted` (fresh upload); the
        // same in-flight `requestId` (a retry) → idempotent `Accepted`, same
        // filename; a different `requestId` → `AcceptedCanceled` (supersede the
        // in-progress upload). `Rejected` is a documented unproduced seam (a real
        // station that refuses concurrent uploads outright). The snapshot→begin
        // window is not lock-guarded: two racing `GetLog`s both produce valid acks
        // (one supersedes the other), so no cross-lock is needed.
        //
        // On a *fresh* accept or a *supersede* the handler queues a
        // `RemoteCommand::V201LogUpload` to run the async progress stream off the
        // inbound-CALL path (so the CALLRESULT is flushed first — no receive-loop
        // re-entrancy). A pure *retry* (the same `requestId` already in flight)
        // deliberately queues nothing: the original upload is still streaming, so
        // a second stream would double-report. `begin` returning `Some(request_id)`
        // (the id it displaced equals this one) is exactly that retry case.
        //
        // Trust boundary: `req.log.remoteLocation` is attacker-influenced CSMS
        // input and is never read here (the simulator does not actually upload);
        // `req.request_id` is only compared and formatted into the filename, never
        // parsed or indexed, so no wire value (including `i32::MIN`/`MAX`) can
        // panic. `req.log_type` is a closed enum (an unknown wire value fails
        // deserialization → CALLERROR before the handler). Ports
        // `ocpp.v201.call.GetLog` → `ocpp.v201.call_result.GetLog`.
        if matches!(protocol_version, OcppVersion::V201) {
            let log_uploads = v201_log_uploads.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201GetLogRequest| {
                let log_uploads = log_uploads.clone();
                let command_sender = command_sender.clone();
                async move {
                    // Snapshot the in-flight upload (read lock dropped before the
                    // decision), decide, then record the accepted request.
                    let in_flight = log_uploads.in_flight().await;
                    let (status, filename) = v201_command::v201_get_log_decision(&req, in_flight);

                    // Every produced status (Accepted / AcceptedCanceled) is an
                    // accept that will upload, so it becomes the new in-flight
                    // request; a re-begin of the same id is an idempotent no-op.
                    if matches!(
                        status,
                        LogStatusEnumType::Accepted | LogStatusEnumType::AcceptedCanceled
                    ) {
                        let displaced = log_uploads.begin(req.request_id).await;

                        // Drive the async progress stream — but not for a pure
                        // retry (the id it displaced is this same request, whose
                        // upload is already streaming). Fresh start (`displaced`
                        // is `None`) and supersede (`displaced` is a *different*
                        // id) both queue a new stream.
                        if displaced != Some(req.request_id)
                            && command_sender
                                .send(RemoteCommand::V201LogUpload {
                                    request_id: req.request_id,
                                })
                                .is_err()
                        {
                            // Consumer gone (CP shutting down): the ack is still
                            // honest — the request was accepted — but the async
                            // progress cannot be streamed. Surface the drop.
                            warn!(
                                "v201 GetLog: consumer gone, cannot stream \
                                 LogStatusNotification for request {}",
                                req.request_id
                            );
                        }
                    }

                    Ok(v201_command::v201_get_log_response(status, None, filename))
                }
            });
        }

        // GetInstalledCertificateIds (OCPP 2.0.1 only) — the CSMS asks the station
        // to enumerate the trust anchors it currently holds (Part 2, A02 / M03–M05,
        // Issue #521). The **read** side of the certificate-*management* family
        // (`InstallCertificate` writes, this reads, `DeleteCertificate` removes),
        // over the same `V201CertificateStore` the `InstallCertificate` handler
        // populates. Registered only on the V201 arm — 1.6 has no per-use trust
        // model here.
        //
        // A pure read-and-answer: unlike `GetDisplayMessages` (whose matches stream
        // back asynchronously as `NotifyDisplayMessages`), the hash chain rides on
        // the `GetInstalledCertificateIds.conf` itself, so this snapshots the store,
        // resolves the optional `certificateType` filter against it
        // (`v201_command::v201_get_installed_certificate_ids_matches`), and answers
        // directly — no queued command, no store mutation. `Accepted` with a chain
        // when ≥1 anchor matched, `NotFound` with no chain otherwise.
        //
        // The reported per-anchor `CertificateHashDataType` is a deterministic
        // placeholder derived from `(use, PEM)` (`v201_certificate_hash_data`) — the
        // simulator does no X.509 parse (the "no PKI" boundary Issue #518 set) — and
        // is the shared hash model a future `DeleteCertificate` (#522) reuses so an
        // enumerated hash round-trips to a delete of the same anchor.
        //
        // Trust boundary: `certificateType` is an `Option<Vec<_>>` of a closed enum
        // (an unknown wire value fails deserialization → CALLERROR before the
        // handler); a filter naming a category the store cannot hold simply matches
        // nothing. Ports `ocpp.v201.call.GetInstalledCertificateIds` →
        // `ocpp.v201.call_result.GetInstalledCertificateIds`.
        if matches!(protocol_version, OcppVersion::V201) {
            let certificates = v201_certificates.clone();
            d.on(move |req: V201GetInstalledCertificateIdsRequest| {
                let certificates = certificates.clone();
                async move {
                    let snapshot = certificates.snapshot().await;
                    let chain = v201_command::v201_get_installed_certificate_ids_matches(
                        req.certificate_type.as_deref(),
                        &snapshot,
                    );
                    Ok(v201_command::v201_get_installed_certificate_ids_response(
                        chain,
                    ))
                }
            });
        }

        // CertificateSigned (OCPP 2.0.1 only) — the CSMS delivers a CA-signed
        // certificate chain to the station (Part 2, A02). The delivery terminus of
        // the certificate-*provisioning* flow (`SignCertificate` → `CertificateSigned`
        // — the station's *own* certificate), distinct from the certificate-
        // *management* family (`InstallCertificate` / `DeleteCertificate` /
        // `GetInstalledCertificateIds` — trust anchors). Registered only on the
        // V201 arm — 1.6 has no such provisioning message here.
        //
        // A **stateless** pure decide-and-answer: no store side effect, no queued
        // command. The decision (`v201_command::v201_certificate_signed_status`) is
        // a lightweight predicate on the PEM string — **no X.509 parse** (the same
        // documented simulator boundary as `InstallCertificate`): a PEM-armored
        // chain with a non-empty body → `Accepted`; an empty / blank / non-PEM /
        // empty-bodied chain → `Rejected` with a `statusInfo` reason. Unlike
        // `InstallCertificate`'s three-value enum, `CertificateSignedStatusEnumType`
        // is binary, so the "recognized but unusable" arm collapses into `Rejected`.
        // A future station-certificate store could persist an accepted chain; for
        // this slice a stateless responder is sufficient and keeps the diff small.
        //
        // Trust boundary: `certificate_chain` is attacker-influenced CSMS input,
        // treated as an opaque bounded string — inspected, never parsed/unwrapped,
        // so no wire value (empty, garbage, very long, control chars) can panic.
        // `certificate_type` is a closed enum (`ChargingStationCertificate` /
        // `V2GCertificate`, or absent) and does not change the decision; an unknown
        // wire value fails deserialization → CALLERROR before the handler. Ports
        // `ocpp.v201.call.CertificateSigned` →
        // `ocpp.v201.call_result.CertificateSigned`.
        if matches!(protocol_version, OcppVersion::V201) {
            d.on(move |req: V201CertificateSignedRequest| async move {
                let status = v201_command::v201_certificate_signed_status(&req.certificate_chain);
                let status_info = match status {
                    CertificateSignedStatusEnumType::Accepted => None,
                    CertificateSignedStatusEnumType::Rejected => Some(StatusInfoType {
                        reason_code: "InvalidChain".to_string(),
                        additional_info: Some(
                            "certificate chain is empty or not a usable PEM-encoded certificate"
                                .to_string(),
                        ),
                        custom_data: None,
                    }),
                };
                Ok(v201_command::v201_certificate_signed_response(
                    status,
                    status_info,
                ))
            });
        }

        // CustomerInformation (OCPP 2.0.1 only) — the CSMS asks the station to
        // report and/or clear the customer data it has stored, identifying the
        // customer by up to three optional selectors: a hashed customer
        // certificate, an idToken, or a free-form customerIdentifier (Part 2,
        // N09/N10 — the privacy / GDPR command, Issue #530). Registered only on the
        // V201 arm — 1.6J has no equivalent, so a 1.6J CP answers CALLERROR.
        //
        // The decision (`v201_command::v201_customer_information_decision`) is
        // `Accepted` when the request names at least one selector and asks for at
        // least one action (report or clear), and `Invalid` otherwise (no
        // customer to act on, or nothing to do). The handler attaches a statusInfo
        // reason to the `Invalid` arm. `Rejected` is a documented unproduced seam
        // (the simulator models no authorization-refusal policy).
        //
        // The synchronous ack is followed by an asynchronous report stream only
        // when the request was `Accepted` **and** set `report: true` (Issue #537).
        // Then the handler records the request in the `V201CustomerInformationStore`
        // and queues a `RemoteCommand::V201CustomerInformationReport` to drive the
        // paged `NotifyCustomerInformation` stream off the inbound-CALL path (so
        // the `CustomerInformation` CALLRESULT flushes before the first
        // notification — no receive-loop re-entrancy, the same discipline as
        // GetLog / the firmware handlers). A `clear`-only accept queues nothing
        // (there is no data to report). A retry of a `requestId` whose stream is
        // still in flight queues no second stream — `begin` returning `false` is
        // exactly that case — so the CSMS is not double-reported. If the consumer
        // is gone (CP shutting down) the queue fails; the ack is still honest, and
        // the in-flight marker is rolled back so a later retry can report afresh.
        //
        // Trust boundary: the three selectors are attacker-influenced CSMS input,
        // inspected only for presence and never unwrapped/parsed/indexed, so no wire
        // value can panic; `request_id` is only echoed/compared (into the store and
        // the queued command), never parsed or indexed, so extreme values
        // (`i32::MIN`/`MAX`) are safe. Over-length selector fields (e.g.
        // `customerIdentifier`, maxLength 64) are refused at the schema layer (→
        // CALLERROR) before the handler runs. Ports
        // `ocpp.v201.call.CustomerInformation` →
        // `ocpp.v201.call_result.CustomerInformation`.
        if matches!(protocol_version, OcppVersion::V201) {
            let customer_information_reports = v201_customer_information_reports.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201CustomerInformationRequest| {
                let customer_information_reports = customer_information_reports.clone();
                let command_sender = command_sender.clone();
                async move {
                    let status = v201_command::v201_customer_information_decision(&req);
                    let status_info = match status {
                        CustomerInformationStatusEnumType::Invalid => Some(StatusInfoType {
                            reason_code: "InvalidRequest".to_string(),
                            additional_info: Some(
                                "CustomerInformation named no usable customer selector, or \
                                 requested neither report nor clear"
                                    .to_string(),
                            ),
                            custom_data: None,
                        }),
                        CustomerInformationStatusEnumType::Accepted
                        | CustomerInformationStatusEnumType::Rejected => None,
                    };

                    // Drive the async report stream only for an accepted *reporting*
                    // request, and only if this `requestId` is not already
                    // streaming (dedup a retry). `begin` returns `true` exactly when
                    // it newly recorded the id; the send then runs off the CALL path.
                    if status == CustomerInformationStatusEnumType::Accepted
                        && req.report
                        && customer_information_reports.begin(req.request_id).await
                        && command_sender
                            .send(RemoteCommand::V201CustomerInformationReport {
                                request_id: req.request_id,
                            })
                            .is_err()
                    {
                        // Consumer gone (CP shutting down): the ack is still honest
                        // — the request was accepted — but the stream cannot run.
                        // Roll back the in-flight marker so a later retry can report
                        // afresh, and surface the drop.
                        customer_information_reports.complete(req.request_id).await;
                        warn!(
                            "v201 CustomerInformation: consumer gone, cannot stream \
                             NotifyCustomerInformation for request {}",
                            req.request_id
                        );
                    }

                    Ok(v201_command::v201_customer_information_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // PublishFirmware (OCPP 2.0.1 only) — the CSMS tells a station acting as a
        // Local Controller to download a firmware image once and cache it locally,
        // so the chargers behind it can pull it over the LAN instead of each
        // fetching it from the CSMS over the WAN (Part 2, the firmware-cache
        // trigger, Issue #538). Registered only on the V201 arm — 1.6J has no
        // `PublishFirmware`, so a 1.6J CP answers CALLERROR.
        //
        // A pure decide-and-answer synchronous ack, followed by an asynchronous
        // progress stream when the request is `Accepted` (Issue #540). The decision
        // (`v201_command::v201_publish_firmware_decision`) is a lightweight shape
        // predicate — **no URL is opened/followed** (the same documented simulator
        // boundary as `CertificateSigned`'s no-PKI arm): a non-empty `location`
        // plus a well-shaped 32-char hex `checksum` → `Accepted`; an empty
        // `location` or a mis-shaped `checksum` → `Rejected` with a `statusInfo`
        // reason. `GenericStatusEnumType` is binary, so there is no third arm.
        //
        // On an `Accepted` request the handler records it in the
        // `V201PublishFirmwareStore` and queues a
        // `RemoteCommand::V201PublishFirmwareStatus` to drive the
        // `PublishFirmwareStatusNotification` progress stream off the inbound-CALL
        // path (so the `PublishFirmware` CALLRESULT flushes before the first
        // notification — no receive-loop re-entrancy, the same discipline as
        // GetLog / the firmware-update handler). A `Rejected` request queues
        // nothing. A retry of a `requestId` whose stream is still in flight queues
        // no second stream — `begin` returning `false` is exactly that case — so
        // the CSMS is not double-reported. If the consumer is gone (CP shutting
        // down) the queue fails; the ack is still honest, and the in-flight marker
        // is rolled back so a later retry can publish afresh.
        //
        // Trust boundary: `location` and `checksum` are attacker-influenced CSMS
        // input, inspected only for shape and never opened/followed/parsed/indexed,
        // so no wire value (empty, garbage, very long, control chars) can panic;
        // `request_id` is only echoed/compared (into the store and the queued
        // command), never parsed or indexed, so extreme values (`i32::MIN`/`MAX`)
        // are safe. Over-length fields (`location` maxLength 512, `checksum`
        // maxLength 32) are refused at the schema layer (→ CALLERROR) before the
        // handler runs. Ports `ocpp.v201.call.PublishFirmware` →
        // `ocpp.v201.call_result.PublishFirmware`.
        if matches!(protocol_version, OcppVersion::V201) {
            let publish_firmwares = v201_publish_firmwares.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201PublishFirmwareRequest| {
                let publish_firmwares = publish_firmwares.clone();
                let command_sender = command_sender.clone();
                async move {
                    let status = v201_command::v201_publish_firmware_decision(&req);
                    let status_info = match status {
                        GenericStatusEnumType::Accepted => None,
                        GenericStatusEnumType::Rejected => Some(StatusInfoType {
                            reason_code: "InvalidRequest".to_string(),
                            additional_info: Some(
                                "PublishFirmware requires a non-empty location and a 32-char hex \
                                 checksum"
                                    .to_string(),
                            ),
                            custom_data: None,
                        }),
                    };

                    // Drive the async progress stream only for an accepted request,
                    // and only if this `requestId` is not already streaming (dedup a
                    // retry). `begin` returns `true` exactly when it newly recorded
                    // the id; the send then runs off the CALL path.
                    if status == GenericStatusEnumType::Accepted
                        && publish_firmwares.begin(req.request_id).await
                        && command_sender
                            .send(RemoteCommand::V201PublishFirmwareStatus {
                                request_id: req.request_id,
                            })
                            .is_err()
                    {
                        // Consumer gone (CP shutting down): the ack is still honest
                        // — the request was accepted — but the stream cannot run.
                        // Roll back the in-flight marker so a later retry can publish
                        // afresh, and surface the drop.
                        publish_firmwares.complete(req.request_id).await;
                        warn!(
                            "v201 PublishFirmware: consumer gone, cannot stream \
                             PublishFirmwareStatusNotification for request {}",
                            req.request_id
                        );
                    }

                    Ok(v201_command::v201_publish_firmware_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // DeleteCertificate (OCPP 2.0.1 only) — the CSMS removes a previously
        // installed trust anchor, named by its `CertificateHashDataType` (Part 2,
        // A02 / M03–M05, Issue #522). The **remove** side of the certificate-
        // *management* family (`InstallCertificate` writes, `GetInstalledCertificateIds`
        // reads, this removes), over the same `V201CertificateStore` those two use.
        // Registered only on the V201 arm — 1.6 has no per-use trust model here.
        //
        // A read-resolve-then-remove: snapshot the store, resolve the requested hash
        // to the anchor it names (`v201_command::v201_delete_certificate_target`,
        // which reuses the shared `v201_certificate_hash_data` seam so a hash
        // enumerated by `GetInstalledCertificateIds` round-trips to a delete of the
        // same anchor), then act:
        //
        // - no anchor matched → `NotFound`, nothing removed. A re-delete of an
        //   already-removed anchor takes this arm, so delete is idempotent.
        // - matched → `remove(use)` under the write lock. `Some` (removed) →
        //   `Accepted`; `None` means the matched anchor was gone by removal time — a
        //   concurrent `DeleteCertificate` for the same anchor raced in between the
        //   snapshot and the remove — which is the "matched but could not remove"
        //   arm → `Failed`.
        //
        // Trust boundary: `certificate_hash_data` is attacker-influenced CSMS input;
        // its fields are only ever string-compared against derived hashes, never
        // parsed or unwrapped, so no wire value (arbitrary bytes, over-long strings)
        // can panic. Ports `ocpp.v201.call.DeleteCertificate` →
        // `ocpp.v201.call_result.DeleteCertificate`.
        if matches!(protocol_version, OcppVersion::V201) {
            let certificates = v201_certificates.clone();
            d.on(move |req: V201DeleteCertificateRequest| {
                let certificates = certificates.clone();
                async move {
                    let snapshot = certificates.snapshot().await;
                    let (status, status_info) = match v201_command::v201_delete_certificate_target(
                        &req.certificate_hash_data,
                        &snapshot,
                    ) {
                        None => (
                            DeleteCertificateStatusEnumType::NotFound,
                            Some(StatusInfoType {
                                reason_code: "NotFound".to_string(),
                                additional_info: Some(
                                    "no installed certificate matches the requested hash"
                                        .to_string(),
                                ),
                                custom_data: None,
                            }),
                        ),
                        Some(use_) => {
                            if certificates.remove(use_).await.is_some() {
                                (DeleteCertificateStatusEnumType::Accepted, None)
                            } else {
                                // Matched a snapshot anchor, but it was already gone
                                // by removal time (a concurrent delete raced in).
                                (
                                    DeleteCertificateStatusEnumType::Failed,
                                    Some(StatusInfoType {
                                        reason_code: "RemovalFailed".to_string(),
                                        additional_info: Some(
                                            "the matched certificate was already removed"
                                                .to_string(),
                                        ),
                                        custom_data: None,
                                    }),
                                )
                            }
                        }
                    };
                    Ok(v201_command::v201_delete_certificate_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // UpdateFirmware (OCPP 2.0.1 only) — the CSMS asks the station to fetch and
        // install a firmware image from a URL, identifying the rollout by a
        // `requestId` (Part 2, firmware management, L01–L03, Issue #532). The 2.0.1
        // successor to the 1.6J empty-conf handler above: where 1.6J answers an
        // empty `.conf`, 2.0.1 carries an `UpdateFirmwareStatusEnumType`. Registered
        // on the V201 arm only (the 1.6J arm keeps the empty-conf handler); the two
        // share the action name `"UpdateFirmware"`, so this arm-split is what keeps a
        // 2.0.1 CSMS from getting the wrong (empty) ack — the exact gap #532 fixes.
        //
        // Mirrors the `GetLog` single-in-flight / supersede model: snapshot the
        // in-flight `requestId` from the `V201FirmwareUpdateStore` (read lock
        // dropped before the decision), decide
        // (`v201_command::v201_update_firmware_decision`), then on an accept record
        // the request. `Accepted` (fresh or retry) and `AcceptedCanceled` (supersede)
        // both `begin` the request as the update now in flight (a re-begin of the
        // same id is an idempotent no-op); `InvalidCertificate` refuses a present-
        // but-unusable signing certificate and stores nothing. On a *fresh* accept
        // or a *supersede* the handler queues a `RemoteCommand::V201FirmwareUpdate`
        // to run the async `FirmwareStatusNotification` progress stream (#534) off
        // the inbound-CALL path (so the CALLRESULT is flushed first — no
        // receive-loop re-entrancy). A pure *retry* (the same `requestId` already
        // in flight) queues nothing: the original rollout is still streaming, so a
        // second stream would double-report. `begin` returning `Some(request_id)`
        // (the id it displaced equals this one) is exactly that retry case — the
        // same discipline the `GetLog` handler uses.
        //
        // Trust boundary: `firmware.location` / `firmware.signature` are never read;
        // `firmware.signing_certificate` is only inspected for PEM shape (no X.509
        // parse — the same boundary `InstallCertificate` set); `request_id` is only
        // compared and stored, never indexed, so no wire value (incl. `i32::MIN` /
        // `MAX`) can panic. Over-length fields are refused at the schema layer
        // (→ CALLERROR) before the handler runs. Ports
        // `ocpp.v201.call.UpdateFirmware` → `ocpp.v201.call_result.UpdateFirmware`.
        if matches!(protocol_version, OcppVersion::V201) {
            let firmware_updates = v201_firmware_updates.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201UpdateFirmwareRequest| {
                let firmware_updates = firmware_updates.clone();
                let command_sender = command_sender.clone();
                async move {
                    let in_flight = firmware_updates.in_flight().await;
                    let status = v201_command::v201_update_firmware_decision(&req, in_flight);

                    // An accept (fresh, retry, or supersede) becomes the new
                    // in-flight update; a re-begin of the same id is idempotent. A
                    // refusal (InvalidCertificate, or the unproduced Rejected /
                    // RevokedCertificate seams) records nothing.
                    if matches!(
                        status,
                        UpdateFirmwareStatusEnumType::Accepted
                            | UpdateFirmwareStatusEnumType::AcceptedCanceled
                    ) {
                        let displaced = firmware_updates.begin(req.request_id).await;

                        // Drive the async progress stream — but not for a pure
                        // retry (the id it displaced is this same request, whose
                        // rollout is already streaming). Fresh start (`displaced`
                        // is `None`) and supersede (`displaced` is a *different*
                        // id) both queue a new stream.
                        if displaced != Some(req.request_id)
                            && command_sender
                                .send(RemoteCommand::V201FirmwareUpdate {
                                    request_id: req.request_id,
                                })
                                .is_err()
                        {
                            // Consumer gone (CP shutting down): the ack is still
                            // honest — the request was accepted — but the async
                            // progress cannot be streamed. Surface the drop.
                            warn!(
                                "v201 UpdateFirmware: consumer gone, cannot stream \
                                 FirmwareStatusNotification for request {}",
                                req.request_id
                            );
                        }
                    }

                    let status_info = match status {
                        UpdateFirmwareStatusEnumType::InvalidCertificate => Some(StatusInfoType {
                            reason_code: "InvalidCertificate".to_string(),
                            additional_info: Some(
                                "the firmware signing certificate is empty or not a usable \
                                 PEM-encoded certificate"
                                    .to_string(),
                            ),
                            custom_data: None,
                        }),
                        _ => None,
                    };

                    Ok(v201_command::v201_update_firmware_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // SetMonitoringBase (OCPP 2.0.1 only) — select which **set** of
        // pre-configured monitors is active for the device-model monitoring family
        // (#494/#495/#499/#503). Where `SetMonitoringLevel` sets the reporting
        // severity threshold, this selects the monitoring *base*: `All` (every
        // monitor), `FactoryDefault` (the station's pre-configured set), or
        // `HardWiredOnly` (only physically hard-wired monitors). The selected base
        // is stored on the shared `V201DeviceModel` (behind the write lock) and
        // readable via `active_monitoring_base()`. Registered only on the V201 arm
        // — "SetMonitoringBase" has no 1.6J twin.
        //
        // Decision / modeled seam: `All` and `FactoryDefault` are recorded and
        // answered `Accepted`. `HardWiredOnly` is a modeled seam — the monitor
        // store models only CSMS-installed monitors and has no hard-wired-monitor
        // seam yet, so the station has no hard-wired set to activate. Rather than
        // silently record a base it cannot honor, it answers `NotSupported` with a
        // `StatusInfoType` reason and leaves the stored base unchanged (the same
        // "a non-accept leaves state unchanged" shape as `SetMonitoringLevel`'s
        // out-of-range rejection). The decision is a pure match on the typed
        // `monitoringBase`, so no wire value can panic. The write is applied on the
        // CALL path (cheap, and must be visible to the CALLRESULT), serialized by
        // the model's write lock; nothing is queued.
        //
        // Base is recorded, not yet **enforced**: selecting a base changes no
        // monitor's activation today (there is no pre-configured / hard-wired
        // monitor seam to gate). It is stored and readable so that seam can honor
        // it later — mirroring how the reporting level (#503) and the monitor store
        // (#494) landed before the emitters that read them. Ports
        // `ocpp.v201.call.SetMonitoringBase` →
        // `ocpp.v201.call_result.SetMonitoringBase`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            d.on(move |req: V201SetMonitoringBaseRequest| {
                let device_model = device_model.clone();
                async move {
                    let status = device_model
                        .write()
                        .await
                        .set_monitoring_base(req.monitoring_base);
                    let status_info = match status {
                        GenericDeviceModelStatusEnumType::Accepted => None,
                        _ => Some(StatusInfoType {
                            reason_code: "NotSupported".to_string(),
                            additional_info: Some(
                                "HardWiredOnly base is not modeled: no hard-wired monitors exist"
                                    .to_string(),
                            ),
                            custom_data: None,
                        }),
                    };
                    Ok(v201_command::v201_set_monitoring_base_response(
                        status,
                        status_info,
                    ))
                }
            });
        }

        // GetBaseReport (OCPP 2.0.1 only) — the device-model **report** seam,
        // completing the read (`GetVariables`) / write (`SetVariables`) / report
        // triad over the same `V201DeviceModel`. Unlike the synchronous read/write
        // handlers, `GetBaseReport` is a two-part exchange: the station
        // acknowledges here with a `GenericDeviceModelStatusEnumType` and then
        // streams the actual inventory back asynchronously as `NotifyReport`
        // CALL(s), correlated by `requestId`.
        //
        // The report snapshot is computed here, under the model's *read* lock, so
        // the queued side effect carries owned data and touches no shared state
        // when it runs. Status:
        // - a non-empty report → `Accepted`, and a `V201NotifyReport` is queued
        //   on the command channel (sent after this CALLRESULT is flushed, off the
        //   inbound-CALL path — same discipline as `TriggerMessage`);
        // - an empty report (e.g. `SummaryInventory` on a healthy simulator) →
        //   `EmptyResultSet`, and nothing is queued (no data to stream);
        // - if the command consumer has gone away (CP shutting down) an accepted
        //   report cannot be delivered, so we answer `Rejected` rather than
        //   promise a `NotifyReport` that will never arrive.
        //
        // Trust boundary: the request carries only a numeric `requestId` and an
        // enum `reportBase` — both schema-bounded, nothing stored — so there is no
        // panic/unwrap surface on wire input. Registered on the V201 arm only;
        // "GetBaseReport" has no 1.6J twin. Ports `ocpp.v201.call.GetBaseReport` →
        // `ocpp.v201.call.NotifyReport`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201GetBaseReportRequest| {
                let device_model = device_model.clone();
                let command_sender = command_sender.clone();
                async move {
                    let report_data = device_model.read().await.report(req.report_base);
                    let status = if report_data.is_empty() {
                        // Request understood, but nothing matched — a legitimate
                        // outcome (e.g. SummaryInventory on a fresh station). No
                        // NotifyReport follows.
                        GenericDeviceModelStatusEnumType::EmptyResultSet
                    } else if command_sender
                        .send(RemoteCommand::V201NotifyReport {
                            request_id: req.request_id,
                            report_data,
                        })
                        .is_ok()
                    {
                        GenericDeviceModelStatusEnumType::Accepted
                    } else {
                        // Consumer gone: we cannot stream the report, so don't
                        // claim we will.
                        GenericDeviceModelStatusEnumType::Rejected
                    };
                    Ok(V201GetBaseReportResponse {
                        status,
                        status_info: None,
                        custom_data: None,
                    })
                }
            });
        }

        // GetReport (OCPP 2.0.1 only) — the *filtered* sibling of GetBaseReport
        // over the same `V201DeviceModel`, completing the device-model report
        // family (#486). It shares GetBaseReport's two-part seam exactly: the
        // station acknowledges here with a `GenericDeviceModelStatusEnumType`,
        // then streams the actual inventory asynchronously as `NotifyReport`
        // CALL(s) correlated by `requestId` — reusing the *same*
        // `RemoteCommand::V201NotifyReport` command and off-CALL-path queueing.
        // It differs from GetBaseReport only in *selection*: instead of a coarse
        // `reportBase` enum, the station filters its device-model inventory by an
        // optional `componentVariable[]` (specific component-variables) and/or an
        // optional `componentCriteria[]` (`Active`/`Available`/`Enabled`/`Problem`).
        //
        // The filtered snapshot is computed here under the model's *read* lock —
        // via the pure `report_filtered`, which owns the selection semantics — so
        // the queued side effect carries owned data and touches no shared state
        // when it runs. Status mirrors GetBaseReport:
        // - a non-empty filtered report → `Accepted`, and a `V201NotifyReport` is
        //   queued (sent after this CALLRESULT is flushed, off the inbound-CALL
        //   path);
        // - request understood but nothing matched the filter (e.g.
        //   `componentCriteria = [Problem]` on a healthy simulator, or a
        //   `componentVariable` naming an unknown/EVSE-scoped component the flat
        //   seed does not hold) → `EmptyResultSet`, and nothing is queued;
        // - the command consumer has gone away (CP shutting down) → `Rejected`
        //   rather than promise a `NotifyReport` that will never arrive.
        //
        // Trust boundary: the request carries only a numeric `requestId`, an
        // optional schema-bounded `componentCriteria[]` enum list, and a
        // schema-bounded `componentVariable[]` — nothing is parsed into an index
        // or unwrapped, so malformed/unknown criteria are rejected upstream by
        // `SchemaValidator::v201()` and nothing here panics on wire input.
        // Registered on the V201 arm only; "GetReport" has no 1.6J twin. Ports
        // `ocpp.v201.call.GetReport` → `ocpp.v201.call.NotifyReport`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201GetReportRequest| {
                let device_model = device_model.clone();
                let command_sender = command_sender.clone();
                async move {
                    let report_data = device_model.read().await.report_filtered(
                        req.component_variable.as_deref(),
                        req.component_criteria.as_deref(),
                    );
                    let status = if report_data.is_empty() {
                        // Understood, but nothing matched the filter — a
                        // legitimate outcome. No NotifyReport follows.
                        GenericDeviceModelStatusEnumType::EmptyResultSet
                    } else if command_sender
                        .send(RemoteCommand::V201NotifyReport {
                            request_id: req.request_id,
                            report_data,
                        })
                        .is_ok()
                    {
                        GenericDeviceModelStatusEnumType::Accepted
                    } else {
                        // Consumer gone: we cannot stream the report, so don't
                        // claim we will.
                        GenericDeviceModelStatusEnumType::Rejected
                    };
                    Ok(V201GetReportResponse {
                        status,
                        status_info: None,
                        custom_data: None,
                    })
                }
            });
        }

        // GetMonitoringReport (OCPP 2.0.1 only) — the *monitoring* counterpart of
        // GetReport, completing the device-model report family (#493). Where
        // GetReport streams the device-model *inventory* (component/variable rows)
        // via NotifyReport, GetMonitoringReport streams the `SetVariableMonitoring`
        // *monitors installed on* those variables (thresholds, deltas, periodics)
        // via NotifyMonitoringReport, filtered by an optional `componentVariable[]`
        // and/or `monitoringCriteria[]`. It shares GetReport's two-part seam
        // exactly: the station acknowledges here with a
        // `GenericDeviceModelStatusEnumType`, then streams the monitors
        // asynchronously as a `NotifyMonitoringReport` CALL correlated by
        // `requestId` — via its own `RemoteCommand::V201NotifyMonitoringReport`
        // command and the same off-CALL-path queueing discipline.
        //
        // The filtered snapshot is computed here under the model's *read* lock so
        // the queued side effect carries owned data and touches no shared state
        // when it runs. Status mirrors GetReport (the pure
        // `v201_get_monitoring_report_status` owns the mapping):
        // - a non-empty snapshot → `Accepted`, and a `V201NotifyMonitoringReport`
        //   is queued (sent after this CALLRESULT is flushed, off the inbound-CALL
        //   path);
        // - request understood but nothing matched → `EmptyResultSet`, nothing
        //   queued;
        // - the command consumer has gone away (CP shutting down) → `Rejected`
        //   rather than promise a `NotifyMonitoringReport` that will never arrive.
        //
        // Modeled answer (issue #493, option b): the simulator installs no
        // per-variable monitors yet, so `monitoring_snapshot` is always empty
        // today and the live outcome is `EmptyResultSet`. The `Accepted` /
        // `Rejected` branches are wired and covered by the pure unit tests; a
        // follow-up adds the monitor store + `SetVariableMonitoring` that
        // populates it.
        //
        // Trust boundary: the request carries only a numeric `requestId`, an
        // optional schema-bounded `monitoringCriteria[]` enum list, and a
        // schema-bounded `componentVariable[]` — nothing is parsed into an index
        // or unwrapped, so malformed/unknown criteria are rejected upstream by
        // `SchemaValidator::v201()` and nothing here panics on wire input.
        // Registered on the V201 arm only; "GetMonitoringReport" has no 1.6J twin.
        // Ports `ocpp.v201.call.GetMonitoringReport` →
        // `ocpp.v201.call.NotifyMonitoringReport`.
        if matches!(protocol_version, OcppVersion::V201) {
            let device_model = v201_device_model.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201GetMonitoringReportRequest| {
                let device_model = device_model.clone();
                let command_sender = command_sender.clone();
                async move {
                    let monitor_data = device_model.read().await.monitoring_snapshot(
                        req.component_variable.as_deref(),
                        req.monitoring_criteria.as_deref(),
                    );
                    // Compute `has_monitors` before `monitor_data` is moved into the
                    // queued command. `queued` short-circuits: an empty snapshot
                    // never attempts the send (nothing to stream). A non-empty
                    // snapshot is queued unless the consumer has gone away.
                    let has_monitors = !monitor_data.is_empty();
                    let queued = has_monitors
                        && command_sender
                            .send(RemoteCommand::V201NotifyMonitoringReport {
                                request_id: req.request_id,
                                monitor_data,
                            })
                            .is_ok();
                    let status =
                        v201_command::v201_get_monitoring_report_status(has_monitors, queued);
                    Ok(v201_command::v201_get_monitoring_report_response(
                        status, None,
                    ))
                }
            });
        }

        // GetChargingProfiles (OCPP 2.0.1 only) — the query half of the smart-
        // charging report flow, alongside SetChargingProfile (#472) /
        // ClearChargingProfile (#477). Like GetBaseReport, it is a two-part
        // exchange: the station answers synchronously with `Accepted` /
        // `NoProfiles`, then streams the matching installed profiles asynchronously
        // as one or more `ReportChargingProfiles` CALL(s), correlated by
        // `requestId`.
        //
        // The match set is resolved here off a snapshot of the `v201_tx_profiles`
        // store (taken under its read lock on the CALL path), so the queued side
        // effect carries owned data and touches no shared state when it runs.
        // Status:
        // - ≥1 matching profile → `Accepted`, and a `V201ReportChargingProfiles`
        //   is queued on the command channel (sent after this CALLRESULT is
        //   flushed, off the inbound-CALL path — same discipline as GetBaseReport);
        // - no matching profile → `NoProfiles`, and nothing is queued.
        //
        // Unlike GetBaseReport, `GetChargingProfileStatusEnumType` has no
        // `Rejected`: its only two values are `Accepted` / `NoProfiles`. So when
        // the command consumer has gone away (CP shutting down) we still answer
        // `Accepted` for a non-empty match — that is the honest status ("I do have
        // matching profiles") — and log that the report could not be streamed,
        // rather than misreport `NoProfiles`.
        //
        // Trust boundary: the request carries only a numeric `requestId`, an
        // optional numeric `evseId`, and a schema-bounded criterion — nothing is
        // parsed into an index or unwrapped, so a `0` / negative / out-of-range
        // `evseId` simply misses the snapshot (→ `NoProfiles`) and never panics.
        // Registered on the V201 arm only; "GetChargingProfiles" has no 1.6J twin.
        // Ports `ocpp.v201.call.GetChargingProfiles` →
        // `ocpp.v201.call.ReportChargingProfiles`.
        if matches!(protocol_version, OcppVersion::V201) {
            let v201_tx_profiles = v201_tx_profiles.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201GetChargingProfilesRequest| {
                let v201_tx_profiles = v201_tx_profiles.clone();
                let command_sender = command_sender.clone();
                async move {
                    // Snapshot off the store lock, then resolve the match set with
                    // the pure selector. In the simulator the CSMS's CALLs are
                    // serialized through the single dispatcher, so the snapshot
                    // cannot be raced by a concurrent install/clear.
                    let installed = v201_tx_profiles.snapshot().await;
                    let matches = v201_command::v201_get_charging_profiles_matches(
                        req.evse_id,
                        &req.charging_profile,
                        &installed,
                    );
                    let matched = !matches.is_empty();
                    if matched
                        && command_sender
                            .send(RemoteCommand::V201ReportChargingProfiles {
                                request_id: req.request_id,
                                profiles: matches,
                            })
                            .is_err()
                    {
                        // Consumer gone: we cannot stream the report, but the
                        // status enum has no Rejected — Accepted is still the
                        // honest answer for a non-empty match. Surface the drop.
                        warn!(
                            "v201 GetChargingProfiles: consumer gone, cannot stream \
                             ReportChargingProfiles for request {}",
                            req.request_id
                        );
                    }
                    Ok(v201_command::v201_get_charging_profiles_response(matched))
                }
            });
        }

        // GetDisplayMessages (OCPP 2.0.1 only) — the query half of the display-
        // message family (#508), alongside SetDisplayMessage (#505) and a future
        // ClearDisplayMessage (#509). Like GetChargingProfiles, it is a two-part
        // exchange: the station answers synchronously with `Accepted` / `Unknown`,
        // then streams the matching installed messages asynchronously as one or
        // more `NotifyDisplayMessages` CALL(s), correlated by `requestId`.
        //
        // The match set is resolved here off a snapshot of the
        // `v201_display_messages` store (taken under its read lock on the CALL
        // path), so the queued side effect carries owned data and touches no
        // shared state when it runs. Status:
        // - ≥1 matching message → `Accepted`, and a `V201NotifyDisplayMessages`
        //   is queued on the command channel (sent after this CALLRESULT is
        //   flushed, off the inbound-CALL path — same discipline as
        //   GetChargingProfiles);
        // - no matching message → `Unknown`, and nothing is queued.
        //
        // Like `GetChargingProfileStatusEnumType`, `GetDisplayMessagesStatusEnumType`
        // has no `Rejected`: its only two values are `Accepted` / `Unknown`. So
        // when the command consumer has gone away (CP shutting down) we still
        // answer `Accepted` for a non-empty match — the honest status ("I do have
        // matching messages") — and log that the report could not be streamed,
        // rather than misreport `Unknown`.
        //
        // Trust boundary: the request carries only a numeric `requestId`, an
        // optional numeric `id[]`, and schema-bounded `priority` / `state` enums —
        // nothing is parsed into an index or unwrapped, so an unknown/negative id,
        // or a priority/state naming no installed message, simply misses the
        // snapshot (→ `Unknown`) and never panics.
        // Registered on the V201 arm only; "GetDisplayMessages" has no 1.6J twin.
        // Ports `ocpp.v201.call.GetDisplayMessages` →
        // `ocpp.v201.call.NotifyDisplayMessages`.
        if matches!(protocol_version, OcppVersion::V201) {
            let v201_display_messages = v201_display_messages.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201GetDisplayMessagesRequest| {
                let v201_display_messages = v201_display_messages.clone();
                let command_sender = command_sender.clone();
                async move {
                    // Snapshot off the store lock, then resolve the match set with
                    // the pure selector. In the simulator the CSMS's CALLs are
                    // serialized through the single dispatcher, so the snapshot
                    // cannot be raced by a concurrent install/clear.
                    let installed = v201_display_messages.snapshot().await;
                    let matches = v201_command::v201_get_display_messages_matches(
                        req.id.as_deref(),
                        req.priority,
                        req.state,
                        &installed,
                    );
                    let matched = !matches.is_empty();
                    if matched
                        && command_sender
                            .send(RemoteCommand::V201NotifyDisplayMessages {
                                request_id: req.request_id,
                                messages: matches,
                            })
                            .is_err()
                    {
                        // Consumer gone: we cannot stream the report, but the
                        // status enum has no Rejected — Accepted is still the
                        // honest answer for a non-empty match. Surface the drop.
                        warn!(
                            "v201 GetDisplayMessages: consumer gone, cannot stream \
                             NotifyDisplayMessages for request {}",
                            req.request_id
                        );
                    }
                    Ok(v201_command::v201_get_display_messages_response(matched))
                }
            });
        }

        // ClearDisplayMessage (OCPP 2.0.1 only) — the teardown half of the display-
        // message family (#509), alongside SetDisplayMessage (#505) and
        // GetDisplayMessages (#508). A CSMS removes one previously-installed message
        // by its `id`. Unlike GetDisplayMessages, this is a pure remove-and-answer
        // with a single side effect and no queued command — the display-message
        // analogue of ClearVariableMonitoring's remove-by-id.
        //
        // `remove(req.id)` on the `v201_display_messages` store (under its write
        // lock) returns the displaced message if the id was installed: `Some` →
        // `Accepted` (found and removed), `None` → `Unknown` (no message with that
        // id). The write is applied on the CALL path (cheap, and must be visible to
        // the CALLRESULT); nothing is queued, so a re-clear of the same id is
        // idempotent (`Unknown` the second time).
        //
        // Like `ClearChargingProfileStatusEnumType`, `ClearMessageStatusEnumType`
        // has only `Accepted` / `Unknown` — there is no capability-failure variant
        // to report, and no consumer to have gone away (nothing is queued), so the
        // answer is decided entirely by whether the id matched.
        //
        // Trust boundary: `id` is a bare `i32`, never parsed or indexed — an
        // unknown, negative, or `i32::MIN`/`MAX` id simply fails to match in the
        // `HashMap` and answers `Unknown`, never a panic (the store's `remove`
        // tolerance is tested in #505). Registered on the V201 arm only;
        // "ClearDisplayMessage" has no 1.6J twin. Ports
        // `ocpp.v201.call.ClearDisplayMessage` →
        // `ocpp.v201.call_result.ClearDisplayMessage`.
        if matches!(protocol_version, OcppVersion::V201) {
            let v201_display_messages = v201_display_messages.clone();
            d.on(move |req: V201ClearDisplayMessageRequest| {
                let v201_display_messages = v201_display_messages.clone();
                async move {
                    let removed = v201_display_messages.remove(req.id).await.is_some();
                    Ok(v201_command::v201_clear_display_message_response(removed))
                }
            });
        }

        // RemoteStartTransaction — accept only when the targeted connector exists
        // and is free to start charging (OCPP 1.6J §5.11). A missing `connectorId`
        // defaults to connector 1, matching the Python reference's example CP.
        // On `Accepted` the actual local `StartTransaction` is queued on the
        // command channel and run by the consumer task in `connect()`, off the
        // inbound-CALL path, so the CALLRESULT is flushed before the CP re-enters
        // the WebSocket (Issue #55). Ports the `@on`/`@after('RemoteStartTransaction')`
        // split from the Python reference's example charge point.
        {
            let connectors = connectors.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: RemoteStartTransactionRequest| {
                let connectors = connectors.clone();
                let command_sender = command_sender.clone();
                async move {
                    let connector_id = req.connector_id.unwrap_or(1);
                    let status = match ConnectorId::new(connector_id) {
                        Ok(cid) => {
                            // Clone the connector out of the map so we don't hold
                            // the map guard across the inner status read.
                            let connector = connectors.read().await.get(&cid).cloned();
                            match connector {
                                Some(connector)
                                    if matches!(
                                        connector.status().await,
                                        ChargePointStatus::Available | ChargePointStatus::Reserved
                                    ) =>
                                {
                                    // Free connector: queue the local StartTransaction
                                    // off the CALL path. If the consumer has gone away
                                    // (CP shutting down) we cannot honor it, so report
                                    // Rejected rather than Accept-and-drop.
                                    match command_sender.send(RemoteCommand::StartTransaction {
                                        connector_id: cid,
                                        id_tag: req.id_tag.clone(),
                                        // 1.6J has no remoteStartId to correlate,
                                        // nor a chargingProfile / groupIdToken on
                                        // RemoteStartTransaction to thread.
                                        remote_start_id: None,
                                        charging_profile: None,
                                        group_id_token: None,
                                    }) {
                                        Ok(()) => RemoteStartStopStatus::Accepted,
                                        Err(_) => RemoteStartStopStatus::Rejected,
                                    }
                                }
                                // Known-but-busy connector, or an unknown connector id.
                                _ => RemoteStartStopStatus::Rejected,
                            }
                        }
                        // connectorId 0 / out of range → not a chargeable connector.
                        Err(_) => RemoteStartStopStatus::Rejected,
                    };
                    Ok(RemoteStartTransactionResponse { status })
                }
            });
        }

        // RequestStartTransaction (OCPP 2.0.1 Part 2) — the 2.0.1 successor to the
        // 1.6J `RemoteStartTransaction` above. Unlike `Reset` / `TriggerMessage` /
        // `ChangeAvailability` (one action name shared by both dialects, one handler
        // per `protocol_version`), this is a *distinct action*
        // (`"RequestStartTransaction"` vs `"RemoteStartTransaction"`), so it gets its
        // own V201-only registration and the 1.6J handler above is left byte-for-byte
        // unchanged. The pure Accepted/Rejected decision + response builder live in
        // `v201_command` (slice 7a, #439); this is the runtime wiring (slice 7b,
        // #442). Same off-CALL-path side-effect discipline and `Err(_) => Rejected`
        // capability outcome as the 1.6J handler.
        if protocol_version == OcppVersion::V201 {
            let connectors = connectors.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201RequestStartTransactionRequest| {
                let connectors = connectors.clone();
                let command_sender = command_sender.clone();
                async move {
                    // A `chargingProfile` attached to a RequestStartTransaction
                    // SHALL be a `TxProfile` — it bounds the single transaction
                    // this request starts (OCPP 2.0.1 Part 2). Reject any other
                    // purpose up front with an explanatory `statusInfo`, before
                    // resolving the EVSE, so a malformed request starts nothing.
                    //
                    // A valid `TxProfile` is threaded through the queued
                    // `StartTransaction` and installed into the v201-typed
                    // `v201_tx_profiles` store when the transaction actually opens
                    // (slice 7d, Issue #450). Installing it here — before the EVSE
                    // resolves and the CSMS accepts the `Started` — would risk a
                    // profile lingering behind a start that never happened, so the
                    // install is tied to `open_transaction` instead. Enforcing the
                    // schedule (bounding the metering) is the enforcement follow-up.
                    if let Some(profile) = req.charging_profile.as_ref() {
                        if profile.charging_profile_purpose
                            != ChargingProfilePurposeEnumType::TxProfile
                        {
                            let info = StatusInfoType {
                                reason_code: "InvalidProfile".to_string(),
                                additional_info: Some(
                                    "RequestStartTransaction.chargingProfile.\
                                     chargingProfilePurpose must be TxProfile"
                                        .to_string(),
                                ),
                                custom_data: None,
                            };
                            return Ok(v201_command::v201_request_start_response(
                                RequestStartStopStatusEnumType::Rejected,
                                Some(info),
                            ));
                        }
                    }

                    // Resolve the targeted EVSE. A missing `evseId` defaults to
                    // EVSE 1, mirroring the 1.6J handler's `connector_id.unwrap_or(1)`.
                    // The simulator's flat topology maps an EVSE id to the same-valued
                    // connector (the slice-2 StatusNotification convention). An EVSE
                    // id of 0 / negative / out of range, or one that maps to no real
                    // connector, resolves to `None` — i.e. "not free to charge".
                    let target = req.evse_id.unwrap_or(1);
                    let resolved: Option<(ConnectorId, bool)> = match u32::try_from(target)
                        .ok()
                        .and_then(|v| ConnectorId::new(v).ok())
                    {
                        Some(cid) => {
                            // Clone the connector out of the map so we don't hold the
                            // map guard across the inner status read.
                            let connector = connectors.read().await.get(&cid).cloned();
                            match connector {
                                Some(connector) => {
                                    let available = matches!(
                                        connector.status().await,
                                        ChargePointStatus::Available | ChargePointStatus::Reserved
                                    );
                                    Some((cid, available))
                                }
                                // Unknown connector on this CP.
                                None => None,
                            }
                        }
                        // EVSE 0 / negative / out of range → not a chargeable EVSE.
                        None => None,
                    };

                    let evse_available = resolved.is_some_and(|(_, available)| available);
                    let mut status =
                        v201_command::v201_request_start_status(req.evse_id, evse_available);

                    // Only an Accepted request queues the local StartTransaction, off
                    // the CALL path (the CALLRESULT is flushed before the CP re-enters
                    // the WebSocket). `Accepted` implies the target resolved to a free
                    // connector, so `resolved` is `Some`; a failed enqueue (consumer
                    // gone, CP shutting down) downgrades to `Rejected` rather than
                    // accept-and-drop.
                    if status == RequestStartStopStatusEnumType::Accepted {
                        if let Some((cid, _)) = resolved {
                            if command_sender
                                .send(RemoteCommand::StartTransaction {
                                    connector_id: cid,
                                    id_tag: req.id_token.id_token.clone(),
                                    // Echoed onto the started transaction's
                                    // TransactionEvent(Started) for CSMS
                                    // correlation (2.0.1 remoteStartId).
                                    remote_start_id: Some(req.remote_start_id),
                                    // A validated TxProfile (purpose guarded
                                    // above) to install against the started
                                    // transaction, and the optional groupIdToken
                                    // to thread onto its auth context (slice 7d).
                                    charging_profile: req.charging_profile.clone().map(Box::new),
                                    group_id_token: req.group_id_token.clone(),
                                })
                                .is_err()
                            {
                                status = RequestStartStopStatusEnumType::Rejected;
                            }
                        } else {
                            status = RequestStartStopStatusEnumType::Rejected;
                        }
                    }

                    Ok(v201_command::v201_request_start_response(status, None))
                }
            });
        }

        // RemoteStopTransaction — accept only when the transaction id matches an
        // active transaction this CP is running (OCPP 1.6J §5.12); otherwise
        // reject. On `Accepted` the matching transaction is ended (StopTransaction,
        // reason `Remote`) via the command channel and consumer task, off the
        // inbound-CALL path so the CALLRESULT is flushed first (Issue #55).
        {
            let active_transactions = active_transactions.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: RemoteStopTransactionRequest| {
                let active_transactions = active_transactions.clone();
                let command_sender = command_sender.clone();
                async move {
                    let known = active_transactions
                        .read()
                        .await
                        .contains_key(&req.transaction_id);
                    let status = if known {
                        // Known transaction: queue the StopTransaction off the CALL
                        // path; if the consumer is gone, report Rejected rather than
                        // Accept-and-drop.
                        match command_sender.send(RemoteCommand::StopTransaction {
                            transaction_id: req.transaction_id,
                        }) {
                            Ok(()) => RemoteStartStopStatus::Accepted,
                            Err(_) => RemoteStartStopStatus::Rejected,
                        }
                    } else {
                        RemoteStartStopStatus::Rejected
                    };
                    Ok(RemoteStopTransactionResponse { status })
                }
            });
        }

        // RequestStopTransaction (OCPP 2.0.1 Part 2) — the 2.0.1 successor to the
        // 1.6J `RemoteStopTransaction` above. Like RequestStartTransaction (and
        // unlike Reset / TriggerMessage / ChangeAvailability), the action name
        // *differs* between dialects (`"RequestStopTransaction"` vs
        // `"RemoteStopTransaction"`), so this is a distinct V201-only registration
        // and the 1.6J handler above stays byte-for-byte unchanged. The pure
        // Accepted/Rejected decision + response builder live in `v201_command`
        // (slice 9); this is the runtime wiring.
        //
        // A 2.0.1 `transactionId` is an opaque string, but the station mints it as
        // the decimal of its internal `i32` transaction key, and
        // `active_transactions` (id → connector) is populated for the V201 path too
        // (see `start_transaction_with_remote_start_id`). So resolving the requested
        // string back to that live `i32` lets this reuse the version-aware
        // `RemoteCommand::StopTransaction { i32 }` path (consumer →
        // `stop_transaction(id, 0, Reason::Remote)` → `TransactionEvent(Ended)` with
        // `stoppedReason` = `Remote`), rather than adding a parallel String-keyed
        // command that would only be parsed back to the same `i32`. Same
        // off-CALL-path side-effect discipline and `Err(_) => Rejected` outcome as
        // the 1.6J handler (Issue #55): the CALLRESULT is flushed before the stop.
        if protocol_version == OcppVersion::V201 {
            let active_transactions = active_transactions.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201RequestStopTransactionRequest| {
                let active_transactions = active_transactions.clone();
                let command_sender = command_sender.clone();
                async move {
                    // Resolve the requested opaque id to a live transaction key by
                    // exact string match against the decimal spelling of each live
                    // id — no numeric parse, so a malformed / non-canonical id just
                    // fails to match and is Rejected (never panics).
                    let live_ids: Vec<(i32, String)> = active_transactions
                        .read()
                        .await
                        .keys()
                        .map(|id| (*id, id.to_string()))
                        .collect();
                    let matched = live_ids
                        .iter()
                        .find(|(_, s)| *s == req.transaction_id)
                        .map(|(id, _)| *id);

                    let live_id_strs: Vec<&str> =
                        live_ids.iter().map(|(_, s)| s.as_str()).collect();
                    let mut status =
                        v201_command::v201_request_stop_status(&req.transaction_id, &live_id_strs);

                    // Only an Accepted request queues the stop, off the CALL path.
                    // `Accepted` implies a match, so `matched` is `Some`; a failed
                    // enqueue (consumer gone, CP shutting down) downgrades to
                    // `Rejected` rather than accept-and-drop.
                    if status == RequestStartStopStatusEnumType::Accepted {
                        if let Some(transaction_id) = matched {
                            if command_sender
                                .send(RemoteCommand::StopTransaction { transaction_id })
                                .is_err()
                            {
                                status = RequestStartStopStatusEnumType::Rejected;
                            }
                        } else {
                            status = RequestStartStopStatusEnumType::Rejected;
                        }
                    }

                    Ok(v201_command::v201_request_stop_response(status, None))
                }
            });
        }

        // GetTransactionStatus (OCPP 2.0.1 Part 2, E13) — a 2.0.1-only query: the
        // CSMS asks whether a specific transaction is still ongoing and/or whether
        // the station still has messages queued for it (e.g. after a reconnect,
        // before deciding to clean up). There is no 1.6J equivalent, so this is
        // registered on the `V201` arm only (Issue #490). Unlike the command
        // handlers this is a pure read-and-answer with no side effect: it reads the
        // live `active_transactions` set and reports directly — nothing is queued.
        if protocol_version == OcppVersion::V201 {
            let active_transactions = active_transactions.clone();
            d.on(move |req: V201GetTransactionStatusRequest| {
                let active_transactions = active_transactions.clone();
                async move {
                    // Resolve the live ids to their decimal spellings, exactly as
                    // the RequestStopTransaction handler does, so an inbound opaque
                    // `transactionId` is matched by exact string equality (never a
                    // numeric parse — a malformed / non-canonical id just misses and
                    // reports `ongoingIndicator = Some(false)`, never panicking).
                    let live_ids: Vec<String> = active_transactions
                        .read()
                        .await
                        .keys()
                        .map(ToString::to_string)
                        .collect();
                    let live_id_strs: Vec<&str> = live_ids.iter().map(String::as_str).collect();

                    // `messagesInQueue` is a modeled `false`: the simulator does not
                    // yet buffer offline messages, so it never has undelivered
                    // messages queued for a transaction. Wiring it as an input (see
                    // `v201_get_transaction_status`) lets a future outbound queue
                    // flip this without reshaping the decision.
                    let (messages_in_queue, ongoing_indicator) =
                        v201_command::v201_get_transaction_status(
                            req.transaction_id.as_deref(),
                            &live_id_strs,
                            false,
                        );

                    Ok(v201_command::v201_get_transaction_status_response(
                        messages_in_queue,
                        ongoing_indicator,
                    ))
                }
            });
        }

        // CostUpdated (OCPP 2.0.1 Part 2, K — Tariff & Cost) — a 2.0.1-only
        // message: the CSMS pushes the running total cost of an ongoing
        // transaction so the station can show the driver an up-to-date price.
        // There is no 1.6J equivalent, so this is registered on the `V201` arm
        // only (Issue #502). It records the latest cost against the wire
        // `transactionId` and answers with an empty `CostUpdatedResponse` —
        // OCPP defines no rejection status for CostUpdated, so the cost is taken
        // up unconditionally (an id the station is not, or not yet, running is
        // recorded pending rather than dropped: OCPP places no ordering
        // guarantee between CostUpdated and the station's own liveness view). The
        // only side effect is the in-memory upsert; nothing is queued, and the
        // opaque `transactionId` is used as an exact string key, never parsed, so
        // a malformed/edge id cannot panic. `totalCost` is stored verbatim (the
        // decoded f64, no rounding — see the #411 float-fidelity direction).
        if protocol_version == OcppVersion::V201 {
            let v201_costs = v201_costs.clone();
            d.on(move |req: V201CostUpdatedRequest| {
                let v201_costs = v201_costs.clone();
                async move {
                    v201_costs.update(&req.transaction_id, req.total_cost).await;
                    Ok(v201_command::v201_cost_updated_response())
                }
            });
        }

        // Reset — acknowledge, then carry out the reset as a real side effect.
        // Both OCPP versions name the action `"Reset"`, so exactly one handler is
        // registered per `protocol_version`; the negotiated subprotocol and the
        // version-aware inbound validator keep the wire on a single dialect, so
        // the other version's request shape never reaches this dispatcher.
        match protocol_version {
            // OCPP 1.6J §5.13. The work is queued on the command channel and run
            // by the consumer task spawned in `connect()`, so the CALLRESULT is
            // flushed before any outbound CALL (graceful StopTransaction, re-boot)
            // and the receive loop never re-enters itself. Returning `Accepted`
            // commits only to *attempting* the reset; if the consumer has gone
            // away (the CP is shutting down) the command cannot be honored, so we
            // report `Rejected` rather than silently dropping it.
            OcppVersion::V16J => {
                let command_sender = command_sender.clone();
                d.on(move |req: ResetRequest| {
                    let command_sender = command_sender.clone();
                    async move {
                        let status = match command_sender.send(RemoteCommand::Reset {
                            reset_type: req.reset_type,
                        }) {
                            Ok(()) => ResetStatus::Accepted,
                            Err(_) => ResetStatus::Rejected,
                        };
                        Ok(ResetResponse { status })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, `Reset`). Ports `ocpp.v201.call.Reset`: the 1.6J
            // Hard/Soft distinction becomes a `ResetEnumType` (`Immediate`/
            // `OnIdle`) and the response gains a `Scheduled` status (accept but
            // defer) plus optional `statusInfo`. The pure decision + response
            // construction live in `v201_command` (slice 4a, #427); this is the
            // runtime wiring (slice 4b, #428). Same off-CALL-path side-effect
            // discipline as the 1.6J handler.
            OcppVersion::V201 => {
                let command_sender = command_sender.clone();
                let active_transactions = active_transactions.clone();
                let pending_v201_reset = pending_v201_reset.clone();
                d.on(move |req: V201ResetRequest| {
                    let command_sender = command_sender.clone();
                    let active_transactions = active_transactions.clone();
                    let pending_v201_reset = pending_v201_reset.clone();
                    async move {
                        // Whether a transaction is running is the only extra input
                        // to the `Accepted` vs `Scheduled` decision for an `OnIdle`
                        // reset. The simulator's flat single-connector-EVSE
                        // topology (established by the slice-2 StatusNotification
                        // path) makes a whole-station reset behaviorally equivalent
                        // to a per-`evseId` one, so the request's optional `evseId`
                        // scope is accepted but does not narrow this check;
                        // per-EVSE-scoped resets are a documented follow-up.
                        let transaction_in_progress = !active_transactions.read().await.is_empty();
                        let mut status =
                            v201_command::v201_reset_status(req.kind, transaction_in_progress);

                        // Only an accepted, immediately-actionable reset drives the
                        // side-effect now: `Accepted` covers `Immediate` (always)
                        // and `OnIdle` while idle. If the consumer has gone away we
                        // cannot honor it, so downgrade to `Rejected` — the same
                        // capability outcome the 1.6J handler reports on a failed
                        // channel send. A `Scheduled` reset (an `OnIdle` request
                        // received while charging) is accepted-but-deferred: no
                        // side-effect is queued now, so the live session is never
                        // interrupted. Instead (slice 4c, #431) the mapped
                        // `ResetType` is armed in `pending_v201_reset`, and
                        // `stop_transaction` carries it out once the station next
                        // goes idle. A later `Scheduled` overwrites the slot; a
                        // later `Immediate` reboots now and supersedes it.
                        if status == ResetStatusEnumType::Accepted {
                            let reset_type = v201_command::v201_reset_reset_type(req.kind);
                            if command_sender
                                .send(RemoteCommand::Reset { reset_type })
                                .is_err()
                            {
                                status = ResetStatusEnumType::Rejected;
                            }
                        } else if status == ResetStatusEnumType::Scheduled {
                            let reset_type = v201_command::v201_reset_reset_type(req.kind);
                            *pending_v201_reset.write().await = Some(reset_type);
                        }

                        Ok(v201_command::v201_reset_response(status, None))
                    }
                });
            }
        }

        // TriggerMessage — acknowledge, then send the requested message as a real
        // side effect. Like Reset, the send is queued on the command channel and
        // run by the consumer task spawned in `connect()`, so the CALLRESULT is
        // flushed before the triggered outbound CALL and the receive loop never
        // re-enters itself. Both OCPP versions name the action `"TriggerMessage"`,
        // so exactly one handler is registered per `protocol_version` (same
        // discipline as the Reset split above); the version-aware inbound
        // validator keeps the wire on a single dialect so the other version's
        // request shape never reaches this dispatcher.
        match protocol_version {
            // OCPP 1.6J §4.x. A `requestedMessage` the CP cannot produce yields
            // `NotImplemented` (no work queued); a supported message the CP cannot
            // honor right now (consumer gone, CP shutting down) yields `Rejected`
            // rather than accept-and-drop.
            OcppVersion::V16J => {
                let command_sender = command_sender.clone();
                d.on(move |req: TriggerMessageRequest| {
                    let command_sender = command_sender.clone();
                    async move {
                        let status = if trigger_message_supported(&req.requested_message) {
                            match command_sender.send(RemoteCommand::TriggerMessage {
                                requested_message: req.requested_message,
                                connector_id: req.connector_id,
                            }) {
                                Ok(()) => TriggerMessageStatus::Accepted,
                                Err(_) => TriggerMessageStatus::Rejected,
                            }
                        } else {
                            TriggerMessageStatus::NotImplemented
                        };
                        Ok(TriggerMessageResponse { status })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, `TriggerMessage`). Ports
            // `ocpp.v201.call.TriggerMessage`: the `requestedMessage` is a
            // `MessageTriggerEnumType` and the response gains an optional
            // `statusInfo`. The pure policy decision (`Accepted` for a message the
            // simulator emits, `NotImplemented` otherwise) lives in `v201_command`
            // (slice 5a, #434); this is the runtime wiring (slice 5b, #433). The
            // request's optional `evse` scope is carried through to the side effect
            // (station-wide when omitted). Same off-CALL-path discipline and
            // `Err(_) => Rejected` capability outcome as the 1.6J handler.
            OcppVersion::V201 => {
                let command_sender = command_sender.clone();
                d.on(move |req: V201TriggerMessageRequest| {
                    let command_sender = command_sender.clone();
                    async move {
                        let requested = req.requested_message;
                        let mut status = v201_command::v201_trigger_message_status(requested);
                        // Only an accepted trigger queues a side effect; a
                        // `NotImplemented` message is recognized but has no emit
                        // path, so nothing is enqueued. A failed enqueue (the
                        // consumer has gone away, CP shutting down) downgrades to
                        // `Rejected` — the runtime capability outcome slice 5a
                        // leaves to the wiring layer — rather than accept-and-drop.
                        if status == TriggerMessageStatusEnumType::Accepted {
                            let evse_id = req.evse.map(|evse| evse.id);
                            if command_sender
                                .send(RemoteCommand::V201TriggerMessage {
                                    requested_message: requested,
                                    evse_id,
                                })
                                .is_err()
                            {
                                status = TriggerMessageStatusEnumType::Rejected;
                            }
                        }
                        Ok(v201_command::v201_trigger_message_response(status, None))
                    }
                });
            }
        }

        // GetDiagnostics — acknowledge with the file name the CP would upload,
        // then run the diagnostics-upload state machine as a real side effect
        // (OCPP 1.6J §4.x, firmware-management profile). Like Reset, the upload
        // is queued on the command channel and run by the consumer task spawned
        // in `connect()`, so the GetDiagnostics CALLRESULT is flushed before the
        // first `DiagnosticsStatusNotification` and the receive loop never
        // re-enters itself. The simulator has no real archive, so it reports a
        // generated `fileName` and drives Uploading → Uploaded on a timer. If
        // the consumer has gone away (CP shutting down), the upload cannot run,
        // so we omit the file name rather than promise an upload that won't
        // happen.
        {
            let command_sender = command_sender.clone();
            d.on(move |_req: GetDiagnosticsRequest| {
                let command_sender = command_sender.clone();
                async move {
                    let file_name = match command_sender.send(RemoteCommand::GetDiagnostics) {
                        Ok(()) => Some(format!(
                            "diagnostics-{}.tar.gz",
                            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
                        )),
                        Err(_) => None,
                    };
                    Ok(GetDiagnosticsResponse { file_name })
                }
            });
        }

        // UpdateFirmware (1.6J arm) — acknowledge with the empty conf the spec
        // defines (no status field), then run the firmware-update state machine as
        // a real side effect (OCPP 1.6J §4.x, firmware-management profile). Like
        // GetDiagnostics, the update is queued on the command channel and run by
        // the consumer task spawned in `connect()`, so the UpdateFirmware
        // CALLRESULT is flushed before the first `FirmwareStatusNotification` and
        // the receive loop never re-enters itself. The simulator has no real
        // image, so it drives Downloading → Downloaded → Installing → Installed
        // on a timer. If the consumer has gone away (CP shutting down) the
        // update can't run, but the spec response is empty either way, so we
        // only log.
        //
        // The 1.6J and 2.0.1 `UpdateFirmware` share the action name `"UpdateFirmware"`
        // but differ in response shape — 1.6J is an empty `.conf`, 2.0.1 carries an
        // `UpdateFirmwareStatusEnumType` — so exactly one handler is registered per
        // `protocol_version` (the same version-split discipline as ChangeAvailability
        // / Reset above). Since the dispatcher keys routes by action name, an
        // ungated registration here would overwrite the V201 handler on a 2.0.1 CP;
        // gating it to the non-V201 arm keeps the empty-conf handler for 1.6J only
        // and leaves the V201 arm to the status-carrying handler below (Issue #532).
        if !matches!(protocol_version, OcppVersion::V201) {
            let command_sender = command_sender.clone();
            d.on(move |_req: UpdateFirmwareRequest| {
                let command_sender = command_sender.clone();
                async move {
                    if command_sender.send(RemoteCommand::UpdateFirmware).is_err() {
                        warn!("UpdateFirmware: command consumer gone, update will not run");
                    }
                    Ok(UpdateFirmwareResponse {})
                }
            });
        }

        // UnlockConnector — faithfully unlock a connector's cable (OCPP 1.6J
        // §5.21). The status is keyed off the connector and this CP's lock
        // capability ([`UnlockConnectorOutcome`]):
        //
        // - unknown / out-of-range `connectorId` (incl. 0) → `UnlockFailed`: the
        //   CP cannot unlock a connector it does not have, and the spec response
        //   has no "Rejected" — `UnlockFailed` is the faithful answer;
        // - `NotSupported` lock capability → `NotSupported`;
        // - `UnlockFailed` lock capability (mechanical fault) → `UnlockFailed`;
        // - otherwise → `Unlocked`. If a transaction is live on the connector it
        //   is stopped first (`StopTransaction`, reason `UnlockCommand`) and the
        //   connector freed — queued on the command channel and run by the
        //   consumer task off the inbound-CALL path so the `UnlockConnector`
        //   CALLRESULT is flushed before the outbound `StopTransaction` CALL (no
        //   receive-loop re-entrancy, same pattern as Reset/RemoteStop). An idle
        //   connector just releases the cable — a purely local action with no
        //   OCPP side effect. If the consumer has gone away (CP shutting down) the
        //   transaction cannot be stopped, so we report `UnlockFailed` rather than
        //   falsely claim `Unlocked`. Ports `@on('UnlockConnector')` from the
        //   Python reference's example charge point.
        {
            let connectors = connectors.clone();
            let active_transactions = active_transactions.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: UnlockConnectorRequest| {
                let connectors = connectors.clone();
                let active_transactions = active_transactions.clone();
                let command_sender = command_sender.clone();
                async move {
                    let status = match ConnectorId::new(req.connector_id) {
                        // connectorId 0 / out of range, or a connector this CP
                        // does not have → not unlockable.
                        Err(_) => UnlockStatus::UnlockFailed,
                        Ok(cid) if !connectors.read().await.contains_key(&cid) => {
                            UnlockStatus::UnlockFailed
                        }
                        Ok(cid) => match unlock_outcome {
                            UnlockConnectorOutcome::NotSupported => UnlockStatus::NotSupported,
                            UnlockConnectorOutcome::UnlockFailed => UnlockStatus::UnlockFailed,
                            UnlockConnectorOutcome::Unlock => {
                                // Find the active transaction (if any) on this
                                // connector; the map is keyed by transaction id, so
                                // scan for the matching connector.
                                let txn = active_transactions
                                    .read()
                                    .await
                                    .iter()
                                    .find_map(|(tid, c)| (*c == cid).then_some(*tid));
                                match txn {
                                    Some(transaction_id) => {
                                        match command_sender.send(RemoteCommand::UnlockConnector {
                                            connector_id: cid,
                                            transaction_id,
                                        }) {
                                            Ok(()) => UnlockStatus::Unlocked,
                                            Err(_) => UnlockStatus::UnlockFailed,
                                        }
                                    }
                                    None => UnlockStatus::Unlocked,
                                }
                            }
                        },
                    };
                    Ok(UnlockConnectorResponse { status })
                }
            });
        }

        // UnlockConnector (OCPP 2.0.1 Part 2) — the 2.0.1 successor to the 1.6J
        // `UnlockConnector` handler above. Both dialects name the action
        // `"UnlockConnector"`, so — like the Reset / TriggerMessage /
        // ChangeAvailability splits, and unlike RequestStartTransaction (a
        // distinct action name) — this is registered as exactly one handler per
        // `protocol_version`: the 1.6J handler stays byte-for-byte on the default
        // arm, and this V201 handler is added only on the `V201` arm. The pure
        // Unlocked / UnlockFailed / OngoingAuthorizedTransaction / UnknownConnector
        // decision + response builder live in `v201_command` (slice 8a, #440);
        // this is the runtime wiring (slice 8b, #446).
        //
        // Where 1.6J addressed a flat `connectorId`, 2.0.1 names both `evseId` and
        // `connectorId`. On the simulator's flat single-connector-EVSE topology an
        // `evseId` maps to the same-valued connector (the slice-2 StatusNotification
        // convention, `evseId = connector_id, connectorId = 1`), and each EVSE has
        // exactly one connector — so a `connectorId` other than 1 addresses a
        // connector-within-EVSE that does not exist and is `UnknownConnector`.
        //
        // 2.0.1 gained the `OngoingAuthorizedTransaction` status precisely so the
        // station *refuses* to release the cable while a session is *still
        // authorized*, rather than force-stopping it as the 1.6J handler always
        // does (1.6J has no such status). So a live, still-authorized transaction
        // on the target maps to `OngoingAuthorizedTransaction` (no unlock).
        //
        // A live but *deauthorized* transaction (`deauthorize`, e.g. the driver
        // re-presented their card / the app revoked authorization) is instead
        // *stoppable*: like the 1.6J handler, this arm stops it first
        // (`StopTransaction`, reason `UnlockCommand`) via the shared
        // `RemoteCommand::UnlockConnector` path — off the inbound-CALL path, the
        // CALLRESULT flushed before the consumer runs (Issue #55) — then reports
        // the mechanical outcome. An idle connector has nothing to stop and takes
        // the mechanical outcome directly. Ports `@on('UnlockConnector')` from the
        // Python reference's 2.0.1 charge point.
        if protocol_version == OcppVersion::V201 {
            let connectors = connectors.clone();
            let active_transactions = active_transactions.clone();
            let v201_sessions = v201_sessions.clone();
            let command_sender = command_sender.clone();
            d.on(move |req: V201UnlockConnectorRequest| {
                let connectors = connectors.clone();
                let active_transactions = active_transactions.clone();
                let v201_sessions = v201_sessions.clone();
                let command_sender = command_sender.clone();
                async move {
                    // Resolve the target to a live connector. Only `connectorId == 1`
                    // (the single connector within each EVSE) can map to a real
                    // connector; the `evseId` then selects the same-valued
                    // `ConnectorId`. A non-1 connectorId, or an evseId that is 0 /
                    // negative / out of range / unmapped, leaves the target unknown.
                    // Structural guards for ids below 1 also live inside the pure
                    // decision, so feeding `connector_known == false` for those is
                    // consistent either way.
                    let target_cid = (req.connector_id == 1)
                        .then(|| {
                            u32::try_from(req.evse_id)
                                .ok()
                                .and_then(|v| ConnectorId::new(v).ok())
                        })
                        .flatten();

                    // `(known, live transaction id on it, still-authorized?)`. Only
                    // scan for a live transaction on a connector this CP actually
                    // has; `active_transactions` is keyed by transaction id, so
                    // match on the connector value.
                    let (connector_known, live_txn, transaction_authorized) = match target_cid {
                        Some(cid) => {
                            let known = connectors.read().await.contains_key(&cid);
                            let live_txn = if known {
                                active_transactions
                                    .read()
                                    .await
                                    .iter()
                                    .find_map(|(tid, c)| (*c == cid).then_some(*tid))
                            } else {
                                None
                            };
                            // A live transaction is "still authorized" unless it was
                            // explicitly deauthorized. A *missing* session (a
                            // transaction being torn down concurrently) is treated as
                            // still authorized: refuse rather than race a redundant
                            // force-stop against its own stop.
                            let authorized = match live_txn {
                                Some(tid) => v201_sessions
                                    .read()
                                    .await
                                    .get(&tid)
                                    .is_none_or(|s| s.authorized.load(Ordering::SeqCst)),
                                None => false,
                            };
                            (known, live_txn, authorized)
                        }
                        None => (false, None, false),
                    };

                    let transaction_active = live_txn.is_some();
                    let mut status = v201_command::v201_unlock_status(
                        req.evse_id,
                        req.connector_id,
                        connector_known,
                        transaction_active,
                        transaction_authorized,
                        unlock_outcome,
                    );

                    // Stoppable transaction the station is about to release: stop it
                    // first (reason `UnlockCommand`) off the CALL path, mirroring the
                    // 1.6J stop-then-unlock. A failed enqueue (consumer gone) downgrades
                    // to `UnlockFailed` rather than claim a release we cannot complete.
                    // Guarded by `status == Unlocked`, so a mechanical `UnlockFailed`
                    // leaves the transaction untouched, and an idle connector (no
                    // `live_txn`) queues nothing.
                    if status == UnlockStatusEnumType::Unlocked {
                        if let (Some(cid), Some(transaction_id)) = (target_cid, live_txn) {
                            if command_sender
                                .send(RemoteCommand::UnlockConnector {
                                    connector_id: cid,
                                    transaction_id,
                                })
                                .is_err()
                            {
                                status = UnlockStatusEnumType::UnlockFailed;
                            }
                        }
                    }

                    Ok(v201_command::v201_unlock_response(status, None))
                }
            });
        }

        // ReserveNow — the CSMS holds an EVSE/connector for a driver until an
        // expiry time. Both dialects name the action `"ReserveNow"` but carry
        // different request shapes (1.6J flat `connectorId` + `idTag` vs 2.0.1
        // optional `evseId` + structured `idToken`/`connectorType`), so — like
        // SetChargingProfile / ClearChargingProfile above — exactly one handler
        // is registered per `protocol_version`; the negotiated subprotocol and
        // the version-aware inbound validator keep the wire on a single dialect,
        // so the other version's request shape never reaches this dispatcher.
        match protocol_version {
            // OCPP 1.6J §5.14 — faithful status semantics keyed off the
            // connector's live status: a free connector is reserved (→ `Reserved`)
            // and the reservationId recorded; a busy connector → `Occupied`, a
            // faulted one → `Faulted`, an unavailable one → `Unavailable`; an
            // unknown/out-of-range connector id (incl. 0) → `Rejected`. The
            // reserve itself is a local state change, but on `Accepted` we also
            // queue a `StatusNotification` (`Reserved`) to the CSMS off the
            // inbound-CALL path so a back office's live connector view flips
            // immediately (Issue #80) without waiting for the next status event.
            // Unchanged from the pre-version-gate behavior. Ports `ReserveNow`
            // from the Python reference's `call.py`/`enums.py`.
            OcppVersion::V16J => {
                let connectors = connectors.clone();
                let reservations = reservations.clone();
                let command_sender = command_sender.clone();
                let expiry_timers = expiry_timers.clone();
                d.on(move |req: ReserveNowRequest| {
                    let connectors = connectors.clone();
                    let reservations = reservations.clone();
                    let command_sender = command_sender.clone();
                    let expiry_timers = expiry_timers.clone();
                    async move {
                        // A reservation whose `expiryDate` has already passed is
                        // nonsensical — it would auto-free instantly — so reject it
                        // outright rather than accept and immediately expire (Issue
                        // #85), consistent with the liberal use of `Rejected` for
                        // other non-reservable requests below.
                        if req.expiry_date <= chrono::Utc::now() {
                            return Ok(ReserveNowResponse {
                                status: ReservationStatus::Rejected,
                            });
                        }
                        let status = match ConnectorId::new(req.connector_id as u32) {
                            Ok(cid) => match connectors.read().await.get(&cid).cloned() {
                                Some(mut connector) => match connector.status().await {
                                    ChargePointStatus::Available => {
                                        match connector.reserve(req.id_tag.clone()).await {
                                            Ok(()) => {
                                                reservations
                                                    .write()
                                                    .await
                                                    .insert(req.reservation_id, cid);
                                                // Best-effort: the reservation is
                                                // already Accepted; a dropped
                                                // notification (consumer gone, CP
                                                // shutting down) must not undo it.
                                                let _ = command_sender.send(
                                                    RemoteCommand::EmitConnectorStatus {
                                                        connector_id: cid,
                                                        status: ChargePointStatus::Reserved,
                                                    },
                                                );
                                                // Arm the auto-expiry timer (Issue
                                                // #85): when `expiryDate` passes,
                                                // free the connector on our own.
                                                Self::arm_reservation_expiry(
                                                    req.reservation_id,
                                                    cid,
                                                    req.expiry_date,
                                                    &connectors,
                                                    &reservations,
                                                    &expiry_timers,
                                                    &command_sender,
                                                    OcppVersion::V16J,
                                                )
                                                .await;
                                                ReservationStatus::Accepted
                                            }
                                            Err(_) => ReservationStatus::Rejected,
                                        }
                                    }
                                    ChargePointStatus::Faulted => ReservationStatus::Faulted,
                                    ChargePointStatus::Unavailable => {
                                        ReservationStatus::Unavailable
                                    }
                                    // Reserved / Occupied / Preparing / Charging /
                                    // Suspended* / Finishing — connector is in use.
                                    _ => ReservationStatus::Occupied,
                                },
                                // Known protocol but no such connector on this CP.
                                None => ReservationStatus::Rejected,
                            },
                            // connectorId 0 / out of range → not a reservable connector.
                            Err(_) => ReservationStatus::Rejected,
                        };
                        Ok(ReserveNowResponse { status })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, `ReserveNow`) — the 2.0.1 successor. The request
            // targets an optional `evseId` (whole-station when omitted) and carries
            // a structured `idToken`/`connectorType` rather than a flat
            // `connectorId`/`idTag`. The pure Accepted/Occupied/Faulted/Unavailable/
            // Rejected decision + response builder live in `v201_command`
            // (mirroring `SetChargingProfile`, #469); this is the runtime wiring:
            // resolve `evseId` against the live connector topology into a distilled
            // `ConnectorReservState`, decide, and on `Accepted` reserve the EVSE,
            // record the reservation, queue the `Reserved` StatusNotification off
            // the CALL path, and arm the auto-expiry timer — reusing the exact
            // reservation/expiry machinery the 1.6J arm uses. Ports
            // `ocpp.v201.call.ReserveNow` and the `@on(Action.reserve_now)` dispatch
            // shape from `ocpp/charge_point.py`.
            OcppVersion::V201 => {
                let connectors = connectors.clone();
                let reservations = reservations.clone();
                let command_sender = command_sender.clone();
                let expiry_timers = expiry_timers.clone();
                d.on(move |req: V201ReserveNowRequest| {
                    let connectors = connectors.clone();
                    let reservations = reservations.clone();
                    let command_sender = command_sender.clone();
                    let expiry_timers = expiry_timers.clone();
                    async move {
                        // Parse the ISO-8601 expiry. A parseable timestamp already
                        // in the past is rejected by the decision (it would auto-free
                        // instantly, #85); a future one arms the auto-expiry timer on
                        // Accept. An unparseable string is treated leniently as "no
                        // schedulable expiry" — the reservation is held until an
                        // explicit CancelReservation — never a panic on the
                        // attacker-supplied text crossing the WebSocket boundary.
                        let expiry = chrono::DateTime::parse_from_rfc3339(&req.expiry_date_time)
                            .ok()
                            .map(|t| t.with_timezone(&chrono::Utc));
                        let already_expired = matches!(expiry, Some(t) if t <= chrono::Utc::now());

                        // Resolve evseId → the targeted connector's distilled
                        // reservability. A None / 0 / negative / out-of-range evseId
                        // resolves to `Missing` (never indexes a connector), which
                        // the pure decision maps to Rejected — never a panic.
                        let cid = req
                            .evse_id
                            .filter(|id| *id >= 1)
                            .and_then(|id| ConnectorId::new(id as u32).ok());
                        let state = match &cid {
                            Some(cid) => match connectors.read().await.get(cid).cloned() {
                                Some(connector) => match connector.status().await {
                                    ChargePointStatus::Available => ConnectorReservState::Free,
                                    ChargePointStatus::Faulted => ConnectorReservState::Faulted,
                                    ChargePointStatus::Unavailable => {
                                        ConnectorReservState::Unavailable
                                    }
                                    // Reserved / Occupied / Preparing / Charging /
                                    // Suspended* / Finishing — the connector is in use.
                                    _ => ConnectorReservState::Busy,
                                },
                                None => ConnectorReservState::Missing,
                            },
                            None => ConnectorReservState::Missing,
                        };

                        let (mut status, mut status_info) = v201_command::v201_reserve_now_status(
                            already_expired,
                            req.evse_id,
                            state,
                        );

                        // An Accepted decision performs the reservation side effect.
                        // `reserve()` only succeeds from `Available`; a lost race
                        // (the EVSE was taken between the status read and here)
                        // downgrades to Rejected rather than reporting a reservation
                        // that did not happen — mirroring the 1.6J `Err(_) =>
                        // Rejected`.
                        if status == ReserveNowStatusEnumType::Accepted {
                            // An Accepted decision guarantees `cid` resolved to a
                            // free connector; the `if let` is a total guard, never
                            // an `unwrap`.
                            if let Some(cid) = cid {
                                let reserved = match connectors.read().await.get(&cid).cloned() {
                                    Some(mut connector) => connector
                                        .reserve(req.id_token.id_token.clone())
                                        .await
                                        .is_ok(),
                                    None => false,
                                };
                                if reserved {
                                    reservations.write().await.insert(req.id, cid);
                                    // Best-effort: the reservation is already
                                    // Accepted; a dropped notification (consumer
                                    // gone) must not undo it.
                                    let _ =
                                        command_sender.send(RemoteCommand::EmitConnectorStatus {
                                            connector_id: cid,
                                            status: ChargePointStatus::Reserved,
                                        });
                                    // Arm auto-expiry only for a parseable *future*
                                    // expiry (Issue #85), reusing the 1.6J machinery.
                                    if let Some(expiry) = expiry {
                                        Self::arm_reservation_expiry(
                                            req.id,
                                            cid,
                                            expiry,
                                            &connectors,
                                            &reservations,
                                            &expiry_timers,
                                            &command_sender,
                                            OcppVersion::V201,
                                        )
                                        .await;
                                    }
                                } else {
                                    status = ReserveNowStatusEnumType::Rejected;
                                    status_info = Some(StatusInfoType {
                                        reason_code: "ReserveFailed".to_string(),
                                        additional_info: Some(
                                            "the targeted EVSE could not be reserved \
                                             (it was taken concurrently)"
                                                .to_string(),
                                        ),
                                        custom_data: None,
                                    });
                                }
                            }
                        }

                        Ok(v201_command::v201_reserve_now_response(status, status_info))
                    }
                });
            }
        }

        // CancelReservation — the teardown counterpart to ReserveNow: the CSMS
        // drops a previously-made reservation by its integer `reservationId`.
        // Both dialects key on the same id and share the same reservation store,
        // but carry different response shapes (1.6J `CancelReservationStatus` vs
        // 2.0.1 `CancelReservationStatusEnumType` + optional `statusInfo`), so —
        // like SetChargingProfile / ClearChargingProfile below — exactly one
        // handler is registered per `protocol_version`; the negotiated
        // subprotocol and the version-aware inbound validator keep the wire on a
        // single dialect, so the other version's request shape never reaches this
        // dispatcher.
        match protocol_version {
            // OCPP 1.6J §5.4 — `Accepted` if the id is held (the connector is
            // freed, `Reserved` → `Available`), `Rejected` if it is unknown.
            // Freeing the connector is a local state change; on `Accepted` we also
            // queue a `StatusNotification` (`Available`) off the inbound-CALL path
            // so the CSMS sees the connector free up immediately (Issue #80). A
            // `cancel_reservation()` that did not actually flip `Reserved` →
            // `Available` (the connector moved on, e.g. a faulted/occupied edge)
            // emits nothing — we only announce the transition we made. Unchanged
            // from the pre-version-gate behavior. Ports `CancelReservation` from
            // the Python reference's `call.py`/`enums.py`.
            OcppVersion::V16J => {
                let connectors = connectors.clone();
                let reservations = reservations.clone();
                let command_sender = command_sender.clone();
                let expiry_timers = expiry_timers.clone();
                d.on(move |req: CancelReservationRequest| {
                    let connectors = connectors.clone();
                    let reservations = reservations.clone();
                    let command_sender = command_sender.clone();
                    let expiry_timers = expiry_timers.clone();
                    async move {
                        let held = reservations.write().await.remove(&req.reservation_id);
                        let status = match held {
                            Some(cid) => {
                                // Disarm the pending auto-expiry timer (Issue #85) so
                                // it can't later fire on a connector that has moved
                                // on. Only done when *we* claimed the reservation
                                // (`held` was Some): the claim above removed the map
                                // entry, so any concurrent expiry task will now see
                                // it gone and no-op — meaning the timer is still
                                // sleeping and aborts cleanly, never mid-free.
                                if let Some(timer) =
                                    expiry_timers.write().await.remove(&req.reservation_id)
                                {
                                    timer.abort();
                                }
                                if let Some(mut connector) =
                                    connectors.read().await.get(&cid).cloned()
                                {
                                    let was_reserved =
                                        connector.status().await == ChargePointStatus::Reserved;
                                    let _ = connector.cancel_reservation().await;
                                    if was_reserved {
                                        // Best-effort: cancellation is already
                                        // Accepted; a dropped notification must not
                                        // undo it.
                                        let _ = command_sender.send(
                                            RemoteCommand::EmitConnectorStatus {
                                                connector_id: cid,
                                                status: ChargePointStatus::Available,
                                            },
                                        );
                                    }
                                }
                                CancelReservationStatus::Accepted
                            }
                            None => CancelReservationStatus::Rejected,
                        };
                        Ok(CancelReservationResponse { status })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, `CancelReservation`) — the inverse of the V201
            // `ReserveNow` slice, answering in the 2.0.1 dialect
            // (`CancelReservationStatusEnumType` + optional `statusInfo`). The pure
            // Accepted/Rejected decision + response builder live in `v201_command`
            // (#482); this is the runtime wiring, reusing the exact reservation
            // store, expiry-timer machinery, and `EmitConnectorStatus` seam the
            // 1.6J arm uses. Ports `ocpp.v201.call.CancelReservation` and the
            // `@on(Action.cancel_reservation)` dispatch shape from
            // `ocpp/charge_point.py`.
            OcppVersion::V201 => {
                let connectors = connectors.clone();
                let reservations = reservations.clone();
                let command_sender = command_sender.clone();
                let expiry_timers = expiry_timers.clone();
                d.on(move |req: V201CancelReservationRequest| {
                    let connectors = connectors.clone();
                    let reservations = reservations.clone();
                    let command_sender = command_sender.clone();
                    let expiry_timers = expiry_timers.clone();
                    async move {
                        // Decide the reported status and claim the reservation
                        // under a single `reservations` write-lock section, so a
                        // racing auto-expiry timer (Issue #85) can never make the
                        // Accepted/Rejected verdict and the removal disagree: the
                        // pure decision reads the held ids, and `remove` claims the
                        // requested id, both before the lock is released. `freed`
                        // carries the connector to release (the 1.6J arm's
                        // atomic-`remove` discipline, re-expressed so the reported
                        // status flows through the pure `v201_command` decision).
                        let (status, freed) = {
                            let mut map = reservations.write().await;
                            let held_ids: Vec<i32> = map.keys().copied().collect();
                            let status = v201_command::v201_cancel_reservation_status(
                                req.reservation_id,
                                &held_ids,
                            );
                            let freed = map.remove(&req.reservation_id);
                            (status, freed)
                        };
                        if let Some(cid) = freed {
                            // Disarm the pending auto-expiry timer (Issue #85) so
                            // it can't later fire on a connector that has moved on.
                            // The claim above already removed the map entry, so a
                            // concurrent expiry task now sees it gone and no-ops —
                            // the timer is still sleeping and aborts cleanly, never
                            // mid-free.
                            if let Some(timer) =
                                expiry_timers.write().await.remove(&req.reservation_id)
                            {
                                timer.abort();
                            }
                            if let Some(mut connector) = connectors.read().await.get(&cid).cloned()
                            {
                                let was_reserved =
                                    connector.status().await == ChargePointStatus::Reserved;
                                let _ = connector.cancel_reservation().await;
                                if was_reserved {
                                    // Best-effort: cancellation is already
                                    // Accepted; a dropped notification must not
                                    // undo it.
                                    let _ =
                                        command_sender.send(RemoteCommand::EmitConnectorStatus {
                                            connector_id: cid,
                                            status: ChargePointStatus::Available,
                                        });
                                }
                            }
                            // Report the CSMS-initiated teardown of a still-held
                            // reservation to the CSMS as `Removed` (Issue #546),
                            // the 2.0.1 counterpart to the auto-expiry `Expired`.
                            // Queued off the CALL path so the `CancelReservation`
                            // CALLRESULT flushes before this outbound CALL (no
                            // receive-loop re-entrancy). Only reached when `freed`
                            // was `Some` — i.e. the reservation was actually held —
                            // so a cancel of an unknown id (already `Rejected`
                            // above) reports nothing. Best-effort like the
                            // connector notification: the cancel is already
                            // Accepted, so a dropped update must not undo it.
                            let _ =
                                command_sender.send(RemoteCommand::V201ReservationStatusUpdate {
                                    reservation_id: req.reservation_id,
                                    status: ReservationUpdateStatusEnumType::Removed,
                                });
                        }
                        Ok(v201_command::v201_cancel_reservation_response(status, None))
                    }
                });
            }
        }

        // SetChargingProfile — install a Smart Charging profile on the station.
        // Both dialects name the action `"SetChargingProfile"` but carry different
        // request shapes (1.6J `connectorId`/`csChargingProfiles` vs 2.0.1
        // `evseId`/`chargingProfile`), so — like ChangeAvailability / Reset above —
        // exactly one handler is registered per `protocol_version`; the negotiated
        // subprotocol and the version-aware inbound validator keep the wire on a
        // single dialect, so the other version's request shape never reaches this
        // dispatcher.
        match protocol_version {
            // OCPP 1.6J §5.16 — install a Smart Charging profile against a
            // connector (0 = charge-point-wide) per the 1.6J stacking rules.
            // Placement is validated faithfully first (ChargePointMaxProfile only
            // at connector 0, TxProfile only at a real connector, unknown connector
            // rejected); only an Accepted profile is stored, replacing any prior
            // one with the same id or (purpose, stackLevel) slot. Storing is a
            // local state change — enforcing the limit on delivered power and
            // computing the composite schedule are out of scope (the latter is
            // GetCompositeSchedule, Issue #95). Ports `SetChargingProfile` from the
            // Python reference's `call.py`/`enums.py`.
            OcppVersion::V16J => {
                let connectors = connectors.clone();
                let charging_profiles = charging_profiles.clone();
                let active_transactions = active_transactions.clone();
                d.on(move |req: SetChargingProfileRequest| {
                    let connectors = connectors.clone();
                    let charging_profiles = charging_profiles.clone();
                    let active_transactions = active_transactions.clone();
                    async move {
                        let connector_id = req.connector_id;
                        // connector 0 is the CP-wide slot (not in the connector map);
                        // any other id must name a connector this CP exposes.
                        let connector_known = match ConnectorId::new(connector_id as u32) {
                            Ok(cid) => connectors.read().await.contains_key(&cid),
                            Err(_) => false,
                        };
                        // A TxProfile is transaction-scoped (§5.16.1): it is only
                        // valid on a connector that currently has an ongoing
                        // transaction. `active_transactions` maps transactionId →
                        // ConnectorId, so the connector is busy iff it appears as a
                        // value. (Connector 0 never has a transaction; its TxProfile
                        // rejection is handled by `set_profile_status`.)
                        let transaction_active = match ConnectorId::new(connector_id as u32) {
                            Ok(cid) => active_transactions.read().await.values().any(|c| *c == cid),
                            Err(_) => false,
                        };
                        let status = crate::charging_profiles::set_profile_status(
                            connector_id,
                            connector_known,
                            transaction_active,
                            &req.cs_charging_profiles.charging_profile_purpose,
                        );
                        if status == ChargingProfileStatus::Accepted {
                            charging_profiles.set(connector_id, req.cs_charging_profiles);
                        }
                        Ok(SetChargingProfileResponse { status })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, `SetChargingProfile`) — the direct CSMS→CS command
            // to install a charging profile on an EVSE, out-of-band from a remote
            // start. The simulator installs a transaction-scoped `TxProfile` into
            // the same `v201_tx_profiles` store the `RequestStartTransaction` path
            // uses (slice 7d, #450), so the periodic-metering resolver (slice 7e,
            // #455/#463) enforces it on the next tick with no further wiring. The
            // pure Accepted/Rejected decision + response builder live in
            // `v201_command` (#469); this is the runtime wiring. Ports
            // `ocpp.v201.call.SetChargingProfile` and the
            // `@on(Action.set_charging_profile)` dispatch shape from
            // `ocpp/charge_point.py`.
            //
            // Every `chargingProfilePurpose` is now honored: `TxProfile` (bound to a
            // live transaction), `TxDefaultProfile` (#512), and the station-wide
            // ceilings `ChargingStationMaxProfile` / `ChargingStationExternalConstraints`
            // (#511) — each routed to its own store below. Only a `TxProfile` with no
            // ongoing transaction on the target EVSE is Rejected, with an explanatory
            // `statusInfo`, mirroring the RequestStartTransaction guard above.
            OcppVersion::V201 => {
                let active_transactions = active_transactions.clone();
                let v201_tx_profiles = v201_tx_profiles.clone();
                let v201_tx_default_profiles = v201_tx_default_profiles.clone();
                let v201_station_ceilings = v201_station_ceilings.clone();
                d.on(move |req: V201SetChargingProfileRequest| {
                    let active_transactions = active_transactions.clone();
                    let v201_tx_profiles = v201_tx_profiles.clone();
                    let v201_tx_default_profiles = v201_tx_default_profiles.clone();
                    let v201_station_ceilings = v201_station_ceilings.clone();
                    async move {
                        let evse_id = req.evse_id;
                        // An ongoing transaction on the target EVSE. The v201 store
                        // and metering resolver key by EVSE id (= connector value)
                        // in the simulator's flat topology, and `active_transactions`
                        // maps transactionId → ConnectorId, so the EVSE is busy iff a
                        // live transaction's connector matches it. An `evseId` of 0 /
                        // negative / out of range simply matches nothing (never
                        // panics) → "no transaction to bind the TxProfile to".
                        let has_active_transaction = evse_id >= 1
                            && active_transactions
                                .read()
                                .await
                                .values()
                                .any(|cid| i64::from(cid.value()) == i64::from(evse_id));

                        let purpose = req.charging_profile.charging_profile_purpose;
                        let (status, status_info) = v201_command::v201_set_charging_profile_status(
                            purpose,
                            has_active_transaction,
                        );

                        // Only an accepted install mutates a store; a rejection
                        // leaves state untouched. The *purpose* selects the store,
                        // each an upsert per EVSE key (last accepted install wins):
                        //
                        // - `TxProfile` → the transaction-scoped `v201_tx_profiles`
                        //   (supersedes a RequestStartTransaction-installed one on
                        //   the same EVSE; `close_transaction` clears it when the
                        //   transaction ends).
                        // - `TxDefaultProfile` → `v201_tx_default_profiles`, station
                        //   configuration that persists across transactions and is
                        //   applied as the fallback when no `TxProfile` is in force
                        //   (`evseId = 0` installs the station-wide default). Nothing
                        //   clears it on transaction end (Issue #471).
                        // - `ChargingStationMaxProfile` /
                        //   `ChargingStationExternalConstraints` → `v201_station_ceilings`,
                        //   station-wide *ceilings* that cap the resolved limit rather
                        //   than substitute for it (`evseId = 0` installs the
                        //   whole-station ceiling). Also persists across transactions
                        //   (Issue #511).
                        if status == ChargingProfileStatusEnumType::Accepted {
                            match purpose {
                                ChargingProfilePurposeEnumType::TxProfile => {
                                    v201_tx_profiles
                                        .install(evse_id, req.charging_profile)
                                        .await;
                                }
                                ChargingProfilePurposeEnumType::TxDefaultProfile => {
                                    v201_tx_default_profiles
                                        .install(evse_id, req.charging_profile)
                                        .await;
                                }
                                ChargingProfilePurposeEnumType::ChargingStationMaxProfile
                                | ChargingProfilePurposeEnumType::ChargingStationExternalConstraints => {
                                    // `from_purpose` is `Some` for exactly these two
                                    // arms; the decision only `Accept`s a ceiling for
                                    // one of them, so this never silently drops one.
                                    if let Some(kind) = CeilingKind::from_purpose(purpose) {
                                        v201_station_ceilings
                                            .install(kind, evse_id, req.charging_profile)
                                            .await;
                                    }
                                }
                            }
                        }

                        Ok(v201_command::v201_set_charging_profile_response(
                            status,
                            status_info,
                        ))
                    }
                });
            }
        }

        // ClearChargingProfile — the teardown counterpart to SetChargingProfile:
        // the CSMS removes installed charging profiles matching an optional
        // selector, mid-session, without ending the transaction. `Accepted` if at
        // least one profile matched, else `Unknown`. The two protocol versions
        // carry different selector shapes and target different stores, so the
        // registration is version-gated exactly like SetChargingProfile above.
        // Ports `ClearChargingProfile` from the Python reference's
        // `call.py`/`enums.py`.
        match protocol_version {
            // OCPP 1.6J §5.3 — clear against the 1.6J `ChargingProfileStore` by
            // the optional filters (`id`, `connectorId`, `chargingProfilePurpose`,
            // `stackLevel`); a `None` filter matches anything, so an all-`None`
            // request clears the whole store. Unchanged from the pre-version-gate
            // behavior.
            OcppVersion::V16J => {
                let charging_profiles = charging_profiles.clone();
                d.on(move |req: ClearChargingProfileRequest| {
                    let charging_profiles = charging_profiles.clone();
                    async move {
                        let status = charging_profiles.clear(
                            req.id,
                            req.connector_id,
                            req.charging_profile_purpose,
                            req.stack_level,
                        );
                        Ok(ClearChargingProfileResponse { status })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, `ClearChargingProfile`) — the inverse of the
            // V201 `SetChargingProfile` arm above: resolve the request's selector
            // against the `v201_tx_profiles` store and remove every matching
            // slot, so the next periodic `TransactionEvent(Updated)` metering
            // reading reports the unbounded power. The pure match decision +
            // response builder live in `v201_command` (#474); this is the runtime
            // wiring. `chargingProfileId` selects exclusively; otherwise the
            // `evseId`/`chargingProfilePurpose`/`stackLevel` criteria filter, and
            // an empty request clears every installed profile.
            OcppVersion::V201 => {
                let v201_tx_profiles = v201_tx_profiles.clone();
                d.on(move |req: V201ClearChargingProfileRequest| {
                    let v201_tx_profiles = v201_tx_profiles.clone();
                    async move {
                        // Snapshot off the store lock, decide, then remove the
                        // matched EVSE slots. In the simulator the CSMS's CALLs
                        // are serialized through the single dispatcher, so no
                        // profile can be swapped underneath the snapshot→clear
                        // gap.
                        let installed = v201_tx_profiles.snapshot().await;
                        let matches = v201_command::v201_clear_charging_profile_matches(
                            req.charging_profile_id,
                            req.charging_profile_criteria.as_ref(),
                            &installed,
                        );
                        for evse_id in &matches {
                            v201_tx_profiles.clear(*evse_id).await;
                        }
                        Ok(v201_command::v201_clear_charging_profile_response(
                            !matches.is_empty(),
                        ))
                    }
                });
            }
        }

        // GetCompositeSchedule — report the effective charging schedule for a
        // connector over the requested window by combining the installed profiles
        // per the 1.6J stacking rules (§5.x). The candidate set is the requested
        // connector's own profiles (more specific) plus the charge-point-wide
        // connector-0 profiles (the ChargePointMaxProfile ceiling and any default
        // inherited from connector 0). An unknown connector, a non-positive
        // duration, or no applicable profile yields `Rejected`; otherwise the
        // composite schedule is computed by `crate::composite`. The Python
        // reference ships only the wire types (its example CP returns a canned
        // response), so the computation follows the 1.6J spec (Issue #95).
        //
        // Both dialects name the action `"GetCompositeSchedule"` but carry
        // different request/response shapes (1.6J `connectorId` /
        // `chargingSchedule` vs 2.0.1 `evseId` / `CompositeScheduleType`), so —
        // like `SetChargingProfile` above — exactly one handler is registered per
        // `protocol_version`; the negotiated subprotocol and the version-aware
        // inbound validator keep the wire on a single dialect.
        match protocol_version {
            OcppVersion::V16J => {
                let connectors = connectors.clone();
                let charging_profiles = charging_profiles.clone();
                d.on(move |req: GetCompositeScheduleRequest| {
                    let connectors = connectors.clone();
                    let charging_profiles = charging_profiles.clone();
                    async move {
                        let connector_id = req.connector_id;
                        let connector_known = match ConnectorId::new(connector_id as u32) {
                            Ok(cid) => connectors.read().await.contains_key(&cid),
                            Err(_) => false,
                        };
                        // connector 0 is the CP-wide slot; any other id must exist.
                        let rejected = GetCompositeScheduleResponse {
                            status: GetCompositeScheduleStatus::Rejected,
                            connector_id: None,
                            schedule_start: None,
                            charging_schedule: None,
                        };
                        if connector_id != 0 && !connector_known {
                            return Ok(rejected);
                        }

                        // Gather candidates: the connector's own profiles (specific)
                        // plus connector-0 profiles (inherited), avoiding a double
                        // count when the request *is* connector 0.
                        let mut candidates: Vec<composite::ScopedProfile> = charging_profiles
                            .profiles_for(connector_id)
                            .into_iter()
                            .map(|profile| composite::ScopedProfile {
                                specific: true,
                                profile,
                            })
                            .collect();
                        if connector_id != 0 {
                            candidates.extend(charging_profiles.profiles_for(0).into_iter().map(
                                |profile| composite::ScopedProfile {
                                    specific: false,
                                    profile,
                                },
                            ));
                        }

                        let start = chrono::Utc::now();
                        match composite::compute_composite(
                            &candidates,
                            start,
                            req.duration,
                            req.charging_rate_unit,
                        ) {
                            Some(schedule) => Ok(GetCompositeScheduleResponse {
                                status: GetCompositeScheduleStatus::Accepted,
                                connector_id: Some(connector_id),
                                schedule_start: Some(start),
                                charging_schedule: Some(schedule),
                            }),
                            None => Ok(rejected),
                        }
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, `GetCompositeSchedule`) — the CSMS asks the
            // station to compute the net schedule it will enforce for an EVSE over
            // the requested window, after stacking the applicable profiles. The
            // simulator resolves the `TxProfile` (else the `TxDefaultProfile`
            // fallback) in force on the EVSE and composes it over the window by the
            // same core the periodic-metering path uses (`v201_charging_profiles`,
            // #464/#466), then **caps** each reported period by any station ceiling
            // in force — `min(resolved, ChargingStationMaxProfile,
            // ChargingStationExternalConstraints)` (#511). An EVSE with no installed
            // base profile — or a profile that constrains no instant in the window
            // (incl. a non-positive `duration`) — yields `Rejected` with no schedule
            // (a ceiling alone never manufactures a schedule). Ports
            // `ocpp.v201.call.GetCompositeSchedule` and the
            // `@on(Action.get_composite_schedule)` dispatch shape from
            // `ocpp/charge_point.py`.
            OcppVersion::V201 => {
                let connectors = connectors.clone();
                let v201_tx_profiles = v201_tx_profiles.clone();
                let v201_tx_default_profiles = v201_tx_default_profiles.clone();
                let v201_station_ceilings = v201_station_ceilings.clone();
                d.on(move |req: V201GetCompositeScheduleRequest| {
                    let connectors = connectors.clone();
                    let v201_tx_profiles = v201_tx_profiles.clone();
                    let v201_tx_default_profiles = v201_tx_default_profiles.clone();
                    let v201_station_ceilings = v201_station_ceilings.clone();
                    async move {
                        let rejected = V201GetCompositeScheduleResponse {
                            status: GenericStatusEnumType::Rejected,
                            schedule: None,
                            status_info: None,
                            custom_data: None,
                        };
                        // The profile in force on the EVSE: the `TxProfile` if one
                        // is installed, else the EVSE's `TxDefaultProfile` fallback
                        // (specific-EVSE default, else the `evseId = 0` station-wide
                        // default), matching the metering resolver's precedence
                        // (Issue #471). An out-of-range / 0 / negative `evseId`
                        // simply misses both stores (never panics). No profile at all
                        // → nothing to compose → Rejected.
                        let profile = match v201_tx_profiles.get(req.evse_id).await {
                            Some(profile) => profile,
                            None => {
                                match v201_tx_default_profiles.effective_for(req.evse_id).await {
                                    Some(profile) => profile,
                                    None => return Ok(rejected),
                                }
                            }
                        };
                        // Nominal voltage for any A↔W conversion, read from the
                        // EVSE's connector config (the value the metering resolver
                        // uses); fall back to the European single-phase nominal if
                        // the connector is somehow absent, so a conversion never
                        // divides by zero.
                        let nominal_voltage_v = match ConnectorId::new(req.evse_id as u32) {
                            Ok(cid) => connectors
                                .read()
                                .await
                                .get(&cid)
                                .map_or(230.0, |c| c.config().max_voltage),
                            Err(_) => 230.0,
                        };
                        // The station ceilings in force for this EVSE (Issue #511):
                        // each caps the composed schedule's periods by `min`. Absent
                        // ceilings, the composite is exactly the base profile's.
                        let max_ceiling = v201_station_ceilings
                            .effective_for(CeilingKind::Max, req.evse_id)
                            .await;
                        let external_ceiling = v201_station_ceilings
                            .effective_for(CeilingKind::External, req.evse_id)
                            .await;
                        let ceilings: Vec<&ChargingProfileType> =
                            [max_ceiling.as_ref(), external_ceiling.as_ref()]
                                .into_iter()
                                .flatten()
                                .collect();
                        let window_start = chrono::Utc::now();
                        match v201_charging_profiles::compose_composite_schedule_capped(
                            req.evse_id,
                            &profile,
                            &ceilings,
                            window_start,
                            req.duration,
                            req.charging_rate_unit,
                            nominal_voltage_v,
                        ) {
                            Some(schedule) => Ok(V201GetCompositeScheduleResponse {
                                status: GenericStatusEnumType::Accepted,
                                schedule: Some(schedule),
                                status_info: None,
                                custom_data: None,
                            }),
                            None => Ok(rejected),
                        }
                    }
                });
            }
        }

        // ClearCache — empty the authorization cache, then accept (Issue #23).
        // The CSMS asks the CP to discard its Authorization Cache. Both dialects
        // clear the same shared, dialect-independent `AuthCache`, but carry
        // different response shapes (1.6J's bare `ClearCacheStatus` vs 2.0.1's
        // `ClearCacheStatusEnumType` + optional `statusInfo`/`customData`), so —
        // like SendLocalList / CancelReservation above — exactly one handler is
        // registered per `protocol_version`. The clear is idempotent (a no-op on
        // an already-empty cache) and the request carries no CSMS-supplied fields,
        // so neither arm has a malformed-input branch.
        match protocol_version {
            // OCPP 1.6J §5.2 (Issue #23). Unchanged from the pre-version-gate
            // behavior.
            OcppVersion::V16J => {
                let auth_cache = auth_cache.clone();
                d.on(move |_req: ClearCacheRequest| {
                    let auth_cache = auth_cache.clone();
                    async move {
                        auth_cache.clear();
                        Ok(ClearCacheResponse {
                            status: ClearCacheStatus::Accepted,
                        })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, D03) — the same shared cache is emptied and the
            // clear reported in the 2.0.1 dialect. The station implements a cache,
            // so the pure decision is `Accepted`. Ports `ocpp.v201.call.ClearCache`
            // and the `@on(Action.clear_cache)` dispatch shape from
            // `ocpp/charge_point.py`.
            OcppVersion::V201 => {
                let auth_cache = auth_cache.clone();
                d.on(move |_req: V201ClearCacheRequest| {
                    let auth_cache = auth_cache.clone();
                    async move {
                        auth_cache.clear();
                        let status = v201_command::v201_clear_cache_status(true);
                        Ok::<V201ClearCacheResponse, _>(v201_command::v201_clear_cache_response(
                            status, None,
                        ))
                    }
                });
            }
        }

        // DataTransfer — route by (vendorId, messageId) through the registry.
        // An unimplemented vendor/message yields the faithful UnknownVendorId /
        // UnknownMessageId; a registered handler decides Accepted/Rejected (+
        // optional data). With no handlers registered the registry answers
        // UnknownVendorId for every request. Both dialects name the action
        // `"DataTransfer"` but carry different request shapes (1.6J's `data` is
        // an opaque string, 2.0.1's is free-form JSON), so — like
        // SetChargingProfile above — exactly one handler is registered per
        // `protocol_version`; the negotiated subprotocol and version-aware
        // inbound validator keep the wire on a single dialect. Both arms consult
        // the *same* shared registry (Issue #101), so `register_data_transfer_handler`
        // is the single registration API for either version.
        match protocol_version {
            // OCPP 1.6J §6.x — the original vendor-extension escape hatch.
            OcppVersion::V16J => {
                let data_transfer = data_transfer.clone();
                d.on(move |req: DataTransferRequest| {
                    let data_transfer = data_transfer.clone();
                    async move { Ok(data_transfer.dispatch(&req)) }
                });
            }
            // OCPP 2.0.1 (Part 2, `DataTransfer`) — the same escape hatch with a
            // free-form JSON `data` field. The `v201_data_transfer` adapter routes
            // through the shared 1.6J registry and maps the request/response at the
            // boundary (Issue #470), so the routing table stays the single source
            // of truth. Ports `ocpp.v201.call.DataTransfer` and the
            // `@on(Action.data_transfer)` dispatch shape from `ocpp/charge_point.py`.
            OcppVersion::V201 => {
                let data_transfer = data_transfer.clone();
                d.on(move |req: V201DataTransferRequest| {
                    let data_transfer = data_transfer.clone();
                    async move { Ok(v201_data_transfer::dispatch(&data_transfer, &req)) }
                });
            }
        }

        // GetLocalListVersion — report the version of the Local Authorization
        // List. Both dialects read the same shared version counter but carry
        // different response shapes (1.6J `listVersion` vs 2.0.1 `versionNumber`
        // + optional `customData`), so — like SendLocalList below and
        // CancelReservation above — exactly one handler is registered per
        // `protocol_version`. `0` for an empty list; the CP never returns `-1`
        // because it implements the profile.
        match protocol_version {
            // OCPP 1.6J §5.x (Issue #93). Unchanged from the pre-version-gate
            // behavior.
            OcppVersion::V16J => {
                let local_list = local_list.clone();
                d.on(move |_req: GetLocalListVersionRequest| {
                    let local_list = local_list.clone();
                    async move {
                        Ok(GetLocalListVersionResponse {
                            list_version: local_list.version(),
                        })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, D02) — the same shared version reported in the
            // 2.0.1 `versionNumber` shape. Ports `ocpp.v201.call.GetLocalListVersion`
            // and the `@on(Action.get_local_list_version)` dispatch shape from
            // `ocpp/charge_point.py`.
            OcppVersion::V201 => {
                let local_list = local_list.clone();
                d.on(move |_req: V201GetLocalListVersionRequest| {
                    let local_list = local_list.clone();
                    async move {
                        Ok(V201GetLocalListVersionResponse {
                            version_number: local_list.version(),
                            custom_data: None,
                        })
                    }
                });
            }
        }

        // SendLocalList — apply a Full or Differential update to the Local
        // Authorization List. Both dialects share the one list store (version,
        // capacity, Full/Differential semantics), but carry different request and
        // response shapes and status enums (1.6J `UpdateStatus` vs 2.0.1
        // `SendLocalListStatusEnumType` + optional `statusInfo`), so exactly one
        // handler is registered per `protocol_version`. The list itself enforces
        // version ordering, duplicate rejection, and the over-capacity guard,
        // returning the faithful status.
        match protocol_version {
            // OCPP 1.6J §5.x (Issue #93). Unchanged from the pre-version-gate
            // behavior.
            OcppVersion::V16J => {
                let local_list = local_list.clone();
                d.on(move |req: SendLocalListRequest| {
                    let local_list = local_list.clone();
                    async move {
                        Ok(SendLocalListResponse {
                            status: local_list.apply(&req),
                        })
                    }
                });
            }
            // OCPP 2.0.1 (Part 2, D01) — the 2.0.1 request is applied against the
            // same store via `apply_v201`, which reuses the 1.6J accept/reject
            // decision and maps the outcome to `SendLocalListStatusEnumType`.
            // Ports `ocpp.v201.call.SendLocalList` and the
            // `@on(Action.send_local_list)` dispatch shape from `ocpp/charge_point.py`.
            OcppVersion::V201 => {
                let local_list = local_list.clone();
                d.on(move |req: V201SendLocalListRequest| {
                    let local_list = local_list.clone();
                    async move {
                        Ok(V201SendLocalListResponse {
                            status: local_list.apply_v201(&req),
                            status_info: None,
                            custom_data: None,
                        })
                    }
                });
            }
        }

        d
    }

    /// Arm (or replace) the auto-expiry timer for an `Accepted` reservation
    /// (OCPP 1.6J §5.14, Issue #85).
    ///
    /// Spawns a task that sleeps until `expiry_date`, then atomically claims the
    /// `reservationId → connector` mapping and — only if it is still held —
    /// frees the connector (`Reserved → Available`) and emits a
    /// `StatusNotification(Available)` off the inbound-CALL path via the
    /// `RemoteCommand` consumer. The claim is taken under the `reservations`
    /// write-lock, so a racing `CancelReservation` / start-consume can never
    /// double-free. Finished handles are pruned and any prior timer for the same
    /// `reservationId` is aborted, keeping the timer map bounded.
    #[allow(clippy::too_many_arguments)]
    async fn arm_reservation_expiry(
        reservation_id: i32,
        connector_id: ConnectorId,
        expiry_date: chrono::DateTime<chrono::Utc>,
        connectors: &Arc<RwLock<HashMap<ConnectorId, Connector>>>,
        reservations: &Arc<RwLock<HashMap<i32, ConnectorId>>>,
        expiry_timers: &Arc<RwLock<HashMap<i32, tokio::task::JoinHandle<()>>>>,
        command_sender: &mpsc::UnboundedSender<RemoteCommand>,
        // The dialect this reservation was made on. A 2.0.1 station additionally
        // reports the auto-expiry to the CSMS via `ReservationStatusUpdate(Expired)`
        // (Issue #546); 1.6J has no such message, so a 1.6J expiry only frees the
        // connector. The reservation/expiry machinery is otherwise shared verbatim
        // between the two `ReserveNow` arms, so the version is threaded in here
        // rather than duplicating the timer body.
        protocol_version: OcppVersion,
    ) {
        // `expiry_date` is in the future here (the handler rejects past-dated
        // reservations before arming); `unwrap_or(ZERO)` is purely defensive
        // against the sub-millisecond gap to `now()`.
        let ttl = (expiry_date - chrono::Utc::now())
            .to_std()
            .unwrap_or(Duration::ZERO);
        let connectors = connectors.clone();
        let reservations = reservations.clone();
        let command_sender = command_sender.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            // Claim the reservation atomically: only the task that still owns
            // this exact (reservationId → connector) mapping frees it, so a
            // racing CancelReservation / start-consume can't cause a double-free
            // or a free of a connector that has since moved on.
            let still_held = {
                let mut map = reservations.write().await;
                if map.get(&reservation_id) == Some(&connector_id) {
                    map.remove(&reservation_id);
                    true
                } else {
                    false
                }
            };
            if still_held {
                if let Some(mut connector) = connectors.read().await.get(&connector_id).cloned() {
                    let was_reserved = connector.status().await == ChargePointStatus::Reserved;
                    let _ = connector.cancel_reservation().await;
                    if was_reserved {
                        // Best-effort: the connector is already freed locally; a
                        // dropped notification (consumer gone) must not undo it.
                        let _ = command_sender.send(RemoteCommand::EmitConnectorStatus {
                            connector_id,
                            status: ChargePointStatus::Available,
                        });
                    }
                }
                // Report the auto-expiry to the CSMS on the 2.0.1 dialect only
                // (Issue #546). Gated on `still_held` (this task actually claimed
                // and freed the reservation), so a reservation already gone — a
                // race-losing cancel or a start that consumed it — emits nothing,
                // and a `CancelReservation` that already reported `Removed` can
                // never be double-reported here (it removed the map entry, making
                // `still_held` false, and aborted this timer besides). Best-effort
                // like the connector notification above.
                if protocol_version == OcppVersion::V201 {
                    let _ = command_sender.send(RemoteCommand::V201ReservationStatusUpdate {
                        reservation_id,
                        status: ReservationUpdateStatusEnumType::Expired,
                    });
                }
            }
            // A fired timer's handle is left in the map; it is reclaimed by the
            // `is_finished()` prune on the next arm and by the abort in `stop()`.
            // We deliberately do NOT self-remove, to avoid evicting a same-id
            // timer that may have replaced this one in the meantime.
        });
        let mut timers = expiry_timers.write().await;
        timers.retain(|_, h| !h.is_finished());
        if let Some(old) = timers.insert(reservation_id, handle) {
            old.abort();
        }
    }

    /// Register a custom `@on` handler for action `Req::ACTION_NAME`.
    ///
    /// Overrides the default handler if one was registered in `new()`. Must be
    /// called before [`ChargePoint::start`].
    ///
    /// Ports the `@on(action)` decorator semantics from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
    pub async fn on<Req, Fut, F>(&self, handler: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        self.dispatcher.write().await.on(handler);
    }

    /// Register a fire-and-forget `@after` hook for action `Req::ACTION_NAME`.
    ///
    /// The hook is spawned after the `@on` handler completes successfully and
    /// does not block the CALLRESULT response path.
    ///
    /// Ports the `@after(action)` decorator semantics from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
    pub async fn after<Req, Fut, F>(&self, hook: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = ()> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        self.dispatcher.write().await.after(hook);
    }

    /// Start the charge point
    pub async fn start(&self) -> Result<()> {
        info!("Starting charge point: {}", self.config.charge_point_id);

        // Initialize all connectors to Available
        let mut connectors = self.connectors.write().await;
        for connector in connectors.values_mut() {
            connector.set_status(ChargePointStatus::Available).await?;
        }
        drop(connectors);

        // Send started event
        let _ = self.event_sender.send(ChargePointEvent::Started);

        // Connect to central system
        self.connect().await?;

        Ok(())
    }

    /// Stop the charge point
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping charge point: {}", self.config.charge_point_id);

        // Stop the heartbeat and all periodic MeterValues samplers.
        self.quiesce_background_tasks().await;

        // Abort the remote-command consumer. It owns a `ChargePoint` clone whose
        // dispatcher holds a command sender, so it would otherwise keep the
        // channel — and all shared state — alive forever. Done only here (never
        // during a reset's quiesce), since the consumer drives the reset itself.
        if let Some(handle) = self.command_consumer.write().await.take() {
            handle.abort();
        }

        // Abort any pending reservation auto-expiry timers (Issue #85) so the
        // tasks don't leak past shutdown.
        for (_id, handle) in self.expiry_timers.write().await.drain() {
            handle.abort();
        }

        // Disconnect from central system
        self.disconnect().await?;

        // Set all connectors to unavailable
        let mut connectors = self.connectors.write().await;
        for connector in connectors.values_mut() {
            connector.set_status(ChargePointStatus::Unavailable).await?;
        }

        Ok(())
    }

    /// Connect to central system.
    ///
    /// On the first call this also spawns the single, long-lived remote-command
    /// consumer task (it takes the receiver; later calls find `None`). The
    /// consumer drives side effects — a CSMS `Reset`, or the local
    /// `StartTransaction`/`StopTransaction` behind an `Accepted`
    /// `RemoteStart`/`RemoteStopTransaction` — off the inbound CALL path so the
    /// CALLRESULT is sent first and the receive loop never re-enters itself. A
    /// Hard reset reconnects via `establish_session` (not `connect`), so it
    /// neither spawns a second consumer nor recurses.
    pub async fn connect(&self) -> Result<()> {
        let maybe_rx = self.command_receiver.write().await.take();
        if let Some(mut rx) = maybe_rx {
            let cp = self.clone();
            let handle = tokio::spawn(async move {
                while let Some(command) = rx.recv().await {
                    match command {
                        RemoteCommand::Reset { reset_type } => {
                            cp.perform_reset(reset_type).await;
                        }
                        RemoteCommand::StartTransaction {
                            connector_id,
                            id_tag,
                            remote_start_id,
                            charging_profile,
                            group_id_token,
                        } => {
                            // meter_start is unknown for a remote-initiated start;
                            // report 0, matching the Python reference's example CP.
                            // `remote_start_id` is `Some` only on the 2.0.1
                            // RequestStartTransaction path and is echoed onto the
                            // Started event for CSMS correlation; `charging_profile`
                            // / `group_id_token` are the 2.0.1 TxProfile install +
                            // groupIdToken threading (slice 7d).
                            if let Err(e) = cp
                                .start_transaction_with_remote_start_id(
                                    connector_id,
                                    &id_tag,
                                    0,
                                    remote_start_id,
                                    charging_profile.map(|b| *b),
                                    group_id_token,
                                )
                                .await
                            {
                                warn!(
                                    "remote start: failed to start transaction on \
                                     connector {}: {e}",
                                    connector_id.value()
                                );
                            }
                        }
                        RemoteCommand::StopTransaction { transaction_id } => {
                            // meter_stop is unknown for a remote-initiated stop;
                            // report 0, matching the reset path.
                            if let Err(e) =
                                cp.stop_transaction(transaction_id, 0, Reason::Remote).await
                            {
                                warn!(
                                    "remote stop: failed to stop transaction \
                                     {transaction_id}: {e}"
                                );
                            }
                        }
                        RemoteCommand::TriggerMessage {
                            requested_message,
                            connector_id,
                        } => {
                            cp.send_triggered_message(requested_message, connector_id)
                                .await;
                        }
                        RemoteCommand::V201TriggerMessage {
                            requested_message,
                            evse_id,
                        } => {
                            cp.send_v201_triggered_message(requested_message, evse_id)
                                .await;
                        }
                        RemoteCommand::V201NotifyReport {
                            request_id,
                            report_data,
                        } => {
                            cp.send_v201_notify_report(request_id, report_data).await;
                        }
                        RemoteCommand::V201NotifyMonitoringReport {
                            request_id,
                            monitor_data,
                        } => {
                            cp.send_v201_notify_monitoring_report(request_id, monitor_data)
                                .await;
                        }
                        RemoteCommand::V201ReportChargingProfiles {
                            request_id,
                            profiles,
                        } => {
                            cp.send_v201_report_charging_profiles(request_id, profiles)
                                .await;
                        }
                        RemoteCommand::V201NotifyDisplayMessages {
                            request_id,
                            messages,
                        } => {
                            cp.send_v201_notify_display_messages(request_id, messages)
                                .await;
                        }
                        RemoteCommand::V201LogUpload { request_id } => {
                            cp.run_v201_log_upload(request_id).await;
                        }
                        RemoteCommand::V201FirmwareUpdate { request_id } => {
                            cp.run_v201_firmware_update(request_id).await;
                        }
                        RemoteCommand::V201CustomerInformationReport { request_id } => {
                            cp.run_v201_customer_information_report(request_id).await;
                        }
                        RemoteCommand::V201PublishFirmwareStatus { request_id } => {
                            cp.run_v201_publish_firmware_status(request_id).await;
                        }
                        RemoteCommand::V201ReservationStatusUpdate {
                            reservation_id,
                            status,
                        } => {
                            cp.send_v201_reservation_status_update(reservation_id, status)
                                .await;
                        }
                        RemoteCommand::V201ApplyAvailability {
                            connector_id,
                            target,
                        } => {
                            cp.apply_v201_availability(connector_id, target).await;
                        }
                        RemoteCommand::GetDiagnostics => {
                            cp.run_diagnostics_upload().await;
                        }
                        RemoteCommand::UpdateFirmware => {
                            cp.run_firmware_update().await;
                        }
                        RemoteCommand::EmitConnectorStatus {
                            connector_id,
                            status,
                        } => {
                            if let Err(e) = cp
                                .send_status_notification(
                                    connector_id.value(),
                                    status,
                                    ChargePointErrorCode::NoError,
                                )
                                .await
                            {
                                warn!(
                                    "reservation StatusNotification({status:?}) for connector \
                                     {} failed: {e}",
                                    connector_id.value()
                                );
                            }
                        }
                        RemoteCommand::UnlockConnector {
                            connector_id,
                            transaction_id,
                        } => {
                            // meter_stop is unknown for an unlock-triggered stop;
                            // report 0 like the reset / remote-stop paths.
                            // stop_transaction frees the connector (→ Available)
                            // and emits the StatusNotification.
                            if let Err(e) = cp
                                .stop_transaction(transaction_id, 0, Reason::UnlockCommand)
                                .await
                            {
                                warn!(
                                    "unlock: failed to stop transaction {transaction_id} on \
                                     connector {}: {e}",
                                    connector_id.value()
                                );
                            }
                        }
                    }
                }
            });
            *self.command_consumer.write().await = Some(handle);
        }

        self.establish_session().await
    }

    /// Open a WebSocket session to the central system and run the boot
    /// handshake (`BootNotification` → retry loop → heartbeat start).
    ///
    /// Split out of [`connect`](Self::connect) so the Hard-reset path can
    /// re-establish a session without re-running the consumer-spawn logic and
    /// without the `connect → perform_reset → connect` recursion.
    async fn establish_session(&self) -> Result<()> {
        let url = format!(
            "{}/ocpp/{}",
            self.config.central_system_url.trim_end_matches('/'),
            self.config.charge_point_id
        );

        info!("Connecting to central system: {}", url);

        // Build the bridge handler that routes messages through the dispatcher.
        let handler = Arc::new(CpHandler {
            dispatcher: self.dispatcher.clone(),
            event_sender: self.event_sender.clone(),
            is_connected: self.is_connected.clone(),
        });

        let client =
            WebSocketClient::new(url, self.config.transport_config.clone(), handler).await?;

        // Store client
        *self.client.write().await = Some(client);
        *self.is_connected.write().await = true;

        // Send connected event
        let _ = self.event_sender.send(ChargePointEvent::Connected);

        // Run the boot sequence: BootNotification → retry loop → heartbeat start.
        self.boot_sequence().await?;

        Ok(())
    }

    /// Disconnect from central system
    pub async fn disconnect(&self) -> Result<()> {
        if let Some(client) = self.client.write().await.take() {
            client.close().await?;
        }

        *self.is_connected.write().await = false;

        let _ = self.event_sender.send(ChargePointEvent::Disconnected {
            reason: "Manual disconnect".to_string(),
        });

        Ok(())
    }

    /// Check if connected to central system
    pub async fn is_connected(&self) -> bool {
        *self.is_connected.read().await
    }

    /// Get registration status
    pub async fn registration_status(&self) -> RegistrationStatus {
        *self.registration_status.read().await
    }

    /// Send a typed OCPP CALL and await the matching CALLRESULT.
    ///
    /// This is the Rust port of `ChargePoint.call()` from the Python reference
    /// (`ocpp/charge_point.py`). It:
    ///
    /// 1. Generates a unique message ID
    /// 2. Registers the ID in `PendingCallMap` *before* sending (race-free)
    /// 3. Sends the CALL frame over the WebSocket
    /// 4. Awaits the response with `config.call_timeout`
    /// 5. Returns the deserialized `Req::Response` or propagates any error
    ///
    /// Returns `OcppError::Timeout` if no response arrives within the
    /// configured timeout, and `OcppError::CallError` if the server replies
    /// with a CALLERROR frame.
    pub async fn call<Req: OcppAction>(&self, request: Req) -> OcppResult<Req::Response> {
        // Serialize the typed request up front so it can be schema-validated
        // *before* any pending-call registration or network I/O.
        let payload = serde_json::to_value(&request).map_err(OcppError::from)?;

        // Validate the OUTGOING request against the `{action}` schema before
        // sending, mirroring `call()` in charge_point.py, which runs
        // `validate_payload(call)` prior to `_send`. A strongly-typed request
        // can still be schema-invalid (e.g. a `String` field exceeding its
        // `maxLength`); the reference rejects such a payload locally rather
        // than putting a malformed CALL on the wire
        // (`test_v16_charge_point.py::test_send_invalid_call`). Gated by
        // `config.validate_payloads`, whose off state is the Rust analog of the
        // reference's `skip_schema_validation=True`. This runs before the
        // connection check so a schema-invalid request surfaces as a
        // `SchemaViolation` regardless of link state, matching the reference's
        // validate-before-`_send` ordering.
        if let Some(validator) = &self.validator {
            validator.validate_call(Req::ACTION_NAME, &payload)?;
        }

        let unique_id = Uuid::new_v4().to_string();

        // 1. Register before sending to avoid the race where the CALLRESULT
        //    arrives before we have a receiver in the map. `register_guarded`
        //    returns an RAII guard that prunes the entry when this future
        //    returns (or is cancelled), so a call that times out — or bails on
        //    the transport error below — leaves no stale sender behind
        //    (Issue #323). On the happy path the recv loop already removed the
        //    entry, so the guard's drop is a harmless no-op.
        let (rx, _prune_guard) = {
            let client_guard = self.client.read().await;
            let client = client_guard.as_ref().ok_or_else(|| OcppError::Transport {
                message: "Not connected to central system".to_string(),
            })?;
            client.pending_calls().register_guarded(unique_id.clone())
        };

        // 2. Build the CALL frame with the same unique_id, reusing the payload
        //    serialized (and validated) above.
        let call_msg = CallMessage {
            message_type: MessageType::Call,
            unique_id: unique_id.clone(),
            action: Req::ACTION_NAME.to_string(),
            payload,
        };

        // 3. Send the frame.
        {
            let client_guard = self.client.read().await;
            match client_guard.as_ref() {
                Some(client) => client.send_message(Message::Call(call_msg)).await?,
                None => {
                    return Err(OcppError::Transport {
                        message: "Not connected to central system".to_string(),
                    })
                }
            }
        }

        // 4. Await the CALLRESULT (or CALLERROR) with timeout.
        //    The recv loop resolves/rejects `rx` when the response arrives.
        let timeout = Duration::from_secs(self.config.call_timeout);
        let raw_result = tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| OcppError::Timeout {
                operation: format!("{} call", Req::ACTION_NAME),
            })?
            .map_err(|_| OcppError::Transport {
                // oneshot RecvError means the sender was dropped (disconnect)
                message: "Connection closed while waiting for CALLRESULT".to_string(),
            })?;

        // 5. Propagate any CALLERROR, then deserialize the success payload.
        let payload = raw_result?;

        // Validate the CALLRESULT against the `{action}Response` schema before
        // deserializing, mirroring `_handle_call_result()` in charge_point.py.
        // A conformant CSMS should never send a malformed response, but the
        // incoming WebSocket frame is a trust boundary — reject it explicitly
        // rather than surfacing an opaque serde error.
        if let Some(validator) = &self.validator {
            validator.validate_call_result(Req::ACTION_NAME, &payload)?;
        }

        serde_json::from_value::<Req::Response>(payload).map_err(OcppError::from)
    }

    /// Port of the BootNotification sequence from
    /// [`charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
    /// and [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    ///
    /// Sends `BootNotification` via `call()`, drives the Rejected-retry loop
    /// using the CSMS-supplied `interval`, and starts the heartbeat task at
    /// the CSMS-supplied interval on `Accepted` or `Pending`.
    ///
    /// Returns `OcppError::BootRejected` after `config.max_boot_retries + 1`
    /// consecutive rejections.
    /// Build a `BootNotificationRequest` from the configured vendor info.
    ///
    /// Shared by the boot handshake and the `TriggerMessage(BootNotification)`
    /// side effect so both report identical charge-point identity.
    fn boot_notification_request(&self) -> BootNotificationRequest {
        BootNotificationRequest {
            charge_point_vendor: self.config.vendor_info.charge_point_vendor.clone(),
            charge_point_model: self.config.vendor_info.charge_point_model.clone(),
            charge_point_serial_number: self.config.vendor_info.charge_point_serial_number.clone(),
            charge_box_serial_number: self.config.vendor_info.charge_box_serial_number.clone(),
            firmware_version: self.config.vendor_info.firmware_version.clone(),
            iccid: self.config.vendor_info.iccid.clone(),
            imsi: self.config.vendor_info.imsi.clone(),
            meter_type: self.config.vendor_info.meter_type.clone(),
            meter_serial_number: self.config.vendor_info.meter_serial_number.clone(),
        }
    }

    /// Send a single `BootNotification` in the configured OCPP version and
    /// return a version-normalized [`BootOutcome`].
    ///
    /// This is the one version-branching seam in the boot handshake: `V16J`
    /// sends the 1.6J [`BootNotificationRequest`] built from vendor info, while
    /// `V201` sends the spec-valid 2.0.1 request built by
    /// [`ChargePointConfig::v201_boot_notification_request`] (slice 1, #417).
    /// Both responses are collapsed onto [`BootOutcome`] so the retry loop in
    /// [`boot_sequence`](Self::boot_sequence) stays version-agnostic.
    ///
    /// `call()` validates the outgoing request and the incoming CALLRESULT
    /// against the CP's version-aware validator, so a `V201` CP checks its
    /// 2.0.1 boot payload against the 2.0.1 schema.
    async fn send_boot_notification(&self) -> OcppResult<BootOutcome> {
        match self.config.protocol_version {
            OcppVersion::V16J => {
                let response = self.call(self.boot_notification_request()).await?;
                Ok(BootOutcome {
                    status: response.status,
                    interval: response.interval,
                    current_time: response.current_time,
                })
            }
            OcppVersion::V201 => {
                let response = self
                    .call(self.config.v201_boot_notification_request())
                    .await?;
                // The 2.0.1 response carries `currentTime` as an RFC 3339 string
                // (the wire form); the schema validated it as `date-time`, so a
                // conformant CSMS always parses. Fall back to "now" on the
                // (schema-rejected) off-nominal case rather than failing an
                // otherwise-`Accepted` boot on a cosmetic timestamp field.
                let current_time = chrono::DateTime::parse_from_rfc3339(&response.current_time)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| {
                        warn!(
                            "v201 BootNotificationResponse.currentTime not RFC 3339 ({:?}); \
                             using local time for the accepted event",
                            response.current_time
                        );
                        chrono::Utc::now()
                    });
                Ok(BootOutcome {
                    status: v201_registration_status_to_canonical(response.status),
                    interval: response.interval,
                    current_time,
                })
            }
        }
    }

    async fn boot_sequence(&self) -> OcppResult<()> {
        let max_attempts = self.config.max_boot_retries + 1;
        for attempt in 1..=max_attempts {
            let response = self.send_boot_notification().await?;
            *self.registration_status.write().await = response.status;

            match response.status {
                RegistrationStatus::Accepted | RegistrationStatus::Pending => {
                    if response.status == RegistrationStatus::Accepted {
                        let _ =
                            self.event_sender
                                .send(ChargePointEvent::BootNotificationAccepted {
                                    current_time: response.current_time,
                                    interval: response.interval,
                                });
                        // Announce every connector as Available once accepted, so
                        // the CSMS has an accurate connector inventory from the
                        // start (OCPP 1.6J spec §4.8).
                        self.announce_connectors_available().await?;
                    }
                    // Use the CSMS-supplied interval, not the static config value.
                    self.start_heartbeat(response.interval.max(1) as u64).await;
                    return Ok(());
                }
                RegistrationStatus::Rejected => {
                    let wait_secs = response.interval.max(1) as u64;
                    if attempt < max_attempts {
                        warn!(
                            "BootNotification rejected (attempt {}/{}), retrying in {}s",
                            attempt, max_attempts, wait_secs
                        );
                        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                    } else {
                        error!(
                            "BootNotification rejected after {} attempt(s), giving up",
                            attempt
                        );
                        return Err(OcppError::BootRejected { attempts: attempt });
                    }
                }
            }
        }
        unreachable!()
    }

    /// Send `StatusNotification(Available, NoError)` for connector `0` (the
    /// charge point as a whole) and for each configured connector.
    ///
    /// Called once after the BootNotification is accepted so the CSMS knows the
    /// initial operational state of every connector. Mirrors the boot-time
    /// status reporting in `examples/v16/charge_point.py`.
    async fn announce_connectors_available(&self) -> OcppResult<()> {
        // 1.6J: connector 0 represents the charge point itself and 1..=count are
        // the physical connectors (§4.8). 2.0.1 has no whole-station connector-0
        // slot — `StatusNotification` is reported per physical connector — so
        // the 2.0.1 path announces 1..=count only.
        let first_connector = match self.config.protocol_version {
            OcppVersion::V16J => 0,
            OcppVersion::V201 => 1,
        };
        for connector_id in first_connector..=self.config.connector_count {
            self.send_status_notification(
                connector_id,
                ChargePointStatus::Available,
                ChargePointErrorCode::NoError,
            )
            .await?;
        }
        Ok(())
    }

    /// Start the heartbeat background task using the CSMS-supplied interval.
    ///
    /// The interval comes from `BootNotificationResponse.interval` — not from
    /// `ChargePointConfig.heartbeat_interval` — matching the Python reference
    /// behaviour in `charge_point.py`.
    ///
    /// The `Heartbeat` CALL is version-agnostic on the wire: both the 1.6J and
    /// 2.0.1 requests carry an empty payload, serializing to the identical
    /// `[2, "<id>", "Heartbeat", {}]` frame. So this task is correct for a
    /// 2.0.1 session as-is and needs no `protocol_version` branch.
    async fn start_heartbeat(&self, interval_secs: u64) {
        let interval = Duration::from_secs(interval_secs.max(1));
        let client = self.client.clone();
        let is_connected = self.is_connected.clone();

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;

                if !*is_connected.read().await {
                    continue;
                }

                let request = HeartbeatRequest {};
                let message = match ocpp_messages::CallMessage::new(
                    HeartbeatRequest::ACTION_NAME.to_string(),
                    request,
                ) {
                    Ok(call) => Message::Call(call),
                    Err(e) => {
                        error!("Failed to create heartbeat message: {}", e);
                        continue;
                    }
                };

                if let Some(client) = client.read().await.as_ref() {
                    if let Err(e) = client.send_message(message).await {
                        error!("Failed to send heartbeat: {}", e);
                    }
                }
            }
        });

        *self.heartbeat_handle.write().await = Some(handle);
    }

    /// Start the periodic metering sampler for an active transaction, in the
    /// wire shape the negotiated protocol version uses.
    ///
    /// `V16J` emits the 1.6J `MeterValues` frame; `V201` emits
    /// `TransactionEvent(Updated)` (slice 3b). Both spawn a fire-and-forget
    /// background task and register its handle the same way, so
    /// [`stop_meter_sampler`](Self::stop_meter_sampler) cancels either uniformly.
    async fn start_meter_sampler(&self, connector_id: ConnectorId, transaction_id: i32) {
        match self.config.protocol_version {
            OcppVersion::V16J => {
                self.start_v16j_meter_sampler(connector_id, transaction_id)
                    .await
            }
            OcppVersion::V201 => {
                self.start_v201_meter_sampler(connector_id, transaction_id)
                    .await
            }
        }
    }

    /// Register a freshly spawned meter-sampler task under `transaction_id`,
    /// aborting any stale task previously registered for that id so a sampler can
    /// never outlive its transaction (defensive against a reused id).
    async fn register_meter_sampler(
        &self,
        transaction_id: i32,
        handle: tokio::task::JoinHandle<()>,
    ) {
        if let Some(previous) = self
            .meter_sampler_handles
            .write()
            .await
            .insert(transaction_id, handle)
        {
            previous.abort();
        }
    }

    /// Start the periodic 1.6J `MeterValues` sampler for an active transaction.
    ///
    /// Ports the periodic background-task pattern from the Python reference
    /// (`ocpp/charge_point.py`), which spawns meter sampling alongside the
    /// heartbeat. The task fires an immediate `Transaction.Begin` snapshot and
    /// then a `Sample.Periodic` frame every `config.meter_values_interval`
    /// seconds, reading the connector's latest meter value each tick.
    ///
    /// Frames are sent fire-and-forget via `send_message` (like the heartbeat):
    /// a periodic emitter must not block on the empty `MeterValuesResponse`, and
    /// a send failure is logged without aborting the transaction.
    async fn start_v16j_meter_sampler(&self, connector_id: ConnectorId, transaction_id: i32) {
        let interval = Duration::from_secs(self.config.meter_values_interval.max(1));
        let measurands = self.config.meter_value_measurands.clone();
        let client = self.client.clone();
        let is_connected = self.is_connected.clone();
        let connectors = self.connectors.clone();

        let handle = tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            // The first tick of `interval` fires immediately, so the
            // `Transaction.Begin` snapshot is sent at t=0 and `Sample.Periodic`
            // frames follow at each subsequent interval.
            let mut sent_begin = false;
            loop {
                timer.tick().await;

                // Skip ticks while offline; don't consume the begin snapshot.
                if !*is_connected.read().await {
                    continue;
                }

                let context = if sent_begin {
                    ReadingContext::SamplePeriodic
                } else {
                    ReadingContext::TransactionBegin
                };

                // Read the connector's latest meter value, releasing the lock
                // before building/sending so we never hold it across the send.
                let reading = {
                    let connectors = connectors.read().await;
                    match connectors.get(&connector_id) {
                        Some(connector) => connector.last_meter_reading().await,
                        None => continue,
                    }
                };

                let request = meter_sampler::build_meter_values_request(
                    connector_id,
                    Some(transaction_id),
                    &reading,
                    &measurands,
                    context,
                );
                if !meter_sampler::has_samples(&request) {
                    // Nothing to report (e.g. no supported measurands); the
                    // OCPP schema forbids an empty sampledValue list.
                    continue;
                }

                let message = match ocpp_messages::CallMessage::new(
                    MeterValuesRequest::ACTION_NAME.to_string(),
                    request,
                ) {
                    Ok(call) => Message::Call(call),
                    Err(e) => {
                        error!("Failed to create MeterValues message: {}", e);
                        continue;
                    }
                };

                if let Some(client) = client.read().await.as_ref() {
                    if let Err(e) = client.send_message(message).await {
                        warn!(
                            "Failed to send MeterValues for transaction {}: {}",
                            transaction_id, e
                        );
                    }
                }

                sent_begin = true;
            }
        });

        self.register_meter_sampler(transaction_id, handle).await;
    }

    /// Start the periodic 2.0.1 `TransactionEvent(Updated)` sampler for an active
    /// transaction (slice 3b, Issue #423).
    ///
    /// The 2.0.1 twin of [`start_v16j_meter_sampler`](Self::start_v16j_meter_sampler):
    /// instead of a 1.6J `MeterValues` frame, each tick emits a
    /// `TransactionEvent(Updated)` (`triggerReason = MeterValuePeriodic`) via the
    /// slice-3a builder, carrying a `Sample.Periodic` reading. The `Started`
    /// event already carried the `Transaction.Begin` reading, so — unlike the
    /// 1.6J sampler — this one emits *only* periodic samples and needs no
    /// begin-snapshot bootstrap.
    ///
    /// `seqNo` is drawn from the transaction's shared counter
    /// ([`V201Session::next_seq_no`]), so the values the sampler emits and the
    /// one `stop_transaction` puts on the `Ended` event form a single strictly
    /// increasing sequence. If the session has already been torn down (a stop
    /// racing the tick), the tick is skipped rather than inventing a `seqNo`.
    async fn start_v201_meter_sampler(&self, connector_id: ConnectorId, transaction_id: i32) {
        let interval_secs = self.config.meter_values_interval.max(1);
        let interval = Duration::from_secs(interval_secs);
        let client = self.client.clone();
        let is_connected = self.is_connected.clone();
        let connectors = self.connectors.clone();
        let sessions = self.v201_sessions.clone();
        // The v201 `TxProfile` store lets each tick consult the profile bounding
        // this transaction (slice 7e, Issue #455), so the periodic reading
        // reflects an installed limit rather than ignoring it.
        let tx_profiles = self.v201_tx_profiles.clone();
        // The `TxDefaultProfile` store, consulted as the fallback each tick when
        // no `TxProfile` bounds this transaction (Issue #471).
        let tx_default_profiles = self.v201_tx_default_profiles.clone();
        // The station-ceiling store, capping the resolved limit each tick by any
        // installed `ChargingStationMaxProfile` / `ChargingStationExternalConstraints`
        // (Issue #511).
        let station_ceilings = self.v201_station_ceilings.clone();
        let evse_id = connector_id.value() as i32;
        let txid_str = transaction_id.to_string();

        // The absolute instant the transaction opened, captured as the sampler
        // spawns (the `Started` event has just been sent). It anchors a
        // `TxProfile`'s composite-schedule resolution (slice 7f, Issue #464):
        // `Relative` schedules offset from it, `Absolute`/`Recurring` schedules
        // and `validFrom`/`validTo` windows are evaluated against `tx_start +
        // elapsed`.
        let tx_start = chrono::Utc::now();

        let handle = tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            // Seconds elapsed since the transaction opened, used to resolve which
            // `chargingSchedulePeriod` of a `TxProfile` is in force. Advanced on
            // every tick (even skipped ones) so it tracks wall time, not the
            // count of emitted samples. The first `interval` tick fires
            // immediately, so the first sample resolves at offset 0.
            let mut elapsed_s: i32 = 0;
            loop {
                timer.tick().await;
                let now_elapsed = elapsed_s;
                elapsed_s = elapsed_s.saturating_add(interval_secs as i32);

                if !*is_connected.read().await {
                    continue;
                }

                // Claim the next seqNo for this transaction. A missing session
                // means the transaction is being stopped; drop the tick.
                let seq_no = match sessions.read().await.get(&transaction_id) {
                    Some(session) => session.next_seq_no.fetch_add(1, Ordering::SeqCst),
                    None => continue,
                };

                // Read the connector's latest meter value plus its unbounded
                // rate (for profile enforcement), releasing the lock before
                // building/sending so we never hold it across the send.
                let (reading, natural_power_w, nominal_voltage_v) = {
                    let connectors = connectors.read().await;
                    match connectors.get(&connector_id) {
                        Some(connector) => {
                            let cfg = connector.config();
                            (
                                connector.last_meter_reading().await,
                                cfg.max_power,
                                cfg.max_voltage,
                            )
                        }
                        None => continue,
                    }
                };

                // The profile bounding this reading, by the Issue #471 precedence:
                // the `TxProfile` in force on the EVSE, else its `TxDefaultProfile`
                // fallback (specific-EVSE default, else the `evseId = 0` station-wide
                // default). That resolved limit is then **capped** by any station
                // ceiling in force — `min(resolved, ChargingStationMaxProfile,
                // ChargingStationExternalConstraints)` (Issue #511). A ceiling binds
                // even with no `TxProfile`/`TxDefaultProfile` present. If the composed
                // limit is tighter than the connector's natural rate, surface the
                // bounded power on the reading; otherwise it is unchanged.
                let effective_profile = match tx_profiles.get(evse_id).await {
                    Some(profile) => Some(profile),
                    None => tx_default_profiles.effective_for(evse_id).await,
                };
                let max_ceiling = station_ceilings
                    .effective_for(CeilingKind::Max, evse_id)
                    .await;
                let external_ceiling = station_ceilings
                    .effective_for(CeilingKind::External, evse_id)
                    .await;
                let ceilings: Vec<&ChargingProfileType> =
                    [max_ceiling.as_ref(), external_ceiling.as_ref()]
                        .into_iter()
                        .flatten()
                        .collect();
                let bounded_power_w = crate::v201_charging_profiles::bounded_power_w_capped(
                    effective_profile.as_ref(),
                    &ceilings,
                    now_elapsed,
                    tx_start,
                    natural_power_w,
                    nominal_voltage_v,
                );

                let session = v201_transaction::SessionRef {
                    transaction_id: &txid_str,
                    evse_id,
                    connector_id: 1,
                };
                let request = v201_transaction::transaction_event_updated(
                    &session,
                    seq_no,
                    reading.energy_wh,
                    bounded_power_w,
                    &reading.timestamp.to_rfc3339(),
                );

                let message = match ocpp_messages::CallMessage::new(
                    ocpp_messages::v201::TransactionEventRequest::ACTION_NAME.to_string(),
                    request,
                ) {
                    Ok(call) => Message::Call(call),
                    Err(e) => {
                        error!("Failed to create TransactionEvent(Updated) message: {}", e);
                        continue;
                    }
                };

                if let Some(client) = client.read().await.as_ref() {
                    if let Err(e) = client.send_message(message).await {
                        warn!(
                            "Failed to send TransactionEvent(Updated) for transaction {}: {}",
                            transaction_id, e
                        );
                    }
                }
            }
        });

        self.register_meter_sampler(transaction_id, handle).await;
    }

    /// Stop and remove the periodic `MeterValues` sampler for a transaction.
    ///
    /// Idempotent: a no-op if no sampler is running for `transaction_id`.
    async fn stop_meter_sampler(&self, transaction_id: i32) {
        if let Some(handle) = self
            .meter_sampler_handles
            .write()
            .await
            .remove(&transaction_id)
        {
            handle.abort();
        }
    }

    /// Get connector by ID
    pub async fn get_connector(&self, connector_id: ConnectorId) -> Option<Connector> {
        self.connectors.read().await.get(&connector_id).cloned()
    }

    /// Get all connectors
    pub async fn get_connectors(&self) -> HashMap<ConnectorId, Connector> {
        self.connectors.read().await.clone()
    }

    /// The store of Smart Charging profiles installed by `SetChargingProfile`
    /// (OCPP 1.6J §5.16). The default `SetChargingProfile` / `ClearChargingProfile`
    /// handlers mutate it; `GetCompositeSchedule` (Issue #95) and tests read it to
    /// inspect the profiles currently in effect on a connector.
    pub fn charging_profiles(&self) -> &Arc<ChargingProfileStore> {
        &self.charging_profiles
    }

    /// The CSMS-managed Local Authorization List (OCPP 1.6J §5.x, Issue #93).
    /// The default `GetLocalListVersion` / `SendLocalList` handlers query and
    /// mutate it; tests (and a future offline-authorization path) read it to
    /// inspect the entries the CSMS has pushed.
    pub fn local_list(&self) -> &Arc<LocalAuthList> {
        &self.local_list
    }

    /// Plug in connector
    pub async fn plug_in(&self, connector_id: ConnectorId) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.plug_in().await?;
        }
        Ok(())
    }

    /// Plug out connector
    pub async fn plug_out(&self, connector_id: ConnectorId) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.plug_out().await?;
        }
        Ok(())
    }

    /// Register a handler for inbound `DataTransfer` requests carrying a given
    /// `(vendorId, messageId)` (OCPP 1.6J §6.x, Issue #101).
    ///
    /// `message_id = Some(id)` handles requests with that exact `messageId`;
    /// `message_id = None` handles requests from this vendor that carry **no**
    /// `messageId`. The handler receives the full [`DataTransferRequest`] and
    /// returns the [`DataTransferResponse`] (status plus optional `data`), so it
    /// may answer `Accepted` or `Rejected` and echo data as the vendor protocol
    /// requires.
    ///
    /// Anything left unregistered resolves to the spec-faithful `UnknownVendorId`
    /// (unknown vendor) or `UnknownMessageId` (known vendor, unknown message);
    /// with no handlers registered at all, every `DataTransfer` is answered
    /// `UnknownVendorId`. May be called before or after [`connect`](Self::connect)
    /// — the registry is shared with the live handler.
    ///
    /// The handler is invoked while a registry read-lock is held, so it must not
    /// call this method re-entrantly.
    pub fn register_data_transfer_handler<F>(
        &self,
        vendor_id: impl Into<String>,
        message_id: Option<String>,
        handler: F,
    ) where
        F: Fn(&DataTransferRequest) -> DataTransferResponse + Send + Sync + 'static,
    {
        self.data_transfer.register(vendor_id, message_id, handler);
    }

    /// Authorize an id tag, consulting the Local Authorization List and the
    /// authorization cache before any CSMS round-trip.
    ///
    /// Ports `_send_authorize()` from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
    /// and layers the OCPP 1.6J offline-authorization precedence (§4.1.3) on top
    /// of the §3.1 Authorization Cache behavior (Issue #23 / #104):
    ///
    /// 1. **Local Authorization List hit** (when
    ///    [`ChargePointConfig::local_auth_list_enabled`]) → return the CSMS-pushed
    ///    `IdTagInfo` without a CALL. The list is **authoritative**: a present,
    ///    non-expired entry is honored *as-is*, including a non-`Accepted` status
    ///    (`Blocked`/`Expired`/`Invalid`), which the CSMS explicitly set. This is
    ///    the key difference from the cache (step 2). An entry whose `expiryDate`
    ///    has passed is **not** honored — it falls through to the cache / CSMS.
    /// 2. **Cache hit (`Accepted`)** → return the cached `IdTagInfo` without a
    ///    CALL. Non-`Accepted` cached results are *not* short-circuited; the CP
    ///    re-checks with the CSMS in case authorization has since been granted.
    ///    The cache is opportunistic (CP-populated), so unlike the list it is not
    ///    treated as authoritative for a rejection.
    /// 3. **Miss / expired / non-accepted** → send `AuthorizeRequest` via
    ///    [`ChargePoint::call`] and cache the fresh result.
    /// 4. **CSMS unreachable (CALL times out)** → if
    ///    [`ChargePointConfig::offline_auth_stale_ok`] is set and a (possibly
    ///    stale) cached entry exists, honor it; otherwise fail safe with
    ///    `AuthorizationStatus::Invalid`.
    ///
    /// The caller is responsible for acting on the returned `status`; this
    /// method does not block the transaction flow by itself.
    pub async fn authorize(&self, id_tag: &str) -> OcppResult<IdTagInfo> {
        // 1. Local Authorization List first: the CSMS-managed list takes
        //    precedence over the cache and the network (§4.1.3). A present,
        //    non-expired entry is authoritative — honored verbatim, even when its
        //    status is a rejection — so a known tag is decided with no round-trip.
        //    An entry past its `expiryDate` is stale and ignored (fall through).
        if self.config.local_auth_list_enabled {
            if let Some(info) = self.local_list.get(id_tag) {
                if !id_tag_info_expired(&info) {
                    return Ok(info);
                }
            }
        }

        // 2. Cache next: a fresh, previously-accepted tag needs no round-trip.
        if let Some(cached) = self.auth_cache.get(id_tag) {
            if cached.status == AuthorizationStatus::Accepted {
                return Ok(cached);
            }
        }

        // 3. Miss (or a cached non-accepted result): ask the CSMS.
        match self
            .call(AuthorizeRequest {
                id_tag: id_tag.to_string(),
            })
            .await
        {
            Ok(response) => {
                self.auth_cache.insert(id_tag, response.id_tag_info.clone());
                Ok(response.id_tag_info)
            }
            // 4. CSMS unreachable: offline authorization decision.
            Err(OcppError::Timeout { .. }) => {
                if self.config.offline_auth_stale_ok {
                    if let Some(stale) = self.auth_cache.get_stale(id_tag) {
                        if stale.status == AuthorizationStatus::Accepted {
                            warn!(
                                id_tag,
                                "CSMS unreachable; honoring stale cached authorization"
                            );
                            return Ok(stale);
                        }
                    }
                }
                warn!(id_tag, "CSMS unreachable; failing authorization safe");
                Ok(IdTagInfo {
                    status: AuthorizationStatus::Invalid,
                    parent_id_tag: None,
                    expiry_date: None,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// The version-specific half of [`start_transaction`](Self::start_transaction):
    /// send the protocol's "transaction opened" CALL and return the
    /// `transactionId` the rest of the flow keys its bookkeeping on.
    ///
    /// * `V16J` sends `StartTransaction` and returns the CSMS-assigned integer
    ///   id, erroring if the CSMS rejects the id tag.
    /// * `V201` mints a station-chosen id (2.0.1 makes the *station* choose it —
    ///   the `TransactionEventResponse` carries none), sends
    ///   `TransactionEvent(Started)` (slice-3a builder), records the
    ///   [`V201Session`] the sampler and the `Ended` event share, and errors if
    ///   the CSMS returns a non-`Accepted` `idTokenInfo`. An empty ack (no
    ///   `idTokenInfo`) is an implicit accept, matching the default 2.0.1 CSMS.
    ///
    /// The `V201` id is minted (and the session recorded) only after the CSMS
    /// accepts, so a rejected start leaves no dangling session state and does not
    /// burn a usable id sequence position for the wire.
    async fn open_transaction(
        &self,
        connector_id: ConnectorId,
        id_tag: &str,
        meter_start: i32,
        remote_start_id: Option<i32>,
        charging_profile: Option<ChargingProfileType>,
        group_id_token: Option<V201IdTokenType>,
    ) -> OcppResult<i32> {
        match self.config.protocol_version {
            OcppVersion::V16J => {
                // 1.6J `StartTransaction` has no `remoteStartId` field; the
                // remote-start correlation on 1.6J is the synchronous
                // `transactionId` returned in `RemoteStartTransaction.conf`.
                // Nor does 1.6J thread a 2.0.1 `chargingProfile` / `groupIdToken`
                // through a start (its `SetChargingProfile` installs separately).
                let _ = remote_start_id;
                let _ = (&charging_profile, &group_id_token);
                let response = self
                    .call(StartTransactionRequest {
                        connector_id: connector_id.value(),
                        id_tag: id_tag.to_string(),
                        meter_start,
                        timestamp: chrono::Utc::now(),
                        reservation_id: None,
                    })
                    .await?;

                if response.id_tag_info.status != AuthorizationStatus::Accepted {
                    return Err(OcppError::Authorization {
                        reason: format!(
                            "StartTransaction rejected: idTagInfo.status = {:?}",
                            response.id_tag_info.status
                        ),
                    });
                }
                Ok(response.transaction_id)
            }
            OcppVersion::V201 => {
                // The station chooses the 2.0.1 transactionId; render the minted
                // integer as its decimal string for the wire (SessionRef).
                let transaction_id = self.next_v201_transaction_id.fetch_add(1, Ordering::SeqCst);
                let txid_str = transaction_id.to_string();
                let session = v201_transaction::SessionRef {
                    transaction_id: &txid_str,
                    // The CP inherits 1.6J's flat connector model, so each
                    // connector maps to a single-connector EVSE — the same
                    // `evseId = connector_id, connectorId = 1` convention the
                    // 2.0.1 StatusNotification path uses.
                    evse_id: connector_id.value() as i32,
                    connector_id: 1,
                };

                let response = self
                    .call(v201_transaction::transaction_event_started(
                        &session,
                        id_tag,
                        meter_start as f64,
                        &v201_now(),
                        remote_start_id,
                    ))
                    .await?;

                // Honor an explicit authorization decision when the CSMS returns
                // one; an empty ack (no idTokenInfo) is an implicit accept.
                if let Some(info) = response.id_token_info {
                    if info.status != AuthorizationStatusEnumType::Accepted {
                        return Err(OcppError::Authorization {
                            reason: format!(
                                "TransactionEvent(Started) rejected: idTokenInfo.status = {:?}",
                                info.status
                            ),
                        });
                    }
                }

                // Record the session before the sampler starts so both share the
                // seqNo counter (Started took 0; the sampler and Ended hand out
                // 1, 2, …) and the authorizing idTag for the Ended event. The
                // optional `groupIdToken` rides on the session's auth context so
                // it travels with the live transaction (slice 7d, Issue #450).
                self.v201_sessions.write().await.insert(
                    transaction_id,
                    V201Session {
                        id_tag: id_tag.to_string(),
                        next_seq_no: Arc::new(AtomicI32::new(1)),
                        authorized: Arc::new(AtomicBool::new(true)),
                        group_id_token,
                    },
                );

                // Install a `TxProfile` (already validated as such by the
                // RequestStartTransaction handler) against this transaction's
                // EVSE, atomically with the session it bounds. A `TxProfile` is
                // transaction-scoped, so `close_transaction` clears it in lockstep
                // when the transaction ends. The install is done only after the
                // CSMS accepted the `Started` above, so a rejected start leaves no
                // dangling profile. (Enforcing the schedule is the follow-up.)
                if let Some(profile) = charging_profile {
                    self.v201_tx_profiles
                        .install(connector_id.value() as i32, profile)
                        .await;
                }
                Ok(transaction_id)
            }
        }
    }

    /// Deauthorize every live 2.0.1 session started by `id_tag`, returning how
    /// many were flipped.
    ///
    /// Models the OCPP 2.0.1 deauthorization event — the driver re-presents
    /// their card, or the app/CSMS revokes authorization — after which the
    /// session is no longer *authorized* but its cable may still be latched. It
    /// is the local-driver simulation counterpart to [`authorize`](Self::authorize)
    /// / [`start_transaction`](Self::start_transaction): a way to drive the
    /// "stopped authorizing" transition the simulator otherwise has no input
    /// for.
    ///
    /// The one behavior this unblocks is `UnlockConnector`: a still-authorized
    /// session refuses the unlock (`OngoingAuthorizedTransaction`); once
    /// deauthorized, an inbound `UnlockConnector` stops the transaction first
    /// (reason `UnlockCommand`) and releases the cable. No-op on the 1.6J path
    /// (which has no `V201Session`).
    pub async fn deauthorize(&self, id_tag: &str) -> usize {
        let sessions = self.v201_sessions.read().await;
        let mut flipped = 0;
        for session in sessions.values() {
            if session.id_tag == id_tag {
                // Release-then-acquire so the unlock handler that later reads
                // this flag sees a consistent view; a plain `store` suffices
                // since the flag only ever moves authorized → deauthorized.
                session.authorized.store(false, Ordering::SeqCst);
                flipped += 1;
            }
        }
        flipped
    }

    /// The 2.0.1 `TxProfile` currently installed on `evse_id` by an accepted
    /// `RequestStartTransaction`, if any (slice 7d, Issue #450).
    ///
    /// The read path making an installed profile observable: an operator (or a
    /// test) can confirm a remote start's `chargingProfile` was actually taken up
    /// against its EVSE, rather than parsed off the wire and dropped. A profile is
    /// present only for the lifetime of the transaction it bounds — installed when
    /// the transaction opens, cleared when it ends — so a `Some` here means a live,
    /// profile-bounded transaction on that EVSE. Always `None` on the 1.6J path
    /// (which has no v201 profile store) and for a transaction that carried no
    /// `chargingProfile`.
    pub async fn installed_tx_profile(&self, evse_id: i32) -> Option<ChargingProfileType> {
        self.v201_tx_profiles.get(evse_id).await
    }

    /// The 2.0.1 `TxDefaultProfile` installed under the exact key `evse_id` by a
    /// `SetChargingProfile`, if any (Issue #471) — the read path making an
    /// installed default observable.
    ///
    /// Returns the default stored under `evse_id` verbatim (key `0` is the
    /// station-wide default). It does **not** apply the `evseId = 0` wildcard
    /// fallback — a query for a specific EVSE with only a station-wide default
    /// returns `None` here; the metering resolver and `GetCompositeSchedule` apply
    /// that fallback themselves. Unlike a `TxProfile`, a default persists across
    /// transactions, so a `Some` here does not imply a live transaction. Always
    /// `None` on the 1.6J path (which has no v201 profile store).
    pub async fn installed_tx_default_profile(&self, evse_id: i32) -> Option<ChargingProfileType> {
        self.v201_tx_default_profiles.get(evse_id).await
    }

    /// The 2.0.1 station ceiling of `kind` installed under the exact key `evse_id`
    /// by a `SetChargingProfile`, if any (Issue #511) — the read path making an
    /// installed `ChargingStationMaxProfile` / `ChargingStationExternalConstraints`
    /// observable.
    ///
    /// Returns the ceiling stored under `(kind, evse_id)` verbatim (key `0` is the
    /// whole-station ceiling). It does **not** apply the `evseId = 0` wildcard
    /// fallback — a query for a specific EVSE with only a whole-station ceiling
    /// returns `None` here; the metering resolver and `GetCompositeSchedule` apply
    /// that fallback themselves. A ceiling persists across transactions, so a
    /// `Some` here does not imply a live transaction. Always `None` on the 1.6J
    /// path (which has no v201 profile store).
    pub async fn installed_station_ceiling(
        &self,
        kind: CeilingKind,
        evse_id: i32,
    ) -> Option<ChargingProfileType> {
        self.v201_station_ceilings.get(kind, evse_id).await
    }

    /// The display message currently installed under `id` by `SetDisplayMessage`,
    /// if any (OCPP 2.0.1 Part 2 E05–E08, Issue #505).
    ///
    /// The read path making an installed display message observable: an operator
    /// (or a test) can confirm a `SetDisplayMessage` the station answered
    /// `Accepted` was actually taken up into the store — and a re-install with the
    /// same `id` replaced rather than duplicated it — rather than parsed off the
    /// wire and dropped. Always `None` on the 1.6J path (which has no v201
    /// display-message store) and for an `id` the station never installed (or one a
    /// later `ClearDisplayMessage` removed). The reader the follow-up
    /// `GetDisplayMessages` / `ClearDisplayMessage` handlers build on.
    pub async fn installed_display_message(&self, id: i32) -> Option<MessageInfoType> {
        self.v201_display_messages.get(id).await
    }

    /// The root/CA certificate currently installed for the trust anchor `use_` by
    /// `InstallCertificate`, if any (OCPP 2.0.1 Part 2, A02 / M03–M05, Issue #518).
    ///
    /// The read path making an installed trust anchor observable: an operator (or
    /// a test) can confirm an `InstallCertificate` the station answered `Accepted`
    /// was actually taken up into the store — and a re-install under the same use
    /// rotated rather than duplicated it — rather than parsed off the wire and
    /// dropped. Returns the PEM as delivered (the simulator does no X.509 parse).
    /// Always `None` on the 1.6J path (which has no v201 trust store) and for a use
    /// the station never installed (or one a later `DeleteCertificate` removed).
    /// The reader the follow-up `GetInstalledCertificateIds` / `DeleteCertificate`
    /// handlers build on.
    pub async fn installed_certificate(
        &self,
        use_: InstallCertificateUseEnumType,
    ) -> Option<String> {
        self.v201_certificates.installed(use_).await
    }

    /// The network connection profile stored in `configuration_slot`, if any
    /// (OCPP 2.0.1 Part 2, provisioning, B09/B10, Issue #528).
    ///
    /// The read path making an accepted `SetNetworkProfile` observable: an
    /// operator (or a test) can confirm a profile the station answered `Accepted`
    /// was actually taken up into the store — and a re-provision of the same slot
    /// rotated rather than duplicated it — rather than parsed off the wire and
    /// dropped. Returns the profile as delivered (the simulator never dials it).
    /// Always `None` on the 1.6J path (which has no v201 network-profile store)
    /// and for a slot the station never provisioned.
    pub async fn network_profile(
        &self,
        configuration_slot: i32,
    ) -> Option<NetworkConnectionProfileType> {
        self.v201_network_profiles.get(configuration_slot).await
    }

    /// The `configurationSlot`s that currently hold a network connection profile,
    /// in ascending order (OCPP 2.0.1, Issue #528).
    ///
    /// The enumerate-side read making the whole network-profile store observable:
    /// a test can confirm which slots a sequence of `SetNetworkProfile`s
    /// populated (and that a same-slot re-provision did not grow the set). Empty
    /// on the 1.6J path and before any profile is provisioned.
    pub async fn configured_network_slots(&self) -> Vec<i32> {
        self.v201_network_profiles.slots().await
    }

    /// The `requestId` of the `GetLog` upload the station is currently serving, if
    /// any (OCPP 2.0.1 Part 2, security profile, Issue #517).
    ///
    /// The read path making the in-flight log upload observable: an operator (or a
    /// test) can confirm a `GetLog` the station answered `Accepted` /
    /// `AcceptedCanceled` was recorded as the request now in flight — so a later
    /// `GetLog` supersedes it rather than starting a second concurrent upload.
    /// Always `None` on the 1.6J path (which has no v201 log-upload tracker) and
    /// whenever the station is idle — including once the async
    /// `LogStatusNotification` stream (#526) settles and compare-and-clears the
    /// slot back to idle.
    pub async fn in_flight_log_upload(&self) -> Option<i32> {
        self.v201_log_uploads.in_flight().await
    }

    /// The `requestId` of the `UpdateFirmware` rollout the station is currently
    /// serving, if any (OCPP 2.0.1 Part 2, firmware management, L01–L03, Issue #532).
    ///
    /// The read path making the in-flight firmware update observable: an operator
    /// (or a test) can confirm an `UpdateFirmware` the station answered `Accepted` /
    /// `AcceptedCanceled` was recorded as the request now in flight — so a later
    /// `UpdateFirmware` supersedes it rather than starting a second concurrent
    /// rollout. Always `None` on the 1.6J path (which keeps the empty-conf
    /// `UpdateFirmware` handler and has no v201 firmware-update tracker), whenever
    /// the station is idle, and after a refused request (`InvalidCertificate`)
    /// which records nothing.
    pub async fn in_flight_firmware_update(&self) -> Option<i32> {
        self.v201_firmware_updates.in_flight().await
    }

    /// Whether the station is currently driving a `PublishFirmware` progress
    /// stream for `request_id` (OCPP 2.0.1 Part 2, firmware management, Issue #540).
    ///
    /// The read path making an in-flight firmware publish observable: an operator
    /// (or a test) can confirm a `PublishFirmware` the station answered `Accepted`
    /// was recorded as streaming — so a retry of the same `requestId` is deduped
    /// rather than starting a second concurrent stream — and that the marker is
    /// cleared once the stream settles. Always `false` on the 1.6J path (which has
    /// no v201 publish tracker), whenever the id is not streaming, and after a
    /// `Rejected` request (which records nothing).
    pub async fn is_publishing_firmware(&self, request_id: i32) -> bool {
        self.v201_publish_firmwares.is_publishing(request_id).await
    }

    /// The latest running total cost the CSMS has pushed for `transaction_id`
    /// via the 2.0.1 `CostUpdated` message, if any (Issue #502).
    ///
    /// The read path making a recorded cost observable: an operator (or a test)
    /// can confirm a `CostUpdated` figure was taken up against its transaction
    /// rather than parsed off the wire and dropped. Keyed by the wire
    /// `transactionId` string (the simulator's live-transaction ids render as
    /// their decimal spelling, so a cost for a live transaction is read back
    /// under the same id the transaction is known by). `None` when the CSMS has
    /// never pushed a cost for that id, and always `None` on the 1.6J path
    /// (which has no `CostUpdated` handler). A `Some` here does not imply the
    /// transaction is still live — a cost is recorded unconditionally — so a
    /// caller wanting "live cost" joins this against the live
    /// `active_transactions` set.
    pub async fn recorded_transaction_cost(&self, transaction_id: &str) -> Option<f64> {
        self.v201_costs.get(transaction_id).await
    }

    /// The `groupIdToken` threaded onto the live 2.0.1 transaction `transaction_id`
    /// at start, if the `RequestStartTransaction` carried one (slice 7d, Issue #450).
    ///
    /// The read path making the group/parent token observable on the session's
    /// auth context. `None` when the transaction is unknown, carried no
    /// `groupIdToken`, or was started locally / on the 1.6J path (no live 2.0.1
    /// session).
    pub async fn transaction_group_id_token(&self, transaction_id: i32) -> Option<V201IdTokenType> {
        self.v201_sessions
            .read()
            .await
            .get(&transaction_id)
            .and_then(|s| s.group_id_token.clone())
    }

    /// Open a charging transaction on `connector_id`, transition it to
    /// `Charging`, and start periodic metering.
    ///
    /// The version-specific "open the transaction" CALL is delegated to
    /// the private `open_transaction` helper: `V16J` sends
    /// `StartTransaction` (CSMS assigns the id); `V201` sends
    /// `TransactionEvent(Started)` (the station mints the id). Everything after —
    /// reservation consumption, the connector state transition, the
    /// (version-aware) `StatusNotification`s, and the meter sampler — is shared.
    ///
    /// Returns the `transactionId` on success, or `OcppError::Authorization` if
    /// the CSMS rejects the id tag.
    ///
    /// Ports `send_start_transaction()` from
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py)
    /// and, on the `V201` path, the `TransactionEvent(Started)` flow from
    /// [`ocpp/v201/call.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py).
    pub async fn start_transaction(
        &self,
        connector_id: ConnectorId,
        id_tag: &str,
        meter_start: i32,
    ) -> OcppResult<i32> {
        // A locally initiated start (e.g. the cable was plugged in) has no
        // remote-start request to correlate; the remote 2.0.1 path threads the
        // `remoteStartId`, `chargingProfile`, and `groupIdToken` via
        // `start_transaction_with_remote_start_id`.
        self.start_transaction_with_remote_start_id(
            connector_id,
            id_tag,
            meter_start,
            None,
            None,
            None,
        )
        .await
    }

    /// [`start_transaction`](Self::start_transaction) plus the optional 2.0.1
    /// `RequestStartTransaction.remoteStartId`, echoed onto the started
    /// transaction's `TransactionEvent(Started)` so a CSMS can correlate its
    /// remote-start request with the session that follows. `remote_start_id` is
    /// `None` on the local-start and 1.6J paths (neither carries a
    /// `remoteStartId`), keeping their behavior byte-for-byte unchanged.
    ///
    /// `charging_profile` (a validated `TxProfile`) and `group_id_token` carry
    /// the 2.0.1 `RequestStartTransaction.chargingProfile` / `groupIdToken`
    /// (slice 7d, Issue #450): the profile is installed against the started
    /// transaction's EVSE and the group token onto its [`V201Session`], both in
    /// `open_transaction` so they land atomically with the session. `None` on the
    /// local-start and 1.6J paths.
    async fn start_transaction_with_remote_start_id(
        &self,
        connector_id: ConnectorId,
        id_tag: &str,
        meter_start: i32,
        remote_start_id: Option<i32>,
        charging_profile: Option<ChargingProfileType>,
        group_id_token: Option<V201IdTokenType>,
    ) -> OcppResult<i32> {
        // Connector is now preparing for a transaction (Available -> Preparing).
        self.send_status_notification(
            connector_id.value(),
            ChargePointStatus::Preparing,
            ChargePointErrorCode::NoError,
        )
        .await?;

        let transaction_id = self
            .open_transaction(
                connector_id,
                id_tag,
                meter_start,
                remote_start_id,
                charging_profile,
                group_id_token,
            )
            .await?;

        // Map transaction ID → connector ID for stop_transaction lookup
        self.active_transactions
            .write()
            .await
            .insert(transaction_id, connector_id);

        // A start on a reserved connector consumes its reservation (OCPP 1.6J
        // §5.14): drop any reservation held on this connector so a later
        // CancelReservation for it is correctly Rejected, and disarm its
        // auto-expiry timer (Issue #85) so it can't later free the now-charging
        // connector.
        let consumed: Vec<i32> = {
            let mut reservations = self.reservations.write().await;
            let ids: Vec<i32> = reservations
                .iter()
                .filter(|(_, cid)| **cid == connector_id)
                .map(|(id, _)| *id)
                .collect();
            reservations.retain(|_, cid| *cid != connector_id);
            ids
        };
        if !consumed.is_empty() {
            let mut timers = self.expiry_timers.write().await;
            for id in consumed {
                if let Some(timer) = timers.remove(&id) {
                    timer.abort();
                }
            }
        }

        // Transition connector to Charging
        {
            let mut connectors = self.connectors.write().await;
            if let Some(connector) = connectors.get_mut(&connector_id) {
                connector
                    .set_status(ChargePointStatus::Charging)
                    .await
                    .map_err(|e| OcppError::Internal {
                        message: e.to_string(),
                    })?;
            }
        }

        // Connector accepted and energising (Preparing -> Charging).
        self.send_status_notification(
            connector_id.value(),
            ChargePointStatus::Charging,
            ChargePointErrorCode::NoError,
        )
        .await?;

        // Begin periodic MeterValues sampling for this transaction.
        self.start_meter_sampler(connector_id, transaction_id).await;

        info!(
            "Transaction {} started on connector {}",
            transaction_id,
            connector_id.value()
        );

        Ok(transaction_id)
    }

    /// The version-specific half of [`stop_transaction`](Self::stop_transaction):
    /// send the protocol's "transaction closed" CALL.
    ///
    /// * `V16J` sends `StopTransaction`.
    /// * `V201` emits `TransactionEvent(Ended)` (slice-3a builder) with the final
    ///   meter reading and the `stoppedReason` / `triggerReason` mapped from
    ///   `reason`. It aborts the periodic sampler *first*, so the `Ended`'s
    ///   `seqNo` is the last value drawn from the transaction's shared counter and
    ///   no in-flight `Updated` can claim a number after it, then removes the
    ///   session record. A missing session (a stop racing an already-torn-down
    ///   transaction) is logged and the `Ended` event skipped rather than
    ///   panicking — the stop path must never panic.
    async fn close_transaction(
        &self,
        connector_id: ConnectorId,
        transaction_id: i32,
        meter_stop: i32,
        reason: Reason,
    ) -> OcppResult<()> {
        match self.config.protocol_version {
            OcppVersion::V16J => {
                self.call(StopTransactionRequest {
                    id_tag: None,
                    meter_stop,
                    timestamp: chrono::Utc::now(),
                    transaction_id,
                    reason: Some(reason),
                    transaction_data: None,
                })
                .await?;
            }
            OcppVersion::V201 => {
                // Silence the periodic sampler before drawing the final seqNo, so
                // no in-flight Updated can claim a number after the Ended event.
                self.stop_meter_sampler(transaction_id).await;

                // A `TxProfile` is transaction-scoped: clear any installed against
                // this EVSE now that its transaction is ending, in lockstep with
                // removing the session (slice 7d, Issue #450). Keyed by EVSE id
                // (= connector value), matching `open_transaction`'s install.
                // Idempotent — a transaction that carried no profile clears nothing.
                self.v201_tx_profiles
                    .clear(connector_id.value() as i32)
                    .await;

                let session = self.v201_sessions.write().await.remove(&transaction_id);
                let session = match session {
                    Some(session) => session,
                    None => {
                        warn!(
                            transaction_id,
                            "V201 stop_transaction: no session state; \
                             skipping TransactionEvent(Ended)"
                        );
                        return Ok(());
                    }
                };

                let seq_no = session.next_seq_no.fetch_add(1, Ordering::SeqCst);
                let txid_str = transaction_id.to_string();
                let event = v201_transaction::transaction_event_ended(
                    &v201_transaction::SessionRef {
                        transaction_id: &txid_str,
                        evse_id: connector_id.value() as i32,
                        connector_id: 1,
                    },
                    seq_no,
                    &session.id_tag,
                    meter_stop as f64,
                    reason,
                    &v201_now(),
                );
                self.call(event).await?;
            }
        }
        Ok(())
    }

    /// Send a `StopTransaction` CALL to the CSMS and transition the connector
    /// back to `Available`.
    ///
    /// Returns `OcppError::NotFound` if `transaction_id` does not correspond
    /// to a transaction started via [`ChargePoint::start_transaction`].
    ///
    /// The "transaction closed" CALL is version-specific (see the private
    /// `close_transaction` helper): `V16J` sends
    /// `StopTransaction`; `V201` emits `TransactionEvent(Ended)`. The connector
    /// state transition and its (version-aware) `StatusNotification`s are shared.
    ///
    /// Ports `send_stop_transaction()` from
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py)
    /// and, on the `V201` path, the `TransactionEvent(Ended)` flow from
    /// [`ocpp/v201/call.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py).
    pub async fn stop_transaction(
        &self,
        transaction_id: i32,
        meter_stop: i32,
        reason: Reason,
    ) -> OcppResult<()> {
        let connector_id = self
            .active_transactions
            .read()
            .await
            .get(&transaction_id)
            .copied()
            .ok_or_else(|| OcppError::NotFound {
                resource: format!("active transaction with ID {}", transaction_id),
            })?;

        self.close_transaction(connector_id, transaction_id, meter_stop, reason)
            .await?;

        // Transaction is wrapping up (Charging -> Finishing).
        self.send_status_notification(
            connector_id.value(),
            ChargePointStatus::Finishing,
            ChargePointErrorCode::NoError,
        )
        .await?;

        // Stop periodic MeterValues sampling for this transaction.
        self.stop_meter_sampler(transaction_id).await;

        // Remove mapping now that CSMS has acknowledged the stop
        self.active_transactions
            .write()
            .await
            .remove(&transaction_id);

        // Transition connector back to Available
        {
            let mut connectors = self.connectors.write().await;
            if let Some(connector) = connectors.get_mut(&connector_id) {
                connector
                    .set_status(ChargePointStatus::Available)
                    .await
                    .map_err(|e| OcppError::Internal {
                        message: e.to_string(),
                    })?;
            }
        }

        // Connector is free again (Finishing -> Available).
        self.send_status_notification(
            connector_id.value(),
            ChargePointStatus::Available,
            ChargePointErrorCode::NoError,
        )
        .await?;

        info!(
            "Transaction {} stopped on connector {}",
            transaction_id,
            connector_id.value()
        );

        // Slice 4c (#431): now that the transaction is fully wound down, carry out
        // a deferred 2.0.1 `Reset(OnIdle)` if one is armed and the station has gone
        // idle (no other connector still charging). Done at the tail — after the
        // connector is back to `Available` — so the stop completes cleanly before
        // the reboot regardless of who called us.
        //
        // `take()` fires the armed reset at most once; a plain stop with no armed
        // reset finds `None` and reboots nothing (guards the hot path against a
        // false trigger). A reset that is *itself* driving this stop
        // (`perform_reset` → `stop_transaction`) cleared the slot up front, so it
        // cannot re-enqueue here and double-reboot. Re-enqueueing onto the same
        // command channel keeps the reboot on the single consumer task, off this
        // (and any inbound-CALL) path — never a background poll/wait.
        //
        // The emptiness check and the `take` are done under one held
        // `active_transactions` read guard so a concurrent `start_transaction`
        // cannot slip a fresh session in between them and have the reboot cut it
        // off — preserving the "never interrupt a live session" invariant. Lock
        // order is `active_transactions` → `pending_v201_reset`, matching the V201
        // `Reset` handler (which reads `active_transactions` before it arms the
        // slot); no path nests these the other way, so this cannot deadlock.
        //
        // Slice 6b (#436) drains a deferred `ChangeAvailability` at the same idle
        // boundary and under the same held `active_transactions` guard, so a
        // scheduled availability change lands the moment the last session ends
        // (never mid-charge) — the availability twin of the deferred reset. Both
        // slots are taken under the one guard; lock order stays
        // `active_transactions → pending_v201_reset → pending_v201_availability`,
        // and no path nests these the other way, so this cannot deadlock.
        let (armed_reset, armed_availability) = {
            let active = self.active_transactions.read().await;
            if active.is_empty() {
                (
                    self.pending_v201_reset.write().await.take(),
                    self.pending_v201_availability.write().await.take(),
                )
            } else {
                (None, None)
            }
        };

        // Apply a deferred availability change before a deferred reset: if both are
        // armed the reset supersedes, but announcing the availability first keeps
        // the CSMS's view consistent right up to the reboot. Resolve the targeted
        // connectors (single EVSE, or the whole station) and enqueue one apply per
        // connector onto the command channel, so the work stays on the single
        // consumer task, off this path.
        if let Some(pending) = armed_availability {
            let targets: Vec<ConnectorId> = match pending.evse_id {
                Some(id) => u32::try_from(id)
                    .ok()
                    .and_then(|v| ConnectorId::new(v).ok())
                    .into_iter()
                    .collect(),
                None => {
                    let mut ids: Vec<ConnectorId> =
                        self.connectors.read().await.keys().copied().collect();
                    ids.sort_by_key(ConnectorId::value);
                    ids
                }
            };
            for cid in targets {
                if self
                    .command_sender
                    .send(RemoteCommand::V201ApplyAvailability {
                        connector_id: cid,
                        target: pending.target,
                    })
                    .is_err()
                {
                    warn!("deferred v201 availability: command consumer gone; not applying");
                    break;
                }
            }
        }

        if let Some(reset_type) = armed_reset {
            if self
                .command_sender
                .send(RemoteCommand::Reset { reset_type })
                .is_err()
            {
                warn!("deferred v201 reset: command consumer gone; not rebooting");
            }
        }

        Ok(())
    }

    /// Abort the heartbeat task and every periodic `MeterValues` sampler.
    ///
    /// Shared by [`stop`](Self::stop) and the reset path: both need to silence
    /// the background emitters before the connection is torn down or re-booted,
    /// so the old tasks don't leak or race a fresh boot. Aborting `JoinHandle`s
    /// is required because dropping one merely detaches the task.
    async fn quiesce_background_tasks(&self) {
        if let Some(handle) = self.heartbeat_handle.write().await.take() {
            handle.abort();
        }
        for (_txn_id, handle) in self.meter_sampler_handles.write().await.drain() {
            handle.abort();
        }
    }

    /// Carry out a CSMS-initiated `Reset` (OCPP 1.6J §5.13) as a real simulator
    /// side effect. Invoked only from the command-consumer task (see
    /// [`connect`](Self::connect)), never inline in the inbound-CALL handler, so
    /// the `Reset` CALLRESULT is flushed first and there is no receive-loop
    /// re-entrancy/deadlock.
    ///
    /// - **Soft** — gracefully stop any active transaction(s) with reason
    ///   `SoftReset`, then restart the application on the *existing* connection
    ///   by re-running the boot handshake (fresh `BootNotification`, connector
    ///   status re-announced, heartbeat restarted).
    /// - **Hard** — gracefully stop any active transaction(s) with reason
    ///   `HardReset`, then perform a full reboot: tear the session down and
    ///   reconnect from scratch, re-announcing connector status on the new boot.
    ///
    /// Mirrors the `@on('Reset')` / `@after('Reset')` split in the Python
    /// reference's [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py):
    /// the handler returns the status, the after-effect performs the reset.
    async fn perform_reset(&self, reset_type: ResetType) {
        let _ = self
            .event_sender
            .send(ChargePointEvent::ResetRequested { reset_type });

        // Slice 4c (#431): this reset is happening now, so it supersedes any
        // deferred `Reset(OnIdle)` still armed — disarm it up front. This is also
        // what makes the deferred-reset fire in `stop_transaction` single-shot:
        // the `stop_transaction` calls below (ending in-flight sessions for this
        // reset) then find an empty slot and cannot re-enqueue a second reboot.
        let _ = self.pending_v201_reset.write().await.take();

        // Gracefully end any in-flight transactions with the reset-specific
        // reason. Snapshot the ids first so we don't hold the map lock across
        // the StopTransaction round-trips.
        let reason = match reset_type {
            ResetType::Soft => Reason::SoftReset,
            ResetType::Hard => Reason::HardReset,
        };
        let active_txns: Vec<i32> = self
            .active_transactions
            .read()
            .await
            .keys()
            .copied()
            .collect();
        for txn_id in active_txns {
            // meter_stop is unknown for a reset-triggered stop; report 0 like the
            // Python reference's example, which does not meter a forced stop.
            if let Err(e) = self.stop_transaction(txn_id, 0, reason).await {
                warn!("reset: failed to stop transaction {txn_id}: {e}");
            }
        }

        // Silence heartbeat + samplers before re-booting so the old tasks don't
        // leak or race the fresh boot (Soft re-boots in place; Hard reconnects,
        // and `disconnect` does not itself abort these).
        self.quiesce_background_tasks().await;

        match reset_type {
            ResetType::Soft => {
                info!("Soft reset: restarting on the existing connection");
                if let Err(e) = self.boot_sequence().await {
                    error!("soft reset: boot sequence failed: {e}");
                }
            }
            ResetType::Hard => {
                info!("Hard reset: tearing down session and reconnecting");
                if let Err(e) = self.disconnect().await {
                    warn!("hard reset: disconnect failed: {e}");
                }
                // Re-establish the session directly (not via `connect`) so we do
                // not re-take the command receiver or recurse.
                if let Err(e) = self.establish_session().await {
                    error!("hard reset: reconnect failed: {e}");
                }
            }
        }
    }

    /// Set connector fault
    pub async fn set_fault(
        &self,
        connector_id: ConnectorId,
        error_code: ocpp_types::v16j::ChargePointErrorCode,
        info: Option<String>,
    ) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.set_fault(error_code, info).await?;
        }
        Ok(())
    }

    /// Clear connector fault
    pub async fn clear_fault(&self, connector_id: ConnectorId) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.clear_fault().await?;
        }
        Ok(())
    }

    /// Set connector availability
    pub async fn set_availability(&self, connector_id: ConnectorId, available: bool) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            if available {
                connector.set_status(ChargePointStatus::Available).await?;
            } else {
                connector.set_status(ChargePointStatus::Unavailable).await?;
            }
        }
        Ok(())
    }

    /// Get event receiver
    pub async fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<ChargePointEvent>> {
        self.event_receiver.write().await.take()
    }

    /// Handle boot notification response (called externally after receiving a CALLRESULT for
    /// BootNotification via `call()`; will be fully integrated in Issue #20).
    pub async fn handle_boot_notification_response(
        &self,
        response: BootNotificationResponse,
    ) -> Result<()> {
        info!("Boot notification response: {:?}", response.status);

        *self.registration_status.write().await = response.status;

        match response.status {
            RegistrationStatus::Accepted => {
                let _ = self
                    .event_sender
                    .send(ChargePointEvent::BootNotificationAccepted {
                        current_time: response.current_time,
                        interval: response.interval,
                    });

                // Start heartbeat with the CSMS-supplied interval.
                self.start_heartbeat(response.interval.max(1) as u64).await;
            }
            RegistrationStatus::Pending => {
                warn!("Boot notification pending, will retry");
            }
            RegistrationStatus::Rejected => {
                error!("Boot notification rejected");
                return Err(anyhow::anyhow!("Boot notification rejected"));
            }
        }

        Ok(())
    }

    /// Send a `StatusNotification` CALL to the CSMS for a single connector and
    /// await the (empty) CALLRESULT.
    ///
    /// `connector_id` is a raw `u32` rather than a [`ConnectorId`] because OCPP
    /// 1.6J reserves connector `0` for the charge point as a whole (used for the
    /// boot-time availability announcement), and `ConnectorId` rejects `0`.
    ///
    /// Ports the `StatusNotification` call from
    /// [`ocpp/v16/call.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/call.py)
    /// (OCPP 1.6J spec §3.14). The timestamp is set to `Utc::now()`.
    pub async fn send_status_notification(
        &self,
        connector_id: u32,
        status: ChargePointStatus,
        error_code: ChargePointErrorCode,
    ) -> OcppResult<()> {
        match self.config.protocol_version {
            OcppVersion::V16J => {
                self.call(StatusNotificationRequest {
                    connector_id,
                    error_code,
                    info: None,
                    status,
                    timestamp: Some(chrono::Utc::now()),
                    vendor_error_code: None,
                    vendor_id: None,
                })
                .await?;
            }
            OcppVersion::V201 => {
                // 2.0.1 reports status per `(evseId, connectorId)` pair and has
                // no `errorCode` field. The CP inherits 1.6J's flat connector
                // model, so each connector maps to a single-connector EVSE:
                // `evseId = connector_id`, `connectorId = 1`. (A richer EVSE
                // topology is out of scope for this wire-shape slice.)
                self.call(V201StatusNotificationRequest {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    connector_status: charge_point_status_to_v201(status),
                    evse_id: connector_id as i32,
                    connector_id: 1,
                    custom_data: None,
                })
                .await?;
            }
        }

        Ok(())
    }

    /// Carry out a 2.0.1 `ChangeAvailability` transition on one connector as a
    /// real simulator side effect (slice 6b, Issue #436): flip the connector's
    /// operative state — the connector's own [`ChargePointStatus`] *is* its
    /// operative state, so `Operative → Available` / `Inoperative → Unavailable`
    /// via [`operational_status_to_cp_status`] — and emit the reflecting
    /// version-aware `StatusNotification` so the CSMS observes the new
    /// availability.
    ///
    /// Runs only on the command-consumer task (see [`connect`](Self::connect)),
    /// never inline in the inbound-CALL handler, so the `ChangeAvailability`
    /// CALLRESULT is flushed before this outbound `StatusNotification` and there is
    /// no receive-loop re-entrancy. Invoked for an `Accepted` change immediately
    /// and for a `Scheduled` change from [`stop_transaction`](Self::stop_transaction)
    /// once the station goes idle. A connector that vanished between the decision
    /// and here is logged and skipped (no panic); this only ever runs on the
    /// idle/accepted boundary, so it does not override a live charging state.
    async fn apply_v201_availability(
        &self,
        connector_id: ConnectorId,
        target: OperationalStatusEnumType,
    ) {
        let new_status = operational_status_to_cp_status(target);

        let connector = self.connectors.read().await.get(&connector_id).cloned();
        let Some(connector) = connector else {
            warn!(
                "v201 ChangeAvailability: unknown connector {}; not applying {target:?}",
                connector_id.value()
            );
            return;
        };
        if let Err(e) = connector.set_status(new_status).await {
            warn!(
                "v201 ChangeAvailability: failed to set connector {} to {new_status:?}: {e}",
                connector_id.value()
            );
            return;
        }

        if let Err(e) = self
            .send_status_notification(
                connector_id.value(),
                new_status,
                ChargePointErrorCode::NoError,
            )
            .await
        {
            warn!(
                "v201 ChangeAvailability: StatusNotification({new_status:?}) for connector \
                 {} failed: {e}",
                connector_id.value()
            );
        }
    }

    /// Send the message a CSMS asked for via `TriggerMessage` (OCPP 1.6J §4.x).
    ///
    /// Runs on the command-consumer task (off the inbound-CALL path), so these
    /// outbound CALLs never re-enter the receive loop. Only the variants
    /// [`trigger_message_supported`] accepts are queued, but the match here is
    /// exhaustive over `MessageTrigger`, so a new variant fails to compile until
    /// it is handled — the gate and this match cannot silently drift.
    async fn send_triggered_message(&self, message: MessageTrigger, connector_id: Option<i32>) {
        match message {
            MessageTrigger::BootNotification => {
                if let Err(e) = self.call(self.boot_notification_request()).await {
                    warn!("TriggerMessage(BootNotification): send failed: {e}");
                }
            }
            MessageTrigger::Heartbeat => {
                if let Err(e) = self.call(HeartbeatRequest {}).await {
                    warn!("TriggerMessage(Heartbeat): send failed: {e}");
                }
            }
            MessageTrigger::StatusNotification => {
                self.trigger_status_notification(connector_id).await;
            }
            MessageTrigger::MeterValues => {
                self.trigger_meter_values(connector_id).await;
            }
            MessageTrigger::DiagnosticsStatusNotification => {
                // Report the CP's *current* diagnostics status on demand without
                // disturbing any in-flight upload (Idle when no GetDiagnostics
                // has run). Closes the deferred half of Issue #65.
                let status = *self.diagnostics_status.read().await;
                self.send_diagnostics_status_notification(status).await;
            }
            MessageTrigger::FirmwareStatusNotification => {
                // Report the CP's *current* firmware status on demand without
                // disturbing any in-flight update (Idle when no UpdateFirmware
                // has run). Closes the deferred half of Issue #65.
                let status = *self.firmware_status.read().await;
                self.send_firmware_status_notification(status).await;
            }
            other @ (MessageTrigger::LogStatusNotification
            | MessageTrigger::SignChargePointCertificate) => {
                // ExtendedTriggerMessage-only Security-extension triggers. The
                // simulator has no LogStatusNotification / SignCertificate state
                // machine, so `trigger_message_supported` reports them
                // unsupported and they never reach here in normal flow; this arm
                // keeps the match exhaustive and aligned with the gate.
                warn!("TriggerMessage({other:?}): not implemented by the simulator");
            }
        }
    }

    /// Send the message a CSMS asked for via an OCPP 2.0.1 `TriggerMessage`
    /// (`ocpp.v201.call.TriggerMessage`).
    ///
    /// The 2.0.1 twin of [`send_triggered_message`](Self::send_triggered_message).
    /// Runs on the command-consumer task (off the inbound-CALL path), so these
    /// outbound CALLs never re-enter the receive loop. Only the variants
    /// [`v201_command::v201_trigger_message_status`] classifies `Accepted` are
    /// ever queued, but the match here is exhaustive over
    /// [`MessageTriggerEnumType`], so a new variant fails to compile until it is
    /// handled — the policy gate and this dispatch cannot silently drift.
    ///
    /// `evse_id` scopes the EVSE-specific messages; `None` targets the whole
    /// Charging Station.
    async fn send_v201_triggered_message(
        &self,
        message: MessageTriggerEnumType,
        evse_id: Option<i32>,
    ) {
        use MessageTriggerEnumType::{
            BootNotification, FirmwareStatusNotification, Heartbeat, LogStatusNotification,
            MeterValues, PublishFirmwareStatusNotification, SignChargingStationCertificate,
            SignCombinedCertificate, SignV2GCertificate, StatusNotification, TransactionEvent,
        };
        match message {
            BootNotification => {
                if let Err(e) = self
                    .call(self.config.v201_boot_notification_request())
                    .await
                {
                    warn!("v201 TriggerMessage(BootNotification): send failed: {e}");
                }
            }
            Heartbeat => {
                // 2.0.1 Heartbeat carries an empty payload, serializing to the
                // same `{}` frame as 1.6J — the request type is version-agnostic.
                if let Err(e) = self.call(HeartbeatRequest {}).await {
                    warn!("v201 TriggerMessage(Heartbeat): send failed: {e}");
                }
            }
            StatusNotification => self.trigger_v201_status_notification(evse_id).await,
            MeterValues => self.trigger_v201_meter_values(evse_id).await,
            TransactionEvent => self.trigger_v201_transaction_event(evse_id).await,
            // Firmware-, diagnostics-log-, and certificate-signing triggers the
            // simulator does not implement. `v201_trigger_message_status` reports
            // these `NotImplemented`, so the handler never enqueues them; this arm
            // keeps the match exhaustive and aligned with that policy.
            other @ (LogStatusNotification
            | FirmwareStatusNotification
            | SignChargingStationCertificate
            | SignV2GCertificate
            | SignCombinedCertificate
            | PublishFirmwareStatusNotification) => {
                warn!("v201 TriggerMessage({other:?}): not implemented by the simulator");
            }
        }
    }

    /// Stream the device-model report an `Accepted` OCPP 2.0.1 `GetBaseReport`
    /// asked for, as a `NotifyReport` CALL (`ocpp.v201.call.NotifyReport`).
    ///
    /// Runs on the command-consumer task (off the inbound-CALL path), so the
    /// outbound `NotifyReport` is sent only after the `GetBaseReport` CALLRESULT
    /// is flushed and never re-enters the receive loop. `request_id` correlates
    /// the report back to the triggering `GetBaseReport`; `report_data` is the
    /// snapshot the handler already computed.
    ///
    /// A single page carries the whole report (`seqNo` 0, `tbc` omitted =
    /// `false`) — correct for the simulator's small seeded device model.
    /// Multi-page chunking (`tbc` paging) is a later slice once the inventory
    /// grows beyond one frame.
    async fn send_v201_notify_report(&self, request_id: i32, report_data: Vec<ReportDataType>) {
        let request = V201NotifyReportRequest {
            request_id,
            generated_at: v201_now(),
            report_data: Some(report_data),
            tbc: None,
            seq_no: 0,
            custom_data: None,
        };
        if let Err(e) = self.call(request).await {
            warn!("v201 GetBaseReport: NotifyReport send failed for request {request_id}: {e}");
        }
    }

    /// Stream the variable-monitoring snapshot an `Accepted` OCPP 2.0.1
    /// `GetMonitoringReport` asked for, as a `NotifyMonitoringReport` CALL
    /// (`ocpp.v201.call.NotifyMonitoringReport`).
    ///
    /// The monitoring twin of [`send_v201_notify_report`](Self::send_v201_notify_report):
    /// runs on the command-consumer task (off the inbound-CALL path), so the
    /// outbound `NotifyMonitoringReport` is sent only after the
    /// `GetMonitoringReport` CALLRESULT is flushed and never re-enters the receive
    /// loop. `request_id` correlates the report back to the triggering
    /// `GetMonitoringReport`; `monitor_data` is the snapshot the handler already
    /// computed.
    ///
    /// A single page carries the whole snapshot (`seqNo` 0, `tbc` omitted =
    /// `false`) — correct for the simulator's small monitor set. Multi-page
    /// chunking (`tbc` paging) is a later slice once the monitor set grows beyond
    /// one frame.
    async fn send_v201_notify_monitoring_report(
        &self,
        request_id: i32,
        monitor_data: Vec<MonitoringDataType>,
    ) {
        let request = V201NotifyMonitoringReportRequest {
            monitor: Some(monitor_data),
            request_id,
            tbc: None,
            seq_no: 0,
            generated_at: v201_now(),
            custom_data: None,
        };
        if let Err(e) = self.call(request).await {
            warn!(
                "v201 GetMonitoringReport: NotifyMonitoringReport send failed for request {request_id}: {e}"
            );
        }
    }

    /// Stream the installed charging profiles an `Accepted` OCPP 2.0.1
    /// `GetChargingProfiles` asked for, as one or more `ReportChargingProfiles`
    /// CALLs (`ocpp.v201.call.ReportChargingProfiles`).
    ///
    /// Runs on the command-consumer task (off the inbound-CALL path), so the
    /// outbound CALLs are sent only after the `GetChargingProfiles` CALLRESULT is
    /// flushed and never re-enter the receive loop. `request_id` correlates the
    /// report back to the triggering query; `profiles` is the `(evse_id, profile)`
    /// match set the handler already resolved. The pure
    /// [`v201_report_charging_profiles_pages`](crate::v201_command::v201_report_charging_profiles_pages)
    /// builder pages it into one CSO-sourced CALL per EVSE (ascending `evseId`,
    /// `tbc` set on every page but the last); each is sent in order so the CSMS
    /// sees the paging flags in sequence. An empty match set builds no pages and
    /// sends nothing (the handler only queues this for a non-empty match).
    async fn send_v201_report_charging_profiles(
        &self,
        request_id: i32,
        profiles: Vec<(i32, ChargingProfileType)>,
    ) {
        let pages = v201_command::v201_report_charging_profiles_pages(request_id, &profiles);
        for page in pages {
            let evse_id = page.evse_id;
            if let Err(e) = self.call(page).await {
                warn!(
                    "v201 GetChargingProfiles: ReportChargingProfiles send failed for \
                     request {request_id} (evse {evse_id}): {e}"
                );
            }
        }
    }

    /// Stream the installed display messages an `Accepted` OCPP 2.0.1
    /// `GetDisplayMessages` asked for, as one or more `NotifyDisplayMessages`
    /// CALLs (`ocpp.v201.call.NotifyDisplayMessages`).
    ///
    /// Runs on the command-consumer task (off the inbound-CALL path), so the
    /// outbound CALLs are sent only after the `GetDisplayMessages` CALLRESULT is
    /// flushed and never re-enter the receive loop. `request_id` correlates the
    /// report back to the triggering query; `messages` is the match set the
    /// handler already resolved. The pure
    /// [`v201_notify_display_messages_pages`](crate::v201_command::v201_notify_display_messages_pages)
    /// builder pages it into one CALL per message (ascending `id`, `tbc` set on
    /// every page but the last); each is sent in order so the CSMS sees the
    /// paging flags in sequence. An empty match set builds no pages and sends
    /// nothing (the handler only queues this for a non-empty match).
    async fn send_v201_notify_display_messages(
        &self,
        request_id: i32,
        messages: Vec<MessageInfoType>,
    ) {
        let pages = v201_command::v201_notify_display_messages_pages(request_id, &messages);
        for page in pages {
            if let Err(e) = self.call(page).await {
                warn!(
                    "v201 GetDisplayMessages: NotifyDisplayMessages send failed for \
                     request {request_id}: {e}"
                );
            }
        }
    }

    /// Run the simulated async log-upload state machine for an `Accepted` /
    /// `AcceptedCanceled` OCPP 2.0.1 `GetLog` (Part 2, security profile, Issue
    /// #526).
    ///
    /// Invoked only from the command-consumer task (see [`connect`](Self::connect)),
    /// never inline in the inbound-CALL handler, so the `GetLog` CALLRESULT is
    /// flushed before the first `LogStatusNotification` and there is no
    /// receive-loop re-entrancy/deadlock — the same discipline as
    /// [`run_diagnostics_upload`](Self::run_diagnostics_upload), the 1.6J analog.
    ///
    /// The simulator has no real archive to upload, so it models the transfer on
    /// a short timer: report [`Uploading`](UploadLogStatusEnumType::Uploading),
    /// wait [`LOG_UPLOAD_DURATION`], then report a terminal status correlated by
    /// the same `request_id`. The terminal status comes from
    /// [`compare-and-clearing`](crate::v201_log_upload::V201LogUploadStore::complete)
    /// the in-flight slot:
    ///
    /// - still the owner → the store is cleared to idle and the terminal status is
    ///   [`Uploaded`](UploadLogStatusEnumType::Uploaded), or
    ///   [`UploadFailure`](UploadLogStatusEnumType::UploadFailure) under the opt-in
    ///   [`log_upload_should_fail`](ChargePointConfig::log_upload_should_fail)
    ///   fault injection;
    /// - superseded while it slept (a newer `GetLog` took the slot) → the store is
    ///   left to the newer upload and this one reports
    ///   [`AcceptedCanceled`](UploadLogStatusEnumType::AcceptedCanceled).
    ///
    /// Because the consumer runs commands serially, a superseding `GetLog`'s own
    /// `V201LogUpload` is not dequeued until this one returns, so the canceled
    /// upload's terminal notification is always emitted before the superseding
    /// upload's `Uploading` — the CSMS sees the cancel before the new upload
    /// begins. Ports the `LogStatusNotification` progress flow from
    /// [`ocpp/v201/enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/enums.py)'s
    /// `UploadLogStatusEnumType`.
    async fn run_v201_log_upload(&self, request_id: i32) {
        self.send_v201_log_status(v201_command::V201_LOG_UPLOAD_IN_PROGRESS, request_id)
            .await;
        tokio::time::sleep(LOG_UPLOAD_DURATION).await;

        // Compare-and-clear: settle (and return to idle) only if we still own the
        // in-flight slot. `!still_owner` is the supersede signal the terminal
        // decision keys off — a newer GetLog took the slot while we slept.
        let still_owner = self.v201_log_uploads.complete(request_id).await;
        let terminal = v201_command::v201_log_upload_terminal_status(
            !still_owner,
            self.config.log_upload_should_fail,
        );
        self.send_v201_log_status(terminal, request_id).await;
    }

    /// Send a single `LogStatusNotification(status)` CALL correlated by
    /// `request_id` (OCPP 2.0.1 Part 2). A best-effort progress report: a send
    /// failure is logged, not propagated (the upload state machine continues).
    async fn send_v201_log_status(&self, status: UploadLogStatusEnumType, request_id: i32) {
        if let Err(e) = self
            .call(v201_command::v201_log_status_notification(
                status, request_id,
            ))
            .await
        {
            warn!("v201 GetLog: LogStatusNotification({status:?}) for request {request_id}: {e}");
        }
    }

    /// Run the simulated async firmware-update state machine for an `Accepted` /
    /// `AcceptedCanceled` OCPP 2.0.1 `UpdateFirmware` (Part 2, firmware
    /// management, L01–L03, Issue #534).
    ///
    /// Invoked only from the command-consumer task (see [`connect`](Self::connect)),
    /// never inline in the inbound-CALL handler, so the `UpdateFirmware`
    /// CALLRESULT is flushed before the first `FirmwareStatusNotification` and
    /// there is no receive-loop re-entrancy/deadlock — the same discipline as
    /// [`run_v201_log_upload`](Self::run_v201_log_upload) (the log-upload sibling)
    /// and [`run_firmware_update`](Self::run_firmware_update) (the 1.6J analog).
    ///
    /// The simulator has no real image to download or install, so it models the
    /// rollout on a short timer, stepping through the lifecycle and emitting a
    /// `FirmwareStatusNotification.req` correlated by `request_id` at each step.
    /// On the happy path (the default) that is `Downloading` → `Downloaded` →
    /// `Installing` → `Installed`. With
    /// [`ChargePointConfig::firmware_update_outcome`] set to a failure variant
    /// (opt-in fault injection, shared with the 1.6J flow) it instead takes the
    /// matching error branch — `Downloading → DownloadFailed`, or `Downloading →
    /// Downloaded → Installing → InstallationFailed` — so a CSMS can be exercised
    /// against a 2.0.1 rollout that fails, not just the happy path.
    ///
    /// The terminal status is emitted through
    /// [`settle_v201_firmware_update`](Self::settle_v201_firmware_update), which
    /// compare-and-clears the [`V201FirmwareUpdateStore`] via
    /// [`complete`](crate::v201_firmware_update::V201FirmwareUpdateStore::complete):
    /// a rollout that still owns the in-flight slot returns the station to idle and
    /// reports its terminal status; a rollout superseded while it ran (a newer
    /// `UpdateFirmware` took the slot) leaves the newer one in flight and reports
    /// no terminal — `FirmwareStatusEnumType` has no cancel value, so an abandoned
    /// rollout simply goes quiet while the newer one's stream (dequeued next by the
    /// serial consumer) proceeds. Because the consumer runs commands serially, a
    /// superseding `UpdateFirmware`'s own `V201FirmwareUpdate` is not dequeued until
    /// this one returns, so the newer rollout's `Downloading` never interleaves
    /// with this one's progress. Ports the `FirmwareStatusNotification` progress
    /// flow from
    /// [`ocpp/v201/enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/enums.py)'s
    /// `FirmwareStatusEnumType`.
    async fn run_v201_firmware_update(&self, request_id: i32) {
        self.send_v201_firmware_status(FirmwareStatusEnumType::Downloading, request_id)
            .await;
        tokio::time::sleep(FIRMWARE_UPDATE_STEP_DURATION).await;

        if self.config.firmware_update_outcome == FirmwareUpdateOutcome::DownloadFailed {
            // Download phase fails: settle here with DownloadFailed; there is no
            // image to install, so the sequence stops.
            self.settle_v201_firmware_update(FirmwareStatusEnumType::DownloadFailed, request_id)
                .await;
            return;
        }
        self.send_v201_firmware_status(FirmwareStatusEnumType::Downloaded, request_id)
            .await;
        tokio::time::sleep(FIRMWARE_UPDATE_STEP_DURATION).await;

        self.send_v201_firmware_status(FirmwareStatusEnumType::Installing, request_id)
            .await;
        tokio::time::sleep(FIRMWARE_UPDATE_STEP_DURATION).await;

        let terminal =
            if self.config.firmware_update_outcome == FirmwareUpdateOutcome::InstallationFailed {
                FirmwareStatusEnumType::InstallationFailed
            } else {
                FirmwareStatusEnumType::Installed
            };
        self.settle_v201_firmware_update(terminal, request_id).await;
    }

    /// Settle a simulated 2.0.1 firmware rollout: compare-and-clear the in-flight
    /// slot and emit the `terminal` `FirmwareStatusNotification` — but **only if**
    /// `request_id` still owns the slot.
    ///
    /// [`complete`](crate::v201_firmware_update::V201FirmwareUpdateStore::complete)
    /// returns the station to idle and reports `true` when this rollout is still
    /// the one in flight; a `false` means a newer `UpdateFirmware` superseded it
    /// while it ran, so the store is left to the newer rollout and this one emits
    /// no terminal (there is no cancel `FirmwareStatusEnumType` — an abandoned
    /// rollout simply stops). `request_id` is only compared, never parsed or
    /// indexed, so no wire value can panic.
    async fn settle_v201_firmware_update(&self, terminal: FirmwareStatusEnumType, request_id: i32) {
        if self.v201_firmware_updates.complete(request_id).await {
            self.send_v201_firmware_status(terminal, request_id).await;
        }
    }

    /// Send a single `FirmwareStatusNotification(status)` CALL correlated by
    /// `request_id` (OCPP 2.0.1 Part 2). A best-effort progress report: a send
    /// failure is logged, not propagated (the update state machine continues).
    /// The 2.0.1 twin of the 1.6J
    /// [`send_firmware_status_notification`](Self::send_firmware_status_notification),
    /// routed through the [`v201_command`] constructor so the v201 wire type stays
    /// out of this module's imports.
    async fn send_v201_firmware_status(&self, status: FirmwareStatusEnumType, request_id: i32) {
        if let Err(e) = self
            .call(v201_command::v201_firmware_status_notification(
                status, request_id,
            ))
            .await
        {
            warn!(
                "v201 UpdateFirmware: FirmwareStatusNotification({status:?}) for \
                 request {request_id}: {e}"
            );
        }
    }

    /// Stream the simulated customer-data report an `Accepted`
    /// `CustomerInformation(report: true)` asked for, as one or more
    /// `NotifyCustomerInformation` CALLs (`ocpp.v201.call.NotifyCustomerInformation`,
    /// OCPP 2.0.1 Part 2, N-series — data privacy, Issue #537).
    ///
    /// Invoked only from the command-consumer task (see [`connect`](Self::connect)),
    /// never inline in the inbound-CALL handler, so the `CustomerInformation`
    /// CALLRESULT is flushed before the first `NotifyCustomerInformation` and there
    /// is no receive-loop re-entrancy — the same discipline as
    /// [`send_v201_notify_report`](Self::send_v201_notify_report) and the
    /// [`run_v201_log_upload`](Self::run_v201_log_upload) / firmware siblings.
    ///
    /// The simulator holds no real customer store, so it streams a small
    /// deterministic body ([`v201_command::V201_SIMULATED_CUSTOMER_INFORMATION_PAGES`]) paged by
    /// the pure
    /// [`v201_notify_customer_information_pages`](crate::v201_command::v201_notify_customer_information_pages)
    /// builder (`seqNo` from 0, `tbc` on every page but the last), each page
    /// correlated by `request_id` and stamped with the current time; every page is
    /// sent in order so the CSMS observes the paging flags in sequence. Once the
    /// stream settles — whether every page sent cleanly or a send failed midway —
    /// the in-flight marker is cleared via
    /// [`complete`](crate::v201_customer_information::V201CustomerInformationStore::complete)
    /// so a later `CustomerInformation` with the same `requestId` can report
    /// afresh. `request_id` is only echoed/compared, never parsed or indexed, so
    /// no wire value (including `i32::MIN`/`MAX`) can panic.
    async fn run_v201_customer_information_report(&self, request_id: i32) {
        let generated_at = v201_now();
        let pages = v201_command::v201_notify_customer_information_pages(
            request_id,
            &generated_at,
            v201_command::V201_SIMULATED_CUSTOMER_INFORMATION_PAGES,
        );
        for page in pages {
            let seq_no = page.seq_no;
            if let Err(e) = self.call(page).await {
                warn!(
                    "v201 CustomerInformation: NotifyCustomerInformation (seqNo {seq_no}) for \
                     request {request_id}: {e}"
                );
            }
        }
        // Settle the report: the id is no longer streaming, so a later request
        // with the same `requestId` reports afresh rather than being deduped.
        self.v201_customer_information_reports
            .complete(request_id)
            .await;
    }

    /// Stream the simulated firmware-publish progress an `Accepted`
    /// `PublishFirmware` triggers, as a sequence of `PublishFirmwareStatusNotification`
    /// CALLs (`ocpp.v201.call.PublishFirmwareStatusNotification`, OCPP 2.0.1 Part 2,
    /// firmware management — the Local-Controller firmware-cache trigger, Issue #540).
    ///
    /// Invoked only from the command-consumer task (see [`connect`](Self::connect)),
    /// never inline in the inbound-CALL handler, so the `PublishFirmware` CALLRESULT
    /// is flushed before the first `PublishFirmwareStatusNotification` and there is
    /// no receive-loop re-entrancy — the same discipline as the
    /// [`run_v201_firmware_update`](Self::run_v201_firmware_update) /
    /// [`run_v201_customer_information_report`](Self::run_v201_customer_information_report)
    /// siblings.
    ///
    /// The simulator caches no real image, so it emits the deterministic happy-path
    /// progression `Idle → DownloadScheduled → Downloading → Downloaded → Published`
    /// on a short timer, each `PublishFirmwareStatusNotification.req` correlated by
    /// `request_id`, with the simulator-supplied download `location` URIs
    /// ([`v201_command::V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS`]) on the terminal
    /// `Published` (and absent on the intermediate states, per the spec). The
    /// failure states (`DownloadFailed` / `InvalidChecksum` / `PublishFailed`) stay
    /// documented unproduced seams — reachable on the wire and in the schema, but
    /// not driven by the simulator (the `UpdateFirmware` convention). Once the
    /// stream settles the in-flight marker is cleared via
    /// [`complete`](crate::v201_publish_firmware::V201PublishFirmwareStore::complete)
    /// so a later `PublishFirmware` with the same `requestId` can publish afresh —
    /// unconditionally, since (unlike the single-slot firmware-update rollout) a
    /// firmware publish is independent per id with no supersede to hand the marker
    /// off to. `request_id` is only echoed/compared, never parsed or indexed, so no
    /// wire value (including `i32::MIN`/`MAX`) can panic. Ports the
    /// `PublishFirmwareStatusNotification` progress flow from
    /// [`ocpp/v201/enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/enums.py)'s
    /// `PublishFirmwareStatusEnumType`.
    async fn run_v201_publish_firmware_status(&self, request_id: i32) {
        // The deterministic happy-path progression. Intermediate states carry no
        // location; only the terminal `Published` advertises the cached image's
        // download URIs.
        let progression = [
            PublishFirmwareStatusEnumType::Idle,
            PublishFirmwareStatusEnumType::DownloadScheduled,
            PublishFirmwareStatusEnumType::Downloading,
            PublishFirmwareStatusEnumType::Downloaded,
            PublishFirmwareStatusEnumType::Published,
        ];
        let last = progression.len() - 1;
        for (i, status) in progression.into_iter().enumerate() {
            let location = (status == PublishFirmwareStatusEnumType::Published).then(|| {
                v201_command::V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect()
            });
            self.send_v201_publish_firmware_status(status, location, request_id)
                .await;
            // Pace the intermediate steps; no trailing wait after the terminal one.
            if i < last {
                tokio::time::sleep(FIRMWARE_UPDATE_STEP_DURATION).await;
            }
        }
        // Settle the publish: the id is no longer streaming, so a later
        // `PublishFirmware` with the same `requestId` publishes afresh rather than
        // being deduped.
        self.v201_publish_firmwares.complete(request_id).await;
    }

    /// Send a single `PublishFirmwareStatusNotification(status)` CALL correlated by
    /// `request_id`, carrying `location` only on the terminal `Published` state
    /// (OCPP 2.0.1 Part 2). A best-effort progress report: a send failure is
    /// logged, not propagated (the publish state machine continues). The
    /// firmware-publish twin of
    /// [`send_v201_firmware_status`](Self::send_v201_firmware_status), routed
    /// through the [`v201_command`] constructor so the v201 wire type stays out of
    /// this module's imports.
    async fn send_v201_publish_firmware_status(
        &self,
        status: PublishFirmwareStatusEnumType,
        location: Option<Vec<String>>,
        request_id: i32,
    ) {
        if let Err(e) = self
            .call(v201_command::v201_publish_firmware_status_notification(
                status, location, request_id,
            ))
            .await
        {
            warn!(
                "v201 PublishFirmware: PublishFirmwareStatusNotification({status:?}) for \
                 request {request_id}: {e}"
            );
        }
    }

    /// Send a single `ReservationStatusUpdate(status)` CALL telling the CSMS that
    /// reservation `reservation_id` is no longer valid (OCPP 2.0.1 Part 2 —
    /// `Expired` off the auto-expiry timer, `Removed` off an accepted
    /// `CancelReservation`). A best-effort notification: a send failure is logged,
    /// not propagated — the reservation is already freed locally, so a dropped
    /// update must not undo it (the same discipline the reservation
    /// [`EmitConnectorStatus`](RemoteCommand::EmitConnectorStatus) side effect
    /// uses). Routed through the [`v201_command`] constructor so the v201 wire type
    /// stays out of this module's imports.
    async fn send_v201_reservation_status_update(
        &self,
        reservation_id: i32,
        status: ReservationUpdateStatusEnumType,
    ) {
        if let Err(e) = self
            .call(v201_command::v201_reservation_status_update(
                reservation_id,
                status,
            ))
            .await
        {
            warn!(
                "v201 reservation: ReservationStatusUpdate({status:?}) for reservation \
                 {reservation_id}: {e}"
            );
        }
    }

    /// Emit a 2.0.1 `StatusNotification` for the EVSE(s) a `TriggerMessage`
    /// targets (`requestedMessage = StatusNotification`).
    ///
    /// `Some(id)` reports just that EVSE; `None` reports every EVSE. Unlike 1.6J
    /// there is no whole-station connector-`0` slot — 2.0.1 reports status per
    /// `(evseId ≥ 1, connectorId)` — so this iterates `1..=connector_count`
    /// (matching the boot-time announcement's V201 path). The already
    /// version-aware [`send_status_notification`](Self::send_status_notification)
    /// emits the 2.0.1 wire shape.
    async fn trigger_v201_status_notification(&self, evse_id: Option<i32>) {
        let count = self.config.connector_count as i32;
        let ids: Vec<u32> = match evse_id {
            Some(id) if (1..=count).contains(&id) => vec![id as u32],
            Some(id) => {
                warn!("v201 TriggerMessage(StatusNotification): unknown EVSE {id}, ignoring");
                return;
            }
            None => (1..=self.config.connector_count).collect(),
        };

        for id in ids {
            let status = self.connector_report_status(id).await;
            if let Err(e) = self
                .send_status_notification(id, status, ChargePointErrorCode::NoError)
                .await
            {
                warn!("v201 TriggerMessage(StatusNotification): EVSE {id} send failed: {e}");
            }
        }
    }

    /// Emit a standalone 2.0.1 `MeterValues` CALL for the EVSE(s) a
    /// `TriggerMessage` targets (`requestedMessage = MeterValues`).
    ///
    /// OCPP 2.0.1 keeps a dedicated `MeterValues` message
    /// (`ocpp.v201.call.MeterValues`) alongside the `TransactionEvent` flow, so a
    /// triggered `MeterValues` reports the EVSE's *current*
    /// `Energy.Active.Import.Register` reading (tagged `ReadingContext::Trigger`)
    /// in its own CALL — not a synthetic `TransactionEvent`. This reads the same
    /// `last_meter_reading()` the periodic sampler uses and works whether or not a
    /// transaction is in progress: an idle EVSE still reports its standing meter
    /// register. `Some(id)` reports just that EVSE; `None` reports every EVSE.
    async fn trigger_v201_meter_values(&self, evse_id: Option<i32>) {
        let count = self.config.connector_count as i32;
        let ids: Vec<u32> = match evse_id {
            Some(id) if (1..=count).contains(&id) => vec![id as u32],
            Some(id) => {
                warn!("v201 TriggerMessage(MeterValues): no meter for EVSE {id}, ignoring");
                return;
            }
            None => (1..=self.config.connector_count).collect(),
        };

        for id in ids {
            let connector_id = match ConnectorId::new(id) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Read the connector's latest meter value, releasing the lock before
            // building/sending so it is never held across the send.
            let reading = {
                let connectors = self.connectors.read().await;
                match connectors.get(&connector_id) {
                    Some(connector) => connector.last_meter_reading().await,
                    None => continue,
                }
            };

            // Flat single-connector-EVSE topology: the connector's value is its
            // EVSE id, consistent with the V201 StatusNotification / sampler paths.
            let request = V201MeterValuesRequest {
                evse_id: id as i32,
                meter_value: v201_transaction::triggered_energy_meter_value(
                    reading.energy_wh,
                    &reading.timestamp.to_rfc3339(),
                ),
                custom_data: None,
            };
            if let Err(e) = self.call(request).await {
                warn!("v201 TriggerMessage(MeterValues): EVSE {id} send failed: {e}");
            }
        }
    }

    /// Emit an on-demand 2.0.1 `TransactionEvent(Updated)` for the in-flight
    /// transaction(s) a `TriggerMessage` targets (`requestedMessage =
    /// TransactionEvent`).
    ///
    /// A 2.0.1 `TransactionEvent` is inherently transaction-scoped, so this
    /// reports one `Updated` event (`triggerReason = Trigger`) per active
    /// transaction on the targeted scope, carrying the connector's current
    /// reading. `seqNo` is drawn from the transaction's shared
    /// [`V201Session`] counter so a triggered event interleaves cleanly with the
    /// periodic sampler. `Some(id)` scopes to that EVSE; `None` reports every
    /// active transaction. With no active transaction on the targeted scope there
    /// is nothing to report — the trigger was `Accepted` as a capability, but idle
    /// there is no event to emit.
    async fn trigger_v201_transaction_event(&self, evse_id: Option<i32>) {
        // Snapshot the (transaction_id -> connector) pairs to report, scoped to
        // the targeted EVSE when present (in the flat topology an EVSE id equals
        // its connector's value). Copied out so no lock is held across a send.
        let targets: Vec<(i32, ConnectorId)> = {
            let active = self.active_transactions.read().await;
            active
                .iter()
                .filter(|(_txn, cid)| match evse_id {
                    Some(id) => cid.value() as i32 == id,
                    None => true,
                })
                .map(|(txn, cid)| (*txn, *cid))
                .collect()
        };

        if targets.is_empty() {
            info!(
                "v201 TriggerMessage(TransactionEvent): no active transaction on the \
                 targeted scope, nothing to emit"
            );
            return;
        }

        for (transaction_id, connector_id) in targets {
            // Claim the next seqNo from the transaction's shared counter. A
            // missing session means the transaction is being torn down; skip it
            // rather than inventing a seqNo (same discipline as the sampler).
            let seq_no = match self.v201_sessions.read().await.get(&transaction_id) {
                Some(session) => session.next_seq_no.fetch_add(1, Ordering::SeqCst),
                None => continue,
            };

            let reading = {
                let connectors = self.connectors.read().await;
                match connectors.get(&connector_id) {
                    Some(connector) => connector.last_meter_reading().await,
                    None => continue,
                }
            };

            let txid_str = transaction_id.to_string();
            let session = v201_transaction::SessionRef {
                transaction_id: &txid_str,
                evse_id: connector_id.value() as i32,
                connector_id: 1,
            };
            let request = v201_transaction::transaction_event_triggered(
                &session,
                seq_no,
                reading.energy_wh,
                &reading.timestamp.to_rfc3339(),
            );
            if let Err(e) = self.call(request).await {
                warn!(
                    "v201 TriggerMessage(TransactionEvent): transaction {transaction_id} \
                     send failed: {e}"
                );
            }
        }
    }

    /// Run the simulated diagnostics-upload state machine for an `Accepted`
    /// `GetDiagnostics` (OCPP 1.6J §4.x, firmware-management profile).
    ///
    /// Invoked only from the command-consumer task (see [`connect`](Self::connect)),
    /// never inline in the inbound-CALL handler, so the `GetDiagnostics`
    /// CALLRESULT is flushed before the first `DiagnosticsStatusNotification`
    /// and there is no receive-loop re-entrancy/deadlock.
    ///
    /// The simulator has no real archive to upload, so it models the upload on
    /// a short timer: report `Uploading`, wait [`DIAGNOSTICS_UPLOAD_DURATION`],
    /// then report a terminal status. The latest status is retained so a
    /// subsequent `TriggerMessage(DiagnosticsStatusNotification)` reports it.
    /// Mirrors the progress reporting in the Python reference's
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    ///
    /// The terminal status is `Uploaded` on the happy path; with
    /// [`ChargePointConfig::diagnostics_upload_should_fail`] set (opt-in fault
    /// injection) it is `UploadFailed` instead, so a CSMS can be tested against
    /// a diagnostics upload that fails (OCPP 1.6J §4.x; `DiagnosticsStatus`).
    async fn run_diagnostics_upload(&self) {
        self.set_diagnostics_status(DiagnosticsStatus::Uploading)
            .await;
        tokio::time::sleep(DIAGNOSTICS_UPLOAD_DURATION).await;
        let terminal = if self.config.diagnostics_upload_should_fail {
            DiagnosticsStatus::UploadFailed
        } else {
            DiagnosticsStatus::Uploaded
        };
        self.set_diagnostics_status(terminal).await;
    }

    /// Record `status` as the CP's current diagnostics status and announce it to
    /// the CSMS with a `DiagnosticsStatusNotification`.
    async fn set_diagnostics_status(&self, status: DiagnosticsStatus) {
        *self.diagnostics_status.write().await = status;
        self.send_diagnostics_status_notification(status).await;
    }

    /// Send a single `DiagnosticsStatusNotification(status)` CALL to the CSMS
    /// (OCPP 1.6J §4.x). Does not mutate the stored status — callers that change
    /// state use [`set_diagnostics_status`](Self::set_diagnostics_status).
    async fn send_diagnostics_status_notification(&self, status: DiagnosticsStatus) {
        if let Err(e) = self
            .call(DiagnosticsStatusNotificationRequest { status })
            .await
        {
            warn!("DiagnosticsStatusNotification({status:?}): send failed: {e}");
        }
    }

    /// Run the simulated firmware-update state machine for an `Accepted`
    /// `UpdateFirmware` (OCPP 1.6J §4.x, firmware-management profile).
    ///
    /// Invoked only from the command-consumer task (see [`connect`](Self::connect)),
    /// never inline in the inbound-CALL handler, so the `UpdateFirmware`
    /// CALLRESULT is flushed before the first `FirmwareStatusNotification` and
    /// there is no receive-loop re-entrancy/deadlock.
    ///
    /// The simulator has no real image to download or install, so it models the
    /// update on a short timer, stepping through the sequence the spec defines
    /// and announcing each step. On the happy path (the default) that is
    /// `Downloading` → `Downloaded` → `Installing` → `Installed`.
    ///
    /// With [`ChargePointConfig::firmware_update_outcome`] set to a failure
    /// variant (opt-in fault injection) the simulator instead takes the matching
    /// error branch — `Downloading → DownloadFailed`, or `Downloading →
    /// Downloaded → Installing → InstallationFailed` — so a CSMS can be tested
    /// against a firmware rollout that fails (OCPP 1.6J §4.x; `FirmwareStatus`).
    /// (`UpdateFirmware.req`'s `retrieveDate` scheduling remains out of scope.)
    ///
    /// The latest status — success or failure — is retained so a subsequent
    /// `TriggerMessage(FirmwareStatusNotification)` reports it. Mirrors the
    /// progress reporting in the Python reference's
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    async fn run_firmware_update(&self) {
        self.set_firmware_status(FirmwareStatus::Downloading).await;
        tokio::time::sleep(FIRMWARE_UPDATE_STEP_DURATION).await;

        if self.config.firmware_update_outcome == FirmwareUpdateOutcome::DownloadFailed {
            // Download phase fails: the CP rests in DownloadFailed; there is no
            // image to install, so the sequence stops here.
            self.set_firmware_status(FirmwareStatus::DownloadFailed)
                .await;
            return;
        }
        self.set_firmware_status(FirmwareStatus::Downloaded).await;
        tokio::time::sleep(FIRMWARE_UPDATE_STEP_DURATION).await;

        self.set_firmware_status(FirmwareStatus::Installing).await;
        tokio::time::sleep(FIRMWARE_UPDATE_STEP_DURATION).await;

        let terminal =
            if self.config.firmware_update_outcome == FirmwareUpdateOutcome::InstallationFailed {
                FirmwareStatus::InstallationFailed
            } else {
                FirmwareStatus::Installed
            };
        self.set_firmware_status(terminal).await;
    }

    /// Record `status` as the CP's current firmware status and announce it to
    /// the CSMS with a `FirmwareStatusNotification`.
    async fn set_firmware_status(&self, status: FirmwareStatus) {
        *self.firmware_status.write().await = status;
        self.send_firmware_status_notification(status).await;
    }

    /// Send a single `FirmwareStatusNotification(status)` CALL to the CSMS
    /// (OCPP 1.6J §4.x). Does not mutate the stored status — callers that change
    /// state use [`set_firmware_status`](Self::set_firmware_status).
    async fn send_firmware_status_notification(&self, status: FirmwareStatus) {
        if let Err(e) = self
            .call(FirmwareStatusNotificationRequest { status })
            .await
        {
            warn!("FirmwareStatusNotification({status:?}): send failed: {e}");
        }
    }

    /// Emit `StatusNotification` for the connector(s) a `TriggerMessage` targets.
    ///
    /// `Some(id)` reports just that connector (id `0` is the charge point as a
    /// whole); `None` reports connector `0` and every physical connector. Each
    /// report carries the connector's *current* status so the CSMS can refresh
    /// its view on demand.
    async fn trigger_status_notification(&self, connector_id: Option<i32>) {
        let ids: Vec<u32> = match connector_id {
            Some(id) if (0..=self.config.connector_count as i32).contains(&id) => vec![id as u32],
            Some(id) => {
                warn!("TriggerMessage(StatusNotification): unknown connector {id}, ignoring");
                return;
            }
            None => (0..=self.config.connector_count).collect(),
        };

        for id in ids {
            let status = self.connector_report_status(id).await;
            if let Err(e) = self
                .send_status_notification(id, status, ChargePointErrorCode::NoError)
                .await
            {
                warn!("TriggerMessage(StatusNotification): connector {id} send failed: {e}");
            }
        }
    }

    /// Emit an on-demand `MeterValues` for the connector(s) a `TriggerMessage`
    /// targets (OCPP 1.6J §4.x, `requestedMessage = MeterValues`).
    ///
    /// `Some(id)` reports just connector `id`; `None` reports every physical
    /// connector. Each frame carries the connector's *current* meter reading
    /// tagged `ReadingContext::Trigger`, plus the active `transactionId` when
    /// the connector is charging. An **idle** connector still reports its
    /// standing meter register (transaction id omitted) — the faithful
    /// "read the meter now" behavior, matching how the periodic sampler reads
    /// the same `last_meter_reading()`. Connector `0` has no meter, so a
    /// `Some(0)` or out-of-range target is logged and ignored.
    async fn trigger_meter_values(&self, connector_id: Option<i32>) {
        let count = self.config.connector_count as i32;
        let ids: Vec<u32> = match connector_id {
            Some(id) if (1..=count).contains(&id) => vec![id as u32],
            Some(id) => {
                warn!("TriggerMessage(MeterValues): no meter for connector {id}, ignoring");
                return;
            }
            None => (1..=self.config.connector_count).collect(),
        };

        let measurands = self.config.meter_value_measurands.clone();
        for id in ids {
            let connector_id = match ConnectorId::new(id) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Read the connector's latest meter value, releasing the lock
            // before building/sending so it is never held across the send.
            let reading = {
                let connectors = self.connectors.read().await;
                match connectors.get(&connector_id) {
                    Some(connector) => connector.last_meter_reading().await,
                    None => continue,
                }
            };

            let transaction_id = self.connector_active_transaction(connector_id).await;
            let request = meter_sampler::build_meter_values_request(
                connector_id,
                transaction_id,
                &reading,
                &measurands,
                ReadingContext::Trigger,
            );
            if !meter_sampler::has_samples(&request) {
                // No supported measurands → nothing worth sending; the OCPP
                // 1.6J MeterValues.json schema forbids an empty sampledValue list.
                continue;
            }

            let message = match ocpp_messages::CallMessage::new(
                MeterValuesRequest::ACTION_NAME.to_string(),
                request,
            ) {
                Ok(call) => Message::Call(call),
                Err(e) => {
                    error!("TriggerMessage(MeterValues): build failed: {e}");
                    continue;
                }
            };

            if let Some(client) = self.client.read().await.as_ref() {
                if let Err(e) = client.send_message(message).await {
                    warn!("TriggerMessage(MeterValues): connector {id} send failed: {e}");
                }
            }
        }
    }

    /// Reverse-lookup the CSMS-assigned `transactionId` currently active on
    /// `connector_id`, if any. Reads the same `transaction_id → connector` map
    /// that `stop_transaction` keeps, so a triggered `MeterValues` can attach
    /// the in-flight transaction id.
    async fn connector_active_transaction(&self, connector_id: ConnectorId) -> Option<i32> {
        self.active_transactions
            .read()
            .await
            .iter()
            .find_map(|(txn, cid)| (*cid == connector_id).then_some(*txn))
    }

    /// Current reportable status of connector `id` (id `0` = the charge point
    /// itself, which has no per-connector state and reports `Available`).
    async fn connector_report_status(&self, id: u32) -> ChargePointStatus {
        if id == 0 {
            return ChargePointStatus::Available;
        }
        match ConnectorId::new(id) {
            Ok(cid) => match self.get_connector(cid).await {
                Some(c) => c.status().await,
                None => ChargePointStatus::Available,
            },
            Err(_) => ChargePointStatus::Available,
        }
    }

    /// Returns the number of registered `@on` handlers in the dispatcher.
    pub async fn handler_count(&self) -> usize {
        self.dispatcher.read().await.handler_count()
    }

    /// Read a configuration key from the shared store.
    pub async fn get_config_value(&self, key: &str) -> Option<String> {
        self.config_store.read().await.get(key).cloned()
    }

    /// Write a configuration key to the shared store.
    pub async fn set_config_value(&self, key: &str, value: String) -> Result<()> {
        self.config_store
            .write()
            .await
            .set(key, value)
            .map_err(|e| ChargePointError::configuration(e).into())
    }
}

/// `TransportMessageHandler` implementation for `ChargePoint` (delegates to
/// the internal dispatcher, same as `CpHandler`). Kept for API completeness;
/// the live message loop uses `CpHandler` (passed to `WebSocketClient`).
#[async_trait::async_trait]
impl TransportMessageHandler for ChargePoint {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        let call = match message {
            Message::Call(c) => c,
            Message::CallResult(_) | Message::CallError(_) => return Ok(None),
        };

        let unique_id = call.unique_id.clone();
        match self.dispatcher.read().await.dispatch(&call).await {
            Ok(payload) => {
                let result = CallResultMessage::new(unique_id, payload).map_err(|e| {
                    OcppError::Internal {
                        message: e.to_string(),
                    }
                })?;
                Ok(Some(Message::CallResult(result)))
            }
            Err(e) => {
                let (code, description) = ocpp_error_to_callerror_code(&e);
                let err_msg = CallErrorMessage::new(unique_id, code, description, None);
                Ok(Some(Message::CallError(err_msg)))
            }
        }
    }

    async fn handle_event(&self, event: TransportEvent) {
        match event {
            TransportEvent::Connected { .. } => {
                info!("Transport connected");
            }
            TransportEvent::Disconnected { reason, .. } => {
                warn!("Transport disconnected: {}", reason);
                *self.is_connected.write().await = false;

                let _ = self
                    .event_sender
                    .send(ChargePointEvent::Disconnected { reason });
            }
            TransportEvent::Error { error, .. } => {
                error!("Transport error: {}", error);
                let _ = self.event_sender.send(ChargePointEvent::Error {
                    error: ChargePointError::TransportError(error.to_string()),
                });
            }
            _ => {}
        }
    }
}

/// Whether an `IdTagInfo` has passed its `expiryDate` and so is stale.
///
/// Used by [`ChargePoint::authorize`] to decide whether a Local Authorization
/// List entry is still fresh enough to honor (OCPP 1.6J §4.1.3): an entry with
/// no `expiryDate` never expires on its own, while one whose `expiryDate` is at
/// or before now is ignored so authorization falls through to the cache / CSMS.
fn id_tag_info_expired(info: &IdTagInfo) -> bool {
    matches!(info.expiry_date, Some(expiry) if chrono::Utc::now() >= expiry)
}

/// Test-only seams onto private state, so wire tests can arrange a scenario the
/// live path reaches only through a transport (e.g. an installed `TxProfile`,
/// normally landed by an accepted `RequestStartTransaction` / `SetChargingProfile`).
#[cfg(test)]
impl ChargePoint {
    /// Install a v201 `TxProfile` directly into the store the
    /// `GetCompositeSchedule` and periodic-metering handlers read, bypassing the
    /// transaction-start plumbing (which needs a live client).
    async fn test_install_v201_tx_profile(&self, evse_id: i32, profile: ChargingProfileType) {
        self.v201_tx_profiles.install(evse_id, profile).await;
    }

    /// Install a v201 `TxDefaultProfile` directly into the fallback store the
    /// `GetCompositeSchedule` and periodic-metering handlers consult, bypassing
    /// the `SetChargingProfile` wire path (Issue #471). `evse_id = 0` seeds the
    /// station-wide default.
    async fn test_install_v201_tx_default_profile(
        &self,
        evse_id: i32,
        profile: ChargingProfileType,
    ) {
        self.v201_tx_default_profiles
            .install(evse_id, profile)
            .await;
    }

    /// Install a v201 station ceiling directly into the store the
    /// `GetCompositeSchedule` and periodic-metering handlers cap by, bypassing the
    /// `SetChargingProfile` wire path (Issue #511). `evse_id = 0` seeds the
    /// whole-station ceiling for `kind`.
    async fn test_install_v201_station_ceiling(
        &self,
        kind: CeilingKind,
        evse_id: i32,
        profile: ChargingProfileType,
    ) {
        self.v201_station_ceilings
            .install(kind, evse_id, profile)
            .await;
    }

    /// Seed a live reservation exactly as an accepted `ReserveNow` would — reserve
    /// the connector (`Available → Reserved`) and record `reservationId →
    /// connector` in the shared store — so a `CancelReservation` wire test has a
    /// reservation to cancel without depending on the (still-open) v201
    /// `ReserveNow` slice. Connector clones share their state through `Arc`s, so
    /// the reserve is visible to the live handler and to
    /// [`test_connector_status`](Self::test_connector_status).
    async fn test_reserve(&self, reservation_id: i32, connector_id: ConnectorId) {
        let mut connector = self
            .connectors
            .read()
            .await
            .get(&connector_id)
            .cloned()
            .expect("test connector should exist");
        connector
            .reserve("test-token".to_string())
            .await
            .expect("a free connector should reserve");
        self.reservations
            .write()
            .await
            .insert(reservation_id, connector_id);
    }

    /// Record a live transaction (`transactionId → connector`) directly in the
    /// shared `active_transactions` store, exactly as an accepted
    /// `RequestStartTransaction` / local start would, so a `GetTransactionStatus`
    /// wire test has an ongoing transaction to query without a live client. The V201
    /// path renders the `i32` key as its decimal string on the wire.
    async fn test_insert_active_transaction(&self, transaction_id: i32, connector_id: ConnectorId) {
        self.active_transactions
            .write()
            .await
            .insert(transaction_id, connector_id);
    }

    /// Read a connector's current status, so a wire test can assert a
    /// `Reserved → Available` transition after a handler runs.
    async fn test_connector_status(&self, connector_id: ConnectorId) -> ChargePointStatus {
        self.connectors
            .read()
            .await
            .get(&connector_id)
            .expect("test connector should exist")
            .status()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::v16j::{
        ChangeConfigurationRequest, GetConfigurationRequest, HeartbeatRequest, HeartbeatResponse,
        RemoteStartTransactionRequest, RemoteStopTransactionRequest, ResetRequest,
    };
    use ocpp_messages::{CallMessage, Message, MessageType};
    use ocpp_types::common::AvailabilityType;
    use ocpp_types::v16j::{ConfigurationStatus, RemoteStartStopStatus, ResetType};
    use ocpp_types::CallResultMessage;

    // Helper: build a CallMessage from an action struct
    fn make_call<T: OcppAction>(req: T) -> CallMessage {
        CallMessage::new(T::ACTION_NAME.to_string(), req).unwrap()
    }

    // --- OCPP 2.0.1 provisioning slice (M7, issue #417) --------------------
    // Ports the version-negotiation + BootNotification-construction half of the
    // reference `examples/v201/charge_point.py`. Runtime message-loop wiring is
    // the slice-2 follow-up; these pin only the negotiation seam and the
    // spec-fidelity of the built provisioning payload.

    #[test]
    fn test_default_config_speaks_v16j() {
        // Regression guard: the default CP is unchanged — 1.6J, offering
        // exactly `ocpp1.6` (never `ocpp2.0.1`), so existing deployments behave
        // identically after this slice.
        let config = ChargePointConfig::default();
        assert_eq!(config.protocol_version, OcppVersion::V16J);
        assert_eq!(
            config.transport_config.sub_protocols,
            vec!["ocpp1.6".to_string()]
        );
    }

    #[test]
    fn test_subprotocols_for_maps_version_to_wire_identifier() {
        // A CP offers exactly the one subprotocol it speaks (the reference
        // `subprotocols=["ocpp1.6"]` / `["ocpp2.0.1"]`).
        assert_eq!(
            subprotocols_for(OcppVersion::V16J),
            vec!["ocpp1.6".to_string()]
        );
        assert_eq!(
            subprotocols_for(OcppVersion::V201),
            vec!["ocpp2.0.1".to_string()]
        );
    }

    #[test]
    fn test_for_version_v201_offers_ocpp201_and_is_consistent() {
        // `for_version` keeps `protocol_version` and the offered subprotocol in
        // lockstep — a 2.0.1 CP offers `ocpp2.0.1`, not the default `ocpp1.6`.
        let config = ChargePointConfig::for_version(OcppVersion::V201);
        assert_eq!(config.protocol_version, OcppVersion::V201);
        assert_eq!(
            config.transport_config.sub_protocols,
            vec!["ocpp2.0.1".to_string()]
        );
        // And `for_version(V16J)` is exactly the default.
        let v16 = ChargePointConfig::for_version(OcppVersion::V16J);
        assert_eq!(v16.protocol_version, OcppVersion::V16J);
        assert_eq!(
            v16.transport_config.sub_protocols,
            vec!["ocpp1.6".to_string()]
        );
    }

    #[test]
    fn test_config_without_protocol_version_deserializes_to_v16j() {
        // Backward-compat: a persisted config predating `protocol_version` must
        // still load (defaulting to 1.6J), not fail deserialization.
        let json = serde_json::to_value(ChargePointConfig::default()).unwrap();
        let mut obj = json.as_object().unwrap().clone();
        obj.remove("protocol_version");
        let restored: ChargePointConfig =
            serde_json::from_value(serde_json::Value::Object(obj)).unwrap();
        assert_eq!(restored.protocol_version, OcppVersion::V16J);
    }

    #[test]
    fn test_v201_boot_notification_request_maps_identity() {
        // `ChargePointVendorInfo` → `ChargingStationType`, faithful to
        // `examples/v201/charge_point.py`'s
        // `BootNotification(charging_station={…}, reason="PowerUp")`.
        let config = ChargePointConfig::default();
        let req = config.v201_boot_notification_request();

        assert_eq!(
            req.charging_station.vendor_name,
            config.vendor_info.charge_point_vendor
        );
        assert_eq!(
            req.charging_station.model,
            config.vendor_info.charge_point_model
        );
        assert_eq!(
            req.charging_station.serial_number,
            config.vendor_info.charge_point_serial_number
        );
        assert_eq!(
            req.charging_station.firmware_version,
            config.vendor_info.firmware_version
        );
        assert_eq!(req.reason, ocpp_types::v201::BootReasonEnumType::PowerUp);
    }

    #[test]
    fn test_v201_boot_notification_request_is_schema_valid() {
        // Wire fidelity: the built payload must satisfy the bundled OCPP 2.0.1
        // BootNotification JSON Schema (which enforces `model` ≤ 20,
        // `vendorName` ≤ 50, `serialNumber` ≤ 25, and `reason`'s enum).
        let config = ChargePointConfig::for_version(OcppVersion::V201);
        let req = config.v201_boot_notification_request();
        let payload = serde_json::to_value(&req).unwrap();

        let validator = SchemaValidator::v201();
        assert!(
            validator
                .validate_call("BootNotification", &payload)
                .is_ok(),
            "built v201 BootNotification should be schema-valid, got: {payload}"
        );
    }

    #[test]
    fn test_v201_registration_status_maps_to_canonical() {
        // Total, lossless 1:1 mapping — every 2.0.1 registration status has a
        // matching canonical 1.6J status, so the shared boot retry loop reads
        // the same three states regardless of version.
        assert_eq!(
            v201_registration_status_to_canonical(RegistrationStatusEnumType::Accepted),
            RegistrationStatus::Accepted
        );
        assert_eq!(
            v201_registration_status_to_canonical(RegistrationStatusEnumType::Pending),
            RegistrationStatus::Pending
        );
        assert_eq!(
            v201_registration_status_to_canonical(RegistrationStatusEnumType::Rejected),
            RegistrationStatus::Rejected
        );
    }

    #[test]
    fn test_charge_point_status_maps_to_v201_connector_status() {
        use ConnectorStatusEnumType as V201;
        // The four "vehicle connected, session in progress" states plus
        // Finishing collapse to Occupied in the reduced 2.0.1 set; the rest
        // carry across unchanged.
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::Available),
            V201::Available
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::Preparing),
            V201::Occupied
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::Charging),
            V201::Occupied
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::SuspendedEV),
            V201::Occupied
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::SuspendedEVSE),
            V201::Occupied
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::Finishing),
            V201::Occupied
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::Reserved),
            V201::Reserved
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::Faulted),
            V201::Faulted
        );
        assert_eq!(
            charge_point_status_to_v201(ChargePointStatus::Unavailable),
            V201::Unavailable
        );
    }

    /// The 2.0.1 availability target maps to the connector state the CP applies
    /// on a `ChangeAvailability` (slice 6b): `Operative → Available`,
    /// `Inoperative → Unavailable`. Total over both variants — a `charge_point_status_to_v201`
    /// round-trip confirms `Inoperative` surfaces as the 2.0.1 `Unavailable`
    /// connector status the CSMS observes.
    #[test]
    fn test_operational_status_maps_to_connector_state() {
        assert_eq!(
            operational_status_to_cp_status(OperationalStatusEnumType::Operative),
            ChargePointStatus::Available
        );
        assert_eq!(
            operational_status_to_cp_status(OperationalStatusEnumType::Inoperative),
            ChargePointStatus::Unavailable
        );
        // An Inoperative connector is what the CSMS sees as 2.0.1 `Unavailable`.
        assert_eq!(
            charge_point_status_to_v201(operational_status_to_cp_status(
                OperationalStatusEnumType::Inoperative
            )),
            ConnectorStatusEnumType::Unavailable
        );
    }

    #[test]
    fn test_v201_config_builds_v201_validator() {
        // A V201-configured CP must validate its outgoing 2.0.1 boot payload
        // against the 2.0.1 schema set. Proven indirectly: the built 2.0.1
        // BootNotification is schema-valid, and a V201 CP is constructible with
        // validation enabled (which selects `SchemaValidator::v201()`).
        let mut config = ChargePointConfig::for_version(OcppVersion::V201);
        config.charge_point_id = "CP_V201_VALIDATOR".to_string();
        assert!(config.validate_payloads);
        let cp = ChargePoint::new(config).expect("V201 CP with validation must build");
        // The validator is private; the observable proxy is that construction
        // succeeded and the config retained the 2.0.1 protocol version.
        assert_eq!(cp.config.protocol_version, OcppVersion::V201);
    }

    #[tokio::test]
    async fn test_charge_point_creation() {
        let config = ChargePointConfig::default();
        let cp = ChargePoint::new(config).unwrap();

        assert!(!cp.is_connected().await);
        assert_eq!(cp.registration_status().await, RegistrationStatus::Rejected);
    }

    #[tokio::test]
    async fn test_connectors_initialization() {
        let config = ChargePointConfig {
            connector_count: 3,
            ..Default::default()
        };
        let cp = ChargePoint::new(config).unwrap();

        let connectors = cp.get_connectors().await;
        assert_eq!(connectors.len(), 3);

        for i in 1..=3 {
            let connector_id = ConnectorId::new(i).unwrap();
            assert!(connectors.contains_key(&connector_id));
        }
    }

    #[tokio::test]
    async fn test_connector_operations() {
        let config = ChargePointConfig::default();
        let cp = ChargePoint::new(config).unwrap();
        let connector_id = ConnectorId::new(1).unwrap();

        // Plug in/out cycle
        cp.plug_in(connector_id).await.unwrap();
        cp.plug_out(connector_id).await.unwrap();

        // Fault operations
        cp.set_fault(
            connector_id,
            ocpp_types::v16j::ChargePointErrorCode::NoError,
            None,
        )
        .await
        .unwrap();
        cp.clear_fault(connector_id).await.unwrap();
    }

    #[test]
    fn test_config_default() {
        let config = ChargePointConfig::default();
        assert_eq!(config.charge_point_id, "CP001");
        assert_eq!(config.connector_count, 2);
        assert_eq!(config.heartbeat_interval, 300);
    }

    #[test]
    fn call_timeout_default_is_30s() {
        let config = ChargePointConfig::default();
        assert_eq!(config.call_timeout, 30);
    }

    #[tokio::test]
    async fn call_returns_transport_error_when_disconnected() {
        let config = ChargePointConfig::default();
        let cp = ChargePoint::new(config).unwrap();

        // No connect() called, so client is None
        let result = cp.call(HeartbeatRequest {}).await;

        match result {
            Err(OcppError::Transport { ref message }) => {
                assert!(
                    message.contains("Not connected"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected Transport error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn timed_out_call_prunes_pending_entry_no_leak() {
        // Issue #323: a CALL the CSMS never answers must not leak its entry in
        // PendingCallMap. Drive the *real* call() path against a mock that
        // accepts the socket but replies to nothing, and assert the pending map
        // returns to its pre-call size — across repeated timeouts.
        // Answer BootNotification (needed for connect()) but nothing else, so
        // every later Heartbeat CALL runs out the timeout.
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            call_timeout: 1, // keep the test fast
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        // Grab the shared pending map so we can observe its size directly.
        let pending = {
            let guard = cp.client.read().await;
            guard.as_ref().expect("connected").pending_calls()
        };
        assert_eq!(pending.len(), 0, "no in-flight calls before");

        for _ in 0..3 {
            let result = cp.call(HeartbeatRequest {}).await;
            assert!(
                matches!(result, Err(OcppError::Timeout { .. })),
                "swallowed reply must time out, got {result:?}"
            );
            assert_eq!(
                pending.len(),
                0,
                "a timed-out call() must leave no residue in PendingCallMap"
            );
        }

        cp.disconnect().await.ok();
    }

    // --- dispatcher wiring tests ---

    #[tokio::test]
    async fn dispatcher_has_19_default_handlers() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        // 10 Core Profile actions + GetDiagnostics + UpdateFirmware
        // (firmware-management, #69/#70) + ReserveNow + CancelReservation
        // (§5.14/§5.4, #71) + SetChargingProfile + ClearChargingProfile
        // (Smart Charging, §5.16/§5.2, #94) + GetCompositeSchedule
        // (Smart Charging, §5.x, #95) + GetLocalListVersion + SendLocalList
        // (Local Authorization List, §5.x, #93).
        assert_eq!(cp.handler_count().await, 19);
    }

    #[tokio::test]
    async fn v201_get_composite_schedule_rejects_an_evse_with_no_profile() {
        // A V201 CP with no installed TxProfile has nothing to compose → Rejected
        // with no schedule. Exercises the wired V201 handler end-to-end over the
        // dispatcher: a 2.0.1 CP now answers `GetCompositeSchedule` with the 2.0.1
        // `evseId`/`GenericStatus` shape rather than the 1.6J one.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let call = make_call(V201GetCompositeScheduleRequest {
            duration: 3_600,
            charging_rate_unit: None,
            evse_id: 1,
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetCompositeScheduleResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericStatusEnumType::Rejected);
                assert!(
                    body.schedule.is_none(),
                    "a rejected response carries no schedule"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_composite_schedule_accepts_and_composes_an_installed_profile() {
        use ocpp_types::v201::{
            ChargingProfileKindEnumType, ChargingRateUnitEnumType, ChargingSchedulePeriodType,
            ChargingScheduleType,
        };
        // A flat 11 kW TxProfile on EVSE 1, installed as an accepted
        // RequestStartTransaction / SetChargingProfile would land it.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let profile = ChargingProfileType {
            id: 1,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeEnumType::TxProfile,
            charging_profile_kind: ChargingProfileKindEnumType::Relative,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: ChargingRateUnitEnumType::W,
                charging_schedule_period: vec![ChargingSchedulePeriodType {
                    start_period: 0,
                    limit: 11_000.0,
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
        };
        cp.test_install_v201_tx_profile(1, profile).await;

        let call = make_call(V201GetCompositeScheduleRequest {
            duration: 3_600,
            charging_rate_unit: None,
            evse_id: 1,
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetCompositeScheduleResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericStatusEnumType::Accepted);
                let schedule = body
                    .schedule
                    .expect("an accepted response carries the composed schedule");
                assert_eq!(schedule.evse_id, 1);
                assert_eq!(schedule.duration, 3_600);
                assert_eq!(schedule.charging_rate_unit, ChargingRateUnitEnumType::W);
                assert_eq!(schedule.charging_schedule_period.len(), 1);
                assert_eq!(schedule.charging_schedule_period[0].start_period, 0);
                assert_eq!(schedule.charging_schedule_period[0].limit, 11_000.0);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    // --- SetChargingProfile / TxDefaultProfile (v201) over the wire --------
    // (Issue #471)

    // Build a v201 `SetChargingProfile` CALL installing `profile` on `evse_id`.
    fn make_v201_set_charging_profile(evse_id: i32, profile: ChargingProfileType) -> CallMessage {
        make_call(V201SetChargingProfileRequest {
            evse_id,
            charging_profile: profile,
            custom_data: None,
        })
    }

    // A single flat-`limit` W schedule of the given `purpose`, tagged with `id`.
    fn v201_flat_profile(
        id: i32,
        purpose: ChargingProfilePurposeEnumType,
        limit: f64,
    ) -> ChargingProfileType {
        use ocpp_types::v201::{
            ChargingProfileKindEnumType, ChargingRateUnitEnumType, ChargingSchedulePeriodType,
            ChargingScheduleType,
        };
        ChargingProfileType {
            id,
            stack_level: 0,
            charging_profile_purpose: purpose,
            charging_profile_kind: ChargingProfileKindEnumType::Relative,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: ChargingRateUnitEnumType::W,
                charging_schedule_period: vec![ChargingSchedulePeriodType {
                    start_period: 0,
                    limit,
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

    #[tokio::test]
    async fn v201_set_charging_profile_accepts_and_installs_a_txdefaultprofile() {
        // A TxDefaultProfile is station configuration, not transaction-scoped, so
        // it is Accepted with no live transaction and lands in the *default* store
        // (read via `installed_tx_default_profile`) — never the transaction-scoped
        // one. Exercises the wired V201 handler's purpose-branched install.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let call = make_v201_set_charging_profile(
            1,
            v201_flat_profile(7, ChargingProfilePurposeEnumType::TxDefaultProfile, 6_000.0),
        );
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetChargingProfileResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ChargingProfileStatusEnumType::Accepted);
                assert!(
                    body.status_info.is_none(),
                    "an accepted install has no detail"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_tx_default_profile(1).await.expect("stored").id,
            7,
            "the default landed in the default store"
        );
        assert!(
            cp.installed_tx_profile(1).await.is_none(),
            "a default does not land in the transaction-scoped TxProfile store"
        );
    }

    #[tokio::test]
    async fn v201_set_charging_profile_accepts_and_installs_a_station_ceiling() {
        // A ChargingStationMaxProfile is station configuration, not
        // transaction-scoped, so it is Accepted with no live transaction and lands
        // in the *ceiling* store (read via `installed_station_ceiling`) — never a
        // Tx/TxDefault store. `evseId = 0` installs the whole-station ceiling
        // (Issue #511).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let call = make_v201_set_charging_profile(
            0,
            v201_flat_profile(
                9,
                ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
                6_000.0,
            ),
        );
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetChargingProfileResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ChargingProfileStatusEnumType::Accepted);
                assert!(
                    body.status_info.is_none(),
                    "an accepted ceiling has no rejection detail"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_station_ceiling(CeilingKind::Max, 0)
                .await
                .expect("stored")
                .id,
            9,
            "the ceiling landed in the ceiling store under the whole-station key"
        );
        assert!(
            cp.installed_tx_default_profile(0).await.is_none(),
            "a ceiling does not land in the TxDefaultProfile store"
        );
        assert!(
            cp.installed_tx_profile(0).await.is_none(),
            "a ceiling does not land in the TxProfile store"
        );
    }

    #[tokio::test]
    async fn v201_set_charging_profile_installs_an_external_constraints_ceiling() {
        // The second ceiling purpose routes to its own kind, not the Max kind.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let call = make_v201_set_charging_profile(
            1,
            v201_flat_profile(
                12,
                ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
                4_000.0,
            ),
        );
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetChargingProfileResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ChargingProfileStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_station_ceiling(CeilingKind::External, 1)
                .await
                .expect("stored")
                .id,
            12
        );
        assert!(
            cp.installed_station_ceiling(CeilingKind::Max, 1)
                .await
                .is_none(),
            "an external-constraints install does not populate the Max kind"
        );
    }

    #[tokio::test]
    async fn v201_get_composite_schedule_caps_the_txprofile_by_a_station_ceiling() {
        use ocpp_types::v201::ChargingRateUnitEnumType;
        // A TxProfile (11 kW) capped by a whole-station ceiling (6 kW): the composed
        // schedule reports min(11k, 6k) = 6 kW end to end (Issue #511).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_install_v201_tx_profile(
            1,
            v201_flat_profile(1, ChargingProfilePurposeEnumType::TxProfile, 11_000.0),
        )
        .await;
        cp.test_install_v201_station_ceiling(
            CeilingKind::Max,
            0,
            v201_flat_profile(
                9,
                ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
                6_000.0,
            ),
        )
        .await;
        let call = make_call(V201GetCompositeScheduleRequest {
            duration: 3_600,
            charging_rate_unit: None,
            evse_id: 1,
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetCompositeScheduleResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericStatusEnumType::Accepted);
                let schedule = body.schedule.expect("composed schedule");
                assert_eq!(schedule.charging_rate_unit, ChargingRateUnitEnumType::W);
                assert_eq!(
                    schedule.charging_schedule_period[0].limit, 6_000.0,
                    "the station ceiling caps the TxProfile limit"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_composite_schedule_falls_back_to_a_txdefaultprofile() {
        use ocpp_types::v201::ChargingRateUnitEnumType;
        // With no TxProfile in force on EVSE 1 but a TxDefaultProfile installed on
        // it, GetCompositeSchedule composes the *default* rather than rejecting —
        // the TxProfile > TxDefaultProfile fallback of Issue #471.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_install_v201_tx_default_profile(
            1,
            v201_flat_profile(7, ChargingProfilePurposeEnumType::TxDefaultProfile, 6_000.0),
        )
        .await;
        let call = make_call(V201GetCompositeScheduleRequest {
            duration: 3_600,
            charging_rate_unit: None,
            evse_id: 1,
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetCompositeScheduleResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericStatusEnumType::Accepted);
                let schedule = body.schedule.expect("composed from the default");
                assert_eq!(schedule.evse_id, 1);
                assert_eq!(schedule.charging_rate_unit, ChargingRateUnitEnumType::W);
                assert_eq!(schedule.charging_schedule_period[0].limit, 6_000.0);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_composite_schedule_txprofile_wins_over_a_default() {
        // Both a TxProfile (11 kW) and a station-wide TxDefaultProfile (6 kW) apply
        // to EVSE 1; the TxProfile takes precedence, so the composite reflects it.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_install_v201_tx_default_profile(
            0,
            v201_flat_profile(7, ChargingProfilePurposeEnumType::TxDefaultProfile, 6_000.0),
        )
        .await;
        cp.test_install_v201_tx_profile(
            1,
            v201_flat_profile(1, ChargingProfilePurposeEnumType::TxProfile, 11_000.0),
        )
        .await;
        let call = make_call(V201GetCompositeScheduleRequest {
            duration: 3_600,
            charging_rate_unit: None,
            evse_id: 1,
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetCompositeScheduleResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericStatusEnumType::Accepted);
                let schedule = body.schedule.expect("composed schedule");
                assert_eq!(
                    schedule.charging_schedule_period[0].limit, 11_000.0,
                    "the TxProfile limit wins over the default's"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_composite_schedule_uses_the_station_wide_default_wildcard() {
        // Only a station-wide (evseId = 0) TxDefaultProfile is installed; a query
        // for a concrete EVSE with no default of its own resolves the wildcard.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_install_v201_tx_default_profile(
            0,
            v201_flat_profile(7, ChargingProfilePurposeEnumType::TxDefaultProfile, 6_000.0),
        )
        .await;
        let call = make_call(V201GetCompositeScheduleRequest {
            duration: 3_600,
            charging_rate_unit: None,
            evse_id: 2,
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetCompositeScheduleResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    GenericStatusEnumType::Accepted,
                    "EVSE 2 with no default of its own resolves the evseId=0 wildcard"
                );
                assert_eq!(
                    body.schedule.expect("composed").charging_schedule_period[0].limit,
                    6_000.0
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    // --- ReserveNow (v201) over the wire -----------------------------------

    // Build a v201 `ReserveNow` CALL for `evse_id` (None = whole-station) whose
    // expiry is one hour out, so the reservation is valid and arms a future timer.
    fn make_v201_reserve_now(id: i32, evse_id: Option<i32>) -> CallMessage {
        make_call(V201ReserveNowRequest {
            id,
            expiry_date_time: (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
            id_token: V201IdTokenType {
                id_token: "DEADBEEF".to_string(),
                kind: ocpp_types::v201::IdTokenEnumType::Iso14443,
                additional_info: None,
                custom_data: None,
            },
            connector_type: None,
            evse_id,
            group_id_token: None,
            custom_data: None,
        })
    }

    async fn v201_connector_status(cp: &ChargePoint, evse_id: u32) -> ChargePointStatus {
        cp.get_connectors()
            .await
            .get(&ConnectorId::new(evse_id).unwrap())
            .expect("connector exists")
            .status()
            .await
    }

    #[tokio::test]
    async fn v201_reserve_now_accepts_a_free_evse_and_reports_reserved() {
        // A V201 CP now answers `ReserveNow` in its own dialect: a free EVSE is
        // reserved (→ Accepted) and the connector flips to `Reserved`, the state a
        // queued `StatusNotification(Reserved)` announces to the CSMS.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        assert_eq!(
            v201_connector_status(&cp, 1).await,
            ChargePointStatus::Available
        );

        let resp = cp
            .handle_message(Message::Call(make_v201_reserve_now(7, Some(1))))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::ReserveNowResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ReserveNowStatusEnumType::Accepted);
                assert!(body.status_info.is_none());
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            v201_connector_status(&cp, 1).await,
            ChargePointStatus::Reserved,
            "an accepted v201 reservation flips the connector to Reserved"
        );
    }

    #[tokio::test]
    async fn v201_reserve_now_reports_occupied_for_a_busy_evse() {
        // A connector mid-cycle (plugged in → Preparing) cannot be reserved →
        // Occupied, and its status is left untouched (not flipped to Reserved).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.plug_in(ConnectorId::new(1).unwrap()).await.unwrap();
        let busy = v201_connector_status(&cp, 1).await;
        assert_ne!(busy, ChargePointStatus::Available);

        let resp = cp
            .handle_message(Message::Call(make_v201_reserve_now(8, Some(1))))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::ReserveNowResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ReserveNowStatusEnumType::Occupied);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            v201_connector_status(&cp, 1).await,
            busy,
            "an Occupied reservation must not mutate the connector"
        );
    }

    #[tokio::test]
    async fn v201_reserve_now_rejects_an_unknown_evse() {
        // A structurally-valid but out-of-range evseId names no EVSE → Rejected,
        // never a panic (trust boundary on the inbound evseId).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_reserve_now(9, Some(99))))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::ReserveNowResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ReserveNowStatusEnumType::Rejected);
                assert_eq!(
                    body.status_info
                        .expect("a refusal carries a reason")
                        .reason_code,
                    "UnknownEvse"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_reserve_now_rejects_a_station_level_reservation() {
        // evseId omitted = reserve the whole station, which the flat simulator
        // does not hold → Rejected, and no connector is reserved.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_reserve_now(10, None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::ReserveNowResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ReserveNowStatusEnumType::Rejected);
                assert_eq!(
                    body.status_info
                        .expect("a refusal carries a reason")
                        .reason_code,
                    "StationLevel"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            v201_connector_status(&cp, 1).await,
            ChargePointStatus::Available,
            "a rejected station-level reservation reserves nothing"
        );
    }

    #[tokio::test]
    async fn v201_cancel_reservation_frees_a_held_reservation() {
        // A V201 CP holding a reservation on EVSE 1 (seeded as an accepted
        // ReserveNow would leave it) answers a 2.0.1 `CancelReservation` for that
        // id with `Accepted` and frees the connector (`Reserved → Available`).
        // Exercises the wired V201 arm end-to-end: a 2.0.1 CP now answers
        // CancelReservation in its own dialect rather than the 1.6J one.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let cid = ConnectorId::new(1).unwrap();
        cp.test_reserve(77, cid).await;
        assert_eq!(
            cp.test_connector_status(cid).await,
            ChargePointStatus::Reserved,
            "precondition: the seeded reservation holds the connector"
        );

        let call = make_call(V201CancelReservationRequest {
            reservation_id: 77,
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::CancelReservationResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::CancelReservationStatusEnumType::Accepted
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.test_connector_status(cid).await,
            ChargePointStatus::Available,
            "an accepted cancel frees the connector"
        );
    }

    #[tokio::test]
    async fn v201_cancel_reservation_rejects_unknown_id_and_leaves_state() {
        // Cancelling an id the station is not holding is `Rejected` and mutates
        // nothing: the genuinely-held reservation on EVSE 1 keeps its connector
        // `Reserved`.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let cid = ConnectorId::new(1).unwrap();
        cp.test_reserve(77, cid).await;

        let call = make_call(V201CancelReservationRequest {
            reservation_id: 88, // not the held id
            custom_data: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::CancelReservationResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::CancelReservationStatusEnumType::Rejected
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.test_connector_status(cid).await,
            ChargePointStatus::Reserved,
            "a rejected cancel leaves the held reservation untouched"
        );
    }

    // --- ReservationStatusUpdate (OCPP 2.0.1, #546) ------------------------
    // The CP→CSMS half that closes the reservation loop: a 2.0.1 station reports
    // an accepted CancelReservation as `Removed` and an auto-expiry as `Expired`.

    /// How many `ReservationStatusUpdate(status)` commands a drained command list
    /// carries for `reservation_id` — the assertion primitive these tests share.
    fn count_reservation_status_updates(
        commands: &[RemoteCommand],
        reservation_id: i32,
        status: ReservationUpdateStatusEnumType,
    ) -> usize {
        commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    RemoteCommand::V201ReservationStatusUpdate { reservation_id: id, status: s }
                        if *id == reservation_id && *s == status
                )
            })
            .count()
    }

    #[tokio::test]
    async fn v201_cancel_reservation_queues_removed_status_update() {
        // An accepted 2.0.1 CancelReservation of a still-held reservation queues
        // exactly one `ReservationStatusUpdate(Removed)` correlated by
        // reservationId — the CP→CSMS report that the slot is free again.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let cid = ConnectorId::new(1).unwrap();
        cp.test_reserve(77, cid).await;

        let call = make_call(V201CancelReservationRequest {
            reservation_id: 77,
            custom_data: None,
        });
        cp.handle_message(Message::Call(call)).await.unwrap();

        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            count_reservation_status_updates(
                &commands,
                77,
                ReservationUpdateStatusEnumType::Removed
            ),
            1,
            "an accepted cancel queues exactly one Removed update, got: {commands:?}"
        );
        // And none reported Expired — a CSMS teardown is Removed, not an expiry.
        assert_eq!(
            count_reservation_status_updates(
                &commands,
                77,
                ReservationUpdateStatusEnumType::Expired
            ),
            0,
        );
    }

    #[tokio::test]
    async fn v201_cancel_reservation_unknown_id_queues_no_status_update() {
        // Cancelling an id the station is not holding is Rejected and queues no
        // ReservationStatusUpdate — there is no reservation whose removal to report.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let cid = ConnectorId::new(1).unwrap();
        cp.test_reserve(77, cid).await;

        let call = make_call(V201CancelReservationRequest {
            reservation_id: 88, // not the held id
            custom_data: None,
        });
        cp.handle_message(Message::Call(call)).await.unwrap();

        let commands = v201_drain_commands(&cp).await;
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, RemoteCommand::V201ReservationStatusUpdate { .. })),
            "a rejected cancel queues no ReservationStatusUpdate, got: {commands:?}"
        );
    }

    #[tokio::test]
    async fn v201_reservation_expiry_queues_expired_status_update() {
        // When the auto-expiry timer fires on a V201 station it frees the
        // connector AND reports `Expired` to the CSMS. Armed with a past
        // expiryDate (ttl 0) and driven to completion by awaiting the spawned
        // timer handle, so the assertion is deterministic — no wall-clock wait.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let cid = ConnectorId::new(1).unwrap();
        cp.test_reserve(77, cid).await;

        let past = chrono::Utc::now() - chrono::Duration::seconds(1);
        ChargePoint::arm_reservation_expiry(
            77,
            cid,
            past,
            &cp.connectors,
            &cp.reservations,
            &cp.expiry_timers,
            &cp.command_sender,
            OcppVersion::V201,
        )
        .await;
        let handle = cp
            .expiry_timers
            .write()
            .await
            .remove(&77)
            .expect("the expiry timer was armed");
        handle.await.expect("the expiry task runs to completion");

        assert_eq!(
            cp.test_connector_status(cid).await,
            ChargePointStatus::Available,
            "an expiry frees the connector"
        );
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            count_reservation_status_updates(
                &commands,
                77,
                ReservationUpdateStatusEnumType::Expired
            ),
            1,
            "an expiry queues exactly one Expired update, got: {commands:?}"
        );
    }

    #[tokio::test]
    async fn v16_reservation_expiry_queues_no_status_update() {
        // ReservationStatusUpdate does not exist in 1.6J: an auto-expiry on a
        // 1.6J station frees the connector (EmitConnectorStatus) but never queues
        // a ReservationStatusUpdate — the version gate holds even though the
        // expiry machinery is shared between the two dialects.
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let cid = ConnectorId::new(1).unwrap();
        cp.test_reserve(55, cid).await;

        let past = chrono::Utc::now() - chrono::Duration::seconds(1);
        ChargePoint::arm_reservation_expiry(
            55,
            cid,
            past,
            &cp.connectors,
            &cp.reservations,
            &cp.expiry_timers,
            &cp.command_sender,
            OcppVersion::V16J,
        )
        .await;
        let handle = cp
            .expiry_timers
            .write()
            .await
            .remove(&55)
            .expect("the expiry timer was armed");
        handle.await.expect("the expiry task runs to completion");

        let commands = v201_drain_commands(&cp).await;
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, RemoteCommand::EmitConnectorStatus { .. })),
            "the 1.6J expiry still frees the connector, got: {commands:?}"
        );
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, RemoteCommand::V201ReservationStatusUpdate { .. })),
            "a 1.6J expiry queues no ReservationStatusUpdate, got: {commands:?}"
        );
    }

    #[tokio::test]
    async fn v201_cancel_tears_down_expiry_timer_so_it_cannot_double_report() {
        // A cancel of a held reservation both reports Removed and disarms the
        // pending auto-expiry timer, so the same reservation can never also be
        // reported Expired later. Arm a far-future timer (it must not fire during
        // the test), cancel, and assert: exactly one Removed, no Expired, and the
        // timer is gone from the store.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let cid = ConnectorId::new(1).unwrap();
        cp.test_reserve(77, cid).await;
        let far_future = chrono::Utc::now() + chrono::Duration::hours(1);
        ChargePoint::arm_reservation_expiry(
            77,
            cid,
            far_future,
            &cp.connectors,
            &cp.reservations,
            &cp.expiry_timers,
            &cp.command_sender,
            OcppVersion::V201,
        )
        .await;
        assert!(
            cp.expiry_timers.read().await.contains_key(&77),
            "precondition: the expiry timer is armed"
        );

        let call = make_call(V201CancelReservationRequest {
            reservation_id: 77,
            custom_data: None,
        });
        cp.handle_message(Message::Call(call)).await.unwrap();

        assert!(
            !cp.expiry_timers.read().await.contains_key(&77),
            "the cancel disarms the auto-expiry timer"
        );
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            count_reservation_status_updates(
                &commands,
                77,
                ReservationUpdateStatusEnumType::Removed
            ),
            1,
            "the cancel reports Removed exactly once, got: {commands:?}"
        );
        assert_eq!(
            count_reservation_status_updates(
                &commands,
                77,
                ReservationUpdateStatusEnumType::Expired
            ),
            0,
            "the disarmed timer never reports Expired"
        );
    }

    // --- GetReport (OCPP 2.0.1, #486) --------------------------------------

    fn make_v201_get_report(
        request_id: i32,
        component_variable: Option<Vec<ocpp_types::v201::ComponentVariableType>>,
        component_criteria: Option<Vec<ocpp_types::v201::ComponentCriterionEnumType>>,
    ) -> CallMessage {
        make_call(V201GetReportRequest {
            component_variable,
            request_id,
            component_criteria,
            custom_data: None,
        })
    }

    /// A single-variable `componentVariable` filter entry.
    fn v201_component_variable(
        component: &str,
        variable: &str,
    ) -> ocpp_types::v201::ComponentVariableType {
        ocpp_types::v201::ComponentVariableType {
            component: ocpp_types::v201::ComponentType {
                name: component.to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: Some(ocpp_types::v201::VariableType {
                name: variable.to_string(),
                instance: None,
                custom_data: None,
            }),
            custom_data: None,
        }
    }

    /// Drain whatever the inbound-CALL handlers queued on the command channel.
    /// Takes the (single) receiver the way `connect()` would — fine in a test
    /// that never connects — and non-blockingly pulls every ready command.
    async fn v201_drain_commands(cp: &ChargePoint) -> Vec<RemoteCommand> {
        let mut rx = cp
            .command_receiver
            .write()
            .await
            .take()
            .expect("a freshly-built CP still owns its command receiver");
        let mut commands = Vec::new();
        while let Ok(command) = rx.try_recv() {
            commands.push(command);
        }
        commands
    }

    #[tokio::test]
    async fn v201_get_report_accepts_and_queues_a_notify_report() {
        // An unfiltered GetReport on a V201 CP is Accepted and queues exactly one
        // NotifyReport carrying the (non-empty) inventory, correlated by
        // requestId — the same two-part seam as GetBaseReport.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_report(501, None, None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetReportResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericDeviceModelStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }

        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "an accepted GetReport queues one NotifyReport"
        );
        match &commands[0] {
            RemoteCommand::V201NotifyReport {
                request_id,
                report_data,
            } => {
                assert_eq!(*request_id, 501, "the report is correlated by requestId");
                assert!(
                    !report_data.is_empty(),
                    "the queued report carries the inventory"
                );
            }
            other => panic!("expected a queued V201NotifyReport, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_report_empty_result_set_queues_nothing() {
        // componentCriteria = [Problem] matches nothing on a healthy simulator →
        // EmptyResultSet, and no NotifyReport is queued (nothing to stream).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_report(
                502,
                None,
                Some(vec![ocpp_types::v201::ComponentCriterionEnumType::Problem]),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetReportResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    GenericDeviceModelStatusEnumType::EmptyResultSet
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "an EmptyResultSet GetReport queues no NotifyReport"
        );
    }

    #[tokio::test]
    async fn v201_get_report_narrows_to_the_requested_variable() {
        // A componentVariable filter narrows the queued report to exactly the
        // requested component-variable row.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let filter = vec![v201_component_variable(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
        )];
        let resp = cp
            .handle_message(Message::Call(make_v201_get_report(503, Some(filter), None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetReportResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericDeviceModelStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }

        let commands = v201_drain_commands(&cp).await;
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            RemoteCommand::V201NotifyReport { report_data, .. } => {
                assert_eq!(report_data.len(), 1, "the filter selects a single row");
                assert_eq!(report_data[0].component.name, "OCPPCommCtrlr");
                assert_eq!(report_data[0].variable.name, "HeartbeatInterval");
            }
            other => panic!("expected a queued V201NotifyReport, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_report_rejects_when_the_consumer_is_gone() {
        // If the command consumer has gone away (CP shutting down), an otherwise
        // non-empty report cannot be streamed, so the station answers Rejected
        // rather than promise a NotifyReport that never arrives.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // Drop the single receiver, closing the channel so every send() fails.
        drop(cp.command_receiver.write().await.take());

        let resp = cp
            .handle_message(Message::Call(make_v201_get_report(504, None, None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: V201GetReportResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericDeviceModelStatusEnumType::Rejected);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[test]
    fn v201_get_report_payloads_are_schema_valid() {
        // Draft-06 schema validity of the request (with both filters) and every
        // response status, independent of the dispatcher's own validation.
        let validator = SchemaValidator::v201();
        let req = V201GetReportRequest {
            component_variable: Some(vec![v201_component_variable(
                "OCPPCommCtrlr",
                "HeartbeatInterval",
            )]),
            request_id: 7,
            component_criteria: Some(vec![
                ocpp_types::v201::ComponentCriterionEnumType::Active,
                ocpp_types::v201::ComponentCriterionEnumType::Available,
            ]),
            custom_data: None,
        };
        validator
            .validate_call("GetReport", &serde_json::to_value(&req).unwrap())
            .expect("GetReport request is schema-valid");

        for status in [
            GenericDeviceModelStatusEnumType::Accepted,
            GenericDeviceModelStatusEnumType::Rejected,
            GenericDeviceModelStatusEnumType::NotSupported,
            GenericDeviceModelStatusEnumType::EmptyResultSet,
        ] {
            let resp = V201GetReportResponse {
                status,
                status_info: Some(StatusInfoType {
                    reason_code: "NoData".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
                custom_data: None,
            };
            validator
                .validate_call_result("GetReport", &serde_json::to_value(&resp).unwrap())
                .expect("GetReport response is schema-valid");
        }
    }

    // --- GetMonitoringReport (OCPP 2.0.1, #493) ----------------------------

    fn make_v201_get_monitoring_report(
        request_id: i32,
        component_variable: Option<Vec<ocpp_types::v201::ComponentVariableType>>,
        monitoring_criteria: Option<Vec<ocpp_types::v201::MonitoringCriterionEnumType>>,
    ) -> CallMessage {
        make_call(V201GetMonitoringReportRequest {
            component_variable,
            request_id,
            monitoring_criteria,
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_get_monitoring_report_registered_and_empty_result_set_queues_nothing() {
        // A V201 CP registers a GetMonitoringReport handler (the response parses,
        // it is not an unrecognized action). The simulator installs no monitors
        // yet (issue #493, option b), so an unfiltered request → EmptyResultSet
        // and no NotifyMonitoringReport is queued — nothing to stream.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_monitoring_report(
                601, None, None,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetMonitoringReportResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    GenericDeviceModelStatusEnumType::EmptyResultSet
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "an EmptyResultSet GetMonitoringReport queues no NotifyMonitoringReport"
        );
    }

    #[tokio::test]
    async fn v201_get_monitoring_report_filters_do_not_panic_and_stay_empty() {
        // Trust boundary: a request narrowed by both a componentVariable filter
        // and a monitoringCriteria filter is handled without panic and still
        // resolves to EmptyResultSet on the monitor-less simulator.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_monitoring_report(
                602,
                Some(vec![v201_component_variable("EVSE", "Temperature")]),
                Some(vec![
                    ocpp_types::v201::MonitoringCriterionEnumType::ThresholdMonitoring,
                    ocpp_types::v201::MonitoringCriterionEnumType::PeriodicMonitoring,
                ]),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetMonitoringReportResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    GenericDeviceModelStatusEnumType::EmptyResultSet
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[test]
    fn v201_get_monitoring_report_payloads_are_schema_valid() {
        // Draft-06 schema validity of the request (with both filters) and every
        // response status, independent of the dispatcher's own validation.
        let validator = SchemaValidator::v201();
        let req = V201GetMonitoringReportRequest {
            component_variable: Some(vec![v201_component_variable("EVSE", "Temperature")]),
            request_id: 7,
            monitoring_criteria: Some(vec![
                ocpp_types::v201::MonitoringCriterionEnumType::ThresholdMonitoring,
                ocpp_types::v201::MonitoringCriterionEnumType::DeltaMonitoring,
            ]),
            custom_data: None,
        };
        validator
            .validate_call("GetMonitoringReport", &serde_json::to_value(&req).unwrap())
            .expect("GetMonitoringReport request is schema-valid");

        for status in [
            GenericDeviceModelStatusEnumType::Accepted,
            GenericDeviceModelStatusEnumType::Rejected,
            GenericDeviceModelStatusEnumType::NotSupported,
            GenericDeviceModelStatusEnumType::EmptyResultSet,
        ] {
            let resp = ocpp_messages::v201::GetMonitoringReportResponse {
                status,
                status_info: Some(StatusInfoType {
                    reason_code: "NoMonitors".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
                custom_data: None,
            };
            validator
                .validate_call_result("GetMonitoringReport", &serde_json::to_value(&resp).unwrap())
                .expect("GetMonitoringReport response is schema-valid");
        }
    }

    // --- SetVariableMonitoring (OCPP 2.0.1, #494) --------------------------
    // A `for_version(V201)` CP installs variable monitors into the same
    // `V201DeviceModel` the read/report seams share; a subsequent
    // GetMonitoringReport then streams them. These exercise the wired V201 arm
    // end-to-end over `handle_message`.

    fn make_v201_set_variable_monitoring(
        data: Vec<ocpp_types::v201::SetMonitoringDataType>,
    ) -> CallMessage {
        make_call(V201SetVariableMonitoringRequest {
            set_monitoring_data: data,
            custom_data: None,
        })
    }

    fn v201_monitor_data(
        component: &str,
        variable: &str,
        kind: ocpp_types::v201::MonitorEnumType,
        value: f64,
        severity: i32,
    ) -> ocpp_types::v201::SetMonitoringDataType {
        ocpp_types::v201::SetMonitoringDataType {
            id: None,
            transaction: None,
            value,
            kind,
            severity,
            component: ocpp_types::v201::ComponentType {
                name: component.to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: ocpp_types::v201::VariableType {
                name: variable.to_string(),
                instance: None,
                custom_data: None,
            },
            custom_data: None,
        }
    }

    #[tokio::test]
    async fn v201_set_variable_monitoring_installs_and_is_accepted() {
        use ocpp_types::v201::{MonitorEnumType, SetMonitoringStatusEnumType};
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_variable_monitoring(vec![
                v201_monitor_data(
                    "OCPPCommCtrlr",
                    "HeartbeatInterval",
                    MonitorEnumType::UpperThreshold,
                    600.0,
                    5,
                ),
            ])))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetVariableMonitoringResponse =
                    r.payload_as().unwrap();
                assert_eq!(body.set_monitoring_result.len(), 1);
                assert_eq!(
                    body.set_monitoring_result[0].status,
                    SetMonitoringStatusEnumType::Accepted
                );
                assert_eq!(body.set_monitoring_result[0].id, Some(1));
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // A pure install queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_set_variable_monitoring_then_get_monitoring_report_streams_the_monitor() {
        use ocpp_types::v201::MonitorEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // Install a monitor.
        cp.handle_message(Message::Call(make_v201_set_variable_monitoring(vec![
            v201_monitor_data(
                "OCPPCommCtrlr",
                "HeartbeatInterval",
                MonitorEnumType::UpperThreshold,
                600.0,
                5,
            ),
        ])))
        .await
        .unwrap();
        // Now GetMonitoringReport finds it → Accepted, and queues one
        // NotifyMonitoringReport carrying the installed monitor.
        let resp = cp
            .handle_message(Message::Call(make_v201_get_monitoring_report(
                701, None, None,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetMonitoringReportResponse =
                    r.payload_as().unwrap();
                assert_eq!(body.status, GenericDeviceModelStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(commands.len(), 1, "one NotifyMonitoringReport is queued");
        match &commands[0] {
            RemoteCommand::V201NotifyMonitoringReport {
                request_id,
                monitor_data,
            } => {
                assert_eq!(*request_id, 701);
                assert_eq!(monitor_data.len(), 1);
                assert_eq!(monitor_data[0].component.name, "OCPPCommCtrlr");
                assert_eq!(monitor_data[0].variable.name, "HeartbeatInterval");
                assert_eq!(monitor_data[0].variable_monitoring.len(), 1);
                assert_eq!(monitor_data[0].variable_monitoring[0].id, 1);
            }
            other => panic!("expected V201NotifyMonitoringReport, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_set_variable_monitoring_rejects_unknown_variable() {
        use ocpp_types::v201::{MonitorEnumType, SetMonitoringStatusEnumType};
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_variable_monitoring(vec![
                v201_monitor_data(
                    "OCPPCommCtrlr",
                    "NoSuchVariable",
                    MonitorEnumType::Delta,
                    1.0,
                    3,
                ),
            ])))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetVariableMonitoringResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.set_monitoring_result[0].status,
                    SetMonitoringStatusEnumType::UnknownVariable
                );
                assert_eq!(body.set_monitoring_result[0].id, None);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[test]
    fn v201_set_variable_monitoring_response_is_schema_valid() {
        use ocpp_types::v201::{
            ComponentType, MonitorEnumType, SetMonitoringResultType, SetMonitoringStatusEnumType,
            StatusInfoType, VariableType,
        };
        let validator = SchemaValidator::v201();
        // A request with one monitor is schema-valid.
        let req = V201SetVariableMonitoringRequest {
            set_monitoring_data: vec![v201_monitor_data(
                "OCPPCommCtrlr",
                "HeartbeatInterval",
                MonitorEnumType::Periodic,
                900.0,
                5,
            )],
            custom_data: None,
        };
        validator
            .validate_call(
                "SetVariableMonitoring",
                &serde_json::to_value(&req).unwrap(),
            )
            .expect("SetVariableMonitoring request is schema-valid");

        // Both an accepted (with id) and a rejected (with statusInfo, no id)
        // per-monitor result serialize to schema-valid responses.
        for (status, id, status_info) in [
            (SetMonitoringStatusEnumType::Accepted, Some(1), None),
            (
                SetMonitoringStatusEnumType::UnknownVariable,
                None,
                Some(StatusInfoType {
                    reason_code: "UnknownVariable".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::SetVariableMonitoringResponse {
                set_monitoring_result: vec![SetMonitoringResultType {
                    id,
                    status,
                    kind: MonitorEnumType::Periodic,
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
                    severity: 5,
                    status_info,
                    custom_data: None,
                }],
                custom_data: None,
            };
            validator
                .validate_call_result(
                    "SetVariableMonitoring",
                    &serde_json::to_value(&resp).unwrap(),
                )
                .expect("SetVariableMonitoring response is schema-valid");
        }
    }

    #[test]
    fn v201_notify_monitoring_report_with_installed_monitors_is_schema_valid() {
        // The snapshot produced by install → GetMonitoringReport is streamed as a
        // NotifyMonitoringReport; a populated monitor list (built exactly as the
        // send path builds it) must be OCPP 2.0.1 schema-valid, not just the empty
        // case #493 covered.
        use ocpp_types::v201::MonitorEnumType;
        let mut model = V201DeviceModel::with_standard_profile();
        model.install_monitors(&[v201_monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )]);
        let monitor_data = model.monitoring_snapshot(None, None);
        assert_eq!(monitor_data.len(), 1);

        let request = V201NotifyMonitoringReportRequest {
            monitor: Some(monitor_data),
            request_id: 701,
            tbc: None,
            seq_no: 0,
            generated_at: "2026-08-11T00:00:00Z".to_string(),
            custom_data: None,
        };
        SchemaValidator::v201()
            .validate_call(
                "NotifyMonitoringReport",
                &serde_json::to_value(&request).unwrap(),
            )
            .expect("NotifyMonitoringReport with installed monitors is schema-valid");
    }

    // --- ClearVariableMonitoring (OCPP 2.0.1, #497) ------------------------
    // The teardown counterpart to SetVariableMonitoring: a `for_version(V201)`
    // CP removes monitors by id from the same shared `V201DeviceModel`, so a
    // subsequent GetMonitoringReport no longer streams them. These exercise the
    // wired V201 arm end-to-end over `handle_message`.

    fn make_v201_clear_variable_monitoring(ids: Vec<i32>) -> CallMessage {
        make_call(V201ClearVariableMonitoringRequest {
            id: ids,
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_clear_variable_monitoring_removes_an_installed_monitor() {
        use ocpp_types::v201::{
            ClearMonitoringStatusEnumType, GenericDeviceModelStatusEnumType, MonitorEnumType,
        };
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // Install a monitor (assigned id 1).
        cp.handle_message(Message::Call(make_v201_set_variable_monitoring(vec![
            v201_monitor_data(
                "OCPPCommCtrlr",
                "HeartbeatInterval",
                MonitorEnumType::UpperThreshold,
                600.0,
                5,
            ),
        ])))
        .await
        .unwrap();

        // Clear it by id → Accepted, and nothing is queued (pure read-and-remove).
        let resp = cp
            .handle_message(Message::Call(make_v201_clear_variable_monitoring(vec![1])))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::ClearVariableMonitoringResponse =
                    r.payload_as().unwrap();
                assert_eq!(body.clear_monitoring_result.len(), 1);
                assert_eq!(
                    body.clear_monitoring_result[0].status,
                    ClearMonitoringStatusEnumType::Accepted
                );
                assert_eq!(body.clear_monitoring_result[0].id, 1);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }

        // The cleared monitor is gone: GetMonitoringReport is now EmptyResultSet
        // and streams no NotifyMonitoringReport.
        let resp = cp
            .handle_message(Message::Call(make_v201_get_monitoring_report(
                702, None, None,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetMonitoringReportResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    GenericDeviceModelStatusEnumType::EmptyResultSet
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // A single drain (the receiver is take-once) proves neither the clear nor
        // the empty GetMonitoringReport queued any side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_clear_variable_monitoring_unknown_id_is_not_found() {
        use ocpp_types::v201::ClearMonitoringStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // No monitors installed — an unknown id is NotFound, never a panic.
        let resp = cp
            .handle_message(Message::Call(make_v201_clear_variable_monitoring(vec![
                999,
            ])))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::ClearVariableMonitoringResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.clear_monitoring_result[0].status,
                    ClearMonitoringStatusEnumType::NotFound
                );
                assert_eq!(body.clear_monitoring_result[0].id, 999);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[test]
    fn v201_clear_variable_monitoring_response_is_schema_valid() {
        use ocpp_types::v201::{
            ClearMonitoringResultType, ClearMonitoringStatusEnumType, StatusInfoType,
        };
        let validator = SchemaValidator::v201();
        // A request carrying one id is schema-valid.
        let req = V201ClearVariableMonitoringRequest {
            id: vec![1, 2],
            custom_data: None,
        };
        validator
            .validate_call(
                "ClearVariableMonitoring",
                &serde_json::to_value(&req).unwrap(),
            )
            .expect("ClearVariableMonitoring request is schema-valid");

        // Each per-id result status (incl. a Rejected with statusInfo) serializes
        // to a schema-valid response.
        for (status, status_info) in [
            (ClearMonitoringStatusEnumType::Accepted, None),
            (ClearMonitoringStatusEnumType::NotFound, None),
            (
                ClearMonitoringStatusEnumType::Rejected,
                Some(StatusInfoType {
                    reason_code: "InUse".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::ClearVariableMonitoringResponse {
                clear_monitoring_result: vec![ClearMonitoringResultType {
                    status,
                    id: 1,
                    status_info,
                    custom_data: None,
                }],
                custom_data: None,
            };
            validator
                .validate_call_result(
                    "ClearVariableMonitoring",
                    &serde_json::to_value(&resp).unwrap(),
                )
                .expect("ClearVariableMonitoring response is schema-valid");
        }
    }

    // --- OCPP 2.0.1 SetMonitoringLevel (M7, issue #500) --------------------
    // A `for_version(V201)` CP sets its reporting severity threshold on the same
    // shared `V201DeviceModel` the monitoring family uses. These exercise the
    // wired V201 arm end-to-end over `handle_message`.

    fn make_v201_set_monitoring_level(severity: i32) -> CallMessage {
        make_call(V201SetMonitoringLevelRequest {
            severity,
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_set_monitoring_level_in_range_is_accepted() {
        use ocpp_types::v201::GenericStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_monitoring_level(5)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetMonitoringLevelResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GenericStatusEnumType::Accepted);
                // Accepted carries no statusInfo.
                assert!(body.status_info.is_none());
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // A pure set queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_set_monitoring_level_boundaries_are_accepted() {
        use ocpp_types::v201::GenericStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // Both inclusive bounds (0 = Danger, 9 = Debug) are accepted.
        for severity in [0, 9] {
            let resp = cp
                .handle_message(Message::Call(make_v201_set_monitoring_level(severity)))
                .await
                .unwrap();
            match resp.unwrap() {
                Message::CallResult(r) => {
                    let body: ocpp_messages::v201::SetMonitoringLevelResponse =
                        r.payload_as().unwrap();
                    assert_eq!(
                        body.status,
                        GenericStatusEnumType::Accepted,
                        "severity {severity} is in range and must be Accepted",
                    );
                }
                other => panic!("expected CallResult, got: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn v201_set_monitoring_level_out_of_range_is_rejected() {
        use ocpp_types::v201::GenericStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // Out-of-range severities (below 0, above 9, and the i32 extremes) are
        // Rejected with a populated statusInfo — never a panic on wire input.
        for severity in [-1, 10, i32::MIN, i32::MAX] {
            let resp = cp
                .handle_message(Message::Call(make_v201_set_monitoring_level(severity)))
                .await
                .unwrap();
            match resp.unwrap() {
                Message::CallResult(r) => {
                    let body: ocpp_messages::v201::SetMonitoringLevelResponse =
                        r.payload_as().unwrap();
                    assert_eq!(
                        body.status,
                        GenericStatusEnumType::Rejected,
                        "severity {severity} is out of range and must be Rejected",
                    );
                    let info = body
                        .status_info
                        .expect("a rejection carries a statusInfo reason");
                    assert_eq!(info.reason_code, "OutOfRange");
                }
                other => panic!("expected CallResult, got: {other:?}"),
            }
        }
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[test]
    fn v201_set_monitoring_level_request_and_response_are_schema_valid() {
        use ocpp_types::v201::{GenericStatusEnumType, StatusInfoType};
        let validator = SchemaValidator::v201();
        // A request carrying a severity is schema-valid.
        let req = V201SetMonitoringLevelRequest {
            severity: 3,
            custom_data: None,
        };
        validator
            .validate_call("SetMonitoringLevel", &serde_json::to_value(&req).unwrap())
            .expect("SetMonitoringLevel request is schema-valid");

        // Both answer shapes — Accepted (no statusInfo) and Rejected (with the
        // OutOfRange statusInfo the handler emits) — serialize to a schema-valid
        // response.
        for (status, status_info) in [
            (GenericStatusEnumType::Accepted, None),
            (
                GenericStatusEnumType::Rejected,
                Some(StatusInfoType {
                    reason_code: "OutOfRange".to_string(),
                    additional_info: Some("severity must be in 0..=9".to_string()),
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::SetMonitoringLevelResponse {
                status,
                status_info,
                custom_data: None,
            };
            validator
                .validate_call_result("SetMonitoringLevel", &serde_json::to_value(&resp).unwrap())
                .expect("SetMonitoringLevel response is schema-valid");
        }
    }

    // --- OCPP 2.0.1 SetDisplayMessage (M7, issue #505) ---------------------
    // A `for_version(V201)` CP installs a display message into its
    // `V201DisplayMessageStore`. These exercise the wired V201 arm end-to-end
    // over `handle_message`, including the upsert-by-id and the
    // UnknownTransaction refusal against the live `active_transactions` set.

    fn make_v201_display_message(id: i32, transaction_id: Option<&str>) -> MessageInfoType {
        use ocpp_types::v201::{
            MessageContentType, MessageFormatEnumType, MessagePriorityEnumType,
        };
        MessageInfoType {
            id,
            priority: MessagePriorityEnumType::NormalCycle,
            message: MessageContentType {
                format: MessageFormatEnumType::Utf8,
                content: format!("message-{id}"),
                language: None,
                custom_data: None,
            },
            state: None,
            start_date_time: None,
            end_date_time: None,
            transaction_id: transaction_id.map(ToString::to_string),
            display: None,
            custom_data: None,
        }
    }

    fn make_v201_set_display_message(message: MessageInfoType) -> CallMessage {
        make_call(V201SetDisplayMessageRequest {
            message,
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_set_display_message_station_wide_is_accepted_and_stored() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_display_message(
                make_v201_display_message(1, None),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetDisplayMessageResponse = r.payload_as().unwrap();
                assert_eq!(body.status, DisplayMessageStatusEnumType::Accepted);
                // Accepted carries no statusInfo.
                assert!(body.status_info.is_none());
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // The accepted message is observable in the store, read back by id.
        assert_eq!(
            cp.installed_display_message(1)
                .await
                .map(|m| m.message.content),
            Some("message-1".to_string()),
        );
        // A pure install queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_set_display_message_same_id_upserts() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.handle_message(Message::Call(make_v201_set_display_message(
            make_v201_display_message(2, None),
        )))
        .await
        .unwrap();

        // Re-install a different message under the SAME id.
        let mut replacement = make_v201_display_message(2, None);
        replacement.message.content = "replaced".to_string();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_display_message(replacement)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetDisplayMessageResponse = r.payload_as().unwrap();
                assert_eq!(body.status, DisplayMessageStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_display_message(2)
                .await
                .map(|m| m.message.content),
            Some("replaced".to_string()),
            "a same-id re-install replaces the stored message rather than duplicating it"
        );
    }

    #[tokio::test]
    async fn v201_set_display_message_bound_to_unknown_transaction_is_refused_and_not_stored() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // The idle station is running no transactions, so a message bound to any
        // transactionId is UnknownTransaction with a populated statusInfo, and is
        // not installed — never a panic on the CSMS-supplied id.
        let resp = cp
            .handle_message(Message::Call(make_v201_set_display_message(
                make_v201_display_message(3, Some("999")),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetDisplayMessageResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    DisplayMessageStatusEnumType::UnknownTransaction
                );
                assert_eq!(
                    body.status_info
                        .expect("a refusal carries a statusInfo reason")
                        .reason_code,
                    "NoTransaction"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_display_message(3).await,
            None,
            "a refused message is not stored"
        );
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[test]
    fn v201_set_display_message_request_and_response_are_schema_valid() {
        use ocpp_types::v201::StatusInfoType;
        let validator = SchemaValidator::v201();
        // A request carrying a MessageInfoType is schema-valid.
        let req = V201SetDisplayMessageRequest {
            message: make_v201_display_message(1, None),
            custom_data: None,
        };
        validator
            .validate_call("SetDisplayMessage", &serde_json::to_value(&req).unwrap())
            .expect("SetDisplayMessage request is schema-valid");

        // Both answer shapes the handler emits — Accepted (no statusInfo) and
        // UnknownTransaction (with the NoTransaction statusInfo) — serialize to a
        // schema-valid response.
        for (status, status_info) in [
            (DisplayMessageStatusEnumType::Accepted, None),
            (
                DisplayMessageStatusEnumType::UnknownTransaction,
                Some(StatusInfoType {
                    reason_code: "NoTransaction".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::SetDisplayMessageResponse {
                status,
                status_info,
                custom_data: None,
            };
            validator
                .validate_call_result("SetDisplayMessage", &serde_json::to_value(&resp).unwrap())
                .expect("SetDisplayMessage response is schema-valid");
        }
    }

    // --- OCPP 2.0.1 InstallCertificate (M7, issue #518) --------------------
    // A `for_version(V201)` CP installs a root/CA certificate into its
    // `V201CertificateStore`. These exercise the wired V201 arm end-to-end over
    // `handle_message`: accept-and-store, per-use isolation, same-use rotation,
    // the reject/fail non-store paths, and V201-only registration.

    // A minimal but structurally-valid PEM certificate (armor + one body line).
    const SAMPLE_INSTALL_PEM: &str =
        "-----BEGIN CERTIFICATE-----\nMIIBkTCB+w==\n-----END CERTIFICATE-----";

    fn make_v201_install_certificate(
        certificate_type: InstallCertificateUseEnumType,
        certificate: &str,
    ) -> CallMessage {
        make_call(V201InstallCertificateRequest {
            certificate_type,
            certificate: certificate.to_string(),
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_install_certificate_accepts_and_stores_a_pem() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_install_certificate(
                InstallCertificateUseEnumType::CSMSRootCertificate,
                SAMPLE_INSTALL_PEM,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::InstallCertificateResponse = r.payload_as().unwrap();
                assert_eq!(body.status, InstallCertificateStatusEnumType::Accepted);
                // Accepted carries no statusInfo.
                assert!(body.status_info.is_none());
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // The accepted anchor is observable in the store, read back by use.
        assert_eq!(
            cp.installed_certificate(InstallCertificateUseEnumType::CSMSRootCertificate)
                .await,
            Some(SAMPLE_INSTALL_PEM.to_string()),
        );
        // A pure install queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_install_certificate_second_use_installs_independently() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.handle_message(Message::Call(make_v201_install_certificate(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            SAMPLE_INSTALL_PEM,
        )))
        .await
        .unwrap();

        // A second anchor under a DIFFERENT use, with distinct body content.
        let v2g_pem = "-----BEGIN CERTIFICATE-----\nQzJHUk9PVA==\n-----END CERTIFICATE-----";
        let resp = cp
            .handle_message(Message::Call(make_v201_install_certificate(
                InstallCertificateUseEnumType::V2GRootCertificate,
                v2g_pem,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::InstallCertificateResponse = r.payload_as().unwrap();
                assert_eq!(body.status, InstallCertificateStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // Both anchors hold their own PEM; the first was not disturbed.
        assert_eq!(
            cp.installed_certificate(InstallCertificateUseEnumType::CSMSRootCertificate)
                .await,
            Some(SAMPLE_INSTALL_PEM.to_string()),
        );
        assert_eq!(
            cp.installed_certificate(InstallCertificateUseEnumType::V2GRootCertificate)
                .await,
            Some(v2g_pem.to_string()),
        );
    }

    #[tokio::test]
    async fn v201_install_certificate_same_use_rotates_the_anchor() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.handle_message(Message::Call(make_v201_install_certificate(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            SAMPLE_INSTALL_PEM,
        )))
        .await
        .unwrap();

        // Re-install a DIFFERENT certificate under the SAME use (a root rotation).
        let rotated = "-----BEGIN CERTIFICATE-----\nUk9UQVRFRA==\n-----END CERTIFICATE-----";
        let resp = cp
            .handle_message(Message::Call(make_v201_install_certificate(
                InstallCertificateUseEnumType::CSMSRootCertificate,
                rotated,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::InstallCertificateResponse = r.payload_as().unwrap();
                assert_eq!(body.status, InstallCertificateStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_certificate(InstallCertificateUseEnumType::CSMSRootCertificate)
                .await,
            Some(rotated.to_string()),
            "a same-use re-install rotates the anchor in place, no duplicate"
        );
    }

    #[tokio::test]
    async fn v201_install_certificate_rejects_empty_and_stores_nothing() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // An empty certificate is refused up front; nothing is installed, and the
        // CSMS-supplied string never panics the handler.
        let resp = cp
            .handle_message(Message::Call(make_v201_install_certificate(
                InstallCertificateUseEnumType::ManufacturerRootCertificate,
                "",
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::InstallCertificateResponse = r.payload_as().unwrap();
                assert_eq!(body.status, InstallCertificateStatusEnumType::Rejected);
                assert_eq!(
                    body.status_info
                        .expect("a refusal carries a statusInfo reason")
                        .reason_code,
                    "InvalidCertificate"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_certificate(InstallCertificateUseEnumType::ManufacturerRootCertificate)
                .await,
            None,
            "a rejected certificate is not stored"
        );
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_install_certificate_fails_on_a_pem_with_no_body_and_stores_nothing() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // PEM-armored but carrying no key material → attempted, could not complete.
        let empty_body = "-----BEGIN CERTIFICATE-----\n\n-----END CERTIFICATE-----";
        let resp = cp
            .handle_message(Message::Call(make_v201_install_certificate(
                InstallCertificateUseEnumType::MORootCertificate,
                empty_body,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::InstallCertificateResponse = r.payload_as().unwrap();
                assert_eq!(body.status, InstallCertificateStatusEnumType::Failed);
                assert_eq!(
                    body.status_info
                        .expect("a failure carries a statusInfo reason")
                        .reason_code,
                    "InstallationFailed"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.installed_certificate(InstallCertificateUseEnumType::MORootCertificate)
                .await,
            None,
            "a failed install stores nothing"
        );
    }

    #[tokio::test]
    async fn v201_install_certificate_is_v201_only() {
        // A 1.6J CP has no InstallCertificate handler (1.6 has no per-use trust
        // model here), so the v201-only action is unrouted → CallError, never a
        // CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_install_certificate(
                InstallCertificateUseEnumType::CSMSRootCertificate,
                SAMPLE_INSTALL_PEM,
            )))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer InstallCertificate with a CallResult, got: {resp:?}"
        );
    }

    #[test]
    fn v201_install_certificate_request_and_response_are_schema_valid() {
        let validator = SchemaValidator::v201();
        // A request carrying a certificateType + PEM is schema-valid.
        let req = V201InstallCertificateRequest {
            certificate_type: InstallCertificateUseEnumType::CSMSRootCertificate,
            certificate: SAMPLE_INSTALL_PEM.to_string(),
            custom_data: None,
        };
        validator
            .validate_call("InstallCertificate", &serde_json::to_value(&req).unwrap())
            .expect("InstallCertificate request is schema-valid");

        // All three answer shapes the handler emits — Accepted (no statusInfo),
        // Rejected and Failed (each with a statusInfo reason) — serialize to a
        // schema-valid response.
        for (status, status_info) in [
            (InstallCertificateStatusEnumType::Accepted, None),
            (
                InstallCertificateStatusEnumType::Rejected,
                Some(StatusInfoType {
                    reason_code: "InvalidCertificate".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
            (
                InstallCertificateStatusEnumType::Failed,
                Some(StatusInfoType {
                    reason_code: "InstallationFailed".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::InstallCertificateResponse {
                status,
                status_info,
                custom_data: None,
            };
            validator
                .validate_call_result("InstallCertificate", &serde_json::to_value(&resp).unwrap())
                .expect("InstallCertificate response is schema-valid");
        }
    }

    // --- OCPP 2.0.1 SetNetworkProfile (M7, issue #528) --------------------
    // A `for_version(V201)` CP stores a network connection profile into its
    // `V201NetworkProfileStore`, keyed by `configurationSlot`. These exercise the
    // wired V201 arm end-to-end over `handle_message`: accept-and-store, per-slot
    // isolation, same-slot rotation, the blank-URL reject non-store path, extreme
    // slots, and V201-only registration.

    /// A minimal, well-formed `NetworkConnectionProfileType` whose `ocppCsmsUrl`
    /// embeds `tag` so tests can tell one stored profile from another.
    fn sample_network_profile(tag: &str) -> NetworkConnectionProfileType {
        use ocpp_types::v201::{OCPPInterfaceEnumType, OCPPTransportEnumType, OCPPVersionEnumType};
        NetworkConnectionProfileType {
            ocpp_version: OCPPVersionEnumType::Ocpp20,
            ocpp_transport: OCPPTransportEnumType::Json,
            ocpp_csms_url: format!("wss://csms.example.com/{tag}"),
            message_timeout: 30,
            security_profile: 2,
            ocpp_interface: OCPPInterfaceEnumType::Wireless0,
            apn: None,
            vpn: None,
            custom_data: None,
        }
    }

    fn make_v201_set_network_profile(
        configuration_slot: i32,
        connection_data: NetworkConnectionProfileType,
    ) -> CallMessage {
        make_call(V201SetNetworkProfileRequest {
            configuration_slot,
            connection_data,
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_set_network_profile_accepts_and_stores_a_profile() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let profile = sample_network_profile("a");
        let resp = cp
            .handle_message(Message::Call(make_v201_set_network_profile(
                1,
                profile.clone(),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetNetworkProfileResponse = r.payload_as().unwrap();
                assert_eq!(body.status, SetNetworkProfileStatusEnumType::Accepted);
                // Accepted carries no statusInfo.
                assert!(body.status_info.is_none());
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // The accepted profile is observable in the store, read back by slot.
        assert_eq!(cp.network_profile(1).await, Some(profile));
        assert_eq!(cp.configured_network_slots().await, vec![1]);
        // A pure store queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_set_network_profile_second_slot_stores_independently() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.handle_message(Message::Call(make_v201_set_network_profile(
            1,
            sample_network_profile("a"),
        )))
        .await
        .unwrap();

        // A second profile in a DIFFERENT slot, with distinct content.
        let resp = cp
            .handle_message(Message::Call(make_v201_set_network_profile(
                2,
                sample_network_profile("b"),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetNetworkProfileResponse = r.payload_as().unwrap();
                assert_eq!(body.status, SetNetworkProfileStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // Both slots hold their own profile; the first was not disturbed.
        assert_eq!(
            cp.network_profile(1).await,
            Some(sample_network_profile("a"))
        );
        assert_eq!(
            cp.network_profile(2).await,
            Some(sample_network_profile("b"))
        );
        assert_eq!(cp.configured_network_slots().await, vec![1, 2]);
    }

    #[tokio::test]
    async fn v201_set_network_profile_same_slot_rotates_the_profile() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.handle_message(Message::Call(make_v201_set_network_profile(
            1,
            sample_network_profile("old"),
        )))
        .await
        .unwrap();

        // Re-provision a DIFFERENT profile into the SAME slot (last-writer-wins).
        let rotated = sample_network_profile("new");
        let resp = cp
            .handle_message(Message::Call(make_v201_set_network_profile(
                1,
                rotated.clone(),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetNetworkProfileResponse = r.payload_as().unwrap();
                assert_eq!(body.status, SetNetworkProfileStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.network_profile(1).await,
            Some(rotated),
            "a same-slot re-provision rotates the profile in place, no duplicate"
        );
        assert_eq!(
            cp.configured_network_slots().await,
            vec![1],
            "a rotation does not grow the set of configured slots"
        );
    }

    #[tokio::test]
    async fn v201_set_network_profile_rejects_a_blank_url_and_stores_nothing() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // A profile whose ocppCsmsUrl is blank names no reachable CSMS — refused
        // up front (schema-valid on the wire: ocppCsmsUrl has no minLength), and
        // the CSMS-supplied profile never panics the handler.
        let mut blank = sample_network_profile("x");
        blank.ocpp_csms_url = "   ".to_string();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_network_profile(7, blank)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetNetworkProfileResponse = r.payload_as().unwrap();
                assert_eq!(body.status, SetNetworkProfileStatusEnumType::Rejected);
                assert_eq!(
                    body.status_info
                        .expect("a refusal carries a statusInfo reason")
                        .reason_code,
                    "InvalidProfile"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.network_profile(7).await,
            None,
            "a rejected profile is not stored"
        );
        assert!(cp.configured_network_slots().await.is_empty());
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_set_network_profile_handles_extreme_slots() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // Extreme configurationSlot values are ordinary HashMap keys — no panic,
        // stored and read back independently.
        for slot in [i32::MIN, i32::MAX] {
            let resp = cp
                .handle_message(Message::Call(make_v201_set_network_profile(
                    slot,
                    sample_network_profile("edge"),
                )))
                .await
                .unwrap();
            match resp.unwrap() {
                Message::CallResult(r) => {
                    let body: ocpp_messages::v201::SetNetworkProfileResponse =
                        r.payload_as().unwrap();
                    assert_eq!(body.status, SetNetworkProfileStatusEnumType::Accepted);
                }
                other => panic!("expected CallResult, got: {other:?}"),
            }
            assert_eq!(
                cp.network_profile(slot).await,
                Some(sample_network_profile("edge"))
            );
        }
        assert_eq!(
            cp.configured_network_slots().await,
            vec![i32::MIN, i32::MAX]
        );
    }

    #[tokio::test]
    async fn v201_set_network_profile_is_v201_only() {
        // A 1.6J CP has no SetNetworkProfile handler (1.6 has no equivalent
        // command), so the v201-only action is unrouted → CallError, never a
        // CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_network_profile(
                1,
                sample_network_profile("a"),
            )))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer SetNetworkProfile with a CallResult, got: {resp:?}"
        );
    }

    // --- OCPP 2.0.1 GetInstalledCertificateIds (M7, issue #521) ------------
    // A `for_version(V201)` CP enumerates its `V201CertificateStore` synchronously
    // — the hash chain rides on the CALLRESULT (no async stream). These exercise
    // the wired V201 arm end-to-end over `handle_message`: enumerate-all, filtered,
    // empty-store `NotFound`, filter-matches-nothing, hostile/duplicate filters,
    // V201-only registration, and the shared-hash round-trip readiness #522 relies
    // on.

    fn make_v201_get_installed_certificate_ids(
        certificate_type: Option<Vec<ocpp_types::v201::GetCertificateIdUseEnumType>>,
    ) -> CallMessage {
        make_call(V201GetInstalledCertificateIdsRequest {
            certificate_type,
            custom_data: None,
        })
    }

    // --- OCPP 2.0.1 CertificateSigned (M7, issue #516) ---------------------
    // A `for_version(V201)` CP receives a CA-signed certificate chain and answers
    // accept/reject synchronously — a stateless responder, no store, nothing
    // queued. These exercise the wired V201 arm end-to-end over `handle_message`:
    // accept for each `certificateType` (and the absent case), the reject/hostile
    // paths, and V201-only registration.

    // A minimal but structurally-valid signed chain, armor + one body line.
    const SAMPLE_SIGNED_PEM: &str =
        "-----BEGIN CERTIFICATE-----\nMIIBkTCB+w==\n-----END CERTIFICATE-----";

    fn make_v201_certificate_signed(
        certificate_chain: &str,
        certificate_type: Option<ocpp_types::v201::CertificateSigningUseEnumType>,
    ) -> CallMessage {
        make_call(V201CertificateSignedRequest {
            certificate_chain: certificate_chain.to_string(),
            certificate_type,
            custom_data: None,
        })
    }

    /// Install `SAMPLE_INSTALL_PEM` under `use_` on `cp` (asserting it is accepted),
    /// so the enumerate tests have known anchors to read back.
    async fn install_anchor(cp: &ChargePoint, use_: InstallCertificateUseEnumType) {
        let resp = cp
            .handle_message(Message::Call(make_v201_install_certificate(
                use_,
                SAMPLE_INSTALL_PEM,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::InstallCertificateResponse = r.payload_as().unwrap();
                assert_eq!(body.status, InstallCertificateStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    /// Read the `CertificateSigned.conf` out of a `handle_message` reply,
    /// asserting the reply is a CALLRESULT.
    async fn certificate_signed_response(
        cp: &ChargePoint,
        certificate_chain: &str,
        certificate_type: Option<ocpp_types::v201::CertificateSigningUseEnumType>,
    ) -> ocpp_messages::v201::CertificateSignedResponse {
        let resp = cp
            .handle_message(Message::Call(make_v201_certificate_signed(
                certificate_chain,
                certificate_type,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => r.payload_as().unwrap(),
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_installed_certificate_ids_enumerates_all() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_anchor(&cp, InstallCertificateUseEnumType::CSMSRootCertificate).await;
        install_anchor(&cp, InstallCertificateUseEnumType::V2GRootCertificate).await;

        let resp = cp
            .handle_message(Message::Call(make_v201_get_installed_certificate_ids(None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetInstalledCertificateIdsResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::GetInstalledCertificateStatusEnumType::Accepted
                );
                let chain = body
                    .certificate_hash_data_chain
                    .expect("Accepted carries a chain");
                assert_eq!(chain.len(), 2, "both installed anchors are enumerated");
                // The reported hash is the shared seam's hash for (use, PEM), so it
                // round-trips to a future DeleteCertificate (#522).
                let csms = chain
                    .iter()
                    .find(|e| {
                        e.certificate_type
                            == ocpp_types::v201::GetCertificateIdUseEnumType::CSMSRootCertificate
                    })
                    .expect("the CSMS anchor is reported");
                assert_eq!(
                    csms.certificate_hash_data,
                    v201_command::v201_certificate_hash_data(
                        InstallCertificateUseEnumType::CSMSRootCertificate,
                        SAMPLE_INSTALL_PEM,
                    )
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // A pure read queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_certificate_signed_accepts_a_pem_chain_for_each_type() {
        use ocpp_types::v201::CertificateSigningUseEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // Both certificate types, and an absent certificate_type, accept a
        // well-formed chain with no statusInfo — and nothing is queued.
        for certificate_type in [
            Some(CertificateSigningUseEnumType::ChargingStationCertificate),
            Some(CertificateSigningUseEnumType::V2GCertificate),
            None,
        ] {
            let body = certificate_signed_response(&cp, SAMPLE_SIGNED_PEM, certificate_type).await;
            assert_eq!(body.status, CertificateSignedStatusEnumType::Accepted);
            assert!(
                body.status_info.is_none(),
                "an accepted chain carries no statusInfo (type={certificate_type:?})"
            );
        }
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "CertificateSigned is a stateless responder — it queues nothing"
        );
    }

    #[tokio::test]
    async fn v201_certificate_signed_rejects_empty_or_hostile_input_and_never_panics() {
        use ocpp_types::v201::CertificateSigningUseEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // Empty, blank, non-PEM, empty-bodied, and control-char CSMS inputs (all
        // within the schema's 10000-char cap, so they reach the handler rather than
        // being rejected at the wire layer) are refused with a statusInfo reason —
        // and never panic the handler. (Over-length input is caught upstream by the
        // schema; the pure `v201_command` test covers the very-long case directly.)
        for chain in [
            "",
            "   ",
            "not a certificate",
            "-----BEGIN CERTIFICATE-----\n\n-----END CERTIFICATE-----",
            "\0\u{1}\u{2}-----BEGIN-----\u{7f}",
        ] {
            let body = certificate_signed_response(
                &cp,
                chain,
                Some(CertificateSigningUseEnumType::ChargingStationCertificate),
            )
            .await;
            assert_eq!(
                body.status,
                CertificateSignedStatusEnumType::Rejected,
                "a malformed chain is refused: {chain:?}"
            );
            assert_eq!(
                body.status_info
                    .expect("a refusal carries a statusInfo reason")
                    .reason_code,
                "InvalidChain"
            );
        }
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_get_installed_certificate_ids_applies_the_filter() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_anchor(&cp, InstallCertificateUseEnumType::CSMSRootCertificate).await;
        install_anchor(&cp, InstallCertificateUseEnumType::V2GRootCertificate).await;

        let resp = cp
            .handle_message(Message::Call(make_v201_get_installed_certificate_ids(
                Some(vec![
                    ocpp_types::v201::GetCertificateIdUseEnumType::V2GRootCertificate,
                ]),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetInstalledCertificateIdsResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::GetInstalledCertificateStatusEnumType::Accepted
                );
                let chain = body.certificate_hash_data_chain.expect("a match → a chain");
                assert_eq!(chain.len(), 1);
                assert_eq!(
                    chain[0].certificate_type,
                    ocpp_types::v201::GetCertificateIdUseEnumType::V2GRootCertificate
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_installed_certificate_ids_empty_store_is_not_found() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_installed_certificate_ids(None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetInstalledCertificateIdsResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::GetInstalledCertificateStatusEnumType::NotFound
                );
                assert!(
                    body.certificate_hash_data_chain.is_none(),
                    "NotFound carries no chain"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_installed_certificate_ids_filter_matching_nothing_is_not_found() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_anchor(&cp, InstallCertificateUseEnumType::CSMSRootCertificate).await;

        // A store the CSMS root is installed in, queried for anchors it does not
        // hold: a different root, plus V2GCertificateChain (never a store key) and a
        // duplicated value — no panic, and NotFound.
        let resp = cp
            .handle_message(Message::Call(make_v201_get_installed_certificate_ids(
                Some(vec![
                    ocpp_types::v201::GetCertificateIdUseEnumType::ManufacturerRootCertificate,
                    ocpp_types::v201::GetCertificateIdUseEnumType::V2GCertificateChain,
                    ocpp_types::v201::GetCertificateIdUseEnumType::V2GCertificateChain,
                ]),
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetInstalledCertificateIdsResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::GetInstalledCertificateStatusEnumType::NotFound
                );
                assert!(body.certificate_hash_data_chain.is_none());
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_installed_certificate_ids_is_v201_only() {
        // A 1.6J CP has no GetInstalledCertificateIds handler (no per-use trust
        // model), so the v201-only action is unrouted → CallError, never a
        // CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_installed_certificate_ids(None)))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer GetInstalledCertificateIds with a CallResult, got: {resp:?}"
        );
    }

    #[tokio::test]
    async fn v201_certificate_signed_is_v201_only() {
        // A 1.6J CP has no CertificateSigned handler, so the v201-only action is
        // unrouted → CallError, never a CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_certificate_signed(
                SAMPLE_SIGNED_PEM,
                None,
            )))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer CertificateSigned with a CallResult, got: {resp:?}"
        );
    }

    #[test]
    fn v201_certificate_signed_request_and_response_are_schema_valid() {
        use ocpp_types::v201::CertificateSigningUseEnumType;
        let validator = SchemaValidator::v201();
        // A request carrying a certificateType + PEM chain is schema-valid.
        let req = V201CertificateSignedRequest {
            certificate_chain: SAMPLE_SIGNED_PEM.to_string(),
            certificate_type: Some(CertificateSigningUseEnumType::ChargingStationCertificate),
            custom_data: None,
        };
        validator
            .validate_call("CertificateSigned", &serde_json::to_value(&req).unwrap())
            .expect("CertificateSigned request is schema-valid");

        // Both answer shapes the handler emits — Accepted (no statusInfo) and
        // Rejected (with a statusInfo reason) — serialize to a schema-valid response.
        for (status, status_info) in [
            (CertificateSignedStatusEnumType::Accepted, None),
            (
                CertificateSignedStatusEnumType::Rejected,
                Some(StatusInfoType {
                    reason_code: "InvalidChain".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::CertificateSignedResponse {
                status,
                status_info,
                custom_data: None,
            };
            validator
                .validate_call_result("CertificateSigned", &serde_json::to_value(&resp).unwrap())
                .expect("CertificateSigned response is schema-valid");
        }
    }

    // --- OCPP 2.0.1 CustomerInformation (M7, issues #530 / #537) -----------
    // A `for_version(V201)` CP synchronously accepts/rejects the privacy/GDPR
    // report-and/or-clear command, and — when an accepted request set
    // `report: true` — queues an async `NotifyCustomerInformation` report stream
    // (#537). These exercise the wired V201 arm end-to-end over `handle_message`:
    // each selector kind with an action is Accepted, a request naming no selector
    // or no action is Invalid, a reporting accept queues exactly one stream, a
    // clear-only accept queues none, a retry of an in-flight id queues no second
    // stream, extreme request ids never panic, and the handler is V201-only.

    fn sample_customer_hash() -> ocpp_types::v201::CertificateHashDataType {
        ocpp_types::v201::CertificateHashDataType {
            hash_algorithm: ocpp_types::v201::HashAlgorithmEnumType::Sha256,
            issuer_name_hash: "a1".to_string(),
            issuer_key_hash: "b2".to_string(),
            serial_number: "c3".to_string(),
            custom_data: None,
        }
    }

    fn sample_customer_id_token() -> V201IdTokenType {
        V201IdTokenType {
            id_token: "RFID-1234".to_string(),
            kind: ocpp_types::v201::IdTokenEnumType::Iso14443,
            additional_info: None,
            custom_data: None,
        }
    }

    fn make_v201_customer_information(
        report: bool,
        clear: bool,
        customer_certificate: Option<ocpp_types::v201::CertificateHashDataType>,
        id_token: Option<V201IdTokenType>,
        customer_identifier: Option<String>,
    ) -> CallMessage {
        make_call(V201CustomerInformationRequest {
            request_id: 7,
            report,
            clear,
            customer_certificate,
            id_token,
            customer_identifier,
            custom_data: None,
        })
    }

    /// Read the `CustomerInformation.conf` out of a `handle_message` reply,
    /// asserting the reply is a CALLRESULT.
    async fn customer_information_response(
        cp: &ChargePoint,
        call: CallMessage,
    ) -> ocpp_messages::v201::CustomerInformationResponse {
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => r.payload_as().unwrap(),
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_customer_information_accepts_each_selector_with_an_action() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // Each selector kind (certificate hash / idToken / free-form identifier),
        // asking to clear (an action, but not report), names a customer →
        // Accepted, no statusInfo. A clear-only accept has no data to stream, so
        // it queues no report — the queue is exercised by the reporting tests.
        for selector_kind in 0..3 {
            let (cert, token, ident) = match selector_kind {
                0 => (Some(sample_customer_hash()), None, None),
                1 => (None, Some(sample_customer_id_token()), None),
                _ => (None, None, Some("customer-abc".to_string())),
            };
            let body = customer_information_response(
                &cp,
                make_v201_customer_information(false, true, cert, token, ident),
            )
            .await;
            assert_eq!(body.status, CustomerInformationStatusEnumType::Accepted);
            assert!(
                body.status_info.is_none(),
                "an accepted request carries no statusInfo (selector_kind={selector_kind})"
            );
        }
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "a clear-only accept queues no NotifyCustomerInformation report stream"
        );
    }

    #[tokio::test]
    async fn v201_customer_information_invalid_without_a_selector_or_action() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // No selector at all (even asking for both actions) → Invalid, with a reason.
        let body = customer_information_response(
            &cp,
            make_v201_customer_information(true, true, None, None, None),
        )
        .await;
        assert_eq!(body.status, CustomerInformationStatusEnumType::Invalid);
        assert_eq!(
            body.status_info
                .expect("an Invalid answer carries a statusInfo reason")
                .reason_code,
            "InvalidRequest"
        );

        // A named customer but neither report nor clear → Invalid (nothing to do).
        let body = customer_information_response(
            &cp,
            make_v201_customer_information(false, false, Some(sample_customer_hash()), None, None),
        )
        .await;
        assert_eq!(body.status, CustomerInformationStatusEnumType::Invalid);

        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_customer_information_extreme_request_id_never_panics() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // request_id is echoed into the store and the queued command, never parsed
        // — an actionable reporting request at every extreme is Accepted, queues a
        // correlated report stream, and never panics.
        let ids = [0, 1, -1, i32::MIN, i32::MAX];
        for request_id in ids {
            let call = make_call(V201CustomerInformationRequest {
                request_id,
                report: true,
                clear: false,
                customer_certificate: None,
                id_token: None,
                customer_identifier: Some("c".to_string()),
                custom_data: None,
            });
            let body = customer_information_response(&cp, call).await;
            assert_eq!(
                body.status,
                CustomerInformationStatusEnumType::Accepted,
                "request_id={request_id} is accepted without panic"
            );
        }
        // Each distinct id queued its own report stream, correlated by requestId.
        let queued: Vec<i32> = v201_drain_commands(&cp)
            .await
            .into_iter()
            .filter_map(|c| match c {
                RemoteCommand::V201CustomerInformationReport { request_id } => Some(request_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            queued,
            ids.to_vec(),
            "every extreme reporting request queues a correlated stream, in order"
        );
    }

    #[tokio::test]
    async fn v201_customer_information_is_v201_only() {
        // A 1.6J CP has no CustomerInformation handler (no privacy command), so the
        // v201-only action is unrouted → CallError, never a CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_customer_information(
                true,
                false,
                None,
                None,
                Some("c".to_string()),
            )))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer CustomerInformation with a CallResult, got: {resp:?}"
        );
    }

    #[tokio::test]
    async fn v201_customer_information_reporting_accept_queues_one_report_stream() {
        // An accepted request that set `report: true` queues exactly one
        // V201CustomerInformationReport correlated by requestId (make_* uses 7).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body = customer_information_response(
            &cp,
            make_v201_customer_information(true, false, None, None, Some("cust".to_string())),
        )
        .await;
        assert_eq!(body.status, CustomerInformationStatusEnumType::Accepted);

        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "a reporting accept queues exactly one report stream"
        );
        assert!(
            matches!(
                &commands[0],
                RemoteCommand::V201CustomerInformationReport { request_id } if *request_id == 7
            ),
            "the queued report is correlated by requestId, got: {:?}",
            commands[0]
        );
    }

    #[tokio::test]
    async fn v201_customer_information_retry_of_in_flight_queues_no_second_stream() {
        // Two reporting requests carrying the same requestId (7): the first queues
        // a stream, the retry — while the first is still in flight (no consumer
        // drains it) — queues none, so the CSMS is not double-reported.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        for _ in 0..2 {
            let body = customer_information_response(
                &cp,
                make_v201_customer_information(true, false, None, None, Some("cust".to_string())),
            )
            .await;
            assert_eq!(body.status, CustomerInformationStatusEnumType::Accepted);
        }
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "a retry of an in-flight requestId queues no second stream"
        );
    }

    #[test]
    fn v201_customer_information_request_and_response_are_schema_valid() {
        let validator = SchemaValidator::v201();
        // A request carrying all three selectors + both actions is schema-valid.
        let req = V201CustomerInformationRequest {
            request_id: 7,
            report: true,
            clear: true,
            customer_certificate: Some(sample_customer_hash()),
            id_token: Some(sample_customer_id_token()),
            customer_identifier: Some("customer-abc".to_string()),
            custom_data: None,
        };
        validator
            .validate_call("CustomerInformation", &serde_json::to_value(&req).unwrap())
            .expect("CustomerInformation request is schema-valid");

        // Both answer shapes the handler emits — Accepted (no statusInfo) and Invalid
        // (with a statusInfo reason) — serialize to a schema-valid response.
        for (status, status_info) in [
            (CustomerInformationStatusEnumType::Accepted, None),
            (
                CustomerInformationStatusEnumType::Invalid,
                Some(StatusInfoType {
                    reason_code: "InvalidRequest".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::CustomerInformationResponse {
                status,
                status_info,
                custom_data: None,
            };
            validator
                .validate_call_result("CustomerInformation", &serde_json::to_value(&resp).unwrap())
                .expect("CustomerInformation response is schema-valid");
        }
    }

    // --- OCPP 2.0.1 PublishFirmware (M7, issue #538) -----------------------
    // A `for_version(V201)` CP synchronously accepts/rejects the Local-Controller
    // firmware-cache trigger: a non-empty `location` plus a 32-char hex `checksum`
    // → Accepted; an empty location or a mis-shaped checksum → Rejected with a
    // statusInfo reason. Stateless (queues nothing), and V201-only. These exercise
    // the wired V201 arm end-to-end over `handle_message`.

    /// A syntactically valid 32-char hex MD5 digest for the wire fixtures.
    const SAMPLE_FW_MD5: &str = "0123456789abcdef0123456789abcdef";

    fn make_v201_publish_firmware(location: &str, checksum: &str, request_id: i32) -> CallMessage {
        make_call(V201PublishFirmwareRequest {
            location: location.to_string(),
            retries: None,
            checksum: checksum.to_string(),
            request_id,
            retry_interval: None,
            custom_data: None,
        })
    }

    /// Read the `PublishFirmware.conf` out of a `handle_message` reply, asserting
    /// the reply is a CALLRESULT.
    async fn publish_firmware_response(
        cp: &ChargePoint,
        call: CallMessage,
    ) -> ocpp_messages::v201::PublishFirmwareResponse {
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => r.payload_as().unwrap(),
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_publish_firmware_accepts_a_location_with_a_hex_checksum() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body = publish_firmware_response(
            &cp,
            make_v201_publish_firmware("https://fw.example/img.bin", SAMPLE_FW_MD5, 7),
        )
        .await;
        assert_eq!(body.status, GenericStatusEnumType::Accepted);
        assert!(
            body.status_info.is_none(),
            "an accepted publish request carries no statusInfo"
        );
        // An accepted publish records the request in flight and queues exactly one
        // progress stream correlated by requestId (make_* uses 7).
        assert!(
            cp.is_publishing_firmware(7).await,
            "an accepted publish is recorded as in flight"
        );
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "an accepted publish queues exactly one progress stream"
        );
        assert!(
            matches!(
                &commands[0],
                RemoteCommand::V201PublishFirmwareStatus { request_id } if *request_id == 7
            ),
            "the queued stream is correlated by requestId, got: {:?}",
            commands[0]
        );
    }

    #[tokio::test]
    async fn v201_publish_firmware_rejects_empty_location_or_bad_checksum() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // Empty location → Rejected, with an InvalidRequest reason.
        let body =
            publish_firmware_response(&cp, make_v201_publish_firmware("", SAMPLE_FW_MD5, 7)).await;
        assert_eq!(body.status, GenericStatusEnumType::Rejected);
        assert_eq!(
            body.status_info
                .expect("a Rejected answer carries a statusInfo reason")
                .reason_code,
            "InvalidRequest"
        );

        // Mis-shaped checksum (not 32 hex chars) → Rejected.
        let body = publish_firmware_response(
            &cp,
            make_v201_publish_firmware("https://fw.example/img.bin", "not-a-hex-digest", 7),
        )
        .await;
        assert_eq!(body.status, GenericStatusEnumType::Rejected);

        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_publish_firmware_extreme_request_id_never_panics() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // request_id is echoed into the store and the queued command, never parsed
        // — an actionable request at every extreme is Accepted, queues a correlated
        // progress stream, and never panics.
        let ids = [0, 1, -1, i32::MIN, i32::MAX];
        for request_id in ids {
            let body = publish_firmware_response(
                &cp,
                make_v201_publish_firmware("https://fw.example/img.bin", SAMPLE_FW_MD5, request_id),
            )
            .await;
            assert_eq!(
                body.status,
                GenericStatusEnumType::Accepted,
                "request_id={request_id} is accepted without panic"
            );
        }
        // Each distinct id queued its own progress stream, correlated by requestId.
        let queued: Vec<i32> = v201_drain_commands(&cp)
            .await
            .into_iter()
            .filter_map(|c| match c {
                RemoteCommand::V201PublishFirmwareStatus { request_id } => Some(request_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            queued,
            ids.to_vec(),
            "every extreme accepted publish queues a correlated stream, in order"
        );
    }

    #[tokio::test]
    async fn v201_publish_firmware_is_v201_only() {
        // A 1.6J CP has no PublishFirmware handler (1.6J has no such command), so the
        // v201-only action is unrouted → CallError, never a CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_publish_firmware(
                "https://fw.example/img.bin",
                SAMPLE_FW_MD5,
                7,
            )))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer PublishFirmware with a CallResult, got: {resp:?}"
        );
    }

    #[test]
    fn v201_publish_firmware_request_and_response_are_schema_valid() {
        let validator = SchemaValidator::v201();
        // A request carrying the optional retry tuning is schema-valid.
        let req = V201PublishFirmwareRequest {
            location: "https://fw.example/img.bin".to_string(),
            retries: Some(3),
            checksum: SAMPLE_FW_MD5.to_string(),
            request_id: 7,
            retry_interval: Some(60),
            custom_data: None,
        };
        validator
            .validate_call("PublishFirmware", &serde_json::to_value(&req).unwrap())
            .expect("PublishFirmware request is schema-valid");

        // Both answer shapes the handler emits — Accepted (no statusInfo) and Rejected
        // (with a statusInfo reason) — serialize to a schema-valid response.
        for (status, status_info) in [
            (GenericStatusEnumType::Accepted, None),
            (
                GenericStatusEnumType::Rejected,
                Some(StatusInfoType {
                    reason_code: "InvalidRequest".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::PublishFirmwareResponse {
                status,
                status_info,
                custom_data: None,
            };
            validator
                .validate_call_result("PublishFirmware", &serde_json::to_value(&resp).unwrap())
                .expect("PublishFirmware response is schema-valid");
        }
    }

    #[tokio::test]
    async fn v201_publish_firmware_rejected_queues_no_stream_and_records_nothing() {
        // AC #540: a Rejected publish drives no async stream and records nothing —
        // so it cannot be observed as in flight and dedups nothing later.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body =
            publish_firmware_response(&cp, make_v201_publish_firmware("", SAMPLE_FW_MD5, 7)).await;
        assert_eq!(body.status, GenericStatusEnumType::Rejected);
        assert!(
            !cp.is_publishing_firmware(7).await,
            "a rejected publish records no in-flight marker"
        );
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "a rejected publish queues no progress stream"
        );
    }

    #[tokio::test]
    async fn v201_publish_firmware_retry_of_in_flight_queues_no_second_stream() {
        // Two accepted publishes carrying the same requestId (7): the first records
        // the id and queues a stream; the retry — while the first is still in flight
        // (no consumer drains it) — records nothing new and queues none, so the CSMS
        // is not double-reported.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        for _ in 0..2 {
            let body = publish_firmware_response(
                &cp,
                make_v201_publish_firmware("https://fw.example/img.bin", SAMPLE_FW_MD5, 7),
            )
            .await;
            assert_eq!(body.status, GenericStatusEnumType::Accepted);
        }
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "a retry of an in-flight requestId queues no second stream"
        );
    }

    #[tokio::test]
    async fn v201_publish_firmware_completes_and_clears_the_in_flight_marker() {
        // AC #540: after the async progress stream settles, the in-flight marker is
        // cleared, so a subsequent PublishFirmware with the same requestId publishes
        // afresh (queues a new stream) rather than being deduped. Drive the state
        // machine directly (the CP is not connected, so the
        // PublishFirmwareStatusNotification CALLs fail-and-warn, but the store
        // transition is what this asserts).
        // `v201_drain_commands` destructively takes the command receiver (closing
        // the channel), so this test never drains mid-way — both publishes' queued
        // streams accumulate in the still-open channel and are drained once at the
        // end. The marker transitions are what this asserts.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // Accept a publish, then run its async stream to completion.
        let first = publish_firmware_response(
            &cp,
            make_v201_publish_firmware("https://fw.example/img.bin", SAMPLE_FW_MD5, 5),
        )
        .await;
        assert_eq!(first.status, GenericStatusEnumType::Accepted);
        assert!(cp.is_publishing_firmware(5).await);

        cp.run_v201_publish_firmware_status(5).await;
        assert!(
            !cp.is_publishing_firmware(5).await,
            "a settled publish stream clears the in-flight marker"
        );

        // A subsequent publish of the same id now starts fresh — it records the id
        // again and queues a new stream (it is not deduped against the settled one).
        let next = publish_firmware_response(
            &cp,
            make_v201_publish_firmware("https://fw.example/img.bin", SAMPLE_FW_MD5, 5),
        )
        .await;
        assert_eq!(next.status, GenericStatusEnumType::Accepted);
        assert!(cp.is_publishing_firmware(5).await);

        // Both publishes queued a stream correlated by requestId — the settle in
        // between did not cause the re-publish to be deduped.
        let queued: Vec<i32> = v201_drain_commands(&cp)
            .await
            .into_iter()
            .filter_map(|c| match c {
                RemoteCommand::V201PublishFirmwareStatus { request_id } => Some(request_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            queued,
            vec![5, 5],
            "the pre-settle publish and the fresh post-settle publish each queued a stream"
        );
    }

    #[tokio::test]
    async fn v201_publish_firmware_streams_are_independent_per_request_id() {
        // Unlike the single-slot firmware-update rollout, two different requestIds
        // each have a stream in flight with neither superseding the other (the
        // set-based, customer-information model). Settling one leaves the other.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.v201_publish_firmwares.begin(1).await;
        cp.v201_publish_firmwares.begin(2).await;

        cp.run_v201_publish_firmware_status(1).await;
        assert!(
            !cp.is_publishing_firmware(1).await,
            "the settled stream clears its own id"
        );
        assert!(
            cp.is_publishing_firmware(2).await,
            "the other id is untouched — no supersede"
        );
    }

    // --- OCPP 2.0.1 DeleteCertificate (M7, issue #522) ---------------------
    // A `for_version(V201)` CP removes an installed trust anchor from its
    // `V201CertificateStore`, named by the same hash `GetInstalledCertificateIds`
    // reports for it. These exercise the wired V201 arm end-to-end over
    // `handle_message`: delete-existing round-trips off the shared hash seam,
    // delete-unknown is `NotFound`, re-delete is idempotent, hostile input never
    // panics, and the handler is V201-only.

    fn make_v201_delete_certificate(
        hash: ocpp_types::v201::CertificateHashDataType,
    ) -> CallMessage {
        make_call(V201DeleteCertificateRequest {
            certificate_hash_data: hash,
            custom_data: None,
        })
    }

    // --- OCPP 2.0.1 GetLog (M7, issue #517) --------------------------------
    // A `for_version(V201)` CP acks a diagnostics/security log-upload request
    // synchronously off its `V201LogUploadStore` (in-flight `requestId` tracker):
    // idle → Accepted + a synthesized filename; a new requestId while one is in
    // flight → AcceptedCanceled (supersede); the same requestId → idempotent
    // Accepted. These exercise the wired V201 arm end-to-end over `handle_message`:
    // both log kinds, supersede, retry, extreme requestId, and V201-only
    // registration.

    fn make_v201_get_log(log_type: ocpp_types::v201::LogEnumType, request_id: i32) -> CallMessage {
        make_call(V201GetLogRequest {
            log: ocpp_types::v201::LogParametersType {
                remote_location: "https://logs.example.test/upload".to_string(),
                oldest_timestamp: None,
                latest_timestamp: None,
                custom_data: None,
            },
            log_type,
            request_id,
            retries: None,
            retry_interval: None,
            custom_data: None,
        })
    }

    /// Read the `DeleteCertificate.conf` out of a `handle_message` reply,
    /// asserting the reply is a CALLRESULT.
    async fn delete_certificate_response(
        cp: &ChargePoint,
        hash: ocpp_types::v201::CertificateHashDataType,
    ) -> ocpp_messages::v201::DeleteCertificateResponse {
        let resp = cp
            .handle_message(Message::Call(make_v201_delete_certificate(hash)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => r.payload_as().unwrap(),
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    /// Read the `GetLog.conf` out of a `handle_message` reply, asserting the reply
    /// is a CALLRESULT.
    async fn get_log_response(
        cp: &ChargePoint,
        log_type: ocpp_types::v201::LogEnumType,
        request_id: i32,
    ) -> ocpp_messages::v201::GetLogResponse {
        let resp = cp
            .handle_message(Message::Call(make_v201_get_log(log_type, request_id)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => r.payload_as().unwrap(),
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_delete_certificate_removes_an_installed_anchor() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_anchor(&cp, InstallCertificateUseEnumType::CSMSRootCertificate).await;
        install_anchor(&cp, InstallCertificateUseEnumType::V2GRootCertificate).await;

        // Delete by the exact hash GetInstalledCertificateIds would report for the
        // CSMS anchor — the round-trip the shared seam guarantees.
        let csms_hash = v201_command::v201_certificate_hash_data(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            SAMPLE_INSTALL_PEM,
        );
        let body = delete_certificate_response(&cp, csms_hash).await;
        assert_eq!(body.status, DeleteCertificateStatusEnumType::Accepted);
        assert!(
            body.status_info.is_none(),
            "an accepted delete carries no statusInfo"
        );

        // The CSMS anchor is gone; the untouched V2G anchor remains.
        assert_eq!(
            cp.installed_certificate(InstallCertificateUseEnumType::CSMSRootCertificate)
                .await,
            None,
            "the deleted anchor is no longer installed"
        );
        assert!(
            cp.installed_certificate(InstallCertificateUseEnumType::V2GRootCertificate)
                .await
                .is_some(),
            "a sibling anchor is undisturbed"
        );

        // GetInstalledCertificateIds now enumerates only the surviving anchor.
        let resp = cp
            .handle_message(Message::Call(make_v201_get_installed_certificate_ids(None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let ids: ocpp_messages::v201::GetInstalledCertificateIdsResponse =
                    r.payload_as().unwrap();
                let chain = ids
                    .certificate_hash_data_chain
                    .expect("one anchor still installed");
                assert_eq!(chain.len(), 1);
                assert_eq!(
                    chain[0].certificate_type,
                    ocpp_types::v201::GetCertificateIdUseEnumType::V2GRootCertificate
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }

        // A pure remove queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_delete_certificate_unknown_hash_is_not_found_and_changes_nothing() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_anchor(&cp, InstallCertificateUseEnumType::CSMSRootCertificate).await;

        // A hash naming an anchor that is not installed removes nothing.
        let unknown = v201_command::v201_certificate_hash_data(
            InstallCertificateUseEnumType::MORootCertificate,
            SAMPLE_INSTALL_PEM,
        );
        let body = delete_certificate_response(&cp, unknown).await;
        assert_eq!(body.status, DeleteCertificateStatusEnumType::NotFound);
        assert!(
            body.status_info.is_some(),
            "a NotFound carries a statusInfo reason"
        );

        // The installed anchor is untouched.
        assert!(
            cp.installed_certificate(InstallCertificateUseEnumType::CSMSRootCertificate)
                .await
                .is_some(),
            "a non-matching delete removes nothing"
        );
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_delete_certificate_is_idempotent() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_anchor(&cp, InstallCertificateUseEnumType::CSMSRootCertificate).await;
        let hash = v201_command::v201_certificate_hash_data(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            SAMPLE_INSTALL_PEM,
        );

        // First delete removes the anchor.
        let first = delete_certificate_response(&cp, hash.clone()).await;
        assert_eq!(first.status, DeleteCertificateStatusEnumType::Accepted);

        // Re-deleting the same hash finds nothing to remove — idempotent NotFound.
        let second = delete_certificate_response(&cp, hash).await;
        assert_eq!(second.status, DeleteCertificateStatusEnumType::NotFound);
    }

    #[tokio::test]
    async fn v201_delete_certificate_does_not_panic_on_hostile_hash() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_anchor(&cp, InstallCertificateUseEnumType::CSMSRootCertificate).await;

        // Control-char and multi-byte hash fields (within the schema length bounds
        // so they reach the handler, rather than being rejected as CALLERROR up
        // front) are only string-compared, never parsed — the handler answers
        // NotFound without panicking. Over-length fields are refused at the schema
        // layer before the handler, which the pure `v201_delete_certificate_target`
        // test covers for arbitrary byte lengths.
        let hostile = ocpp_types::v201::CertificateHashDataType {
            hash_algorithm: ocpp_types::v201::HashAlgorithmEnumType::Sha512,
            issuer_name_hash: "\0\u{1}\u{7f}control\u{feff}".to_string(),
            issuer_key_hash: "💥🔥".repeat(10),
            serial_number: "\0\u{1}\u{2}not-a-hash".to_string(),
            custom_data: None,
        };
        let body = delete_certificate_response(&cp, hostile).await;
        assert_eq!(body.status, DeleteCertificateStatusEnumType::NotFound);
    }

    #[tokio::test]
    async fn v201_delete_certificate_is_v201_only() {
        // A 1.6J CP has no DeleteCertificate handler (no per-use trust model), so
        // the v201-only action is unrouted → CallError, never a CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let hash = v201_command::v201_certificate_hash_data(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            SAMPLE_INSTALL_PEM,
        );
        let resp = cp
            .handle_message(Message::Call(make_v201_delete_certificate(hash)))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer DeleteCertificate with a CallResult, got: {resp:?}"
        );
    }

    #[tokio::test]
    async fn v201_get_log_accepts_and_names_a_file_when_idle() {
        use ocpp_types::v201::LogEnumType;
        // Each log kind, from an idle station: Accepted, a non-empty kind-tagged
        // filename, and the request recorded as the one now in flight. A fresh CP
        // per kind so each starts idle.
        for (log_type, prefix) in [
            (LogEnumType::DiagnosticsLog, "diagnostics_"),
            (LogEnumType::SecurityLog, "security_"),
        ] {
            let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
            assert_eq!(cp.in_flight_log_upload().await, None, "starts idle");

            let body = get_log_response(&cp, log_type, 42).await;
            assert_eq!(body.status, LogStatusEnumType::Accepted);
            let filename = body.filename.expect("an accepted GetLog names a file");
            assert!(!filename.is_empty());
            assert_eq!(filename, format!("{prefix}42.log"));

            // The accept is recorded as the in-flight upload and queues exactly
            // one V201LogUpload to drive the async LogStatusNotification stream,
            // correlated by the same requestId.
            assert_eq!(cp.in_flight_log_upload().await, Some(42));
            let commands = v201_drain_commands(&cp).await;
            assert_eq!(commands.len(), 1, "an accepted GetLog queues one upload");
            assert!(
                matches!(&commands[0], RemoteCommand::V201LogUpload { request_id } if *request_id == 42),
                "the queued upload is correlated by requestId, got: {:?}",
                commands[0]
            );
        }
    }

    #[tokio::test]
    async fn v201_get_log_supersedes_an_in_flight_upload() {
        use ocpp_types::v201::LogEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // First upload is accepted and becomes in flight.
        let first = get_log_response(&cp, LogEnumType::DiagnosticsLog, 1).await;
        assert_eq!(first.status, LogStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_log_upload().await, Some(1));

        // A second GetLog with a DIFFERENT requestId supersedes the first.
        let second = get_log_response(&cp, LogEnumType::SecurityLog, 2).await;
        assert_eq!(second.status, LogStatusEnumType::AcceptedCanceled);
        assert_eq!(second.filename, Some("security_2.log".to_string()));
        assert_eq!(
            cp.in_flight_log_upload().await,
            Some(2),
            "the superseding request becomes the one in flight"
        );

        // Each accept — the fresh one and the supersede — queued its own upload
        // stream, correlated by its requestId.
        let ids: Vec<i32> = v201_drain_commands(&cp)
            .await
            .into_iter()
            .filter_map(|c| match c {
                RemoteCommand::V201LogUpload { request_id } => Some(request_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec![1, 2],
            "fresh accept then supersede each queue an upload"
        );
    }

    #[tokio::test]
    async fn v201_get_log_retry_of_the_same_request_is_idempotent() {
        use ocpp_types::v201::LogEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        let first = get_log_response(&cp, LogEnumType::DiagnosticsLog, 5).await;
        assert_eq!(first.status, LogStatusEnumType::Accepted);

        // The SAME requestId again (a retry) is idempotently Accepted, same file,
        // no spurious cancel — the in-flight request is unchanged.
        let retry = get_log_response(&cp, LogEnumType::DiagnosticsLog, 5).await;
        assert_eq!(retry.status, LogStatusEnumType::Accepted);
        assert_eq!(retry.filename, first.filename);
        assert_eq!(cp.in_flight_log_upload().await, Some(5));

        // The retry must NOT queue a second upload stream — the original is still
        // streaming. Only the first accept queued one (requestId 5).
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "a retry queues no second upload; only the original accept did"
        );
        assert!(
            matches!(&commands[0], RemoteCommand::V201LogUpload { request_id } if *request_id == 5),
            "the single queued upload is the original, got: {:?}",
            commands[0]
        );
    }

    #[tokio::test]
    async fn v201_log_upload_completes_and_returns_the_station_to_idle() {
        use ocpp_types::v201::LogEnumType;
        // AC #526: after the async upload settles, the store is idle again, so a
        // subsequent GetLog is a FRESH Accepted — not an AcceptedCanceled against
        // a phantom upload. Drive the state machine directly (the CP is not
        // connected, so the LogStatusNotification CALLs fail-and-warn, but the
        // store transitions are what this asserts).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // Accept an upload, then run its async stream to completion.
        let first = get_log_response(&cp, LogEnumType::DiagnosticsLog, 5).await;
        assert_eq!(first.status, LogStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_log_upload().await, Some(5));
        cp.run_v201_log_upload(5).await;
        assert_eq!(
            cp.in_flight_log_upload().await,
            None,
            "a settled upload clears the store back to idle"
        );

        // A subsequent GetLog now starts fresh — Accepted, not AcceptedCanceled.
        let next = get_log_response(&cp, LogEnumType::SecurityLog, 6).await;
        assert_eq!(
            next.status,
            LogStatusEnumType::Accepted,
            "after completion the next GetLog is fresh, not a supersede"
        );
        assert_eq!(cp.in_flight_log_upload().await, Some(6));
    }

    #[tokio::test]
    async fn v201_log_upload_superseded_does_not_clear_the_newer_upload() {
        // A superseded upload settling must not wipe the newer upload's slot: the
        // compare-and-clear leaves the store to whoever currently owns it. Model
        // the serial-consumer supersede: requestId 1 begins, requestId 2
        // supersedes it (now in flight), then 1's async task finishes.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.v201_log_uploads.begin(1).await;
        cp.v201_log_uploads.begin(2).await; // 2 supersedes 1; slot is now 2's.

        cp.run_v201_log_upload(1).await; // the superseded upload settles...
        assert_eq!(
            cp.in_flight_log_upload().await,
            Some(2),
            "a superseded upload settling leaves the newer upload in flight"
        );

        cp.run_v201_log_upload(2).await; // ...then the owner settles.
        assert_eq!(
            cp.in_flight_log_upload().await,
            None,
            "the owning upload settles the store back to idle"
        );
    }

    #[tokio::test]
    async fn v201_get_log_tolerates_an_extreme_request_id() {
        use ocpp_types::v201::LogEnumType;
        // An `i32::MIN` requestId must not panic; it is accepted and named.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body = get_log_response(&cp, LogEnumType::SecurityLog, i32::MIN).await;
        assert_eq!(body.status, LogStatusEnumType::Accepted);
        let filename = body.filename.expect("named");
        assert!(!filename.is_empty());
        assert!(filename.len() <= 255);
        assert_eq!(cp.in_flight_log_upload().await, Some(i32::MIN));
    }

    #[tokio::test]
    async fn v201_get_log_is_v201_only() {
        use ocpp_types::v201::LogEnumType;
        // A 1.6J CP has no GetLog handler, so the v201-only action is unrouted →
        // CallError, never a CallResult.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_log(
                LogEnumType::DiagnosticsLog,
                1,
            )))
            .await
            .unwrap();
        assert!(
            matches!(resp, Some(Message::CallError(_))),
            "a 1.6J CP does not answer GetLog with a CallResult, got: {resp:?}"
        );
    }

    // --- OCPP 2.0.1 UpdateFirmware (M7, issue #532) ------------------------
    // A `for_version(V201)` CP acks a firmware-update request synchronously off
    // its `V201FirmwareUpdateStore` (in-flight `requestId` tracker), the direct
    // sibling of the GetLog supersede model: idle → Accepted; the same requestId
    // → idempotent Accepted; a different requestId → AcceptedCanceled (supersede);
    // a present-but-malformed signing certificate → InvalidCertificate (nothing
    // recorded). The 1.6J arm keeps its status-less empty conf. These exercise the
    // wired V201 arm end-to-end over `handle_message` — accept + record, retry,
    // supersede, InvalidCertificate, extreme requestId, and the V201-vs-1.6J split.

    fn make_v201_update_firmware(
        request_id: i32,
        signing_certificate: Option<&str>,
    ) -> CallMessage {
        make_call(V201UpdateFirmwareRequest {
            request_id,
            firmware: ocpp_types::v201::FirmwareType {
                location: "https://firmware.example.test/image.bin".to_string(),
                retrieve_date_time: "2026-08-20T00:00:00Z".to_string(),
                install_date_time: None,
                signing_certificate: signing_certificate.map(str::to_string),
                signature: None,
                custom_data: None,
            },
            retries: None,
            retry_interval: None,
            custom_data: None,
        })
    }

    /// Read the v201 `UpdateFirmware.conf` out of a `handle_message` reply,
    /// asserting the reply is a CALLRESULT.
    async fn v201_update_firmware_response(
        cp: &ChargePoint,
        request_id: i32,
        signing_certificate: Option<&str>,
    ) -> ocpp_messages::v201::UpdateFirmwareResponse {
        let resp = cp
            .handle_message(Message::Call(make_v201_update_firmware(
                request_id,
                signing_certificate,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => r.payload_as().unwrap(),
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_update_firmware_accepts_when_idle_and_records_in_flight() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        assert_eq!(cp.in_flight_firmware_update().await, None, "starts idle");

        let body = v201_update_firmware_response(&cp, 42, None).await;
        assert_eq!(body.status, UpdateFirmwareStatusEnumType::Accepted);
        assert!(
            body.status_info.is_none(),
            "an accepted update carries no statusInfo"
        );

        // The accept is recorded as the in-flight rollout and queues exactly one
        // V201FirmwareUpdate to drive the async FirmwareStatusNotification stream
        // (#534), correlated by the same requestId.
        assert_eq!(cp.in_flight_firmware_update().await, Some(42));
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "an accepted UpdateFirmware queues one progress stream"
        );
        assert!(
            matches!(&commands[0], RemoteCommand::V201FirmwareUpdate { request_id } if *request_id == 42),
            "the queued update is correlated by requestId, got: {:?}",
            commands[0]
        );
    }

    #[tokio::test]
    async fn v201_update_firmware_accepts_a_well_formed_signing_certificate() {
        // A present, usable PEM signing certificate does not take the
        // InvalidCertificate arm — the update is accepted and recorded.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body = v201_update_firmware_response(&cp, 1, Some(SAMPLE_INSTALL_PEM)).await;
        assert_eq!(body.status, UpdateFirmwareStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_firmware_update().await, Some(1));
    }

    #[tokio::test]
    async fn v201_update_firmware_retry_of_the_same_request_is_idempotent() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        let first = v201_update_firmware_response(&cp, 5, None).await;
        assert_eq!(first.status, UpdateFirmwareStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_firmware_update().await, Some(5));

        // The SAME requestId again (a retry) is idempotently Accepted — no spurious
        // cancel — and the in-flight request is unchanged.
        let retry = v201_update_firmware_response(&cp, 5, None).await;
        assert_eq!(retry.status, UpdateFirmwareStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_firmware_update().await, Some(5));

        // The retry must NOT queue a second progress stream — the original is still
        // streaming. Only the first accept queued one (requestId 5).
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "a retry queues no second update; only the original accept did"
        );
        assert!(
            matches!(&commands[0], RemoteCommand::V201FirmwareUpdate { request_id } if *request_id == 5),
            "the single queued update is the original, got: {:?}",
            commands[0]
        );
    }

    #[tokio::test]
    async fn v201_update_firmware_supersedes_a_different_in_flight_update() {
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        let first = v201_update_firmware_response(&cp, 1, None).await;
        assert_eq!(first.status, UpdateFirmwareStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_firmware_update().await, Some(1));

        // A second UpdateFirmware with a DIFFERENT requestId supersedes the first.
        let second = v201_update_firmware_response(&cp, 2, None).await;
        assert_eq!(
            second.status,
            UpdateFirmwareStatusEnumType::AcceptedCanceled
        );
        assert_eq!(
            cp.in_flight_firmware_update().await,
            Some(2),
            "the superseding request becomes the one in flight"
        );

        // Each accept — the fresh one and the supersede — queued its own progress
        // stream, correlated by its requestId.
        let ids: Vec<i32> = v201_drain_commands(&cp)
            .await
            .into_iter()
            .filter_map(|c| match c {
                RemoteCommand::V201FirmwareUpdate { request_id } => Some(request_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec![1, 2],
            "fresh accept then supersede each queue an update"
        );
    }

    #[tokio::test]
    async fn v201_update_firmware_rejects_a_malformed_signing_certificate() {
        // A present-but-unusable signing certificate is refused with
        // InvalidCertificate + a statusInfo reason, and nothing is recorded — an
        // untrusted image never becomes the in-flight rollout.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body = v201_update_firmware_response(&cp, 7, Some("not a certificate")).await;
        assert_eq!(
            body.status,
            UpdateFirmwareStatusEnumType::InvalidCertificate
        );
        assert!(
            body.status_info.is_some(),
            "an InvalidCertificate carries a statusInfo reason"
        );
        assert_eq!(
            cp.in_flight_firmware_update().await,
            None,
            "a refused update records nothing"
        );
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_update_firmware_tolerates_an_extreme_request_id() {
        // An `i32::MIN` requestId must not panic; it is accepted and recorded.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body = v201_update_firmware_response(&cp, i32::MIN, None).await;
        assert_eq!(body.status, UpdateFirmwareStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_firmware_update().await, Some(i32::MIN));
    }

    #[tokio::test]
    async fn v201_update_firmware_version_split_preserves_the_1_6j_empty_conf() {
        // The core of #532: the two versions share the action name "UpdateFirmware"
        // but answer different shapes. A V201 CP answers the status-carrying 2.0.1
        // conf; a V16J CP still answers the empty 1.6J conf (no regression), routed
        // to the preserved 1.6J handler.

        // V201 arm: a status-carrying response, and the request recorded in flight.
        let v201 = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let body = v201_update_firmware_response(&v201, 1, None).await;
        assert_eq!(body.status, UpdateFirmwareStatusEnumType::Accepted);
        assert_eq!(v201.in_flight_firmware_update().await, Some(1));

        // V16J arm: the 1.6J UpdateFirmware still answers its empty conf `{}`, and
        // has no v201 firmware-update tracker (always idle).
        let v16 = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V16J)).unwrap();
        let call = make_call(UpdateFirmwareRequest {
            location: "ftp://firmware.example.test/image.bin".to_string(),
            retries: None,
            retrieve_date: chrono::Utc::now(),
            retry_interval: None,
        });
        let resp = v16.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                assert_eq!(
                    r.payload,
                    serde_json::json!({}),
                    "the 1.6J UpdateFirmware answers the empty conf (no status field)"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            v16.in_flight_firmware_update().await,
            None,
            "the 1.6J path has no v201 firmware-update tracker"
        );
    }

    #[tokio::test]
    async fn v201_firmware_update_completes_and_returns_the_station_to_idle() {
        // AC #534: after the async rollout settles, the store is idle again, so a
        // subsequent UpdateFirmware is a FRESH Accepted — not an AcceptedCanceled
        // against a phantom rollout. Drive the state machine directly (the CP is not
        // connected, so the FirmwareStatusNotification CALLs fail-and-warn, but the
        // store transitions are what this asserts).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // Accept a rollout, then run its async stream to completion.
        let first = v201_update_firmware_response(&cp, 5, None).await;
        assert_eq!(first.status, UpdateFirmwareStatusEnumType::Accepted);
        assert_eq!(cp.in_flight_firmware_update().await, Some(5));
        cp.run_v201_firmware_update(5).await;
        assert_eq!(
            cp.in_flight_firmware_update().await,
            None,
            "a settled rollout clears the store back to idle"
        );

        // A subsequent UpdateFirmware now starts fresh — Accepted, not
        // AcceptedCanceled against a phantom rollout.
        let next = v201_update_firmware_response(&cp, 6, None).await;
        assert_eq!(
            next.status,
            UpdateFirmwareStatusEnumType::Accepted,
            "after completion the next UpdateFirmware is fresh, not a supersede"
        );
        assert_eq!(cp.in_flight_firmware_update().await, Some(6));
    }

    #[tokio::test]
    async fn v201_firmware_update_superseded_does_not_clear_the_newer_update() {
        // A superseded rollout settling must not wipe the newer rollout's slot: the
        // compare-and-clear leaves the store to whoever currently owns it. Model the
        // serial-consumer supersede: requestId 1 begins, requestId 2 supersedes it
        // (now in flight), then 1's async task finishes.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.v201_firmware_updates.begin(1).await;
        cp.v201_firmware_updates.begin(2).await; // 2 supersedes 1; slot is now 2's.

        cp.run_v201_firmware_update(1).await; // the superseded rollout settles...
        assert_eq!(
            cp.in_flight_firmware_update().await,
            Some(2),
            "a superseded rollout settling leaves the newer one in flight"
        );

        cp.run_v201_firmware_update(2).await; // ...then the owner settles.
        assert_eq!(
            cp.in_flight_firmware_update().await,
            None,
            "the owning rollout settles the store back to idle"
        );
    }

    #[tokio::test]
    async fn v201_firmware_update_failure_branches_still_settle_to_idle() {
        // Both fault-injection outcomes settle the owning rollout back to idle (the
        // failed terminal is still a completion for slot purposes), so a subsequent
        // UpdateFirmware is a fresh Accepted rather than a supersede against a
        // phantom. Exercises the DownloadFailed early-return and the
        // InstallationFailed terminal, each on its own CP.
        for outcome in [
            FirmwareUpdateOutcome::DownloadFailed,
            FirmwareUpdateOutcome::InstallationFailed,
        ] {
            let cp = ChargePoint::new(ChargePointConfig {
                firmware_update_outcome: outcome,
                ..ChargePointConfig::for_version(OcppVersion::V201)
            })
            .unwrap();
            cp.v201_firmware_updates.begin(9).await;
            cp.run_v201_firmware_update(9).await;
            assert_eq!(
                cp.in_flight_firmware_update().await,
                None,
                "a {outcome:?} rollout still settles the store back to idle"
            );
        }
    }

    // --- OCPP 2.0.1 GetDisplayMessages (M7, issue #508) --------------------
    // A `for_version(V201)` CP answers the query synchronously (Accepted /
    // Unknown) off a snapshot of its `V201DisplayMessageStore`, and queues a
    // `V201NotifyDisplayMessages` command carrying the match set on Accepted.
    // These exercise the wired V201 arm end-to-end over `handle_message`.

    fn make_v201_get_display_messages(
        request_id: i32,
        id: Option<Vec<i32>>,
        priority: Option<ocpp_types::v201::MessagePriorityEnumType>,
        state: Option<ocpp_types::v201::MessageStateEnumType>,
    ) -> CallMessage {
        make_call(V201GetDisplayMessagesRequest {
            request_id,
            id,
            priority,
            state,
            custom_data: None,
        })
    }

    /// Install a display message on the CP via the wired `SetDisplayMessage`
    /// handler, so the `GetDisplayMessages` tests query a store populated exactly
    /// the way a CSMS would populate it.
    async fn install_v201_display_message(cp: &ChargePoint, message: MessageInfoType) {
        cp.handle_message(Message::Call(make_v201_set_display_message(message)))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn v201_get_display_messages_on_empty_store_is_unknown_and_queues_nothing() {
        use ocpp_types::v201::GetDisplayMessagesStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        // No messages installed → the station has nothing to report.
        let resp = cp
            .handle_message(Message::Call(make_v201_get_display_messages(
                1, None, None, None,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetDisplayMessagesResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GetDisplayMessagesStatusEnumType::Unknown);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "an Unknown query queues no NotifyDisplayMessages"
        );
    }

    #[tokio::test]
    async fn v201_get_display_messages_streams_the_installed_messages() {
        use ocpp_types::v201::GetDisplayMessagesStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_v201_display_message(&cp, make_v201_display_message(10, None)).await;
        install_v201_display_message(&cp, make_v201_display_message(20, None)).await;

        // An unfiltered query matches both → Accepted, one V201NotifyDisplayMessages
        // command carrying the match set, correlated by requestId.
        let resp = cp
            .handle_message(Message::Call(make_v201_get_display_messages(
                701, None, None, None,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetDisplayMessagesResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GetDisplayMessagesStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(
            commands.len(),
            1,
            "one NotifyDisplayMessages stream is queued"
        );
        match &commands[0] {
            RemoteCommand::V201NotifyDisplayMessages {
                request_id,
                messages,
            } => {
                assert_eq!(*request_id, 701);
                let mut ids: Vec<i32> = messages.iter().map(|m| m.id).collect();
                ids.sort_unstable();
                assert_eq!(ids, vec![10, 20]);
            }
            other => panic!("expected V201NotifyDisplayMessages, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_display_messages_id_filter_narrows_the_stream() {
        use ocpp_types::v201::GetDisplayMessagesStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        for id in [1, 2, 3] {
            install_v201_display_message(&cp, make_v201_display_message(id, None)).await;
        }
        // Filter to id ∈ {2, 99}: 2 is installed, 99 is not → only 2 streams.
        let resp = cp
            .handle_message(Message::Call(make_v201_get_display_messages(
                5,
                Some(vec![2, 99]),
                None,
                None,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetDisplayMessagesResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GetDisplayMessagesStatusEnumType::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        let commands = v201_drain_commands(&cp).await;
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            RemoteCommand::V201NotifyDisplayMessages { messages, .. } => {
                assert_eq!(messages.iter().map(|m| m.id).collect::<Vec<_>>(), vec![2]);
            }
            other => panic!("expected V201NotifyDisplayMessages, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_display_messages_unknown_id_filter_is_unknown_and_queues_nothing() {
        use ocpp_types::v201::GetDisplayMessagesStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        for id in [1, 2, 3] {
            install_v201_display_message(&cp, make_v201_display_message(id, None)).await;
        }
        // A filter naming nothing installed → Unknown, nothing queued (never a
        // panic on the CSMS-supplied id).
        let resp = cp
            .handle_message(Message::Call(make_v201_get_display_messages(
                6,
                Some(vec![404]),
                None,
                None,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetDisplayMessagesResponse = r.payload_as().unwrap();
                assert_eq!(body.status, GetDisplayMessagesStatusEnumType::Unknown);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[test]
    fn v201_get_display_messages_request_is_schema_valid() {
        use ocpp_types::v201::{MessagePriorityEnumType, MessageStateEnumType};
        let validator = SchemaValidator::v201();
        // Both the minimal (requestId only) and fully-filtered request shapes the
        // CSMS can send are schema-valid.
        for req in [
            V201GetDisplayMessagesRequest {
                request_id: 1,
                id: None,
                priority: None,
                state: None,
                custom_data: None,
            },
            V201GetDisplayMessagesRequest {
                request_id: 2,
                id: Some(vec![1, 2, 3]),
                priority: Some(MessagePriorityEnumType::AlwaysFront),
                state: Some(MessageStateEnumType::Charging),
                custom_data: None,
            },
        ] {
            validator
                .validate_call("GetDisplayMessages", &serde_json::to_value(&req).unwrap())
                .expect("GetDisplayMessages request is schema-valid");
        }
    }

    // --- OCPP 2.0.1 ClearDisplayMessage (M7, issue #509) -------------------
    // A `for_version(V201)` CP removes one installed display message by id from
    // the same `V201DisplayMessageStore` SetDisplayMessage populates, answering
    // `Accepted` when the id was installed and removed, `Unknown` when it named
    // nothing. Pure remove-and-answer — nothing is queued. These exercise the
    // wired V201 arm end-to-end over `handle_message`.

    fn make_v201_clear_display_message(id: i32) -> CallMessage {
        make_call(V201ClearDisplayMessageRequest {
            id,
            custom_data: None,
        })
    }

    /// Read the `ClearDisplayMessage.conf` status out of a `handle_message` reply,
    /// asserting the reply is a CALLRESULT.
    async fn clear_display_message_status(
        cp: &ChargePoint,
        id: i32,
    ) -> ocpp_types::v201::ClearMessageStatusEnumType {
        let resp = cp
            .handle_message(Message::Call(make_v201_clear_display_message(id)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::ClearDisplayMessageResponse =
                    r.payload_as().unwrap();
                body.status
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_clear_display_message_removes_an_installed_message() {
        use ocpp_types::v201::ClearMessageStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_v201_display_message(&cp, make_v201_display_message(7, None)).await;
        install_v201_display_message(&cp, make_v201_display_message(8, None)).await;

        // Clearing an installed id → Accepted.
        assert_eq!(
            clear_display_message_status(&cp, 7).await,
            ClearMessageStatusEnumType::Accepted
        );
        // Removing the message is a pure side effect — nothing is queued (the
        // single-shot `v201_drain_commands` may be called once per CP).
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "ClearDisplayMessage queues no command"
        );

        // The clear is scoped to exactly the requested id: id 7 is gone from the
        // store, while its sibling id 8 is untouched. Read the store directly
        // (non-destructively) rather than through a second GetDisplayMessages, so
        // this assertion does not depend on the command channel the drain above
        // already consumed.
        assert!(
            cp.installed_display_message(7).await.is_none(),
            "the cleared id 7 no longer sees a message"
        );
        assert!(
            cp.installed_display_message(8).await.is_some(),
            "clearing id 7 must not touch id 8"
        );
    }

    #[tokio::test]
    async fn v201_clear_display_message_unknown_id_is_unknown_and_never_panics() {
        use ocpp_types::v201::ClearMessageStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_v201_display_message(&cp, make_v201_display_message(1, None)).await;

        // An id naming nothing installed → Unknown, no panic — including the
        // extreme wire ids a CSMS could send.
        for id in [404, -1, i32::MIN, i32::MAX] {
            assert_eq!(
                clear_display_message_status(&cp, id).await,
                ClearMessageStatusEnumType::Unknown,
                "clearing an uninstalled id {id} answers Unknown"
            );
        }
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_clear_display_message_is_idempotent_on_re_clear() {
        use ocpp_types::v201::ClearMessageStatusEnumType;
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_v201_display_message(&cp, make_v201_display_message(5, None)).await;

        // First clear removes it (Accepted); a second clear of the same id finds
        // nothing (Unknown) — remove-and-answer is idempotent.
        assert_eq!(
            clear_display_message_status(&cp, 5).await,
            ClearMessageStatusEnumType::Accepted
        );
        assert_eq!(
            clear_display_message_status(&cp, 5).await,
            ClearMessageStatusEnumType::Unknown,
            "a re-clear of an already-removed id is Unknown"
        );
    }

    #[tokio::test]
    async fn v201_clear_display_message_response_is_schema_valid() {
        // The station's synchronous CALLRESULT for both an installed and an
        // unknown id is OCPP 2.0.1 schema-valid over the wire.
        let validator = SchemaValidator::v201();
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        install_v201_display_message(&cp, make_v201_display_message(3, None)).await;
        for id in [3, 999] {
            let resp = cp
                .handle_message(Message::Call(make_v201_clear_display_message(id)))
                .await
                .unwrap();
            match resp.unwrap() {
                Message::CallResult(r) => {
                    let body: ocpp_messages::v201::ClearDisplayMessageResponse =
                        r.payload_as().unwrap();
                    validator
                        .validate_call_result(
                            "ClearDisplayMessage",
                            &serde_json::to_value(&body).unwrap(),
                        )
                        .expect("ClearDisplayMessage response is schema-valid");
                }
                other => panic!("expected CallResult, got: {other:?}"),
            }
        }
    }

    // --- OCPP 2.0.1 SetMonitoringBase (M7, issue #501) --------------------
    // A `for_version(V201)` CP selects its active monitoring base on the same
    // shared `V201DeviceModel` the monitoring family uses. These exercise the
    // wired V201 arm end-to-end over `handle_message`.

    fn make_v201_set_monitoring_base(base: ocpp_types::v201::MonitorBaseEnumType) -> CallMessage {
        make_call(V201SetMonitoringBaseRequest {
            monitoring_base: base,
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_set_monitoring_base_all_and_factory_default_are_accepted() {
        use ocpp_types::v201::{GenericDeviceModelStatusEnumType, MonitorBaseEnumType};
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        for base in [
            MonitorBaseEnumType::All,
            MonitorBaseEnumType::FactoryDefault,
        ] {
            let resp = cp
                .handle_message(Message::Call(make_v201_set_monitoring_base(base)))
                .await
                .unwrap();
            match resp.unwrap() {
                Message::CallResult(r) => {
                    let body: ocpp_messages::v201::SetMonitoringBaseResponse =
                        r.payload_as().unwrap();
                    assert_eq!(
                        body.status,
                        GenericDeviceModelStatusEnumType::Accepted,
                        "base {base:?} is modeled and must be Accepted",
                    );
                    // Accepted carries no statusInfo.
                    assert!(body.status_info.is_none());
                }
                other => panic!("expected CallResult, got: {other:?}"),
            }
        }
        // A pure set queues no side-effect command.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[tokio::test]
    async fn v201_set_monitoring_base_hard_wired_only_is_not_supported() {
        use ocpp_types::v201::{GenericDeviceModelStatusEnumType, MonitorBaseEnumType};
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_set_monitoring_base(
                MonitorBaseEnumType::HardWiredOnly,
            )))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::SetMonitoringBaseResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    GenericDeviceModelStatusEnumType::NotSupported,
                    "no hard-wired monitors are modeled: HardWiredOnly is NotSupported",
                );
                let info = body
                    .status_info
                    .expect("a NotSupported answer carries a statusInfo reason");
                assert_eq!(info.reason_code, "NotSupported");
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // No side-effect command is queued even on the modeled-seam path.
        assert!(v201_drain_commands(&cp).await.is_empty());
    }

    #[test]
    fn v201_set_monitoring_base_request_and_response_are_schema_valid() {
        use ocpp_types::v201::{
            GenericDeviceModelStatusEnumType, MonitorBaseEnumType, StatusInfoType,
        };
        let validator = SchemaValidator::v201();
        // A request for each base is schema-valid.
        for base in [
            MonitorBaseEnumType::All,
            MonitorBaseEnumType::FactoryDefault,
            MonitorBaseEnumType::HardWiredOnly,
        ] {
            let req = V201SetMonitoringBaseRequest {
                monitoring_base: base,
                custom_data: None,
            };
            validator
                .validate_call("SetMonitoringBase", &serde_json::to_value(&req).unwrap())
                .expect("SetMonitoringBase request is schema-valid");
        }

        // Both answer shapes the handler emits — Accepted (no statusInfo) and
        // NotSupported (with the reason it emits for HardWiredOnly) — serialize to
        // a schema-valid response.
        for (status, status_info) in [
            (GenericDeviceModelStatusEnumType::Accepted, None),
            (
                GenericDeviceModelStatusEnumType::NotSupported,
                Some(StatusInfoType {
                    reason_code: "NotSupported".to_string(),
                    additional_info: Some(
                        "HardWiredOnly base is not modeled: no hard-wired monitors exist"
                            .to_string(),
                    ),
                    custom_data: None,
                }),
            ),
        ] {
            let resp = ocpp_messages::v201::SetMonitoringBaseResponse {
                status,
                status_info,
                custom_data: None,
            };
            validator
                .validate_call_result("SetMonitoringBase", &serde_json::to_value(&resp).unwrap())
                .expect("SetMonitoringBase response is schema-valid");
        }
    }

    // --- GetTransactionStatus (M7, issue #490) -----------------------------
    // A `for_version(V201)` CP answers the 2.0.1-only GetTransactionStatus query
    // against the live `active_transactions` set. These exercise the wired V201
    // arm end-to-end over `handle_message`.

    fn make_v201_get_transaction_status(transaction_id: Option<&str>) -> CallMessage {
        make_call(V201GetTransactionStatusRequest {
            transaction_id: transaction_id.map(ToString::to_string),
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_get_transaction_status_station_wide_omits_ongoing_indicator() {
        // A station-wide query (no transactionId) is registered (the response
        // parses, it is not an unrecognized action), reports messagesInQueue =
        // false (modeled: no offline queue yet), omits ongoingIndicator, and
        // queues nothing.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_get_transaction_status(None)))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetTransactionStatusResponse =
                    r.payload_as().unwrap();
                assert!(!body.messages_in_queue);
                assert_eq!(
                    body.ongoing_indicator, None,
                    "a station-wide query omits ongoingIndicator"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "GetTransactionStatus is a pure read — it queues nothing"
        );
    }

    #[tokio::test]
    async fn v201_get_transaction_status_reports_ongoing_for_a_live_transaction() {
        // With transaction 7 live on the station, a query for "7" reports
        // ongoingIndicator = Some(true).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_insert_active_transaction(7, ConnectorId::new(1).unwrap())
            .await;
        let resp = cp
            .handle_message(Message::Call(make_v201_get_transaction_status(Some("7"))))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ocpp_messages::v201::GetTransactionStatusResponse =
                    r.payload_as().unwrap();
                assert_eq!(
                    body.ongoing_indicator,
                    Some(true),
                    "a live transaction is reported ongoing"
                );
                assert!(!body.messages_in_queue);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn v201_get_transaction_status_unknown_or_noncanonical_id_is_not_ongoing() {
        // An id the station is not running — an unknown id, or a non-canonical
        // spelling of a live one ("07" ≠ "7") — reports ongoingIndicator =
        // Some(false) and never panics (trust boundary: exact string match, no
        // numeric parse).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_insert_active_transaction(7, ConnectorId::new(1).unwrap())
            .await;
        for probe in ["999", "07"] {
            let resp = cp
                .handle_message(Message::Call(make_v201_get_transaction_status(Some(probe))))
                .await
                .unwrap();
            match resp.unwrap() {
                Message::CallResult(r) => {
                    let body: ocpp_messages::v201::GetTransactionStatusResponse =
                        r.payload_as().unwrap();
                    assert_eq!(
                        body.ongoing_indicator,
                        Some(false),
                        "id {probe:?} is not live → Some(false)"
                    );
                }
                other => panic!("expected CallResult, got: {other:?}"),
            }
        }
    }

    // --- CostUpdated (M7, issue #502) --------------------------------------
    // A `for_version(V201)` CP records the running total cost the CSMS pushes for
    // an ongoing transaction and answers with an empty `CostUpdatedResponse`.
    // These exercise the wired V201 arm end-to-end over `handle_message`.

    fn make_v201_cost_updated(total_cost: f64, transaction_id: &str) -> CallMessage {
        make_call(V201CostUpdatedRequest {
            total_cost,
            transaction_id: transaction_id.to_string(),
            custom_data: None,
        })
    }

    #[tokio::test]
    async fn v201_cost_updated_records_cost_for_a_live_transaction() {
        // With transaction 7 live, a CostUpdated for "7" is acknowledged with an
        // empty response, records the cost (read back exactly), and queues
        // nothing (the only side effect is the in-memory upsert).
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_insert_active_transaction(7, ConnectorId::new(1).unwrap())
            .await;
        assert_eq!(cp.recorded_transaction_cost("7").await, None);

        let resp = cp
            .handle_message(Message::Call(make_v201_cost_updated(12.5, "7")))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                // The acknowledgement parses as a CostUpdatedResponse (the action
                // is registered) and serializes to an empty body.
                let body: ocpp_messages::v201::CostUpdatedResponse = r.payload_as().unwrap();
                assert_eq!(serde_json::to_value(&body).unwrap(), serde_json::json!({}));
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.recorded_transaction_cost("7").await,
            Some(12.5),
            "the pushed cost is recorded against the live transaction"
        );
        assert!(
            v201_drain_commands(&cp).await.is_empty(),
            "CostUpdated only upserts in memory — it queues nothing"
        );
    }

    #[tokio::test]
    async fn v201_cost_updated_same_id_overwrites_with_the_latest_figure() {
        // The running total moves forward over a session: a later CostUpdated for
        // the same transaction replaces the earlier figure rather than stacking.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.test_insert_active_transaction(7, ConnectorId::new(1).unwrap())
            .await;
        cp.handle_message(Message::Call(make_v201_cost_updated(12.0, "7")))
            .await
            .unwrap();
        cp.handle_message(Message::Call(make_v201_cost_updated(18.25, "7")))
            .await
            .unwrap();
        assert_eq!(
            cp.recorded_transaction_cost("7").await,
            Some(18.25),
            "the latest figure wins"
        );
    }

    #[tokio::test]
    async fn v201_cost_updated_unknown_id_is_recorded_pending() {
        // OCPP defines no rejection for CostUpdated, so a cost for a transaction
        // the station is not running is still acknowledged (empty response) and
        // recorded pending rather than dropped — never a panic on the opaque id.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        let resp = cp
            .handle_message(Message::Call(make_v201_cost_updated(3.0, "not-live")))
            .await
            .unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let _body: ocpp_messages::v201::CostUpdatedResponse = r.payload_as().unwrap();
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.recorded_transaction_cost("not-live").await,
            Some(3.0),
            "an unknown id is recorded pending, not dropped"
        );
    }

    #[tokio::test]
    async fn v201_cost_updated_request_and_response_are_schema_valid() {
        // The request the CSMS sends and the response the station builds both
        // satisfy the bundled OCPP 2.0.1 CostUpdated schemas.
        let validator = SchemaValidator::v201();
        let req = V201CostUpdatedRequest {
            total_cost: 42.75,
            transaction_id: "7".to_string(),
            custom_data: None,
        };
        validator
            .validate_call("CostUpdated", &serde_json::to_value(&req).unwrap())
            .expect("CostUpdated request is schema-valid");
        let resp = v201_command::v201_cost_updated_response();
        validator
            .validate_call_result("CostUpdated", &serde_json::to_value(&resp).unwrap())
            .expect("CostUpdated response is schema-valid");
    }

    // --- OCPP 2.0.1 Local Authorization List (M7, issue #485) --------------
    // A `for_version(V201)` CP answers `SendLocalList` / `GetLocalListVersion`
    // in the 2.0.1 dialect against the one shared list store. These exercise the
    // wired V201 arms end-to-end over `handle_message`.

    fn v201_local_list_entry(
        id: &str,
        status: ocpp_types::v201::AuthorizationStatusEnumType,
    ) -> ocpp_types::v201::AuthorizationData {
        ocpp_types::v201::AuthorizationData {
            id_token: ocpp_types::v201::IdTokenType {
                id_token: id.to_string(),
                kind: ocpp_types::v201::IdTokenEnumType::Iso14443,
                additional_info: None,
                custom_data: None,
            },
            id_token_info: Some(ocpp_types::v201::IdTokenInfoType {
                status,
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
        }
    }

    #[tokio::test]
    async fn v201_send_local_list_full_then_get_local_list_version() {
        // A Full `SendLocalList` is `Accepted` and bumps the stored version; a
        // following `GetLocalListVersion` reports that new version — both in the
        // 2.0.1 dialect.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        // Fresh station reports version 0.
        let get0 = make_call(V201GetLocalListVersionRequest::default());
        match cp
            .handle_message(Message::Call(get0))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallResult(r) => {
                let body: V201GetLocalListVersionResponse = r.payload_as().unwrap();
                assert_eq!(body.version_number, 0, "a fresh list is at version 0");
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }

        let send = make_call(V201SendLocalListRequest {
            version_number: 42,
            update_type: ocpp_types::v201::UpdateEnumType::Full,
            local_authorization_list: Some(vec![
                v201_local_list_entry(
                    "TAG-A",
                    ocpp_types::v201::AuthorizationStatusEnumType::Accepted,
                ),
                v201_local_list_entry(
                    "TAG-B",
                    ocpp_types::v201::AuthorizationStatusEnumType::Blocked,
                ),
            ]),
            custom_data: None,
        });
        match cp
            .handle_message(Message::Call(send))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallResult(r) => {
                let body: V201SendLocalListResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::SendLocalListStatusEnumType::Accepted
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }

        let get = make_call(V201GetLocalListVersionRequest::default());
        match cp
            .handle_message(Message::Call(get))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallResult(r) => {
                let body: V201GetLocalListVersionResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.version_number, 42,
                    "GetLocalListVersion reports the version the accepted update set"
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        // The shared store holds the two entries under their idToken keys.
        assert_eq!(cp.local_list().len(), 2);
    }

    #[tokio::test]
    async fn v201_send_local_list_stale_differential_is_version_mismatch() {
        // A Differential update that does not advance the version is
        // `VersionMismatch` over the wire and leaves the stored version intact.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();

        let seed = make_call(V201SendLocalListRequest {
            version_number: 5,
            update_type: ocpp_types::v201::UpdateEnumType::Full,
            local_authorization_list: Some(vec![v201_local_list_entry(
                "TAG-A",
                ocpp_types::v201::AuthorizationStatusEnumType::Accepted,
            )]),
            custom_data: None,
        });
        cp.handle_message(Message::Call(seed)).await.unwrap();

        let stale = make_call(V201SendLocalListRequest {
            version_number: 5, // not strictly greater than the current version
            update_type: ocpp_types::v201::UpdateEnumType::Differential,
            local_authorization_list: Some(vec![v201_local_list_entry(
                "TAG-Z",
                ocpp_types::v201::AuthorizationStatusEnumType::Accepted,
            )]),
            custom_data: None,
        });
        match cp
            .handle_message(Message::Call(stale))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallResult(r) => {
                let body: V201SendLocalListResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::SendLocalListStatusEnumType::VersionMismatch
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert_eq!(
            cp.local_list().version(),
            5,
            "a rejected update does not bump the version"
        );
        assert_eq!(
            cp.local_list().get("TAG-Z"),
            None,
            "nothing partially applied"
        );
    }

    #[tokio::test]
    async fn v201_clear_cache_empties_the_shared_cache() {
        // A 2.0.1 `ClearCache` empties the same shared Authorization Cache the
        // 1.6J path clears, and answers `Accepted` in the 2.0.1 dialect.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        cp.auth_cache.insert("TAG001", accepted_info());
        cp.auth_cache.insert("TAG002", accepted_info());
        assert_eq!(cp.auth_cache.len(), 2);

        let call = make_call(V201ClearCacheRequest::default());
        match cp
            .handle_message(Message::Call(call))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallResult(r) => {
                let body: V201ClearCacheResponse = r.payload_as().unwrap();
                assert_eq!(
                    body.status,
                    ocpp_types::v201::ClearCacheStatusEnumType::Accepted
                );
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
        assert!(
            cp.auth_cache.is_empty(),
            "ClearCache should empty the shared cache"
        );
    }

    #[tokio::test]
    async fn v201_clear_cache_on_empty_cache_is_idempotent() {
        // Clearing an already-empty cache is a no-op that still reports
        // `Accepted` — never a panic. Two clears in a row prove idempotency.
        let cp = ChargePoint::new(ChargePointConfig::for_version(OcppVersion::V201)).unwrap();
        assert!(cp.auth_cache.is_empty());

        for _ in 0..2 {
            let call = make_call(V201ClearCacheRequest::default());
            match cp
                .handle_message(Message::Call(call))
                .await
                .unwrap()
                .unwrap()
            {
                Message::CallResult(r) => {
                    let body: V201ClearCacheResponse = r.payload_as().unwrap();
                    assert_eq!(
                        body.status,
                        ocpp_types::v201::ClearCacheStatusEnumType::Accepted
                    );
                }
                other => panic!("expected CallResult, got: {other:?}"),
            }
            assert!(cp.auth_cache.is_empty());
        }
    }

    #[tokio::test]
    async fn default_change_availability_returns_accepted() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(ChangeAvailabilityRequest {
            connector_id: 1,
            availability_type: AvailabilityType::Operative,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        let result = resp.unwrap();
        match result {
            Message::CallResult(r) => {
                let body: ChangeAvailabilityResponse = r.payload_as().unwrap();
                assert_eq!(body.status, AvailabilityStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_trigger_message_supported_accepted() {
        // A supported requestedMessage is queued on the command channel (whose
        // receiver still lives, pre-connect) and accepted.
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(TriggerMessageRequest {
            requested_message: MessageTrigger::Heartbeat,
            connector_id: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: TriggerMessageResponse = r.payload_as().unwrap();
                assert_eq!(body.status, TriggerMessageStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_trigger_message_meter_values_accepted() {
        // On-demand MeterValues is now a supported requestedMessage: the CALL is
        // accepted and the send is queued on the command channel (Issue #65).
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(TriggerMessageRequest {
            requested_message: MessageTrigger::MeterValues,
            connector_id: Some(1),
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: TriggerMessageResponse = r.payload_as().unwrap();
                assert_eq!(body.status, TriggerMessageStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_trigger_message_firmware_status_accepted() {
        // `FirmwareStatusNotification` is now a supported trigger (Issue #70,
        // closing the deferred half of #65): the CP has a firmware-update state
        // machine to report against, so the request is Accepted. (Every OCPP
        // 1.6J MessageTrigger variant is supported; the NotImplemented path is a
        // defensive fallback for hypothetical future variants.)
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(TriggerMessageRequest {
            requested_message: MessageTrigger::FirmwareStatusNotification,
            connector_id: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: TriggerMessageResponse = r.payload_as().unwrap();
                assert_eq!(body.status, TriggerMessageStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_change_configuration_writable_key_accepted() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(ChangeConfigurationRequest {
            key: "HeartbeatInterval".to_string(),
            value: "120".to_string(),
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        let result = resp.unwrap();
        match result {
            Message::CallResult(r) => {
                let body: ChangeConfigurationResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ConfigurationStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_change_configuration_readonly_key_rejected() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(ChangeConfigurationRequest {
            key: "NumberOfConnectors".to_string(),
            value: "5".to_string(),
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ChangeConfigurationResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ConfigurationStatus::Rejected);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_get_configuration_returns_all_keys_when_none_specified() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(GetConfigurationRequest { key: None });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: GetConfigurationResponse = r.payload_as().unwrap();
                assert!(body.configuration_keys.is_some());
                let keys = body.configuration_keys.unwrap();
                assert!(!keys.is_empty());
                // HeartbeatInterval should be in the default store
                assert!(keys.iter().any(|k| k.key == "HeartbeatInterval"));
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_get_configuration_unknown_key_appears_in_unknown_list() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(GetConfigurationRequest {
            key: Some(vec![
                "HeartbeatInterval".to_string(),
                "NoSuchKey".to_string(),
            ]),
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: GetConfigurationResponse = r.payload_as().unwrap();
                let cfg = body.configuration_keys.unwrap();
                assert_eq!(cfg.len(), 1);
                assert_eq!(cfg[0].key, "HeartbeatInterval");
                let unknown = body.unknown_keys.unwrap();
                assert_eq!(unknown, vec!["NoSuchKey"]);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_remote_start_returns_accepted() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(RemoteStartTransactionRequest {
            connector_id: Some(1),
            id_tag: "abc".to_string(),
            charging_profile: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: RemoteStartTransactionResponse = r.payload_as().unwrap();
                assert_eq!(body.status, RemoteStartStopStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn remote_start_unknown_connector_returns_rejected() {
        // Default config has connectors 1..=2; connector 7 does not exist, so the
        // CP cannot honor a remote start there (OCPP 1.6J §5.11).
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(RemoteStartTransactionRequest {
            connector_id: Some(7),
            id_tag: "abc".to_string(),
            charging_profile: None,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: RemoteStartTransactionResponse = r.payload_as().unwrap();
                assert_eq!(body.status, RemoteStartStopStatus::Rejected);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn remote_stop_unknown_transaction_returns_rejected() {
        // No transaction is active, so transaction id 42 is unknown and the CP
        // must reject the remote stop (OCPP 1.6J §5.12).
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(RemoteStopTransactionRequest { transaction_id: 42 });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: RemoteStopTransactionResponse = r.payload_as().unwrap();
                assert_eq!(body.status, RemoteStartStopStatus::Rejected);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_reset_returns_accepted() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(ResetRequest {
            reset_type: ResetType::Soft,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ResetResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ResetStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_action_returns_callerror_not_supported() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = CallMessage::new("NoSuchAction".to_string(), serde_json::json!({})).unwrap();
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallError(e) => {
                assert_eq!(e.error_code, CallErrorCode::NotSupported);
            }
            other => panic!("expected CallError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn user_on_handler_overrides_default() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();

        // Override the default Reset handler to return Rejected
        cp.on(|_req: ResetRequest| async move {
            Ok(ResetResponse {
                status: ResetStatus::Rejected,
            })
        })
        .await;

        let call = make_call(ResetRequest {
            reset_type: ResetType::Hard,
        });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: ResetResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ResetStatus::Rejected);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn after_hook_fires_on_successful_dispatch() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();

        cp.after(move |_req: ResetRequest| {
            let f = f.clone();
            async move {
                f.store(true, Ordering::SeqCst);
            }
        })
        .await;

        let call = make_call(ResetRequest {
            reset_type: ResetType::Soft,
        });
        cp.handle_message(Message::Call(call)).await.unwrap();

        // Allow the spawned after hook to run
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(fired.load(Ordering::SeqCst), "after hook did not fire");
    }

    #[tokio::test]
    async fn callresult_and_callerror_return_none() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();

        let result_msg = ocpp_messages::CallResultMessage::new(
            "some-id".to_string(),
            HeartbeatResponse {
                current_time: chrono::Utc::now(),
            },
        )
        .unwrap();
        let res = cp
            .handle_message(Message::CallResult(result_msg))
            .await
            .unwrap();
        assert!(res.is_none());

        let err_msg = CallErrorMessage::new(
            "some-id".to_string(),
            CallErrorCode::InternalError,
            "test".to_string(),
            None,
        );
        let res = cp
            .handle_message(Message::CallError(err_msg))
            .await
            .unwrap();
        assert!(res.is_none());
    }

    // --- boot sequence tests ---
    //
    // These tests use an in-process tokio-tungstenite server (no subprotocol
    // negotiation needed — the client does not enforce it).
    //
    // Python reference: ocpp/charge_point.py, examples/v16/charge_point.py

    #[test]
    fn max_boot_retries_config_default_is_3() {
        let config = ChargePointConfig::default();
        assert_eq!(config.max_boot_retries, 3);
    }

    #[test]
    fn boot_rejected_error_display_and_clone() {
        let err = OcppError::BootRejected { attempts: 4 };
        let msg = err.to_string();
        assert!(
            msg.contains("4"),
            "message should mention attempt count: {msg}"
        );
        assert_eq!(err.clone(), err);
    }

    /// Spawn a mock CSMS that routes each incoming CALL by action name and
    /// responds with the configured payload. Unknown actions receive no reply,
    /// except `StatusNotification`, which is always answered with an empty
    /// CALLRESULT — the charge point now emits these automatically during the
    /// boot and transaction lifecycle (Issue #28), so every connected-session
    /// test would otherwise hang on the un-routed notification.
    ///
    /// This is the action-routing sibling of `spawn_mock_csms` — it handles
    /// messages out-of-order and can serve multiple concurrent actions (e.g.
    /// BootNotification + Authorize + StartTransaction in the same session).
    async fn spawn_mock_csms_routing(
        routes: std::collections::HashMap<String, serde_json::Value>,
    ) -> std::net::SocketAddr {
        use futures_util::{SinkExt, StreamExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                while let Some(Ok(frame)) = ws.next().await {
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = frame {
                        if let Ok(Message::Call(call)) = serde_json::from_str::<Message>(&text) {
                            let payload = routes.get(&call.action).cloned().or_else(|| {
                                (call.action == "StatusNotification").then(|| serde_json::json!({}))
                            });
                            if let Some(payload) = payload {
                                let result = Message::CallResult(CallResultMessage {
                                    message_type: MessageType::CallResult,
                                    unique_id: call.unique_id,
                                    payload,
                                });
                                let json = serde_json::to_string(&result).unwrap();
                                let _ = ws
                                    .send(tokio_tungstenite::tungstenite::Message::Text(json))
                                    .await;
                            }
                        }
                    }
                }
            }
        });

        addr
    }

    // --- id_tag_info_expired() helper (Issue #104) ---

    fn id_tag_info(expiry: Option<chrono::DateTime<chrono::Utc>>) -> IdTagInfo {
        IdTagInfo {
            status: AuthorizationStatus::Accepted,
            parent_id_tag: None,
            expiry_date: expiry,
        }
    }

    #[test]
    fn id_tag_info_without_expiry_never_expires() {
        assert!(!id_tag_info_expired(&id_tag_info(None)));
    }

    #[test]
    fn id_tag_info_with_future_expiry_is_fresh() {
        let future = chrono::Utc::now() + chrono::Duration::seconds(3600);
        assert!(!id_tag_info_expired(&id_tag_info(Some(future))));
    }

    #[test]
    fn id_tag_info_with_past_expiry_is_stale() {
        let past = chrono::Utc::now() - chrono::Duration::seconds(10);
        assert!(id_tag_info_expired(&id_tag_info(Some(past))));
    }

    // --- authorize() tests (Issue #21) ---
    // Python ref: ocpp/charge_point.py _send_authorize()

    #[tokio::test]
    async fn authorize_accepted_returns_id_tag_info() {
        use ocpp_types::common::AuthorizationStatus;
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "Authorize".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Accepted"}}),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let info = cp.authorize("TAG001").await.unwrap();
        assert_eq!(info.status, AuthorizationStatus::Accepted);
    }

    #[tokio::test]
    async fn authorize_blocked_returns_blocked_status() {
        use ocpp_types::common::AuthorizationStatus;
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "Authorize".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Blocked"}}),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let info = cp.authorize("BLOCKED_TAG").await.unwrap();
        assert_eq!(info.status, AuthorizationStatus::Blocked);
    }

    // --- authorization cache tests (Issue #23) ---
    // Python ref: ocpp/charge_point.py (cache-then-call); OCPP 1.6J §3.1.

    fn accepted_info() -> IdTagInfo {
        IdTagInfo {
            status: AuthorizationStatus::Accepted,
            parent_id_tag: None,
            expiry_date: None,
        }
    }

    #[tokio::test]
    async fn authorize_cache_hit_does_not_send_ocpp_call() {
        // Pre-seed an Accepted entry. The CP is never connected, so if
        // authorize() tried to send a CALL it would error on the missing client.
        // A clean Accepted return therefore proves the cache short-circuited.
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        cp.auth_cache.insert("TAG001", accepted_info());

        let info = cp.authorize("TAG001").await.unwrap();
        assert_eq!(info.status, AuthorizationStatus::Accepted);
        assert!(
            !cp.is_connected().await,
            "test relies on the CP being offline"
        );
    }

    #[tokio::test]
    async fn authorize_cache_miss_sends_ocpp_call_and_caches_result() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "Authorize".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Accepted"}}),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        // Cold cache → CALL is sent → result returned and cached.
        assert!(cp.auth_cache.get("TAG001").is_none());
        let info = cp.authorize("TAG001").await.unwrap();
        assert_eq!(info.status, AuthorizationStatus::Accepted);
        let cached = cp
            .auth_cache
            .get("TAG001")
            .expect("result should be cached");
        assert_eq!(cached.status, AuthorizationStatus::Accepted);
    }

    #[tokio::test]
    async fn authorize_expired_entry_is_evicted_and_call_sent() {
        // A stale Accepted entry must not be used; the CP re-checks with the
        // CSMS, and the fresh (Blocked) result replaces the stale row.
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "Authorize".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Blocked"}}),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let past = chrono::Utc::now() - chrono::Duration::seconds(10);
        cp.auth_cache.insert(
            "TAG001",
            IdTagInfo {
                status: AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: Some(past),
            },
        );

        let info = cp.authorize("TAG001").await.unwrap();
        // Stale Accepted ignored; CSMS verdict (Blocked) used and now cached.
        assert_eq!(info.status, AuthorizationStatus::Blocked);
        let refreshed = cp.auth_cache.get_stale("TAG001").unwrap();
        assert_eq!(refreshed.status, AuthorizationStatus::Blocked);
    }

    #[tokio::test]
    async fn clear_cache_command_empties_auth_cache() {
        // Drive a real ClearCache CALL through the live dispatcher and assert it
        // empties the authorization cache (Issue #23 wiring).
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        cp.auth_cache.insert("TAG001", accepted_info());
        cp.auth_cache.insert("TAG002", accepted_info());
        assert_eq!(cp.auth_cache.len(), 2);

        let call = Message::Call(CallMessage {
            message_type: MessageType::Call,
            unique_id: "cc-1".to_string(),
            action: "ClearCache".to_string(),
            payload: serde_json::json!({}),
        });
        let response = cp.handle_message(call).await.unwrap();

        match response {
            Some(Message::CallResult(r)) => {
                assert_eq!(r.payload["status"], "Accepted");
            }
            other => panic!("expected ClearCache CALLRESULT, got {other:?}"),
        }
        assert!(
            cp.auth_cache.is_empty(),
            "ClearCache should empty the cache"
        );
    }

    #[tokio::test]
    async fn authorize_csms_timeout_with_stale_ok_returns_cached() {
        // CSMS accepts the WebSocket and answers BootNotification but never
        // answers Authorize → the CALL times out. With offline_auth_stale_ok the
        // CP honors a stale-but-Accepted cached entry.
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        // Intentionally no "Authorize" route → no response → timeout.
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            call_timeout: 1, // keep the test fast
            offline_auth_stale_ok: true,
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let past = chrono::Utc::now() - chrono::Duration::seconds(10);
        cp.auth_cache.insert(
            "TAG001",
            IdTagInfo {
                status: AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: Some(past),
            },
        );

        let info = cp.authorize("TAG001").await.unwrap();
        assert_eq!(info.status, AuthorizationStatus::Accepted);
    }

    #[tokio::test]
    async fn authorize_csms_timeout_without_stale_ok_fails_safe_invalid() {
        // Same timeout, but offline_auth_stale_ok is false (default) → fail safe
        // with Invalid even though a stale Accepted entry exists.
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            call_timeout: 1,
            offline_auth_stale_ok: false,
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let past = chrono::Utc::now() - chrono::Duration::seconds(10);
        cp.auth_cache.insert(
            "TAG001",
            IdTagInfo {
                status: AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: Some(past),
            },
        );

        let info = cp.authorize("TAG001").await.unwrap();
        assert_eq!(info.status, AuthorizationStatus::Invalid);
    }

    #[test]
    fn auth_cache_config_defaults() {
        let config = ChargePointConfig::default();
        assert_eq!(config.auth_cache_ttl, 24 * 60 * 60);
        assert!(!config.offline_auth_stale_ok);
    }

    // --- start_transaction() tests (Issue #21) ---
    // Python ref: examples/v16/charge_point.py send_start_transaction()

    #[tokio::test]
    async fn start_transaction_sends_request_and_stores_csms_id() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "StartTransaction".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Accepted"}, "transactionId": 42}),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let connector_id = ConnectorId::new(1).unwrap();
        let txn_id = cp
            .start_transaction(connector_id, "TAG001", 0)
            .await
            .unwrap();

        assert_eq!(txn_id, 42, "should use CSMS-assigned transaction ID");
        // Connector transitions to Charging
        let connector = cp.get_connector(connector_id).await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Charging);
    }

    #[tokio::test]
    async fn start_transaction_blocked_id_tag_returns_authorization_error() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "StartTransaction".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Blocked"}, "transactionId": 0}),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let result = cp
            .start_transaction(ConnectorId::new(1).unwrap(), "BLOCKED", 0)
            .await;
        assert!(
            matches!(result, Err(OcppError::Authorization { .. })),
            "expected Authorization error, got: {result:?}"
        );
    }

    // --- stop_transaction() tests (Issue #21) ---
    // Python ref: examples/v16/charge_point.py send_stop_transaction()

    #[tokio::test]
    async fn stop_transaction_sends_request_and_transitions_connector() {
        use ocpp_types::common::Reason;

        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "StartTransaction".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Accepted"}, "transactionId": 99}),
        );
        routes.insert("StopTransaction".to_string(), serde_json::json!({}));
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let connector_id = ConnectorId::new(1).unwrap();
        let txn_id = cp
            .start_transaction(connector_id, "TAG001", 0)
            .await
            .unwrap();
        assert_eq!(txn_id, 99);

        cp.stop_transaction(txn_id, 1000, Reason::Local)
            .await
            .unwrap();

        // Connector back to Available
        let connector = cp.get_connector(connector_id).await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Available);

        // Second stop on same ID → NotFound
        let result = cp.stop_transaction(txn_id, 1000, Reason::Local).await;
        assert!(
            matches!(result, Err(OcppError::NotFound { .. })),
            "expected NotFound after transaction removed, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn stop_transaction_unknown_id_returns_not_found() {
        use ocpp_types::common::Reason;
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let result = cp.stop_transaction(999, 0, Reason::Local).await;
        assert!(
            matches!(result, Err(OcppError::NotFound { .. })),
            "expected NotFound for unknown transaction ID, got: {result:?}"
        );
    }

    // --- schema validation wiring (Issue #33) ---
    //
    // Python ref: ocpp/charge_point.py `_handle_call()` / `_handle_call_result()`
    // both run `_validate()`. Incoming malformed CALLs become CALLERRORs;
    // malformed CALLRESULTs from the CSMS are rejected before deserialization.

    #[test]
    fn validate_payloads_default_is_true() {
        let config = ChargePointConfig::default();
        assert!(config.validate_payloads);
    }

    #[test]
    fn firmware_update_outcome_defaults_to_succeed() {
        // Fault injection must be opt-in: the default config takes the happy
        // path so existing simulator behavior is unchanged (Issue #83).
        let config = ChargePointConfig::default();
        assert_eq!(
            config.firmware_update_outcome,
            FirmwareUpdateOutcome::Succeed
        );
        assert_eq!(
            FirmwareUpdateOutcome::default(),
            FirmwareUpdateOutcome::Succeed
        );
    }

    #[tokio::test]
    async fn validator_present_by_default_absent_when_disabled() {
        let on = ChargePoint::new(ChargePointConfig::default()).unwrap();
        assert!(on.validator.is_some());
        assert!(on.dispatcher.read().await.has_validator());

        let off = ChargePoint::new(ChargePointConfig {
            validate_payloads: false,
            ..Default::default()
        })
        .unwrap();
        assert!(off.validator.is_none());
        assert!(!off.dispatcher.read().await.has_validator());
    }

    #[tokio::test]
    async fn incoming_malformed_call_returns_callerror_formation_violation() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();

        // Schema-valid `type` but an extra property the schema forbids
        // (additionalProperties: false). Deserializes fine via serde — only
        // the validator catches it — so this proves dispatch-time validation
        // is active, not just serde.
        let call = CallMessage::new(
            "Reset".to_string(),
            serde_json::json!({ "type": "Soft", "unexpected": 1 }),
        )
        .unwrap();

        match cp
            .handle_message(Message::Call(call))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallError(e) => {
                assert_eq!(e.error_code, CallErrorCode::FormationViolation);
            }
            other => panic!("expected CallError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn incoming_call_type_failure_returns_type_constraint_violation() {
        // A `type` schema failure (`connectorId` as a string) must surface as
        // CALLERROR `TypeConstraintViolation` through the full dispatch path,
        // per `_validate_payload()` in the Python reference.
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = CallMessage::new(
            "ChangeAvailability".to_string(),
            serde_json::json!({ "connectorId": "not-an-int", "type": "Operative" }),
        )
        .unwrap();

        match cp
            .handle_message(Message::Call(call))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallError(e) => {
                assert_eq!(e.error_code, CallErrorCode::TypeConstraintViolation);
            }
            other => panic!("expected CallError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn incoming_call_required_failure_returns_protocol_error() {
        // A missing required field is a `required` schema failure → CALLERROR
        // `ProtocolError` ("Payload for Action is incomplete").
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = CallMessage::new(
            "ChangeAvailability".to_string(),
            serde_json::json!({ "connectorId": 1 }), // missing required `type`
        )
        .unwrap();

        match cp
            .handle_message(Message::Call(call))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallError(e) => {
                assert_eq!(e.error_code, CallErrorCode::ProtocolError);
            }
            other => panic!("expected CallError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn validation_disabled_lets_extra_property_call_through() {
        // Same payload, but with validation disabled the extra property is
        // ignored by serde and the handler runs normally.
        let cp = ChargePoint::new(ChargePointConfig {
            validate_payloads: false,
            ..Default::default()
        })
        .unwrap();

        let call = CallMessage::new(
            "Reset".to_string(),
            serde_json::json!({ "type": "Soft", "unexpected": 1 }),
        )
        .unwrap();

        match cp
            .handle_message(Message::Call(call))
            .await
            .unwrap()
            .unwrap()
        {
            Message::CallResult(r) => {
                let body: ResetResponse = r.payload_as().unwrap();
                assert_eq!(body.status, ResetStatus::Accepted);
            }
            other => panic!("expected CallResult, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn call_result_validation_rejects_malformed_response() {
        // CSMS returns an empty AuthorizeResponse — missing the required
        // `idTagInfo`. With validation on, `call()` rejects it as a
        // ValidationError before deserialization.
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert("Authorize".to_string(), serde_json::json!({}));
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let err = cp.authorize("TAG001").await.unwrap_err();
        // Empty AuthorizeResponse is missing the required `idTagInfo` field.
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "expected SchemaViolation for malformed CALLRESULT, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn call_result_validation_disabled_accepts_schema_invalid_response() {
        // Response carries an extra property (schema additionalProperties:
        // false) that serde ignores. With validation disabled the call
        // succeeds; with validation on it would be rejected — proving the
        // CALLRESULT validation path is gated by `validate_payloads`.
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "Authorize".to_string(),
            serde_json::json!({ "idTagInfo": { "status": "Accepted" }, "extra": 7 }),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            validate_payloads: false,
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let info = cp.authorize("TAG001").await.unwrap();
        assert_eq!(info.status, AuthorizationStatus::Accepted);
    }

    #[tokio::test]
    async fn call_validates_outbound_request_before_send() {
        // Port of `test_v16_charge_point.py::test_send_invalid_call`: a
        // schema-invalid request must be rejected locally by `call()` before it
        // reaches the wire. The reference builds `Reset(type="Medium")`; the
        // strongly-typed Rust model can't express that, so we use the
        // equivalent schema-but-not-type violation — an `idTag` longer than the
        // Authorize schema's `maxLength: 20`.
        //
        // The charge point is intentionally NOT connected: outbound validation
        // runs before the connection check, so an invalid payload surfaces as a
        // `SchemaViolation` rather than the "Not connected" transport error.
        // Getting `SchemaViolation` (not `Transport`) is exactly what proves the
        // request was validated *before* any send attempt.
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let err = cp
            .call(AuthorizeRequest {
                id_tag: "A".repeat(21), // 21 chars > maxLength 20
            })
            .await
            .unwrap_err();
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "expected SchemaViolation for an over-long idTag, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn call_valid_outbound_request_passes_validation_then_hits_transport() {
        // A schema-valid, non-trivial request must clear outbound validation and
        // proceed to the send path — where, on a disconnected charge point, it
        // fails with the transport "Not connected" error. This proves outbound
        // validation does not reject well-formed requests.
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let err = cp
            .call(AuthorizeRequest {
                id_tag: "TAG001".to_string(),
            })
            .await
            .unwrap_err();
        assert!(
            matches!(err, OcppError::Transport { .. }),
            "expected Transport error for a valid request on a disconnected CP, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn call_outbound_validation_skipped_when_disabled() {
        // With `validate_payloads: false` — the Rust analog of the reference's
        // `skip_schema_validation=True`
        // (`test_v16_charge_point.py::test_call_skip_schema_validation`) — the
        // same over-long idTag is NOT rejected locally: `call()` skips outbound
        // validation and proceeds to the send path, failing only with the
        // transport error. Proves the gate is honored in both directions.
        let cp = ChargePoint::new(ChargePointConfig {
            validate_payloads: false,
            ..Default::default()
        })
        .unwrap();
        let err = cp
            .call(AuthorizeRequest {
                id_tag: "A".repeat(21),
            })
            .await
            .unwrap_err();
        assert!(
            matches!(err, OcppError::Transport { .. }),
            "expected Transport error (validation skipped), got: {err:?}"
        );
    }

    /// Spawn a mock CSMS that responds to each BootNotification CALL with a
    /// pre-determined payload. Returns the bound address.
    ///
    /// The mock uses the same serde wire encoding as the `WebSocketClient`
    /// (struct with numeric-renamed fields, not the OCPP JSON-array form).
    /// It accepts a single WS connection and handles `responses.len()`
    /// consecutive CALL messages, replying with the matching CALLRESULT each time.
    async fn spawn_mock_csms(responses: Vec<serde_json::Value>) -> std::net::SocketAddr {
        use futures_util::{SinkExt, StreamExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let mut resp_iter = responses.into_iter();
                while let Some(Ok(frame)) = ws.next().await {
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = frame {
                        // Parse using the same serde schema the transport uses.
                        if let Ok(Message::Call(call)) = serde_json::from_str::<Message>(&text) {
                            // Auto-answer lifecycle `StatusNotification` frames
                            // (Issue #28) with an empty CALLRESULT without
                            // consuming a queued response, so the ordered
                            // `responses` stay aligned with the actions the test
                            // actually exercises.
                            let payload = if call.action == "StatusNotification" {
                                Some(serde_json::json!({}))
                            } else {
                                resp_iter.next()
                            };
                            if let Some(payload) = payload {
                                let result = Message::CallResult(CallResultMessage {
                                    message_type: MessageType::CallResult,
                                    unique_id: call.unique_id,
                                    payload,
                                });
                                let json = serde_json::to_string(&result).unwrap();
                                ws.send(tokio_tungstenite::tungstenite::Message::Text(json))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
            }
        });

        addr
    }

    /// Build a `BootNotificationResponse` JSON payload.
    fn boot_response(status: &str, interval: i32) -> serde_json::Value {
        serde_json::json!({
            "currentTime": "2024-01-01T00:00:00Z",
            "interval": interval,
            "status": status
        })
    }

    #[tokio::test]
    async fn boot_accepted_sets_accepted_status() {
        let addr = spawn_mock_csms(vec![boot_response("Accepted", 60)]).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();

        cp.connect().await.expect("connect should succeed");

        assert_eq!(cp.registration_status().await, RegistrationStatus::Accepted);
        assert!(cp.is_connected().await);
    }

    #[tokio::test]
    async fn boot_pending_sets_pending_status() {
        let addr = spawn_mock_csms(vec![boot_response("Pending", 30)]).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();

        cp.connect()
            .await
            .expect("Pending should be treated as success");

        assert_eq!(cp.registration_status().await, RegistrationStatus::Pending);
    }

    #[tokio::test]
    async fn boot_accepted_fires_boot_notification_event() {
        let addr = spawn_mock_csms(vec![boot_response("Accepted", 60)]).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        let mut events = cp.take_event_receiver().await.unwrap();

        cp.connect().await.unwrap();

        // Drain events until we find BootNotificationAccepted.
        let mut got_boot_accepted = false;
        while let Ok(evt) = events.try_recv() {
            if matches!(evt, ChargePointEvent::BootNotificationAccepted { .. }) {
                got_boot_accepted = true;
            }
        }
        assert!(got_boot_accepted, "expected BootNotificationAccepted event");
    }

    // NOTE: integration tests for the Rejected retry path (multiple BootNotifications
    // over a single WS connection) are skipped here because the current transport
    // layer holds a mutex over the entire `receive_message().await` call, which
    // prevents the send task from sending the 2nd BootNotification during the
    // retry sleep. This is a known transport limitation; fixing it (split WS
    // sink/stream, remove shared mutex) is tracked as a follow-up to Issue #20.
    // The retry STATE MACHINE is tested through the unit tests above (config,
    // error variant) and through the accepted/pending integration tests below.

    #[tokio::test]
    async fn boot_accepted_starts_heartbeat_task() {
        let addr = spawn_mock_csms(vec![boot_response("Accepted", 60)]).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();

        cp.connect().await.unwrap();

        // The heartbeat task handle should be set after a successful boot.
        let has_heartbeat = cp.heartbeat_handle.read().await.is_some();
        assert!(has_heartbeat, "heartbeat task should be running after boot");
    }

    // --- MeterValues periodic sampling tests (Issue #22) ---
    // Python ref: ocpp/charge_point.py periodic-task pattern; ocpp/v16/call.py MeterValues

    /// Spawn a mock CSMS that records every received CALL over a channel (so a
    /// test can observe periodic `MeterValues` frames) while still answering the
    /// configured actions so the charge point can reach the `Charging` state.
    async fn spawn_recording_csms(
        routes: std::collections::HashMap<String, serde_json::Value>,
    ) -> (std::net::SocketAddr, mpsc::UnboundedReceiver<CallMessage>) {
        use futures_util::{SinkExt, StreamExt};
        use tokio::net::TcpListener;

        let (tx, rx) = mpsc::unbounded_channel::<CallMessage>();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                while let Some(Ok(frame)) = ws.next().await {
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = frame {
                        if let Ok(Message::Call(call)) = serde_json::from_str::<Message>(&text) {
                            // Record the call for the test to observe, then reply.
                            let _ = tx.send(call.clone());
                            // Auto-answer lifecycle `StatusNotification` frames
                            // (Issue #28) with an empty CALLRESULT so connected
                            // sessions don't stall; they're still recorded above
                            // and simply skipped by `recv_until_action`.
                            let payload = routes.get(&call.action).cloned().or_else(|| {
                                (call.action == "StatusNotification").then(|| serde_json::json!({}))
                            });
                            if let Some(payload) = payload {
                                let result = Message::CallResult(CallResultMessage {
                                    message_type: MessageType::CallResult,
                                    unique_id: call.unique_id,
                                    payload,
                                });
                                let json = serde_json::to_string(&result).unwrap();
                                let _ = ws
                                    .send(tokio_tungstenite::tungstenite::Message::Text(json))
                                    .await;
                            }
                        }
                    }
                }
            }
        });

        (addr, rx)
    }

    /// Receive from a recording CSMS channel until a CALL with `action` arrives,
    /// skipping unrelated frames (BootNotification, StartTransaction, …). Fails
    /// the test rather than hanging if the frame never arrives.
    async fn recv_until_action(
        rx: &mut mpsc::UnboundedReceiver<CallMessage>,
        action: &str,
    ) -> CallMessage {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await {
                    Some(call) if call.action == action => return call,
                    Some(_) => continue,
                    None => panic!("recording CSMS channel closed before a {action} frame"),
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for a {action} frame"))
    }

    fn start_routes() -> std::collections::HashMap<String, serde_json::Value> {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "BootNotification".to_string(),
            boot_response("Accepted", 3600),
        );
        routes.insert(
            "StartTransaction".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Accepted"}, "transactionId": 77}),
        );
        routes
    }

    #[tokio::test]
    async fn start_transaction_spawns_meter_sampler() {
        let addr = spawn_mock_csms_routing(start_routes()).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let txn_id = cp
            .start_transaction(ConnectorId::new(1).unwrap(), "TAG001", 0)
            .await
            .unwrap();

        assert!(
            cp.meter_sampler_handles.read().await.contains_key(&txn_id),
            "a MeterValues sampler should be registered for the active transaction"
        );
    }

    #[tokio::test]
    async fn stop_transaction_aborts_meter_sampler() {
        use ocpp_types::common::Reason;

        let mut routes = start_routes();
        routes.insert(
            "StopTransaction".to_string(),
            serde_json::json!({"idTagInfo": {"status": "Accepted"}}),
        );
        let addr = spawn_mock_csms_routing(routes).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();

        let txn_id = cp
            .start_transaction(ConnectorId::new(1).unwrap(), "TAG001", 0)
            .await
            .unwrap();
        assert!(cp.meter_sampler_handles.read().await.contains_key(&txn_id));

        cp.stop_transaction(txn_id, 1000, Reason::Local)
            .await
            .unwrap();

        assert!(
            !cp.meter_sampler_handles.read().await.contains_key(&txn_id),
            "the sampler should be cancelled and removed after stop_transaction"
        );
    }

    #[tokio::test]
    async fn meter_values_begin_snapshot_has_transaction_begin_context_and_measurand() {
        // A long interval so only the immediate Transaction.Begin snapshot fires.
        let (addr, mut rx) = spawn_recording_csms(start_routes()).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            meter_values_interval: 60,
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();
        cp.start_transaction(ConnectorId::new(1).unwrap(), "TAG001", 0)
            .await
            .unwrap();

        let mv = recv_until_action(&mut rx, "MeterValues").await;
        let sampled = &mv.payload["meterValue"][0]["sampledValue"][0];
        assert_eq!(mv.payload["connectorId"], 1);
        assert_eq!(mv.payload["transactionId"], 77);
        assert_eq!(
            sampled["context"].as_str(),
            Some("Transaction.Begin"),
            "first snapshot must use the Transaction.Begin context"
        );
        assert_eq!(
            sampled["measurand"].as_str(),
            Some("Energy.Active.Import.Register"),
            "default measurand should be the active-import energy register"
        );
        assert_eq!(sampled["unit"].as_str(), Some("Wh"));
    }

    #[tokio::test]
    async fn meter_values_sent_periodically_with_sample_periodic_context() {
        // 1-second interval: Transaction.Begin at t=0, then Sample.Periodic at t=1.
        let (addr, mut rx) = spawn_recording_csms(start_routes()).await;
        let cp = ChargePoint::new(ChargePointConfig {
            central_system_url: format!("ws://{addr}"),
            meter_values_interval: 1,
            ..Default::default()
        })
        .unwrap();
        cp.connect().await.unwrap();
        cp.start_transaction(ConnectorId::new(1).unwrap(), "TAG001", 0)
            .await
            .unwrap();

        let begin = recv_until_action(&mut rx, "MeterValues").await;
        assert_eq!(
            begin.payload["meterValue"][0]["sampledValue"][0]["context"].as_str(),
            Some("Transaction.Begin")
        );

        let periodic = recv_until_action(&mut rx, "MeterValues").await;
        assert_eq!(
            periodic.payload["meterValue"][0]["sampledValue"][0]["context"].as_str(),
            Some("Sample.Periodic"),
            "subsequent frames at the configured interval must use Sample.Periodic"
        );
    }
}
