//! Property-based decode-robustness sweep — ports the mobilityhouse/ocpp
//! reference's
//! [`tests/test_messages.py::test_unpack_and_pack`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py).
//!
//! ## What the reference pins
//!
//! ```python
//! @given(binary())
//! def test_unpack_and_pack(data):
//!     try:
//!         assert unpack(data) == pack(data)
//!     except Exception as e:
//!         assert type(e) in [
//!             FormatViolationError,
//!             ProtocolError,
//!             PropertyConstraintViolationError,
//!         ]
//! ```
//!
//! The point is a **trust-boundary guarantee**, not round-trip equality
//! (`pack` there is a no-op identity on the raw text): decoding
//! attacker-controlled bytes off the wire must never panic, hang, or surface an
//! *unexpected* error kind. It either yields a well-formed frame or exactly one
//! of a small, sanctioned set of protocol errors. Everything the CSMS and CP
//! recv loops feed the decoder is untrusted, so this is the property that keeps
//! a malformed or malicious frame from taking a task down.
//!
//! ## The Rust analog
//!
//! The faithful analog of `unpack()` is the
//! [`MessageSerializer::deserialize_message`] path
//! (`crates/ocpp-messages/src/serialization.rs`): `serde_json::from_str::<RawMessage>`
//! → [`RawMessage::into_message`] (`crates/ocpp-types/src/message.rs`). Python's
//! `json.loads` accepts `bytes` and raises `UnicodeDecodeError` on non-UTF-8;
//! our decoder takes `&str`, so [`decode`] first does the UTF-8 boundary check
//! and maps a failure to the same class of error the reference does.
//!
//! The reference's three sanctioned exception types map onto `OcppError` as:
//!
//! | Python (`ocpp.exceptions`)          | `unpack` trigger                                  | Rust (`OcppError`)          |
//! |-------------------------------------|---------------------------------------------------|-----------------------------|
//! | `FormatViolationError`              | not valid JSON / not valid UTF-8                  | `Json`                      |
//! | `ProtocolError`                     | JSON isn't a list / missing / too few elements    | `Json` or `ProtocolViolation` |
//! | `PropertyConstraintViolationError`  | `msg[0]` (MessageTypeId) is not 2/3/4             | `InvalidMessageType`        |
//!
//! Two Rust-side wrinkles vs. the Python control flow:
//!   * The reference distinguishes "not a list" (`ProtocolError`) from "bad
//!     JSON" (`FormatViolationError`) after a successful parse. Our `RawMessage`
//!     is an `#[serde(untagged)]` enum decoded in one shot, so a value that is
//!     valid JSON but not a well-shaped OCPP array fails inside `from_str` and
//!     surfaces as `Json` rather than a separate post-parse `ProtocolViolation`.
//!     Both are sanctioned, so the guarantee holds either way.
//!   * `into_message` also rejects an unknown CALLERROR `error_code` with
//!     `ProtocolViolation` ("Unknown error code: …") — an extra sanctioned exit
//!     the Python `unpack` doesn't reach (it defers code validation to
//!     `to_exception`). Still within the sanctioned set.
//!
//! [`OcppError::ValidationError`] (the size guard) is included defensively: the
//! bounded inputs here never approach the 64 KiB limit, so it should not fire,
//! but admitting it keeps the assertion about *which* errors are acceptable
//! rather than silently depending on input size — it is still emphatically not
//! a panic and not an unexpected semantic variant (`CallError`, `Timeout`,
//! `Internal`, `NotImplemented`, …).
//!
//! `proptest` is a workspace dev-dependency; the case count is kept at the
//! default 256 per property so CI stays fast.
//!
//! Part of **M8 — Conformance** (Issue #406). Test-only; no production code.

use ocpp_messages::serialization::MessageSerializer;
use ocpp_types::{Message, OcppError};
use proptest::prelude::*;
use serde_json::Value;

/// Faithful analog of the reference `unpack()`: decode arbitrary wire bytes into
/// a typed [`Message`], or one of the sanctioned protocol errors.
///
/// Python's `json.loads(bytes)` handles the bytes→text step and raises
/// `UnicodeDecodeError` (a `FormatViolationError`) on invalid UTF-8. Our decoder
/// only accepts `&str`, so we mirror that boundary here and map a non-UTF-8
/// input to [`OcppError::Json`] — the same `FormatViolationError` class.
fn decode(bytes: &[u8]) -> Result<Message, OcppError> {
    let text = std::str::from_utf8(bytes).map_err(|e| OcppError::Json {
        message: format!("invalid UTF-8: {e}"),
    })?;
    // Version only selects the size limit; the decode semantics are identical.
    MessageSerializer::new().deserialize_message(text, "1.6J")
}

/// The closed set of error variants the decode boundary is allowed to surface.
/// Anything else — or a panic — is a hardening bug.
fn is_sanctioned(err: &OcppError) -> bool {
    matches!(
        err,
        // `FormatViolationError`: unparseable JSON, non-UTF-8, or a valid JSON
        // value that isn't a well-shaped OCPP frame array.
        OcppError::Json { .. }
            // `PropertyConstraintViolationError`: array is frame-shaped but its
            // MessageTypeId discriminator is not 2/3/4.
            | OcppError::InvalidMessageType(_)
            // `ProtocolError`: structurally-valid-but-wrong frame, or an unknown
            // CALLERROR error_code.
            | OcppError::ProtocolViolation { .. }
            // Size guard — see module docs; not reachable for these bounded
            // inputs, admitted defensively.
            | OcppError::ValidationError { .. }
    )
}

