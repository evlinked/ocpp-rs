//! JSON Schema validation for OCPP 1.6J messages.
//!
//! Mirrors `get_validator()` + `_validate_payload()` from
//! `ocpp/messages.py` in mobilityhouse/ocpp: every CALL and CALLRESULT
//! payload is checked against the corresponding JSON Schema (Draft 4)
//! before dispatch, preventing malformed payloads from reaching handlers.

use std::collections::HashMap;

use jsonschema::error::ValidationErrorKind;
use ocpp_types::{OcppError, OcppResult, SchemaKeyword};
use serde_json::Value;

/// (schema-name, embedded JSON text) pairs for all 78 OCPP 1.6J schemas.
static SCHEMA_TEXTS_V16J: &[(&str, &str)] = &[
    ("Authorize", include_str!("../schemas/v16j/Authorize.json")),
    (
        "AuthorizeResponse",
        include_str!("../schemas/v16j/AuthorizeResponse.json"),
    ),
    (
        "BootNotification",
        include_str!("../schemas/v16j/BootNotification.json"),
    ),
    (
        "BootNotificationResponse",
        include_str!("../schemas/v16j/BootNotificationResponse.json"),
    ),
    (
        "CancelReservation",
        include_str!("../schemas/v16j/CancelReservation.json"),
    ),
    (
        "CancelReservationResponse",
        include_str!("../schemas/v16j/CancelReservationResponse.json"),
    ),
    (
        "CertificateSigned",
        include_str!("../schemas/v16j/CertificateSigned.json"),
    ),
    (
        "CertificateSignedResponse",
        include_str!("../schemas/v16j/CertificateSignedResponse.json"),
    ),
    (
        "ChangeAvailability",
        include_str!("../schemas/v16j/ChangeAvailability.json"),
    ),
    (
        "ChangeAvailabilityResponse",
        include_str!("../schemas/v16j/ChangeAvailabilityResponse.json"),
    ),
    (
        "ChangeConfiguration",
        include_str!("../schemas/v16j/ChangeConfiguration.json"),
    ),
    (
        "ChangeConfigurationResponse",
        include_str!("../schemas/v16j/ChangeConfigurationResponse.json"),
    ),
    (
        "ClearCache",
        include_str!("../schemas/v16j/ClearCache.json"),
    ),
    (
        "ClearCacheResponse",
        include_str!("../schemas/v16j/ClearCacheResponse.json"),
    ),
    (
        "ClearChargingProfile",
        include_str!("../schemas/v16j/ClearChargingProfile.json"),
    ),
    (
        "ClearChargingProfileResponse",
        include_str!("../schemas/v16j/ClearChargingProfileResponse.json"),
    ),
    (
        "DataTransfer",
        include_str!("../schemas/v16j/DataTransfer.json"),
    ),
    (
        "DataTransferResponse",
        include_str!("../schemas/v16j/DataTransferResponse.json"),
    ),
    (
        "DeleteCertificate",
        include_str!("../schemas/v16j/DeleteCertificate.json"),
    ),
    (
        "DeleteCertificateResponse",
        include_str!("../schemas/v16j/DeleteCertificateResponse.json"),
    ),
    (
        "DiagnosticsStatusNotification",
        include_str!("../schemas/v16j/DiagnosticsStatusNotification.json"),
    ),
    (
        "DiagnosticsStatusNotificationResponse",
        include_str!("../schemas/v16j/DiagnosticsStatusNotificationResponse.json"),
    ),
    (
        "ExtendedTriggerMessage",
        include_str!("../schemas/v16j/ExtendedTriggerMessage.json"),
    ),
    (
        "ExtendedTriggerMessageResponse",
        include_str!("../schemas/v16j/ExtendedTriggerMessageResponse.json"),
    ),
    (
        "FirmwareStatusNotification",
        include_str!("../schemas/v16j/FirmwareStatusNotification.json"),
    ),
    (
        "FirmwareStatusNotificationResponse",
        include_str!("../schemas/v16j/FirmwareStatusNotificationResponse.json"),
    ),
    (
        "GetCompositeSchedule",
        include_str!("../schemas/v16j/GetCompositeSchedule.json"),
    ),
    (
        "GetCompositeScheduleResponse",
        include_str!("../schemas/v16j/GetCompositeScheduleResponse.json"),
    ),
    (
        "GetConfiguration",
        include_str!("../schemas/v16j/GetConfiguration.json"),
    ),
    (
        "GetConfigurationResponse",
        include_str!("../schemas/v16j/GetConfigurationResponse.json"),
    ),
    (
        "GetDiagnostics",
        include_str!("../schemas/v16j/GetDiagnostics.json"),
    ),
    (
        "GetDiagnosticsResponse",
        include_str!("../schemas/v16j/GetDiagnosticsResponse.json"),
    ),
    (
        "GetInstalledCertificateIds",
        include_str!("../schemas/v16j/GetInstalledCertificateIds.json"),
    ),
    (
        "GetInstalledCertificateIdsResponse",
        include_str!("../schemas/v16j/GetInstalledCertificateIdsResponse.json"),
    ),
    (
        "GetLocalListVersion",
        include_str!("../schemas/v16j/GetLocalListVersion.json"),
    ),
    (
        "GetLocalListVersionResponse",
        include_str!("../schemas/v16j/GetLocalListVersionResponse.json"),
    ),
    ("GetLog", include_str!("../schemas/v16j/GetLog.json")),
    (
        "GetLogResponse",
        include_str!("../schemas/v16j/GetLogResponse.json"),
    ),
    ("Heartbeat", include_str!("../schemas/v16j/Heartbeat.json")),
    (
        "HeartbeatResponse",
        include_str!("../schemas/v16j/HeartbeatResponse.json"),
    ),
    (
        "InstallCertificate",
        include_str!("../schemas/v16j/InstallCertificate.json"),
    ),
    (
        "InstallCertificateResponse",
        include_str!("../schemas/v16j/InstallCertificateResponse.json"),
    ),
    (
        "LogStatusNotification",
        include_str!("../schemas/v16j/LogStatusNotification.json"),
    ),
    (
        "LogStatusNotificationResponse",
        include_str!("../schemas/v16j/LogStatusNotificationResponse.json"),
    ),
    (
        "MeterValues",
        include_str!("../schemas/v16j/MeterValues.json"),
    ),
    (
        "MeterValuesResponse",
        include_str!("../schemas/v16j/MeterValuesResponse.json"),
    ),
    (
        "RemoteStartTransaction",
        include_str!("../schemas/v16j/RemoteStartTransaction.json"),
    ),
    (
        "RemoteStartTransactionResponse",
        include_str!("../schemas/v16j/RemoteStartTransactionResponse.json"),
    ),
    (
        "RemoteStopTransaction",
        include_str!("../schemas/v16j/RemoteStopTransaction.json"),
    ),
    (
        "RemoteStopTransactionResponse",
        include_str!("../schemas/v16j/RemoteStopTransactionResponse.json"),
    ),
    (
        "ReserveNow",
        include_str!("../schemas/v16j/ReserveNow.json"),
    ),
    (
        "ReserveNowResponse",
        include_str!("../schemas/v16j/ReserveNowResponse.json"),
    ),
    ("Reset", include_str!("../schemas/v16j/Reset.json")),
    (
        "ResetResponse",
        include_str!("../schemas/v16j/ResetResponse.json"),
    ),
    (
        "SecurityEventNotification",
        include_str!("../schemas/v16j/SecurityEventNotification.json"),
    ),
    (
        "SecurityEventNotificationResponse",
        include_str!("../schemas/v16j/SecurityEventNotificationResponse.json"),
    ),
    (
        "SendLocalList",
        include_str!("../schemas/v16j/SendLocalList.json"),
    ),
    (
        "SendLocalListResponse",
        include_str!("../schemas/v16j/SendLocalListResponse.json"),
    ),
    (
        "SetChargingProfile",
        include_str!("../schemas/v16j/SetChargingProfile.json"),
    ),
    (
        "SetChargingProfileResponse",
        include_str!("../schemas/v16j/SetChargingProfileResponse.json"),
    ),
    (
        "SignCertificate",
        include_str!("../schemas/v16j/SignCertificate.json"),
    ),
    (
        "SignCertificateResponse",
        include_str!("../schemas/v16j/SignCertificateResponse.json"),
    ),
    (
        "SignedFirmwareStatusNotification",
        include_str!("../schemas/v16j/SignedFirmwareStatusNotification.json"),
    ),
    (
        "SignedFirmwareStatusNotificationResponse",
        include_str!("../schemas/v16j/SignedFirmwareStatusNotificationResponse.json"),
    ),
    (
        "SignedUpdateFirmware",
        include_str!("../schemas/v16j/SignedUpdateFirmware.json"),
    ),
    (
        "SignedUpdateFirmwareResponse",
        include_str!("../schemas/v16j/SignedUpdateFirmwareResponse.json"),
    ),
    (
        "StartTransaction",
        include_str!("../schemas/v16j/StartTransaction.json"),
    ),
    (
        "StartTransactionResponse",
        include_str!("../schemas/v16j/StartTransactionResponse.json"),
    ),
    (
        "StatusNotification",
        include_str!("../schemas/v16j/StatusNotification.json"),
    ),
    (
        "StatusNotificationResponse",
        include_str!("../schemas/v16j/StatusNotificationResponse.json"),
    ),
    (
        "StopTransaction",
        include_str!("../schemas/v16j/StopTransaction.json"),
    ),
    (
        "StopTransactionResponse",
        include_str!("../schemas/v16j/StopTransactionResponse.json"),
    ),
    (
        "TriggerMessage",
        include_str!("../schemas/v16j/TriggerMessage.json"),
    ),
    (
        "TriggerMessageResponse",
        include_str!("../schemas/v16j/TriggerMessageResponse.json"),
    ),
    (
        "UnlockConnector",
        include_str!("../schemas/v16j/UnlockConnector.json"),
    ),
    (
        "UnlockConnectorResponse",
        include_str!("../schemas/v16j/UnlockConnectorResponse.json"),
    ),
    (
        "UpdateFirmware",
        include_str!("../schemas/v16j/UpdateFirmware.json"),
    ),
    (
        "UpdateFirmwareResponse",
        include_str!("../schemas/v16j/UpdateFirmwareResponse.json"),
    ),
];

