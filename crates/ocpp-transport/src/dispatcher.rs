//! Type-erased async action dispatcher: port of Python's `@on`/`@after` decorator semantics.
//!
//! See [`ActionDispatcher`] for the primary API.

use futures::future::BoxFuture;
use ocpp_messages::{CallMessage, OcppAction};
use ocpp_types::{CallErrorCode, CallResultMessage, Message, MessageType, OcppError, OcppResult};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

/// Alias for the type-erased handler stored in the dispatch table.
type HandlerFn = dyn Fn(Value) -> BoxFuture<'static, OcppResult<Value>> + Send + Sync;

/// Type-erased async dispatch table for incoming OCPP CALL messages.
///
/// Port of `charge_point.py`'s `create_route_map()` / `_handle_call()` / `_get_handler()`.
///
/// ## Usage
///
/// ```ignore
/// let mut dispatcher = ActionDispatcher::new();
///
/// dispatcher.on(|req: HeartbeatRequest| async move {
///     Ok(HeartbeatResponse { current_time: Utc::now() })
/// });
///
/// dispatcher.after(|req: AuthorizeRequest| async move {
///     tracing::info!("post-auth hook: {}", req.id_tag);
///     Ok(AuthorizeResponse { id_tag_info: IdTagInfo { status: AuthorizationStatus::Accepted, .. } })
/// });
///
/// // Wire into WebSocketClient:
/// let client = WebSocketClient::new(url, config, Arc::new(dispatcher)).await?;
/// ```
pub struct ActionDispatcher {
    handlers: HashMap<&'static str, Arc<HandlerFn>>,
    after_hooks: HashMap<&'static str, Arc<HandlerFn>>,
}

impl ActionDispatcher {
    /// Create an empty dispatcher.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            after_hooks: HashMap::new(),
        }
    }

    /// Register a primary handler for incoming CALLs matching `Req::ACTION_NAME`.
    ///
    /// Analogous to Python's `@on(Action.xxx)` decorator. Replaces any existing
    /// handler for the same action name.
    pub fn on<Req, Fut, F>(&mut self, handler: F)
    where
        Req: OcppAction + 'static,
        Req::Response: OcppAction + 'static,
        Fut: std::future::Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        let f: Arc<HandlerFn> = Arc::new(move |payload: Value| {
            let handler = handler.clone();
            Box::pin(async move {
                let req: Req = serde_json::from_value(payload).map_err(OcppError::from)?;
                let resp = handler(req).await?;
                serde_json::to_value(resp).map_err(OcppError::from)
            })
        });
        self.handlers.insert(Req::ACTION_NAME, f);
    }

    /// Register a fire-and-forget post-processing hook for `Req::ACTION_NAME`.
    ///
    /// Analogous to Python's `@after(Action.xxx)`. Fires in a detached
    /// `tokio::spawn` after the primary handler's CALLRESULT is returned, so it
    /// never blocks the response path. The hook's return value is ignored.
    pub fn after<Req, Fut, F>(&mut self, hook: F)
    where
        Req: OcppAction + 'static,
        Req::Response: OcppAction + 'static,
        Fut: std::future::Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        let f: Arc<HandlerFn> = Arc::new(move |payload: Value| {
            let hook = hook.clone();
            Box::pin(async move {
                if let Ok(req) = serde_json::from_value::<Req>(payload) {
                    let _ = hook(req).await;
                }
                Ok(Value::Null)
            })
        });
        self.after_hooks.insert(Req::ACTION_NAME, f);
    }

    /// Dispatch an incoming `CallMessage` to the registered primary handler.
    ///
    /// Returns the serialized response `Value` on success.
    ///
    /// - Unknown action → `OcppError::NotSupported`
    /// - Handler returns `Err` → propagated
    /// - After hook (if registered) fires asynchronously via `tokio::spawn` after result is returned.
    pub async fn dispatch(&self, call: &CallMessage) -> OcppResult<Value> {
        let handler =
            self.handlers
                .get(call.action.as_str())
                .ok_or_else(|| OcppError::NotSupported {
                    feature: call.action.clone(),
                })?;

        let result = handler(call.payload.clone()).await;

        // Fire after hook in a detached task — does not block the response path.
        if let Some(after) = self.after_hooks.get(call.action.as_str()) {
            let after = Arc::clone(after);
            let payload = call.payload.clone();
            tokio::spawn(async move {
                let _ = after(payload).await;
            });
        }

        result
    }

    /// Returns `true` if a primary handler is registered for `action_name`.
    pub fn has_handler(&self, action_name: &str) -> bool {
        self.handlers.contains_key(action_name)
    }
}

