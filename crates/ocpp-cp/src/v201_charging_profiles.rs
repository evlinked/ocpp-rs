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
    ChargingSchedulePeriodType, ChargingScheduleType, CompositeScheduleType,
    RecurrencyKindEnumType,
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

    /// A cloned `(evse_id, profile)` snapshot of every installed profile.
    ///
    /// The read a `ClearChargingProfile` needs: its selector is resolved against
    /// the store's *current* contents by a pure decision that must see each
    /// installed slot's EVSE key alongside the profile's `id` /
    /// `chargingProfilePurpose` / `stackLevel`. Returning an owned snapshot lets
    /// that matching run without holding the store lock, so the subsequent
    /// per-EVSE [`clear`](Self::clear) removals take the write lock cleanly. The
    /// order is unspecified (it is a `HashMap` walk); callers key off `evse_id`,
    /// never position.
    pub async fn snapshot(&self) -> Vec<(i32, ChargingProfileType)> {
        self.by_evse
            .read()
            .await
            .iter()
            .map(|(evse_id, profile)| (*evse_id, profile.clone()))
            .collect()
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
    let (schedule, period) = resolve_active(profile, elapsed_s, tx_start)?;
    Some((floored_limit(schedule, period), schedule.charging_rate_unit))
}

/// Resolve the `(schedule, period)` in force `elapsed_s` seconds into the
/// transaction — the shared core of [`active_limit`] and [`bounded_power_w`].
/// Applies, in order: the profile's `validFrom`/`validTo` window, the recurrence
/// phasing, the schedule `duration` bound, and the "last period whose
/// `startPeriod` has been reached" selection. Returns `None` when the profile
/// imposes nothing at this instant (outside its validity window, past its
/// schedule `duration`, or with no period yet in force).
fn resolve_active(
    profile: &ChargingProfileType,
    elapsed_s: i32,
    tx_start: DateTime<Utc>,
) -> Option<(&ChargingScheduleType, &ChargingSchedulePeriodType)> {
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
    Some((schedule, period))
}

/// The period `limit` lifted to the schedule's `minChargingRate` floor (Issue
/// #467), in the schedule's own `chargingRateUnit` (so an ampere floor lifts an
/// ampere limit, a watt floor a watt limit). `f64::max` is total — a floor at or
/// below the period limit is a no-op, and even a degenerate floor cannot panic —
/// so a CSMS-supplied `minChargingRate` only ever raises the schedule-unit
/// value, never poisons resolution.
fn floored_limit(schedule: &ChargingScheduleType, period: &ChargingSchedulePeriodType) -> f64 {
    match schedule.min_charging_rate {
        Some(floor) => period.limit.max(floor),
        None => period.limit,
    }
}

/// The number of phases `period`'s ampere→watt conversion applies (Issue #465),
/// honoring its `numberPhases` and defaulting to **3** when absent — OCPP 2.0.1
/// `ChargingSchedulePeriodType.numberPhases` ("if a number of phases is needed,
/// numberPhases=3 will be assumed unless another number is given"; the Python
/// reference's
/// [`ocpp.v201.datatypes.ChargingSchedulePeriodType`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/datatypes.py)).
/// The value is clamped into the schema's `1..=3` band so a malformed CSMS
/// `numberPhases` (0, negative, or >3 — this is an incoming-WebSocket trust
/// boundary) can neither zero the bound (stall the car at 0 W) nor inflate it
/// past a genuine three-phase draw.
fn effective_phases(period: &ChargingSchedulePeriodType) -> i32 {
    period.number_phases.unwrap_or(3).clamp(1, 3)
}

