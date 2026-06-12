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
pub mod state_machine;
pub mod transaction;

use anyhow::Result;
use auth_cache::AuthCache;
use connector::{Connector, ConnectorConfig};
use error::ChargePointError;
use message_handler::MessageHandler;
use ocpp_messages::v16j::{
    AuthorizeRequest, BootNotificationRequest, BootNotificationResponse, HeartbeatRequest,
    RegistrationStatus, StatusNotificationRequest,
};
use ocpp_messages::{CallMessage, Message, MessageType, OcppAction};
use ocpp_transport::client::WebSocketClient;
use ocpp_transport::{
    MessageHandler as TransportMessageHandler, Transport, TransportConfig, TransportEvent,
};
use ocpp_types::common::AuthorizationStatus;
use ocpp_types::common::IdTagInfo;
use ocpp_types::v16j::{ChargePointStatus, ChargePointVendorInfo};
use ocpp_types::{ConnectorId, OcppError, OcppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Authorization cache TTL in seconds (default 86400 = 24 h).
    /// Used when `IdTagInfo.expiry_date` is absent.
    pub auth_cache_ttl: u64,
    /// If true, return a stale (expired) cached auth result when the CSMS
    /// is unreachable and the `authorize` CALL times out.
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
            heartbeat_interval: 300,       // 5 minutes
            meter_values_interval: 60,     // 1 minute
            connection_retry_interval: 30, // 30 seconds
            max_connection_retries: 10,
            auto_reconnect: true,
            call_timeout: 30,
            auth_cache_ttl: 86400,
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
    /// Error occurred
    Error { error: ChargePointError },
}

/// Charge point event handler trait
#[async_trait::async_trait]
pub trait ChargePointEventHandler: Send + Sync {
    /// Handle charge point event
    async fn handle_event(&self, event: ChargePointEvent);
}

/// Main charge point implementation
pub struct ChargePoint {
    /// Configuration
    config: ChargePointConfig,
    /// Connectors
    connectors: Arc<RwLock<HashMap<ConnectorId, Connector>>>,
    /// WebSocket client
    client: Arc<RwLock<Option<WebSocketClient>>>,
    /// Message handler
    message_handler: Arc<MessageHandler>,
    /// Authorization cache (shared with MessageHandler for ClearCache wiring)
    auth_cache: Arc<AuthCache>,
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
    /// Create a new charge point
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

        let auth_cache = Arc::new(AuthCache::new(Duration::from_secs(config.auth_cache_ttl)));
        let message_handler = Arc::new(MessageHandler::with_auth_cache(
            event_sender.clone(),
            Arc::clone(&auth_cache),
        ));

        Ok(Self {
            config,
            connectors: Arc::new(RwLock::new(connectors)),
            client: Arc::new(RwLock::new(None)),
            message_handler,
            auth_cache,
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
            registration_status: Arc::new(RwLock::new(RegistrationStatus::Rejected)),
            is_connected: Arc::new(RwLock::new(false)),
            heartbeat_handle: Arc::new(RwLock::new(None)),
        })
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

        let client = WebSocketClient::new(
            url,
            self.config.transport_config.clone(),
            self.message_handler.clone(),
        )
        .await?;

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

    /// Send boot notification
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

    /// Start transaction
    pub async fn start_transaction(&self, connector_id: ConnectorId, id_tag: String) -> Result<()> {
        let mut connectors = self.connectors.write().await;
        if let Some(connector) = connectors.get_mut(&connector_id) {
            connector.start_transaction(id_tag).await?;
        }
        Ok(())
    }

    /// Stop transaction
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

    /// Look up an id-tag in the local authorization cache without sending an OCPP CALL.
    ///
    /// Returns `Some(IdTagInfo)` on a live cache hit, `None` on miss or expiry.
    pub async fn local_auth_lookup(&self, id_tag: &str) -> Option<IdTagInfo> {
        self.auth_cache.get(id_tag).await
    }

