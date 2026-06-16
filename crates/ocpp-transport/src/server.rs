//! WebSocket server implementation for OCPP Central System Management System (CSMS)

use crate::{
    error::{TransportError, TransportResult},
    ConnectionInfo, ConnectionState, MessageHandler, TransportConfig, TransportEvent,
};
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use ocpp_messages::Message;
use ocpp_types::{CallErrorCode, OcppError};
use std::{net::SocketAddr, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{error, info, warn};
use uuid::Uuid;

/// State shared across all per-CP axum handler invocations.
struct ServerState {
    connections: Arc<DashMap<Uuid, ConnectionInfo>>,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    config: TransportConfig,
    message_handler: Arc<dyn MessageHandler>,
}

/// WebSocket server for OCPP Central System (CSMS).
///
/// Call `start()` to bind and begin accepting connections, then `stop()` to shut down.
pub struct OcppServer {
    config: TransportConfig,
    connections: Arc<DashMap<Uuid, ConnectionInfo>>,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    state: ConnectionState,
    message_handler: Arc<dyn MessageHandler>,
    serve_handle: Option<JoinHandle<()>>,
    local_addr: Option<SocketAddr>,
}

impl OcppServer {
    /// Create a new OCPP server.
    ///
    /// Returns the server and the receive end of the transport-event channel.
    pub fn new(
        config: TransportConfig,
        message_handler: Arc<dyn MessageHandler>,
    ) -> (Self, mpsc::UnboundedReceiver<TransportEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let server = Self {
            config,
            connections: Arc::new(DashMap::new()),
            event_tx,
            state: ConnectionState::Closed,
            message_handler,
            serve_handle: None,
            local_addr: None,
        };
        (server, event_rx)
    }

    /// Bind to `bind_addr` and start accepting WebSocket connections.
    ///
    /// The serve loop runs in a background Tokio task; this method returns as soon as the TCP
    /// socket is bound. Use `local_addr()` to discover the actual port (useful with `:0`).
    pub async fn start(&mut self, bind_addr: &str) -> TransportResult<()> {
        let listener = tokio::net::TcpListener::bind(bind_addr)
            .await
            .map_err(|e| TransportError::IoError {
                message: e.to_string(),
            })?;

        let local_addr = listener.local_addr().map_err(|e| TransportError::IoError {
            message: e.to_string(),
        })?;

        let shared = Arc::new(ServerState {
            connections: Arc::clone(&self.connections),
            event_tx: self.event_tx.clone(),
            config: self.config.clone(),
            message_handler: Arc::clone(&self.message_handler),
        });

        let app = Router::new()
            .route("/ocpp/:charge_point_id", get(ws_handler))
            .with_state(shared);

        self.local_addr = Some(local_addr);
        self.state = ConnectionState::Connected;
        info!("OCPP server listening on {}", local_addr);

        let handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("OCPP server error: {}", e);
            }
        });
        self.serve_handle = Some(handle);

        Ok(())
    }

    /// Stop the server, abort the serve task, and drop all connection tracking.
    pub async fn stop(&mut self) -> TransportResult<()> {
        info!("Stopping OCPP server");
        self.state = ConnectionState::Closing;

        if let Some(handle) = self.serve_handle.take() {
            handle.abort();
        }

        self.connections.clear();
        self.local_addr = None;
        self.state = ConnectionState::Closed;
        Ok(())
    }

    /// Address the server is bound to, or `None` if not yet started.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    /// Number of charge points currently connected.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Connection info for a specific CP, if present.
    pub fn get_connection(&self, connection_id: &Uuid) -> Option<ConnectionInfo> {
        self.connections.get(connection_id).map(|c| c.clone())
    }

    /// Info for every connected CP.
    pub fn get_all_connections(&self) -> Vec<ConnectionInfo> {
        self.connections.iter().map(|c| c.clone()).collect()
    }

    /// Evict connections that have been idle for more than 2× the keep-alive interval.
    pub async fn cleanup_idle_connections(&self) -> TransportResult<usize> {
        let timeout = self.config.keep_alive_interval * 2;
        let idle: Vec<Uuid> = self
            .connections
            .iter()
            .filter_map(|e| e.is_idle(timeout).then_some(*e.key()))
            .collect();

        for id in &idle {
            self.connections.remove(id);
            let _ = self.event_tx.send(TransportEvent::Disconnected {
                connection_id: *id,
                reason: "Idle timeout".to_string(),
            });
        }

        let removed = idle.len();
        if removed > 0 {
            info!("Cleaned up {} idle connections", removed);
        }
        Ok(removed)
    }

    /// Basic server statistics.
    pub fn get_stats(&self) -> ServerStats {
        ServerStats {
            active_connections: self.connection_count(),
        }
    }
}

