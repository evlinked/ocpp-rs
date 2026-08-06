//! Pure builders for the OCPP 2.0.1 `TransactionEvent` message.
//!
//! Ports `ocpp.v201.call.TransactionEvent` (`ocpp/v201/call.py`), the unified
//! 2.0.1 message that replaces the 1.6J `StartTransaction` / `MeterValues` /
//! `StopTransaction` triad. A transaction is reported as a sequence of events:
//! one `Started`, zero or more `Updated`, and one `Ended`
//! ([`TransactionEventEnumType`]).
//!
//! Following the same cadence #419 used for the boot handshake (slice 1 landed a
//! pure, schema-validated builder; slice 2 wired it into the live loop), this
//! module holds only the **pure** message-construction logic — the same split
//! used by [`meter_sampler`](crate::meter_sampler). It can be unit-tested
//! without a runtime or a socket; branching the CP's transactional loop on
//! `protocol_version` to emit these builders over the wire is the slice-3b
//! runtime follow-up.

use ocpp_types::common::Reason;
use ocpp_types::v201::{
    ChargingStateEnumType, EvseType, IdTokenEnumType, IdTokenType, MeasurandEnumType,
    MeterValueType, ReadingContextEnumType, ReasonEnumType, SampledValueType,
    TransactionEventEnumType, TransactionType, TriggerReasonEnumType,
};

use ocpp_messages::v201::TransactionEventRequest;

/// Identity of the transaction a `TransactionEvent` is reported for.
///
/// Bundles the fields that are constant across every event in a single
/// transaction's lifecycle (its id and the EVSE/connector it runs on), so the
/// per-event builders stay within the project's `too-many-arguments` budget and
/// callers can't accidentally report two events for the same transaction under
/// different EVSEs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRef<'a> {
    /// Unique transaction identifier (2.0.1 `transactionId`, max length 36).
    ///
    /// In 1.6J the CSMS assigns an integer `transactionId`; 2.0.1 makes it a
    /// station-chosen string, so the runtime renders the 1.6J id as its decimal
    /// string when bridging.
    pub transaction_id: &'a str,
    /// EVSE the transaction runs on (2.0.1 `evse.id`, ≥ 1).
    pub evse_id: i32,
    /// Connector within the EVSE (2.0.1 `evse.connectorId`, ≥ 1).
    pub connector_id: i32,
}

/// Map a 1.6J [`StopTransaction` reason](Reason) onto the 2.0.1
/// [`ReasonEnumType`] reported as `transactionInfo.stoppedReason` on the `Ended`
/// event.
///
/// Total mapping. Most values carry across by name; the three without a direct
/// 2.0.1 twin are mapped by intent and documented inline. The 1.6J
/// `Hard`/`Soft` reset split maps onto the 2.0.1 `ImmediateReset`/`Reboot`
/// distinction (a hard reset restarts immediately; a soft reset is a graceful
/// reboot). A stop triggered by the `UnlockConnector` command has no dedicated
/// 2.0.1 stop-reason, so it reports as `Remote` — an operator-issued unlock is
/// a remotely commanded stop.
pub fn reason_to_v201(reason: Reason) -> ReasonEnumType {
    match reason {
        Reason::EmergencyStop => ReasonEnumType::EmergencyStop,
        Reason::EVDisconnected => ReasonEnumType::EVDisconnected,
        Reason::HardReset => ReasonEnumType::ImmediateReset,
        Reason::Local => ReasonEnumType::Local,
        Reason::Other => ReasonEnumType::Other,
        Reason::PowerLoss => ReasonEnumType::PowerLoss,
        Reason::Reboot => ReasonEnumType::Reboot,
        Reason::Remote => ReasonEnumType::Remote,
        Reason::SoftReset => ReasonEnumType::Reboot,
        Reason::UnlockCommand => ReasonEnumType::Remote,
        Reason::DeAuthorized => ReasonEnumType::DeAuthorized,
    }
}

