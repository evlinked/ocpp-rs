//! # OCPP Transport
//!
//! This crate provides the transport layer implementation for OCPP protocol over WebSocket.
//! It handles WebSocket connections, message framing, and connection management for both
//! client (Charge Point) and server (Central System) sides.

pub mod client;
pub mod dispatch_handler;
pub mod error;
pub mod pending;
pub mod server;
pub mod websocket;

pub use client::WebSocketClient;
pub use dispatch_handler::DispatchHandler;
pub use error::*;
pub use pending::PendingCallMap;

use ocpp_messages::Message;
use ocpp_types::OcppResult;
use std::time::Duration;
use uuid::Uuid;

/// Transport configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Keep-alive interval
    pub keep_alive_interval: Duration,
    /// Maximum number of pending messages
    pub message_buffer_size: usize,
    /// Enable message compression
    pub enable_compression: bool,
    /// WebSocket sub-protocols
    pub sub_protocols: Vec<String>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            max_message_size: 65536, // 64KB
            connection_timeout: Duration::from_secs(30),
            keep_alive_interval: Duration::from_secs(60),
            message_buffer_size: 1000,
            enable_compression: false,
            sub_protocols: vec!["ocpp1.6".to_string()],
        }
    }
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection is being established
    Connecting,
    /// Connection is established and ready
    Connected,
    /// Connection is being closed
    Closing,
    /// Connection is closed
    Closed,
    /// Connection failed
    Failed,
}

/// Transport event
#[derive(Debug, Clone)]
pub enum TransportEvent {
    /// Connection established
    Connected {
        connection_id: Uuid,
        remote_addr: String,
    },
    /// Connection closed
    Disconnected { connection_id: Uuid, reason: String },
    /// Message received
    MessageReceived {
        connection_id: Uuid,
        message: Message,
    },
    /// Message sent
    MessageSent {
        connection_id: Uuid,
        message_id: String,
    },
    /// Error occurred
    Error {
        connection_id: Option<Uuid>,
        error: TransportError,
    },
}

/// Transport trait for sending and receiving messages
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    /// Send a message
    async fn send_message(&self, message: Message) -> OcppResult<()>;

    /// Close the connection
    async fn close(&self) -> OcppResult<()>;

    /// Get connection state
    fn state(&self) -> ConnectionState;

    /// Get connection ID
    fn connection_id(&self) -> Uuid;
}

/// Message handler trait
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle incoming message
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>>;

    /// Handle transport event
    async fn handle_event(&self, event: TransportEvent);
}

/// Connection information
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Connection ID
    pub id: Uuid,
    /// Remote address
    pub remote_addr: String,
    /// Local address
    pub local_addr: String,
    /// Connected timestamp
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,
    /// Sub-protocol used
    pub sub_protocol: Option<String>,
}

impl ConnectionInfo {
    /// Create new connection info
    pub fn new(remote_addr: String, local_addr: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            remote_addr,
            local_addr,
            connected_at: now,
            last_activity: now,
            sub_protocol: None,
        }
    }

    /// Update last activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = chrono::Utc::now();
    }

    /// Check if connection is idle
    pub fn is_idle(&self, timeout: Duration) -> bool {
        let now = chrono::Utc::now();
        let idle_time = now.signed_duration_since(self.last_activity);
        idle_time.num_seconds() as u64 > timeout.as_secs()
    }
}

/// Message with metadata
#[derive(Debug, Clone)]
pub struct MessageEnvelope {
    /// The message
    pub message: Message,
    /// Message ID for tracking
    pub id: String,
    /// Timestamp when message was created
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Priority (higher number = higher priority)
    pub priority: u8,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Current retry count
    pub retry_count: u32,
}

impl MessageEnvelope {
    /// Create new message envelope
    pub fn new(message: Message) -> Self {
        Self {
            id: message.unique_id().to_string(),
            message,
            timestamp: chrono::Utc::now(),
            priority: 0,
            max_retries: 3,
            retry_count: 0,
        }
    }

    /// Create with priority
    pub fn with_priority(message: Message, priority: u8) -> Self {
        let mut envelope = Self::new(message);
        envelope.priority = priority;
        envelope
    }

    /// Increment retry count
    pub fn retry(&mut self) -> bool {
        if self.retry_count < self.max_retries {
            self.retry_count += 1;
            true
        } else {
            false
        }
    }

    /// Check if message has expired
    pub fn is_expired(&self, timeout: Duration) -> bool {
        let now = chrono::Utc::now();
        let age = now.signed_duration_since(self.timestamp);
        age.num_seconds() as u64 > timeout.as_secs()
    }
}

