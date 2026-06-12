//! WebSocket server implementation for OCPP Central System Management System (CSMS)

use crate::{
    error::{TransportError, TransportResult},
    ConnectionInfo, ConnectionState, MessageHandler, TransportConfig, TransportEvent,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use ocpp_messages::Message;
use ocpp_types::{CallErrorCode, OcppError, RawMessage};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// No-op handler — used as the default when no handler is supplied.
pub struct NoOpHandler;

#[async_trait::async_trait]
impl MessageHandler for NoOpHandler {
    async fn handle_message(&self, _message: Message) -> ocpp_types::OcppResult<Option<Message>> {
        Ok(None)
    }

    async fn handle_event(&self, _event: TransportEvent) {}
}

/// WebSocket server for OCPP Central System
pub struct OcppServer {
    config: TransportConfig,
    /// Active connections, keyed by charge-point ID
    connections: Arc<DashMap<String, ConnectionInfo>>,
    /// Per-connection outbound sinks, keyed by charge-point ID
    connection_sinks: Arc<DashMap<String, mpsc::UnboundedSender<tungstenite::Message>>>,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    state: ConnectionState,
    handler: Arc<dyn MessageHandler>,
    shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
    accept_task: Option<JoinHandle<()>>,
}

impl OcppServer {
    /// Create a new OCPP server with the given message handler.
    pub fn new(
        config: TransportConfig,
        handler: Arc<dyn MessageHandler>,
    ) -> (Self, mpsc::UnboundedReceiver<TransportEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let server = Self {
            config,
            connections: Arc::new(DashMap::new()),
            connection_sinks: Arc::new(DashMap::new()),
            event_tx,
            state: ConnectionState::Closed,
            handler,
            shutdown_tx: None,
            accept_task: None,
        };
        (server, event_rx)
    }

    /// Bind to `bind_addr`, spawn the accept loop, and return the actual local address.
    ///
    /// Returns immediately; the accept loop runs in a background task.
    pub async fn start(&mut self, bind_addr: &str) -> TransportResult<SocketAddr> {
        info!("Starting OCPP server on {}", bind_addr);

        let listener = tokio::net::TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;

        let config = self.config.clone();
        let connections = Arc::clone(&self.connections);
        let connection_sinks = Arc::clone(&self.connection_sinks);
        let event_tx = self.event_tx.clone();
        let handler = Arc::clone(&self.handler);

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, peer_addr)) => {
                                let config = config.clone();
                                let connections = Arc::clone(&connections);
                                let connection_sinks = Arc::clone(&connection_sinks);
                                let event_tx = event_tx.clone();
                                let handler = Arc::clone(&handler);
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(
                                        stream,
                                        peer_addr,
                                        config,
                                        connections,
                                        connection_sinks,
                                        event_tx,
                                        handler,
                                    )
                                    .await
                                    {
                                        debug!("Connection from {} closed: {}", peer_addr, e);
                                    }
                                });
                            }
                            Err(e) => error!("Accept error: {}", e),
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("OCPP server shutting down");
                            break;
                        }
                    }
                }
            }
        });

        self.accept_task = Some(task);
        self.state = ConnectionState::Connected;
        info!("OCPP server listening on {}", local_addr);
        Ok(local_addr)
    }

    /// Stop the server: signal the accept loop, abort its task, and clear connection state.
    pub async fn stop(&mut self) -> TransportResult<()> {
        info!("Stopping OCPP server");
        self.state = ConnectionState::Closing;

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        if let Some(task) = self.accept_task.take() {
            task.abort();
        }

        self.connections.clear();
        self.connection_sinks.clear();
        self.state = ConnectionState::Closed;
        Ok(())
    }

    /// Number of currently connected charge points.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Look up a connection by charge-point ID.
    pub fn get_connection(&self, cp_id: &str) -> Option<ConnectionInfo> {
        self.connections.get(cp_id).map(|r| r.clone())
    }

    /// All currently connected charge points.
    pub fn get_all_connections(&self) -> Vec<ConnectionInfo> {
        self.connections.iter().map(|r| r.clone()).collect()
    }

    /// Push a message to the given charge-point connection.
    pub async fn send_to_connection(&self, cp_id: &str, message: Message) -> TransportResult<()> {
        let text = serialize_message(&message)?;
        if let Some(sink) = self.connection_sinks.get(cp_id) {
            sink.send(tungstenite::Message::Text(text)).map_err(|_| {
                TransportError::ConnectionClosed {
                    reason: format!("CP {} disconnected", cp_id),
                }
            })?;
            Ok(())
        } else {
            Err(TransportError::ConnectionError {
                message: format!("no connection for charge point '{}'", cp_id),
            })
        }
    }

    /// Broadcast a message to all connected charge points; returns CP IDs that failed.
    pub async fn broadcast_message(&self, message: Message) -> TransportResult<Vec<String>> {
        let text = serialize_message(&message)?;
        let ws_msg = tungstenite::Message::Text(text);
        let mut failed = Vec::new();

        for entry in self.connection_sinks.iter() {
            if entry.value().send(ws_msg.clone()).is_err() {
                failed.push(entry.key().clone());
            }
        }
        Ok(failed)
    }

    /// Evict idle connections (last_activity older than 2× keep_alive_interval).
    pub async fn cleanup_idle_connections(&self) -> TransportResult<usize> {
        let timeout = self.config.keep_alive_interval * 2;
        let mut removed = 0;

        let idle: Vec<String> = self
            .connections
            .iter()
            .filter_map(|entry| {
                if entry.is_idle(timeout) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();

        for cp_id in idle {
            if let Some((_, info)) = self.connections.remove(&cp_id) {
                self.connection_sinks.remove(&cp_id);
                info!("Evicted idle connection: {}", cp_id);
                removed += 1;
                let _ = self.event_tx.send(TransportEvent::Disconnected {
                    connection_id: info.id,
                    reason: "Idle timeout".to_string(),
                });
            }
        }
        Ok(removed)
    }

    /// Basic server statistics.
    pub fn get_stats(&self) -> ServerStats {
        ServerStats {
            active_connections: self.connection_count(),
            total_connections: 0,
            uptime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default(),
        }
    }
}

/// Server statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub active_connections: usize,
    pub total_connections: u64,
    pub uptime: std::time::Duration,
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn serialize_message(message: &Message) -> TransportResult<String> {
    let raw = RawMessage::from(message.clone());
    serde_json::to_string(&raw).map_err(|e| TransportError::SerializationError {
        message: e.to_string(),
    })
}

fn parse_message(text: &str) -> Result<Message, OcppError> {
    let raw: RawMessage = serde_json::from_str(text).map_err(|e| OcppError::Json {
        message: e.to_string(),
    })?;
    raw.into_message()
}

/// Build a `tungstenite::WebSocketConfig` from our `TransportConfig`.
fn make_ws_config(cfg: &TransportConfig) -> tungstenite::protocol::WebSocketConfig {
    tungstenite::protocol::WebSocketConfig {
        max_message_size: Some(cfg.max_message_size),
        max_frame_size: Some(cfg.max_message_size),
        write_buffer_size: cfg.max_message_size,
        max_write_buffer_size: cfg.max_message_size * 2,
        accept_unmasked_frames: false,
        ..Default::default()
    }
}

/// Extract the charge-point ID from a `/ocpp/{cp_id}` URL path.
fn extract_cp_id(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    parts.next(); // leading ""
    let segment = parts.next()?;
    if segment != "ocpp" {
        return None;
    }
    let cp_id = parts.next()?;
    if cp_id.is_empty() || cp_id.len() > 64 {
        return None;
    }
    Some(cp_id.to_string())
}

/// Per-connection task: perform the WS handshake, validate subprotocol + path,
/// then run the OCPP message loop until the connection closes.
// The tungstenite handshake callback signature forces Result<Resp, Resp> which is large.
#[allow(clippy::result_large_err)]
async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    config: TransportConfig,
    connections: Arc<DashMap<String, ConnectionInfo>>,
    connection_sinks: Arc<DashMap<String, mpsc::UnboundedSender<tungstenite::Message>>>,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    handler: Arc<dyn MessageHandler>,
) -> TransportResult<()> {
    let allowed_protocols = config.sub_protocols.clone();

    // State extracted during the HTTP handshake callback.
    let cp_id_cell: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
    let cp_id_capture = Arc::clone(&cp_id_cell);

    let ws_stream = tokio_tungstenite::accept_hdr_async_with_config(
        stream,
        move |req: &tungstenite::http::Request<()>, mut resp: tungstenite::http::Response<()>| {
            // Validate URL path → extract CP ID.
            let cp_id = extract_cp_id(req.uri().path()).ok_or_else(|| {
                tungstenite::http::Response::builder()
                    .status(400)
                    .body(Some(
                        "Invalid path: expected /ocpp/{charge_point_id}".to_string(),
                    ))
                    .expect("static response is valid")
            })?;

            // Validate Sec-WebSocket-Protocol header.
            let requested = req
                .headers()
                .get("sec-websocket-protocol")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("");

            let selected = requested
                .split(',')
                .map(|p| p.trim())
                .find(|p| allowed_protocols.iter().any(|ap| ap == p))
                .map(|s| s.to_string());

            let proto = selected.ok_or_else(|| {
                tungstenite::http::Response::builder()
                    .status(400)
                    .body(Some(format!(
                        "No supported OCPP subprotocol; server supports: {}",
                        allowed_protocols.join(", ")
                    )))
                    .expect("static response is valid")
            })?;

            resp.headers_mut().insert(
                "sec-websocket-protocol",
                proto.parse().expect("valid header value"),
            );

            *cp_id_capture.lock().unwrap() = Some(cp_id);
            Ok(resp)
        },
        Some(make_ws_config(&config)),
    )
    .await
    .map_err(|e| TransportError::HandshakeError {
        message: e.to_string(),
    })?;

    let cp_id = cp_id_cell
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| TransportError::Internal {
            message: "handshake callback was not invoked".to_string(),
        })?;

    // Build ConnectionInfo.
    let mut conn_info = ConnectionInfo::new(peer_addr.to_string(), "server".to_string());
    conn_info.sub_protocol = config.sub_protocols.first().cloned();
    conn_info.charge_point_id = Some(cp_id.clone());
    let conn_id = conn_info.id;

    // Register connection.
    connections.insert(cp_id.clone(), conn_info.clone());

    // Outbound channel: the recv-loop writes responses here; `send_to_connection` also writes here.
    let (sink_tx, mut sink_rx) = mpsc::unbounded_channel::<tungstenite::Message>();
    let sink_for_responses = sink_tx.clone();
    connection_sinks.insert(cp_id.clone(), sink_tx);

    let _ = event_tx.send(TransportEvent::Connected {
        connection_id: conn_id,
        remote_addr: peer_addr.to_string(),
    });
    handler
        .handle_event(TransportEvent::Connected {
            connection_id: conn_id,
            remote_addr: peer_addr.to_string(),
        })
        .await;

    info!("Charge point '{}' connected from {}", cp_id, peer_addr);

    // Split the WebSocket stream.
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Drain task: forwards messages from `sink_rx` to the WebSocket sink.
    let drain_task = tokio::spawn(async move {
        while let Some(msg) = sink_rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Receive loop.
    let loop_result: TransportResult<()> = async {
        loop {
            match ws_stream.next().await {
                Some(Ok(tungstenite::Message::Text(text))) => {
                    if let Some(mut info) = connections.get_mut(&cp_id) {
                        info.update_activity();
                    }

                    let message = match parse_message(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("Malformed OCPP message from '{}': {}", cp_id, e);
                            // Cannot correlate a unique_id when parsing fails — log and skip.
                            continue;
                        }
                    };

                    let is_call = matches!(message, Message::Call(_));
                    let unique_id = message.unique_id().to_string();

                    match handler.handle_message(message).await {
                        Ok(Some(response)) => {
                            let text = serialize_message(&response)?;
                            let _ = sink_for_responses.send(tungstenite::Message::Text(text));
                        }
                        Ok(None) => {}
                        Err(e) if is_call => {
                            let code = match &e {
                                OcppError::NotSupported { .. } => CallErrorCode::NotImplemented,
                                OcppError::ValidationError { .. } => {
                                    CallErrorCode::FormationViolation
                                }
                                _ => CallErrorCode::InternalError,
                            };
                            let err_msg = Message::call_error(unique_id, code, e.to_string(), None);
                            let text = serialize_message(&err_msg)?;
                            let _ = sink_for_responses.send(tungstenite::Message::Text(text));
                        }
                        Err(e) => {
                            warn!("Handler error for non-CALL from '{}': {}", cp_id, e);
                        }
                    }
                }
                Some(Ok(tungstenite::Message::Ping(data))) => {
                    let _ = sink_for_responses.send(tungstenite::Message::Pong(data));
                }
                Some(Ok(tungstenite::Message::Close(_))) => {
                    debug!("Charge point '{}' sent close frame", cp_id);
                    break;
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    return Err(TransportError::from(e));
                }
                None => break,
            }
        }
        Ok(())
    }
    .await;

    // Cleanup (always runs).
    drain_task.abort();
    connections.remove(&cp_id);
    connection_sinks.remove(&cp_id);

    let reason = loop_result
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "Connection closed".to_string());

    info!("Charge point '{}' disconnected: {}", cp_id, reason);

    let disconnect_event = TransportEvent::Disconnected {
        connection_id: conn_id,
        reason: reason.clone(),
    };
    let _ = event_tx.send(disconnect_event.clone());
    handler.handle_event(disconnect_event).await;

    loop_result
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::{CallMessage, CallResultMessage};
    use ocpp_types::{OcppError, OcppResult};
    use serde_json::json;

    // ── test handler ──────────────────────────────────────────────────────────

    /// Echoes every CALL back as a CALLRESULT with an empty payload.
    struct EchoHandler;

    #[async_trait::async_trait]
    impl MessageHandler for EchoHandler {
        async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
            match message {
                Message::Call(call) => {
                    let resp = CallResultMessage::new(call.unique_id, json!({})).map_err(|e| {
                        OcppError::Internal {
                            message: e.to_string(),
                        }
                    })?;
                    Ok(Some(Message::CallResult(resp)))
                }
                _ => Ok(None),
            }
        }

        async fn handle_event(&self, _event: TransportEvent) {}
    }

    /// Returns `OcppError::NotSupported` for every CALL.
    struct RejectAllHandler;

    #[async_trait::async_trait]
    impl MessageHandler for RejectAllHandler {
        async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
            match message {
                Message::Call(call) => Err(OcppError::NotSupported {
                    feature: call.action,
                }),
                _ => Ok(None),
            }
        }

        async fn handle_event(&self, _event: TransportEvent) {}
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Starts a server on a random port; returns (server, event_rx, bound_addr).
    async fn start_test_server(
        handler: Arc<dyn MessageHandler>,
    ) -> (
        OcppServer,
        mpsc::UnboundedReceiver<TransportEvent>,
        SocketAddr,
    ) {
        let (mut server, event_rx) = OcppServer::new(TransportConfig::default(), handler);
        let addr = server.start("127.0.0.1:0").await.expect("server started");
        (server, event_rx, addr)
    }

    /// Connect a raw tokio-tungstenite client to the server.
    async fn connect_client(
        addr: SocketAddr,
        cp_id: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let url = format!("ws://{}/ocpp/{}", addr, cp_id);
        let req = tungstenite::client::IntoClientRequest::into_client_request(url.as_str())
            .expect("valid url");
        let mut req = req;
        req.headers_mut().insert(
            "sec-websocket-protocol",
            "ocpp1.6".parse().expect("valid header"),
        );
        let (ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .expect("client connected");
        ws
    }

    fn send_call(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        call: CallMessage,
    ) -> impl std::future::Future<Output = ()> + '_ {
        let text = {
            let raw = RawMessage::from(Message::Call(call));
            serde_json::to_string(&raw).expect("serialized")
        };
        async move {
            ws.send(tungstenite::Message::Text(text))
                .await
                .expect("sent");
        }
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn server_creation_uses_no_op_handler() {
        let (server, _rx) = OcppServer::new(TransportConfig::default(), Arc::new(NoOpHandler));
        assert_eq!(server.connection_count(), 0);
        assert_eq!(server.state, ConnectionState::Closed);
    }

    #[tokio::test]
    async fn server_start_stop_round_trip() {
        let (mut server, _rx) = OcppServer::new(TransportConfig::default(), Arc::new(NoOpHandler));
        let addr = server.start("127.0.0.1:0").await.expect("start");
        assert!(addr.port() > 0);
        assert_eq!(server.state, ConnectionState::Connected);
        server.stop().await.expect("stop");
        assert_eq!(server.state, ConnectionState::Closed);
    }

    #[tokio::test]
    async fn server_accepts_valid_ocpp_connection() {
        let (mut server, mut event_rx, addr) = start_test_server(Arc::new(EchoHandler)).await;

        let _client = connect_client(addr, "CP-001").await;

        // Wait for the Connected event.
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .expect("event within timeout")
            .expect("some event");

        assert!(
            matches!(event, TransportEvent::Connected { .. }),
            "expected Connected, got {:?}",
            event
        );

        // Connection should appear in the map.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(server.connection_count(), 1);
        assert!(server.get_connection("CP-001").is_some());

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_rejects_missing_subprotocol() {
        let (mut server, _rx, addr) = start_test_server(Arc::new(NoOpHandler)).await;

        // Connect without Sec-WebSocket-Protocol.
        let url = format!("ws://{}/ocpp/CP-002", addr);
        let result = tokio_tungstenite::connect_async(url.as_str()).await;
        // tokio-tungstenite should surface a handshake failure.
        assert!(result.is_err(), "expected handshake rejection");

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_rejects_invalid_path() {
        let (mut server, _rx, addr) = start_test_server(Arc::new(NoOpHandler)).await;

        let url = format!("ws://{}/invalid/path", addr);
        let mut req =
            tungstenite::client::IntoClientRequest::into_client_request(url.as_str()).unwrap();
        req.headers_mut()
            .insert("sec-websocket-protocol", "ocpp1.6".parse().unwrap());
        let result = tokio_tungstenite::connect_async(req).await;
        assert!(result.is_err(), "expected rejection for bad path");

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_dispatches_call_and_returns_callresult() {
        let (mut server, _rx, addr) = start_test_server(Arc::new(EchoHandler)).await;

        let mut client = connect_client(addr, "CP-003").await;

        // Send a Heartbeat CALL.
        let call = CallMessage::new("Heartbeat".to_string(), json!({})).unwrap();
        let call_id = call.unique_id.clone();
        send_call(&mut client, call).await;

        // Read back the CALLRESULT.
        let response = tokio::time::timeout(std::time::Duration::from_secs(2), client.next())
            .await
            .expect("response within timeout")
            .expect("some frame")
            .expect("ok frame");

        if let tungstenite::Message::Text(text) = response {
            let msg = parse_message(&text).expect("valid OCPP");
            match msg {
                Message::CallResult(r) => assert_eq!(r.unique_id, call_id),
                other => panic!("expected CallResult, got {:?}", other),
            }
        } else {
            panic!("expected text frame");
        }

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_sends_callerror_for_not_supported_action() {
        let (mut server, _rx, addr) = start_test_server(Arc::new(RejectAllHandler)).await;

        let mut client = connect_client(addr, "CP-004").await;

        let call = CallMessage::new("UnknownAction".to_string(), json!({})).unwrap();
        let call_id = call.unique_id.clone();
        send_call(&mut client, call).await;

        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), client.next())
            .await
            .expect("frame within timeout")
            .expect("some frame")
            .expect("ok frame");

        if let tungstenite::Message::Text(text) = frame {
            let msg = parse_message(&text).expect("valid OCPP");
            match msg {
                Message::CallError(e) => {
                    assert_eq!(e.unique_id, call_id);
                    assert_eq!(e.error_code, CallErrorCode::NotImplemented);
                }
                other => panic!("expected CallError, got {:?}", other),
            }
        } else {
            panic!("expected text frame");
        }

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_removes_connection_on_disconnect() {
        let (mut server, mut event_rx, addr) = start_test_server(Arc::new(EchoHandler)).await;

        let mut client = connect_client(addr, "CP-005").await;

        // Wait for Connected event.
        tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        // Close the client.
        client.close(None).await.unwrap();

        // Wait for Disconnected event.
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .expect("disconnect event")
            .expect("some event");

        assert!(
            matches!(event, TransportEvent::Disconnected { .. }),
            "expected Disconnected"
        );

        // Allow the server task time to clean up the map.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(server.connection_count(), 0);

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_cleanup_idle_connections() {
        let config = TransportConfig {
            keep_alive_interval: std::time::Duration::from_millis(1),
            ..Default::default()
        };
        let (server, _rx) = OcppServer::new(config, Arc::new(NoOpHandler));

        let mut info = ConnectionInfo::new("127.0.0.1:9999".to_string(), "server".to_string());
        info.charge_point_id = Some("CP-IDLE".to_string());
        // Fake an old last_activity timestamp.
        info.last_activity = chrono::Utc::now() - chrono::Duration::seconds(10);
        server.connections.insert("CP-IDLE".to_string(), info);

        let removed = server.cleanup_idle_connections().await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(server.connection_count(), 0);
    }

    #[test]
    fn extract_cp_id_valid_paths() {
        assert_eq!(extract_cp_id("/ocpp/CP001"), Some("CP001".to_string()));
        assert_eq!(extract_cp_id("/ocpp/my-cp"), Some("my-cp".to_string()));
    }

    #[test]
    fn extract_cp_id_invalid_paths() {
        assert!(extract_cp_id("/invalid/path").is_none());
        assert!(extract_cp_id("/ocpp/").is_none());
        assert!(extract_cp_id("/ocpp").is_none());
    }
}
