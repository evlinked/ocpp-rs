//! Property-based encode↔decode round-trip fixpoint — the **`pack` half** of the
//! mobilityhouse/ocpp reference's
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
//!         assert type(e) in [ ... ]
//! ```
//!
//! Sibling test [`decode_fuzz_robustness.rs`](./decode_fuzz_robustness.rs)
//! (Issue #406 / #408) ports the **unpack-safety** half: arbitrary bytes never
//! panic and only surface sanctioned protocol errors. This file ports the
//! complementary **round-trip fixpoint** half: any frame that successfully
//! decodes must survive a `decode → serialize → decode` cycle unchanged — the
//! observable analog of `pack ∘ unpack` being a fixpoint on well-formed frames.
//!
//! ## Why it matters
//!
//! The serialize and deserialize paths are written independently
//! ([`RawMessage`]'s `From<Message>` / `into_message` in
//! `crates/ocpp-types/src/message.rs`, driven by
//! [`MessageSerializer::{serialize_message, deserialize_message}`] in
//! `crates/ocpp-messages/src/serialization.rs`). Nothing but a test forces them
//! to agree. This sweep is what would catch an asymmetry between them — a field
//! that decodes but re-encodes to a different shape, an `error_code` whose
//! `as_str()` wire spelling does not decode back to the same [`CallErrorCode`]
//! variant, or a discriminator mismatch across the three arities.
//!
//! The real wire path is exercised end to end: `MessageSerializer` converts a
//! [`Message`] to the OCPP array form (`[2,…]` / `[3,…]` / `[4,…]`) via
//! [`RawMessage`], serializes that, and the decode direction parses the array
//! back through [`RawMessage::into_message`]. So this pins the *framing* fixpoint,
//! not merely `serde` symmetry on the in-memory struct.
//!
//! ## Two directions
//!
//! - **encode → decode** ([`serialize_then_decode_is_fixpoint`]): start from a
//!   constructed valid [`Message`] `m`, assert `deserialize(serialize(m)) == m`.
//! - **decode → encode** ([`decode_then_serialize_is_fixpoint`]): start from a
//!   valid array-form wire frame, decode it to `m`, then assert
//!   `deserialize(serialize(m)) == m`. Inputs that don't decode are skipped —
//!   their safety is already the subject of `decode_fuzz_robustness.rs`.
//!
//! ## Numbers: why the sweep uses integers, and floats are pinned by hand
//!
//! The framing layer moves the `serde_json::Value` payload through untouched, so
//! the *only* thing that can perturb a payload across a round-trip is
//! serde_json's own JSON text codec. For **integers** (`i64` / `u64`, including
//! `i64::MIN` and `u64::MAX`) that codec is exact, so they are swept
//! property-based below.
//!
//! For **`f64`** it is not. serde_json's default float formatter is not a
//! round-trip fixpoint: `from_str(to_string(f)) != f` for a large fraction of
//! doubles (measured ≈30% of arbitrary finite `f64`, ≈10% even within
//! `[-1e9, 1e9]`), and it is not even idempotent — a second `to_string`∘`from_str`
//! can drift another ULP. `to_string(-976129690.5457033)` emits
//! `"-976129690.5457033"`, which parses back to `-976129690.5457032` (1 ULP low).
//! This is a serde_json property — the OCPP framing under test never inspects the
//! number — so sweeping arbitrary floats here would assert a serde_json guarantee
//! that does not exist, not a framing one. Short, well-behaved decimals (`21.4`,
//! `-273.15`, `1e-9`) *do* round-trip, so representative OCPP fractional values
//! (meter readings, temperatures) are covered deterministically in
//! [`named_frames_round_trip_exactly`] instead. See Issue #411 for the broader
//! "should the wire codec guarantee float fidelity" question this surfaced.
//!
//! `proptest` is a workspace dev-dependency; the case count is kept at the
//! default 256 per property so CI stays fast.
//!
//! Part of **M8 — Conformance** (Issue #410). Test-only; no production code.

use ocpp_messages::serialization::MessageSerializer;
use ocpp_types::{
    CallErrorCode, CallErrorMessage, CallMessage, CallResultMessage, Message, MessageType,
};
use proptest::prelude::*;
use serde_json::Value;

/// Frames are decoded/encoded through the 1.6J rules; only the size limit
/// differs between versions and the bounded inputs here never approach it, so
/// the choice is immaterial to the fixpoint property.
const VERSION: &str = "1.6J";