/// Assert the decode of `bytes` upholds the trust-boundary guarantee: it either
/// decodes to a `Message` or fails with a sanctioned variant — never a panic,
/// never an unexpected variant.
fn assert_decode_is_safe(bytes: &[u8]) -> Result<(), TestCaseError> {
    match decode(bytes) {
        Ok(_) => Ok(()),
        Err(e) => {
            prop_assert!(
                is_sanctioned(&e),
                "decode surfaced a non-sanctioned error variant for input {:?}: {:?}",
                bytes,
                e
            );
            Ok(())
        }
    }
}

/// A recursive `serde_json::Value` strategy — leaves plus bounded arrays/objects
/// — so the sweep exercises the structural-validation path (frame-shaped arrays,
/// object-form frames, nesting) beyond what raw random bytes reach, which are
/// almost always rejected at the JSON-parse step.
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

/// Frame-shaped arrays with an arbitrary leading discriminator and per-arity
/// element types, so the `RawMessage::into_message` branches (including the
/// `InvalidMessageType` and unknown-`error_code` exits) are hit directly rather
/// than relying on `arb_json` to stumble onto a well-typed frame.
fn arb_frame_shaped() -> impl Strategy<Value = Value> {
    let discr = any::<u8>().prop_map(|n| Value::Number(n.into()));
    let text = ".*".prop_map(Value::String);
    prop_oneof![
        // CALL arity: [discr, unique_id, action, payload]
        (discr.clone(), text.clone(), text.clone(), arb_json())
            .prop_map(|(d, id, a, p)| { Value::Array(vec![d, id, a, p]) }),
        // CALLRESULT arity: [discr, unique_id, payload]
        (discr.clone(), text.clone(), arb_json())
            .prop_map(|(d, id, p)| Value::Array(vec![d, id, p])),
        // CALLERROR arity: [discr, unique_id, error_code, error_description, error_details]
        (discr, text.clone(), text.clone(), text, arb_json()).prop_map(
            |(d, id, code, desc, details)| { Value::Array(vec![d, id, code, desc, details]) }
        ),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Arbitrary raw bytes (including non-UTF-8) — the literal port of the
    /// reference's `@given(binary())`.
    #[test]
    fn decode_arbitrary_bytes_is_safe(data in prop::collection::vec(any::<u8>(), 0..512)) {
        assert_decode_is_safe(&data)?;
    }

    /// Arbitrary valid-UTF-8 JSON values serialized to text — exercises the
    /// structural-validation path past the raw-bytes JSON-parse rejection.
    #[test]
    fn decode_arbitrary_json_is_safe(value in arb_json()) {
        let text = serde_json::to_string(&value).expect("serde_json::Value always serializes");
        assert_decode_is_safe(text.as_bytes())?;
    }

    /// Frame-shaped arrays with arbitrary discriminators — drives the
    /// `into_message` branch logic directly.
    #[test]
    fn decode_frame_shaped_is_safe(frame in arb_frame_shaped()) {
        let text = serde_json::to_string(&frame).expect("serde_json::Value always serializes");
        assert_decode_is_safe(text.as_bytes())?;
    }
}

/// A handful of hand-picked adversarial inputs, pinned as plain unit assertions
/// so the specific boundary cases the reference's sibling tests name
/// (`test_unpack_with_invalid_json`, `test_unpack_without_jsonified_list`,
/// `test_unpack_without_message_type_id_in_json`,
/// `test_unpack_with_invalid_message_type_id_in_json`) are covered
/// deterministically, not just probabilistically by the sweep.
#[test]
fn decode_named_adversarial_inputs_are_safe() {
    let cases: &[&[u8]] = &[
        b"",                                     // empty
        b"\x01",                                 // invalid JSON (reference: FormatViolationError)
        &[0xff, 0xfe, 0xfd],                     // invalid UTF-8
        b"\"3\"",                     // valid JSON, not a list (reference: ProtocolError)
        b"[]",                        // list without MessageTypeId (reference: ProtocolError)
        b"[5, \"1\", \"Reset\", {}]", // bad MessageTypeId (reference: PropertyConstraintViolationError)
        b"[2]",                       // too few elements
        b"[2, \"1\"]",                // too few elements
        b"[4, \"1\", \"BogusCode\", \"d\", {}]", // unknown CALLERROR error_code
        b"{\"not\": \"a frame\"}",    // JSON object, not a frame array
        b"null",
        b"12345",
        b"[[[[[[[[[]]]]]]]]]", // deep nesting
    ];
    for input in cases {
        match decode(input) {
            Ok(_) => {}
            Err(e) => assert!(
                is_sanctioned(&e),
                "named input {input:?} surfaced a non-sanctioned error: {e:?}"
            ),
        }
    }

    // Spot-check the two most load-bearing mappings resolve to the expected
    // sanctioned variant, not merely *a* sanctioned variant.
    assert!(
        matches!(decode(b"\x01"), Err(OcppError::Json { .. })),
        "invalid JSON must map to the FormatViolationError analog (Json)"
    );
    assert!(
        matches!(
            decode(b"[5, \"1\", \"Reset\", {}]"),
            Err(OcppError::InvalidMessageType(5))
        ),
        "bad MessageTypeId must map to the PropertyConstraintViolationError analog (InvalidMessageType)"
    );
    assert!(
        matches!(
            decode(b"[4, \"1\", \"BogusCode\", \"d\", {}]"),
            Err(OcppError::ProtocolViolation { .. })
        ),
        "unknown CALLERROR error_code must map to the ProtocolError analog (ProtocolViolation)"
    );
}
