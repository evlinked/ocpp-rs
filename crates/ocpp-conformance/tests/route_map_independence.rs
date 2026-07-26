//! Route-map independence across dispatcher instances — ports the
//! mobilityhouse/ocpp reference's
//! [`tests/test_charge_point.py::test_multiple_classes_with_same_name_for_handler`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point.py).
//!
//! ## What the reference pins
//!
//! ```python
//! def test_multiple_classes_with_same_name_for_handler():
//!     class ChargerA(cp_201):
//!         @on(Action.heartbeat)
//!         def heartbeat(self, **kwargs): pass
//!     class ChargerB(cp_201):
//!         @on(Action.heartbeat)
//!         def heartbeat(self, **kwargs): pass
//!     A, B = ChargerA("A", None), ChargerB("B", None)
//!     assert create_route_map(A)["Heartbeat"] != create_route_map(B)["Heartbeat"]
//! ```
//!
//! Two `ChargePoint` **subclasses** each declare a handler for the **same**
//! action name. The reference asserts their route-map entries for that action
//! are **distinct** — each class routes the action to *its own* method, with no
//! cross-class bleed. It guards a real Python footgun: `@on`/`@after` are
//! declared at class scope and `create_route_map()` walks the class, so a route
//! map kept in class-level (shared) state could let the second registration
//! overwrite or leak into the first.
//!
//! ## How the Rust model maps
//!
//! The Rust side has no class-level route table: each
//! [`ActionDispatcher`] owns an **instance** `HashMap` of handlers (and
//! `after_hooks`) keyed on `OcppAction::ACTION_NAME` — there is no `static`,
//! `OnceCell`, or `lazy_static` route registry shared across instances. So the
//! reference's "route entries differ per class" becomes the observable
//! invariant: two independent dispatchers, each with a handler registered for
//! the **identical** action name, must dispatch that action to *their own*
//! handler — never share routing state.
//!
//! This is structurally likely in Rust (per-instance maps), which is exactly
//! why it is worth pinning: a future refactor that introduced a shared/global
//! route registry (or a cache keyed only on action name) would be caught here.
//! It complements [`routing.rs::distinct_on_handlers_route_independently`]
//! (`test_create_route_map`), which pins a *different* invariant — two
//! *different* actions on *one* dispatcher. Nothing else pins the
//! cross-*instance*, **same-action-name** case.
//!
//! Part of **M8 — Conformance** (Issue #396). Test-only; no production code.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ocpp_messages::v16j::{BootNotificationRequest, BootNotificationResponse, RegistrationStatus};
use ocpp_messages::{ActionDispatcher, CallMessage};
use serde_json::json;
use tokio::sync::Notify;

// ─── helpers ────────────────────────────────────────────────────────────────

/// A `BootNotificationResponse` whose `interval` is the caller-chosen sentinel,
/// so each dispatcher's handler returns a value distinguishable from the other.
fn boot_response(interval: i32) -> BootNotificationResponse {
    BootNotificationResponse {
        current_time: chrono::Utc::now(),
        interval,
        status: RegistrationStatus::Accepted,
    }
}

/// One well-formed `BootNotification` CALL — the *identical* frame is dispatched
/// to both dispatchers, so any difference in outcome is attributable solely to
/// which instance's handler ran.
fn boot_call() -> CallMessage {
    CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "vendor", "chargePointModel": "model" }),
    )
    .unwrap()
}

// ─── @on: same action name on two instances routes per-instance ─────────────

