//! `@on` / `@after` route-map + dispatch conformance suite — ports the
//! mobilityhouse/ocpp reference's
//! [`tests/test_routing.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_routing.py)
//! (backed by [`ocpp/routing.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/routing.py)
//! and the `_handle_call()` / `_raise_key_error()` dispatch logic in
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)).
//!
//! ## What the reference pins
//!
//! `test_create_route_map` asserts that `create_route_map()` builds, from the
//! `@on` / `@after` decorators, a map keyed on `Action` where each entry holds:
//!   - `_on_action`   — the handler a CALL for that action dispatches to;
//!   - `_after_action` — an optional post-response hook;
//!   - `_skip_schema_validation` — a per-route flag (default `False`);
//!
//! and that **undecorated** methods are *not* routed.
//!
//! ## How the Rust model differs (and why this suite pins behaviour, not shape)
//!
//! The Rust side has no inspectable `route_map` dict: routing lives in
//! [`ActionDispatcher`] (`ocpp-messages`), whose `on()` / `after()` builders and
//! `dispatch()` method *are* the route map. So rather than compare a dict, this
//! suite pins the observable routing **contract** through the public dispatcher
//! API — and, for the full CALL→CALLRESULT/CALLERROR frame, through the
//! transport-level [`DispatchHandler`] adapter (the CSMS analog of
//! `_handle_call()`), exactly as a spec-conformant peer would exercise it.
//!
//! Each test maps to the `test_routing.py` / `_handle_call()` behaviour it ports
//! (see the per-test comments).
//!
//!   1. **Per-handler `skip_schema_validation` (Issue #275, now implemented).**
//!      The reference records the flag per route and `_handle_call()` consults
//!      it per action. The Rust [`ActionDispatcher`] now mirrors this: a route
//!      registered via `on_skip_validation` bypasses the dispatcher's
//!      [`SchemaValidator`] for that action only, while sibling routes on the
//!      same dispatcher are still validated. Pinned by
//!      [`skip_schema_validation_is_per_route`] and
//!      [`unvalidated_dispatcher_skips_all_routes`].
//!
//! One faithful-port **gap** surfaced by this audit remains pinned as the
//! *current* Rust behaviour and tracked as a follow-up:
//!
//!   2. **Unregistered action always → `NotSupported`.** The reference's
//!      `_raise_key_error()` distinguishes a *known* OCPP action with no handler
//!      (`NotImplemented`) from an action the version doesn't define at all
//!      (`NotSupported`). The Rust dispatcher returns `NotSupported` for *both*.
//!      Pinned by [`unregistered_action_yields_not_supported_callerror`]; the
//!      `NotImplemented` distinction is tracked as a follow-up.
//!
//! The `@after` hook *does* exist in the Rust model (spawned after the
//! CALLRESULT), so it is asserted directly rather than filed as a gap.
//!
//! Part of **M8 — Conformance** (Issue #272). Test-only; no production code.

use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    MeterValuesRequest, MeterValuesResponse, RegistrationStatus,
};
use ocpp_messages::{ActionDispatcher, CallMessage, Message};
use ocpp_transport::{DispatchHandler, MessageHandler};
use ocpp_types::{CallErrorCode, OcppError};
use serde_json::json;
use tokio::sync::Notify;

// ─── helpers ────────────────────────────────────────────────────────────────

/// A `HeartbeatResponse` with a fixed timestamp — the response the reference's
/// `@on(Action.heartbeat)` handler returns.
fn heartbeat_response() -> HeartbeatResponse {
    HeartbeatResponse {
        current_time: chrono::Utc::now(),
    }
}

fn boot_response() -> BootNotificationResponse {
    BootNotificationResponse {
        current_time: chrono::Utc::now(),
        interval: 300,
        status: RegistrationStatus::Accepted,
    }
}

/// A well-formed `Heartbeat` CALL frame (empty payload, like the reference).
fn heartbeat_call() -> CallMessage {
    CallMessage::new("Heartbeat".to_string(), json!({})).unwrap()
}