impl Default for ActionDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// `ActionDispatcher` implements `MessageHandler` so it can be passed directly to
/// `WebSocketClient::new(…, Arc::new(dispatcher))`.
///
/// - Incoming CALL → `dispatch()` → CALLRESULT on success, CALLERROR on error.
/// - Incoming CALLRESULT / CALLERROR → `Ok(None)` (handled by the pending-call map).
#[async_trait::async_trait]
impl crate::MessageHandler for ActionDispatcher {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        let Message::Call(call) = message else {
            // CALLRESULT and CALLERROR are correlated by the pending-call map.
            return Ok(None);
        };

        let unique_id = call.unique_id.clone();

        let response = match self.dispatch(&call).await {
            Ok(payload) => Message::CallResult(CallResultMessage {
                message_type: MessageType::CallResult,
                unique_id,
                payload,
            }),
            Err(OcppError::NotSupported { feature }) => Message::call_error(
                unique_id,
                CallErrorCode::NotImplemented,
                format!("Action '{}' is not implemented", feature),
                None,
            ),
            Err(OcppError::Json { message: msg }) => {
                Message::call_error(unique_id, CallErrorCode::FormationViolation, msg, None)
            }
            Err(e) => {
                Message::call_error(unique_id, CallErrorCode::InternalError, e.to_string(), None)
            }
        };

