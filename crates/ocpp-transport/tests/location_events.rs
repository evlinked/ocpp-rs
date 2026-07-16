//! Deriving an OCPI-shaped Location / EVSE inventory end-to-end (Issue #347).
//!
//! The charge-hub embeddable-CSMS surface (#66) asks a host to derive OCPI
//! **Locations** from OCPP telemetry. The enabling events landed already —
//! [`TransportEvent::BootNotification`] (#345, self-reported identity) and
//! [`TransportEvent::StatusNotification`] (#47, per-connector availability) — and
//! `examples/csms_location.rs` demonstrates folding them into a Location snapshot.
//!
//! This pins that derivation *over a real `OcppServer` + loopback WebSocket*: a
//! charge point boots, then reports `StatusNotification` for two connectors; the
//! test collects the emitted events and folds them through the same synthesis the
//! example documents, asserting the resulting Location's identity and per-connector
//! EVSE status. The Location counterpart to `tests/lifecycle_events.rs` /
//! `tests/transaction_events.rs`.
//!
//! Reference: `@on('BootNotification')` / `@on('StatusNotification')` in
//! [`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py).

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, RegistrationStatus,
    StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::{ActionDispatcher, Message};
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, MessageHandler, TransportConfig, TransportEvent};
use ocpp_types::v16j::ChargePointStatus;
use serde_json::json;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
};

// ---------------------------------------------------------------------------
// The synthesis under test: fold Boot + Status events into a Location snapshot.
// A trimmed copy of `examples/csms_location.rs`'s tracker (integration tests can't
// import an example), pinning the same derivation over a real server.
// ---------------------------------------------------------------------------

/// Coarse OCPI-style EVSE status folded from a 1.6J `ChargePointStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvseStatus {
    Available,
    Charging,
    Occupied,
    Reserved,
    OutOfOrder,
    Inoperative,
}

impl EvseStatus {
    fn from_ocpp(status: ChargePointStatus) -> Self {
        match status {
            ChargePointStatus::Available => EvseStatus::Available,
            ChargePointStatus::Charging => EvseStatus::Charging,
            ChargePointStatus::Preparing
            | ChargePointStatus::SuspendedEV
            | ChargePointStatus::SuspendedEVSE
            | ChargePointStatus::Finishing => EvseStatus::Occupied,
            ChargePointStatus::Reserved => EvseStatus::Reserved,
            ChargePointStatus::Faulted => EvseStatus::OutOfOrder,
            ChargePointStatus::Unavailable => EvseStatus::Inoperative,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Location {
    cp_id: String,
    vendor: String,
    model: String,
    serial_number: Option<String>,
    firmware_version: Option<String>,
    station_status: Option<EvseStatus>,
    evses: BTreeMap<u32, EvseStatus>,
}

#[derive(Debug, Default)]
struct LocationTracker {
    locations: BTreeMap<String, Location>,
}

impl LocationTracker {
    fn observe(&mut self, event: &TransportEvent) {
        match event {
            TransportEvent::BootNotification {
                cp_id,
                vendor,
                model,
                serial_number,
                firmware_version,
            } => {
                let loc = self.entry(cp_id);
                loc.vendor = vendor.clone();
                loc.model = model.clone();
                loc.serial_number = serial_number.clone();
                loc.firmware_version = firmware_version.clone();
            }
            TransportEvent::StatusNotification {
                cp_id,
                connector_id,
                status,
            } => {
                let mapped = EvseStatus::from_ocpp(*status);
                let loc = self.entry(cp_id);
                if *connector_id == 0 {
                    loc.station_status = Some(mapped);
                } else {
                    loc.evses.insert(*connector_id, mapped);
                }
            }
            _ => {}
        }
    }

    fn entry(&mut self, cp_id: &str) -> &mut Location {
        self.locations
            .entry(cp_id.to_string())
            .or_insert_with(|| Location {
                cp_id: cp_id.to_string(),
                ..Location::default()
            })
    }
}

// ---------------------------------------------------------------------------
// Server harness (mirrors tests/lifecycle_events.rs).
// ---------------------------------------------------------------------------

/// A `DispatchHandler` that accepts `BootNotification` and `StatusNotification`, so
/// the receive loop reaches the accepted-CALLRESULT path that emits the events.
fn location_handler() -> Arc<dyn MessageHandler> {
    let mut d = ActionDispatcher::new();

    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: Utc.with_ymd_and_hms(2026, 7, 16, 10, 0, 0).unwrap(),
            interval: 300,
            status: RegistrationStatus::Accepted,
        })
    });

    d.on(|_req: StatusNotificationRequest| async move { Ok(StatusNotificationResponse {}) });

    Arc::new(DispatchHandler::new(Arc::new(d)))
}

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

type CpWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Send `call` and await the single response frame, asserting it is a CALLRESULT.
async fn round_trip_accepted(ws: &mut CpWs, call: &Message) {
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
    let text = match frame {
        WsMsg::Text(t) => t,
        other => panic!("expected a text response frame, got {other:?}"),
    };
    assert!(
        matches!(
            serde_json::from_str::<Message>(&text),
            Ok(Message::CallResult(_))
        ),
        "CALL must be accepted, got {text}"
    );
}

/// Drain the event channel, folding every Boot/Status event into `tracker`, until
/// it has seen at least `expected` of them or the window elapses.
async fn drain_into(
    rx: &mut UnboundedReceiver<TransportEvent>,
    tracker: &mut LocationTracker,
    expected: usize,
) {
    let mut seen = 0;
    let deadline = Duration::from_millis(500);
    while seen < expected {
        match timeout(deadline, rx.recv()).await {
            Ok(Some(
                ev @ (TransportEvent::BootNotification { .. }
                | TransportEvent::StatusNotification { .. }),
            )) => {
                tracker.observe(&ev);
                seen += 1;
            }
            // Housekeeping event (Connected / MessageReceived / …) — keep draining.
            Ok(Some(_)) => continue,
            // Channel closed or nothing more within the window.
            Ok(None) | Err(_) => break,
        }
    }
    assert_eq!(
        seen, expected,
        "expected {expected} Boot/Status events, saw {seen}"
    );
}

/// A charge point that boots and reports two connectors surfaces a Location whose
/// identity comes from the `BootNotification` and whose per-connector EVSE status
/// comes from each `StatusNotification`.
#[tokio::test]
async fn boot_and_status_derive_location_inventory() {
    let (mut server, addr, mut rx) = start_server(location_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-LOC"))
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
    round_trip_accepted(&mut ws, &boot).await;

    let status1 = Message::call(
        "StatusNotification".to_string(),
        json!({ "connectorId": 1, "errorCode": "NoError", "status": "Available" }),
    )
    .expect("build StatusNotification CALL");
    round_trip_accepted(&mut ws, &status1).await;

    let status2 = Message::call(
        "StatusNotification".to_string(),
        json!({ "connectorId": 2, "errorCode": "NoError", "status": "Charging" }),
    )
    .expect("build StatusNotification CALL");
    round_trip_accepted(&mut ws, &status2).await;

    let mut tracker = LocationTracker::default();
    drain_into(&mut rx, &mut tracker, 3).await;

    let location = tracker
        .locations
        .get("CP-LOC")
        .expect("a Location was derived for CP-LOC");

    // Identity from the BootNotification.
    assert_eq!(location.vendor, "AcmeCharge");
    assert_eq!(location.model, "AC-22");
    assert_eq!(location.serial_number.as_deref(), Some("SN-0001"));
    assert_eq!(location.firmware_version.as_deref(), Some("1.4.2"));

    // One EVSE per reported connector, coarse-mapped from the OCPP status.
    assert_eq!(location.evses.get(&1), Some(&EvseStatus::Available));
    assert_eq!(location.evses.get(&2), Some(&EvseStatus::Charging));
    assert_eq!(location.evses.len(), 2);
    assert!(
        location.station_status.is_none(),
        "no connectorId==0 status was reported"
    );

    server.stop().await.expect("server stop");
}

/// A `connectorId == 0` `StatusNotification` addresses the charge point as a whole,
/// so it updates the station-wide status rather than adding a phantom EVSE.
#[tokio::test]
async fn connector_zero_updates_station_status() {
    let (mut server, addr, mut rx) = start_server(location_handler()).await;
    let (mut ws, _resp) = connect_async(ocpp16_request(addr, "CP-STATION"))
        .await
        .expect("ocpp1.6 handshake");

    let status0 = Message::call(
        "StatusNotification".to_string(),
        json!({ "connectorId": 0, "errorCode": "NoError", "status": "Unavailable" }),
    )
    .expect("build StatusNotification CALL");
    round_trip_accepted(&mut ws, &status0).await;

    let mut tracker = LocationTracker::default();
    drain_into(&mut rx, &mut tracker, 1).await;

    let location = tracker
        .locations
        .get("CP-STATION")
        .expect("a Location was derived even before a BootNotification");

    assert_eq!(location.station_status, Some(EvseStatus::Inoperative));
    assert!(
        location.evses.is_empty(),
        "connectorId==0 must not create an EVSE entry"
    );
    // A Status-before-Boot CP still yields a Location, with empty identity.
    assert!(location.vendor.is_empty());

    server.stop().await.expect("server stop");
}
