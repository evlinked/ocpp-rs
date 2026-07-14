//! BootNotification / MeterValues transport events (Issue #345).
//!
//! Extends the charge-hub embeddable-CSMS event surface (#66) with the two CP→CSMS
//! lifecycle signals that the transaction bookends (#344) left unmodeled: a
//! `BootNotification` on connect (for Location / EVSE inventory synthesis) and the
//! periodic `MeterValues` a CP sends during a transaction (for sub-session energy
//! curves and interim billing). Both surface as typed [`TransportEvent`]s on the
//! channel returned by [`OcppServer::new`], so a host embedding the CSMS reacts to
//! them without parsing raw frames.
//!
//! Like the transaction events, the server's per-CP receive loop emits
//! [`TransportEvent::BootNotification`] / [`MeterValues`](TransportEvent::MeterValues)
//! *after* the registered handler accepts the CALL, bridging the `cp_id` (which
//! lives at the connection layer, not the payload the version-generic dispatcher
//! sees). All the other event fields come from the request.
//!
//! Reference: `@on('BootNotification')` / `@on('MeterValues')` in
//! [`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py).
//! Drives the crate purely through its public API, so it lives as an integration
//! test rather than an inline `#[cfg(test)]` module.
//!
//! ## What each test pins
//!
//! | Test | Contract |
//! |---|---|
//! | [`boot_notification_emits_identity_event`] | an accepted `BootNotification` surfaces a `BootNotification` event carrying the `cp_id` and the CP's self-reported vendor / model / serial / firmware. |
//! | [`meter_values_emit_event_with_transaction_id`] | an accepted `MeterValues` reported *during* a transaction surfaces a `MeterValues` event carrying the `cp_id`, connector, `transaction_id`, and the sampled meter readings. |
//! | [`meter_values_without_transaction_thread_none`] | a `MeterValues` with no `transactionId` (metering outside a transaction) surfaces the event with `transaction_id == None`. |
//! | [`rejected_boot_notification_emits_no_event`] | a `BootNotification` the CSMS refuses (no handler → CALLERROR) surfaces **no** event — only accepted CALLs are observed. |

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, MeterValuesRequest, MeterValuesResponse,
    RegistrationStatus,
};
use ocpp_messages::{ActionDispatcher, Message};
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, MessageHandler, TransportConfig, TransportEvent};
use serde_json::json;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
};

/// A `DispatchHandler` that accepts `BootNotification` and `MeterValues`, so the
/// receive loop reaches the accepted-CALLRESULT path that emits the events. No
/// schema validator is attached: the event surface fires on any accepted
/// CALLRESULT and is deliberately validator-agnostic, so the test pins emission
/// independent of schema coverage (validated separately by the conformance suite).
fn lifecycle_handler() -> Arc<dyn MessageHandler> {
    let mut d = ActionDispatcher::new();

    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap(),
            interval: 300,
            status: RegistrationStatus::Accepted,
        })
    });

    d.on(|_req: MeterValuesRequest| async move { Ok(MeterValuesResponse {}) });

    Arc::new(DispatchHandler::new(Arc::new(d)))
}

/// A CSMS with **no** handlers: every CALL is refused with a `NotSupported`
/// CALLERROR, so nothing is ever accepted.
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

/// Drain the event channel until the next Boot/MeterValues lifecycle event
/// arrives, skipping housekeeping events (`Connected`, `MessageReceived`, …).
/// Returns `None` if none arrives within the timeout — used to assert *absence*.
async fn next_lifecycle_event(
    rx: &mut UnboundedReceiver<TransportEvent>,
) -> Option<TransportEvent> {
    let deadline = Duration::from_millis(500);
    loop {
        match timeout(deadline, rx.recv()).await {
            Ok(Some(
                ev @ (TransportEvent::BootNotification { .. } | TransportEvent::MeterValues { .. }),
            )) => return Some(ev),
            // A non-lifecycle event (Connected / MessageReceived / …) — keep draining.
            Ok(Some(_)) => continue,
            // Channel closed or nothing more within the window.
            Ok(None) | Err(_) => return None,
        }
    }
}

