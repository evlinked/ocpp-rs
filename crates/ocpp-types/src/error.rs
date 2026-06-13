//! Error types for OCPP operations

use thiserror::Error;

/// Main error type for OCPP operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum OcppError {
    /// Invalid message type identifier
    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),

    /// Invalid connector ID (must be > 0)
    #[error("Invalid connector ID: {0} (must be > 0)")]
    InvalidConnectorId(u32),

    /// JSON serialization/deserialization error
    #[error("JSON error: {message}")]
    Json { message: String },

    /// Protocol violation
    #[error("Protocol violation: {message}")]
    ProtocolViolation { message: String },

    /// Message validation error
    #[error("Message validation error: {message}")]
    ValidationError { message: String },

    /// Transport error
    #[error("Transport error: {message}")]
    Transport { message: String },

    /// Timeout error
    #[error("Operation timed out: {operation}")]
    Timeout { operation: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// Authentication error
    #[error("Authentication failed: {reason}")]
    Authentication { reason: String },

    /// Authorization error
    #[error("Authorization failed: {reason}")]
    Authorization { reason: String },

    /// Internal error
    #[error("Internal error: {message}")]
    Internal { message: String },

    /// Feature not supported
    #[error("Feature not supported: {feature}")]
    NotSupported { feature: String },

    /// Resource not found
    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    /// Resource already exists
    #[error("Resource already exists: {resource}")]
    AlreadyExists { resource: String },

    /// Invalid state for operation
    #[error("Invalid state for operation: {operation}, current state: {state}")]
    InvalidState { operation: String, state: String },

    /// Rate limit exceeded
    #[error("Rate limit exceeded for {operation}")]
    RateLimitExceeded { operation: String },

    /// Database error
    #[error("Database error: {message}")]
    Database { message: String },

    /// A CALL received a CALLERROR response from the remote endpoint.
    /// Carries the structured error code, human-readable description, and
    /// optional detail payload from the CALLERROR frame.
    #[error("CALLERROR [{code}]: {description}")]
    CallError {
        code: CallErrorCode,
        description: String,
        details: serde_json::Value,
    },

    /// BootNotification rejected by the CSMS after all retry attempts exhausted.
    #[error("Boot rejected by central system after {attempts} attempt(s)")]
    BootRejected { attempts: u32 },
}

impl From<serde_json::Error> for OcppError {
    fn from(err: serde_json::Error) -> Self {
        OcppError::Json {
            message: err.to_string(),
        }
    }
}

impl From<anyhow::Error> for OcppError {
    fn from(err: anyhow::Error) -> Self {
        OcppError::Internal {
            message: err.to_string(),
        }
    }
}

/// OCPP Call Error codes as defined in the specification
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CallErrorCode {
    /// Requested Action is not known by receiver
    NotImplemented,

    /// Requested Action is recognized but not supported by the receiver
    NotSupported,

    /// An internal error occurred and the receiver was not able to process the requested Action successfully
    InternalError,

    /// Payload for Action is incomplete
    ProtocolError,

    /// During the processing of Action a security issue occurred preventing receiver from completing the Action successfully
    SecurityError,

    /// Payload for Action is syntactically incorrect or not conform the PDU structure for Action
    FormationViolation,

    /// Payload is syntactically correct but at least one field contains an invalid value
    PropertyConstraintViolation,

    /// Payload for Action is syntactically correct but at least one of the fields violates occurrence constraints
    OccurrenceConstraintViolation,

    /// Payload for Action is syntactically correct but at least one of the fields violates data type constraints (e.g. "somestring": 12)
    TypeConstraintViolation,

    /// Any other error not covered by the above
    GenericError,
}

impl std::error::Error for CallErrorCode {}

impl std::fmt::Display for CallErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallErrorCode::NotImplemented => write!(f, "Not implemented"),
            CallErrorCode::NotSupported => write!(f, "Not supported"),
            CallErrorCode::InternalError => write!(f, "Internal error"),
            CallErrorCode::ProtocolError => write!(f, "Protocol error"),
            CallErrorCode::SecurityError => write!(f, "Security error"),
            CallErrorCode::FormationViolation => write!(f, "Formation violation"),
            CallErrorCode::PropertyConstraintViolation => {
                write!(f, "Property constraint violation")
            }
            CallErrorCode::OccurrenceConstraintViolation => {
                write!(f, "Occurrence constraint violation")
            }
            CallErrorCode::TypeConstraintViolation => write!(f, "Type constraint violation"),
            CallErrorCode::GenericError => write!(f, "Generic error"),
        }
    }
}

impl CallErrorCode {
    /// Convert to string as defined in OCPP spec
    pub fn as_str(&self) -> &'static str {
        match self {
            CallErrorCode::NotImplemented => "NotImplemented",
            CallErrorCode::NotSupported => "NotSupported",
            CallErrorCode::InternalError => "InternalError",
            CallErrorCode::ProtocolError => "ProtocolError",
            CallErrorCode::SecurityError => "SecurityError",
            CallErrorCode::FormationViolation => "FormationViolation",
            CallErrorCode::PropertyConstraintViolation => "PropertyConstraintViolation",
            CallErrorCode::OccurrenceConstraintViolation => "OccurrenceConstraintViolation",
            CallErrorCode::TypeConstraintViolation => "TypeConstraintViolation",
            CallErrorCode::GenericError => "GenericError",
        }
    }
}

/// Result type alias for OCPP operations
pub type OcppResult<T> = Result<T, OcppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_error_code_display() {
        assert_eq!(CallErrorCode::NotImplemented.to_string(), "Not implemented");
        assert_eq!(CallErrorCode::InternalError.to_string(), "Internal error");
        assert_eq!(CallErrorCode::ProtocolError.to_string(), "Protocol error");
    }

    #[test]
    fn test_call_error_code_serialization() {
        let error = CallErrorCode::NotImplemented;
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(json, "\"NotImplemented\"");

        let deserialized: CallErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, CallErrorCode::NotImplemented);
    }

    #[test]
    fn test_ocpp_error_from_serde_json() {
        let json_error = serde_json::from_str::<i32>("invalid json").unwrap_err();
        let ocpp_error = OcppError::from(json_error);

        match ocpp_error {
            OcppError::Json { message } => assert!(!message.is_empty()),
            _ => panic!("Expected Json error"),
        }
    }

    #[test]
    fn test_ocpp_error_display() {
        let error = OcppError::InvalidConnectorId(0);
        assert_eq!(error.to_string(), "Invalid connector ID: 0 (must be > 0)");

        let error = OcppError::ValidationError {
            message: "test validation".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Message validation error: test validation"
        );
    }

    #[test]
    fn test_ocpp_error_clone() {
        let error = OcppError::NotSupported {
            feature: "TestFeature".to_string(),
        };
        let cloned = error.clone();
        assert_eq!(error, cloned);
    }

    #[test]
    fn call_error_variant_display() {
        let err = OcppError::CallError {
            code: CallErrorCode::NotImplemented,
            description: "Action not implemented".to_string(),
            details: serde_json::Value::Null,
        };
        let msg = err.to_string();
        assert!(msg.contains("NotImplemented") || msg.contains("Not implemented"));
        assert!(msg.contains("Action not implemented"));
    }

    #[test]
    fn call_error_variant_clone_and_eq() {
        let err = OcppError::CallError {
            code: CallErrorCode::InternalError,
            description: "boom".to_string(),
            details: serde_json::json!({"hint": "see logs"}),
        };
        assert_eq!(err.clone(), err);
    }
}
