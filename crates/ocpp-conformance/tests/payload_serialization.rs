//! Payload-serialization conformance suite — pins the serde analog of the
//! mobilityhouse/ocpp reference's `remove_nones` / `serialize_as_dict` and the
//! snake↔camel key helpers from
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py),
//! as exercised by
//! [`tests/test_charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point.py).
//!
//! The reference builds each outgoing CALL/CALLRESULT payload from a dataclass,
//! then runs `remove_nones(asdict(payload))` so that **unset optional fields are
//! dropped from the wire entirely** — never serialized as `null` — recursively
//! through nested objects and lists of objects. Field names are converted
//! snake_case → camelCase via `snake_to_camel_case()`. ocpp-rs gets the exact
//! same wire shape from serde: `#[serde(rename = "…")]` (or `rename_all`) for
//! the key spelling, and `#[serde(skip_serializing_if = "Option::is_none")]` for
//! the omission.
//!
//! That omission invariant is load-bearing for interop — a struct that emitted
//! an unset optional as `"field": null` would diverge from the reference and can
//! fail a conformant peer's `additionalProperties: false` / typed-optional
//! schema — but nothing pinned it: the existing suites round-trip *present*
//! values; none assert *absent* optionals are omitted, including through nesting
//! and lists. This file is that guard. Nothing here would catch a regression a
//! human review of a serde attribute change wouldn't, but a machine can run it
//! on every commit.
//!
//! Each `#[test]` names the `test_charge_point.py` function it ports. Every
//! omission case additionally cross-checks that the emitted JSON validates
//! against the bundled schema via [`SchemaValidator`] — an absent optional is
//! schema-legal, a `null` may not be — so the suite pins *schema-valid*
//! omission, not merely omission.
//!
//! Test-only; no production code. Part of **M8 — Conformance** (Issue #291).

use ocpp_messages::schema_validation::SchemaValidator;
use ocpp_messages::v16j::{BootNotificationRequest, GetConfigurationRequest, MeterValuesRequest};
use ocpp_messages::v201::SetNetworkProfileRequest;
use ocpp_types::common::{MeterValue, SampledValue};
use ocpp_types::v201::{
    APNAuthenticationEnumType, APNType, NetworkConnectionProfileType, OCPPInterfaceEnumType,
    OCPPTransportEnumType, OCPPVersionEnumType,
};
use ocpp_types::{DateTime, Utc};
use serde_json::{Map, Value};

/// Serialize `value` to a JSON object, panicking if it does not serialize to a
/// map (every OCPP payload does).
fn to_object<T: serde::Serialize>(value: &T) -> Map<String, Value> {
    match serde_json::to_value(value).expect("payload serializes") {
        Value::Object(map) => map,
        other => panic!("expected a JSON object, got {other}"),
    }
}

/// Assert `map` has exactly `expected` as its key set and carries no `null`
/// value — the Rust analog of asserting `remove_nones` dropped every unset
/// optional (rather than emitting it as `null`) and kept every set field.
fn assert_exact_keys(map: &Map<String, Value>, expected: &[&str]) {
    let mut got: Vec<&str> = map.keys().map(String::as_str).collect();
    got.sort_unstable();
    let mut want: Vec<&str> = expected.to_vec();
    want.sort_unstable();
    assert_eq!(got, want, "unexpected key set");
    for (key, value) in map {
        assert!(
            !value.is_null(),
            "field `{key}` serialized as null; an unset optional must be absent, not null",
        );
    }
}

/// Ports `test_charge_point.py::test_remove_nones` — a payload with unset
/// optionals serializes to an object containing **only** the set fields; each
/// omitted key is absent, not `null`. Uses a 1.6J `BootNotification` with only
/// its two required fields set (`chargePointSerialNumber` etc. = `None`).
#[test]
fn flat_unset_optionals_are_omitted_not_null() {
    let boot = BootNotificationRequest {
        charge_point_vendor: "VendorX".into(),
        charge_point_model: "ModelY".into(),
        charge_point_serial_number: None,
        charge_box_serial_number: None,
        firmware_version: None,
        iccid: None,
        imsi: None,
        meter_type: None,
        meter_serial_number: None,
    };

    let map = to_object(&boot);
    assert_exact_keys(&map, &["chargePointVendor", "chargePointModel"]);

    SchemaValidator::v16j()
        .validate_call("BootNotification", &Value::Object(map))
        .expect("BootNotification with omitted optionals must be schema-valid");
}

