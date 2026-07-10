//! OCPP 2.0.1 datatype wire-shape conformance suite.
//!
//! A port of the mobilityhouse/ocpp reference's
//! [`tests/v201/test_v201_data_types.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v201/test_v201_data_types.py),
//! grown one thematic slice at a time. This is the datatype-level analog of the
//! enum suite `enums_v201.rs` (#268 / #271); slices are tracked from #273.
//!
//! - **Slice 1 — the transaction / metering hot path** (#273): the datatypes
//!   that ride `TransactionEvent` / `MeterValues`: `AdditionalInfoType`,
//!   `IdTokenType`, `IdTokenInfoType` (+ `MessageContentType`),
//!   `SampledValueType`, `UnitOfMeasureType`, `SignedMeterValueType`,
//!   `MeterValueType`, `EvseType`, and `TransactionType`.
//! - **Slice 2 — the device-model / provisioning path** (#279): the datatypes
//!   carried by `BootNotification`, `NotifyReport`, `GetVariables` /
//!   `SetVariables`, and `SetNetworkProfile`: `ModemType`,
//!   `ChargingStationType`, `ComponentType`, `VariableType`,
//!   `ComponentVariableType`, `GetVariableDataType` / `GetVariableResultType`,
//!   `SetVariableResultType`, `VariableAttributeType`,
//!   `VariableCharacteristicsType`, `ReportDataType`, `APNType`, and
//!   `NetworkConnectionProfileType` (+ nested `VPNType`).
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
//! `MessageContentType` against `schemas/v201/AuthorizeResponse.json`. Slice 2's
//! device-model / provisioning datatypes are verified against
//! `schemas/v201/NotifyReport.json` (`ComponentType`, `VariableType`,
//! `ReportDataType`, `VariableAttributeType`, `VariableCharacteristicsType`),
//! `GetVariablesResponse.json` / `SetVariablesResponse.json` (the
//! get/set-variable result datatypes), `BootNotification.json`
//! (`ChargingStationType`, `ModemType`), and `SetNetworkProfile.json` (`APNType`,
//! `NetworkConnectionProfileType`, `VPNType`). Each test notes the
//! `test_v201_data_types.py` function it ports.
//!
//! ### FINAL-schema-vs-Python-dataclass divergences pinned in slice 2
//!
//! The reference's slice-2 tests construct several fields with shapes the FINAL
//! schemas reject; the Python dataclasses don't validate, so the looseness is
//! invisible there. This suite pins the **schema-valid** shape (matching the
//! Rust datatypes) and records the divergence, matching the #271 discipline:
//!
//! - `VariableCharacteristicsType.min_limit` / `max_limit` — the reference passes
//!   *strings* (`"-20"` / `"50"`); the schema (and Rust) type is `number`.
//! - `VariableCharacteristicsType.values_list` — the reference passes a *list*
//!   (`["10","20","30"]`); the schema (and Rust) type is a single CSV `string`.
//! - `NetworkConnectionProfileType.vpn` — the reference passes a bare
//!   `VPNEnumType` member; the schema (and Rust) field is a nested `VPNType`
//!   *object* (whose `type` field is the enum).
//! - `StatusInfoType.reason_code` — the reference passes a `ReasonEnumType`
//!   member; the schema (and Rust) field is a free `string` (max length 20).
//!
//! ## Deferred to later slices (tracked by a follow-up to #279)
//!
//! The reference file has ~40 datatype tests. Slices 1–2 cover the
//! transaction/metering and device-model/provisioning paths. Still unpinned by a
//! crate-boundary suite, in rough thematic groups for the next slices:
//!
//! - **Monitoring / events:** `EventDataType`, `MonitoringDataType`,
//!   `SetMonitoringDataType`/`ResultType`, `ClearMonitoringResultType`,
//!   `VariableMonitoringType`, `MessageInfoType`.
//! - **Smart charging / tariffs (slice 3):** `ChargingProfileType`,
//!   `ChargingScheduleType`, `ChargingSchedulePeriodType`,
//!   `CompositeScheduleType`, `SalesTariffEntryType`, `ConsumptionCostType`,
//!   `CostType`, `ChargingNeedsType`, `AC/DCChargingParametersType`,
//!   `RelativeTimeIntervalType`.
//! - **Certificates / ISO 15118 (slice 3):** `CertificateHashDataType`/`ChainType`,
//!   `OCSPRequestDataType`, `LogParametersType`, `FirmwareType`.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, to_value, Value};

