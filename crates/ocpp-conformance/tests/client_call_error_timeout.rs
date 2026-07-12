//! Client-side `call()` CALLERROR-response + timeout contract — ports the
//! mobilityhouse/ocpp reference's
//! [`tests/v16/test_v16_charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v16/test_v16_charge_point.py)
//! (`test_raise_call_error`, `test_suppress_call_error`,
//! `test_send_call_with_timeout`), backed by the `ChargePoint.call()` logic in
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
//!
//! ## What the reference pins
//!
//! The reference's `call()` awaits a `Future` keyed by the outgoing message's
//! `unique_id` and reacts to how that future resolves:
//!
//!   - `test_raise_call_error` — the peer answers with a **CALLERROR**; with
//!     `suppress=False`, `call()` raises an exception carrying the error code
//!     and description.
//!   - `test_suppress_call_error` — by default (`suppress=True`) the same
//!     CALLERROR is swallowed and `call()` returns `None`.
//!   - `test_send_call_with_timeout` — **no** reply arrives within the timeout,
//!     so `call()` raises `asyncio.TimeoutError`, **and the call lock is
//!     released** so a subsequent `call()` does not deadlock (see
//!     [mobilityhouse/ocpp#46](https://github.com/mobilityhouse/ocpp/issues/46)).
//!
//! ## How the Rust model maps
//!
//! `ocpp-cp`'s [`ChargePoint::call()`] (`crates/ocpp-cp/src/lib.rs`) composes
//! three public building blocks, all exercised here directly so the contract is
//! pinned without standing up a mock WebSocket:
//!
//!   1. [`PendingCallMap::register`] mints a `oneshot` receiver keyed by the
//!      CALL's `unique_id` *before* the frame is sent (race-free).
//!   2. The transport recv loop turns an inbound `CALLERROR` frame into an
//!      [`OcppError::CallError`] via `OcppError::from(&CallErrorMessage)` and
//!      calls [`PendingCallMap::reject`]; a `CALLRESULT` calls
//!      [`PendingCallMap::resolve`].
//!   3. `call()` awaits the receiver under `tokio::time::timeout`, mapping an
//!      elapsed timer to [`OcppError::Timeout`] and propagating whatever the
//!      map delivered otherwise.
//!
//! [`resolve_like_call`] below reproduces step 3 verbatim (a faithful port of
//! `call()`'s await-half at `lib.rs` §"Await the CALLRESULT (or CALLERROR)"),
//! so each test drives the *same* resolution logic the real client uses.
//!
//! ## Companion end-to-end suite
//!
//! This suite is the **unit-level** check: it pins each branch of `call()`'s
//! resolution against the primitives in isolation — including two the transport
//! can't easily reproduce, the disconnect→`Transport` branch
//! ([`dropped_sender_surfaces_as_transport_error`]) and the virtual-time timer
//! ([`timeout_does_not_wedge_subsequent_calls`], `start_paused`). The
//! higher-fidelity companion, `charge_point_call_e2e.rs`, drives the **real**
//! `ChargePoint::call()` end-to-end over an in-process loopback `OcppServer`
//! (framing, `unique_id` correlation, and the recv loop's CALLERROR translation
//! all exercised for real). Keep both: this one for the isolated branches, the
//! e2e one for the integrated path (Issue #321).
//!
//! ## Idiomatic divergences from the Python reference (pinned, not ported)
//!
//!   - **`suppress` flag.** Rust's `call()` returns `OcppResult<Response>`, so
//!     there is no runtime `suppress` parameter: propagating vs. discarding the
//!     `Err` is the *caller's* choice at the `?`/`.ok()` site.
//!     `caller_may_discard_call_error` pins that mapping.
//!   - **`_call_lock`.** The reference serialises every `call()` behind one
//!     global lock, which is why a timed-out call must explicitly release it
//!     (#46). Rust keys each in-flight call by its own `oneshot` in
//!     [`PendingCallMap`], so there is no shared lock to wedge — a timed-out
//!     call cannot block a later one. `timeout_does_not_wedge_subsequent_calls`
//!     pins the observable equivalent.

