//! OCPP 2.0.1 datatype wire-shape conformance suite — **slice 1: the
//! transaction / metering hot path**.
//!
//! A port of the mobilityhouse/ocpp reference's
//! [`tests/v201/test_v201_data_types.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v201/test_v201_data_types.py),
//! restricted to the datatypes that ride the `TransactionEvent` / `MeterValues`
//! path: `AdditionalInfoType`, `IdTokenType`, `IdTokenInfoType`
//! (+ `MessageContentType`), `SampledValueType`, `UnitOfMeasureType`,
//! `SignedMeterValueType`, `MeterValueType`, `EvseType`, and `TransactionType`.
//! This is the datatype-level analog of the enum suite `enums_v201.rs`
//! (#268 / #271) and the first of ~3 slices tracked by #273.
//!
//! ## What is pinned, and why it is stronger than the reference
//!
//! The reference's `to_datatype()` helper round-trips a dataclass through
//! `asdict → json.dumps → json.loads → cls(**…)` and asserts the fields, nested
//! dicts, and enum strings survive. That pins the *dataclass* shape (snake_case
//! keys); the camelCase **wire** format is applied later, in `call.py`. Because
//! the Rust datatypes serialize straight to the wire, each test here builds the
//! Rust struct, serializes it, and asserts the **exact JSON wire object** —
//! camelCase field names, `#[serde(skip_serializing_if)]` optional omission,
//! nested arrays/objects, and enum wire strings — then deserializes the
//! expected JSON back and asserts it reconstructs the original struct
//! (serde round-trip). So a single test catches a wrong `rename`, a dropped
//! `skip_serializing_if`, a mis-nested array, or an enum rename drift.
//!
//! ## Source of truth
//!
//! Every expected wire string is cross-checked against the bundled **FINAL**
//! 2.0.1 JSON schemas that `SchemaValidator::v201()` enforces — the same
//! discipline that surfaced the `cChaoJi` / `passwordString` divergence in
//! #271 — not merely against the Python dataclass. The metering datatypes are
//! verified against `schemas/v201/TransactionEvent.json`; `IdTokenInfoType` and
//! `MessageContentType` against `schemas/v201/AuthorizeResponse.json`. Each test
//! notes the `test_v201_data_types.py` function it ports.
//!
//! ## Deferred to slices 2+ (tracked by a follow-up to #273)
//!
//! The reference file has ~40 datatype tests. This slice deliberately covers
//! only the transaction/metering path above. Still unpinned by a crate-boundary
//! suite, in rough thematic groups for the next slices:
//!
//! - **Device model / provisioning:** `ComponentType`, `VariableType`,
//!   `ComponentVariableType`, `ReportDataType`, `VariableAttributeType`,
//!   `VariableCharacteristicsType`, `GetVariableDataType`/`ResultType`,
//!   `SetVariableResultType`, `ChargingStationType`, `ModemType`, `APNType`,
//!   `NetworkConnectionProfileType`.
//! - **Monitoring / events:** `EventDataType`, `MonitoringDataType`,
//!   `SetMonitoringDataType`/`ResultType`, `ClearMonitoringResultType`,
//!   `VariableMonitoringType`, `MessageInfoType`.
//! - **Smart charging / tariffs:** `ChargingProfileType`,
//!   `ChargingScheduleType`, `ChargingSchedulePeriodType`,
//!   `CompositeScheduleType`, `SalesTariffEntryType`, `ConsumptionCostType`,
//!   `CostType`, `ChargingNeedsType`, `AC/DCChargingParametersType`,
//!   `RelativeTimeIntervalType`.
//! - **Certificates / ISO 15118:** `CertificateHashDataType`/`ChainType`,
//!   `OCSPRequestDataType`, `LogParametersType`, `FirmwareType`.
//!
//! Deferred set filed as the slice-2 follow-up on #273.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, to_value, Value};

use ocpp_types::v201::{
    AdditionalInfoType, AuthorizationStatusEnumType, ChargingStateEnumType, EvseType,
    IdTokenEnumType, IdTokenInfoType, IdTokenType, LocationEnumType, MeasurandEnumType,
    MessageContentType, MessageFormatEnumType, MeterValueType, PhaseEnumType,
    ReadingContextEnumType, ReasonEnumType, SampledValueType, SignedMeterValueType,
    TransactionType, UnitOfMeasureType,
};

/// Assert `value` serializes to *exactly* `expected` and that `expected`
/// deserializes back to `value` — the reference's `to_datatype()` round-trip,
/// tightened to pin the byte-for-byte wire object (field names, optional
/// omission, nesting, enum strings) in both directions.
fn round_trip<T>(value: T, expected: Value)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let wire = to_value(&value).expect("serialize");
    assert_eq!(wire, expected, "serialized wire object mismatch");

    let back: T = from_value(expected).expect("deserialize expected wire object");
    assert_eq!(back, value, "round-trip did not reconstruct the original");
}

