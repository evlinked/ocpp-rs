//! Route-map construction is **inert** — ports the mobilityhouse/ocpp
//! reference's
//! [`tests/test_charge_point.py::test_getters_should_not_be_called_during_routemap_setup`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point.py).
//!
//! ## What the reference pins
//!
//! ```python
//! def test_getters_should_not_be_called_during_routemap_setup():
//!     class ChargePoint(cp_201):
//!         @property
//!         def foo(self):
//!             raise RuntimeError("this will be raised")
//!
//!     try:
//!         ChargePoint("blah", None)   # builds the route map
//!     except RuntimeError:
//!         pytest.fail("Getter was called during ChargePoint creation")
//! ```
//!
//! The reference's `create_route_map()`
//! ([`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py))
//! walks only the names in `routables` (the `@on` / `@after`-decorated methods)
//! rather than every attribute, so a side-effectful `@property` is never touched
//! at construction time. The invariant it guards: **building the route map only
//! inspects the registered routes; it never invokes arbitrary user code.**
//!
//! ## How the Rust model maps
//!
//! Rust has no property getters, so the faithful image of the invariant is about
//! the handler closures themselves. The Rust route map is the
//! [`ActionDispatcher`] (`crates/ocpp-messages/src/dispatcher.rs`): its `on()` /
//! `on_skip_validation()` / `after()` builders wrap the user closure
//! `F: Fn(Req) -> Fut` in an erasure and insert it into an instance `HashMap`.
//! The user closure is only ever *called* inside `dispatch()`
//! (`(route.handler)(payload, unique_id).await`). So the reference's
//! "getter not called during setup" becomes:
//!
//! > Registering a handler **stores** it; it does not **call** it. No `@on`
//! > body, no `@after` body, and no side effect a body would produce runs until
//! > a matching CALL is dispatched.
//!
//! This is a real safety property — handlers must not carry registration-time
//! side effects — and it is otherwise unpinned: `routing.rs` pins
//! skip-validation / unrouted-action / after-hook firing, and
//! `route_map_independence.rs` pins per-instance isolation, but neither asserts
//! that *registration itself* is side-effect-free.
//!
//! Part of **M8 — Conformance** ([Issue #415](https://github.com/EVLinked/ocpp-rs/issues/415)).
//! Test-only; no production code.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    MeterValuesRequest, MeterValuesResponse, RegistrationStatus,
};
use ocpp_messages::{ActionDispatcher, CallMessage};
use serde_json::json;
use tokio::sync::Notify;

// ─── helpers ────────────────────────────────────────────────────────────────

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

// ─── @on registration is inert ──────────────────────────────────────────────

/// Core port. A handler closure records a side effect *at the point it is
/// called*; registering it — and then querying the route map — must leave that
/// side effect count at **0**. Only a real `dispatch()` for the action runs the
/// body, and only for the dispatched action (sibling routes stay inert). This is
/// the Rust image of "the `@property` getter was never accessed during
/// `ChargePoint` construction".
#[tokio::test]
async fn registering_on_handlers_does_not_invoke_their_bodies() {
    let calls = Arc::new(AtomicU32::new(0));

    let mut d = ActionDispatcher::new();

    // The `fetch_add` is in the closure-call position (before the returned
    // future), so it fires the instant the closure is *invoked* — never at
    // registration, where the closure is only moved/cloned into the erasure.
    let c_hb = calls.clone();
    d.on(move |_req: HeartbeatRequest| {
        c_hb.fetch_add(1, Ordering::SeqCst);
        async move { Ok(heartbeat_response()) }
    });
    let c_mv = calls.clone();
    d.on(move |_req: MeterValuesRequest| {
        c_mv.fetch_add(1, Ordering::SeqCst);
        async move { Ok(MeterValuesResponse {}) }
    });

    // Building the route map + inspecting it must not have run any body.
    assert!(d.has_handler("Heartbeat"));
    assert!(d.has_handler("MeterValues"));
    assert_eq!(d.handler_count(), 2);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no handler body may run while the route map is only being built/queried"
    );

    // A real CALL runs exactly one body: the dispatched action's. The sibling
    // route remains inert.
    let _ = d.dispatch(&heartbeat_call()).await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "dispatch must run the target handler exactly once, and no sibling"
    );
}

