//! Message envelope types for OCPP protocol

use crate::{CallErrorCode, MessageType, OcppError, OcppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// OCPP message envelope that wraps all message types.
///
/// Serializes (via the `untagged` derive) as the inner variant's object, whose
/// `"0"` key carries the [`MessageType`] discriminator (`"CALL"` / `"CALLRESULT"`
/// / `"CALLERROR"`). Deserialization is **hand-written** to dispatch on that
/// discriminator rather than a `serde(untagged)` first-match: the three variants
/// share the `"0".."3"` key prefix, and a `CALLERROR`'s `"2"` is a string error
/// code that also satisfies `CallMessage`'s `"2"` action string — so an untagged
/// decode silently mis-reads every inbound `CALLERROR` as a `Call`, leaving the
/// real CALLERROR uncorrelated to its pending call. Dispatching on `"0"` keeps
/// each frame on its own variant (Issue #321).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Message {
    /// Call message (request)
    Call(CallMessage),
    /// CallResult message (successful response)
    CallResult(CallResultMessage),
    /// CallError message (error response)
    CallError(CallErrorMessage),
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        // Buffer the frame, then dispatch on the `"0"` message-type
        // discriminator. JSON (the only wire format for OCPP here) is
        // self-describing, so buffering into a `Value` is sound.
        let value = serde_json::Value::deserialize(deserializer)?;
        match value.get("0").and_then(|t| t.as_str()) {
            Some("CALL") => serde_json::from_value(value)
                .map(Message::Call)
                .map_err(D::Error::custom),
            Some("CALLRESULT") => serde_json::from_value(value)
                .map(Message::CallResult)
                .map_err(D::Error::custom),
            Some("CALLERROR") => serde_json::from_value(value)
                .map(Message::CallError)
                .map_err(D::Error::custom),
            _ => Err(D::Error::custom(
                "OCPP message: missing or unknown message-type discriminator at key \"0\" \
                 (expected \"CALL\", \"CALLRESULT\", or \"CALLERROR\")",
            )),
        }
    }
}

/// Human-readable frame representation, faithfully adapting the
/// mobilityhouse/ocpp reference's `Call`/`CallResult`/`CallError` `__repr__`
/// (`ocpp/messages.py`) — the `<Kind - field=…, …>` envelope its
/// `test_messages.py` representation tests pin. Delegates to the active
/// variant, so `format!("{message}")` renders whichever frame it holds.
///
/// See the per-variant `Display` impls below for the two faithful-adaptation
/// divergences from the Python `repr`: the payload is rendered as **compact
/// JSON** (Rust payloads are `serde_json::Value`, not Python dicts), and the
/// `CallResult` envelope omits `action` (the OCPP `[3, unique_id, payload]`
/// wire frame carries none — unlike the reference's in-memory `CallResult`
/// attribute).
impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Call(msg) => msg.fmt(f),
            Message::CallResult(msg) => msg.fmt(f),
            Message::CallError(msg) => msg.fmt(f),
        }
    }
}

impl Message {
    /// Get the message type
    pub fn message_type(&self) -> MessageType {
        match self {
            Message::Call(_) => MessageType::Call,
            Message::CallResult(_) => MessageType::CallResult,
            Message::CallError(_) => MessageType::CallError,
        }
    }

    /// Get the unique message ID
    pub fn unique_id(&self) -> &str {
        match self {
            Message::Call(msg) => &msg.unique_id,
            Message::CallResult(msg) => &msg.unique_id,
            Message::CallError(msg) => &msg.unique_id,
        }
    }

    /// Create a new Call message
    pub fn call<T>(action: String, payload: T) -> OcppResult<Self>
    where
        T: Serialize,
    {
        Ok(Message::Call(CallMessage {
            message_type: MessageType::Call,
            unique_id: Uuid::new_v4().to_string(),
            action,
            payload: serde_json::to_value(payload)?,
        }))
    }

    /// Create a CallResult message in response to a Call
    pub fn call_result<T>(unique_id: String, payload: T) -> OcppResult<Self>
    where
        T: Serialize,
    {
        Ok(Message::CallResult(CallResultMessage {
            message_type: MessageType::CallResult,
            unique_id,
            payload: serde_json::to_value(payload)?,
        }))
    }

    /// Create a CallError message in response to a Call
    pub fn call_error(
        unique_id: String,
        error_code: CallErrorCode,
        error_description: String,
        error_details: Option<serde_json::Value>,
    ) -> Self {
        Message::CallError(CallErrorMessage {
            message_type: MessageType::CallError,
            unique_id,
            error_code,
            error_description,
            error_details: error_details.unwrap_or(serde_json::Value::Object(Default::default())),
        })
    }
}