/// Ports `test_charge_point.py::test_nested_remove_nones` — optionals unset
/// inside a nested object are omitted at the nested level too. Uses a 2.0.1
/// `SetNetworkProfile` whose `NetworkConnectionProfileType` has no `vpn` block
/// and carries an `APNType` with all of its own optionals unset, exercising
/// omission one *and* two levels deep.
#[test]
fn nested_unset_optionals_are_omitted() {
    let req = SetNetworkProfileRequest {
        configuration_slot: 1,
        connection_data: NetworkConnectionProfileType {
            ocpp_version: OCPPVersionEnumType::Ocpp20,
            ocpp_transport: OCPPTransportEnumType::Json,
            ocpp_csms_url: "wss://csms.example/ocpp".into(),
            message_timeout: 30,
            security_profile: 2,
            ocpp_interface: OCPPInterfaceEnumType::Wired0,
            apn: Some(APNType {
                apn: "internet".into(),
                apn_user_name: None,
                apn_password: None,
                sim_pin: None,
                preferred_network: None,
                use_only_preferred_network: None,
                apn_authentication: APNAuthenticationEnumType::Pap,
                custom_data: None,
            }),
            vpn: None,
            custom_data: None,
        },
        custom_data: None,
    };

    let map = to_object(&req);
    assert_exact_keys(&map, &["configurationSlot", "connectionData"]);

    let conn = map["connectionData"]
        .as_object()
        .expect("connectionData is an object");
    assert!(conn.contains_key("apn"), "set apn block must be present");
    assert!(!conn.contains_key("vpn"), "unset vpn block must be omitted");
    assert!(
        !conn.contains_key("customData"),
        "unset customData must be omitted at the nested level",
    );

    // Nested-within-nested: the APN's own unset optionals are dropped too.
    let apn = conn["apn"].as_object().expect("apn is an object");
    assert_exact_keys(apn, &["apn", "apnAuthentication"]);

    SchemaValidator::v201()
        .validate_call("SetNetworkProfile", &Value::Object(map))
        .expect("SetNetworkProfile with omitted nested optionals must be schema-valid");
}

/// Ports `test_charge_point.py::test_nested_list_remove_nones` (and
/// `test_serialization_of_collection_of_multiple_elements`) — optionals unset
/// inside objects that live *within a list* are omitted per element, across a
/// collection of more than one element. Uses a 1.6J `MeterValues` with two
/// `meterValue` entries, each holding `sampledValue` readings whose optionals
/// are all unset (only `value` remains).
#[test]
fn list_of_nested_unset_optionals_omitted_per_element() {
    let ts: DateTime<Utc> = "2027-01-01T00:00:00Z".parse().unwrap();
    let sample = |v: &str| SampledValue {
        value: v.into(),
        context: None,
        format: None,
        measurand: None,
        phase: None,
        location: None,
        unit: None,
    };

    let req = MeterValuesRequest {
        connector_id: 1,
        transaction_id: None,
        meter_values: vec![
            MeterValue {
                timestamp: ts,
                sampled_values: vec![sample("100"), sample("200")],
            },
            MeterValue {
                timestamp: ts,
                sampled_values: vec![sample("300")],
            },
        ],
    };

    let map = to_object(&req);
    // The unset `transactionId` optional is omitted, not null.
    assert_exact_keys(&map, &["connectorId", "meterValue"]);

    let meter_values = map["meterValue"]
        .as_array()
        .expect("meterValue is an array");
    assert_eq!(meter_values.len(), 2, "both meterValue elements serialized");
    for mv in meter_values {
        let samples = mv["sampledValue"]
            .as_array()
            .expect("sampledValue is an array");
        for sv in samples {
            let sv = sv.as_object().expect("sampledValue element is an object");
            // Each reading carries only `value`; every unset optional is gone.
            assert_exact_keys(sv, &["value"]);
        }
    }

    SchemaValidator::v16j()
        .validate_call("MeterValues", &Value::Object(map))
        .expect("MeterValues with per-element omitted optionals must be schema-valid");
}