use ocpp_transport::PendingCallMap;
use ocpp_types::message::CallErrorMessage;
use ocpp_types::{CallErrorCode, OcppError, OcppResult};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::oneshot;

/// Faithful port of the await-half of [`ChargePoint::call()`]
/// (`crates/ocpp-cp/src/lib.rs`): await the pending `oneshot` under a timeout,
/// mapping an elapsed timer to [`OcppError::Timeout`], a dropped sender
/// (disconnect) to [`OcppError::Transport`], and otherwise propagating whatever
/// the [`PendingCallMap`] delivered — a successful `CALLRESULT` payload or the
/// [`OcppError::CallError`] built from a `CALLERROR`.
async fn resolve_like_call(
    rx: oneshot::Receiver<OcppResult<Value>>,
    timeout: Duration,
    action: &str,
) -> OcppResult<Value> {
    let raw_result = tokio::time::timeout(timeout, rx)
        .await
        .map_err(|_| OcppError::Timeout {
            operation: format!("{action} call"),
        })?
        .map_err(|_| OcppError::Transport {
            message: "Connection closed while waiting for CALLRESULT".to_string(),
        })?;
    raw_result
}

// ─── CALLERROR → OcppError::CallError (ports test_raise_call_error) ──────────

/// An inbound `CALLERROR` for an outstanding `call()` surfaces as
/// [`OcppError::CallError`] with the wire `error_code`, `error_description`, and
/// `error_details` preserved verbatim — the Rust analog of the reference
/// raising with the code + description intact.
#[tokio::test]
async fn callerror_surfaces_as_ocpp_call_error() {
    let map = PendingCallMap::new();
    let unique_id = "call-boot-1".to_string();
    let rx = map.register(unique_id.clone());

    // The frame exactly as the peer puts it on the wire.
    let frame = CallErrorMessage::new(
        unique_id.clone(),
        CallErrorCode::InternalError,
        "central system unavailable".to_string(),
        Some(json!({"retryAfter": 30})),
    );

    // Mirror the transport recv loop: translate via the real seam and reject
    // the pending call by its unique_id.
    assert!(
        map.reject(&frame.unique_id, OcppError::from(&frame)),
        "reject must find the registered unique_id"
    );

    let result = resolve_like_call(rx, Duration::from_secs(30), "BootNotification").await;

    match result {
        Err(OcppError::CallError {
            code,
            description,
            details,
        }) => {
            assert_eq!(code, CallErrorCode::InternalError);
            assert_eq!(description, "central system unavailable");
            assert_eq!(details, json!({"retryAfter": 30}));
        }
        other => panic!("expected OcppError::CallError, got {other:?}"),
    }
}

/// The error code round-trips independently of the description text: a
/// `SecurityError` CALLERROR does not get flattened into a generic variant.
#[tokio::test]
async fn callerror_preserves_the_specific_error_code() {
    let map = PendingCallMap::new();
    let rx = map.register("call-2".to_string());

    let frame = CallErrorMessage::new(
        "call-2".to_string(),
        CallErrorCode::SecurityError,
        "signature check failed".to_string(),
        None,
    );
    map.reject(&frame.unique_id, OcppError::from(&frame));

    let err = resolve_like_call(rx, Duration::from_secs(30), "Authorize")
        .await
        .expect_err("a CALLERROR must resolve the call as Err");

    assert!(
        matches!(
            err,
            OcppError::CallError {
                code: CallErrorCode::SecurityError,
                ..
            }
        ),
        "got {err:?}"
    );
}

// ─── suppress = caller discards the Err (ports test_suppress_call_error) ─────

