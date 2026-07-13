//! WebSocket connection *lifecycle* contract — conformance suite.
//!
//! A faithful port of the runtime half of the mobilityhouse/ocpp reference's
//! [`tests/test_charge_point_connection.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point_connection.py)
//! — the two socket-driven classes `TestChargePointStart` and
//! `TestCreateAndStartChargePoint`, backed by
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)'s
//! `ChargePoint.start()` and `create_and_start_charge_point`. The connect-path
//! *parsing* half (`TestExtractChargePointId`) is pinned separately as a pure
//! function in [`connection_handling`](connection_handling.rs) (issue #326 /
//! PR #327); this suite pins the observable *session* behaviour those two
//! classes assert, driven end-to-end over a real loopback WebSocket.
//!
//! ## Faithful adaptation, not a literal port
//!
//! Rust's CSMS has no per-socket `ChargePoint.start()` object that `raise`s a
//! connection error back to a caller. The per-connection read loop is the
//! server-side `handle_cp_socket` (`crates/ocpp-transport/src/server.rs`),
//! spawned per accepted socket; path validation happens in the WebSocket
//! handshake. So each Python assertion is mapped onto the **observable** CSMS
//! contract, using the same in-process loopback harness the rest of the
//! conformance suite uses (`OcppServer` + a real [`ChargePoint`] over
//! `ws://127.0.0.1:0`, as in `charge_point_call_e2e.rs` / `full_session.rs`) —
//! with **zero** production-code change and no new dependencies.
//!
//! | Reference test | Observable contract pinned here |
//! |---|---|
//! | `TestCreateAndStartChargePoint::test_valid_path_creates_and_starts` | [`valid_path_creates_and_registers_charge_point`] — a valid connect path completes the handshake and registers the CP under the extracted id; the socket stays open. |
//! | `TestCreateAndStartChargePoint::test_invalid_path_closes_connection` | [`invalid_path_registers_no_charge_point`] — a path with no usable id is refused at the handshake; **no** connection and **no** routing key are registered. |
//! | `TestChargePointStart::test_start_processes_messages_before_exception` | [`message_delivered_before_close_is_dispatched_exactly_once`] — a CALL delivered before the socket drops is dispatched exactly once (not lost, not re-dispatched by the close). |
//! | `TestChargePointStart::test_start_propagates_exception_on_connection_closed` | [`socket_close_deregisters_cp_and_call_yields_cp_not_connected`] — when the socket closes, the CP is deregistered and a subsequent CSMS `call()` returns [`OcppError::CpNotConnected`]; the failure is **observable** to the CSMS, not silently swallowed. |
//! | `TestChargePointStart::test_reconnection_with_new_instance` | [`sequential_reconnection_with_same_id_reregisters_and_routes`] — after a session closes, a fresh session on the same id registers cleanly and routes. |
//!
//! ### The `raises` → `deregister`/`CpNotConnected` divergence — pinned, not dropped
//!
//! Python's `start()` re-`raise`s the connection exception so the *consumer*
//! that awaited `start()` learns the socket died. Rust's CSMS owns the socket in
//! a background task, so it can't hand an exception back to an awaiting caller;
//! instead the failure becomes observable through the routing table: the read
//! loop **deregisters** the CP on close, so `is_cp_connected` flips to `false`
//! and any later CSMS-initiated `call()` returns [`OcppError::CpNotConnected`]
//! rather than hanging or being silently swallowed. That is the faithful Rust
//! analogue of "the exception reaches the consumer", and it is what
//! [`socket_close_deregisters_cp_and_call_yields_cp_not_connected`] pins.
//!
//! ### Invalid-path scope
//!
//! The reference's invalid-path case is `/` (and, at the parsing layer,
//! whitespace-only). This suite pins the `/`-shaped case (an empty, unusable id)
//! against the **live** handshake. The whitespace-only refusal is pinned at the
//! pure-function layer in [`connection_handling`](connection_handling.rs): the
//! live axum handshake (`ws_handler`) validates the id inline (non-empty,
//! ≤ 48 chars) rather than routing through `validate_handshake_request`, so
//! wiring the hardened whitespace/trim check into the runtime handshake is a
//! separate, production-touching change tracked as its own follow-up rather than
//! folded into this dependency-free test port.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, RegistrationStatus,
    StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::{ActionDispatcher, Message};
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, MessageHandler, TransportConfig, TransportEvent};
use ocpp_types::v16j::RemoteStartStopStatus;
use ocpp_types::{OcppError, OcppResult};
use tokio::time::{Duration, Instant};