/// Ports `test_additional_info_type`. Both fields required; `type` is a
/// free-form party string (not an enum) and Rust names it `kind` → wire `type`.
#[test]
fn additional_info_type() {
    round_trip(
        AdditionalInfoType {
            additional_id_token: "additional_token123".to_string(),
            kind: "type_value".to_string(),
            custom_data: None,
        },
        serde_json::json!({
            "additionalIdToken": "additional_token123",
            "type": "type_value",
        }),
    );
}

/// The bare required form of `IdTokenType` (`idToken` + `type`) — `type`
/// serializes the `IdTokenEnumType` wire string, `additionalInfo` is omitted
/// when absent. Constructed by the reference's `Authorize`/`TransactionEvent`
/// request tests; verified against `TransactionEvent.json`'s `IdTokenType`.
#[test]
fn id_token_type_minimal() {
    round_trip(
        IdTokenType {
            id_token: "045918D2CD5C80".to_string(),
            kind: IdTokenEnumType::Iso14443,
            additional_info: None,
            custom_data: None,
        },
        serde_json::json!({
            "idToken": "045918D2CD5C80",
            "type": "ISO14443",
        }),
    );
}

/// `IdTokenType` carrying the optional `additionalInfo` array — pins the nested
/// `AdditionalInfoType` objects inside the array (the schema requires ≥ 1 item
/// when the field is present).
#[test]
fn id_token_type_with_additional_info() {
    round_trip(
        IdTokenType {
            id_token: "primary".to_string(),
            kind: IdTokenEnumType::Central,
            additional_info: Some(vec![AdditionalInfoType {
                additional_id_token: "linked".to_string(),
                kind: "parent".to_string(),
                custom_data: None,
            }]),
            custom_data: None,
        },
        serde_json::json!({
            "idToken": "primary",
            "type": "Central",
            "additionalInfo": [
                { "additionalIdToken": "linked", "type": "parent" }
            ],
        }),
    );
}