use ocpp_types::v201::{
    APNAuthenticationEnumType, APNType, AdditionalInfoType, AttributeEnumType,
    AuthorizationStatusEnumType, ChargingStateEnumType, ChargingStationType, ComponentType,
    ComponentVariableType, DataEnumType, EvseType, GetVariableDataType, GetVariableResultType,
    GetVariableStatusEnumType, IdTokenEnumType, IdTokenInfoType, IdTokenType, LocationEnumType,
    MeasurandEnumType, MessageContentType, MessageFormatEnumType, MeterValueType, ModemType,
    MutabilityEnumType, NetworkConnectionProfileType, OCPPInterfaceEnumType, OCPPTransportEnumType,
    OCPPVersionEnumType, PhaseEnumType, ReadingContextEnumType, ReasonEnumType, ReportDataType,
    SampledValueType, SetVariableResultType, SetVariableStatusEnumType, SignedMeterValueType,
    StatusInfoType, TransactionType, UnitOfMeasureType, VPNEnumType, VPNType,
    VariableAttributeType, VariableCharacteristicsType, VariableType,
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

// ---------------------------------------------------------------------------
// Slice 2 — the device-model / provisioning path (#279).
//
// The datatypes carried by `BootNotification`, `NotifyReport`,
// `GetVariables`/`SetVariables`, and `SetNetworkProfile`. Verified against the
// FINAL schemas named in each test's doc comment.
// ---------------------------------------------------------------------------

/// Ports `test_modem_type`. Both fields optional; an empty `ModemType`
/// serializes to `{}`. Verified against `BootNotification.json`'s `ModemType`
/// (the `iccid` / `imsi` field names, not renamed).
#[test]
fn modem_type() {
    round_trip(
        ModemType {
            iccid: Some("89012345678901234567".to_string()),
            imsi: Some("123456789012345".to_string()),
            custom_data: None,
        },
        serde_json::json!({
            "iccid": "89012345678901234567",
            "imsi": "123456789012345",
        }),
    );

    // both-optional form: `{}` (never `null`s).
    round_trip(
        ModemType {
            iccid: None,
            imsi: None,
            custom_data: None,
        },
        serde_json::json!({}),
    );
}

/// Ports `test_charging_station_type`. `model` + `vendorName` required; pins the
/// optional `serialNumber` / `firmwareVersion` and the nested `modem`
/// (`ModemType`) object. Verified against `BootNotification.json`'s
/// `ChargingStationType` — note the wire name is `vendorName`, not `vendor_name`.
#[test]
fn charging_station_type() {
    round_trip(
        ChargingStationType {
            vendor_name: "Vendor ABC".to_string(),
            model: "Station Model X".to_string(),
            serial_number: Some("SN123456".to_string()),
            firmware_version: Some("1.2.3".to_string()),
            modem: Some(ModemType {
                iccid: Some("89001234567890123456".to_string()),
                imsi: Some("123456789012345".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "vendorName": "Vendor ABC",
            "model": "Station Model X",
            "serialNumber": "SN123456",
            "firmwareVersion": "1.2.3",
            "modem": {
                "iccid": "89001234567890123456",
                "imsi": "123456789012345",
            },
        }),
    );

    // required-only form: model + vendorName, everything else omitted.
    round_trip(
        ChargingStationType {
            vendor_name: "V".to_string(),
            model: "M".to_string(),
            serial_number: None,
            firmware_version: None,
            modem: None,
            custom_data: None,
        },
        serde_json::json!({ "vendorName": "V", "model": "M" }),
    );
}

/// Ports `test_component_type`. Only `name` is required; pins the optional
/// `instance` and the nested `evse` (`EvseType`) object. Verified against
/// `NotifyReport.json`'s `ComponentType`.
#[test]
fn component_type() {
    round_trip(
        ComponentType {
            name: "MainController".to_string(),
            instance: Some("instance1".to_string()),
            evse: Some(EvseType {
                id: 1,
                connector_id: Some(2),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "name": "MainController",
            "instance": "instance1",
            "evse": { "id": 1, "connectorId": 2 },
        }),
    );

    // name-only form: instance + evse omitted.
    round_trip(
        ComponentType {
            name: "MainController".to_string(),
            instance: None,
            evse: None,
            custom_data: None,
        },
        serde_json::json!({ "name": "MainController" }),
    );
}

/// `VariableType` — the variable-reference datatype the reference constructs
/// inline in `test_component_variable_type` / `test_get_variable_*`. Only `name`
/// is required; `instance` omitted when absent. Verified against
/// `NotifyReport.json`'s `VariableType`.
#[test]
fn variable_type() {
    round_trip(
        VariableType {
            name: "CurrentLimit".to_string(),
            instance: Some("instance1".to_string()),
            custom_data: None,
        },
        serde_json::json!({ "name": "CurrentLimit", "instance": "instance1" }),
    );

    round_trip(
        VariableType {
            name: "CurrentLimit".to_string(),
            instance: None,
            custom_data: None,
        },
        serde_json::json!({ "name": "CurrentLimit" }),
    );
}

/// Ports `test_component_variable_type`. Pins the nested `component`
/// (`ComponentType`) + optional `variable` (`VariableType`) — the narrowing key
/// carried by `GetReport`. Verified against `NotifyReport.json`.
#[test]
fn component_variable_type() {
    round_trip(
        ComponentVariableType {
            component: ComponentType {
                name: "MainController".to_string(),
                instance: Some("instance1".to_string()),
                evse: None,
                custom_data: None,
            },
            variable: Some(VariableType {
                name: "CurrentLimit".to_string(),
                instance: Some("instance1".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "CurrentLimit", "instance": "instance1" },
        }),
    );

    // component-only form: the whole component is referenced (`variable` omitted).
    round_trip(
        ComponentVariableType {
            component: ComponentType {
                name: "MainController".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: None,
            custom_data: None,
        },
        serde_json::json!({ "component": { "name": "MainController" } }),
    );
}

/// Ports `test_get_variable_data_type`. One entry in a `GetVariables` request:
/// `component` + `variable` required, `attributeType` omitted means `Actual`.
/// Verified against `GetVariables` request schema (shared `AttributeEnumType`).
#[test]
fn get_variable_data_type() {
    round_trip(
        GetVariableDataType {
            component: ComponentType {
                name: "MainController".to_string(),
                instance: Some("instance1".to_string()),
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "CurrentLimit".to_string(),
                instance: Some("instance1".to_string()),
                custom_data: None,
            },
            attribute_type: Some(AttributeEnumType::Actual),
            custom_data: None,
        },
        serde_json::json!({
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "CurrentLimit", "instance": "instance1" },
            "attributeType": "Actual",
        }),
    );

    // attributeType omitted → defaults to Actual on the peer.
    round_trip(
        GetVariableDataType {
            component: ComponentType {
                name: "C".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "V".to_string(),
                instance: None,
                custom_data: None,
            },
            attribute_type: None,
            custom_data: None,
        },
        serde_json::json!({
            "component": { "name": "C" },
            "variable": { "name": "V" },
        }),
    );
}

/// Ports `test_get_variable_result_type`. One entry in a `GetVariables`
/// response: pins the `attributeStatus` (`GetVariableStatusEnumType`) +
/// `attributeType` enum strings, the optional `attributeValue`, and the nested
/// `component`/`variable`. Verified against `GetVariablesResponse.json`.
#[test]
fn get_variable_result_type() {
    round_trip(
        GetVariableResultType {
            attribute_status: GetVariableStatusEnumType::Accepted,
            component: ComponentType {
                name: "MainController".to_string(),
                instance: Some("instance1".to_string()),
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "CurrentLimit".to_string(),
                instance: Some("instance1".to_string()),
                custom_data: None,
            },
            attribute_type: Some(AttributeEnumType::Actual),
            attribute_value: Some("100".to_string()),
            attribute_status_info: None,
            custom_data: None,
        },
        serde_json::json!({
            "attributeStatus": "Accepted",
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "CurrentLimit", "instance": "instance1" },
            "attributeType": "Actual",
            "attributeValue": "100",
        }),
    );

    // Rejected read: no value echoed, optional attributeType omitted.
    round_trip(
        GetVariableResultType {
            attribute_status: GetVariableStatusEnumType::UnknownVariable,
            component: ComponentType {
                name: "C".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "V".to_string(),
                instance: None,
                custom_data: None,
            },
            attribute_type: None,
            attribute_value: None,
            attribute_status_info: None,
            custom_data: None,
        },
        serde_json::json!({
            "attributeStatus": "UnknownVariable",
            "component": { "name": "C" },
            "variable": { "name": "V" },
        }),
    );
}

/// Ports `test_set_variable_result_type`. The write-path counterpart to
/// `GetVariableResultType` — echoes a `SetVariableStatusEnumType` and no value.
/// Pins the nested `attributeStatusInfo` (`StatusInfoType`). Verified against
/// `SetVariablesResponse.json`.
///
/// Divergence pinned: the reference passes `reason_code=ReasonEnumType.other`,
/// but `StatusInfoType.reasonCode` is a free `string` (max length 20) in the
/// FINAL schema — so the wire value is the plain string `"Other"`.
#[test]
fn set_variable_result_type() {
    round_trip(
        SetVariableResultType {
            attribute_status: SetVariableStatusEnumType::Accepted,
            component: ComponentType {
                name: "MainController".to_string(),
                instance: Some("instance1".to_string()),
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "CurrentLimit".to_string(),
                instance: Some("instance1".to_string()),
                custom_data: None,
            },
            attribute_type: Some(AttributeEnumType::Actual),
            attribute_status_info: Some(StatusInfoType {
                reason_code: "Other".to_string(),
                additional_info: Some("Successfully set variable".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "attributeStatus": "Accepted",
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "CurrentLimit", "instance": "instance1" },
            "attributeType": "Actual",
            "attributeStatusInfo": {
                "reasonCode": "Other",
                "additionalInfo": "Successfully set variable",
            },
        }),
    );

    // RebootRequired write: status only, every optional omitted.
    round_trip(
        SetVariableResultType {
            attribute_status: SetVariableStatusEnumType::RebootRequired,
            component: ComponentType {
                name: "C".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "V".to_string(),
                instance: None,
                custom_data: None,
            },
            attribute_type: None,
            attribute_status_info: None,
            custom_data: None,
        },
        serde_json::json!({
            "attributeStatus": "RebootRequired",
            "component": { "name": "C" },
            "variable": { "name": "V" },
        }),
    );
}

/// Ports `test_variable_attribute_type`. Every field is optional: pins the
/// `type` (`AttributeEnumType`) + `mutability` (`MutabilityEnumType`) enum
/// strings and the `persistent` / `constant` booleans. Verified against
/// `NotifyReport.json`'s `VariableAttributeType`.
#[test]
fn variable_attribute_type() {
    round_trip(
        VariableAttributeType {
            kind: Some(AttributeEnumType::Actual),
            value: Some("25.5".to_string()),
            mutability: Some(MutabilityEnumType::ReadWrite),
            persistent: Some(true),
            constant: Some(false),
            custom_data: None,
        },
        serde_json::json!({
            "type": "Actual",
            "value": "25.5",
            "mutability": "ReadWrite",
            "persistent": true,
            "constant": false,
        }),
    );

    // all-optional form: `{}` — proves every field is `skip_serializing_if`.
    round_trip(
        VariableAttributeType {
            kind: None,
            value: None,
            mutability: None,
            persistent: None,
            constant: None,
            custom_data: None,
        },
        serde_json::json!({}),
    );
}

/// Ports `test_variable_characteristics_type`. `dataType` +
/// `supportsMonitoring` required; pins the `DataEnumType` wire string (note the
/// lowercase `"decimal"`). Verified against `NotifyReport.json`'s
/// `VariableCharacteristicsType`.
///
/// Divergences pinned (the reference's dataclass is loose; the FINAL schema is
/// not): `minLimit` / `maxLimit` are `number` here (the reference passes the
/// strings `"-20"` / `"50"`), and `valuesList` is a single CSV `string` (the
/// reference passes the list `["10","20","30"]`).
#[test]
fn variable_characteristics_type() {
    round_trip(
        VariableCharacteristicsType {
            unit: Some("Celsius".to_string()),
            data_type: DataEnumType::Decimal,
            min_limit: Some(-20.0),
            max_limit: Some(50.0),
            values_list: Some("10,20,30".to_string()),
            supports_monitoring: true,
            custom_data: None,
        },
        serde_json::json!({
            "unit": "Celsius",
            "dataType": "decimal",
            "minLimit": -20.0,
            "maxLimit": 50.0,
            "valuesList": "10,20,30",
            "supportsMonitoring": true,
        }),
    );

    // required-only form: dataType + supportsMonitoring, everything else omitted.
    round_trip(
        VariableCharacteristicsType {
            unit: None,
            data_type: DataEnumType::Boolean,
            min_limit: None,
            max_limit: None,
            values_list: None,
            supports_monitoring: false,
            custom_data: None,
        },
        serde_json::json!({ "dataType": "boolean", "supportsMonitoring": false }),
    );
}

/// Ports `test_report_data_type`. One entry in a `NotifyReport`: pins the nested
/// `component` / `variable`, the non-empty `variableAttribute` array (schema:
/// 1–4 items), and the optional `variableCharacteristics`. Verified against
/// `NotifyReport.json`'s `ReportDataType`.
#[test]
fn report_data_type() {
    round_trip(
        ReportDataType {
            component: ComponentType {
                name: "MainController".to_string(),
                instance: Some("instance1".to_string()),
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "Temperature".to_string(),
                instance: Some("instance1".to_string()),
                custom_data: None,
            },
            variable_attribute: vec![VariableAttributeType {
                kind: Some(AttributeEnumType::Actual),
                value: Some("25.5".to_string()),
                mutability: Some(MutabilityEnumType::ReadWrite),
                persistent: Some(true),
                constant: Some(false),
                custom_data: None,
            }],
            variable_characteristics: Some(VariableCharacteristicsType {
                unit: Some("Celsius".to_string()),
                data_type: DataEnumType::Decimal,
                min_limit: Some(-20.0),
                max_limit: Some(50.0),
                values_list: Some("10,20,30".to_string()),
                supports_monitoring: true,
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "Temperature", "instance": "instance1" },
            "variableAttribute": [
                {
                    "type": "Actual",
                    "value": "25.5",
                    "mutability": "ReadWrite",
                    "persistent": true,
                    "constant": false,
                }
            ],
            "variableCharacteristics": {
                "unit": "Celsius",
                "dataType": "decimal",
                "minLimit": -20.0,
                "maxLimit": 50.0,
                "valuesList": "10,20,30",
                "supportsMonitoring": true,
            },
        }),
    );
}

/// Ports `test_apn_type`. `apn` + `apnAuthentication` required; pins every
/// optional (`apnUserName`, `apnPassword`, `simPin`, `preferredNetwork`,
/// `useOnlyPreferredNetwork`) and the `APNAuthenticationEnumType` wire string
/// (`"AUTO"`). Verified against `SetNetworkProfile.json`'s `APNType`.
#[test]
fn apn_type() {
    round_trip(
        APNType {
            apn: "internet.example.com".to_string(),
            apn_user_name: Some("username".to_string()),
            apn_password: Some("password".to_string()),
            sim_pin: Some(1234),
            preferred_network: Some("preferred".to_string()),
            use_only_preferred_network: Some(true),
            apn_authentication: APNAuthenticationEnumType::Auto,
            custom_data: None,
        },
        serde_json::json!({
            "apn": "internet.example.com",
            "apnUserName": "username",
            "apnPassword": "password",
            "simPin": 1234,
            "preferredNetwork": "preferred",
            "useOnlyPreferredNetwork": true,
            "apnAuthentication": "AUTO",
        }),
    );

    // required-only form: apn + apnAuthentication, everything else omitted.
    round_trip(
        APNType {
            apn: "a".to_string(),
            apn_user_name: None,
            apn_password: None,
            sim_pin: None,
            preferred_network: None,
            use_only_preferred_network: None,
            apn_authentication: APNAuthenticationEnumType::None,
            custom_data: None,
        },
        serde_json::json!({ "apn": "a", "apnAuthentication": "NONE" }),
    );
}

/// Ports `test_network_connection_profile_type`. Pins the six required
/// connectivity parameters and their enum wire strings
/// (`OCPPVersionEnumType` = `"OCPP20"`, `OCPPTransportEnumType` = `"JSON"`,
/// `OCPPInterfaceEnumType` = `"Wired0"`) plus the optional bearer blocks.
/// Verified against `SetNetworkProfile.json`'s `NetworkConnectionProfileType`.
///
/// Divergence pinned: the reference passes `vpn=VPNEnumType.ikev2` (a bare enum
/// member), but the FINAL schema's `vpn` field is a nested `VPNType` *object*
/// whose own `type` field carries the enum — so this test builds the full
/// object (`"type": "IKEv2"`).
#[test]
fn network_connection_profile_type() {
    // required-only form: the six connectivity params, no bearer blocks.
    round_trip(
        NetworkConnectionProfileType {
            ocpp_version: OCPPVersionEnumType::Ocpp20,
            ocpp_transport: OCPPTransportEnumType::Json,
            ocpp_csms_url: "wss://example.com/ocpp".to_string(),
            message_timeout: 30,
            security_profile: 1,
            ocpp_interface: OCPPInterfaceEnumType::Wired0,
            apn: None,
            vpn: None,
            custom_data: None,
        },
        serde_json::json!({
            "ocppVersion": "OCPP20",
            "ocppTransport": "JSON",
            "ocppCsmsUrl": "wss://example.com/ocpp",
            "messageTimeout": 30,
            "securityProfile": 1,
            "ocppInterface": "Wired0",
        }),
    );

    // full form: both bearer blocks present, `vpn` as a proper `VPNType` object.
    round_trip(
        NetworkConnectionProfileType {
            ocpp_version: OCPPVersionEnumType::Ocpp20,
            ocpp_transport: OCPPTransportEnumType::Json,
            ocpp_csms_url: "wss://example.com/ocpp".to_string(),
            message_timeout: 30,
            security_profile: 1,
            ocpp_interface: OCPPInterfaceEnumType::Wired0,
            apn: Some(APNType {
                apn: "internet.example.com".to_string(),
                apn_user_name: None,
                apn_password: None,
                sim_pin: None,
                preferred_network: None,
                use_only_preferred_network: None,
                apn_authentication: APNAuthenticationEnumType::Auto,
                custom_data: None,
            }),
            vpn: Some(VPNType {
                server: "vpn.example.com".to_string(),
                user: "vpnuser".to_string(),
                group: None,
                password: "secret".to_string(),
                key: "sharedkey".to_string(),
                vpn_type: VPNEnumType::Ikev2,
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "ocppVersion": "OCPP20",
            "ocppTransport": "JSON",
            "ocppCsmsUrl": "wss://example.com/ocpp",
            "messageTimeout": 30,
            "securityProfile": 1,
            "ocppInterface": "Wired0",
            "apn": {
                "apn": "internet.example.com",
                "apnAuthentication": "AUTO",
            },
            "vpn": {
                "server": "vpn.example.com",
                "user": "vpnuser",
                "password": "secret",
                "key": "sharedkey",
                "type": "IKEv2",
            },
        }),
    );
}
