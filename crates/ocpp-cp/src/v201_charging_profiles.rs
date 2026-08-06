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

use ocpp_types::v201::{ChargingProfileType, ChargingRateUnitEnumType};
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

/// Resolve the charging limit a `TxProfile` imposes `elapsed_s` seconds into the
/// transaction it bounds, as `(limit, unit)` — or `None` if the profile imposes
/// no limit at that offset.
///
/// The `chargingSchedule` (OCPP 2.0.1 Part 2, §K01 / the Python reference's
/// [`ocpp.v201.call.RequestStartTransaction`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)
/// `chargingProfile.chargingSchedule`) is a list of `chargingSchedulePeriod`s,
/// each starting at a `startPeriod` second-offset from the schedule's start and
/// holding a flat `limit` in the schedule's `chargingRateUnit` until the next
/// period begins. The limit in force at `elapsed_s` is therefore the period
/// with the **greatest `startPeriod ≤ elapsed_s`**.
///
/// **First-slice scope (Issue #455):** only the profile's *first*
/// `chargingSchedule` is honored, and periods are resolved by their relative
/// `startPeriod` alone. Composing multiple schedules, `validFrom`/`validTo`
/// windows, `Absolute`-vs-`Relative` kinds, and `startSchedule` offsets is the
/// composite-schedule follow-up. Returns `None` when the first schedule has no
/// period yet in force — an empty period list, or every `startPeriod` still in
/// the future — so the caller reports the unbounded reading rather than
/// inventing a limit of `0`.
#[must_use]
pub fn active_limit(
    profile: &ChargingProfileType,
    elapsed_s: i32,
) -> Option<(f64, ChargingRateUnitEnumType)> {
    let schedule = profile.charging_schedule.first()?;
    let period = schedule
        .charging_schedule_period
        .iter()
        .filter(|p| p.start_period <= elapsed_s)
        .max_by_key(|p| p.start_period)?;
    Some((period.limit, schedule.charging_rate_unit))
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
/// An ampere limit is converted at the connector's nominal voltage,
/// single-phase (`limit_a × nominal_voltage_v`); a poly-phase conversion that
/// honors the period's `numberPhases` is a follow-up, so a three-phase station
/// is bounded conservatively (to its single-phase equivalent) rather than
/// wrongly. A watt limit is used directly.
#[must_use]
pub fn bounded_power_w(
    profile: &ChargingProfileType,
    elapsed_s: i32,
    natural_power_w: f64,
    nominal_voltage_v: f64,
) -> Option<f64> {
    let (limit, unit) = active_limit(profile, elapsed_s)?;
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
        ChargingProfileKindEnumType, ChargingProfilePurposeEnumType, ChargingRateUnitEnumType,
        ChargingSchedulePeriodType, ChargingScheduleType,
    };

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
            active_limit(&profile, 0),
            Some((11_000.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 3_600),
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
            active_limit(&profile, 0),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 1_799),
            Some((7_400.0, ChargingRateUnitEnumType::W))
        );
        // At and after its startPeriod, the second period wins (greatest
        // startPeriod ≤ elapsed).
        assert_eq!(
            active_limit(&profile, 1_800),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
        assert_eq!(
            active_limit(&profile, 10_000),
            Some((3_680.0, ChargingRateUnitEnumType::W))
        );
    }

    #[test]
    fn active_limit_is_none_when_no_period_is_yet_in_force_or_the_schedule_is_empty() {
        // Every period starts in the future relative to elapsed 0 → nothing in
        // force yet.
        let deferred = stepped_profile(ChargingRateUnitEnumType::W, &[(600, 7_400.0)]);
        assert_eq!(active_limit(&deferred, 0), None);
        assert_eq!(active_limit(&deferred, 599), None);
        assert_eq!(
            active_limit(&deferred, 600),
            Some((7_400.0, ChargingRateUnitEnumType::W)),
            "the deferred period comes into force at its startPeriod"
        );

        // A degenerate schedule with no periods resolves to nothing rather than
        // panicking or inventing a limit.
        let empty = stepped_profile(ChargingRateUnitEnumType::W, &[]);
        assert_eq!(active_limit(&empty, 0), None);
    }

    #[test]
    fn bounded_power_w_binds_only_a_watt_limit_below_the_natural_rate() {
        let natural = 7_360.0;
        // A 3.68 kW limit is tighter than the 7.36 kW connector → binds.
        let tight = tx_profile(1, 3_680.0);
        assert_eq!(bounded_power_w(&tight, 0, natural, 230.0), Some(3_680.0));

        // An 11 kW limit is looser than the connector's natural rate → no bind,
        // the reading is left unchanged.
        let loose = tx_profile(2, 11_000.0);
        assert_eq!(bounded_power_w(&loose, 0, natural, 230.0), None);

        // A limit exactly at the natural rate is not *tighter*, so it does not
        // bend the reading either.
        let at_rate = tx_profile(3, natural);
        assert_eq!(bounded_power_w(&at_rate, 0, natural, 230.0), None);
    }

    #[test]
    fn bounded_power_w_converts_an_ampere_limit_at_nominal_voltage() {
        let natural = 7_360.0;
        let voltage = 230.0;
        // 16 A × 230 V = 3 680 W, below the natural rate → binds at the
        // converted wattage.
        let amps = stepped_profile(ChargingRateUnitEnumType::A, &[(0, 16.0)]);
        assert_eq!(
            bounded_power_w(&amps, 0, natural, voltage),
            Some(16.0 * voltage)
        );
        // 32 A × 230 V = 7 360 W == natural rate → not tighter → no bind.
        let full = stepped_profile(ChargingRateUnitEnumType::A, &[(0, 32.0)]);
        assert_eq!(bounded_power_w(&full, 0, natural, voltage), None);
    }

    #[test]
    fn bounded_power_w_is_none_when_no_period_is_in_force() {
        // No period yet in force → nothing to bind, whatever the connector rate.
        let deferred = stepped_profile(ChargingRateUnitEnumType::W, &[(600, 1_000.0)]);
        assert_eq!(bounded_power_w(&deferred, 0, 7_360.0, 230.0), None);
    }
}
