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

    /// Consume this wrapper and return the inner [`WebSocketStream`].
    ///
    /// Used by [`crate::client::WebSocketClient`] to split the stream into independent
    /// sink and stream halves so that send and receive can proceed
    /// concurrently without a shared mutex.
    pub fn into_inner(self) -> WebSocketStream<S> {
        self.stream
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
        subprotocols: &[String],
        config: &TransportConfig,
    ) -> TransportResult<
        WebSocketConnection<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    > {
        use tungstenite::client::IntoClientRequest;

        info!("Connecting to WebSocket server: {}", url);

        let url = Url::parse(url).map_err(|e| TransportError::ConnectionError {
            message: format!("Invalid URL: {}", e),
        })?;

        // Build the handshake request and offer the configured OCPP
        // subprotocols via `Sec-WebSocket-Protocol`. An OCPP CSMS (and the
        // `OcppServer` in this crate) rejects the upgrade with HTTP 400 unless
        // the client offers a subprotocol the server accepts (its configured
        // set, e.g. `ocpp1.6` / `ocpp2.0.1`), so this header is mandatory
        // rather than cosmetic.
        let mut request =
            url.as_str()
                .into_client_request()
                .map_err(|e| TransportError::ConnectionError {
                    message: format!("Invalid handshake request: {}", e),
                })?;
        if !subprotocols.is_empty() {
            let offered = subprotocols.join(", ");
            request.headers_mut().insert(
                "sec-websocket-protocol",
                offered
                    .parse()
                    .map_err(|_| TransportError::ConnectionError {
                        message: format!("Invalid subprotocol value: {:?}", subprotocols),
                    })?,
            );
        }

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
            connect_async_with_config(request, Some(ws_config), false),
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

    /// Extract the charge-point identity from a WebSocket connect path.
    ///
    /// Ported from the mobilityhouse/ocpp reference `extract_charge_point_id`
    /// (`ocpp/charge_point.py`). The connect path is an attacker-influenceable
    /// trust boundary — the returned id becomes the routing key for every
    /// subsequent CALL — so parsing is deliberately strict:
    ///
    /// * query strings (`?…`) and fragments (`#…`) are stripped (the `urlparse`
    ///   `.path` component),
    /// * the path is split on `/` and empty segments are discarded, which
    ///   collapses leading, trailing and repeated slashes,
    /// * the last non-empty segment is taken and trimmed of surrounding
    ///   whitespace,
    /// * a path with no usable segment — or a whitespace-only segment — yields
    ///   `None` (rejected) rather than becoming a live routing key.
    ///
    /// The returned slice borrows from `path`. Rust has no nullable `&str`, so
    /// the reference's `None`-input row maps to the empty-string row here (both
    /// return `None`).
    pub fn extract_charge_point_id(path: &str) -> Option<&str> {
        // urlparse(path).path — drop any query string / fragment.
        let clean = path.split(['?', '#']).next().unwrap_or(path);

        // Last non-empty segment, whitespace-trimmed; empty ⇒ None.
        clean
            .split('/')
            .rfind(|segment| !segment.is_empty())
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }

    /// Validate WebSocket handshake request
    pub fn validate_handshake_request(
        request: &tungstenite::http::Request<()>,
        allowed_protocols: &[String],
    ) -> Result<Option<String>, String> {
        // Extract path and validate charge point ID
        let path = request.uri().path();
        // Connect-contract policy (option B): the charge-point id must sit
        // under the `/ocpp/` prefix. Robust extraction of the id itself is
        // delegated to `extract_charge_point_id`, which rejects whitespace-only
        // / empty segments so they can never become a routing key.
        let remainder = path
            .strip_prefix("/ocpp/")
            .ok_or_else(|| "Invalid path: must start with /ocpp/".to_string())?;

        let charge_point_id = extract_charge_point_id(remainder)
            .ok_or_else(|| "Invalid path: missing charge point ID".to_string())?;

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

    /// Trust-boundary regression: the `/ocpp/` prefix with no usable id — bare
    /// or (once a segment survives URI parsing) whitespace-only — must be
    /// rejected, not accepted as a live routing key. Previously a `/ocpp/   `
    /// path passed the only guard (`len() > 20`). Whitespace-only rejection is
    /// pinned directly on the helper below (`…whitespace_only_segment`), since a
    /// raw-whitespace segment is not expressible through `http::Uri`; here we
    /// pin the reachable sibling: the bare `/ocpp/` prefix with no id.
    #[test]
    fn test_validate_handshake_request_missing_id_rejected() {
        use tungstenite::http::{Request, Uri};

        let uri = Uri::from_static("/ocpp/");
        let request = Request::builder()
            .uri(uri)
            .header("sec-websocket-protocol", "ocpp1.6")
            .body(())
            .unwrap();

        let allowed_protocols = vec!["ocpp1.6".to_string()];
        let result = server::validate_handshake_request(&request, &allowed_protocols);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing charge point ID"));
    }

    // --- Ported from mobilityhouse/ocpp tests/test_charge_point_connection.py
    //     (TestExtractChargePointId). One assertion per reference row, plus the
    //     trust-boundary negatives. `extract_charge_point_id` is the faithful
    //     reference contract; `validate_handshake_request` layers the `/ocpp/`
    //     policy on top of it. Rust has no nullable `&str`, so the reference's
    //     `None`-input row folds into the empty-string row.

    #[test]
    fn test_extract_charge_point_id_simple_path() {
        assert_eq!(server::extract_charge_point_id("/CP001"), Some("CP001"));
    }

    #[test]
    fn test_extract_charge_point_id_nested_path() {
        assert_eq!(
            server::extract_charge_point_id("/ocpp/CP001"),
            Some("CP001")
        );
    }

    #[test]
    fn test_extract_charge_point_id_deeply_nested_path() {
        assert_eq!(
            server::extract_charge_point_id("/api/v1/ocpp/CP001"),
            Some("CP001")
        );
    }

    #[test]
    fn test_extract_charge_point_id_trailing_slash() {
        assert_eq!(server::extract_charge_point_id("/CP001/"), Some("CP001"));
    }

    #[test]
    fn test_extract_charge_point_id_root_path_returns_none() {
        assert_eq!(server::extract_charge_point_id("/"), None);
    }

    #[test]
    fn test_extract_charge_point_id_empty_string_returns_none() {
        // Also covers the reference's `None`-input row (no nullable &str in Rust).
        assert_eq!(server::extract_charge_point_id(""), None);
    }

    #[test]
    fn test_extract_charge_point_id_path_without_leading_slash() {
        assert_eq!(server::extract_charge_point_id("CP001"), Some("CP001"));
    }

    #[test]
    fn test_extract_charge_point_id_path_with_query_string() {
        assert_eq!(
            server::extract_charge_point_id("/CP001?token=abc123"),
            Some("CP001")
        );
    }

    #[test]
    fn test_extract_charge_point_id_path_with_fragment() {
        assert_eq!(
            server::extract_charge_point_id("/CP001#section"),
            Some("CP001")
        );
    }

    #[test]
    fn test_extract_charge_point_id_whitespace_only_segment() {
        assert_eq!(server::extract_charge_point_id("/   "), None);
    }

    #[test]
    fn test_extract_charge_point_id_with_special_chars() {
        assert_eq!(
            server::extract_charge_point_id("/CP-001_v2"),
            Some("CP-001_v2")
        );
    }

    #[test]
    fn test_extract_charge_point_id_with_dots() {
        assert_eq!(
            server::extract_charge_point_id("/EVB-P12354.00.01"),
            Some("EVB-P12354.00.01")
        );
    }

    #[test]
    fn test_extract_charge_point_id_multiple_slashes() {
        assert_eq!(server::extract_charge_point_id("///CP001"), Some("CP001"));
    }
}
