//! # OCPP Types
//!
//! This crate provides the foundational types and data structures for OCPP protocol implementation.
//! It includes common types, enums, and utilities used across all OCPP versions.

pub mod common;
pub mod error;
pub mod message;
pub mod v16j;
pub mod v201;

pub use error::*;
pub use message::*;

/// Re-export commonly used types
pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};
pub use uuid::Uuid;

/// OCPP protocol version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OcppVersion {
    #[serde(rename = "1.6")]
    V16J,
    #[serde(rename = "2.0.1")]
    V201,
}

impl std::fmt::Display for OcppVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcppVersion::V16J => write!(f, "1.6"),
            OcppVersion::V201 => write!(f, "2.0.1"),
        }
    }
}

/// Message type identifier for OCPP messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MessageType {
    Call = 2,
    CallResult = 3,
    CallError = 4,
}

impl TryFrom<u8> for MessageType {
    type Error = OcppError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            2 => Ok(MessageType::Call),
            3 => Ok(MessageType::CallResult),
            4 => Ok(MessageType::CallError),
            _ => Err(OcppError::InvalidMessageType(value)),
        }
    }
}

impl From<MessageType> for u8 {
    fn from(msg_type: MessageType) -> Self {
        msg_type as u8
    }
}

/// Unique identifier type used throughout OCPP
pub type IdToken = String;

/// Connector identifier (1-based)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectorId(pub u32);

impl ConnectorId {
    /// Create a new connector ID
    pub fn new(id: u32) -> Result<Self, OcppError> {
        if id == 0 {
            Err(OcppError::InvalidConnectorId(id))
        } else {
            Ok(ConnectorId(id))
        }
    }

    /// Get the inner value
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for ConnectorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Transaction identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(pub i32);

impl TransactionId {
    /// Create a new transaction ID
    pub fn new(id: i32) -> Self {
        TransactionId(id)
    }

    /// Get the inner value
    pub fn value(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_conversion() {
        assert_eq!(MessageType::try_from(2).unwrap(), MessageType::Call);
        assert_eq!(MessageType::try_from(3).unwrap(), MessageType::CallResult);
        assert_eq!(MessageType::try_from(4).unwrap(), MessageType::CallError);
        assert!(MessageType::try_from(1).is_err());
        assert!(MessageType::try_from(5).is_err());

        assert_eq!(u8::from(MessageType::Call), 2);
        assert_eq!(u8::from(MessageType::CallResult), 3);
        assert_eq!(u8::from(MessageType::CallError), 4);
    }

    #[test]
    fn test_connector_id() {
        let connector = ConnectorId::new(1).unwrap();
        assert_eq!(connector.value(), 1);
        assert_eq!(connector.to_string(), "1");

        assert!(ConnectorId::new(0).is_err());
    }

    #[test]
    fn test_transaction_id() {
        let tx_id = TransactionId::new(12345);
        assert_eq!(tx_id.value(), 12345);
        assert_eq!(tx_id.to_string(), "12345");
    }

    #[test]
    fn test_ocpp_version_display() {
        assert_eq!(OcppVersion::V16J.to_string(), "1.6");
        assert_eq!(OcppVersion::V201.to_string(), "2.0.1");
    }

    #[test]
    fn test_ocpp_version_serialization() {
        let v16j = OcppVersion::V16J;
        let json = serde_json::to_string(&v16j).unwrap();
        assert_eq!(json, "\"1.6\"");

        let v201 = OcppVersion::V201;
        let json = serde_json::to_string(&v201).unwrap();
        assert_eq!(json, "\"2.0.1\"");
    }
}