/// The charging power (in watts) `profile` resolves to `elapsed_s` into the
/// transaction — its active `chargingSchedulePeriod` limit converted to watts —
/// or `None` when the profile imposes no limit at this instant (outside its
/// validity window, past its schedule `duration`, or with no period yet in
/// force).
///
/// Unlike [`bounded_power_w`] this applies **no** natural-rate gate: it returns
/// the profile's own resolved watt limit whether or not it is tighter than the
/// connector's draw. The shared conversion core of [`bounded_power_w`] (which
/// adds the gate) and [`bounded_power_w_capped`] (which composes several resolved
/// limits by `min`). The resolved limit is floored at the schedule's
/// `minChargingRate` (Issue #467) before an ampere limit's A→W conversion at
/// `nominal_voltage_v` across the period's clamped phase count (Issue #465).
#[must_use]
pub fn active_limit_w(
    profile: &ChargingProfileType,
    elapsed_s: i32,
    tx_start: DateTime<Utc>,
    nominal_voltage_v: f64,
) -> Option<f64> {
    let (schedule, period) = resolve_active(profile, elapsed_s, tx_start)?;
    let limit = floored_limit(schedule, period);
    Some(match schedule.charging_rate_unit {
        ChargingRateUnitEnumType::W => limit,
        ChargingRateUnitEnumType::A => {
            limit * nominal_voltage_v * f64::from(effective_phases(period))
        }
    })
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
/// The resolved limit is floored at the schedule's `minChargingRate` (Issue
/// #467, via `floored_limit`) before conversion, so a schedule that declares a
/// floor above the natural rate simply yields no bind here.
///
/// An ampere limit is converted at the connector's nominal voltage across the
/// period's phase count (`limit_a × nominal_voltage_v × numberPhases`, Issue
/// #465), so a three-phase station is bounded to its real capacity rather than
/// the ~⅓ single-phase equivalent it was pinned to before. The phase count
/// defaults to 3 when `numberPhases` is absent and is clamped to `1..=3` — see
/// `effective_phases`. A watt limit is used directly (its magnitude already
/// accounts for phases, so `numberPhases` does not apply).
#[must_use]
pub fn bounded_power_w(
    profile: &ChargingProfileType,
    elapsed_s: i32,
    tx_start: DateTime<Utc>,
    natural_power_w: f64,
    nominal_voltage_v: f64,
) -> Option<f64> {
    let limit_w = active_limit_w(profile, elapsed_s, tx_start, nominal_voltage_v)?;
    (limit_w < natural_power_w).then_some(limit_w)
}

/// The charging power (in watts) the connector is bound to `elapsed_s` into the
/// transaction once the resolved `TxProfile`/`TxDefaultProfile` limit is **capped
/// by the applicable station ceilings** (OCPP 2.0.1 Part 2, §K01; Issue #511) —
/// or `None` when nothing binds below the connector's natural rate at this offset.
///
/// The effective limit is the minimum of every constraint in force:
///
/// * `effective_profile` — the resolved `TxProfile`/`TxDefaultProfile` in force on
///   the EVSE, with its `TxProfile > TxDefaultProfile` precedence already applied
///   by the caller (so this is the substitutive limit, or `None` if no such
///   profile is in force);
/// * each ceiling in `ceilings` — the applicable `ChargingStationMaxProfile` /
///   `ChargingStationExternalConstraints`, each of which *caps* rather than
///   substitutes.
///
/// Composition is `min(natural, effective_profile?, ceilings…)`, so a ceiling
/// binds even with **no** `TxProfile`/`TxDefaultProfile` present (a station-wide
/// limit still caps the natural rate), and a profile below every ceiling is left
/// untouched. Because every term enters a `min`, stacking is order-independent —
/// the tightest ceiling wins regardless of the `ceilings` order, so no precedence
/// between `ChargingStationMaxProfile` and `ChargingStationExternalConstraints`
/// need be threaded here (the latter is the semantic outermost cap; a `min`
/// honors it either way).
///
/// As with [`bounded_power_w`], `None` is returned when the composed limit is at
/// or above `natural_power_w`: a bound no tighter than the connector's own draw
/// does not bend the reading. A profile or ceiling that constrains nothing at
/// this instant simply drops out of the `min` (its `active_limit_w` is `None`).
#[must_use]
pub fn bounded_power_w_capped(
    effective_profile: Option<&ChargingProfileType>,
    ceilings: &[&ChargingProfileType],
    elapsed_s: i32,
    tx_start: DateTime<Utc>,
    natural_power_w: f64,
    nominal_voltage_v: f64,
) -> Option<f64> {
    let mut limit_w = natural_power_w;
    for profile in effective_profile
        .into_iter()
        .chain(ceilings.iter().copied())
    {
        if let Some(w) = active_limit_w(profile, elapsed_s, tx_start, nominal_voltage_v) {
            limit_w = limit_w.min(w);
        }
    }
    (limit_w < natural_power_w).then_some(limit_w)
}

/// Convert a rate `limit` from unit `from` to unit `to`, using the connector's
/// `nominal_voltage_v` and the period's `phases` for an A↔W conversion; identity
/// when the units already match.
///
/// `GetCompositeSchedule` lets the CSMS force the reported unit
/// ([`GetCompositeScheduleRequest::charging_rate_unit`](ocpp_messages::v201::GetCompositeScheduleRequest)),
/// so a schedule authored in amperes may have to be reported in watts (or vice
/// versa). The conversion mirrors [`bounded_power_w`]'s poly-phase A→W
/// (`limit_a × nominal_voltage_v × numberPhases`, Issue #465), so a reported
/// schedule agrees with the limit the metering resolver actually enforces.
///
/// `phases` arrives from [`effective_phases`] (already clamped to `1..=3`, so
/// never `0`) and `nominal_voltage_v` from the connector config; the W→A branch
/// still guards a zero denominator defensively (a degenerate `0 V` connector)
/// rather than divide by zero, returning the watt value unchanged.
fn convert_rate(
    limit: f64,
    from: ChargingRateUnitEnumType,
    to: ChargingRateUnitEnumType,
    phases: i32,
    nominal_voltage_v: f64,
) -> f64 {
    match (from, to) {
        (ChargingRateUnitEnumType::A, ChargingRateUnitEnumType::A)
        | (ChargingRateUnitEnumType::W, ChargingRateUnitEnumType::W) => limit,
        (ChargingRateUnitEnumType::A, ChargingRateUnitEnumType::W) => {
            limit * nominal_voltage_v * f64::from(phases)
        }
        (ChargingRateUnitEnumType::W, ChargingRateUnitEnumType::A) => {
            let denom = nominal_voltage_v * f64::from(phases);
            if denom == 0.0 {
                limit
            } else {
                limit / denom
            }
        }
    }
}

/// The limit (already expressed in `out_unit`) and the period's raw `numberPhases`
/// the profile imposes `offset` seconds into the query window, or `None` when it
/// imposes nothing then (a gap).
///
/// Reuses [`resolve_active`] verbatim — the same period-selection,
/// `validFrom`/`validTo`, recurrence-phasing, schedule-`duration` and
/// `minChargingRate`-floor logic the metering path applies — then converts into
/// the reported unit. The stored `numberPhases` is the period's raw value (so the
/// coalescer distinguishes a phase change), while the A↔W conversion uses the
/// clamped [`effective_phases`].
fn composite_contribution_at(
    profile: &ChargingProfileType,
    offset: i32,
    window_start: DateTime<Utc>,
    out_unit: ChargingRateUnitEnumType,
    nominal_voltage_v: f64,
) -> Option<(f64, Option<i32>)> {
    let (schedule, period) = resolve_active(profile, offset, window_start)?;
    let limit = floored_limit(schedule, period);
    let value = convert_rate(
        limit,
        schedule.charging_rate_unit,
        out_unit,
        effective_phases(period),
        nominal_voltage_v,
    );
    Some((value, period.number_phases))
}

/// Candidate window-relative offsets (seconds within `[0, duration)`) at which the
/// composite limit may change: the schedule's period starts, its `duration` end,
/// and the profile's `validFrom`/`validTo` edges — repeated for every recurrence
/// occurrence that overlaps the window.
///
/// A superset of the true breakpoints is fine: [`compose_composite_schedule`]
/// coalesces adjacent equal periods, so an extra boundary that does not actually
/// change the limit is dropped. Mirrors the 1.6J `composite::boundary_offsets`;
/// the walk is bounded by the number of schedule periods times the occurrence
/// count (`duration / recurrence_period`), never a per-second scan — so an
/// attacker-supplied `duration` cannot force an unbounded loop.
fn composite_boundary_offsets(
    profile: &ChargingProfileType,
    window_start: DateTime<Utc>,
    duration: i32,
) -> Vec<i32> {
    let mut offsets = vec![0i32];
    let dur = i64::from(duration);
    let push = |offsets: &mut Vec<i32>, o: i64| {
        if o > 0 && o < dur {
            offsets.push(o as i32);
        }
    };
    if let Some(schedule) = profile.charging_schedule.first() {
        let base = (anchor(profile, schedule, window_start) - window_start).num_seconds();
        match recurrence_period(profile) {
            // Recurring: emit boundaries for every occurrence overlapping the
            // window. Start one occurrence at or before offset 0 (so an active
            // span ending inside the window is captured) and step until the next
            // occurrence begins at or after the window end.
            Some(period) if period > 0 => {
                let mut k = (-base).div_euclid(period);
                loop {
                    let start = base + k * period;
                    if start >= dur {
                        break;
                    }
                    for p in &schedule.charging_schedule_period {
                        push(&mut offsets, start + i64::from(p.start_period));
                    }
                    if let Some(d) = schedule.duration {
                        push(&mut offsets, start + i64::from(d));
                    }
                    k += 1;
                }
            }
            // Non-recurring (and the unreachable non-positive recurrence period):
            // a single schedule anchored at `base`.
            _ => {
                for p in &schedule.charging_schedule_period {
                    push(&mut offsets, base + i64::from(p.start_period));
                }
                if let Some(d) = schedule.duration {
                    push(&mut offsets, base + i64::from(d));
                }
            }
        }
    }
    if let Some(vf) = parse_rfc3339(&profile.valid_from) {
        push(&mut offsets, (vf - window_start).num_seconds());
    }
    if let Some(vt) = parse_rfc3339(&profile.valid_to) {
        // The limit is gone the second after validTo.
        push(&mut offsets, (vt - window_start).num_seconds() + 1);
    }
    offsets.sort_unstable();
    offsets.dedup();
    offsets
}

/// Build the composite [`CompositeScheduleType`] a `GetCompositeSchedule` reports
/// for `evse_id` over `[window_start, window_start + duration)`, from the single
/// `TxProfile` installed on that EVSE — or `None` when the profile constrains no
/// instant in the window (the caller maps that to `GenericStatusEnumType::Rejected`).
///
/// This is the query counterpart of the periodic-metering resolver: it walks the
/// candidate breakpoints (`composite_boundary_offsets`) and reads the effective
/// limit at each via `composite_contribution_at` (reusing `resolve_active`), then
/// coalesces adjacent equal periods into the reported schedule. The output unit is
/// the CSMS-requested unit when present, else the schedule's own unit; limits are
/// converted into it (`convert_rate`).
///
/// **Anchoring.** `window_start` is passed to the resolver as its `tx_start`, so a
/// `Relative` schedule is anchored at the reported window start and
/// `Absolute`/`Recurring` schedules at their own `startSchedule` — matching the
/// 1.6J `GetCompositeSchedule` composite (`composite::compute_composite`).
/// Anchoring a mid-transaction `Relative` profile at its *true* transaction start
/// (rather than the query instant) would require persisting the transaction start
/// in `V201Session`; it is left as a follow-up (the two coincide for a query at
/// transaction start).
///
/// Returns `None` when `duration <= 0` or no period is ever in force, so an
/// `Accepted` response always carries a schema-valid, non-empty period list.
#[must_use]
pub fn compose_composite_schedule(
    evse_id: i32,
    profile: &ChargingProfileType,
    window_start: DateTime<Utc>,
    duration: i32,
    requested_unit: Option<ChargingRateUnitEnumType>,
    nominal_voltage_v: f64,
) -> Option<CompositeScheduleType> {
    compose_composite_schedule_capped(
        evse_id,
        profile,
        &[],
        window_start,
        duration,
        requested_unit,
        nominal_voltage_v,
    )
}

/// The station-ceiling-capped counterpart of [`compose_composite_schedule`]
/// (OCPP 2.0.1 Part 2, §K01; Issue #511): the composite of the base
/// `TxProfile`/`TxDefaultProfile` `profile`, with every reported period **capped**
/// by the applicable station ceilings (`ChargingStationMaxProfile` /
/// `ChargingStationExternalConstraints`) in force at that instant.
///
/// Walks the union of the base profile's and every ceiling's candidate breakpoints
/// (so a ceiling change mid-window opens a new reported period), reads the base
/// limit at each via `composite_contribution_at`, and caps it by the `min` of the
/// ceilings active there — all converted into the reported `out_unit`. The base
/// period's `numberPhases` is preserved (a ceiling caps the magnitude, not the
/// phase count), and a ceiling that constrains nothing at an offset simply drops
/// out of the `min`. As in the uncapped composite, an offset where the base
/// profile itself imposes no limit is a gap (no period emitted) — a ceiling alone,
/// with no base profile in force, contributes no schedule (matching the handler,
/// which reports `Rejected` when no base profile is installed).
///
/// With an empty `ceilings` slice this is exactly [`compose_composite_schedule`],
/// which delegates here.
#[must_use]
pub fn compose_composite_schedule_capped(
    evse_id: i32,
    profile: &ChargingProfileType,
    ceilings: &[&ChargingProfileType],
    window_start: DateTime<Utc>,
    duration: i32,
    requested_unit: Option<ChargingRateUnitEnumType>,
    nominal_voltage_v: f64,
) -> Option<CompositeScheduleType> {
    if duration <= 0 {
        return None;
    }
    let out_unit = requested_unit.unwrap_or_else(|| {
        profile
            .charging_schedule
            .first()
            .map_or(ChargingRateUnitEnumType::W, |s| s.charging_rate_unit)
    });

    // Candidate breakpoints: the base profile's, plus every ceiling's — a ceiling
    // stepping to a new limit mid-window must open a new reported period even if
    // the base profile is flat across it. A superset is fine; adjacent equal
    // periods coalesce below.
    let mut offsets = composite_boundary_offsets(profile, window_start, duration);
    for ceiling in ceilings {
        offsets.extend(composite_boundary_offsets(ceiling, window_start, duration));
    }
    offsets.sort_unstable();
    offsets.dedup();

    let mut periods: Vec<ChargingSchedulePeriodType> = Vec::new();
    // The (limit, phases) at the previous boundary, or `None` if that boundary was
    // a gap. Coalescing compares against this rather than the last emitted period,
    // so a limit that disappears into a gap and returns at the same value is
    // re-emitted (the gap between carried a different, unconstrained limit) —
    // matters for a recurring profile whose `duration` leaves a daily/weekly gap.
    let mut prev: Option<(f64, Option<i32>)> = None;
    for offset in offsets {
        // The base contribution — a gap here means the EVSE has no limit at this
        // instant, so nothing to cap or report (a ceiling never manufactures a
        // limit where the base profile imposes none).
        let current =
            composite_contribution_at(profile, offset, window_start, out_unit, nominal_voltage_v)
                .map(|(base_limit, phases)| {
                    let capped = ceilings.iter().filter_map(|ceiling| {
                        composite_contribution_at(
                            ceiling,
                            offset,
                            window_start,
                            out_unit,
                            nominal_voltage_v,
                        )
                        .map(|(ceiling_limit, _)| ceiling_limit)
                    });
                    // min(base, ceilings…) in `out_unit`; the base phase count is
                    // preserved (a ceiling caps magnitude, not phases).
                    (capped.fold(base_limit, f64::min), phases)
                });
        if let Some((limit, phases)) = current {
            let unchanged = matches!(
                prev,
                Some((pl, pp)) if (pl - limit).abs() < f64::EPSILON && pp == phases
            );
            if !unchanged {
                periods.push(ChargingSchedulePeriodType {
                    start_period: offset,
                    limit,
                    number_phases: phases,
                    phase_to_use: None,
                    custom_data: None,
                });
            }
        }
        prev = current;
    }

    if periods.is_empty() {
        return None;
    }

    Some(CompositeScheduleType {
        evse_id,
        duration,
        schedule_start: window_start.to_rfc3339(),
        charging_rate_unit: out_unit,
        charging_schedule_period: periods,
        custom_data: None,
    })
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

    #[tokio::test]
    async fn snapshot_reflects_every_installed_profile_by_evse() {
        let store = V201TxProfileStore::new();
        assert!(
            store.snapshot().await.is_empty(),
            "an empty store snapshots to nothing"
        );

        let p1 = tx_profile(10, 11_000.0);
        let p2 = tx_profile(20, 7_400.0);
        store.install(1, p1.clone()).await;
        store.install(2, p2.clone()).await;

        // Order is a HashMap walk, so sort by EVSE key before comparing.
        let mut snap = store.snapshot().await;
        snap.sort_by_key(|(evse_id, _)| *evse_id);
        assert_eq!(snap, vec![(1, p1), (2, p2)]);

        // The snapshot is a detached clone: clearing the store afterward does not
        // mutate an already-taken snapshot.
        let taken = store.snapshot().await;
        store.clear(1).await;
        assert_eq!(
            taken.len(),
            2,
            "a taken snapshot is independent of the store"
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

    /// A single-period ampere `TxProfile` with an explicit `numberPhases`, for
    /// the poly-phase A→W conversion tests. `None` exercises the absent-default.
    fn ampere_profile(number_phases: Option<i32>, limit_a: f64) -> ChargingProfileType {
        let mut profile = stepped_profile(ChargingRateUnitEnumType::A, &[(0, limit_a)]);
        profile.charging_schedule[0].charging_schedule_period[0].number_phases = number_phases;
        profile
    }

    #[test]
    fn bounded_power_w_converts_a_single_phase_ampere_limit_at_nominal_voltage() {
        let natural = 7_360.0;
        let voltage = 230.0;
        // 16 A × 230 V × 1φ = 3 680 W, below the natural rate → binds at the
        // converted wattage.
        let amps = ampere_profile(Some(1), 16.0);
        assert_eq!(
            bounded_power_w(&amps, 0, t0(), natural, voltage),
            Some(16.0 * voltage)
        );
        // 32 A × 230 V × 1φ = 7 360 W == natural rate → not tighter → no bind.
        let full = ampere_profile(Some(1), 32.0);
        assert_eq!(bounded_power_w(&full, 0, t0(), natural, voltage), None);
    }

    #[test]
    fn bounded_power_w_scales_an_ampere_limit_by_number_phases() {
        // The same 16 A limit resolves to a wider bound as the phase count
        // rises: 1φ → 3 680 W, 2φ → 7 360 W, 3φ → 11 040 W. A three-phase
        // station is bounded to its real capacity, not the single-phase
        // equivalent it was pinned to before (Issue #465). `natural` is high
        // enough that every phase count binds, isolating the scaling.
        let voltage = 230.0;
        let natural = 20_000.0;
        for (phases, expected_w) in [(1, 3_680.0), (2, 7_360.0), (3, 11_040.0)] {
            let amps = ampere_profile(Some(phases), 16.0);
            assert_eq!(
                bounded_power_w(&amps, 0, t0(), natural, voltage),
                Some(expected_w),
                "{phases}-phase 16 A at 230 V should bind at {expected_w} W"
            );
        }
    }

    #[test]
    fn bounded_power_w_defaults_absent_number_phases_to_three() {
        // A `numberPhases`-absent period assumes 3 (the OCPP 2.0.1 default), so
        // it converts identically to an explicit 3-phase period.
        let voltage = 230.0;
        let natural = 20_000.0;
        let absent = ampere_profile(None, 16.0);
        let explicit = ampere_profile(Some(3), 16.0);
        assert_eq!(
            bounded_power_w(&absent, 0, t0(), natural, voltage),
            bounded_power_w(&explicit, 0, t0(), natural, voltage),
            "an absent numberPhases resolves to the 3-phase conversion"
        );
        assert_eq!(
            bounded_power_w(&absent, 0, t0(), natural, voltage),
            Some(16.0 * voltage * 3.0)
        );
    }

    #[test]
    fn bounded_power_w_clamps_out_of_range_number_phases() {
        // A malformed CSMS `numberPhases` (an incoming-WebSocket trust boundary)
        // is clamped into 1..=3: 0 / negative → single-phase (never a 0 W bound
        // that would stall the car), >3 → three-phase (never inflated past a
        // real three-phase draw).
        let voltage = 230.0;
        let natural = 20_000.0;
        assert_eq!(
            bounded_power_w(&ampere_profile(Some(0), 16.0), 0, t0(), natural, voltage),
            Some(16.0 * voltage),
            "0 phases clamps up to single-phase"
        );
        assert_eq!(
            bounded_power_w(&ampere_profile(Some(-1), 16.0), 0, t0(), natural, voltage),
            Some(16.0 * voltage),
            "a negative phase count clamps up to single-phase"
        );
        assert_eq!(
            bounded_power_w(&ampere_profile(Some(9), 16.0), 0, t0(), natural, voltage),
            Some(16.0 * voltage * 3.0),
            ">3 phases clamps down to three-phase"
        );
    }

    #[test]
    fn bounded_power_w_ignores_number_phases_for_a_watt_limit() {
        // `numberPhases` only scales an ampere→watt conversion; a watt limit is
        // already a power, so the phase count must not touch it.
        let natural = 7_360.0;
        let mut profile = tx_profile(1, 3_680.0); // watt-unit
        profile.charging_schedule[0].charging_schedule_period[0].number_phases = Some(3);
        assert_eq!(
            bounded_power_w(&profile, 0, t0(), natural, 230.0),
            Some(3_680.0),
            "a 3-phase tag does not triple a watt limit"
        );
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
        // 8 A floored to 16 A, converted single-phase at 230 V → 3 680 W, below
        // the 7.36 kW connector → binds at the converted, floored wattage. Pinned
        // to 1 phase so this exercises the floor, not the phase scaling (#465).
        let natural = 7_360.0;
        let voltage = 230.0;
        let mut profile = floored_profile(ChargingRateUnitEnumType::A, 16.0, &[(0, 8.0)]);
        profile.charging_schedule[0].charging_schedule_period[0].number_phases = Some(1);
        assert_eq!(
            bounded_power_w(&profile, 0, t0(), natural, voltage),
            Some(16.0 * voltage)
        );
    }

    // --- compose_composite_schedule (GetCompositeSchedule builder, #475) ------

    #[test]
    fn compose_flat_profile_yields_one_period_over_the_window() {
        let profile = tx_profile(1, 11_000.0);
        let sched = compose_composite_schedule(1, &profile, t0(), 3_600, None, 230.0)
            .expect("a flat profile constrains the whole window → Accepted");
        assert_eq!(sched.evse_id, 1);
        assert_eq!(sched.duration, 3_600);
        assert_eq!(sched.charging_rate_unit, ChargingRateUnitEnumType::W);
        assert_eq!(sched.schedule_start, t0().to_rfc3339());
        assert_eq!(sched.charging_schedule_period.len(), 1);
        let p = &sched.charging_schedule_period[0];
        assert_eq!(p.start_period, 0);
        assert_eq!(p.limit, 11_000.0);
    }

    #[test]
    fn compose_multi_period_schedule_steps_at_each_breakpoint() {
        // 11 kW for the first 30 min, then a taper to 7.4 kW.
        let profile = stepped_profile(
            ChargingRateUnitEnumType::W,
            &[(0, 11_000.0), (1_800, 7_400.0)],
        );
        let sched = compose_composite_schedule(1, &profile, t0(), 3_600, None, 230.0)
            .expect("both periods fall inside the window");
        let periods = &sched.charging_schedule_period;
        assert_eq!(periods.len(), 2, "one period per limit step, coalesced");
        assert_eq!((periods[0].start_period, periods[0].limit), (0, 11_000.0));
        assert_eq!(
            (periods[1].start_period, periods[1].limit),
            (1_800, 7_400.0)
        );
    }

    #[test]
    fn compose_honors_a_requested_watt_unit_over_an_ampere_schedule() {
        // A three-phase 16 A schedule reported in watts: 16 A × 230 V × 3φ.
        let profile = ampere_profile(Some(3), 16.0);
        let sched = compose_composite_schedule(
            1,
            &profile,
            t0(),
            3_600,
            Some(ChargingRateUnitEnumType::W),
            230.0,
        )
        .expect("the ampere schedule constrains the window");
        assert_eq!(sched.charging_rate_unit, ChargingRateUnitEnumType::W);
        assert_eq!(sched.charging_schedule_period.len(), 1);
        let p = &sched.charging_schedule_period[0];
        assert_eq!(p.limit, 16.0 * 230.0 * 3.0);
        assert_eq!(
            p.number_phases,
            Some(3),
            "the reported period keeps the schedule's raw numberPhases"
        );
    }

    #[test]
    fn compose_is_rejected_when_no_period_is_in_force_over_the_window() {
        // The only period starts at 600 s but the window is just 300 s long, so
        // nothing constrains any instant in it → None (Rejected, no schedule).
        let deferred = stepped_profile(ChargingRateUnitEnumType::W, &[(600, 7_400.0)]);
        assert_eq!(
            compose_composite_schedule(1, &deferred, t0(), 300, None, 230.0),
            None
        );
    }

    #[test]
    fn compose_is_rejected_for_a_non_positive_duration() {
        let profile = tx_profile(1, 11_000.0);
        assert_eq!(
            compose_composite_schedule(1, &profile, t0(), 0, None, 230.0),
            None
        );
        assert_eq!(
            compose_composite_schedule(1, &profile, t0(), -5, None, 230.0),
            None
        );
    }

    #[test]
    fn compose_stops_at_the_schedule_duration_leaving_the_tail_a_gap() {
        // A flat 5 kW schedule bounded to its first 1 800 s, queried over a
        // 3 600 s window: the limit applies for the first half and then lapses
        // (no period pinned forever) → a single reported period, gap after.
        let mut profile = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 5_000.0)]);
        profile.charging_schedule[0].duration = Some(1_800);
        let sched = compose_composite_schedule(1, &profile, t0(), 3_600, None, 230.0)
            .expect("the schedule constrains the first half of the window");
        assert_eq!(sched.charging_schedule_period.len(), 1);
        assert_eq!(
            (
                sched.charging_schedule_period[0].start_period,
                sched.charging_schedule_period[0].limit
            ),
            (0, 5_000.0)
        );
    }

    /// Wire fidelity: a built `GetCompositeSchedule.conf` — both the `Accepted`
    /// case carrying the composed `CompositeScheduleType` and the bare `Rejected`
    /// case — satisfies the bundled OCPP 2.0.1 `GetCompositeScheduleResponse`
    /// schema (the `minItems: 1` period list included), the same guarantee the
    /// CP's version-aware validator gives on the live path.
    #[test]
    fn built_get_composite_schedule_responses_are_schema_valid() {
        use ocpp_messages::v201::GetCompositeScheduleResponse;
        use ocpp_messages::SchemaValidator;
        use ocpp_types::v201::GenericStatusEnumType;

        let validator = SchemaValidator::v201();
        let profile = stepped_profile(
            ChargingRateUnitEnumType::W,
            &[(0, 11_000.0), (1_800, 7_400.0)],
        );
        let schedule = compose_composite_schedule(1, &profile, t0(), 3_600, None, 230.0)
            .expect("the profile constrains the window");

        let accepted = GetCompositeScheduleResponse {
            status: GenericStatusEnumType::Accepted,
            schedule: Some(schedule),
            status_info: None,
            custom_data: None,
        };
        let rejected = GetCompositeScheduleResponse {
            status: GenericStatusEnumType::Rejected,
            schedule: None,
            status_info: None,
            custom_data: None,
        };
        for resp in [accepted, rejected] {
            let payload = serde_json::to_value(&resp).unwrap();
            assert!(
                validator
                    .validate_call_result("GetCompositeSchedule", &payload)
                    .is_ok(),
                "built {:?} GetCompositeScheduleResponse should be schema-valid, got: {payload}",
                resp.status
            );
        }
    }

    // ---- Station-ceiling composition (Issue #511) ----

    #[test]
    fn active_limit_w_returns_the_resolved_limit_without_a_natural_gate() {
        // Unlike `bounded_power_w`, `active_limit_w` reports the profile's own watt
        // limit whether or not it is below any connector rate.
        let profile = tx_profile(1, 6_000.0);
        assert_eq!(active_limit_w(&profile, 0, t0(), 230.0), Some(6_000.0));
        // An ampere limit is converted at nominal voltage across its phases.
        let amps = ampere_profile(Some(3), 16.0);
        assert_eq!(
            active_limit_w(&amps, 0, t0(), 230.0),
            Some(16.0 * 230.0 * 3.0)
        );
        // No period in force yet → None.
        let deferred = stepped_profile(ChargingRateUnitEnumType::W, &[(600, 7_400.0)]);
        assert_eq!(active_limit_w(&deferred, 0, t0(), 230.0), None);
    }

    #[test]
    fn capped_binds_the_ceiling_below_the_tx_limit() {
        // TxProfile at 11 kW, a 6 kW station ceiling, natural rate 22 kW → the
        // metering bound is min(22k, 11k, 6k) = 6 kW.
        let tx = tx_profile(1, 11_000.0);
        let ceiling = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 6_000.0)]);
        assert_eq!(
            bounded_power_w_capped(Some(&tx), &[&ceiling], 0, t0(), 22_000.0, 230.0),
            Some(6_000.0)
        );
    }

    #[test]
    fn capped_leaves_a_ceiling_above_the_tx_limit_untouched() {
        // A ceiling looser than the TxProfile does not bend the result: min is the
        // 7.4 kW TxProfile, still below the 22 kW natural rate.
        let tx = tx_profile(1, 7_400.0);
        let ceiling = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 20_000.0)]);
        assert_eq!(
            bounded_power_w_capped(Some(&tx), &[&ceiling], 0, t0(), 22_000.0, 230.0),
            Some(7_400.0)
        );
    }

    #[test]
    fn capped_ceiling_binds_with_no_tx_profile_in_force() {
        // A station ceiling caps the natural rate even when no TxProfile /
        // TxDefaultProfile is in force: min(22k, 8k) = 8 kW.
        let ceiling = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 8_000.0)]);
        assert_eq!(
            bounded_power_w_capped(None, &[&ceiling], 0, t0(), 22_000.0, 230.0),
            Some(8_000.0)
        );
        // …but a ceiling at or above the natural rate leaves the reading unbounded.
        let loose = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 30_000.0)]);
        assert_eq!(
            bounded_power_w_capped(None, &[&loose], 0, t0(), 22_000.0, 230.0),
            None
        );
    }

    #[test]
    fn capped_stacks_two_ceilings_taking_the_tightest() {
        // TxProfile 11 kW, ChargingStationMaxProfile 9 kW, external constraint
        // 5 kW → the outermost (tightest) wins: min(22k, 11k, 9k, 5k) = 5 kW.
        // Order-independent (both enter a `min`).
        let tx = tx_profile(1, 11_000.0);
        let max = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 9_000.0)]);
        let external = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 5_000.0)]);
        assert_eq!(
            bounded_power_w_capped(Some(&tx), &[&max, &external], 0, t0(), 22_000.0, 230.0),
            Some(5_000.0)
        );
        assert_eq!(
            bounded_power_w_capped(Some(&tx), &[&external, &max], 0, t0(), 22_000.0, 230.0),
            Some(5_000.0),
            "min is order-independent"
        );
    }

    #[test]
    fn capped_with_no_ceilings_matches_bounded_power_w() {
        // The empty-ceilings composition is exactly the pre-#511 single-profile
        // bound (no regression to the metering path).
        let tx = tx_profile(1, 6_000.0);
        for natural in [22_000.0, 5_000.0, 6_000.0] {
            assert_eq!(
                bounded_power_w_capped(Some(&tx), &[], 0, t0(), natural, 230.0),
                bounded_power_w(&tx, 0, t0(), natural, 230.0),
                "natural={natural}"
            );
        }
    }

    #[test]
    fn compose_capped_caps_each_period_by_a_flat_ceiling() {
        // Base steps 11 kW → 7.4 kW; a flat 8 kW ceiling clips only the first
        // period, so the composite reads 8 kW then 7.4 kW.
        let base = stepped_profile(
            ChargingRateUnitEnumType::W,
            &[(0, 11_000.0), (1_800, 7_400.0)],
        );
        let ceiling = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 8_000.0)]);
        let sched =
            compose_composite_schedule_capped(1, &base, &[&ceiling], t0(), 3_600, None, 230.0)
                .expect("the base profile constrains the window");
        let periods = &sched.charging_schedule_period;
        assert_eq!(periods.len(), 2);
        assert_eq!((periods[0].start_period, periods[0].limit), (0, 8_000.0));
        assert_eq!(
            (periods[1].start_period, periods[1].limit),
            (1_800, 7_400.0)
        );
    }

    #[test]
    fn compose_capped_opens_a_new_period_at_a_ceiling_step() {
        // A flat 11 kW base, but a ceiling that steps 9 kW → 4 kW at 1 800 s: the
        // composite must break at the ceiling's step even though the base is flat.
        let base = tx_profile(1, 11_000.0);
        let ceiling = stepped_profile(
            ChargingRateUnitEnumType::W,
            &[(0, 9_000.0), (1_800, 4_000.0)],
        );
        let sched =
            compose_composite_schedule_capped(1, &base, &[&ceiling], t0(), 3_600, None, 230.0)
                .expect("the base profile constrains the whole window");
        let periods = &sched.charging_schedule_period;
        assert_eq!(periods.len(), 2, "the ceiling step opens a new period");
        assert_eq!((periods[0].start_period, periods[0].limit), (0, 9_000.0));
        assert_eq!(
            (periods[1].start_period, periods[1].limit),
            (1_800, 4_000.0)
        );
    }

    #[test]
    fn compose_capped_with_no_ceilings_equals_the_uncapped_composite() {
        // No regression: capped with an empty ceiling slice is the uncapped result.
        let base = stepped_profile(
            ChargingRateUnitEnumType::W,
            &[(0, 11_000.0), (1_800, 7_400.0)],
        );
        assert_eq!(
            compose_composite_schedule_capped(1, &base, &[], t0(), 3_600, None, 230.0),
            compose_composite_schedule(1, &base, t0(), 3_600, None, 230.0)
        );
    }

    #[test]
    fn compose_capped_ceiling_alone_never_manufactures_a_schedule() {
        // A base profile with no period in force over the window yields None even
        // with a ceiling installed — a ceiling caps, it does not create a schedule.
        let deferred = stepped_profile(ChargingRateUnitEnumType::W, &[(600, 7_400.0)]);
        let ceiling = stepped_profile(ChargingRateUnitEnumType::W, &[(0, 5_000.0)]);
        assert_eq!(
            compose_composite_schedule_capped(1, &deferred, &[&ceiling], t0(), 300, None, 230.0),
            None
        );
    }
}
