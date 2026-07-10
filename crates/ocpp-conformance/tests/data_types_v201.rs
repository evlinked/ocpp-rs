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
//! - **Slice 3a — the smart-charging / tariffs path** (#281): the datatypes
//!   carried by `SetChargingProfile`, `NotifyEVChargingSchedule`,
//!   `NotifyEVChargingNeeds`, and `GetCompositeSchedule`:
//!   `ACChargingParametersType`, `DCChargingParametersType`, `CostType`,
//!   `ConsumptionCostType`, `RelativeTimeIntervalType`,
//!   `ChargingSchedulePeriodType`, `ChargingScheduleType`,
//!   `ChargingProfileType`, `CompositeScheduleType`, `SalesTariffEntryType`,
//!   `SalesTariffType`, and `ChargingNeedsType`.
//! - **Slice 3b — the monitoring/events + certificates/ISO-15118 paths**
//!   (#284): the datatypes carried by `NotifyEvent`, `NotifyMonitoringReport`,
//!   `SetVariableMonitoring` / `ClearVariableMonitoring`, `SetDisplayMessage`,
//!   `GetInstalledCertificateIds` / `DeleteCertificate`, `GetCertificateStatus`,
//!   `GetLog`, and `PublishFirmware`: `EventDataType`, `VariableMonitoringType`,
//!   `MonitoringDataType`, `SetMonitoringDataType`, `SetMonitoringResultType`,
//!   `ClearMonitoringResultType`, `MessageInfoType`, `CertificateHashDataType`,
//!   `CertificateHashDataChainType`, `OCSPRequestDataType`, `LogParametersType`,
//!   and `FirmwareType`.
//! - **Slice 3c — the smart-charging *control* datatypes** (#285): the three
//!   remaining smart-charging datatypes that sit on the command side rather than
//!   the tariff side slice 3a covered — the *externally-imposed limit* reported
//!   by `NotifyChargingLimit` and the two *profile-selection filters* used by
//!   `GetChargingProfiles` / `ClearChargingProfile`: `ChargingLimitType`,
//!   `ChargingProfileCriterionType`, and `ClearChargingProfileType`. This closes
//!   full datatype coverage of `test_v201_data_types.py`.
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
//! ### FINAL-schema-vs-Python-dataclass divergences pinned in slice 3a
//!
//! - `ACChargingParametersType.energy_amount` / `ev_min_current` /
//!   `ev_max_current` / `ev_max_voltage` — the reference passes *floats*
//!   (`20.5` / `10.0` / `32.0` / `400`); every one is `integer` in the FINAL
//!   schema (and Rust `i32`), so the wire values are integers.
//! - `CostType.amount` — the reference passes the *float* `1.0`; the schema (and
//!   Rust `i32`) type is `integer`.
//! - `CostType.cost_kind` — the reference's `ConsumptionCostType` /
//!   `SalesTariffEntryType` tests pass the raw string `"RelativePrice"`, which is
//!   **not** a member of `CostKindEnumType` (the FINAL enum is
//!   `CarbonDioxideEmission` / `RelativePricePercentage` /
//!   `RenewableGenerationPercentage`). This suite pins the schema-valid
//!   `RelativePricePercentage` instead.
//!
//! ### FINAL-schema-vs-Python-dataclass divergences pinned in slice 3b
//!
//! - `MonitoringDataType.variable_monitoring` — the reference passes a *single*
//!   `VariableMonitoringType`; the FINAL schema (`NotifyMonitoringReport.json`)
//!   and Rust type require an *array* (`minItems: 1`). Pinned as an array.
//! - `MessageInfoType.priority` — the reference passes the integer `1`; the
//!   schema and Rust type is the enum `MessagePriorityEnumType`. Pinned as the
//!   valid member `AlwaysFront`.
//! - `MessageInfoType.state` — the reference passes `ChargingStateEnumType`;
//!   the schema and Rust field is `MessageStateEnumType`. The wire string
//!   `"Charging"` is valid in both enums, so it is pinned as
//!   `MessageStateEnumType::Charging`.
//! - `CertificateHashDataChainType.certificate_type` — the reference passes
//!   `"V2G"`, not a member of `GetCertificateIdUseEnumType`. Pinned as the valid
//!   member `V2GRootCertificate`.
//! - `SetMonitoringResultType` / `ClearMonitoringResultType` `statusInfo` —
//!   `reasonCode` is a free `string` (the #278/#280 divergence), so the wire
//!   value is the bare `"Other"` rather than a `ReasonEnumType` member.
//!
//! ## Datatype coverage of `test_v201_data_types.py`
//!
//! Slices 1–3c pin the full metering, device-model/provisioning,
//! smart-charging/tariffs, monitoring/events, certificate/ISO-15118, and
//! smart-charging/control datatype trees. Together with `AuthorizationData`
//! (pinned by the local-list suite `local_list.rs`), **every datatype exercised
//! by `test_v201_data_types.py` is now pinned by a crate-boundary suite** — the
//! port of the reference file's datatype tests is complete. Slice 3c found no
//! new Python-dataclass-vs-FINAL-schema divergence: the reference constructs
//! schema-valid values for all three control datatypes, and the Rust structs,
//! Python dataclasses, and FINAL schemas (`NotifyChargingLimit.json`,
//! `GetChargingProfiles.json`, `ClearChargingProfile.json`) agree field-for-field.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_value, to_value, Value};

