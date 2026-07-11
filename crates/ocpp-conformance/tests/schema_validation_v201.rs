//! 2.0.1 schema-validation conformance suite — the OCPP 2.0.1 counterpart of
//! [`schema_validation_v16.rs`](./schema_validation_v16.rs), porting the
//! mobilityhouse/ocpp reference's
//! [`tests/test_messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py)
//! (backed by [`ocpp/messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py)'s
//! `_validate_payload`).
//!
//! `_validate_payload` is **version-agnostic** — the same routine validates
//! 1.6J and 2.0.1 payloads, differing only in the schema set and the JSON Schema
//! draft (2.0.1 is draft-06; [`SchemaValidator::run_validation`] detects the
//! draft per schema via `$schema`). So the behaviour the 1.6J suite pins ports
//! faithfully to 2.0.1: each malformed payload collapses to a single
//! [`SchemaKeyword`] which the CALLERROR layer maps to a code via
//! [`SchemaKeyword::call_error_code`]. This suite drives the **public**
//! `SchemaValidator::v201()` API at the crate boundary and asserts, for a
//! representative set of 2.0.1 actions, both the failing keyword and the
//! CALLERROR code it resolves to — the Rust analog of the reference's
//! `pytest.raises(<Exception>)`.
//!
//! Keyword → reference exception mapping (identical to the 1.6J suite, because
//! `_validate_payload` is shared):
//!
//! | reference exception              | Rust keyword            | `call_error_code()`       |
//! |----------------------------------|-------------------------|---------------------------|
//! | `TypeConstraintViolationError`   | `Type` / `MaxLength`    | `TypeConstraintViolation` |
//! | `ProtocolError`                  | `Required`              | `ProtocolError`           |
//! | `FormatViolationError`           | `AdditionalProperties`  | `FormationViolation`      |
//! | `FormatViolationError` (default) | `Other` (e.g. `enum`)   | `FormationViolation`      |
//! | `NotImplementedError`            | — (`OcppError::NotSupported`) | —                    |
//!
//! The 2.0.1 schemas are deeply nested (objects-in-objects, arrays-of-objects),
//! so several cases deliberately trip violations *below the top level* to
//! exercise draft-06 nested/`$ref` validation — something the flatter 1.6J
//! schemas can't reach.
//!
//! Part of **M8 — Conformance** (Issue #293). Test-only; no production code.

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_types::{OcppError, SchemaKeyword};
use serde_json::{json, Value};

/// Assert a CALL payload violates its schema with exactly the expected keyword,
/// and that the keyword resolves to the expected CALLERROR code — the Rust
/// analog of the reference asserting a specific `_validate_payload` exception.
fn expect_call_keyword(action: &str, payload: &Value, keyword: SchemaKeyword) {
    match SchemaValidator::v201().validate_call(action, payload) {
        Err(OcppError::SchemaViolation { keyword: got, .. }) => assert_eq!(
            got, keyword,
            "{action} CALL should fail on `{keyword}`, got `{got}`"
        ),
        other => panic!("expected SchemaViolation(`{keyword}`) for {action}, got {other:?}"),
    }
}

/// CALLRESULT counterpart of [`expect_call_keyword`] — validates against the
/// `{action}Response` schema.
fn expect_call_result_keyword(action: &str, payload: &Value, keyword: SchemaKeyword) {
    match SchemaValidator::v201().validate_call_result(action, payload) {
        Err(OcppError::SchemaViolation { keyword: got, .. }) => assert_eq!(
            got, keyword,
            "{action} CALLRESULT should fail on `{keyword}`, got `{got}`"
        ),
        other => panic!("expected SchemaViolation(`{keyword}`) for {action}, got {other:?}"),
    }
}

/// A fully-valid `BootNotification` CALL, cloned and mutated by the negative
/// cases below so each asserts a *single* isolated violation.
fn valid_boot() -> Value {
    json!({
        "reason": "PowerUp",
        "chargingStation": { "model": "Turbo-3000", "vendorName": "ACME" }
    })
}

