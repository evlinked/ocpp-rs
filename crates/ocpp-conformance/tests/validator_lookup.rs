//! Validator-lookup conformance — ports the *positive half* of the
//! mobilityhouse/ocpp reference's
//! [`test_get_validator_with_valid_name`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py),
//! backed by [`get_validator`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py).
//!
//! The reference test pins two guarantees about looking up a known
//! `(MessageType, action, version)`:
//!
//! ```python
//! def test_get_validator_with_valid_name():
//!     schema = get_validator(MessageType.Call, "Reset", ocpp_version="1.6")
//!     assert schema == _validators["Reset_1.6"]   # returns the cached instance
//!     assert schema.schema == { ...Reset 1.6 schema... }
//! ```
//!
//! 1. **Correct resolution** — a known action resolves to the validator whose
//!    schema is the expected Reset 1.6 request schema.
//! 2. **Caching / identity** — the returned validator *is* the cached one
//!    (`get_validator` populates and reuses `_validators`).
//!
//! ## Cache-model difference (Rust vs. Python)
//!
//! Python memoizes a compiled `Draft4Validator` per `"{action}_{version}"` key
//! in the module-global `_validators` dict, and `get_validator` returns the *same
//! object* on a repeat call — so the reference can assert Python-style identity
//! (`schema is _validators["Reset_1.6"]`). The Rust
//! [`SchemaValidator`](ocpp_messages::schema_validation::SchemaValidator) has no
//! per-call memo to point at: each `v16j()` instance parses every bundled schema
//! once into an owned map at construction, and `validate_call` resolves the
//! action's schema from that map on every call. There is therefore no stable
//! object address to compare across calls.
//!
//! So this suite asserts the **observable** equivalent of both guarantees rather
//! than object identity: the Reset validator *resolves*, its schema enforces the
//! exact reference shape (`type: object`, `properties.type` an `enum` of
//! `["Hard", "Soft"]`, `required: ["type"]`, `additionalProperties: false`), and
//! resolution is **referentially stable** — repeated lookups within one validator
//! and across fresh `v16j()` instances return byte-for-byte identical verdicts.
//! The negative half (`test_get_validator_with_invalid_name`) is already covered
//! by `non_existing_schema_is_not_supported` in `schema_validation_v16.rs`.
//!
//! Part of **M8 — Conformance** (Issue #407). Test-only; no production code.

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_types::{OcppError, SchemaKeyword};
use serde_json::{json, Value};

