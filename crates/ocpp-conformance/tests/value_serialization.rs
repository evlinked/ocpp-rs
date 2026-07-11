//! Outgoing value-encoding conformance suite — ports the mobilityhouse/ocpp
//! reference's `test_serializing_decimal` and `test_serializing_custom_types`
//! from [`tests/test_messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py)
//! (backed by [`ocpp/messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py)).
//!
//! These two tests pin the reference's *serialize side*: how a value is encoded
//! once it is on the wire, the companion to the omission/camelCase suite
//! (`payload_serialization.rs`, #291) and the validation suites
//! (`schema_validation_v16.rs` #263 / `schema_validation_v201.rs` #293) — those
//! pin *which* keys go on the wire and *how* an error classifies, not *how a
//! value is encoded*.
//!
//! 1. **`test_serializing_decimal`** — the reference's `_DecimalEncoder` collapses
//!    a float-derived `Decimal` so a charging-limit like `21.4` never lands as
//!    `21.399999999999999`. On the Rust side numbers are `f64` and `serde_json`
//!    emits the **shortest round-trippable decimal** (ryu), so a typed `limit:
//!    f64 = 21.4` serializes to the token `21.4`. This is the serialize-side
//!    mirror of the validator's `is_multiple_of_precision_artifact` handling:
//!    the *validate* side already forgives the f64 `multipleOf` artifact (see
//!    `schema_validation_v16.rs::set_charging_profile_float_limit_passes`);
//!    nothing pinned that the *serialize* side emits the clean value in the
//!    first place. We pin it for both 1.6J and 2.0.1, and re-validate the
//!    emitted payload through `SchemaValidator`.
//!
//!    Divergence, pinned deliberately (per the suite convention of pinning the
//!    actual Rust behaviour, not silently matching the reference): the Python
//!    `_DecimalEncoder` force-rounds *every* `Decimal` to one decimal place
//!    (`float("%.1f" % obj)`), so `Decimal(2.000001)` → `2.0`. Rust does **not**
//!    round — `serde_json` emits the shortest value that round-trips the `f64`,
//!    so `2.000001_f64` → `2.000001`. The two coincide exactly on the spec's
//!    one-decimal charging limits (`21.4`, `15.2`, …), which is the case that
//!    matters for interop and the case we pin. `precision_is_not_force_rounded`
//!    documents the divergence explicitly.
//!
//! 2. **`test_serializing_custom_types`** — a `Call` whose payload fails
//!    validation is turned into a `CallError` and serialized to JSON without
//!    error (regression guard for mobilityhouse/ocpp#395, where `Call` was not
//!    JSON-serializable). The Rust analog: a payload that fails
//!    `SchemaValidator::validate_call` yields an `OcppError::SchemaViolation`
//!    whose `SchemaKeyword` resolves (via `call_error_code`) to a
//!    `CallErrorCode`; a `Message::CallError` built from it serializes to a
//!    well-formed `[4, "<id>", "<code>", "<desc>", {…}]` frame — through the
//!    real wire path `MessageSerializer::serialize_message` — **without
//!    panicking**, and deserializes back to the same code. `exceptions_v16.rs`
//!    (#264) round-trips CALLERROR *codes*; this pins that a *validation
//!    failure* serializes cleanly into one.
//!
//! Each `#[test]` names the `test_messages.py` function it ports. Test-only; no
//! production code. Part of **M8 — Conformance** (Issue #295).

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::serialization::MessageSerializer;
use ocpp_types::v16j::ChargingSchedulePeriod;
use ocpp_types::v201::SampledValueType;
use ocpp_types::{CallErrorCode, Message, OcppError, SchemaKeyword};
use serde_json::{json, Value};

/// Serialize a [`Message`] through the public wire path, panicking with context
/// on failure — the analog of the reference's `.to_json()`.
fn wire(message: &Message, version: &str) -> String {
    MessageSerializer::new()
        .serialize_message(message, version)
        .unwrap_or_else(|e| panic!("serialize_message({version}) failed: {e:?}"))
}

// ---------------------------------------------------------------------------
// Decimal / number encoding — ports `test_serializing_decimal`.
// ---------------------------------------------------------------------------

