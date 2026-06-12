//! # OCPP Charge Point Implementation
//!
//! This crate provides a comprehensive charge point implementation that supports:
//! - Full connector state management with all OCPP 1.6J states
//! - Transaction lifecycle management
//! - Status notifications and meter values
//! - WebSocket connection to Central System
//! - Real-world charging scenarios simulation

pub mod connector;
pub mod dispatching_handler;
pub mod error;
pub mod message_handler;
pub mod state_machine;
pub mod transaction;

use anyhow::Result;
use connector::{Connector, ConnectorConfig};
use dispatching_handler::DispatchingHandler;
use error::ChargePointError;
use message_handler::ConfigurationStore;
use ocpp_messages::v16j::{
    BootNotificationRequest, ChangeAvailabilityRequest, ChangeAvailabilityResponse,
    ChangeConfigurationRequest, ChangeConfigurationResponse, ClearCacheRequest, ClearCacheResponse,
    DataTransferRequest, DataTransferResponse, GetConfigurationRequest, GetConfigurationResponse,
    HeartbeatRequest, RegistrationStatus, RemoteStartTransactionRequest,
    RemoteStartTransactionResponse, RemoteStopTransactionRequest, RemoteStopTransactionResponse,
    ResetRequest, ResetResponse, StatusNotificationRequest, UnlockConnectorRequest,
    UnlockConnectorResponse,
};
use ocpp_messages::{ActionDispatcher, CallMessage, Message, MessageType, OcppAction};
use ocpp_transport::client::WebSocketClient;
use ocpp_transport::{MessageHandler as TransportMessageHandler, Transport, TransportConfig};
use ocpp_types::common::{AvailabilityStatus, KeyValue};
use ocpp_types::v16j::{
    ClearCacheStatus, ConfigurationStatus, DataTransferStatus, RemoteStartStopStatus, ResetStatus,
    ResetType, UnlockStatus,
};
use ocpp_types::{ConnectorId, OcppError, OcppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

use ocpp_types::v16j::ChargePointStatus;

/// Charge point configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChargePointConfig {
    /// Charge point identifier
    pub charge_point_id: String,
    /// Central system WebSocket URL
    pub central_system_url: String,
    /// Charge point vendor information
    pub vendor_info: ocpp_types::v16j::ChargePointVendorInfo,
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
    /// Maximum number of BootNotification attempts before giving up.
    /// On each Rejected or Pending response the CP waits the server-supplied
    /// `interval` before retrying.  Matches the Python reference behaviour in
    /// `charge_point.py`.  Default: 3.
    pub max_boot_retries: u32,
    /// Transport configuration (not serialized; uses Default on deserialization)
    #[serde(skip)]
    pub transport_config: TransportConfig,
}