use ocpp_types::v201::{
    ACChargingParametersType, APNAuthenticationEnumType, APNType, AdditionalInfoType,
    AttributeEnumType, AuthorizationStatusEnumType, ChargingNeedsType, ChargingProfileKindEnumType,
    ChargingProfilePurposeEnumType, ChargingProfileType, ChargingRateUnitEnumType,
    ChargingSchedulePeriodType, ChargingScheduleType, ChargingStateEnumType, ChargingStationType,
    ComponentType, ComponentVariableType, CompositeScheduleType, ConsumptionCostType,
    CostKindEnumType, CostType, DCChargingParametersType, DataEnumType, EnergyTransferModeEnumType,
    EvseType, GetVariableDataType, GetVariableResultType, GetVariableStatusEnumType,
    IdTokenEnumType, IdTokenInfoType, IdTokenType, LocationEnumType, MeasurandEnumType,
    MessageContentType, MessageFormatEnumType, MeterValueType, ModemType, MutabilityEnumType,
    NetworkConnectionProfileType, OCPPInterfaceEnumType, OCPPTransportEnumType,
    OCPPVersionEnumType, PhaseEnumType, ReadingContextEnumType, ReasonEnumType,
    RelativeTimeIntervalType, ReportDataType, SalesTariffEntryType, SalesTariffType,
    SampledValueType, SetVariableResultType, SetVariableStatusEnumType, SignedMeterValueType,
    StatusInfoType, TransactionType, UnitOfMeasureType, VPNEnumType, VPNType,
    VariableAttributeType, VariableCharacteristicsType, VariableType,
};
// Slice 3b — monitoring/events + certificates/ISO-15118 datatypes (#284).
use ocpp_types::v201::{
    CertificateHashDataChainType, CertificateHashDataType, ClearMonitoringResultType,
    ClearMonitoringStatusEnumType, EventDataType, EventNotificationEnumType, EventTriggerEnumType,
    FirmwareType, GetCertificateIdUseEnumType, HashAlgorithmEnumType, LogParametersType,
    MessageInfoType, MessagePriorityEnumType, MessageStateEnumType, MonitorEnumType,
    MonitoringDataType, OCSPRequestDataType, SetMonitoringDataType, SetMonitoringResultType,
    SetMonitoringStatusEnumType, VariableMonitoringType,
};
// Slice 3c — smart-charging *control* datatypes (#285).
use ocpp_types::v201::{
    ChargingLimitSourceEnumType, ChargingLimitType, ChargingProfileCriterionType,
    ClearChargingProfileType,
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

// ---------------------------------------------------------------------------
// Slice 3a — the smart-charging / tariffs path (#281).
//
// The datatypes carried by `SetChargingProfile`, `NotifyEVChargingSchedule`,
// `NotifyEVChargingNeeds`, and `GetCompositeSchedule`. Verified against the
// FINAL schemas named in each test's doc comment. The reference's dataclasses
// are loose about numeric widths and enum membership; where the FINAL schema is
// stricter, this suite pins the schema-valid shape and records the divergence
// (see the module doc's "slice 3a" divergence list).
// ---------------------------------------------------------------------------

/// Ports `test_ac_charging_parameters_type`. All four fields are required and
/// `integer` in the FINAL schema. Verified against
/// `NotifyEVChargingNeeds.json`'s `ACChargingParametersType`.
///
/// Divergence pinned: the reference passes *floats*
/// (`energy_amount=20.5`, `ev_min_current=10.0`, `ev_max_current=32.0`,
/// `ev_max_voltage=400`); the schema (and Rust `i32`) type is `integer`.
#[test]
fn ac_charging_parameters_type() {
    round_trip(
        ACChargingParametersType {
            energy_amount: 20,
            ev_min_current: 10,
            ev_max_current: 32,
            ev_max_voltage: 400,
            custom_data: None,
        },
        serde_json::json!({
            "energyAmount": 20,
            "evMinCurrent": 10,
            "evMaxCurrent": 32,
            "evMaxVoltage": 400,
        }),
    );
}

/// `DCChargingParametersType` — constructed inside the reference's
/// `test_charging_needs_type` (no standalone reference test). `evMaxCurrent` +
/// `evMaxVoltage` required; pins the optional `energyAmount` / `evMaxPower` /
/// `stateOfCharge` and their camelCase wire names. Verified against
/// `NotifyEVChargingNeeds.json`'s `DCChargingParametersType`.
#[test]
fn dc_charging_parameters_type() {
    round_trip(
        DCChargingParametersType {
            ev_max_current: 100,
            ev_max_voltage: 500,
            energy_amount: Some(50),
            ev_max_power: Some(50000),
            state_of_charge: Some(80),
            ev_energy_capacity: None,
            full_soc: None,
            bulk_soc: None,
            custom_data: None,
        },
        serde_json::json!({
            "evMaxCurrent": 100,
            "evMaxVoltage": 500,
            "energyAmount": 50,
            "evMaxPower": 50000,
            "stateOfCharge": 80,
        }),
    );

    // required-only form: evMaxCurrent + evMaxVoltage, every optional omitted.
    round_trip(
        DCChargingParametersType {
            ev_max_current: 32,
            ev_max_voltage: 400,
            energy_amount: None,
            ev_max_power: None,
            state_of_charge: None,
            ev_energy_capacity: None,
            full_soc: None,
            bulk_soc: None,
            custom_data: None,
        },
        serde_json::json!({ "evMaxCurrent": 32, "evMaxVoltage": 400 }),
    );
}

/// Ports `test_cost_type`. `costKind` + `amount` required; pins the
/// `CostKindEnumType` wire string and the optional `amountMultiplier`. Verified
/// against `SetChargingProfile.json`'s `CostType`.
///
/// Divergence pinned: the reference passes the *float* `amount=1.0`; the schema
/// (and Rust `i32`) type is `integer`, so the wire value is the integer `1`.
#[test]
fn cost_type() {
    round_trip(
        CostType {
            cost_kind: CostKindEnumType::CarbonDioxideEmission,
            amount: 1,
            amount_multiplier: Some(0),
            custom_data: None,
        },
        serde_json::json!({
            "costKind": "CarbonDioxideEmission",
            "amount": 1,
            "amountMultiplier": 0,
        }),
    );

    // required-only form: amountMultiplier omitted.
    round_trip(
        CostType {
            cost_kind: CostKindEnumType::RenewableGenerationPercentage,
            amount: 42,
            amount_multiplier: None,
            custom_data: None,
        },
        serde_json::json!({
            "costKind": "RenewableGenerationPercentage",
            "amount": 42,
        }),
    );
}

/// Ports `test_consumption_cost_type`. `startValue` (a `number`) + a non-empty
/// `cost` array (schema: 1–3 items) required; pins the nested `CostType`.
/// Verified against `SetChargingProfile.json`'s `ConsumptionCostType`.
///
/// Divergence pinned: the reference passes `cost_kind="RelativePrice"`, which is
/// not a member of `CostKindEnumType`; this test pins the schema-valid
/// `RelativePricePercentage`.
#[test]
fn consumption_cost_type() {
    round_trip(
        ConsumptionCostType {
            start_value: 0.0,
            cost: vec![CostType {
                cost_kind: CostKindEnumType::RelativePricePercentage,
                amount: 1,
                amount_multiplier: Some(0),
                custom_data: None,
            }],
            custom_data: None,
        },
        serde_json::json!({
            "startValue": 0.0,
            "cost": [
                {
                    "costKind": "RelativePricePercentage",
                    "amount": 1,
                    "amountMultiplier": 0,
                }
            ],
        }),
    );
}

/// Ports `test_relative_time_interval_type`. Only `start` is required;
/// `duration` omitted when absent. Verified against
/// `SetChargingProfile.json`'s `RelativeTimeIntervalType`.
#[test]
fn relative_time_interval_type() {
    round_trip(
        RelativeTimeIntervalType {
            start: 0,
            duration: Some(3600),
            custom_data: None,
        },
        serde_json::json!({ "start": 0, "duration": 3600 }),
    );

    // start-only form: duration omitted, not null.
    round_trip(
        RelativeTimeIntervalType {
            start: 900,
            duration: None,
            custom_data: None,
        },
        serde_json::json!({ "start": 900 }),
    );
}

/// Ports `test_charging_schedule_period_type`. `startPeriod` + `limit` (a
/// `number`) required; pins the optional `numberPhases` / `phaseToUse`. Verified
/// against `SetChargingProfile.json`'s `ChargingSchedulePeriodType`.
#[test]
fn charging_schedule_period_type() {
    round_trip(
        ChargingSchedulePeriodType {
            start_period: 0,
            limit: 32.0,
            number_phases: Some(3),
            phase_to_use: Some(1),
            custom_data: None,
        },
        serde_json::json!({
            "startPeriod": 0,
            "limit": 32.0,
            "numberPhases": 3,
            "phaseToUse": 1,
        }),
    );

    // required-only form: numberPhases (default 3) + phaseToUse omitted.
    round_trip(
        ChargingSchedulePeriodType {
            start_period: 3600,
            limit: 11000.0,
            number_phases: None,
            phase_to_use: None,
            custom_data: None,
        },
        serde_json::json!({ "startPeriod": 3600, "limit": 11000.0 }),
    );
}

/// `ChargingScheduleType` — constructed inside the reference's
/// `test_charging_profile_type`. `id` + `chargingRateUnit` + a non-empty
/// `chargingSchedulePeriod` array required; pins the `ChargingRateUnitEnumType`
/// wire string (`"W"`), the optional `startSchedule` / `duration`, and the
/// nested period objects. Verified against `SetChargingProfile.json`'s
/// `ChargingScheduleType`.
#[test]
fn charging_schedule_type() {
    round_trip(
        ChargingScheduleType {
            id: 1,
            charging_rate_unit: ChargingRateUnitEnumType::W,
            charging_schedule_period: vec![ChargingSchedulePeriodType {
                start_period: 0,
                limit: 11000.0,
                number_phases: Some(3),
                phase_to_use: None,
                custom_data: None,
            }],
            start_schedule: Some("2024-01-01T10:00:00Z".to_string()),
            duration: Some(3600),
            min_charging_rate: None,
            sales_tariff: None,
            custom_data: None,
        },
        serde_json::json!({
            "id": 1,
            "chargingRateUnit": "W",
            "chargingSchedulePeriod": [
                { "startPeriod": 0, "limit": 11000.0, "numberPhases": 3 }
            ],
            "startSchedule": "2024-01-01T10:00:00Z",
            "duration": 3600,
        }),
    );

    // required-only form: id + chargingRateUnit + one period; the absolute-time
    // and tariff optionals all omitted.
    round_trip(
        ChargingScheduleType {
            id: 2,
            charging_rate_unit: ChargingRateUnitEnumType::A,
            charging_schedule_period: vec![ChargingSchedulePeriodType {
                start_period: 0,
                limit: 16.0,
                number_phases: None,
                phase_to_use: None,
                custom_data: None,
            }],
            start_schedule: None,
            duration: None,
            min_charging_rate: None,
            sales_tariff: None,
            custom_data: None,
        },
        serde_json::json!({
            "id": 2,
            "chargingRateUnit": "A",
            "chargingSchedulePeriod": [ { "startPeriod": 0, "limit": 16.0 } ],
        }),
    );
}

/// Ports `test_charging_profile_type`. `id` + `stackLevel` +
/// `chargingProfilePurpose` + `chargingProfileKind` + a non-empty
/// `chargingSchedule` array required; pins the two enum wire strings
/// (`"TxDefaultProfile"`, `"Absolute"`), the optional `validFrom` / `validTo`,
/// and the nested schedule. Verified against `SetChargingProfile.json`'s
/// `ChargingProfileType`.
#[test]
fn charging_profile_type() {
    round_trip(
        ChargingProfileType {
            id: 1,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeEnumType::TxDefaultProfile,
            charging_profile_kind: ChargingProfileKindEnumType::Absolute,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: ChargingRateUnitEnumType::W,
                charging_schedule_period: vec![ChargingSchedulePeriodType {
                    start_period: 0,
                    limit: 11000.0,
                    number_phases: Some(3),
                    phase_to_use: None,
                    custom_data: None,
                }],
                start_schedule: Some("2024-01-01T10:00:00Z".to_string()),
                duration: Some(3600),
                min_charging_rate: None,
                sales_tariff: None,
                custom_data: None,
            }],
            recurrency_kind: None,
            valid_from: Some("2024-01-01T00:00:00Z".to_string()),
            valid_to: Some("2024-12-31T23:59:59Z".to_string()),
            transaction_id: None,
            custom_data: None,
        },
        serde_json::json!({
            "id": 1,
            "stackLevel": 0,
            "chargingProfilePurpose": "TxDefaultProfile",
            "chargingProfileKind": "Absolute",
            "chargingSchedule": [
                {
                    "id": 1,
                    "chargingRateUnit": "W",
                    "chargingSchedulePeriod": [
                        { "startPeriod": 0, "limit": 11000.0, "numberPhases": 3 }
                    ],
                    "startSchedule": "2024-01-01T10:00:00Z",
                    "duration": 3600,
                }
            ],
            "validFrom": "2024-01-01T00:00:00Z",
            "validTo": "2024-12-31T23:59:59Z",
        }),
    );
}

