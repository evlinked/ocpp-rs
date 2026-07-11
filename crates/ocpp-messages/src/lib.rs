//! # OCPP Messages
//!
//! This crate provides message definitions and serialization for OCPP protocol messages.
//! It includes all message types for OCPP 1.6J and provides a foundation for OCPP 2.0.1.

pub mod dispatcher;
pub mod schema_validation;
pub mod serialization;
pub mod v16j;
pub mod v201;
pub mod validation;

pub use dispatcher::ActionDispatcher;
pub use ocpp_types::{CallErrorMessage, CallMessage, CallResultMessage, Message, MessageType};
use ocpp_types::{OcppError, OcppResult};
pub use schema_validation::SchemaValidator;

/// Re-export commonly used types
pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};
pub use uuid::Uuid;

/// Action trait for OCPP messages
pub trait OcppAction: Serialize + for<'de> Deserialize<'de> + Send + Sync {
    /// The action name as defined in the OCPP specification
    const ACTION_NAME: &'static str;
    /// The corresponding response type
    type Response: OcppAction;

    /// Validate the message content
    fn validate(&self) -> OcppResult<()> {
        Ok(())
    }
}

/// Response trait for OCPP response messages
pub trait OcppResponse: OcppAction {}

/// Message handler trait for processing OCPP messages
#[async_trait::async_trait]
pub trait MessageHandler<T: OcppAction> {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Handle an incoming message and return a response
    async fn handle(&self, message: T) -> Result<T::Response, Self::Error>;
}

/// Message router for dispatching messages to appropriate handlers
pub struct MessageRouter {
    handlers: std::collections::HashMap<String, Box<dyn std::any::Any + Send + Sync>>,
}

