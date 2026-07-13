//! Outbound CALLRESULT schema-validation conformance suite — ports
//! [`tests/v16/test_v16_charge_point.py::test_route_message_without_validation`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v16/test_v16_charge_point.py)
//! and its validated-route corollary.
//!
//! ## Why this suite exists
//!
//! The reference's `_handle_call()` ([`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py))
//! schema-validates the payload **twice** — the inbound CALL before the handler
//! runs, and the outbound CALLRESULT the handler produced before it is sent —
//! both gated by the route's `_skip_schema_validation` flag:
//!
//! ```python
//! response = msg.create_call_result(camel_case_payload)
//! if not handlers.get("_skip_schema_validation", False):
//!     await validate_payload(response, self._ocpp_version)   # ← outbound check
//! await self._send(response.to_json())
//! ```
//!
//! A failure raises an `OCPPError` that `route_message()` catches and turns into
//! a **CALLERROR** (`except OCPPError → create_call_error`), so a handler bug
//! (an out-of-enum `status`, a missing field, a wrong type) never puts an
//! invalid frame on the wire — the peer gets a CALLERROR instead.
//! `test_route_message_without_validation` pins the *other* side of the flag:
//! with `@on(..., skip_schema_validation=True)` an invalid `"Yolo"` response
//! **is** allowed through.
//!
//! ocpp-rs's [`ActionDispatcher::dispatch`] previously validated only the
//! inbound CALL; the outbound response was returned unchecked (Issue #334). This
//! suite pins the now-symmetric behaviour through the full transport path a
//! spec-conformant peer takes.
//!
//! ## Faithful adaptation (not a literal port)
//!
//! - **Raise → `Err` → CALLERROR.** Rust's `dispatch()` returns
//!   `OcppResult<Value>`; a failed response validation is an `Err`, which the
//!   transport [`DispatchHandler`] maps to a `Message::CallError` — exactly what
//!   the reference's `route_message()` does when `validate_payload(response)`
//!   raises. So "invalid response ⇒ CALLERROR, not an invalid CALLRESULT" is the
//!   assertion, matching the reference frame-for-frame in intent.
//! - **Isolating the response side.** The reference test also sends a
//!   *malformed request* (skip lets it through too). Rust deserialises the CALL
//!   into a typed request first, so a structurally-broken request would fail at
//!   serde regardless of the skip flag. To keep the focus on the *response*
//!   check, these tests send a **valid** request and vary only the response, so
//!   the CALLRESULT-vs-CALLERROR outcome is attributable solely to the outbound
//!   validation.
//!
//! Part of **M8 — Conformance** (Issue #334). Test-only; the production change is
//! the outbound `validate_call_result` in `dispatch()`.

use std::sync::Arc;

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::{ActionDispatcher, CallMessage, Message, OcppAction, OcppResponse};
use ocpp_transport::{DispatchHandler, MessageHandler};
use serde::{Deserialize, Serialize};
use serde_json::json;

// ─── a stand-in BootNotification whose response can be schema-invalid ────────

/// A `BootNotification` request stand-in. `ACTION_NAME` is `BootNotification`,
/// so the bundled 1.6J request *and* response schemas both apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawBootRequest {
    #[serde(rename = "chargePointVendor")]
    vendor: String,
    #[serde(rename = "chargePointModel")]
    model: String,
}

/// A response with a free-form `status: String` — so a handler can emit the
/// out-of-enum `"Yolo"` value straight from the reference test, which the typed
/// `BootNotificationResponse` (a `RegistrationStatus` enum) could never produce.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawBootResponse {
    #[serde(rename = "currentTime")]
    current_time: String,
    interval: i64,
    status: String,
}

impl OcppAction for RawBootRequest {
    const ACTION_NAME: &'static str = "BootNotification";
    type Response = RawBootResponse;
}

impl OcppAction for RawBootResponse {
    const ACTION_NAME: &'static str = "BootNotificationResponse";
    type Response = RawBootResponse;
}

