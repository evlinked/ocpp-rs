//! Single v201 end-to-end dispatch + `@after` injection — ports the
//! mobilityhouse/ocpp reference's
//! [`tests/v201/test_v201_charge_point.py::test_route_message_with_existing_route`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v201/test_v201_charge_point.py).
//!
//! ## What the reference pins
//!
//! ```python
//! @on(Action.boot_notification)
//! def on_boot_notification(reason, charging_station, **kwargs):
//!     assert reason == "PowerUp"
//!     assert charging_station == {
//!         "vendor_name": "ICU Eve Mini",
//!         "firmware_version": "#1:3.4.0-2990#N:217H;1.0-223",
//!         "model": "ICU Eve Mini",
//!     }
//!     return call_result.BootNotification(
//!         current_time="2018-05-29T17:37:05.495259", interval=350, status="Accepted")
//!
//! @after(Action.boot_notification)
//! def after_boot_notification(reason, charging_station, **kwargs):
//!     assert reason == "PowerUp"
//!     assert charging_station == { ... }  # same request fields
//!
//! await base_central_system.route_message(boot_notification_call)
//! base_central_system._connection.send.assert_called_once_with(
//!     json.dumps([3, "1",
//!         {"currentTime": "2018-05-29T17:37:05.495259", "interval": 350, "status": "Accepted"}],
//!         separators=(",", ":")))
//! ```
//!
//! A single v201 CALL is routed to its registered `@on` handler; the request
//! payload is injected into the handler with correct camelCase↔snake_case field
//! mapping; the handler's response is threaded into the `@after` hook; and the
//! whole thing round-trips to a wire CALLRESULT with the exact 2.0.1 field
//! spellings (`currentTime` / `interval` / `status`).
//!
//! ## Why this consolidates piecewise coverage (Issue #402)
//!
//! Each half of this invariant is already pinned somewhere in the Rust suite,
//! but no single test drives all of them **in one v201 flow** the way the
//! reference does:
//!
//! - v201 dispatch + serde field-injection + response serialization →
//!   `ocpp-transport/src/central_system_v201.rs::default_boot_notification_returns_accepted_with_interval`;
//! - `@after` hook receives the request **and** the response →
//!   `routing.rs::after_with_response_injects_the_on_handlers_response` — but
//!   that test uses a **v16** `BootNotification`, not v201;
//! - camelCase wire spellings → `field_wire_spellings.rs` / `data_types_v201.rs`.
//!
//! This test exercises "v201 `@on` + `@after` request/response injection + wire
//! serialization" end-to-end through the [`ActionDispatcher`] with
//! [`SchemaValidator::v201`] attached, exactly as the reference's
//! `test_route_message_with_existing_route` does through `route_message`.
//!
//! Mechanism under test: `ActionDispatcher` (`ocpp-messages`) with
//! `SchemaValidator::v201()`. Part of **M8 — Conformance**.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ocpp_messages::v201::{BootNotificationRequest, BootNotificationResponse};
use ocpp_messages::{ActionDispatcher, CallMessage, SchemaValidator};
use ocpp_types::v201::{BootReasonEnumType, ChargingStationType, RegistrationStatusEnumType};
use serde_json::json;
use tokio::sync::Notify;

/// The reference's `boot_notification_call` fixture, on the wire: a v201
/// `BootNotification` CALL payload in camelCase (`chargingStation`,
/// `vendorName`, `firmwareVersion`).
fn boot_notification_payload() -> serde_json::Value {
    json!({
        "reason": "PowerUp",
        "chargingStation": {
            "vendorName": "ICU Eve Mini",
            "firmwareVersion": "#1:3.4.0-2990#N:217H;1.0-223",
            "model": "ICU Eve Mini",
        }
    })
}

/// The reference's hard-coded `call_result.BootNotification`.
///
/// The reference's literal `current_time` is `"2018-05-29T17:37:05.495259"` — a
/// naive timestamp with **no** timezone offset, which its mock connection never
/// validates. The Rust dispatcher, unlike the Python test, validates the
/// **outbound** response against the 2.0.1 schema, whose `currentTime` carries a
/// `"date-time"` format (RFC 3339, offset required). So we keep the reference's
/// exact date/time and append an explicit `Z` to make it a schema-valid
/// `date-time` — the stricter-than-reference outbound check is itself part of
/// what this end-to-end flow exercises.
fn fixed_boot_response() -> BootNotificationResponse {
    BootNotificationResponse {
        current_time: "2018-05-29T17:37:05.495259Z".to_string(),
        interval: 350,
        status: RegistrationStatusEnumType::Accepted,
        status_info: None,
        custom_data: None,
    }
}

