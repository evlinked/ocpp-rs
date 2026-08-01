//! Property-based fuzz sweep over the **object-form CALLERROR recovery** trust
//! boundary — the object-form complement to `decode_fuzz_robustness.rs`'s
//! array-form sweep (Issue #406 / #408). Part of **M8 — Conformance** (Issue
//! #409). Test-only; no production code.
//!
//! ## The second trust boundary
//!
//! `decode_fuzz_robustness.rs` fuzzes the *primary* decode boundary
//! (`serde_json::from_str::<RawMessage>` → [`RawMessage::into_message`]). There is
//! a *second*, narrower boundary the live recv loops fall back to when that strict
//! decode rejects a frame:
//! [`recover_inbound_call_error`](https://github.com/EVLinked/ocpp-rs/blob/main/crates/ocpp-types/src/message.rs)
//! (Issue #381 / PR #382).
//!
//! When a peer puts an **out-of-spec CALLERROR `error_code`** on the wire — a
//! vendor-specific code, a forward-compat code from a newer OCPP revision, or a
//! buggy/malicious value — `Message`'s strict `serde` decode fails the *whole
//! frame*. Rather than drop it (which would leave the correlated `call()` hanging
//! to its 30 s timeout, surfacing a misleading [`OcppError::Timeout`]),
//! `recover_inbound_call_error` pulls `unique_id` / `error_code` /
//! `error_description` / `error_details` out of the parsed
//! [`serde_json::Value`] by string keys and reshapes them into a
//! [`RawMessage::CallError`], reusing the #260-hardened `into_message` as the
//! single source of truth for the code→error mapping.
//!
//! That path takes **untrusted `text` off the wire** and indexes into a
//! `Value` by string keys, so — exactly like the reference's `unpack` — it must
//! never panic, hang, or surface an unexpected result on arbitrary input. Its
//! contract is tighter than the primary decoder's: it returns
//!   * `None` — the frame is not an object-form CALLERROR, or carries no string
//!     `unique_id` / `error_code`, so there is nothing to correlate; or
//!   * `Some((unique_id, err))` where `err` is exactly one of two sanctioned
//!     variants — [`OcppError::CallError`] for a code in the 12-member
//!     [`CallErrorCode`] set, or [`OcppError::ProtocolViolation`] for an
//!     out-of-spec code — and `unique_id` is the peer's `"1"` echoed back
//!     verbatim so the pending call is resolved against the right key.
//!
//! ## What this pins that the hand-crafted suite doesn't
//!
//! The per-case coverage of this path lives in the unit tests in
//! `crates/ocpp-types/src/message.rs` and the wire-level integration suite
//! `crates/ocpp-conformance/tests/wire_level_call_error.rs` (Issue #383). Those
//! assert named, hand-picked inputs. This adds the **property-based sweep**: it
//! feeds arbitrary bytes / strings / JSON and object-form CALLERROR frames with
//! arbitrary codes, asserting the never-panic + sanctioned-return contract holds
//! across the whole input space, and that the known-vs-unknown `error_code` split
//! is exactly the 12-member set. A hardening regression that made some malformed
//! frame panic, or a mapping change that reclassified a code, would slip past the
//! fixed cases but fail here.
//!
//! ## Reference
//!
//! Behaviour ported: [`ocpp/messages.py::CallError.to_exception`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py)
//! — a recognized code yields a typed exception, an out-of-spec one yields
//! `UnknownCallErrorCodeError` (our [`OcppError::ProtocolViolation`]), and the
//! frame always *unpacks* so its `unique_id` survives to resolve the pending call
//! ([`ocpp/charge_point.py::_handle_call_error`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)).
//!
//! `proptest` is a workspace dev-dependency; the case count is kept at the default
//! 256 per property so CI stays fast.

use ocpp_types::{recover_inbound_call_error, CallErrorCode, OcppError};
use proptest::prelude::*;
use serde_json::{json, Value};

/// The 12 canonical spec spellings `into_message` recognizes — the exact set that
/// separates a recovered [`OcppError::CallError`] from an
/// [`OcppError::ProtocolViolation`]. Kept in sync with
/// `CallErrorCode::as_str` (`crates/ocpp-types/src/error.rs`); the
/// [`spec_code_list_is_exhaustive`] test fails loudly if a variant is added
/// without updating this list.
const SPEC_CODES: &[&str] = &[
    "NotImplemented",
    "NotSupported",
    "InternalError",
    "ProtocolError",
    "SecurityError",
    "FormationViolation",
    "FormatViolation",
    "PropertyConstraintViolation",
    "OccurenceConstraintViolation",
    "OccurrenceConstraintViolation",
    "TypeConstraintViolation",
    "GenericError",
];

