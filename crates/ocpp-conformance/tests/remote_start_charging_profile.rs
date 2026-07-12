//! Nested `ChargingProfile` round-trip inside `RemoteStartTransaction` (Issue #306).
//!
//! Rust port of the reference
//! [`tests/v16/test_v16_charging_profiles.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v16/test_v16_charging_profiles.py),
//! which pins that a fully-populated `ChargingProfile` — a nested
//! `ChargingSchedule` carrying `ChargingSchedulePeriod`s, plus `transaction_id`,
//! `recurrency_kind`, `valid_from`, `valid_to` — embedded in a
//! `RemoteStartTransaction` survives a `dict → json → dict` round-trip with
//! every nested field intact.
//!
//! The reference has two cases, `test_remote_start_transaction_as_dict` and
//! `..._as_class`, differing only in whether the nested profile is passed as a
//! `dataclass` or a pre-`asdict`-ed dict. On the Rust side the model is *always*
//! strongly typed (`RemoteStartTransactionRequest.charging_profile:
//! Option<ChargingProfile>`), so that Python distinction collapses: there is a
//! single typed round-trip. What still needs pinning — and what the existing
//! `charging_profile.rs` (which only drives `SetChargingProfile` /
//! `ClearChargingProfile` *dispatch*) never covers — is that the nested tree
//! serializes to the exact OCPP-J wire keys and survives serialize→deserialize.
//! A dropped `skip_serializing_if`, a wrong `#[serde(rename)]` on `numberPhases`
//! / `startPeriod` / `chargingRateUnit` / `validFrom`, or a lost optional would
//! silently corrupt a remote-start-with-profile command; these tests guard it.
//!
//! Part of **M8 — Conformance**. Test-only; no production code.

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::v16j::RemoteStartTransactionRequest;
use ocpp_types::v16j::{
    ChargingProfile, ChargingProfileKindType, ChargingProfilePurposeType, ChargingRateUnitType,
    ChargingSchedule, ChargingSchedulePeriod, RecurrencyKindType,
};
use serde_json::Value;

/// The reference fixture, expressed once as a typed Rust value. Mirrors
/// `test_v16_charging_profiles.py`: profile 1 / stack 1, a
/// `ChargePointMaxProfile` `Absolute` profile, a `W`-unit schedule with a single
/// period (`start_period: 0`, `limit: 10`, `number_phases: 3`), a
/// `TxProfile`-style `transaction_id`, `Daily` recurrence, and a
/// `validFrom`/`validTo` window.
fn reference_remote_start() -> RemoteStartTransactionRequest {
    RemoteStartTransactionRequest {
        connector_id: Some(1),
        id_tag: "12345".to_string(),
        charging_profile: Some(ChargingProfile {
            charging_profile_id: 1,
            transaction_id: Some(1),
            stack_level: 1,
            charging_profile_purpose: ChargingProfilePurposeType::ChargePointMaxProfile,
            charging_profile_kind: ChargingProfileKindType::Absolute,
            recurrency_kind: Some(RecurrencyKindType::Daily),
            valid_from: Some(
                "2021-01-01T00:00:00Z"
                    .parse()
                    .expect("valid_from is RFC3339"),
            ),
            valid_to: Some("2021-01-02T00:00:00Z".parse().expect("valid_to is RFC3339")),
            charging_schedule: ChargingSchedule {
                duration: None,
                start_schedule: None,
                charging_rate_unit: ChargingRateUnitType::W,
                charging_schedule_period: vec![ChargingSchedulePeriod {
                    start_period: 0,
                    limit: 10.0,
                    number_phases: Some(3),
                }],
                min_charging_rate: None,
            },
        }),
    }
}

/// The Rust analog of the reference's `to_datatype(...)` round-trip: serialize
/// to JSON text, deserialize back, and assert the *entire* nested tree is
/// preserved. `assert_eq!` on the whole struct is strictly stronger than the
/// reference's field-by-field walk (it would also catch a *spurious* extra
/// field), and the explicit per-field asserts below mirror the reference so a
/// regression names the offending field.
#[test]
fn nested_charging_profile_round_trips() {
    let original = reference_remote_start();

    let json = serde_json::to_string(&original).expect("serialize RemoteStartTransaction");
    let round_tripped: RemoteStartTransactionRequest =
        serde_json::from_str(&json).expect("deserialize RemoteStartTransaction");

    // Whole-tree equality — the strongest possible round-trip assertion.
    assert_eq!(
        original, round_tripped,
        "nested ChargingProfile tree must survive serialize→deserialize intact"
    );

    // Field-by-field, mirroring the reference's granular asserts so a failure
    // pinpoints the corrupted field rather than just "structs differ".
    assert_eq!(original.id_tag, round_tripped.id_tag);
    assert_eq!(original.connector_id, round_tripped.connector_id);

    let orig_profile = original.charging_profile.as_ref().unwrap();
    let new_profile = round_tripped.charging_profile.as_ref().unwrap();
    assert_eq!(
        orig_profile.charging_profile_id,
        new_profile.charging_profile_id
    );
    assert_eq!(orig_profile.stack_level, new_profile.stack_level);
    assert_eq!(
        orig_profile.charging_profile_purpose,
        new_profile.charging_profile_purpose
    );
    assert_eq!(
        orig_profile.charging_profile_kind,
        new_profile.charging_profile_kind
    );
    assert_eq!(orig_profile.transaction_id, new_profile.transaction_id);
    assert_eq!(orig_profile.recurrency_kind, new_profile.recurrency_kind);
    assert_eq!(orig_profile.valid_from, new_profile.valid_from);
    assert_eq!(orig_profile.valid_to, new_profile.valid_to);

    let orig_schedule = &orig_profile.charging_schedule;
    let new_schedule = &new_profile.charging_schedule;
    assert_eq!(
        orig_schedule.charging_rate_unit,
        new_schedule.charging_rate_unit
    );

    let orig_period = &orig_schedule.charging_schedule_period[0];
    let new_period = &new_schedule.charging_schedule_period[0];
    assert_eq!(orig_period.start_period, new_period.start_period);
    assert_eq!(orig_period.limit, new_period.limit);
    assert_eq!(orig_period.number_phases, new_period.number_phases);
}

