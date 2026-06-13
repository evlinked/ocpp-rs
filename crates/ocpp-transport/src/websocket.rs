//! WebSocket utilities and abstractions for OCPP transport

use crate::{
    error::{TransportError, TransportResult},
    ConnectionInfo, TransportConfig,
};
use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use tracing::{debug, info, warn};

/// WebSocket connection wrapper
pub struct WebSocketConnection<S> {
    /// WebSocket stream
    stream: WebSocketStream<S>,
    /// Connection info
    connection_info: ConnectionInfo,
    /// Configuration
    config: TransportConfig,
}

impl<S> WebSocketConnection<S>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    /// Create new WebSocket connection
    pub fn new(
        stream: WebSocketStream<S>,
        connection_info: ConnectionInfo,
        config: TransportConfig,
    ) -> Self {
        Self {
            stream,
            connection_info,
            config,
        }
    }

    /// Send a message
    pub async fn send_message(&mut self, message: String) -> TransportResult<()> {
        debug!("Sending WebSocket message: {} bytes", message.len());

        // Check message size
        if message.len() > self.config.max_message_size {
            return Err(TransportError::MessageTooLarge {
                size: message.len(),
                limit: self.config.max_message_size,
            });
        }

        // Send with timeout
        timeout(
            self.config.connection_timeout,
            self.stream.send(Message::Text(message)),
        )
        .await
        .map_err(|_| TransportError::Timeout {
            timeout_secs: self.config.connection_timeout.as_secs(),
        })?
        .map_err(TransportError::from)?;

        Ok(())
    }

    /// Receive a message
    pub async fn receive_message(&mut self) -> TransportResult<Option<String>> {
        let message = timeout(self.config.connection_timeout, self.stream.next())
            .await
            .map_err(|_| TransportError::Timeout {
                timeout_secs: self.config.connection_timeout.as_secs(),
            })?;

        match message {
            Some(Ok(msg)) => match msg {
                Message::Text(text) => {
                    debug!("Received WebSocket text message: {} bytes", text.len());

                    // Check message size
                    if text.len() > self.config.max_message_size {
                        return Err(TransportError::MessageTooLarge {
                            size: text.len(),
                            limit: self.config.max_message_size,
                        });
                    }

                    Ok(Some(text))
                }
                Message::Binary(data) => {
                    warn!("Received unexpected binary message: {} bytes", data.len());
                    Ok(None)
                }
                Message::Ping(data) => {
                    debug!("Received ping, sending pong");
                    self.stream
                        .send(Message::Pong(data))
                        .await
                        .map_err(TransportError::from)?;
                    Ok(None)
                }
                Message::Pong(_) => {
                    debug!("Received pong");
                    Ok(None)
                }
                Message::Close(frame) => {
                    info!("Received close frame: {:?}", frame);
                    Err(TransportError::ConnectionClosed {
                        reason: frame
                            .map(|f| f.reason.to_string())
                            .unwrap_or_else(|| "Unknown".to_string()),
                    })
                }
                _ => {
                    warn!("Received unsupported message type");
                    Ok(None)
                }
            },
            Some(Err(e)) => Err(TransportError::from(e)),
            None => Err(TransportError::ConnectionClosed {
                reason: "Stream ended".to_string(),
            }),
        }
    }

    /// Send ping frame
    pub async fn send_ping(&mut self, data: Vec<u8>) -> TransportResult<()> {
        debug!("Sending ping frame");
        self.stream
            .send(Message::Ping(data))
            .await
            .map_err(TransportError::from)?;
        Ok(())
    }

    /// Close the connection
    pub async fn close(&mut self) -> TransportResult<()> {
        info!("Closing WebSocket connection");
        self.stream
            .close(None)
            .await
            .map_err(TransportError::from)?;
        Ok(())
    }

    /// Consume this connection and return independent split sink and stream halves
    /// together with the transport config.
    ///
    /// The returned halves use a `BiLock` that releases while awaiting I/O, so
    /// a send task and a receive task can run concurrently without deadlocking
    /// (unlike sharing a `tokio::sync::Mutex` across an `.await`).
    pub fn into_split(
        self,
    ) -> (
        futures_util::stream::SplitSink<WebSocketStream<S>, Message>,
        futures_util::stream::SplitStream<WebSocketStream<S>>,
        TransportConfig,
    ) {
        let (sink, stream) = self.stream.split();
        (sink, stream, self.config)
    }

    /// Get connection info
    pub fn connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }

    /// Update connection activity
    pub fn update_activity(&mut self) {
        self.connection_info.update_activity();
    }
}

