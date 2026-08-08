//! v201 `TxProfile` store — transaction-scoped charging profiles installed by an
//! accepted `RequestStartTransaction` (slice 7d, Issue #450).
//!
//! The 1.6J [`ChargingProfileStore`](crate::charging_profiles::ChargingProfileStore)
//! is typed on [`ocpp_types::v16j::ChargingProfile`]; the 2.0.1
//! [`ChargingProfileType`] is a distinct, richer type (an array of
//! `ChargingSchedule`s, string `transactionId`, `validFrom`/`validTo`, …), so
//! honoring a 2.0.1 `TxProfile` needs its own store rather than a lossy shoehorn
//! into the 1.6J one — the design seam Issue #450 calls out.
//!
//! A `TxProfile` bounds exactly one transaction (OCPP 2.0.1 Part 2, §K01), so at
//! most one is installed per EVSE at a time: the store is keyed by EVSE id, a
//! second start on the same EVSE replaces the first, and the profile is cleared
//! the moment its transaction ends. That install→read→clear lifecycle is driven
//! by [`ChargePoint::open_transaction`](crate::ChargePoint) /
//! [`close_transaction`](crate::ChargePoint) — the store holds no state its
//! owners do not manage.
//!
//! Enforcing the schedule (bounding the periodic `TransactionEvent(Updated)`
//! reading by the profile's limit) is deliberately *not* this store's job — it
//! is the enforcement follow-up. This slice makes the installed profile
//! observable; the follow-up makes it binding.

use std::collections::HashMap;

use ocpp_types::v201::{
    ChargingProfileKindEnumType, ChargingProfileType, ChargingRateUnitEnumType,
    ChargingScheduleType, RecurrencyKindEnumType,
};
use ocpp_types::{DateTime, Utc};
use tokio::sync::RwLock;

/// A v201-typed store of installed `TxProfile`s, keyed by EVSE id.
///
/// Populated by an accepted `RequestStartTransaction` (via
/// [`ChargePoint::open_transaction`](crate::ChargePoint), atomically with the
/// session it bounds) and drained when that transaction ends. Interior-mutable
/// behind an [`RwLock`] so a single `Arc<V201TxProfileStore>` can be shared
/// across the charge point's tasks, mirroring the 1.6J
/// [`ChargingProfileStore`](crate::charging_profiles::ChargingProfileStore).
#[derive(Debug, Default)]
pub struct V201TxProfileStore {
    /// EVSE id → the single `TxProfile` currently bounding its live transaction.
    by_evse: RwLock<HashMap<i32, ChargingProfileType>>,
}

impl V201TxProfileStore {
    /// A new, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install `profile` against `evse_id`, replacing any profile already
    /// installed there.
    ///
    /// A `TxProfile` is transaction-scoped, so a new transaction on an EVSE that
    /// somehow still held a stale profile (a lost teardown) supersedes it rather
    /// than stacking — the last accepted start wins, matching how a fresh
    /// transaction relegates any earlier one on the connector.
    pub async fn install(&self, evse_id: i32, profile: ChargingProfileType) {
        self.by_evse.write().await.insert(evse_id, profile);
    }

    /// The profile currently installed on `evse_id`, if any.
    ///
    /// A cloned snapshot: the caller inspects it without holding the store lock.
    pub async fn get(&self, evse_id: i32) -> Option<ChargingProfileType> {
        self.by_evse.read().await.get(&evse_id).cloned()
    }

    /// Remove the profile installed on `evse_id`, returning it if one was
    /// present. Idempotent — clearing an EVSE that holds no profile is a no-op
    /// returning `None` (a locally-started transaction, or one that carried no
    /// `chargingProfile`, installs nothing to clear).
    pub async fn clear(&self, evse_id: i32) -> Option<ChargingProfileType> {
        self.by_evse.write().await.remove(&evse_id)
    }
}

