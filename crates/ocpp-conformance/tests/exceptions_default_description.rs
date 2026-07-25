//! Per-code `default_description` conformance suite — pins the Rust
//! [`CallErrorCode::default_description`] registry and the
//! `description=None → default_description` fallback byte-for-byte against the
//! mobilityhouse/ocpp reference.
//!
//! ## What the reference pins
//!
//! Every OCPP error subclass in
//! [`ocpp/exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/exceptions.py)
//! declares a canonical `default_description`, and `OCPPError.__init__` uses it
//! as the description whenever an error is raised without an explicit one:
//!
//! ```python
//! def __init__(self, description=None, details=None):
//!     self.description = description
//!     if description is None:
//!         self.description = self.default_description
//! ```
//!
//! [`tests/test_exceptions.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_exceptions.py)
//! pins the fallback directly:
//!
//! ```python
//! def test_exception_without_error_details():
//!     exception = ProtocolError()
//!     assert exception.description == "Payload for Action is incomplete"
//!     assert exception.details == {}
//! ```
//!
//! ## Why this suite exists alongside `call_error_details.rs`
//!
//! `call_error_details.rs` pins the *wire frame* the CSMS emits for an unrouted
//! action (code + description + `cause`) by driving the real `DispatchHandler`.
//! Since #379 the two descriptions it depends on are sourced from
//! [`CallErrorCode::default_description`] rather than duplicated string literals,
//! and the other ten codes' canonical descriptions gained a home too. This suite
//! pins that registry and the [`CallErrorMessage::from_code`] fallback — the port
//! of `OCPPError.__init__` — which `call_error_details.rs` does not cover.
//!
//! Part of **M8 — Conformance** (Issue #379). Faithful reproduction of
//! `ocpp/exceptions.py`.

use ocpp_types::{CallErrorCode, CallErrorMessage};
use serde_json::json;

/// Every `CallErrorCode` paired with its authoritative `default_description`,
/// transcribed verbatim from `ocpp/exceptions.py`. This table is the reference
/// oracle: it is written out independently of the production `match` so a typo
/// in either surface is caught here.
///
/// Faithful-port quirks preserved exactly:
/// - `NotImplemented` reads *"Request Action"* (not *"Requested"*) — a reference typo.
/// - `Format`/`Formation` and `Occurence`/`Occurrence` each pair to one description.
/// - `TypeConstraintViolation` embeds curly quotes (`\u{201c}\u{201d}`), not ASCII.
const REFERENCE_DEFAULT_DESCRIPTIONS: &[(CallErrorCode, &str)] = &[
    (
        CallErrorCode::NotImplemented,
        "Request Action is recognized but not supported by the receiver",
    ),
    (
        CallErrorCode::NotSupported,
        "Requested Action is not known by receiver",
    ),
    (
        CallErrorCode::InternalError,
        "An internal error occurred and the receiver was not able to process the requested Action successfully",
    ),
    (
        CallErrorCode::ProtocolError,
        "Payload for Action is incomplete",
    ),
    (
        CallErrorCode::SecurityError,
        "During the processing of Action a security issue occurred preventing receiver from completing the Action successfully",
    ),
    (
        CallErrorCode::FormatViolation,
        "Payload for Action is syntactically incorrect or structure for Action",
    ),
    (
        CallErrorCode::FormationViolation,
        "Payload for Action is syntactically incorrect or structure for Action",
    ),
    (
        CallErrorCode::PropertyConstraintViolation,
        "Payload is syntactically correct but at least one field contains an invalid value",
    ),
    (
        CallErrorCode::OccurenceConstraintViolation,
        "Payload for Action is syntactically correct but at least one of the fields violates occurence constraints",
    ),
    (
        CallErrorCode::OccurrenceConstraintViolation,
        "Payload for Action is syntactically correct but at least one of the fields violates occurence constraints",
    ),
    (
        CallErrorCode::TypeConstraintViolation,
        "Payload for Action is syntactically correct but at least one of the fields violates data type constraints (e.g. \u{201c}somestring\u{201d}: 12)",
    ),
    (
        CallErrorCode::GenericError,
        "Any other error not all other OCPP defined errors",
    ),
];

/// Every code's `default_description()` matches the reference oracle byte-for-byte.
#[test]
fn all_default_descriptions_match_reference() {
    for (code, expected) in REFERENCE_DEFAULT_DESCRIPTIONS {
        assert_eq!(
            code.default_description(),
            *expected,
            "default_description for {code:?} diverges from ocpp/exceptions.py"
        );
    }
}

