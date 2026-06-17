//! # Message Handler for OCPP Charge Point
//!
//! This module provides message handling capabilities for processing incoming
//! OCPP messages from the Central System and generating appropriate responses.

use crate::error::ChargePointError;
use anyhow::Result;
use ocpp_messages::v16j::{
    BootNotificationResponse, ChangeAvailabilityRequest, ChangeAvailabilityResponse,
    ChangeConfigurationRequest, ChangeConfigurationResponse, ClearCacheRequest, ClearCacheResponse,
    DataTransferRequest, DataTransferResponse, GetConfigurationRequest, GetConfigurationResponse,
    HeartbeatResponse, RemoteStartTransactionRequest, RemoteStartTransactionResponse,
    RemoteStopTransactionRequest, RemoteStopTransactionResponse, ResetRequest, ResetResponse,
    UnlockConnectorRequest, UnlockConnectorResponse,
};
use ocpp_messages::{CallMessage, CallResultMessage, Message};
use ocpp_transport::MessageHandler as TransportMessageHandler;
use ocpp_types::common::{AvailabilityStatus, AvailabilityType, KeyValue};
use ocpp_types::v16j::{
    ConfigurationStatus, DataTransferStatus, RemoteStartStopStatus, ResetStatus, ResetType,
    UnlockStatus,
};
use ocpp_types::{ConnectorId, OcppResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::ChargePointEvent;

/// Configuration key-value store
#[derive(Debug, Clone)]
pub struct ConfigurationStore {
    /// Configuration keys with values
    keys: HashMap<String, String>,
    /// Read-only keys
    readonly_keys: std::collections::HashSet<String>,
}

impl ConfigurationStore {
    /// Create new configuration store with default values
    pub fn new() -> Self {
        let mut keys = HashMap::new();
        let mut readonly_keys = std::collections::HashSet::new();

        // Add default OCPP configuration keys
        keys.insert("AuthorizeRemoteTxRequests".to_string(), "true".to_string());
        keys.insert("ClockAlignedDataInterval".to_string(), "0".to_string());
        keys.insert("ConnectionTimeOut".to_string(), "60".to_string());
        keys.insert("GetConfigurationMaxKeys".to_string(), "100".to_string());
        keys.insert("HeartbeatInterval".to_string(), "86400".to_string());
        keys.insert("LocalAuthListEnabled".to_string(), "false".to_string());
        keys.insert("LocalAuthListMaxLength".to_string(), "0".to_string());
        keys.insert("LocalAuthorizeOffline".to_string(), "true".to_string());
        keys.insert("LocalPreAuthorize".to_string(), "false".to_string());
        keys.insert("MeterValuesAlignedData".to_string(), "".to_string());
        keys.insert(
            "MeterValuesSampledData".to_string(),
            "Energy.Active.Import.Register".to_string(),
        );
        keys.insert("MeterValueSampleInterval".to_string(), "60".to_string());
        keys.insert("NumberOfConnectors".to_string(), "2".to_string());
        keys.insert("ResetRetries".to_string(), "3".to_string());
        keys.insert(
            "ConnectorPhaseRotation".to_string(),
            "NotApplicable".to_string(),
        );
        keys.insert(
            "StopTransactionOnEVSideDisconnect".to_string(),
            "true".to_string(),
        );
        keys.insert("StopTransactionOnInvalidId".to_string(), "true".to_string());
        keys.insert("StopTxnAlignedData".to_string(), "".to_string());
        keys.insert("StopTxnSampledData".to_string(), "".to_string());
        keys.insert("SupportedFeatureProfiles".to_string(), "Core".to_string());
        keys.insert("TransactionMessageAttempts".to_string(), "3".to_string());
        keys.insert(
            "TransactionMessageRetryInterval".to_string(),
            "60".to_string(),
        );
        keys.insert(
            "UnlockConnectorOnEVSideDisconnect".to_string(),
            "true".to_string(),
        );

        // Mark some keys as read-only
        readonly_keys.insert("NumberOfConnectors".to_string());
        readonly_keys.insert("SupportedFeatureProfiles".to_string());

        Self {
            keys,
            readonly_keys,
        }
    }

    /// Get configuration value
    pub fn get(&self, key: &str) -> Option<&String> {
        self.keys.get(key)
    }

    /// Set configuration value
    pub fn set(&mut self, key: &str, value: String) -> Result<(), String> {
        if self.readonly_keys.contains(key) {
            return Err(format!("Key '{}' is read-only", key));
        }
        self.keys.insert(key.to_string(), value);
        Ok(())
    }

    /// Get all keys
    pub fn keys(&self) -> &HashMap<String, String> {
        &self.keys
    }

    /// Check if key is read-only
    pub fn is_readonly(&self, key: &str) -> bool {
        self.readonly_keys.contains(key)
    }
}

impl Default for ConfigurationStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Message handler for processing OCPP messages
pub struct MessageHandler {
    /// Event sender for charge point events
    event_sender: mpsc::UnboundedSender<ChargePointEvent>,
    /// Configuration store
    config_store: Arc<RwLock<ConfigurationStore>>,
    /// Connector states
    connector_states: Arc<RwLock<HashMap<ConnectorId, ocpp_types::v16j::ChargePointStatus>>>,
    /// Active transactions
    active_transactions: Arc<RwLock<HashMap<ConnectorId, i32>>>,
}

impl MessageHandler {
    /// Create a new message handler
    pub fn new(event_sender: mpsc::UnboundedSender<ChargePointEvent>) -> Self {
        Self {
            event_sender,
            config_store: Arc::new(RwLock::new(ConfigurationStore::new())),
            connector_states: Arc::new(RwLock::new(HashMap::new())),
            active_transactions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle BootNotificationResponse
    async fn handle_boot_notification_response(
        &self,
        response: BootNotificationResponse,
    ) -> Result<()> {
        info!("Received boot notification response: {:?}", response.status);

        let _ = self
            .event_sender
            .send(ChargePointEvent::BootNotificationAccepted {
                current_time: response.current_time,
                interval: response.interval,
            });

        // Update heartbeat interval configuration
        let mut config = self.config_store.write().await;
        config
            .set("HeartbeatInterval", response.interval.to_string())
            .unwrap_or_else(|e| warn!("Failed to update HeartbeatInterval: {}", e));

        Ok(())
    }

    /// Handle HeartbeatResponse
    async fn handle_heartbeat_response(&self, response: HeartbeatResponse) -> Result<()> {
        debug!("Received heartbeat response at: {}", response.current_time);
        Ok(())
    }

    /// Handle ChangeAvailability request
    async fn handle_change_availability(
        &self,
        request: ChangeAvailabilityRequest,
    ) -> Result<ChangeAvailabilityResponse> {
        info!(
            "Change availability request - Connector: {}, Type: {:?}",
            request.connector_id, request.availability_type
        );

        let connector_id = if request.connector_id == 0 {
            // Connector 0 means all connectors
            None
        } else {
            Some(ConnectorId::new(request.connector_id)?)
        };

        let available = matches!(request.availability_type, AvailabilityType::Operative);

        // For now, always accept the change
        let status = AvailabilityStatus::Accepted;

        debug!(
            "Change availability: {:?} -> {} (status: {:?})",
            connector_id, available, status
        );

        Ok(ChangeAvailabilityResponse { status })
    }

    /// Handle ChangeConfiguration request
    async fn handle_change_configuration(
        &self,
        request: ChangeConfigurationRequest,
    ) -> Result<ChangeConfigurationResponse> {
        info!(
            "Change configuration request - Key: {}, Value: {}",
            request.key, request.value
        );

        let mut config = self.config_store.write().await;

        let status = match config.set(&request.key, request.value.clone()) {
            Ok(()) => {
                info!("Configuration changed: {} = {}", request.key, request.value);
                ConfigurationStatus::Accepted
            }
            Err(e) if e.contains("read-only") => {
                warn!("Rejected read-only configuration key: {}", request.key);
                ConfigurationStatus::Rejected
            }
            Err(e) => {
                error!("Failed to set configuration: {}", e);
                ConfigurationStatus::NotSupported
            }
        };

        // Some configuration changes might require reboot
        let status = match request.key.as_str() {
            "HeartbeatInterval" | "MeterValueSampleInterval" => {
                // These can be changed at runtime
                status
            }
            "NumberOfConnectors" | "SupportedFeatureProfiles" => {
                // These require reboot but are read-only anyway
                status
            }
            _ => status,
        };

        Ok(ChangeConfigurationResponse { status })
    }

    /// Handle GetConfiguration request
    async fn handle_get_configuration(
        &self,
        request: GetConfigurationRequest,
    ) -> Result<GetConfigurationResponse> {
        let config = self.config_store.read().await;

        let (configuration_keys, unknown_keys) = if let Some(keys) = request.key {
            // Return specific keys
            let mut config_keys = Vec::new();
            let mut unknown = Vec::new();

            for key in keys {
                if let Some(value) = config.get(&key) {
                    config_keys.push(KeyValue {
                        key: key.clone(),
                        readonly: Some(config.is_readonly(&key)),
                        value: Some(value.clone()),
                    });
                } else {
                    unknown.push(key);
                }
            }

            (
                Some(config_keys),
                if unknown.is_empty() {
                    None
                } else {
                    Some(unknown)
                },
            )
        } else {
            // Return all keys
            let config_keys: Vec<_> = config
                .keys()
                .iter()
                .map(|(key, value)| KeyValue {
                    key: key.clone(),
                    readonly: Some(config.is_readonly(key)),
                    value: Some(value.clone()),
                })
                .collect();

            (Some(config_keys), None)
        };

        debug!(
            "Get configuration response: {} keys, {} unknown",
            configuration_keys.as_ref().map_or(0, |k| k.len()),
            unknown_keys.as_ref().map_or(0, |k| k.len())
        );

        Ok(GetConfigurationResponse {
            configuration_keys,
            unknown_keys,
        })
    }

    /// Handle RemoteStartTransaction request
    async fn handle_remote_start_transaction(
        &self,
        request: RemoteStartTransactionRequest,
    ) -> Result<RemoteStartTransactionResponse> {
        info!(
            "Remote start transaction request - Connector: {:?}, ID Tag: {}",
            request.connector_id, request.id_tag
        );

        // Check if connector is available
        let connector_id = if let Some(id) = request.connector_id {
            ConnectorId::new(id)?
        } else {
            // Find an available connector
            // For simplicity, use connector 1
            ConnectorId::new(1)?
        };

        let states = self.connector_states.read().await;
        let status = if let Some(state) = states.get(&connector_id) {
            match state {
                ocpp_types::v16j::ChargePointStatus::Available
                | ocpp_types::v16j::ChargePointStatus::Reserved => RemoteStartStopStatus::Accepted,
                _ => RemoteStartStopStatus::Rejected,
            }
        } else {
            RemoteStartStopStatus::Accepted // Assume available if not tracked
        };

        if status == RemoteStartStopStatus::Accepted {
            debug!(
                "Remote start transaction accepted for connector {} with ID tag {}",
                connector_id.value(),
                request.id_tag
            );
        } else {
            warn!(
                "Remote start transaction rejected for connector {} (not available)",
                connector_id.value()
            );
        }

        Ok(RemoteStartTransactionResponse { status })
    }

    /// Handle RemoteStopTransaction request
    async fn handle_remote_stop_transaction(
        &self,
        request: RemoteStopTransactionRequest,
    ) -> Result<RemoteStopTransactionResponse> {
        info!(
            "Remote stop transaction request - Transaction ID: {}",
            request.transaction_id
        );

        let transactions = self.active_transactions.read().await;

        // Find connector with this transaction
        let status = if transactions
            .values()
            .any(|&tx_id| tx_id == request.transaction_id)
        {
            RemoteStartStopStatus::Accepted
        } else {
            RemoteStartStopStatus::Rejected
        };

        if status == RemoteStartStopStatus::Accepted {
            debug!(
                "Remote stop transaction accepted for transaction ID {}",
                request.transaction_id
            );
        } else {
            warn!(
                "Remote stop transaction rejected for transaction ID {} (not found)",
                request.transaction_id
            );
        }

        Ok(RemoteStopTransactionResponse { status })
    }

    /// Handle Reset request
    async fn handle_reset(&self, request: ResetRequest) -> Result<ResetResponse> {
        info!("Reset request - Type: {:?}", request.reset_type);

        // Always accept reset requests
        let status = ResetStatus::Accepted;

        match request.reset_type {
            ResetType::Soft => {
                info!("Performing soft reset...");
                // In a real implementation, this would restart the application
            }
            ResetType::Hard => {
                info!("Performing hard reset...");
                // In a real implementation, this would restart the hardware
            }
        }

        Ok(ResetResponse { status })
    }

    /// Handle UnlockConnector request
    async fn handle_unlock_connector(
        &self,
        request: UnlockConnectorRequest,
    ) -> Result<UnlockConnectorResponse> {
        info!(
            "Unlock connector request - Connector ID: {}",
            request.connector_id
        );

        let connector_id = ConnectorId::new(request.connector_id)?;

        // Check connector state
        let states = self.connector_states.read().await;
        let status = if let Some(state) = states.get(&connector_id) {
            match state {
                ocpp_types::v16j::ChargePointStatus::Charging
                | ocpp_types::v16j::ChargePointStatus::SuspendedEV
                | ocpp_types::v16j::ChargePointStatus::SuspendedEVSE
                | ocpp_types::v16j::ChargePointStatus::Finishing => UnlockStatus::Unlocked,
                ocpp_types::v16j::ChargePointStatus::Available => UnlockStatus::UnlockFailed,
                _ => UnlockStatus::NotSupported,
            }
        } else {
            UnlockStatus::UnlockFailed
        };

        debug!(
            "Unlock connector {} result: {:?}",
            request.connector_id, status
        );

        Ok(UnlockConnectorResponse { status })
    }

    /// Handle ClearCache request
    async fn handle_clear_cache(&self, _request: ClearCacheRequest) -> Result<ClearCacheResponse> {
        info!("Clear cache request");

        // For now, always accept
        let status = ocpp_types::v16j::ClearCacheStatus::Accepted;

        debug!("Clear cache result: {:?}", status);

        Ok(ClearCacheResponse { status })
    }

    /// Handle DataTransfer request
    async fn handle_data_transfer(
        &self,
        request: DataTransferRequest,
    ) -> Result<DataTransferResponse> {
        info!(
            "Data transfer request - Vendor ID: {}, Message ID: {:?}",
            request.vendor_id, request.message_id
        );

        // For this implementation, we'll accept data transfer but not process it
        let status = DataTransferStatus::Accepted;
        let data = request.data; // Echo back the data

        debug!("Data transfer result: {:?}", status);

        Ok(DataTransferResponse { status, data })
    }

    /// Extract message payload as specific type
    fn extract_payload<T>(&self, call: &CallMessage) -> Result<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        call.payload_as().map_err(|e| {
            ChargePointError::serialization(format!("Failed to deserialize payload: {}", e)).into()
        })
    }

    /// Create call result message
    fn create_call_result<T>(&self, unique_id: String, payload: T) -> Result<Message>
    where
        T: serde::Serialize,
    {
        let result = CallResultMessage::new(unique_id, payload).map_err(|e| {
            ChargePointError::serialization(format!("Failed to create call result: {}", e))
        })?;
        Ok(Message::CallResult(result))
    }

    /// Update connector state
    pub async fn update_connector_state(
        &self,
        connector_id: ConnectorId,
        state: ocpp_types::v16j::ChargePointStatus,
    ) {
        self.connector_states
            .write()
            .await
            .insert(connector_id, state);
    }

    /// Update active transaction
    pub async fn update_active_transaction(
        &self,
        connector_id: ConnectorId,
        transaction_id: Option<i32>,
    ) {
        let mut transactions = self.active_transactions.write().await;
        if let Some(tx_id) = transaction_id {
            transactions.insert(connector_id, tx_id);
        } else {
            transactions.remove(&connector_id);
        }
    }

    /// Get configuration value
    pub async fn get_config_value(&self, key: &str) -> Option<String> {
        self.config_store.read().await.get(key).cloned()
    }

    /// Set configuration value
    pub async fn set_config_value(&self, key: &str, value: String) -> Result<()> {
        self.config_store
            .write()
            .await
            .set(key, value)
            .map_err(|e| ChargePointError::configuration(e).into())
    }
}

#[async_trait::async_trait]
impl TransportMessageHandler for MessageHandler {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        match message {
            Message::Call(call) => {
                debug!("Handling call: {}", call.action);

                let response = match call.action.as_str() {
                    "ChangeAvailability" => {
                        let request: ChangeAvailabilityRequest = self.extract_payload(&call)?;
                        let response = self.handle_change_availability(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "ChangeConfiguration" => {
                        let request: ChangeConfigurationRequest = self.extract_payload(&call)?;
                        let response = self.handle_change_configuration(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "GetConfiguration" => {
                        let request: GetConfigurationRequest = self.extract_payload(&call)?;
                        let response = self.handle_get_configuration(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "RemoteStartTransaction" => {
                        let request: RemoteStartTransactionRequest = self.extract_payload(&call)?;
                        let response = self.handle_remote_start_transaction(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "RemoteStopTransaction" => {
                        let request: RemoteStopTransactionRequest = self.extract_payload(&call)?;
                        let response = self.handle_remote_stop_transaction(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "Reset" => {
                        let request: ResetRequest = self.extract_payload(&call)?;
                        let response = self.handle_reset(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "UnlockConnector" => {
                        let request: UnlockConnectorRequest = self.extract_payload(&call)?;
                        let response = self.handle_unlock_connector(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "ClearCache" => {
                        let request: ClearCacheRequest = self.extract_payload(&call)?;
                        let response = self.handle_clear_cache(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    "DataTransfer" => {
                        let request: DataTransferRequest = self.extract_payload(&call)?;
                        let response = self.handle_data_transfer(request).await?;
                        self.create_call_result(call.unique_id, response)?
                    }
                    _ => {
                        warn!("Unsupported action: {}", call.action);
                        return Err(ocpp_types::OcppError::NotSupported {
                            feature: call.action,
                        });
                    }
                };

                Ok(Some(response))
            }
            Message::CallResult(result) => {
                debug!("Handling call result: {}", result.unique_id);

                // Try to parse as different response types
                if let Ok(response) = result.payload_as::<BootNotificationResponse>() {
                    self.handle_boot_notification_response(response).await?;
                } else if let Ok(response) = result.payload_as::<HeartbeatResponse>() {
                    self.handle_heartbeat_response(response).await?;
                } else {
                    debug!("Unknown call result type for ID: {}", result.unique_id);
                }

                Ok(None)
            }
            Message::CallError(error) => {
                error!(
                    "Received call error: {} - {} ({})",
                    error.unique_id, error.error_code, error.error_description
                );

                let _ = self.event_sender.send(ChargePointEvent::Error {
                    error: ChargePointError::central_system(format!(
                        "Call error: {} - {}",
                        error.error_code, error.error_description
                    )),
                });

                Ok(None)
            }
        }
    }

    async fn handle_event(&self, event: ocpp_transport::TransportEvent) {
        match event {
            ocpp_transport::TransportEvent::Connected { connection_id, .. } => {
                debug!("Transport connected: {}", connection_id);
            }
            ocpp_transport::TransportEvent::Disconnected {
                connection_id,
                reason,
            } => {
                warn!("Transport disconnected: {} - {}", connection_id, reason);
            }
            ocpp_transport::TransportEvent::MessageReceived {
                connection_id,
                message,
            } => {
                debug!("Message received from {}: {:?}", connection_id, message);
            }
            ocpp_transport::TransportEvent::MessageSent {
                connection_id,
                message_id,
            } => {
                debug!("Message sent to {}: {}", connection_id, message_id);
            }
            // CSMS-side observability event (#47): a CP only ever sends
            // StatusNotification, so receiving one here is unexpected — log it.
            ocpp_transport::TransportEvent::StatusNotification {
                cp_id,
                connector_id,
                status,
            } => {
                debug!(
                    "StatusNotification event for {} connector {}: {:?}",
                    cp_id, connector_id, status
                );
            }
            ocpp_transport::TransportEvent::Error {
                connection_id,
                error,
            } => {
                error!(
                    "Transport error on {:?}: {}",
                    connection_id.unwrap_or_default(),
                    error
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_message_handler_creation() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = MessageHandler::new(sender);

        let config_value = handler.get_config_value("HeartbeatInterval").await;
        assert_eq!(config_value, Some("86400".to_string()));
    }

    #[tokio::test]
    async fn test_configuration_store() {
        let mut store = ConfigurationStore::new();

        // Test get
        assert!(store.get("HeartbeatInterval").is_some());
        assert!(store.get("NonExistentKey").is_none());

        // Test set
        assert!(store.set("CustomKey", "CustomValue".to_string()).is_ok());
        assert_eq!(store.get("CustomKey"), Some(&"CustomValue".to_string()));

        // Test readonly
        assert!(store.set("NumberOfConnectors", "5".to_string()).is_err());
        assert!(store.is_readonly("NumberOfConnectors"));
    }

    #[tokio::test]
    async fn test_change_configuration() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = MessageHandler::new(sender);

        let request = ChangeConfigurationRequest {
            key: "HeartbeatInterval".to_string(),
            value: "300".to_string(),
        };

        let response = handler.handle_change_configuration(request).await.unwrap();
        assert_eq!(response.status, ConfigurationStatus::Accepted);

        let config_value = handler.get_config_value("HeartbeatInterval").await;
        assert_eq!(config_value, Some("300".to_string()));
    }

    #[tokio::test]
    async fn test_get_configuration() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = MessageHandler::new(sender);

        // Get specific keys
        let request = GetConfigurationRequest {
            key: Some(vec![
                "HeartbeatInterval".to_string(),
                "NonExistent".to_string(),
            ]),
        };

        let response = handler.handle_get_configuration(request).await.unwrap();
        assert!(response.configuration_keys.is_some());
        assert!(response.unknown_keys.is_some());

        let config_keys = response.configuration_keys.unwrap();
        assert_eq!(config_keys.len(), 1);
        assert_eq!(config_keys[0].key, "HeartbeatInterval");

        let unknown_keys = response.unknown_keys.unwrap();
        assert_eq!(unknown_keys.len(), 1);
        assert_eq!(unknown_keys[0], "NonExistent");
    }

    #[tokio::test]
    async fn test_change_availability() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = MessageHandler::new(sender);

        let request = ChangeAvailabilityRequest {
            connector_id: 1,
            availability_type: AvailabilityType::Operative,
        };

        let response = handler.handle_change_availability(request).await.unwrap();
        assert_eq!(response.status, AvailabilityStatus::Accepted);
    }

    #[tokio::test]
    async fn test_remote_start_transaction() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = MessageHandler::new(sender);

        let request = RemoteStartTransactionRequest {
            connector_id: Some(1),
            id_tag: "user123".to_string(),
            charging_profile: None,
        };

        let response = handler
            .handle_remote_start_transaction(request)
            .await
            .unwrap();
        assert_eq!(response.status, RemoteStartStopStatus::Accepted);
    }

    #[tokio::test]
    async fn test_reset() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = MessageHandler::new(sender);

        let request = ResetRequest {
            reset_type: ResetType::Soft,
        };

        let response = handler.handle_reset(request).await.unwrap();
        assert_eq!(response.status, ResetStatus::Accepted);
    }
}