/// A CSMS message handler that routes every inbound CALL through a real
/// [`ActionDispatcher`] and additionally **counts** the dispatches of one chosen
/// action. The count lets a test pin that a frame delivered before the socket
/// drops is handled exactly once — the one thing the stock [`DispatchHandler`]
/// can't report on its own. Keeping the counter here leaves the production
/// transport path untouched.
struct CountingCsms {
    inner: DispatchHandler,
    action: String,
    count: Arc<AtomicUsize>,
}

#[async_trait]
impl MessageHandler for CountingCsms {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        if let Message::Call(call) = &message {
            if call.action == self.action {
                self.count.fetch_add(1, Ordering::SeqCst);
            }
        }
        self.inner.handle_message(message).await
    }

    async fn handle_event(&self, _event: TransportEvent) {}
}

/// A dispatcher that accepts the boot handshake and the connector-status
/// announcements a fresh [`ChargePoint`] emits during `connect()`, so every test
/// starts from a cleanly-booted session. The BootNotification advertises a long
/// heartbeat interval so no background `Heartbeat` races the assertions.
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

/// Start an in-process CSMS on a random free loopback port, counting inbound
/// CALLs whose action equals `action_to_count` (pass `""` to count nothing).
/// Returns the server, its bound addr, and the shared dispatch counter.
async fn start_csms(action_to_count: &str) -> (OcppServer, SocketAddr, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(CountingCsms {
        inner: DispatchHandler::new(Arc::new(base_dispatcher())),
        action: action_to_count.to_string(),
        count: Arc::clone(&count),
    });
    let (mut server, _events) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr, count)
}

fn cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        // One physical connector keeps the boot announcement deterministic.
        connector_count: 1,
        // Deterministic: no background reconnect storm racing the assertions.
        auto_reconnect: false,
        ..ChargePointConfig::default()
    }
}

