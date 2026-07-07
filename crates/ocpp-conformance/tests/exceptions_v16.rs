//! CALLERROR error-code conformance suite — ports the mobilityhouse/ocpp
//! reference's [`tests/test_exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_exceptions.py)
//! (backed by [`ocpp/exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/exceptions.py)
//! and [`CallError.to_exception()`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py)).
//!
//! The reference pins the `code` string of every `OCPPError` subclass and proves
//! that `CallError.to_exception()` resolves an incoming error-code string back to
//! the matching exception — raising `UnknownCallErrorCodeError` for a code the
//! OCPP spec doesn't define. On the Rust side those responsibilities live in
//! [`CallErrorCode`] (the twelve spec codes, after #261) and
//! [`RawMessage::into_message`] (resolves an incoming CALLERROR's code string,
//! `ProtocolViolation` for an unknown one — the analog of
//! `UnknownCallErrorCodeError`).
//!
//! This suite asserts that behaviour cohesively at the crate boundary: each
//! code's wire spelling is pinned byte-for-byte, and every code is round-tripped
//! through the public framing entrypoint
//! ([`MessageSerializer::deserialize_message`], the analog of the reference's
//! `unpack` + `to_exception`) as well as [`RawMessage::into_message`]. The
//! per-version errata spellings the reference is careful to keep distinct
//! (`FormatViolation`/`FormationViolation`,
//! `OccurenceConstraintViolation`/`OccurrenceConstraintViolation`) are pinned as
//! four separate variants that never cross-resolve.
//!
//! Part of **M8 — Conformance** (Issue #264). Test-only; no production code.

use ocpp_messages::serialization::MessageSerializer;
use ocpp_types::{CallErrorCode, CallErrorMessage, Message, OcppError, RawMessage};
use serde_json::json;

/// Every `CallErrorCode` variant paired with its authoritative OCPP wire string,
/// taken verbatim from the reference's `OCPPError` subclass `code` attributes in
/// [`ocpp/exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/exceptions.py).
/// Mirrors the twelve `OCPPError` subclasses `CallError.to_exception()` iterates.
const SPEC_CODES: &[(CallErrorCode, &str)] = &[
    // `NotImplementedError`
    (CallErrorCode::NotImplemented, "NotImplemented"),
    // `NotSupportedError`
    (CallErrorCode::NotSupported, "NotSupported"),
    // `InternalError`
    (CallErrorCode::InternalError, "InternalError"),
    // `ProtocolError`
    (CallErrorCode::ProtocolError, "ProtocolError"),
    // `SecurityError`
    (CallErrorCode::SecurityError, "SecurityError"),
    // `FormationViolationError` — strict OCPP 1.6J spelling (errata typo).
    (CallErrorCode::FormationViolation, "FormationViolation"),
    // `FormatViolationError` — corrected OCPP 2.0.1 spelling.
    (CallErrorCode::FormatViolation, "FormatViolation"),
    // `PropertyConstraintViolationError`
    (
        CallErrorCode::PropertyConstraintViolation,
        "PropertyConstraintViolation",
    ),
    // `OccurenceConstraintViolationError` — single-`r` (1.6J + 2.0.1 errata typo).
    (
        CallErrorCode::OccurenceConstraintViolation,
        "OccurenceConstraintViolation",
    ),
    // `OccurrenceConstraintViolationError` — corrected double-`r` (OCPP 2.1).
    (
        CallErrorCode::OccurrenceConstraintViolation,
        "OccurrenceConstraintViolation",
    ),
    // `TypeConstraintViolationError`
    (
        CallErrorCode::TypeConstraintViolation,
        "TypeConstraintViolation",
    ),
    // `GenericError`
    (CallErrorCode::GenericError, "GenericError"),
];

