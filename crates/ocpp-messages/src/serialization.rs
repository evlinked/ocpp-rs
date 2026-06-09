//! Message serialization and deserialization utilities for OCPP messages
//!
//! This module provides utilities for converting OCPP messages between different
//! formats and handling protocol-specific serialization requirements.

use crate::OcppAction;
use ocpp_types::{Message, OcppError, OcppResult, RawMessage};

use serde_json::Value;
use std::collections::HashMap;

/// Serializer for OCPP messages with version-specific handling
pub struct MessageSerializer {
    /// Version-specific serialization rules
    version_rules: HashMap<String, SerializationRules>,
}

/// Serialization rules for a specific OCPP version
#[derive(Debug, Clone)]
pub struct SerializationRules {
    /// Whether to include null fields in serialization
    pub include_null_fields: bool,
    /// Whether to use strict field ordering
    pub strict_field_ordering: bool,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Date/time format to use
    pub datetime_format: DateTimeFormat,
}

/// Date/time format options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DateTimeFormat {
    /// RFC 3339 format (ISO 8601)
    Rfc3339,
    /// Custom format string
    Custom(&'static str),
}

impl Default for SerializationRules {
    fn default() -> Self {
        Self {
            include_null_fields: false,
            strict_field_ordering: false,
            max_message_size: 65536, // 64KB
            datetime_format: DateTimeFormat::Rfc3339,
        }
    }
}

impl MessageSerializer {
    /// Create a new message serializer
    pub fn new() -> Self {
        let mut serializer = Self {
            version_rules: HashMap::new(),
        };

        // Add default rules for OCPP 1.6J
        serializer.version_rules.insert(
            "1.6J".to_string(),
            SerializationRules {
                include_null_fields: false,
                strict_field_ordering: false,
                max_message_size: 65536,
                datetime_format: DateTimeFormat::Rfc3339,
            },
        );

        // Add default rules for OCPP 2.0.1
        serializer.version_rules.insert(
            "2.0.1".to_string(),
            SerializationRules {
                include_null_fields: false,
                strict_field_ordering: true,
                max_message_size: 131072, // 128KB
                datetime_format: DateTimeFormat::Rfc3339,
            },
        );

        serializer
    }

    /// Add or update serialization rules for a version
    pub fn add_version_rules(&mut self, version: String, rules: SerializationRules) {
        self.version_rules.insert(version, rules);
    }

    /// Serialize a message to JSON string
    pub fn serialize_message(&self, message: &Message, version: &str) -> OcppResult<String> {
        let default_rules = SerializationRules::default();
        let rules = self.version_rules.get(version).unwrap_or(&default_rules);

        // Convert to raw message format first
        let raw: RawMessage = message.clone().into();

        // Serialize to JSON
        let json_str = serde_json::to_string(&raw).map_err(|e| OcppError::Json {
            message: e.to_string(),
        })?;

        // Check message size
        if json_str.len() > rules.max_message_size {
            return Err(OcppError::ValidationError {
                message: format!(
                    "Message size {} exceeds maximum allowed size {}",
                    json_str.len(),
                    rules.max_message_size
                ),
            });
        }

        Ok(json_str)
    }

    /// Deserialize a JSON string to a message
    pub fn deserialize_message(&self, json_str: &str, version: &str) -> OcppResult<Message> {
        let default_rules = SerializationRules::default();
        let rules = self.version_rules.get(version).unwrap_or(&default_rules);

        // Check message size
        if json_str.len() > rules.max_message_size {
            return Err(OcppError::ValidationError {
                message: format!(
                    "Message size {} exceeds maximum allowed size {}",
                    json_str.len(),
                    rules.max_message_size
                ),
            });
        }

        // Parse JSON
        let raw: RawMessage = serde_json::from_str(json_str).map_err(|e| OcppError::Json {
            message: e.to_string(),
        })?;

        // Convert to typed message
        raw.into_message()
    }

    /// Serialize an action payload to JSON value
    pub fn serialize_payload<T: OcppAction>(
        &self,
        payload: &T,
        version: &str,
    ) -> OcppResult<Value> {
        let _default_rules = SerializationRules::default();
        let _rules = self.version_rules.get(version).unwrap_or(&_default_rules);

        serde_json::to_value(payload).map_err(|e| OcppError::Json {
            message: e.to_string(),
        })
    }

