//! Installed Smart Charging profiles (OCPP 1.6J §5.16 / §5.2).
//!
//! A CSMS pushes [`ChargingProfile`]s to cap the power or current a connector
//! (or the whole charge point, connector 0) may draw over time. This module
//! owns the **store** of installed profiles and the spec's install/clear
//! semantics; computing the effective (composite) schedule from them is the
//! `GetCompositeSchedule` follow-up (Issue #95), which reads this store via
//! [`ChargingProfileStore::profiles_for`].
//!
//! The OCPP 1.6J message types and enums are the Rust counterpart of the Python
//! reference's `SetChargingProfile` / `ClearChargingProfile`
//! ([`ocpp/v16/call.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/call.py),
//! [`ocpp/v16/enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/enums.py)).
//! The Python reference ships only the wire types and an example CP that blanket
//! -accepts, so the install/clear/stacking behavior here is ported faithfully
//! from the 1.6J specification rather than from Python code.

use std::collections::HashMap;
use std::sync::Mutex;

use ocpp_types::v16j::{
    ChargingProfile, ChargingProfilePurposeType, ChargingProfileStatus, ClearChargingProfileStatus,
};

/// Decide the [`ChargingProfileStatus`] for a `SetChargingProfile` purely from
/// the spec's placement rules, independent of any stored state.
///
/// Faithful to OCPP 1.6J §5.16 / the `ChargingProfilePurposeType` documentation:
///
/// * `ChargePointMaxProfile` MAY only be installed at connector 0 (the
///   charge-point-wide profile) — at any real connector it is `Rejected`.
/// * `TxProfile` is transaction-scoped and SHALL only target a real connector
///   (`> 0`) — at connector 0 it is `Rejected`.
/// * `TxDefaultProfile` is valid at connector 0 (applies to all connectors) or
///   at a specific connector.
/// * An unknown connector id (neither 0 nor a connector this CP exposes) is
///   `Rejected`.
///
/// `connector_known` tells the function whether `connector_id` (when `> 0`)
/// names a connector that exists on this charge point; the caller supplies it
/// from the live connector map.
pub fn set_profile_status(
    connector_id: i32,
    connector_known: bool,
    purpose: &ChargingProfilePurposeType,
) -> ChargingProfileStatus {
    // Connector 0 = charge-point-wide; any other id must be a real connector.
    if connector_id != 0 && !connector_known {
        return ChargingProfileStatus::Rejected;
    }
    match purpose {
        // CP-wide cap: connector 0 only.
        ChargingProfilePurposeType::ChargePointMaxProfile if connector_id != 0 => {
            ChargingProfileStatus::Rejected
        }
        // Transaction profile: a real connector only.
        ChargingProfilePurposeType::TxProfile if connector_id == 0 => {
            ChargingProfileStatus::Rejected
        }
        _ => ChargingProfileStatus::Accepted,
    }
}

/// Thread-safe store of installed charging profiles, keyed by connector id
/// (`0` = charge-point-wide).
///
/// Cheap to share across tasks behind a plain `Arc<ChargingProfileStore>`: a
/// single inner [`Mutex`] guards the map and is only ever held for the duration
/// of a synchronous install/clear/read (no `.await` while locked), so it cannot
/// deadlock the async dispatcher.
#[derive(Debug, Default)]
pub struct ChargingProfileStore {
    /// connector id → profiles installed against it.
    by_connector: Mutex<HashMap<i32, Vec<ChargingProfile>>>,
}