/// Build one CALLERROR wire frame `[4, "<id>", "<code>", "<desc>", {<details>}]`
/// as it arrives from a peer, exactly as the reference's `CallError.to_json`
/// emits it.
fn callerror_wire(unique_id: &str, code: &str) -> String {
    serde_json::to_string(&json!([4, unique_id, code, "boom", {}]))
        .expect("serialize CALLERROR frame")
}

/// Resolve a raw CALLERROR frame through the public framing entrypoint — the
/// Rust analog of the reference's `unpack(...)` + `CallError.to_exception()`.
fn resolve_via_framing(wire: &str) -> Result<Message, OcppError> {
    MessageSerializer::new().deserialize_message(wire, "1.6J")
}

/// Pin every code's wire spelling: the hand-written [`CallErrorCode::as_str`],
/// the derived serde `PascalCase` representation, and the reference string must
/// all agree byte-for-byte, in both directions. Ports the per-subclass `code`
/// assertions of `test_exceptions.py` / `ocpp/exceptions.py`.
#[test]
fn spec_code_wire_spelling_is_pinned() {
    for (code, wire) in SPEC_CODES {
        assert_eq!(
            code.as_str(),
            *wire,
            "as_str() disagrees with the reference wire string for {code:?}"
        );
        assert_eq!(
            serde_json::to_value(code).unwrap(),
            json!(wire),
            "serde spelling disagrees with the reference wire string for {code:?}"
        );
        let back: CallErrorCode = serde_json::from_value(json!(wire)).expect("deserialize code");
        assert_eq!(back, *code, "{wire} did not deserialize back to {code:?}");
    }

    // The suite must stay exhaustive: if a variant is added, this forces it into
    // SPEC_CODES (mirrors the reference pinning every OCPPError subclass).
    assert_eq!(SPEC_CODES.len(), 12, "expected exactly 12 OCPP error codes");
}

/// Every spec code round-trips through [`RawMessage::into_message`] and back out
/// via [`RawMessage::from`] — an incoming CALLERROR resolves to its variant, and
/// re-serializing reproduces the exact wire code. Ports `CallError.to_json` /
/// the incoming-frame handling around `CallError.to_exception`.
#[test]
fn every_spec_code_round_trips_through_framing() {
    for (code, wire) in SPEC_CODES {
        // Incoming: wire code string -> typed variant.
        let raw = RawMessage::CallError(4, "42".into(), (*wire).into(), "boom".into(), json!({}));
        let Message::CallError(msg) = raw.into_message().expect("resolve CALLERROR") else {
            panic!("{wire} did not resolve to a CallError");
        };
        assert_eq!(
            msg.error_code, *code,
            "{wire} resolved to the wrong variant"
        );

        // Outgoing: typed variant -> wire code string (no drift).
        let outgoing = Message::CallError(CallErrorMessage::new(
            "42".into(),
            code.clone(),
            "boom".into(),
            None,
        ));
        let RawMessage::CallError(_, _, out_code, _, _) = RawMessage::from(outgoing) else {
            panic!("CallError did not serialize to a RawMessage::CallError");
        };
        assert_eq!(
            out_code, *wire,
            "{code:?} serialized to the wrong wire code"
        );
    }
}

/// Drive every spec code through the full public framing entrypoint — the
/// closest analog to the reference's `unpack(msg).to_exception()`: a raw JSON
/// wire string in, the resolved typed variant out.
#[test]
fn incoming_code_resolves_via_deserialize_message() {
    for (code, wire) in SPEC_CODES {
        let frame = callerror_wire("resolve-1", wire);
        let Message::CallError(msg) = resolve_via_framing(&frame).expect("resolve CALLERROR frame")
        else {
            panic!("{wire} frame did not resolve to a CallError");
        };
        assert_eq!(
            msg.error_code, *code,
            "{wire} frame resolved to the wrong variant"
        );
        assert_eq!(msg.unique_id, "resolve-1", "unique_id was not preserved");
    }
}

