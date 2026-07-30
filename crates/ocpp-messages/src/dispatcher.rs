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

/// Type-erased on-handler: `(payload Value, triggering CALL's unique_id)` in,
/// serialised response `Value` out.
///
/// The `unique_id` arg is the port of the reference's optional `call_unique_id`
/// handler parameter (`_handle_call()` in `ocpp/charge_point.py`): id-aware
/// handlers registered via [`ActionDispatcher::on_with_id`] consume it, while the
/// plain [`ActionDispatcher::on`] erasure ignores it. Threading it through the
/// erased signature (rather than storing two handler variants) keeps a single
/// route type and lets `dispatch()` supply the id unconditionally.
type HandlerFn = Box<dyn Fn(Value, String) -> BoxFuture<OcppResult<Value>> + Send + Sync>;

/// Type-erased after-hook: `(request payload Value, `@on` handler's response
/// Value, triggering CALL's unique_id)` in, fire-and-forget (`()`).
///
/// As with [`HandlerFn`], each erasure consumes only the arguments its builder
/// opted into and ignores the rest: the plain [`ActionDispatcher::after`] uses
/// just the request, [`ActionDispatcher::after_with_id`] adds the `unique_id`,
/// [`ActionDispatcher::after_with_response`] adds the handler's `response`
/// (the port of the reference's `@after(action, inject_response=True)` /
/// `call_response`), and [`ActionDispatcher::after_with_id_and_response`] takes
/// both — the hook that declares `call_unique_id` *and* `inject_response=True`.
/// Threading all three through one signature (rather than
/// storing several hook variants) keeps a single `after_hooks` map; `dispatch()`
/// always supplies every argument.
type AfterFn = Box<dyn Fn(Value, Value, String) -> BoxFuture<()> + Send + Sync>;

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

    /// The OCPP version label of the attached [`SchemaValidator`] (`"1.6"` /
    /// `"2.0.1"`), or `None` when the dispatcher is version-generic (no validator
    /// attached).
    ///
    /// This is the version context the reference threads into `_raise_key_error`
    /// for the unrouted-action `NotSupported` cause
    /// (`f"{action} not supported by OCPP{version}."`). With no validator there
    /// is no version to report, so the CALLERROR builder falls back to a
    /// version-agnostic cause — mirroring the same conservative choice
    /// `unrouted_action_error` makes when it cannot tell a known action from an
    /// unknown one.
    pub fn ocpp_version(&self) -> Option<&'static str> {
        self.validator.as_ref().map(|v| v.ocpp_version())
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
        let erased: HandlerFn = Box::new(move |raw: Value, _unique_id: String| {
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

    /// Register a typed `@on` handler for `Req::ACTION_NAME` that additionally
    /// receives the **triggering CALL's `unique_id`** as a second argument.
    ///
    /// Ports the reference's opt-in `call_unique_id` handler parameter: in
    /// `_handle_call()` (`ocpp/charge_point.py`), a handler that declares a
    /// `call_unique_id` parameter is passed the CALL's id, while one that does not
    /// is called without it. Rust has no runtime signature reflection, so the
    /// opt-in is expressed by *choosing the builder*: use [`on`](Self::on) for a
    /// plain handler, `on_with_id` for one that needs the id (e.g. to correlate a
    /// side effect back to the originating message). The plain builder is
    /// unaffected — its handler never sees the id.
    ///
    /// The handler closure receives `(Req, String)` and, like [`on`](Self::on),
    /// returns `OcppResult<Req::Response>`. The route is schema-validated by the
    /// dispatcher's [`SchemaValidator`] (if attached), identical to [`on`](Self::on),
    /// and replaces any previously registered handler for the same action.
    pub fn on_with_id<Req, Fut, F>(&mut self, handler: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = OcppResult<Req::Response>> + Send + 'static,
        F: Fn(Req, String) -> Fut + Send + Sync + Clone + 'static,
    {
        let erased: HandlerFn = Box::new(move |raw: Value, unique_id: String| {
            let h = handler.clone();
            Box::pin(async move {
                let req: Req = serde_json::from_value(raw).map_err(|e| OcppError::Json {
                    message: e.to_string(),
                })?;
                let resp = h(req, unique_id).await?;
                serde_json::to_value(resp).map_err(|e| OcppError::Json {
                    message: e.to_string(),
                })
            })
        });
        self.handlers.insert(
            Req::ACTION_NAME,
            Route {
                handler: erased,
                skip_validation: false,
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
        let erased: AfterFn = Box::new(move |raw: Value, _response: Value, _unique_id: String| {
            let h = hook.clone();
            Box::pin(async move {
                if let Ok(req) = serde_json::from_value::<Req>(raw) {
                    h(req).await;
                }
            })
        });
        self.after_hooks.insert(Req::ACTION_NAME, erased);
    }

    /// Register a fire-and-forget `@after` hook for `Req::ACTION_NAME` that
    /// additionally receives the **triggering CALL's `unique_id`** as a second
    /// argument.
    ///
    /// The id-aware counterpart of [`after`](Self::after), and the natural home
    /// for the reference's opt-in `call_unique_id` on an `@after` hook (e.g.
    /// correlating a post-response side effect back to the original CALL, or
    /// logging which message triggered it). Like [`after`](Self::after), the hook
    /// is spawned via [`tokio::spawn`] after the handler returns successfully,
    /// does not block the CALLRESULT path, and is silently skipped if the payload
    /// fails to deserialise. The plain [`after`](Self::after) builder is
    /// unaffected — its hook never sees the id.
    pub fn after_with_id<Req, Fut, F>(&mut self, hook: F)
    where
        Req: OcppAction + 'static,
        Fut: Future<Output = ()> + Send + 'static,
        F: Fn(Req, String) -> Fut + Send + Sync + Clone + 'static,
    {
        let erased: AfterFn = Box::new(move |raw: Value, _response: Value, unique_id: String| {
            let h = hook.clone();
            Box::pin(async move {
                if let Ok(req) = serde_json::from_value::<Req>(raw) {
                    h(req, unique_id).await;
                }
            })
        });
        self.after_hooks.insert(Req::ACTION_NAME, erased);
    }

    /// Register a fire-and-forget `@after` hook for `Req::ACTION_NAME` that
    /// additionally receives the **`@on` handler's response** as a second
    /// argument — the port of the reference's
    /// `@after(action, inject_response=True)` and its injected `call_response`
    /// keyword (`ocpp/routing.py::after` + `_handle_call()` in
    /// [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py),
    /// which sets `snake_case_payload["call_response"] = response_payload` when
    /// the hook was decorated with `inject_response=True`).
    ///
    /// The hook closure receives `(Req, Req::Response)`: the deserialised request
    /// and the strongly-typed response the `@on` handler returned (and that was
    /// sent back to the counterparty). This is the supported way to run a
    /// post-response side effect keyed on *what was actually answered* — e.g.
    /// recording the CSMS-assigned `interval`/`status` from a `BootNotification`
    /// response, or the `idTagInfo` returned to an `Authorize`.
    ///
    /// As with the reference, the response is the **validated** one: `dispatch()`
    /// spawns this hook only after the `@on` handler succeeds *and* the outgoing
    /// CALLRESULT passes response validation (a failed `validate_call_result`
    /// short-circuits before the spawn), so an invalid response never reaches
    /// this hook. Like [`after`](Self::after) / [`after_with_id`](Self::after_with_id),
    /// it is spawned via [`tokio::spawn`], does not block the CALLRESULT path,
    /// and is silently skipped if either payload fails to deserialise. The plain
    /// [`after`](Self::after) / [`after_with_id`](Self::after_with_id) builders are
    /// unaffected — their hooks never see the response.
    pub fn after_with_response<Req, Fut, F>(&mut self, hook: F)
    where
        Req: OcppAction + 'static,
        Req::Response: 'static,
        Fut: Future<Output = ()> + Send + 'static,
        F: Fn(Req, Req::Response) -> Fut + Send + Sync + Clone + 'static,
    {
        let erased: AfterFn = Box::new(move |raw: Value, response: Value, _unique_id: String| {
            let h = hook.clone();
            Box::pin(async move {
                // Both the request and the response must deserialise for the hook
                // to run — mirroring the plain `after`, which is skipped when the
                // request fails to deserialise. `response` is the already-
                // validated CALLRESULT payload threaded in by `dispatch()`.
                if let (Ok(req), Ok(resp)) = (
                    serde_json::from_value::<Req>(raw),
                    serde_json::from_value::<Req::Response>(response),
                ) {
                    h(req, resp).await;
                }
            })
        });
        self.after_hooks.insert(Req::ACTION_NAME, erased);
    }

    /// Register a fire-and-forget `@after` hook for `Req::ACTION_NAME` that
    /// receives **both** the triggering CALL's `unique_id` **and** the `@on`
    /// handler's response — the port of an `@after` hook that declares
    /// `call_unique_id` *and* is decorated `inject_response=True`.
    ///
    /// The reference's `_handle_call()`
    /// ([`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py))
    /// injects the two **independently and simultaneously**:
    ///
    /// ```python
    /// if call_unique_id_required:
    ///     snake_case_payload["call_unique_id"] = msg.unique_id
    /// if getattr(handler, "_inject_response", False):
    ///     snake_case_payload["call_response"] = response_payload
    /// response = handler(**snake_case_payload)
    /// ```
    ///
    /// so a single hook can see both. This is the fourth and most complete of
    /// the `@after` builders — the combination of
    /// [`after_with_id`](Self::after_with_id) and
    /// [`after_with_response`](Self::after_with_response) — and the home for a
    /// post-response side effect that must correlate back to the originating
    /// CALL *and* key on what was answered (e.g. "record that CALL `<id>`
    /// received an `Authorize` response of `Blocked`" for an audit trail).
    ///
    /// The hook closure receives `(Req, Req::Response, String)`: the deserialised
    /// request, the strongly-typed (validated) response the `@on` handler
    /// returned, and the triggering `unique_id`. As with
    /// [`after_with_response`](Self::after_with_response), the response is the
    /// **validated** one — `dispatch()` spawns this hook only after the handler
    /// succeeds *and* the outgoing CALLRESULT passes response validation — and
    /// the hook is silently skipped if either payload fails to deserialise. Like
    /// the other three builders it is spawned via [`tokio::spawn`] and does not
    /// block the CALLRESULT path. The existing
    /// [`after`](Self::after) / [`after_with_id`](Self::after_with_id) /
    /// [`after_with_response`](Self::after_with_response) builders are unaffected.
    pub fn after_with_id_and_response<Req, Fut, F>(&mut self, hook: F)
    where
        Req: OcppAction + 'static,
        Req::Response: 'static,
        Fut: Future<Output = ()> + Send + 'static,
        F: Fn(Req, Req::Response, String) -> Fut + Send + Sync + Clone + 'static,
    {
        let erased: AfterFn = Box::new(move |raw: Value, response: Value, unique_id: String| {
            let h = hook.clone();
            Box::pin(async move {
                // Both the request and the response must deserialise for the hook
                // to run — matching `after_with_response`. `response` is the
                // already-validated CALLRESULT payload and `unique_id` the
                // triggering CALL's id, both threaded in by `dispatch()`.
                if let (Ok(req), Ok(resp)) = (
                    serde_json::from_value::<Req>(raw),
                    serde_json::from_value::<Req::Response>(response),
                ) {
                    h(req, resp, unique_id).await;
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

        // Thread the triggering CALL's `unique_id` to both the handler and the
        // after-hook, mirroring `_handle_call()` which makes `call_unique_id`
        // available to any handler that opts into it. Plain `on`/`after`
        // erasures ignore it; `on_with_id`/`after_with_id` consume it.
        let payload = call.payload.clone();
        let unique_id = call.unique_id.clone();
        let response = (route.handler)(payload.clone(), unique_id.clone()).await?;

        // Schema-validate the *outgoing* CALLRESULT before returning it, the
        // symmetric port of `_handle_call()`'s second `validate_payload(response,
        // …)` in charge_point.py — gated by the *same* per-route
        // `skip_validation` flag as the request-side check above. A handler that
        // produces a schema-invalid response must never put an invalid frame on
        // the wire: returning `Err` here lets the caller emit a CALLERROR instead
        // (mirroring `route_message`'s `except OCPPError → create_call_error`),
        // exactly as a rejected *request* already does. Because the check runs
        // before the `@after` spawn and short-circuits via `?`, a failed response
        // validation also skips the after-hook — matching the reference, which
        // raises before reaching its after block.
        //
        // `validate_call_result` keys the `{action}Response` schema. The bundled
        // 1.6J and 2.0.1 schema sets are request/response-symmetric (every action
        // ships both `X.json` and `XResponse.json`), so an action whose request
        // just validated always has a response schema — no missing-schema guard
        // is needed, keeping this symmetric with the request-side branch. A
        // `skip_validation` route bypasses the check, so its handler may return
        // an out-of-schema response unaltered (ports
        // `test_route_message_without_validation`).
        if let Some(validator) = &self.validator {
            if !route.skip_validation {
                validator.validate_call_result(action, &response)?;
            }
        }

        // Thread the request payload, the *validated* response, and the
        // `unique_id` into the after-hook. Each erasure consumes only the
        // arguments its builder opted into (`after` → request; `after_with_id` →
        // request + id; `after_with_response` → request + response), ports the
        // reference's conditional `call_response`/`call_unique_id` injection in
        // `_handle_call()`. `response` is cloned because it is also returned below.
        if let Some(after) = self.after_hooks.get(action) {
            tokio::spawn(after(payload, response.clone(), unique_id));
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

    // --- outbound CALLRESULT validation (Issue #334) ---
    //
    // Python ref: `_handle_call()` in ocpp/charge_point.py validates the payload
    // a *second* time — the outgoing CALLRESULT — before `_send`, gated by the
    // same `_skip_schema_validation` flag as the request-side check. A failure
    // raises an OCPPError that `route_message()` turns into a CALLERROR.
    // `dispatch()` mirrors this: the response is validated after the handler
    // returns and before the `@after` hook, so an invalid response becomes an
    // `Err` (→ CALLERROR) rather than an invalid frame — unless the route skips.
    //
    // A stand-in `BootNotification` action whose response carries a free-form
    // `status: String` lets a handler emit an out-of-enum value (`"Yolo"`,
    // straight from `test_route_message_without_validation`) that the typed
    // `BootNotificationResponse` could never produce. Its ACTION_NAME is
    // `BootNotification`, so the bundled 1.6J request *and* response schemas both
    // apply.

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct RawBootRequest {
        #[serde(rename = "chargePointVendor")]
        vendor: String,
        #[serde(rename = "chargePointModel")]
        model: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct RawBootResponse {
        #[serde(rename = "currentTime")]
        current_time: String,
        interval: i64,
        status: String,
    }

    impl OcppAction for RawBootRequest {
        const ACTION_NAME: &'static str = "BootNotification";
        type Response = RawBootResponse;
    }

    impl OcppAction for RawBootResponse {
        const ACTION_NAME: &'static str = "BootNotificationResponse";
        type Response = RawBootResponse;
    }

    impl OcppResponse for RawBootResponse {}

    /// A valid `BootNotification` CALL — the request always passes so these
    /// tests isolate the *response* side.
    fn valid_boot_call() -> CallMessage {
        CallMessage::new(
            "BootNotification".to_string(),
            serde_json::json!({ "chargePointVendor": "V", "chargePointModel": "M" }),
        )
        .unwrap()
    }

    fn raw_boot_response(status: &str) -> RawBootResponse {
        RawBootResponse {
            // A schema-valid RFC3339 `date-time` (the response schema enforces
            // the format), so the only schema violation in the reject test is the
            // `status` value itself.
            current_time: "2018-05-29T17:37:05.495259Z".to_string(),
            interval: 350,
            status: status.to_string(),
        }
    }

    #[tokio::test]
    async fn dispatch_rejects_a_schema_invalid_response() {
        // The corollary of `test_route_message_without_validation`: with
        // validation ON (the default route), a handler that returns an
        // out-of-enum `status` must NOT be sent — dispatch returns Err so the
        // caller emits a CALLERROR.
        let after_ran = Arc::new(AtomicBool::new(false));
        let a = after_ran.clone();

        let mut d = validating_dispatcher();
        d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Yolo")) });
        d.after(move |_req: RawBootRequest| {
            let a = a.clone();
            async move {
                a.store(true, Ordering::SeqCst);
            }
        });

        let err = d.dispatch(&valid_boot_call()).await.unwrap_err();
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "an out-of-enum response `status` must fail response validation, got {err:?}"
        );

        // The reference validates the response *before* its after block, so a
        // failed response validation must skip the `@after` hook. Give any
        // (incorrectly) spawned hook a chance to run before asserting.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !after_ran.load(Ordering::SeqCst),
            "the @after hook must not run when response validation fails"
        );
    }

    #[tokio::test]
    async fn skip_route_passes_an_invalid_response_through() {
        // Direct port of `test_route_message_without_validation`: a
        // `skip_schema_validation=True` route lets the handler's invalid `"Yolo"`
        // response through unaltered.
        let mut d = validating_dispatcher();
        d.on_skip_validation(|_req: RawBootRequest| async move { Ok(raw_boot_response("Yolo")) });

        let resp = d.dispatch(&valid_boot_call()).await.unwrap();
        assert_eq!(
            resp["status"], "Yolo",
            "a skip route must return the handler's response unvalidated"
        );
    }

    #[tokio::test]
    async fn dispatch_accepts_a_schema_valid_response() {
        // A response that satisfies the `BootNotificationResponse` schema passes
        // the new outbound check and flows through unchanged.
        let mut d = validating_dispatcher();
        d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Accepted")) });

        let resp = d.dispatch(&valid_boot_call()).await.unwrap();
        assert_eq!(resp["status"], "Accepted");
    }

    #[tokio::test]
    async fn without_validator_response_is_not_validated() {
        // Symmetric with the request side: with no validator attached there is
        // no schema context, so even an out-of-enum response flows through.
        let mut d = ActionDispatcher::new();
        assert!(!d.has_validator());
        d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Yolo")) });

        let resp = d.dispatch(&valid_boot_call()).await.unwrap();
        assert_eq!(resp["status"], "Yolo");
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

    // --- id-aware handlers: on_with_id / after_with_id (Issue #317) ---
    //
    // Python ref: `_handle_call()` in ocpp/charge_point.py passes the triggering
    // CALL's `unique_id` to any handler that declares a `call_unique_id`
    // parameter. Rust expresses the opt-in by choosing the builder — `on_with_id`
    // / `after_with_id` receive the id; plain `on`/`after` never see it.

    fn ping_call_with_id(id: &str, nonce: u32) -> CallMessage {
        CallMessage::with_id(
            id.to_string(),
            "Ping".to_string(),
            serde_json::json!({ "nonce": nonce }),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn on_with_id_receives_triggering_unique_id() {
        let seen = Arc::new(std::sync::Mutex::new(None::<String>));
        let s = seen.clone();

        let mut d = ActionDispatcher::new();
        d.on_with_id(move |req: PingRequest, unique_id: String| {
            let s = s.clone();
            async move {
                *s.lock().unwrap() = Some(unique_id);
                Ok(PingResponse { echoed: req.nonce })
            }
        });

        let resp = d.dispatch(&ping_call_with_id("call-42", 7)).await.unwrap();
        assert_eq!(
            resp["echoed"], 7,
            "the id-aware handler still runs normally"
        );
        assert_eq!(
            seen.lock().unwrap().as_deref(),
            Some("call-42"),
            "on_with_id must see the triggering CALL's exact unique_id"
        );
    }

    #[tokio::test]
    async fn after_with_id_receives_triggering_unique_id() {
        let seen = Arc::new(std::sync::Mutex::new(None::<String>));
        let s = seen.clone();
        let notify = Arc::new(Notify::new());
        let n = notify.clone();

        let mut d = ActionDispatcher::new();
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });
        d.after_with_id(move |_req: PingRequest, unique_id: String| {
            let s = s.clone();
            let n = n.clone();
            async move {
                *s.lock().unwrap() = Some(unique_id);
                n.notify_one();
            }
        });

        d.dispatch(&ping_call_with_id("call-99", 1)).await.unwrap();

        tokio::time::timeout(Duration::from_secs(5), notify.notified())
            .await
            .expect("the after_with_id hook must fire after a successful dispatch");
        assert_eq!(
            seen.lock().unwrap().as_deref(),
            Some("call-99"),
            "after_with_id must see the triggering CALL's exact unique_id"
        );
    }

    #[tokio::test]
    async fn plain_on_and_after_are_unaffected_by_id_threading() {
        // The plain `on`/`after` builders keep working unchanged: the id is
        // threaded through `dispatch()` but their erasures ignore it.
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

        let resp = d.dispatch(&ping_call_with_id("ignored", 3)).await.unwrap();
        assert_eq!(resp["echoed"], 3);
        tokio::time::timeout(Duration::from_secs(5), notify.notified())
            .await
            .expect("a plain @after hook must still fire when the id is threaded");
    }

    // --- response-aware after hooks: after_with_response (Issue #388) ---
    //
    // Python ref: `@after(action, inject_response=True)` in ocpp/routing.py, and
    // `_handle_call()` in ocpp/charge_point.py setting
    // `snake_case_payload["call_response"] = response_payload` before invoking the
    // after-hook. `after_with_response` receives the `@on` handler's (validated)
    // response as a strongly-typed second argument; plain `after` never does.

    #[tokio::test]
    async fn after_with_response_receives_the_handler_response() {
        let seen = Arc::new(std::sync::Mutex::new(None::<PingResponse>));
        let s = seen.clone();
        let notify = Arc::new(Notify::new());
        let n = notify.clone();

        let mut d = ActionDispatcher::new();
        // The `@on` handler echoes the nonce back in its response.
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });
        d.after_with_response(move |_req: PingRequest, resp: PingResponse| {
            let s = s.clone();
            let n = n.clone();
            async move {
                *s.lock().unwrap() = Some(resp);
                n.notify_one();
            }
        });

        d.dispatch(&ping_call(7)).await.unwrap();

        tokio::time::timeout(Duration::from_secs(5), notify.notified())
            .await
            .expect("the after_with_response hook must fire after a successful dispatch");
        assert_eq!(
            seen.lock().unwrap().as_ref().map(|r| r.echoed),
            Some(7),
            "after_with_response must see the exact response the @on handler returned"
        );
    }

    #[tokio::test]
    async fn after_with_response_not_run_on_response_validation_failure() {
        // Same guard as `dispatch_rejects_a_schema_invalid_response`: with a
        // validator attached and a handler returning an out-of-enum `status`, the
        // outgoing CALLRESULT fails validation and short-circuits *before* the
        // after-hook spawn — so a response-aware hook must not run, matching the
        // reference which raises before reaching its after block.
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();

        let mut d = validating_dispatcher();
        d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Yolo")) });
        d.after_with_response(move |_req: RawBootRequest, _resp: RawBootResponse| {
            let r = r.clone();
            async move {
                r.store(true, Ordering::SeqCst);
            }
        });

        let err = d.dispatch(&valid_boot_call()).await.unwrap_err();
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "the out-of-enum response must fail validation, got {err:?}"
        );

        // Give any (erroneously) spawned hook a chance to run before asserting.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !ran.load(Ordering::SeqCst),
            "after_with_response must not run when response validation fails"
        );
    }

    #[tokio::test]
    async fn after_with_response_sees_a_validated_response() {
        // The passing counterpart: a schema-valid response is threaded into the
        // hook. Uses the raw response type so the hook observes the exact
        // (validated) `status` the handler produced.
        let seen = Arc::new(std::sync::Mutex::new(None::<String>));
        let s = seen.clone();
        let notify = Arc::new(Notify::new());
        let n = notify.clone();

        let mut d = validating_dispatcher();
        d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Accepted")) });
        d.after_with_response(move |_req: RawBootRequest, resp: RawBootResponse| {
            let s = s.clone();
            let n = n.clone();
            async move {
                *s.lock().unwrap() = Some(resp.status);
                n.notify_one();
            }
        });

        d.dispatch(&valid_boot_call()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), notify.notified())
            .await
            .expect("the after_with_response hook must fire on a valid response");
        assert_eq!(
            seen.lock().unwrap().as_deref(),
            Some("Accepted"),
            "the hook must see the validated response status"
        );
    }

    // --- id- AND response-aware after hooks: after_with_id_and_response (Issue #391) ---
    //
    // Python ref: `_handle_call()` in ocpp/charge_point.py injects
    // `call_unique_id` and `call_response` *independently and simultaneously*, so
    // a single `@after` hook decorated `inject_response=True` and declaring a
    // `call_unique_id` parameter receives both. `after_with_id_and_response` is
    // the Rust analog — the combination of `after_with_id` + `after_with_response`.

    #[tokio::test]
    async fn after_with_id_and_response_receives_both_id_and_response() {
        let seen_id = Arc::new(std::sync::Mutex::new(None::<String>));
        let seen_resp = Arc::new(std::sync::Mutex::new(None::<PingResponse>));
        let si = seen_id.clone();
        let sr = seen_resp.clone();
        let notify = Arc::new(Notify::new());
        let n = notify.clone();

        let mut d = ActionDispatcher::new();
        d.on(|req: PingRequest| async move { Ok(PingResponse { echoed: req.nonce }) });
        d.after_with_id_and_response(
            move |_req: PingRequest, resp: PingResponse, unique_id: String| {
                let si = si.clone();
                let sr = sr.clone();
                let n = n.clone();
                async move {
                    *si.lock().unwrap() = Some(unique_id);
                    *sr.lock().unwrap() = Some(resp);
                    n.notify_one();
                }
            },
        );

        d.dispatch(&ping_call_with_id("call-77", 42)).await.unwrap();

        tokio::time::timeout(Duration::from_secs(5), notify.notified())
            .await
            .expect("the after_with_id_and_response hook must fire after a successful dispatch");
        assert_eq!(
            seen_id.lock().unwrap().as_deref(),
            Some("call-77"),
            "after_with_id_and_response must see the triggering CALL's exact unique_id"
        );
        assert_eq!(
            seen_resp.lock().unwrap().as_ref().map(|r| r.echoed),
            Some(42),
            "after_with_id_and_response must see the exact response the @on handler returned"
        );
    }

    #[tokio::test]
    async fn after_with_id_and_response_not_run_on_response_validation_failure() {
        // Same short-circuit guard as `after_with_response`: with a validator
        // attached and a handler returning an out-of-enum `status`, the outgoing
        // CALLRESULT fails validation before the after-hook spawn, so a hook that
        // opted into both id and response must not run.
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();

        let mut d = validating_dispatcher();
        d.on(|_req: RawBootRequest| async move { Ok(raw_boot_response("Yolo")) });
        d.after_with_id_and_response(
            move |_req: RawBootRequest, _resp: RawBootResponse, _unique_id: String| {
                let r = r.clone();
                async move {
                    r.store(true, Ordering::SeqCst);
                }
            },
        );

        let err = d.dispatch(&valid_boot_call()).await.unwrap_err();
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "the out-of-enum response must fail validation, got {err:?}"
        );

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !ran.load(Ordering::SeqCst),
            "after_with_id_and_response must not run when response validation fails"
        );
    }
}