/// Ports `test_composite_schedule_type`. Every field is required: `evseId`,
/// `duration`, `scheduleStart`, `chargingRateUnit`, and a non-empty
/// `chargingSchedulePeriod` array. Pins the `ChargingRateUnitEnumType` wire
/// string and the nested periods. Verified against
/// `GetCompositeScheduleResponse.json`'s `CompositeScheduleType`.
#[test]
fn composite_schedule_type() {
    round_trip(
        CompositeScheduleType {
            evse_id: 1,
            duration: 3600,
            schedule_start: "2024-01-01T10:00:00Z".to_string(),
            charging_rate_unit: ChargingRateUnitEnumType::W,
            charging_schedule_period: vec![ChargingSchedulePeriodType {
                start_period: 0,
                limit: 11000.0,
                number_phases: Some(3),
                phase_to_use: None,
                custom_data: None,
            }],
            custom_data: None,
        },
        serde_json::json!({
            "evseId": 1,
            "duration": 3600,
            "scheduleStart": "2024-01-01T10:00:00Z",
            "chargingRateUnit": "W",
            "chargingSchedulePeriod": [
                { "startPeriod": 0, "limit": 11000.0, "numberPhases": 3 }
            ],
        }),
    );
}

/// Ports `test_sales_tariff_entry_type`. Only `relativeTimeInterval` is
/// required; pins the optional `ePriceLevel` and the deeply nested
/// `consumptionCost[].cost[]` chain. Verified against
/// `SetChargingProfile.json`'s `SalesTariffEntryType`.
///
/// Divergence pinned: the reference's nested cost passes
/// `cost_kind="RelativePrice"` (not a `CostKindEnumType` member); this test pins
/// the schema-valid `RelativePricePercentage`.
#[test]
fn sales_tariff_entry_type() {
    round_trip(
        SalesTariffEntryType {
            relative_time_interval: RelativeTimeIntervalType {
                start: 0,
                duration: Some(3600),
                custom_data: None,
            },
            e_price_level: Some(1),
            consumption_cost: Some(vec![ConsumptionCostType {
                start_value: 0.0,
                cost: vec![CostType {
                    cost_kind: CostKindEnumType::RelativePricePercentage,
                    amount: 1,
                    amount_multiplier: Some(0),
                    custom_data: None,
                }],
                custom_data: None,
            }]),
            custom_data: None,
        },
        serde_json::json!({
            "relativeTimeInterval": { "start": 0, "duration": 3600 },
            "ePriceLevel": 1,
            "consumptionCost": [
                {
                    "startValue": 0.0,
                    "cost": [
                        {
                            "costKind": "RelativePricePercentage",
                            "amount": 1,
                            "amountMultiplier": 0,
                        }
                    ],
                }
            ],
        }),
    );

    // interval-only form: ePriceLevel + consumptionCost omitted.
    round_trip(
        SalesTariffEntryType {
            relative_time_interval: RelativeTimeIntervalType {
                start: 3600,
                duration: None,
                custom_data: None,
            },
            e_price_level: None,
            consumption_cost: None,
            custom_data: None,
        },
        serde_json::json!({
            "relativeTimeInterval": { "start": 3600 },
        }),
    );
}

