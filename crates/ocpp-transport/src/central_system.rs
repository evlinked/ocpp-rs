//! Default Central System (CSMS) handler set — spec-valid responders for the
//! boot-time message trio a charge point sends before any transaction.
//!
//! Ports the `@on` handler set from the Python reference's runnable CSMS example
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! into a reusable, overridable builder so a CSMS comes up "batteries included":
//!
//! | Action | Default response |
//! |---|---|
//! | `BootNotification` | `Accepted` + configurable `interval` + `current_time` |
//! | `Heartbeat` | `current_time` |
//! | `StatusNotification` | empty `{}` |
//!
//! `Authorize` / `StartTransaction` / `StopTransaction` defaults are
//! intentionally **out of scope** — they live in M3, where authorization and
//! transaction policy is decided. This module is strictly the boot-time trio.
//!
//! The defaults are overridable: register your own `@on` for any action *after*
//! the defaults are installed and it replaces the default, matching
//! [`ActionDispatcher::on`] semantics.

use std::sync::Arc;

use chrono::Utc;
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    RegistrationStatus, StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::{ActionDispatcher, SchemaValidator};
use tokio::sync::mpsc;

use crate::server::OcppServer;
use crate::{DispatchHandler, MessageHandler, TransportConfig, TransportEvent};

/// Configuration for the default Central System handler set.
///
/// Lets callers simulate `Pending`/`Rejected` boot flows and tune the heartbeat
/// interval the CSMS advertises, without writing their own handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CentralSystemConfig {
    /// Heartbeat interval (seconds) returned in the `BootNotification` response.
    ///
    /// Defaults to `300` — the OCPP 1.6J default. (The Python example uses `10`
    /// for a snappy demo; production CSMSs typically use a larger value.)
    pub boot_interval: i32,
    /// Registration status returned in the `BootNotification` response.
    ///
    /// Defaults to [`RegistrationStatus::Accepted`]; set to `Pending`/`Rejected`
    /// to exercise those charge-point code paths.
    pub registration_status: RegistrationStatus,
}

impl Default for CentralSystemConfig {
    fn default() -> Self {
        Self {
            boot_interval: 300,
            registration_status: RegistrationStatus::Accepted,
        }
    }
}

/// Build an [`ActionDispatcher`] pre-registered with the default CS-side
/// handlers for the boot-time trio, using [`CentralSystemConfig::default`].
///
/// The returned dispatcher has the bundled OCPP 1.6J [`SchemaValidator`]
/// attached, so incoming CALL payloads are validated before dispatch (matching
/// the validator-on-by-default posture from #33). Register additional `@on`
/// handlers, then wrap in `Arc` and hand to
/// [`DispatchHandler::new`](crate::DispatchHandler::new):
///
/// ```ignore
/// let dispatcher = central_system_dispatcher();
/// let handler = Arc::new(DispatchHandler::new(Arc::new(dispatcher)));
/// let (server, events) = OcppServer::new(TransportConfig::default(), handler);
/// ```
pub fn central_system_dispatcher() -> ActionDispatcher {
    central_system_dispatcher_with(CentralSystemConfig::default())
}

/// Like [`central_system_dispatcher`] but with an explicit
/// [`CentralSystemConfig`] (custom boot interval / registration status).
pub fn central_system_dispatcher_with(config: CentralSystemConfig) -> ActionDispatcher {
    let mut dispatcher = ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v16j()));
    register_default_handlers(&mut dispatcher, config);
    dispatcher
}

/// Install the default boot-time handlers onto an existing [`ActionDispatcher`].
///
/// Exposed for callers who construct the dispatcher themselves (e.g. to attach
/// a custom validator, or none at all) but still want the batteries-included
/// boot responders. Handlers registered for the same action *after* this call
/// replace the default, matching [`ActionDispatcher::on`] semantics.
pub fn register_default_handlers(dispatcher: &mut ActionDispatcher, config: CentralSystemConfig) {
    // `i32`/`RegistrationStatus` are `Copy`, so the closures stay `Clone` as
    // `ActionDispatcher::on` requires.
    let interval = config.boot_interval;
    let status = config.registration_status;

    dispatcher.on(move |_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: Utc::now(),
            interval,
            status,
        })
    });

    dispatcher.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: Utc::now(),
        })
    });

    dispatcher
        .on(|_req: StatusNotificationRequest| async move { Ok(StatusNotificationResponse {}) });
}