/// WebSocket client utilities
pub mod client {
    use super::*;
    use tokio_tungstenite::connect_async_with_config;
    use url::Url;

    /// Connect to WebSocket server
    pub async fn connect(
        url: &str,
        _subprotocols: &[String],
        config: &TransportConfig,
    ) -> TransportResult<
        WebSocketConnection<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    > {
        info!("Connecting to WebSocket server: {}", url);

        let url = Url::parse(url).map_err(|e| TransportError::ConnectionError {
            message: format!("Invalid URL: {}", e),
        })?;

        // Create WebSocket configuration
        let ws_config = tungstenite::protocol::WebSocketConfig {
            write_buffer_size: config.max_message_size,
            max_write_buffer_size: config.max_message_size * 2,
            max_message_size: Some(config.max_message_size),
            max_frame_size: Some(config.max_message_size),
            accept_unmasked_frames: false,
            ..Default::default()
        };

        let (ws_stream, response) = timeout(
            config.connection_timeout,
            connect_async_with_config(url.clone(), Some(ws_config), false),
        )
        .await
        .map_err(|_| TransportError::Timeout {
            timeout_secs: config.connection_timeout.as_secs(),
        })?
        .map_err(TransportError::from)?;

        info!("WebSocket connection established");
        debug!("Server response: {:?}", response);

        // Extract connection info
        let remote_addr = url.host_str().unwrap_or("unknown").to_string();
        let local_addr = "0.0.0.0:0".to_string(); // Client doesn't know local address
        let mut connection_info = ConnectionInfo::new(remote_addr, local_addr);

        // Set subprotocol if negotiated
        if let Some(protocol) = response.headers().get("sec-websocket-protocol") {
            if let Ok(protocol_str) = protocol.to_str() {
                connection_info.sub_protocol = Some(protocol_str.to_string());
            }
        }

        Ok(WebSocketConnection::new(
            ws_stream,
            connection_info,
            config.clone(),
        ))
    }
}

/// WebSocket server utilities
pub mod server {
    use super::*;
    use tokio_tungstenite::accept_async_with_config;

    /// Accept WebSocket connection from client
    pub async fn accept_connection<S>(
        stream: S,
        config: &TransportConfig,
    ) -> TransportResult<WebSocketConnection<S>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        info!("Accepting WebSocket connection");

        // Create WebSocket configuration
        let ws_config = tungstenite::protocol::WebSocketConfig {
            write_buffer_size: config.max_message_size,
            max_write_buffer_size: config.max_message_size * 2,
            max_message_size: Some(config.max_message_size),
            max_frame_size: Some(config.max_message_size),
            accept_unmasked_frames: false,
            ..Default::default()
        };

        let ws_stream = timeout(
            config.connection_timeout,
            accept_async_with_config(stream, Some(ws_config)),
        )
        .await
        .map_err(|_| TransportError::Timeout {
            timeout_secs: config.connection_timeout.as_secs(),
        })?
        .map_err(TransportError::from)?;

        info!("WebSocket connection accepted");

        // Create connection info (server side doesn't know remote address yet)
        let connection_info = ConnectionInfo::new(
            "unknown".to_string(),
            "0.0.0.0:8080".to_string(), // Default server address
        );