/// Transport statistics
#[derive(Debug, Default, Clone)]
pub struct TransportStats {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Connection count
    pub connections: u64,
    /// Error count
    pub errors: u64,
    /// Last reset timestamp
    pub last_reset: Option<chrono::DateTime<chrono::Utc>>,
}

impl TransportStats {
    /// Reset all counters
    pub fn reset(&mut self) {
        *self = Self {
            last_reset: Some(chrono::Utc::now()),
            ..Default::default()
        };
    }

    /// Record message sent
    pub fn record_sent(&mut self, bytes: usize) {
        self.messages_sent += 1;
        self.bytes_sent += bytes as u64;
    }

    /// Record message received
    pub fn record_received(&mut self, bytes: usize) {
        self.messages_received += 1;
        self.bytes_received += bytes as u64;
    }

    /// Record connection
    pub fn record_connection(&mut self) {
        self.connections += 1;
    }

    /// Record error
    pub fn record_error(&mut self) {
        self.errors += 1;
    }
}

/// Utility functions
pub mod utils {
    use super::*;

    /// Generate connection ID
    pub fn generate_connection_id() -> Uuid {
        Uuid::new_v4()
    }

    /// Validate WebSocket subprotocol
    pub fn validate_subprotocol(protocol: &str) -> bool {
        matches!(protocol, "ocpp1.6" | "ocpp2.0" | "ocpp2.0.1")
    }

    /// Extract charge point ID from WebSocket path
    pub fn extract_charge_point_id(path: &str) -> Option<&str> {
        // Expected format: /ocpp/{charge_point_id}
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 3 && parts[1] == "ocpp" {
            Some(parts[2])
        } else {
            None
        }
    }

    /// Create WebSocket URL for charge point
    pub fn create_websocket_url(base_url: &str, charge_point_id: &str) -> String {
        format!(
            "{}/ocpp/{}",
            base_url.trim_end_matches('/'),
            charge_point_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.max_message_size, 65536);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert!(config.sub_protocols.contains(&"ocpp1.6".to_string()));
    }

    #[test]
    fn test_connection_info() {
        let mut info = ConnectionInfo::new(
            "192.168.1.100:12345".to_string(),
            "0.0.0.0:8080".to_string(),
        );

        assert!(!info.id.is_nil());
        assert_eq!(info.remote_addr, "192.168.1.100:12345");

        // Test activity update
        let old_activity = info.last_activity;
        std::thread::sleep(Duration::from_millis(1));
        info.update_activity();
        assert!(info.last_activity > old_activity);

        // Test idle check
        assert!(!info.is_idle(Duration::from_secs(1)));
    }

    #[test]
    fn test_message_envelope() {
        let message = ocpp_messages::Message::Call(
            ocpp_messages::CallMessage::new("Test".to_string(), serde_json::json!({})).unwrap(),
        );

        let mut envelope = MessageEnvelope::new(message.clone());
        assert_eq!(envelope.message.unique_id(), message.unique_id());
        assert_eq!(envelope.priority, 0);
        assert_eq!(envelope.retry_count, 0);

        // Test retry
        assert!(envelope.retry());
        assert_eq!(envelope.retry_count, 1);

        // Test priority envelope
        let priority_envelope = MessageEnvelope::with_priority(message, 5);
        assert_eq!(priority_envelope.priority, 5);
    }

    #[test]
    fn test_transport_stats() {
        let mut stats = TransportStats::default();

        stats.record_sent(100);
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.bytes_sent, 100);

        stats.record_received(200);
        assert_eq!(stats.messages_received, 1);
        assert_eq!(stats.bytes_received, 200);

        stats.record_connection();
        assert_eq!(stats.connections, 1);

        stats.record_error();
        assert_eq!(stats.errors, 1);

        stats.reset();
        assert_eq!(stats.messages_sent, 0);
        assert!(stats.last_reset.is_some());
    }

    #[test]
    fn test_utils() {
        assert!(utils::validate_subprotocol("ocpp1.6"));
        assert!(utils::validate_subprotocol("ocpp2.0"));
        assert!(!utils::validate_subprotocol("invalid"));

        assert_eq!(utils::extract_charge_point_id("/ocpp/CP001"), Some("CP001"));
        assert_eq!(utils::extract_charge_point_id("/invalid/path"), None);

        assert_eq!(
            utils::create_websocket_url("ws://localhost:8080", "CP001"),
            "ws://localhost:8080/ocpp/CP001"
        );
        assert_eq!(
            utils::create_websocket_url("ws://localhost:8080/", "CP001"),
            "ws://localhost:8080/ocpp/CP001"
        );
    }
}