/// (schema-name, embedded JSON text) pairs for the bundled OCPP 2.0.1 schemas.
///
/// 2.0.1 schemas are JSON Schema **draft-06** (vs. draft-04 for 1.6J); the
/// draft is detected per-schema at validation time. Keyed by action name for
/// CALLs and `{action}Response` for CALLRESULTs, matching the v16j convention.
/// Grows as more 2.0.1 messages are ported (M7).
static SCHEMA_TEXTS_V201: &[(&str, &str)] = &[
    (
        "BootNotification",
        include_str!("../schemas/v201/BootNotification.json"),
    ),
    (
        "BootNotificationResponse",
        include_str!("../schemas/v201/BootNotificationResponse.json"),
    ),
    (
        "GetVariables",
        include_str!("../schemas/v201/GetVariables.json"),
    ),
    (
        "GetVariablesResponse",
        include_str!("../schemas/v201/GetVariablesResponse.json"),
    ),
    (
        "SetVariables",
        include_str!("../schemas/v201/SetVariables.json"),
    ),
    (
        "SetVariablesResponse",
        include_str!("../schemas/v201/SetVariablesResponse.json"),
    ),
    ("Heartbeat", include_str!("../schemas/v201/Heartbeat.json")),
    (
        "HeartbeatResponse",
        include_str!("../schemas/v201/HeartbeatResponse.json"),
    ),
    (
        "StatusNotification",
        include_str!("../schemas/v201/StatusNotification.json"),
    ),
    (
        "StatusNotificationResponse",
        include_str!("../schemas/v201/StatusNotificationResponse.json"),
    ),
    ("Authorize", include_str!("../schemas/v201/Authorize.json")),
    (
        "AuthorizeResponse",
        include_str!("../schemas/v201/AuthorizeResponse.json"),
    ),
    (
        "TransactionEvent",
        include_str!("../schemas/v201/TransactionEvent.json"),
    ),
    (
        "TransactionEventResponse",
        include_str!("../schemas/v201/TransactionEventResponse.json"),
    ),
    ("Reset", include_str!("../schemas/v201/Reset.json")),
    (
        "ResetResponse",
        include_str!("../schemas/v201/ResetResponse.json"),
    ),
    (
        "RequestStartTransaction",
        include_str!("../schemas/v201/RequestStartTransaction.json"),
    ),
    (
        "RequestStartTransactionResponse",
        include_str!("../schemas/v201/RequestStartTransactionResponse.json"),
    ),
    (
        "MeterValues",
        include_str!("../schemas/v201/MeterValues.json"),
    ),
    (
        "MeterValuesResponse",
        include_str!("../schemas/v201/MeterValuesResponse.json"),
    ),
    (
        "RequestStopTransaction",
        include_str!("../schemas/v201/RequestStopTransaction.json"),
    ),
    (
        "RequestStopTransactionResponse",
        include_str!("../schemas/v201/RequestStopTransactionResponse.json"),
    ),
    (
        "ClearCache",
        include_str!("../schemas/v201/ClearCache.json"),
    ),
    (
        "ClearCacheResponse",
        include_str!("../schemas/v201/ClearCacheResponse.json"),
    ),
    (
        "GetLocalListVersion",
        include_str!("../schemas/v201/GetLocalListVersion.json"),
    ),
    (
        "GetLocalListVersionResponse",
        include_str!("../schemas/v201/GetLocalListVersionResponse.json"),
    ),
    (
        "ChangeAvailability",
        include_str!("../schemas/v201/ChangeAvailability.json"),
    ),
    (
        "ChangeAvailabilityResponse",
        include_str!("../schemas/v201/ChangeAvailabilityResponse.json"),
    ),
    (
        "UnlockConnector",
        include_str!("../schemas/v201/UnlockConnector.json"),
    ),
    (
        "UnlockConnectorResponse",
        include_str!("../schemas/v201/UnlockConnectorResponse.json"),
    ),
    (
        "TriggerMessage",
        include_str!("../schemas/v201/TriggerMessage.json"),
    ),
    (
        "TriggerMessageResponse",
        include_str!("../schemas/v201/TriggerMessageResponse.json"),
    ),
    (
        "DataTransfer",
        include_str!("../schemas/v201/DataTransfer.json"),
    ),
    (
        "DataTransferResponse",
        include_str!("../schemas/v201/DataTransferResponse.json"),
    ),
    (
        "ReserveNow",
        include_str!("../schemas/v201/ReserveNow.json"),
    ),
    (
        "ReserveNowResponse",
        include_str!("../schemas/v201/ReserveNowResponse.json"),
    ),
    (
        "CancelReservation",
        include_str!("../schemas/v201/CancelReservation.json"),
    ),
    (
        "CancelReservationResponse",
        include_str!("../schemas/v201/CancelReservationResponse.json"),
    ),
    (
        "SendLocalList",
        include_str!("../schemas/v201/SendLocalList.json"),
    ),
    (
        "SendLocalListResponse",
        include_str!("../schemas/v201/SendLocalListResponse.json"),
    ),
];

/// Validates CALL and CALLRESULT payloads against the bundled OCPP 1.6J
/// JSON Schemas (Draft 4).
///
/// ## Python reference
/// `ocpp/messages.py` — `get_validator()` + `_validate_payload()`
/// `ocpp/v16/schemas/*.json` — 78 schema files (39 action pairs)
///
/// ## Usage
/// ```rust,no_run
/// use ocpp_messages::schema_validation::SchemaValidator;
/// use serde_json::json;
///
/// let validator = SchemaValidator::v16j();
/// let payload = json!({
///     "chargePointVendor": "ACME",
///     "chargePointModel": "Turbo-3000"
/// });
/// assert!(validator.validate_call("BootNotification", &payload).is_ok());
/// ```
pub struct SchemaValidator {
    /// action-name → parsed JSON Schema value
    schemas: HashMap<String, Value>,
}

impl SchemaValidator {
    /// Build a validator pre-loaded with all bundled OCPP 1.6J schemas (78
    /// files, 39 action pairs).
    ///
    /// Schemas are embedded at compile time via `include_str!` and parsed
    /// once on construction.  Call this once and share the result (e.g. via
    /// `Arc`) — construction is cheap but not free.
    pub fn v16j() -> Self {
        let mut schemas = HashMap::with_capacity(SCHEMA_TEXTS_V16J.len());
        for (name, text) in SCHEMA_TEXTS_V16J {
            let value: Value =
                serde_json::from_str(text).expect("bundled OCPP 1.6J schema is valid JSON");
            schemas.insert((*name).to_string(), value);
        }
        Self { schemas }
    }