/// The oracle enumerates all twelve reference codes (guards against a code being
/// added to the enum but forgotten here / in the production `match`).
#[test]
fn oracle_covers_all_twelve_codes() {
    assert_eq!(
        REFERENCE_DEFAULT_DESCRIPTIONS.len(),
        12,
        "the reference defines exactly twelve OCPP error codes"
    );
}

/// Ports `test_exception_without_error_details`: a CALLERROR built from a code
/// with no explicit description falls back to that code's `default_description`,
/// and absent details default to an empty object — matching
/// `OCPPError(description=None, details=None)`.
#[test]
fn from_code_without_details_uses_default_description() {
    let msg = CallErrorMessage::from_code("u1".into(), CallErrorCode::ProtocolError, None);
    assert_eq!(
        msg.error_description, "Payload for Action is incomplete",
        "description must fall back to ProtocolError's default_description"
    );
    assert_eq!(
        msg.error_details,
        json!({}),
        "absent details must default to {{}} (OCPPError.details or {{}})"
    );
    assert_eq!(msg.error_code, CallErrorCode::ProtocolError);

    // Holds for every code, not just ProtocolError.
    for (code, expected) in REFERENCE_DEFAULT_DESCRIPTIONS {
        let m = CallErrorMessage::from_code("u".into(), code.clone(), None);
        assert_eq!(&m.error_description, expected);
        assert_eq!(m.error_details, json!({}));
    }
}

/// The with-details variant of `from_code` keeps the caller's details while
/// still defaulting the description — the analog of
/// `test_exception_with_error_details` for the no-description path.
#[test]
fn from_code_with_details_preserves_details_and_defaults_description() {
    let details = json!({ "cause": "boom", "key": "value" });
    let msg = CallErrorMessage::from_code(
        "u2".into(),
        CallErrorCode::TypeConstraintViolation,
        Some(details.clone()),
    );
    assert_eq!(
        msg.error_description,
        CallErrorCode::TypeConstraintViolation.default_description()
    );
    assert_eq!(msg.error_details, details);
}

/// The two spelling-pair codes share a single description, exactly as the
/// reference (which gives `FormatViolationError`/`FormationViolationError` and
/// `Occurence`/`Occurrence` the same `default_description`). The *wire spelling*
/// (`as_str`) still differs — only the human description is shared.
#[test]
fn paired_spellings_share_one_description() {
    assert_eq!(
        CallErrorCode::FormatViolation.default_description(),
        CallErrorCode::FormationViolation.default_description(),
    );
    assert_eq!(
        CallErrorCode::OccurenceConstraintViolation.default_description(),
        CallErrorCode::OccurrenceConstraintViolation.default_description(),
    );
    // …while remaining distinct codes on the wire.
    assert_ne!(
        CallErrorCode::FormatViolation.as_str(),
        CallErrorCode::FormationViolation.as_str(),
    );
    assert_ne!(
        CallErrorCode::OccurenceConstraintViolation.as_str(),
        CallErrorCode::OccurrenceConstraintViolation.as_str(),
    );
}

/// `TypeConstraintViolation` must carry the reference's curly quotes, not ASCII
/// `"` — a common transcription slip that would silently diverge on the wire.
#[test]
fn type_constraint_description_uses_curly_quotes() {
    let desc = CallErrorCode::TypeConstraintViolation.default_description();
    assert!(
        desc.contains('\u{201c}') && desc.contains('\u{201d}'),
        "expected curly quotes \u{201c}\u{201d} in: {desc}"
    );
    assert!(
        !desc.contains('"'),
        "must not contain ASCII double-quotes: {desc}"
    );
}

/// No description is empty (the reference gives every concrete subclass a
/// non-empty `default_description`; only the abstract `OCPPError` base is `""`).
#[test]
fn no_default_description_is_empty() {
    for (code, _) in REFERENCE_DEFAULT_DESCRIPTIONS {
        assert!(
            !code.default_description().is_empty(),
            "{code:?} has an empty default_description"
        );
    }
}

/// Guards the DRY refactor of `build_call_error` (Issue #379): the two
/// unrouted-action descriptions that `call_error_details.rs` pins on the wire
/// are now sourced from `default_description()`. Pinning the exact strings here
/// makes the coupling explicit — if either canonical string changes, both this
/// test and `call_error_details.rs` fail together, flagging a wire-visible change.
#[test]
fn unrouted_action_descriptions_are_the_canonical_defaults() {
    assert_eq!(
        CallErrorCode::NotImplemented.default_description(),
        "Request Action is recognized but not supported by the receiver",
    );
    assert_eq!(
        CallErrorCode::NotSupported.default_description(),
        "Requested Action is not known by receiver",
    );
}