impl MessageRouter {
    /// Create a new message router
    pub fn new() -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
        }
    }

    /// Register a handler for a specific message type
    pub fn register_handler<T, H>(&mut self, handler: H)
    where
        T: OcppAction + 'static,
        H: MessageHandler<T> + Send + Sync + 'static,
    {
        self.handlers
            .insert(T::ACTION_NAME.to_string(), Box::new(handler));
    }

    /// Route a message to the appropriate handler
    pub async fn route(&self, message: &Message) -> OcppResult<Message> {
        match message {
            Message::Call(call) => {
                tracing::debug!("Routing call message: {}", call.action);
                // TODO: Implement actual routing logic
                Err(OcppError::NotSupported {
                    feature: format!("Action: {}", call.action),
                })
            }
            _ => Err(OcppError::ProtocolViolation {
                message: "Only Call messages can be routed".to_string(),
            }),
        }
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Utilities for working with OCPP messages
pub mod utils {
    use super::*;

    /// Generate a unique message ID
    pub fn generate_message_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Create a Call message from an action, minting a fresh `unique_id`
    /// (UUIDv4). This is the default, auto-generate branch of the reference's
    /// `ChargePoint.call()` (mobilityhouse/ocpp `ocpp/charge_point.py`) — use
    /// [`create_call_with_id`] when the caller already owns the id.
    pub fn create_call<T: OcppAction>(action: T) -> OcppResult<CallMessage> {
        CallMessage::new(T::ACTION_NAME.to_string(), action)
    }

    /// Create a Call message from an action, using a **caller-supplied**
    /// `unique_id` verbatim.
    ///
    /// Mirrors the caller-supplied branch of the reference's
    /// `ChargePoint.call(payload, unique_id=…)` (`ocpp/charge_point.py`): the
    /// caller can correlate the CALLRESULT it will later receive against an id it
    /// already owns, instead of having a fresh UUID forced on it. The
    /// auto-generate [`create_call`] remains the default when no id is supplied.
    pub fn create_call_with_id<T: OcppAction>(
        unique_id: String,
        action: T,
    ) -> OcppResult<CallMessage> {
        CallMessage::with_id(unique_id, T::ACTION_NAME.to_string(), action)
    }

    /// Create a CallResult message from a response
    pub fn create_call_result<T: OcppResponse>(
        unique_id: String,
        response: T,
    ) -> OcppResult<CallResultMessage> {
        CallResultMessage::new(unique_id, response)
    }

    /// Extract action payload from a Call message
    pub fn extract_payload<T: OcppAction>(call: &CallMessage) -> OcppResult<T> {
        if call.action != T::ACTION_NAME {
            return Err(OcppError::ProtocolViolation {
                message: format!(
                    "Expected action '{}', got '{}'",
                    T::ACTION_NAME,
                    call.action
                ),
            });
        }
        call.payload_as()
    }

    /// Get current timestamp in OCPP format
    pub fn current_timestamp() -> DateTime<Utc> {
        Utc::now()
    }

    /// Format timestamp for OCPP messages
    pub fn format_timestamp(timestamp: DateTime<Utc>) -> String {
        timestamp.to_rfc3339()
    }

    /// Parse OCPP timestamp string
    pub fn parse_timestamp(timestamp_str: &str) -> OcppResult<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(timestamp_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| OcppError::ValidationError {
                message: format!("Invalid timestamp format: {}", e),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Test action for testing purposes
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestAction {
        pub test_field: String,
    }

    impl OcppAction for TestAction {
        const ACTION_NAME: &'static str = "TestAction";
        type Response = TestResponse;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestResponse {
        pub success: bool,
    }

    impl OcppAction for TestResponse {
        const ACTION_NAME: &'static str = "TestResponse";
        type Response = TestResponse; // Self-referencing for test
    }

    impl OcppResponse for TestResponse {}

    #[test]
    fn test_message_router_creation() {
        let router = MessageRouter::new();
        assert_eq!(router.handlers.len(), 0);
    }

    #[test]
    fn test_generate_message_id() {
        let id1 = utils::generate_message_id();
        let id2 = utils::generate_message_id();

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
    }

    #[test]
    fn test_create_call() {
        let action = TestAction {
            test_field: "test_value".to_string(),
        };

        let call = utils::create_call(action.clone()).unwrap();

        assert_eq!(call.action, "TestAction");
        assert!(!call.unique_id.is_empty());

        let extracted: TestAction = utils::extract_payload(&call).unwrap();
        assert_eq!(extracted, action);
    }

    #[test]
    fn test_create_call_with_id_uses_id_verbatim() {
        let action = TestAction {
            test_field: "test_value".to_string(),
        };

        let call = utils::create_call_with_id("12345".to_string(), action.clone()).unwrap();

        // The caller-supplied id is used exactly as given.
        assert_eq!(call.unique_id, "12345");
        assert_eq!(call.action, "TestAction");

        let extracted: TestAction = utils::extract_payload(&call).unwrap();
        assert_eq!(extracted, action);
    }

    #[test]
    fn test_create_call_default_id_is_fresh() {
        // The auto-generate path stays the default: each CALL gets a distinct id.
        let a = TestAction {
            test_field: "a".to_string(),
        };
        let call1 = utils::create_call(a.clone()).unwrap();
        let call2 = utils::create_call(a).unwrap();

        assert!(!call1.unique_id.is_empty());
        assert_ne!(call1.unique_id, call2.unique_id);
    }

    #[test]
    fn test_create_call_result() {
        let response = TestResponse { success: true };
        let unique_id = "test-id".to_string();

        let result = utils::create_call_result(unique_id.clone(), response.clone()).unwrap();

        assert_eq!(result.unique_id, unique_id);

        let extracted: TestResponse = result.payload_as().unwrap();
        assert_eq!(extracted, response);
    }

    #[test]
    fn test_extract_payload_wrong_action() {
        let call = CallMessage::new("WrongAction".to_string(), json!({})).unwrap();
        let result: OcppResult<TestAction> = utils::extract_payload(&call);

        assert!(result.is_err());
        match result.unwrap_err() {
            OcppError::ProtocolViolation { message } => {
                assert!(message.contains("Expected action 'TestAction'"));
            }
            _ => panic!("Expected ProtocolViolation error"),
        }
    }

    #[test]
    fn test_timestamp_utilities() {
        let now = utils::current_timestamp();
        let formatted = utils::format_timestamp(now);
        let parsed = utils::parse_timestamp(&formatted).unwrap();

        // Allow for small differences due to precision
        let diff = (now.timestamp_millis() - parsed.timestamp_millis()).abs();
        assert!(diff < 1000); // Less than 1 second difference
    }

    #[test]
    fn test_parse_invalid_timestamp() {
        let result = utils::parse_timestamp("invalid-timestamp");
        assert!(result.is_err());

        match result.unwrap_err() {
            OcppError::ValidationError { message } => {
                assert!(message.contains("Invalid timestamp format"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[tokio::test]
    async fn test_message_router_unsupported_action() {
        let router = MessageRouter::new();
        let call = CallMessage::new("UnsupportedAction".to_string(), json!({})).unwrap();
        let message = Message::Call(call);

        let result = router.route(&message).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            OcppError::NotSupported { feature } => {
                assert!(feature.contains("UnsupportedAction"));
            }
            _ => panic!("Expected NotSupported error"),
        }
    }
}