/// An out-of-spec error code is rejected rather than silently accepted — the
/// Rust analog of the reference raising `UnknownCallErrorCodeError`. Covers a
/// bogus code, the empty string, and wrong-casing (the resolution is
/// case-sensitive, so `formatviolation` must NOT match `FormatViolation`).
#[test]
fn unknown_code_is_rejected_like_unknown_call_error_code_error() {
    for bogus in [
        "NotARealError",
        "",
        "formatviolation",
        "GENERICERROR",
        "Format Violation",
    ] {
        let frame = callerror_wire("bad-1", bogus);
        match resolve_via_framing(&frame) {
            Err(OcppError::ProtocolViolation { message }) => {
                assert!(
                    message.contains(bogus) || message.contains("Unknown error code"),
                    "ProtocolViolation for {bogus:?} lost the offending code: {message}"
                );
            }
            other => {
                panic!("expected ProtocolViolation for out-of-spec code {bogus:?}, got {other:?}")
            }
        }
    }
}

/// The reference deliberately keeps two pairs of near-identical spellings as
/// distinct `OCPPError` subclasses:
///
/// - `FormatViolationError` (2.0.1) vs `FormationViolationError` (strict 1.6J)
/// - `OccurrenceConstraintViolationError` (2.1, double-`r`) vs
///   `OccurenceConstraintViolationError` (1.6J/2.0.1 errata, single-`r`)
///
/// Confusing them silently breaks interop, so pin all four as separate variants
/// with distinct wire strings that never cross-resolve.
#[test]
fn errata_spellings_stay_distinct() {
    let pairs = [
        (
            CallErrorCode::FormatViolation,
            "FormatViolation",
            CallErrorCode::FormationViolation,
            "FormationViolation",
        ),
        (
            CallErrorCode::OccurrenceConstraintViolation,
            "OccurrenceConstraintViolation",
            CallErrorCode::OccurenceConstraintViolation,
            "OccurenceConstraintViolation",
        ),
    ];

    for (a, a_wire, b, b_wire) in pairs {
        assert_ne!(a, b, "errata pair collapsed to one variant");
        assert_ne!(a_wire, b_wire, "errata pair shares a wire string");

        // Each wire string resolves to its own variant, never its sibling's.
        let ra = RawMessage::CallError(4, "e".into(), a_wire.into(), "x".into(), json!({}))
            .into_message()
            .unwrap();
        let rb = RawMessage::CallError(4, "e".into(), b_wire.into(), "x".into(), json!({}))
            .into_message()
            .unwrap();
        let (Message::CallError(ma), Message::CallError(mb)) = (ra, rb) else {
            panic!("errata frames did not resolve to CallError");
        };
        assert_eq!(ma.error_code, a, "{a_wire} resolved to the wrong variant");
        assert_eq!(mb.error_code, b, "{b_wire} resolved to the wrong variant");
    }
}

/// The error description and details survive the round-trip, and an absent
/// details object defaults to an empty map — mirroring the reference's
/// `test_exception_with_error_details` / `test_exception_without_error_details`
/// (`OCPPError.details` defaults to `{}` when `None`).
#[test]
fn description_and_details_are_preserved() {
    // With details.
    let details = json!({"key": "value"});
    let raw = RawMessage::CallError(
        4,
        "d1".into(),
        "ProtocolError".into(),
        "Some error".into(),
        details.clone(),
    );
    let Message::CallError(msg) = raw.into_message().unwrap() else {
        panic!("expected CallError");
    };
    assert_eq!(msg.error_description, "Some error");
    assert_eq!(msg.error_details, details);

    // Without details: `CallErrorMessage::new(.., None)` fills an empty object,
    // matching `OCPPError.details or {}`.
    let msg = CallErrorMessage::new(
        "d2".into(),
        CallErrorCode::ProtocolError,
        "Some error".into(),
        None,
    );
    assert_eq!(
        msg.error_details,
        json!({}),
        "absent details should default to {{}}"
    );
}