/// Port of `test_multiple_classes_with_same_name_for_handler`'s core claim.
///
/// Two independent `ActionDispatcher`s each register an `@on` handler for the
/// **same** action (`BootNotification`) returning a **distinguishable** response
/// (`interval: 111` vs `222`). Dispatching the *identical* valid CALL to each
/// must reach that instance's own handler — the reference's
/// `route_mapA["Heartbeat"] != route_mapB["Heartbeat"]`, observed through the
/// dispatch contract rather than by comparing route-map objects.
#[tokio::test]
async fn same_action_name_routes_per_instance() {
    let mut a = ActionDispatcher::new();
    a.on(|_req: BootNotificationRequest| async move { Ok(boot_response(111)) });

    let mut b = ActionDispatcher::new();
    b.on(|_req: BootNotificationRequest| async move { Ok(boot_response(222)) });

    // Both instances route the same action, each via its own single handler.
    assert!(a.has_handler("BootNotification"));
    assert!(b.has_handler("BootNotification"));
    assert_eq!(a.handler_count(), 1);
    assert_eq!(b.handler_count(), 1);

    // The identical CALL, dispatched to each, reaches that instance's handler.
    let resp_a = a.dispatch(&boot_call()).await.unwrap();
    let resp_b = b.dispatch(&boot_call()).await.unwrap();

    assert_eq!(
        resp_a["interval"], 111,
        "dispatcher `a` must route BootNotification to its own handler"
    );
    assert_eq!(
        resp_b["interval"], 222,
        "dispatcher `b` must route BootNotification to its own handler"
    );
    assert_ne!(
        resp_a["interval"], resp_b["interval"],
        "the two instances' same-action routes must not share state (no cross-instance bleed)"
    );
}

// ─── @after: the post-response hook is per-instance too ─────────────────────

/// The reference's route-map entry bundles the `@after` hook alongside the `@on`
/// handler, so per-class independence covers hooks as well. This pins that
/// `after_hooks` is per-instance: dispatching to `a` fires **only** `a`'s hook,
/// never `b`'s, even though both are registered for the same action name.
///
/// Each hook writes a distinct sentinel into its own slot; dispatching the
/// identical CALL to one instance must set that instance's slot and leave the
/// other's untouched. A shared/global `after_hooks` table (second registration
/// overwriting the first) would be caught here.
#[tokio::test]
async fn after_hooks_are_per_instance() {
    let slot_a = Arc::new(Mutex::new(None::<u32>));
    let slot_b = Arc::new(Mutex::new(None::<u32>));
    let notify_a = Arc::new(Notify::new());
    let notify_b = Arc::new(Notify::new());

    let mut a = ActionDispatcher::new();
    a.on(|_req: BootNotificationRequest| async move { Ok(boot_response(111)) });
    {
        let sa = slot_a.clone();
        let na = notify_a.clone();
        a.after(move |_req: BootNotificationRequest| {
            let sa = sa.clone();
            let na = na.clone();
            async move {
                *sa.lock().unwrap() = Some(111);
                na.notify_one();
            }
        });
    }

    let mut b = ActionDispatcher::new();
    b.on(|_req: BootNotificationRequest| async move { Ok(boot_response(222)) });
    {
        let sb = slot_b.clone();
        let nb = notify_b.clone();
        b.after(move |_req: BootNotificationRequest| {
            let sb = sb.clone();
            let nb = nb.clone();
            async move {
                *sb.lock().unwrap() = Some(222);
                nb.notify_one();
            }
        });
    }

    // Dispatch only to `a`: its hook must fire; `b`'s must not.
    a.dispatch(&boot_call()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), notify_a.notified())
        .await
        .expect("dispatcher `a`'s after hook must fire after a successful dispatch");
    assert_eq!(
        *slot_a.lock().unwrap(),
        Some(111),
        "`a`'s dispatch must run `a`'s after hook"
    );
    assert_eq!(
        *slot_b.lock().unwrap(),
        None,
        "`a`'s dispatch must NOT run `b`'s after hook (per-instance hooks)"
    );

    // Now dispatch to `b`: its own hook fires.
    b.dispatch(&boot_call()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), notify_b.notified())
        .await
        .expect("dispatcher `b`'s after hook must fire after a successful dispatch");
    assert_eq!(
        *slot_b.lock().unwrap(),
        Some(222),
        "`b`'s dispatch must run `b`'s own after hook"
    );
}