/// The 12 spec-defined [`CallErrorCode`]s. A CALLERROR frame only decodes when
/// its `error_code` is one of these (an out-of-spec code is rejected by
/// [`RawMessage::into_message`] — that path is covered by
/// `decode_fuzz_robustness.rs`), so the round-trip strategy samples from exactly
/// this set to stay on the "valid frame" side of the boundary.
const SPEC_ERROR_CODES: [CallErrorCode; 12] = [
    CallErrorCode::NotImplemented,
    CallErrorCode::NotSupported,
    CallErrorCode::InternalError,
    CallErrorCode::ProtocolError,
    CallErrorCode::SecurityError,
    CallErrorCode::FormationViolation,
    CallErrorCode::FormatViolation,
    CallErrorCode::PropertyConstraintViolation,
    CallErrorCode::OccurenceConstraintViolation,
    CallErrorCode::OccurrenceConstraintViolation,
    CallErrorCode::TypeConstraintViolation,
    CallErrorCode::GenericError,
];

/// A recursive `serde_json::Value` strategy for payload / details fields.
///
/// Numbers are **integers only** (`i64` spanning negatives and `u64` reaching
/// past `i64::MAX`), which serde_json's text codec round-trips exactly. Floats
/// are deliberately excluded from the sweep — serde_json does not round-trip
/// arbitrary `f64` (see the module-level "Numbers" note) — and are pinned
/// separately with realistic values in [`named_frames_round_trip_exactly`]. The
/// rest of the leaves (null, bool, arbitrary UTF-8 strings) all round-trip
/// exactly, so any sweep failure points at the framing, not the codec.
fn arb_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| Value::Number(n.into())),
        any::<u64>().prop_map(|n| Value::Number(n.into())),
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

/// A valid [`Message`] of each of the three kinds, with arbitrary `unique_id` /
/// action / description strings and arbitrary `Value` payloads, and (for
/// CALLERROR) an `error_code` drawn from [`SPEC_ERROR_CODES`] so the frame is
/// always decodable. Details are set explicitly rather than defaulted, so the
/// constructed `m` is exactly what a decode of its own serialization yields.
fn arb_message() -> impl Strategy<Value = Message> {
    let call = (".*", ".*", arb_json()).prop_map(|(unique_id, action, payload)| {
        Message::Call(CallMessage {
            message_type: MessageType::Call,
            unique_id,
            action,
            payload,
        })
    });
    let call_result = (".*", arb_json()).prop_map(|(unique_id, payload)| {
        Message::CallResult(CallResultMessage {
            message_type: MessageType::CallResult,
            unique_id,
            payload,
        })
    });
    let call_error = (
        ".*",
        prop::sample::select(SPEC_ERROR_CODES.as_slice()),
        ".*",
        arb_json(),
    )
        .prop_map(
            |(unique_id, error_code, error_description, error_details)| {
                Message::CallError(CallErrorMessage {
                    message_type: MessageType::CallError,
                    unique_id,
                    error_code,
                    error_description,
                    error_details,
                })
            },
        );
    prop_oneof![call, call_result, call_error]
}