/// Boot a real [`ChargePoint`] against `addr` and return it, connected.
async fn booted_cp(addr: SocketAddr, id: &str) -> ChargePoint {
    let cp = ChargePoint::new(cp_config(addr, id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    cp
}

/// Poll until `server` no longer routes `id`, or fail after a generous deadline.
/// The read loop removes the routing handle during socket teardown, which
/// happens-after the client's `disconnect()` returns; polling avoids a fixed
/// sleep while staying robust to scheduling jitter.
async fn wait_until_deregistered(server: &OcppServer, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.is_cp_connected(id) {
        assert!(
            Instant::now() < deadline,
            "server never deregistered {id} after its socket closed"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// `TestCreateAndStartChargePoint::test_valid_path_creates_and_starts` — a valid
/// connect path completes the handshake and registers the CP under the extracted
/// id, exposed through the **public** server API (`is_cp_connected` /
/// `connected_cp_ids` / `connection_count`); the socket is left open.
#[tokio::test]
async fn valid_path_creates_and_registers_charge_point() {
    let (mut server, addr, _count) = start_csms("").await;

    let cp = booted_cp(addr, "CP001").await;

    assert!(
        server.is_cp_connected("CP001"),
        "a booted CP must be registered under its extracted id"
    );
    assert_eq!(
        server.connected_cp_ids(),
        vec!["CP001".to_string()],
        "the extracted id is the routing key"
    );
    assert_eq!(server.connection_count(), 1, "exactly one live session");
    assert!(
        cp.is_connected().await,
        "the socket is left open, not closed"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// `TestCreateAndStartChargePoint::test_invalid_path_closes_connection` — a
/// connect path with no usable charge-point id (an empty id ⇒ path `/ocpp/`, the
/// `/`-shaped reference case) is refused at the handshake, so `connect()` fails
/// and **nothing** is registered: no connection, no routing key. Trust-boundary
/// negative — a refused handshake must never yield a live routing key.
#[tokio::test]
async fn invalid_path_registers_no_charge_point() {
    let (mut server, addr, _count) = start_csms("").await;

    let cp = ChargePoint::new(cp_config(addr, "")).expect("build charge point");
    let result = cp.connect().await;

    assert!(
        result.is_err(),
        "a connect path with no usable id must be refused, got {result:?}"
    );
    assert_eq!(
        server.connection_count(),
        0,
        "a refused handshake must register no connection"
    );
    assert!(
        server.connected_cp_ids().is_empty(),
        "a refused handshake must register no routing key"
    );

    server.stop().await.expect("server stop");
}

/// `TestChargePointStart::test_start_processes_messages_before_exception` — a
/// CALL delivered before the socket drops is dispatched exactly once. The boot
/// handshake delivers exactly one `BootNotification` CALL; after the socket
/// closes the count is unchanged — the pre-close frame was neither lost nor
/// re-dispatched by the teardown.
#[tokio::test]
async fn message_delivered_before_close_is_dispatched_exactly_once() {
    let (mut server, addr, boot_count) = start_csms("BootNotification").await;

    let cp = booted_cp(addr, "CP_ONCE").await;
    assert_eq!(
        boot_count.load(Ordering::SeqCst),
        1,
        "the single pre-close BootNotification must be dispatched exactly once"
    );

    cp.disconnect().await.expect("disconnect");
    wait_until_deregistered(&server, "CP_ONCE").await;

    assert_eq!(
        boot_count.load(Ordering::SeqCst),
        1,
        "closing the socket must not lose or re-dispatch the pre-close frame"
    );

    server.stop().await.expect("server stop");
}

/// `TestChargePointStart::test_start_propagates_exception_on_connection_closed` —
/// the Rust analogue of Python's `start()` re-raising to its consumer. When the
/// socket closes, the read loop deregisters the CP, so `is_cp_connected` flips to
/// `false` and a subsequent CSMS-initiated `call()` returns
/// [`OcppError::CpNotConnected`] — the failure is surfaced to the CSMS, not
/// silently swallowed.
#[tokio::test]
async fn socket_close_deregisters_cp_and_call_yields_cp_not_connected() {
    let (mut server, addr, _count) = start_csms("").await;

    let cp = booted_cp(addr, "CP_GONE").await;
    assert!(
        server.is_cp_connected("CP_GONE"),
        "registered while connected"
    );

    cp.disconnect().await.expect("disconnect");
    wait_until_deregistered(&server, "CP_GONE").await;

    assert!(
        !server.is_cp_connected("CP_GONE"),
        "a closed socket must deregister the CP"
    );
    let err = server
        .remote_start_transaction("CP_GONE", "TAG", Some(1))
        .await
        .expect_err("a CALL to a deregistered CP must fail, not hang or succeed");
    assert!(
        matches!(err, OcppError::CpNotConnected { ref cp_id } if cp_id == "CP_GONE"),
        "expected CpNotConnected surfaced to the CSMS, got {err:?}"
    );

    server.stop().await.expect("server stop");
}

/// `TestChargePointStart::test_reconnection_with_new_instance` — after a session
/// closes, a fresh [`ChargePoint`] on the **same** id registers cleanly and
/// routes: a CSMS-initiated CALL resolves against the reconnected session. This
/// is the sequential-reconnect contract (the racy-reconnect variant is pinned by
/// the transport crate's own issue-#50 regression test).
#[tokio::test]
async fn sequential_reconnection_with_same_id_reregisters_and_routes() {
    let (mut server, addr, _count) = start_csms("").await;

    // Session A connects, then disconnects and is fully deregistered.
    let cp_a = booted_cp(addr, "CP_RECON").await;
    assert!(server.is_cp_connected("CP_RECON"));
    cp_a.disconnect().await.expect("disconnect A");
    wait_until_deregistered(&server, "CP_RECON").await;

    // Session B — a brand-new instance on the same id — registers cleanly.
    let cp_b = booted_cp(addr, "CP_RECON").await;
    assert!(
        server.is_cp_connected("CP_RECON"),
        "a reconnecting CP must re-register under the same id"
    );

    // The decisive check: a CSMS CALL routes to the reconnected session and
    // resolves, rather than failing with CpNotConnected.
    let status = server
        .remote_start_transaction("CP_RECON", "TAG", Some(1))
        .await
        .expect("call must route to the reconnected session");
    assert_eq!(status, RemoteStartStopStatus::Accepted);

    cp_b.disconnect().await.expect("disconnect B");
    server.stop().await.expect("server stop");
}
