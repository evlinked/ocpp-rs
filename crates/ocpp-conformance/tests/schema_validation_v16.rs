//! 1.6J schema-validation conformance suite — ports the mobilityhouse/ocpp
//! reference's [`tests/test_messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py)
//! (backed by [`ocpp/messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py)'s
//! `_validate_payload` and `unpack`).
//!
//! `test_messages.py` is the reference's authoritative behavioural spec for
//! `_validate_payload()`: it pins the exact exception each malformed payload
//! produces, plus several tricky "must pass" cases (float-precision limits, the
//! `Hertz`/`Frequency` errata). On the Rust side that responsibility lives in
//! [`SchemaValidator`] (`ocpp-messages`), which collapses the rich `jsonschema`
//! error set down to a single [`SchemaKeyword`] that the CALLERROR layer maps to
//! an error code via [`SchemaKeyword::call_error_code`]. This suite drives the
//! **public** `SchemaValidator::v16j()` API at the crate boundary and asserts,
//! for each reference case, both the failing keyword and the CALLERROR code it
//! resolves to — the Rust analog of the reference's `pytest.raises(<Exception>)`.
//!
//! Keyword → reference exception mapping (see `_validate_payload`):
//!
//! | reference exception              | Rust keyword            | `call_error_code()`            |
//! |----------------------------------|-------------------------|--------------------------------|
//! | `TypeConstraintViolationError`   | `Type` / `MaxLength`    | `TypeConstraintViolation`      |
//! | `ProtocolError`                  | `Required`              | `ProtocolError`                |
//! | `FormatViolationError`           | `AdditionalProperties`  | `FormationViolation`           |
//! | `NotImplementedError`            | — (`OcppError::NotSupported`) | —                         |
//!
//! The final block ports the `unpack` framing edge-cases (`test_unpack_*`),
//! pinning them against the Rust framing layer's actual classification and
//! documenting where it diverges from the reference's `ProtocolError` /
//! `PropertyConstraintViolation` split.
//!
//! Part of **M8 — Conformance** (Issue #263). Test-only; no production code.

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::serialization::{format, MessageSerializer};
use ocpp_types::{OcppError, SchemaKeyword};
use serde_json::{json, Value};

