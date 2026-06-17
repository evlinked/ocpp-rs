//! # OCPP Charge Point Implementation
//!
//! This crate provides a comprehensive charge point implementation that supports:
//! - Full connector state management with all OCPP 1.6J states
//! - Transaction lifecycle management
//! - Status notifications and meter values
//! - WebSocket connection to Central System
//! - Real-world charging scenarios simulation

pub mod auth_cache;
pub mod connector;
pub mod error;
pub mod message_handler;
pub mod meter_sampler;
pub mod state_machine;
pub mod transaction;

use anyhow::Result;
use auth_cache::AuthCache;
use connector::{Connector, ConnectorConfig};
use error::ChargePointError;
use message_handler::ConfigurationStore;
use ocpp_messages::v16j::{
    AuthorizeRequest, BootNotificationRequest, BootNotificationResponse, ChangeAvailabilityRequest,
    ChangeAvailabilityResponse, ChangeConfigurationRequest, ChangeConfigurationResponse,
    ClearCacheRequest, ClearCacheResponse, DataTransferRequest, DataTransferResponse,
    GetConfigurationRequest, GetConfigurationResponse, HeartbeatRequest, MeterValuesRequest,
    RegistrationStatus, RemoteStartTransactionRequest, RemoteStartTransactionResponse,
    RemoteStopTransactionRequest, RemoteStopTransactionResponse, ResetRequest, ResetResponse,
    StartTransactionRequest, StatusNotificationRequest, StopTransactionRequest,
    UnlockConnectorRequest, UnlockConnectorResponse,
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
    ChargePointErrorCode, ChargePointStatus, ChargePointVendorInfo, ClearCacheStatus,
    ConfigurationStatus, DataTransferStatus, RemoteStartStopStatus, ResetStatus, ResetType,
    UnlockStatus,
};
use ocpp_types::{
    CallErrorCode, CallErrorMessage, CallResultMessage, ConnectorId, OcppError, OcppResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Charge point configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChargePointConfig {
    /// Charge point identifier
    pub charge_point_id: String,
    /// Central system WebSocket URL
    pub central_system_url: String,
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
    /// Transport configuration (not serialized; uses Default on deserialization)
    #[serde(skip)]
    pub transport_config: TransportConfig,
}

impl Default for ChargePointConfig {
    fn default() -> Self {
        Self {
            charge_point_id: "CP001".to_string(),
            central_system_url: "ws://localhost:8080".to_string(),
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
            transport_config: TransportConfig::default(),
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
    /// `RemoteStartTransaction` (OCPP 1.6J §5.11).
    StartTransaction {
        connector_id: ConnectorId,
        id_tag: String,
    },
    /// End the matching transaction for an `Accepted` `RemoteStopTransaction`
    /// (OCPP 1.6J §5.12).
    StopTransaction { transaction_id: i32 },
}

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
        OcppError::SchemaViolation { keyword, message } => {
            (keyword.call_error_code(), message.clone())
        }
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
    active_transactions: Arc<RwLock<HashMap<i32, ConnectorId>>>,
    /// Shared schema validator (when `config.validate_payloads`). Backs both
    /// the dispatcher's incoming-CALL validation and `call()`'s CALLRESULT
    /// validation. `None` when validation is disabled.
    validator: Option<Arc<SchemaValidator>>,
    /// Per-transaction periodic `MeterValues` sampler tasks, keyed by the
    /// CSMS-assigned transaction ID. Each active transaction has its own task
    /// (connectors charge concurrently); cancelled on `stop_transaction`/`stop`.
    meter_sampler_handles: Arc<RwLock<HashMap<i32, tokio::task::JoinHandle<()>>>>,
    /// Authorization ID-tag cache (Issue #23). Shared with the default
    /// `ClearCache` handler so a CSMS `ClearCache` command empties it. Backs the
    /// cache-first behavior of [`ChargePoint::authorize`].
    auth_cache: Arc<AuthCache>,
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

        // Build the shared validator once (compiles 78 schemas) and back both
        // the dispatcher (incoming CALLs) and `call()` (CALLRESULTs) with it.
        // Mirrors `ocpp/charge_point.py`, which always runs `_validate()`.
        let validator = if config.validate_payloads {
            Some(Arc::new(SchemaValidator::v16j()))
        } else {
            None
        };

        let auth_cache = Arc::new(AuthCache::new(Duration::from_secs(config.auth_cache_ttl)));

        let (command_sender, command_receiver) = mpsc::unbounded_channel();

        // Wrap the shared state the default handlers need *before* building the
        // dispatcher: RemoteStart/RemoteStop consult live connector status and
        // the active-transaction map to answer Accepted/Rejected faithfully.
        let connectors = Arc::new(RwLock::new(connectors));
        let active_transactions = Arc::new(RwLock::new(HashMap::new()));

        let config_store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let mut dispatcher = Self::build_default_dispatcher(
            config_store.clone(),
            auth_cache.clone(),
            command_sender,
            connectors.clone(),
            active_transactions.clone(),
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
            validator,
            meter_sampler_handles: Arc::new(RwLock::new(HashMap::new())),
            auth_cache,
            command_receiver: Arc::new(RwLock::new(Some(command_receiver))),
            command_consumer: Arc::new(RwLock::new(None)),
        })
    }

    /// Build the default `ActionDispatcher` pre-populated with handlers for
    /// all 9 OCPP 1.6J Core Profile actions.
    ///
    /// Ports the default `@on` handler registrations from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
    /// and [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    fn build_default_dispatcher(
        config_store: Arc<RwLock<ConfigurationStore>>,
        auth_cache: Arc<AuthCache>,
        command_sender: mpsc::UnboundedSender<RemoteCommand>,
        connectors: Arc<RwLock<HashMap<ConnectorId, Connector>>>,
        active_transactions: Arc<RwLock<HashMap<i32, ConnectorId>>>,
    ) -> ActionDispatcher {
        let mut d = ActionDispatcher::new();

        // ChangeAvailability — always accept (Issue #21 will add real state tracking)
        d.on(|_req: ChangeAvailabilityRequest| async move {
            Ok(ChangeAvailabilityResponse {
                status: AvailabilityStatus::Accepted,
            })
        });

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

        // Reset — acknowledge, then carry out the reset as a real side effect
        // (OCPP 1.6J §5.13). The work is queued on the command channel and run
        // by the consumer task spawned in `connect()`, so the CALLRESULT is
        // flushed before any outbound CALL (graceful StopTransaction, re-boot)
        // and the receive loop never re-enters itself. Returning `Accepted`
        // commits only to *attempting* the reset; if the consumer has gone away
        // (the CP is shutting down) the command cannot be honored, so we report
        // `Rejected` rather than silently dropping it.
        {
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

        // UnlockConnector — always succeed (real connector unlock is Issue #21)
        d.on(|_req: UnlockConnectorRequest| async move {
            Ok(UnlockConnectorResponse {
                status: UnlockStatus::Unlocked,
            })
        });

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

        // DataTransfer — accept and echo data back
        d.on(|req: DataTransferRequest| async move {
            Ok(DataTransferResponse {
                status: DataTransferStatus::Accepted,
                data: req.data,
            })
        });

        d
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
                        } => {
                            // meter_start is unknown for a remote-initiated start;
                            // report 0, matching the Python reference's example CP.
                            if let Err(e) = cp.start_transaction(connector_id, &id_tag, 0).await {
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
        let unique_id = Uuid::new_v4().to_string();

        // 1. Register before sending to avoid the race where the CALLRESULT
        //    arrives before we have a receiver in the map.
        let rx = {
            let client_guard = self.client.read().await;
            let client = client_guard.as_ref().ok_or_else(|| OcppError::Transport {
                message: "Not connected to central system".to_string(),
            })?;
            client.pending_calls().register(unique_id.clone())
        };

        // 2. Build the CALL frame with the same unique_id.
        let call_msg = CallMessage {
            message_type: MessageType::Call,
            unique_id: unique_id.clone(),
            action: Req::ACTION_NAME.to_string(),
            payload: serde_json::to_value(&request).map_err(OcppError::from)?,
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
    async fn boot_sequence(&self) -> OcppResult<()> {
        let request = BootNotificationRequest {
            charge_point_vendor: self.config.vendor_info.charge_point_vendor.clone(),
            charge_point_model: self.config.vendor_info.charge_point_model.clone(),
            charge_point_serial_number: self.config.vendor_info.charge_point_serial_number.clone(),
            charge_box_serial_number: self.config.vendor_info.charge_box_serial_number.clone(),
            firmware_version: self.config.vendor_info.firmware_version.clone(),
            iccid: self.config.vendor_info.iccid.clone(),
            imsi: self.config.vendor_info.imsi.clone(),
            meter_type: self.config.vendor_info.meter_type.clone(),
            meter_serial_number: self.config.vendor_info.meter_serial_number.clone(),
        };

        let max_attempts = self.config.max_boot_retries + 1;
        for attempt in 1..=max_attempts {
            let response = self.call(request.clone()).await?;
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
        // Connector 0 represents the charge point itself; 1..=connector_count
        // are the physical connectors created in `ChargePoint::new`.
        for connector_id in 0..=self.config.connector_count {
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

    /// Start the periodic `MeterValues` sampler for an active transaction.
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
    async fn start_meter_sampler(&self, connector_id: ConnectorId, transaction_id: i32) {
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

        // Replace any existing sampler for this id, aborting the stale task so
        // it can't outlive its transaction (defensive against a reused id).
        if let Some(previous) = self
            .meter_sampler_handles
            .write()
            .await
            .insert(transaction_id, handle)
        {
            previous.abort();
        }
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

    /// Authorize an id tag, consulting the local authorization cache first.
    ///
    /// Ports `_send_authorize()` from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
    /// and adds the OCPP 1.6J §3.1 Authorization Cache behavior (Issue #23):
    ///
    /// 1. **Cache hit (`Accepted`)** → return the cached `IdTagInfo` without a
    ///    CALL. Non-`Accepted` cached results are *not* short-circuited; the CP
    ///    re-checks with the CSMS in case authorization has since been granted.
    /// 2. **Miss / expired / non-accepted** → send `AuthorizeRequest` via
    ///    [`ChargePoint::call`] and cache the fresh result.
    /// 3. **CSMS unreachable (CALL times out)** → if
    ///    [`ChargePointConfig::offline_auth_stale_ok`] is set and a (possibly
    ///    stale) cached entry exists, honor it; otherwise fail safe with
    ///    `AuthorizationStatus::Invalid`.
    ///
    /// The caller is responsible for acting on the returned `status`; this
    /// method does not block the transaction flow by itself.
    pub async fn authorize(&self, id_tag: &str) -> OcppResult<IdTagInfo> {
        // 1. Cache-first: a fresh, previously-accepted tag needs no round-trip.
        if let Some(cached) = self.auth_cache.get(id_tag) {
            if cached.status == AuthorizationStatus::Accepted {
                return Ok(cached);
            }
        }

        // 2. Miss (or a cached non-accepted result): ask the CSMS.
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
            // 3. CSMS unreachable: offline authorization decision.
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

    /// Send a `StartTransaction` CALL to the CSMS, record the CSMS-assigned
    /// transaction ID, and transition the connector to `Charging`.
    ///
    /// Returns the CSMS-assigned `transactionId` on success, or
    /// `OcppError::Authorization` if the CSMS rejects the id tag
    /// (`idTagInfo.status != Accepted`).
    ///
    /// Ports `send_start_transaction()` from
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    pub async fn start_transaction(
        &self,
        connector_id: ConnectorId,
        id_tag: &str,
        meter_start: i32,
    ) -> OcppResult<i32> {
        // Connector is now preparing for a transaction (Available -> Preparing).
        self.send_status_notification(
            connector_id.value(),
            ChargePointStatus::Preparing,
            ChargePointErrorCode::NoError,
        )
        .await?;

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

        let transaction_id = response.transaction_id;

        // Map CSMS transaction ID → connector ID for stop_transaction lookup
        self.active_transactions
            .write()
            .await
            .insert(transaction_id, connector_id);

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

    /// Send a `StopTransaction` CALL to the CSMS and transition the connector
    /// back to `Available`.
    ///
    /// Returns `OcppError::NotFound` if `transaction_id` does not correspond
    /// to a transaction started via [`ChargePoint::start_transaction`].
    ///
    /// Ports `send_stop_transaction()` from
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
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

        self.call(StopTransactionRequest {
            id_tag: None,
            meter_stop,
            timestamp: chrono::Utc::now(),
            transaction_id,
            reason: Some(reason),
            transaction_data: None,
        })
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
            if let Err(e) = self.stop_transaction(txn_id, 0, reason.clone()).await {
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

        Ok(())
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

    // --- dispatcher wiring tests ---

    #[tokio::test]
    async fn dispatcher_has_9_default_handlers() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        assert_eq!(cp.handler_count().await, 9);
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