    /// Deserialize a JSON value to an action payload
    pub fn deserialize_payload<T: OcppAction>(
        &self,
        value: &Value,
        version: &str,
    ) -> OcppResult<T> {
        let _default_rules = SerializationRules::default();
        let _rules = self.version_rules.get(version).unwrap_or(&_default_rules);

        serde_json::from_value(value.clone()).map_err(|e| OcppError::Json {
            message: e.to_string(),
        })
    }
}

impl Default for MessageSerializer {
    fn default() -> Self {
        Self::new()
    }
}

/// Utilities for working with OCPP message formats
pub mod format {
    use super::*;

    /// Format a timestamp according to OCPP requirements
    pub fn format_timestamp(
        timestamp: chrono::DateTime<chrono::Utc>,
        format: DateTimeFormat,
    ) -> String {
        match format {
            DateTimeFormat::Rfc3339 => timestamp.to_rfc3339(),
            DateTimeFormat::Custom(fmt) => timestamp.format(fmt).to_string(),
        }
    }

    /// Parse a timestamp from string according to OCPP requirements
    pub fn parse_timestamp(timestamp_str: &str) -> OcppResult<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::parse_from_rfc3339(timestamp_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|e| OcppError::ValidationError {
                message: format!("Invalid timestamp format: {}", e),
            })
    }

    /// Validate JSON structure for OCPP message
    pub fn validate_json_structure(json_value: &Value) -> OcppResult<()> {
        // Must be an array
        if !json_value.is_array() {
            return Err(OcppError::ProtocolViolation {
                message: "OCPP message must be a JSON array".to_string(),
            });
        }

        let array = json_value.as_array().unwrap();

        // Must have at least 3 elements
        if array.len() < 3 {
            return Err(OcppError::ProtocolViolation {
                message: "OCPP message array must have at least 3 elements".to_string(),
            });
        }

        // First element must be message type (number)
        if !array[0].is_number() {
            return Err(OcppError::ProtocolViolation {
                message: "First element must be message type number".to_string(),
            });
        }

        let msg_type = array[0].as_u64().unwrap_or(0) as u8;
        if ![2, 3, 4].contains(&msg_type) {
            return Err(OcppError::ProtocolViolation {
                message: format!("Invalid message type: {}", msg_type),
            });
        }

        // Second element must be unique ID (string)
        if !array[1].is_string() {
            return Err(OcppError::ProtocolViolation {
                message: "Second element must be unique ID string".to_string(),
            });
        }

        // Validate based on message type
        match msg_type {
            2 => validate_call_structure(array)?,
            3 => validate_call_result_structure(array)?,
            4 => validate_call_error_structure(array)?,
            _ => unreachable!(),
        }

        Ok(())
    }

    fn validate_call_structure(array: &[Value]) -> OcppResult<()> {
        if array.len() != 4 {
            return Err(OcppError::ProtocolViolation {
                message: "Call message must have exactly 4 elements".to_string(),
            });
        }

        // Third element must be action (string)
        if !array[2].is_string() {
            return Err(OcppError::ProtocolViolation {
                message: "Third element must be action string".to_string(),
            });
        }

        // Fourth element must be payload (object)
        if !array[3].is_object() {
            return Err(OcppError::ProtocolViolation {
                message: "Fourth element must be payload object".to_string(),
            });
        }

        Ok(())
    }

    fn validate_call_result_structure(array: &[Value]) -> OcppResult<()> {
        if array.len() != 3 {
            return Err(OcppError::ProtocolViolation {
                message: "CallResult message must have exactly 3 elements".to_string(),
            });
        }

        // Third element must be payload (object)
        if !array[2].is_object() {
            return Err(OcppError::ProtocolViolation {
                message: "Third element must be payload object".to_string(),
            });
        }

        Ok(())
    }

    fn validate_call_error_structure(array: &[Value]) -> OcppResult<()> {
        if array.len() != 5 {
            return Err(OcppError::ProtocolViolation {
                message: "CallError message must have exactly 5 elements".to_string(),
            });
        }

        // Third element must be error code (string)
        if !array[2].is_string() {
            return Err(OcppError::ProtocolViolation {
                message: "Third element must be error code string".to_string(),
            });
        }

        // Fourth element must be error description (string)
        if !array[3].is_string() {
            return Err(OcppError::ProtocolViolation {
                message: "Fourth element must be error description string".to_string(),
            });
        }

        // Fifth element must be error details (object)
        if !array[4].is_object() {
            return Err(OcppError::ProtocolViolation {
                message: "Fifth element must be error details object".to_string(),
            });
        }

        Ok(())
    }
}