/// Map a 1.6J [`StopTransaction` reason](Reason) onto the 2.0.1
/// [`TriggerReasonEnumType`] reported as the `triggerReason` of the `Ended`
/// event.
///
/// The 1.6J `Reason` answers "why did the transaction stop"; 2.0.1 splits that
/// into `stoppedReason` (see [`reason_to_v201`]) *and* `triggerReason` ("what
/// event fired this message"). Total mapping, chosen by intent: an EV
/// disconnect is an `EVDeparted` trigger, a remote stop is `RemoteStop`, an
/// unlock command is `UnlockCommand`, any reset is a `ResetCommand`,
/// deauthorization is `Deauthorized`, an emergency stop or power loss is an
/// `AbnormalCondition`, and a plain local/other stop reports as
/// `StopAuthorized`.
pub fn reason_to_v201_trigger(reason: Reason) -> TriggerReasonEnumType {
    match reason {
        Reason::EVDisconnected => TriggerReasonEnumType::EVDeparted,
        Reason::Remote => TriggerReasonEnumType::RemoteStop,
        Reason::UnlockCommand => TriggerReasonEnumType::UnlockCommand,
        Reason::HardReset | Reason::SoftReset | Reason::Reboot => {
            TriggerReasonEnumType::ResetCommand
        }
        Reason::DeAuthorized => TriggerReasonEnumType::Deauthorized,
        Reason::EmergencyStop | Reason::PowerLoss => TriggerReasonEnumType::AbnormalCondition,
        Reason::Local | Reason::Other => TriggerReasonEnumType::StopAuthorized,
    }
}

/// The 2.0.1 `evse` object for a session (`{ id, connectorId }`).
fn evse_of(session: &SessionRef) -> EvseType {
    EvseType {
        id: session.evse_id,
        connector_id: Some(session.connector_id),
        custom_data: None,
    }
}

/// A 2.0.1 `idToken` derived from a 1.6J `idTag` string.
///
/// A 1.6J `idTag` is an untyped identifier that in practice holds an RFID card's
/// UID, so it maps onto an `ISO14443` [`IdTokenType`] — the RFID token kind.
fn id_token_of(id_tag: &str) -> IdTokenType {
    IdTokenType {
        id_token: id_tag.to_string(),
        kind: IdTokenEnumType::Iso14443,
        additional_info: None,
        custom_data: None,
    }
}

/// A single-sample `meterValue` array carrying one active-import energy reading.
///
/// The value is an `Energy.Active.Import.Register` measurand in Wh (unit and
/// measurand default per the spec, so `unitOfMeasure` is omitted); `context`
/// distinguishes a transaction-begin, periodic-sample, or transaction-end
/// reading.
fn energy_meter_value(
    energy_wh: f64,
    context: ReadingContextEnumType,
    timestamp: &str,
) -> Vec<MeterValueType> {
    vec![MeterValueType {
        timestamp: timestamp.to_string(),
        sampled_value: vec![SampledValueType {
            value: energy_wh,
            context: Some(context),
            measurand: Some(MeasurandEnumType::EnergyActiveImportRegister),
            phase: None,
            location: None,
            signed_meter_value: None,
            unit_of_measure: None,
            custom_data: None,
        }],
        custom_data: None,
    }]
}

/// Build a `TransactionEvent(Started)` — the first event of a transaction.
///
/// `triggerReason = Authorized` (an `idToken` authorized the session), `seqNo =
/// 0`, `chargingState = Charging`, and a `Transaction.Begin` meter reading. The
/// authorizing `idToken` and the `evse` are carried so the CSMS can bind the
/// transaction to a driver and a connector. Replaces the 1.6J `StartTransaction`
/// CALL for a `V201` charge point.
///
/// `remote_start_id` carries the `remoteStartId` of the CSMS
/// `RequestStartTransaction` that initiated this session (OCPP 2.0.1 Part 2,
/// `transactionInfo.remoteStartId`), so the CSMS can correlate its remote-start
/// request with the `TransactionEvent(Started)` that follows — the 2.0.1
/// mechanism replacing 1.6J's synchronous `transactionId` in
/// `RemoteStartTransaction.conf`. It is `None` for a locally initiated (e.g.
/// cable-plugged-in) start, which has no remote-start request to correlate.
pub fn transaction_event_started(
    session: &SessionRef,
    id_tag: &str,
    meter_start_wh: f64,
    timestamp: &str,
    remote_start_id: Option<i32>,
) -> TransactionEventRequest {
    TransactionEventRequest {
        event_type: TransactionEventEnumType::Started,
        timestamp: timestamp.to_string(),
        trigger_reason: TriggerReasonEnumType::Authorized,
        seq_no: 0,
        transaction_info: TransactionType {
            transaction_id: session.transaction_id.to_string(),
            charging_state: Some(ChargingStateEnumType::Charging),
            time_spent_charging: None,
            stopped_reason: None,
            remote_start_id,
            custom_data: None,
        },
        offline: None,
        number_of_phases_used: None,
        cable_max_current: None,
        reservation_id: None,
        evse: Some(evse_of(session)),
        meter_value: Some(energy_meter_value(
            meter_start_wh,
            ReadingContextEnumType::TransactionBegin,
            timestamp,
        )),
        id_token: Some(id_token_of(id_tag)),
        custom_data: None,
    }
}

