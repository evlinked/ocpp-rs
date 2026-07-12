//! CALLERROR `details` conformance suite for **schema-validation** failures —
//! ports the mobilityhouse/ocpp reference's "surface the triggering message"
//! contract from
//! [`tests/test_exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_exceptions.py)
//! (`test_exception_show_triggered_message_type_constraint` /
//! `test_exception_show_triggered_message_format`), backed by
//! [`ocpp/messages.py::_validate_payload`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py).
//!
//! ## What the reference pins
//!
//! `_validate_payload` does not merely pick an error *code* on a schema failure
//! — it wraps the raised `OCPPError` with a `details` map that carries the
//! *triggering message's* context, so an operator can see **which** CALL failed
//! and **why**. Per failing keyword:
//!
//! | `e.validator`        | exception                     | `details`                          |
//! |----------------------|-------------------------------|------------------------------------|
//! | `type`               | `TypeConstraintViolationError`| `{"cause": e.message, "ocpp_message": …}` |
//! | `maxLength`          | `TypeConstraintViolationError`| `{"cause": e.message, "ocpp_message": …}` |
//! | `additionalProperties`| `FormatViolationError`       | `{"cause": e.message, "ocpp_message": …}` |
//! | `required`           | `ProtocolError`               | `{"cause": e.message}` *(no context)* |
//! | *(else)*             | `FormatViolationError`        | `{"cause": …, "ocpp_message": …}`  |
//!
//! `test_exceptions.py` asserts the `ocpp_message` context is present in the
//! type-constraint and format cases:
//!
//! ```python
//! ocpp_message = ("'ocpp_message': <Call - unique_id=123456, "
//!                 "action=BootNotification, payload={…}")
//! assert ocpp_message in str(exception_info.value)
//! ```
//!
//! ## The idiomatic port (issue #313)
//!
//! The reference stores the whole triggering `Call` under `ocpp_message` — a
//! Python `repr` that also echoes the full payload back to the peer that just
//! sent it (redundant, and unbounded in size). We surface just the offending
//! **`action`** name plus the machine-readable **`cause`** — the schema-violation
//! message, the equivalent of the reference's `e.message`. Faithful to the
//! reference's per-keyword split, the `required` branch (`ProtocolError`, whose
//! reference `details` carry only a `cause` and *no* `ocpp_message`) omits the
//! `action`.
//!
//! Every assertion drives the **real** production path — a `Message::Call` in
//! through the transport-level [`DispatchHandler`] (the CSMS analog of
//! `_handle_call`), a `Message::CallError` out — so what a spec-conformant peer
//! receives on the wire is exactly what is pinned here.
//!
//! Part of **M8 — Conformance** (Issue #313). Follow-up to the unrouted-action
//! `details` port in `call_error_details.rs` (#311).

use std::sync::Arc;

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::v16j::{BootNotificationRequest, BootNotificationResponse, RegistrationStatus};
use ocpp_messages::{ActionDispatcher, CallMessage, Message};
use ocpp_transport::{DispatchHandler, MessageHandler};
use ocpp_types::{CallErrorCode, CallErrorMessage};
use serde_json::{json, Value};

// ─── helpers ────────────────────────────────────────────────────────────────

/// A `v16j` dispatcher with a **registered** `BootNotification` handler behind
/// the 1.6J [`SchemaValidator`]. The handler must exist so the CALL reaches
/// schema validation at all — an *unrouted* action short-circuits to the
/// key-error path (`NotImplemented`/`NotSupported`, pinned in
/// `call_error_details.rs`) *before* any validation runs. The handler itself
/// never fires here: every payload in this suite fails validation first, so it
/// is a placeholder proving the malformed frame is rejected *before* handler
/// deserialization (mirroring `_handle_call`, which validates then dispatches).
fn boot_dispatcher() -> ActionDispatcher {
    let mut d = ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v16j()));
    d.on(|_req: BootNotificationRequest| async {
        Ok(BootNotificationResponse {
            current_time: chrono::Utc::now(),
            interval: 300,
            status: RegistrationStatus::Accepted,
        })
    });
    d
}