/// `SalesTariffType` — the tariff container carried on `ChargingScheduleType`
/// (no standalone reference test; the reference exercises only
/// `SalesTariffEntryType`). `id` + a non-empty `salesTariffEntry` array
/// required; pins the optional `salesTariffDescription` / `numEPriceLevels` and
/// the nested entries. Verified against `SetChargingProfile.json`'s
/// `SalesTariffType`.
#[test]
fn sales_tariff_type() {
    round_trip(
        SalesTariffType {
            id: 1,
            sales_tariff_entry: vec![SalesTariffEntryType {
                relative_time_interval: RelativeTimeIntervalType {
                    start: 0,
                    duration: Some(3600),
                    custom_data: None,
                },
                e_price_level: Some(1),
                consumption_cost: None,
                custom_data: None,
            }],
            sales_tariff_description: Some("Off-peak".to_string()),
            num_e_price_levels: Some(2),
            custom_data: None,
        },
        serde_json::json!({
            "id": 1,
            "salesTariffEntry": [
                {
                    "relativeTimeInterval": { "start": 0, "duration": 3600 },
                    "ePriceLevel": 1,
                }
            ],
            "salesTariffDescription": "Off-peak",
            "numEPriceLevels": 2,
        }),
    );

    // required-only form: id + one entry, description/numEPriceLevels omitted.
    round_trip(
        SalesTariffType {
            id: 2,
            sales_tariff_entry: vec![SalesTariffEntryType {
                relative_time_interval: RelativeTimeIntervalType {
                    start: 0,
                    duration: None,
                    custom_data: None,
                },
                e_price_level: None,
                consumption_cost: None,
                custom_data: None,
            }],
            sales_tariff_description: None,
            num_e_price_levels: None,
            custom_data: None,
        },
        serde_json::json!({
            "id": 2,
            "salesTariffEntry": [ { "relativeTimeInterval": { "start": 0 } } ],
        }),
    );
}