// ─── axum WebSocket handlers ──────────────────────────────────────────────────

/// Validate the WebSocket upgrade request (subprotocol + CP-ID) then hand off.
async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(charge_point_id): Path<String>,
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    // OCPP 1.6J §3.1 — charge-point IDs are ≤ 48 characters (CiString48)
    if charge_point_id.is_empty() || charge_point_id.len() > 48 {
        warn!(
            "Rejected connection: invalid charge_point_id '{}'",
            charge_point_id
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    // Require Sec-WebSocket-Protocol: ocpp1.6; reject anything else with HTTP 400
    let offers_ocpp16 = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(str::trim).any(|p| p == "ocpp1.6"))
        .unwrap_or(false);

    if !offers_ocpp16 {
        warn!(
            "Rejected '{}': Sec-WebSocket-Protocol: ocpp1.6 not offered",
            charge_point_id
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(ws
        .protocols(["ocpp1.6"])
        .on_upgrade(move |socket| handle_cp_socket(socket, charge_point_id, state)))
}

/// Per-charge-point receive loop.
///
/// Parses incoming text frames as OCPP messages, dispatches CALLs through the
/// `MessageHandler`, and writes CALLRESULT or CALLERROR responses back. On clean close or
/// any WS error the connection is removed from the tracking map.
async fn handle_cp_socket(socket: WebSocket, charge_point_id: String, state: Arc<ServerState>) {
    let mut info = ConnectionInfo::new(charge_point_id.clone(), "csms".to_string());
    info.sub_protocol = Some("ocpp1.6".to_string());
    let connection_id = info.id;

    state.connections.insert(connection_id, info);
    let _ = state.event_tx.send(TransportEvent::Connected {
        connection_id,
        remote_addr: charge_point_id.clone(),
    });
    info!(
        "ChargePoint '{}' connected (id={})",
        charge_point_id, connection_id
    );

    let (mut tx, mut rx) = socket.split();

    while let Some(result) = rx.next().await {
        let text = match result {
            Ok(WsMessage::Text(t)) => t,
            Ok(WsMessage::Close(_)) | Err(_) => break,
            _ => continue, // ping/pong/binary handled by axum or irrelevant
        };

        if text.len() > state.config.max_message_size {
            warn!(
                "Message from '{}' exceeds {} bytes; closing",
                charge_point_id, state.config.max_message_size
            );
            break;
        }

        let response_text = match serde_json::from_str::<Message>(&text) {
            Ok(Message::Call(call)) => {
                let unique_id = call.unique_id.clone();
                let msg = Message::Call(call);

                let _ = state.event_tx.send(TransportEvent::MessageReceived {
                    connection_id,
                    message: msg.clone(),
                });

                match state.message_handler.handle_message(msg).await {
                    Ok(Some(response)) => serde_json::to_string(&response)
                        .map_err(|e| error!("Failed to serialize response: {}", e))
                        .ok(),
                    Ok(None) => None,
                    Err(e) => {
                        let callerror = build_call_error(&unique_id, &e);
                        serde_json::to_string(&callerror)
                            .map_err(|e| error!("Failed to serialize CALLERROR: {}", e))
                            .ok()
                    }
                }
            }
            Ok(msg) => {
                // CALLRESULT / CALLERROR from CP (response to a CSMS-initiated CALL)
                let _ = state.event_tx.send(TransportEvent::MessageReceived {
                    connection_id,
                    message: msg,
                });
                None
            }
            Err(e) => {
                // Cannot correlate without a parseable unique_id — log and continue
                warn!("Malformed OCPP frame from '{}': {}", charge_point_id, e);
                None
            }
        };

        if let Some(resp) = response_text {
            if tx.send(WsMessage::Text(resp)).await.is_err() {
                break;
            }
        }
    }

    state.connections.remove(&connection_id);
    let _ = state.event_tx.send(TransportEvent::Disconnected {
        connection_id,
        reason: "Connection closed".to_string(),
    });
    info!("ChargePoint '{}' disconnected", charge_point_id);
}

/// Map an `OcppError` to the appropriate OCPP error code for a CALLERROR frame.
fn build_call_error(unique_id: &str, error: &OcppError) -> Message {
    let code = match error {
        OcppError::NotSupported { .. } => CallErrorCode::NotSupported,
        // Keyword-granular code from the failing JSON-Schema keyword, per
        // `_validate_payload()` in `ocpp/messages.py`.
        OcppError::SchemaViolation { keyword, .. } => keyword.call_error_code(),
        OcppError::ValidationError { .. } | OcppError::Json { .. } => {
            CallErrorCode::FormationViolation
        }
        _ => CallErrorCode::InternalError,
    };
    Message::call_error(unique_id.to_string(), code, error.to_string(), None)
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// Basic server statistics.
#[derive(Debug, Clone)]
pub struct ServerStats {
    /// Number of charge points currently connected.
    pub active_connections: usize,
}

/// Stub per-connection manager kept for API surface compatibility.
pub struct ConnectionManager {
    #[allow(dead_code)]
    connection_id: Uuid,
    #[allow(dead_code)]
    connection_info: ConnectionInfo,
    message_handler: Option<Arc<dyn MessageHandler>>,
}

impl ConnectionManager {
    pub fn new(connection_info: ConnectionInfo) -> Self {
        Self {
            connection_id: connection_info.id,
            connection_info,
            message_handler: None,
        }
    }

    pub fn set_message_handler(&mut self, handler: Arc<dyn MessageHandler>) {
        self.message_handler = Some(handler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use ocpp_types::{CallResultMessage, MessageType, OcppResult};
    use std::{net::SocketAddr, sync::Arc};
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
    };

    // ── Mock: returns an empty CALLRESULT for any CALL ──────────────────────

    struct EchoHandler;

    #[async_trait::async_trait]
    impl MessageHandler for EchoHandler {
        async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
            if let Message::Call(call) = message {
                Ok(Some(Message::CallResult(CallResultMessage {
                    message_type: MessageType::CallResult,
                    unique_id: call.unique_id,
                    payload: serde_json::json!({}),
                })))
            } else {
                Ok(None)
            }
        }

        async fn handle_event(&self, _: TransportEvent) {}
    }

    // ── Mock: always returns NotSupported ───────────────────────────────────

    struct RejectHandler;

    #[async_trait::async_trait]
    impl MessageHandler for RejectHandler {
        async fn handle_message(&self, _: Message) -> OcppResult<Option<Message>> {
            Err(OcppError::NotSupported {
                feature: "stub".to_string(),
            })
        }

        async fn handle_event(&self, _: TransportEvent) {}
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    async fn start_server(handler: Arc<dyn MessageHandler>) -> (OcppServer, SocketAddr) {
        let (mut server, _rx) = OcppServer::new(TransportConfig::default(), handler);
        server.start("127.0.0.1:0").await.expect("server start");
        let addr = server.local_addr().unwrap();
        (server, addr)
    }

    /// Build a WS upgrade request with `ocpp1.6` subprotocol.
    ///
    /// Using `into_client_request()` on the URL string ensures tungstenite adds the required
    /// `Sec-WebSocket-Key`, `Upgrade`, and `Connection` headers automatically.
    fn ocpp_request(addr: SocketAddr, cp_id: &str) -> Request<()> {
        let mut req = format!("ws://{}/ocpp/{}", addr, cp_id)
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("sec-websocket-protocol", "ocpp1.6".parse().unwrap());
        req
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn server_accepts_valid_ocpp16_connection() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        connect_async(ocpp_request(addr, "CP001"))
            .await
            .expect("should connect with ocpp1.6 subprotocol");

        // Poll until the per-CP task inserts into the map (usually < 10 ms)
        let start = tokio::time::Instant::now();
        while server.connection_count() == 0 && start.elapsed() < Duration::from_millis(500) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(server.connection_count(), 1);

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_rejects_connection_without_subprotocol() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        let result = connect_async(format!("ws://{}/ocpp/CP001", addr)).await;
        assert!(
            result.is_err(),
            "expected rejection (HTTP 400) with no subprotocol"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_rejects_wrong_subprotocol() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        let mut req = format!("ws://{}/ocpp/CP001", addr)
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("sec-websocket-protocol", "ocpp2.0".parse().unwrap());

        let result = connect_async(req).await;
        assert!(
            result.is_err(),
            "expected rejection for unsupported subprotocol"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_round_trip_returns_callresult() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let (mut ws, _) = connect_async(ocpp_request(addr, "CP002")).await.unwrap();

        let call = Message::call("Heartbeat".to_string(), serde_json::json!({})).unwrap();
        let call_id = call.unique_id().to_string();
        ws.send(WsMsg::Text(serde_json::to_string(&call).unwrap()))
            .await
            .unwrap();

        let frame = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting for response")
            .expect("stream ended")
            .expect("WS error");

        if let WsMsg::Text(text) = frame {
            let msg: Message = serde_json::from_str(&text).unwrap();
            assert!(
                matches!(&msg, Message::CallResult(r) if r.unique_id == call_id),
                "expected CALLRESULT with unique_id={}, got {:?}",
                call_id,
                msg
            );
        } else {
            panic!("expected a text frame, got {:?}", frame);
        }

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_unknown_action_returns_callerror_not_supported() {
        let (mut server, addr) = start_server(Arc::new(RejectHandler)).await;
        let (mut ws, _) = connect_async(ocpp_request(addr, "CP003")).await.unwrap();

        let call = Message::call("UnknownAction".to_string(), serde_json::json!({})).unwrap();
        let call_id = call.unique_id().to_string();
        ws.send(WsMsg::Text(serde_json::to_string(&call).unwrap()))
            .await
            .unwrap();

        let frame = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting for CALLERROR")
            .expect("stream ended")
            .expect("WS error");

        // Parse directly as CallErrorMessage: the untagged Message enum is ambiguous for CALLERROR
        // (CallMessage accepts any MessageType including CallError), so we decode the specific type.
        if let WsMsg::Text(text) = frame {
            let err: ocpp_types::CallErrorMessage =
                serde_json::from_str(&text).unwrap_or_else(|e| {
                    panic!("expected CallErrorMessage, got parse error: {e}\nraw: {text}")
                });
            assert_eq!(err.unique_id, call_id);
            assert_eq!(err.error_code, CallErrorCode::NotSupported);
        } else {
            panic!("expected a text frame, got {:?}", frame);
        }

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_removes_connection_on_close() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let (ws, _) = connect_async(ocpp_request(addr, "CP004")).await.unwrap();

        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(server.connection_count(), 1);

        // Dropping the stream triggers a TCP close that the server detects.
        drop(ws);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            server.connection_count(),
            0,
            "map should be empty after close"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_cleanup_idle_connections() {
        let config = TransportConfig {
            keep_alive_interval: std::time::Duration::from_millis(1),
            ..Default::default()
        };
        let (server, _rx) = OcppServer::new(config, Arc::new(EchoHandler));

        let mut stale = ConnectionInfo::new("stale".to_string(), "csms".to_string());
        stale.last_activity = chrono::Utc::now() - chrono::Duration::seconds(10);
        server.connections.insert(stale.id, stale);

        let removed = server.cleanup_idle_connections().await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(server.connection_count(), 0);
    }

    #[test]
    fn connection_manager_stores_id() {
        let info = ConnectionInfo::new("192.168.1.1:9000".to_string(), "0.0.0.0:8080".to_string());
        let mgr = ConnectionManager::new(info.clone());
        assert_eq!(mgr.connection_id, info.id);
    }
}
