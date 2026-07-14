//! Default Central System (CSMS) handler set for **OCPP 2.0.1** — spec-valid
//! responders for the core lifecycle messages a Charging Station sends.
//!
//! The 2.0.1 twin of [`crate::central_system`]. Ports the `@on` handler set from
//! the Python reference's runnable 2.0.1 CSMS example
//! ([`examples/v201/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v201/central_system.py))
//! into a reusable, overridable builder so a 2.0.1 CSMS comes up "batteries
//! included":
//!
//! | Action | Default response |
//! |---|---|
//! | `BootNotification` | `Accepted` + configurable `interval` + `currentTime` |
//! | `Heartbeat` | `currentTime` |
//! | `StatusNotification` | empty `{}` |
//! | `TransactionEvent` | empty `{}` (acknowledge; no cost/authorization policy) |
//!
//! `Authorize` and the device-model / remote-command families are intentionally
//! **out of scope**: they encode authorization and provisioning policy, which is
//! decided per-deployment rather than baked into a default responder. This
//! module is strictly the boot-time + transaction-acknowledgement lifecycle that
//! lets a station connect, boot, heartbeat, and report a transaction against a
//! stock CSMS.
//!
//! Only the version-specific handler closures differ from the 1.6J builder; the
//! [`ActionDispatcher`] / [`DispatchHandler`] / [`SchemaValidator`] machinery is
//! shared, and the 1.6J API in [`crate::central_system`] is untouched.
//!
//! The defaults are overridable: register your own `@on` for any action *after*
//! the defaults are installed and it replaces the default, matching
//! [`ActionDispatcher::on`] semantics.

use std::sync::Arc;

use chrono::{SecondsFormat, Utc};
use ocpp_messages::v201::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    StatusNotificationRequest, StatusNotificationResponse, TransactionEventRequest,
    TransactionEventResponse,
};
use ocpp_messages::{ActionDispatcher, SchemaValidator};
use ocpp_types::v201::RegistrationStatusEnumType;

use crate::DispatchHandler;

/// Configuration for the default 2.0.1 Central System handler set.
///
/// Lets callers simulate `Pending`/`Rejected` boot flows and tune the heartbeat
/// interval the CSMS advertises, without writing their own handlers. The 2.0.1
/// twin of [`CentralSystemConfig`](crate::CentralSystemConfig); it carries the
/// 2.0.1 [`RegistrationStatusEnumType`] rather than the 1.6J `RegistrationStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CentralSystemConfigV201 {
    /// Heartbeat interval (seconds) returned in the `BootNotification` response.
    ///
    /// Defaults to `300`. (The Python example uses `10` for a snappy demo;
    /// production CSMSs typically use a larger value.)
    pub boot_interval: i32,
    /// Registration status returned in the `BootNotification` response.
    ///
    /// Defaults to [`RegistrationStatusEnumType::Accepted`]; set to
    /// `Pending`/`Rejected` to exercise those charging-station code paths.
    pub registration_status: RegistrationStatusEnumType,
}

impl Default for CentralSystemConfigV201 {
    fn default() -> Self {
        Self {
            boot_interval: 300,
            registration_status: RegistrationStatusEnumType::Accepted,
        }
    }
}

/// The CSMS's current time as an RFC 3339 / ISO 8601 string with a `Z` suffix.
///
/// 2.0.1 response types carry `currentTime` as a plain `String` (unlike 1.6J's
/// typed `DateTime`), so the builder renders it here. Second precision with `Z`
/// keeps it unambiguously `format: date-time`-valid for the bundled schemas.
fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Build an [`ActionDispatcher`] pre-registered with the default 2.0.1 CS-side
/// lifecycle handlers, using [`CentralSystemConfigV201::default`].
///
/// The returned dispatcher has the bundled OCPP 2.0.1 [`SchemaValidator`]
/// attached, so incoming CALL payloads are validated before dispatch. For a
/// one-call, `OcppServer`-ready handler, prefer [`central_system_handler_v201`].
pub fn central_system_dispatcher_v201() -> ActionDispatcher {
    central_system_dispatcher_v201_with(CentralSystemConfigV201::default())
}

/// Like [`central_system_dispatcher_v201`] but with an explicit
/// [`CentralSystemConfigV201`] (custom boot interval / registration status).
pub fn central_system_dispatcher_v201_with(config: CentralSystemConfigV201) -> ActionDispatcher {
    let mut dispatcher = ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v201()));
    register_default_handlers_v201(&mut dispatcher, config);
    dispatcher
}

