//! End-to-end `ChargePoint::call()` CALLRESULT / CALLERROR / timeout contract —
//! ports the mobilityhouse/ocpp reference's
//! [`tests/v16/test_v16_charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v16/test_v16_charge_point.py)
//! (`test_raise_call_error`, `test_suppress_call_error`,
//! `test_send_call_with_timeout`), backed by the `ChargePoint.call()` logic in
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
//!
//! ## Why this suite exists (Issue #321)
//!
//! The companion [`client_call_error_timeout`] suite pins the same three
//! behaviours through the **public primitives** `call()` composes
//! (`PendingCallMap` + `tokio::time::timeout`), which faithfully pins the
//! *pieces* but not `ChargePoint::call()` itself: the real method mints a random
//! `unique_id` internally and requires a live transport, so a regression in its
//! own await/timeout wiring would slip past a primitive-level test.
//!
//! The reference drives the actual `ChargePoint.call()` through a mock
//! connection. The `ocpp-conformance` crate already has the higher-fidelity
//! equivalent: an **in-process loopback** harness (`OcppServer` +
//! `DispatchHandler` + a real [`ChargePoint`] over `ws://127.0.0.1:0`, as in
//! `full_session.rs`). Standing that up here drives the **real**
//! `ChargePoint::call()` end-to-end over a real WebSocket and the real transport
//! recv loop — the CALL is framed, sent, correlated by `unique_id`, and the
//! peer's reply is routed back — with **zero** production-code change. No new
//! `Transport`-trait / duplex seam is needed (the two options sketched in #321);
//! the loopback server *is* the seam.
//!
//! The test never needs to observe the generated `unique_id`: correlation is
//! exercised *by the real recv loop*, so `call()` returning the correct result
//! per case is the proof the id was matched.
//!
//! ## What each test pins
//!
//!   - [`call_returns_ok_response_on_callresult`] — a `CALLRESULT` resolves
//!     `call()` as `Ok(Response)`.
//!   - [`callerror_from_peer_surfaces_as_ocpp_call_error`] — a `CALLERROR`
//!     resolves `call()` as `Err(OcppError::CallError)` with the wire `code` +
//!     `description` intact (the reference's `test_raise_call_error`; the
//!     `suppress=True` swallow is the caller's `.ok()` choice in Rust, pinned in
//!     the companion suite).
//!   - [`no_reply_times_out_without_wedging_subsequent_calls`] — no reply
//!     resolves `call()` as `Err(OcppError::Timeout)`, and a *subsequent*
//!     `call()` on the same connection still succeeds — the Rust analog of "the
//!     call lock is released" ([mobilityhouse/ocpp#46](https://github.com/mobilityhouse/ocpp/issues/46)).

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, DataTransferRequest, HeartbeatRequest,
    HeartbeatResponse, RegistrationStatus, StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::{ActionDispatcher, Message};
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, MessageHandler, TransportConfig, TransportEvent};
use ocpp_types::{CallErrorCode, OcppError, OcppResult};

/// A CSMS message handler that routes through a real [`ActionDispatcher`] but
/// *swallows* a designated set of actions — receiving the CALL yet replying with
/// no frame at all (`Ok(None)`), so the peer's `call()` runs out its timeout
/// while the connection stays live for subsequent calls.
///
/// This is the one piece the stock [`DispatchHandler`] can't express: it always
/// answers a CALL with a CALLRESULT or CALLERROR, never silence. Keeping the
/// swallow logic here (test-only) leaves the production transport path
/// untouched.
struct ScriptedCsms {
    inner: DispatchHandler,
    swallow: HashSet<String>,
}

#[async_trait]
impl MessageHandler for ScriptedCsms {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        if let Message::Call(call) = &message {
            if self.swallow.contains(&call.action) {
                // Received but deliberately unanswered → the client times out.
                return Ok(None);
            }
        }
        self.inner.handle_message(message).await
    }

    async fn handle_event(&self, _event: TransportEvent) {}
}

/// A dispatcher that accepts the boot handshake and the connector status
/// announcements a fresh `ChargePoint` emits during `connect()`, so every test
/// starts from a cleanly-booted session. Callers add the per-test `Heartbeat`
/// behaviour on top.
///
/// The BootNotification advertises a long heartbeat interval so no background
/// `Heartbeat` races the per-test assertions.
fn base_dispatcher() -> ActionDispatcher {
    let mut d = ActionDispatcher::new();

    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: Utc::now(),
            interval: 3600,
            status: RegistrationStatus::Accepted,
        })
    });

    d.on(|_req: StatusNotificationRequest| async move { Ok(StatusNotificationResponse {}) });

    d
}