impl OcppResponse for RawBootResponse {}

// ─── helpers ────────────────────────────────────────────────────────────────

fn v16j_validating_dispatcher() -> ActionDispatcher {
    ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v16j()))
}

/// A valid `BootNotification` CALL — the request always passes, so the outcome
/// isolates the response-side check.
fn valid_boot_call() -> CallMessage {
    CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "V", "chargePointModel": "M" }),
    )
    .unwrap()
}

fn raw_boot_response(status: &str) -> RawBootResponse {
    RawBootResponse {
        // A schema-valid RFC3339 `date-time` (the response schema enforces the
        // format), so the only possible violation is the `status` value.
        current_time: "2018-05-29T17:37:05.495259Z".to_string(),
        interval: 350,
        status: status.to_string(),
    }
}

/// Drive a CALL through the transport-level [`DispatchHandler`] — the CSMS
/// analog of `route_message()`/`_handle_call()` — and return the resulting
/// frame (`Message::CallResult` or `Message::CallError`).
async fn route_frame(dispatcher: ActionDispatcher, call: CallMessage) -> Message {
    let handler = DispatchHandler::new(Arc::new(dispatcher));
    handler
        .handle_message(Message::Call(call))
        .await
        .expect("handle_message must not error at the transport layer")
        .expect("a CALL must always produce a response frame")
}

// ─── skip route: the invalid response is sent (ports the reference test) ─────

/// Direct port of `test_route_message_without_validation`: a
/// `@on(..., skip_schema_validation=True)` route lets the handler's invalid
/// `"Yolo"` response through — the emitted frame is a CALLRESULT carrying that
/// status verbatim, not a CALLERROR.
#[tokio::test]
async fn skip_validation_route_emits_the_invalid_response_as_callresult() {
    let mut d = v16j_validating_dispatcher();
    d.on_skip_validation(|_req: RawBootRequest| async move { Ok(raw_boot_response("Yolo")) });

    let call = valid_boot_call();
    let unique_id = call.unique_id.clone();

    match route_frame(d, call).await {
        Message::CallResult(res) => {
            assert_eq!(
                res.unique_id, unique_id,
                "CALLRESULT must reuse the CALL id"
            );
            assert_eq!(
                res.payload["status"], "Yolo",
                "a skip route must send the handler's response unvalidated"
            );
        }
        other => panic!("expected a CALLRESULT carrying the unvalidated response, got {other:?}"),
    }
}

// ─── validated route: the invalid response becomes a CALLERROR ───────────────

/// The corollary the reference implies but does not spell out: with validation
/// **on** (the default route), the same invalid `"Yolo"` response must NOT reach
/// the peer. `dispatch()` returns `Err`, and the transport emits a CALLERROR —
/// the Rust analog of `route_message()`'s `except OCPPError → create_call_error`.
#[tokio::test]
async fn validated_route_turns_an_invalid_response_into_a_callerror() {
    let mut d = v16j_validating_dispatcher();
    d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Yolo")) });

    let call = valid_boot_call();
    let unique_id = call.unique_id.clone();

    match route_frame(d, call).await {
        Message::CallError(err) => {
            assert_eq!(
                err.unique_id, unique_id,
                "the CALLERROR must correlate to the triggering CALL"
            );
        }
        other => panic!("expected a CALLERROR for a schema-invalid response, got {other:?}"),
    }
}

/// A response that satisfies the `BootNotificationResponse` schema is unaffected
/// by the new outbound check: it comes back as a normal CALLRESULT.
#[tokio::test]
async fn validated_route_passes_a_valid_response_through() {
    let mut d = v16j_validating_dispatcher();
    d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Accepted")) });

    match route_frame(d, valid_boot_call()).await {
        Message::CallResult(res) => {
            assert_eq!(res.payload["status"], "Accepted");
        }
        other => panic!("expected a CALLRESULT for a schema-valid response, got {other:?}"),
    }
}