/// The periodic `Sample.Periodic` meter value: the active-import energy reading,
/// plus — when `bounded_power_w` is `Some` — a `Power.Active.Import` reading of
/// the profile-bounded power the station may draw (slice 7e, Issue #455).
///
/// A `TxProfile` installed by a `RequestStartTransaction` becomes *binding* here:
/// once its `chargingSchedule` limit is tighter than the connector's natural
/// rate, the station reports the bounded power it is actually drawing. Without a
/// binding profile (`None`) the array is byte-for-byte the single energy sample
/// the unbounded path always emitted, so the 1.6J and profile-less 2.0.1 paths
/// are unchanged.
fn periodic_meter_value(
    energy_wh: f64,
    bounded_power_w: Option<f64>,
    timestamp: &str,
) -> Vec<MeterValueType> {
    let mut sampled_value = vec![SampledValueType {
        value: energy_wh,
        context: Some(ReadingContextEnumType::SamplePeriodic),
        measurand: Some(MeasurandEnumType::EnergyActiveImportRegister),
        phase: None,
        location: None,
        signed_meter_value: None,
        unit_of_measure: None,
        custom_data: None,
    }];
    if let Some(power_w) = bounded_power_w {
        sampled_value.push(SampledValueType {
            value: power_w,
            context: Some(ReadingContextEnumType::SamplePeriodic),
            measurand: Some(MeasurandEnumType::PowerActiveImport),
            phase: None,
            location: None,
            signed_meter_value: None,
            // Power.Active.Import defaults to W, so `unitOfMeasure` is omitted —
            // the same default-unit convention the energy sample uses for Wh.
            unit_of_measure: None,
            custom_data: None,
        });
    }
    vec![MeterValueType {
        timestamp: timestamp.to_string(),
        sampled_value,
        custom_data: None,
    }]
}

/// Build a `TransactionEvent(Updated)` — a periodic mid-transaction sample.
///
/// `triggerReason = MeterValuePeriodic`, `chargingState = Charging`, and a
/// `Sample.Periodic` meter reading. `seq_no` must increment across the
/// transaction's events so the CSMS can detect gaps and reorder. No `idToken`
/// (the session is already authorized). Replaces the 1.6J periodic `MeterValues`
/// CALL for a `V201` charge point.
///
/// `bounded_power_w` carries the profile-bounded power when a `TxProfile`
/// installed on this EVSE imposes a limit tighter than the connector's natural
/// rate (slice 7e, Issue #455); it adds a `Power.Active.Import` sample alongside
/// the energy reading. `None` — no profile, or a profile no tighter than the
/// natural rate — leaves the reading exactly as the unbounded path emits it.
pub fn transaction_event_updated(
    session: &SessionRef,
    seq_no: i32,
    meter_wh: f64,
    bounded_power_w: Option<f64>,
    timestamp: &str,
) -> TransactionEventRequest {
    TransactionEventRequest {
        event_type: TransactionEventEnumType::Updated,
        timestamp: timestamp.to_string(),
        trigger_reason: TriggerReasonEnumType::MeterValuePeriodic,
        seq_no,
        transaction_info: TransactionType {
            transaction_id: session.transaction_id.to_string(),
            charging_state: Some(ChargingStateEnumType::Charging),
            time_spent_charging: None,
            stopped_reason: None,
            remote_start_id: None,
            custom_data: None,
        },
        offline: None,
        number_of_phases_used: None,
        cable_max_current: None,
        reservation_id: None,
        evse: Some(evse_of(session)),
        meter_value: Some(periodic_meter_value(meter_wh, bounded_power_w, timestamp)),
        id_token: None,
        custom_data: None,
    }
}

