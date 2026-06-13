//! # OCPP Charge Point Implementation
//!
//! This crate provides a comprehensive charge point implementation that supports:
//! - Full connector state management with all OCPP 1.6J states
//! - Transaction lifecycle management
//! - Status notifications and meter values
//! - WebSocket connection to Central System
//! - Real-world charging scenarios simulation

pub mod connector;
pub mod error;
pub mod message_handler;
pub mod state_machine;
pub mod transaction;

use anyhow::Result;
use connector::{Connector, ConnectorConfig};
use error::ChargePointError;
use message_handler::ConfigurationStore;
use ocpp_messages::v16j::{
    AuthorizeRequest, BootNotificationRequest, BootNotificationResponse, ChangeAvailabilityRequest,
    ChangeAvailabilityResponse, ChangeConfigurationRequest, ChangeConfigurationResponse,
    ClearCacheRequest, ClearCacheResponse, DataTransferRequest, DataTransferResponse,
    GetConfigurationRequest, GetConfigurationResponse, HeartbeatRequest, RegistrationStatus,
    RemoteStartTransactionRequest, RemoteStartTransactionResponse, RemoteStopTransactionRequest,
    RemoteStopTransactionResponse, ResetRequest, ResetResponse, StartTransactionRequest,
    StatusNotificationRequest, StopTransactionRequest, UnlockConnectorRequest,
    UnlockConnectorResponse,
};
use ocpp_messages::{ActionDispatcher, CallMessage, Message, MessageType, OcppAction};
use ocpp_transport::client::WebSocketClient;
use ocpp_transport::{
    MessageHandler as TransportMessageHandler, Transport, TransportConfig, TransportEvent,
};
use ocpp_types::common::{AuthorizationStatus, AvailabilityStatus, IdTagInfo, KeyValue, Reason};
use ocpp_types::v16j::{
    ChargePointStatus, ChargePointVendorInfo, ClearCacheStatus, ConfigurationStatus,
    DataTransferStatus, RemoteStartStopStatus, ResetStatus, ResetType, UnlockStatus,
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
    /// Connection retry interval in seconds
    pub connection_retry_interval: u64,
    /// Maximum connection retry attempts
    pub max_connection_retries: u32,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    /// Timeout for individual OCPP CALL/CALLRESULT round-trips in seconds.
    /// Matches the Python reference default of 30 s (charge_point.py).
    pub call_timeout: u64,
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
            heartbeat_interval: 300,       // 5 minutes
            meter_values_interval: 60,     // 1 minute
            connection_retry_interval: 30, // 30 seconds
            max_connection_retries: 10,
            auto_reconnect: true,
            call_timeout: 30, // 30 seconds, matches Python reference default
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
    /// Error occurred
    Error { error: ChargePointError },
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
        OcppError::ValidationError { message } => {
            (CallErrorCode::PropertyConstraintViolation, message.clone())
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

        let config_store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let dispatcher = Arc::new(RwLock::new(Self::build_default_dispatcher(
            config_store.clone(),
        )));

        Ok(Self {
            config,
            connectors: Arc::new(RwLock::new(connectors)),
            client: Arc::new(RwLock::new(None)),
            dispatcher,
            config_store,
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
            registration_status: Arc::new(RwLock::new(RegistrationStatus::Rejected)),
            is_connected: Arc::new(RwLock::new(false)),
            heartbeat_handle: Arc::new(RwLock::new(None)),
        })
    }

    /// Build the default `ActionDispatcher` pre-populated with handlers for
    /// all 9 OCPP 1.6J Core Profile actions.
    ///
    /// Ports the default `@on` handler registrations from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
    /// and [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    fn build_default_dispatcher(config_store: Arc<RwLock<ConfigurationStore>>) -> ActionDispatcher {
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

        // RemoteStartTransaction — accept (real auth/connector checks are Issue #21)
        d.on(|_req: RemoteStartTransactionRequest| async move {
            Ok(RemoteStartTransactionResponse {
                status: RemoteStartStopStatus::Accepted,
            })
        });

        // RemoteStopTransaction — accept (real transaction lookup is Issue #21)
        d.on(|_req: RemoteStopTransactionRequest| async move {
            Ok(RemoteStopTransactionResponse {
                status: RemoteStartStopStatus::Accepted,
            })
        });

        // Reset — always accept
        d.on(|req: ResetRequest| async move {
            match req.reset_type {
                ResetType::Soft => info!("Soft reset requested"),
                ResetType::Hard => info!("Hard reset requested"),
            }
            Ok(ResetResponse {
                status: ResetStatus::Accepted,
            })
        });

        // UnlockConnector — always succeed (real connector unlock is Issue #21)
        d.on(|_req: UnlockConnectorRequest| async move {
            Ok(UnlockConnectorResponse {
                status: UnlockStatus::Unlocked,
            })
        });

        // ClearCache — always accept
        d.on(|_req: ClearCacheRequest| async move {
            Ok(ClearCacheResponse {
                status: ClearCacheStatus::Accepted,
            })
        });

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

        // Stop heartbeat
        if let Some(handle) = self.heartbeat_handle.write().await.take() {
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

    /// Connect to central system
    pub async fn connect(&self) -> Result<()> {
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

        // Send boot notification
        self.send_boot_notification().await?;

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
        serde_json::from_value::<Req::Response>(payload).map_err(OcppError::from)
    }

    /// Send boot notification (fire-and-forget; upgrading to call() is Issue #20)
    async fn send_boot_notification(&self) -> Result<()> {
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

        let message = Message::Call(ocpp_messages::CallMessage::new(
            BootNotificationRequest::ACTION_NAME.to_string(),
            request,
        )?);

        if let Some(client) = self.client.read().await.as_ref() {
            client.send_message(message).await?;
        }

        Ok(())
    }

    /// Start heartbeat task
    async fn start_heartbeat(&self) {
        let interval = Duration::from_secs(self.config.heartbeat_interval);
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

    /// Send an `AuthorizeRequest` CALL and return the `IdTagInfo` from the response.
    ///
    /// Ports `_send_authorize()` / `_handle_call_result()` from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
    pub async fn authorize(&self, id_tag: &str) -> OcppResult<IdTagInfo> {
        let response = self
            .call(AuthorizeRequest {
                id_tag: id_tag.to_string(),
            })
            .await?;
        Ok(response.id_tag_info)
    }

    /// Send a `StartTransactionRequest` CALL, store the CSMS-assigned `transactionId`
    /// in the local connector state, and transition the connector to `Charging`.
    ///
    /// Returns the CSMS-assigned `transactionId` on success.
    /// Returns `OcppError::Authorization` when the CSMS rejects the id-tag.
    ///
    /// Ports `send_start_transaction()` from
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    pub async fn start_transaction(
        &self,
        connector_id: ConnectorId,
        id_tag: &str,
        meter_start: i32,
    ) -> OcppResult<i32> {
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
                    "StartTransaction rejected — idTag status: {:?}",
                    response.id_tag_info.status
                ),
            });
        }

        let csms_id = response.transaction_id;
        {
            let mut connectors = self.connectors.write().await;
            if let Some(connector) = connectors.get_mut(&connector_id) {
                connector
                    .start_transaction_with_csms_id(id_tag.to_string(), csms_id, meter_start)
                    .await
                    .map_err(|e| OcppError::Internal {
                        message: e.to_string(),
                    })?;
            }
        }

        info!(
            "Transaction {} started on connector {}",
            csms_id, connector_id
        );
        Ok(csms_id)
    }

    /// Send a `StopTransactionRequest` CALL and clear local connector state.
    ///
    /// `transaction_id` must be the CSMS-assigned ID returned by `start_transaction()`.
    /// `meter_stop` is the final meter reading in Wh.
    ///
    /// Ports `send_stop_transaction()` from
    /// [`examples/v16/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/charge_point.py).
    pub async fn stop_transaction(
        &self,
        transaction_id: i32,
        meter_stop: i32,
        reason: Option<Reason>,
    ) -> OcppResult<()> {
        let _response = self
            .call(StopTransactionRequest {
                id_tag: None,
                meter_stop,
                timestamp: chrono::Utc::now(),
                transaction_id,
                reason,
                transaction_data: None,
            })
            .await?;

        // Clear local connector state for the matching transaction.
        let mut connectors = self.connectors.write().await;
        for connector in connectors.values_mut() {
            if connector
                .has_active_transaction_with_id(transaction_id)
                .await
            {
                connector
                    .stop_transaction("Local".to_string())
                    .await
                    .map_err(|e| OcppError::Internal {
                        message: e.to_string(),
                    })?;
                break;
            }
        }

        info!("Transaction {} stopped", transaction_id);
        Ok(())
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

                // Start heartbeat with the interval from central system
                self.start_heartbeat().await;
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

    /// Send status notification for a connector
    pub async fn send_status_notification(
        &self,
        connector_id: ConnectorId,
        status: ChargePointStatus,
        error_code: ocpp_types::v16j::ChargePointErrorCode,
        info: Option<String>,
    ) -> Result<()> {
        let request = StatusNotificationRequest {
            connector_id: connector_id.value(),
            error_code,
            info,
            status,
            timestamp: Some(chrono::Utc::now()),
            vendor_error_code: None,
            vendor_id: None,
        };

        let message = Message::Call(ocpp_messages::CallMessage::new(
            StatusNotificationRequest::ACTION_NAME.to_string(),
            request,
        )?);

        if let Some(client) = self.client.read().await.as_ref() {
            client.send_message(message).await?;
        }

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
    use ocpp_messages::{CallMessage, Message};
    use ocpp_types::common::AvailabilityType;
    use ocpp_types::v16j::{ConfigurationStatus, RemoteStartStopStatus, ResetType};

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

        // Plug in/out — local state only, no OCPP call needed
        cp.plug_in(connector_id).await.unwrap();
        cp.plug_out(connector_id).await.unwrap();

        // Fault lifecycle — local state only
        cp.set_fault(
            connector_id,
            ocpp_types::v16j::ChargePointErrorCode::NoError,
            None,
        )
        .await
        .unwrap();
        cp.clear_fault(connector_id).await.unwrap();
        // start_transaction / stop_transaction now send OCPP messages and require
        // a live WebSocket connection — see the mock-CSMS tests below.
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
    async fn default_remote_stop_returns_accepted() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        let call = make_call(RemoteStopTransactionRequest { transaction_id: 42 });
        let resp = cp.handle_message(Message::Call(call)).await.unwrap();
        match resp.unwrap() {
            Message::CallResult(r) => {
                let body: RemoteStopTransactionResponse = r.payload_as().unwrap();
                assert_eq!(body.status, RemoteStartStopStatus::Accepted);
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

    // -------------------------------------------------------------------------
    // Mock-CSMS helpers for testing outgoing CALL flows
    // -------------------------------------------------------------------------

    /// Spawn a minimal raw WebSocket server on a random port.
    ///
    /// `handler` receives the raw OCPP JSON text frames and returns the response
    /// frames. Stops after processing `max_messages` frames.
    async fn spawn_mock_csms(
        handler: impl Fn(String) -> Option<String> + Send + Sync + 'static,
        max_messages: usize,
    ) -> std::net::SocketAddr {
        use futures_util::{SinkExt, StreamExt};
        use tokio::net::TcpListener;
        use tokio_tungstenite::{accept_async, tungstenite::Message as WsMsg};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut ws = accept_async(stream).await.unwrap();
                // Consume the BootNotification first (sent by connect())
                let mut processed = 0usize;
                while processed < max_messages {
                    match ws.next().await {
                        Some(Ok(WsMsg::Text(text))) => {
                            if let Some(reply) = handler(text) {
                                let _ = ws.send(WsMsg::Text(reply)).await;
                            }
                            processed += 1;
                        }
                        _ => break,
                    }
                }
            }
        });

        addr
    }

    /// Build a JSON CALLRESULT frame: `[3, "<uid>", <payload>]`
    fn callresult(unique_id: &str, payload: serde_json::Value) -> String {
        serde_json::json!([3, unique_id, payload]).to_string()
    }

    /// Extract the unique_id and action from a CALL frame: `[2, uid, action, payload]`
    fn parse_call(text: &str) -> Option<(String, String, serde_json::Value)> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let arr = v.as_array()?;
        if arr.len() < 4 {
            return None;
        }
        Some((
            arr[1].as_str()?.to_string(),
            arr[2].as_str()?.to_string(),
            arr[3].clone(),
        ))
    }

    /// Connect a ChargePoint to a mock server, bypassing the BootNotification
    /// handshake by answering it automatically.
    async fn connect_cp_to_mock(addr: std::net::SocketAddr) -> ChargePoint {
        let config = ChargePointConfig {
            central_system_url: format!("ws://{}", addr),
            call_timeout: 5,
            ..Default::default()
        };
        let cp = ChargePoint::new(config).unwrap();
        cp.connect().await.unwrap();
        cp
    }

    // -------------------------------------------------------------------------
    // authorize() tests
    // -------------------------------------------------------------------------

    // The transport layer's recv task holds the WS mutex across the receive
    // await, blocking the send task.  Multi-thread tests side-step this by
    // letting both tasks truly run in parallel.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn authorize_accepted_returns_id_tag_info() {
        use ocpp_types::common::AuthorizationStatus;

        // Server: answer BootNotification then Authorize
        let addr = spawn_mock_csms(
            |text| {
                let (uid, action, _payload) = parse_call(&text)?;
                match action.as_str() {
                    "BootNotification" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "status": "Accepted",
                            "currentTime": "2026-06-13T00:00:00Z",
                            "interval": 300
                        }),
                    )),
                    "Authorize" => Some(callresult(
                        &uid,
                        serde_json::json!({ "idTagInfo": { "status": "Accepted" } }),
                    )),
                    _ => None,
                }
            },
            2,
        )
        .await;

        let cp = connect_cp_to_mock(addr).await;

        let id_tag_info = cp.authorize("VALID_TAG").await.unwrap();
        assert_eq!(id_tag_info.status, AuthorizationStatus::Accepted);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn authorize_rejected_returns_blocked_status() {
        use ocpp_types::common::AuthorizationStatus;

        let addr = spawn_mock_csms(
            |text| {
                let (uid, action, _) = parse_call(&text)?;
                match action.as_str() {
                    "BootNotification" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "status": "Accepted",
                            "currentTime": "2026-06-13T00:00:00Z",
                            "interval": 300
                        }),
                    )),
                    "Authorize" => Some(callresult(
                        &uid,
                        serde_json::json!({ "idTagInfo": { "status": "Blocked" } }),
                    )),
                    _ => None,
                }
            },
            2,
        )
        .await;

        let cp = connect_cp_to_mock(addr).await;

        let id_tag_info = cp.authorize("BLOCKED_TAG").await.unwrap();
        assert_eq!(id_tag_info.status, AuthorizationStatus::Blocked);
    }

    // -------------------------------------------------------------------------
    // start_transaction() tests
    // -------------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn start_transaction_sends_request_and_stores_csms_id() {
        // Mock server: handle Boot + StartTransaction
        let addr = spawn_mock_csms(
            |text| {
                let (uid, action, _) = parse_call(&text)?;
                match action.as_str() {
                    "BootNotification" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "status": "Accepted",
                            "currentTime": "2026-06-13T00:00:00Z",
                            "interval": 300
                        }),
                    )),
                    "StartTransaction" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "transactionId": 42,
                            "idTagInfo": { "status": "Accepted" }
                        }),
                    )),
                    _ => None,
                }
            },
            2,
        )
        .await;

        let cp = connect_cp_to_mock(addr).await;
        let connector_id = ConnectorId::new(1).unwrap();

        // Plug in so connector is in Preparing state
        cp.plug_in(connector_id).await.unwrap();

        let txn_id = cp
            .start_transaction(connector_id, "TAG_001", 0)
            .await
            .unwrap();

        assert_eq!(txn_id, 42, "should return CSMS-assigned transaction ID");

        // Connector should now be in Charging state
        let connectors = cp.get_connectors().await;
        let connector = connectors.get(&connector_id).unwrap();
        assert_eq!(
            connector.status().await,
            ChargePointStatus::Charging,
            "connector must transition to Charging after accepted StartTransaction"
        );
        assert!(
            connector.has_active_transaction_with_id(42).await,
            "connector must hold transaction with CSMS-assigned ID 42"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn start_transaction_rejected_id_tag_returns_authorization_error() {
        let addr = spawn_mock_csms(
            |text| {
                let (uid, action, _) = parse_call(&text)?;
                match action.as_str() {
                    "BootNotification" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "status": "Accepted",
                            "currentTime": "2026-06-13T00:00:00Z",
                            "interval": 300
                        }),
                    )),
                    "StartTransaction" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "transactionId": 0,
                            "idTagInfo": { "status": "Blocked" }
                        }),
                    )),
                    _ => None,
                }
            },
            2,
        )
        .await;

        let cp = connect_cp_to_mock(addr).await;
        let connector_id = ConnectorId::new(1).unwrap();
        cp.plug_in(connector_id).await.unwrap();

        let result = cp.start_transaction(connector_id, "BLOCKED_TAG", 0).await;

        match result {
            Err(OcppError::Authorization { reason }) => {
                assert!(
                    reason.contains("Blocked"),
                    "error reason should mention Blocked, got: {reason}"
                );
            }
            other => panic!("expected Authorization error, got: {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // stop_transaction() tests
    // -------------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_transaction_sends_request_with_correct_id() {
        let addr = spawn_mock_csms(
            |text| {
                let (uid, action, payload) = parse_call(&text)?;
                match action.as_str() {
                    "BootNotification" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "status": "Accepted",
                            "currentTime": "2026-06-13T00:00:00Z",
                            "interval": 300
                        }),
                    )),
                    "StartTransaction" => Some(callresult(
                        &uid,
                        serde_json::json!({
                            "transactionId": 99,
                            "idTagInfo": { "status": "Accepted" }
                        }),
                    )),
                    "StopTransaction" => {
                        // Verify the correct transactionId was sent
                        assert_eq!(
                            payload["transactionId"].as_i64(),
                            Some(99),
                            "StopTransaction must carry the CSMS-assigned transactionId"
                        );
                        Some(callresult(&uid, serde_json::json!({})))
                    }
                    _ => None,
                }
            },
            3,
        )
        .await;

        let cp = connect_cp_to_mock(addr).await;
        let connector_id = ConnectorId::new(1).unwrap();
        cp.plug_in(connector_id).await.unwrap();

        let txn_id = cp
            .start_transaction(connector_id, "TAG_999", 0)
            .await
            .unwrap();
        assert_eq!(txn_id, 99);

        cp.stop_transaction(txn_id, 1000, None).await.unwrap();

        // Connector should no longer have an active transaction
        let connectors = cp.get_connectors().await;
        let connector = connectors.get(&connector_id).unwrap();
        assert!(
            !connector.has_active_transaction_with_id(99).await,
            "transaction should be cleared after stop"
        );
    }
}
