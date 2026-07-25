//! Inbound CALLERROR with an **out-of-spec `error_code`** must resolve the
//! correlated pending CALL *promptly*, not drop the frame and let the caller
//! hang to its call-timeout — issue #381.
//!
//! ## What the reference pins
//!
//! In the mobilityhouse/ocpp reference a `CallError` frame **always unpacks**
//! ([`ocpp/messages.py::CallError`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py));
//! only `CallError.to_exception()` distinguishes a recognized code (→ the
//! matching `OCPPError` subclass) from an unknown one (→ `UnknownCallErrorCodeError`).
//! Crucially, `ChargePoint._get_specific_response()` resolves the pending future
//! by `unique_id` **before** `to_exception()` runs
//! ([`ocpp/charge_point.py::_handle_call_error`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)):
//! the outstanding call is always *resolved with an error*, never left dangling.
//!
//! ## The Rust bug this pins
//!
//! Both live transport recv loops decode straight into the strict serde enum
//! `CallErrorCode`, which has no fallback variant — so an unknown `error_code`
//! fails the **whole-frame** decode. The client's recv loop
//! (`crates/ocpp-transport/src/client.rs`) drops it in its `Err(_)` arm, and the
//! server's `classify_frame` (`crates/ocpp-transport/src/server.rs`) swallows the
//! failure with `.ok()` and returns `None`. Either way the pending CALL keyed by
//! that `unique_id` is never rejected, so `call().await` blocks until its timeout
//! fires — surfacing a misleading [`OcppError::Timeout`] instead of a prompt,
//! correlated error carrying the peer's code.
//!
//! ## How this suite maps
//!
//! Like the companion [`client_call_error_timeout`] suite, these tests pin the
//! **recv-loop translation seam** against the public primitives it composes,
//! without standing up a mock WebSocket: the recv loop feeds an undecodable
//! frame's text to [`recover_inbound_call_error`] (the exact call the live
//! `client.rs`/`server.rs` paths now make) and rejects the pending call by the
//! recovered `unique_id`. [`resolve_like_call`] reproduces the await-half of
//! `ChargePoint::call()` / `OcppServer::call()` verbatim, so each test drives the
//! same resolution logic the real endpoints use. The wire text is built as the
//! live transport serializes it (object-form `Message`), so the tolerant recovery
//! is exercised against a real frame.

use ocpp_transport::PendingCallMap;
use ocpp_types::{recover_inbound_call_error, CallErrorCode, OcppError, OcppResult};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::oneshot;

/// Faithful port of the await-half of `ChargePoint::call()` / `OcppServer::call()`:
/// await the pending `oneshot` under a timeout, mapping an elapsed timer to
/// [`OcppError::Timeout`] and otherwise propagating whatever the
/// [`PendingCallMap`] delivered. Identical to the companion suite's helper.
async fn resolve_like_call(
    rx: oneshot::Receiver<OcppResult<Value>>,
    timeout: Duration,
    action: &str,
) -> OcppResult<Value> {
    tokio::time::timeout(timeout, rx)
        .await
        .map_err(|_| OcppError::Timeout {
            operation: format!("{action} call"),
        })?
        .map_err(|_| OcppError::Transport {
            message: "Connection closed while waiting for CALLRESULT".to_string(),
        })?
}

/// Object-form CALLERROR wire text, exactly as the live transport serializes a
/// `Message::CallError` (`{"0":"CALLERROR","1":…,"2":code,"3":desc,"4":details}`).
fn wire_call_error(unique_id: &str, code: &str, desc: &str, details: Value) -> String {
    serde_json::to_string(&json!({
        "0": "CALLERROR",
        "1": unique_id,
        "2": code,
        "3": desc,
        "4": details,
    }))
    .unwrap()
}

/// Reproduce the transport recv loop's new fallback: an undecodable inbound frame
/// is handed to [`recover_inbound_call_error`], and if it is a correlatable
/// CALLERROR the pending call is rejected by the recovered `unique_id`. Returns
/// whether a pending call was rejected (mirrors the `reject` bool the loops log on).
fn recv_loop_reject(map: &PendingCallMap, text: &str) -> bool {
    match recover_inbound_call_error(text) {
        Some((unique_id, err)) => map.reject(&unique_id, err),
        None => false,
    }
}