/// Ports `test_id_token_info_type`. Pins the full optional field set
/// (`cacheExpiryDateTime`, `chargingPriority`, `language1`, `language2`), the
/// nested `groupIdToken` (`IdTokenType`) and `personalMessage`
/// (`MessageContentType`) objects, and the `status` enum string. Field names
/// and the `AuthorizationStatusEnumType` / `MessageFormatEnumType` wire strings
/// verified against `AuthorizeResponse.json`.
#[test]
fn id_token_info_type() {
    round_trip(
        IdTokenInfoType {
            status: AuthorizationStatusEnumType::Accepted,
            cache_expiry_date_time: Some("2024-01-01T10:00:00Z".to_string()),
            charging_priority: Some(1),
            language1: Some("en".to_string()),
            evse_id: None,
            language2: Some("fr".to_string()),
            group_id_token: Some(IdTokenType {
                id_token: "group-1".to_string(),
                kind: IdTokenEnumType::Central,
                additional_info: None,
                custom_data: None,
            }),
            personal_message: Some(MessageContentType {
                format: MessageFormatEnumType::Ascii,
                content: "Welcome back!".to_string(),
                language: Some("en".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "status": "Accepted",
            "cacheExpiryDateTime": "2024-01-01T10:00:00Z",
            "chargingPriority": 1,
            "language1": "en",
            "language2": "fr",
            "groupIdToken": { "idToken": "group-1", "type": "Central" },
            "personalMessage": {
                "format": "ASCII",
                "content": "Welcome back!",
                "language": "en",
            },
        }),
    );
}

/// The minimal `status`-only `IdTokenInfoType` — every optional field omitted,
/// proving `skip_serializing_if` drops them rather than emitting `null`.
#[test]
fn id_token_info_type_status_only() {
    round_trip(
        IdTokenInfoType {
            status: AuthorizationStatusEnumType::Invalid,
            cache_expiry_date_time: None,
            charging_priority: None,
            language1: None,
            evse_id: None,
            language2: None,
            group_id_token: None,
            personal_message: None,
            custom_data: None,
        },
        serde_json::json!({ "status": "Invalid" }),
    );
}

/// Ports `test_sampled_value_type`. `SampledValueType` is the field with the
/// most rename/enum risk on the metering path: pins `value` (a float),
/// the `context` / `measurand` / `phase` / `location` enum strings, and the
/// nested `unitOfMeasure` object. Enum strings verified against
/// `TransactionEvent.json`.
#[test]
fn sampled_value_type() {
    round_trip(
        SampledValueType {
            value: 230.0,
            context: Some(ReadingContextEnumType::SamplePeriodic),
            measurand: Some(MeasurandEnumType::Voltage),
            phase: Some(PhaseEnumType::L1),
            location: Some(LocationEnumType::Outlet),
            signed_meter_value: None,
            unit_of_measure: Some(UnitOfMeasureType {
                unit: Some("V".to_string()),
                multiplier: Some(0),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "value": 230.0,
            "context": "Sample.Periodic",
            "measurand": "Voltage",
            "phase": "L1",
            "location": "Outlet",
            "unitOfMeasure": { "unit": "V", "multiplier": 0 },
        }),
    );
}

/// The bare required form of `SampledValueType` (`value` only) — with every
/// optional field absent the reading defaults to active-import energy in Wh per
/// the spec. Proves the optionals are omitted, not emitted as `null`.
#[test]
fn sampled_value_type_value_only() {
    round_trip(
        SampledValueType {
            value: 12345.67,
            context: None,
            measurand: None,
            phase: None,
            location: None,
            signed_meter_value: None,
            unit_of_measure: None,
            custom_data: None,
        },
        serde_json::json!({ "value": 12345.67 }),
    );
}

/// `SampledValueType` carrying a `signedMeterValue` — pins the nested
/// `SignedMeterValueType`'s four required fields and their camelCase wire names
/// (`signedMeterData`, `signingMethod`, `encodingMethod`, `publicKey`).
#[test]
fn sampled_value_type_with_signed_meter_value() {
    round_trip(
        SampledValueType {
            value: 0.0,
            context: None,
            measurand: None,
            phase: None,
            location: None,
            signed_meter_value: Some(SignedMeterValueType {
                signed_meter_data: "c2lnbmVk".to_string(),
                signing_method: "ECDSA".to_string(),
                encoding_method: "DLMS Message".to_string(),
                public_key: "cHVi".to_string(),
                custom_data: None,
            }),
            unit_of_measure: None,
            custom_data: None,
        },
        serde_json::json!({
            "value": 0.0,
            "signedMeterValue": {
                "signedMeterData": "c2lnbmVk",
                "signingMethod": "ECDSA",
                "encodingMethod": "DLMS Message",
                "publicKey": "cHVi",
            },
        }),
    );
}

/// Ports `test_unit_of_measure_type`. Both fields optional; an empty
/// `UnitOfMeasureType` serializes to `{}` (both omitted — the value then
/// defaults to `"Wh"` / multiplier `0`).
#[test]
fn unit_of_measure_type_empty() {
    round_trip(
        UnitOfMeasureType {
            unit: None,
            multiplier: None,
            custom_data: None,
        },
        serde_json::json!({}),
    );
}

/// Ports `test_meter_value_type`. Pins the required `timestamp` +
/// non-empty `sampledValue` array, and the `SampledValueType` objects nested
/// inside it. Verified against `TransactionEvent.json`'s `MeterValueType`.
#[test]
fn meter_value_type() {
    round_trip(
        MeterValueType {
            timestamp: "2024-01-01T10:00:00Z".to_string(),
            sampled_value: vec![SampledValueType {
                value: 230.0,
                context: Some(ReadingContextEnumType::SamplePeriodic),
                measurand: Some(MeasurandEnumType::Voltage),
                phase: Some(PhaseEnumType::L1),
                location: Some(LocationEnumType::Outlet),
                signed_meter_value: None,
                unit_of_measure: None,
                custom_data: None,
            }],
            custom_data: None,
        },
        serde_json::json!({
            "timestamp": "2024-01-01T10:00:00Z",
            "sampledValue": [
                {
                    "value": 230.0,
                    "context": "Sample.Periodic",
                    "measurand": "Voltage",
                    "phase": "L1",
                    "location": "Outlet",
                }
            ],
        }),
    );
}

/// `EvseType` — the connector-scoping datatype on the transaction path. Only
/// `id` is required; `connectorId` omitted when absent. Verified against
/// `TransactionEvent.json`'s `EVSEType`.
#[test]
fn evse_type() {
    round_trip(
        EvseType {
            id: 1,
            connector_id: Some(2),
            custom_data: None,
        },
        serde_json::json!({ "id": 1, "connectorId": 2 }),
    );

    // id-only form: connectorId omitted, not null.
    round_trip(
        EvseType {
            id: 3,
            connector_id: None,
            custom_data: None,
        },
        serde_json::json!({ "id": 3 }),
    );
}

/// `TransactionType` — the transaction-state datatype carried by every
/// `TransactionEvent`. Only `transactionId` is required; pins the optional
/// `chargingState` / `timeSpentCharging` / `stoppedReason` / `remoteStartId`
/// and their enum wire strings. Verified against `TransactionEvent.json`.
#[test]
fn transaction_type() {
    round_trip(
        TransactionType {
            transaction_id: "tx-0001".to_string(),
            charging_state: Some(ChargingStateEnumType::Charging),
            time_spent_charging: Some(3600),
            stopped_reason: Some(ReasonEnumType::Local),
            remote_start_id: Some(42),
            custom_data: None,
        },
        serde_json::json!({
            "transactionId": "tx-0001",
            "chargingState": "Charging",
            "timeSpentCharging": 3600,
            "stoppedReason": "Local",
            "remoteStartId": 42,
        }),
    );

    // transactionId-only form: every optional omitted.
    round_trip(
        TransactionType {
            transaction_id: "tx-0002".to_string(),
            charging_state: None,
            time_spent_charging: None,
            stopped_reason: None,
            remote_start_id: None,
            custom_data: None,
        },
        serde_json::json!({ "transactionId": "tx-0002" }),
    );
}