/// A `MeterValues` CALL frame — the reference's second `@on` action. An empty
/// `meterValue` array keeps the payload minimal; routing tests use a
/// dispatcher *without* a validator so the sparse payload is accepted.
fn meter_values_call() -> CallMessage {
    CallMessage::new(
        "MeterValues".to_string(),
        json!({ "connectorId": 1, "meterValue": [] }),
    )
    .unwrap()
}

/// Register the reference's `@on(Action.heartbeat)` handler on `d`.
fn register_heartbeat(d: &mut ActionDispatcher) {
    d.on(|_req: HeartbeatRequest| async move { Ok(heartbeat_response()) });
}

/// Drive a CALL through the transport-level [`DispatchHandler`] — the CSMS
/// analog of `_handle_call()` — and return the resulting response frame. This
/// is the full public path a spec-conformant peer takes: a `Message::Call` in,
/// a `Message::CallResult` or `Message::CallError` out.
async fn route_frame(dispatcher: ActionDispatcher, call: CallMessage) -> Message {
    let handler = DispatchHandler::new(Arc::new(dispatcher));
    handler
        .handle_message(Message::Call(call))
        .await
        .expect("handle_message must not error at the transport layer")
        .expect("a CALL must always produce a response frame")
}

// ─── @on: a registered action dispatches to its handler ─────────────────────

/// Ports `test_create_route_map`'s core claim — a method decorated
/// `@on(Action.x)` becomes the `_on_action` for `Action.x` — to the dispatch
/// model: a CALL for the registered action reaches the handler and its return
/// value comes back as the CALLRESULT, preserving the CALL's `unique_id`
/// (`_handle_call()` → `msg.create_call_result(...)`).
#[tokio::test]
async fn on_registered_action_dispatches_and_produces_callresult() {
    let mut d = ActionDispatcher::new();
    register_heartbeat(&mut d);

    let call = heartbeat_call();
    let unique_id = call.unique_id.clone();

    match route_frame(d, call).await {
        Message::CallResult(res) => {
            // Same correlation id as the CALL (`create_call_result` reuses it).
            assert_eq!(res.unique_id, unique_id);
            // HeartbeatResponse serialises to a `currentTime` field.
            assert!(
                res.payload.get("currentTime").is_some(),
                "CALLRESULT payload must carry the handler's response, got {}",
                res.payload
            );
        }
        other => panic!("expected a CALLRESULT frame, got {other:?}"),
    }
}

/// The reference's map holds *both* decorated actions independently
/// (`Action.heartbeat` and `Action.meter_values`). Two `@on` handlers on one
/// dispatcher must each route to the correct handler — never cross-fire.
#[tokio::test]
async fn distinct_on_handlers_route_independently() {
    let mut d = ActionDispatcher::new();
    register_heartbeat(&mut d);
    d.on(|_req: MeterValuesRequest| async move { Ok(MeterValuesResponse {}) });

    assert!(d.has_handler("Heartbeat"));
    assert!(d.has_handler("MeterValues"));
    assert_eq!(d.handler_count(), 2);

    // Heartbeat → non-empty response; MeterValues → empty-object response.
    let hb = d.dispatch(&heartbeat_call()).await.unwrap();
    assert!(hb.get("currentTime").is_some());

    let mv = d.dispatch(&meter_values_call()).await.unwrap();
    assert_eq!(mv, json!({}), "MeterValuesResponse serialises to {{}}");
}

// ─── undecorated / unregistered actions are not routed ──────────────────────

/// Ports `test_create_route_map`'s "undecorated methods are not routed": an
/// action with no `@on` handler is absent from the route map. Registering a
/// handler for one action must not make an unrelated action routable.
#[tokio::test]
async fn undecorated_action_is_not_routed() {
    let mut d = ActionDispatcher::new();
    register_heartbeat(&mut d);

    assert!(d.has_handler("Heartbeat"), "the decorated action is routed");
    assert!(
        !d.has_handler("MeterValues"),
        "an undecorated action must not be routed"
    );
    assert!(
        !d.has_handler("DataTransfer"),
        "an entirely unknown action must not be routed"
    );
}

