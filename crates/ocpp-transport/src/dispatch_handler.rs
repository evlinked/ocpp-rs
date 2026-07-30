//! Server-side adapter bridging a typed [`ActionDispatcher`] to the transport
//! [`MessageHandler`].
//!
//! Ports the **side-agnostic** `_handle_call()` routing from
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)
//! to the server (CSMS) direction: the same `@on` route map that the charge
//! point already uses (`crates/ocpp-cp/src/message_handler.rs`) now drives the
//! Central System. An incoming `CALL` is looked up in the route map, the
//! handler runs, and its result becomes a `CALLRESULT` (success) or `CALLERROR`
//! (failure) — using the same [`build_call_error`](crate::server) mapping as the
//! inline server receive loop so error codes stay consistent.
//!
//! Only incoming `CALL` frames are dispatched here. `CALLRESULT`/`CALLERROR`
//! frames are responses to CSMS-initiated CALLs and are correlated by the
//! pending-call map (tracked in #30); they are intentionally ignored.

use std::sync::Arc;

use async_trait::async_trait;
use ocpp_messages::{ActionDispatcher, Message};
use ocpp_types::OcppResult;

use crate::server::build_call_error;
use crate::{MessageHandler, TransportEvent};

/// A [`MessageHandler`] that routes incoming `CALL`s through an
/// [`ActionDispatcher`].
///
/// Build an `ActionDispatcher`, register `@on`/`@after` handlers (and, once
/// available via #37, a schema validator with `with_validator`), wrap it in an
/// `Arc`, and hand it to [`DispatchHandler::new`]. Pass the result to
/// [`OcppServer::new`](crate::server::OcppServer::new) to give the CSMS the same
/// typed routing the charge point already enjoys:
///
/// ```ignore
/// let mut dispatcher = ActionDispatcher::new();
/// dispatcher.on(|_req: HeartbeatRequest| async move {
///     Ok(HeartbeatResponse { current_time: Utc::now() })
/// });
/// let handler = Arc::new(DispatchHandler::new(Arc::new(dispatcher)));
/// let (server, events) = OcppServer::new(TransportConfig::default(), handler);
/// ```
///
/// The adapter is deliberately validation-agnostic: schema validation, when
/// enabled, happens *inside* [`ActionDispatcher::dispatch`], and its
/// `ValidationError` is mapped here to a `FormationViolation` CALLERROR via the
/// shared `build_call_error` helper in [`crate::server`].
pub struct DispatchHandler {
    dispatcher: Arc<ActionDispatcher>,
}

impl DispatchHandler {
    /// Wrap a shared [`ActionDispatcher`] as a transport [`MessageHandler`].
    pub fn new(dispatcher: Arc<ActionDispatcher>) -> Self {
        Self { dispatcher }
    }

    /// Borrow the underlying dispatcher (e.g. to inspect registered handlers).
    pub fn dispatcher(&self) -> &Arc<ActionDispatcher> {
        &self.dispatcher
    }
}