/// Batch serialization utilities for handling multiple messages
pub struct BatchSerializer {
    serializer: MessageSerializer,
}

impl BatchSerializer {
    /// Create a new batch serializer
    pub fn new() -> Self {
        Self {
            serializer: MessageSerializer::new(),
        }
    }

    /// Serialize multiple messages to a JSON array
    pub fn serialize_batch(&self, messages: &[Message], version: &str) -> OcppResult<String> {
        let mut serialized_messages = Vec::new();

        for message in messages {
            let serialized = self.serializer.serialize_message(message, version)?;
            let parsed: Value = serde_json::from_str(&serialized).map_err(|e| OcppError::Json {
                message: e.to_string(),
            })?;
            serialized_messages.push(parsed);
        }

        serde_json::to_string(&serialized_messages).map_err(|e| OcppError::Json {
            message: e.to_string(),
        })
    }

    /// Deserialize a JSON array to multiple messages
    pub fn deserialize_batch(&self, json_str: &str, version: &str) -> OcppResult<Vec<Message>> {
        let array: Vec<Value> = serde_json::from_str(json_str).map_err(|e| OcppError::Json {
            message: e.to_string(),
        })?;

        let mut messages = Vec::new();
        for value in array {
            let json_str = serde_json::to_string(&value).map_err(|e| OcppError::Json {
                message: e.to_string(),
            })?;
            let message = self.serializer.deserialize_message(&json_str, version)?;
            messages.push(message);
        }

        Ok(messages)
    }
}

impl Default for BatchSerializer {
    fn default() -> Self {
        Self::new()
    }
}

/// Pretty printing utilities for debugging
pub mod pretty {
    use super::*;

    /// Pretty print a message for debugging
    pub fn print_message(message: &Message) -> String {
        match serde_json::to_string_pretty(message) {
            Ok(json) => json,
            Err(_) => format!("{:?}", message),
        }
    }