    /// Look up an id-tag including expired entries (offline-fallback path).
    ///
    /// Only used when `ChargePointConfig::offline_auth_stale_ok` is true and
    /// a fresh `AuthorizeRequest` CALL has timed out.
    pub async fn local_auth_lookup_stale(&self, id_tag: &str) -> Option<IdTagInfo> {
        if self.config.offline_auth_stale_ok {
            self.auth_cache.get_stale(id_tag).await
        } else {
            None
        }
    }

    /// Authorize an ID tag via local cache or CSMS.
    ///
    /// Ports the local-auth-list + `_send_authorize()` pattern from
    /// `ocpp/charge_point.py`:
    ///
    /// 1. **Cache hit** (live entry) → return immediately, no CALL sent.
    /// 2. **Cache miss** → send `AuthorizeRequest` via `call()`, cache the result.
    /// 3. **Timeout** with `offline_auth_stale_ok = true` → return stale cached entry.
    /// 4. **Timeout** with `offline_auth_stale_ok = false` → return `Invalid`.
    pub async fn authorize(&self, id_tag: &str) -> OcppResult<IdTagInfo> {
        if let Some(info) = self.auth_cache.get(id_tag).await {
            return Ok(info);
        }

        let response = match self
            .call(AuthorizeRequest {
                id_tag: id_tag.to_string(),
            })
            .await
        {
            Ok(r) => r,
            Err(OcppError::Timeout { .. }) => {
                if self.config.offline_auth_stale_ok {
                    if let Some(stale) = self.auth_cache.get_stale(id_tag).await {
                        warn!(
                            "authorize: CSMS unreachable, using stale cache for {}",
                            id_tag
                        );
                        return Ok(stale);
                    }
                }
                warn!(
                    "authorize: CSMS unreachable, no cache, returning Invalid for {}",
                    id_tag
                );
                return Ok(IdTagInfo {
                    status: AuthorizationStatus::Invalid,
                    parent_id_tag: None,
                    expiry_date: None,
                });
            }
            Err(e) => return Err(e),
        };

        self.auth_cache
            .insert(id_tag, response.id_tag_info.clone())
            .await;
        Ok(response.id_tag_info)
    }

    /// Get event receiver
    pub async fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<ChargePointEvent>> {
        self.event_receiver.write().await.take()
    }

    /// Handle boot notification response
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
}

#[async_trait::async_trait]
impl TransportMessageHandler for ChargePoint {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        self.message_handler.handle_message(message).await
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

        // Test plug in/out cycle
        cp.plug_in(connector_id).await.unwrap();
        cp.plug_out(connector_id).await.unwrap();

        // Test transaction operations (cable must be plugged in first)
        cp.plug_in(connector_id).await.unwrap();
        cp.start_transaction(connector_id, "test_tag".to_string())
            .await
            .unwrap();
        cp.stop_transaction(connector_id, Some("Test stop".to_string()))
            .await
            .unwrap();

        // Test fault operations
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

    #[test]
    fn auth_cache_ttl_default_is_24h() {
        let config = ChargePointConfig::default();
        assert_eq!(config.auth_cache_ttl, 86400);
    }

    #[test]
    fn offline_auth_stale_ok_default_is_false() {
        let config = ChargePointConfig::default();
        assert!(!config.offline_auth_stale_ok);
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
    async fn local_auth_lookup_returns_none_on_empty_cache() {
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        assert!(cp.local_auth_lookup("UNKNOWN_TAG").await.is_none());
    }

    #[tokio::test]
    async fn local_auth_lookup_returns_cached_result() {
        use ocpp_types::common::{AuthorizationStatus, IdTagInfo};

        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        cp.auth_cache
            .insert(
                "TAG_CACHED",
                IdTagInfo {
                    status: AuthorizationStatus::Accepted,
                    parent_id_tag: None,
                    expiry_date: None,
                },
            )
            .await;

        let result = cp.local_auth_lookup("TAG_CACHED").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, AuthorizationStatus::Accepted);
    }