    /// Build a validator pre-loaded with the bundled OCPP 2.0.1 schemas.
    ///
    /// 2.0.1 schemas are JSON Schema draft-06; `run_validation` detects
    /// the draft per-schema, so a `v201()` validator and a [`Self::v16j()`]
    /// validator can coexist without interfering. Currently carries the
    /// `BootNotification`, `Heartbeat`, `StatusNotification`, and
    /// `GetVariables` schemas (M7); grows as more messages land.
    pub fn v201() -> Self {
        let mut schemas = HashMap::with_capacity(SCHEMA_TEXTS_V201.len());
        for (name, text) in SCHEMA_TEXTS_V201 {
            let value: Value =
                serde_json::from_str(text).expect("bundled OCPP 2.0.1 schema is valid JSON");
            schemas.insert((*name).to_string(), value);
        }
        Self { schemas }
    }

    /// Validate a CALL payload against the schema for `action`.
    ///
    /// - Returns `Ok(())` if `payload` satisfies the JSON Schema.
    /// - Returns `Err(OcppError::ValidationError)` on schema violation.
    /// - Returns `Err(OcppError::NotSupported)` if no schema exists for the
    ///   action (unknown action name).
    pub fn validate_call(&self, action: &str, payload: &Value) -> OcppResult<()> {
        let schema = self.get_schema(action)?;
        Self::run_validation(schema, payload)
    }

    /// Validate a CALLRESULT payload against the `{action}Response` schema.
    ///
    /// Same error semantics as [`Self::validate_call`].
    pub fn validate_call_result(&self, action: &str, payload: &Value) -> OcppResult<()> {
        let key = format!("{}Response", action);
        let schema = self.get_schema(&key)?;
        Self::run_validation(schema, payload)
    }

    /// Number of loaded schemas.  For OCPP 1.6J this is always 78.
    pub fn schema_count(&self) -> usize {
        self.schemas.len()
    }

    /// Returns `true` if a CALL schema exists for `action`.
    pub fn has_schema(&self, action: &str) -> bool {
        self.schemas.contains_key(action)
    }

    fn get_schema(&self, key: &str) -> OcppResult<&Value> {
        self.schemas
            .get(key)
            .ok_or_else(|| OcppError::NotSupported {
                feature: format!("JSON schema for '{}'", key),
            })
    }

    /// Pick the JSON Schema draft to compile a bundled schema under.
    ///
    /// OCPP 1.6J schemas declare draft-04 and OCPP 2.0.1 schemas declare
    /// draft-06; we honor the schema's own `$schema` so both validators behave
    /// faithfully. Anything unrecognized (or absent) defaults to draft-04,
    /// preserving the original 1.6J behavior exactly.
    fn draft_for(schema: &Value) -> jsonschema::Draft {
        match schema.get("$schema").and_then(Value::as_str) {
            Some(s) if s.contains("draft-07") => jsonschema::Draft::Draft7,
            Some(s) if s.contains("draft-06") => jsonschema::Draft::Draft6,
            _ => jsonschema::Draft::Draft4,
        }
    }

    fn run_validation(schema: &Value, payload: &Value) -> OcppResult<()> {
        let compiled = jsonschema::JSONSchema::options()
            .with_draft(Self::draft_for(schema))
            .compile(schema)
            .map_err(|e| OcppError::Internal {
                message: format!("failed to compile bundled schema: {}", e),
            })?;

        if let Err(errors) = compiled.validate(payload) {
            // Drop `multipleOf` errors that are pure f64 representation
            // artifacts (e.g. `21.4` against `multipleOf: 0.1`). Every other
            // keyword violation is kept. This is the Rust equivalent of the
            // `decimal.Decimal` re-parse in `ocpp/messages.py::_validate_payload`.
            //
            // While collecting messages we also track the dominant failing
            // keyword so the CALLERROR layer can pick a keyword-granular code
            // (`type`/`maxLength` → TypeConstraintViolation, `required` →
            // ProtocolError, else → FormationViolation). The Python reference
            // inspects only the *first* `jsonschema` error; `jsonschema`'s
            // iteration order isn't a stable contract, so instead we pick a
            // deterministic, documented priority across all surviving errors:
            // `required > type > maxLength > additionalProperties > other`.
            let mut messages: Vec<String> = Vec::new();
            let mut keyword: Option<SchemaKeyword> = None;
            for error in errors {
                if is_multiple_of_precision_artifact(&error) {
                    continue;
                }
                let candidate = classify_keyword(&error);
                keyword = Some(match keyword {
                    Some(current) if keyword_priority(current) >= keyword_priority(candidate) => {
                        current
                    }
                    _ => candidate,
                });
                messages.push(error.to_string());
            }
            if let Some(keyword) = keyword {
                return Err(OcppError::SchemaViolation {
                    keyword,
                    message: messages.join("; "),
                });
            }
        }
        Ok(())
    }
}

/// Collapse a `jsonschema` validation error to the single OCPP-relevant keyword
/// the CALLERROR mapping cares about. Keywords with no dedicated OCPP code
/// (`enum`, `minimum`, `multipleOf`, …) fall into [`SchemaKeyword::Other`],
/// matching the default `FormatViolationError` bucket of the Python reference.
fn classify_keyword(error: &jsonschema::ValidationError) -> SchemaKeyword {
    match &error.kind {
        ValidationErrorKind::Type { .. } => SchemaKeyword::Type,
        ValidationErrorKind::MaxLength { .. } => SchemaKeyword::MaxLength,
        ValidationErrorKind::AdditionalProperties { .. } => SchemaKeyword::AdditionalProperties,
        ValidationErrorKind::Required { .. } => SchemaKeyword::Required,
        _ => SchemaKeyword::Other,
    }
}

/// Deterministic precedence for choosing one keyword when a payload trips
/// several at once. Higher value wins, in this order: `type` (4), `maxLength`
/// (3), `additionalProperties` (2), `required` (1), `other` (0).
///
/// A concrete violation on a *present* field (`type`/`maxLength`/an unexpected
/// property) is more informative than the generic "a field is missing"
/// (`required`), which almost any partial payload trips. This deprioritisation
/// of `required` is exactly what the Python reference's tests expect: e.g.
/// `test_validate_set_maxlength_violation_payload` sends an over-long `idTag` in
/// a payload that is *also* missing required fields, yet expects
/// `TypeConstraintViolation` (maxLength) rather than `ProtocolError` (required).
fn keyword_priority(keyword: SchemaKeyword) -> u8 {
    match keyword {
        SchemaKeyword::Type => 4,
        SchemaKeyword::MaxLength => 3,
        SchemaKeyword::AdditionalProperties => 2,
        SchemaKeyword::Required => 1,
        SchemaKeyword::Other => 0,
    }
}

/// Returns `true` when `error` is a `multipleOf` violation that only fails
/// because of f64 representation error — i.e. the value really *is* an exact
/// multiple of the divisor when judged as a base-10 decimal.
///
/// `jsonschema` 0.17's `MultipleOfFloatValidator` operates on raw f64 bit
/// values, so `21.4` (stored as `21.39999999999999857…`) is rejected against
/// `multipleOf: 0.1` even though `21.4` is a valid one-decimal value. The
/// Python reference dodges this by re-parsing both schema and payload with
/// `decimal.Decimal` ([`ocpp/messages.py::_validate_payload`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/messages.py));
/// we reach the same verdict by comparing the *shortest decimal
/// representations* of the two numbers exactly.
///
/// Only `multipleOf` errors are ever forgiven; `type`, `required`,
/// `additionalProperties`, `enum`, `minimum`, … are untouched. Anything we
/// cannot prove is an exact multiple stays a genuine violation (fail-safe).
fn is_multiple_of_precision_artifact(error: &jsonschema::ValidationError) -> bool {
    if let ValidationErrorKind::MultipleOf { multiple_of } = &error.kind {
        if let Some(value) = error.instance.as_f64() {
            return is_exact_decimal_multiple(value, *multiple_of);
        }
    }
    false
}