impl Default for ChargePointConfig {
    fn default() -> Self {
        Self {
            charge_point_id: "CP001".to_string(),
            central_system_url: "ws://localhost:8080".to_string(),
            vendor_info: ocpp_types::v16j::ChargePointVendorInfo {
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
            heartbeat_interval: 300,
            meter_values_interval: 60,
            connection_retry_interval: 30,
            max_connection_retries: 10,
            auto_reconnect: true,
            call_timeout: 30,
            max_boot_retries: 3,
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

/// Main charge point implementation.
///
/// ## Setup — Python `@on` / `@after` decorator semantics
///
/// Register custom handlers before calling [`start()`][ChargePoint::start]:
///
/// ```rust,ignore
/// let mut cp = ChargePoint::new(config)?;
///
/// // Override the default Reset handler
/// cp.on(|req: ResetRequest| async move {
///     tracing::info!("reset requested: {:?}", req.reset_type);
///     Ok(ResetResponse { status: ResetStatus::Accepted })
/// });
///
/// cp.start().await?;
/// ```
///
/// Default handlers are pre-registered for all nine OCPP 1.6J CSMS→CP
/// actions: `ChangeAvailability`, `ChangeConfiguration`, `GetConfiguration`,
/// `RemoteStartTransaction`, `RemoteStopTransaction`, `Reset`,
/// `UnlockConnector`, `ClearCache`, `DataTransfer`.
pub struct ChargePoint {
    config: ChargePointConfig,
    connectors: Arc<RwLock<HashMap<ConnectorId, Connector>>>,
    client: Arc<RwLock<Option<WebSocketClient>>>,
    /// Holds the dispatcher until the first `connect()`.  Consumed (taken)
    /// when the handler is frozen into `Arc<ActionDispatcher>` and passed to
    /// `WebSocketClient`.  Subsequent `connect()` calls are rejected.
    pending_dispatcher: std::sync::Mutex<Option<ActionDispatcher>>,
    /// OCPP configuration key-value store (shared with default handlers).
    config_store: Arc<RwLock<ConfigurationStore>>,
    /// Connector states visible to default handlers.
    connector_states: Arc<RwLock<HashMap<ConnectorId, ChargePointStatus>>>,
    /// Active transaction IDs (connector → transaction_id).
    active_transactions: Arc<RwLock<HashMap<ConnectorId, i32>>>,
    event_sender: mpsc::UnboundedSender<ChargePointEvent>,
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<ChargePointEvent>>>>,
    registration_status: Arc<RwLock<RegistrationStatus>>,
    is_connected: Arc<RwLock<bool>>,
    heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Send a typed OCPP CALL and await the matching CALLRESULT.
///
/// Free-function version of `ChargePoint::call()` that can be called from
/// spawned tasks (which cannot hold a reference to `ChargePoint`).  Extracted
/// so both `ChargePoint::call()` and the heartbeat task share the same logic.
async fn call_action<Req: OcppAction>(
    client: &Arc<RwLock<Option<WebSocketClient>>>,
    call_timeout: u64,
    request: Req,
) -> OcppResult<Req::Response> {
    let unique_id = Uuid::new_v4().to_string();

    let rx = {
        let guard = client.read().await;
        let c = guard.as_ref().ok_or_else(|| OcppError::Transport {
            message: "Not connected to central system".to_string(),
        })?;
        c.pending_calls().register(unique_id.clone())
    };

    let call_msg = CallMessage {
        message_type: MessageType::Call,
        unique_id: unique_id.clone(),
        action: Req::ACTION_NAME.to_string(),
        payload: serde_json::to_value(&request).map_err(OcppError::from)?,
    };

    {
        let guard = client.read().await;
        match guard.as_ref() {
            Some(c) => c.send_message(Message::Call(call_msg)).await?,
            None => {
                return Err(OcppError::Transport {
                    message: "Not connected to central system".to_string(),
                })
            }
        }
    }

    let timeout = Duration::from_secs(call_timeout);
    let raw_result = tokio::time::timeout(timeout, rx)
        .await
        .map_err(|_| OcppError::Timeout {
            operation: format!("{} call", Req::ACTION_NAME),
        })?
        .map_err(|_| OcppError::Transport {
            message: "Connection closed while waiting for CALLRESULT".to_string(),
        })?;

    serde_json::from_value::<Req::Response>(raw_result?).map_err(OcppError::from)
}

impl ChargePoint {
    /// Create a new charge point with default handlers for all nine CSMS→CP
    /// actions.  Call [`on()`][ChargePoint::on] to override any default before
    /// calling [`start()`][ChargePoint::start].
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
                max_power: 7360.0,
                phases: 1,
                energy_meter_serial: Some(format!("EM{:03}", i)),
            };
            connectors.insert(connector_id, Connector::new(connector_config)?);
        }

        let config_store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let connector_states = Arc::new(RwLock::new(HashMap::new()));
        let active_transactions = Arc::new(RwLock::new(HashMap::new()));

        let dispatcher = build_default_dispatcher(
            config_store.clone(),
            connector_states.clone(),
            active_transactions.clone(),
        );

        Ok(Self {
            config,
            connectors: Arc::new(RwLock::new(connectors)),
            client: Arc::new(RwLock::new(None)),
            pending_dispatcher: std::sync::Mutex::new(Some(dispatcher)),
            config_store,
            connector_states,
            active_transactions,
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
            registration_status: Arc::new(RwLock::new(RegistrationStatus::Rejected)),
            is_connected: Arc::new(RwLock::new(false)),
            heartbeat_handle: Arc::new(RwLock::new(None)),
        })
    }

    /// Register (or override) a typed `@on` handler for `Req::ACTION_NAME`.
    ///
    /// Must be called **before** [`start()`][ChargePoint::start].  Panics if
    /// called after the dispatcher has been consumed by `connect()`.
    ///
    /// Port of the Python `@on(Action.xxx)` decorator from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
    pub fn on<Req, Fut, F>(&mut self, handler: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        self.pending_dispatcher
            .get_mut()
            .expect("dispatcher lock poisoned")
            .as_mut()
            .expect("on() called after connect()")
            .on::<Req, Fut, F>(handler);
    }

    /// Start the charge point: initialize connectors and connect to the CSMS.
    pub async fn start(&self) -> Result<()> {
        info!("Starting charge point: {}", self.config.charge_point_id);

        let mut connectors = self.connectors.write().await;
        for connector in connectors.values_mut() {
            connector.set_status(ChargePointStatus::Available).await?;
        }
        drop(connectors);

        let _ = self.event_sender.send(ChargePointEvent::Started);
        self.connect().await?;
        Ok(())
    }

    /// Stop the charge point gracefully.
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping charge point: {}", self.config.charge_point_id);

        if let Some(handle) = self.heartbeat_handle.write().await.take() {
            handle.abort();
        }
        self.disconnect().await?;

        let mut connectors = self.connectors.write().await;
        for connector in connectors.values_mut() {
            connector.set_status(ChargePointStatus::Unavailable).await?;
        }
        Ok(())
    }

    /// Connect to the central system.
    ///
    /// Consumes the `ActionDispatcher` built during setup (freezes it into an
    /// `Arc`) and passes it to a new `WebSocketClient`.  Subsequent calls to
    /// `on()` after this point will panic; subsequent calls to `connect()` will
    /// return an error.
    pub async fn connect(&self) -> Result<()> {
        let url = format!(
            "{}/ocpp/{}",
            self.config.central_system_url.trim_end_matches('/'),
            self.config.charge_point_id
        );
        info!("Connecting to central system: {}", url);

        // Freeze the dispatcher — take it out of the Mutex, wrap in Arc.
        let dispatcher = self
            .pending_dispatcher
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| anyhow::anyhow!("connect() already called; dispatcher consumed"))?;
        let dispatcher = Arc::new(dispatcher);

        let handler: Arc<dyn TransportMessageHandler> = Arc::new(DispatchingHandler::new(
            dispatcher,
            self.event_sender.clone(),
        ));

        let client =
            WebSocketClient::new(url, self.config.transport_config.clone(), handler).await?;

        *self.client.write().await = Some(client);
        *self.is_connected.write().await = true;
        let _ = self.event_sender.send(ChargePointEvent::Connected);
        self.perform_boot_sequence().await?;
        Ok(())
    }

