//! Message handlers for OCPP CSMS

use ocpp_messages::{v16j::*, Message};
use ocpp_types::{OcppError, OcppResult};
use tracing::{debug, info, warn};

/// Message handler registry
pub struct MessageHandlerRegistry {
    /// Core profile handlers
    core_handlers: CoreHandlers,
    /// Firmware management handlers
    firmware_handlers: FirmwareHandlers,
    /// Smart charging handlers
    #[allow(dead_code)]
    smart_charging_handlers: SmartChargingHandlers,
}

impl MessageHandlerRegistry {
    /// Create new handler registry
    pub fn new() -> Self {
        Self {
            core_handlers: CoreHandlers::new(),
            firmware_handlers: FirmwareHandlers::new(),
            smart_charging_handlers: SmartChargingHandlers::new(),
        }
    }

    /// Route message to appropriate handler
    pub async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        match &message {
            Message::Call(call) => {
                debug!("Handling call message: {}", call.action);
                match call.action.as_str() {
                    // Core Profile
                    "Authorize" => self.core_handlers.handle_authorize(call).await,
                    "BootNotification" => self.core_handlers.handle_boot_notification(call).await,
                    "Heartbeat" => self.core_handlers.handle_heartbeat(call).await,
                    "MeterValues" => self.core_handlers.handle_meter_values(call).await,
                    "StartTransaction" => self.core_handlers.handle_start_transaction(call).await,
                    "StatusNotification" => {
                        self.core_handlers.handle_status_notification(call).await
                    }
                    "StopTransaction" => self.core_handlers.handle_stop_transaction(call).await,

                    // Firmware Management
                    "DiagnosticsStatusNotification" => {
                        self.firmware_handlers
                            .handle_diagnostics_status_notification(call)
                            .await
                    }
                    "FirmwareStatusNotification" => {
                        self.firmware_handlers
                            .handle_firmware_status_notification(call)
                            .await
                    }

                    // Data Transfer (generic)
                    "DataTransfer" => self.core_handlers.handle_data_transfer(call).await,

                    _ => {
                        warn!("Unhandled message action: {}", call.action);
                        Err(OcppError::NotSupported {
                            feature: call.action.clone(),
                        })
                    }
                }
            }
            _ => {
                warn!("Only Call messages can be handled by handlers");
                Err(OcppError::ProtocolViolation {
                    message: "Only Call messages are handled".to_string(),
                })
            }
        }
    }
}

impl Default for MessageHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Core profile message handlers
pub struct CoreHandlers {
    // TODO: Add dependencies like database, auth service, etc.
}

impl CoreHandlers {
    pub fn new() -> Self {
        Self {}
    }

