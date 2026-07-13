//! OCPP **2.0.1** transport end-to-end (Issue #340).
//!
//! Proves that 2.0.1 is wired *all the way through a real socket*, not merely in
//! the in-memory `dispatch()` path pinned by the conformance suite's `routing.rs`
//! or the handshake-only negotiation unit-tested in this crate's
//! `server_accepts_ocpp201_subprotocol`. This suite stands up a real
//! [`OcppServer`] whose [`MessageHandler`] is a
//! `DispatchHandler(ActionDispatcher::with_validator(SchemaValidator::v201()))`
//! and round-trips genuine 2.0.1 frames over a loopback WebSocket, exercising
//! **handshake negotiation → JSON framing → routing → v201 schema validation**
//! together.
//!
//! It is the 2.0.1 twin of the 1.6J transport test
//! `server_routes_call_to_registered_handler` in
//! [`dispatch_handler.rs`](../src/dispatch_handler.rs)'s `ws_tests`, differing
//! only in the injected validator (`v201()` instead of `v16j()`) and the message
//! vocabulary (`ChargingStationType` / `BootReasonEnumType`, empty-`{}`
//! `Heartbeat`). It lives as an integration test rather than an inline `#[cfg]`
//! module because it drives the crate purely through its public API.
//!
//! ## What each test pins
//!
//! | Test | Contract |
//! |---|---|
//! | [`negotiates_ocpp201_subprotocol_on_connect`] | a client offering `Sec-WebSocket-Protocol: ocpp2.0.1` completes the handshake and the server echoes `ocpp2.0.1`. |
//! | [`v201_boot_notification_round_trips_to_call_result`] | a 2.0.1 `BootNotification` CALL routes to its `@on` handler and a **schema-valid** 2.0.1 CALLRESULT comes back over the wire. |
//! | [`v201_heartbeat_round_trips_to_call_result`] | the empty-payload 2.0.1 `Heartbeat` likewise round-trips to a CALLRESULT. |
//! | [`v201_schema_invalid_call_yields_call_error`] | a payload that fails **v201** schema validation (a `BootNotification` missing the required `reason`) is refused with a CALLERROR — not silently accepted, not dispatched to the handler. |
//!
//! Reference: `ocpp/charge_point.py` `route_message` / `_handle_call` (the
//! version-agnostic routing the `v201()` validator specialises). Part of
//! **M7 — OCPP 2.0.1**; directly unblocked by #338/#339 (handshake negotiation).
//! Test-only; **zero** production-code change and no new dependencies.

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::v201::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
};
use ocpp_messages::{ActionDispatcher, Message};
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, MessageHandler, TransportConfig};
use ocpp_types::v201::RegistrationStatusEnumType;
use ocpp_types::{CallErrorCode, CallErrorMessage};
use serde_json::json;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
};

/// A fixed, schema-valid `date-time` for outbound CALLRESULTs. Deterministic so
/// the test never depends on wall-clock formatting; the same literal the v201
/// schema-validation suite proves valid for `currentTime`.
const FIXED_TIME: &str = "2026-07-13T00:00:00Z";

/// A `DispatchHandler` wired to the **v201** validator with the two core
/// lifecycle handlers registered — the minimal CSMS a 2.0.1 station boots
/// against. Both handlers return minimal, schema-valid 2.0.1 CALLRESULTs (the
/// dispatcher validates the outbound result before returning it), mirroring the
/// reference's default `@on` handlers in `examples/v201/central_system.py`.
fn v201_handler() -> Arc<dyn MessageHandler> {
    let mut d = ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v201()));

    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: FIXED_TIME.to_string(),
            interval: 3600,
            status: RegistrationStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        })
    });

    d.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: FIXED_TIME.to_string(),
            custom_data: None,
        })
    });

    Arc::new(DispatchHandler::new(Arc::new(d)))
}

/// Start an in-process CSMS bound to a random free loopback port.
async fn start_server(handler: Arc<dyn MessageHandler>) -> (OcppServer, SocketAddr) {
    let (mut server, _rx) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr)
}

/// A WebSocket upgrade request offering **only** the `ocpp2.0.1` subprotocol —
/// the header a real 2.0.1 station sends. Forcing a single offer proves the
/// server accepts 2.0.1 on its own, independent of server-preference ordering
/// when both are offered.
fn ocpp201_request(addr: SocketAddr, cp_id: &str) -> Request<()> {
    let mut req = format!("ws://{addr}/ocpp/{cp_id}")
        .into_client_request()
        .expect("valid ws request");
    req.headers_mut().insert(
        "sec-websocket-protocol",
        "ocpp2.0.1".parse().expect("valid header value"),
    );
    req
}

/// Connect offering `ocpp2.0.1`, send `call`, and return the single text frame
/// the server replies with. Panics on handshake failure, timeout, or a
/// non-text frame — a v201 CALL must always draw exactly one response frame.
async fn round_trip(addr: SocketAddr, cp_id: &str, call: &Message) -> String {
    let (mut ws, _resp) = connect_async(ocpp201_request(addr, cp_id))
        .await
        .expect("ocpp2.0.1 handshake");
    ws.send(WsMsg::Text(
        serde_json::to_string(call).expect("serialise CALL"),
    ))
    .await
    .expect("send CALL");
    let frame = timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("timed out waiting for response")
        .expect("stream ended before a response")
        .expect("WS error");
    match frame {
        WsMsg::Text(t) => t,
        other => panic!("expected a text response frame, got {other:?}"),
    }
}

