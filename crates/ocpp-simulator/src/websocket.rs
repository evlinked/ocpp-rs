//! # WebSocket Module
//!
//! This module provides WebSocket utilities and handlers for the OCPP simulator,
//! including client connection management and message handling.

use crate::error::SimulatorError;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Disconnected
    Disconnected,
    /// Connecting
    Connecting,
    /// Connected and ready
    Connected,
    /// Connection lost, attempting reconnect
    Reconnecting,
    /// Connection failed
    Failed,
}

/// WebSocket message types for simulator communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMessage {
    /// Ping message
    Ping {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Pong response
    Pong {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Event message
    Event(serde_json::Value),
    /// Command message
    Command {
        id: String,
        command: String,
        parameters: Option<serde_json::Value>,
    },
    /// Response message
    Response {
        id: String,
        success: bool,
        data: Option<serde_json::Value>,
        error: Option<String>,
    },
    /// Status update
    Status {
        connector_id: Option<u32>,
        status: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Error message
    Error {
        code: String,
        message: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

/// WebSocket connection configuration
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Message timeout
    pub message_timeout: Duration,
    /// Ping interval
    pub ping_interval: Duration,
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Reconnection delay
    pub reconnect_delay: Duration,
    /// Maximum message size
    pub max_message_size: usize,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            message_timeout: Duration::from_secs(10),
            ping_interval: Duration::from_secs(30),
            max_reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(5),
            max_message_size: 65536, // 64KB
        }
    }
}

/// WebSocket connection statistics
#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Connection attempts
    pub connection_attempts: u32,
    /// Successful connections
    pub successful_connections: u32,
    /// Connection errors
    pub connection_errors: u32,
    /// Last connection time
    pub last_connection_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Last error
    pub last_error: Option<String>,
}

impl ConnectionStats {
    /// Record a sent message
    pub fn record_sent(&mut self, size: usize) {
        self.messages_sent += 1;
        self.bytes_sent += size as u64;
    }

    /// Record a received message
    pub fn record_received(&mut self, size: usize) {
        self.messages_received += 1;
        self.bytes_received += size as u64;
    }

    /// Record a connection attempt
    pub fn record_connection_attempt(&mut self) {
        self.connection_attempts += 1;
    }

    /// Record a successful connection
    pub fn record_successful_connection(&mut self) {
        self.successful_connections += 1;
        self.last_connection_time = Some(chrono::Utc::now());
    }

    /// Record a connection error
    pub fn record_error(&mut self, error: String) {
        self.connection_errors += 1;
        self.last_error = Some(error);
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.connection_attempts == 0 {
            0.0
        } else {
            self.successful_connections as f64 / self.connection_attempts as f64
        }
    }
}

/// WebSocket client wrapper
pub struct WebSocketClient {
    /// Connection URL
    url: String,
    /// Configuration
    config: WebSocketConfig,
    /// Current connection state
    state: ConnectionState,
    /// Connection statistics
    stats: ConnectionStats,
}

impl WebSocketClient {
    /// Create new WebSocket client
    pub fn new(url: String, config: WebSocketConfig) -> Self {
        Self {
            url,
            config,
            state: ConnectionState::Disconnected,
            stats: ConnectionStats::default(),
        }
    }

    /// Connect to WebSocket server
    pub async fn connect(&mut self) -> Result<(), SimulatorError> {
        self.state = ConnectionState::Connecting;
        self.stats.record_connection_attempt();

        info!("Connecting to WebSocket: {}", self.url);

        // In a real implementation, this would establish the actual WebSocket connection
        // For now, we simulate a successful connection
        match timeout(self.config.connect_timeout, self.simulate_connection()).await {
            Ok(Ok(())) => {
                self.state = ConnectionState::Connected;
                self.stats.record_successful_connection();
                info!("WebSocket connected successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                self.state = ConnectionState::Failed;
                self.stats.record_error(e.to_string());
                error!("WebSocket connection failed: {}", e);
                Err(e)
            }
            Err(_) => {
                let error = SimulatorError::timeout(
                    "WebSocket connect",
                    self.config.connect_timeout.as_millis() as u64,
                );
                self.state = ConnectionState::Failed;
                self.stats.record_error(error.to_string());
                error!("WebSocket connection timed out");
                Err(error)
            }
        }
    }

    /// Disconnect from WebSocket server
    pub async fn disconnect(&mut self) -> Result<(), SimulatorError> {
        info!("Disconnecting from WebSocket");
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    /// Send message
    pub async fn send_message(&mut self, message: WebSocketMessage) -> Result<(), SimulatorError> {
        if self.state != ConnectionState::Connected {
            return Err(SimulatorError::invalid_state(
                "send_message",
                format!("{:?}", self.state),
            ));
        }

        let json = serde_json::to_string(&message)
            .map_err(|e| SimulatorError::serialization(e.to_string()))?;

        if json.len() > self.config.max_message_size {
            return Err(SimulatorError::validation(
                "message_size",
                format!(
                    "Message size {} exceeds maximum {}",
                    json.len(),
                    self.config.max_message_size
                ),
            ));
        }

        // Simulate sending message
        self.stats.record_sent(json.len());
        debug!("Sent WebSocket message: {} bytes", json.len());

        Ok(())
    }

    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Get connection statistics
    pub fn stats(&self) -> &ConnectionStats {
        &self.stats
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Reconnect with retry logic
    pub async fn reconnect(&mut self) -> Result<(), SimulatorError> {
        self.state = ConnectionState::Reconnecting;

        for attempt in 1..=self.config.max_reconnect_attempts {
            info!(
                "Reconnection attempt {} of {}",
                attempt, self.config.max_reconnect_attempts
            );

            match self.connect().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!("Reconnection attempt {} failed: {}", attempt, e);
                    if attempt < self.config.max_reconnect_attempts {
                        tokio::time::sleep(self.config.reconnect_delay).await;
                    }
                }
            }
        }

        self.state = ConnectionState::Failed;
        Err(SimulatorError::connection(
            "Max reconnection attempts exceeded",
        ))
    }

    // Private helper methods

    async fn simulate_connection(&self) -> Result<(), SimulatorError> {
        // Simulate connection delay
        tokio::time::sleep(Duration::from_millis(100)).await;

        // For simulation purposes, randomly succeed or fail
        use rand::Rng;
        let mut rng = rand::thread_rng();

        if rng.gen_bool(0.9) {
            // 90% success rate
            Ok(())
        } else {
            Err(SimulatorError::connection("Simulated connection failure"))
        }
    }
}

/// WebSocket message handler trait
#[async_trait::async_trait]
pub trait WebSocketMessageHandler: Send + Sync {
    /// Handle incoming message
    async fn handle_message(
        &self,
        message: WebSocketMessage,
    ) -> Result<Option<WebSocketMessage>, SimulatorError>;

