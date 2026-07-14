//! Transaction-lifecycle transport events (Issue #66).
//!
//! Proves that a host embedding the CSMS as a library observes the 1.6J
//! transaction lifecycle as typed [`TransportEvent`]s on the channel returned by
//! [`OcppServer::new`], without parsing raw frames — the event/callback surface
//! the charge-hub pure-CPO adapter needs to synthesize Sessions / CDRs.
//!
//! This is the transaction twin of the connector-state
//! `TransportEvent::StatusNotification` surface (#47): the server's per-CP
//! receive loop emits [`TransportEvent::TransactionStarted`] /
//! [`TransactionStopped`](TransportEvent::TransactionStopped) *after* the
//! registered handler accepts the CALL, bridging the `cp_id` (connection layer)
//! and the CSMS-assigned `transactionId` (the `StartTransaction` *response*) that
//! the version-generic dispatcher can't see.
//!
//! Reference: `@on('StartTransaction')` / `@on('StopTransaction')` in
//! [`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py).
//! Drives the crate purely through its public API, so it lives as an integration
//! test rather than an inline `#[cfg(test)]` module.
//!
//! ## What each test pins
//!
//! | Test | Contract |
//! |---|---|
//! | [`start_then_stop_emit_correlated_events`] | an accepted Start→Stop pair surfaces `TransactionStarted` then `TransactionStopped`, correlated by the CSMS-assigned `transaction_id`, carrying the originating `cp_id` and the meter/timestamp fields. |
//! | [`rejected_transaction_emits_no_event`] | a `StartTransaction` the CSMS refuses (no handler → CALLERROR) surfaces **no** lifecycle event — only accepted transactions are observed. |

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use ocpp_messages::v16j::{
    StartTransactionRequest, StartTransactionResponse, StopTransactionRequest,
    StopTransactionResponse,
};
use ocpp_messages::{ActionDispatcher, Message};
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, MessageHandler, TransportConfig, TransportEvent};
use ocpp_types::common::{AuthorizationStatus, IdTagInfo, Reason};
use serde_json::json;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
};

/// The CSMS-assigned transaction id the `StartTransaction` handler returns —
/// the correlation key both emitted events must carry.
const TXN_ID: i32 = 77;

/// A `DispatchHandler` with `StartTransaction` / `StopTransaction` handlers that
/// accept the CALL (returning a fixed `transactionId`), so the receive loop
/// reaches the accepted-CALLRESULT path that emits the lifecycle events. No
/// schema validator is attached: the event surface fires on any accepted
/// CALLRESULT and is deliberately validator-agnostic, so the test pins emission
/// independent of schema coverage (validated separately by the conformance suite).
fn transaction_handler() -> Arc<dyn MessageHandler> {
    let mut d = ActionDispatcher::new();

    d.on(|_req: StartTransactionRequest| async move {
        Ok(StartTransactionResponse {
            id_tag_info: IdTagInfo {
                status: AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: None,
            },
            transaction_id: TXN_ID,
        })
    });

    d.on(|_req: StopTransactionRequest| async move {
        Ok(StopTransactionResponse { id_tag_info: None })
    });

    Arc::new(DispatchHandler::new(Arc::new(d)))
}

/// A CSMS with **no** transaction handlers: every `StartTransaction` is refused
/// with a `NotSupported` CALLERROR, so no transaction is ever accepted.
fn empty_handler() -> Arc<dyn MessageHandler> {
    Arc::new(DispatchHandler::new(Arc::new(ActionDispatcher::new())))
}

/// Start an in-process CSMS on a random loopback port, keeping the event
/// receiver so the test can observe the lifecycle events.
async fn start_server(
    handler: Arc<dyn MessageHandler>,
) -> (OcppServer, SocketAddr, UnboundedReceiver<TransportEvent>) {
    let (mut server, rx) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr, rx)
}

fn ocpp16_request(addr: SocketAddr, cp_id: &str) -> Request<()> {
    let mut req = format!("ws://{addr}/ocpp/{cp_id}")
        .into_client_request()
        .expect("valid ws request");
    req.headers_mut().insert(
        "sec-websocket-protocol",
        "ocpp1.6".parse().expect("valid header value"),
    );
    req
}

