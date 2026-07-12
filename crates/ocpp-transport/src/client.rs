//! WebSocket client implementation for OCPP Charge Points

use crate::{
    error::TransportResult, pending::PendingCallMap, websocket::client::connect, ConnectionState,
    MessageHandler, Transport, TransportConfig, TransportEvent,
};
use futures_util::{SinkExt, StreamExt};
use ocpp_messages::Message;
use ocpp_types::{OcppError, OcppResult};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// WebSocket client for OCPP Charge Points
pub struct WebSocketClient {
    /// Connection ID
    connection_id: Uuid,
    /// Current connection state
    state: Arc<RwLock<ConnectionState>>,
    /// Configuration
    config: TransportConfig,
    /// Message handler
    message_handler: Arc<dyn MessageHandler>,
    /// Message sender channel
    message_tx: Arc<RwLock<Option<mpsc::UnboundedSender<Message>>>>,
    /// In-flight CALL correlation map
    pending_calls: Arc<PendingCallMap>,
    /// Task handles
    task_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
}

impl WebSocketClient {
    /// Create a new WebSocket client
    pub async fn new(
        url: String,
        config: TransportConfig,
        message_handler: Arc<dyn MessageHandler>,
    ) -> TransportResult<Self> {
        info!("Creating WebSocket client for URL: {}", url);

        let connection_id = Uuid::new_v4();
        let state = Arc::new(RwLock::new(ConnectionState::Connecting));
        let message_tx = Arc::new(RwLock::new(None));
        let pending_calls = Arc::new(PendingCallMap::new());
        let task_handles = Arc::new(RwLock::new(Vec::new()));

        let client = Self {
            connection_id,
            state: state.clone(),
            config: config.clone(),
            message_handler: message_handler.clone(),
            message_tx: message_tx.clone(),
            pending_calls: pending_calls.clone(),
            task_handles: task_handles.clone(),
        };

        // Connect to WebSocket server
        client.connect_internal(url).await?;

        Ok(client)
    }

