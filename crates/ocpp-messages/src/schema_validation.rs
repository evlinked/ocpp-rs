//! JSON Schema validation for OCPP 1.6J messages.
//!
//! Mirrors `get_validator()` + `_validate_payload()` from
//! `ocpp/messages.py` in mobilityhouse/ocpp: every CALL and CALLRESULT
//! payload is checked against the corresponding JSON Schema (Draft 4)
//! before dispatch, preventing malformed payloads from reaching handlers.

use std::collections::HashMap;

use ocpp_types::{OcppError, OcppResult};
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

    fn run_validation(schema: &Value, payload: &Value) -> OcppResult<()> {
        // jsonschema 0.17 `MultipleOfFloatValidator` computes
        // `(item / multiple_of) % 1.0` and checks `remainder < f64::EPSILON`.
        // For `21.4_f64 / 0.1_f64 = 213.9999…` the remainder is `0.9999…`
        // rather than `0.0`, so the check incorrectly rejects valid 1-decimal
        // values like `21.4`, `15.2`, `6.6`.
        //
        // Fix: replace each float `x` with `round(x / d) * d` for every
        // float-valued `multipleOf` divisor `d` found in the schema.
        // Empirically `(N * d) / d == N.0` exactly in IEEE 754 for these
        // divisors (verified via tests), so the normalised value passes the
        // `% 1.0 < EPSILON` check.  Values that are NOT near an exact
        // multiple (e.g. `21.41` → normalised to `4.1`, delta `0.01 ≫ 1e-9`)
        // are left unchanged and continue to fail validation correctly.
        //
        // Mirrors the Python reference's `decimal.Decimal` re-parse in
        // `ocpp/messages.py` `_validate_payload()`.
        let divisors = collect_float_divisors(schema);
        let payload_owned;
        let payload: &Value = if divisors.is_empty() {
            payload
        } else {
            payload_owned = normalize_floats(payload.clone(), &divisors);
            &payload_owned
        };

        let compiled = jsonschema::JSONSchema::options()
            .with_draft(jsonschema::Draft::Draft4)
            .compile(schema)
            .map_err(|e| OcppError::Internal {
                message: format!("failed to compile bundled schema: {}", e),
            })?;

        if let Err(errors) = compiled.validate(payload) {
            let messages: Vec<String> = errors.map(|e| e.to_string()).collect();
            return Err(OcppError::ValidationError {
                message: messages.join("; "),
            });
        }
        Ok(())
    }
}

/// Recursively walk a JSON Schema value and collect all `"multipleOf"`
/// divisors that have a non-zero fractional part (i.e. are float, not
/// integer). These are the cases that trigger `MultipleOfFloatValidator`
/// in `jsonschema` 0.17 and require pre-normalisation of payload floats.
fn collect_float_divisors(schema: &Value) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    collect_float_divisors_inner(schema, &mut out);
    // Deduplicate while preserving meaningful ordering.
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-15);
    out
}

