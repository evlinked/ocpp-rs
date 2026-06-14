//! Periodic `MeterValues` sampling for active transactions.
//!
//! Ports the periodic background-task pattern from the Python reference
//! ([`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)):
//! while a transaction is active the charge point samples its meter at a fixed
//! interval (the OCPP `MeterValueSampleInterval` key) and sends a `MeterValues`
//! CALL to the CSMS.
//!
//! This module holds only the **pure** message-construction logic so it can be
//! unit-tested without a runtime or a socket. The background-task spawn/cancel
//! wiring lives on [`crate::ChargePoint`] (see `start_meter_sampler`), mirroring
//! the existing heartbeat task.

use crate::connector::MeterReading;
use ocpp_messages::v16j::MeterValuesRequest;
use ocpp_types::common::{
    Measurand, MeterValue, ReadingContext, SampledValue, UnitOfMeasure, ValueFormat,
};
use ocpp_types::ConnectorId;

/// Render an `f64` meter reading as an OCPP `SampledValue.value` string.
///
/// OCPP transmits sampled values as strings. Integral readings (the common case
/// for the `Energy.Active.Import.Register` register, expressed in Wh) are
/// rendered without a trailing `.0`; fractional readings keep their decimals.
fn format_value(v: f64) -> String {
    if v.fract() == 0.0 {
        // `v` is integral; render as an integer to match real meter output.
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Build a [`SampledValue`] for a single `measurand` from a meter `reading`.
///
/// Returns `None` for measurands the simulator's [`MeterReading`] cannot supply
/// (e.g. frequency, power factor, or temperature when the connector reports
/// none) so callers can simply `filter_map` over the configured measurand list.
fn sampled_value_for(
    measurand: &Measurand,
    reading: &MeterReading,
    context: ReadingContext,
) -> Option<SampledValue> {
    let (value, unit) = match measurand {
        Measurand::EnergyActiveImportRegister => (reading.energy_wh, UnitOfMeasure::Wh),
        Measurand::PowerActiveImport => (reading.power_w, UnitOfMeasure::W),
        Measurand::Voltage => (reading.voltage_v, UnitOfMeasure::V),
        Measurand::CurrentImport => (reading.current_a, UnitOfMeasure::A),
        Measurand::Temperature => (reading.temperature_c?, UnitOfMeasure::Celsius),
        // Other measurands aren't derivable from the simulator's reading.
        _ => return None,
    };

    Some(SampledValue {
        value: format_value(value),
        context: Some(context),
        format: Some(ValueFormat::Raw),
        measurand: Some(measurand.clone()),
        phase: None,
        location: None,
        unit: Some(unit),
    })
}

/// Build a `MeterValues` request for `connector_id` / `transaction_id` from a
/// meter `reading`, emitting one `SampledValue` per supported configured
/// measurand.
///
/// `context` distinguishes the transaction-begin snapshot
/// ([`ReadingContext::TransactionBegin`]) from periodic samples
/// ([`ReadingContext::SamplePeriodic`]).
///
/// Measurands that the reading cannot supply are skipped; the returned request
/// may therefore contain an empty `sampledValue` list if `measurands` is empty
/// or none are supported. Callers (the sampler task) skip sending in that case,
/// since the OCPP schema requires at least one sampled value.
pub(crate) fn build_meter_values_request(
    connector_id: ConnectorId,
    transaction_id: Option<i32>,
    reading: &MeterReading,
    measurands: &[Measurand],
    context: ReadingContext,
) -> MeterValuesRequest {
    let sampled_values: Vec<SampledValue> = measurands
        .iter()
        .filter_map(|m| sampled_value_for(m, reading, context.clone()))
        .collect();

    MeterValuesRequest {
        connector_id: connector_id.value(),
        transaction_id,
        meter_values: vec![MeterValue {
            timestamp: reading.timestamp,
            sampled_values,
        }],
    }
}

/// `true` if `request` carries at least one sampled value worth sending.
///
/// Guards the sampler against emitting a `MeterValues` frame with an empty
/// `sampledValue` array, which the OCPP 1.6J `MeterValues.json` schema rejects.
pub(crate) fn has_samples(request: &MeterValuesRequest) -> bool {
    request
        .meter_values
        .iter()
        .any(|mv| !mv.sampled_values.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn reading() -> MeterReading {
        MeterReading {
            timestamp: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            energy_wh: 1234.0,
            power_w: 7360.0,
            voltage_v: 229.5,
            current_a: 32.0,
            temperature_c: None,
        }
    }

    #[test]
    fn format_value_renders_integral_without_decimal() {
        assert_eq!(format_value(1234.0), "1234");
        assert_eq!(format_value(0.0), "0");
    }

    #[test]
    fn format_value_keeps_fractional_decimals() {
        assert_eq!(format_value(229.5), "229.5");
    }

    #[test]
    fn default_energy_measurand_produces_one_wh_sample() {
        let req = build_meter_values_request(
            ConnectorId::new(1).unwrap(),
            Some(42),
            &reading(),
            &[Measurand::EnergyActiveImportRegister],
            ReadingContext::SamplePeriodic,
        );

        assert_eq!(req.connector_id, 1);
        assert_eq!(req.transaction_id, Some(42));
        assert_eq!(req.meter_values.len(), 1);
        let sv = &req.meter_values[0].sampled_values;
        assert_eq!(sv.len(), 1);
        assert_eq!(sv[0].value, "1234");
        assert_eq!(sv[0].measurand, Some(Measurand::EnergyActiveImportRegister));
        assert_eq!(sv[0].unit, Some(UnitOfMeasure::Wh));
        assert_eq!(sv[0].context, Some(ReadingContext::SamplePeriodic));
        assert_eq!(sv[0].format, Some(ValueFormat::Raw));
    }

    #[test]
    fn multiple_measurands_produce_multiple_samples_with_correct_units() {
        let req = build_meter_values_request(
            ConnectorId::new(2).unwrap(),
            Some(7),
            &reading(),
            &[
                Measurand::EnergyActiveImportRegister,
                Measurand::PowerActiveImport,
                Measurand::Voltage,
                Measurand::CurrentImport,
            ],
            ReadingContext::SamplePeriodic,
        );

        let sv = &req.meter_values[0].sampled_values;
        assert_eq!(sv.len(), 4);
        assert_eq!(sv[1].unit, Some(UnitOfMeasure::W));
        assert_eq!(sv[2].unit, Some(UnitOfMeasure::V));
        assert_eq!(sv[2].value, "229.5");
        assert_eq!(sv[3].unit, Some(UnitOfMeasure::A));
    }

    #[test]
    fn unsupported_or_missing_measurands_are_skipped() {
        // Temperature has no reading; Frequency is not derivable at all.
        let req = build_meter_values_request(
            ConnectorId::new(1).unwrap(),
            None,
            &reading(),
            &[Measurand::Temperature, Measurand::Frequency],
            ReadingContext::SamplePeriodic,
        );
        assert!(req.meter_values[0].sampled_values.is_empty());
        assert!(!has_samples(&req));
    }

    #[test]
    fn temperature_sample_emitted_when_reading_present() {
        let mut r = reading();
        r.temperature_c = Some(24.0);
        let req = build_meter_values_request(
            ConnectorId::new(1).unwrap(),
            None,
            &r,
            &[Measurand::Temperature],
            ReadingContext::SamplePeriodic,
        );
        let sv = &req.meter_values[0].sampled_values;
        assert_eq!(sv.len(), 1);
        assert_eq!(sv[0].value, "24");
        assert_eq!(sv[0].unit, Some(UnitOfMeasure::Celsius));
    }

    #[test]
    fn begin_snapshot_serializes_with_dotted_transaction_begin_context() {
        let req = build_meter_values_request(
            ConnectorId::new(1).unwrap(),
            Some(1),
            &reading(),
            &[Measurand::EnergyActiveImportRegister],
            ReadingContext::TransactionBegin,
        );
        let json = serde_json::to_string(&req).unwrap();
        // OCPP wire keys + dotted enum values.
        assert!(json.contains("\"connectorId\":1"), "{json}");
        assert!(json.contains("\"transactionId\":1"), "{json}");
        assert!(json.contains("\"sampledValue\""), "{json}");
        assert!(json.contains("\"Transaction.Begin\""), "{json}");
        assert!(json.contains("\"Energy.Active.Import.Register\""), "{json}");
    }

    #[test]
    fn has_samples_true_when_at_least_one_value() {
        let req = build_meter_values_request(
            ConnectorId::new(1).unwrap(),
            Some(1),
            &reading(),
            &[Measurand::EnergyActiveImportRegister],
            ReadingContext::SamplePeriodic,
        );
        assert!(has_samples(&req));
    }
}
