//! CALLERROR `description` + `details` conformance suite — ports the
//! mobilityhouse/ocpp reference's no-route frame contract from
//! [`tests/v201/test_v201_charge_point.py::test_route_message_with_no_route`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v201/test_v201_charge_point.py)
//! (backed by [`ocpp/charge_point.py::_raise_key_error`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
//! and the per-code `default_description` in
//! [`ocpp/exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/exceptions.py)).
//!
//! ## What the reference pins
//!
//! When an incoming `CALL` names an action with **no registered handler**,
//! `route_message` → `_handle_call` → `_raise_key_error` builds a structured
//! CALLERROR whose `description` is the spec-canonical text and whose `details`
//! carry a machine-readable `cause`:
//!
//! ```json
//! [4, 1, "NotImplemented",
//!  "Request Action is recognized but not supported by the receiver",
//!  {"cause": "No handler for Heartbeat registered."}]
//! ```
//!
//! and, for an action the version does not define at all, the `NotSupported`
//! analog (`"Requested Action is not known by receiver"` + a `{"cause": …}`
//! detail).
//!
//! ## Why this suite exists alongside `routing.rs`
//!
//! `routing.rs` already pins the *code* of the unrouted-action split
//! (`NotImplemented` vs `NotSupported`). This suite pins the rest of the frame —
//! the spec-canonical `description` and the `{"cause": …}` `details` — which the
//! Rust CALLERROR builder previously dropped (it emitted our generic `Display`
//! text and an empty `{}` detail map). The `cause` on a rejected CALL is the
//! operator-facing signal for *why* a frame was refused at the routing trust
//! boundary, so it is pinned byte-for-byte here.
//!
//! Every assertion drives the **real** production path — a `Message::Call` in
//! through the transport-level [`DispatchHandler`] (the CSMS analog of
//! `_handle_call`), a `Message::CallError` out — exactly as a spec-conformant
//! peer would exercise it.
//!
//! Part of **M8 — Conformance** (Issue #311). Faithful framing at the routing
//! trust boundary.

use std::sync::Arc;

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::{ActionDispatcher, CallMessage, Message};
use ocpp_transport::{DispatchHandler, MessageHandler};
use ocpp_types::{CallErrorCode, CallErrorMessage};
use serde_json::json;

// ─── helpers ────────────────────────────────────────────────────────────────

/// A well-formed CALL frame for `action` with an empty payload. An unrouted
/// action short-circuits to the key-error path *before* any schema validation,
/// so the payload shape is irrelevant here (matching the reference, whose
/// no-route test sends a minimal frame).
fn call(action: &str) -> CallMessage {
    CallMessage::new(action.to_string(), json!({})).unwrap()
}