/// OCPP Call message (request)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallMessage {
    /// Message type identifier (always 2 for Call)
    #[serde(rename = "0")]
    pub message_type: MessageType,
    /// Unique message identifier
    #[serde(rename = "1")]
    pub unique_id: String,
    /// Action name (e.g., "Authorize", "StartTransaction")
    #[serde(rename = "2")]
    pub action: String,
    /// Message payload
    #[serde(rename = "3")]
    pub payload: serde_json::Value,
}

impl CallMessage {
    /// Create a new Call message
    pub fn new<T>(action: String, payload: T) -> OcppResult<Self>
    where
        T: Serialize,
    {
        Ok(CallMessage {
            message_type: MessageType::Call,
            unique_id: Uuid::new_v4().to_string(),
            action,
            payload: serde_json::to_value(payload)?,
        })
    }

    /// Create a new Call message with a caller-supplied `unique_id`, used verbatim.
    ///
    /// Unlike [`CallMessage::new`] — which mints a fresh UUIDv4 — this preserves
    /// the caller's id exactly as given, so the caller can correlate the
    /// CALLRESULT it will later receive against an id it already owns (e.g. a job
    /// id from its own queue). Mirrors the caller-supplied branch of the
    /// mobilityhouse/ocpp reference's `ChargePoint.call(unique_id=…)`
    /// (`ocpp/charge_point.py`), where `unique_id = unique_id if unique_id is not
    /// None else str(self._unique_id_generator())`.
    ///
    /// The id is used exactly as passed — no normalization or truncation. The
    /// schema-level `maxLength: 36` bound on message ids is enforced at the
    /// wire/framing (schema-validation) layer, as elsewhere, not here.
    pub fn with_id<T>(unique_id: String, action: String, payload: T) -> OcppResult<Self>
    where
        T: Serialize,
    {
        Ok(CallMessage {
            message_type: MessageType::Call,
            unique_id,
            action,
            payload: serde_json::to_value(payload)?,
        })
    }

    /// Extract the payload as a specific type
    pub fn payload_as<T>(&self) -> OcppResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        Ok(serde_json::from_value(self.payload.clone())?)
    }
}

/// Human-readable CALL frame, adapting the reference's
/// `Call.__repr__` — `<Call - unique_id=…, action=…, payload=…>`
/// ([`ocpp/messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py),
/// pinned by `test_messages.py::test_call_representation`).
///
/// Faithful-adaptation divergence: the payload is a [`serde_json::Value`], so
/// it renders as **compact JSON** (`{}`, `{"idTag":"T1"}`) via `Value`'s own
/// `Display`, not the reference's Python dict-repr (`{'idTag': 'T1'}`).
impl std::fmt::Display for CallMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<Call - unique_id={}, action={}, payload={}>",
            self.unique_id, self.action, self.payload
        )
    }
}

/// OCPP CallResult message (successful response)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallResultMessage {
    /// Message type identifier (always 3 for CallResult)
    #[serde(rename = "0")]
    pub message_type: MessageType,
    /// Unique message identifier (same as corresponding Call)
    #[serde(rename = "1")]
    pub unique_id: String,
    /// Response payload
    #[serde(rename = "2")]
    pub payload: serde_json::Value,
}

impl CallResultMessage {
    /// Create a new CallResult message
    pub fn new<T>(unique_id: String, payload: T) -> OcppResult<Self>
    where
        T: Serialize,
    {
        Ok(CallResultMessage {
            message_type: MessageType::CallResult,
            unique_id,
            payload: serde_json::to_value(payload)?,
        })
    }

    /// Extract the payload as a specific type
    pub fn payload_as<T>(&self) -> OcppResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        Ok(serde_json::from_value(self.payload.clone())?)
    }
}

/// Human-readable CALLRESULT frame, adapting the reference's
/// `CallResult.__repr__`
/// ([`ocpp/messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py),
/// pinned by `test_messages.py::test_call_result_representation`).
///
/// Two faithful-adaptation divergences from the Python `repr`:
/// - **No `action`.** The OCPP CALLRESULT wire frame is `[3, unique_id,
///   payload]` and carries no action; [`CallResultMessage`] models exactly that
///   and has no `action` field. The reference keeps `action` only as an
///   in-memory attribute on its `CallResult` object (set by the caller that
///   knows which CALL it answers), so rendering it here would mean inventing
///   data the envelope does not hold. The envelope therefore renders
///   `<CallResult - unique_id=…, payload=…>`.
/// - **Compact JSON payload**, as for [`CallMessage`] above (`{"status":
///   "Accepted"}` → `{"status":"Accepted"}`), not Python dict-repr.
impl std::fmt::Display for CallResultMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<CallResult - unique_id={}, payload={}>",
            self.unique_id, self.payload
        )
    }
}