    /// Format message summary for logging
    pub fn format_message_summary(message: &Message) -> String {
        match message {
            Message::Call(call) => {
                format!("CALL [{}] {}", call.unique_id, call.action)
            }
            Message::CallResult(result) => {
                format!("CALL_RESULT [{}]", result.unique_id)
            }
            Message::CallError(error) => {
                format!(
                    "CALL_ERROR [{}] {}: {}",
                    error.unique_id, error.error_code, error.error_description
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::{CallErrorCode, CallErrorMessage, CallMessage, CallResultMessage};
    use serde_json::json;

    #[test]
    fn test_message_serializer_creation() {
        let serializer = MessageSerializer::new();
        assert!(serializer.version_rules.contains_key("1.6J"));
        assert!(serializer.version_rules.contains_key("2.0.1"));
    }

    #[test]
    fn test_serialize_call_message() {
        let serializer = MessageSerializer::new();
        let call = CallMessage::new("Heartbeat".to_string(), json!({})).unwrap();
        let message = Message::Call(call);

        let serialized = serializer.serialize_message(&message, "1.6J").unwrap();
        assert!(serialized.contains("Heartbeat"));
        assert!(serialized.starts_with('['));
        assert!(serialized.ends_with(']'));
    }

    #[test]
    fn test_serialize_call_result_message() {
        let serializer = MessageSerializer::new();
        let result = CallResultMessage::new(
            "test-id".to_string(),
            json!({"currentTime": "2024-01-01T00:00:00Z"}),
        )
        .unwrap();
        let message = Message::CallResult(result);

        let serialized = serializer.serialize_message(&message, "1.6J").unwrap();
        assert!(serialized.contains("test-id"));
        assert!(serialized.contains("currentTime"));
    }

    #[test]
    fn test_serialize_call_error_message() {
        let serializer = MessageSerializer::new();
        let error = CallErrorMessage::new(
            "test-id".to_string(),
            CallErrorCode::InternalError,
            "Test error".to_string(),
            None,
        );
        let message = Message::CallError(error);

        let serialized = serializer.serialize_message(&message, "1.6J").unwrap();
        assert!(serialized.contains("test-id"));
        assert!(serialized.contains("InternalError"));
        assert!(serialized.contains("Test error"));
    }

    #[test]
    fn test_deserialize_message() {
        let serializer = MessageSerializer::new();
        let json_str = r#"[2, "test-id", "Heartbeat", {}]"#;

        let message = serializer.deserialize_message(json_str, "1.6J").unwrap();

        match message {
            Message::Call(call) => {
                assert_eq!(call.unique_id, "test-id");
                assert_eq!(call.action, "Heartbeat");
            }
            _ => panic!("Expected Call message"),
        }
    }

    #[test]
    fn test_message_size_validation() {
        let rules = SerializationRules {
            max_message_size: 10, // Very small limit for testing
            ..Default::default()
        };

        let mut serializer = MessageSerializer::new();
        serializer.add_version_rules("test".to_string(), rules);

        let call = CallMessage::new(
            "VeryLongActionNameThatExceedsLimit".to_string(),
            json!({"veryLongFieldName": "veryLongValue"}),
        )
        .unwrap();
        let message = Message::Call(call);

        let result = serializer.serialize_message(&message, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_serialization() {
        let batch_serializer = BatchSerializer::new();

        let messages = vec![
            Message::Call(CallMessage::new("Action1".to_string(), json!({})).unwrap()),
            Message::Call(CallMessage::new("Action2".to_string(), json!({})).unwrap()),
        ];

        let serialized = batch_serializer.serialize_batch(&messages, "1.6J").unwrap();
        assert!(serialized.starts_with('['));
        assert!(serialized.ends_with(']'));
        assert!(serialized.contains("Action1"));
        assert!(serialized.contains("Action2"));

        let deserialized = batch_serializer
            .deserialize_batch(&serialized, "1.6J")
            .unwrap();
        assert_eq!(deserialized.len(), 2);
    }

    #[test]
    fn test_json_validation() {
        // Valid Call message
        let valid_call = json!([2, "test-id", "Action", {}]);
        assert!(format::validate_json_structure(&valid_call).is_ok());

        // Valid CallResult message
        let valid_result = json!([3, "test-id", {}]);
        assert!(format::validate_json_structure(&valid_result).is_ok());

        // Valid CallError message
        let valid_error = json!([4, "test-id", "InternalError", "Error description", {}]);
        assert!(format::validate_json_structure(&valid_error).is_ok());

        // Invalid: not an array
        let invalid_not_array = json!({"not": "array"});
        assert!(format::validate_json_structure(&invalid_not_array).is_err());

        // Invalid: too few elements
        let invalid_short = json!([2, "test-id"]);
        assert!(format::validate_json_structure(&invalid_short).is_err());

        // Invalid: wrong message type
        let invalid_msg_type = json!([1, "test-id", "Action", {}]);
        assert!(format::validate_json_structure(&invalid_msg_type).is_err());

        // Invalid: unique ID not string
        let invalid_id = json!([2, 123, "Action", {}]);
        assert!(format::validate_json_structure(&invalid_id).is_err());
    }

    #[test]
    fn test_timestamp_formatting() {
        let timestamp = chrono::DateTime::from_timestamp(1640995200, 0).unwrap();

        let rfc3339 = format::format_timestamp(timestamp, DateTimeFormat::Rfc3339);
        assert!(rfc3339.contains("2022-01-01"));

        let parsed = format::parse_timestamp(&rfc3339).unwrap();
        assert_eq!(parsed.timestamp(), timestamp.timestamp());
    }

    #[test]
    fn test_pretty_printing() {
        let call = CallMessage::new("Test".to_string(), json!({})).unwrap();
        let message = Message::Call(call);

        let pretty = pretty::print_message(&message);
        assert!(!pretty.is_empty());

        let summary = pretty::format_message_summary(&message);
        assert!(summary.contains("CALL"));
        assert!(summary.contains("Test"));
    }
}