/// A batteries-included, `OcppServer`-ready 2.0.1 CSMS [`DispatchHandler`].
///
/// The single-call entry point the issue asks for: the returned handler is
/// pre-wired with `SchemaValidator::v201()` and the default lifecycle handlers,
/// and can be handed straight to [`OcppServer::new`](crate::server::OcppServer::new):
///
/// ```ignore
/// use std::sync::Arc;
/// use ocpp_transport::{central_system_handler_v201, OcppServer, TransportConfig};
///
/// let handler = Arc::new(central_system_handler_v201());
/// let (server, events) = OcppServer::new(TransportConfig::default(), handler);
/// ```
pub fn central_system_handler_v201() -> DispatchHandler {
    central_system_handler_v201_with(CentralSystemConfigV201::default())
}

/// Like [`central_system_handler_v201`] but with an explicit
/// [`CentralSystemConfigV201`].
pub fn central_system_handler_v201_with(config: CentralSystemConfigV201) -> DispatchHandler {
    DispatchHandler::new(Arc::new(central_system_dispatcher_v201_with(config)))
}

/// Install the default 2.0.1 lifecycle handlers onto an existing
/// [`ActionDispatcher`].
///
/// Exposed for callers who construct the dispatcher themselves (e.g. to attach a
/// custom validator, or none at all) but still want the batteries-included
/// responders. Handlers registered for the same action *after* this call replace
/// the default, matching [`ActionDispatcher::on`] semantics.
pub fn register_default_handlers_v201(
    dispatcher: &mut ActionDispatcher,
    config: CentralSystemConfigV201,
) {
    // `i32`/`RegistrationStatusEnumType` are `Copy`, so the closure stays `Clone`
    // as `ActionDispatcher::on` requires.
    let interval = config.boot_interval;
    let status = config.registration_status;

    dispatcher.on(move |_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: now_rfc3339(),
            interval,
            status,
            status_info: None,
            custom_data: None,
        })
    });

    dispatcher.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: now_rfc3339(),
            custom_data: None,
        })
    });

    dispatcher.on(|_req: StatusNotificationRequest| async move {
        Ok(StatusNotificationResponse { custom_data: None })
    });

    // A default CSMS acknowledges the unified 2.0.1 transaction message with an
    // empty `{}` — no running-cost, priority, or re-authorization policy, which
    // is deployment-specific. Mirrors the reference example's `@on` default.
    dispatcher
        .on(|_req: TransactionEventRequest| async move { Ok(TransactionEventResponse::default()) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::{CallMessage, OcppAction};
    use serde_json::{json, Value};

    /// Dispatch `action` with `payload` through a freshly built default v201
    /// dispatcher and return the serialized CALLRESULT payload.
    async fn dispatch_default(action: &str, payload: Value) -> Value {
        let d = central_system_dispatcher_v201();
        let call = CallMessage::new(action.to_string(), payload).unwrap();
        d.dispatch(&call)
            .await
            .expect("default handler should succeed")
    }

    fn valid_boot_payload() -> Value {
        json!({
            "chargingStation": { "vendorName": "ICU Eve Mini", "model": "ICU Eve Mini" },
            "reason": "PowerUp"
        })
    }

    fn valid_status_payload() -> Value {
        json!({
            "timestamp": "2026-07-13T00:00:00Z",
            "connectorStatus": "Available",
            "evseId": 1,
            "connectorId": 1
        })
    }

    fn valid_transaction_event_payload() -> Value {
        json!({
            "eventType": "Started",
            "timestamp": "2026-07-13T00:00:00Z",
            "triggerReason": "Authorized",
            "seqNo": 0,
            "transactionInfo": { "transactionId": "txn-1" }
        })
    }

    #[tokio::test]
    async fn default_boot_notification_returns_accepted_with_interval() {
        let resp = dispatch_default("BootNotification", valid_boot_payload()).await;
        assert_eq!(resp["status"], "Accepted");
        assert_eq!(resp["interval"], 300);
        let ts = resp
            .get("currentTime")
            .and_then(Value::as_str)
            .expect("boot response must carry currentTime");
        chrono::DateTime::parse_from_rfc3339(ts).expect("currentTime must be RFC3339");
    }

    #[tokio::test]
    async fn default_heartbeat_returns_current_time() {
        let resp = dispatch_default("Heartbeat", json!({})).await;
        let ts = resp
            .get("currentTime")
            .and_then(Value::as_str)
            .expect("heartbeat response must carry currentTime");
        chrono::DateTime::parse_from_rfc3339(ts).expect("currentTime must be RFC3339");
    }

    #[tokio::test]
    async fn default_status_notification_returns_empty() {
        let resp = dispatch_default("StatusNotification", valid_status_payload()).await;
        assert_eq!(resp, json!({}), "StatusNotification response must be empty");
    }

    #[tokio::test]
    async fn default_transaction_event_returns_empty() {
        let resp = dispatch_default("TransactionEvent", valid_transaction_event_payload()).await;
        assert_eq!(resp, json!({}), "TransactionEvent ack must be empty");
    }

    #[tokio::test]
    async fn custom_handler_overrides_default() {
        // Build the default set, then override BootNotification with Rejected.
        let mut d = central_system_dispatcher_v201();
        d.on(|_req: BootNotificationRequest| async move {
            Ok(BootNotificationResponse {
                current_time: now_rfc3339(),
                interval: 42,
                status: RegistrationStatusEnumType::Rejected,
                status_info: None,
                custom_data: None,
            })
        });

        let call = CallMessage::new("BootNotification".to_string(), valid_boot_payload()).unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Rejected");
        assert_eq!(resp["interval"], 42);
    }

    #[tokio::test]
    async fn config_controls_registration_status_and_interval() {
        let d = central_system_dispatcher_v201_with(CentralSystemConfigV201 {
            boot_interval: 60,
            registration_status: RegistrationStatusEnumType::Pending,
        });
        let call = CallMessage::new("BootNotification".to_string(), valid_boot_payload()).unwrap();
        let resp = d.dispatch(&call).await.unwrap();
        assert_eq!(resp["status"], "Pending");
        assert_eq!(resp["interval"], 60);
    }

    #[tokio::test]
    async fn default_responses_pass_schema_validation() {
        // The whole point of "spec-valid defaults": every default response must
        // validate against its bundled 2.0.1 `{action}Response` schema.
        let validator = SchemaValidator::v201();

        let boot = dispatch_default("BootNotification", valid_boot_payload()).await;
        validator
            .validate_call_result(BootNotificationRequest::ACTION_NAME, &boot)
            .expect("default BootNotification response must satisfy its v201 schema");

        let heartbeat = dispatch_default("Heartbeat", json!({})).await;
        validator
            .validate_call_result(HeartbeatRequest::ACTION_NAME, &heartbeat)
            .expect("default Heartbeat response must satisfy its v201 schema");

        let status = dispatch_default("StatusNotification", valid_status_payload()).await;
        validator
            .validate_call_result(StatusNotificationRequest::ACTION_NAME, &status)
            .expect("default StatusNotification response must satisfy its v201 schema");

        let txn = dispatch_default("TransactionEvent", valid_transaction_event_payload()).await;
        validator
            .validate_call_result(TransactionEventRequest::ACTION_NAME, &txn)
            .expect("default TransactionEvent response must satisfy its v201 schema");
    }

    #[tokio::test]
    async fn default_dispatcher_binds_the_v201_schema() {
        // The attached validator is 2.0.1: a 1.6J-shaped BootNotification
        // (chargePointVendor/chargePointModel) is rejected before the handler
        // runs, proving the v201 schema — not 1.6J — is in force.
        let d = central_system_dispatcher_v201();
        assert!(d.has_validator());
        let v16j_shaped = CallMessage::new(
            "BootNotification".to_string(),
            json!({ "chargePointVendor": "ACME", "chargePointModel": "Wallbox-1" }),
        )
        .unwrap();
        let err = d.dispatch(&v16j_shaped).await.unwrap_err();
        assert!(
            matches!(err, ocpp_types::OcppError::SchemaViolation { .. }),
            "expected SchemaViolation for 1.6J-shaped boot under the v201 validator, got {err:?}"
        );
    }

    #[tokio::test]
    async fn builder_registers_exactly_the_lifecycle_four() {
        let d = central_system_dispatcher_v201();
        assert!(d.has_handler("BootNotification"));
        assert!(d.has_handler("Heartbeat"));
        assert!(d.has_handler("StatusNotification"));
        assert!(d.has_handler("TransactionEvent"));
        // Authorization / device-model actions are intentionally absent.
        assert!(!d.has_handler("Authorize"));
        assert!(!d.has_handler("SetVariables"));
        assert_eq!(d.handler_count(), 4);
    }

    #[tokio::test]
    async fn handler_constructor_wraps_a_ready_dispatcher() {
        // `central_system_handler_v201` yields an OcppServer-ready DispatchHandler
        // whose dispatcher carries the validator and the four lifecycle handlers.
        let handler = central_system_handler_v201();
        let d = handler.dispatcher();
        assert!(d.has_validator());
        assert_eq!(d.handler_count(), 4);
    }
}
