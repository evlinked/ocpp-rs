//! Composite charging-schedule computation for `GetCompositeSchedule`
//! (OCPP 1.6J §5.x Smart Charging, Issue #95).
//!
//! `GetCompositeSchedule` asks the charge point to report the **effective**
//! charging schedule for a connector over a requested window, by combining all
//! installed [`ChargingProfile`]s (see [`crate::charging_profiles`]) according
//! to the 1.6J stacking/priority rules. This module is the pure, side-effect
//! free core of that computation; the CP handler in `lib.rs` gathers the
//! candidate profiles from the store and calls [`compute_composite`].
//!
//! The Python reference (`mobilityhouse/ocpp`) ships only the `GetCompositeSchedule`
//! wire types and an example CP that returns a canned response — it does **not**
//! compute composite schedules — so the algorithm here is ported faithfully from
//! the OCPP 1.6J specification rather than from Python code.
//!
//! ## Combining rules (faithful subset)
//!
//! At every instant in the window the effective limit is the **minimum** of:
//!
//! * the *override* limit — the schedule of the highest-precedence
//!   transaction-scoped profile that applies: `TxProfile` outranks
//!   `TxDefaultProfile`, a profile installed on the requested connector outranks
//!   one inherited from connector 0, and a higher `stackLevel` outranks a lower
//!   one; and
//! * the *ceiling* limit — the `ChargePointMaxProfile` installed at connector 0
//!   (highest `stackLevel`), the charge-point-wide cap.
//!
//! Each profile's instantaneous limit is read from its [`ChargingSchedule`]
//! periods, honoring `validFrom`/`validTo`, the schedule `duration`, and the
//! profile kind.
//!
//! ## Recurring profiles
//!
//! `ChargingProfileKindType::Recurring` profiles repeat their schedule every
//! `Daily` (86 400 s) or `Weekly` (604 800 s) period, phased off `startSchedule`
//! (the recurrence base — only its time-of-day / time-of-week matters). An
//! absolute instant is mapped into the recurrence period and the schedule's
//! periods are evaluated against that wrapped offset; the schedule `duration`,
//! when present, bounds the *active span* of each occurrence (the remainder of
//! the period is an unconstrained gap). Boundaries are emitted for every
//! occurrence overlapping the requested window, so multi-day/-week windows step
//! correctly. `Absolute`/`Relative` profiles are evaluated as a single,
//! non-repeating schedule.
//!
//! ## Known gaps (tracked as follow-ups)
//!
//! * Phase-aware power/current conversion uses a nominal voltage (see
//!   [`NOMINAL_VOLTAGE`]); 1.6J carries no voltage, so an exact W↔A conversion is
//!   not possible.
//! * Intervals where *no* profile applies are reported as gaps (no period),
//!   rather than as an explicit "unlimited" period.

use ocpp_types::v16j::{
    ChargingProfile, ChargingProfileKindType, ChargingProfilePurposeType, ChargingRateUnitType,
    ChargingSchedule, ChargingSchedulePeriod, RecurrencyKindType,
};
use ocpp_types::{DateTime, Utc};

/// Nominal line voltage (volts) used to convert between Amperes and Watts when
/// the requested `chargingRateUnit` differs from a profile's. OCPP 1.6J does not
/// carry a voltage, so this is a documented assumption (European single-phase
/// nominal). Conversions are therefore approximate.
pub const NOMINAL_VOLTAGE: f64 = 230.0;

/// A candidate profile fed into the composite computation, tagged with whether
/// it lives on the requested connector (`specific = true`) or is inherited from
/// the charge-point-wide connector 0 (`specific = false`). The flag only breaks
/// precedence ties between overrides of equal purpose and stack level.
#[derive(Debug, Clone)]
pub struct ScopedProfile {
    /// Installed on the requested connector itself (vs. connector 0).
    pub specific: bool,
    /// The installed profile.
    pub profile: ChargingProfile,
}

/// The instantaneous contribution of a single profile at a point in time.
struct Contribution {
    limit: f64,
    unit: ChargingRateUnitType,
    phases: Option<i32>,
}