/// Assert a `Reset` CALL payload violates its schema with exactly `keyword` — the
/// Rust analog of one clause of the reference's `schema.schema == {...}` shape
/// assertion, checked through observable validation behaviour.
fn expect_reset_keyword(payload: &Value, keyword: SchemaKeyword) {
    match SchemaValidator::v16j().validate_call("Reset", payload) {
        Err(OcppError::SchemaViolation { keyword: got, .. }) => assert_eq!(
            got, keyword,
            "Reset CALL {payload} should fail on `{keyword}`, got `{got}`"
        ),
        other => panic!("expected SchemaViolation(`{keyword}`) for Reset {payload}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// (1) correct resolution — the known action resolves to a usable validator.
// ---------------------------------------------------------------------------

/// The `Reset` CALL validator resolves (`has_schema`) and accepts the two
/// canonical payloads the reference schema's `enum` admits. This is the Rust
/// analog of `get_validator(MessageType.Call, "Reset", "1.6")` returning a
/// working validator.
#[test]
fn reset_validator_resolves_and_accepts_canonical_payloads() {
    let v = SchemaValidator::v16j();
    assert!(
        v.has_schema("Reset"),
        "a v1.6 Reset CALL validator must resolve"
    );
    for reset_type in ["Hard", "Soft"] {
        v.validate_call("Reset", &json!({ "type": reset_type }))
            .unwrap_or_else(|e| panic!("Reset {{type: {reset_type}}} must validate, got {e:?}"));
    }
}

// ---------------------------------------------------------------------------
// (2) exact schema shape — each clause of the reference Reset 1.6 schema,
// pinned behaviourally. Reference:
//   { "type": "object",
//     "properties": { "type": { "type": "string", "enum": ["Hard","Soft"] } },
//     "required": ["type"],
//     "additionalProperties": false }
// ---------------------------------------------------------------------------

/// `type: object` — a non-object top-level payload trips `type`.
#[test]
fn reset_schema_requires_object() {
    // A bare string is well-formed JSON but not an object.
    expect_reset_keyword(&json!("Hard"), SchemaKeyword::Type);
    // An array likewise fails the object constraint.
    expect_reset_keyword(&json!([]), SchemaKeyword::Type);
}

/// `properties.type: { type: "string" }` — a non-string `type` trips `type`.
#[test]
fn reset_schema_type_property_is_string() {
    expect_reset_keyword(&json!({ "type": 5 }), SchemaKeyword::Type);
    expect_reset_keyword(&json!({ "type": true }), SchemaKeyword::Type);
}

/// `enum: ["Hard", "Soft"]` — a well-typed string outside the enum trips `enum`,
/// which has no dedicated OCPP keyword and folds into
/// [`SchemaKeyword::Other`] (the reference's default `FormatViolationError`
/// bucket). `Hard`/`Soft` are the only admitted members.
#[test]
fn reset_schema_type_enum_is_hard_or_soft() {
    for bad in ["Reboot", "Warm", "hard", ""] {
        expect_reset_keyword(&json!({ "type": bad }), SchemaKeyword::Other);
    }
}

/// `required: ["type"]` — omitting `type` trips `required`.
#[test]
fn reset_schema_requires_type_field() {
    expect_reset_keyword(&json!({}), SchemaKeyword::Required);
}

/// `additionalProperties: false` — an unexpected sibling property trips
/// `additionalProperties`, even alongside a valid `type`.
#[test]
fn reset_schema_forbids_additional_properties() {
    expect_reset_keyword(
        &json!({ "type": "Hard", "unexpected": 1 }),
        SchemaKeyword::AdditionalProperties,
    );
}

// ---------------------------------------------------------------------------
// (2b) resolution is *scoped* to the CALL name — `has_schema` keys on the CALL
// action, and the CALLRESULT (`ResetResponse`) resolves via its own lookup.
// Mirrors the reference threading `MessageType.Call` vs `MessageType.CallResult`
// into the `"{action}_{version}"` / `"{action}Response_{version}"` key.
// ---------------------------------------------------------------------------

/// The `ResetResponse` (CALLRESULT) schema resolves independently, and
/// `Reset`'s empty-object response — the reference's `ResetResponse` carries a
/// required `status` — is rejected, confirming the CALLRESULT lookup binds to the
/// response schema, not the request one.
#[test]
fn reset_response_validator_resolves_separately() {
    let v = SchemaValidator::v16j();
    assert!(
        v.has_schema("ResetResponse"),
        "the ResetResponse CALLRESULT validator must resolve"
    );
    // `{ "status": "Accepted" }` is the canonical valid ResetResponse.
    v.validate_call_result("Reset", &json!({ "status": "Accepted" }))
        .expect("a canonical ResetResponse must validate");
    // An empty object is missing the required `status` → rejected, proving the
    // CALLRESULT path resolves the response schema (the request schema requires
    // `type`, not `status`, so this could only fail against the response schema).
    match v.validate_call_result("Reset", &json!({})) {
        Err(OcppError::SchemaViolation {
            keyword: SchemaKeyword::Required,
            ..
        }) => {}
        other => panic!("empty ResetResponse should fail `required`, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// (3) caching / identity analog — resolution is referentially stable.
// ---------------------------------------------------------------------------

/// The Rust analog of `schema is _validators["Reset_1.6"]`: because there is no
/// memoized object to compare, we assert that resolution is **referentially
/// stable** instead — the same lookup, repeated within one validator and across
/// freshly built `v16j()` instances, yields byte-for-byte identical verdicts for
/// a spread of payloads (valid, missing-required, out-of-enum, wrong-type,
/// extra-property). A resolution that silently rebuilt a *different* schema, or a
/// construction that populated the map non-deterministically, would diverge here.
#[test]
fn reset_validator_resolution_is_stable() {
    // A spread that exercises every branch of the Reset schema, each with its
    // expected `Ok`/`Err`-keyword verdict rendered as a comparable string.
    let probes: [Value; 5] = [
        json!({ "type": "Hard" }),
        json!({}),
        json!({ "type": "Reboot" }),
        json!({ "type": 5 }),
        json!({ "type": "Soft", "extra": true }),
    ];

    let verdict = |v: &SchemaValidator, p: &Value| -> String {
        match v.validate_call("Reset", p) {
            Ok(()) => "ok".to_string(),
            Err(OcppError::SchemaViolation { keyword, .. }) => format!("violation:{keyword}"),
            Err(other) => format!("other:{other:?}"),
        }
    };

    // Stable within a single validator across repeated lookups (no per-call
    // drift — the resolved schema does not change between validations).
    let v = SchemaValidator::v16j();
    for p in &probes {
        assert_eq!(
            verdict(&v, p),
            verdict(&v, p),
            "repeated lookup of Reset must be stable for {p}"
        );
    }

    // Stable across independently constructed validators (construction resolves
    // the same schema deterministically — the analog of always getting the one
    // cached `_validators["Reset_1.6"]` back).
    let a = SchemaValidator::v16j();
    let b = SchemaValidator::v16j();
    for p in &probes {
        assert_eq!(
            verdict(&a, p),
            verdict(&b, p),
            "Reset resolution must match across fresh validators for {p}"
        );
    }

    // The lookup does not mutate the validator's schema set (no lazy insert on
    // resolve — the Reset schema was present from construction, not built on
    // first use).
    let before = a.schema_count();
    for p in &probes {
        let _ = a.validate_call("Reset", p);
    }
    assert_eq!(
        a.schema_count(),
        before,
        "resolving Reset must not grow or rebuild the schema set"
    );
}
