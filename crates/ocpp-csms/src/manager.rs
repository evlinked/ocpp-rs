//! Charge point manager module for OCPP CSMS

use crate::CsmsResult;
use dashmap::DashMap;
use ocpp_messages::Message;
use ocpp_transport::TransportEvent;
use ocpp_types::OcppResult;
use std::sync::Arc;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Charge point manager
pub struct ChargePointManager {
    /// Connected charge points
    charge_points: Arc<DashMap<String, ChargePointInfo>>,
    /// Database pool reference
    #[allow(dead_code)]
    database: Arc<crate::database::DatabasePool>,
    /// Metrics registry reference
    #[allow(dead_code)]
    metrics: Arc<crate::metrics::MetricsRegistry>,
}

impl ChargePointManager {
    /// Create new charge point manager
    pub fn new(
        database: Arc<crate::database::DatabasePool>,
        metrics: Arc<crate::metrics::MetricsRegistry>,
    ) -> Self {
        Self {
            charge_points: Arc::new(DashMap::new()),
            database,
            metrics,
        }
    }

    /// Register a charge point
    pub async fn register_charge_point(&self, info: ChargePointInfo) -> CsmsResult<()> {
        info!("Registering charge point: {}", info.charge_point_id);
        self.charge_points
            .insert(info.charge_point_id.clone(), info);
        Ok(())
    }

    /// Unregister a charge point
    pub async fn unregister_charge_point(&self, charge_point_id: &str) -> CsmsResult<()> {
        info!("Unregistering charge point: {}", charge_point_id);
        self.charge_points.remove(charge_point_id);
        Ok(())
    }

    /// Get charge point info
    pub fn get_charge_point(&self, charge_point_id: &str) -> Option<ChargePointInfo> {
        self.charge_points.get(charge_point_id).map(|c| c.clone())
    }

    /// Get all charge points
    pub fn get_all_charge_points(&self) -> Vec<ChargePointInfo> {
        self.charge_points.iter().map(|c| c.clone()).collect()
    }

    /// Get count of active charge points
    pub fn active_count(&self) -> usize {
        self.charge_points.len()
    }

    /// Get total count (including inactive)
    pub fn total_count(&self) -> usize {
        // TODO: Get from database
        self.charge_points.len()
    }

    /// Get active transactions count
    pub fn active_transactions(&self) -> usize {
        // TODO: Query database for active transactions
        0
    }

    /// Check if manager is healthy
    pub fn is_healthy(&self) -> bool {
        // TODO: Implement health checks
        true
    }

    /// Handle transport event
    pub async fn handle_event(&self, event: TransportEvent) {
        match event {
            TransportEvent::Connected {
                connection_id,
                remote_addr,
            } => {
                info!(
                    "Charge point connected: {} ({})",
                    connection_id, remote_addr
                );
                // TODO: Extract charge point ID from connection and register
            }
            TransportEvent::Disconnected {
                connection_id,
                reason,
            } => {
                info!(
                    "Charge point disconnected: {} (reason: {})",
                    connection_id, reason
                );
                // TODO: Update charge point status
            }
            TransportEvent::MessageReceived {
                connection_id,
                message,
            } => {
                debug!(
                    "Message received from {}: {}",
                    connection_id,
                    message.unique_id()
                );
                // Message handling is done elsewhere
            }
            TransportEvent::MessageSent {
                connection_id,
                message_id,
            } => {
                debug!("Message sent to {}: {}", connection_id, message_id);
            }
            TransportEvent::Error {
                connection_id,
                error,
            } => {
                error!(
                    "Transport error for {:?}: {}",
                    connection_id.unwrap_or_default(),
                    error
                );
            }
        }
    }

    /// Handle OCPP message
    pub async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        debug!("Handling message: {}", message.unique_id());

        // TODO: Route message to appropriate handler
        let handlers = crate::handlers::MessageHandlerRegistry::new();
        handlers.handle_message(message).await
    }
}

/// Charge point information
#[derive(Debug, Clone)]
pub struct ChargePointInfo {
    /// Charge point identifier
    pub charge_point_id: String,
    /// Connection ID
    pub connection_id: Option<Uuid>,
    /// Vendor information
    pub vendor: Option<String>,
    /// Model information
    pub model: Option<String>,
    /// Serial number
    pub serial_number: Option<String>,
    /// Firmware version
    pub firmware_version: Option<String>,
    /// Current status
    pub status: ChargePointStatus,
    /// Last heartbeat timestamp
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
    /// Registration timestamp
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

impl ChargePointInfo {
    /// Create new charge point info
    pub fn new(charge_point_id: String) -> Self {
        Self {
            charge_point_id,
            connection_id: None,
            vendor: None,
            model: None,
            serial_number: None,
            firmware_version: None,
            status: ChargePointStatus::Offline,
            last_heartbeat: None,
            registered_at: chrono::Utc::now(),
        }
    }

    /// Update heartbeat timestamp
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Some(chrono::Utc::now());
        if self.status == ChargePointStatus::Offline {
            self.status = ChargePointStatus::Online;
        }
    }

    /// Check if charge point is online
    pub fn is_online(&self) -> bool {
        matches!(self.status, ChargePointStatus::Online)
    }
}

/// Charge point status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargePointStatus {
    /// Charge point is online and responsive
    Online,
    /// Charge point is offline or not responding
    Offline,
    /// Charge point is in error state
    Error,
    /// Charge point is being registered
    Registering,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_charge_point_info_creation() {
        let info = ChargePointInfo::new("CP001".to_string());
        assert_eq!(info.charge_point_id, "CP001");
        assert_eq!(info.status, ChargePointStatus::Offline);
        assert!(!info.is_online());
    }

    #[test]
    fn test_charge_point_info_heartbeat() {
        let mut info = ChargePointInfo::new("CP001".to_string());
        info.update_heartbeat();
        assert!(info.is_online());
        assert!(info.last_heartbeat.is_some());
    }
}