// ---------------------------------------------------------------------------
// "must pass" positive controls — one valid payload per action validates
// cleanly (ports the 2.0.1 arm of `test_validate_payload_with_valid_payload`).
// ---------------------------------------------------------------------------

#[test]
fn valid_boot_notification_passes() {
    SchemaValidator::v201()
        .validate_call("BootNotification", &valid_boot())
        .expect("valid 2.0.1 BootNotification must validate");
}

#[test]
fn valid_boot_notification_response_passes() {
    let payload = json!({
        "currentTime": "2026-07-11T00:00:00Z",
        "interval": 300,
        "status": "Accepted"
    });
    SchemaValidator::v201()
        .validate_call_result("BootNotification", &payload)
        .expect("valid 2.0.1 BootNotificationResponse must validate");
}

#[test]
fn valid_status_notification_passes() {
    let payload = json!({
        "timestamp": "2026-07-11T00:00:00Z",
        "connectorStatus": "Available",
        "evseId": 1,
        "connectorId": 1
    });
    SchemaValidator::v201()
        .validate_call("StatusNotification", &payload)
        .expect("valid 2.0.1 StatusNotification must validate");
}

#[test]
fn valid_set_variables_passes() {
    let payload = json!({
        "setVariableData": [{
            "attributeValue": "60",
            "component": { "name": "SampledDataCtrlr" },
            "variable": { "name": "TxUpdatedInterval" }
        }]
    });
    SchemaValidator::v201()
        .validate_call("SetVariables", &payload)
        .expect("valid 2.0.1 SetVariables must validate");
}

#[test]
fn valid_meter_values_passes() {
    let payload = json!({
        "evseId": 1,
        "meterValue": [{
            "timestamp": "2026-07-11T00:00:00Z",
            "sampledValue": [{ "value": 12345.0 }]
        }]
    });
    SchemaValidator::v201()
        .validate_call("MeterValues", &payload)
        .expect("valid 2.0.1 MeterValues must validate");
}

#[test]
fn valid_transaction_event_passes() {
    let payload = json!({
        "eventType": "Started",
        "timestamp": "2026-07-11T00:00:00Z",
        "triggerReason": "Authorized",
        "seqNo": 0,
        "transactionInfo": { "transactionId": "T-0001" }
    });
    SchemaValidator::v201()
        .validate_call("TransactionEvent", &payload)
        .expect("valid 2.0.1 TransactionEvent must validate");
}

// ---------------------------------------------------------------------------
// Missing required → `Required` → ProtocolError
// (ports `test_validate_payload_with_invalid_missing_property_payload`).
// ---------------------------------------------------------------------------

/// Top-level required field absent: a `BootNotification` missing `reason`.
#[test]
fn boot_missing_required_reason_is_protocol_error() {
    let payload = json!({ "chargingStation": { "model": "M", "vendorName": "V" } });
    expect_call_keyword("BootNotification", &payload, SchemaKeyword::Required);
    assert_eq!(
        SchemaKeyword::Required.call_error_code().as_str(),
        "ProtocolError",
    );
}

/// Required field absent inside a **list element**: `SetVariables` whose only
/// `setVariableData` entry omits the required nested `variable`. Exercises
/// draft-06 required-keyword validation on an array item's `$ref` subschema.
#[test]
fn set_variables_list_element_missing_required_is_protocol_error() {
    let payload = json!({
        "setVariableData": [{
            "attributeValue": "60",
            "component": { "name": "SampledDataCtrlr" }
            // required nested `variable` omitted
        }]
    });
    expect_call_keyword("SetVariables", &payload, SchemaKeyword::Required);
}

/// `TransactionEvent` missing the required scalar `seqNo`.
#[test]
fn transaction_event_missing_required_seqno_is_protocol_error() {
    let payload = json!({
        "eventType": "Started",
        "timestamp": "2026-07-11T00:00:00Z",
        "triggerReason": "Authorized",
        "transactionInfo": { "transactionId": "T-0001" }
        // required `seqNo` omitted
    });
    expect_call_keyword("TransactionEvent", &payload, SchemaKeyword::Required);
}