/// OCPP CallError message (error response)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallErrorMessage {
    /// Message type identifier (always 4 for CallError)
    #[serde(rename = "0")]
    pub message_type: MessageType,
    /// Unique message identifier (same as corresponding Call)
    #[serde(rename = "1")]
    pub unique_id: String,
    /// Error code
    #[serde(rename = "2")]
    pub error_code: CallErrorCode,
    /// Human-readable error description
    #[serde(rename = "3")]
    pub error_description: String,
    /// Additional error details
    #[serde(rename = "4")]
    pub error_details: serde_json::Value,
}

impl CallErrorMessage {
    /// Create a new CallError message
    pub fn new(
        unique_id: String,
        error_code: CallErrorCode,
        error_description: String,
        error_details: Option<serde_json::Value>,
    ) -> Self {
        CallErrorMessage {
            message_type: MessageType::CallError,
            unique_id,
            error_code,
            error_description,
            error_details: error_details.unwrap_or(serde_json::Value::Object(Default::default())),
        }
    }
}

/// Human-readable CALLERROR frame, adapting the reference's
/// `CallError.__repr__` — `<CallError - unique_id=…, error_code=…,
/// error_description=…, error_details=…>`
/// ([`ocpp/messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py),
/// pinned by `test_messages.py::test_call_error_representation`).
///
/// The `error_code` renders as its **wire spelling** (`GenericError`) via
/// [`CallErrorCode::as_str`], matching the reference — not the human `Display`
/// (`Generic error`) used in log/error text elsewhere. `error_details` renders
/// as compact JSON, the same divergence as the other frames.
impl std::fmt::Display for CallErrorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<CallError - unique_id={}, error_code={}, error_description={}, error_details={}>",
            self.unique_id,
            self.error_code.as_str(),
            self.error_description,
            self.error_details
        )
    }
}

/// Raw OCPP message as received over the wire (array format)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawMessage {
    /// Call message: [2, "unique_id", "action", payload]
    Call(u8, String, String, serde_json::Value),
    /// CallResult message: [3, "unique_id", payload]
    CallResult(u8, String, serde_json::Value),
    /// CallError message: [4, "unique_id", "error_code", "error_description", error_details]
    CallError(u8, String, String, String, serde_json::Value),
}

impl RawMessage {
    /// Convert raw message to typed message
    pub fn into_message(self) -> OcppResult<Message> {
        match self {
            RawMessage::Call(msg_type, unique_id, action, payload) => {
                if msg_type != 2 {
                    return Err(OcppError::InvalidMessageType(msg_type));
                }
                Ok(Message::Call(CallMessage {
                    message_type: MessageType::Call,
                    unique_id,
                    action,
                    payload,
                }))
            }
            RawMessage::CallResult(msg_type, unique_id, payload) => {
                if msg_type != 3 {
                    return Err(OcppError::InvalidMessageType(msg_type));
                }
                Ok(Message::CallResult(CallResultMessage {
                    message_type: MessageType::CallResult,
                    unique_id,
                    payload,
                }))
            }
            RawMessage::CallError(
                msg_type,
                unique_id,
                error_code_str,
                error_description,
                error_details,
            ) => {
                if msg_type != 4 {
                    return Err(OcppError::InvalidMessageType(msg_type));
                }

                let error_code = match error_code_str.as_str() {
                    "NotImplemented" => CallErrorCode::NotImplemented,
                    "NotSupported" => CallErrorCode::NotSupported,
                    "InternalError" => CallErrorCode::InternalError,
                    "ProtocolError" => CallErrorCode::ProtocolError,
                    "SecurityError" => CallErrorCode::SecurityError,
                    "FormationViolation" => CallErrorCode::FormationViolation,
                    "FormatViolation" => CallErrorCode::FormatViolation,
                    "PropertyConstraintViolation" => CallErrorCode::PropertyConstraintViolation,
                    "OccurenceConstraintViolation" => CallErrorCode::OccurenceConstraintViolation,
                    "OccurrenceConstraintViolation" => CallErrorCode::OccurrenceConstraintViolation,
                    "TypeConstraintViolation" => CallErrorCode::TypeConstraintViolation,
                    "GenericError" => CallErrorCode::GenericError,
                    _ => {
                        return Err(OcppError::ProtocolViolation {
                            message: format!("Unknown error code: {}", error_code_str),
                        })
                    }
                };

                Ok(Message::CallError(CallErrorMessage {
                    message_type: MessageType::CallError,
                    unique_id,
                    error_code,
                    error_description,
                    error_details,
                }))
            }
        }
    }
}