/// Exact "is `value` an integer multiple of `divisor`?", decided on the
/// shortest round-tripping decimal representation of each f64 (the digits a
/// human actually wrote in the JSON) via integer mantissa/scale arithmetic.
/// Mirrors `decimal.Decimal` semantics from the Python reference.
///
/// Returns `false` if either number can't be expressed as an exact
/// `mantissa × 10⁻ˢᶜᵃˡᵉ` (exponent-form output, `i128` overflow, or a zero
/// divisor) so callers never accept a payload that cannot be proven valid.
fn is_exact_decimal_multiple(value: f64, divisor: f64) -> bool {
    let (vm, vs) = match decimal_mantissa_scale(value) {
        Some(t) => t,
        None => return false,
    };
    let (dm, ds) = match decimal_mantissa_scale(divisor) {
        Some(t) => t,
        None => return false,
    };
    if dm == 0 {
        return false;
    }
    // value / divisor = (vm / 10^vs) / (dm / 10^ds)
    //                 = (vm * 10^ds) / (dm * 10^vs)
    // which is an integer iff numerator % denominator == 0.
    let num = match 10i128.checked_pow(ds).and_then(|p| vm.checked_mul(p)) {
        Some(n) => n,
        None => return false,
    };
    let den = match 10i128.checked_pow(vs).and_then(|p| dm.checked_mul(p)) {
        Some(d) => d,
        None => return false,
    };
    den != 0 && num % den == 0
}