/// Ports the `_handle_call()` KeyError path: a CALL for an unregistered action
/// must yield a CALLERROR — never a panic or a silently dropped frame.
///
/// **Documented divergence (gap 2 in the module doc):** the reference's
/// `_raise_key_error()` returns `NotImplemented` for a *known* OCPP action with
/// no handler and `NotSupported` only for an unknown one. The Rust dispatcher
/// collapses both into `NotSupported`; this test pins that *current* behaviour
/// (cross-checked against the wire spelling in `exceptions_v16.rs`). The
/// `NotImplemented` distinction is tracked in a follow-up filed with this suite.
#[tokio::test]
async fn unregistered_action_yields_not_supported_callerror() {
    // `Heartbeat` is a valid OCPP 1.6J action, but no handler is registered.
    let d = ActionDispatcher::new();

    // At the dispatcher API: an `Err(NotSupported)`, not a panic.
    let err = d.dispatch(&heartbeat_call()).await.unwrap_err();
    assert!(
        matches!(err, OcppError::NotSupported { ref feature } if feature == "Heartbeat"),
        "expected NotSupported for an unregistered action, got {err:?}"
    );

    // At the transport layer: a real CALLERROR frame carrying the spec code.
    let call = heartbeat_call();
    let unique_id = call.unique_id.clone();
    match route_frame(ActionDispatcher::new(), call).await {
        Message::CallError(e) => {
            assert_eq!(e.unique_id, unique_id);
            // Wire spelling pinned in exceptions_v16.rs: "NotSupported".
            assert_eq!(e.error_code, CallErrorCode::NotSupported);
        }
        other => panic!("expected a CALLERROR frame, got {other:?}"),
    }
}

// ─── @after: post-response hook ─────────────────────────────────────────────

/// Ports the `_after_action` half of the route map + the `_handle_call()`
/// contract that the after-hook runs *after* the response is produced. The
/// Rust `after()` hook is spawned once the handler succeeds, so we register
/// both an `@on` and an `@after` for the same action and assert the hook fires.
#[tokio::test]
async fn after_hook_fires_after_successful_dispatch() {
    let notify = Arc::new(Notify::new());
    let n = notify.clone();

    let mut d = ActionDispatcher::new();
    register_heartbeat(&mut d);
    d.after(move |_req: HeartbeatRequest| {
        let n = n.clone();
        async move {
            n.notify_one();
        }
    });

    let resp = d.dispatch(&heartbeat_call()).await.unwrap();
    assert!(
        resp.get("currentTime").is_some(),
        "the on-handler still runs"
    );

    // The after-hook is spawned; wait (bounded) for it rather than sleeping a
    // fixed duration, to keep the test non-flaky under load.
    tokio::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("the @after hook must fire after a successful dispatch");
}

/// The `_after_action` hook is reached only *after* the `_on_action` handler
/// runs in `_handle_call()`. With no `@on` handler registered, dispatch
/// short-circuits to the KeyError path and the after-hook must never fire —
/// mirroring the reference, where an `_after_action`-only route still raises a
/// key error before any hook runs.
#[tokio::test]
async fn after_hook_does_not_fire_without_a_matching_on_handler() {
    let fired = Arc::new(AtomicBool::new(false));
    let f = fired.clone();

    let mut d = ActionDispatcher::new();
    // Register ONLY the after-hook; no `@on` handler for Heartbeat.
    d.after(move |_req: HeartbeatRequest| {
        let f = f.clone();
        async move {
            f.store(true, Ordering::SeqCst);
        }
    });

    // An after-only registration does not make the action routable.
    assert!(
        !d.has_handler("Heartbeat"),
        "an @after-only route must not be dispatchable"
    );

    let err = d.dispatch(&heartbeat_call()).await.unwrap_err();
    assert!(matches!(err, OcppError::NotSupported { .. }));

    // Give any (erroneously) spawned hook a chance to run, then assert it didn't.
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !fired.load(Ordering::SeqCst),
        "the @after hook must not fire when no @on handler was reached"
    );
}

