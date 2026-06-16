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
}
