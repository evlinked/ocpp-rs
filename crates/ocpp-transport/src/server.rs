//! WebSocket server implementation for OCPP Central System Management System (CSMS)

use crate::{
    error::TransportResult, ConnectionInfo, ConnectionState, MessageHandler, TransportConfig,
    TransportEvent,
};
use dashmap::DashMap;
use ocpp_messages::Message;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// WebSocket server for OCPP Central System
pub struct OcppServer {
    /// Server configuration
    config: TransportConfig,
    /// Active connections
    connections: Arc<DashMap<Uuid, ConnectionInfo>>,
    /// Event sender
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    /// Server state
    state: ConnectionState,
}

impl OcppServer {
    /// Create a new OCPP server
    pub fn new(config: TransportConfig) -> (Self, mpsc::UnboundedReceiver<TransportEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let server = Self {
            config,
            connections: Arc::new(DashMap::new()),
            event_tx,
            state: ConnectionState::Closed,
        };
        (server, event_rx)
    }

    /// Start the server on the specified address
    pub async fn start(&mut self, bind_addr: &str) -> TransportResult<()> {
        info!("Starting OCPP server on {}", bind_addr);
        self.state = ConnectionState::Connecting;

        // TODO: Implement actual server startup logic
        // This would include:
        // - Creating TCP listener
        // - Setting up WebSocket upgrade handling
        // - Connection management
        // - Message routing

        self.state = ConnectionState::Connected;
        Ok(())
    }

    /// Stop the server
    pub async fn stop(&mut self) -> TransportResult<()> {
        info!("Stopping OCPP server");
        self.state = ConnectionState::Closing;

        // Close all active connections
        for connection in self.connections.iter() {
            debug!("Closing connection {}", connection.id);
            // TODO: Send close frame to client
        }

        self.connections.clear();
        self.state = ConnectionState::Closed;
        Ok(())
    }

    /// Get connection count
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Get connection info by ID
    pub fn get_connection(&self, connection_id: &Uuid) -> Option<ConnectionInfo> {
        self.connections.get(connection_id).map(|c| c.clone())
    }

    /// Get all connections
    pub fn get_all_connections(&self) -> Vec<ConnectionInfo> {
        self.connections.iter().map(|c| c.clone()).collect()
    }

    /// Send message to specific connection
    pub async fn send_to_connection(
        &self,
        connection_id: &Uuid,
        message: Message,
    ) -> TransportResult<()> {
        debug!(
            "Sending message to connection {}: {}",
            connection_id,
            message.unique_id()
        );

        // TODO: Implement message sending to specific connection

        Ok(())
    }

    /// Broadcast message to all connections
    pub async fn broadcast_message(&self, message: Message) -> TransportResult<Vec<Uuid>> {
        debug!("Broadcasting message: {}", message.unique_id());

        let mut failed_connections = Vec::new();

        for connection in self.connections.iter() {
            if let Err(e) = self
                .send_to_connection(connection.key(), message.clone())
                .await
            {
                error!(
                    "Failed to send message to connection {}: {}",
                    connection.key(),
                    e
                );
                failed_connections.push(*connection.key());
            }
        }

        Ok(failed_connections)
    }