/// Parse an optional RFC 3339 timestamp field (`validFrom`, `validTo`,
/// `startSchedule`) into a UTC instant, tolerating a malformed value as absent.
///
/// The store's profiles arrive from an accepted `RequestStartTransaction` and so
/// have passed schema validation, but the schema constrains only the *shape* of a
/// `date-time` string; a value the `chrono` parser rejects is treated as "field
/// not set" rather than allowed to abort resolution — a lenient trust boundary on
/// CSMS-supplied input, matching how [`active_limit`] never panics on a profile.
fn parse_rfc3339(field: &Option<String>) -> Option<DateTime<Utc>> {
    field
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// The absolute instant a `schedule` is anchored to.
///
/// Mirrors the 1.6J `composite::anchor`: a `Relative` schedule is offset from the
/// transaction start (`tx_start`); `Absolute`/`Recurring` schedules anchor on
/// their `startSchedule` when present (the recurrence base for `Recurring` — only
/// its time-of-day/-week phase matters), else fall back to `tx_start`.
fn anchor(
    profile: &ChargingProfileType,
    schedule: &ChargingScheduleType,
    tx_start: DateTime<Utc>,
) -> DateTime<Utc> {
    match profile.charging_profile_kind {
        ChargingProfileKindEnumType::Relative => tx_start,
        ChargingProfileKindEnumType::Absolute | ChargingProfileKindEnumType::Recurring => {
            parse_rfc3339(&schedule.start_schedule).unwrap_or(tx_start)
        }
    }
}

/// The recurrence period in seconds for a `Recurring` profile, or `None` for a
/// non-recurring profile (and for a malformed `Recurring` profile carrying no
/// `recurrencyKind`, which cannot be unrolled and so evaluates once, like
/// `Absolute`). Mirrors the 1.6J `composite::recurrence_period`.
fn recurrence_period(profile: &ChargingProfileType) -> Option<i64> {
    if profile.charging_profile_kind != ChargingProfileKindEnumType::Recurring {
        return None;
    }
    match profile.recurrency_kind {
        Some(RecurrencyKindEnumType::Daily) => Some(86_400),
        Some(RecurrencyKindEnumType::Weekly) => Some(604_800),
        None => None,
    }
}

/// Resolve the charging limit a `TxProfile` imposes `elapsed_s` seconds into the
/// transaction it bounds, as `(limit, unit)` — or `None` if the profile imposes
/// no limit at that offset.
///
/// The `chargingSchedule` (OCPP 2.0.1 Part 2, §K01 / the Python reference's
/// [`ocpp.v201.call.RequestStartTransaction`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)
/// `chargingProfile.chargingSchedule`) is a list of `chargingSchedulePeriod`s,
/// each starting at a `startPeriod` second-offset from the schedule's start and
/// holding a flat `limit` in the schedule's `chargingRateUnit` until the next
/// period begins. The limit in force is the period with the **greatest
/// `startPeriod ≤ offset`**, where `offset` is the position *within the schedule*
/// of the sample instant `tx_start + elapsed_s`.
///
/// This is the composite-schedule resolution (Issue #464), generalizing the flat
/// single-period first slice (#455). It still honors only the profile's *first*
/// `chargingSchedule` (a `TxProfile` carries one), but now:
///
/// * **anchors by kind** — `Relative` schedules are offset from `tx_start`,
///   `Absolute`/`Recurring` from `startSchedule` (so `elapsed_s` is mapped to a
///   schedule-relative offset before period selection);
/// * **honors `validFrom`/`validTo`** — a profile whose validity window does not
///   contain the sample instant imposes no limit (`None`);
/// * **honors the schedule `duration`** — once the schedule has elapsed the
///   resolution falls back to no-limit rather than pinning the last period
///   forever; for `Recurring` profiles the offset wraps into the daily/weekly
///   recurrence period (phased off `startSchedule`) so each occurrence's
///   `duration` bounds that occurrence's active span.
///
/// Returns `None` when the profile is outside its validity window, past its
/// schedule `duration`, or has no period yet in force (an empty period list, or
/// every `startPeriod` still in the future) — so the caller reports the unbounded
/// reading rather than inventing a limit of `0`.
///
/// **`minChargingRate` floor (Issue #467):** the resolved period `limit` is
/// floored at the schedule's `minChargingRate` — the EV's minimum draw (OCPP
/// 2.0.1 Part 2, §K01 / the Python reference's
/// [`ocpp.v201.datatypes.ChargingScheduleType.min_charging_rate`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/datatypes.py)).
/// A period whose `limit` dips below the floor is lifted to it, expressed in the
/// schedule's own `chargingRateUnit`, *before* any A→W conversion in
/// [`bounded_power_w`]. The floor changes only the schedule-unit value returned,
/// never whether that value ends up binding against the connector's natural rate.
/// A `minChargingRate`-absent schedule (the common case) returns the period
/// `limit` unchanged.
#[must_use]
pub fn active_limit(
    profile: &ChargingProfileType,
    elapsed_s: i32,
    tx_start: DateTime<Utc>,
) -> Option<(f64, ChargingRateUnitEnumType)> {
    // The absolute instant of this sample: `elapsed_s` seconds into the
    // transaction. `validFrom`/`validTo` and an `Absolute`/`Recurring` anchor are
    // calendar quantities, so period selection is done against this instant, not
    // the bare relative offset.
    let now = tx_start + chrono::Duration::seconds(i64::from(elapsed_s));

    if let Some(valid_from) = parse_rfc3339(&profile.valid_from) {
        if now < valid_from {
            return None;
        }
    }
    if let Some(valid_to) = parse_rfc3339(&profile.valid_to) {
        if now > valid_to {
            return None;
        }
    }

    let schedule = profile.charging_schedule.first()?;
    let raw_offset = (now - anchor(profile, schedule, tx_start)).num_seconds();
    // `Recurring`: map the instant into `[0, period)` phased off the recurrence
    // base — the pattern is fully periodic, with `validFrom`/`validTo` bounding
    // the calendar range. Non-recurring schedules evaluate once and do not apply
    // before their anchor.
    let offset = match recurrence_period(profile) {
        Some(period) => raw_offset.rem_euclid(period),
        None => {
            if raw_offset < 0 {
                return None;
            }
            raw_offset
        }
    };
    // `duration` bounds the active span — of the single schedule for a
    // non-recurring profile, or of each occurrence for a recurring one (the rest
    // of the recurrence period is an unconstrained gap → no limit).
    if let Some(duration) = schedule.duration {
        if offset >= i64::from(duration) {
            return None;
        }
    }

    // The applicable period is the last one whose `startPeriod` has been reached.
    // Periods are kept in `startPeriod` order per the spec; tolerate unordered
    // input by scanning for the greatest start that is ≤ the schedule offset.
    let period = schedule
        .charging_schedule_period
        .iter()
        .filter(|p| i64::from(p.start_period) <= offset)
        .max_by_key(|p| p.start_period)?;
    // Floor the period limit at the schedule's `minChargingRate`, in the
    // schedule's `chargingRateUnit` (so an ampere floor lifts an ampere limit, a
    // watt floor a watt limit). `f64::max` is total — a floor at or below the
    // period limit is a no-op, and even a degenerate floor cannot panic — so a
    // CSMS-supplied `minChargingRate` only ever raises the schedule-unit value,
    // never poisons resolution.
    let limit = match schedule.min_charging_rate {
        Some(floor) => period.limit.max(floor),
        None => period.limit,
    };
    Some((limit, schedule.charging_rate_unit))
}

/// The charging power (in watts) `profile` binds the connector to `elapsed_s`
/// into the transaction, given the connector's unbounded `natural_power_w` and
/// its `nominal_voltage_v` (for an ampere limit's A→W conversion) — or `None`
/// when the profile imposes **no binding** power limit at this offset.
///
/// `None` is returned both when [`active_limit`] finds no period in force and
/// when the resolved limit is **at or above** the connector's natural rate: a
/// limit no tighter than what the connector would draw anyway does not bend the
/// reading, so the caller leaves the periodic sample untouched (the issue's "a
/// profile above the natural rate is unchanged"). Only a genuinely-tighter
/// limit yields `Some(bounded_power)`, which the sampler surfaces as a
/// `Power.Active.Import` reading on the `TransactionEvent(Updated)`.
///
/// The limit fed in has already been floored at the schedule's
/// `minChargingRate` by [`active_limit`] (Issue #467), so a schedule that
/// declares a floor above the natural rate simply yields no bind here.
///
/// An ampere limit is converted at the connector's nominal voltage,
/// single-phase (`limit_a × nominal_voltage_v`); a poly-phase conversion that
/// honors the period's `numberPhases` is a follow-up, so a three-phase station
/// is bounded conservatively (to its single-phase equivalent) rather than
/// wrongly. A watt limit is used directly.
#[must_use]
pub fn bounded_power_w(
    profile: &ChargingProfileType,
    elapsed_s: i32,
    tx_start: DateTime<Utc>,
    natural_power_w: f64,
    nominal_voltage_v: f64,
) -> Option<f64> {
    let (limit, unit) = active_limit(profile, elapsed_s, tx_start)?;
    let limit_w = match unit {
        ChargingRateUnitEnumType::W => limit,
        ChargingRateUnitEnumType::A => limit * nominal_voltage_v,
    };
    (limit_w < natural_power_w).then_some(limit_w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        ChargingProfilePurposeEnumType, ChargingRateUnitEnumType, ChargingSchedulePeriodType,
    };

    /// A fixed transaction-start anchor for resolution tests, so `validFrom` /
    /// `validTo` / `startSchedule` can be positioned relative to a known instant.
    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("valid RFC 3339")
            .with_timezone(&Utc)
    }

    /// A minimal schema-shaped `TxProfile` bounding the session to one flat
    /// power limit, tagged with `id` so tests can tell two profiles apart.
    fn tx_profile(id: i32, limit: f64) -> ChargingProfileType {
        ChargingProfileType {
            id,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeEnumType::TxProfile,
            charging_profile_kind: ChargingProfileKindEnumType::Relative,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: ChargingRateUnitEnumType::W,
                charging_schedule_period: vec![ChargingSchedulePeriodType {
                    start_period: 0,
                    limit,
                    number_phases: None,
                    phase_to_use: None,
                    custom_data: None,
                }],
                start_schedule: None,
                duration: None,
                min_charging_rate: None,
                sales_tariff: None,
                custom_data: None,
            }],
            recurrency_kind: None,
            valid_from: None,
            valid_to: None,
            transaction_id: None,
            custom_data: None,
        }
    }

    #[tokio::test]
    async fn get_returns_none_for_an_evse_with_no_profile() {
        let store = V201TxProfileStore::new();
        assert_eq!(store.get(1).await, None);
    }

    #[tokio::test]
    async fn install_then_get_round_trips_the_profile() {
        let store = V201TxProfileStore::new();
        let profile = tx_profile(7, 11_000.0);
        store.install(2, profile.clone()).await;
        assert_eq!(store.get(2).await, Some(profile));
        // Scoped to its EVSE — a sibling EVSE is unaffected.
        assert_eq!(store.get(1).await, None);
    }

    #[tokio::test]
    async fn reinstalling_on_the_same_evse_replaces_the_previous_profile() {
        let store = V201TxProfileStore::new();
        store.install(1, tx_profile(1, 11_000.0)).await;
        let replacement = tx_profile(2, 7_400.0);
        store.install(1, replacement.clone()).await;
        assert_eq!(
            store.get(1).await,
            Some(replacement),
            "the last accepted start wins; profiles do not stack"
        );
    }

    #[tokio::test]
    async fn clear_removes_and_returns_the_profile_then_is_a_noop() {
        let store = V201TxProfileStore::new();
        let profile = tx_profile(9, 22_000.0);
        store.install(3, profile.clone()).await;
        assert_eq!(
            store.clear(3).await,
            Some(profile),
            "clear returns what it removed"
        );
        assert_eq!(store.get(3).await, None, "the profile is gone after clear");
        assert_eq!(
            store.clear(3).await,
            None,
            "clearing an empty EVSE is a no-op"
        );
    }

    /// A `TxProfile` whose single schedule steps through several periods, so
    /// `active_limit` has more than one candidate to resolve by `elapsed`. Each
    /// `(start_period, limit)` pair is one step in the schedule.
    fn stepped_profile(
        unit: ChargingRateUnitEnumType,
        steps: &[(i32, f64)],
    ) -> ChargingProfileType {
        ChargingProfileType {
            id: 1,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeEnumType::TxProfile,
            charging_profile_kind: ChargingProfileKindEnumType::Relative,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: unit,
                charging_schedule_period: steps
                    .iter()
                    .map(|&(start_period, limit)| ChargingSchedulePeriodType {
                        start_period,
                        limit,
                        number_phases: None,
                        phase_to_use: None,
                        custom_data: None,
                    })
                    .collect(),
                start_schedule: None,
                duration: None,
                min_charging_rate: None,
                sales_tariff: None,
                custom_data: None,
            }],
            recurrency_kind: None,
            valid_from: None,
            valid_to: None,
            transaction_id: None,
            custom_data: None,
        }
    }

    #[test]
    fn active_limit_resolves_the_first_period_of_a_flat_schedule() {
        let profile = tx_profile(1, 11_000.0);
        // A single period at startPeriod 0 is in force from the very start and
        // stays in force however far in we look.
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((11_000.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 3_600, t0()),
            Some((11_000.0, ChargingRateUnitEnumType::W))
        );
    }

    #[test]
    fn active_limit_picks_the_period_in_force_at_the_elapsed_offset() {
        // 7.4 kW for the first 30 min, then a taper to 3.68 kW.
        let profile = stepped_profile(
            ChargingRateUnitEnumType::W,
            &[(0, 7_400.0), (1_800, 3_680.0)],
        );
        // Before the second period begins, the first is in force.
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 1_799, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        // At and after its startPeriod, the second period wins (greatest
        // startPeriod ≤ elapsed).
        assert_eq!(
            active_limit(&profile, 1_800, t0()),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 10_000, t0()),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
    }

    #[test]
    fn active_limit_is_none_when_no_period_is_yet_in_force_or_the_schedule_is_empty() {
        // Every period starts in the future relative to elapsed 0 → nothing in
        // force yet.
        let deferred = stepped_profile(ChargingRateUnitEnumType::W, &[(600, 7_400.0)]);
        assert_eq!(active_limit(&deferred, 0, t0()), None);
        assert_eq!(active_limit(&deferred, 599, t0()), None);
        assert_eq!(
            active_limit(&deferred, 600, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W)),
            "the deferred period comes into force at its startPeriod"
        );

        // A degenerate schedule with no periods resolves to nothing rather than
        // panicking or inventing a limit.
        let empty = stepped_profile(ChargingRateUnitEnumType::W, &[]);
        assert_eq!(active_limit(&empty, 0, t0()), None);
    }

    #[test]
    fn bounded_power_w_binds_only_a_watt_limit_below_the_natural_rate() {
        let natural = 7_360.0;
        // A 3.68 kW limit is tighter than the 7.36 kW connector → binds.
        let tight = tx_profile(1, 3_680.0);
        assert_eq!(
            bounded_power_w(&tight, 0, t0(), natural, 230.0),
            Some(3_680.0)
        );

        // An 11 kW limit is looser than the connector's natural rate → no bind,
        // the reading is left unchanged.
        let loose = tx_profile(2, 11_000.0);
        assert_eq!(bounded_power_w(&loose, 0, t0(), natural, 230.0), None);

        // A limit exactly at the natural rate is not *tighter*, so it does not
        // bend the reading either.
        let at_rate = tx_profile(3, natural);
        assert_eq!(bounded_power_w(&at_rate, 0, t0(), natural, 230.0), None);
    }

    #[test]
    fn bounded_power_w_converts_an_ampere_limit_at_nominal_voltage() {
        let natural = 7_360.0;
        let voltage = 230.0;
        // 16 A × 230 V = 3 680 W, below the natural rate → binds at the
        // converted wattage.
        let amps = stepped_profile(ChargingRateUnitEnumType::A, &[(0, 16.0)]);
        assert_eq!(
            bounded_power_w(&amps, 0, t0(), natural, voltage),
            Some(16.0 * voltage)
        );
        // 32 A × 230 V = 7 360 W == natural rate → not tighter → no bind.
        let full = stepped_profile(ChargingRateUnitEnumType::A, &[(0, 32.0)]);
        assert_eq!(bounded_power_w(&full, 0, t0(), natural, voltage), None);
    }

    #[test]
    fn bounded_power_w_is_none_when_no_period_is_in_force() {
        // No period yet in force → nothing to bind, whatever the connector rate.
        let deferred = stepped_profile(ChargingRateUnitEnumType::W, &[(600, 1_000.0)]);
        assert_eq!(bounded_power_w(&deferred, 0, t0(), 7_360.0, 230.0), None);
    }

    /// A single-period profile of a given kind, letting a test set `startSchedule`
    /// / `duration` / `validFrom` / `validTo` to exercise composite resolution.
    #[allow(clippy::too_many_arguments)]
    fn kinded_profile(
        kind: ChargingProfileKindEnumType,
        recurrency: Option<RecurrencyKindEnumType>,
        start_schedule: Option<&str>,
        duration: Option<i32>,
        valid_from: Option<&str>,
        valid_to: Option<&str>,
        steps: &[(i32, f64)],
    ) -> ChargingProfileType {
        ChargingProfileType {
            id: 1,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeEnumType::TxProfile,
            charging_profile_kind: kind,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: ChargingRateUnitEnumType::W,
                charging_schedule_period: steps
                    .iter()
                    .map(|&(start_period, limit)| ChargingSchedulePeriodType {
                        start_period,
                        limit,
                        number_phases: None,
                        phase_to_use: None,
                        custom_data: None,
                    })
                    .collect(),
                start_schedule: start_schedule.map(str::to_owned),
                duration,
                min_charging_rate: None,
                sales_tariff: None,
                custom_data: None,
            }],
            recurrency_kind: recurrency,
            valid_from: valid_from.map(str::to_owned),
            valid_to: valid_to.map(str::to_owned),
            transaction_id: None,
            custom_data: None,
        }
    }

    #[test]
    fn absolute_kind_anchors_period_selection_on_start_schedule() {
        // An Absolute schedule starting 1 h *after* the transaction opened: at
        // t0 the schedule has not begun (raw offset negative → no limit); the
        // second period comes into force 30 min into the schedule, i.e. 90 min
        // after tx start.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Absolute,
            None,
            Some("2026-01-01T01:00:00Z"),
            None,
            None,
            None,
            &[(0, 7_400.0), (1_800, 3_680.0)],
        );
        // 0 and 30 min in: before the schedule's own start → nothing in force.
        assert_eq!(active_limit(&profile, 0, t0()), None);
        assert_eq!(active_limit(&profile, 1_800, t0()), None);
        // 1 h in: schedule start, first period.
        assert_eq!(
            active_limit(&profile, 3_600, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        // 90 min in: 30 min into the schedule → second period.
        assert_eq!(
            active_limit(&profile, 5_400, t0()),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
    }

    #[test]
    fn absolute_kind_without_start_schedule_falls_back_to_tx_start() {
        // No startSchedule on an Absolute profile → anchor at the transaction
        // start, so it behaves like a Relative schedule for offset selection.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Absolute,
            None,
            None,
            None,
            None,
            None,
            &[(0, 7_400.0), (1_800, 3_680.0)],
        );
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 1_800, t0()),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
    }

    #[test]
    fn validity_window_gates_the_limit() {
        // Valid only for the second hour of the transaction.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Relative,
            None,
            None,
            None,
            Some("2026-01-01T01:00:00Z"),
            Some("2026-01-01T02:00:00Z"),
            &[(0, 7_400.0)],
        );
        // Before validFrom → no limit.
        assert_eq!(active_limit(&profile, 0, t0()), None);
        assert_eq!(active_limit(&profile, 3_599, t0()), None);
        // Inside the window → the limit binds.
        assert_eq!(
            active_limit(&profile, 3_600, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 7_200, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        // After validTo → no limit again.
        assert_eq!(active_limit(&profile, 7_201, t0()), None);
    }

    #[test]
    fn schedule_duration_releases_the_limit_after_it_elapses() {
        // A Relative schedule bounded to its first hour; past that it imposes no
        // limit rather than pinning the last period forever.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Relative,
            None,
            None,
            Some(3_600),
            None,
            None,
            &[(0, 7_400.0)],
        );
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 3_599, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        // At exactly `duration` the schedule has elapsed → no limit.
        assert_eq!(active_limit(&profile, 3_600, t0()), None);
        assert_eq!(active_limit(&profile, 10_000, t0()), None);
    }

    #[test]
    fn recurring_daily_wraps_the_offset_into_each_day() {
        // A daily-recurring schedule anchored at tx start: 16 A-equivalent for the
        // first hour of each day, then a taper — active again at the same offset
        // 24 h later.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Recurring,
            Some(RecurrencyKindEnumType::Daily),
            Some("2026-01-01T00:00:00Z"),
            None,
            None,
            None,
            &[(0, 7_400.0), (3_600, 3_680.0)],
        );
        // Day 1.
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 3_600, t0()),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
        // Day 2, same wrapped offsets (86 400 s later).
        assert_eq!(
            active_limit(&profile, 86_400, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 90_000, t0()),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
    }

    #[test]
    fn recurring_daily_duration_leaves_a_gap_each_day() {
        // Active only the first hour of each day; the rest of the day is an
        // unconstrained gap that re-opens at the next occurrence.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Recurring,
            Some(RecurrencyKindEnumType::Daily),
            Some("2026-01-01T00:00:00Z"),
            Some(3_600),
            None,
            None,
            &[(0, 7_400.0)],
        );
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        // 2 h into day 1: past the 1 h duration → gap.
        assert_eq!(active_limit(&profile, 7_200, t0()), None);
        // Start of day 2: the occurrence re-opens.
        assert_eq!(
            active_limit(&profile, 86_400, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(active_limit(&profile, 93_600, t0()), None);
    }

    #[test]
    fn malformed_recurring_without_recurrency_kind_evaluates_once() {
        // A Recurring profile carrying no recurrencyKind cannot be unrolled; it is
        // evaluated as a single, non-repeating schedule (like Absolute) rather
        // than panicking or wrapping.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Recurring,
            None,
            Some("2026-01-01T00:00:00Z"),
            Some(3_600),
            None,
            None,
            &[(0, 7_400.0)],
        );
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        // Past the single occurrence's duration → gone, not repeated on day two.
        assert_eq!(active_limit(&profile, 3_600, t0()), None);
        assert_eq!(active_limit(&profile, 86_400, t0()), None);
    }

    #[test]
    fn unparseable_date_fields_are_treated_as_absent() {
        // A garbage validFrom does not gate the limit (lenient trust boundary),
        // and a garbage startSchedule on an Absolute profile falls back to the
        // tx-start anchor rather than aborting resolution.
        let profile = kinded_profile(
            ChargingProfileKindEnumType::Absolute,
            None,
            Some("not-a-timestamp"),
            None,
            Some("also-garbage"),
            None,
            &[(0, 7_400.0)],
        );
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((7_400.0, ChargingRateUnitEnumType::W)),
            "malformed date fields are ignored; the limit still resolves"
        );
    }

    /// A single-schedule `TxProfile` carrying a `minChargingRate` floor, so a
    /// test can drive a period `limit` below it (lifted) or above it (unchanged).
    fn floored_profile(
        unit: ChargingRateUnitEnumType,
        min_charging_rate: f64,
        steps: &[(i32, f64)],
    ) -> ChargingProfileType {
        let mut profile = stepped_profile(unit, steps);
        profile.charging_schedule[0].min_charging_rate = Some(min_charging_rate);
        profile
    }

    #[test]
    fn active_limit_floors_a_watt_limit_at_min_charging_rate() {
        // A 3 kW period under a 5 kW floor is lifted to the floor; a 6 kW period
        // already above it is returned unchanged. The unit stays the schedule's.
        let below = floored_profile(ChargingRateUnitEnumType::W, 5_000.0, &[(0, 3_000.0)]);
        assert_eq!(
            active_limit(&below, 0, t0()),
            Some((5_000.0, ChargingRateUnitEnumType::W)),
            "a period limit below minChargingRate is lifted to the floor"
        );
        let above = floored_profile(ChargingRateUnitEnumType::W, 5_000.0, &[(0, 6_000.0)]);
        assert_eq!(
            active_limit(&above, 0, t0()),
            Some((6_000.0, ChargingRateUnitEnumType::W)),
            "a period limit at or above minChargingRate is unchanged"
        );
    }

    #[test]
    fn active_limit_floors_an_ampere_limit_in_its_own_unit() {
        // The floor is applied in the schedule's chargingRateUnit: a 6 A period
        // under a 10 A floor resolves to 10 A, still amperes — the A→W conversion
        // is bounded_power_w's job, downstream of the floor.
        let profile = floored_profile(ChargingRateUnitEnumType::A, 10.0, &[(0, 6.0)]);
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((10.0, ChargingRateUnitEnumType::A))
        );
    }

    #[test]
    fn active_limit_without_min_charging_rate_is_unchanged() {
        // The common case: no floor set → the raw period limit, unchanged (no
        // regression to the pre-#467 resolution).
        let profile = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 3_000.0)]);
        assert_eq!(
            active_limit(&profile, 0, t0()),
            Some((3_000.0, ChargingRateUnitEnumType::W))
        );
    }

    #[test]
    fn bounded_power_w_binds_at_the_floored_limit() {
        // A 2 kW period under a 3.68 kW floor: the floor lifts the schedule-unit
        // limit to 3.68 kW, still below the 7.36 kW connector → binds there, not
        // at the un-floored 2 kW.
        let natural = 7_360.0;
        let profile = floored_profile(ChargingRateUnitEnumType::W, 3_680.0, &[(0, 2_000.0)]);
        assert_eq!(
            bounded_power_w(&profile, 0, t0(), natural, 230.0),
            Some(3_680.0)
        );
    }

    #[test]
    fn bounded_power_w_floored_above_the_natural_rate_does_not_bind() {
        // A floor above the connector's natural rate lifts the limit past it, so
        // it is no longer tighter → no bind, the reading is left untouched. The
        // floor never *creates* a binding limit against the natural rate.
        let natural = 7_360.0;
        let profile = floored_profile(ChargingRateUnitEnumType::W, 20_000.0, &[(0, 3_000.0)]);
        assert_eq!(bounded_power_w(&profile, 0, t0(), natural, 230.0), None);
    }

    #[test]
    fn bounded_power_w_floors_an_ampere_limit_before_conversion() {
        // 8 A floored to 16 A, converted at 230 V → 3 680 W, below the 7.36 kW
        // connector → binds at the converted, floored wattage.
        let natural = 7_360.0;
        let voltage = 230.0;
        let profile = floored_profile(ChargingRateUnitEnumType::A, 16.0, &[(0, 8.0)]);
        assert_eq!(
            bounded_power_w(&profile, 0, t0(), natural, voltage),
            Some(16.0 * voltage)
        );
    }
}