/// The direct image of the reference's raising `@property`. A handler whose
/// body would `panic!` the instant it is invoked is registered, and the route
/// map is built and queried. Registration must not panic — because it must not
/// call the body. The route is never dispatched, so the body never runs.
#[tokio::test]
#[allow(unreachable_code)]
async fn registering_a_handler_with_a_blowup_body_does_not_run_it_at_setup() {
    let mut d = ActionDispatcher::new();

    d.on(move |_req: HeartbeatRequest| {
        // Reached only if the route map *called* this closure during setup —
        // the reference's `RuntimeError`-raising getter analog. It must not.
        panic!("handler body must never run during route-map construction");
        async move { Ok(heartbeat_response()) }
    });

    // If registration had invoked the closure, the test would have panicked
    // above. It did not: the route is present, inert, awaiting dispatch.
    assert!(
        d.has_handler("Heartbeat"),
        "the route is registered even though its body never ran"
    );
    assert_eq!(d.handler_count(), 1);
}

/// Bulk registration is inert: registering many routes runs none of their
/// bodies, and no registration triggers a sibling. Guards against a future
/// refactor that eagerly evaluated handlers (e.g. to precompute anything) at
/// build time.
#[tokio::test]
async fn many_routes_register_without_running_any_body() {
    let calls = Arc::new(AtomicU32::new(0));
    let mut d = ActionDispatcher::new();

    let c1 = calls.clone();
    d.on(move |_req: HeartbeatRequest| {
        c1.fetch_add(1, Ordering::SeqCst);
        async move { Ok(heartbeat_response()) }
    });
    let c2 = calls.clone();
    d.on(move |_req: MeterValuesRequest| {
        c2.fetch_add(1, Ordering::SeqCst);
        async move { Ok(MeterValuesResponse {}) }
    });
    let c3 = calls.clone();
    d.on(move |_req: BootNotificationRequest| {
        c3.fetch_add(1, Ordering::SeqCst);
        async move { Ok(boot_response()) }
    });

    assert_eq!(d.handler_count(), 3);
    for action in ["Heartbeat", "MeterValues", "BootNotification"] {
        assert!(d.has_handler(action), "{action} must be routed");
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "registering N routes must run zero handler bodies"
    );
}

// ─── @after registration is inert ───────────────────────────────────────────

/// The `@after` hook analog. Registering an `after` hook must not fire it; the
/// hook runs only after a *successful* dispatch of its `@on` handler (spawned
/// post-CALLRESULT). Asserts the count is 0 across registration and rises to 1
/// only after dispatch — verified via the established bounded-`Notify` pattern
/// so the spawned hook is awaited rather than slept on.
#[tokio::test]
async fn registering_an_after_hook_does_not_fire_it() {
    let fired = Arc::new(AtomicU32::new(0));
    let notify = Arc::new(Notify::new());

    let mut d = ActionDispatcher::new();
    d.on(|_req: HeartbeatRequest| async move { Ok(heartbeat_response()) });

    let f = fired.clone();
    let n = notify.clone();
    d.after(move |_req: HeartbeatRequest| {
        let f = f.clone();
        let n = n.clone();
        async move {
            f.fetch_add(1, Ordering::SeqCst);
            n.notify_one();
        }
    });

    // Route map built (an @on + its @after): the hook must not have fired.
    assert!(d.has_handler("Heartbeat"));
    assert_eq!(
        fired.load(Ordering::SeqCst),
        0,
        "an @after hook must not fire merely from being registered"
    );

    // A successful dispatch spawns the hook; wait (bounded) for it to run.
    let _ = d.dispatch(&heartbeat_call()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("the @after hook must fire after a successful dispatch");
    assert_eq!(
        fired.load(Ordering::SeqCst),
        1,
        "the @after hook fires exactly once, only after dispatch"
    );
}