// ─── client path: unknown code resolves promptly, not as Timeout ────────────

/// The primary regression. A client with an outstanding CALL receives a
/// CALLERROR whose `error_code` (`418`) is outside the 12-member spec set.
/// `call().await` must resolve **promptly** as [`OcppError::ProtocolViolation`]
/// — *not* wait out the call-timeout and surface [`OcppError::Timeout`].
#[tokio::test(start_paused = true)]
async fn client_unknown_code_resolves_promptly_not_timeout() {
    let map = PendingCallMap::new();
    let unique_id = "call-boot-1".to_string();
    let rx = map.register(unique_id.clone());

    let frame = wire_call_error(&unique_id, "418", "I'm a teapot", json!({}));
    assert!(
        recv_loop_reject(&map, &frame),
        "recv loop must reject the pending call for an unknown-code CALLERROR"
    );

    // A generous timeout: if the frame were dropped (the bug) this would elapse
    // in virtual time and surface Timeout. Instead the reject already fired.
    let result = resolve_like_call(rx, Duration::from_secs(30), "BootNotification").await;
    match result {
        Err(OcppError::ProtocolViolation { message }) => {
            assert!(
                message.contains("418"),
                "code should be surfaced: {message}"
            );
        }
        other => panic!("expected a prompt ProtocolViolation, got {other:?}"),
    }
}

/// Regression guard: a *recognized* code on the same recovery path still surfaces
/// as the typed [`OcppError::CallError`] with the wire code/description/details
/// intact — unknown-code tolerance must not flatten known codes.
#[tokio::test]
async fn client_known_code_still_surfaces_typed_call_error() {
    let map = PendingCallMap::new();
    let rx = map.register("call-2".to_string());

    let frame = wire_call_error(
        "call-2",
        "InternalError",
        "central system unavailable",
        json!({"retryAfter": 30}),
    );
    assert!(recv_loop_reject(&map, &frame));

    let err = resolve_like_call(rx, Duration::from_secs(30), "Authorize")
        .await
        .expect_err("a CALLERROR must resolve the call as Err");
    assert_eq!(
        err,
        OcppError::CallError {
            code: CallErrorCode::InternalError,
            description: "central system unavailable".to_string(),
            details: json!({"retryAfter": 30}),
        }
    );
}

// ─── server path: a CSMS-initiated CALL awaiting a CP CALLERROR ─────────────

/// The server-side mirror: a CSMS-initiated CALL awaiting a CP's CALLERROR with
/// an out-of-spec code must also reject promptly (no hang). The recovery seam is
/// identical (`server.rs`'s `classify_frame`-None fallback), so the same helper
/// drives it — the point is that `recover_inbound_call_error` no longer returns
/// the "uncorrelatable" `None` that left the CSMS call hanging.
#[tokio::test(start_paused = true)]
async fn server_unknown_code_rejects_pending_csms_call() {
    let map = PendingCallMap::new();
    let unique_id = "csms-reset-9".to_string();
    let rx = map.register(unique_id.clone());

    let frame = wire_call_error(
        &unique_id,
        "VendorSpecificOops",
        "custom failure",
        json!({}),
    );
    assert!(
        recv_loop_reject(&map, &frame),
        "server recv loop must reject the in-flight CSMS CALL, not drop the frame"
    );

    let result = resolve_like_call(rx, Duration::from_secs(30), "Reset").await;
    assert!(
        matches!(result, Err(OcppError::ProtocolViolation { .. })),
        "expected a prompt ProtocolViolation, got {result:?}"
    );
}

/// A genuinely undecodable, non-CALLERROR frame is still treated as malformed:
/// recovery declines it, so no spurious pending call is rejected.
#[tokio::test]
async fn undecodable_non_call_error_frame_is_not_recovered() {
    let map = PendingCallMap::new();
    let _rx = map.register("live".to_string());
    assert!(
        !recv_loop_reject(&map, r#"{"garbage": true}"#),
        "a non-CALLERROR frame must not reject any pending call"
    );
    assert_eq!(map.len(), 1, "the live pending call is untouched");
}