/// Assert the request the reference's `on_boot_notification` /
/// `after_boot_notification` handlers receive — `reason == "PowerUp"` and the
/// three `chargingStation` fields deserialized into their snake_case struct
/// fields.
fn assert_injected_request(req: &BootNotificationRequest) {
    assert_eq!(req.reason, BootReasonEnumType::PowerUp);
    assert_eq!(
        req.charging_station,
        ChargingStationType {
            vendor_name: "ICU Eve Mini".to_string(),
            model: "ICU Eve Mini".to_string(),
            serial_number: None,
            firmware_version: Some("#1:3.4.0-2990#N:217H;1.0-223".to_string()),
            modem: None,
            custom_data: None,
        }
    );
}

/// Port of `test_route_message_with_existing_route`: one v201 CALL dispatched
/// through `@on` → response → `@after` injection with `SchemaValidator::v201()`,
/// asserting both the wire-serialized CALLRESULT and the `@after` hook's
/// observed request + response.
#[tokio::test]
async fn v201_call_routes_through_on_and_after_with_wire_serialized_result() {
    // Capture what the (spawned) `@after` hook observes so the assertions run on
    // the test thread — a panic inside the spawned hook would not fail the test
    // on its own. Mirrors the reference asserting a call count of 1 so the
    // hook's inner assertions are not silently skipped.
    let seen = Arc::new(Mutex::new(
        None::<(BootNotificationRequest, BootNotificationResponse)>,
    ));
    let notify = Arc::new(Notify::new());

    // A CS-side dispatcher with the bundled OCPP 2.0.1 schema attached, so the
    // inbound CALL payload is validated before dispatch — the version context
    // the reference gets from `ocpp.v201.ChargePoint`.
    let mut d = ActionDispatcher::new().with_validator(Arc::new(SchemaValidator::v201()));

    // `@on(Action.boot_notification)` — assert the injected request (runs inline
    // in `dispatch()`, so a mismatch fails the test directly) and return the
    // reference's fixed response.
    d.on(move |req: BootNotificationRequest| async move {
        assert_injected_request(&req);
        Ok(fixed_boot_response())
    });

    // `@after(Action.boot_notification)` — the reference's `after` hook re-checks
    // the request fields; the Rust `after_with_response` hook additionally sees
    // the exact response threaded through from `@on`, pinning `call_response`
    // injection for v201.
    let s = seen.clone();
    let n = notify.clone();
    d.after_with_response(
        move |req: BootNotificationRequest, resp: BootNotificationResponse| {
            let s = s.clone();
            let n = n.clone();
            async move {
                *s.lock().unwrap() = Some((req, resp));
                n.notify_one();
            }
        },
    );

    let call = CallMessage::new("BootNotification".to_string(), boot_notification_payload())
        .expect("valid CALL frame");
    let result = d
        .dispatch(&call)
        .await
        .expect("v201 BootNotification dispatch");

    // The wire CALLRESULT payload: exact 2.0.1 field spellings and values, matching
    // the reference's `json.dumps([3, "1", {...}])`. `statusInfo`/`customData` are
    // `skip_serializing_if = None`, so the object is exactly these three keys.
    assert_eq!(
        result,
        json!({
            "currentTime": "2018-05-29T17:37:05.495259Z",
            "interval": 350,
            "status": "Accepted",
        })
    );

    // The `@after` hook is spawned; wait (bounded) for it rather than sleeping a
    // fixed duration, to keep the test non-flaky under load.
    tokio::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("the @after hook must fire after a successful v201 dispatch");

    let (after_req, after_resp) = seen
        .lock()
        .unwrap()
        .take()
        .expect("the @after hook must have observed the request and response");
    // The hook saw the same injected request the reference's `after` re-asserts…
    assert_injected_request(&after_req);
    // …and the exact response threaded through from the `@on` handler.
    assert_eq!(after_resp, fixed_boot_response());
}