/// Ports `test_charging_needs_type`. Only `requestedEnergyTransfer` is required;
/// pins the `EnergyTransferModeEnumType` wire string (`"DC"`), the optional
/// `departureTime`, and the nested `acChargingParameters` /
/// `dcChargingParameters` objects. Verified against
/// `NotifyEVChargingNeeds.json`'s `ChargingNeedsType`.
#[test]
fn charging_needs_type() {
    round_trip(
        ChargingNeedsType {
            requested_energy_transfer: EnergyTransferModeEnumType::Dc,
            departure_time: Some("2024-01-01T10:00:00Z".to_string()),
            ac_charging_parameters: Some(ACChargingParametersType {
                energy_amount: 20,
                ev_min_current: 10,
                ev_max_current: 32,
                ev_max_voltage: 400,
                custom_data: None,
            }),
            dc_charging_parameters: Some(DCChargingParametersType {
                ev_max_current: 100,
                ev_max_voltage: 500,
                energy_amount: Some(50),
                ev_max_power: Some(50000),
                state_of_charge: Some(80),
                ev_energy_capacity: None,
                full_soc: None,
                bulk_soc: None,
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "requestedEnergyTransfer": "DC",
            "departureTime": "2024-01-01T10:00:00Z",
            "acChargingParameters": {
                "energyAmount": 20,
                "evMinCurrent": 10,
                "evMaxCurrent": 32,
                "evMaxVoltage": 400,
            },
            "dcChargingParameters": {
                "evMaxCurrent": 100,
                "evMaxVoltage": 500,
                "energyAmount": 50,
                "evMaxPower": 50000,
                "stateOfCharge": 80,
            },
        }),
    );

    // required-only form: the mode alone, both parameter blocks omitted.
    round_trip(
        ChargingNeedsType {
            requested_energy_transfer: EnergyTransferModeEnumType::AcThreePhase,
            departure_time: None,
            ac_charging_parameters: None,
            dc_charging_parameters: None,
            custom_data: None,
        },
        serde_json::json!({ "requestedEnergyTransfer": "AC_three_phase" }),
    );
}

// ---------------------------------------------------------------------------
// Slice 3b — monitoring / events + certificates / ISO 15118 (#284)
//
// Completes the datatype port of `test_v201_data_types.py` for the
// monitoring/events (`NotifyEvent` / `NotifyMonitoringReport` /
// `SetVariableMonitoring` / `ClearVariableMonitoring` / `SetDisplayMessage`)
// and certificate/ISO-15118 (`GetInstalledCertificateIds` / `GetCertificateStatus`
// / `GetLog` / `PublishFirmware`) families. Types below are imported in the
// slice-3b `use` block near the top of the file.
// ---------------------------------------------------------------------------

/// Ports `test_event_data_type`. `trigger` and `eventNotificationType` carry
/// their enum wire strings; `component` / `variable` nest; `cleared` is
/// `Some(false)` so it is emitted (not omitted). Verified against
/// `NotifyEvent.json`'s `EventDataType`.
#[test]
fn event_data_type() {
    round_trip(
        EventDataType {
            event_id: 1,
            timestamp: "2024-01-01T10:00:00Z".to_string(),
            trigger: EventTriggerEnumType::Alerting,
            actual_value: "High Temperature".to_string(),
            event_notification_type: EventNotificationEnumType::HardWiredNotification,
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
            cause: None,
            tech_code: Some("TC001".to_string()),
            tech_info: Some("Temperature sensor reading high".to_string()),
            cleared: Some(false),
            transaction_id: Some("TX001".to_string()),
            variable_monitoring_id: Some(1),
            custom_data: None,
        },
        serde_json::json!({
            "eventId": 1,
            "timestamp": "2024-01-01T10:00:00Z",
            "trigger": "Alerting",
            "actualValue": "High Temperature",
            "eventNotificationType": "HardWiredNotification",
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "Temperature", "instance": "instance1" },
            "techCode": "TC001",
            "techInfo": "Temperature sensor reading high",
            "cleared": false,
            "transactionId": "TX001",
            "variableMonitoringId": 1,
        }),
    );
}

/// Ports `test_variable_monitoring_type`. All five fields required; `type`
/// carries the `MonitorEnumType` wire string and Rust names it `kind`; `value`
/// is a JSON number. Verified against `NotifyMonitoringReport.json`.
#[test]
fn variable_monitoring_type() {
    round_trip(
        VariableMonitoringType {
            id: 1,
            transaction: true,
            value: 100.0,
            kind: MonitorEnumType::UpperThreshold,
            severity: 1,
            custom_data: None,
        },
        serde_json::json!({
            "id": 1,
            "transaction": true,
            "value": 100.0,
            "type": "UpperThreshold",
            "severity": 1,
        }),
    );
}