fn collect_float_divisors_inner(schema: &Value, out: &mut Vec<f64>) {
    match schema {
        Value::Object(map) => {
            if let Some(Value::Number(n)) = map.get("multipleOf") {
                if let Some(f) = n.as_f64() {
                    if f.fract() != 0.0 && f > 0.0 {
                        out.push(f);
                    }
                }
            }
            for v in map.values() {
                collect_float_divisors_inner(v, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_float_divisors_inner(v, out);
            }
        }
        _ => {}
    }
}

/// Walk a JSON Value and replace any positive `f64` number `x` with
/// `round(x / d) * d` for the first divisor `d` for which
/// `|candidate - x| < 1e-9 * max(1, |x|)`.
///
/// This converts, e.g., `21.399999… → 21.400000000000002` (= `214 * 0.1_f64`)
/// whose remainder `(value / 0.1_f64) % 1.0` is exactly `0.0` in IEEE 754,
/// satisfying `jsonschema` 0.17's `MultipleOfFloatValidator`. Values already
/// far from an exact multiple (e.g. `21.41`) are left unchanged.
fn normalize_floats(value: Value, divisors: &[f64]) -> Value {
    match value {
        Value::Number(n) => {
            if n.is_f64() {
                if let Some(f) = n.as_f64() {
                    if f.is_finite() && f > 0.0 {
                        for &d in divisors {
                            let count = (f / d).round() as i64;
                            let candidate = count as f64 * d;
                            let tol = 1e-9_f64 * f.abs().max(1.0_f64);
                            if (candidate - f).abs() < tol {
                                if let Some(num) = serde_json::Number::from_f64(candidate) {
                                    return Value::Number(num);
                                }
                            }
                        }
                    }
                }
            }
            Value::Number(n)
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, normalize_floats(v, divisors)))
                .collect(),
        ),
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| normalize_floats(v, divisors))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── construction ──────────────────────────────────────────────────────────

    #[test]
    fn v16j_loads_all_78_schemas() {
        let v = SchemaValidator::v16j();
        assert_eq!(v.schema_count(), 78);
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

    /// Port of test_validate_set_charging_profile_payload (Python reference).
    ///
    /// The Python reference uses `21.4` and relies on `decimal.Decimal`
    /// parsing to avoid float precision issues with `"multipleOf": 0.1`.
    /// Our fix normalises `21.4_f64` → `214 * 0.1_f64` before validation,
    /// whose IEEE 754 remainder when divided by `0.1_f64` is exactly `0.0`.
    #[test]
    fn validate_set_charging_profile_float_limit_passes() {
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

    /// Port of test_validate_get_composite_profile_payload (Python reference).
    /// Uses `15.2` (matching the Python test) rather than the workaround `15.0`.
    #[test]
    fn validate_get_composite_schedule_response_float_limit_passes() {
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

    /// Port of the third Python reference case (RemoteStartTransaction with
    /// a charging profile containing a `limit` with 1 decimal place).
    #[test]
    fn validate_remote_start_transaction_float_limit_passes() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "idTag": "TAG001",
            "chargingProfile": {
                "chargingProfileId": 1,
                "stackLevel": 0,
                "chargingProfilePurpose": "TxProfile",
                "chargingProfileKind": "Relative",
                "chargingSchedule": {
                    "chargingRateUnit": "A",
                    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 11.5}]
                }
            }
        });
        assert!(v.validate_call("RemoteStartTransaction", &payload).is_ok());
    }

    /// A `limit` with more than 1 significant decimal (e.g. `21.41`) is NOT a
    /// multiple of `0.1` and must still be rejected after normalisation.
    /// Normalising `21.41` would require rounding to `4.1` (delta `0.01 ≫ 1e-9`)
    /// which our tolerance check correctly rejects, leaving the value unchanged.
    #[test]
    fn validate_set_charging_profile_two_decimal_limit_fails() {
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
                    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 21.41}]
                },
                "transactionId": 1
            }
        });
        let err = v.validate_call("SetChargingProfile", &payload).unwrap_err();
        assert!(
            matches!(err, OcppError::ValidationError { .. }),
            "expected ValidationError for non-multiple-of-0.1 limit, got {:?}",
            err
        );
    }

    /// `minChargingRate` also has `"multipleOf": 0.1` in the schema; verify
    /// the fix applies there too.
    #[test]
    fn validate_set_charging_profile_float_min_charging_rate_passes() {
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
                    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 6.6}],
                    "minChargingRate": 6.6
                },
                "transactionId": 1
            }
        });
        assert!(v.validate_call("SetChargingProfile", &payload).is_ok());
    }

    // ── invalid payloads are rejected ────────────────────────────────────────

    /// Port of test_validate_payload_with_invalid_additional_properties_payload
    #[test]
    fn validate_heartbeat_response_with_extra_key_fails() {
        let v = SchemaValidator::v16j();
        let payload = json!({"invalid_key": true});
        let err = v.validate_call_result("Heartbeat", &payload).unwrap_err();
        assert!(
            matches!(err, OcppError::ValidationError { .. }),
            "expected ValidationError, got {:?}",
            err
        );
    }

    /// Port of test_validate_payload_with_invalid_type_payload
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
        assert!(
            matches!(err, OcppError::ValidationError { .. }),
            "expected ValidationError, got {:?}",
            err
        );
    }

    /// Port of test_validate_payload_with_invalid_missing_property_payload
    #[test]
    fn validate_start_transaction_missing_required_field_fails() {
        let v = SchemaValidator::v16j();
        let payload = json!({
            "connectorId": 1,
            "idTag": "okTag"
            // meterStart and timestamp are required but omitted
        });
        let err = v.validate_call("StartTransaction", &payload).unwrap_err();
        assert!(
            matches!(err, OcppError::ValidationError { .. }),
            "expected ValidationError, got {:?}",
            err
        );
    }

    #[test]
    fn validate_boot_notification_missing_required_vendor_fails() {
        let v = SchemaValidator::v16j();
        // chargePointModel present but chargePointVendor missing
        let payload = json!({"chargePointModel": "Turbo-3000"});
        let err = v.validate_call("BootNotification", &payload).unwrap_err();
        assert!(matches!(err, OcppError::ValidationError { .. }));
    }

    #[test]
    fn validate_reset_with_unknown_type_enum_fails() {
        let v = SchemaValidator::v16j();
        // "Warm" is not a valid ResetType (only "Hard" and "Soft" are)
        let payload = json!({"type": "Warm"});
        let err = v.validate_call("Reset", &payload).unwrap_err();
        assert!(matches!(err, OcppError::ValidationError { .. }));
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
            Err(OcppError::ValidationError { message }) => {
                assert!(
                    !message.is_empty(),
                    "validation error message must not be empty"
                );
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }
}