/// Ports `test_charge_point.py::test_remove_nones_with_list_of_strings` —
/// `remove_nones` must leave a list of primitive strings untouched, while an
/// unset optional list is omitted wholesale (not emitted as `null` or `[]`).
/// Uses the 1.6J `GetConfiguration` `key` field (`Option<Vec<String>>`).
#[test]
fn list_of_strings_is_preserved_and_omitted_when_unset() {
    // Present list of strings survives verbatim.
    let with_keys = GetConfigurationRequest {
        key: Some(vec![
            "MeterValueSampleInterval".into(),
            "HeartbeatInterval".into(),
        ]),
    };
    let map = to_object(&with_keys);
    assert_exact_keys(&map, &["key"]);
    let got: Vec<&str> = map["key"]
        .as_array()
        .expect("key is an array")
        .iter()
        .map(|v| v.as_str().expect("string element"))
        .collect();
    assert_eq!(
        got,
        ["MeterValueSampleInterval", "HeartbeatInterval"],
        "string list preserved verbatim",
    );
    SchemaValidator::v16j()
        .validate_call("GetConfiguration", &Value::Object(map))
        .expect("GetConfiguration with a key list must be schema-valid");

    // Unset list optional → the whole field is absent (not null, not []).
    let without = GetConfigurationRequest { key: None };
    let map = to_object(&without);
    assert!(
        map.is_empty(),
        "unset key list must be omitted entirely, got {map:?}",
    );
    SchemaValidator::v16j()
        .validate_call("GetConfiguration", &Value::Object(map))
        .expect("GetConfiguration with omitted key list must be schema-valid");
}

/// Ports `test_charge_point.py::test_serialize_as_dict` — a payload carrying a
/// *set* optional survives serialize → JSON text → deserialize unchanged (the
/// serde analog of the reference's `asdict`/`json` round-trip), confirming the
/// omission machinery does not disturb present values.
#[test]
fn payload_with_set_optionals_round_trips_through_json() {
    let ts: DateTime<Utc> = "2027-01-01T00:00:00Z".parse().unwrap();
    let original = MeterValuesRequest {
        connector_id: 3,
        transaction_id: Some(77),
        meter_values: vec![MeterValue {
            timestamp: ts,
            sampled_values: vec![SampledValue {
                value: "42.5".into(),
                context: None,
                format: None,
                measurand: None,
                phase: None,
                location: None,
                unit: None,
            }],
        }],
    };

    let json = serde_json::to_string(&original).expect("serialize");
    let back: MeterValuesRequest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, original, "set optional survives the round-trip");
    // The set optional really is on the wire under its camelCase key.
    assert!(
        json.contains("\"transactionId\":77"),
        "transactionId should appear on the wire, got {json}",
    );
}

/// Ports the `test_charge_point.py` snake↔camel pairs
/// (`test_snake_to_camel_case` / `test_camel_to_snake_case`) — multi-word Rust
/// fields serialize with camelCase wire keys and deserialize back from them,
/// with no snake_case key leaking through. Covers both a 1.6J and a 2.0.1
/// payload (including nested fields).
#[test]
fn camel_case_wire_keys_round_trip() {
    let boot = BootNotificationRequest {
        charge_point_vendor: "VendorX".into(),
        charge_point_model: "ModelY".into(),
        charge_point_serial_number: Some("SN-1".into()),
        charge_box_serial_number: None,
        firmware_version: Some("1.2.3".into()),
        iccid: None,
        imsi: None,
        meter_type: None,
        meter_serial_number: None,
    };
    let map = to_object(&boot);
    for key in [
        "chargePointVendor",
        "chargePointModel",
        "chargePointSerialNumber",
        "firmwareVersion",
    ] {
        assert!(map.contains_key(key), "expected camelCase wire key `{key}`");
    }
    assert!(
        !map.contains_key("charge_point_vendor"),
        "snake_case key must not appear on the wire",
    );
    let back: BootNotificationRequest =
        serde_json::from_value(Value::Object(map)).expect("deserialize from camelCase");
    assert_eq!(back, boot);

    // 2.0.1 payload with nested camelCase fields.
    let req = SetNetworkProfileRequest {
        configuration_slot: 4,
        connection_data: NetworkConnectionProfileType {
            ocpp_version: OCPPVersionEnumType::Ocpp20,
            ocpp_transport: OCPPTransportEnumType::Json,
            ocpp_csms_url: "wss://csms.example/ocpp".into(),
            message_timeout: 10,
            security_profile: 1,
            ocpp_interface: OCPPInterfaceEnumType::Wired0,
            apn: None,
            vpn: None,
            custom_data: None,
        },
        custom_data: None,
    };
    let map = to_object(&req);
    assert!(map.contains_key("configurationSlot"));
    assert!(map.contains_key("connectionData"));
    let conn = map["connectionData"]
        .as_object()
        .expect("connectionData is an object");
    for key in [
        "ocppVersion",
        "ocppTransport",
        "ocppCsmsUrl",
        "messageTimeout",
        "securityProfile",
        "ocppInterface",
    ] {
        assert!(
            conn.contains_key(key),
            "expected camelCase wire key `{key}` in connectionData",
        );
    }
    let back: SetNetworkProfileRequest =
        serde_json::from_value(Value::Object(map)).expect("deserialize from camelCase");
    assert_eq!(back, req);
}