/// Decompose an f64 into `(mantissa, scale)` such that the value equals
/// `mantissa as f64 / 10_f64.powi(scale as i32)`, using the shortest decimal
/// representation produced by `f64::to_string` (ryū). Returns `None` for
/// exponent-form output or magnitudes that don't fit in `i128`.
fn decimal_mantissa_scale(x: f64) -> Option<(i128, u32)> {
    if !x.is_finite() {
        return None;
    }
    let s = x.to_string();
    // ryū uses plain decimal notation across the magnitudes OCPP limit fields
    // use; if it ever emits exponent form, bail rather than mis-parse.
    if s.contains(['e', 'E']) {
        return None;
    }
    let (digits, scale) = match s.split_once('.') {
        Some((int_part, frac_part)) => (format!("{int_part}{frac_part}"), frac_part.len() as u32),
        None => (s, 0),
    };
    digits.parse::<i128>().ok().map(|m| (m, scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::CallErrorCode;
    use serde_json::json;

    // ── construction ──────────────────────────────────────────────────────────

    #[test]
    fn v16j_loads_all_78_schemas() {
        let v = SchemaValidator::v16j();
        // Derive the expected count from the registry rather than a hard-coded
        // literal: the count then tracks `SCHEMA_TEXTS_V16J` automatically and a
        // new schema never needs to bump this assertion. A mismatch here also
        // catches an accidental duplicate action key (HashMap would dedupe it).
        assert_eq!(v.schema_count(), SCHEMA_TEXTS_V16J.len());
    }

    #[test]
    fn v16j_has_schema_for_all_actions() {
        let v = SchemaValidator::v16j();
        // spot-check a selection of actions
        for action in [
            "BootNotification",
            "Heartbeat",
            "Authorize",
            "StartTransaction",
            "StopTransaction",
            "StatusNotification",
            "MeterValues",
            "SetChargingProfile",
            "GetCompositeSchedule",
            "Reset",
        ] {
            assert!(v.has_schema(action), "missing schema for {}", action);
            let response = format!("{}Response", action);
            assert!(v.has_schema(&response), "missing schema for {}", response);
        }
    }

    // ── valid payloads pass ───────────────────────────────────────────────────

    /// Port of test_validate_payload_with_valid_payload (Python reference)
    #[test]
    fn validate_heartbeat_response_valid_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({"currentTime": "2022-01-25T19:18:30.018Z"});
        assert!(v.validate_call_result("Heartbeat", &payload).is_ok());
    }

    #[test]
    fn validate_heartbeat_call_empty_object_passes() {
        let v = SchemaValidator::v16j();
        assert!(v.validate_call("Heartbeat", &json!({})).is_ok());
    }

    #[test]
    fn validate_boot_notification_call_required_fields_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "chargePointVendor": "ACME",
            "chargePointModel": "Turbo-3000"
        });
        assert!(v.validate_call("BootNotification", &payload).is_ok());
    }

    #[test]
    fn validate_boot_notification_call_all_optional_fields_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "chargePointVendor": "ACME",
            "chargePointModel": "Turbo-3000",
            "chargePointSerialNumber": "SN-001",
            "chargeBoxSerialNumber": "CB-001",
            "firmwareVersion": "1.2.3",
            "iccid": "12345678901234567890",
            "imsi": "123456789012345",
            "meterType": "SomeMeter",
            "meterSerialNumber": "MTR-001"
        });
        assert!(v.validate_call("BootNotification", &payload).is_ok());
    }

    #[test]
    fn validate_boot_notification_response_accepted_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "currentTime": "2022-01-25T19:18:30.018Z",
            "interval": 300,
            "status": "Accepted"
        });
        assert!(v.validate_call_result("BootNotification", &payload).is_ok());
    }

    #[test]
    fn validate_authorize_call_valid_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({"idTag": "TAG123"});
        assert!(v.validate_call("Authorize", &payload).is_ok());
    }

    #[test]
    fn validate_start_transaction_call_valid_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "idTag": "okTag",
            "meterStart": 12345,
            "timestamp": "2022-01-25T19:18:30.018Z"
        });
        assert!(v.validate_call("StartTransaction", &payload).is_ok());
    }

    #[test]
    fn validate_reset_call_hard_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({"type": "Hard"});
        assert!(v.validate_call("Reset", &payload).is_ok());
    }

    /// Port of `test_validate_set_charging_profile_payload`
    /// ([`tests/test_messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py)).
    ///
    /// Uses the reference value `21.4`. As f64 that is `21.39999999999999857…`,
    /// so `jsonschema` 0.17's `multipleOf: 0.1` check rejects it outright;
    /// `is_multiple_of_precision_artifact` forgives the false positive, the way
    /// the Python reference re-parses with `decimal.Decimal`.
    #[test]
    fn validate_set_charging_profile_with_decimal_limit_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "csChargingProfiles": {
                "chargingProfileId": 1,
                "stackLevel": 0,
                "chargingProfilePurpose": "TxProfile",
                "chargingProfileKind": "Relative",
                "chargingSchedule": {
                    "chargingRateUnit": "A",
                    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 21.4}]
                },
                "transactionId": 123456789
            }
        });
        assert!(v.validate_call("SetChargingProfile", &payload).is_ok());
    }

    /// Port of `test_validate_get_composite_profile_payload`
    /// ([`tests/test_messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py)),
    /// using the reference value `15.2` (f64 `15.19999999999999857…`).
    #[test]
    fn validate_get_composite_schedule_response_with_decimal_limit_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "status": "Accepted",
            "connectorId": 1,
            "scheduleStart": "2021-06-15T14:01:32Z",
            "chargingSchedule": {
                "duration": 60,
                "chargingRateUnit": "A",
                "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 15.2}]
            }
        });
        assert!(v
            .validate_call_result("GetCompositeSchedule", &payload)
            .is_ok());
    }

    /// `RemoteStartTransaction` is the third schema carrying `multipleOf: 0.1`
    /// (on `chargingProfile.chargingSchedule.chargingSchedulePeriod[*].limit`);
    /// the Python reference lists it next to `SetChargingProfile` in
    /// `_validate_payload`. A one-decimal limit must validate here too.
    #[test]
    fn validate_remote_start_transaction_with_decimal_limit_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "idTag": "ABC123",
            "chargingProfile": {
                "chargingProfileId": 1,
                "stackLevel": 0,
                "chargingProfilePurpose": "TxProfile",
                "chargingProfileKind": "Relative",
                "chargingSchedule": {
                    "chargingRateUnit": "A",
                    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 21.4}]
                }
            }
        });
        assert!(v.validate_call("RemoteStartTransaction", &payload).is_ok());
    }

    /// A genuine `multipleOf: 0.1` violation (more than one decimal) must STILL
    /// be rejected — the precision filter only forgives exact decimals. `4.11`
    /// is the Python reference's canonical invalid value.
    #[test]
    fn validate_set_charging_profile_rejects_two_decimal_limit() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "csChargingProfiles": {
                "chargingProfileId": 1,
                "stackLevel": 0,
                "chargingProfilePurpose": "TxProfile",
                "chargingProfileKind": "Relative",
                "chargingSchedule": {
                    "chargingRateUnit": "A",
                    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 4.11}]
                },
                "transactionId": 123456789
            }
        });
        let err = v.validate_call("SetChargingProfile", &payload).unwrap_err();
        // A genuine `multipleOf` failure has no dedicated OCPP code → `Other`
        // (the Python reference's default `FormatViolationError` bucket).
        assert!(
            matches!(
                err,
                OcppError::SchemaViolation {
                    keyword: SchemaKeyword::Other,
                    ..
                }
            ),
            "expected SchemaViolation(Other) for 4.11, got {err:?}"
        );
    }

    /// The precision filter must not swallow *other* errors in the same
    /// payload: a forgivable `21.4` limit next to a genuine type violation
    /// (`connectorId` as a string) still fails validation.
    #[test]
    fn precision_filter_does_not_mask_sibling_errors() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": "not-an-integer",
            "csChargingProfiles": {
                "chargingProfileId": 1,
                "stackLevel": 0,
                "chargingProfilePurpose": "TxProfile",
                "chargingProfileKind": "Relative",
                "chargingSchedule": {
                    "chargingRateUnit": "A",
                    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 21.4}]
                },
                "transactionId": 123456789
            }
        });
        let err = v.validate_call("SetChargingProfile", &payload).unwrap_err();
        // A `type` failure survives → keyword is `Type`, never swallowed.
        assert!(
            matches!(
                err,
                OcppError::SchemaViolation {
                    keyword: SchemaKeyword::Type,
                    ..
                }
            ),
            "sibling type error must survive the multipleOf filter, got {err:?}"
        );
    }

    // ── invalid payloads are rejected, with keyword-granular codes ────────────
    //
    // These port the per-keyword cases from the Python reference's
    // `tests/test_messages.py` (`test_validate_payload_with_invalid_*`) and
    // assert the exact `SchemaKeyword` → `CallErrorCode` mapping from
    // `_validate_payload()`.

    /// Port of `test_validate_payload_with_invalid_additional_properties_payload`.
    /// `additionalProperties` → `FormationViolation`.
    #[test]
    fn validate_heartbeat_response_with_extra_key_fails() {
        let v = SchemaValidator::v16j();
        let payload = json!({"invalid_key": true});
        let err = v.validate_call_result("Heartbeat", &payload).unwrap_err();
        match err {
            OcppError::SchemaViolation { keyword, .. } => {
                assert_eq!(keyword, SchemaKeyword::AdditionalProperties);
                assert_eq!(keyword.call_error_code(), CallErrorCode::FormationViolation);
            }
            other => panic!("expected SchemaViolation(AdditionalProperties), got {other:?}"),
        }
    }

    /// Port of `test_validate_payload_with_invalid_type_payload`.
    /// `type` → `TypeConstraintViolation`.
    #[test]
    fn validate_start_transaction_with_wrong_type_fails() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "idTag": "okTag",
            "meterStart": "invalid_type", // should be integer
            "timestamp": "2022-01-25T19:18:30.018Z"
        });
        let err = v.validate_call("StartTransaction", &payload).unwrap_err();
        match err {
            OcppError::SchemaViolation { keyword, .. } => {
                assert_eq!(keyword, SchemaKeyword::Type);
                assert_eq!(
                    keyword.call_error_code(),
                    CallErrorCode::TypeConstraintViolation
                );
            }
            other => panic!("expected SchemaViolation(Type), got {other:?}"),
        }
    }

    /// Port of `test_validate_payload_with_invalid_missing_property_payload`.
    /// `required` → `ProtocolError`.
    #[test]
    fn validate_start_transaction_missing_required_field_fails() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "idTag": "okTag"
            // meterStart and timestamp are required but omitted
        });
        let err = v.validate_call("StartTransaction", &payload).unwrap_err();
        match err {
            OcppError::SchemaViolation { keyword, .. } => {
                assert_eq!(keyword, SchemaKeyword::Required);
                assert_eq!(keyword.call_error_code(), CallErrorCode::ProtocolError);
            }
            other => panic!("expected SchemaViolation(Required), got {other:?}"),
        }
    }

    #[test]
    fn validate_boot_notification_missing_required_vendor_fails() {
        let v = SchemaValidator::v16j();
        // chargePointModel present but chargePointVendor missing
        let payload = json!({"chargePointModel": "Turbo-3000"});
        let err = v.validate_call("BootNotification", &payload).unwrap_err();
        assert!(matches!(
            err,
            OcppError::SchemaViolation {
                keyword: SchemaKeyword::Required,
                ..
            }
        ));
    }

    /// `maxLength` → `TypeConstraintViolation` (an `idTag` longer than the
    /// schema's 20-char limit). This keyword shares the Python reference's
    /// `TypeConstraintViolationError` branch with `type`.
    #[test]
    fn validate_authorize_with_overlong_id_tag_fails_max_length() {
        let v = SchemaValidator::v16j();
        let payload = json!({"idTag": "X".repeat(21)}); // CiString20Type → maxLength 20
        let err = v.validate_call("Authorize", &payload).unwrap_err();
        match err {
            OcppError::SchemaViolation { keyword, .. } => {
                assert_eq!(keyword, SchemaKeyword::MaxLength);
                assert_eq!(
                    keyword.call_error_code(),
                    CallErrorCode::TypeConstraintViolation
                );
            }
            other => panic!("expected SchemaViolation(MaxLength), got {other:?}"),
        }
    }

    /// An `enum` failure has no dedicated OCPP code → `Other` →
    /// `FormationViolation` (the default `FormatViolationError` bucket).
    #[test]
    fn validate_reset_with_unknown_type_enum_fails() {
        let v = SchemaValidator::v16j();
        // "Warm" is not a valid ResetType (only "Hard" and "Soft" are)
        let payload = json!({"type": "Warm"});
        let err = v.validate_call("Reset", &payload).unwrap_err();
        match err {
            OcppError::SchemaViolation { keyword, .. } => {
                assert_eq!(keyword, SchemaKeyword::Other);
                assert_eq!(keyword.call_error_code(), CallErrorCode::FormationViolation);
            }
            other => panic!("expected SchemaViolation(Other) for enum failure, got {other:?}"),
        }
    }

    /// Port of `test_validate_set_maxlength_violation_payload`
    /// ([`tests/test_messages.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_messages.py)).
    /// The payload trips both `maxLength` (21-char `idTag`) and `required`
    /// (missing `meterStart`/`timestamp`); the reference expects
    /// `TypeConstraintViolation`, so our precedence must report `maxLength` over
    /// `required`.
    #[test]
    fn multiple_failures_maxlength_outranks_required() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "idTag": "012345678901234567890", // 21 chars → maxLength
            "connectorId": 1
            // meterStart + timestamp missing → required
        });
        let err = v.validate_call("StartTransaction", &payload).unwrap_err();
        match err {
            OcppError::SchemaViolation { keyword, .. } => {
                assert_eq!(keyword, SchemaKeyword::MaxLength);
                assert_eq!(
                    keyword.call_error_code(),
                    CallErrorCode::TypeConstraintViolation
                );
            }
            other => panic!("maxLength must outrank required, got {other:?}"),
        }
    }

    // ── unknown actions ───────────────────────────────────────────────────────

    /// Port of test_validate_payload_with_non_existing_schema
    #[test]
    fn validate_call_unknown_action_returns_not_supported() {
        let v = SchemaValidator::v16j();
        let err = v.validate_call("MagicSpell", &json!({})).unwrap_err();
        assert!(
            matches!(err, OcppError::NotSupported { .. }),
            "expected NotSupported, got {:?}",
            err
        );
    }

    #[test]
    fn validate_call_result_unknown_action_returns_not_supported() {
        let v = SchemaValidator::v16j();
        let err = v
            .validate_call_result("MagicSpell", &json!({}))
            .unwrap_err();
        assert!(matches!(err, OcppError::NotSupported { .. }));
    }

    // ── port fidelity: absent optionals omitted, not serialized as null ───────

    /// The Python reference's `remove_nones()` drops `None` fields before
    /// validation; our serde structs must do the same, or a `null` value fails
    /// the action schema (e.g. `key: null` violates `key: array`). Regression
    /// guard for the `skip_serializing_if` attributes on optional v16j fields.
    #[test]
    fn absent_optionals_are_omitted_and_payloads_validate() {
        use crate::v16j::{GetConfigurationRequest, StatusNotificationRequest};
        use ocpp_types::v16j::{ChargePointErrorCode, ChargePointStatus};

        let v = SchemaValidator::v16j();

        // `GetConfigurationRequest { key: None }` → `{}`, not `{"key": null}`.
        let payload = serde_json::to_value(GetConfigurationRequest { key: None }).unwrap();
        assert_eq!(payload, json!({}));
        assert!(v.validate_call("GetConfiguration", &payload).is_ok());

        // A StatusNotification with no optional fields set must validate.
        let payload = serde_json::to_value(StatusNotificationRequest {
            connector_id: 1,
            error_code: ChargePointErrorCode::NoError,
            status: ChargePointStatus::Available,
            info: None,
            timestamp: None,
            vendor_error_code: None,
            vendor_id: None,
        })
        .unwrap();
        assert!(payload.get("info").is_none(), "info must be omitted");
        assert!(
            payload.get("timestamp").is_none(),
            "timestamp must be omitted"
        );
        assert!(v.validate_call("StatusNotification", &payload).is_ok());
    }

    // ── error message content ─────────────────────────────────────────────────

    #[test]
    fn validation_error_contains_useful_message() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "idTag": "okTag",
            "meterStart": "not_a_number",
            "timestamp": "2022-01-25T19:18:30.018Z"
        });
        match v.validate_call("StartTransaction", &payload) {
            Err(OcppError::SchemaViolation { message, .. }) => {
                assert!(
                    !message.is_empty(),
                    "validation error message must not be empty"
                );
            }
            other => panic!("expected SchemaViolation, got {:?}", other),
        }
    }

    // ── multipleOf precision helpers (unit) ───────────────────────────────────

    #[test]
    fn exact_decimal_multiple_accepts_one_decimal_values() {
        // Values that are exact multiples of 0.1 when read as base-10 decimals,
        // even though several are inexact as f64 (21.4, 15.2, 6.6, 999.9).
        for v in [21.4_f64, 15.2, 6.6, 0.1, 21.0, 0.0, 214.0, 999.9] {
            assert!(
                is_exact_decimal_multiple(v, 0.1),
                "{v} should be an exact multiple of 0.1"
            );
        }
    }

    #[test]
    fn exact_decimal_multiple_rejects_finer_precision() {
        // More than one decimal place is a genuine multipleOf: 0.1 violation.
        for v in [4.11_f64, 21.45, 21.401, 0.01, 100.001] {
            assert!(
                !is_exact_decimal_multiple(v, 0.1),
                "{v} should NOT be an exact multiple of 0.1"
            );
        }
    }

    #[test]
    fn exact_decimal_multiple_handles_integer_divisors_and_zero() {
        assert!(is_exact_decimal_multiple(6.0, 2.0));
        assert!(!is_exact_decimal_multiple(5.0, 2.0));
        assert!(is_exact_decimal_multiple(-21.4, 0.1)); // sign-agnostic
        assert!(!is_exact_decimal_multiple(5.0, 0.0)); // zero divisor → false
    }

    #[test]
    fn decimal_mantissa_scale_decomposes_shortest_repr() {
        assert_eq!(decimal_mantissa_scale(21.4), Some((214, 1)));
        assert_eq!(decimal_mantissa_scale(0.1), Some((1, 1)));
        assert_eq!(decimal_mantissa_scale(21.0), Some((21, 0)));
        assert_eq!(decimal_mantissa_scale(-5.5), Some((-55, 1)));
        assert_eq!(decimal_mantissa_scale(f64::NAN), None);
        assert_eq!(decimal_mantissa_scale(f64::INFINITY), None);
    }

    // ---- OCPP 2.0.1 (draft-06) schema validation -------------------------

    #[test]
    fn v201_loads_bundled_boot_notification_schemas() {
        let v = SchemaValidator::v201();
        // Derive from the registry length so adding a 2.0.1 message never
        // requires bumping a hard-coded count literal here — this assertion was
        // the guaranteed textual conflict between stacked v201 PRs (see #124).
        // The `has_schema` checks below still pin the specific actions present.
        assert_eq!(v.schema_count(), SCHEMA_TEXTS_V201.len());
        assert!(v.has_schema("BootNotification"));
        assert!(v.has_schema("BootNotificationResponse"));
        assert!(v.has_schema("GetVariables"));
        assert!(v.has_schema("GetVariablesResponse"));
        assert!(v.has_schema("Heartbeat"));
        assert!(v.has_schema("HeartbeatResponse"));
        assert!(v.has_schema("StatusNotification"));
        assert!(v.has_schema("StatusNotificationResponse"));
        assert!(v.has_schema("Authorize"));
        assert!(v.has_schema("AuthorizeResponse"));
        assert!(v.has_schema("TransactionEvent"));
        assert!(v.has_schema("TransactionEventResponse"));
        assert!(v.has_schema("Reset"));
        assert!(v.has_schema("ResetResponse"));
    }

    #[test]
    fn v201_authorize_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        // Reference: tests/v201/conftest.py.
        let call = json!({
            "idToken": { "idToken": "045918E24B6D80", "type": "ISO14443" }
        });
        assert!(v.validate_call("Authorize", &call).is_ok());
        let result = json!({ "idTokenInfo": { "status": "Accepted" } });
        assert!(v.validate_call_result("Authorize", &result).is_ok());
    }

    #[test]
    fn v201_authorize_call_missing_required_id_token_fails() {
        let v = SchemaValidator::v201();
        // `idToken` is the one required property.
        let err = v.validate_call("Authorize", &json!({})).unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_authorize_call_unknown_id_token_type_fails() {
        let v = SchemaValidator::v201();
        let call = json!({
            "idToken": { "idToken": "abc", "type": "RFID" }
        });
        assert!(v.validate_call("Authorize", &call).is_err());
    }

    #[test]
    fn v201_authorize_call_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        let call = json!({
            "idToken": { "idToken": "abc", "type": "ISO14443" },
            "bogusExtra": true
        });
        assert!(v.validate_call("Authorize", &call).is_err());
    }

    #[test]
    fn v201_authorize_result_missing_status_fails() {
        let v = SchemaValidator::v201();
        // `status` is required inside `idTokenInfo`.
        let result = json!({ "idTokenInfo": {} });
        let err = v.validate_call_result("Authorize", &result).unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_authorize_result_unknown_status_fails() {
        let v = SchemaValidator::v201();
        let result = json!({ "idTokenInfo": { "status": "Maybe" } });
        assert!(v.validate_call_result("Authorize", &result).is_err());
    }

    #[test]
    fn v201_authorize_call_accepts_deferred_certificate_fields() {
        // The Rust `AuthorizeRequest` defers the ISO 15118 certificate path, but
        // the bundled schema is verbatim and still validates those fields when a
        // peer sends them.
        let v = SchemaValidator::v201();
        let call = json!({
            "idToken": { "idToken": "abc", "type": "eMAID" },
            "certificate": "-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----"
        });
        assert!(v.validate_call("Authorize", &call).is_ok());
    }

    #[test]
    fn v201_heartbeat_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        // Heartbeat.req carries no fields.
        assert!(v.validate_call("Heartbeat", &json!({})).is_ok());
        // Heartbeat.conf requires a date-time `currentTime`.
        let payload = json!({ "currentTime": "2018-05-29T17:37:05.495259Z" });
        assert!(v.validate_call_result("Heartbeat", &payload).is_ok());
    }

    #[test]
    fn v201_heartbeat_response_missing_required_field_fails() {
        let v = SchemaValidator::v201();
        // `currentTime` is required on the response.
        let err = v.validate_call_result("Heartbeat", &json!({})).unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_status_notification_call_valid_passes() {
        let v = SchemaValidator::v201();
        let payload = json!({
            "timestamp": "2018-05-29T17:37:05.495259Z",
            "connectorStatus": "Available",
            "evseId": 1,
            "connectorId": 2
        });
        assert!(v.validate_call("StatusNotification", &payload).is_ok());
        // The response is an empty object.
        assert!(v
            .validate_call_result("StatusNotification", &json!({}))
            .is_ok());
    }

    #[test]
    fn v201_status_notification_missing_required_field_fails() {
        let v = SchemaValidator::v201();
        // `connectorId` is required.
        let payload = json!({
            "timestamp": "2018-05-29T17:37:05.495259Z",
            "connectorStatus": "Available",
            "evseId": 1
        });
        let err = v.validate_call("StatusNotification", &payload).unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_status_notification_unknown_enum_value_fails() {
        let v = SchemaValidator::v201();
        // `Charging` is a 1.6J-only state, not part of the 2.0.1 enum.
        let payload = json!({
            "timestamp": "2018-05-29T17:37:05.495259Z",
            "connectorStatus": "Charging",
            "evseId": 1,
            "connectorId": 2
        });
        assert!(v.validate_call("StatusNotification", &payload).is_err());
    }

    #[test]
    fn v201_status_notification_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        let payload = json!({
            "timestamp": "2018-05-29T17:37:05.495259Z",
            "connectorStatus": "Available",
            "evseId": 1,
            "connectorId": 2,
            "errorCode": "NoError"
        });
        assert!(v.validate_call("StatusNotification", &payload).is_err());
    }

    #[test]
    fn v201_boot_notification_call_valid_passes() {
        let v = SchemaValidator::v201();
        // The reference fixture (tests/v201/conftest.py).
        let payload = json!({
            "chargingStation": {
                "vendorName": "ICU Eve Mini",
                "model": "ICU Eve Mini",
                "firmwareVersion": "#1:3.4.0-2990#N:217H;1.0-223"
            },
            "reason": "PowerUp"
        });
        assert!(v.validate_call("BootNotification", &payload).is_ok());
    }

    #[test]
    fn v201_boot_notification_response_valid_passes() {
        let v = SchemaValidator::v201();
        // RFC 3339 with offset — the schema asserts `format: date-time`, which
        // the `jsonschema` crate enforces (unlike Python's default validator).
        let payload = json!({
            "currentTime": "2018-05-29T17:37:05.495259Z",
            "interval": 350,
            "status": "Accepted"
        });
        assert!(v.validate_call_result("BootNotification", &payload).is_ok());
    }

    #[test]
    fn v201_boot_notification_missing_required_field_fails() {
        let v = SchemaValidator::v201();
        // `chargingStation` is required.
        let payload = json!({ "reason": "PowerUp" });
        let err = v.validate_call("BootNotification", &payload).unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_boot_notification_unknown_enum_value_fails() {
        let v = SchemaValidator::v201();
        let payload = json!({
            "chargingStation": { "vendorName": "V", "model": "M" },
            "reason": "Bogus"
        });
        assert!(v.validate_call("BootNotification", &payload).is_err());
    }

    #[test]
    fn v201_boot_notification_rejects_additional_properties() {
        // The schema's `additionalProperties: false` is enforced (the strict
        // 2.0.1 contract). Draft selection itself is covered by
        // `draft_detection_picks_expected_drafts`.
        let v = SchemaValidator::v201();
        let payload = json!({
            "chargingStation": { "vendorName": "V", "model": "M" },
            "reason": "PowerUp",
            "bogusExtra": true
        });
        assert!(v.validate_call("BootNotification", &payload).is_err());
    }

    #[test]
    fn v201_get_variables_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        let call = json!({
            "getVariableData": [{
                "component": { "name": "OCPPCommCtrlr" },
                "variable": { "name": "HeartbeatInterval" }
            }]
        });
        assert!(v.validate_call("GetVariables", &call).is_ok());
        let result = json!({
            "getVariableResult": [{
                "attributeStatus": "Accepted",
                "component": { "name": "OCPPCommCtrlr" },
                "variable": { "name": "HeartbeatInterval" },
                "attributeValue": "300"
            }]
        });
        assert!(v.validate_call_result("GetVariables", &result).is_ok());
    }

    #[test]
    fn v201_get_variables_missing_required_field_fails() {
        let v = SchemaValidator::v201();
        // Each entry requires both `component` and `variable`.
        let payload = json!({
            "getVariableData": [{ "component": { "name": "OCPPCommCtrlr" } }]
        });
        let err = v.validate_call("GetVariables", &payload).unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_get_variables_unknown_enum_value_fails() {
        let v = SchemaValidator::v201();
        // `Maybe` is not a member of GetVariableStatusEnumType.
        let payload = json!({
            "getVariableResult": [{
                "attributeStatus": "Maybe",
                "component": { "name": "OCPPCommCtrlr" },
                "variable": { "name": "HeartbeatInterval" }
            }]
        });
        assert!(v.validate_call_result("GetVariables", &payload).is_err());
    }

    #[test]
    fn v201_get_variables_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        let payload = json!({
            "getVariableData": [{
                "component": { "name": "OCPPCommCtrlr" },
                "variable": { "name": "HeartbeatInterval" }
            }],
            "bogusExtra": true
        });
        assert!(v.validate_call("GetVariables", &payload).is_err());
    }

    #[test]
    fn v201_request_stop_transaction_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        let call = json!({ "transactionId": "txn-0001" });
        assert!(v.validate_call("RequestStopTransaction", &call).is_ok());
        let result = json!({ "status": "Accepted" });
        assert!(v
            .validate_call_result("RequestStopTransaction", &result)
            .is_ok());
        // `statusInfo` is optional but validates when present.
        let result_full = json!({
            "status": "Rejected",
            "statusInfo": { "reasonCode": "NoTransaction" }
        });
        assert!(v
            .validate_call_result("RequestStopTransaction", &result_full)
            .is_ok());
    }

    #[test]
    fn v201_request_stop_transaction_call_missing_transaction_id_fails() {
        let v = SchemaValidator::v201();
        // `transactionId` is the one required property.
        let err = v
            .validate_call("RequestStopTransaction", &json!({}))
            .unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_request_stop_transaction_call_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        let call = json!({ "transactionId": "txn-0001", "bogusExtra": true });
        assert!(v.validate_call("RequestStopTransaction", &call).is_err());
    }

    #[test]
    fn v201_request_stop_transaction_result_missing_status_fails() {
        let v = SchemaValidator::v201();
        let err = v
            .validate_call_result("RequestStopTransaction", &json!({}))
            .unwrap_err();
        assert!(matches!(err, OcppError::SchemaViolation { .. }));
    }

    #[test]
    fn v201_request_stop_transaction_result_unknown_status_fails() {
        let v = SchemaValidator::v201();
        // `Scheduled` is a valid ResetStatus but NOT a RequestStartStopStatus.
        let result = json!({ "status": "Scheduled" });
        assert!(v
            .validate_call_result("RequestStopTransaction", &result)
            .is_err());
    }

    #[test]
    fn v201_request_stop_transaction_result_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        let result = json!({ "status": "Accepted", "bogusExtra": true });
        assert!(v
            .validate_call_result("RequestStopTransaction", &result)
            .is_err());
    }

    #[test]
    fn v201_clear_cache_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        // Empty request is valid.
        assert!(v.validate_call("ClearCache", &json!({})).is_ok());
        // And with a vendor extension.
        assert!(v
            .validate_call(
                "ClearCache",
                &json!({ "customData": { "vendorId": "ACME" } })
            )
            .is_ok());
        assert!(v
            .validate_call_result("ClearCache", &json!({ "status": "Accepted" }))
            .is_ok());
        assert!(v
            .validate_call_result("ClearCache", &json!({ "status": "Rejected" }))
            .is_ok());
    }

    #[test]
    fn v201_clear_cache_result_missing_status_fails() {
        let v = SchemaValidator::v201();
        assert!(v.validate_call_result("ClearCache", &json!({})).is_err());
    }

    #[test]
    fn v201_clear_cache_result_unknown_status_fails() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call_result("ClearCache", &json!({ "status": "Scheduled" }))
            .is_err());
    }

    #[test]
    fn v201_clear_cache_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call("ClearCache", &json!({ "bogus": true }))
            .is_err());
        assert!(v
            .validate_call_result(
                "ClearCache",
                &json!({ "status": "Accepted", "bogus": true })
            )
            .is_err());
    }

    #[test]
    fn v201_cancel_reservation_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call("CancelReservation", &json!({ "reservationId": 42 }))
            .is_ok());
        assert!(v
            .validate_call(
                "CancelReservation",
                &json!({ "reservationId": 1, "customData": { "vendorId": "ACME" } })
            )
            .is_ok());
        assert!(v
            .validate_call_result("CancelReservation", &json!({ "status": "Accepted" }))
            .is_ok());
        assert!(v
            .validate_call_result("CancelReservation", &json!({ "status": "Rejected" }))
            .is_ok());
    }

    #[test]
    fn v201_cancel_reservation_call_missing_reservation_id_fails() {
        let v = SchemaValidator::v201();
        assert!(v.validate_call("CancelReservation", &json!({})).is_err());
    }

    #[test]
    fn v201_cancel_reservation_call_non_integer_reservation_id_fails() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call("CancelReservation", &json!({ "reservationId": "42" }))
            .is_err());
    }

    #[test]
    fn v201_cancel_reservation_result_missing_or_unknown_status_fails() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call_result("CancelReservation", &json!({}))
            .is_err());
        assert!(v
            .validate_call_result("CancelReservation", &json!({ "status": "Scheduled" }))
            .is_err());
    }

    #[test]
    fn v201_cancel_reservation_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call(
                "CancelReservation",
                &json!({ "reservationId": 1, "bogus": true })
            )
            .is_err());
        assert!(v
            .validate_call_result(
                "CancelReservation",
                &json!({ "status": "Accepted", "bogus": true })
            )
            .is_err());
    }

    #[test]
    fn v201_get_local_list_version_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        // The request is an empty object.
        assert!(v.validate_call("GetLocalListVersion", &json!({})).is_ok());
        // The response carries a single integer version number.
        assert!(v
            .validate_call_result("GetLocalListVersion", &json!({ "versionNumber": 7 }))
            .is_ok());
    }

    #[test]
    fn v201_get_local_list_version_result_missing_version_number_fails() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call_result("GetLocalListVersion", &json!({}))
            .is_err());
    }

    #[test]
    fn v201_get_local_list_version_result_wrong_type_fails() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call_result("GetLocalListVersion", &json!({ "versionNumber": "7" }))
            .is_err());
    }

    #[test]
    fn v201_get_local_list_version_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call("GetLocalListVersion", &json!({ "bogusExtra": true }))
            .is_err());
        assert!(v
            .validate_call_result(
                "GetLocalListVersion",
                &json!({ "versionNumber": 1, "bogusExtra": true })
            )
            .is_err());
    }

    #[test]
    fn v201_change_availability_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        // Whole-station target: only the required operationalStatus.
        assert!(v
            .validate_call(
                "ChangeAvailability",
                &json!({ "operationalStatus": "Operative" })
            )
            .is_ok());
        // EVSE-scoped target.
        assert!(v
            .validate_call(
                "ChangeAvailability",
                &json!({ "operationalStatus": "Inoperative", "evse": { "id": 1 } })
            )
            .is_ok());
        // All three response statuses, incl. the schema-only `Scheduled`.
        for status in ["Accepted", "Rejected", "Scheduled"] {
            assert!(v
                .validate_call_result("ChangeAvailability", &json!({ "status": status }))
                .is_ok());
        }
    }

    #[test]
    fn v201_change_availability_missing_required_fields_fail() {
        let v = SchemaValidator::v201();
        assert!(v.validate_call("ChangeAvailability", &json!({})).is_err());
        assert!(v
            .validate_call_result("ChangeAvailability", &json!({}))
            .is_err());
    }

    #[test]
    fn v201_change_availability_rejects_unknown_enum_values() {
        let v = SchemaValidator::v201();
        // `Scheduled` is a valid response status but NOT an operationalStatus.
        assert!(v
            .validate_call(
                "ChangeAvailability",
                &json!({ "operationalStatus": "Scheduled" })
            )
            .is_err());
        // `Operative` is a request value, not a ChangeAvailabilityStatus.
        assert!(v
            .validate_call_result("ChangeAvailability", &json!({ "status": "Operative" }))
            .is_err());
    }

    #[test]
    fn v201_change_availability_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call(
                "ChangeAvailability",
                &json!({ "operationalStatus": "Operative", "bogusExtra": true })
            )
            .is_err());
        assert!(v
            .validate_call_result(
                "ChangeAvailability",
                &json!({ "status": "Accepted", "bogusExtra": true })
            )
            .is_err());
    }

    #[test]
    fn v201_unlock_connector_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call("UnlockConnector", &json!({ "evseId": 1, "connectorId": 2 }))
            .is_ok());
        for status in [
            "Unlocked",
            "UnlockFailed",
            "OngoingAuthorizedTransaction",
            "UnknownConnector",
        ] {
            assert!(v
                .validate_call_result("UnlockConnector", &json!({ "status": status }))
                .is_ok());
        }
    }

    #[test]
    fn v201_unlock_connector_call_missing_required_ids_fail() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call("UnlockConnector", &json!({ "evseId": 1 }))
            .is_err());
        assert!(v
            .validate_call("UnlockConnector", &json!({ "connectorId": 2 }))
            .is_err());
    }

    #[test]
    fn v201_unlock_connector_call_rejects_non_integer_ids() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call(
                "UnlockConnector",
                &json!({ "evseId": "1", "connectorId": 2 })
            )
            .is_err());
    }

    #[test]
    fn v201_unlock_connector_result_missing_or_unknown_status_fails() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call_result("UnlockConnector", &json!({}))
            .is_err());
        // `NotSupported` is a 1.6J UnlockStatus value, not valid in 2.0.1.
        assert!(v
            .validate_call_result("UnlockConnector", &json!({ "status": "NotSupported" }))
            .is_err());
    }

    #[test]
    fn v201_unlock_connector_rejects_additional_properties() {
        let v = SchemaValidator::v201();
        assert!(v
            .validate_call(
                "UnlockConnector",
                &json!({ "evseId": 1, "connectorId": 2, "bogusExtra": true })
            )
            .is_err());
        assert!(v
            .validate_call_result(
                "UnlockConnector",
                &json!({ "status": "Unlocked", "bogusExtra": true })
            )
            .is_err());
    }

    #[test]
    fn v201_data_transfer_call_and_result_valid_pass() {
        let v = SchemaValidator::v201();
        // Minimal request: just the required vendorId.
        assert!(v
            .validate_call("DataTransfer", &json!({ "vendorId": "ACME" }))
            .is_ok());
        // Full request with optional messageId and free-form data.
        assert!(v
            .validate_call(
                "DataTransfer",
                &json!({
                    "vendorId": "ACME",
                    "messageId": "diag.run",
                    "data": { "level": 3, "tags": ["a", "b"] }
                })
            )
            .is_ok());
        // Minimal response: just the required status.
        assert!(v
            .validate_call_result("DataTransfer", &json!({ "status": "Accepted" }))
            .is_ok());
        // Response carrying free-form data back.
        assert!(v
            .validate_call_result(
                "DataTransfer",
                &json!({ "status": "Accepted", "data": [1, 2, 3] })
            )
            .is_ok());
    }

    #[test]
    fn v201_data_transfer_accepts_arbitrary_data_shapes() {
        let v = SchemaValidator::v201();
        // `data` is unconstrained: object, array, string, number, bool, null
        // all validate against the FINAL schema.
        for data in [
            json!({ "k": "v" }),
            json!([1, 2, 3]),
            json!("a string"),
            json!(42),
            json!(true),
            json!(null),
        ] {
            assert!(
                v.validate_call("DataTransfer", &json!({ "vendorId": "ACME", "data": data }))
                    .is_ok(),
                "data shape should validate"
            );
        }
    }

    #[test]
    fn v201_data_transfer_call_missing_required_vendor_id_fails() {
        let v = SchemaValidator::v201();
        // `vendorId` is required even when other fields are present.
        assert!(v
            .validate_call("DataTransfer", &json!({ "messageId": "x" }))
            .is_err());
    }

    #[test]
    fn v201_data_transfer_result_missing_or_unknown_status_fails() {
        let v = SchemaValidator::v201();
        assert!(v.validate_call_result("DataTransfer", &json!({})).is_err());
        assert!(v
            .validate_call_result("DataTransfer", &json!({ "status": "Bogus" }))
            .is_err());
    }

    #[test]
    fn v201_data_transfer_rejects_additional_properties_at_root() {
        let v = SchemaValidator::v201();
        // The freedom lives *inside* `data`; the message root is still closed.
        assert!(v
            .validate_call(
                "DataTransfer",
                &json!({ "vendorId": "ACME", "bogusExtra": true })
            )
            .is_err());
        assert!(v
            .validate_call_result(
                "DataTransfer",
                &json!({ "status": "Accepted", "bogusExtra": true })
            )
            .is_err());
    }

    #[test]
    fn draft_detection_picks_expected_drafts() {
        let d6 = json!({ "$schema": "http://json-schema.org/draft-06/schema#" });
        let d4 = json!({ "$schema": "http://json-schema.org/draft-04/schema#" });
        let none = json!({});
        assert!(matches!(
            SchemaValidator::draft_for(&d6),
            jsonschema::Draft::Draft6
        ));
        // 1.6J path is unchanged: draft-04 and unspecified both map to Draft4.
        assert!(matches!(
            SchemaValidator::draft_for(&d4),
            jsonschema::Draft::Draft4
        ));
        assert!(matches!(
            SchemaValidator::draft_for(&none),
            jsonschema::Draft::Draft4
        ));
    }
}