    /// Handle connection opened
    async fn on_connect(&self) {}

    /// Handle connection closed
    async fn on_disconnect(&self, _reason: Option<String>) {}

    /// Handle connection error
    async fn on_error(&self, _error: SimulatorError) {}
}

/// Utility functions for WebSocket operations
pub mod utils {
    use super::*;

    /// Create ping message
    pub fn create_ping() -> WebSocketMessage {
        WebSocketMessage::Ping {
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create pong message
    pub fn create_pong() -> WebSocketMessage {
        WebSocketMessage::Pong {
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create command message
    pub fn create_command(
        id: String,
        command: String,
        parameters: Option<serde_json::Value>,
    ) -> WebSocketMessage {
        WebSocketMessage::Command {
            id,
            command,
            parameters,
        }
    }

    /// Create response message
    pub fn create_response(
        id: String,
        success: bool,
        data: Option<serde_json::Value>,
        error: Option<String>,
    ) -> WebSocketMessage {
        WebSocketMessage::Response {
            id,
            success,
            data,
            error,
        }
    }

    /// Create error message
    pub fn create_error(code: String, message: String) -> WebSocketMessage {
        WebSocketMessage::Error {
            code,
            message,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Validate message size
    pub fn validate_message_size(
        message: &WebSocketMessage,
        max_size: usize,
    ) -> Result<(), SimulatorError> {
        let json = serde_json::to_string(message)
            .map_err(|e| SimulatorError::serialization(e.to_string()))?;

        if json.len() > max_size {
            return Err(SimulatorError::validation(
                "message_size",
                format!("Message size {} exceeds maximum {}", json.len(), max_size),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_stats() {
        let mut stats = ConnectionStats::default();

        stats.record_sent(100);
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.bytes_sent, 100);

        stats.record_received(200);
        assert_eq!(stats.messages_received, 1);
        assert_eq!(stats.bytes_received, 200);

        stats.record_connection_attempt();
        stats.record_successful_connection();
        assert_eq!(stats.connection_attempts, 1);
        assert_eq!(stats.successful_connections, 1);
        assert_eq!(stats.success_rate(), 1.0);

        stats.record_error("Test error".to_string());
        assert_eq!(stats.connection_errors, 1);
        assert_eq!(stats.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_websocket_message_serialization() {
        let ping = WebSocketMessage::Ping {
            timestamp: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&ping).unwrap();
        let deserialized: WebSocketMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            WebSocketMessage::Ping { .. } => {}
            _ => panic!("Expected Ping message"),
        }
    }

    #[test]
    fn test_websocket_config_default() {
        let config = WebSocketConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.max_message_size, 65536);
    }

    #[tokio::test]
    async fn test_websocket_client_creation() {
        let config = WebSocketConfig::default();
        let client = WebSocketClient::new("ws://localhost:8080".to_string(), config);

        assert_eq!(client.state(), ConnectionState::Disconnected);
        assert!(!client.is_connected());
        assert_eq!(client.stats().messages_sent, 0);
    }

    #[test]
    fn test_utils() {
        let ping = utils::create_ping();
        match ping {
            WebSocketMessage::Ping { .. } => {}
            _ => panic!("Expected Ping message"),
        }

        let command = utils::create_command(
            "test-id".to_string(),
            "test-command".to_string(),
            Some(serde_json::json!({"param": "value"})),
        );
        match command {
            WebSocketMessage::Command { id, command, .. } => {
                assert_eq!(id, "test-id");
                assert_eq!(command, "test-command");
            }
            _ => panic!("Expected Command message"),
        }

        let error = utils::create_error("TEST_ERROR".to_string(), "Test error message".to_string());
        match error {
            WebSocketMessage::Error { code, message, .. } => {
                assert_eq!(code, "TEST_ERROR");
                assert_eq!(message, "Test error message");
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_message_validation() {
        let message = WebSocketMessage::Ping {
            timestamp: chrono::Utc::now(),
        };

        // Should pass with reasonable size limit
        let result = utils::validate_message_size(&message, 1000);
        assert!(result.is_ok());

        // Should fail with very small size limit
        let result = utils::validate_message_size(&message, 10);
        assert!(result.is_err());
    }
}