/// Batteries-included CSMS server constructor for **OCPP 1.6J**.
///
/// Builds an [`OcppServer`] from `config` and `handler` and attaches an OCPP
/// 1.6J [`SchemaValidator`] to its CSMS-initiated
/// [`call`](OcppServer::call) path, so the recommended CSMS setup validates
/// **both directions of an outbound command out of the box**: the outbound CALL
/// before it goes on the wire, and the inbound CALLRESULT before it is
/// deserialized (a charge point's response is an untrusted trust boundary). This
/// matches the CP side's validate-by-default posture
/// (`ChargePointConfig::validate_payloads` defaults to `true`) and the
/// reference's side-agnostic
/// [`charge_point.py::call`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py),
/// whose `skip_schema_validation` defaults to `False`.
///
/// The `handler` argument controls the *inbound* dispatch path independently:
/// wrap a [`central_system_dispatcher`] (which attaches the same 1.6J validator
/// to inbound CALLs) in a [`DispatchHandler`] to get a
/// CSMS that validates every direction:
///
/// ```ignore
/// use std::sync::Arc;
/// use ocpp_transport::{central_system_dispatcher, central_system_server, DispatchHandler, TransportConfig};
///
/// let dispatcher = central_system_dispatcher();
/// let handler = Arc::new(DispatchHandler::new(Arc::new(dispatcher)));
/// let (mut server, _events) = central_system_server(TransportConfig::default(), handler);
/// server.start("0.0.0.0:9000").await?;
/// ```
///
/// **Opt-out** (the `skip_schema_validation=True` analog): to build a CSMS whose
/// `call()` path performs no validation, construct the server directly with
/// [`OcppServer::new`], which defaults to no validator.
pub fn central_system_server(
    config: TransportConfig,
    handler: Arc<dyn MessageHandler>,
) -> (OcppServer, mpsc::UnboundedReceiver<TransportEvent>) {
    build_validated_server(config, handler, SchemaValidator::v16j())
}

/// Like [`central_system_server`] but attaches an **OCPP 2.0.1**
/// [`SchemaValidator`] to the [`call`](OcppServer::call) path, for a CSMS that
/// speaks 2.0.1. Pair with
/// [`central_system_dispatcher_v201`](crate::central_system_dispatcher_v201) on
/// the inbound side.
pub fn central_system_server_v201(
    config: TransportConfig,
    handler: Arc<dyn MessageHandler>,
) -> (OcppServer, mpsc::UnboundedReceiver<TransportEvent>) {
    build_validated_server(config, handler, SchemaValidator::v201())
}

/// Shared construction for the version-specific batteries-included server
/// helpers: build the [`OcppServer`] and attach `validator` to its `call()`
/// path. The 1.6J and 2.0.1 entry points differ only in which bundled validator
/// they pass, so the wiring lives here once.
fn build_validated_server(
    config: TransportConfig,
    handler: Arc<dyn MessageHandler>,
    validator: SchemaValidator,
) -> (OcppServer, mpsc::UnboundedReceiver<TransportEvent>) {
    let (server, events) = OcppServer::new(config, handler);
    (server.with_validator(Arc::new(validator)), events)
}

