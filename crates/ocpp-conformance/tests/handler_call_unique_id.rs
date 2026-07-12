//! Ports the reference's
//! [`tests/test_charge_point.py::test_call_unique_id_added_to_handler_args_correctly`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point.py),
//! backed by `_handle_call()` in
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
//!
//! ## What the reference pins
//!
//! A handler (`@on` or `@after`) that declares a `call_unique_id` parameter is
//! passed the triggering CALL's `unique_id`; one that does not is called without
//! it. The reference pins **both directions across two chargers**:
//!
//!   - **ChargerA** — `@on` *without* `call_unique_id` (sees only the payload),
//!     `@after` *with* `call_unique_id` (must equal the CALL's id).
//!   - **ChargerB** — `@on` *with* `call_unique_id` (must equal the CALL's id),
//!     `@after` *without* it.
//!
//! and that all four handlers run exactly once.
//!
//! ## How the Rust port expresses the opt-in (Issue #317)
//!
//! Rust has no runtime signature reflection, so the Python "declare the param to
//! opt in" mechanism does not map directly. The [`ActionDispatcher`] expresses
//! the opt-in by *choosing the builder*: [`ActionDispatcher::on_with_id`] /
//! [`ActionDispatcher::after_with_id`] receive `(Req, unique_id: String)`, while
//! the plain [`ActionDispatcher::on`] / [`ActionDispatcher::after`] receive only
//! `Req` — never the id. `dispatch()` threads `call.unique_id` to both paths;
//! the plain erasures ignore it. This suite pins that observable contract exactly
//! as the reference's ChargerA / ChargerB do.
//!
//! Part of **M8 — Conformance**. Test-only; the production change lives in
//! `crates/ocpp-messages/src/dispatcher.rs`.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use ocpp_messages::v16j::{BootNotificationRequest, BootNotificationResponse, RegistrationStatus};
use ocpp_messages::{ActionDispatcher, CallMessage};
use serde_json::json;
use tokio::sync::mpsc;

/// The fixed CALLRESULT payload both chargers' `@on` handlers return — the
/// analog of the reference handlers' `return call_result.BootNotification(...)`.
fn boot_response() -> BootNotificationResponse {
    BootNotificationResponse {
        current_time: chrono::Utc::now(),
        interval: 300,
        status: RegistrationStatus::Accepted,
    }
}

/// A well-formed `BootNotification` CALL with a caller-chosen `unique_id`, so the
/// test can assert an id-aware handler sees *that exact* id (the analog of the
/// reference's `charger_a_test_call_unique_id` / `charger_b_test_call_unique_id`).
fn boot_call(unique_id: &str, vendor: &str, model: &str) -> CallMessage {
    CallMessage::with_id(
        unique_id.to_string(),
        "BootNotification".to_string(),
        json!({ "chargePointVendor": vendor, "chargePointModel": model }),
    )
    .unwrap()
}

// ─── ChargerA: @on without the id, @after with it ───────────────────────────

/// Ports **ChargerA**: the `@on` handler is *not* id-aware (it only asserts the
/// deserialised payload — the reference's `assert kwargs == camel_to_snake_case(
/// payload_a)`), while the `@after` handler *is* id-aware and must receive the
/// triggering CALL's exact `unique_id`. Both fire exactly once.
#[tokio::test]
async fn charger_a_after_opts_into_unique_id_on_does_not() {
    const A_ID: &str = "charger-a-test-call-unique-id";

    // The `@on` handler runs inline (awaited by `dispatch`), so capture what it
    // observed and assert in the test body. The `@after` hook is *spawned*, so a
    // panic inside it would not fail the test — send its observation over a
    // channel and assert in the body instead.
    let on_seen_vendor = Arc::new(Mutex::new(None::<String>));
    let on_count = Arc::new(AtomicU32::new(0));
    let after_count = Arc::new(AtomicU32::new(0));
    let (after_tx, mut after_rx) = mpsc::unbounded_channel::<String>();

    let mut charger_a = ActionDispatcher::new();

    // @on(boot_notification) — plain: sees only the payload, never the id.
    {
        let vendor = on_seen_vendor.clone();
        let count = on_count.clone();
        charger_a.on(move |req: BootNotificationRequest| {
            let vendor = vendor.clone();
            let count = count.clone();
            async move {
                *vendor.lock().unwrap() = Some(req.charge_point_vendor.clone());
                count.fetch_add(1, Ordering::SeqCst);
                Ok(boot_response())
            }
        });
    }

    // @after(boot_notification) — id-aware: receives the triggering CALL's id.
    {
        let tx = after_tx.clone();
        let count = after_count.clone();
        charger_a.after_with_id(move |_req: BootNotificationRequest, unique_id: String| {
            let tx = tx.clone();
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                let _ = tx.send(unique_id);
            }
        });
    }

    charger_a
        .dispatch(&boot_call(A_ID, "VendorA", "ModelA"))
        .await
        .expect("dispatch must succeed");

    // The plain @on handler ran once and saw the payload — never the id.
    assert_eq!(
        on_count.load(Ordering::SeqCst),
        1,
        "the @on handler must run exactly once"
    );
    assert_eq!(
        on_seen_vendor.lock().unwrap().as_deref(),
        Some("VendorA"),
        "the plain @on handler still deserialises the payload"
    );

    // The id-aware @after hook ran once and received the exact triggering id.
    let got = tokio::time::timeout(Duration::from_secs(5), after_rx.recv())
        .await
        .expect("the @after hook must fire")
        .expect("the @after hook must send the id it received");
    assert_eq!(
        got, A_ID,
        "after_with_id must receive the triggering CALL's exact unique_id"
    );
    assert_eq!(
        after_count.load(Ordering::SeqCst),
        1,
        "the @after hook must fire exactly once"
    );
}

