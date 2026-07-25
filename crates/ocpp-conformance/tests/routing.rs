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
//!   2. **Unrouted-action split (Issue #276, now implemented).** The reference's
//!      `_raise_key_error()` distinguishes a *known* OCPP action with no handler
//!      (`NotImplemented`) from an action the version doesn't define at all
//!      (`NotSupported`). The Rust [`ActionDispatcher`] now mirrors this: its
//!      attached [`SchemaValidator`] supplies the version context (its bundled
//!      schema set is the version-scoped known-action registry standing in for
//!      the reference's `v16_Action`/`v201_Action` enum), so a known-but-
//!      unregistered action yields `NotImplemented` while an undefined action
//!      yields `NotSupported`. With no validator attached there is no version
//!      context, so the dispatcher conservatively reports `NotSupported`. Pinned
//!      by [`unrouted_known_action_is_not_implemented`],
//!      [`unrouted_unknown_action_is_not_supported`], and
//!      [`unrouted_action_without_version_context_is_not_supported`].
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

/// A validator-backed dispatcher with **no** handlers registered: the version
/// context (1.6J) is present, so the unrouted-action split can distinguish a
/// known action from an undefined one — but every action still misses.
fn v16j_validating_dispatcher() -> ActionDispatcher {
    ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v16j()))
}

/// Ports the `NotImplementedError` branch of `_raise_key_error(action, version)`:
/// a CALL for a *known* OCPP action with no registered handler yields a
/// `NotImplemented` CALLERROR — never a panic or a silently dropped frame.
///
/// `Heartbeat` is a valid 1.6J action (the attached `v16j` validator has its
/// schema), so the version-generic dispatcher can tell it is *known-but-
/// unhandled*. Asserted at both the dispatcher API and the transport frame; the
/// `NotImplemented` wire spelling is pinned in `exceptions_v16.rs`.
#[tokio::test]
async fn unrouted_known_action_is_not_implemented() {
    let d = v16j_validating_dispatcher();
    assert!(!d.has_handler("Heartbeat"));

    // At the dispatcher API: an `Err(NotImplemented)`, not a panic.
    let err = d.dispatch(&heartbeat_call()).await.unwrap_err();
    assert!(
        matches!(err, OcppError::NotImplemented { ref feature } if feature == "Heartbeat"),
        "expected NotImplemented for a known-but-unregistered action, got {err:?}"
    );

    // At the transport layer: a real CALLERROR frame carrying the spec code.
    let call = heartbeat_call();
    let unique_id = call.unique_id.clone();
    match route_frame(v16j_validating_dispatcher(), call).await {
        Message::CallError(e) => {
            assert_eq!(e.unique_id, unique_id);
            assert_eq!(e.error_code, CallErrorCode::NotImplemented);
        }
        other => panic!("expected a CALLERROR frame, got {other:?}"),
    }
}

/// Ports the `NotSupportedError` branch of `_raise_key_error`: a CALL for an
/// action the version does **not** define yields a `NotSupported` CALLERROR,
/// even when a validator (version context) is attached — the validator simply
/// has no schema for it.
#[tokio::test]
async fn unrouted_unknown_action_is_not_supported() {
    let d = v16j_validating_dispatcher();
    let unknown = CallMessage::new("TotallyUnknownAction".to_string(), json!({})).unwrap();

    let err = d.dispatch(&unknown).await.unwrap_err();
    assert!(
        matches!(err, OcppError::NotSupported { ref feature } if feature == "TotallyUnknownAction"),
        "expected NotSupported for an action the version does not define, got {err:?}"
    );

    let call = CallMessage::new("TotallyUnknownAction".to_string(), json!({})).unwrap();
    let unique_id = call.unique_id.clone();
    match route_frame(v16j_validating_dispatcher(), call).await {
        Message::CallError(e) => {
            assert_eq!(e.unique_id, unique_id);
            // Wire spelling pinned in exceptions_v16.rs: "NotSupported".
            assert_eq!(e.error_code, CallErrorCode::NotSupported);
        }
        other => panic!("expected a CALLERROR frame, got {other:?}"),
    }
}