/// Ports `test_monitoring_data_type`.
///
/// FINAL-schema-vs-Python-dataclass divergence: the reference passes a **single**
/// `VariableMonitoringType` for `variable_monitoring`; the FINAL schema
/// (`NotifyMonitoringReport.json`) and the Rust datatype require an **array**
/// (`minItems: 1`). This suite pins the schema-valid array shape.
#[test]
fn monitoring_data_type() {
    round_trip(
        MonitoringDataType {
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
            variable_monitoring: vec![VariableMonitoringType {
                id: 1,
                transaction: true,
                value: 100.0,
                kind: MonitorEnumType::UpperThreshold,
                severity: 1,
                custom_data: None,
            }],
            custom_data: None,
        },
        serde_json::json!({
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "Temperature", "instance": "instance1" },
            "variableMonitoring": [
                {
                    "id": 1,
                    "transaction": true,
                    "value": 100.0,
                    "type": "UpperThreshold",
                    "severity": 1,
                }
            ],
        }),
    );
}

/// Ports `test_set_monitoring_data_type`. `id` / `transaction` are optional on
/// the request; here `id` is set (replacing an existing monitor) and
/// `transaction` omitted. Verified against `SetVariableMonitoring.json`'s
/// `SetMonitoringDataType`.
#[test]
fn set_monitoring_data_type() {
    round_trip(
        SetMonitoringDataType {
            id: Some(123456),
            transaction: None,
            value: 100.0,
            kind: MonitorEnumType::UpperThreshold,
            severity: 1,
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
            custom_data: None,
        },
        serde_json::json!({
            "id": 123456,
            "value": 100.0,
            "type": "UpperThreshold",
            "severity": 1,
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "Temperature", "instance": "instance1" },
        }),
    );
}