/// The 1.6J arm: a `SetChargingProfile` CALL carrying the typed one-decimal
/// limit `21.4` serializes to the clean token `21.4` (not the f64 artifact
/// `21.399999999999999`, and not the padded `21.40`), and the emitted payload
/// re-validates through `SchemaValidator::v16j()`.
#[test]
fn v16j_decimal_limit_serializes_clean_and_revalidates() {
    // Build the numeric leaf from a typed `f64`, so this pins the serde model
    // (not a JSON literal) — `21.4_f64` is internally `21.39999999999999857…`.
    let period = ChargingSchedulePeriod {
        start_period: 0,
        limit: 21.4,
        number_phases: None,
    };
    let payload = json!({
        "connectorId": 1,
        "csChargingProfiles": {
            "chargingProfileId": 1,
            "stackLevel": 0,
            "chargingProfilePurpose": "TxProfile",
            "chargingProfileKind": "Relative",
            "chargingSchedule": {
                "chargingRateUnit": "A",
                "chargingSchedulePeriod": [serde_json::to_value(&period).unwrap()]
            },
            "transactionId": 123456789
        }
    });

    let msg = Message::call("SetChargingProfile".to_string(), &payload).unwrap();
    let json_str = wire(&msg, "1.6J");

    assert!(
        json_str.contains("\"limit\":21.4"),
        "expected clean `\"limit\":21.4` on the wire, got: {json_str}"
    );
    assert!(
        !json_str.contains("21.399") && !json_str.contains("21.40"),
        "wire must not carry an f64 artifact or padded zero: {json_str}"
    );

    SchemaValidator::v16j()
        .validate_call("SetChargingProfile", &payload)
        .expect("the emitted 21.4 payload must re-validate");
}

/// The 2.0.1 arm: a `MeterValues` CALL whose typed `SampledValueType.value` is
/// the one-decimal reading `21.4` serializes to the clean token `21.4` and
/// re-validates through `SchemaValidator::v201()`. (MeterValues' `value` is the
/// 2.0.1 decimal-on-the-wire analog of the 1.6J charging-schedule limit.)
#[test]
fn v201_decimal_value_serializes_clean_and_revalidates() {
    let sampled = SampledValueType {
        value: 21.4,
        context: None,
        measurand: None,
        phase: None,
        location: None,
        signed_meter_value: None,
        unit_of_measure: None,
        custom_data: None,
    };
    let payload = json!({
        "evseId": 1,
        "meterValue": [{
            "timestamp": "2026-07-11T00:00:00Z",
            "sampledValue": [serde_json::to_value(&sampled).unwrap()]
        }]
    });

    let msg = Message::call("MeterValues".to_string(), &payload).unwrap();
    let json_str = wire(&msg, "2.0.1");

    assert!(
        json_str.contains("\"value\":21.4"),
        "expected clean `\"value\":21.4` on the wire, got: {json_str}"
    );
    assert!(
        !json_str.contains("21.399") && !json_str.contains("21.40"),
        "wire must not carry an f64 artifact or padded zero: {json_str}"
    );

    SchemaValidator::v201()
        .validate_call("MeterValues", &payload)
        .expect("the emitted 21.4 payload must re-validate");
}

/// Pins the divergence from `_DecimalEncoder` (§1 of the module docs): Rust does
/// **not** force-round to one decimal place. `2.000001_f64` serializes to
/// `2.000001` on the wire, where the reference's `_DecimalEncoder` would have
/// emitted `2.0`. The suite pins the actual Rust behaviour rather than the
/// reference's lossy 1-dp rounding, which only ever applied to `Decimal`-typed
/// fields re-parsed with `parse_float=decimal.Decimal`.
#[test]
fn precision_is_not_force_rounded() {
    let period = ChargingSchedulePeriod {
        start_period: 0,
        limit: 2.000001,
        number_phases: None,
    };
    let json_str = serde_json::to_string(&period).unwrap();
    assert!(
        json_str.contains("\"limit\":2.000001"),
        "Rust keeps the shortest round-trippable f64 (no 1-dp rounding): {json_str}"
    );
}

// ---------------------------------------------------------------------------
// Error → CALLERROR serialization — ports `test_serializing_custom_types`.
// ---------------------------------------------------------------------------

