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
use message_handler::MessageHandler;
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    RegistrationStatus, StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::{Message, OcppAction};
use ocpp_transport::client::WebSocketClient;
use ocpp_transport::{
    MessageHandler as TransportMessageHandler, Transport, TransportConfig, TransportEvent,
};
use ocpp_types::v16j::{ChargePointStatus, ChargePointVendorInfo};
use ocpp_types::{ConnectorId, OcppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
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

        let message_handler = Arc::new(MessageHandler::new(event_sender.clone()));

        Ok(Self {
            config,
            connectors: Arc::new(RwLock::new(connectors)),
            client: Arc::new(RwLock::new(None)),
            message_handler,
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
}