    /// Disconnect from central system.
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

    /// Check if connected to central system.
    pub async fn is_connected(&self) -> bool {
        *self.is_connected.read().await
    }

    /// Get registration status.
    pub async fn registration_status(&self) -> RegistrationStatus {
        *self.registration_status.read().await
    }

    /// Send a typed OCPP CALL and await the matching CALLRESULT.
    ///
    /// Port of `ChargePoint.call()` from the Python reference
    /// (`ocpp/charge_point.py`).
    pub async fn call<Req: OcppAction>(&self, request: Req) -> OcppResult<Req::Response> {
        call_action::<Req>(&self.client, self.config.call_timeout, request).await
    }

    /// Port of `ChargePoint.start()` boot sequence from the Python reference
    /// (`ocpp/charge_point.py`).
    ///
    /// Sends `BootNotificationRequest` via `call()` and handles each response:
    /// - `Accepted` → emit event, start heartbeat at server-supplied interval.
    /// - `Pending` / `Rejected` → wait `interval` seconds, retry.
    ///   After `max_boot_retries` failed attempts returns an error.
    async fn perform_boot_sequence(&self) -> Result<()> {
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

        let max_retries = self.config.max_boot_retries;

        for attempt in 1..=max_retries {
            info!(
                "Sending BootNotification (attempt {}/{})",
                attempt, max_retries
            );

            let response = self
                .call(request.clone())
                .await
                .map_err(|e| anyhow::anyhow!("BootNotification call failed: {}", e))?;

            let interval_secs = response.interval.max(1) as u64;
            *self.registration_status.write().await = response.status;

            match response.status {
                RegistrationStatus::Accepted => {
                    info!(
                        "BootNotification accepted; heartbeat every {}s",
                        interval_secs
                    );
                    let _ = self
                        .event_sender
                        .send(ChargePointEvent::BootNotificationAccepted {
                            current_time: response.current_time,
                            interval: response.interval,
                        });
                    self.start_heartbeat(interval_secs).await;
                    return Ok(());
                }
                RegistrationStatus::Pending | RegistrationStatus::Rejected => {
                    if attempt < max_retries {
                        warn!(
                            "BootNotification {:?} (attempt {}/{}), retrying in {}s",
                            response.status, attempt, max_retries, interval_secs
                        );
                        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "BootNotification not accepted after {} attempts",
            max_retries
        ))
    }

    /// Spawn the heartbeat task using the server-supplied `interval_secs`.
    ///
    /// Uses `call_action()` so each `HeartbeatRequest` is correlated via
    /// `PendingCallMap` — the same semantics as `ChargePoint::call()`.
    /// Heartbeat errors are logged but do not abort the session.
    async fn start_heartbeat(&self, interval_secs: u64) {
        let interval = Duration::from_secs(interval_secs.max(1));
        let client = self.client.clone();
        let call_timeout = self.config.call_timeout;
        let is_connected = self.is_connected.clone();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // Skip the immediate first tick so the heartbeat starts after one interval.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if !*is_connected.read().await {
                    break;
                }
                match call_action::<HeartbeatRequest>(&client, call_timeout, HeartbeatRequest {})
                    .await
                {
                    Ok(_) => {}
                    Err(OcppError::Timeout { .. }) => warn!("Heartbeat timed out"),
                    Err(OcppError::Transport { .. }) => break,
                    Err(e) => error!("Heartbeat error: {}", e),
                }
            }
        });

        *self.heartbeat_handle.write().await = Some(handle);
    }

    /// Get connector by ID.
    pub async fn get_connector(&self, connector_id: ConnectorId) -> Option<Connector> {
        self.connectors.read().await.get(&connector_id).cloned()
    }

    /// Get all connectors.
    pub async fn get_connectors(&self) -> HashMap<ConnectorId, Connector> {
        self.connectors.read().await.clone()
    }

    /// Plug in connector.
    pub async fn plug_in(&self, connector_id: ConnectorId) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.plug_in().await?;
        }
        Ok(())
    }

    /// Plug out connector.
    pub async fn plug_out(&self, connector_id: ConnectorId) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.plug_out().await?;
        }
        Ok(())
    }

    /// Start transaction (local state only; OCPP wire message is issue #21).
    pub async fn start_transaction(&self, connector_id: ConnectorId, id_tag: String) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.start_transaction(id_tag).await?;
        }
        Ok(())
    }

    /// Stop transaction (local state only; OCPP wire message is issue #21).
    pub async fn stop_transaction(
        &self,
        connector_id: ConnectorId,
        reason: Option<String>,
    ) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector
                .stop_transaction(reason.unwrap_or_else(|| "Local".to_string()))
                .await?;
        }
        Ok(())
    }

    /// Set connector fault.
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

    /// Clear connector fault.
    pub async fn clear_fault(&self, connector_id: ConnectorId) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.clear_fault().await?;
        }
        Ok(())
    }

    /// Set connector availability.
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

    /// Take the event receiver (can only be called once).
    pub async fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<ChargePointEvent>> {
        self.event_receiver.write().await.take()
    }

    /// Send status notification for a connector.
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

    /// Read-only access to the OCPP configuration store (e.g. for testing).
    pub fn config_store(&self) -> Arc<RwLock<ConfigurationStore>> {
        self.config_store.clone()
    }

    /// Update the connector state visible to the default `RemoteStartTransaction`
    /// and `UnlockConnector` handlers.  Call this whenever a connector's
    /// `ChargePointStatus` changes (e.g. after `plug_in()` starts a charging
    /// session).
    pub async fn update_connector_state(
        &self,
        connector_id: ConnectorId,
        state: ChargePointStatus,
    ) {
        self.connector_states
            .write()
            .await
            .insert(connector_id, state);
    }

    /// Update the active-transaction map visible to the default
    /// `RemoteStopTransaction` handler.
    pub async fn update_active_transaction(
        &self,
        connector_id: ConnectorId,
        transaction_id: Option<i32>,
    ) {
        let mut txs = self.active_transactions.write().await;
        match transaction_id {
            Some(id) => {
                txs.insert(connector_id, id);
            }
            None => {
                txs.remove(&connector_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Default handler registration
// ---------------------------------------------------------------------------

/// Register the nine default CSMS→CP action handlers.
///
/// Mirrors the hard-coded `match call.action.as_str()` block that was
/// previously in `MessageHandler::handle_message()`.
fn build_default_dispatcher(
    config_store: Arc<RwLock<ConfigurationStore>>,
    connector_states: Arc<RwLock<HashMap<ConnectorId, ChargePointStatus>>>,
    active_transactions: Arc<RwLock<HashMap<ConnectorId, i32>>>,
) -> ActionDispatcher {
    let mut d = ActionDispatcher::new();

    // ChangeAvailability — always accept
    d.on(|_req: ChangeAvailabilityRequest| async move {
        Ok(ChangeAvailabilityResponse {
            status: AvailabilityStatus::Accepted,
        })
    });

    // ChangeConfiguration
    {
        let cs = config_store.clone();
        d.on(move |req: ChangeConfigurationRequest| {
            let cs = cs.clone();
            async move {
                let status = match cs.write().await.set(&req.key, req.value.clone()) {
                    Ok(()) => ConfigurationStatus::Accepted,
                    Err(e) if e.contains("read-only") => ConfigurationStatus::Rejected,
                    Err(_) => ConfigurationStatus::NotSupported,
                };
                Ok(ChangeConfigurationResponse { status })
            }
        });
    }

    // GetConfiguration
    {
        let cs = config_store.clone();
        d.on(move |req: GetConfigurationRequest| {
            let cs = cs.clone();
            async move {
                let store = cs.read().await;
                let (configuration_keys, unknown_keys) = if let Some(keys) = req.key {
                    let mut known = Vec::new();
                    let mut unknown = Vec::new();
                    for key in keys {
                        if let Some(value) = store.get(&key) {
                            known.push(KeyValue {
                                key: key.clone(),
                                readonly: Some(store.is_readonly(&key)),
                                value: Some(value.clone()),
                            });
                        } else {
                            unknown.push(key);
                        }
                    }
                    (
                        Some(known),
                        if unknown.is_empty() {
                            None
                        } else {
                            Some(unknown)
                        },
                    )
                } else {
                    let all = store
                        .keys()
                        .iter()
                        .map(|(k, v)| KeyValue {
                            key: k.clone(),
                            readonly: Some(store.is_readonly(k)),
                            value: Some(v.clone()),
                        })
                        .collect();
                    (Some(all), None)
                };
                Ok(GetConfigurationResponse {
                    configuration_keys,
                    unknown_keys,
                })
            }
        });
    }

    // RemoteStartTransaction
    {
        let states = connector_states.clone();
        d.on(move |req: RemoteStartTransactionRequest| {
            let states = states.clone();
            async move {
                let connector_id = match req.connector_id {
                    Some(id) => ConnectorId::new(id).map_err(|e| OcppError::Internal {
                        message: e.to_string(),
                    })?,
                    None => ConnectorId::new(1).unwrap(),
                };
                let s = states.read().await;
                let status = match s.get(&connector_id) {
                    Some(ChargePointStatus::Available | ChargePointStatus::Reserved) => {
                        RemoteStartStopStatus::Accepted
                    }
                    Some(_) => RemoteStartStopStatus::Rejected,
                    None => RemoteStartStopStatus::Accepted,
                };
                Ok(RemoteStartTransactionResponse { status })
            }
        });
    }

    // RemoteStopTransaction
    {
        let txs = active_transactions.clone();
        d.on(move |req: RemoteStopTransactionRequest| {
            let txs = txs.clone();
            async move {
                let transactions = txs.read().await;
                let status = if transactions.values().any(|&id| id == req.transaction_id) {
                    RemoteStartStopStatus::Accepted
                } else {
                    RemoteStartStopStatus::Rejected
                };
                Ok(RemoteStopTransactionResponse { status })
            }
        });
    }

    // Reset — always accept
    d.on(|req: ResetRequest| async move {
        match req.reset_type {
            ResetType::Soft => info!("Performing soft reset"),
            ResetType::Hard => info!("Performing hard reset"),
        }
        Ok(ResetResponse {
            status: ResetStatus::Accepted,
        })
    });

    // UnlockConnector
    {
        let states = connector_states.clone();
        d.on(move |req: UnlockConnectorRequest| {
            let states = states.clone();
            async move {
                let connector_id =
                    ConnectorId::new(req.connector_id).map_err(|e| OcppError::Internal {
                        message: e.to_string(),
                    })?;
                let s = states.read().await;
                let status = match s.get(&connector_id) {
                    Some(
                        ChargePointStatus::Charging
                        | ChargePointStatus::SuspendedEV
                        | ChargePointStatus::SuspendedEVSE
                        | ChargePointStatus::Finishing,
                    ) => UnlockStatus::Unlocked,
                    Some(ChargePointStatus::Available) => UnlockStatus::UnlockFailed,
                    Some(_) => UnlockStatus::NotSupported,
                    None => UnlockStatus::UnlockFailed,
                };
                Ok(UnlockConnectorResponse { status })
            }
        });
    }

    // ClearCache — always accept
    d.on(|_req: ClearCacheRequest| async move {
        Ok(ClearCacheResponse {
            status: ClearCacheStatus::Accepted,
        })
    });

    // DataTransfer — echo data, always accept
    d.on(|req: DataTransferRequest| async move {
        Ok(DataTransferResponse {
            status: DataTransferStatus::Accepted,
            data: req.data,
        })
    });

    d
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::v16j::{ResetRequest, ResetResponse};

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

        cp.plug_in(connector_id).await.unwrap();
        cp.plug_out(connector_id).await.unwrap();

        cp.plug_in(connector_id).await.unwrap();
        cp.start_transaction(connector_id, "test_tag".to_string())
            .await
            .unwrap();
        cp.stop_transaction(connector_id, Some("Test stop".to_string()))
            .await
            .unwrap();

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

    // --- ActionDispatcher integration tests ---

    #[test]
    fn default_dispatcher_has_nine_handlers() {
        let store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let states = Arc::new(RwLock::new(HashMap::new()));
        let txs = Arc::new(RwLock::new(HashMap::new()));
        let d = build_default_dispatcher(store, states, txs);
        assert_eq!(d.handler_count(), 9);
    }

    #[test]
    fn default_dispatcher_has_all_expected_actions() {
        let store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let states = Arc::new(RwLock::new(HashMap::new()));
        let txs = Arc::new(RwLock::new(HashMap::new()));
        let d = build_default_dispatcher(store, states, txs);

        for action in [
            "ChangeAvailability",
            "ChangeConfiguration",
            "GetConfiguration",
            "RemoteStartTransaction",
            "RemoteStopTransaction",
            "Reset",
            "UnlockConnector",
            "ClearCache",
            "DataTransfer",
        ] {
            assert!(d.has_handler(action), "missing handler for {action}");
        }
    }

    #[tokio::test]
    async fn on_overrides_default_reset_handler() {
        let config = ChargePointConfig::default();
        let mut cp = ChargePoint::new(config).unwrap();

        // Override the default Reset handler
        cp.on(|_req: ResetRequest| async move {
            Ok(ResetResponse {
                status: ResetStatus::Rejected,
            })
        });

        // Verify the override is in place by dispatching directly
        let d = cp
            .pending_dispatcher
            .into_inner()
            .unwrap()
            .expect("dispatcher should still be present");

        let call = ocpp_messages::CallMessage::new(
            "Reset".to_string(),
            serde_json::json!({ "type": "Soft" }),
        )
        .unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Rejected");
    }

    #[tokio::test]
    async fn dispatcher_routes_change_configuration() {
        let config_store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let states = Arc::new(RwLock::new(HashMap::new()));
        let txs = Arc::new(RwLock::new(HashMap::new()));
        let d = build_default_dispatcher(config_store.clone(), states, txs);

        let call = ocpp_messages::CallMessage::new(
            "ChangeConfiguration".to_string(),
            serde_json::json!({ "key": "HeartbeatInterval", "value": "120" }),
        )
        .unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Accepted");

        // Confirm it was stored
        let stored = config_store
            .read()
            .await
            .get("HeartbeatInterval")
            .cloned()
            .unwrap();
        assert_eq!(stored, "120");
    }

    #[tokio::test]
    async fn dispatcher_routes_get_configuration() {
        let config_store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let states = Arc::new(RwLock::new(HashMap::new()));
        let txs = Arc::new(RwLock::new(HashMap::new()));
        let d = build_default_dispatcher(config_store, states, txs);

        let call = ocpp_messages::CallMessage::new(
            "GetConfiguration".to_string(),
            serde_json::json!({ "key": ["HeartbeatInterval"] }),
        )
        .unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        let keys = resp["configurationKey"].as_array().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0]["key"], "HeartbeatInterval");
    }

    #[tokio::test]
    async fn dispatcher_routes_clear_cache() {
        let store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let states = Arc::new(RwLock::new(HashMap::new()));
        let txs = Arc::new(RwLock::new(HashMap::new()));
        let d = build_default_dispatcher(store, states, txs);

        let call = ocpp_messages::CallMessage::new("ClearCache".to_string(), serde_json::json!({}))
            .unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Accepted");
    }

    #[tokio::test]
    async fn dispatcher_routes_data_transfer_echo() {
        let store = Arc::new(RwLock::new(ConfigurationStore::new()));
        let states = Arc::new(RwLock::new(HashMap::new()));
        let txs = Arc::new(RwLock::new(HashMap::new()));
        let d = build_default_dispatcher(store, states, txs);

        let call = ocpp_messages::CallMessage::new(
            "DataTransfer".to_string(),
            serde_json::json!({ "vendorId": "TestVendor", "data": "hello" }),
        )
        .unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Accepted");
        assert_eq!(resp["data"], "hello");
    }

    #[tokio::test]
    async fn dispatching_handler_call_returns_callresult() {
        use ocpp_messages::v16j::{HeartbeatRequest, HeartbeatResponse};
        use ocpp_messages::ActionDispatcher;

        let mut d = ActionDispatcher::new();
        d.on(|_req: HeartbeatRequest| async move {
            Ok(HeartbeatResponse {
                current_time: chrono::Utc::now(),
            })
        });

        let (tx, _rx) = mpsc::unbounded_channel();
        let handler = DispatchingHandler::new(Arc::new(d), tx);

        let call_msg = ocpp_messages::CallMessage::new(
            HeartbeatRequest::ACTION_NAME.to_string(),
            HeartbeatRequest {},
        )
        .unwrap();
        let unique_id = call_msg.unique_id.clone();
        let result = handler
            .handle_message(Message::Call(call_msg))
            .await
            .unwrap();

        let Some(Message::CallResult(cr)) = result else {
            panic!("expected CallResult");
        };
        assert_eq!(cr.unique_id, unique_id);
        assert!(cr.payload.get("currentTime").is_some());
    }

    #[tokio::test]
    async fn dispatching_handler_unknown_action_returns_callerror() {
        use ocpp_messages::ActionDispatcher;
        use ocpp_types::CallErrorCode;

        let d = ActionDispatcher::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        let handler = DispatchingHandler::new(Arc::new(d), tx);

        let call_msg =
            ocpp_messages::CallMessage::new("Unknown".to_string(), serde_json::json!({})).unwrap();
        let result = handler
            .handle_message(Message::Call(call_msg))
            .await
            .unwrap();

        let Some(Message::CallError(ce)) = result else {
            panic!("expected CallError");
        };
        assert_eq!(ce.error_code, CallErrorCode::NotSupported);
    }

    #[tokio::test]
    async fn dispatching_handler_callresult_passthrough_returns_none() {
        use ocpp_messages::ActionDispatcher;

        let d = ActionDispatcher::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        let handler = DispatchingHandler::new(Arc::new(d), tx);

        let cr = ocpp_types::CallResultMessage {
            message_type: MessageType::CallResult,
            unique_id: "test".to_string(),
            payload: serde_json::json!({}),
        };
        let result = handler
            .handle_message(Message::CallResult(cr))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // Boot-sequence tests — in-process mock CSMS
    // -----------------------------------------------------------------------
    //
    // The mock CSMS binds on a random port and responds to each incoming CALL
    // frame with the next pre-loaded JSON payload.  All OCPP framing is done
    // manually so there is no dependency on `OcppServer`.

    struct MockCsms {
        addr: std::net::SocketAddr,
        received_rx: mpsc::UnboundedReceiver<serde_json::Value>,
        _handle: tokio::task::JoinHandle<()>,
    }

    impl MockCsms {
        /// Start a mock CSMS.  `responses` is consumed in order: each CALL
        /// receives the next response payload (wrapped as `[3, unique_id, payload]`).
        /// CALLs beyond the pre-loaded responses are recorded but not replied to.
        async fn start(responses: Vec<serde_json::Value>) -> Self {
            use futures::{SinkExt, StreamExt};
            use tokio::net::TcpListener;
            use tokio_tungstenite::{accept_async, tungstenite::Message as WsMsg};

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let (received_tx, received_rx) = mpsc::unbounded_channel();

            let handle = tokio::spawn(async move {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let ws = accept_async(stream).await.unwrap();
                let (mut sink, mut source) = ws.split();
                let mut responses = responses.into_iter();

                while let Some(Ok(WsMsg::Text(text))) = source.next().await {
                    let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                    let _ = received_tx.send(frame.clone());

                    // Wire format: {"0": "CALL", "1": "<uid>", "2": "Action", "3": {payload}}
                    // MessageType serializes as UPPERCASE string ("CALL"/"CALLRESULT"/"CALLERROR")
                    let unique_id = frame["1"].as_str().unwrap().to_string();
                    if let Some(payload) = responses.next() {
                        let reply =
                            serde_json::json!({"0": "CALLRESULT", "1": unique_id, "2": payload})
                                .to_string();
                        let _ = sink.send(WsMsg::Text(reply)).await;
                    }
                }
            });

            MockCsms {
                addr,
                received_rx,
                _handle: handle,
            }
        }

        /// Wait up to 5 s for the next CALL received by the mock server.
        async fn next_call(&mut self) -> serde_json::Value {
            tokio::time::timeout(std::time::Duration::from_secs(5), self.received_rx.recv())
                .await
                .expect("call arrived within 5s")
                .expect("channel open")
        }

        fn ws_base_url(&self) -> String {
            format!("ws://127.0.0.1:{}", self.addr.port())
        }

        /// Build a `ChargePointConfig` pointing at this mock server.
        fn cp_config(&self, max_boot_retries: u32, call_timeout: u64) -> ChargePointConfig {
            ChargePointConfig {
                central_system_url: self.ws_base_url(),
                call_timeout,
                max_boot_retries,
                ..Default::default()
            }
        }
    }

    fn boot_accepted_payload(interval: i32) -> serde_json::Value {
        serde_json::json!({
            "status": "Accepted",
            "currentTime": "2026-06-12T00:00:00Z",
            "interval": interval
        })
    }

    fn boot_rejected_payload(interval: i32) -> serde_json::Value {
        serde_json::json!({
            "status": "Rejected",
            "currentTime": "2026-06-12T00:00:00Z",
            "interval": interval
        })
    }

    fn boot_pending_payload(interval: i32) -> serde_json::Value {
        serde_json::json!({
            "status": "Pending",
            "currentTime": "2026-06-12T00:00:00Z",
            "interval": interval
        })
    }

    fn heartbeat_payload() -> serde_json::Value {
        serde_json::json!({ "currentTime": "2026-06-12T00:00:00Z" })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn start_sends_boot_notification_first() {
        // Mock server accepts immediately, no retries needed.
        let mut mock = MockCsms::start(vec![boot_accepted_payload(300)]).await;

        let config = mock.cp_config(3, 5);
        let cp = ChargePoint::new(config).unwrap();

        // connect() runs perform_boot_sequence() which sends BootNotification.
        cp.connect().await.unwrap();

        // First frame the mock received must be a BootNotification CALL.
        let first = mock.next_call().await;
        assert_eq!(first["0"], "CALL", "first frame should be a CALL");
        assert_eq!(first["2"], "BootNotification");
        assert!(
            cp.is_connected().await,
            "charge point should be connected after successful boot"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn start_retries_on_rejected_then_fails() {
        // Three Rejected responses with interval=1s → should fail after 3 attempts.
        let responses = vec![
            boot_rejected_payload(1),
            boot_rejected_payload(1),
            boot_rejected_payload(1),
        ];
        let mut mock = MockCsms::start(responses).await;

        let config = mock.cp_config(3, 5);
        let cp = ChargePoint::new(config).unwrap();

        let result = cp.connect().await;
        assert!(
            result.is_err(),
            "expected error after all retries exhausted"
        );

        // The mock should have received exactly 3 BootNotification CALLs.
        let mut boot_count = 0u32;
        for _ in 0..3 {
            let call = mock.next_call().await;
            if call["2"] == "BootNotification" {
                boot_count += 1;
            }
        }
        assert_eq!(boot_count, 3, "expected 3 BootNotification attempts");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn start_accepted_starts_heartbeat() {
        // BootNotification accepted with interval=1s; pre-load heartbeat responses.
        let responses = vec![
            boot_accepted_payload(1),
            heartbeat_payload(),
            heartbeat_payload(),
            heartbeat_payload(),
        ];
        let mut mock = MockCsms::start(responses).await;

        let config = mock.cp_config(3, 5);
        let cp = ChargePoint::new(config).unwrap();

        cp.connect().await.unwrap();

        // First received frame: BootNotification.
        let boot = mock.next_call().await;
        assert_eq!(boot["2"], "BootNotification");

        // Wait slightly more than one heartbeat interval.
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // At least one Heartbeat should have arrived.
        let hb = mock.next_call().await;
        assert_eq!(
            hb["2"], "Heartbeat",
            "expected a Heartbeat CALL after interval"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn start_pending_then_accepted_on_retry() {
        // First response: Pending (interval=1s). Second: Accepted.
        let responses = vec![boot_pending_payload(1), boot_accepted_payload(300)];
        let mut mock = MockCsms::start(responses).await;

        let config = mock.cp_config(3, 5);
        let cp = ChargePoint::new(config).unwrap();

        // Should succeed after the retry.
        cp.connect().await.unwrap();

        let first = mock.next_call().await;
        assert_eq!(first["2"], "BootNotification");

        let second = mock.next_call().await;
        assert_eq!(
            second["2"], "BootNotification",
            "second attempt should also be BootNotification"
        );

        assert_eq!(cp.registration_status().await, RegistrationStatus::Accepted);
    }

    #[test]
    fn boot_retries_default_is_3() {
        assert_eq!(ChargePointConfig::default().max_boot_retries, 3);
    }
}