    /// Internal connection method
    async fn connect_internal(&self, url: String) -> TransportResult<()> {
        info!("Connecting to WebSocket server: {}", url);

        // Update state to connecting
        *self.state.write().await = ConnectionState::Connecting;

        // Connect to the WebSocket server
        let ws_connection = connect(&url, &self.config.sub_protocols, &self.config).await?;

        // Update state to connected
        *self.state.write().await = ConnectionState::Connected;

        // Create message channel
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        *self.message_tx.write().await = Some(tx);

        // Send connection event
        let event = TransportEvent::Connected {
            connection_id: self.connection_id,
            remote_addr: url.clone(),
        };
        self.message_handler.handle_event(event).await;

        // Split the WebSocket into independent sink and stream halves.
        // This is the key architectural fix: the old design shared a single
        // Mutex<WebSocketConnection> between a send task and a recv task.
        // Holding the mutex across `receive_message().await` blocked the send
        // task, causing all post-boot-sequence outgoing CALLs to time out.
        // With a split design, a single task uses `tokio::select!` to
        // interleave sends and receives without any shared mutex.
        let (mut ws_sink, mut ws_stream) = ws_connection.into_inner().split();

        let message_handler = self.message_handler.clone();
        let connection_id = self.connection_id;
        let state = self.state.clone();
        let pending_calls = self.pending_calls.clone();

        // Single combined task: interleaves outbound sends (from the mpsc
        // channel) and inbound receives (from the WebSocket stream) via
        // `tokio::select!`.  No shared mutex is needed because only this task
        // touches the sink or the stream.
        let combined_task = tokio::spawn(async move {
            'run: loop {
                tokio::select! {
                    // ── Outbound branch ──────────────────────────────────
                    msg = rx.recv() => {
                        match msg {
                            Some(message) => {
                                match serde_json::to_string(&message) {
                                    Ok(json) => {
                                        if let Err(e) = ws_sink.send(WsMessage::Text(json)).await {
                                            error!("Failed to send WS frame: {}", e);
                                            break 'run;
                                        }
                                    }
                                    Err(e) => error!("Failed to serialize OCPP message: {}", e),
                                }
                            }
                            None => break 'run, // channel closed → disconnect
                        }
                    }
                    // ── Inbound branch ───────────────────────────────────
                    frame = ws_stream.next() => {
                        match frame {
                            Some(Ok(WsMessage::Text(text))) => {
                                match serde_json::from_str::<Message>(&text) {
                                    Ok(message) => {
                                        match &message {
                                            Message::CallResult(r) => {
                                                if !pending_calls.resolve(
                                                    &r.unique_id,
                                                    r.payload.clone(),
                                                ) {
                                                    warn!(
                                                        "CALLRESULT for unknown unique_id '{}'",
                                                        r.unique_id
                                                    );
                                                }
                                            }
                                            Message::CallError(e) => {
                                                if !pending_calls
                                                    .reject(&e.unique_id, OcppError::from(e))
                                                {
                                                    warn!(
                                                        "CALLERROR for unknown unique_id '{}'",
                                                        e.unique_id
                                                    );
                                                }
                                            }
                                            Message::Call(_) => {
                                                match message_handler
                                                    .handle_message(message.clone())
                                                    .await
                                                {
                                                    Ok(Some(response)) => {
                                                        match serde_json::to_string(&response) {
                                                            Ok(resp_json) => {
                                                                if let Err(e) = ws_sink
                                                                    .send(WsMessage::Text(resp_json))
                                                                    .await
                                                                {
                                                                    error!(
                                                                        "Failed to send CALL response: {}",
                                                                        e
                                                                    );
                                                                    break 'run;
                                                                }
                                                            }
                                                            Err(e) => error!(
                                                                "Failed to serialize response: {}",
                                                                e
                                                            ),
                                                        }
                                                    }
                                                    Ok(None) => {}
                                                    Err(e) => error!("Handler error: {}", e),
                                                }
                                            }
                                        }
                                        message_handler
                                            .handle_event(TransportEvent::MessageReceived {
                                                connection_id,
                                                message,
                                            })
                                            .await;
                                    }
                                    Err(e) => error!("Failed to parse WS text frame: {}", e),
                                }
                            }
                            Some(Ok(WsMessage::Ping(data))) => {
                                if let Err(e) = ws_sink.send(WsMessage::Pong(data)).await {
                                    error!("Failed to send Pong: {}", e);
                                    break 'run;
                                }
                            }
                            Some(Ok(WsMessage::Close(_))) | None => break 'run,
                            Some(Ok(_)) => {} // binary, pong, etc. — ignore
                            Some(Err(e)) => {
                                error!("WebSocket error: {}", e);
                                break 'run;
                            }
                        }
                    }
                }
            }

            // Clean up on exit
            pending_calls.cancel_all();
            *state.write().await = ConnectionState::Closed;
            message_handler
                .handle_event(TransportEvent::Disconnected {
                    connection_id,
                    reason: "Connection closed".to_string(),
                })
                .await;
        });

        let mut handles = self.task_handles.write().await;
        handles.push(combined_task);

        Ok(())
    }

    /// Disconnect from Central System
    pub async fn disconnect(&self) -> TransportResult<()> {
        info!("Disconnecting from WebSocket server");

        *self.state.write().await = ConnectionState::Closing;

        // Cancel in-flight calls before dropping the tasks that would resolve them
        self.pending_calls.cancel_all();

        // Cancel all tasks
        let mut handles = self.task_handles.write().await;
        for handle in handles.drain(..) {
            handle.abort();
        }

        *self.state.write().await = ConnectionState::Closed;

        Ok(())
    }

    /// Return a reference-counted handle to the in-flight call map.
    ///
    /// Register a `unique_id` here *before* sending the CALL frame so the
    /// matching CALLRESULT/CALLERROR can be correlated back by the recv loop.
    pub fn pending_calls(&self) -> Arc<PendingCallMap> {
        self.pending_calls.clone()
    }

    /// Get connection state (async)
    pub async fn state_async(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        matches!(*self.state.read().await, ConnectionState::Connected)
    }
}

