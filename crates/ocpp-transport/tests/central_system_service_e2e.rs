//! One-call batteries-included CSMS builder end-to-end (Issue #389).
//!
//! Proves that [`central_system_service`] / [`central_system_service_v201`]
//! assemble a working CSMS *all the way through a real socket* from a single
//! call: the inbound dispatch path (default `@on` handlers answer), the outbound
//! [`call`](OcppServer::call) path (schema-validated), and the extensibility
//! seam (a customizer's handlers reach the wire without dropping the defaults).
//!
//! The cheaper no-socket assertions (the outbound validator is *attached*, and
//! the customizer *runs*) live inline in `central_system.rs` /
//! `central_system_v201.rs`; this suite pins the parts that only a real
//! WebSocket can: both directions actually carrying validated frames, and a
//! customizer-registered handler answering over the wire.
//!
//! Reference: `examples/v16/central_system.py` — the runnable "batteries
//! included" CSMS this convenience layer mirrors in one call. Part of
//! **M8 — Conformance**. Test-only; no new dependencies.

use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use ocpp_messages::v16j::{AuthorizeRequest, AuthorizeResponse, RemoteStartTransactionRequest};
use ocpp_messages::Message;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{
    central_system_service, central_system_service_v201, CentralSystemConfig,
    CentralSystemConfigV201, TransportConfig,
};
use ocpp_types::common::{AuthorizationStatus, IdTagInfo};
use ocpp_types::OcppError;
use serde_json::{json, Value};
use tokio::time::{timeout, Duration, Instant};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
};

/// Concrete WebSocket stream type returned by `connect_async`.
type CpWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A WS upgrade request offering exactly `subprotocol` (e.g. `ocpp1.6` or
/// `ocpp2.0.1`) at `/ocpp/{cp_id}`.
fn ocpp_request(addr: SocketAddr, cp_id: &str, subprotocol: &str) -> Request<()> {
    let mut req = format!("ws://{addr}/ocpp/{cp_id}")
        .into_client_request()
        .expect("valid ws request");
    req.headers_mut().insert(
        "sec-websocket-protocol",
        subprotocol.parse().expect("valid header value"),
    );
    req
}

/// Start `server` on a random loopback port and return its address.
async fn start(server: &mut OcppServer) -> SocketAddr {
    server.start("127.0.0.1:0").await.expect("server start");
    server.local_addr().expect("server local addr")
}

/// Connect as charge point `cp_id` offering `subprotocol`, and wait until the
/// server has registered its routing handle (so `server.call(cp_id, …)` finds
/// it).
async fn connect_cp(server: &OcppServer, addr: SocketAddr, cp_id: &str, subprotocol: &str) -> CpWs {
    let (ws, _resp) = connect_async(ocpp_request(addr, cp_id, subprotocol))
        .await
        .expect("handshake");
    let deadline = Instant::now() + Duration::from_millis(500);
    while !server.is_cp_connected(cp_id) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(server.is_cp_connected(cp_id), "server registered {cp_id}");
    ws
}

/// Send `call` over `ws`.
async fn send_call(ws: &mut CpWs, call: &Message) {
    ws.send(WsMsg::Text(
        serde_json::to_string(call).expect("serialise CALL"),
    ))
    .await
    .expect("send CALL");
}

/// Read one text frame from `ws` (2 s budget) and parse it as a [`Message`].
async fn read_message(ws: &mut CpWs) -> Message {
    let frame = timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("timed out waiting for a frame")
        .expect("stream ended")
        .expect("WS error");
    match frame {
        WsMsg::Text(t) => serde_json::from_str(&t)
            .unwrap_or_else(|e| panic!("frame must parse as a Message: {e}\nraw: {t}")),
        other => panic!("expected a text frame, got {other:?}"),
    }
}

/// Read one CALL frame the server sent and return its `unique_id`.
async fn read_call_id(ws: &mut CpWs) -> String {
    match read_message(ws).await {
        Message::Call(c) => c.unique_id,
        other => panic!("expected a CALL, got {other:?}"),
    }
}

/// Serialise a CALLRESULT frame for `unique_id` with `payload`.
fn call_result_frame(unique_id: &str, payload: Value) -> String {
    serde_json::to_string(&Message::call_result(unique_id.to_string(), payload).unwrap()).unwrap()
}

fn remote_start(id_tag: &str) -> RemoteStartTransactionRequest {
    RemoteStartTransactionRequest {
        connector_id: None,
        id_tag: id_tag.to_string(),
        charging_profile: None,
    }
}

/// **Both directions through one call.** A CSMS built by the one-call
/// `central_system_service` (a) answers an inbound `BootNotification` from its
/// default handler, and (b) rejects a schema-invalid CALLRESULT on its outbound
/// `call()` path — the acceptance test proving the single builder validates both
/// directions over a real socket.
#[tokio::test]
async fn service_round_trips_inbound_boot_and_rejects_invalid_outbound_callresult() {
    let (mut server, _events) = central_system_service(
        CentralSystemConfig::default(),
        TransportConfig::default(),
        |_| {},
    );
    let addr = start(&mut server).await;
    let mut cp = connect_cp(&server, addr, "CP_SVC", "ocpp1.6").await;

    // (a) Inbound: the default boot handler answers Accepted + interval 300.
    let boot = Message::call(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "ACME", "chargePointModel": "Wallbox-1" }),
    )
    .unwrap();
    let boot_id = boot.unique_id().to_string();
    send_call(&mut cp, &boot).await;
    match read_message(&mut cp).await {
        Message::CallResult(r) => {
            assert_eq!(r.unique_id, boot_id, "CALLRESULT must reuse the CALL id");
            assert_eq!(r.payload["status"], "Accepted");
            assert_eq!(r.payload["interval"], 300);
        }
        other => panic!("expected a CALLRESULT for BootNotification, got {other:?}"),
    }

    // (b) Outbound: `call()` sends a valid CALL, but the CP replies with a
    // CALLRESULT carrying a schema-forbidden extra property
    // (`additionalProperties: false`). The server's `call()`-path validator must
    // reject it as `SchemaViolation` rather than deserialise it. Run the CSMS
    // `call()` and the CP-side responder concurrently over the one socket.
    let call_fut = server.call("CP_SVC", remote_start("TAG"));
    let responder_fut = async {
        let id = read_call_id(&mut cp).await;
        cp.send(WsMsg::Text(call_result_frame(
            &id,
            json!({ "status": "Accepted", "extra": true }),
        )))
        .await
        .unwrap();
    };
    let (call_res, ()) = tokio::join!(call_fut, responder_fut);
    let err = call_res.expect_err("schema-invalid CALLRESULT must be rejected");
    assert!(
        matches!(err, OcppError::SchemaViolation { .. }),
        "expected SchemaViolation for additionalProperties violation, got {err:?}"
    );

    server.stop().await.expect("server stop");
}