impl ChargingProfileStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Install `profile` against `connector_id`, applying the 1.6J stacking
    /// rules (§5.16): a charging profile is uniquely identified by its
    /// `chargingProfileId`, and "it is not possible to have multiple charging
    /// profiles with the same `chargingProfilePurpose` and `stackLevel`". So we
    /// first evict any profile with the same id (wherever it lives — ids are
    /// CP-unique) and any profile occupying the same (purpose, stackLevel) slot
    /// on the target connector, then insert the newcomer.
    ///
    /// Caller is responsible for validating placement first via
    /// [`set_profile_status`]; this method assumes the profile is acceptable.
    pub fn set(&self, connector_id: i32, profile: ChargingProfile) {
        let mut map = self.lock();

        // Profile ids are unique CP-wide: drop any prior copy on any connector,
        // and prune connectors that become empty.
        for profiles in map.values_mut() {
            profiles.retain(|p| p.charging_profile_id != profile.charging_profile_id);
        }

        let slot = map.entry(connector_id).or_default();
        // Only one profile per (purpose, stackLevel) on a connector.
        slot.retain(|p| {
            !(p.charging_profile_purpose == profile.charging_profile_purpose
                && p.stack_level == profile.stack_level)
        });
        slot.push(profile);

        map.retain(|_, profiles| !profiles.is_empty());
    }

    /// Clear every installed profile matching *all* of the supplied filters
    /// (OCPP 1.6J §5.2 `ClearChargingProfile`). A `None` filter matches any
    /// value, so an all-`None` request clears the entire store.
    ///
    /// Returns `Accepted` if at least one profile matched and was removed,
    /// `Unknown` otherwise — the faithful `ClearChargingProfileStatus`.
    pub fn clear(
        &self,
        id: Option<i32>,
        connector_id: Option<i32>,
        purpose: Option<ChargingProfilePurposeType>,
        stack_level: Option<i32>,
    ) -> ClearChargingProfileStatus {
        let mut map = self.lock();
        let mut removed = 0usize;

        for (cid, profiles) in map.iter_mut() {
            if let Some(want) = connector_id {
                if *cid != want {
                    continue;
                }
            }
            let before = profiles.len();
            profiles.retain(|p| {
                let matches = id.is_none_or(|v| p.charging_profile_id == v)
                    && purpose
                        .as_ref()
                        .is_none_or(|v| &p.charging_profile_purpose == v)
                    && stack_level.is_none_or(|v| p.stack_level == v);
                // Retain the ones that do NOT match the clear filter.
                !matches
            });
            removed += before - profiles.len();
        }

        map.retain(|_, profiles| !profiles.is_empty());

        if removed > 0 {
            ClearChargingProfileStatus::Accepted
        } else {
            ClearChargingProfileStatus::Unknown
        }
    }

    /// Snapshot the profiles installed against `connector_id`.
    ///
    /// Used by `GetCompositeSchedule` (Issue #95) and by tests; returns a clone
    /// so the caller never holds the inner lock.
    pub fn profiles_for(&self, connector_id: i32) -> Vec<ChargingProfile> {
        self.lock().get(&connector_id).cloned().unwrap_or_default()
    }

    /// Total number of profiles installed across all connectors.
    pub fn len(&self) -> usize {
        self.lock().values().map(Vec::len).sum()
    }

    /// Whether the store holds no profiles.
    pub fn is_empty(&self) -> bool {
        self.lock().values().all(Vec::is_empty)
    }

    /// Lock the inner map, recovering the guard even if a previous holder
    /// panicked — a poisoned lock must not wedge the whole charge point, and the
    /// stored data is plain values with no broken invariant to protect.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<i32, Vec<ChargingProfile>>> {
        self.by_connector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v16j::{
        ChargingProfileKindType, ChargingRateUnitType, ChargingSchedule, ChargingSchedulePeriod,
    };

    fn schedule(limit: f64) -> ChargingSchedule {
        ChargingSchedule {
            duration: None,
            start_schedule: None,
            charging_rate_unit: ChargingRateUnitType::A,
            charging_schedule_period: vec![ChargingSchedulePeriod {
                start_period: 0,
                limit,
                number_phases: None,
            }],
            min_charging_rate: None,
        }
    }

    fn profile(id: i32, stack_level: i32, purpose: ChargingProfilePurposeType) -> ChargingProfile {
        ChargingProfile {
            charging_profile_id: id,
            transaction_id: None,
            stack_level,
            charging_profile_purpose: purpose,
            charging_profile_kind: ChargingProfileKindType::Absolute,
            recurrency_kind: None,
            valid_from: None,
            valid_to: None,
            charging_schedule: schedule(id as f64),
        }
    }

    #[test]
    fn cp_max_profile_only_at_connector_zero() {
        assert_eq!(
            set_profile_status(0, false, &ChargingProfilePurposeType::ChargePointMaxProfile),
            ChargingProfileStatus::Accepted
        );
        assert_eq!(
            set_profile_status(1, true, &ChargingProfilePurposeType::ChargePointMaxProfile),
            ChargingProfileStatus::Rejected
        );
    }

    #[test]
    fn tx_profile_only_at_real_connector() {
        assert_eq!(
            set_profile_status(0, false, &ChargingProfilePurposeType::TxProfile),
            ChargingProfileStatus::Rejected
        );
        assert_eq!(
            set_profile_status(1, true, &ChargingProfilePurposeType::TxProfile),
            ChargingProfileStatus::Accepted
        );
    }

    #[test]
    fn unknown_connector_is_rejected() {
        assert_eq!(
            set_profile_status(7, false, &ChargingProfilePurposeType::TxDefaultProfile),
            ChargingProfileStatus::Rejected
        );
    }

    #[test]
    fn tx_default_profile_valid_at_zero_and_real_connector() {
        assert_eq!(
            set_profile_status(0, false, &ChargingProfilePurposeType::TxDefaultProfile),
            ChargingProfileStatus::Accepted
        );
        assert_eq!(
            set_profile_status(2, true, &ChargingProfilePurposeType::TxDefaultProfile),
            ChargingProfileStatus::Accepted
        );
    }

    #[test]
    fn set_installs_and_lists_by_connector() {
        let store = ChargingProfileStore::new();
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        assert_eq!(store.len(), 1);
        let on_1 = store.profiles_for(1);
        assert_eq!(on_1.len(), 1);
        assert_eq!(on_1[0].charging_profile_id, 10);
        assert!(store.profiles_for(2).is_empty());
    }

    #[test]
    fn same_id_replaces_prior_profile() {
        let store = ChargingProfileStore::new();
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        // Re-set the same id (different stack level) → replaces, not duplicates.
        store.set(
            1,
            profile(10, 5, ChargingProfilePurposeType::TxDefaultProfile),
        );
        assert_eq!(store.len(), 1);
        assert_eq!(store.profiles_for(1)[0].stack_level, 5);
    }

    #[test]
    fn same_purpose_and_stack_level_replaces_prior_profile() {
        let store = ChargingProfileStore::new();
        store.set(
            1,
            profile(10, 3, ChargingProfilePurposeType::TxDefaultProfile),
        );
        // Different id, same (purpose, stackLevel) → the slot is replaced.
        store.set(
            1,
            profile(11, 3, ChargingProfilePurposeType::TxDefaultProfile),
        );
        let on_1 = store.profiles_for(1);
        assert_eq!(on_1.len(), 1);
        assert_eq!(on_1[0].charging_profile_id, 11);
    }

    #[test]
    fn different_stack_levels_coexist() {
        let store = ChargingProfileStore::new();
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        store.set(
            1,
            profile(11, 1, ChargingProfilePurposeType::TxDefaultProfile),
        );
        assert_eq!(store.profiles_for(1).len(), 2);
    }

    #[test]
    fn clear_by_id_removes_only_that_profile() {
        let store = ChargingProfileStore::new();
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        store.set(
            1,
            profile(11, 1, ChargingProfilePurposeType::TxDefaultProfile),
        );
        assert_eq!(
            store.clear(Some(10), None, None, None),
            ClearChargingProfileStatus::Accepted
        );
        let on_1 = store.profiles_for(1);
        assert_eq!(on_1.len(), 1);
        assert_eq!(on_1[0].charging_profile_id, 11);
    }

    #[test]
    fn clear_by_connector_removes_that_connectors_profiles() {
        let store = ChargingProfileStore::new();
        store.set(
            0,
            profile(1, 0, ChargingProfilePurposeType::ChargePointMaxProfile),
        );
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        assert_eq!(
            store.clear(None, Some(1), None, None),
            ClearChargingProfileStatus::Accepted
        );
        assert!(store.profiles_for(1).is_empty());
        assert_eq!(store.profiles_for(0).len(), 1);
    }

    #[test]
    fn clear_by_purpose_and_stack_level() {
        let store = ChargingProfileStore::new();
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        store.set(1, profile(11, 1, ChargingProfilePurposeType::TxProfile));
        assert_eq!(
            store.clear(
                None,
                None,
                Some(ChargingProfilePurposeType::TxProfile),
                Some(1)
            ),
            ClearChargingProfileStatus::Accepted
        );
        let on_1 = store.profiles_for(1);
        assert_eq!(on_1.len(), 1);
        assert_eq!(on_1[0].charging_profile_id, 10);
    }

    #[test]
    fn clear_all_with_empty_filter() {
        let store = ChargingProfileStore::new();
        store.set(
            0,
            profile(1, 0, ChargingProfilePurposeType::ChargePointMaxProfile),
        );
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        assert_eq!(
            store.clear(None, None, None, None),
            ClearChargingProfileStatus::Accepted
        );
        assert!(store.is_empty());
    }

    #[test]
    fn clear_no_match_is_unknown() {
        let store = ChargingProfileStore::new();
        store.set(
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile),
        );
        assert_eq!(
            store.clear(Some(999), None, None, None),
            ClearChargingProfileStatus::Unknown
        );
        // Nothing was removed.
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn clear_empty_store_is_unknown() {
        let store = ChargingProfileStore::new();
        assert_eq!(
            store.clear(None, None, None, None),
            ClearChargingProfileStatus::Unknown
        );
    }
}