/// The version-generic dispatcher has no version context without a validator, so
/// it cannot know whether a missing-handler action is "known". It then
/// conservatively reports `NotSupported` — even for `Heartbeat`, a valid 1.6J
/// action — rather than guess `NotImplemented`. This pins that the split is
/// gated on the injected version context, matching the reference where
/// `_raise_key_error` always receives an explicit `version`.
#[tokio::test]
async fn unrouted_action_without_version_context_is_not_supported() {
    let d = ActionDispatcher::new();
    assert!(!d.has_validator());

    let err = d.dispatch(&heartbeat_call()).await.unwrap_err();
    assert!(
        matches!(err, OcppError::NotSupported { ref feature } if feature == "Heartbeat"),
        "without version context a missing handler must yield NotSupported, got {err:?}"
    );

    let call = heartbeat_call();
    let unique_id = call.unique_id.clone();
    match route_frame(ActionDispatcher::new(), call).await {
        Message::CallError(e) => {
            assert_eq!(e.unique_id, unique_id);
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

/// **Port of `@after(action, inject_response=True)`** — the reference's
/// `test_response_injected_to_after_handler` (`tests/test_charge_point.py`).
/// When an `@after` hook opts into the response via `after_with_response`, it
/// receives the exact response the `@on` handler returned (and that was sent
/// back to the counterparty), mirroring `_handle_call()` injecting
/// `call_response = response_payload`. The reference asserts the hook sees
/// `current_time` / `interval` / `status`; the strongly-typed Rust hook asserts
/// the same three fields on a `BootNotificationResponse`.
#[tokio::test]
async fn after_with_response_injects_the_on_handlers_response() {
    // A fixed response (the reference uses a hard-coded `2024-11-01T00:00:00Z`)
    // so the hook can assert an exact `current_time`.
    let current_time = chrono::DateTime::parse_from_rfc3339("2024-11-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let fixed = BootNotificationResponse {
        current_time,
        interval: 300,
        status: RegistrationStatus::Accepted,
    };

    // Capture the injected response so assertions run on the main test thread
    // (a panic inside the spawned hook would not fail the test on its own).
    let seen = Arc::new(std::sync::Mutex::new(None::<BootNotificationResponse>));
    let s = seen.clone();
    let notify = Arc::new(Notify::new());
    let n = notify.clone();

    let mut d = ActionDispatcher::new();
    d.on(move |_req: BootNotificationRequest| {
        let fixed = fixed.clone();
        async move { Ok(fixed) }
    });
    d.after_with_response(
        move |_req: BootNotificationRequest, resp: BootNotificationResponse| {
            let s = s.clone();
            let n = n.clone();
            async move {
                *s.lock().unwrap() = Some(resp);
                n.notify_one();
            }
        },
    );

    let call = CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "vendor", "chargePointModel": "model" }),
    )
    .unwrap();
    d.dispatch(&call).await.unwrap();

    // Ensure the after handler actually ran (the reference asserts a call count
    // of 1 so its inner assertions are not silently skipped).
    tokio::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("the after_with_response hook must fire after a successful dispatch");

    let got = seen
        .lock()
        .unwrap()
        .clone()
        .expect("the hook must have run");
    assert_eq!(got.current_time, current_time, "injected current_time");
    assert_eq!(got.interval, 300, "injected interval");
    assert_eq!(got.status, RegistrationStatus::Accepted, "injected status");
}

/// **Port of an `@after` hook declaring `call_unique_id` *and*
/// `inject_response=True`** — the reference's `_handle_call()` injects the two
/// independently and simultaneously (`snake_case_payload["call_unique_id"]` and
/// `snake_case_payload["call_response"]`), so a single hook can receive both.
/// `after_with_id_and_response` is the Rust analog: the hook sees the triggering
/// CALL's exact `unique_id` alongside the `@on` handler's response.
#[tokio::test]
async fn after_with_id_and_response_injects_both_id_and_response() {
    let current_time = chrono::DateTime::parse_from_rfc3339("2024-11-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let fixed = BootNotificationResponse {
        current_time,
        interval: 300,
        status: RegistrationStatus::Accepted,
    };

    let seen_resp = Arc::new(std::sync::Mutex::new(None::<BootNotificationResponse>));
    let seen_id = Arc::new(std::sync::Mutex::new(None::<String>));
    let sr = seen_resp.clone();
    let si = seen_id.clone();
    let notify = Arc::new(Notify::new());
    let n = notify.clone();

    let mut d = ActionDispatcher::new();
    d.on(move |_req: BootNotificationRequest| {
        let fixed = fixed.clone();
        async move { Ok(fixed) }
    });
    d.after_with_id_and_response(
        move |_req: BootNotificationRequest, resp: BootNotificationResponse, unique_id: String| {
            let sr = sr.clone();
            let si = si.clone();
            let n = n.clone();
            async move {
                *sr.lock().unwrap() = Some(resp);
                *si.lock().unwrap() = Some(unique_id);
                n.notify_one();
            }
        },
    );

    let call = CallMessage::with_id(
        "boot-abc-123".to_string(),
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "vendor", "chargePointModel": "model" }),
    )
    .unwrap();
    d.dispatch(&call).await.unwrap();

    tokio::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("the after_with_id_and_response hook must fire after a successful dispatch");

    let got = seen_resp
        .lock()
        .unwrap()
        .clone()
        .expect("the hook must have run");
    assert_eq!(got.current_time, current_time, "injected current_time");
    assert_eq!(got.interval, 300, "injected interval");
    assert_eq!(got.status, RegistrationStatus::Accepted, "injected status");
    assert_eq!(
        seen_id.lock().unwrap().as_deref(),
        Some("boot-abc-123"),
        "the hook must also see the triggering CALL's exact unique_id"
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
