//! Type-safe action dispatcher for incoming OCPP CALL messages.
//!
//! Ports the `@on`/`@after` decorator semantics from
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py):
//!   - `create_route_map()` / `_get_handler()` → `ActionDispatcher::on()`
//!   - `_handle_call()` dispatch logic  → `ActionDispatcher::dispatch()`
//!   - `@after` hook invocation          → `ActionDispatcher::after()` + `tokio::spawn`

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ocpp_types::{OcppError, OcppResult};
use serde_json::Value;

use crate::schema_validation::SchemaValidator;
use crate::{CallMessage, OcppAction};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Type-erased on-handler: `Value` payload in, serialised response `Value` out.
type HandlerFn = Box<dyn Fn(Value) -> BoxFuture<OcppResult<Value>> + Send + Sync>;

/// Type-erased after-hook: `Value` payload in, fire-and-forget (`()`).
type AfterFn = Box<dyn Fn(Value) -> BoxFuture<()> + Send + Sync>;

/// Type-safe dispatcher for incoming OCPP CALL messages.
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
///     tracing::info!("post-auth: {}", req.id_tag);
/// });
///
/// let dispatcher = Arc::new(dispatcher);
/// let response_payload = dispatcher.dispatch(&call_msg).await?;
/// ```
///
/// ## Thread safety
///
/// `ActionDispatcher` is `Send + Sync` and can be wrapped in `Arc` for sharing
/// across tasks once all handlers have been registered.
pub struct ActionDispatcher {
    handlers: HashMap<&'static str, HandlerFn>,
    after_hooks: HashMap<&'static str, AfterFn>,
    /// Optional JSON Schema validator. When present, every incoming CALL
    /// payload is validated against its action schema before the handler is
    /// invoked. Ports the `_validate()` call at the top of `_handle_call()` in
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
    /// Shared behind `Arc` so one validator (78 compiled schemas) can back
    /// several dispatchers without re-parsing.
    validator: Option<Arc<SchemaValidator>>,
}

