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

    /// JSON-Schema validation failure on a CALL/CALLRESULT payload, carrying the
    /// dominant failing schema keyword so it can be mapped to a keyword-granular
    /// CALLERROR code (`type`/`maxLength` → `TypeConstraintViolation`,
    /// `required` → `ProtocolError`, everything else → `FormationViolation`),
    /// mirroring the `e.validator` switch in `ocpp/messages.py::_validate_payload`.
    ///
    /// `action` names the offending message so the CALLERROR-build layer can
    /// surface the triggering-message context in its `details` — the port of the
    /// `ocpp_message` context `_validate_payload` attaches to the raised
    /// `OCPPError` (`tests/test_exceptions.py::test_exception_show_triggered_*`).
    /// It carries only the action name, not the full payload echo the reference's
    /// Python `repr` embeds (see issue #313).
    #[error("Schema validation error ({keyword}): {message}")]
    SchemaViolation {
        keyword: SchemaKeyword,
        message: String,
        action: String,
    },

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

    /// Action is a known/valid OCPP action for the negotiated version, but no
    /// handler is registered for it.
    ///
    /// Ports `NotImplementedError` from
    /// [`ocpp/exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/exceptions.py),
    /// which `_raise_key_error` raises when `v16_Action(action)` /
    /// `v201_Action(action)` succeeds (the action *is* defined by the version)
    /// but dispatch found no handler. Distinct from [`OcppError::NotSupported`],
    /// which is for an action the version does not define at all. Maps to
    /// [`CallErrorCode::NotImplemented`] on the wire.
    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

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

    /// A CSMS-initiated CALL targeted a charge point that is not currently
    /// connected (no live WebSocket session for `cp_id`).
    #[error("Charge point not connected: {cp_id}")]
    CpNotConnected { cp_id: String },
}

/// The dominant failing JSON-Schema keyword from a schema-validation failure.
///
/// Mirrors the `e.validator` value inspected in
/// [`ocpp/messages.py::_validate_payload`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py),
/// which raises a different OCPP exception depending on which keyword failed.
/// `SchemaValidator` collapses the rich `jsonschema` error set down to one of
/// these so the CALLERROR layer can pick a faithful error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaKeyword {
    /// `type` — wrong JSON type (e.g. string where an integer is expected).
    Type,
    /// `maxLength` — a string exceeds its maximum length.
    MaxLength,
    /// `additionalProperties` — an unexpected property is present.
    AdditionalProperties,
    /// `required` — a required property is missing.
    Required,
    /// Any other keyword (`enum`, `minimum`, `multipleOf`, …). Falls into the
    /// default `FormatViolationError` bucket of the Python reference.
    Other,
}

impl SchemaKeyword {
    /// Map the failing keyword to the OCPP CALLERROR code raised for it by
    /// `_validate_payload()`:
    ///
    /// | keyword | code |
    /// |---|---|
    /// | `type`, `maxLength` | [`CallErrorCode::TypeConstraintViolation`] |
    /// | `required` | [`CallErrorCode::ProtocolError`] |
    /// | `additionalProperties`, *(default)* | [`CallErrorCode::FormationViolation`] |
    pub fn call_error_code(self) -> CallErrorCode {
        match self {
            SchemaKeyword::Type | SchemaKeyword::MaxLength => {
                CallErrorCode::TypeConstraintViolation
            }
            SchemaKeyword::Required => CallErrorCode::ProtocolError,
            SchemaKeyword::AdditionalProperties | SchemaKeyword::Other => {
                CallErrorCode::FormationViolation
            }
        }
    }
}

impl std::fmt::Display for SchemaKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SchemaKeyword::Type => "type",
            SchemaKeyword::MaxLength => "maxLength",
            SchemaKeyword::AdditionalProperties => "additionalProperties",
            SchemaKeyword::Required => "required",
            SchemaKeyword::Other => "other",
        };
        f.write_str(name)
    }
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