/// The reference's `suppress=True` (swallow the CALLERROR) maps to a *caller
/// choice* in Rust, not a runtime flag: because `call()` yields a `Result`, the
/// caller decides whether to propagate the `Err` or drop it with `.ok()`. This
/// pins that discarding a `CallError` yields `None` while a success passes
/// through — the two branches the reference's `suppress` switch selects between.
#[tokio::test]
async fn caller_may_discard_call_error() {
    /// The idiomatic "suppress": the caller keeps only the success value.
    fn suppress<T>(result: OcppResult<T>) -> Option<T> {
        result.ok()
    }

    // A CALLERROR: suppressing yields None (the reference's `suppress=True`
    // return of `None`).
    let map = PendingCallMap::new();
    let rx = map.register("call-err".to_string());
    let frame = CallErrorMessage::new(
        "call-err".to_string(),
        CallErrorCode::GenericError,
        "boom".to_string(),
        None,
    );
    map.reject(&frame.unique_id, OcppError::from(&frame));
    let errored = resolve_like_call(rx, Duration::from_secs(30), "Heartbeat").await;
    assert!(errored.is_err(), "sanity: the call resolved as Err");
    assert!(
        suppress(errored).is_none(),
        "suppressing a CALLERROR must discard it (analog of suppress=True → None)"
    );

    // A CALLRESULT still flows through the same suppression untouched.
    let rx_ok = map.register("call-ok".to_string());
    assert!(map.resolve("call-ok", json!({"currentTime": "2026-07-12T00:00:00Z"})));
    let succeeded = resolve_like_call(rx_ok, Duration::from_secs(30), "Heartbeat").await;
    assert_eq!(
        suppress(succeeded),
        Some(json!({"currentTime": "2026-07-12T00:00:00Z"})),
    );
}

// ─── timeout → OcppError::Timeout + no wedge (ports test_send_call_with_timeout)

/// A `call()` whose response never arrives resolves as [`OcppError::Timeout`],
/// and — critically — a *subsequent* call on the same [`PendingCallMap`] still
/// completes. This is the Rust analog of the reference's "the call lock is
/// released" ([mobilityhouse/ocpp#46](https://github.com/mobilityhouse/ocpp/issues/46)):
/// per-call `oneshot` keying means a timed-out call leaves nothing that could
/// block a later one.
///
/// `start_paused` lets the timeout fire against virtual time, so the test does
/// not actually wait out the timeout wall-clock.
#[tokio::test(start_paused = true)]
async fn timeout_does_not_wedge_subsequent_calls() {
    let map = PendingCallMap::new();

    // First call: register but never resolve → the timer elapses.
    let rx_a = map.register("call-A".to_string());
    let timed_out = resolve_like_call(rx_a, Duration::from_secs(2), "StatusNotification").await;
    assert!(
        matches!(timed_out, Err(OcppError::Timeout { .. })),
        "a call with no reply must resolve as Timeout, got {timed_out:?}"
    );

    // The stale "call-A" entry is harmless: its receiver is gone, so a late
    // resolve is a no-op (`false`) rather than a panic or wedge.
    assert!(
        !map.resolve("call-A", json!({})),
        "late resolve of a timed-out call must be a harmless no-op"
    );

    // Second call on the *same* map still works end-to-end — no shared lock was
    // left held (the observable equivalent of the reference's lock release).
    let rx_b = map.register("call-B".to_string());
    assert!(map.resolve("call-B", json!({"status": "Accepted"})));
    let succeeded = resolve_like_call(rx_b, Duration::from_secs(2), "StatusNotification").await;
    assert_eq!(
        succeeded.expect("the follow-up call must succeed after a prior timeout"),
        json!({"status": "Accepted"}),
    );
}

/// A disconnect (sender dropped without resolve/reject, e.g. via
/// [`PendingCallMap::cancel_all`]) surfaces as [`OcppError::Transport`], not a
/// silent hang — pinning the third branch of `call()`'s await mapping.
#[tokio::test]
async fn dropped_sender_surfaces_as_transport_error() {
    let map = PendingCallMap::new();
    let rx = map.register("call-drop".to_string());

    // Simulate a disconnect: all pending senders are dropped.
    map.cancel_all();

    let err = resolve_like_call(rx, Duration::from_secs(30), "Heartbeat")
        .await
        .expect_err("a dropped sender must resolve the call as Err");
    assert!(
        matches!(err, OcppError::Transport { .. }),
        "expected Transport, got {err:?}"
    );
}