impl ActionDispatcher {
    /// Create an empty dispatcher with no schema validation.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            after_hooks: HashMap::new(),
            validator: None,
        }
    }

    /// Attach a [`SchemaValidator`] so `dispatch()` validates each incoming
    /// CALL payload against its action schema before dispatch.
    ///
    /// Builder-style: returns `self` for chaining after `new()`.
    pub fn with_validator(mut self, validator: Arc<SchemaValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Returns `true` if a schema validator is attached.
    pub fn has_validator(&self) -> bool {
        self.validator.is_some()
    }

    /// Register a typed `@on` handler for `Req::ACTION_NAME`.
    ///
    /// Replaces any previously registered handler for the same action.
    /// The closure receives a deserialised `Req` and must return
    /// `OcppResult<Req::Response>`.
    pub fn on<Req, Fut, F>(&mut self, handler: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        let erased: HandlerFn = Box::new(move |raw: Value| {
            let h = handler.clone();
            Box::pin(async move {
                let req: Req = serde_json::from_value(raw).map_err(|e| OcppError::Json {
                    message: e.to_string(),
                })?;
                let resp = h(req).await?;
                serde_json::to_value(resp).map_err(|e| OcppError::Json {
                    message: e.to_string(),
                })
            })
        });
        self.handlers.insert(Req::ACTION_NAME, erased);
    }

    /// Register a fire-and-forget `@after` hook for `Req::ACTION_NAME`.
    ///
    /// The hook receives the same deserialised request as the `@on` handler.
    /// Its return value is ignored; if deserialisation fails the hook is
    /// silently skipped. The hook is spawned via [`tokio::spawn`] and does
    /// not block the CALLRESULT response path.
    pub fn after<Req, Fut, F>(&mut self, hook: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = ()> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        let erased: AfterFn = Box::new(move |raw: Value| {
            let h = hook.clone();
            Box::pin(async move {
                if let Ok(req) = serde_json::from_value::<Req>(raw) {
                    h(req).await;
                }
            })
        });
        self.after_hooks.insert(Req::ACTION_NAME, erased);
    }

    /// Dispatch an incoming [`CallMessage`] to the registered `@on` handler.
    ///
    /// Returns the serialised response `Value` to wrap in a CALLRESULT, or
    /// `OcppError::NotSupported` when no handler is registered for
    /// `call.action`.
    ///
    /// If an `@after` hook is registered, it is spawned via `tokio::spawn`
    /// after the handler returns successfully (non-blocking).
    pub async fn dispatch(&self, call: &CallMessage) -> OcppResult<Value> {
        let action = call.action.as_str();

        // Schema-validate the incoming payload before touching the handler,
        // mirroring `_handle_call()` in charge_point.py which runs `_validate()`
        // first and short-circuits to a CALLERROR on failure. A malformed
        // payload therefore never reaches handler deserialization.
        if let Some(validator) = &self.validator {
            validator.validate_call(action, &call.payload)?;
        }

        let handler = self
            .handlers
            .get(action)
            .ok_or_else(|| OcppError::NotSupported {
                feature: action.to_string(),
            })?;

        let payload = call.payload.clone();
        let response = handler(payload.clone()).await?;

        if let Some(after) = self.after_hooks.get(action) {
            tokio::spawn(after(payload));
        }

        Ok(response)
    }

    /// Returns `true` if a handler is registered for `action`.
    pub fn has_handler(&self, action: &str) -> bool {
        self.handlers.contains_key(action)
    }

    /// Returns the number of registered `@on` handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

impl Default for ActionDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: HandlerFn and AfterFn are Box<dyn ... + Send + Sync>, so the
// containing HashMap is Send + Sync, making ActionDispatcher Send + Sync.
// The compiler derives this automatically but we assert it here explicitly.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<ActionDispatcher>();
    }
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OcppResponse;
    use serde::{Deserialize, Serialize};
    use std::sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    };
    use std::time::Duration;
    use tokio::sync::Notify;

    // --- minimal test action pair ---

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct PingRequest {
        pub nonce: u32,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct PingResponse {
        pub echoed: u32,
    }

    impl OcppAction for PingRequest {
        const ACTION_NAME: &'static str = "Ping";
        type Response = PingResponse;
    }

    impl OcppAction for PingResponse {
        const ACTION_NAME: &'static str = "PingResponse";
        type Response = PingResponse;
    }

    impl OcppResponse for PingResponse {}

    // --- second action for multi-handler tests ---

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct PongRequest {
        pub value: String,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct PongResponse {
        pub value: String,
    }

    impl OcppAction for PongRequest {
        const ACTION_NAME: &'static str = "Pong";
        type Response = PongResponse;
    }

    impl OcppAction for PongResponse {
        const ACTION_NAME: &'static str = "PongResponse";
        type Response = PongResponse;
    }

    impl OcppResponse for PongResponse {}

    // --- helpers ---

    fn ping_call(nonce: u32) -> CallMessage {
        CallMessage::new("Ping".to_string(), serde_json::json!({ "nonce": nonce })).unwrap()
    }

    fn pong_call(value: &str) -> CallMessage {
        CallMessage::new("Pong".to_string(), serde_json::json!({ "value": value })).unwrap()
    }

    // --- tests ---

    #[tokio::test]
    async fn dispatch_known_action_returns_response() {
        let mut d = ActionDispatcher::new();
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });

        let resp = d.dispatch(&ping_call(42)).await.unwrap();
        assert_eq!(resp["echoed"], 42);
    }

    #[tokio::test]
    async fn dispatch_unknown_action_returns_not_supported() {
        let d = ActionDispatcher::new();
        let call = CallMessage::new("UnknownAction".to_string(), serde_json::json!({})).unwrap();
        let err = d.dispatch(&call).await.unwrap_err();
        assert!(
            matches!(err, OcppError::NotSupported { ref feature } if feature == "UnknownAction"),
            "expected NotSupported, got {err:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_handler_error_is_propagated() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: PingRequest| async move {
            Err::<PingResponse, _>(OcppError::Internal {
                message: "boom".to_string(),
            })
        });

        let err = d.dispatch(&ping_call(1)).await.unwrap_err();
        assert!(
            matches!(err, OcppError::Internal { ref message } if message == "boom"),
            "expected Internal error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_malformed_payload_returns_json_error() {
        let mut d = ActionDispatcher::new();
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });

        // payload has wrong field name
        let bad_call =
            CallMessage::new("Ping".to_string(), serde_json::json!({ "wrong": true })).unwrap();
        let err = d.dispatch(&bad_call).await.unwrap_err();
        assert!(
            matches!(err, OcppError::Json { .. }),
            "expected Json error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn after_hook_fires_after_successful_dispatch() {
        let notify = Arc::new(Notify::new());
        let n = notify.clone();

        let mut d = ActionDispatcher::new();
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });
        d.after(move |_req: PingRequest| {
            let n = n.clone();
            async move {
                n.notify_one();
            }
        });

        d.dispatch(&ping_call(7)).await.unwrap();

        // Wait up to 100 ms for the spawned after-hook to fire.
        tokio::time::timeout(Duration::from_millis(100), notify.notified())
            .await
            .expect("after hook did not fire within 100 ms");
    }

    #[tokio::test]
    async fn after_hook_does_not_fire_on_handler_error() {
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();

        let mut d = ActionDispatcher::new();
        d.on(|_req: PingRequest| async move {
            Err::<PingResponse, _>(OcppError::Internal {
                message: "err".to_string(),
            })
        });
        d.after(move |_req: PingRequest| {
            let f = f.clone();
            async move {
                f.store(true, Ordering::SeqCst);
            }
        });

        d.dispatch(&ping_call(0)).await.unwrap_err();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            !fired.load(Ordering::SeqCst),
            "after hook must not fire on error"
        );
    }

    #[tokio::test]
    async fn multiple_handlers_dispatch_to_correct_one() {
        let mut d = ActionDispatcher::new();
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });
        d.on(|req: PongRequest| async move {
            Ok(PongResponse {
                value: req.value.to_uppercase(),
            })
        });

        let ping_resp = d.dispatch(&ping_call(99)).await.unwrap();
        assert_eq!(ping_resp["echoed"], 99);

        let pong_resp = d.dispatch(&pong_call("hello")).await.unwrap();
        assert_eq!(pong_resp["value"], "HELLO");
    }

    #[tokio::test]
    async fn registering_handler_twice_overwrites_previous() {
        let mut d = ActionDispatcher::new();
        d.on(|_req: PingRequest| async move { Ok(PingResponse { echoed: 1 }) });
        d.on(|_req: PingRequest| async move { Ok(PingResponse { echoed: 2 }) });

        let resp = d.dispatch(&ping_call(0)).await.unwrap();
        assert_eq!(resp["echoed"], 2);
    }

    #[tokio::test]
    async fn has_handler_reflects_registration() {
        let mut d = ActionDispatcher::new();
        assert!(!d.has_handler("Ping"));
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });
        assert!(d.has_handler("Ping"));
        assert!(!d.has_handler("Pong"));
        assert_eq!(d.handler_count(), 1);
    }

    // --- schema validation wiring (Issue #33) ---
    //
    // Python ref: ocpp/charge_point.py `_handle_call()` calls `_validate()`
    // before invoking the handler. These tests use real OCPP 1.6J actions
    // (BootNotification) so the bundled schemas apply.

    use crate::schema_validation::SchemaValidator;
    use crate::v16j::{BootNotificationRequest, BootNotificationResponse, RegistrationStatus};

    fn validating_dispatcher() -> ActionDispatcher {
        ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v16j()))
    }

    fn boot_response_payload() -> BootNotificationResponse {
        BootNotificationResponse {
            current_time: chrono::Utc::now(),
            interval: 300,
            status: RegistrationStatus::Accepted,
        }
    }

    #[tokio::test]
    async fn dispatch_with_validator_rejects_malformed_payload() {
        let called = Arc::new(AtomicBool::new(false));
        let c = called.clone();

        let mut d = validating_dispatcher();
        d.on(move |_req: BootNotificationRequest| {
            let c = c.clone();
            async move {
                c.store(true, Ordering::SeqCst);
                Ok(boot_response_payload())
            }
        });

        // Missing the required `chargePointVendor` field.
        let bad = CallMessage::new(
            "BootNotification".to_string(),
            serde_json::json!({ "chargePointModel": "M" }),
        )
        .unwrap();

        let err = d.dispatch(&bad).await.unwrap_err();
        // Missing `chargePointVendor` is a `required` failure → keyword Required.
        assert!(
            matches!(
                err,
                OcppError::SchemaViolation {
                    keyword: ocpp_types::SchemaKeyword::Required,
                    ..
                }
            ),
            "expected SchemaViolation(Required), got {err:?}"
        );
        assert!(
            !called.load(Ordering::SeqCst),
            "handler must NOT run when validation fails"
        );
    }

    #[tokio::test]
    async fn dispatch_with_validator_accepts_valid_payload() {
        let called = Arc::new(AtomicBool::new(false));
        let c = called.clone();

        let mut d = validating_dispatcher();
        d.on(move |_req: BootNotificationRequest| {
            let c = c.clone();
            async move {
                c.store(true, Ordering::SeqCst);
                Ok(boot_response_payload())
            }
        });

        let good = CallMessage::new(
            "BootNotification".to_string(),
            serde_json::json!({ "chargePointVendor": "V", "chargePointModel": "M" }),
        )
        .unwrap();

        let resp = d.dispatch(&good).await.unwrap();
        assert_eq!(resp["status"], "Accepted");
        assert!(called.load(Ordering::SeqCst), "handler should have run");
    }

    #[tokio::test]
    async fn dispatch_without_validator_accepts_any_payload() {
        // No validator → malformed payload bypasses schema checks and fails at
        // serde deserialization instead (existing behaviour preserved).
        let mut d = ActionDispatcher::new();
        assert!(!d.has_validator());
        d.on(|_req: BootNotificationRequest| async move { Ok(boot_response_payload()) });

        let bad = CallMessage::new(
            "BootNotification".to_string(),
            serde_json::json!({ "chargePointModel": "M" }),
        )
        .unwrap();

        let err = d.dispatch(&bad).await.unwrap_err();
        assert!(
            matches!(err, OcppError::Json { .. }),
            "expected Json (serde) error without a validator, got {err:?}"
        );
    }

    #[tokio::test]
    async fn concurrent_dispatch_is_correct() {
        use std::sync::Arc;
        let mut d = ActionDispatcher::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        d.on(move |req: PingRequest| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(PingResponse { echoed: req.nonce })
            }
        });

        let d = Arc::new(d);
        let mut handles = Vec::new();
        for i in 0..10u32 {
            let d = d.clone();
            handles.push(tokio::spawn(async move {
                let resp = d.dispatch(&ping_call(i)).await.unwrap();
                assert_eq!(resp["echoed"], i);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }
}