#[async_trait]
impl MessageHandler for DispatchHandler {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        match message {
            Message::Call(call) => {
                let unique_id = call.unique_id.clone();
                // Thread the routing dispatcher's negotiated OCPP version into the
                // CALLERROR builder so an unrouted-action `NotSupported` cause
                // embeds it byte-exactly (`… not supported by OCPP1.6.`), matching
                // the reference's `_raise_key_error` (issue #404). `None` for a
                // version-generic dispatcher keeps the version-agnostic fallback.
                let ocpp_version = self.dispatcher.ocpp_version();
                let response = match self.dispatcher.dispatch(&call).await {
                    Ok(payload) => Message::call_result(unique_id.clone(), payload)
                        // Serialising a handler's own response should never fail;
                        // if it somehow does, surface a CALLERROR rather than
                        // silently dropping the frame.
                        .unwrap_or_else(|e| build_call_error(&unique_id, &e, ocpp_version)),
                    Err(e) => build_call_error(&unique_id, &e, ocpp_version),
                };
                Ok(Some(response))
            }
            // CALLRESULT / CALLERROR are responses to CSMS-initiated CALLs and
            // are correlated by the pending-call map (#30), not dispatched here.
            // Matches the existing server receive-loop behaviour (Ok(None)).
            _ => Ok(None),
        }
    }

    async fn handle_event(&self, _event: TransportEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ocpp_messages::v16j::{HeartbeatRequest, HeartbeatResponse};
    use ocpp_types::{CallErrorCode, CallErrorMessage, OcppError};

    fn heartbeat_dispatcher() -> Arc<ActionDispatcher> {
        let mut d = ActionDispatcher::new();
        d.on(|_req: HeartbeatRequest| async move {
            Ok(HeartbeatResponse {
                current_time: Utc::now(),
            })
        });
        Arc::new(d)
    }

    /// Extract the [`CallErrorMessage`] from a handler response, panicking with a
    /// helpful message otherwise. The untagged `Message` enum can't reliably
    /// round-trip a CALLERROR, so we match the variant directly.
    fn expect_call_error(msg: Option<Message>) -> CallErrorMessage {
        match msg {
            Some(Message::CallError(e)) => e,
            other => panic!("expected CALLERROR, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_handler_returns_call_result_on_success() {
        let handler = DispatchHandler::new(heartbeat_dispatcher());
        let call = Message::call("Heartbeat".to_string(), serde_json::json!({})).unwrap();
        let id = call.unique_id().to_string();

        let resp = handler.handle_message(call).await.unwrap();
        match resp {
            Some(Message::CallResult(r)) => {
                assert_eq!(r.unique_id, id);
                assert!(r.payload.get("currentTime").is_some());
            }
            other => panic!("expected CALLRESULT, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_handler_unknown_action_returns_not_supported() {
        let handler = DispatchHandler::new(heartbeat_dispatcher());
        let call = Message::call("NoSuchAction".to_string(), serde_json::json!({})).unwrap();
        let id = call.unique_id().to_string();

        let err = expect_call_error(handler.handle_message(call).await.unwrap());
        assert_eq!(err.unique_id, id);
        assert_eq!(err.error_code, CallErrorCode::NotSupported);
    }

    #[tokio::test]
    async fn dispatch_handler_handler_error_returns_internal_error() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: HeartbeatRequest| async move {
            Err::<HeartbeatResponse, _>(OcppError::Internal {
                message: "boom".to_string(),
            })
        });
        let handler = DispatchHandler::new(Arc::new(d));
        let call = Message::call("Heartbeat".to_string(), serde_json::json!({})).unwrap();

        let err = expect_call_error(handler.handle_message(call).await.unwrap());
        assert_eq!(err.error_code, CallErrorCode::InternalError);
    }

    /// A `ValidationError` — the variant the schema validator (#37) returns —
    /// maps to a `FormationViolation` CALLERROR. This proves the mapping the
    /// validator relies on, independent of where the error originates.
    #[tokio::test]
    async fn dispatch_handler_validation_error_maps_to_formation_violation() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: HeartbeatRequest| async move {
            Err::<HeartbeatResponse, _>(OcppError::ValidationError {
                message: "schema mismatch".to_string(),
            })
        });
        let handler = DispatchHandler::new(Arc::new(d));
        let call = Message::call("Heartbeat".to_string(), serde_json::json!({})).unwrap();

        let err = expect_call_error(handler.handle_message(call).await.unwrap());
        assert_eq!(err.error_code, CallErrorCode::FormationViolation);
    }

    /// A structurally-invalid payload (missing a required field) fails handler
    /// deserialization with `OcppError::Json`, which also maps to
    /// `FormationViolation` — the same code the schema validator produces.
    #[tokio::test]
    async fn dispatch_handler_malformed_payload_returns_formation_violation() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: ocpp_messages::v16j::AuthorizeRequest| async move {
            // Never reached — the payload fails to deserialize before the
            // handler runs; the Err only satisfies the return type.
            Err::<ocpp_messages::v16j::AuthorizeResponse, _>(OcppError::Internal {
                message: "unreachable".to_string(),
            })
        });
        let handler = DispatchHandler::new(Arc::new(d));
        // Authorize requires `idTag`; an empty object can't deserialize.
        let call = Message::call("Authorize".to_string(), serde_json::json!({})).unwrap();

        let err = expect_call_error(handler.handle_message(call).await.unwrap());
        assert_eq!(err.error_code, CallErrorCode::FormationViolation);
    }

    #[tokio::test]
    async fn dispatch_handler_ignores_non_call_frames() {
        let handler = DispatchHandler::new(heartbeat_dispatcher());

        let call_result = Message::call_result("abc".to_string(), serde_json::json!({})).unwrap();
        assert!(handler.handle_message(call_result).await.unwrap().is_none());

        let call_error = Message::call_error(
            "def".to_string(),
            CallErrorCode::InternalError,
            "x".to_string(),
            None,
        );
        assert!(handler.handle_message(call_error).await.unwrap().is_none());
    }
}