// ─── skip_schema_validation — per-route flag (Issue #275) ───────────────────

/// An over-length `BootNotification` CALL: `chargePointVendor` exceeds the
/// schema's `maxLength: 20`, so the payload fails schema validation yet still
/// deserialises into a valid `BootNotificationRequest` (it is a valid
/// `String`). This is the discriminator between a *validated* route (rejects
/// it) and a *skipped* route (runs the handler on it).
fn overlong_boot_call() -> CallMessage {
    CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "V".repeat(21), "chargePointModel": "M" }),
    )
    .unwrap()
}

/// **Port of `@on(action, skip_schema_validation=True)`** (gap 1, now closed).
/// The reference records `_skip_schema_validation` *per route*; `_handle_call()`
/// consults it per action. This test pins that the Rust dispatcher does the
/// same: on one dispatcher carrying a validator, a `BootNotification` route
/// registered via `on_skip_validation` waves through a payload that its
/// normally-registered `Heartbeat` sibling's validator would (and does) reject.
#[tokio::test]
async fn skip_schema_validation_is_per_route() {
    let boot_ran = Arc::new(AtomicBool::new(false));
    let b = boot_ran.clone();

    let mut d = ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v16j()));
    d.on_skip_validation(move |_req: BootNotificationRequest| {
        let b = b.clone();
        async move {
            b.store(true, Ordering::SeqCst);
            Ok(boot_response())
        }
    });
    register_heartbeat(&mut d); // normal `on` → validated

    assert!(d.has_validator());
    assert!(
        d.skips_validation("BootNotification"),
        "the BootNotification route opted out of validation"
    );
    assert!(
        !d.skips_validation("Heartbeat"),
        "the sibling route did not opt out"
    );

    // Skipped route: the over-length vendor bypasses the validator and reaches
    // the handler.
    let resp = d.dispatch(&overlong_boot_call()).await.unwrap();
    assert_eq!(resp["status"], "Accepted");
    assert!(
        boot_ran.load(Ordering::SeqCst),
        "the skipped route must run its handler"
    );

    // Validated sibling: an unexpected property on a Heartbeat is still rejected
    // by the same dispatcher's validator (additionalProperties: false) — the
    // skip on one route did not leak to another.
    let bad_hb = CallMessage::new("Heartbeat".to_string(), json!({ "unexpected": true })).unwrap();
    let err = d.dispatch(&bad_hb).await.unwrap_err();
    assert!(
        matches!(err, OcppError::SchemaViolation { .. }),
        "the sibling route must remain validated, got {err:?}"
    );
}

/// A dispatcher with **no** validator attached skips validation for *every*
/// route — there is nothing to validate against. This pins that
/// `on_skip_validation` is observationally identical to `on` in that case (the
/// payload the validating dispatcher above rejects is accepted here) and that
/// `skips_validation` still reports the per-route flag independently of whether
/// a validator is present.
#[tokio::test]
async fn unvalidated_dispatcher_skips_all_routes() {
    let ran = Arc::new(AtomicU32::new(0));
    let r = ran.clone();

    let mut d = ActionDispatcher::new(); // no validator
    assert!(!d.has_validator());
    d.on(move |_req: BootNotificationRequest| {
        let r = r.clone();
        async move {
            r.fetch_add(1, Ordering::SeqCst);
            Ok(boot_response())
        }
    });

    // The over-length vendor would fail schema `maxLength` under a validator,
    // but with none attached it deserialises and the handler runs.
    let resp = d.dispatch(&overlong_boot_call()).await.unwrap();
    assert_eq!(resp["status"], "Accepted");
    assert_eq!(
        ran.load(Ordering::SeqCst),
        1,
        "the handler runs with no validator gating the route"
    );
    assert!(
        !d.skips_validation("BootNotification"),
        "a route registered via `on` reports its flag as false regardless of validator"
    );
}