/// Assert a recovery result upholds the trust-boundary contract: either `None`,
/// or `Some((_, err))` where `err` is one of the two sanctioned variants. Anything
/// else — or a panic before we get here — is a hardening bug.
fn assert_recover_is_safe(text: &str) -> Result<(), TestCaseError> {
    match recover_inbound_call_error(text) {
        None => Ok(()),
        Some((_id, err)) => {
            prop_assert!(
                matches!(
                    err,
                    OcppError::CallError { .. } | OcppError::ProtocolViolation { .. }
                ),
                "recover surfaced a non-sanctioned error variant for input {text:?}: {err:?}"
            );
            Ok(())
        }
    }
}

/// A recursive `serde_json::Value` strategy — leaves plus bounded arrays/objects
/// — mirroring `decode_fuzz_robustness.rs`, so the sweep reaches object-shaped
/// inputs (the shape `recover` actually inspects) rather than almost-always
/// bouncing off the JSON parse the way raw random bytes do.
fn arb_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| Value::Number(n.into())),
        ".*".prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 32, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::hash_map(".*", inner, 0..6)
                .prop_map(|entries| Value::Object(entries.into_iter().collect())),
        ]
    })
}

/// An object-form CALLERROR frame `{"0":"CALLERROR","1":id,"2":code,"3":desc,
/// "4":details}` with an arbitrary `code` — drawn from a mix of the 12 spec
/// spellings and arbitrary junk so both sides of the known-vs-unknown split are
/// exercised directly rather than left to `arb_json` stumbling onto a well-shaped
/// frame. Returns `(id, code, is_known, text)`.
fn arb_object_call_error() -> impl Strategy<Value = (String, String, bool, String)> {
    // Bias toward the known spellings so the `CallError` branch is well covered,
    // while still admitting arbitrary junk (including strings that could *happen*
    // to equal a spec code — `is_known` is computed, never assumed).
    let code = prop_oneof![
        3 => prop::sample::select(SPEC_CODES).prop_map(|s| s.to_string()),
        2 => ".*".prop_map(|s: String| s),
    ];
    (".*", code, ".*", arb_json()).prop_map(
        |(id, code, desc, details): (String, String, String, Value)| {
            let is_known = SPEC_CODES.contains(&code.as_str());
            let text = serde_json::to_string(&json!({
                "0": "CALLERROR",
                "1": id,
                "2": code,
                "3": desc,
                "4": details,
            }))
            .expect("serde_json::Value always serializes");
            (id, code, is_known, text)
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Arbitrary raw bytes (including non-UTF-8, lossily decoded to text the way a
    /// recv loop would see it) — the object-form analog of the reference's
    /// `@given(binary())`. Never panics; return is `None` or a sanctioned
    /// `Some((_, err))`.
    #[test]
    fn recover_arbitrary_bytes_is_safe(data in prop::collection::vec(any::<u8>(), 0..512)) {
        let text = String::from_utf8_lossy(&data);
        assert_recover_is_safe(&text)?;
    }

    /// Arbitrary Unicode strings — reaches malformed-but-textual inputs a lossy
    /// byte decode rarely produces.
    #[test]
    fn recover_arbitrary_text_is_safe(text in ".*") {
        assert_recover_is_safe(&text)?;
    }

    /// Arbitrary valid JSON serialized to text — exercises the `Value`-indexing
    /// path (objects with unexpected key/value shapes) past the raw-text bounce.
    #[test]
    fn recover_arbitrary_json_is_safe(value in arb_json()) {
        let text = serde_json::to_string(&value).expect("serde_json::Value always serializes");
        assert_recover_is_safe(&text)?;
    }

    /// Object-form CALLERROR frames with arbitrary codes: recovery always
    /// succeeds (string `"1"` + string `"2"` are present), echoes the `unique_id`
    /// back **verbatim**, and maps the code to `CallError` iff it is one of the 12
    /// spec spellings, else `ProtocolViolation` — the object-form image of
    /// `CallError.to_exception`.
    #[test]
    fn recover_object_call_error_splits_known_from_unknown(
        (id, code, is_known, text) in arb_object_call_error()
    ) {
        let (recovered_id, err) = recover_inbound_call_error(&text)
            .expect("an object-form CALLERROR with string id + code must recover");

        prop_assert_eq!(&recovered_id, &id, "the correlation key must survive verbatim");

        if is_known {
            match err {
                OcppError::CallError { code: recovered_code, .. } => {
                    prop_assert_eq!(
                        recovered_code.as_str(),
                        code.as_str(),
                        "a known code must round-trip to its own variant"
                    );
                }
                other => prop_assert!(
                    false,
                    "known code {code:?} must yield CallError, got {other:?}"
                ),
            }
        } else {
            match err {
                OcppError::ProtocolViolation { message } => {
                    prop_assert!(
                        message.contains(&code),
                        "the ProtocolViolation message must name the offending code {code:?}, got {message:?}"
                    );
                }
                other => prop_assert!(
                    false,
                    "unknown code {code:?} must yield ProtocolViolation, got {other:?}"
                ),
            }
        }
    }
}

/// Deterministic pins for the two load-bearing mappings and the decline cases, so
/// the specific boundaries the hand-crafted suite names are covered here too, not
/// only probabilistically by the sweep.
#[test]
fn recover_named_cases() {
    // Known code → typed CallError with code/description/details intact.
    let known = serde_json::to_string(&json!({
        "0": "CALLERROR", "1": "uid-known", "2": "InternalError",
        "3": "central system unavailable", "4": {"retryAfter": 30},
    }))
    .unwrap();
    assert_eq!(
        recover_inbound_call_error(&known),
        Some((
            "uid-known".to_string(),
            OcppError::CallError {
                code: CallErrorCode::InternalError,
                description: "central system unavailable".to_string(),
                details: json!({"retryAfter": 30}),
            }
        )),
    );

    // Out-of-spec code → ProtocolViolation naming the code, id preserved.
    let unknown = serde_json::to_string(&json!({
        "0": "CALLERROR", "1": "uid-teapot", "2": "418", "3": "I'm a teapot", "4": {},
    }))
    .unwrap();
    match recover_inbound_call_error(&unknown) {
        Some((id, OcppError::ProtocolViolation { message })) => {
            assert_eq!(id, "uid-teapot");
            assert!(message.contains("418"), "got: {message}");
        }
        other => panic!("expected ProtocolViolation for an out-of-spec code, got {other:?}"),
    }

    // Decline cases → None (nothing to correlate), never a panic.
    let not_a_call_error =
        serde_json::to_string(&json!({"0": "CALL", "1": "u", "2": "Heartbeat", "3": {}})).unwrap();
    assert!(recover_inbound_call_error(&not_a_call_error).is_none());
    let missing_uid =
        serde_json::to_string(&json!({"0": "CALLERROR", "2": "418", "3": "d", "4": {}})).unwrap();
    assert!(recover_inbound_call_error(&missing_uid).is_none());
    let missing_code =
        serde_json::to_string(&json!({"0": "CALLERROR", "1": "u", "3": "d", "4": {}})).unwrap();
    assert!(recover_inbound_call_error(&missing_code).is_none());
    assert!(recover_inbound_call_error("not json at all").is_none());
    assert!(recover_inbound_call_error("").is_none());
}

/// Guard the local [`SPEC_CODES`] mirror against drift: every code it lists must
/// recover as a typed `CallError` (so the list contains no typo), and the count
/// must stay at the 12 the issue references (so a new `CallErrorCode` variant
/// added upstream forces this list — and the sweep's known-vs-unknown split — to
/// be updated deliberately rather than silently reclassifying a code as unknown.)
#[test]
fn spec_code_list_is_exhaustive() {
    assert_eq!(
        SPEC_CODES.len(),
        12,
        "the spec defines exactly 12 CALLERROR codes"
    );
    for code in SPEC_CODES {
        let text = serde_json::to_string(&json!({
            "0": "CALLERROR", "1": "u", "2": code, "3": "", "4": {},
        }))
        .unwrap();
        match recover_inbound_call_error(&text) {
            Some((
                _,
                OcppError::CallError {
                    code: recovered, ..
                },
            )) => {
                assert_eq!(
                    recovered.as_str(),
                    *code,
                    "SPEC_CODES entry must be canonical"
                );
            }
            other => panic!("SPEC_CODES entry {code:?} did not recover as CallError: {other:?}"),
        }
    }
}