/// End-to-end tests: a real `OcppServer` backed by a `DispatchHandler`, driven
/// over a WebSocket client, proving incoming CALLs route through the typed
/// `@on` table and produce the expected CALLRESULT / CALLERROR frames.
#[cfg(test)]
mod ws_tests {
    use super::*;
    use crate::{server::OcppServer, MessageHandler, TransportConfig};
    use chrono::Utc;
    use futures_util::{SinkExt, StreamExt};
    use ocpp_messages::v16j::{HeartbeatRequest, HeartbeatResponse};
    use ocpp_types::{CallErrorCode, OcppError};
    use std::net::SocketAddr;
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
    };

    async fn start_server(handler: Arc<dyn MessageHandler>) -> (OcppServer, SocketAddr) {
        let (mut server, _rx) = OcppServer::new(TransportConfig::default(), handler);
        server.start("127.0.0.1:0").await.expect("server start");
        let addr = server.local_addr().unwrap();
        (server, addr)
    }

    fn ocpp_request(addr: SocketAddr, cp_id: &str) -> Request<()> {
        let mut req = format!("ws://{}/ocpp/{}", addr, cp_id)
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("sec-websocket-protocol", "ocpp1.6".parse().unwrap());
        req
    }

    /// Send `call`, return the single text frame the server replies with.
    async fn round_trip(addr: SocketAddr, cp_id: &str, call: &Message) -> String {
        let (mut ws, _) = connect_async(ocpp_request(addr, cp_id)).await.unwrap();
        ws.send(WsMsg::Text(serde_json::to_string(call).unwrap()))
            .await
            .unwrap();
        let frame = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting for response")
            .expect("stream ended")
            .expect("WS error");
        match frame {
            WsMsg::Text(t) => t,
            other => panic!("expected text frame, got {other:?}"),
        }
    }

    fn heartbeat_handler() -> Arc<dyn MessageHandler> {
        let mut d = ActionDispatcher::new();
        d.on(|_req: HeartbeatRequest| async move {
            Ok(HeartbeatResponse {
                current_time: Utc::now(),
            })
        });
        Arc::new(DispatchHandler::new(Arc::new(d)))
    }

    #[tokio::test]
    async fn server_routes_call_to_registered_handler() {
        let (mut server, addr) = start_server(heartbeat_handler()).await;
        let call = Message::call("Heartbeat".to_string(), serde_json::json!({})).unwrap();
        let id = call.unique_id().to_string();

        let text = round_trip(addr, "CP001", &call).await;
        let msg: Message = serde_json::from_str(&text).unwrap();
        assert!(
            matches!(&msg, Message::CallResult(r) if r.unique_id == id),
            "expected CALLRESULT for id={id}, got {msg:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_unknown_action_returns_not_supported() {
        let (mut server, addr) = start_server(heartbeat_handler()).await;
        let call = Message::call("BootNotification".to_string(), serde_json::json!({})).unwrap();
        let id = call.unique_id().to_string();

        let text = round_trip(addr, "CP002", &call).await;
        let err: ocpp_types::CallErrorMessage = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("expected CALLERROR, parse error {e}\nraw: {text}"));
        assert_eq!(err.unique_id, id);
        assert_eq!(err.error_code, CallErrorCode::NotSupported);

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_handler_error_returns_callerror() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: HeartbeatRequest| async move {
            Err::<HeartbeatResponse, _>(OcppError::Internal {
                message: "kaboom".to_string(),
            })
        });
        let (mut server, addr) = start_server(Arc::new(DispatchHandler::new(Arc::new(d)))).await;
        let call = Message::call("Heartbeat".to_string(), serde_json::json!({})).unwrap();

        let text = round_trip(addr, "CP003", &call).await;
        let err: ocpp_types::CallErrorMessage = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("expected CALLERROR, parse error {e}\nraw: {text}"));
        assert_eq!(err.error_code, CallErrorCode::InternalError);

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_malformed_payload_returns_formation_violation() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: ocpp_messages::v16j::AuthorizeRequest| async move {
            Err::<ocpp_messages::v16j::AuthorizeResponse, _>(OcppError::Internal {
                message: "unreachable".to_string(),
            })
        });
        let (mut server, addr) = start_server(Arc::new(DispatchHandler::new(Arc::new(d)))).await;
        // Authorize requires `idTag`; an empty object fails to deserialize.
        let call = Message::call("Authorize".to_string(), serde_json::json!({})).unwrap();

        let text = round_trip(addr, "CP004", &call).await;
        let err: ocpp_types::CallErrorMessage = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("expected CALLERROR, parse error {e}\nraw: {text}"));
        assert_eq!(err.error_code, CallErrorCode::FormationViolation);

        server.stop().await.unwrap();
    }
}