// ─── ChargerB: @on with the id, @after without it ───────────────────────────

/// Ports **ChargerB** (the mirror of ChargerA): the `@on` handler *is* id-aware
/// and must receive the triggering CALL's exact `unique_id`, while the `@after`
/// hook is *not* id-aware. Both fire exactly once. This pins that the opt-in is
/// per-builder and works on the `@on` path too — not only `@after`.
#[tokio::test]
async fn charger_b_on_opts_into_unique_id_after_does_not() {
    const B_ID: &str = "charger-b-test-call-unique-id";

    let on_seen_id = Arc::new(Mutex::new(None::<String>));
    let on_seen_vendor = Arc::new(Mutex::new(None::<String>));
    let on_count = Arc::new(AtomicU32::new(0));
    let after_count = Arc::new(AtomicU32::new(0));
    let (after_tx, mut after_rx) = mpsc::unbounded_channel::<()>();

    let mut charger_b = ActionDispatcher::new();

    // @on(boot_notification) — id-aware: receives the triggering CALL's id.
    {
        let seen_id = on_seen_id.clone();
        let seen_vendor = on_seen_vendor.clone();
        let count = on_count.clone();
        charger_b.on_with_id(move |req: BootNotificationRequest, unique_id: String| {
            let seen_id = seen_id.clone();
            let seen_vendor = seen_vendor.clone();
            let count = count.clone();
            async move {
                *seen_id.lock().unwrap() = Some(unique_id);
                *seen_vendor.lock().unwrap() = Some(req.charge_point_vendor.clone());
                count.fetch_add(1, Ordering::SeqCst);
                Ok(boot_response())
            }
        });
    }

    // @after(boot_notification) — plain: never sees the id.
    {
        let tx = after_tx.clone();
        let count = after_count.clone();
        charger_b.after(move |_req: BootNotificationRequest| {
            let tx = tx.clone();
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                let _ = tx.send(());
            }
        });
    }

    charger_b
        .dispatch(&boot_call(B_ID, "VendorB", "ModelB"))
        .await
        .expect("dispatch must succeed");

    // The id-aware @on handler ran once, saw the exact id AND the payload.
    assert_eq!(
        on_count.load(Ordering::SeqCst),
        1,
        "the @on handler must run exactly once"
    );
    assert_eq!(
        on_seen_id.lock().unwrap().as_deref(),
        Some(B_ID),
        "on_with_id must receive the triggering CALL's exact unique_id"
    );
    assert_eq!(
        on_seen_vendor.lock().unwrap().as_deref(),
        Some("VendorB"),
        "the id-aware @on handler still deserialises the payload"
    );

    // The plain @after hook fired exactly once, unaffected by the `on` opt-in.
    tokio::time::timeout(Duration::from_secs(5), after_rx.recv())
        .await
        .expect("the plain @after hook must fire")
        .expect("the plain @after hook must send its signal");
    assert_eq!(
        after_count.load(Ordering::SeqCst),
        1,
        "the @after hook must fire exactly once"
    );
}