    /// Handle Authorize request
    pub async fn handle_authorize(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: AuthorizeRequest = call.payload_as()?;
        info!("Authorizing ID tag: {}", request.id_tag);

        // TODO: Implement actual authorization logic
        let response = AuthorizeResponse {
            id_tag_info: ocpp_types::common::IdTagInfo {
                status: ocpp_types::common::AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: None,
            },
        };

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle BootNotification request
    pub async fn handle_boot_notification(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: BootNotificationRequest = call.payload_as()?;
        info!(
            "Boot notification from {}/{}",
            request.charge_point_vendor, request.charge_point_model
        );

        // TODO: Register charge point in database
        let response = BootNotificationResponse {
            current_time: chrono::Utc::now(),
            interval: 60, // Heartbeat interval in seconds
            status: RegistrationStatus::Accepted,
        };

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle Heartbeat request
    pub async fn handle_heartbeat(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let _request: HeartbeatRequest = call.payload_as()?;
        debug!("Received heartbeat");

        let response = HeartbeatResponse {
            current_time: chrono::Utc::now(),
        };

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle MeterValues request
    pub async fn handle_meter_values(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: MeterValuesRequest = call.payload_as()?;
        debug!(
            "Received meter values for connector {} (tx: {:?})",
            request.connector_id, request.transaction_id
        );

        // TODO: Store meter values in database
        let response = MeterValuesResponse {};

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle StartTransaction request
    pub async fn handle_start_transaction(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: StartTransactionRequest = call.payload_as()?;
        info!(
            "Starting transaction on connector {} with ID tag {}",
            request.connector_id, request.id_tag
        );

        // TODO: Create transaction in database
        let transaction_id = 12345; // Generate unique transaction ID

        let response = StartTransactionResponse {
            id_tag_info: ocpp_types::common::IdTagInfo {
                status: ocpp_types::common::AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: None,
            },
            transaction_id,
        };

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle StatusNotification request
    pub async fn handle_status_notification(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: StatusNotificationRequest = call.payload_as()?;
        debug!(
            "Status notification for connector {}: {:?} ({})",
            request.connector_id, request.status, request.error_code
        );

        // TODO: Update connector status in database
        let response = StatusNotificationResponse {};

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle StopTransaction request
    pub async fn handle_stop_transaction(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: StopTransactionRequest = call.payload_as()?;
        info!(
            "Stopping transaction {} at {} Wh",
            request.transaction_id, request.meter_stop
        );

        // TODO: Update transaction in database
        let response = StopTransactionResponse {
            id_tag_info: None, // Optional
        };

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle DataTransfer request
    pub async fn handle_data_transfer(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: DataTransferRequest = call.payload_as()?;
        debug!(
            "Data transfer from vendor {} (message: {:?})",
            request.vendor_id, request.message_id
        );

        // TODO: Handle vendor-specific data transfer
        let response = DataTransferResponse {
            status: ocpp_types::v16j::DataTransferStatus::Accepted,
            data: None,
        };

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }
}

impl Default for CoreHandlers {
    fn default() -> Self {
        Self::new()
    }
}

/// Firmware management profile handlers
pub struct FirmwareHandlers {}

impl FirmwareHandlers {
    pub fn new() -> Self {
        Self {}
    }

    /// Handle DiagnosticsStatusNotification
    pub async fn handle_diagnostics_status_notification(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: DiagnosticsStatusNotificationRequest = call.payload_as()?;
        info!("Diagnostics status notification: {:?}", request.status);

        // TODO: Update diagnostics status in database
        let response = DiagnosticsStatusNotificationResponse {};

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }

    /// Handle FirmwareStatusNotification
    pub async fn handle_firmware_status_notification(
        &self,
        call: &ocpp_messages::CallMessage,
    ) -> OcppResult<Option<Message>> {
        let request: FirmwareStatusNotificationRequest = call.payload_as()?;
        info!("Firmware status notification: {:?}", request.status);

        // TODO: Update firmware status in database
        let response = FirmwareStatusNotificationResponse {};

        let result_message = Message::call_result(call.unique_id.clone(), response)?;
        Ok(Some(result_message))
    }
}

impl Default for FirmwareHandlers {
    fn default() -> Self {
        Self::new()
    }
}

/// Smart charging profile handlers
pub struct SmartChargingHandlers {}

impl SmartChargingHandlers {
    pub fn new() -> Self {
        Self {}
    }

    // TODO: Add smart charging handlers when needed
}

impl Default for SmartChargingHandlers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::CallMessage;
    use serde_json::json;

    #[tokio::test]
    async fn test_handler_registry_creation() {
        let _registry = MessageHandlerRegistry::new();
    }

    #[tokio::test]
    async fn test_heartbeat_handler() {
        let handlers = CoreHandlers::new();
        let call = CallMessage::new("Heartbeat".to_string(), json!({})).unwrap();

        let result = handlers.handle_heartbeat(&call).await;
        assert!(result.is_ok());

        if let Ok(Some(Message::CallResult(response))) = result {
            let heartbeat_response: HeartbeatResponse = response.payload_as().unwrap();
            // Just verify we get a current timestamp
            assert!(
                (chrono::Utc::now() - heartbeat_response.current_time)
                    .num_seconds()
                    .abs()
                    < 5
            );
        } else {
            panic!("Expected CallResult message");
        }
    }

    #[tokio::test]
    async fn test_authorize_handler() {
        let handlers = CoreHandlers::new();
        let request = AuthorizeRequest {
            id_tag: "TEST123".to_string(),
        };
        let call = CallMessage::new("Authorize".to_string(), request).unwrap();

        let result = handlers.handle_authorize(&call).await;
        assert!(result.is_ok());

        if let Ok(Some(Message::CallResult(response))) = result {
            let auth_response: AuthorizeResponse = response.payload_as().unwrap();
            assert_eq!(
                auth_response.id_tag_info.status,
                ocpp_types::common::AuthorizationStatus::Accepted
            );
        } else {
            panic!("Expected CallResult message");
        }
    }

    #[tokio::test]
    async fn test_unsupported_message() {
        let registry = MessageHandlerRegistry::new();
        let call = CallMessage::new("UnsupportedAction".to_string(), json!({})).unwrap();
        let message = Message::Call(call);

        let result = registry.handle_message(message).await;
        assert!(result.is_err());

        if let Err(OcppError::NotSupported { feature }) = result {
            assert_eq!(feature, "UnsupportedAction");
        } else {
            panic!("Expected NotSupported error");
        }
    }
}