// ---------------------------------------------------------------------------
// Wrong scalar type → `Type` → TypeConstraintViolation
// (ports `test_validate_payload_with_invalid_type_payload`).
// ---------------------------------------------------------------------------

/// A `StatusNotification` whose required `evseId` is a string where an integer
/// is required. All required fields are present, so `type` is the only fault.
#[test]
fn status_notification_wrong_scalar_type_is_type_constraint() {
    let payload = json!({
        "timestamp": "2026-07-11T00:00:00Z",
        "connectorStatus": "Available",
        "evseId": "1", // integer required
        "connectorId": 1
    });
    expect_call_keyword("StatusNotification", &payload, SchemaKeyword::Type);
    assert_eq!(
        SchemaKeyword::Type.call_error_code().as_str(),
        "TypeConstraintViolation",
    );
}

/// Wrong type on a **nested** field: `chargingStation.model` given as an integer
/// where the `$ref`'d `ChargingStationType` requires a string. Reaches a
/// violation one level below the top-level object.
#[test]
fn boot_nested_wrong_type_is_type_constraint() {
    let mut payload = valid_boot();
    payload["chargingStation"]["model"] = json!(42);
    expect_call_keyword("BootNotification", &payload, SchemaKeyword::Type);
}

/// Wrong type on a field nested **inside an array element inside an array
/// element**: `meterValue[].sampledValue[].value` given as a string where a
/// number is required. Exercises the deepest draft-06 nesting in the suite.
#[test]
fn meter_values_deeply_nested_wrong_type_is_type_constraint() {
    let payload = json!({
        "evseId": 1,
        "meterValue": [{
            "timestamp": "2026-07-11T00:00:00Z",
            "sampledValue": [{ "value": "not-a-number" }]
        }]
    });
    expect_call_keyword("MeterValues", &payload, SchemaKeyword::Type);
}

// ---------------------------------------------------------------------------
// maxLength → `MaxLength` → TypeConstraintViolation
// (ports `test_validate_set_maxlength_violation_payload`).
// ---------------------------------------------------------------------------

/// `chargingStation.model` has `maxLength: 20`; a 21-char value trips it. The
/// nested object is otherwise complete, so `maxLength` is the sole violation
/// (no co-firing `required`, unlike the 1.6J port).
#[test]
fn boot_nested_maxlength_is_type_constraint() {
    let mut payload = valid_boot();
    payload["chargingStation"]["model"] = json!("X".repeat(21));
    expect_call_keyword("BootNotification", &payload, SchemaKeyword::MaxLength);
    assert_eq!(
        SchemaKeyword::MaxLength.call_error_code().as_str(),
        "TypeConstraintViolation",
    );
}

// ---------------------------------------------------------------------------
// additionalProperties → `AdditionalProperties` → FormationViolation
// (ports `test_validate_payload_with_invalid_additional_properties_payload`).
// ---------------------------------------------------------------------------

/// The 2.0.1 request bodies carry `additionalProperties: false`; an unexpected
/// top-level key on an otherwise-valid `BootNotification` trips it. All required
/// fields remain present, so `additionalProperties` is the only violation.
#[test]
fn boot_extra_property_is_format_violation() {
    let mut payload = valid_boot();
    payload["notAField"] = json!(true);
    expect_call_keyword(
        "BootNotification",
        &payload,
        SchemaKeyword::AdditionalProperties,
    );
    assert_eq!(
        SchemaKeyword::AdditionalProperties
            .call_error_code()
            .as_str(),
        "FormationViolation",
    );
}

// ---------------------------------------------------------------------------
// enum-value violation → `Other` → FormationViolation.
//
// An `enum` failure has no dedicated OCPP keyword, so it falls into
// `SchemaKeyword::Other` — the Rust analog of the reference's default
// `FormatViolationError` bucket (same as the 1.6J `Reset { type: "Warm" }`
// case in `schema_validation_v16.rs`). 2.0.1 leans heavily on enums, so this
// is a high-value classification to pin.
// ---------------------------------------------------------------------------