/// Convert `limit` from `from` to `to`, using [`NOMINAL_VOLTAGE`] and the period
/// phase count (defaulting to single-phase) when the units differ.
fn convert(limit: f64, from: &ChargingRateUnitType, to: &ChargingRateUnitType, phases: i32) -> f64 {
    match (from, to) {
        (ChargingRateUnitType::A, ChargingRateUnitType::A)
        | (ChargingRateUnitType::W, ChargingRateUnitType::W) => limit,
        // Amps → Watts: P = I · V · phases.
        (ChargingRateUnitType::A, ChargingRateUnitType::W) => {
            limit * NOMINAL_VOLTAGE * phases as f64
        }
        // Watts → Amps: I = P / (V · phases).
        (ChargingRateUnitType::W, ChargingRateUnitType::A) => {
            limit / (NOMINAL_VOLTAGE * phases as f64)
        }
    }
}

/// The absolute time a profile's schedule is anchored to.
///
/// `Absolute` and `Recurring` profiles anchor at `startSchedule` when present,
/// otherwise at the composite window start; for `Recurring` this anchor is the
/// recurrence base (its time-of-day / time-of-week phase). `Relative` profiles
/// are relative to the start of the reported window.
fn anchor(profile: &ChargingProfile, window_start: DateTime<Utc>) -> DateTime<Utc> {
    match profile.charging_profile_kind {
        ChargingProfileKindType::Relative => window_start,
        ChargingProfileKindType::Absolute | ChargingProfileKindType::Recurring => profile
            .charging_schedule
            .start_schedule
            .unwrap_or(window_start),
    }
}

/// The recurrence period in seconds for a `Recurring` profile, or `None` for
/// non-recurring profiles (and for a malformed `Recurring` profile with no
/// `recurrencyKind`, which cannot be unrolled and so falls back to a single,
/// non-repeating evaluation).
fn recurrence_period(profile: &ChargingProfile) -> Option<i64> {
    if profile.charging_profile_kind != ChargingProfileKindType::Recurring {
        return None;
    }
    match profile.recurrency_kind {
        Some(RecurrencyKindType::Daily) => Some(86_400),
        Some(RecurrencyKindType::Weekly) => Some(604_800),
        None => None,
    }
}

