//! Frame string-representation conformance suite — ports the
//! mobilityhouse/ocpp reference's `Call` / `CallResult` / `CallError`
//! representation tests from
//! [`tests/test_messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py)
//! (`test_call_representation`, `test_call_result_representation`,
//! `test_call_error_representation`), backed by the `__repr__` methods on those
//! classes in
//! [`ocpp/messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py).
//!
//! ## What the reference pins
//!
//! Each frame class has a stable `<Kind - field=…, …>` string form. The three
//! reference tests assert the exact envelope:
//!
//! ```text
//! <Call - unique_id=1, action=Heartbeat, payload={}>
//! <CallResult - unique_id=1, action=Authorize, payload={'status': 'Accepted'}>
//! <CallError - unique_id=1, error_code=GenericError, error_description=Some message, error_details={}>
//! ```
//!
//! On the Rust side that form is the `Display` impl on `CallMessage`,
//! `CallResultMessage`, and `CallErrorMessage` (and `Message`, which delegates
//! to its active variant) in `ocpp-types::message`. This suite pins it through
//! the crate's **public** API, so the wire-visible human form of a frame is a
//! stable contract, not an internal detail.
//!
//! ## Divergences from the reference — pinned, not dropped
//!
//! Two faithful adaptations (same convention as `connection_handling.rs` — a
//! divergence is documented and pinned, never silently altered):
//!
//! 1. **Payload rendered as compact JSON.** Rust payloads are
//!    [`serde_json::Value`], not Python dicts, so the payload renders as compact
//!    JSON (`{"status":"Accepted"}`) via `Value`'s own `Display`, rather than
//!    the reference's Python dict-repr (`{'status': 'Accepted'}`). The `CallResult`
//!    case below asserts the JSON spelling for exactly this reason.
//! 2. **`CallResult` omits `action`.** The OCPP CALLRESULT wire frame is
//!    `[3, unique_id, payload]` and carries no action; `CallResultMessage`
//!    models exactly that and has no `action` field to render. The reference
//!    keeps `action` only as an in-memory attribute on its `CallResult` object,
//!    so the Rust envelope renders `<CallResult - unique_id=…, payload=…>` and
//!    the ported case asserts that shape.

use ocpp_types::{
    CallErrorCode, CallErrorMessage, CallMessage, CallResultMessage, Message, MessageType,
};
use serde_json::json;

/// Reference: `test_messages.py::test_call_representation`.
///
/// `Call(unique_id="1", action=Action.heartbeat, payload={})` renders as
/// `<Call - unique_id=1, action=Heartbeat, payload={}>`. The empty payload
/// `{}` is identical under Python dict-repr and compact JSON, so this case
/// matches the reference string verbatim.
#[test]
fn call_representation() {
    let call = CallMessage::with_id("1".to_string(), "Heartbeat".to_string(), json!({})).unwrap();

    assert_eq!(
        call.to_string(),
        "<Call - unique_id=1, action=Heartbeat, payload={}>"
    );
}

/// Reference: `test_messages.py::test_call_result_representation`.
///
/// The reference asserts
/// `<CallResult - unique_id=1, action=Authorize, payload={'status': 'Accepted'}>`.
/// The Rust envelope diverges as documented in the module header — **no
/// `action`** (not on the CALLRESULT wire frame) and **compact-JSON payload** —
/// so the pinned form is
/// `<CallResult - unique_id=1, payload={"status":"Accepted"}>`.
#[test]
fn call_result_representation() {
    let result = CallResultMessage::new("1".to_string(), json!({ "status": "Accepted" })).unwrap();

    assert_eq!(
        result.to_string(),
        r#"<CallResult - unique_id=1, payload={"status":"Accepted"}>"#
    );
}

/// Reference: `test_messages.py::test_call_error_representation`.
///
/// `CallError(unique_id=1, error_code="GenericError", error_description="Some
/// message", error_details={})` renders as `<CallError - unique_id=1,
/// error_code=GenericError, error_description=Some message, error_details={}>`.
/// The `error_code` uses its wire spelling (`GenericError`) — matching the
/// reference — via `CallErrorCode::as_str`, not the human `Display`
/// (`Generic error`). The empty `error_details` `{}` matches verbatim.
#[test]
fn call_error_representation() {
    let error = CallErrorMessage::new(
        "1".to_string(),
        CallErrorCode::GenericError,
        "Some message".to_string(),
        Some(json!({})),
    );

    assert_eq!(
        error.to_string(),
        "<CallError - unique_id=1, error_code=GenericError, \
         error_description=Some message, error_details={}>"
    );
}

/// The `Message` enum's `Display` delegates to the active variant, so a frame
/// renders identically whether formatted directly or through the envelope enum
/// (the form the CSMS/CP log lines and `_validate_payload`-style `ocpp_message`
/// context render). Not a distinct reference case — this pins the delegation so
/// the three ported forms above have a single source of truth.
#[test]
fn message_enum_display_delegates_to_active_variant() {
    let call = CallMessage::with_id("1".to_string(), "Heartbeat".to_string(), json!({})).unwrap();
    let result = CallResultMessage::new("1".to_string(), json!({ "status": "Accepted" })).unwrap();
    let error = CallErrorMessage::new(
        "1".to_string(),
        CallErrorCode::GenericError,
        "Some message".to_string(),
        Some(json!({})),
    );

    // Sanity-check the variants are what we think before asserting delegation.
    assert_eq!(
        Message::Call(call.clone()).message_type(),
        MessageType::Call
    );

    assert_eq!(Message::Call(call.clone()).to_string(), call.to_string());
    assert_eq!(
        Message::CallResult(result.clone()).to_string(),
        result.to_string()
    );
    assert_eq!(
        Message::CallError(error.clone()).to_string(),
        error.to_string()
    );
}