/// An accepted `BootNotification` surfaces exactly one `BootNotification` event
/// carrying the originating `cp_id` and the CP's self-reported identity.
#[tokio::test]
async fn boot_notification_emits_identity_event() {
    let (mut server, addr, mut rx) = start_server(lifecycle_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-BOOT"))
        .await
        .expect("ocpp1.6 handshake");

    let boot = Message::call(
        "BootNotification".to_string(),
        json!({
            "chargePointVendor": "AcmeCharge",
            "chargePointModel": "AC-22",
            "chargePointSerialNumber": "SN-0001",
            "firmwareVersion": "1.4.2"
        }),
    )
    .expect("build BootNotification CALL");
    let resp = round_trip(&mut ws, &boot).await;
    assert!(
        matches!(
            serde_json::from_str::<Message>(&resp),
            Ok(Message::CallResult(_))
        ),
        "BootNotification must be accepted, got {resp}"
    );

    match next_lifecycle_event(&mut rx).await {
        Some(TransportEvent::BootNotification {
            cp_id,
            vendor,
            model,
            serial_number,
            firmware_version,
        }) => {
            assert_eq!(cp_id, "CP-BOOT");
            assert_eq!(vendor, "AcmeCharge");
            assert_eq!(model, "AC-22");
            assert_eq!(serial_number.as_deref(), Some("SN-0001"));
            assert_eq!(firmware_version.as_deref(), Some("1.4.2"));
        }
        other => panic!("expected BootNotification, got {other:?}"),
    }

    server.stop().await.expect("server stop");
}

/// An accepted `MeterValues` reported during a transaction surfaces a
/// `MeterValues` event carrying the `cp_id`, connector, `transaction_id`, and the
/// sampled readings.
#[tokio::test]
async fn meter_values_emit_event_with_transaction_id() {
    let (mut server, addr, mut rx) = start_server(lifecycle_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-METER"))
        .await
        .expect("ocpp1.6 handshake");

    let meter = Message::call(
        "MeterValues".to_string(),
        json!({
            "connectorId": 3,
            "transactionId": 4242,
            "meterValue": [{
                "timestamp": "2026-07-14T10:05:00Z",
                "sampledValue": [
                    { "value": "1500", "measurand": "Energy.Active.Import.Register", "unit": "Wh" },
                    { "value": "16.0", "measurand": "Current.Import", "unit": "A" }
                ]
            }]
        }),
    )
    .expect("build MeterValues CALL");
    let resp = round_trip(&mut ws, &meter).await;
    assert!(
        matches!(
            serde_json::from_str::<Message>(&resp),
            Ok(Message::CallResult(_))
        ),
        "MeterValues must be accepted, got {resp}"
    );

    match next_lifecycle_event(&mut rx).await {
        Some(TransportEvent::MeterValues {
            cp_id,
            connector_id,
            transaction_id,
            meter_values,
        }) => {
            assert_eq!(cp_id, "CP-METER");
            assert_eq!(connector_id, 3);
            assert_eq!(transaction_id, Some(4242));
            assert_eq!(
                meter_values.len(),
                1,
                "one MeterValue entry carried through"
            );
            assert_eq!(
                meter_values[0].sampled_values.len(),
                2,
                "both sampled values carried through"
            );
            assert_eq!(meter_values[0].sampled_values[0].value, "1500");
        }
        other => panic!("expected MeterValues, got {other:?}"),
    }

    server.stop().await.expect("server stop");
}

/// A `MeterValues` with no `transactionId` (a CP reporting connector metering
/// outside a transaction) surfaces the event with `transaction_id == None` —
/// `MeterValues.req`'s optional field is threaded through faithfully.
#[tokio::test]
async fn meter_values_without_transaction_thread_none() {
    let (mut server, addr, mut rx) = start_server(lifecycle_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-METER-NO-TXN"))
        .await
        .expect("ocpp1.6 handshake");

    let meter = Message::call(
        "MeterValues".to_string(),
        json!({
            "connectorId": 0,
            "meterValue": [{
                "timestamp": "2026-07-14T10:05:00Z",
                "sampledValue": [ { "value": "230.0" } ]
            }]
        }),
    )
    .expect("build MeterValues CALL");
    let resp = round_trip(&mut ws, &meter).await;
    assert!(
        matches!(
            serde_json::from_str::<Message>(&resp),
            Ok(Message::CallResult(_))
        ),
        "MeterValues must be accepted, got {resp}"
    );

    match next_lifecycle_event(&mut rx).await {
        Some(TransportEvent::MeterValues {
            connector_id,
            transaction_id,
            ..
        }) => {
            assert_eq!(connector_id, 0);
            assert_eq!(
                transaction_id, None,
                "no transactionId → None, not defaulted"
            );
        }
        other => panic!("expected MeterValues, got {other:?}"),
    }

    server.stop().await.expect("server stop");
}

/// A `BootNotification` the CSMS refuses (no registered handler → `NotSupported`
/// CALLERROR) surfaces **no** event: only *accepted* CALLs are observed, mirroring
/// the transaction events' acceptance gate.
#[tokio::test]
async fn rejected_boot_notification_emits_no_event() {
    let (mut server, addr, mut rx) = start_server(empty_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-REJECT"))
        .await
        .expect("ocpp1.6 handshake");

    let boot = Message::call(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "AcmeCharge", "chargePointModel": "AC-22" }),
    )
    .expect("build BootNotification CALL");
    let resp = round_trip(&mut ws, &boot).await;
    assert!(
        matches!(
            serde_json::from_str::<Message>(&resp),
            Ok(Message::CallError(_))
        ) || serde_json::from_str::<ocpp_types::CallErrorMessage>(&resp).is_ok(),
        "an unhandled BootNotification must be refused with a CALLERROR, got {resp}"
    );

    assert!(
        next_lifecycle_event(&mut rx).await.is_none(),
        "a rejected BootNotification must not emit a lifecycle event"
    );

    server.stop().await.expect("server stop");
}