    #[allow(dead_code)]
    async fn handle_new_connection(&self, connection_info: ConnectionInfo) -> TransportResult<()> {
        info!(
            "New connection from {}: {}",
            connection_info.remote_addr, connection_info.id
        );

        self.connections
            .insert(connection_info.id, connection_info.clone());

        // Send connection event
        let event = TransportEvent::Connected {
            connection_id: connection_info.id,
            remote_addr: connection_info.remote_addr.clone(),
        };

        if let Err(e) = self.event_tx.send(event) {
            error!("Failed to send connection event: {}", e);
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn handle_connection_closed(
        &self,
        connection_id: Uuid,
        reason: String,
    ) -> TransportResult<()> {
        info!("Connection closed: {} ({})", connection_id, reason);

        self.connections.remove(&connection_id);

        // Send disconnection event
        let event = TransportEvent::Disconnected {
            connection_id,
            reason,
        };

        if let Err(e) = self.event_tx.send(event) {
            error!("Failed to send disconnection event: {}", e);
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn handle_message(&self, connection_id: Uuid, message: Message) -> TransportResult<()> {
        debug!(
            "Received message from {}: {}",
            connection_id,
            message.unique_id()
        );

        // Update connection activity
        if let Some(mut connection) = self.connections.get_mut(&connection_id) {
            connection.update_activity();
        }

        // Send message event
        let event = TransportEvent::MessageReceived {
            connection_id,
            message,
        };

        if let Err(e) = self.event_tx.send(event) {
            error!("Failed to send message event: {}", e);
        }

        Ok(())
    }

    /// Clean up idle connections
    pub async fn cleanup_idle_connections(&self) -> TransportResult<usize> {
        let timeout = self.config.keep_alive_interval * 2; // Allow 2x heartbeat interval
        let mut removed_count = 0;

        let idle_connections: Vec<Uuid> = self
            .connections
            .iter()
            .filter_map(|entry| {
                if entry.is_idle(timeout) {
                    Some(*entry.key())
                } else {
                    None
                }
            })
            .collect();

        for connection_id in idle_connections {
            info!("Removing idle connection: {}", connection_id);
            self.connections.remove(&connection_id);
            removed_count += 1;

            // Send disconnection event
            let event = TransportEvent::Disconnected {
                connection_id,
                reason: "Idle timeout".to_string(),
            };

            if let Err(e) = self.event_tx.send(event) {
                error!("Failed to send disconnection event: {}", e);
            }
        }

        if removed_count > 0 {
            info!("Cleaned up {} idle connections", removed_count);
        }

        Ok(removed_count)
    }

    /// Get server statistics
    pub fn get_stats(&self) -> ServerStats {
        ServerStats {
            active_connections: self.connection_count(),
            total_connections: 0, // TODO: Track total connections
            uptime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default(),
        }
    }
}

/// Server statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    /// Current active connections
    pub active_connections: usize,
    /// Total connections since startup
    pub total_connections: u64,
    /// Server uptime
    pub uptime: std::time::Duration,
}

/// Connection manager for handling individual WebSocket connections
pub struct ConnectionManager {
    #[allow(dead_code)]
    connection_id: Uuid,
    #[allow(dead_code)]
    connection_info: ConnectionInfo,
    /// Message handler
    message_handler: Option<Arc<dyn MessageHandler>>,
}

impl ConnectionManager {
    /// Create new connection manager
    pub fn new(connection_info: ConnectionInfo) -> Self {
        Self {
            connection_id: connection_info.id,
            connection_info,
            message_handler: None,
        }
    }

    /// Set message handler
    pub fn set_message_handler(&mut self, handler: Arc<dyn MessageHandler>) {
        self.message_handler = Some(handler);
    }

    /// Handle WebSocket frame
    pub async fn handle_frame(&mut self, frame: tungstenite::Message) -> TransportResult<()> {
        match frame {
            tungstenite::Message::Text(text) => {
                // Parse OCPP message
                // TODO: Implement message parsing and handling
                debug!("Received text frame: {}", text);
            }
            tungstenite::Message::Binary(data) => {
                warn!("Received unexpected binary frame of {} bytes", data.len());
            }
            tungstenite::Message::Ping(_data) => {
                debug!("Received ping frame");
                // TODO: Send pong response
            }
            tungstenite::Message::Pong(_) => {
                debug!("Received pong frame");
            }
            tungstenite::Message::Close(frame) => {
                info!("Received close frame: {:?}", frame);
                // TODO: Handle connection close
            }
            _ => {
                warn!("Received unsupported frame type");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let config = TransportConfig::default();
        let (server, _rx) = OcppServer::new(config);
        assert_eq!(server.connection_count(), 0);
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let config = TransportConfig::default();
        let (mut server, _rx) = OcppServer::new(config);

        // This will not actually bind to a port in tests
        let _result = server.start("127.0.0.1:0").await;
        let _result = server.stop().await;
    }

    #[test]
    fn test_connection_manager() {
        let connection_info = ConnectionInfo::new(
            "192.168.1.100:12345".to_string(),
            "0.0.0.0:8080".to_string(),
        );
        let manager = ConnectionManager::new(connection_info.clone());
        assert_eq!(manager.connection_id, connection_info.id);
    }

    #[tokio::test]
    async fn test_cleanup_idle_connections() {
        let config = TransportConfig {
            keep_alive_interval: std::time::Duration::from_millis(1),
            ..Default::default()
        };
        let (server, _rx) = OcppServer::new(config);

        // Add a connection that will be idle
        let mut connection_info = ConnectionInfo::new(
            "192.168.1.100:12345".to_string(),
            "0.0.0.0:8080".to_string(),
        );
        // Make the connection appear idle by setting an old timestamp
        connection_info.last_activity = chrono::Utc::now() - chrono::Duration::seconds(10);

        server
            .connections
            .insert(connection_info.id, connection_info);

        let removed = server.cleanup_idle_connections().await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(server.connection_count(), 0);
    }
}