/// Fully-assembled, batteries-included default **1.6J** CSMS in one call.
///
/// Wires the three primitives the recommended CSMS is built from — the
/// inbound-validated boot-trio [`central_system_dispatcher_with`], the
/// [`DispatchHandler`] adapter, and the outbound-validated
/// [`central_system_server`] — so **both directions are schema-validated out of
/// the box** (inbound CALLs by the dispatcher's validator, outbound CALLs and
/// inbound CALLRESULTs by the server's `call()`-path validator) and the
/// boot-time trio is pre-registered. Returns the ready-to-[`start`](OcppServer::start)
/// server and its [`TransportEvent`] receiver — the last wiring step callers
/// otherwise get subtly wrong (forgetting the adapter, or pairing the server
/// with a non-validating dispatcher).
///
/// `customize` runs against the fully-defaulted [`ActionDispatcher`] *before* it
/// is shared into the handler, so callers can register additional `@on`/`@after`
/// handlers — or override a default — without dropping the defaults or either
/// validator (later registrations win, per [`ActionDispatcher::on`]). Pass
/// `|_| {}` for the pure-defaults CSMS:
///
/// ```ignore
/// use ocpp_transport::{central_system_service, CentralSystemConfig, TransportConfig};
/// use ocpp_messages::v16j::{AuthorizeRequest, AuthorizeResponse};
///
/// // Pure defaults — the whole default CSMS in one call:
/// let (mut server, _events) =
///     central_system_service(CentralSystemConfig::default(), TransportConfig::default(), |_| {});
///
/// // …or register an extra handler before start(), defaults intact:
/// let (mut server, _events) = central_system_service(
///     CentralSystemConfig::default(),
///     TransportConfig::default(),
///     |d| {
///         d.on(|_req: AuthorizeRequest| async move { Ok(/* AuthorizeResponse { .. } */) });
///     },
/// );
/// server.start("0.0.0.0:9000").await?;
/// ```
///
/// This is a convenience layer over the three primitives, which stay public and
/// unchanged — construct them by hand for finer control (e.g. a different
/// inbound vs outbound validator, or the `skip_schema_validation` opt-out via
/// plain [`OcppServer::new`]). See
/// [`central_system_service_v201`](crate::central_system_service_v201) for the
/// 2.0.1 twin.
pub fn central_system_service(
    cs_config: CentralSystemConfig,
    transport_config: TransportConfig,
    customize: impl FnOnce(&mut ActionDispatcher),
) -> (OcppServer, mpsc::UnboundedReceiver<TransportEvent>) {
    let mut dispatcher = central_system_dispatcher_with(cs_config);
    customize(&mut dispatcher);
    let handler = Arc::new(DispatchHandler::new(Arc::new(dispatcher)));
    central_system_server(transport_config, handler)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::{CallMessage, OcppAction};
    use serde_json::{json, Value};

    /// Dispatch `action` with `payload` through a freshly built default
    /// dispatcher and return the serialized CALLRESULT payload.
    async fn dispatch_default(action: &str, payload: Value) -> Value {
        let d = central_system_dispatcher();
        let call = CallMessage::new(action.to_string(), payload).unwrap();
        d.dispatch(&call)
            .await
            .expect("default handler should succeed")
    }

    fn valid_boot_payload() -> Value {
        json!({ "chargePointVendor": "ACME", "chargePointModel": "Wallbox-1" })
    }

    fn valid_status_payload() -> Value {
        json!({ "connectorId": 1, "errorCode": "NoError", "status": "Available" })
    }

    #[tokio::test]
    async fn default_boot_notification_returns_accepted_with_interval() {
        let resp = dispatch_default("BootNotification", valid_boot_payload()).await;
        assert_eq!(resp["status"], "Accepted");
        assert_eq!(resp["interval"], 300);
        assert!(
            resp.get("currentTime").and_then(Value::as_str).is_some(),
            "expected an RFC3339 currentTime, got {resp:?}"
        );
    }

    #[tokio::test]
    async fn default_heartbeat_returns_current_time() {
        let resp = dispatch_default("Heartbeat", json!({})).await;
        let ts = resp
            .get("currentTime")
            .and_then(Value::as_str)
            .expect("heartbeat response must carry currentTime");
        // Round-trips as a valid UTC timestamp.
        chrono::DateTime::parse_from_rfc3339(ts).expect("currentTime must be RFC3339");
    }

    #[tokio::test]
    async fn default_status_notification_returns_empty() {
        let resp = dispatch_default("StatusNotification", valid_status_payload()).await;
        assert_eq!(resp, json!({}), "StatusNotification response must be empty");
    }

    #[tokio::test]
    async fn custom_handler_overrides_default() {
        // Build the default set, then override BootNotification with Rejected.
        let mut d = central_system_dispatcher();
        d.on(|_req: BootNotificationRequest| async move {
            Ok(BootNotificationResponse {
                current_time: Utc::now(),
                interval: 42,
                status: RegistrationStatus::Rejected,
            })
        });

        let call = CallMessage::new("BootNotification".to_string(), valid_boot_payload()).unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Rejected");
        assert_eq!(resp["interval"], 42);
    }

    #[tokio::test]
    async fn config_controls_registration_status_and_interval() {
        let d = central_system_dispatcher_with(CentralSystemConfig {
            boot_interval: 60,
            registration_status: RegistrationStatus::Pending,
        });
        let call = CallMessage::new("BootNotification".to_string(), valid_boot_payload()).unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Pending");
        assert_eq!(resp["interval"], 60);
    }

    #[tokio::test]
    async fn default_responses_pass_schema_validation() {
        // The whole point of "spec-valid defaults": every default response must
        // validate against its bundled `{action}Response` schema.
        let validator = SchemaValidator::v16j();

        let boot = dispatch_default("BootNotification", valid_boot_payload()).await;
        validator
            .validate_call_result(BootNotificationRequest::ACTION_NAME, &boot)
            .expect("default BootNotification response must satisfy its schema");

        let heartbeat = dispatch_default("Heartbeat", json!({})).await;
        validator
            .validate_call_result(HeartbeatRequest::ACTION_NAME, &heartbeat)
            .expect("default Heartbeat response must satisfy its schema");

        let status = dispatch_default("StatusNotification", valid_status_payload()).await;
        validator
            .validate_call_result(StatusNotificationRequest::ACTION_NAME, &status)
            .expect("default StatusNotification response must satisfy its schema");
    }

    #[tokio::test]
    async fn default_dispatcher_validates_incoming_calls() {
        // A malformed BootNotification (missing required chargePointVendor) is
        // rejected by the attached validator before reaching the handler.
        let d = central_system_dispatcher();
        assert!(d.has_validator());
        let bad = CallMessage::new(
            "BootNotification".to_string(),
            json!({ "chargePointModel": "Wallbox-1" }),
        )
        .unwrap();
        let err = d.dispatch(&bad).await.unwrap_err();
        assert!(
            matches!(err, ocpp_types::OcppError::SchemaViolation { .. }),
            "expected SchemaViolation for malformed boot payload, got {err:?}"
        );
    }

    #[tokio::test]
    async fn builder_registers_exactly_the_boot_trio() {
        let d = central_system_dispatcher();
        assert!(d.has_handler("BootNotification"));
        assert!(d.has_handler("Heartbeat"));
        assert!(d.has_handler("StatusNotification"));
        // Transaction-era actions are intentionally absent (M3 territory).
        assert!(!d.has_handler("Authorize"));
        assert!(!d.has_handler("StartTransaction"));
        assert_eq!(d.handler_count(), 3);
    }

    // ── central_system_server: outbound `call()` validator wiring ────────────
    //
    // These assertions exploit `OcppServer::call`'s documented ordering — the
    // outbound CALL is schema-validated *before* the connection check — so the
    // validator's presence is observable without ever starting the server or
    // connecting a CP: a schema-invalid request fails with `SchemaViolation`
    // (validator attached), whereas the same request on a validator-less server
    // reaches the connection check and fails with `CpNotConnected`. The
    // end-to-end inbound-CALLRESULT rejection over a real socket is covered by
    // `server::tests::central_system_server_validates_callresult_from_fake_cp`.

    use ocpp_messages::v16j::RemoteStartTransactionRequest;
    use ocpp_types::OcppError;

    /// A `RemoteStartTransaction` request with the given `idTag`. A 21-char tag
    /// is serde-valid but violates the `CiString20` `maxLength` in the schema.
    fn remote_start(id_tag: &str) -> RemoteStartTransactionRequest {
        RemoteStartTransactionRequest {
            connector_id: None,
            id_tag: id_tag.to_string(),
            charging_profile: None,
        }
    }

    #[tokio::test]
    async fn central_system_server_wires_outbound_validator() {
        // A server built via the helper rejects a schema-invalid outbound CALL
        // (21-char idTag over CiString20's maxLength) as `SchemaViolation`
        // *before* the connection check — proving the 1.6J validator is wired
        // to the `call()` path.
        let (server, _events) =
            central_system_server(TransportConfig::default(), Arc::new(NoopHandler));
        let err = server
            .call("NEVER_CONNECTED", remote_start(&"x".repeat(21)))
            .await
            .expect_err("overlong idTag must be rejected before send");
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "expected SchemaViolation (validator wired), got {err:?}"
        );
    }

    #[tokio::test]
    async fn opt_out_server_has_no_outbound_validator() {
        // The `skip_schema_validation=True` analog: a server built with plain
        // `OcppServer::new` performs no `call()`-path validation, so the same
        // overlong-idTag request sails past validation and fails only at the
        // connection check (`CpNotConnected`). This pins the opt-out as reachable
        // and behaviorally distinct from the helper.
        let (server, _events) = OcppServer::new(TransportConfig::default(), Arc::new(NoopHandler));
        let err = server
            .call("NEVER_CONNECTED", remote_start(&"x".repeat(21)))
            .await
            .expect_err("no validator ⇒ reaches connection check");
        assert!(
            matches!(err, OcppError::CpNotConnected { .. }),
            "expected CpNotConnected (no validator), got {err:?}"
        );
    }

    #[tokio::test]
    async fn central_system_service_wires_outbound_validator() {
        // The one-call `central_system_service` builder attaches the 1.6J
        // validator to the `call()` path: a schema-invalid outbound CALL
        // (21-char idTag over CiString20's maxLength) is rejected as
        // `SchemaViolation` *before* the connection check — proving the outbound
        // direction is validated through the one-call builder, without a socket.
        // The inbound leg and the CALLRESULT-rejection leg are pinned over a real
        // socket in `tests/central_system_service_e2e.rs`.
        let (server, _events) = central_system_service(
            CentralSystemConfig::default(),
            TransportConfig::default(),
            |_| {},
        );
        let err = server
            .call("NEVER_CONNECTED", remote_start(&"x".repeat(21)))
            .await
            .expect_err("overlong idTag must be rejected before send");
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "expected SchemaViolation (outbound validator wired), got {err:?}"
        );
    }

    #[tokio::test]
    async fn central_system_service_customizer_runs_before_wrapping() {
        // The customizer is invoked exactly once against the fully-defaulted
        // dispatcher before it is shared into the handler. Observing the extra
        // handler's *effect* needs a socket (see the e2e suite); here we pin the
        // cheaper invariant that the hook fires, so a caller's registrations are
        // never silently dropped.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in = Arc::clone(&calls);
        let (_server, _events) = central_system_service(
            CentralSystemConfig::default(),
            TransportConfig::default(),
            move |d| {
                calls_in.fetch_add(1, Ordering::SeqCst);
                // Registering here must not drop the defaults installed above.
                assert!(d.has_handler("BootNotification"));
                assert!(d.has_validator());
            },
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "customizer runs exactly once"
        );
    }

    #[tokio::test]
    async fn central_system_server_v201_selects_v201_validator() {
        // The v201 helper attaches a *2.0.1* validator, whose schema set does not
        // include the 1.6J-only `RemoteStartTransaction` (renamed
        // `RequestStartTransaction` in 2.0.1). An outbound CALL for an action the
        // attached validator doesn't know surfaces as `NotSupported` — proving
        // the helper wired the v201 validator specifically, not the 1.6J one
        // (under which this same, otherwise-valid request would pass validation
        // and fail later with `CpNotConnected`).
        let (server, _events) =
            central_system_server_v201(TransportConfig::default(), Arc::new(NoopHandler));
        let err = server
            .call("NEVER_CONNECTED", remote_start("TAG"))
            .await
            .expect_err("RemoteStartTransaction is unknown to the v201 validator");
        assert!(
            matches!(err, OcppError::NotSupported { .. }),
            "expected NotSupported from the v201 validator, got {err:?}"
        );
    }

    /// A no-op [`MessageHandler`] — these tests drive the CSMS→CP `call()` path,
    /// which never routes through the inbound handler.
    struct NoopHandler;

    #[async_trait::async_trait]
    impl MessageHandler for NoopHandler {
        async fn handle_message(
            &self,
            _message: ocpp_messages::Message,
        ) -> ocpp_types::OcppResult<Option<ocpp_messages::Message>> {
            Ok(None)
        }

        async fn handle_event(&self, _event: TransportEvent) {}
    }
}