/// Drive a CALL through the transport-level [`DispatchHandler`] — the CSMS
/// analog of `_handle_call` — and return the resulting CALLERROR, panicking if
/// the frame is anything else. This is the full public path a spec-conformant
/// peer takes.
async fn no_route_call_error(dispatcher: ActionDispatcher, action: &str) -> CallErrorMessage {
    let call = call(action);
    let handler = DispatchHandler::new(Arc::new(dispatcher));
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

/// A validator-backed dispatcher with **no** handlers registered. The attached
/// validator supplies the version context (its bundled schema set stands in for
/// the reference's `v16_Action` / `v201_Action` registry), so an action it
/// knows becomes `NotImplemented` while an unknown one becomes `NotSupported`.
fn validating_dispatcher(validator: SchemaValidator) -> ActionDispatcher {
    ActionDispatcher::new().with_validator(Arc::new(validator))
}

// ─── NotImplemented: known action, no handler ───────────────────────────────

/// Direct port of `test_route_message_with_no_route` (v201): a `Heartbeat` CALL
/// with an empty route map yields a `NotImplemented` CALLERROR carrying the
/// spec-canonical description and `details = {"cause": "No handler for Heartbeat
/// registered."}`. `Heartbeat` is a known 2.0.1 action (the `v201` validator
/// has its schema), so the version-generic dispatcher classifies it as
/// known-but-unhandled.
#[tokio::test]
async fn v201_no_route_is_not_implemented_with_cause_detail() {
    let err =
        no_route_call_error(validating_dispatcher(SchemaValidator::v201()), "Heartbeat").await;

    assert_eq!(err.error_code, CallErrorCode::NotImplemented);
    assert_eq!(
        err.error_description, "Request Action is recognized but not supported by the receiver",
        "description must be the reference NotImplementedError default_description"
    );
    assert_eq!(
        err.error_details,
        json!({ "cause": "No handler for Heartbeat registered." }),
        "details must carry the reference `cause` hint verbatim"
    );
}

/// The 1.6J analog: the reference `_raise_key_error` is version-generic, so a
/// known-but-unrouted 1.6J action produces the same `NotImplemented` frame shape
/// under the `v16j` validator.
#[tokio::test]
async fn v16_no_route_is_not_implemented_with_cause_detail() {
    let err =
        no_route_call_error(validating_dispatcher(SchemaValidator::v16j()), "Heartbeat").await;

    assert_eq!(err.error_code, CallErrorCode::NotImplemented);
    assert_eq!(
        err.error_description,
        "Request Action is recognized but not supported by the receiver"
    );
    assert_eq!(
        err.error_details,
        json!({ "cause": "No handler for Heartbeat registered." })
    );
}

// ─── NotSupported: unknown action ───────────────────────────────────────────

/// The `NotSupportedError` branch of `_raise_key_error`: a CALL for an action
/// the version does not define yields a `NotSupported` CALLERROR with the
/// canonical description and a `{"cause": …}` detail naming the offending action.
///
/// The reference cause embeds the OCPP version (`… not supported by OCPP<v>.`),
/// which isn't threaded to the CALLERROR-build layer (see #311), so the Rust
/// side emits a version-agnostic cause; the description is faithful and the
/// offending action name is preserved.
#[tokio::test]
async fn unknown_action_is_not_supported_with_cause_detail() {
    let err = no_route_call_error(
        validating_dispatcher(SchemaValidator::v201()),
        "TotallyUnknownAction",
    )
    .await;

    assert_eq!(err.error_code, CallErrorCode::NotSupported);
    assert_eq!(
        err.error_description, "Requested Action is not known by receiver",
        "description must be the reference NotSupportedError default_description"
    );
    assert_eq!(
        err.error_details,
        json!({ "cause": "TotallyUnknownAction not supported by receiver." }),
        "details must name the offending action in its `cause`"
    );
}

/// Without a validator the dispatcher has no version context, so it cannot tell
/// a known action from an unknown one and conservatively reports `NotSupported`
/// (pinned in `routing.rs`). This suite adds that the resulting frame still
/// carries the canonical `NotSupported` description and a `{"cause": …}` detail —
/// the observability hint is present regardless of version context.
#[tokio::test]
async fn no_version_context_still_carries_not_supported_cause_detail() {
    let err = no_route_call_error(ActionDispatcher::new(), "Heartbeat").await;

    assert_eq!(err.error_code, CallErrorCode::NotSupported);
    assert_eq!(
        err.error_description,
        "Requested Action is not known by receiver"
    );
    assert_eq!(
        err.error_details,
        json!({ "cause": "Heartbeat not supported by receiver." })
    );
}

/// The CALLERROR echoes the triggering CALL's `unique_id` (the reference's
/// `create_call_error` reuses `msg.unique_id`), and the `details` survive the
/// full serialize → wire → deserialize round-trip through the framing layer —
/// proving the `cause` reaches a peer intact, not just the in-process struct.
#[tokio::test]
async fn cause_detail_survives_the_wire_round_trip() {
    let call = call("Heartbeat");
    let unique_id = call.unique_id.clone();

    let handler = DispatchHandler::new(Arc::new(validating_dispatcher(SchemaValidator::v201())));
    let frame = handler
        .handle_message(Message::Call(call))
        .await
        .unwrap()
        .expect("a CALL must produce a response frame");

    // Serialize to the on-wire array form, then parse it straight back — the
    // path a real peer's bytes take.
    let wire = serde_json::to_string(&frame).expect("serialize CALLERROR frame");
    let back: CallErrorMessage = serde_json::from_str(&wire)
        .unwrap_or_else(|e| panic!("reparse CALLERROR: {e}\nraw: {wire}"));

    assert_eq!(back.unique_id, unique_id, "unique_id must be echoed");
    assert_eq!(back.error_code, CallErrorCode::NotImplemented);
    assert_eq!(
        back.error_details,
        json!({ "cause": "No handler for Heartbeat registered." }),
        "the cause detail must survive the wire round-trip"
    );
}
