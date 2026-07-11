//! End-to-end: OCPP 2.0.1 CALLs route through [`ActionDispatcher`] against the
//! **2.0.1** schema set (Issue #258).
//!
//! The `ActionDispatcher` is version-generic — it dispatches on
//! `OcppAction::ACTION_NAME` and takes an injectable [`SchemaValidator`] — but
//! every dispatch test in `dispatcher.rs` exercises 1.6J actions. These tests
//! close the "is 2.0.1 actually wired?" question (#256, option 2) by driving the
//! public API with `SchemaValidator::v201()` and real v201 message types,
//! faithful to `charge_point.py`'s `route_message` / `_handle_call` for 2.0.1:
//! validate-before-dispatch, correct-handler routing, and the `@after` hook.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ocpp_messages::v201::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::{ActionDispatcher, CallMessage, CallResultMessage, SchemaValidator};
use ocpp_types::v201::RegistrationStatusEnumType;
use ocpp_types::OcppError;

use serde_json::json;
use tokio::sync::Notify;

/// A dispatcher whose incoming CALLs are validated against the bundled 2.0.1
/// schemas — the wiring under test.
fn v201_dispatcher() -> ActionDispatcher {
    ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v201()))
}

/// The CSMS's canned `BootNotification.conf`. A fixed `currentTime` keeps the
/// round-trip deterministic; the schema enforces `format: date-time`.
fn boot_response() -> BootNotificationResponse {
    BootNotificationResponse {
        current_time: "2026-07-06T00:00:00Z".to_string(),
        interval: 300,
        status: RegistrationStatusEnumType::Accepted,
        status_info: None,
        custom_data: None,
    }
}

/// A v201-shaped `BootNotification.req` payload (`chargingStation` + `reason`),
/// matching the reference `tests/v201/conftest.py` fixture.
fn v201_boot_payload() -> serde_json::Value {
    json!({
        "chargingStation": { "vendorName": "ICU Eve Mini", "model": "ICU Eve Mini" },
        "reason": "PowerUp"
    })
}

/// Register the core-lifecycle v201 handlers (`BootNotification`, `Heartbeat`,
/// `StatusNotification`) on `d`, each returning a spec-valid response.
fn register_lifecycle_handlers(d: &mut ActionDispatcher) {
    d.on(|_req: BootNotificationRequest| async move { Ok(boot_response()) });
    d.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: "2026-07-06T00:00:00Z".to_string(),
            custom_data: None,
        })
    });
    d.on(|_req: StatusNotificationRequest| async move {
        Ok(StatusNotificationResponse { custom_data: None })
    });
}

/// A valid v201 CALL round-trips CALL → CALLRESULT: the handler runs, the
/// returned payload deserializes back into the strongly-typed response, the
/// CALLRESULT echoes the CALL's `unique_id`, and the response itself is a valid
/// 2.0.1 `BootNotification.conf`.
#[tokio::test]
async fn v201_boot_notification_round_trips_call_to_call_result() {
    let mut d = v201_dispatcher();
    register_lifecycle_handlers(&mut d);

    let call = CallMessage::new("BootNotification".to_string(), v201_boot_payload()).unwrap();
    let response_payload = d.dispatch(&call).await.unwrap();

    // Build the CALLRESULT frame the transport would send back.
    let result = CallResultMessage::new(call.unique_id.clone(), &response_payload).unwrap();
    assert_eq!(
        result.unique_id, call.unique_id,
        "CALLRESULT must echo the CALL's unique_id"
    );

    // The payload deserializes into the strongly-typed v201 response.
    let typed: BootNotificationResponse = result.payload_as().unwrap();
    assert_eq!(typed.status, RegistrationStatusEnumType::Accepted);
    assert_eq!(typed.interval, 300);

    // And it is a spec-valid `BootNotification.conf` on the wire.
    let validator = SchemaValidator::v201();
    validator
        .validate_call_result("BootNotification", &result.payload)
        .expect("response payload must validate against the v201 CALLRESULT schema");
}

/// The dispatcher is bound to the **2.0.1** schema set, not 1.6J: a v201-shaped
/// `BootNotification` is accepted, but a **1.6J-shaped** one
/// (`chargePointVendor` / `chargePointModel`) sent to the same `BootNotification`
/// action is rejected before the handler runs.
#[tokio::test]
async fn v201_dispatcher_binds_the_v201_schema_not_v16j() {
    let handler_ran = Arc::new(AtomicBool::new(false));
    let flag = handler_ran.clone();

    let mut d = v201_dispatcher();
    d.on(move |_req: BootNotificationRequest| {
        let flag = flag.clone();
        async move {
            flag.store(true, Ordering::SeqCst);
            Ok(boot_response())
        }
    });

    // v201 shape → accepted, handler runs.
    let good = CallMessage::new("BootNotification".to_string(), v201_boot_payload()).unwrap();
    d.dispatch(&good)
        .await
        .expect("v201-shaped boot must dispatch");
    assert!(handler_ran.load(Ordering::SeqCst));

    handler_ran.store(false, Ordering::SeqCst);

    // 1.6J shape → rejected by the v201 schema (unknown props / missing required),
    // handler never runs.
    let v16j_shaped = CallMessage::new(
        "BootNotification".to_string(),
        json!({ "chargePointVendor": "ACME", "chargePointModel": "Model-1" }),
    )
    .unwrap();
    let err = d.dispatch(&v16j_shaped).await.unwrap_err();
    assert!(
        matches!(err, OcppError::SchemaViolation { .. }),
        "1.6J-shaped payload must fail the v201 schema, got {err:?}"
    );
    assert!(
        !handler_ran.load(Ordering::SeqCst),
        "handler must not run when v201 validation fails"
    );
}

