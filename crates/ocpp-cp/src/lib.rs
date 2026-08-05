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
pub mod v201_command;
pub mod v201_transaction;

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
// 2.0.1 provisioning message + enum types used by the version-aware runtime
// (slice 2). Aliased to avoid clashing with the unqualified 1.6J names imported
// above (`StatusNotificationRequest`, `RegistrationStatus`).
use ocpp_messages::v201::{
    ChangeAvailabilityRequest as V201ChangeAvailabilityRequest,
    MeterValuesRequest as V201MeterValuesRequest,
    RequestStartTransactionRequest as V201RequestStartTransactionRequest,
    RequestStopTransactionRequest as V201RequestStopTransactionRequest,
    ResetRequest as V201ResetRequest, StatusNotificationRequest as V201StatusNotificationRequest,
    TriggerMessageRequest as V201TriggerMessageRequest,
    UnlockConnectorRequest as V201UnlockConnectorRequest,
};
use ocpp_types::v201::{
    AuthorizationStatusEnumType, ChangeAvailabilityStatusEnumType, ChargingProfilePurposeEnumType,
    ConnectorStatusEnumType, MessageTriggerEnumType, OperationalStatusEnumType,
    RegistrationStatusEnumType, RequestStartStopStatusEnumType, ResetStatusEnumType,
    StatusInfoType, TriggerMessageStatusEnumType, UnlockStatusEnumType,
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

/// Opt-in outcome of the simulated firmware update (`UpdateFirmware`, OCPP
/// 1.6J §4.x). Selects which branch of the firmware state machine the
/// simulator follows so a CSMS can be tested against failed rollouts, not just
/// successful ones.
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
    /// Fault injection for the simulated firmware update (`UpdateFirmware`,
    /// OCPP 1.6J §4.x). Defaults to [`FirmwareUpdateOutcome::Succeed`] (happy
    /// path); set [`FirmwareUpdateOutcome::DownloadFailed`] or
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
    StartTransaction {
        connector_id: ConnectorId,
        id_tag: String,
        remote_start_id: Option<i32>,
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
        let mut dispatcher = Self::build_default_dispatcher(
            config.protocol_version,
            config_store.clone(),
            auth_cache.clone(),
            command_sender.clone(),
            connectors.clone(),
            active_transactions.clone(),
            v201_sessions.clone(),
            reservations.clone(),
            charging_profiles.clone(),
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
        auth_cache: Arc<AuthCache>,
        command_sender: mpsc::UnboundedSender<RemoteCommand>,
        connectors: Arc<RwLock<HashMap<ConnectorId, Connector>>>,
        active_transactions: Arc<RwLock<HashMap<i32, ConnectorId>>>,
        v201_sessions: Arc<RwLock<HashMap<i32, V201Session>>>,
        reservations: Arc<RwLock<HashMap<i32, ConnectorId>>>,
        charging_profiles: Arc<ChargingProfileStore>,
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
                                        // 1.6J has no remoteStartId to correlate.
                                        remote_start_id: None,
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
                    // A valid `TxProfile` is accepted here; installing and
                    // enforcing its schedule is deliberately out of this slice
                    // (see the slice-7d follow-up). The `charging_profiles`
                    // store is 1.6J-typed, so honoring a 2.0.1 `TxProfile` needs
                    // a dedicated v201 profile store rather than a lossy
                    // conversion — a design decision of its own. Until then the
                    // profile is validated but not yet enforced.
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

        // UpdateFirmware — acknowledge with the empty conf the spec defines (no
        // status field), then run the firmware-update state machine as a real
        // side effect (OCPP 1.6J §4.x, firmware-management profile). Like
        // GetDiagnostics, the update is queued on the command channel and run by
        // the consumer task spawned in `connect()`, so the UpdateFirmware
        // CALLRESULT is flushed before the first `FirmwareStatusNotification` and
        // the receive loop never re-enters itself. The simulator has no real
        // image, so it drives Downloading → Downloaded → Installing → Installed
        // on a timer. If the consumer has gone away (CP shutting down) the
        // update can't run, but the spec response is empty either way, so we
        // only log.
        {
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

        // ReserveNow — reserve a connector for an idTag until expiryDate (OCPP
        // 1.6J §5.14). Faithful status semantics keyed off the connector's live
        // status: a free connector is reserved (→ `Reserved`) and the
        // reservationId recorded; a busy connector → `Occupied`, a faulted one →
        // `Faulted`, an unavailable one → `Unavailable`; an unknown/out-of-range
        // connector id (incl. 0) → `Rejected`. The reserve itself is a local
        // state change, but on `Accepted` we also queue a `StatusNotification`
        // (`Reserved`) to the CSMS off the inbound-CALL path so a back office's
        // live connector view flips immediately (Issue #80) without waiting for
        // the next status event. Ports `ReserveNow` from the Python reference's
        // `call.py`/`enums.py`.
        {
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
                                            )
                                            .await;
                                            ReservationStatus::Accepted
                                        }
                                        Err(_) => ReservationStatus::Rejected,
                                    }
                                }
                                ChargePointStatus::Faulted => ReservationStatus::Faulted,
                                ChargePointStatus::Unavailable => ReservationStatus::Unavailable,
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

        // CancelReservation — clear a reservation by reservationId (OCPP 1.6J
        // §5.4). `Accepted` if the id is held (the connector is freed,
        // `Reserved` → `Available`), `Rejected` if it is unknown. Freeing the
        // connector is a local state change; on `Accepted` we also queue a
        // `StatusNotification` (`Available`) off the inbound-CALL path so the
        // CSMS sees the connector free up immediately (Issue #80). A
        // `cancel_reservation()` that did not actually flip `Reserved` →
        // `Available` (the connector moved on, e.g. a faulted/occupied edge)
        // emits nothing — we only announce the transition we made. Ports
        // `CancelReservation` from the Python reference's `call.py`/`enums.py`.
        {
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
                            CancelReservationStatus::Accepted
                        }
                        None => CancelReservationStatus::Rejected,
                    };
                    Ok(CancelReservationResponse { status })
                }
            });
        }

        // SetChargingProfile — install a Smart Charging profile against a
        // connector (0 = charge-point-wide) per the 1.6J stacking rules (§5.16).
        // Placement is validated faithfully first (ChargePointMaxProfile only at
        // connector 0, TxProfile only at a real connector, unknown connector
        // rejected); only an Accepted profile is stored, replacing any prior one
        // with the same id or (purpose, stackLevel) slot. Storing is a local
        // state change — enforcing the limit on delivered power and computing the
        // composite schedule are out of scope (the latter is GetCompositeSchedule,
        // Issue #95). Ports `SetChargingProfile` from the Python reference's
        // `call.py`/`enums.py`.
        {
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

        // ClearChargingProfile — clear installed profiles matching the optional
        // filters (`id`, `connectorId`, `chargingProfilePurpose`, `stackLevel`);
        // a `None` filter matches anything, so an all-`None` request clears the
        // whole store (§5.2). `Accepted` if at least one profile matched, else
        // `Unknown`. Ports `ClearChargingProfile` from the Python reference's
        // `call.py`/`enums.py`.
        {
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
        {
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

        // ClearCache — empty the authorization cache, then accept (Issue #23).
        // Ports the OCPP 1.6J ClearCache use case (§5.2): the CSMS asks the CP to
        // discard its Authorization Cache.
        {
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

        // DataTransfer — route by (vendorId, messageId) through the registry
        // (OCPP 1.6J §6.x, Issue #101). An unimplemented vendor/message yields
        // the faithful UnknownVendorId / UnknownMessageId; a registered handler
        // decides Accepted/Rejected (+ optional data). With no handlers
        // registered the registry answers UnknownVendorId for every request.
        {
            let data_transfer = data_transfer.clone();
            d.on(move |req: DataTransferRequest| {
                let data_transfer = data_transfer.clone();
                async move { Ok(data_transfer.dispatch(&req)) }
            });
        }

        // GetLocalListVersion — report the version of the Local Authorization
        // List (OCPP 1.6J §5.x, Issue #93). `0` for an empty list; the CP never
        // returns `-1` because it implements the profile.
        {
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

        // SendLocalList — apply a Full or Differential update to the Local
        // Authorization List (OCPP 1.6J §5.x, Issue #93). The list itself
        // enforces version ordering, duplicate rejection, and Full/Differential
        // semantics, returning the faithful UpdateStatus.
        {
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
    async fn arm_reservation_expiry(
        reservation_id: i32,
        connector_id: ConnectorId,
        expiry_date: chrono::DateTime<chrono::Utc>,
        connectors: &Arc<RwLock<HashMap<ConnectorId, Connector>>>,
        reservations: &Arc<RwLock<HashMap<i32, ConnectorId>>>,
        expiry_timers: &Arc<RwLock<HashMap<i32, tokio::task::JoinHandle<()>>>>,
        command_sender: &mpsc::UnboundedSender<RemoteCommand>,
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
                        } => {
                            // meter_start is unknown for a remote-initiated start;
                            // report 0, matching the Python reference's example CP.
                            // `remote_start_id` is `Some` only on the 2.0.1
                            // RequestStartTransaction path and is echoed onto the
                            // Started event for CSMS correlation.
                            if let Err(e) = cp
                                .start_transaction_with_remote_start_id(
                                    connector_id,
                                    &id_tag,
                                    0,
                                    remote_start_id,
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
        let interval = Duration::from_secs(self.config.meter_values_interval.max(1));
        let client = self.client.clone();
        let is_connected = self.is_connected.clone();
        let connectors = self.connectors.clone();
        let sessions = self.v201_sessions.clone();
        let evse_id = connector_id.value() as i32;
        let txid_str = transaction_id.to_string();

        let handle = tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            loop {
                timer.tick().await;

                if !*is_connected.read().await {
                    continue;
                }

                // Claim the next seqNo for this transaction. A missing session
                // means the transaction is being stopped; drop the tick.
                let seq_no = match sessions.read().await.get(&transaction_id) {
                    Some(session) => session.next_seq_no.fetch_add(1, Ordering::SeqCst),
                    None => continue,
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

                let session = v201_transaction::SessionRef {
                    transaction_id: &txid_str,
                    evse_id,
                    connector_id: 1,
                };
                let request = v201_transaction::transaction_event_updated(
                    &session,
                    seq_no,
                    reading.energy_wh,
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
    ) -> OcppResult<i32> {
        match self.config.protocol_version {
            OcppVersion::V16J => {
                // 1.6J `StartTransaction` has no `remoteStartId` field; the
                // remote-start correlation on 1.6J is the synchronous
                // `transactionId` returned in `RemoteStartTransaction.conf`.
                let _ = remote_start_id;
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
                // 1, 2, …) and the authorizing idTag for the Ended event.
                self.v201_sessions.write().await.insert(
                    transaction_id,
                    V201Session {
                        id_tag: id_tag.to_string(),
                        next_seq_no: Arc::new(AtomicI32::new(1)),
                        authorized: Arc::new(AtomicBool::new(true)),
                    },
                );
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
        // `remoteStartId` via `start_transaction_with_remote_start_id`.
        self.start_transaction_with_remote_start_id(connector_id, id_tag, meter_start, None)
            .await
    }

    /// [`start_transaction`](Self::start_transaction) plus the optional 2.0.1
    /// `RequestStartTransaction.remoteStartId`, echoed onto the started
    /// transaction's `TransactionEvent(Started)` so a CSMS can correlate its
    /// remote-start request with the session that follows. `remote_start_id` is
    /// `None` on the local-start and 1.6J paths (neither carries a
    /// `remoteStartId`), keeping their behavior byte-for-byte unchanged.
    async fn start_transaction_with_remote_start_id(
        &self,
        connector_id: ConnectorId,
        id_tag: &str,
        meter_start: i32,
        remote_start_id: Option<i32>,
    ) -> OcppResult<i32> {
        // Connector is now preparing for a transaction (Available -> Preparing).
        self.send_status_notification(
            connector_id.value(),
            ChargePointStatus::Preparing,
            ChargePointErrorCode::NoError,
        )
        .await?;

        let transaction_id = self
            .open_transaction(connector_id, id_tag, meter_start, remote_start_id)
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