        Ok(Some(response))
    }

    async fn handle_event(&self, _event: crate::TransportEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageHandler as _;
    use ocpp_messages::{OcppAction, OcppResponse};
    use ocpp_types::OcppError;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicBool, Ordering};

    // ---------- minimal test types ----------

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct EchoRequest {
        message: String,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct EchoResponse {
        echoed: String,
    }

    // EchoResponse doubles as its own Response for test purposes (standard pattern).
    impl OcppAction for EchoRequest {
        const ACTION_NAME: &'static str = "Echo";
        type Response = EchoResponse;
    }

    impl OcppAction for EchoResponse {
        const ACTION_NAME: &'static str = "EchoResponse";
        type Response = EchoResponse;
    }

    impl OcppResponse for EchoResponse {}

    fn echo_call(msg: &str) -> CallMessage {
        CallMessage::new("Echo".to_string(), serde_json::json!({ "message": msg })).unwrap()
    }

    // ---------- dispatch() tests ----------

    #[tokio::test]
    async fn dispatch_happy_path() {
        let mut d = ActionDispatcher::new();
        d.on(|req: EchoRequest| async move {
            Ok(EchoResponse {
                echoed: req.message,
            })
        });

        let result = d.dispatch(&echo_call("hello")).await.unwrap();
        let resp: EchoResponse = serde_json::from_value(result).unwrap();
        assert_eq!(resp.echoed, "hello");
    }

    #[tokio::test]
    async fn dispatch_unknown_action_returns_not_supported() {
        let d = ActionDispatcher::new();
        let err = d.dispatch(&echo_call("ignored")).await.unwrap_err();
        assert!(matches!(err, OcppError::NotSupported { .. }));
    }

    #[tokio::test]
    async fn dispatch_handler_error_propagated() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: EchoRequest| async move {
            Err::<EchoResponse, _>(OcppError::Internal {
                message: "boom".into(),
            })
        });

        let err = d.dispatch(&echo_call("fail")).await.unwrap_err();
        assert!(matches!(err, OcppError::Internal { .. }));
    }

    #[tokio::test]
    async fn dispatch_malformed_payload_returns_json_error() {
        let mut d = ActionDispatcher::new();
        d.on(|req: EchoRequest| async move {
            Ok(EchoResponse {
                echoed: req.message,
            })
        });

        // Inject a payload that cannot be deserialized as EchoRequest.
        let bad_call =
            CallMessage::new("Echo".to_string(), serde_json::json!({ "wrong_field": 42 })).unwrap();
        let err = d.dispatch(&bad_call).await.unwrap_err();
        assert!(matches!(err, OcppError::Json { .. }));
    }

    #[tokio::test]
    async fn after_hook_fires_after_dispatch() {
        let fired = Arc::new(AtomicBool::new(false));
        let fired2 = Arc::clone(&fired);

        let mut d = ActionDispatcher::new();
        d.on(|req: EchoRequest| async move {
            Ok(EchoResponse {
                echoed: req.message,
            })
        });
        d.after(move |_req: EchoRequest| {
            let fired = Arc::clone(&fired2);
            async move {
                fired.store(true, Ordering::SeqCst);
                Ok(EchoResponse {
                    echoed: String::new(),
                })
            }
        });

        let _ = d.dispatch(&echo_call("test")).await.unwrap();

        // Yield to the runtime so the spawned after-hook task can execute.
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        assert!(fired.load(Ordering::SeqCst), "after hook should have fired");
    }

    #[tokio::test]
    async fn after_hook_does_not_block_response() {
        use tokio::time::{sleep, Duration};

        let after_started = Arc::new(AtomicBool::new(false));
        let after_started2 = Arc::clone(&after_started);

        let mut d = ActionDispatcher::new();
        d.on(|req: EchoRequest| async move {
            Ok(EchoResponse {
                echoed: req.message,
            })
        });
        d.after(move |_req: EchoRequest| {
            let started = Arc::clone(&after_started2);
            async move {
                started.store(true, Ordering::SeqCst);
                sleep(Duration::from_millis(100)).await; // slow hook
                Ok(EchoResponse {
                    echoed: String::new(),
                })
            }
        });

        // dispatch() must return before the slow hook finishes
        let start = tokio::time::Instant::now();
        let _ = d.dispatch(&echo_call("timing")).await.unwrap();
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "dispatch() should not wait for the after hook"
        );
    }

    #[tokio::test]
    async fn has_handler_reports_correctly() {
        let mut d = ActionDispatcher::new();
        assert!(!d.has_handler("Echo"));
        d.on(|req: EchoRequest| async move {
            Ok(EchoResponse {
                echoed: req.message,
            })
        });
        assert!(d.has_handler("Echo"));
        assert!(!d.has_handler("Other"));
    }

    // ---------- MessageHandler impl tests ----------

    #[tokio::test]
    async fn message_handler_call_returns_callresult() {
        let mut d = ActionDispatcher::new();
        d.on(|req: EchoRequest| async move {
            Ok(EchoResponse {
                echoed: req.message,
            })
        });

        let result = d
            .handle_message(Message::Call(echo_call("round-trip")))
            .await
            .unwrap();

        let Some(Message::CallResult(cr)) = result else {
            panic!("expected CallResult, got {result:?}");
        };
        let resp: EchoResponse = cr.payload_as().unwrap();
        assert_eq!(resp.echoed, "round-trip");
    }

    #[tokio::test]
    async fn message_handler_unknown_action_returns_callerror_not_implemented() {
        let d = ActionDispatcher::new(); // no handlers
        let result = d
            .handle_message(Message::Call(echo_call("ignored")))
            .await
            .unwrap();

        let Some(Message::CallError(ce)) = result else {
            panic!("expected CallError, got {result:?}");
        };
        assert_eq!(ce.error_code, CallErrorCode::NotImplemented);
    }

    #[tokio::test]
    async fn message_handler_handler_error_returns_callerror_internal() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: EchoRequest| async move {
            Err::<EchoResponse, _>(OcppError::Internal {
                message: "oops".into(),
            })
        });

        let result = d
            .handle_message(Message::Call(echo_call("fail")))
            .await
            .unwrap();

        let Some(Message::CallError(ce)) = result else {
            panic!("expected CallError, got {result:?}");
        };
        assert_eq!(ce.error_code, CallErrorCode::InternalError);
    }

    #[tokio::test]
    async fn message_handler_callresult_passes_through() {
        let d = ActionDispatcher::new();
        let cr = ocpp_types::CallResultMessage {
            message_type: MessageType::CallResult,
            unique_id: "uid1".into(),
            payload: serde_json::json!({}),
        };
        let result = d.handle_message(Message::CallResult(cr)).await.unwrap();
        assert!(result.is_none(), "CALLRESULT should pass through as None");
    }

    #[tokio::test]
    async fn message_handler_callerror_passes_through() {
        let d = ActionDispatcher::new();
        let ce = ocpp_types::CallErrorMessage::new(
            "uid2".into(),
            CallErrorCode::GenericError,
            "upstream error".into(),
            None,
        );
        let result = d.handle_message(Message::CallError(ce)).await.unwrap();
        assert!(result.is_none(), "CALLERROR should pass through as None");
    }
}