/// Drive an invalid CALL through the validator, turn the resulting
/// `OcppError::SchemaViolation` into a `Message::CallError`, serialize it, and
/// assert a well-formed `[4, id, code, desc, details]` frame that also
/// round-trips back to the same code. Returns nothing — the point is that none
/// of these steps panic (regression guard for mobilityhouse/ocpp#395).
fn assert_error_serializes_to_call_error(
    validator: SchemaValidator,
    version: &str,
    action: &str,
    bad_payload: &Value,
    expected_keyword: SchemaKeyword,
    expected_code: CallErrorCode,
) {
    let err = validator
        .validate_call(action, bad_payload)
        .expect_err("payload must fail validation");

    let (keyword, description) = match err {
        OcppError::SchemaViolation { keyword, message } => (keyword, message),
        other => panic!("expected SchemaViolation for {action}, got {other:?}"),
    };
    assert_eq!(keyword, expected_keyword, "{action} keyword");

    let code = keyword.call_error_code();
    assert_eq!(code, expected_code, "{action} CALLERROR code");

    // Build the CALLERROR the way a CSMS would when rejecting a bad CALL, using
    // the offending Call's unique_id (the reference uses "1234").
    let call_error = Message::call_error(
        "1234".to_string(),
        code,
        description,
        Some(json!({ "action": action })),
    );

    // to_json() analog — must not panic (the #395 bug).
    let json_str = wire(&call_error, version);

    // Well-formed 5-element CALLERROR frame: [4, "1234", "<code>", "<desc>", {…}].
    let frame: Vec<Value> = serde_json::from_str(&json_str).expect("CALLERROR is valid JSON array");
    assert_eq!(frame.len(), 5, "CALLERROR has 5 elements: {json_str}");
    assert_eq!(frame[0], json!(4), "MessageTypeId is 4");
    assert_eq!(frame[1], json!("1234"), "unique_id preserved");
    assert_eq!(frame[2], json!(expected_code.as_str()), "error code");
    assert!(frame[3].is_string(), "error description is a string");
    assert!(frame[4].is_object(), "error details is an object");

    // Round-trips back to the same typed code — the peer that receives it can
    // recover the classification.
    match MessageSerializer::new()
        .deserialize_message(&json_str, version)
        .expect("CALLERROR re-parses")
    {
        Message::CallError(m) => assert_eq!(m.error_code, expected_code, "code round-trips"),
        other => panic!("expected CallError, got {other:?}"),
    }
}

/// Ports `test_serializing_custom_types` verbatim: the reference's 1.6J
/// `StartTransaction` with `meterStart: "invalid_type"` fails on `type`
/// (`TypeConstraintViolationError`) and serializes cleanly into a
/// `TypeConstraintViolation` CALLERROR.
#[test]
fn v16j_invalid_call_serializes_into_call_error() {
    let bad = json!({
        "connectorId": 1,
        "idTag": "okTag",
        "meterStart": "invalid_type",
        "timestamp": "2022-01-25T19:18:30.018Z"
    });
    assert_error_serializes_to_call_error(
        SchemaValidator::v16j(),
        "1.6J",
        "StartTransaction",
        &bad,
        SchemaKeyword::Type,
        CallErrorCode::TypeConstraintViolation,
    );
}

/// 2.0.1 analog of `test_serializing_custom_types`: a `TransactionEvent` with a
/// wrong-typed `seqNo` (string where an integer is required) fails on `type`
/// and serializes cleanly into a `TypeConstraintViolation` CALLERROR through the
/// 2.0.1 wire path.
#[test]
fn v201_invalid_call_serializes_into_call_error() {
    let bad = json!({
        "eventType": "Started",
        "timestamp": "2026-07-11T00:00:00Z",
        "triggerReason": "Authorized",
        "seqNo": "not-an-integer",
        "transactionInfo": { "transactionId": "T-0001" }
    });
    assert_error_serializes_to_call_error(
        SchemaValidator::v201(),
        "2.0.1",
        "TransactionEvent",
        &bad,
        SchemaKeyword::Type,
        CallErrorCode::TypeConstraintViolation,
    );
}
