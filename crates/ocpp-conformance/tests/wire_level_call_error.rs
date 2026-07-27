//! Wire-level end-to-end recovery of an **out-of-spec CALLERROR** through both
//! live transport recv loops — the integration companion to the primitive-level
//! `call_error_unknown_code` / `client_call_error_timeout` suites and the fix
//! shipped in #381 / PR #382.
//!
//! ## What this pins that nothing else does (Issue #383)
//!
//! `ocpp_types::recover_inbound_call_error` (unit-tested in
//! `crates/ocpp-types/src/message.rs`) turns a CALLERROR whose `error_code` is
//! outside the 12-member [`CallErrorCode`] set — untrusted peer input: a
//! vendor-specific code, a forward-compat code from a newer OCPP revision, or a
//! buggy/malicious peer — into a prompt, correlated
//! [`OcppError::ProtocolViolation`] instead of a dropped frame that hangs the
//! outstanding `call()` to its timeout.
//!
//! What the primitive tests can't reach is the **wiring**: that
//! [`client.rs`](https://github.com/evlinked/ocpp-rs/blob/main/crates/ocpp-transport/src/client.rs)'s
//! recv-loop `Err(_)` arm and
//! [`server.rs`](https://github.com/evlinked/ocpp-rs/blob/main/crates/ocpp-transport/src/server.rs)'s
//! `classify_frame`-returns-`None` arm actually *call* `recover_inbound_call_error`.
//! A regression that deleted either call site would slip past every existing
//! test — the helper would still pass its own unit tests while the live loops
//! silently dropped the frame. These two tests fail loudly if that happens.
//!
//! The blocker the reference stock server hits is that [`OcppServer`] can only
//! emit the 12 spec [`CallErrorCode`]s (strict enum), so it cannot put an
//! out-of-spec code on the wire. The fix is a **raw `tokio-tungstenite` peer**
//! (dev-dependency only) that hand-crafts the frame, driving the real loops end
//! to end over an actual WebSocket.
//!
//! ## Wire representation
//!
//! `ocpp-rs` frames are the object form (`Message` uses `#[serde(rename = "0")]`
//! …), not the classic OCPP-J array — `recover_inbound_call_error` keys on
//! `value["0"] == "CALLERROR"`. So a CALL is
//! `{"0":"CALL","1":<uid>,"2":<action>,"3":<payload>}`, a CALLRESULT is
//! `{"0":"CALLRESULT","1":<uid>,"2":<payload>}`, and the out-of-spec CALLERROR
//! this suite injects is `{"0":"CALLERROR","1":<uid>,"2":"418","3":…,"4":{}}`.
//!
//! ## Reference
//!
//! Behaviour ported: [`ocpp/messages.py::CallError.to_exception`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py)
//! (unknown code → `UnknownCallErrorCodeError`) + [`ocpp/charge_point.py::_handle_call_error`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
//! (the pending call is always resolved, never left dangling). Part of
//! **M8 — Conformance**.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{DataTransferRequest, ResetRequest};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::v16j::ResetType;
use ocpp_types::OcppError;
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// Outer guard so a *removed* recover call site — which would drop the frame and
/// leave `call()` to hang to its 30 s call-timeout — surfaces as a fast,
/// unambiguous test failure rather than a 30 s stall. With the wiring intact,
/// the call resolves in milliseconds, so this never actually elapses.
const CALL_GUARD: Duration = Duration::from_secs(5);

/// Encode an out-of-spec CALLERROR (object form) echoing `uid`. `"418"` is not a
/// member of [`CallErrorCode`], so a conformant `OcppServer` can never emit it;
/// only a hand-crafted peer can, which is the whole point of this suite.
fn out_of_spec_call_error(uid: &str) -> String {
    serde_json::to_string(&json!({
        "0": "CALLERROR",
        "1": uid,
        "2": "418",
        "3": "I'm a teapot",
        "4": {},
    }))
    .expect("serialize CALLERROR")
}

/// Encode a CALLRESULT (object form) carrying `payload` for `uid`.
fn call_result(uid: &str, payload: Value) -> String {
    serde_json::to_string(&json!({ "0": "CALLRESULT", "1": uid, "2": payload }))
        .expect("serialize CALLRESULT")
}

