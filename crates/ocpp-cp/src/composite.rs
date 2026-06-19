//! Composite charging-schedule computation for `GetCompositeSchedule`
//! (OCPP 1.6J §5.x Smart Charging, Issue #95).
//!
//! `GetCompositeSchedule` asks the charge point to report the **effective**
//! charging schedule for a connector over a requested window, by combining all
//! installed [`ChargingProfile`]s (see [`crate::charging_profiles`]) according
//! to the 1.6J stacking/priority rules. This module is the pure, side-effect
//! free core of that computation; the CP handler in [`crate::lib`] gathers the
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
//! ## Known gaps (tracked as follow-ups)
//!
//! * **Recurring profiles** (`ChargingProfileKindType::Recurring`,
//!   `Daily`/`Weekly`) are anchored at their `startSchedule` and evaluated as a
//!   single (non-repeating) schedule — the recurrence is **not** unrolled across
//!   the window. Continuous `Absolute`/`Relative` profiles are fully supported.
//! * Phase-aware power/current conversion uses a nominal voltage (see
//!   [`NOMINAL_VOLTAGE`]); 1.6J carries no voltage, so an exact W↔A conversion is
//!   not possible.
//! * Intervals where *no* profile applies are reported as gaps (no period),
//!   rather than as an explicit "unlimited" period.

use ocpp_types::v16j::{
    ChargingProfile, ChargingProfileKindType, ChargingProfilePurposeType, ChargingRateUnitType,
    ChargingSchedule, ChargingSchedulePeriod,
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
/// `Absolute` (and the `Recurring` fallback) anchor at `startSchedule` when
/// present, otherwise at the composite window start. `Relative` profiles are
/// relative to the start of the reported window.
fn anchor(profile: &ChargingProfile, window_start: DateTime<Utc>) -> DateTime<Utc> {
    match profile.charging_profile_kind {
        ChargingProfileKindType::Relative => window_start,
        ChargingProfileKindType::Absolute | ChargingProfileKindType::Recurring => profile
            .charging_schedule
            .start_schedule
            .unwrap_or(window_start),
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
    let offset = (at - anchor(profile, window_start)).num_seconds();
    if offset < 0 {
        return None;
    }
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
        for period in &sched.charging_schedule_period {
            push(&mut offsets, base + period.start_period as i64);
        }
        if let Some(d) = sched.duration {
            push(&mut offsets, base + d as i64);
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
    for offset in boundary_offsets(candidates, start, duration_secs) {
        let at = start + chrono::Duration::seconds(offset as i64);

        // Convert every present contribution into the output unit, then take the
        // minimum — the effective (most restrictive) limit at this instant.
        let mut limit: Option<f64> = None;
        let mut phases: Option<i32> = None;
        for contrib in [
            override_limit_at(candidates, at, start),
            ceiling_limit_at(candidates, at, start),
        ]
        .into_iter()
        .flatten()
        {
            // Default to single-phase, and treat a malformed non-positive phase
            // count (untrusted CSMS input) as single-phase to avoid a div-by-zero.
            let phase_count = contrib.phases.filter(|p| *p > 0).unwrap_or(1);
            let value = convert(contrib.limit, &contrib.unit, &unit, phase_count);
            match limit {
                Some(current) if current <= value => {}
                _ => {
                    limit = Some(value);
                    phases = contrib.phases;
                }
            }
        }

        let Some(limit) = limit else {
            // No profile constrains this instant — leave a gap.
            continue;
        };

        // Coalesce: skip a boundary that does not change the limit/phases.
        if let Some(last) = periods.last() {
            if (last.limit - limit).abs() < f64::EPSILON && last.number_phases == phases {
                continue;
            }
        }
        periods.push(ChargingSchedulePeriod {
            start_period: offset,
            limit,
            number_phases: phases,
        });
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
}