/// Ports `test_set_monitoring_result_type`. Pins the nested `statusInfo`.
///
/// FINAL-schema-vs-Python-dataclass divergence (the #278/#280 `StatusInfoType`
/// pattern): the reference passes `reason_code=ReasonEnumType.other`; the FINAL
/// schema (`SetVariableMonitoringResponse.json`) and Rust type `reasonCode` as a
/// free `string` (max length 20), so the wire value is the bare `"Other"`.
#[test]
fn set_monitoring_result_type() {
    round_trip(
        SetMonitoringResultType {
            id: Some(123),
            status: SetMonitoringStatusEnumType::Accepted,
            kind: MonitorEnumType::UpperThreshold,
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
            severity: 1,
            status_info: Some(StatusInfoType {
                reason_code: "Other".to_string(),
                additional_info: Some("Successfully set monitoring".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "id": 123,
            "status": "Accepted",
            "type": "UpperThreshold",
            "component": { "name": "MainController", "instance": "instance1" },
            "variable": { "name": "Temperature", "instance": "instance1" },
            "severity": 1,
            "statusInfo": {
                "reasonCode": "Other",
                "additionalInfo": "Successfully set monitoring",
            },
        }),
    );
}

/// Ports `test_clear_monitoring_result_type`. Pins the nested `statusInfo`
/// (same free-`string` `reasonCode` divergence as `set_monitoring_result_type`).
/// Verified against `ClearVariableMonitoringResponse.json`.
#[test]
fn clear_monitoring_result_type() {
    round_trip(
        ClearMonitoringResultType {
            status: ClearMonitoringStatusEnumType::Accepted,
            id: 123,
            status_info: Some(StatusInfoType {
                reason_code: "Other".to_string(),
                additional_info: Some("Successfully cleared monitoring".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "status": "Accepted",
            "id": 123,
            "statusInfo": {
                "reasonCode": "Other",
                "additionalInfo": "Successfully cleared monitoring",
            },
        }),
    );
}

/// Ports `test_message_info_type`. Pins the nested `message`
/// (`MessageContentType`) and `display` (`ComponentType`).
///
/// Two FINAL-schema-vs-Python-dataclass divergences pinned here
/// (`SetDisplayMessage.json` / `NotifyDisplayMessages.json`):
/// - `priority` — the reference passes the integer `1`; the schema and Rust
///   type is `MessagePriorityEnumType`, so a valid member (`AlwaysFront`) is
///   pinned.
/// - `state` — the reference passes `ChargingStateEnumType.charging` (the wrong
///   enum); the schema and Rust field is `MessageStateEnumType`. The wire string
///   `"Charging"` is a valid member of *both* enums, so it round-trips as
///   `MessageStateEnumType::Charging`.
#[test]
fn message_info_type() {
    round_trip(
        MessageInfoType {
            id: 1,
            priority: MessagePriorityEnumType::AlwaysFront,
            message: MessageContentType {
                format: MessageFormatEnumType::Ascii,
                content: "Important notice".to_string(),
                language: Some("en".to_string()),
                custom_data: None,
            },
            state: Some(MessageStateEnumType::Charging),
            start_date_time: None,
            end_date_time: None,
            transaction_id: None,
            display: Some(ComponentType {
                name: "MainDisplay".to_string(),
                instance: Some("instance1".to_string()),
                evse: None,
                custom_data: None,
            }),
            custom_data: None,
        },
        serde_json::json!({
            "id": 1,
            "priority": "AlwaysFront",
            "message": {
                "format": "ASCII",
                "content": "Important notice",
                "language": "en",
            },
            "state": "Charging",
            "display": { "name": "MainDisplay", "instance": "instance1" },
        }),
    );
}

/// Ports `test_certificate_hash_data_type`. All four fields required;
/// `hashAlgorithm` carries the `HashAlgorithmEnumType` wire string (`"SHA256"`).
/// Verified against `GetCertificateStatus.json` / `DeleteCertificate.json`'s
/// `CertificateHashDataType`.
#[test]
fn certificate_hash_data_type() {
    round_trip(
        CertificateHashDataType {
            hash_algorithm: HashAlgorithmEnumType::Sha256,
            issuer_name_hash: "issuer_hash".to_string(),
            issuer_key_hash: "key_hash".to_string(),
            serial_number: "serial123".to_string(),
            custom_data: None,
        },
        serde_json::json!({
            "hashAlgorithm": "SHA256",
            "issuerNameHash": "issuer_hash",
            "issuerKeyHash": "key_hash",
            "serialNumber": "serial123",
        }),
    );
}

/// Ports `test_certificate_hash_data_chain_type`. Pins the nested
/// `certificateHashData` and the `childCertificateHashData` array.
///
/// FINAL-schema-vs-Python-dataclass divergence: the reference passes
/// `certificate_type="V2G"`, which is **not** a member of
/// `GetCertificateIdUseEnumType` (the FINAL enum in
/// `GetInstalledCertificateIdsResponse.json` is
/// `V2GRootCertificate` / `MORootCertificate` / `CSMSRootCertificate` /
/// `V2GCertificateChain` / `ManufacturerRootCertificate`). This suite pins the
/// schema-valid `V2GRootCertificate`.
#[test]
fn certificate_hash_data_chain_type() {
    round_trip(
        CertificateHashDataChainType {
            certificate_type: GetCertificateIdUseEnumType::V2GRootCertificate,
            certificate_hash_data: CertificateHashDataType {
                hash_algorithm: HashAlgorithmEnumType::Sha256,
                issuer_name_hash: "issuer_hash".to_string(),
                issuer_key_hash: "key_hash".to_string(),
                serial_number: "serial123".to_string(),
                custom_data: None,
            },
            child_certificate_hash_data: Some(vec![CertificateHashDataType {
                hash_algorithm: HashAlgorithmEnumType::Sha256,
                issuer_name_hash: "child_issuer_hash".to_string(),
                issuer_key_hash: "child_key_hash".to_string(),
                serial_number: "child_serial123".to_string(),
                custom_data: None,
            }]),
            custom_data: None,
        },
        serde_json::json!({
            "certificateType": "V2GRootCertificate",
            "certificateHashData": {
                "hashAlgorithm": "SHA256",
                "issuerNameHash": "issuer_hash",
                "issuerKeyHash": "key_hash",
                "serialNumber": "serial123",
            },
            "childCertificateHashData": [
                {
                    "hashAlgorithm": "SHA256",
                    "issuerNameHash": "child_issuer_hash",
                    "issuerKeyHash": "child_key_hash",
                    "serialNumber": "child_serial123",
                }
            ],
        }),
    );
}

/// Ports `test_ocsp_request_data_type`. Note `responder_url` renames to the
/// upper-cased wire key `responderURL`. Verified against
/// `GetCertificateStatus.json`'s `OCSPRequestDataType`.
#[test]
fn ocsp_request_data_type() {
    round_trip(
        OCSPRequestDataType {
            hash_algorithm: HashAlgorithmEnumType::Sha256,
            issuer_name_hash: "issuer_hash_value".to_string(),
            issuer_key_hash: "issuer_key_hash_value".to_string(),
            serial_number: "serial123".to_string(),
            responder_url: "https://ocsp.example.com".to_string(),
            custom_data: None,
        },
        serde_json::json!({
            "hashAlgorithm": "SHA256",
            "issuerNameHash": "issuer_hash_value",
            "issuerKeyHash": "issuer_key_hash_value",
            "serialNumber": "serial123",
            "responderURL": "https://ocsp.example.com",
        }),
    );
}

/// Ports `test_log_parameters_type`. `oldestTimestamp` / `latestTimestamp` are
/// optional; the full and minimal (`remoteLocation` only) forms are pinned to
/// catch a dropped `skip_serializing_if`. Verified against `GetLog.json`.
#[test]
fn log_parameters_type() {
    round_trip(
        LogParametersType {
            remote_location: "https://logs.example.com".to_string(),
            oldest_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
            latest_timestamp: Some("2024-01-01T23:59:59Z".to_string()),
            custom_data: None,
        },
        serde_json::json!({
            "remoteLocation": "https://logs.example.com",
            "oldestTimestamp": "2024-01-01T00:00:00Z",
            "latestTimestamp": "2024-01-01T23:59:59Z",
        }),
    );

    // required-only form: both timestamps omitted.
    round_trip(
        LogParametersType {
            remote_location: "https://logs.example.com".to_string(),
            oldest_timestamp: None,
            latest_timestamp: None,
            custom_data: None,
        },
        serde_json::json!({ "remoteLocation": "https://logs.example.com" }),
    );
}

/// Ports `test_firmware_type`. `location` / `retrieveDateTime` are required;
/// `installDateTime` / `signingCertificate` / `signature` are optional. The full
/// and required-only forms are pinned. Verified against `PublishFirmware.json` /
/// `UpdateFirmware.json`'s `FirmwareType`.
#[test]
fn firmware_type() {
    round_trip(
        FirmwareType {
            location: "https://firmware.example.com/v1.2.3".to_string(),
            retrieve_date_time: "2024-01-01T10:00:00Z".to_string(),
            install_date_time: Some("2024-01-01T11:00:00Z".to_string()),
            signing_certificate: Some("MIIB...".to_string()),
            signature: Some("SHA256...".to_string()),
            custom_data: None,
        },
        serde_json::json!({
            "location": "https://firmware.example.com/v1.2.3",
            "retrieveDateTime": "2024-01-01T10:00:00Z",
            "installDateTime": "2024-01-01T11:00:00Z",
            "signingCertificate": "MIIB...",
            "signature": "SHA256...",
        }),
    );

    // required-only form: the three optional fields omitted.
    round_trip(
        FirmwareType {
            location: "https://firmware.example.com/v1.2.3".to_string(),
            retrieve_date_time: "2024-01-01T10:00:00Z".to_string(),
            install_date_time: None,
            signing_certificate: None,
            signature: None,
            custom_data: None,
        },
        serde_json::json!({
            "location": "https://firmware.example.com/v1.2.3",
            "retrieveDateTime": "2024-01-01T10:00:00Z",
        }),
    );
}

// --- Slice 3c: smart-charging *control* datatypes (#285) ---------------------
//
// The three datatypes that ride the smart-charging *command* messages rather
// than the tariff schedules slice 3a covered: the externally-imposed limit
// reported by `NotifyChargingLimit`, and the two profile-selection *filters*
// carried by `GetChargingProfiles` / `ClearChargingProfile`. Verified against
// `NotifyChargingLimit.json`, `GetChargingProfiles.json`, and
// `ClearChargingProfile.json`. The reference constructs schema-valid values for
// all three, so — unlike slices 2/3a/3b — no dataclass-vs-FINAL-schema
// divergence surfaced here; the structs, dataclasses, and schemas agree.

/// Ports `test_charging_limit_type`. `chargingLimitSource` (the origin of the
/// limit) is required; `isGridCritical` is optional. The enum's `Ems` /`So` /
/// `Cso` variants carry acronym renames (`"EMS"` / `"SO"` / `"CSO"`) that this
/// test pins. Verified against `NotifyChargingLimit.json`'s `ChargingLimitType`
/// (`required: ["chargingLimitSource"]`) and its `ChargingLimitSourceEnumType`
/// (`["EMS", "Other", "SO", "CSO"]`).
#[test]
fn charging_limit_type() {
    // The reference case: source "EMS" + is_grid_critical=True.
    round_trip(
        ChargingLimitType {
            charging_limit_source: ChargingLimitSourceEnumType::Ems,
            is_grid_critical: Some(true),
            custom_data: None,
        },
        serde_json::json!({
            "chargingLimitSource": "EMS",
            "isGridCritical": true,
        }),
    );

    // Required-only form: the optional `isGridCritical` omitted, and a plain
    // (un-renamed) `Other` source to pin that branch too.
    round_trip(
        ChargingLimitType {
            charging_limit_source: ChargingLimitSourceEnumType::Other,
            is_grid_critical: None,
            custom_data: None,
        },
        serde_json::json!({ "chargingLimitSource": "Other" }),
    );
}

/// Ports `test_charging_profile_criterion_type`. A *filter* for
/// `GetChargingProfiles`: every field is optional and an empty `{}` matches every
/// installed profile. Verified against `GetChargingProfiles.json`'s
/// `ChargingProfileCriterionType` — the two list fields have `minItems: 1` (and
/// `chargingLimitSource` additionally `maxItems: 4`), enforced at the schema
/// layer; here we pin the wire shape (camelCase names, nested arrays, enum
/// strings, and optional omission).
#[test]
fn charging_profile_criterion_type() {
    // The reference case: purpose + stack level + an id list (no source filter).
    round_trip(
        ChargingProfileCriterionType {
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            stack_level: Some(0),
            charging_profile_id: Some(vec![1, 2, 3]),
            charging_limit_source: None,
            custom_data: None,
        },
        serde_json::json!({
            "chargingProfilePurpose": "TxDefaultProfile",
            "stackLevel": 0,
            "chargingProfileId": [1, 2, 3],
        }),
    );

    // Full form: additionally pin the `chargingLimitSource` list-of-enum at its
    // `maxItems: 4` ceiling, which also exercises the three acronym renames
    // (`EMS` / `SO` / `CSO`) inside a nested array.
    round_trip(
        ChargingProfileCriterionType {
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxProfile),
            stack_level: Some(7),
            charging_profile_id: Some(vec![42]),
            charging_limit_source: Some(vec![
                ChargingLimitSourceEnumType::Ems,
                ChargingLimitSourceEnumType::So,
                ChargingLimitSourceEnumType::Cso,
                ChargingLimitSourceEnumType::Other,
            ]),
            custom_data: None,
        },
        serde_json::json!({
            "chargingProfilePurpose": "TxProfile",
            "stackLevel": 7,
            "chargingProfileId": [42],
            "chargingLimitSource": ["EMS", "SO", "CSO", "Other"],
        }),
    );

    // Empty criterion: every field optional → an empty wire object, matching
    // "report every installed profile".
    round_trip(
        ChargingProfileCriterionType {
            charging_profile_purpose: None,
            stack_level: None,
            charging_profile_id: None,
            charging_limit_source: None,
            custom_data: None,
        },
        serde_json::json!({}),
    );
}

/// Ports `test_clear_charging_profile_type`. A *filter* for
/// `ClearChargingProfile`: every field is optional and a profile is cleared only
/// if it matches all criteria present; an `evseId` of `0` targets the
/// station-wide profile and an empty `{}` clears every profile. Verified against
/// `ClearChargingProfile.json`'s `ClearChargingProfileType` (no `required` list).
#[test]
fn clear_charging_profile_type() {
    // The reference case: evse_id=1, purpose, stack level=0.
    round_trip(
        ClearChargingProfileType {
            evse_id: Some(1),
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            stack_level: Some(0),
            custom_data: None,
        },
        serde_json::json!({
            "evseId": 1,
            "chargingProfilePurpose": "TxDefaultProfile",
            "stackLevel": 0,
        }),
    );

    // Station-wide target: `evseId: 0` (the overall Charging Station), other
    // criteria omitted.
    round_trip(
        ClearChargingProfileType {
            evse_id: Some(0),
            charging_profile_purpose: None,
            stack_level: None,
            custom_data: None,
        },
        serde_json::json!({ "evseId": 0 }),
    );

    // Empty filter: every field optional → an empty wire object, matching "clear
    // every installed profile".
    round_trip(
        ClearChargingProfileType {
            evse_id: None,
            charging_profile_purpose: None,
            stack_level: None,
            custom_data: None,
        },
        serde_json::json!({}),
    );
}