/// The action name of an inbound CALL frame (`"2"`), or `None` if the frame is
/// not a CALL / carries no string action.
fn call_action(v: &Value) -> Option<&str> {
    (v.get("0").and_then(Value::as_str) == Some("CALL"))
        .then(|| v.get("2").and_then(Value::as_str))
        .flatten()
}

/// The `unique_id` (`"1"`) of any frame, or `""` if absent.
fn unique_id(v: &Value) -> String {
    v.get("1")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

// ─── Client path: real ChargePoint recv loop ────────────────────────────────

/// A raw WebSocket *CSMS* that answers a real `ChargePoint`'s boot handshake so
/// `connect()` succeeds, then replies to a subsequent `DataTransfer` CALL with
/// an out-of-spec CALLERROR. Everything else (the fire-and-forget `Heartbeat`
/// the CP emits right after boot) is ignored — the CP never awaits it.
///
/// Consumes a bound `TcpListener`, accepts exactly one connection, and serves it
/// until the socket closes.
async fn raw_csms(listener: TcpListener) {
    let (stream, _) = listener.accept().await.expect("accept CP socket");
    let mut ws = tokio_tungstenite::accept_async(stream)
        .await
        .expect("CP WebSocket handshake");

    while let Some(frame) = ws.next().await {
        let text = match frame {
            Ok(WsMessage::Text(t)) => t,
            Ok(WsMessage::Ping(d)) => {
                let _ = ws.send(WsMessage::Pong(d)).await;
                continue;
            }
            Ok(WsMessage::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        };
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let uid = unique_id(&v);
        let reply = match call_action(&v) {
            // BootNotification.conf: a schema-valid, Accepted response so the
            // CP's `call()` result-validation passes and boot completes.
            Some("BootNotification") => Some(call_result(
                &uid,
                json!({
                    "currentTime": "2026-07-26T00:00:00Z",
                    "interval": 3600,
                    "status": "Accepted",
                }),
            )),
            // StatusNotification.conf is an empty object; the CP awaits each
            // during `announce_connectors_available()`.
            Some("StatusNotification") => Some(call_result(&uid, json!({}))),
            // The frame under test: an out-of-spec CALLERROR for the explicit
            // `cp.call(DataTransfer)` the test issues after boot.
            Some("DataTransfer") => Some(out_of_spec_call_error(&uid)),
            // Fire-and-forget Heartbeat (and anything unexpected): ignore.
            _ => None,
        };
        if let Some(msg) = reply {
            if ws.send(WsMessage::Text(msg)).await.is_err() {
                break;
            }
        }
    }
}

/// An out-of-spec CALLERROR arriving for an outstanding `ChargePoint::call()`
/// resolves it *promptly* as [`OcppError::ProtocolViolation`] — driven through
/// the real `client.rs` recv loop over an actual WebSocket. The frame fails
/// `Message`'s strict decode (`"418"` is not a [`CallErrorCode`]), lands in the
/// recv loop's `Err(_)` arm, and is recovered by `recover_inbound_call_error`,
/// which rejects the pending call keyed by its `unique_id`.
///
/// If that call site were removed, the frame would be dropped and `call()` would
/// hang to its 30 s timeout — the `CALL_GUARD` would fire first and fail the
/// test, so this pins the *wiring*, not just the helper.
#[tokio::test]
async fn client_recv_loop_recovers_out_of_spec_call_error_over_the_wire() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind CSMS");
    let addr = listener.local_addr().expect("CSMS local addr");
    let csms = tokio::spawn(raw_csms(listener));

    let cp = ChargePoint::new(ChargePointConfig {
        charge_point_id: "CP_WIRE_CLIENT".to_string(),
        central_system_url: format!("ws://{addr}"),
        // One physical connector → boot announces connector 0 + 1 only.
        connector_count: 1,
        // No reconnect storm racing the assertion.
        auto_reconnect: false,
        ..ChargePointConfig::default()
    })
    .expect("build charge point");

    cp.connect()
        .await
        .expect("boot handshake over the raw CSMS peer");
    assert!(cp.is_connected().await, "CP should be booted");

    let err = tokio::time::timeout(
        CALL_GUARD,
        cp.call(DataTransferRequest {
            vendor_id: "com.example".to_string(),
            message_id: None,
            data: None,
        }),
    )
    .await
    .expect("call() must resolve promptly, not hang to its 30s timeout (recover wiring regression)")
    .expect_err("an out-of-spec CALLERROR must resolve call() as Err");

    assert!(
        matches!(err, OcppError::ProtocolViolation { .. }),
        "an unknown CALLERROR code must surface as ProtocolViolation, got {err:?}"
    );
    // Explicit: the pre-#382 failure mode was a misleading Timeout.
    assert!(
        !matches!(err, OcppError::Timeout { .. }),
        "the recovered CALLERROR must NOT surface as a Timeout"
    );

    cp.disconnect().await.ok();
    csms.abort();
}

// ─── Server path: real OcppServer recv loop ─────────────────────────────────

/// A raw WebSocket *charge point* (fake CP): connect to `url` offering the
/// `ocpp1.6` subprotocol, then reply to the first inbound server-initiated CALL
/// with an out-of-spec CALLERROR echoing its `unique_id`.
async fn connect_fake_cp(
    url: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>> {
    let mut request = url.into_client_request().expect("build handshake request");
    // The `OcppServer` upgrade rejects a CP that offers no accepted subprotocol.
    request.headers_mut().insert(
        "sec-websocket-protocol",
        "ocpp1.6".parse().expect("subprotocol header"),
    );
    let (ws, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .expect("fake CP WebSocket handshake");
    ws
}

/// An out-of-spec CALLERROR sent by a charge point in reply to a
/// server-initiated CALL resolves `OcppServer::call()` *promptly* as
/// [`OcppError::ProtocolViolation`] — driven through the real `server.rs` recv
/// loop, whose `classify_frame`-returns-`None` arm calls
/// `recover_inbound_call_error`. Mirror of the client-path test on the CSMS side.
#[tokio::test]
async fn server_recv_loop_recovers_out_of_spec_call_error_over_the_wire() {
    // An empty dispatcher is fine: the fake CP never sends an inbound CALL the
    // CSMS must route — it only *replies* to the server's Reset CALL.
    let handler = Arc::new(DispatchHandler::new(Arc::new(ActionDispatcher::new())));
    let (mut server, _events) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    let cp_id = "CP_WIRE_SERVER";

    let mut ws = connect_fake_cp(&format!("ws://{addr}/ocpp/{cp_id}")).await;
    let fake_cp = tokio::spawn(async move {
        while let Some(frame) = ws.next().await {
            let text = match frame {
                Ok(WsMessage::Text(t)) => t,
                Ok(WsMessage::Ping(d)) => {
                    let _ = ws.send(WsMessage::Pong(d)).await;
                    continue;
                }
                Ok(WsMessage::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            };
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                if call_action(&v).is_some() {
                    let reply = out_of_spec_call_error(&unique_id(&v));
                    if ws.send(WsMessage::Text(reply)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Wait for the handshake to register the CP handle before calling — the
    // insert happens inside the server's per-CP task, slightly after
    // `connect_async` returns, so poll rather than assume (race-free).
    let mut registered = false;
    for _ in 0..100 {
        if server.is_cp_connected(cp_id) {
            registered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        registered,
        "the fake CP should register a routable handle before call()"
    );

    let err = tokio::time::timeout(
        CALL_GUARD,
        server.call(cp_id, ResetRequest { reset_type: ResetType::Soft }),
    )
    .await
    .expect("server.call() must resolve promptly, not hang to its 30s timeout (recover wiring regression)")
    .expect_err("an out-of-spec CALLERROR must resolve server.call() as Err");

    assert!(
        matches!(err, OcppError::ProtocolViolation { .. }),
        "an unknown CALLERROR code must surface as ProtocolViolation, got {err:?}"
    );
    assert!(
        !matches!(err, OcppError::Timeout { .. }),
        "the recovered CALLERROR must NOT surface as a Timeout"
    );

    fake_cp.abort();
    server.stop().await.ok();
}