/// Top-level enum: `BootNotification.reason` outside `BootReasonEnumType`.
#[test]
fn boot_unknown_reason_enum_is_other() {
    let mut payload = valid_boot();
    payload["reason"] = json!("Nonsense");
    expect_call_keyword("BootNotification", &payload, SchemaKeyword::Other);
    assert_eq!(
        SchemaKeyword::Other.call_error_code().as_str(),
        "FormationViolation",
    );
}

/// Enum on a field nested in a list-of-objects: a `sampledValue[].measurand`
/// value absent from `MeasurandEnumType`.
#[test]
fn meter_values_nested_unknown_measurand_enum_is_other() {
    let payload = json!({
        "evseId": 1,
        "meterValue": [{
            "timestamp": "2026-07-11T00:00:00Z",
            "sampledValue": [{ "value": 1.0, "measurand": "Not.A.Measurand" }]
        }]
    });
    expect_call_keyword("MeterValues", &payload, SchemaKeyword::Other);
}

// ---------------------------------------------------------------------------
// CALLRESULT path — the `{action}Response` schema is validated too.
// ---------------------------------------------------------------------------

/// A `BootNotificationResponse` whose `status` is not a valid
/// `RegistrationStatusEnumType` value → `Other` (enum), via the CALLRESULT
/// entrypoint (`validate_call_result`), confirming response schemas are wired
/// the same way requests are.
#[test]
fn boot_notification_response_unknown_status_enum_is_other() {
    let payload = json!({
        "currentTime": "2026-07-11T00:00:00Z",
        "interval": 300,
        "status": "Maybe"
    });
    expect_call_result_keyword("BootNotification", &payload, SchemaKeyword::Other);
}

// ---------------------------------------------------------------------------
// Unknown action → `OcppError::NotSupported`
// (ports `test_validate_payload_with_non_existing_schema`).
// ---------------------------------------------------------------------------

/// Validating against an action with no bundled 2.0.1 schema yields
/// `NotSupported` (the Rust analog of the reference's `NotImplementedError`),
/// not a `SchemaViolation` — checked for both a CALL and a CALLRESULT lookup.
#[test]
fn non_existing_v201_schema_is_not_supported() {
    match SchemaValidator::v201().validate_call("MagicSpell", &json!({})) {
        Err(OcppError::NotSupported { feature }) => assert!(
            feature.contains("MagicSpell"),
            "NotSupported should name the missing schema, got: {feature}"
        ),
        other => panic!("expected NotSupported for unknown v201 CALL, got {other:?}"),
    }
    assert!(
        matches!(
            SchemaValidator::v201().validate_call_result("MagicSpell", &json!({})),
            Err(OcppError::NotSupported { .. })
        ),
        "unknown v201 CALLRESULT action should also be NotSupported"
    );
}

// ---------------------------------------------------------------------------
// The keyword → CALLERROR-code table, pinned in one place. Identical to the
// 1.6J suite's table because `_validate_payload`'s mapping is version-agnostic;
// duplicated here so a change to `SchemaKeyword::call_error_code` is caught by
// the 2.0.1 suite on its own.
// ---------------------------------------------------------------------------

#[test]
fn keyword_to_call_error_code_table_is_pinned() {
    let cases = [
        (SchemaKeyword::Type, "TypeConstraintViolation"),
        (SchemaKeyword::MaxLength, "TypeConstraintViolation"),
        (SchemaKeyword::Required, "ProtocolError"),
        (SchemaKeyword::AdditionalProperties, "FormationViolation"),
        (SchemaKeyword::Other, "FormationViolation"),
    ];
    for (keyword, code) in cases {
        assert_eq!(
            keyword.call_error_code().as_str(),
            code,
            "`{keyword}` must map to {code}"
        );
    }
}