/// The handshake half: a client offering *only* `ocpp2.0.1` connects and the
/// server echoes the negotiated subprotocol back as `ocpp2.0.1`. This is the
/// socket-level proof that 2.0.1 connections are accepted (the twin of the
/// `server_accepts_ocpp201_subprotocol` unit test, exercised here through the
/// full harness the round-trip tests build on).
#[tokio::test]
async fn negotiates_ocpp201_subprotocol_on_connect() {
    let (mut server, addr) = start_server(v201_handler()).await;

    let (_ws, resp) = connect_async(ocpp201_request(addr, "CP201"))
        .await
        .expect("server must accept an ocpp2.0.1 handshake");
    assert_eq!(
        resp.headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok()),
        Some("ocpp2.0.1"),
        "server must echo the negotiated ocpp2.0.1 subprotocol"
    );

    server.stop().await.expect("server stop");
}

/// A 2.0.1 `BootNotification` CALL round-trips to a **schema-valid** 2.0.1
/// CALLRESULT over a real socket, correlation id preserved. Exercises
/// handshake → framing → routing → `v201()` validation of both the inbound CALL
/// and the outbound CALLRESULT.
#[tokio::test]
async fn v201_boot_notification_round_trips_to_call_result() {
    let (mut server, addr) = start_server(v201_handler()).await;

    // A well-formed 2.0.1 BootNotification, as a station sends on boot — the
    // minimal valid payload (`reason` + `chargingStation.{model,vendorName}`).
    let call = Message::call(
        "BootNotification".to_string(),
        json!({
            "reason": "PowerUp",
            "chargingStation": { "model": "Turbo-3000", "vendorName": "ACME" }
        }),
    )
    .expect("build BootNotification CALL");
    let id = call.unique_id().to_string();

    let text = round_trip(addr, "CP201", &call).await;
    let msg: Message = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("response must parse as a Message: {e}\nraw: {text}"));
    match msg {
        Message::CallResult(r) => {
            assert_eq!(r.unique_id, id, "CALLRESULT must reuse the CALL's id");
            // The boot response the CSMS returned, validated by v201() on the
            // way out: the three required fields are present on the wire.
            assert_eq!(r.payload["status"], "Accepted");
            assert_eq!(r.payload["interval"], 3600);
            assert_eq!(r.payload["currentTime"], FIXED_TIME);
        }
        other => panic!("expected a v201 CALLRESULT, got {other:?}\nraw: {text}"),
    }

    server.stop().await.expect("server stop");
}

/// The empty-payload 2.0.1 `Heartbeat` (`{}` on the wire) round-trips to a
/// CALLRESULT carrying `currentTime` — proving a second, distinctly-shaped v201
/// action also routes end-to-end, not just `BootNotification`.
#[tokio::test]
async fn v201_heartbeat_round_trips_to_call_result() {
    let (mut server, addr) = start_server(v201_handler()).await;

    let call = Message::call("Heartbeat".to_string(), json!({})).expect("build Heartbeat CALL");
    let id = call.unique_id().to_string();

    let text = round_trip(addr, "CP201", &call).await;
    let msg: Message = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("response must parse as a Message: {e}\nraw: {text}"));
    match msg {
        Message::CallResult(r) => {
            assert_eq!(r.unique_id, id, "CALLRESULT must reuse the CALL's id");
            assert_eq!(r.payload["currentTime"], FIXED_TIME);
        }
        other => panic!("expected a v201 CALLRESULT, got {other:?}\nraw: {text}"),
    }

    server.stop().await.expect("server stop");
}

/// The trust-boundary negative: a `BootNotification` **missing the required
/// `reason`** fails v201 schema validation *inside the dispatcher* and comes
/// back as a CALLERROR — never dispatched to the handler, never silently
/// accepted. Missing-required maps to `ProtocolError` (per `SchemaKeyword`),
/// the same keyword-granular code the in-memory v201 schema suite pins; here we
/// prove it survives the trip over a real socket.
#[tokio::test]
async fn v201_schema_invalid_call_yields_call_error() {
    let (mut server, addr) = start_server(v201_handler()).await;

    // Structurally a valid object, but `reason` (required) is absent: the v201
    // schema rejects it even though `chargingStation` is well-formed.
    let call = Message::call(
        "BootNotification".to_string(),
        json!({ "chargingStation": { "model": "M", "vendorName": "V" } }),
    )
    .expect("build invalid BootNotification CALL");
    let id = call.unique_id().to_string();

    let text = round_trip(addr, "CP201", &call).await;
    let err: CallErrorMessage = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("expected a CALLERROR, parse error {e}\nraw: {text}"));
    assert_eq!(err.unique_id, id, "CALLERROR must reuse the CALL's id");
    assert_eq!(
        err.error_code,
        CallErrorCode::ProtocolError,
        "a missing required field must be refused as ProtocolError, got {text}"
    );

    server.stop().await.expect("server stop");
}