/// Build a `TransactionEvent(Ended)` — the final event of a transaction.
///
/// `chargingState = Idle`, a `Transaction.End` meter reading, and both 2.0.1
/// stop fields derived from the 1.6J `reason`: `transactionInfo.stoppedReason`
/// via [`reason_to_v201`] and `triggerReason` via [`reason_to_v201_trigger`].
/// The `idToken` is echoed so a CSMS that authorizes stops can match it.
/// `seq_no` must be the next value after the last `Updated`. Replaces the 1.6J
/// `StopTransaction` CALL for a `V201` charge point.
pub fn transaction_event_ended(
    session: &SessionRef,
    seq_no: i32,
    id_tag: &str,
    meter_stop_wh: f64,
    reason: Reason,
    timestamp: &str,
) -> TransactionEventRequest {
    TransactionEventRequest {
        event_type: TransactionEventEnumType::Ended,
        timestamp: timestamp.to_string(),
        trigger_reason: reason_to_v201_trigger(reason),
        seq_no,
        transaction_info: TransactionType {
            transaction_id: session.transaction_id.to_string(),
            charging_state: Some(ChargingStateEnumType::Idle),
            time_spent_charging: None,
            stopped_reason: Some(reason_to_v201(reason)),
            remote_start_id: None,
            custom_data: None,
        },
        offline: None,
        number_of_phases_used: None,
        cable_max_current: None,
        reservation_id: None,
        evse: Some(evse_of(session)),
        meter_value: Some(energy_meter_value(
            meter_stop_wh,
            ReadingContextEnumType::TransactionEnd,
            timestamp,
        )),
        id_token: Some(id_token_of(id_tag)),
        custom_data: None,
    }
}

/// A single-sample `meterValue` array carrying one active-import energy reading
/// tagged `ReadingContext::Trigger` — the reading a station reports for an
/// on-demand `TriggerMessage(MeterValues)`.
///
/// The 2.0.1 twin of the 1.6J triggered-`MeterValues` reading: same
/// `Energy.Active.Import.Register` sample as the periodic path, but the reading
/// `context` records that a `TriggerMessage` prompted it rather than a periodic
/// sampler. Exposed (unlike the private per-event builders) because a standalone
/// `MeterValues` CALL is built directly from it, outside the `TransactionEvent`
/// flow.
#[must_use]
pub fn triggered_energy_meter_value(energy_wh: f64, timestamp: &str) -> Vec<MeterValueType> {
    energy_meter_value(energy_wh, ReadingContextEnumType::Trigger, timestamp)
}