#[async_trait::async_trait]
impl Transport for WebSocketClient {
    async fn send_message(&self, message: Message) -> OcppResult<()> {
        debug!("Sending message: {}", message.unique_id());

        let tx = self.message_tx.read().await;
        if let Some(sender) = tx.as_ref() {
            sender.send(message).map_err(|_| OcppError::Transport {
                message: "Failed to send message: channel closed".to_string(),
            })?;
            Ok(())
        } else {
            Err(OcppError::Transport {
                message: "Not connected".to_string(),
            })
        }
    }

    async fn close(&self) -> OcppResult<()> {
        self.disconnect().await.map_err(|e| OcppError::Transport {
            message: format!("Failed to close connection: {}", e),
        })
    }

    fn state(&self) -> ConnectionState {
        // Sync accessor; use state_async() from async contexts.
        ConnectionState::Connected
    }

    fn connection_id(&self) -> Uuid {
        self.connection_id
    }
}

/// Legacy OcppClient for backward compatibility
pub struct OcppClient {
    /// Connection ID
    connection_id: Uuid,
    /// Current connection state
    state: ConnectionState,
    /// Configuration
    _config: TransportConfig,
    /// Event sender
    _event_tx: mpsc::UnboundedSender<TransportEvent>,
}

impl OcppClient {
    /// Create a new OCPP client
    pub fn new(config: TransportConfig) -> (Self, mpsc::UnboundedReceiver<TransportEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let client = Self {
            connection_id: Uuid::new_v4(),
            state: ConnectionState::Closed,
            _config: config,
            _event_tx: event_tx,
        };
        (client, event_rx)
    }

    /// Connect to OCPP Central System
    pub async fn connect(&mut self, url: &str) -> TransportResult<()> {
        info!("Connecting to OCPP Central System at {}", url);
        self.state = ConnectionState::Connecting;

        // TODO: Implement actual WebSocket connection

        self.state = ConnectionState::Connected;
        Ok(())
    }

    /// Disconnect from Central System
    pub async fn disconnect(&mut self) -> TransportResult<()> {
        info!("Disconnecting from OCPP Central System");
        self.state = ConnectionState::Closing;

        // TODO: Implement actual disconnect logic

        self.state = ConnectionState::Closed;
        Ok(())
    }

    /// Start the client event loop
    pub async fn run(&mut self) -> TransportResult<()> {
        info!("Starting OCPP client");

        // TODO: Implement event loop

        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for OcppClient {
    async fn send_message(&self, message: Message) -> OcppResult<()> {
        debug!("Sending message: {}", message.unique_id());

        // TODO: Implement message sending

        Ok(())
    }

    async fn close(&self) -> OcppResult<()> {
        info!("Closing OCPP client connection");

        // TODO: Implement close logic

        Ok(())
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    fn connection_id(&self) -> Uuid {
        self.connection_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageHandler;
    use std::sync::Arc;

    // Mock message handler for testing
    struct MockMessageHandler;

    #[async_trait::async_trait]
    impl MessageHandler for MockMessageHandler {
        async fn handle_message(&self, _message: Message) -> OcppResult<Option<Message>> {
            Ok(None)
        }

        async fn handle_event(&self, _event: TransportEvent) {
            // Do nothing
        }
    }

    #[tokio::test]
    async fn test_client_creation() {
        let config = TransportConfig::default();
        let (client, _rx) = OcppClient::new(config);
        assert_eq!(client.state(), ConnectionState::Closed);
        assert!(!client.connection_id().is_nil());
    }

    #[tokio::test]
    async fn test_client_connect() {
        let config = TransportConfig::default();
        let (mut client, _rx) = OcppClient::new(config);

        // This will fail in tests since we don't have an actual server
        // but it tests the state changes
        let _result = client.connect("ws://localhost:8080/ocpp/test").await;
    }

    #[tokio::test]
    async fn test_websocket_client_creation() {
        let config = TransportConfig::default();
        let handler = Arc::new(MockMessageHandler);

        // This will fail in tests since we don't have an actual server
        // but it tests the creation logic
        let result =
            WebSocketClient::new("ws://localhost:8080/ocpp/test".to_string(), config, handler)
                .await;

        // We expect this to fail in tests due to no server
        assert!(result.is_err());
    }
}