    #[tokio::test]
    async fn local_auth_lookup_stale_off_returns_none_when_disabled() {
        use chrono::Duration as CD;
        use ocpp_types::common::{AuthorizationStatus, IdTagInfo};

        let cp = ChargePoint::new(ChargePointConfig {
            offline_auth_stale_ok: false,
            ..Default::default()
        })
        .unwrap();

        cp.auth_cache
            .insert(
                "STALE_TAG",
                IdTagInfo {
                    status: AuthorizationStatus::Accepted,
                    parent_id_tag: None,
                    expiry_date: Some(chrono::Utc::now() - CD::seconds(1)),
                },
            )
            .await;

        assert!(cp.local_auth_lookup_stale("STALE_TAG").await.is_none());
    }

    #[tokio::test]
    async fn local_auth_lookup_stale_on_returns_expired_entry() {
        use chrono::Duration as CD;
        use ocpp_types::common::{AuthorizationStatus, IdTagInfo};

        let cp = ChargePoint::new(ChargePointConfig {
            offline_auth_stale_ok: true,
            ..Default::default()
        })
        .unwrap();

        cp.auth_cache
            .insert(
                "STALE_TAG2",
                IdTagInfo {
                    status: AuthorizationStatus::Accepted,
                    parent_id_tag: None,
                    expiry_date: Some(chrono::Utc::now() - CD::seconds(1)),
                },
            )
            .await;

        let result = cp.local_auth_lookup_stale("STALE_TAG2").await;
        assert!(result.is_some());
    }

    // --- authorize() tests ---

    #[tokio::test]
    async fn authorize_cache_hit_returns_accepted_without_ocpp_call() {
        // Pre-populate cache; authorize() must return from cache without touching
        // the WS client (which is None / not connected).
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        cp.auth_cache
            .insert(
                "KNOWN_TAG",
                IdTagInfo {
                    status: AuthorizationStatus::Accepted,
                    parent_id_tag: None,
                    expiry_date: None,
                },
            )
            .await;

        let result = cp.authorize("KNOWN_TAG").await;
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert_eq!(result.unwrap().status, AuthorizationStatus::Accepted);
    }

    #[tokio::test]
    async fn authorize_cache_miss_attempts_ocpp_call() {
        // No cache entry, no WS connection → authorize() must attempt ChargePoint::call()
        // which returns OcppError::Transport (not an AuthorizationError or Ok).
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();

        let result = cp.authorize("UNCACHED_TAG").await;
        match result {
            Err(OcppError::Transport { .. }) => {} // expected: call() tried, WS not connected
            other => panic!("expected Transport error on cache miss, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn authorize_expired_cache_entry_triggers_fresh_call() {
        // An expired entry must be evicted and a fresh CALL attempted.
        use chrono::Duration as CD;

        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        cp.auth_cache
            .insert(
                "EXPIRED_TAG",
                IdTagInfo {
                    status: AuthorizationStatus::Accepted,
                    parent_id_tag: None,
                    expiry_date: Some(chrono::Utc::now() - CD::seconds(1)),
                },
            )
            .await;

        // authorize() must NOT return the cached (expired) Accepted — it must
        // attempt a fresh CALL and fail with Transport (no WS connection).
        let result = cp.authorize("EXPIRED_TAG").await;
        match result {
            Err(OcppError::Transport { .. }) => {} // stale entry evicted, fresh call attempted
            Ok(info) if info.status == AuthorizationStatus::Accepted => {
                panic!("authorize() returned expired cached Accepted — should have evicted it")
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn authorize_blocked_cache_hit_returned_without_call() {
        // A cached Blocked status is returned immediately (no CALL needed to re-check).
        let cp = ChargePoint::new(ChargePointConfig::default()).unwrap();
        cp.auth_cache
            .insert(
                "BLOCKED_TAG",
                IdTagInfo {
                    status: AuthorizationStatus::Blocked,
                    parent_id_tag: None,
                    expiry_date: None,
                },
            )
            .await;

        let result = cp.authorize("BLOCKED_TAG").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, AuthorizationStatus::Blocked);
    }
}