/// Build a `TransactionEvent(Updated)` for an on-demand
/// `TriggerMessage(TransactionEvent)` — a mid-transaction sample the CSMS asked
/// for *now*.
///
/// Identical wire shape to [`transaction_event_updated`] (a `Charging` update
/// carrying the current reading, no `idToken`), except the `triggerReason` is
/// [`Trigger`](TriggerReasonEnumType::Trigger) — the event was prompted by a
/// `TriggerMessage`, not the periodic sampler — and the reading `context` is
/// `Trigger` to match. `seq_no` must still be drawn from the transaction's shared
/// counter so a triggered event interleaves cleanly with the periodic samples.
#[must_use]
pub fn transaction_event_triggered(
    session: &SessionRef,
    seq_no: i32,
    meter_wh: f64,
    timestamp: &str,
) -> TransactionEventRequest {
    TransactionEventRequest {
        event_type: TransactionEventEnumType::Updated,
        timestamp: timestamp.to_string(),
        trigger_reason: TriggerReasonEnumType::Trigger,
        seq_no,
        transaction_info: TransactionType {
            transaction_id: session.transaction_id.to_string(),
            charging_state: Some(ChargingStateEnumType::Charging),
            time_spent_charging: None,
            stopped_reason: None,
            remote_start_id: None,
            custom_data: None,
        },
        offline: None,
        number_of_phases_used: None,
        cable_max_current: None,
        reservation_id: None,
        evse: Some(evse_of(session)),
        meter_value: Some(energy_meter_value(
            meter_wh,
            ReadingContextEnumType::Trigger,
            timestamp,
        )),
        id_token: None,
        custom_data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::SchemaValidator;

    const TS: &str = "2027-01-01T00:00:00Z";

    fn session() -> SessionRef<'static> {
        SessionRef {
            transaction_id: "42",
            evse_id: 1,
            connector_id: 1,
        }
    }

    /// Every 1.6J stop reason maps to a 2.0.1 `stoppedReason`; spot-check the
    /// name-preserving cases and the three intent-mapped ones.
    #[test]
    fn reason_maps_to_v201_stopped_reason() {
        assert_eq!(
            reason_to_v201(Reason::EmergencyStop),
            ReasonEnumType::EmergencyStop
        );
        assert_eq!(
            reason_to_v201(Reason::EVDisconnected),
            ReasonEnumType::EVDisconnected
        );
        assert_eq!(reason_to_v201(Reason::Local), ReasonEnumType::Local);
        assert_eq!(reason_to_v201(Reason::Other), ReasonEnumType::Other);
        assert_eq!(reason_to_v201(Reason::PowerLoss), ReasonEnumType::PowerLoss);
        assert_eq!(reason_to_v201(Reason::Reboot), ReasonEnumType::Reboot);
        assert_eq!(reason_to_v201(Reason::Remote), ReasonEnumType::Remote);
        assert_eq!(
            reason_to_v201(Reason::DeAuthorized),
            ReasonEnumType::DeAuthorized
        );
        // Intent-mapped: hard reset is immediate, soft reset is a graceful
        // reboot, and an unlock command is a remotely commanded stop.
        assert_eq!(
            reason_to_v201(Reason::HardReset),
            ReasonEnumType::ImmediateReset
        );
        assert_eq!(reason_to_v201(Reason::SoftReset), ReasonEnumType::Reboot);
        assert_eq!(
            reason_to_v201(Reason::UnlockCommand),
            ReasonEnumType::Remote
        );
    }

    /// Every 1.6J stop reason maps to a 2.0.1 `triggerReason`.
    #[test]
    fn reason_maps_to_v201_trigger_reason() {
        use TriggerReasonEnumType as T;
        assert_eq!(
            reason_to_v201_trigger(Reason::EVDisconnected),
            T::EVDeparted
        );
        assert_eq!(reason_to_v201_trigger(Reason::Remote), T::RemoteStop);
        assert_eq!(
            reason_to_v201_trigger(Reason::UnlockCommand),
            T::UnlockCommand
        );
        assert_eq!(reason_to_v201_trigger(Reason::HardReset), T::ResetCommand);
        assert_eq!(reason_to_v201_trigger(Reason::SoftReset), T::ResetCommand);
        assert_eq!(reason_to_v201_trigger(Reason::Reboot), T::ResetCommand);
        assert_eq!(
            reason_to_v201_trigger(Reason::DeAuthorized),
            T::Deauthorized
        );
        assert_eq!(
            reason_to_v201_trigger(Reason::EmergencyStop),
            T::AbnormalCondition
        );
        assert_eq!(
            reason_to_v201_trigger(Reason::PowerLoss),
            T::AbnormalCondition
        );
        assert_eq!(reason_to_v201_trigger(Reason::Local), T::StopAuthorized);
        assert_eq!(reason_to_v201_trigger(Reason::Other), T::StopAuthorized);
    }

    #[test]
    fn started_event_carries_started_shape() {
        let s = session();
        let req = transaction_event_started(&s, "RFID-CAFE", 1000.0, TS, None);

        assert_eq!(req.event_type, TransactionEventEnumType::Started);
        assert_eq!(req.trigger_reason, TriggerReasonEnumType::Authorized);
        assert_eq!(req.seq_no, 0);
        assert_eq!(req.transaction_info.transaction_id, "42");
        assert_eq!(
            req.transaction_info.charging_state,
            Some(ChargingStateEnumType::Charging)
        );
        assert!(req.transaction_info.stopped_reason.is_none());
        // A locally initiated start has no remote-start request to correlate.
        assert!(
            req.transaction_info.remote_start_id.is_none(),
            "a local start carries no remoteStartId"
        );
        let evse = req.evse.as_ref().expect("started carries evse");
        assert_eq!(evse.id, 1);
        assert_eq!(evse.connector_id, Some(1));
        let token = req.id_token.as_ref().expect("started carries idToken");
        assert_eq!(token.id_token, "RFID-CAFE");
        assert_eq!(token.kind, IdTokenEnumType::Iso14443);
        let mv = req
            .meter_value
            .as_ref()
            .expect("started carries meterValue");
        assert_eq!(mv[0].sampled_value[0].value, 1000.0);
        assert_eq!(
            mv[0].sampled_value[0].context,
            Some(ReadingContextEnumType::TransactionBegin)
        );
    }

    #[test]
    fn started_event_carries_remote_start_id_when_remotely_initiated() {
        let s = session();
        // A CSMS RequestStartTransaction carrying remoteStartId = 4242 initiated
        // this session; the Started event must echo it so the CSMS can correlate.
        let req = transaction_event_started(&s, "RFID-CAFE", 1000.0, TS, Some(4242));

        assert_eq!(
            req.transaction_info.remote_start_id,
            Some(4242),
            "a remotely initiated start echoes the request's remoteStartId"
        );
    }

    #[test]
    fn updated_event_carries_periodic_shape() {
        let s = session();
        let req = transaction_event_updated(&s, 3, 1500.0, None, TS);

        assert_eq!(req.event_type, TransactionEventEnumType::Updated);
        assert_eq!(
            req.trigger_reason,
            TriggerReasonEnumType::MeterValuePeriodic
        );
        assert_eq!(req.seq_no, 3);
        // A mid-session sample is not an authorization event.
        assert!(req.id_token.is_none());
        let mv = req
            .meter_value
            .as_ref()
            .expect("updated carries meterValue");
        // With no binding profile the reading is the single energy sample the
        // unbounded path always emitted — nothing added.
        assert_eq!(mv[0].sampled_value.len(), 1);
        assert_eq!(
            mv[0].sampled_value[0].measurand,
            Some(MeasurandEnumType::EnergyActiveImportRegister)
        );
        assert_eq!(
            mv[0].sampled_value[0].context,
            Some(ReadingContextEnumType::SamplePeriodic)
        );
    }

    #[test]
    fn updated_event_appends_a_bounded_power_sample_when_profile_binds() {
        let s = session();
        // A binding TxProfile bounds the station to 3 680 W; the periodic sample
        // surfaces that as a Power.Active.Import reading alongside the energy one.
        let req = transaction_event_updated(&s, 3, 1500.0, Some(3_680.0), TS);

        let mv = req
            .meter_value
            .as_ref()
            .expect("updated carries meterValue");
        assert_eq!(
            mv[0].sampled_value.len(),
            2,
            "a binding profile adds a power sample beside the energy one"
        );
        // The energy sample is still first and unchanged.
        assert_eq!(
            mv[0].sampled_value[0].measurand,
            Some(MeasurandEnumType::EnergyActiveImportRegister)
        );
        assert_eq!(mv[0].sampled_value[0].value, 1500.0);
        // The appended sample is the bounded active-import power, SamplePeriodic,
        // default (W) unit.
        let power = &mv[0].sampled_value[1];
        assert_eq!(power.measurand, Some(MeasurandEnumType::PowerActiveImport));
        assert_eq!(power.value, 3_680.0);
        assert_eq!(power.context, Some(ReadingContextEnumType::SamplePeriodic));
        assert!(
            power.unit_of_measure.is_none(),
            "Power.Active.Import defaults to W, so unitOfMeasure is omitted"
        );
    }

    #[test]
    fn ended_event_carries_ended_shape() {
        let s = session();
        let req = transaction_event_ended(&s, 4, "RFID-CAFE", 2000.0, Reason::EVDisconnected, TS);

        assert_eq!(req.event_type, TransactionEventEnumType::Ended);
        // EV unplug: EVDeparted trigger, EVDisconnected stopped-reason.
        assert_eq!(req.trigger_reason, TriggerReasonEnumType::EVDeparted);
        assert_eq!(req.seq_no, 4);
        assert_eq!(
            req.transaction_info.stopped_reason,
            Some(ReasonEnumType::EVDisconnected)
        );
        assert_eq!(
            req.transaction_info.charging_state,
            Some(ChargingStateEnumType::Idle)
        );
        let mv = req.meter_value.as_ref().expect("ended carries meterValue");
        assert_eq!(
            mv[0].sampled_value[0].context,
            Some(ReadingContextEnumType::TransactionEnd)
        );
    }

    #[test]
    fn triggered_event_carries_trigger_reason_and_context() {
        let s = session();
        let req = transaction_event_triggered(&s, 7, 1750.0, TS);

        // Same Updated shape as a periodic sample, but flagged as trigger-driven.
        assert_eq!(req.event_type, TransactionEventEnumType::Updated);
        assert_eq!(req.trigger_reason, TriggerReasonEnumType::Trigger);
        assert_eq!(req.seq_no, 7);
        assert!(req.id_token.is_none());
        assert_eq!(
            req.transaction_info.charging_state,
            Some(ChargingStateEnumType::Charging)
        );
        let mv = req
            .meter_value
            .as_ref()
            .expect("triggered event carries meterValue");
        assert_eq!(mv[0].sampled_value[0].value, 1750.0);
        assert_eq!(
            mv[0].sampled_value[0].context,
            Some(ReadingContextEnumType::Trigger)
        );
    }

    #[test]
    fn triggered_meter_value_is_a_trigger_context_energy_reading() {
        let mv = triggered_energy_meter_value(1234.0, TS);
        assert_eq!(mv.len(), 1);
        let sample = &mv[0].sampled_value[0];
        assert_eq!(sample.value, 1234.0);
        assert_eq!(sample.context, Some(ReadingContextEnumType::Trigger));
        assert_eq!(
            sample.measurand,
            Some(MeasurandEnumType::EnergyActiveImportRegister)
        );
    }

    /// Wire fidelity: a standalone 2.0.1 `MeterValues` CALL built from a triggered
    /// reading satisfies the bundled `MeterValues` JSON Schema — the same
    /// guarantee the CP's version-aware validator gives on the live path.
    #[test]
    fn triggered_meter_values_message_is_schema_valid() {
        use ocpp_messages::v201::MeterValuesRequest;
        let req = MeterValuesRequest {
            evse_id: 1,
            meter_value: triggered_energy_meter_value(1234.0, TS),
            custom_data: None,
        };
        let payload = serde_json::to_value(&req).unwrap();
        assert!(
            SchemaValidator::v201()
                .validate_call("MeterValues", &payload)
                .is_ok(),
            "a triggered MeterValues CALL should be schema-valid, got: {payload}"
        );
    }

    /// Wire fidelity: each built event must satisfy the bundled OCPP 2.0.1
    /// `TransactionEvent` JSON Schema — the same guarantee the CP's version-aware
    /// validator gives on the live path.
    #[test]
    fn built_events_are_schema_valid() {
        let s = session();
        let validator = SchemaValidator::v201();
        for req in [
            transaction_event_started(&s, "RFID-CAFE", 1000.0, TS, Some(99)),
            transaction_event_updated(&s, 1, 1500.0, None, TS),
            // The bounded-power variant must also satisfy the schema — a second
            // sampled value on the periodic reading.
            transaction_event_updated(&s, 2, 1500.0, Some(3_680.0), TS),
            transaction_event_ended(&s, 3, "RFID-CAFE", 2000.0, Reason::Remote, TS),
            transaction_event_triggered(&s, 4, 1750.0, TS),
        ] {
            let payload = serde_json::to_value(&req).unwrap();
            assert!(
                validator
                    .validate_call("TransactionEvent", &payload)
                    .is_ok(),
                "built {:?} TransactionEvent should be schema-valid, got: {payload}",
                req.event_type
            );
        }
    }
}