        Ok(WebSocketConnection::new(
            ws_stream,
            connection_info,
            config.clone(),
        ))
    }

    /// Validate WebSocket handshake request
    pub fn validate_handshake_request(
        request: &tungstenite::http::Request<()>,
        allowed_protocols: &[String],
    ) -> Result<Option<String>, String> {
        // Extract path and validate charge point ID
        let path = request.uri().path();
        if !path.starts_with("/ocpp/") {
            return Err("Invalid path: must start with /ocpp/".to_string());
        }

        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() < 3 || parts[2].is_empty() {
            return Err("Invalid path: missing charge point ID".to_string());
        }

        let charge_point_id = parts[2];
        if charge_point_id.len() > 20 {
            return Err("Invalid charge point ID: too long".to_string());
        }

        // Validate subprotocol
        let requested_protocols = request
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|h| h.to_str().ok())
            .map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let selected_protocol = requested_protocols
            .iter()
            .find(|p| allowed_protocols.contains(p))
            .cloned();

        if requested_protocols.is_empty() || selected_protocol.is_none() {
            return Err("No supported subprotocol found".to_string());
        }

        Ok(selected_protocol)
    }
}

/// WebSocket frame types and utilities
pub mod frames {
    /// Frame type
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FrameType {
        Text,
        Binary,
        Ping,
        Pong,
        Close,
    }

    /// Frame statistics
    #[derive(Debug, Default, Clone)]
    pub struct FrameStats {
        pub text_frames: u64,
        pub binary_frames: u64,
        pub ping_frames: u64,
        pub pong_frames: u64,
        pub close_frames: u64,
        pub total_bytes: u64,
    }

    impl FrameStats {
        /// Update statistics for a frame
        pub fn update(&mut self, frame_type: FrameType, size: usize) {
            self.total_bytes += size as u64;
            match frame_type {
                FrameType::Text => self.text_frames += 1,
                FrameType::Binary => self.binary_frames += 1,
                FrameType::Ping => self.ping_frames += 1,
                FrameType::Pong => self.pong_frames += 1,
                FrameType::Close => self.close_frames += 1,
            }
        }

        /// Get total frame count
        pub fn total_frames(&self) -> u64 {
            self.text_frames
                + self.binary_frames
                + self.ping_frames
                + self.pong_frames
                + self.close_frames
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_stats() {
        let mut stats = frames::FrameStats::default();

        stats.update(frames::FrameType::Text, 100);
        stats.update(frames::FrameType::Ping, 4);

        assert_eq!(stats.text_frames, 1);
        assert_eq!(stats.ping_frames, 1);
        assert_eq!(stats.total_bytes, 104);
        assert_eq!(stats.total_frames(), 2);
    }

    #[test]
    fn test_validate_handshake_request() {
        use tungstenite::http::{Request, Uri};

        // Create a mock request
        let uri = Uri::from_static("/ocpp/CP001");
        let request = Request::builder()
            .uri(uri)
            .header("sec-websocket-protocol", "ocpp1.6")
            .body(())
            .unwrap();

        let allowed_protocols = vec!["ocpp1.6".to_string()];
        let result = server::validate_handshake_request(&request, &allowed_protocols);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("ocpp1.6".to_string()));
    }

    #[test]
    fn test_validate_handshake_request_invalid_path() {
        use tungstenite::http::{Request, Uri};

        let uri = Uri::from_static("/invalid/path");
        let request = Request::builder().uri(uri).body(()).unwrap();

        let allowed_protocols = vec!["ocpp1.6".to_string()];
        let result = server::validate_handshake_request(&request, &allowed_protocols);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid path"));
    }

    #[test]
    fn test_validate_handshake_request_no_protocol() {
        use tungstenite::http::{Request, Uri};

        let uri = Uri::from_static("/ocpp/CP001");
        let request = Request::builder().uri(uri).body(()).unwrap();

        let allowed_protocols = vec!["ocpp1.6".to_string()];
        let result = server::validate_handshake_request(&request, &allowed_protocols);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No supported subprotocol"));
    }
}