/// Start an in-process CSMS using `dispatcher`, swallowing (never replying to)
/// each action in `swallow`. Returns the server and its bound loopback addr.
async fn start_csms(dispatcher: ActionDispatcher, swallow: &[&str]) -> (OcppServer, SocketAddr) {
    let handler = Arc::new(ScriptedCsms {
        inner: DispatchHandler::new(Arc::new(dispatcher)),
        swallow: swallow.iter().map(|s| s.to_string()).collect(),
    });
    let (mut server, _events) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr)
}

fn cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        // A short per-call timeout keeps the timeout test quick while leaving
        // ample headroom for a loopback boot round-trip.
        call_timeout: 2,
        // Deterministic: no background reconnect storm racing the assertions.
        auto_reconnect: false,
        ..ChargePointConfig::default()
    }
}

/// Boot a real `ChargePoint` against `addr` and return it, connected.
async fn booted_cp(addr: SocketAddr, id: &str) -> ChargePoint {
    let cp = ChargePoint::new(cp_config(addr, id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    cp
}

/// A `CALLRESULT` for an outstanding `call()` resolves it as `Ok(Response)` —
/// the real method frames the CALL, the CSMS's `@on` handler replies, and the
/// recv loop deserializes the matching `HeartbeatResponse` back to the caller.
#[tokio::test]
async fn call_returns_ok_response_on_callresult() {
    let mut d = base_dispatcher();
    d.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: Utc::now(),
        })
    });
    let (mut server, addr) = start_csms(d, &[]).await;

    let cp = booted_cp(addr, "CP_CALL_OK").await;

    let resp = cp
        .call(HeartbeatRequest {})
        .await
        .expect("a CALLRESULT must resolve call() as Ok");
    // A well-formed HeartbeatResponse carries the CSMS clock; deserializing it
    // into the typed (non-optional) `current_time` is the success assertion.
    assert!(
        resp.current_time <= Utc::now(),
        "CSMS clock should not be in the future"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// A `CALLERROR` for an outstanding `call()` resolves it as
/// `Err(OcppError::CallError)` with the wire `error_code` + `error_description`
/// intact. The CSMS handler returns `Err(OcppError::Internal { .. })`, which the
/// server frames as a real `InternalError` CALLERROR; the CP's recv loop turns
/// it back into `OcppError::CallError` via `From<&CallErrorMessage>`.
#[tokio::test]
async fn callerror_from_peer_surfaces_as_ocpp_call_error() {
    const CAUSE: &str = "central system unavailable";

    let mut d = base_dispatcher();
    d.on(|_req: HeartbeatRequest| async move {
        Err::<HeartbeatResponse, _>(OcppError::Internal {
            message: CAUSE.to_string(),
        })
    });
    let (mut server, addr) = start_csms(d, &[]).await;

    let cp = booted_cp(addr, "CP_CALL_ERR").await;

    let err = cp
        .call(HeartbeatRequest {})
        .await
        .expect_err("a CALLERROR must resolve call() as Err");

    match err {
        OcppError::CallError {
            code, description, ..
        } => {
            assert_eq!(
                code,
                CallErrorCode::InternalError,
                "the wire error code must be preserved through the recv loop"
            );
            // `build_call_error` maps an `OcppError::Internal` through its
            // `Display` (`"Internal error: {message}"`); that text is what the
            // peer put on the wire and must round-trip verbatim.
            assert_eq!(
                description,
                format!("Internal error: {CAUSE}"),
                "the wire error description must be preserved intact"
            );
        }
        other => panic!("expected OcppError::CallError, got {other:?}"),
    }

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// A `call()` whose reply the peer swallows resolves as `OcppError::Timeout`,
/// and — critically — a *subsequent* `call()` on the same connection still
/// succeeds. This is the Rust analog of the reference's "the call lock is
/// released" ([mobilityhouse/ocpp#46](https://github.com/mobilityhouse/ocpp/issues/46)):
/// the per-call `oneshot` keying means a timed-out call leaves nothing that
/// could wedge a later one, proven here over the *real* transport rather than a
/// bare `PendingCallMap`.
#[tokio::test]
async fn no_reply_times_out_without_wedging_subsequent_calls() {
    let mut d = base_dispatcher();
    d.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: Utc::now(),
        })
    });
    // The CSMS receives DataTransfer CALLs but never replies to them.
    let (mut server, addr) = start_csms(d, &["DataTransfer"]).await;

    let cp = booted_cp(addr, "CP_CALL_TIMEOUT").await;

    // 1. A call whose reply is swallowed runs out the timeout.
    let timed_out = cp
        .call(DataTransferRequest {
            vendor_id: "com.example".to_string(),
            message_id: None,
            data: None,
        })
        .await;
    assert!(
        matches!(timed_out, Err(OcppError::Timeout { .. })),
        "a swallowed reply must surface as Timeout, got {timed_out:?}"
    );

    // 2. The client is not wedged: a subsequent call on the *same* connection
    //    completes normally (no shared lock was left held).
    cp.call(HeartbeatRequest {})
        .await
        .expect("a subsequent call() must still succeed after a prior timeout");

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
