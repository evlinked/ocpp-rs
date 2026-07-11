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

/// A registered `@on` route: the handler plus its per-route options.
///
/// Ports the per-action entry `create_route_map()` builds in
/// [`ocpp/routing.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/routing.py):
/// each route stores its handler (`_on_action`) alongside a
/// `_skip_schema_validation` flag (default `False`), which
/// `_handle_call()` consults *per action* to decide whether to validate.
struct Route {
    handler: HandlerFn,
    /// When `true`, `dispatch()` bypasses the dispatcher's [`SchemaValidator`]
    /// for this action only — the port of `@on(action,
    /// skip_schema_validation=True)`. Defaults to `false` (validate), matching
    /// the reference.
    skip_validation: bool,
}

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
    handlers: HashMap<&'static str, Route>,
    after_hooks: HashMap<&'static str, AfterFn>,
    /// Optional JSON Schema validator. When present, every incoming CALL
    /// payload is validated against its action schema before the handler is
    /// invoked — unless that action's route opted out via
    /// [`on_skip_validation`](ActionDispatcher::on_skip_validation), consulted
    /// per route (the port of `@on(action, skip_schema_validation=True)`). Ports
    /// the `_validate()` call at the top of `_handle_call()` in
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
    ///
    /// The route validates against the dispatcher's [`SchemaValidator`] (if
    /// one is attached). To register a handler that bypasses validation, use
    /// [`ActionDispatcher::on_skip_validation`].
    pub fn on<Req, Fut, F>(&mut self, handler: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        self.register::<Req, Fut, F>(handler, false);
    }

    /// Register a typed `@on` handler for `Req::ACTION_NAME` that **skips schema
    /// validation** for this action only.
    ///
    /// Ports `@on(action, skip_schema_validation=True)` from
    /// [`ocpp/routing.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/routing.py):
    /// even when the dispatcher carries a [`SchemaValidator`], an incoming CALL
    /// for this action bypasses it and goes straight to the handler. The flag is
    /// recorded *per route*, so sibling actions on the same dispatcher are still
    /// validated. Like [`ActionDispatcher::on`], this replaces any previously
    /// registered handler for the same action (including its skip flag).
    ///
    /// With no validator attached the behaviour is identical to
    /// [`ActionDispatcher::on`] (there is nothing to skip).
    pub fn on_skip_validation<Req, Fut, F>(&mut self, handler: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
    {
        self.register::<Req, Fut, F>(handler, true);
    }

    /// Shared registration path for [`on`](Self::on) /
    /// [`on_skip_validation`](Self::on_skip_validation): erase the typed handler
    /// and store it under `Req::ACTION_NAME` with its per-route skip flag.
    fn register<Req, Fut, F>(&mut self, handler: F, skip_validation: bool)
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
        self.handlers.insert(
            Req::ACTION_NAME,
            Route {
                handler: erased,
                skip_validation,
            },
        );
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
    /// Returns the serialised response `Value` to wrap in a CALLRESULT. When no
    /// handler is registered for `call.action`, the error mirrors the
    /// reference's `_raise_key_error` split: a *known* action for the negotiated
    /// version yields [`OcppError::NotImplemented`], while an action the version
    /// does not define yields [`OcppError::NotSupported`] (see the private
    /// `unrouted_action_error` helper).
    ///
    /// If an `@after` hook is registered, it is spawned via `tokio::spawn`
    /// after the handler returns successfully (non-blocking).
    pub async fn dispatch(&self, call: &CallMessage) -> OcppResult<Value> {
        let action = call.action.as_str();

        // Resolve the route first: the per-route `skip_validation` flag decides
        // whether the validator runs, so the lookup must precede validation.
        // (For an unrouted action, `unrouted_action_error` selects the reference
        // NotImplemented/NotSupported split before any validation would run.)
        let route = match self.handlers.get(action) {
            Some(route) => route,
            None => return Err(self.unrouted_action_error(action)),
        };

        // Schema-validate the incoming payload before invoking the handler,
        // mirroring `_handle_call()` in charge_point.py which runs `_validate()`
        // first and short-circuits to a CALLERROR on failure — unless this route
        // opted out via `on_skip_validation` (the port of
        // `@on(action, skip_schema_validation=True)`), consulted per action so a
        // sibling route on the same dispatcher is still validated. A malformed
        // payload therefore never reaches handler deserialization on a validated
        // route.
        if let Some(validator) = &self.validator {
            if !route.skip_validation {
                validator.validate_call(action, &call.payload)?;
            }
        }

        let payload = call.payload.clone();
        let response = (route.handler)(payload.clone()).await?;

        if let Some(after) = self.after_hooks.get(action) {
            tokio::spawn(after(payload));
        }

        Ok(response)
    }

    /// Select the error for a CALL whose action has no registered handler,
    /// porting `_raise_key_error(action, version)` from
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py):
    ///
    /// - a **known** action for the negotiated version (the reference's
    ///   `v16_Action(action)` / `v201_Action(action)` succeeds) → no handler is
    ///   registered → [`OcppError::NotImplemented`] ("No handler registered");
    /// - an action the version **does not define** → [`OcppError::NotSupported`].
    ///
    /// The [`ActionDispatcher`] is deliberately version-generic, so its attached
    /// [`SchemaValidator`] supplies the version context: the validator's bundled
    /// schema set is the version-scoped known-action registry that stands in for
    /// the reference's per-version `Action` enum. With **no** validator attached
    /// there is no version context to consult, so we conservatively report
    /// `NotSupported` (the dispatcher's prior behaviour) rather than guess that
    /// an action is "known".
    ///
    /// Note: for OCPP 2.0.1 the `v201()` validator currently bundles a subset of
    /// schemas, so a valid-but-not-yet-bundled 2.0.1 action is reported as
    /// `NotSupported`; it upgrades to `NotImplemented` automatically as more
    /// schemas land. This tracks "what this validator knows", which is the most
    /// faithful signal available to a version-generic dispatcher.
    fn unrouted_action_error(&self, action: &str) -> OcppError {
        let known_for_version = self
            .validator
            .as_ref()
            .is_some_and(|v| v.has_schema(action));
        if known_for_version {
            OcppError::NotImplemented {
                feature: action.to_string(),
            }
        } else {
            OcppError::NotSupported {
                feature: action.to_string(),
            }
        }
    }

    /// Returns `true` if a handler is registered for `action`.
    pub fn has_handler(&self, action: &str) -> bool {
        self.handlers.contains_key(action)
    }

    /// Returns `true` if `action` is registered **and** its route opted out of
    /// schema validation via [`on_skip_validation`](Self::on_skip_validation).
    ///
    /// Returns `false` for an unregistered action or a normally-registered one —
    /// the port of reading a route's `_skip_schema_validation` flag (default
    /// `False`).
    pub fn skips_validation(&self, action: &str) -> bool {
        self.handlers
            .get(action)
            .is_some_and(|route| route.skip_validation)
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
    use crate::v16j::{
        BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
        RegistrationStatus,
    };

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

    // --- unrouted-action split: NotImplemented vs NotSupported (Issue #276) ---
    //
    // Python ref: `_raise_key_error(action, version)` in ocpp/charge_point.py
    // returns NotImplemented for a *known* action with no handler and
    // NotSupported for an action the version does not define. The attached
    // validator (its bundled schema set) is the version-scoped known-action
    // registry that stands in for the reference's `v16_Action` enum.

    #[tokio::test]
    async fn unrouted_known_action_with_validator_is_not_implemented() {
        // BootNotification is a valid 1.6J action (the validator has its schema)
        // but no handler is registered → NotImplemented.
        let d = validating_dispatcher();
        assert!(!d.has_handler("BootNotification"));

        let call = CallMessage::new("BootNotification".to_string(), serde_json::json!({})).unwrap();
        let err = d.dispatch(&call).await.unwrap_err();
        assert!(
            matches!(err, OcppError::NotImplemented { ref feature } if feature == "BootNotification"),
            "expected NotImplemented for a known-but-unregistered action, got {err:?}"
        );
    }

    #[tokio::test]
    async fn unrouted_unknown_action_with_validator_is_not_supported() {
        // An action the version does not define (no schema) → NotSupported, even
        // with a validator attached.
        let d = validating_dispatcher();
        let call =
            CallMessage::new("TotallyUnknownAction".to_string(), serde_json::json!({})).unwrap();
        let err = d.dispatch(&call).await.unwrap_err();
        assert!(
            matches!(err, OcppError::NotSupported { ref feature } if feature == "TotallyUnknownAction"),
            "expected NotSupported for an action the version does not define, got {err:?}"
        );
    }

    #[tokio::test]
    async fn unrouted_action_without_validator_stays_not_supported() {
        // No validator → no version context → the conservative NotSupported, so
        // even a valid 1.6J action name cannot be reported as NotImplemented.
        let d = ActionDispatcher::new();
        assert!(!d.has_validator());
        let call = CallMessage::new("BootNotification".to_string(), serde_json::json!({})).unwrap();
        let err = d.dispatch(&call).await.unwrap_err();
        assert!(
            matches!(err, OcppError::NotSupported { ref feature } if feature == "BootNotification"),
            "without a validator a missing handler must yield NotSupported, got {err:?}"
        );
    }

    // --- per-handler skip_schema_validation (Issue #275) ---
    //
    // Python ref: `@on(action, skip_schema_validation=True)` in ocpp/routing.py
    // records `_skip_schema_validation` per route; `_handle_call()` consults it
    // per action. These tests use a real 1.6J validator so the bundled schemas
    // apply. `chargePointVendor` carries `maxLength: 20`, so a 21-char vendor
    // passes serde (a valid `String`) but fails schema validation — the
    // discriminator between a "validated" and a "skipped" route.

    fn overlong_boot() -> CallMessage {
        CallMessage::new(
            "BootNotification".to_string(),
            serde_json::json!({
                "chargePointVendor": "V".repeat(21),
                "chargePointModel": "M",
            }),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn on_skip_validation_bypasses_validator_for_that_route() {
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();

        let mut d = validating_dispatcher();
        d.on_skip_validation(move |_req: BootNotificationRequest| {
            let r = r.clone();
            async move {
                r.store(true, Ordering::SeqCst);
                Ok(boot_response_payload())
            }
        });

        assert!(d.has_validator());
        assert!(d.skips_validation("BootNotification"));

        // A payload the global validator would reject (vendor over maxLength)
        // reaches the handler untouched because this route opted out.
        let resp = d.dispatch(&overlong_boot()).await.unwrap();
        assert_eq!(resp["status"], "Accepted");
        assert!(
            ran.load(Ordering::SeqCst),
            "a skipped route must still run its handler"
        );
    }

    #[tokio::test]
    async fn on_registers_a_validated_route_that_rejects_the_same_payload() {
        // Control for the test above: the identical payload on a normally
        // registered route IS rejected by the same validator.
        let mut d = validating_dispatcher();
        d.on(|_req: BootNotificationRequest| async move { Ok(boot_response_payload()) });

        assert!(!d.skips_validation("BootNotification"));
        let err = d.dispatch(&overlong_boot()).await.unwrap_err();
        assert!(
            matches!(
                err,
                OcppError::SchemaViolation {
                    keyword: ocpp_types::SchemaKeyword::MaxLength,
                    ..
                }
            ),
            "expected SchemaViolation(MaxLength), got {err:?}"
        );
    }

    #[tokio::test]
    async fn skip_flag_is_per_route_sibling_still_validated() {
        // One dispatcher, one validator: BootNotification skips, Heartbeat does
        // not. The skip on one route must not leak to its sibling.
        let mut d = validating_dispatcher();
        d.on_skip_validation(
            |_req: BootNotificationRequest| async move { Ok(boot_response_payload()) },
        );
        d.on(|_req: HeartbeatRequest| async move {
            Ok(HeartbeatResponse {
                current_time: chrono::Utc::now(),
            })
        });

        assert!(d.skips_validation("BootNotification"));
        assert!(!d.skips_validation("Heartbeat"));

        // Skipped route: over-length vendor waved through.
        let boot = d.dispatch(&overlong_boot()).await.unwrap();
        assert_eq!(boot["status"], "Accepted");

        // Validated sibling: a Heartbeat carrying an unexpected property is
        // rejected by the still-active validator (additionalProperties: false).
        let bad_hb = CallMessage::new(
            "Heartbeat".to_string(),
            serde_json::json!({ "unexpected": true }),
        )
        .unwrap();
        let err = d.dispatch(&bad_hb).await.unwrap_err();
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "the sibling route must still be validated, got {err:?}"
        );
    }

    #[tokio::test]
    async fn skip_without_validator_behaves_like_on() {
        // With no validator there is nothing to skip: on_skip_validation is
        // observationally identical to on.
        let mut d = ActionDispatcher::new();
        assert!(!d.has_validator());
        d.on_skip_validation(
            |_req: BootNotificationRequest| async move { Ok(boot_response_payload()) },
        );
        assert!(d.skips_validation("BootNotification"));

        // A deserialisable payload reaches the handler (as it would under `on`).
        let resp = d.dispatch(&overlong_boot()).await.unwrap();
        assert_eq!(resp["status"], "Accepted");
    }

    #[tokio::test]
    async fn re_registering_with_on_clears_the_skip_flag() {
        // Last registration wins for the whole route, flag included — mirroring
        // `on`'s documented "replaces any previously registered handler".
        let mut d = validating_dispatcher();
        d.on_skip_validation(
            |_req: BootNotificationRequest| async move { Ok(boot_response_payload()) },
        );
        assert!(d.skips_validation("BootNotification"));

        d.on(|_req: BootNotificationRequest| async move { Ok(boot_response_payload()) });
        assert!(!d.skips_validation("BootNotification"));

        // Validation is back on: the over-length payload is now rejected.
        let err = d.dispatch(&overlong_boot()).await.unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[tokio::test]
    async fn skips_validation_is_false_for_unregistered_action() {
        let d = validating_dispatcher();
        assert!(!d.skips_validation("BootNotification"));
        assert!(!d.skips_validation("NotAnAction"));
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