/// **Extensibility without regressions.** A customizer that registers an extra
/// `@on(Authorize)` handler reaches the wire — an inbound `Authorize` gets the
/// custom response — while the pre-installed default `Heartbeat` handler still
/// answers, proving the customizer neither replaces the defaults nor drops the
/// inbound validator.
#[tokio::test]
async fn service_customizer_handler_reachable_and_defaults_preserved() {
    let (mut server, _events) = central_system_service(
        CentralSystemConfig::default(),
        TransportConfig::default(),
        |d| {
            d.on(|_req: AuthorizeRequest| async move {
                Ok(AuthorizeResponse {
                    id_tag_info: IdTagInfo {
                        status: AuthorizationStatus::Accepted,
                        parent_id_tag: None,
                        expiry_date: None,
                    },
                })
            });
        },
    );
    let addr = start(&mut server).await;
    let mut cp = connect_cp(&server, addr, "CP_CUSTOM", "ocpp1.6").await;

    // The customizer's Authorize handler answers (a stock boot-trio CSMS would
    // reject Authorize as NotSupported — it isn't in the default trio).
    let authorize = Message::call("Authorize".to_string(), json!({ "idTag": "ABC123" })).unwrap();
    send_call(&mut cp, &authorize).await;
    match read_message(&mut cp).await {
        Message::CallResult(r) => assert_eq!(r.payload["idTagInfo"]["status"], "Accepted"),
        other => panic!("expected a CALLRESULT for Authorize (custom handler), got {other:?}"),
    }

    // …and the default Heartbeat handler is still installed.
    let heartbeat = Message::call("Heartbeat".to_string(), json!({})).unwrap();
    send_call(&mut cp, &heartbeat).await;
    match read_message(&mut cp).await {
        Message::CallResult(r) => assert!(
            r.payload.get("currentTime").is_some(),
            "default Heartbeat must still answer with currentTime, got {:?}",
            r.payload
        ),
        other => panic!("expected a CALLRESULT for Heartbeat (default), got {other:?}"),
    }

    server.stop().await.expect("server stop");
}

/// The **2.0.1 twin**: a CSMS built by the one-call `central_system_service_v201`
/// negotiates `ocpp2.0.1` and answers an inbound 2.0.1 `BootNotification` from
/// its default lifecycle handler with a schema-valid CALLRESULT.
#[tokio::test]
async fn service_v201_round_trips_inbound_boot() {
    let (mut server, _events) = central_system_service_v201(
        CentralSystemConfigV201::default(),
        TransportConfig::default(),
        |_| {},
    );
    let addr = start(&mut server).await;
    let mut cp = connect_cp(&server, addr, "CP201_SVC", "ocpp2.0.1").await;

    let boot = Message::call(
        "BootNotification".to_string(),
        json!({
            "reason": "PowerUp",
            "chargingStation": { "model": "Turbo-3000", "vendorName": "ACME" }
        }),
    )
    .unwrap();
    let boot_id = boot.unique_id().to_string();
    send_call(&mut cp, &boot).await;
    match read_message(&mut cp).await {
        Message::CallResult(r) => {
            assert_eq!(r.unique_id, boot_id, "CALLRESULT must reuse the CALL id");
            assert_eq!(r.payload["status"], "Accepted");
            assert_eq!(r.payload["interval"], 300);
            assert!(
                r.payload.get("currentTime").is_some(),
                "v201 boot response must carry currentTime, got {:?}",
                r.payload
            );
        }
        other => panic!("expected a v201 CALLRESULT for BootNotification, got {other:?}"),
    }

    server.stop().await.expect("server stop");
}