/// Drive a `BootNotification` CALL carrying `payload` through the transport-level
/// [`DispatchHandler`] — the CSMS analog of `_handle_call` → `_validate_payload`
/// — and return the resulting CALLERROR, panicking if the frame is anything else.
async fn boot_call_error(payload: Value) -> CallErrorMessage {
    let call = CallMessage::new("BootNotification".to_string(), payload).unwrap();
    let handler = DispatchHandler::new(Arc::new(boot_dispatcher()));
    match handler
        .handle_message(Message::Call(call))
        .await
        .expect("handle_message must not error at the transport layer")
        .expect("a CALL must always produce a response frame")
    {
        Message::CallError(e) => e,
        other => panic!("expected a CALLERROR frame, got {other:?}"),
    }
}

/// Extract a `details` field as a `&str`, asserting it is a present, non-empty
/// string (the `cause`/`action` observability hints must never be blank).
fn detail_str<'a>(err: &'a CallErrorMessage, key: &str) -> &'a str {
    let s = err
        .error_details
        .get(key)
        .unwrap_or_else(|| {
            panic!(
                "details must carry a `{key}` field; got {:?}",
                err.error_details
            )
        })
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "`{key}` detail must be a string; got {:?}",
                err.error_details
            )
        });
    assert!(!s.is_empty(), "`{key}` detail must not be empty");
    s
}

// ─── type constraint: `chargePointVendor` is an int, not a string ────────────

/// Direct port of `test_exception_show_triggered_message_type_constraint`: a
/// `BootNotification` whose `chargePointVendor` is the integer `1` (schema wants
/// a string) trips the `type` keyword. The reference raises
/// `TypeConstraintViolationError` with `details={"cause": e.message,
/// "ocpp_message": <the Call>}`; the idiomatic port surfaces the offending
/// `action` in place of the full-`Call` echo, alongside the `cause`.
#[tokio::test]
async fn type_constraint_failure_names_triggering_action() {
    let err = boot_call_error(json!({
        "chargePointVendor": 1,
        "chargePointModel": "SingleSocketCharger",
    }))
    .await;

    // Code is the keyword-granular one — unchanged classification (`type` →
    // TypeConstraintViolation), exactly as the reference maps it.
    assert_eq!(err.error_code, CallErrorCode::TypeConstraintViolation);

    // The port's substance: the triggering message is surfaced, not an empty {}.
    assert_eq!(
        detail_str(&err, "action"),
        "BootNotification",
        "details must name the offending action (the `ocpp_message` context port)"
    );
    // The `cause` mirrors the reference's `e.message` — it must describe the
    // constraint (a wrong JSON `type`).
    assert!(
        detail_str(&err, "cause").contains("string"),
        "cause should describe the failed `type` constraint; got {:?}",
        err.error_details
    );
}

/// The `maxLength` sibling — the reference routes it through the *same*
/// `TypeConstraintViolationError` + `ocpp_message` branch. A `chargePointVendor`
/// over the schema's 20-char limit must likewise surface the action.
#[tokio::test]
async fn max_length_failure_names_triggering_action() {
    let err = boot_call_error(json!({
        "chargePointVendor": "V".repeat(21),
        "chargePointModel": "M",
    }))
    .await;

    assert_eq!(err.error_code, CallErrorCode::TypeConstraintViolation);
    assert_eq!(detail_str(&err, "action"), "BootNotification");
    // cause is present and non-empty (asserted by `detail_str`).
    let _ = detail_str(&err, "cause");
}

// ─── format violation: an unexpected property ────────────────────────────────