/// Assert a payload violates its CALL schema with exactly the expected keyword,
/// and that the keyword resolves to the expected CALLERROR code — the Rust
/// analog of the reference asserting a specific `_validate_payload` exception.
fn expect_call_keyword(action: &str, payload: &Value, keyword: SchemaKeyword) {
    match SchemaValidator::v16j().validate_call(action, payload) {
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
    match SchemaValidator::v16j().validate_call_result(action, payload) {
        Err(OcppError::SchemaViolation { keyword: got, .. }) => assert_eq!(
            got, keyword,
            "{action} CALLRESULT should fail on `{keyword}`, got `{got}`"
        ),
        other => panic!("expected SchemaViolation(`{keyword}`) for {action}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// "must pass" cases — valid payloads validate cleanly.
// ---------------------------------------------------------------------------

/// Ports `test_validate_payload_with_valid_payload` (the 1.6 arm): a well-formed
/// `HeartbeatResponse` validates without error.
#[test]
fn valid_heartbeat_response_passes() {
    let payload = json!({ "currentTime": "2021-06-15T14:01:32Z" });
    SchemaValidator::v16j()
        .validate_call_result("Heartbeat", &payload)
        .expect("valid HeartbeatResponse must validate");
}

/// Ports `test_validate_set_charging_profile_payload`: a `SetChargingProfile`
/// CALL carrying the float limit `21.4` must validate. `21.4` is internally
/// `21.399999999999999…`, which trips `multipleOf: 0.1` as a pure f64
/// representation artifact — the Rust validator drops it exactly like the
/// reference's `decimal.Decimal` re-parse in `_validate_payload`.
#[test]
fn set_charging_profile_float_limit_passes() {
    let payload = json!({
        "connectorId": 1,
        "csChargingProfiles": {
            "chargingProfileId": 1,
            "stackLevel": 0,
            "chargingProfilePurpose": "TxProfile",
            "chargingProfileKind": "Relative",
            "chargingSchedule": {
                "chargingRateUnit": "A",
                "chargingSchedulePeriod": [{ "startPeriod": 0, "limit": 21.4 }]
            },
            "transactionId": 123456789
        }
    });
    SchemaValidator::v16j()
        .validate_call("SetChargingProfile", &payload)
        .expect("SetChargingProfile with 21.4 limit must validate");
}

/// Ports `test_validate_get_composite_profile_payload`: a
/// `GetCompositeScheduleResponse` carrying the float limit `15.2` must validate
/// (same f64-vs-`multipleOf` artifact as `21.4`).
#[test]
fn get_composite_schedule_float_limit_passes() {
    let payload = json!({
        "status": "Accepted",
        "connectorId": 1,
        "scheduleStart": "2021-06-15T14:01:32Z",
        "chargingSchedule": {
            "duration": 60,
            "chargingRateUnit": "A",
            "chargingSchedulePeriod": [{ "startPeriod": 0, "limit": 15.2 }]
        }
    });
    SchemaValidator::v16j()
        .validate_call_result("GetCompositeSchedule", &payload)
        .expect("GetCompositeScheduleResponse with 15.2 limit must validate");
}

/// Ports `test_validate_meter_values_hertz`: a `MeterValues` CALL whose sampled
/// value carries unit `"Hertz"` / measurand `"Frequency"` must validate. Missing
/// from the original 1.6 spec, this was added as an errata (OCPP 1.6 Errata sheet
/// v4.0, 2019-10-23, p.34). Guards that the bundled `MeterValues.json` still
/// carries the errata members (new coverage on the Rust side).
#[test]
fn meter_values_hertz_errata_validates() {
    let payload = json!({
        "connectorId": 1,
        "transactionId": 123456789,
        "meterValue": [{
            "timestamp": "2020-02-21T13:48:45.459756Z",
            "sampledValue": [{
                "value": "50.0",
                "measurand": "Frequency",
                "unit": "Hertz"
            }]
        }]
    });
    SchemaValidator::v16j()
        .validate_call("MeterValues", &payload)
        .expect("MeterValues with the Hertz/Frequency errata must validate");
}

// ---------------------------------------------------------------------------
// classification cases — each maps to the reference exception it ports.
// ---------------------------------------------------------------------------

/// Ports `test_validate_payload_with_invalid_additional_properties_payload`
/// (reference: `FormatViolationError`). An unexpected property on a
/// `HeartbeatResponse` trips `additionalProperties`. The reference payload
/// `{"invalid_key": true}` also omits the required `currentTime`, so both
/// `additionalProperties` and `required` fire; the validator's documented
/// precedence (`additionalProperties` > `required`) selects the same keyword the
/// reference's first-error inspection reports.
#[test]
fn additional_properties_is_format_violation() {
    let payload = json!({ "invalid_key": true });
    expect_call_result_keyword("Heartbeat", &payload, SchemaKeyword::AdditionalProperties);
    assert_eq!(
        SchemaKeyword::AdditionalProperties
            .call_error_code()
            .as_str(),
        "FormationViolation",
        "additionalProperties must map to the FormatViolationError analog",
    );
}

/// Ports `test_v16_charge_point.py::test_send_invalid_call` (reference:
/// `validate_payload` raising on the schema's `enum` keyword). A `Reset` CALL
/// whose `type` is `"Medium"` — a well-typed string that is not one of the
/// schema's `enum` members `Hard`/`Soft` — trips `enum`. No dedicated OCPP
/// keyword exists for `enum`, so the validator folds it into
/// [`SchemaKeyword::Other`], which resolves to `FormationViolation`. All other
/// constraints pass (`type` is present and a string), so `enum` is the sole
/// violation. On the typed Rust API this value is unconstructable, but the
/// validator still guards raw inbound frames — so this pins the `enum` →
/// `FormationViolation` classification the reference asserts.
#[test]
fn out_of_enum_value_is_format_violation() {
    let payload = json!({ "type": "Medium" }); // only Hard/Soft are valid
    expect_call_keyword("Reset", &payload, SchemaKeyword::Other);
    assert_eq!(
        SchemaKeyword::Other.call_error_code().as_str(),
        "FormationViolation",
        "an out-of-enum value must map to the FormatViolationError analog",
    );
}

/// Ports `test_validate_payload_with_invalid_type_payload` (reference:
/// `TypeConstraintViolationError`). A `StartTransaction` CALL whose `meterStart`
/// is a string where an integer is required trips `type`. All required fields are
/// present so `type` is the only violation.
#[test]
fn wrong_type_is_type_constraint() {
    let payload = json!({
        "connectorId": 1,
        "idTag": "okTag",
        "meterStart": "invalid_type",
        "timestamp": "2022-01-25T19:18:30.018Z"
    });
    expect_call_keyword("StartTransaction", &payload, SchemaKeyword::Type);
    assert_eq!(
        SchemaKeyword::Type.call_error_code().as_str(),
        "TypeConstraintViolation",
    );
}

/// Ports `test_validate_payload_with_invalid_missing_property_payload`
/// (reference: `ProtocolError`). A `StartTransaction` CALL missing the required
/// `meterStart` trips `required`.
#[test]
fn missing_required_is_protocol_error() {
    let payload = json!({
        "connectorId": 1,
        "idTag": "okTag",
        // meterStart purposely missing
        "timestamp": "2022-01-25T19:18:30.018Z"
    });
    expect_call_keyword("StartTransaction", &payload, SchemaKeyword::Required);
    assert_eq!(
        SchemaKeyword::Required.call_error_code().as_str(),
        "ProtocolError",
    );
}

/// Ports `test_validate_set_maxlength_violation_payload` (reference:
/// `TypeConstraintViolationError`). A `StartTransaction` CALL whose `idTag`
/// exceeds `maxLength: 20` (21 chars) trips `maxLength`. The reference payload
/// also omits `meterStart`/`timestamp`, so `required` co-fires; precedence
/// (`maxLength` > `required`) keeps `maxLength` dominant — matching the
/// reference's `TypeConstraintViolationError`.
#[test]
fn maxlength_is_type_constraint() {
    let payload = json!({
        "idTag": "012345678901234567890", // 21 chars
        "connectorId": 1
    });
    expect_call_keyword("StartTransaction", &payload, SchemaKeyword::MaxLength);
    assert_eq!(
        SchemaKeyword::MaxLength.call_error_code().as_str(),
        "TypeConstraintViolation",
    );
}

/// Ports `test_validate_payload_with_non_existing_schema` (reference:
/// `NotImplementedError`). Validating against an action with no bundled schema
/// yields `OcppError::NotSupported` — the Rust analog — rather than a
/// `SchemaViolation`. Checked for both a CALL and a CALLRESULT lookup.
#[test]
fn non_existing_schema_is_not_supported() {
    let payload = json!({ "invalid_key": true });
    match SchemaValidator::v16j().validate_call_result("MagicSpell", &payload) {
        Err(OcppError::NotSupported { feature }) => {
            assert!(
                feature.contains("MagicSpellResponse"),
                "NotSupported should name the missing schema, got: {feature}"
            );
        }
        other => panic!("expected NotSupported for unknown action, got {other:?}"),
    }
    assert!(
        matches!(
            SchemaValidator::v16j().validate_call("MagicSpell", &payload),
            Err(OcppError::NotSupported { .. })
        ),
        "unknown CALL action should also be NotSupported"
    );
}

/// The full keyword → CALLERROR-code table `_validate_payload` implements, pinned
/// in one place so a change to [`SchemaKeyword::call_error_code`] is caught here.
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

// ---------------------------------------------------------------------------
// `unpack` framing edge-cases — ports `test_unpack_*`.
//
// The reference's `unpack()` performs, in order: JSON parse (→
// `FormatViolationError`), is-a-list (→ `ProtocolError`), non-empty + has a
// MessageTypeId (→ `ProtocolError`), MessageTypeId ∈ {2,3,4} (→
// `PropertyConstraintViolation`). The Rust framing layer splits these across
// two entrypoints: `MessageSerializer::deserialize_message` parses (a serde
// failure → `OcppError::Json`, the `FormatViolationError` analog), and
// `format::validate_json_structure` performs the structural checks, folding
// every structural fault — non-array, too-short, bad MessageTypeId — into
// `OcppError::ProtocolViolation`. That fold is a documented divergence: the
// reference distinguishes `PropertyConstraintViolation` for a bad
// MessageTypeId, whereas Rust reports `ProtocolViolation` for all of them.
// ---------------------------------------------------------------------------

/// Ports `test_unpack_with_invalid_json`: non-JSON input is rejected at the parse
/// step. Reference → `FormatViolationError`; Rust → `OcppError::Json` (the
/// analog — a malformed frame the peer sent).
#[test]
fn framing_invalid_json_is_rejected() {
    match MessageSerializer::new().deserialize_message("\u{1}", "1.6J") {
        Err(OcppError::Json { .. }) => {}
        other => panic!("expected Json error for invalid JSON, got {other:?}"),
    }
}

/// Ports `test_unpack_without_jsonified_list` (reference: `ProtocolError`): a
/// JSON value that isn't an array is a structural fault → `ProtocolViolation`.
#[test]
fn framing_non_array_is_protocol_violation() {
    match format::validate_json_structure(&json!("3")) {
        Err(OcppError::ProtocolViolation { .. }) => {}
        other => panic!("expected ProtocolViolation for non-array frame, got {other:?}"),
    }
}

/// Ports `test_unpack_without_message_type_id_in_json` (reference:
/// `ProtocolError`): an empty array carries no MessageTypeId → `ProtocolViolation`.
#[test]
fn framing_empty_array_is_protocol_violation() {
    match format::validate_json_structure(&json!([])) {
        Err(OcppError::ProtocolViolation { .. }) => {}
        other => panic!("expected ProtocolViolation for empty frame, got {other:?}"),
    }
}

/// Ports `test_unpack_with_invalid_message_type_id_in_json` (reference:
/// `PropertyConstraintViolation`): a MessageTypeId outside {2,3,4}. The Rust
/// structural validator folds this into `ProtocolViolation` (documented
/// divergence — Rust does not raise a distinct `PropertyConstraintViolation`),
/// and the message names the offending id.
#[test]
fn framing_invalid_message_type_id_is_protocol_violation() {
    // A 3-element array so the length check passes and the MessageTypeId check
    // is the one that fires (the reference's `[5, 1]` fails length first here).
    match format::validate_json_structure(&json!([5, "1", {}])) {
        Err(OcppError::ProtocolViolation { message }) => assert!(
            message.contains('5'),
            "ProtocolViolation should name the invalid MessageTypeId, got: {message}"
        ),
        other => panic!("expected ProtocolViolation for MessageTypeId 5, got {other:?}"),
    }

    // Through the real deserialize path, an otherwise-Call-shaped frame with a
    // bad MessageTypeId surfaces as `InvalidMessageType` from `into_message`.
    match MessageSerializer::new().deserialize_message(r#"[5,"1","Heartbeat",{}]"#, "1.6J") {
        Err(OcppError::InvalidMessageType(5)) => {}
        other => panic!("expected InvalidMessageType(5) via deserialize, got {other:?}"),
    }
}