/// A valid **array-form** wire frame (as a `serde_json::Value::Array`) for each
/// kind, mirroring the reference's `[2,id,action,payload]` / `[3,id,payload]` /
/// `[4,id,code,desc,details]`. Used to exercise the decode→encode direction from
/// raw wire text rather than from a constructed `Message`.
fn arb_wire_frame() -> impl Strategy<Value = Value> {
    let s = || ".*".prop_map(Value::String);
    let code = prop::sample::select(SPEC_ERROR_CODES.as_slice())
        .prop_map(|c| Value::String(c.as_str().to_string()));
    prop_oneof![
        // CALL: [2, unique_id, action, payload]
        (s(), s(), arb_json()).prop_map(|(id, action, p)| Value::Array(vec![
            2.into(),
            id,
            action,
            p
        ])),
        // CALLRESULT: [3, unique_id, payload]
        (s(), arb_json()).prop_map(|(id, p)| Value::Array(vec![3.into(), id, p])),
        // CALLERROR: [4, unique_id, error_code, error_description, error_details]
        (s(), code, s(), arb_json()).prop_map(|(id, c, d, det)| Value::Array(vec![
            4.into(),
            id,
            c,
            d,
            det
        ])),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// encode → decode: a constructed valid frame survives `serialize` then
    /// `deserialize` unchanged. This is the literal `pack ∘ unpack == id`
    /// property for well-formed frames.
    #[test]
    fn serialize_then_decode_is_fixpoint(m in arb_message()) {
        let ser = MessageSerializer::new();
        let text = ser
            .serialize_message(&m, VERSION)
            .map_err(|e| TestCaseError::fail(format!("serialize of a valid frame failed: {e:?}")))?;
        let back = ser
            .deserialize_message(&text, VERSION)
            .map_err(|e| TestCaseError::fail(format!("decode of our own serialization failed: {e:?} (text: {text})")))?;
        prop_assert_eq!(back, m, "serialized text: {}", text);
    }

    /// decode → encode: any valid array-form wire frame that decodes must
    /// re-serialize and re-decode to the identical `Message`. Guards the
    /// direction where the *input* is untrusted wire text, not a value we built.
    #[test]
    fn decode_then_serialize_is_fixpoint(frame in arb_wire_frame()) {
        let text = serde_json::to_string(&frame).expect("serde_json::Value always serializes");
        let ser = MessageSerializer::new();
        // Only well-formed frames are in scope here; anything that fails to
        // decode is the concern of decode_fuzz_robustness.rs, so skip it.
        let Ok(m) = ser.deserialize_message(&text, VERSION) else {
            return Ok(());
        };
        let reser = ser
            .serialize_message(&m, VERSION)
            .map_err(|e| TestCaseError::fail(format!("re-serialize of a decoded frame failed: {e:?}")))?;
        let m2 = ser
            .deserialize_message(&reser, VERSION)
            .map_err(|e| TestCaseError::fail(format!("re-decode failed: {e:?} (text: {reser})")))?;
        prop_assert_eq!(m2, m, "original wire text: {}", text);
    }
}

/// Deterministic number-edge and per-kind coverage, pinned as plain assertions
/// so the specific values the round-trip must preserve — fractional floats,
/// integer extremes past the `i64`/`u64` boundary, empty and deeply-nested
/// payloads — are guaranteed exercised, not merely reached probabilistically by
/// the sweep above.
#[test]
fn named_frames_round_trip_exactly() {
    let ser = MessageSerializer::new();

    let cases: Vec<Message> = vec![
        // CALL with an empty payload.
        Message::Call(CallMessage::new("Heartbeat".to_string(), serde_json::json!({})).unwrap()),
        // CALL whose payload carries a fractional float — the value most prone
        // to precision drift across serialize→parse (ties into the f64 /
        // multipleOf handling pinned in schema_validation_v16.rs).
        Message::Call(
            CallMessage::new(
                "MeterValues".to_string(),
                serde_json::json!({ "value": 21.4, "sampledValue": [0.0, -273.15, 1e-9] }),
            )
            .unwrap(),
        ),
        // CALL whose payload carries integer extremes on both sides of the
        // i64/u64 boundary — u64::MAX does not fit i64 and must not silently
        // narrow or float-ify.
        Message::Call(
            CallMessage::new(
                "SetChargingProfile".to_string(),
                serde_json::json!({
                    "i64max": i64::MAX,
                    "i64min": i64::MIN,
                    "u64max": u64::MAX,
                    "zero": 0,
                }),
            )
            .unwrap(),
        ),
        // CALLRESULT with a nested payload.
        Message::CallResult(
            CallResultMessage::new(
                "res-1".to_string(),
                serde_json::json!({ "idTagInfo": { "status": "Accepted", "parentIdTag": "P1" } }),
            )
            .unwrap(),
        ),
        // (CALLERROR for every spec code is exercised in the loop below.)
    ];

    for m in &cases {
        assert_round_trips(&ser, m);
    }

    // Every spec error code must round-trip as a CALLERROR — this is the
    // as_str() ⇄ into_message() bijection viewed through the framing layer.
    for code in SPEC_ERROR_CODES {
        // `as_str()` yields `&'static str`, so capture the wire spelling before
        // `code` is moved into the frame.
        let spelling = code.as_str();
        let m = Message::CallError(CallErrorMessage::new(
            format!("err-{spelling}"),
            code,
            "something went wrong".to_string(),
            Some(serde_json::json!({ "retryAfter": 30, "detail": spelling })),
        ));
        assert_round_trips(&ser, &m);
    }
}

/// Assert `deserialize(serialize(m)) == m` through the real wire path, with a
/// message that names the frame on failure.
fn assert_round_trips(ser: &MessageSerializer, m: &Message) {
    let text = ser
        .serialize_message(m, VERSION)
        .unwrap_or_else(|e| panic!("serialize failed for {m:?}: {e:?}"));
    let back = ser
        .deserialize_message(&text, VERSION)
        .unwrap_or_else(|e| panic!("decode failed for {text}: {e:?}"));
    assert_eq!(&back, m, "frame did not round-trip; wire text: {text}");
}