/// The limit a single profile imposes at absolute time `at`, or `None` when the
/// profile does not apply then (outside `validFrom`/`validTo`, before its
/// schedule starts, after its schedule `duration`, or with no periods).
fn profile_limit_at(
    profile: &ChargingProfile,
    at: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> Option<Contribution> {
    if let Some(valid_from) = profile.valid_from {
        if at < valid_from {
            return None;
        }
    }
    if let Some(valid_to) = profile.valid_to {
        if at > valid_to {
            return None;
        }
    }

    let sched = &profile.charging_schedule;
    let raw_offset = (at - anchor(profile, window_start)).num_seconds();
    // For `Recurring` profiles the schedule repeats every period, so map the
    // instant into `[0, period)` phased off the recurrence base; the pattern is
    // fully periodic (only the anchor's time-of-day/-week matters), with
    // `validFrom`/`validTo` bounding the calendar range. Non-recurring profiles
    // evaluate once and do not apply before their anchor.
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
    // of the recurrence period is an unconstrained gap).
    if let Some(duration) = sched.duration {
        if offset >= duration as i64 {
            return None;
        }
    }

    // The applicable period is the last one whose startPeriod has been reached.
    // Periods are kept in startPeriod order per the spec; tolerate unordered
    // input by scanning for the max start that is <= offset.
    let mut chosen: Option<&ChargingSchedulePeriod> = None;
    for period in &sched.charging_schedule_period {
        if period.start_period as i64 <= offset
            && chosen.is_none_or(|c| period.start_period >= c.start_period)
        {
            chosen = Some(period);
        }
    }
    let period = chosen?;
    Some(Contribution {
        limit: period.limit,
        unit: sched.charging_rate_unit.clone(),
        phases: period.number_phases,
    })
}

/// Precedence rank of an override purpose (higher wins). `ChargePointMaxProfile`
/// is the ceiling, handled separately, so it is not an override.
fn override_rank(purpose: &ChargingProfilePurposeType) -> Option<u8> {
    match purpose {
        ChargingProfilePurposeType::TxProfile => Some(2),
        ChargingProfilePurposeType::TxDefaultProfile => Some(1),
        ChargingProfilePurposeType::ChargePointMaxProfile => None,
    }
}

/// Pick the winning override contribution at `at`: the applicable
/// `TxProfile`/`TxDefaultProfile` with the greatest
/// `(rank, specific, stackLevel)`.
fn override_limit_at(
    candidates: &[ScopedProfile],
    at: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> Option<Contribution> {
    candidates
        .iter()
        .filter_map(|c| {
            let rank = override_rank(&c.profile.charging_profile_purpose)?;
            let contrib = profile_limit_at(&c.profile, at, window_start)?;
            Some((rank, c.specific, c.profile.stack_level, contrib))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)))
        .map(|(_, _, _, contrib)| contrib)
}

/// Pick the ceiling contribution at `at`: the applicable `ChargePointMaxProfile`
/// with the greatest `stackLevel`.
fn ceiling_limit_at(
    candidates: &[ScopedProfile],
    at: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> Option<Contribution> {
    candidates
        .iter()
        .filter(|c| {
            c.profile.charging_profile_purpose == ChargingProfilePurposeType::ChargePointMaxProfile
        })
        .filter_map(|c| {
            let contrib = profile_limit_at(&c.profile, at, window_start)?;
            Some((c.profile.stack_level, contrib))
        })
        .max_by_key(|(stack, _)| *stack)
        .map(|(_, contrib)| contrib)
}

/// Choose the output unit: the explicitly requested unit, else the unit of the
/// highest-precedence candidate, else Amperes (the 1.6J default).
fn output_unit(
    candidates: &[ScopedProfile],
    requested: Option<ChargingRateUnitType>,
) -> ChargingRateUnitType {
    if let Some(unit) = requested {
        return unit;
    }
    // Prefer the unit of the most authoritative override, else any ceiling.
    candidates
        .iter()
        .filter(|c| override_rank(&c.profile.charging_profile_purpose).is_some())
        .max_by_key(|c| {
            (
                override_rank(&c.profile.charging_profile_purpose),
                c.profile.stack_level,
            )
        })
        .or_else(|| candidates.first())
        .map(|c| c.profile.charging_schedule.charging_rate_unit.clone())
        .unwrap_or(ChargingRateUnitType::A)
}

/// Collect the candidate time offsets (seconds from `window_start`, within
/// `[0, duration)`) at which the composite limit may change: profile period
/// starts, schedule ends, and `validFrom`/`validTo` edges. Extra boundaries are
/// harmless — equal adjacent periods are coalesced — so a superset is fine.
fn boundary_offsets(
    candidates: &[ScopedProfile],
    window_start: DateTime<Utc>,
    duration: i32,
) -> Vec<i32> {
    let mut offsets = vec![0i32];
    let push = |offsets: &mut Vec<i32>, o: i64| {
        if o > 0 && o < duration as i64 {
            offsets.push(o as i32);
        }
    };
    for c in candidates {
        let base = (anchor(&c.profile, window_start) - window_start).num_seconds();
        let sched = &c.profile.charging_schedule;
        match recurrence_period(&c.profile) {
            // Recurring: emit boundaries for every occurrence overlapping the
            // window. Start one occurrence at or before offset 0 (so an active
            // span ending inside the window is captured) and step until the next
            // occurrence begins at or after the window end.
            Some(period) => {
                let mut k = (-base).div_euclid(period);
                loop {
                    let start = base + k * period;
                    if start >= duration as i64 {
                        break;
                    }
                    for p in &sched.charging_schedule_period {
                        push(&mut offsets, start + p.start_period as i64);
                    }
                    if let Some(d) = sched.duration {
                        push(&mut offsets, start + d as i64);
                    }
                    k += 1;
                }
            }
            // Non-recurring: a single schedule anchored at `base`.
            None => {
                for period in &sched.charging_schedule_period {
                    push(&mut offsets, base + period.start_period as i64);
                }
                if let Some(d) = sched.duration {
                    push(&mut offsets, base + d as i64);
                }
            }
        }
        if let Some(vf) = c.profile.valid_from {
            push(&mut offsets, (vf - window_start).num_seconds());
        }
        if let Some(vt) = c.profile.valid_to {
            // The limit is gone the second after validTo.
            push(&mut offsets, (vt - window_start).num_seconds() + 1);
        }
    }
    offsets.sort_unstable();
    offsets.dedup();
    offsets
}

/// The effective (most restrictive) limit and phase count at absolute time
/// `at`, expressed in `unit`, or `None` when no profile constrains this instant.
///
/// Converts every present contribution (the winning override and the ceiling)
/// into `unit` and takes the minimum.
fn effective_limit_at(
    candidates: &[ScopedProfile],
    at: DateTime<Utc>,
    window_start: DateTime<Utc>,
    unit: &ChargingRateUnitType,
) -> Option<(f64, Option<i32>)> {
    let mut limit: Option<f64> = None;
    let mut phases: Option<i32> = None;
    for contrib in [
        override_limit_at(candidates, at, window_start),
        ceiling_limit_at(candidates, at, window_start),
    ]
    .into_iter()
    .flatten()
    {
        // Default to single-phase, and treat a malformed non-positive phase
        // count (untrusted CSMS input) as single-phase to avoid a div-by-zero.
        let phase_count = contrib.phases.filter(|p| *p > 0).unwrap_or(1);
        let value = convert(contrib.limit, &contrib.unit, unit, phase_count);
        match limit {
            Some(current) if current <= value => {}
            _ => {
                limit = Some(value);
                phases = contrib.phases;
            }
        }
    }
    limit.map(|l| (l, phases))
}

/// Compute the composite [`ChargingSchedule`] for a connector over
/// `[start, start + duration_secs)` from the supplied candidate profiles.
///
/// Returns `None` when no profile yields any limit anywhere in the window (the
/// caller maps this to `GetCompositeScheduleStatus::Rejected`); otherwise a
/// schedule anchored at `start`, with periods in the chosen output unit and
/// adjacent equal periods coalesced.
pub fn compute_composite(
    candidates: &[ScopedProfile],
    start: DateTime<Utc>,
    duration_secs: i32,
    requested_unit: Option<ChargingRateUnitType>,
) -> Option<ChargingSchedule> {
    if duration_secs <= 0 {
        return None;
    }
    let unit = output_unit(candidates, requested_unit);

    let mut periods: Vec<ChargingSchedulePeriod> = Vec::new();
    // The effective limit/phases at the *previous* boundary, or `None` if that
    // boundary was a gap (no constraint). Coalescing compares against this rather
    // than the last emitted period, so a limit that disappears into a gap and
    // later returns at the same value is re-emitted — the gap in between carried
    // a different (unconstrained) limit. This matters for recurring profiles
    // whose `duration` leaves a daily/weekly gap.
    let mut prev: Option<(f64, Option<i32>)> = None;
    for offset in boundary_offsets(candidates, start, duration_secs) {
        let at = start + chrono::Duration::seconds(offset as i64);
        let current = effective_limit_at(candidates, at, start, &unit);

        // A `None` here is a gap (no profile constrains this instant); the next
        // present limit is always emitted because `prev` becomes `None`.
        if let Some((limit, phases)) = current {
            let unchanged = matches!(
                prev,
                Some((pl, pp)) if (pl - limit).abs() < f64::EPSILON && pp == phases
            );
            if !unchanged {
                periods.push(ChargingSchedulePeriod {
                    start_period: offset,
                    limit,
                    number_phases: phases,
                });
            }
        }
        prev = current;
    }

    if periods.is_empty() {
        return None;
    }

    Some(ChargingSchedule {
        duration: Some(duration_secs),
        start_schedule: Some(start),
        charging_rate_unit: unit,
        charging_schedule_period: periods,
        min_charging_rate: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn period(start_period: i32, limit: f64) -> ChargingSchedulePeriod {
        ChargingSchedulePeriod {
            start_period,
            limit,
            number_phases: None,
        }
    }

    fn profile(
        id: i32,
        stack_level: i32,
        purpose: ChargingProfilePurposeType,
        periods: Vec<ChargingSchedulePeriod>,
        unit: ChargingRateUnitType,
    ) -> ChargingProfile {
        ChargingProfile {
            charging_profile_id: id,
            transaction_id: None,
            stack_level,
            charging_profile_purpose: purpose,
            charging_profile_kind: ChargingProfileKindType::Absolute,
            recurrency_kind: None,
            valid_from: None,
            valid_to: None,
            charging_schedule: ChargingSchedule {
                duration: None,
                start_schedule: None,
                charging_rate_unit: unit,
                charging_schedule_period: periods,
                min_charging_rate: None,
            },
        }
    }

    fn scoped(specific: bool, profile: ChargingProfile) -> ScopedProfile {
        ScopedProfile { specific, profile }
    }

    #[test]
    fn no_profiles_yields_none() {
        assert!(compute_composite(&[], Utc::now(), 3600, None).is_none());
    }

    #[test]
    fn nonpositive_duration_yields_none() {
        let p = scoped(
            true,
            profile(
                1,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0)],
                ChargingRateUnitType::A,
            ),
        );
        assert!(compute_composite(&[p], Utc::now(), 0, None).is_none());
    }

    #[test]
    fn single_default_profile_passthrough() {
        let p = scoped(
            true,
            profile(
                1,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0)],
                ChargingRateUnitType::A,
            ),
        );
        let sched = compute_composite(&[p], Utc::now(), 3600, None).expect("schedule");
        assert_eq!(sched.charging_rate_unit, ChargingRateUnitType::A);
        assert_eq!(sched.charging_schedule_period.len(), 1);
        assert_eq!(sched.charging_schedule_period[0].start_period, 0);
        assert_eq!(sched.charging_schedule_period[0].limit, 16.0);
        assert_eq!(sched.duration, Some(3600));
    }

    #[test]
    fn cp_max_caps_the_default_profile() {
        // Default wants 32 A but the CP-wide ceiling is 16 A → composite is 16 A.
        let default = scoped(
            true,
            profile(
                10,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 32.0)],
                ChargingRateUnitType::A,
            ),
        );
        let ceiling = scoped(
            false,
            profile(
                1,
                0,
                ChargingProfilePurposeType::ChargePointMaxProfile,
                vec![period(0, 16.0)],
                ChargingRateUnitType::A,
            ),
        );
        let sched = compute_composite(&[default, ceiling], Utc::now(), 3600, None).expect("sched");
        assert_eq!(sched.charging_schedule_period.len(), 1);
        assert_eq!(sched.charging_schedule_period[0].limit, 16.0);
    }

    #[test]
    fn tx_profile_outranks_default() {
        let default = scoped(
            true,
            profile(
                10,
                5,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0)],
                ChargingRateUnitType::A,
            ),
        );
        // TxProfile (lower stack level) still wins on purpose precedence.
        let tx = scoped(
            true,
            profile(
                11,
                0,
                ChargingProfilePurposeType::TxProfile,
                vec![period(0, 10.0)],
                ChargingRateUnitType::A,
            ),
        );
        let sched = compute_composite(&[default, tx], Utc::now(), 3600, None).expect("sched");
        assert_eq!(sched.charging_schedule_period[0].limit, 10.0);
    }

    #[test]
    fn higher_stack_level_wins_within_purpose() {
        let low = scoped(
            true,
            profile(
                10,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0)],
                ChargingRateUnitType::A,
            ),
        );
        let high = scoped(
            true,
            profile(
                11,
                9,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 8.0)],
                ChargingRateUnitType::A,
            ),
        );
        let sched = compute_composite(&[low, high], Utc::now(), 3600, None).expect("sched");
        assert_eq!(sched.charging_schedule_period[0].limit, 8.0);
    }

    #[test]
    fn connector_specific_outranks_inherited_default() {
        let inherited = scoped(
            false,
            profile(
                10,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 32.0)],
                ChargingRateUnitType::A,
            ),
        );
        let specific = scoped(
            true,
            profile(
                11,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0)],
                ChargingRateUnitType::A,
            ),
        );
        let sched =
            compute_composite(&[inherited, specific], Utc::now(), 3600, None).expect("sched");
        assert_eq!(sched.charging_schedule_period[0].limit, 16.0);
    }

    #[test]
    fn multi_period_profile_is_coalesced_and_stepped() {
        // 0–1800 s @ 16 A, then 1800+ @ 8 A.
        let p = scoped(
            true,
            profile(
                1,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0), period(1800, 8.0)],
                ChargingRateUnitType::A,
            ),
        );
        let sched = compute_composite(&[p], Utc::now(), 3600, None).expect("sched");
        assert_eq!(sched.charging_schedule_period.len(), 2);
        assert_eq!(sched.charging_schedule_period[0].start_period, 0);
        assert_eq!(sched.charging_schedule_period[0].limit, 16.0);
        assert_eq!(sched.charging_schedule_period[1].start_period, 1800);
        assert_eq!(sched.charging_schedule_period[1].limit, 8.0);
    }

    #[test]
    fn unit_conversion_amps_to_watts() {
        let p = scoped(
            true,
            profile(
                1,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![ChargingSchedulePeriod {
                    start_period: 0,
                    limit: 16.0,
                    number_phases: Some(1),
                }],
                ChargingRateUnitType::A,
            ),
        );
        let sched =
            compute_composite(&[p], Utc::now(), 3600, Some(ChargingRateUnitType::W)).expect("s");
        assert_eq!(sched.charging_rate_unit, ChargingRateUnitType::W);
        // 16 A · 230 V · 1 phase = 3680 W.
        assert!((sched.charging_schedule_period[0].limit - 3680.0).abs() < f64::EPSILON);
    }

    #[test]
    fn expired_profile_yields_none() {
        let mut p = profile(
            1,
            0,
            ChargingProfilePurposeType::TxDefaultProfile,
            vec![period(0, 16.0)],
            ChargingRateUnitType::A,
        );
        p.valid_to = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(compute_composite(&[scoped(true, p)], Utc::now(), 3600, None).is_none());
    }

    /// Turn an (Absolute) test `profile` into a recurring one anchored at `start`.
    fn make_recurring(
        mut p: ChargingProfile,
        kind: RecurrencyKindType,
        start: DateTime<Utc>,
        duration: Option<i32>,
    ) -> ChargingProfile {
        p.charging_profile_kind = ChargingProfileKindType::Recurring;
        p.recurrency_kind = Some(kind);
        p.charging_schedule.start_schedule = Some(start);
        p.charging_schedule.duration = duration;
        p
    }

    #[test]
    fn daily_recurrence_steps_each_day() {
        // 16 A for the first 8 h of each day, 8 A for the rest — repeated daily.
        let start = Utc::now();
        let p = make_recurring(
            profile(
                1,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0), period(28_800, 8.0)],
                ChargingRateUnitType::A,
            ),
            RecurrencyKindType::Daily,
            start,
            None,
        );
        // Two full days.
        let sched = compute_composite(&[scoped(true, p)], start, 172_800, None).expect("sched");
        let ps = &sched.charging_schedule_period;
        assert_eq!(ps.len(), 4, "two days × two periods");
        assert_eq!((ps[0].start_period, ps[0].limit), (0, 16.0));
        assert_eq!((ps[1].start_period, ps[1].limit), (28_800, 8.0));
        // The second day repeats the pattern, stepped by one period (86 400 s).
        assert_eq!((ps[2].start_period, ps[2].limit), (86_400, 16.0));
        assert_eq!((ps[3].start_period, ps[3].limit), (115_200, 8.0));
    }

    #[test]
    fn daily_recurrence_duration_leaves_a_daily_gap() {
        // A 10 A cap active only for the first hour of each day; the rest of the
        // day is an unconstrained gap. The cap must be re-emitted on day two even
        // though the value is unchanged — the gap between carried no limit.
        let start = Utc::now();
        let p = make_recurring(
            profile(
                2,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 10.0)],
                ChargingRateUnitType::A,
            ),
            RecurrencyKindType::Daily,
            start,
            Some(3600),
        );
        let sched = compute_composite(&[scoped(true, p)], start, 172_800, None).expect("sched");
        let ps = &sched.charging_schedule_period;
        assert_eq!(
            ps.len(),
            2,
            "one active span per day, re-emitted across the gap"
        );
        assert_eq!((ps[0].start_period, ps[0].limit), (0, 10.0));
        assert_eq!((ps[1].start_period, ps[1].limit), (86_400, 10.0));
    }

    #[test]
    fn weekly_recurrence_steps_each_week() {
        // 32 A on the first day of the week, 16 A for the remaining six — weekly.
        let start = Utc::now();
        let p = make_recurring(
            profile(
                3,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 32.0), period(86_400, 16.0)],
                ChargingRateUnitType::A,
            ),
            RecurrencyKindType::Weekly,
            start,
            None,
        );
        // Two full weeks.
        let sched = compute_composite(&[scoped(true, p)], start, 1_209_600, None).expect("sched");
        let ps = &sched.charging_schedule_period;
        assert_eq!(ps.len(), 4, "two weeks × two periods");
        assert_eq!((ps[0].start_period, ps[0].limit), (0, 32.0));
        assert_eq!((ps[1].start_period, ps[1].limit), (86_400, 16.0));
        assert_eq!((ps[2].start_period, ps[2].limit), (604_800, 32.0));
        assert_eq!((ps[3].start_period, ps[3].limit), (691_200, 16.0));
    }

    #[test]
    fn recurrence_phase_follows_start_schedule() {
        // The recurrence is phased off `startSchedule`, not the window start: the
        // anchor sits 1 h before the window, with a 30-min active span. So the
        // first occurrence inside the window opens at 23 h (86 400 − 3 600) and
        // closes 30 min later; the window opens mid-gap.
        let start = Utc::now();
        let anchor = start - chrono::Duration::seconds(3600);
        let p = make_recurring(
            profile(
                4,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![period(0, 16.0)],
                ChargingRateUnitType::A,
            ),
            RecurrencyKindType::Daily,
            anchor,
            Some(1800),
        );
        // 25 h — long enough to contain exactly one occurrence.
        let sched = compute_composite(&[scoped(true, p)], start, 90_000, None).expect("sched");
        let ps = &sched.charging_schedule_period;
        assert_eq!(ps.len(), 1, "exactly one active span within the window");
        assert_eq!((ps[0].start_period, ps[0].limit), (82_800, 16.0));
    }

    #[test]
    fn recurring_without_recurrency_kind_falls_back_to_single_schedule() {
        // A malformed Recurring profile with no recurrencyKind cannot be unrolled;
        // it is evaluated once (like Absolute), not repeated.
        let start = Utc::now();
        let mut p = profile(
            5,
            0,
            ChargingProfilePurposeType::TxDefaultProfile,
            vec![period(0, 16.0)],
            ChargingRateUnitType::A,
        );
        p.charging_profile_kind = ChargingProfileKindType::Recurring;
        p.recurrency_kind = None;
        p.charging_schedule.start_schedule = Some(start);
        p.charging_schedule.duration = Some(3600);
        let sched = compute_composite(&[scoped(true, p)], start, 172_800, None).expect("sched");
        let ps = &sched.charging_schedule_period;
        // Single occurrence only: one period at offset 0, no day-two repeat.
        assert_eq!(ps.len(), 1);
        assert_eq!((ps[0].start_period, ps[0].limit), (0, 16.0));
    }
}