/// Direct port of `test_exception_show_triggered_message_format`: the reference
/// sends the bare `{"syntactically": "incorrect"}` and expects
/// `FormatViolationError`. That payload trips both `additionalProperties` (the
/// unexpected property) and `required` (both required fields missing); our
/// validator's deterministic dominant-keyword precedence ranks
/// `additionalProperties` above `required` (see `keyword_priority` in
/// `schema_validation.rs`), so it classifies exactly as the reference does —
/// `FormationViolation` (the strict 1.6J spelling of the reference's
/// `FormatViolationError`). The port pins that this format failure surfaces the
/// triggering `action` in its `details` (the reference attaches `ocpp_message`).
#[tokio::test]
async fn format_violation_names_triggering_action() {
    let err = boot_call_error(json!({ "syntactically": "incorrect" })).await;

    assert_eq!(err.error_code, CallErrorCode::FormationViolation);
    assert_eq!(detail_str(&err, "action"), "BootNotification");
    assert!(
        detail_str(&err, "cause").contains("syntactically"),
        "cause should name the unexpected property; got {:?}",
        err.error_details
    );
}

// ─── required: the reference's context-free branch ───────────────────────────

/// The `required` branch is the reference's one exception to attaching context:
/// `_validate_payload` raises `ProtocolError(details={"cause": e.message})` with
/// **no** `ocpp_message`. Ported faithfully, a `BootNotification` missing a
/// required field — with no other (higher-priority) violation to outrank it —
/// yields a `ProtocolError` CALLERROR whose `details` carry a `cause` but *no*
/// `action`: the machine-readable reason without the triggering-message echo,
/// exactly as the reference splits it.
#[tokio::test]
async fn required_failure_carries_cause_but_no_action() {
    // Only `chargePointVendor` is missing; `chargePointModel` is a valid string
    // and there is no unexpected property, so `required` is the sole — hence
    // dominant — keyword (no `additionalProperties`/`type` to outrank it).
    let err = boot_call_error(json!({ "chargePointModel": "M" })).await;

    assert_eq!(err.error_code, CallErrorCode::ProtocolError);
    // cause present and non-empty…
    let _ = detail_str(&err, "cause");
    // …but the `required` branch omits the action, mirroring the reference's
    // context-free `details={"cause": …}`.
    assert!(
        err.error_details.get("action").is_none(),
        "the `required` branch must not echo an `action` (reference omits ocpp_message); got {:?}",
        err.error_details
    );
}

// ─── the details reach the peer intact ───────────────────────────────────────

/// The `action` + `cause` `details` must survive the full serialize → wire →
/// deserialize round-trip through the framing layer, proving they reach a peer
/// intact rather than living only in the in-process struct — the same guarantee
/// `call_error_details.rs` pins for the no-route `cause`.
#[tokio::test]
async fn validation_details_survive_the_wire_round_trip() {
    let call = CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": 1, "chargePointModel": "M" }),
    )
    .unwrap();
    let unique_id = call.unique_id.clone();

    let handler = DispatchHandler::new(Arc::new(boot_dispatcher()));
    let frame = handler
        .handle_message(Message::Call(call))
        .await
        .unwrap()
        .expect("a CALL must produce a response frame");

    let wire = serde_json::to_string(&frame).expect("serialize CALLERROR frame");
    let back: CallErrorMessage = serde_json::from_str(&wire)
        .unwrap_or_else(|e| panic!("reparse CALLERROR: {e}\nraw: {wire}"));

    assert_eq!(back.unique_id, unique_id, "unique_id must be echoed");
    assert_eq!(back.error_code, CallErrorCode::TypeConstraintViolation);
    assert_eq!(
        back.error_details.get("action").and_then(Value::as_str),
        Some("BootNotification"),
        "the `action` detail must survive the wire round-trip"
    );
    assert!(
        back.error_details
            .get("cause")
            .and_then(Value::as_str)
            .is_some_and(|c| !c.is_empty()),
        "the `cause` detail must survive the wire round-trip"
    );
}