/// A tungstenite WebSocket stream over a loopback TCP connection.
type CpWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Send `call` over `ws` and await the single response frame (CALLRESULT or
/// CALLERROR) the CSMS replies with.
async fn round_trip(ws: &mut CpWs, call: &Message) -> String {
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

/// Drain the event channel until the next transaction-lifecycle event arrives,
/// skipping the housekeeping events (`Connected`, `MessageReceived`, …). Returns
/// `None` if none arrives within the timeout — used to assert *absence*.
async fn next_transaction_event(
    rx: &mut UnboundedReceiver<TransportEvent>,
) -> Option<TransportEvent> {
    let deadline = Duration::from_millis(500);
    loop {
        match timeout(deadline, rx.recv()).await {
            Ok(Some(
                ev @ (TransportEvent::TransactionStarted { .. }
                | TransportEvent::TransactionStopped { .. }),
            )) => return Some(ev),
            // A non-transaction event (Connected / MessageReceived / …) — keep draining.
            Ok(Some(_)) => continue,
            // Channel closed or nothing more within the window.
            Ok(None) | Err(_) => return None,
        }
    }
}

/// An accepted `StartTransaction` then `StopTransaction` surface exactly the two
/// lifecycle events, correlated by the CSMS-assigned `transaction_id` and
/// attributed to the originating `cp_id`, with the meter / id-tag / reason fields
/// carried through from the frames.
#[tokio::test]
async fn start_then_stop_emit_correlated_events() {
    let (mut server, addr, mut rx) = start_server(transaction_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-TXN"))
        .await
        .expect("ocpp1.6 handshake");

    // ── StartTransaction ──────────────────────────────────────────────────────
    let start = Message::call(
        "StartTransaction".to_string(),
        json!({
            "connectorId": 2,
            "idTag": "RFID-ABC",
            "meterStart": 1000,
            "timestamp": "2026-07-14T10:00:00Z"
        }),
    )
    .expect("build StartTransaction CALL");
    let start_resp = round_trip(&mut ws, &start).await;
    let start_msg: Message = serde_json::from_str(&start_resp).expect("Start response parses");
    let Message::CallResult(r) = start_msg else {
        panic!("StartTransaction must be accepted, got {start_resp}");
    };
    assert_eq!(r.payload["transactionId"], TXN_ID);

    match next_transaction_event(&mut rx).await {
        Some(TransportEvent::TransactionStarted {
            cp_id,
            connector_id,
            id_tag,
            meter_start,
            transaction_id,
            ..
        }) => {
            assert_eq!(cp_id, "CP-TXN");
            assert_eq!(connector_id, 2);
            assert_eq!(id_tag, "RFID-ABC");
            assert_eq!(meter_start, 1000);
            assert_eq!(transaction_id, TXN_ID);
        }
        other => panic!("expected TransactionStarted, got {other:?}"),
    }

    // ── StopTransaction ───────────────────────────────────────────────────────
    let stop = Message::call(
        "StopTransaction".to_string(),
        json!({
            "transactionId": TXN_ID,
            "meterStop": 3500,
            "timestamp": "2026-07-14T11:00:00Z",
            "reason": "Local"
        }),
    )
    .expect("build StopTransaction CALL");
    let stop_resp = round_trip(&mut ws, &stop).await;
    assert!(
        matches!(
            serde_json::from_str::<Message>(&stop_resp),
            Ok(Message::CallResult(_))
        ),
        "StopTransaction must be accepted, got {stop_resp}"
    );

    match next_transaction_event(&mut rx).await {
        Some(TransportEvent::TransactionStopped {
            cp_id,
            transaction_id,
            meter_stop,
            reason,
            ..
        }) => {
            assert_eq!(cp_id, "CP-TXN");
            assert_eq!(
                transaction_id, TXN_ID,
                "must correlate with the started txn"
            );
            assert_eq!(meter_stop, 3500);
            assert_eq!(reason, Some(Reason::Local));
        }
        other => panic!("expected TransactionStopped, got {other:?}"),
    }

    server.stop().await.expect("server stop");
}

/// A `StartTransaction` the CSMS refuses (no registered handler → `NotSupported`
/// CALLERROR) surfaces **no** lifecycle event: only *accepted* transactions are
/// observed, so a host never synthesizes a session for a rejected CALL.
#[tokio::test]
async fn rejected_transaction_emits_no_event() {
    let (mut server, addr, mut rx) = start_server(empty_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-REJECT"))
        .await
        .expect("ocpp1.6 handshake");

    let start = Message::call(
        "StartTransaction".to_string(),
        json!({
            "connectorId": 1,
            "idTag": "RFID-XYZ",
            "meterStart": 0,
            "timestamp": "2026-07-14T10:00:00Z"
        }),
    )
    .expect("build StartTransaction CALL");
    let resp = round_trip(&mut ws, &start).await;
    assert!(
        matches!(
            serde_json::from_str::<Message>(&resp),
            Ok(Message::CallError(_))
        ) || serde_json::from_str::<ocpp_types::CallErrorMessage>(&resp).is_ok(),
        "an unhandled StartTransaction must be refused with a CALLERROR, got {resp}"
    );

    assert!(
        next_transaction_event(&mut rx).await.is_none(),
        "a rejected StartTransaction must not emit a lifecycle event"
    );

    server.stop().await.expect("server stop");
}