impl From<&crate::message::CallErrorMessage> for OcppError {
    /// Translate an inbound CALLERROR frame into the error an outstanding
    /// `call()` surfaces to its caller, preserving the wire `error_code`,
    /// `error_description`, and `error_details` verbatim.
    ///
    /// This is the single mapping the transport recv loop applies when it
    /// rejects a pending CALL (`crates/ocpp-transport/src/client.rs`), ported
    /// from `_handle_call_error()` in
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py),
    /// which resolves the matching future with a `CallError` carrying the
    /// error code and description. Taking `&CallErrorMessage` lets the caller
    /// build the error without consuming the frame it still needs for logging.
    fn from(msg: &crate::message::CallErrorMessage) -> Self {
        OcppError::CallError {
            code: msg.error_code.clone(),
            description: msg.error_description.clone(),
            details: msg.error_details.clone(),
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

    /// Payload for Action is syntactically incorrect or not conform the PDU structure for Action.
    ///
    /// This is the strict **OCPP 1.6J** spelling — an errata-sheet typo the spec
    /// declined to fix (`FormationViolationError` in the reference). For OCPP
    /// 2.0.1 use [`CallErrorCode::FormatViolation`] instead.
    FormationViolation,

    /// Payload for Action is syntactically incorrect or not conform the PDU structure for Action.
    ///
    /// This is the corrected **OCPP 2.0.1** spelling (`FormatViolationError` in
    /// the reference). A 2.0.1 peer reporting a malformed payload sends this
    /// code; [`CallErrorCode::FormationViolation`] is the 1.6J counterpart.
    FormatViolation,

    /// Payload is syntactically correct but at least one field contains an invalid value
    PropertyConstraintViolation,

    /// Payload for Action is syntactically correct but at least one of the fields violates occurrence constraints.
    ///
    /// This is the strict **OCPP 1.6J + 2.0.1** spelling (single *"Occurence"* —
    /// an errata-sheet typo, `OccurenceConstraintViolationError` in the
    /// reference). [`CallErrorCode::OccurrenceConstraintViolation`] is the
    /// corrected double-*r* spelling, which the reference marks valid only in
    /// OCPP 2.1.
    OccurenceConstraintViolation,

    /// Payload for Action is syntactically correct but at least one of the fields violates occurrence constraints.
    ///
    /// This is the corrected double-*r* spelling, which the reference documents
    /// as *"Not valid OCPP 2.0.1. Valid in OCPP 2.1"*. For 1.6J / 2.0.1 peers use
    /// [`CallErrorCode::OccurenceConstraintViolation`].
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
            CallErrorCode::FormatViolation => write!(f, "Format violation"),
            CallErrorCode::PropertyConstraintViolation => {
                write!(f, "Property constraint violation")
            }
            CallErrorCode::OccurenceConstraintViolation => {
                write!(f, "Occurence constraint violation")
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
            CallErrorCode::FormatViolation => "FormatViolation",
            CallErrorCode::PropertyConstraintViolation => "PropertyConstraintViolation",
            CallErrorCode::OccurenceConstraintViolation => "OccurenceConstraintViolation",
            CallErrorCode::OccurrenceConstraintViolation => "OccurrenceConstraintViolation",
            CallErrorCode::TypeConstraintViolation => "TypeConstraintViolation",
            CallErrorCode::GenericError => "GenericError",
        }
    }

    /// The spec-canonical **default description** for this error code, ported
    /// verbatim from the reference's per-subclass `default_description`
    /// ([`ocpp/exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/exceptions.py)).
    ///
    /// The reference fills `OCPPError.description` from this string whenever an
    /// error is raised without an explicit description (`OCPPError.__init__`,
    /// pinned by `tests/test_exceptions.py::test_exception_without_error_details`).
    /// This method is the single source of truth for those strings; the CALLERROR
    /// builder (`crates/ocpp-transport/src/server.rs::build_call_error`) and
    /// [`crate::message::CallErrorMessage::from_code`] both source their fallback
    /// text here rather than duplicating literals.
    ///
    /// The strings are reproduced **exactly**, including the reference's own
    /// quirks a faithful port must preserve:
    /// - [`NotImplemented`](CallErrorCode::NotImplemented) reads *"Request Action"*
    ///   (not *"Requested"*) — a typo in the reference, kept for byte-fidelity.
    /// - The two `Format`/`Formation` spellings and the two
    ///   `Occur[r]enceConstraintViolation` spellings each share one description,
    ///   matching the reference (which pairs the strict-1.6J and corrected forms).
    /// - [`TypeConstraintViolation`](CallErrorCode::TypeConstraintViolation)
    ///   embeds the reference's curly quotes (`“somestring”`), not ASCII quotes.
    pub fn default_description(&self) -> &'static str {
        match self {
            CallErrorCode::NotImplemented => {
                "Request Action is recognized but not supported by the receiver"
            }
            CallErrorCode::NotSupported => "Requested Action is not known by receiver",
            CallErrorCode::InternalError => {
                "An internal error occurred and the receiver was not able to process the \
                 requested Action successfully"
            }
            CallErrorCode::ProtocolError => "Payload for Action is incomplete",
            CallErrorCode::SecurityError => {
                "During the processing of Action a security issue occurred preventing receiver \
                 from completing the Action successfully"
            }
            // The reference's `FormatViolationError` and `FormationViolationError`
            // carry the same `default_description` (implicit string concatenation
            // in `ocpp/exceptions.py` collapses to this exact text for both).
            CallErrorCode::FormatViolation | CallErrorCode::FormationViolation => {
                "Payload for Action is syntactically incorrect or structure for Action"
            }
            CallErrorCode::PropertyConstraintViolation => {
                "Payload is syntactically correct but at least one field contains an invalid value"
            }
            // Both `Occurence`/`Occurrence` spellings share one description.
            CallErrorCode::OccurenceConstraintViolation
            | CallErrorCode::OccurrenceConstraintViolation => {
                "Payload for Action is syntactically correct but at least one of the fields \
                 violates occurence constraints"
            }
            CallErrorCode::TypeConstraintViolation => {
                "Payload for Action is syntactically correct but at least one of the fields \
                 violates data type constraints (e.g. \u{201c}somestring\u{201d}: 12)"
            }
            CallErrorCode::GenericError => "Any other error not all other OCPP defined errors",
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

    /// The 12 CALLERROR codes defined by the reference `ocpp/exceptions.py`,
    /// including both spelling variants of the format/occurrence codes.
    const ALL_CALL_ERROR_CODES: [CallErrorCode; 12] = [
        CallErrorCode::NotImplemented,
        CallErrorCode::NotSupported,
        CallErrorCode::InternalError,
        CallErrorCode::ProtocolError,
        CallErrorCode::SecurityError,
        CallErrorCode::FormationViolation,
        CallErrorCode::FormatViolation,
        CallErrorCode::PropertyConstraintViolation,
        CallErrorCode::OccurenceConstraintViolation,
        CallErrorCode::OccurrenceConstraintViolation,
        CallErrorCode::TypeConstraintViolation,
        CallErrorCode::GenericError,
    ];

    #[test]
    fn serde_pascal_case_matches_as_str_for_every_code() {
        // The wire (de)serialization goes through two paths: `RawMessage` uses
        // the hand-written `as_str()`, while a directly-serialized
        // `CallErrorMessage` uses the serde `PascalCase` derive. They must agree
        // byte-for-byte, otherwise the same code round-trips differently
        // depending on which path built the frame.
        for code in ALL_CALL_ERROR_CODES {
            let via_serde = serde_json::to_string(&code).unwrap();
            assert_eq!(
                via_serde,
                format!("\"{}\"", code.as_str()),
                "serde spelling disagrees with as_str() for {code:?}"
            );
            let back: CallErrorCode = serde_json::from_str(&via_serde).unwrap();
            assert_eq!(back, code);
        }
    }

    #[test]
    fn format_and_occurence_spelling_variants_are_distinct() {
        // The 1.6J errata spellings and their corrected counterparts are
        // separate codes on the wire and must not collapse into one another.
        assert_ne!(
            CallErrorCode::FormationViolation.as_str(),
            CallErrorCode::FormatViolation.as_str()
        );
        assert_ne!(
            CallErrorCode::OccurenceConstraintViolation.as_str(),
            CallErrorCode::OccurrenceConstraintViolation.as_str()
        );
        // Exact wire spellings, pinned so a rename can't silently change them.
        assert_eq!(CallErrorCode::FormatViolation.as_str(), "FormatViolation");
        assert_eq!(
            CallErrorCode::OccurenceConstraintViolation.as_str(),
            "OccurenceConstraintViolation"
        );
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
    fn not_implemented_is_distinct_from_not_supported() {
        // The two unrouted-action variants (`_raise_key_error` split) must not
        // be equal and must render distinct messages.
        let ni = OcppError::NotImplemented {
            feature: "Reset".to_string(),
        };
        let ns = OcppError::NotSupported {
            feature: "Reset".to_string(),
        };
        assert_ne!(ni, ns);
        assert_eq!(ni.to_string(), "Feature not implemented: Reset");
        assert_eq!(ns.to_string(), "Feature not supported: Reset");
        assert_eq!(ni.clone(), ni);
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

    #[test]
    fn from_call_error_message_preserves_wire_fields() {
        use crate::message::CallErrorMessage;

        let frame = CallErrorMessage::new(
            "unique-42".to_string(),
            CallErrorCode::InternalError,
            "central system exploded".to_string(),
            Some(serde_json::json!({"trace": "abc"})),
        );

        // The recv loop translates by reference; the frame is still usable after.
        let err = OcppError::from(&frame);

        assert_eq!(
            err,
            OcppError::CallError {
                code: CallErrorCode::InternalError,
                description: "central system exploded".to_string(),
                details: serde_json::json!({"trace": "abc"}),
            }
        );
        // The `unique_id` is intentionally *not* part of the surfaced error —
        // it is the correlation key the caller already holds, matching the
        // reference's `_handle_call_error`.
        assert_eq!(frame.unique_id, "unique-42");
    }
}