/// Validate-before-dispatch: a malformed v201 payload short-circuits to a
/// `SchemaViolation` and the handler never runs — mirroring `_validate()` at the
/// top of `_handle_call` in `charge_point.py`.
#[tokio::test]
async fn v201_malformed_payload_short_circuits_before_handler() {
    let handler_ran = Arc::new(AtomicBool::new(false));
    let flag = handler_ran.clone();

    let mut d = v201_dispatcher();
    d.on(move |_req: StatusNotificationRequest| {
        let flag = flag.clone();
        async move {
            flag.store(true, Ordering::SeqCst);
            Ok(StatusNotificationResponse { custom_data: None })
        }
    });

    // Missing the required `connectorId` field.
    let missing_field = CallMessage::new(
        "StatusNotification".to_string(),
        json!({
            "timestamp": "2026-07-06T00:00:00Z",
            "connectorStatus": "Available",
            "evseId": 1
        }),
    )
    .unwrap();
    let err = d.dispatch(&missing_field).await.unwrap_err();
    assert!(
        matches!(err, OcppError::SchemaViolation { .. }),
        "missing required field must be a SchemaViolation, got {err:?}"
    );

    // `Charging` is a 1.6J-only connector state, absent from the 2.0.1 enum.
    let unknown_enum = CallMessage::new(
        "StatusNotification".to_string(),
        json!({
            "timestamp": "2026-07-06T00:00:00Z",
            "connectorStatus": "Charging",
            "evseId": 1,
            "connectorId": 1
        }),
    )
    .unwrap();
    let err = d.dispatch(&unknown_enum).await.unwrap_err();
    assert!(
        matches!(err, OcppError::SchemaViolation { .. }),
        "unknown enum value must be a SchemaViolation, got {err:?}"
    );

    assert!(
        !handler_ran.load(Ordering::SeqCst),
        "handler must never run when the payload fails v201 validation"
    );
}

/// Multiple v201 handlers on one dispatcher route to the correct one with no
/// cross-talk. Ports the `_raise_key_error(action, "2.0.1")` split (Issue #276):
/// an unregistered *known* v201 action (the `v201()` validator has its schema)
/// yields `NotImplemented`, while an action the version does not define yields
/// `NotSupported`.
#[tokio::test]
async fn v201_handlers_route_to_the_correct_action() {
    let mut d = v201_dispatcher();
    register_lifecycle_handlers(&mut d);
    assert_eq!(d.handler_count(), 3);

    // Heartbeat.req carries no fields.
    let hb = CallMessage::new("Heartbeat".to_string(), json!({})).unwrap();
    let hb_resp = d.dispatch(&hb).await.unwrap();
    assert_eq!(hb_resp["currentTime"], "2026-07-06T00:00:00Z");
    assert!(
        hb_resp.get("status").is_none(),
        "Heartbeat.conf has no status field"
    );

    let sn = CallMessage::new(
        "StatusNotification".to_string(),
        json!({
            "timestamp": "2026-07-06T00:00:00Z",
            "connectorStatus": "Available",
            "evseId": 1,
            "connectorId": 1
        }),
    )
    .unwrap();
    let sn_resp = d.dispatch(&sn).await.unwrap();
    assert_eq!(
        sn_resp,
        json!({}),
        "StatusNotification.conf is an empty object"
    );

    let boot = CallMessage::new("BootNotification".to_string(), v201_boot_payload()).unwrap();
    let boot_resp = d.dispatch(&boot).await.unwrap();
    assert_eq!(boot_resp["status"], "Accepted");

    // A *known* v201 action with no registered handler → NotImplemented: the
    // `v201()` validator has the Authorize schema, so it is a valid 2.0.1 action
    // the dispatcher simply has no handler for (the `NotImplementedError` branch
    // of `_raise_key_error`).
    let authorize = CallMessage::new(
        "Authorize".to_string(),
        json!({ "idToken": { "idToken": "abc", "type": "ISO14443" } }),
    )
    .unwrap();
    let err = d.dispatch(&authorize).await.unwrap_err();
    assert!(
        matches!(err, OcppError::NotImplemented { ref feature } if feature == "Authorize"),
        "a known-but-unregistered v201 action must be NotImplemented, got {err:?}"
    );

    // An action the version does not define (no bundled schema) → NotSupported,
    // the `NotSupportedError` branch of the same split.
    let undefined = CallMessage::new("NoSuchV201Action".to_string(), json!({})).unwrap();
    let err = d.dispatch(&undefined).await.unwrap_err();
    assert!(
        matches!(err, OcppError::NotSupported { ref feature } if feature == "NoSuchV201Action"),
        "an action the version does not define must be NotSupported, got {err:?}"
    );
}

/// An `@after` hook registered for a v201 action fires after a successful
/// dispatch (the fire-and-forget `@after` semantics apply to 2.0.1 too).
#[tokio::test]
async fn v201_after_hook_fires_after_successful_dispatch() {
    let notify = Arc::new(Notify::new());
    let n = notify.clone();

    let mut d = v201_dispatcher();
    d.on(|_req: BootNotificationRequest| async move { Ok(boot_response()) });
    d.after(move |_req: BootNotificationRequest| {
        let n = n.clone();
        async move {
            n.notify_one();
        }
    });

    let call = CallMessage::new("BootNotification".to_string(), v201_boot_payload()).unwrap();
    d.dispatch(&call).await.unwrap();

    tokio::time::timeout(Duration::from_millis(100), notify.notified())
        .await
        .expect("v201 @after hook did not fire within 100 ms");
}