impl From<Message> for RawMessage {
    fn from(message: Message) -> Self {
        match message {
            Message::Call(msg) => RawMessage::Call(2, msg.unique_id, msg.action, msg.payload),
            Message::CallResult(msg) => RawMessage::CallResult(3, msg.unique_id, msg.payload),
            Message::CallError(msg) => RawMessage::CallError(
                4,
                msg.unique_id,
                msg.error_code.as_str().to_string(),
                msg.error_description,
                msg.error_details,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_call_message_creation() {
        let payload = json!({"idTag": "12345"});
        let msg = CallMessage::new("Authorize".to_string(), &payload).unwrap();

        assert_eq!(msg.action, "Authorize");
        assert_eq!(msg.message_type, MessageType::Call);
        assert!(!msg.unique_id.is_empty());
        assert_eq!(msg.payload, payload);
    }

    #[test]
    fn test_call_result_message_creation() {
        let payload = json!({"idTagInfo": {"status": "Accepted"}});
        let msg = CallResultMessage::new("12345".to_string(), &payload).unwrap();

        assert_eq!(msg.unique_id, "12345");
        assert_eq!(msg.message_type, MessageType::CallResult);
        assert_eq!(msg.payload, payload);
    }

    #[test]
    fn test_call_error_message_creation() {
        let msg = CallErrorMessage::new(
            "12345".to_string(),
            CallErrorCode::NotImplemented,
            "Action not implemented".to_string(),
            None,
        );

        assert_eq!(msg.unique_id, "12345");
        assert_eq!(msg.message_type, MessageType::CallError);
        assert_eq!(msg.error_code, CallErrorCode::NotImplemented);
        assert_eq!(msg.error_description, "Action not implemented");
    }

    #[test]
    fn test_message_enum() {
        let payload = json!({"test": "data"});
        let msg = Message::call("TestAction".to_string(), &payload).unwrap();

        assert_eq!(msg.message_type(), MessageType::Call);
        assert!(!msg.unique_id().is_empty());
    }

    /// Each `Message` variant must round-trip through `serde_json` back to the
    /// *same* variant. The regression guarded here (Issue #321): a `CALLERROR`
    /// deserialized to `Message::Call` under the old `untagged` first-match,
    /// because its `"2"` string error code satisfied `CallMessage`'s action
    /// field — so an inbound CALLERROR was never correlated to its pending call.
    #[test]
    fn message_variants_round_trip_by_discriminator() {
        let call = Message::Call(
            CallMessage::new("Heartbeat".to_string(), json!({"idTag": "T1"})).unwrap(),
        );
        let call_result =
            Message::CallResult(CallResultMessage::new("id-1".to_string(), json!({})).unwrap());
        let call_error = Message::call_error(
            "id-2".to_string(),
            CallErrorCode::InternalError,
            "central system unavailable".to_string(),
            Some(json!({"retryAfter": 30})),
        );

        for msg in [&call, &call_result, &call_error] {
            let text = serde_json::to_string(msg).unwrap();
            let back: Message = serde_json::from_str(&text).unwrap();
            assert_eq!(
                &back, msg,
                "variant must survive a serde round-trip: {text}"
            );
        }

        // The specific regression: a CALLERROR frame decodes as CallError, with
        // its error code + description + details intact — not as a Call.
        let text = serde_json::to_string(&call_error).unwrap();
        match serde_json::from_str::<Message>(&text).unwrap() {
            Message::CallError(e) => {
                assert_eq!(e.error_code, CallErrorCode::InternalError);
                assert_eq!(e.error_description, "central system unavailable");
                assert_eq!(e.error_details, json!({"retryAfter": 30}));
            }
            other => panic!("a CALLERROR must decode as Message::CallError, got {other:?}"),
        }
    }

    /// A frame whose `"0"` discriminator is missing or unrecognized is rejected,
    /// rather than silently coerced onto a variant.
    #[test]
    fn message_rejects_unknown_discriminator() {
        assert!(serde_json::from_str::<Message>(r#"{"0":"NOPE","1":"x"}"#).is_err());
        assert!(serde_json::from_str::<Message>(r#"{"1":"x"}"#).is_err());
    }

    #[test]
    fn test_raw_message_conversion() {
        let payload = json!({"idTag": "12345"});
        let call_msg = CallMessage::new("Authorize".to_string(), &payload).unwrap();
        let message = Message::Call(call_msg.clone());

        let raw: RawMessage = message.into();
        let converted_back = raw.into_message().unwrap();

        if let Message::Call(converted_call) = converted_back {
            assert_eq!(converted_call.action, call_msg.action);
            assert_eq!(converted_call.unique_id, call_msg.unique_id);
            assert_eq!(converted_call.payload, call_msg.payload);
        } else {
            panic!("Expected Call message");
        }
    }

    #[test]
    fn test_call_message_serialization() {
        let payload = json!({"idTag": "TEST123"});
        let msg = CallMessage::new("Authorize".to_string(), &payload).unwrap();

        let json_str = serde_json::to_string(&msg).unwrap();
        let deserialized: CallMessage = serde_json::from_str(&json_str).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_call_error_serialization() {
        let msg = CallErrorMessage::new(
            "test-id".to_string(),
            CallErrorCode::InternalError,
            "Test error".to_string(),
            Some(json!({"detail": "more info"})),
        );

        let json_str = serde_json::to_string(&msg).unwrap();
        let deserialized: CallErrorMessage = serde_json::from_str(&json_str).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_raw_message_invalid_type() {
        let raw = RawMessage::Call(5, "test".to_string(), "Action".to_string(), json!({}));
        let result = raw.into_message();
        assert!(result.is_err());

        if let Err(OcppError::InvalidMessageType(5)) = result {
            // Expected
        } else {
            panic!("Expected InvalidMessageType error");
        }
    }

    /// Build a raw CALLERROR frame carrying `code` and parse it back, asserting
    /// the error-code string round-trips to the same [`CallErrorCode`] variant.
    fn parse_call_error_code(code: &str) -> OcppResult<CallErrorCode> {
        let raw = RawMessage::CallError(
            4,
            "uid-1".to_string(),
            code.to_string(),
            "desc".to_string(),
            json!({}),
        );
        match raw.into_message()? {
            Message::CallError(msg) => Ok(msg.error_code),
            other => panic!("expected CallError, got {other:?}"),
        }
    }

    #[test]
    fn spec_defined_call_error_codes_all_round_trip() {
        // Every code the reference `ocpp/exceptions.py` defines must survive
        // as_str() -> wire -> into_message() -> same variant, so a peer's
        // CALLERROR is never dropped as "unknown".
        let codes = [
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
        for code in codes {
            let parsed = parse_call_error_code(code.as_str())
                .unwrap_or_else(|e| panic!("{} failed to parse: {e:?}", code.as_str()));
            assert_eq!(parsed, code, "{} did not round-trip", code.as_str());
        }
    }

    #[test]
    fn incoming_2_0_1_format_violation_parses_instead_of_being_rejected() {
        // Regression: a spec-compliant 2.0.1 peer reports a malformed payload
        // with `FormatViolation` (the corrected 2.0.1 spelling). It must parse,
        // not fall through to ProtocolViolation("Unknown error code").
        assert_eq!(
            parse_call_error_code("FormatViolation").unwrap(),
            CallErrorCode::FormatViolation
        );
    }

    #[test]
    fn incoming_1_6j_occurence_constraint_violation_parses() {
        // Regression: a 1.6J device using the errata spelling
        // `OccurenceConstraintViolation` (single-r) must parse.
        assert_eq!(
            parse_call_error_code("OccurenceConstraintViolation").unwrap(),
            CallErrorCode::OccurenceConstraintViolation
        );
    }

    #[test]
    fn genuinely_unknown_call_error_code_still_rejected() {
        // Codes outside the spec must still be rejected (mirrors the reference's
        // UnknownCallErrorCodeError), rather than silently accepted.
        let err = parse_call_error_code("TotallyMadeUpCode").unwrap_err();
        match err {
            OcppError::ProtocolViolation { message } => {
                assert!(message.contains("TotallyMadeUpCode"), "got: {message}");
            }
            other => panic!("expected ProtocolViolation, got {other:?}"),
        }
    }

    #[test]
    fn test_payload_extraction() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct TestPayload {
            id_tag: String,
        }

        let payload = TestPayload {
            id_tag: "TEST123".to_string(),
        };

        let msg = CallMessage::new("Authorize".to_string(), &payload).unwrap();
        let extracted: TestPayload = msg.payload_as().unwrap();

        assert_eq!(extracted, payload);
    }
}