/// Pin the exact OCPP-J *wire key names* and serialized enum/timestamp values —
/// not just that the Rust field values survive. The round-trip test above can't
/// catch a symmetric rename bug (both serialize and deserialize agreeing on the
/// wrong key), so this test asserts the JSON shape directly against the key
/// names in `crates/ocpp-messages/schemas/v16j/RemoteStartTransaction.json`.
#[test]
fn nested_charging_profile_wire_keys_are_pinned() {
    let value: Value =
        serde_json::to_value(reference_remote_start()).expect("serialize to serde_json::Value");

    // Top-level RemoteStartTransaction keys.
    assert_eq!(value["idTag"], "12345");
    assert_eq!(value["connectorId"], 1);

    let profile = &value["chargingProfile"];
    assert!(profile.is_object(), "chargingProfile must be an object");
    assert_eq!(profile["chargingProfileId"], 1);
    assert_eq!(profile["transactionId"], 1);
    assert_eq!(profile["stackLevel"], 1);
    assert_eq!(profile["chargingProfilePurpose"], "ChargePointMaxProfile");
    assert_eq!(profile["chargingProfileKind"], "Absolute");
    assert_eq!(profile["recurrencyKind"], "Daily");
    assert_eq!(profile["validFrom"], "2021-01-01T00:00:00Z");
    assert_eq!(profile["validTo"], "2021-01-02T00:00:00Z");

    let schedule = &profile["chargingSchedule"];
    assert!(schedule.is_object(), "chargingSchedule must be an object");
    assert_eq!(schedule["chargingRateUnit"], "W");

    let periods = schedule["chargingSchedulePeriod"]
        .as_array()
        .expect("chargingSchedulePeriod must be an array");
    assert_eq!(periods.len(), 1);
    let period = &periods[0];
    assert_eq!(period["startPeriod"], 0);
    assert_eq!(period["limit"], 10.0);
    assert_eq!(period["numberPhases"], 3);
}

/// Cross-check that the produced JSON validates against the bundled FINAL
/// `RemoteStartTransaction.json` — the schema-validation discipline used across
/// the M8 sweep. Drives the public `SchemaValidator::v16j()` boundary, the same
/// validator that guards real incoming CALLs.
#[test]
fn remote_start_with_profile_is_schema_valid() {
    let value: Value =
        serde_json::to_value(reference_remote_start()).expect("serialize to serde_json::Value");

    SchemaValidator::v16j()
        .validate_call("RemoteStartTransaction", &value)
        .expect("a fully-populated RemoteStartTransaction must be schema-valid");
}

/// Guard the `skip_serializing_if = "Option::is_none"` on the optional
/// `connectorId` and `chargingProfile`: a minimal remote-start must serialize to
/// exactly `{"idTag": ...}` (no `null`s leaking onto the wire) and still
/// validate — `idTag` is the schema's only required field. This is the negative
/// counterpart to the fully-populated cases and pins that the optional profile
/// really is optional.
#[test]
fn remote_start_without_profile_omits_optionals() {
    let minimal = RemoteStartTransactionRequest {
        connector_id: None,
        id_tag: "12345".to_string(),
        charging_profile: None,
    };

    let value: Value = serde_json::to_value(&minimal).expect("serialize minimal RemoteStart");

    assert_eq!(value["idTag"], "12345");
    assert!(
        value.get("connectorId").is_none(),
        "None connectorId must be omitted, not serialized as null"
    );
    assert!(
        value.get("chargingProfile").is_none(),
        "None chargingProfile must be omitted, not serialized as null"
    );
    assert_eq!(
        value.as_object().unwrap().len(),
        1,
        "minimal payload must carry only idTag"
    );

    SchemaValidator::v16j()
        .validate_call("RemoteStartTransaction", &value)
        .expect("a minimal RemoteStartTransaction (idTag only) must be schema-valid");

    // And it still round-trips.
    let round_tripped: RemoteStartTransactionRequest =
        serde_json::from_value(value).expect("deserialize minimal RemoteStart");
    assert_eq!(minimal, round_tripped);
}
