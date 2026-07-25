//! Concurrent-dispatch conformance suite — ports the mobilityhouse/ocpp
//! reference's
//! [`tests/test_messages.py::test_validate_payload_threads`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py).
//!
//! ## What the reference pins
//!
//! `jsonschema` validation is CPU-bound, so the reference offloads it to a
//! thread-pool executor to keep it off the asyncio event loop.
//! `test_validate_payload_threads(use_threads)` parametrizes over the inline
//! path vs. the thread-pool path and asserts **validation produces identical
//! results regardless of where it runs** — a regression guard for that
//! offloading.
//!
//! ## The Rust analog
//!
//! Rust has no such off-loop shuffle: [`SchemaValidator`] is `Send + Sync` and
//! is injected into [`ActionDispatcher`], which the transport `DispatchHandler`
//! shares as an `Arc`. In production, many concurrent inbound CALLs are
//! validated **and** dispatched through *one shared validator* from many tokio
//! tasks. So the faithful analog of "same verdict inline vs. in a thread pool"
//! is "same verdict when driven concurrently through one `Arc`-shared validating
//! dispatcher as when driven serially." A regression that introduced interior
//! mutability, a non-`Sync` cache, or a lock that serialized/deadlocked
//! validation would break this — and nothing else in the suite exercises that
//! concurrency (the existing `spawn` usages are fire-and-forget `@after` hooks,
//! not concurrent `dispatch()`).
//!
//! These tests drive `TASKS` concurrent `dispatch()` calls through a single
//! `Arc<ActionDispatcher>` on a multi-threaded runtime and assert that valid
//! CALLs all get the identical correct CALLRESULT and invalid CALLs are all
//! rejected with the identical `SchemaViolation` keyword — concurrent
//! validation is byte-for-byte the same as it would be inline.
//!
//! Part of **M8 — Conformance** (Issue #394). Test-only; no production code.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::v16j::{BootNotificationRequest, BootNotificationResponse, RegistrationStatus};
use ocpp_messages::{ActionDispatcher, CallMessage};
use ocpp_types::{OcppError, SchemaKeyword};
use serde_json::{json, Value};

/// Number of CALLs dispatched concurrently through the one shared dispatcher.
const TASKS: u32 = 64;

/// A fixed `BootNotificationResponse` so every valid dispatch yields a
/// **byte-identical** CALLRESULT — the timestamp is pinned (not `Utc::now()`)
/// precisely so concurrent responses can be compared for exact equality.
fn fixed_boot_response() -> BootNotificationResponse {
    BootNotificationResponse {
        current_time: "2021-06-15T14:01:32Z".parse().expect("valid RFC3339"),
        interval: 300,
        status: RegistrationStatus::Accepted,
    }
}

/// Build an `Arc`-shared validating dispatcher (1.6J `SchemaValidator`) with a
/// `BootNotification` `@on` handler, plus the handler-invocation counter so a
/// test can assert the handler ran exactly as often as validation admitted —
/// i.e. that rejected CALLs never reached it, even under concurrency.
fn shared_dispatcher() -> (Arc<ActionDispatcher>, Arc<AtomicU32>) {
    let calls = Arc::new(AtomicU32::new(0));
    let c = calls.clone();

    let mut d = ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v16j()));
    d.on(move |_req: BootNotificationRequest| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(fixed_boot_response())
        }
    });
    (Arc::new(d), calls)
}

/// A well-formed `BootNotification` CALL — validates cleanly.
fn valid_call() -> CallMessage {
    CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "V", "chargePointModel": "M" }),
    )
    .unwrap()
}

/// A schema-invalid `BootNotification` CALL: the vendor is 21 chars, exceeding
/// the schema's `maxLength: 20`. It deserialises fine as a `String`, so this is
/// a fault **only the validator** catches (→ `MaxLength`) — exactly the seam the
/// shared validator owns.
fn invalid_call() -> CallMessage {
    CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "V".repeat(21), "chargePointModel": "M" }),
    )
    .unwrap()
}

/// All-valid: `TASKS` concurrent valid CALLs through one shared validating
/// dispatcher must every one produce the identical correct CALLRESULT, and the
/// handler must run exactly `TASKS` times (concurrent validation admits them all
/// and does not drop or duplicate any).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_valid_calls_get_identical_results() {
    let (d, calls) = shared_dispatcher();
    let golden: Value = serde_json::to_value(fixed_boot_response()).unwrap();

    let mut handles = Vec::new();
    for _ in 0..TASKS {
        let d = d.clone();
        handles.push(tokio::spawn(async move { d.dispatch(&valid_call()).await }));
    }

    for h in handles {
        let resp = h
            .await
            .expect("task panicked")
            .expect("valid CALL must dispatch");
        assert_eq!(
            resp, golden,
            "every concurrent valid CALL must yield the identical CALLRESULT"
        );
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        TASKS,
        "the handler must run once per admitted CALL — no drops or duplicates"
    );
}

/// All-invalid: `TASKS` concurrent schema-invalid CALLs must every one be
/// rejected with the identical `SchemaViolation` keyword (`MaxLength`) — the
/// validator gives the same verdict under concurrency as it would inline — and
/// the handler must never run (validation short-circuits before it).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_invalid_calls_are_consistently_rejected() {
    let (d, calls) = shared_dispatcher();

    let mut handles = Vec::new();
    for _ in 0..TASKS {
        let d = d.clone();
        handles.push(tokio::spawn(
            async move { d.dispatch(&invalid_call()).await },
        ));
    }

    for h in handles {
        match h.await.expect("task panicked") {
            Err(OcppError::SchemaViolation { keyword, .. }) => assert_eq!(
                keyword,
                SchemaKeyword::MaxLength,
                "every concurrent invalid CALL must be rejected on the identical keyword"
            ),
            other => panic!("expected SchemaViolation(MaxLength), got {other:?}"),
        }
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a rejected CALL must never reach the handler, even under concurrency"
    );
}

/// Mixed: valid and invalid CALLs interleaved through the *same* shared
/// dispatcher must each get their own correct verdict — pinning that concurrent
/// validation does not cross-contaminate results (a valid CALL is never
/// mis-rejected, an invalid one never mis-accepted), and the handler runs
/// exactly once per valid CALL.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_mixed_calls_keep_their_own_verdict() {
    let (d, calls) = shared_dispatcher();
    let golden: Value = serde_json::to_value(fixed_boot_response()).unwrap();
    let expected_valid = TASKS / 2 + TASKS % 2; // even indices are valid

    let mut handles = Vec::new();
    for i in 0..TASKS {
        let d = d.clone();
        let golden = golden.clone();
        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                let resp = d
                    .dispatch(&valid_call())
                    .await
                    .expect("valid CALL must dispatch");
                assert_eq!(
                    resp, golden,
                    "a valid CALL must not be mis-rejected under concurrency"
                );
            } else {
                match d.dispatch(&invalid_call()).await {
                    Err(OcppError::SchemaViolation {
                        keyword: SchemaKeyword::MaxLength,
                        ..
                    }) => {}
                    other => panic!("an invalid CALL must not be mis-accepted, got {other:?}"),
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        expected_valid,
        "the handler must run exactly once per valid CALL and never for a rejected one"
    );
}
