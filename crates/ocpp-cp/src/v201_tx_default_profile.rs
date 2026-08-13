//! v201 `TxDefaultProfile` store — the default charging schedule a station
//! applies to a transaction on an EVSE when no `TxProfile` overrides it
//! (OCPP 2.0.1 Part 2, §K01 Smart Charging; Issue #471).
//!
//! Unlike the transaction-scoped
//! [`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore) —
//! which holds at most one profile per EVSE only for the lifetime of the
//! transaction it bounds — a `TxDefaultProfile` is **station configuration**: it
//! is installed out-of-band by a `SetChargingProfile` and **persists across
//! transactions** until explicitly replaced (a later install on the same key) or
//! cleared. It is therefore a distinct store with its own lifecycle, not managed
//! by `open_transaction` / `close_transaction`.
//!
//! ## Keying and the `evseId = 0` wildcard
//!
//! The store is keyed by EVSE id, mirroring the `TxProfile` store, **with one
//! extra rule the schema mandates**: for a `TxDefaultProfile`, `evseId = 0`
//! "applies the profile to each individual evse" (the verbatim
//! `SetChargingProfile` schema note on `evseId`). So the store holds:
//!
//! * a per-EVSE default under its own `evseId` (`>= 1`), and
//! * a single station-wide default under key `0`.
//!
//! [`effective_for`](V201TxDefaultProfileStore::effective_for) resolves the
//! default in force for a concrete EVSE by that precedence: an EVSE-specific
//! default (`>= 1`) wins over the `0` wildcard, which is the fallback for every
//! EVSE that has no default of its own.
//!
//! ## Precedence against a `TxProfile`
//!
//! A `TxDefaultProfile` is only ever a **fallback**: the metering resolver and
//! `GetCompositeSchedule` consult the `TxProfile` store first and reach for this
//! store only when no `TxProfile` is in force on the EVSE — the
//! `TxProfile > TxDefaultProfile` precedence OCPP 2.0.1 §K01 defines. Composing
//! the two purposes (rather than one falling back to the other) and honoring the
//! `ChargingStationMaxProfile` / `ChargingStationExternalConstraints` ceilings is
//! deliberately out of scope here — those cap a resolved limit rather than
//! substitute for it, and land in a follow-up slice.

use std::collections::HashMap;

use ocpp_types::v201::ChargingProfileType;
use tokio::sync::RwLock;

/// A v201-typed store of installed `TxDefaultProfile`s, keyed by EVSE id (with
/// key `0` the station-wide default that applies to every EVSE lacking its own).
///
/// Interior-mutable behind an [`RwLock`] so a single
/// `Arc<V201TxDefaultProfileStore>` can be shared across the charge point's
/// tasks (the dispatcher that installs, the metering sampler and
/// `GetCompositeSchedule` handler that read), the same discipline as
/// [`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore).
#[derive(Debug, Default)]
pub struct V201TxDefaultProfileStore {
    /// EVSE id → the `TxDefaultProfile` currently in force for it; key `0` is the
    /// station-wide default applied to each EVSE that has no default of its own.
    by_evse: RwLock<HashMap<i32, ChargingProfileType>>,
}

impl V201TxDefaultProfileStore {
    /// A new, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install `profile` against `evse_id`, replacing (upsert) any default
    /// already installed there and returning the displaced one.
    ///
    /// A `TxDefaultProfile` is not transaction-scoped, so a re-install on the
    /// same key **replaces** rather than stacks — the store holds one default per
    /// EVSE key (the last accepted install wins), matching how the `TxProfile`
    /// store keeps one profile per EVSE. `evse_id = 0` installs/replaces the
    /// station-wide default.
    pub async fn install(
        &self,
        evse_id: i32,
        profile: ChargingProfileType,
    ) -> Option<ChargingProfileType> {
        self.by_evse.write().await.insert(evse_id, profile)
    }

    /// The default installed under the exact key `evse_id`, if any (a cloned
    /// snapshot the caller inspects without holding the store lock). Does **not**
    /// apply the `0`-wildcard fallback — use [`effective_for`](Self::effective_for)
    /// for the default *in force* for a concrete EVSE.
    pub async fn get(&self, evse_id: i32) -> Option<ChargingProfileType> {
        self.by_evse.read().await.get(&evse_id).cloned()
    }

    /// The `TxDefaultProfile` in force for a concrete `evse_id`, applying the
    /// schema's `evseId = 0` wildcard: an EVSE-specific default (`>= 1`) wins;
    /// absent that, the station-wide default under key `0` applies; absent both,
    /// `None`.
    ///
    /// A cloned snapshot, taken under a single read lock. An `evse_id` of `0`
    /// resolves to the station-wide default itself (there is no more-specific key
    /// than `0`), and a negative `evse_id` — never a chargeable EVSE — falls
    /// straight through to the `0` wildcard, never panicking.
    pub async fn effective_for(&self, evse_id: i32) -> Option<ChargingProfileType> {
        let by_evse = self.by_evse.read().await;
        by_evse.get(&evse_id).or_else(|| by_evse.get(&0)).cloned()
    }

    /// Remove the default installed under `evse_id`, returning it if one was
    /// present. Idempotent — clearing a key that holds no default is a no-op
    /// returning `None`.
    pub async fn clear(&self, evse_id: i32) -> Option<ChargingProfileType> {
        self.by_evse.write().await.remove(&evse_id)
    }

    /// A cloned `(evse_id, profile)` snapshot of every installed default. The
    /// order is unspecified (a `HashMap` walk); callers key off `evse_id`, never
    /// position.
    pub async fn snapshot(&self) -> Vec<(i32, ChargingProfileType)> {
        self.by_evse
            .read()
            .await
            .iter()
            .map(|(evse_id, profile)| (*evse_id, profile.clone()))
            .collect()
    }

    /// The number of installed defaults (distinct EVSE keys).
    pub async fn len(&self) -> usize {
        self.by_evse.read().await.len()
    }

    /// Whether no default is installed.
    pub async fn is_empty(&self) -> bool {
        self.by_evse.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        ChargingProfileKindEnumType, ChargingProfilePurposeEnumType, ChargingRateUnitEnumType,
        ChargingSchedulePeriodType, ChargingScheduleType,
    };

    /// A minimal schema-shaped `TxDefaultProfile` carrying one flat-limit
    /// schedule, tagged with `id` so an upsert's displaced profile is identifiable.
    fn default_profile(id: i32, limit: f64) -> ChargingProfileType {
        ChargingProfileType {
            id,
            stack_level: 0,
            charging_profile_purpose: ChargingProfilePurposeEnumType::TxDefaultProfile,
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
    async fn install_then_read_back() {
        let store = V201TxDefaultProfileStore::new();
        assert!(store.is_empty().await);
        assert!(store
            .install(1, default_profile(10, 6000.0))
            .await
            .is_none());
        assert_eq!(store.len().await, 1);
        assert_eq!(store.get(1).await.expect("installed").id, 10);
        assert!(store.get(2).await.is_none());
    }

    #[tokio::test]
    async fn install_is_upsert_returning_displaced() {
        let store = V201TxDefaultProfileStore::new();
        assert!(store
            .install(1, default_profile(10, 6000.0))
            .await
            .is_none());
        // A second install on the same EVSE replaces and returns the displaced.
        let displaced = store
            .install(1, default_profile(11, 3000.0))
            .await
            .expect("displaced the first default");
        assert_eq!(displaced.id, 10, "the earlier default is returned");
        assert_eq!(store.len().await, 1, "the store does not grow on upsert");
        assert_eq!(store.get(1).await.expect("installed").id, 11);
    }

    #[tokio::test]
    async fn effective_for_prefers_evse_specific_over_wildcard() {
        let store = V201TxDefaultProfileStore::new();
        store.install(0, default_profile(100, 3000.0)).await; // station-wide
        store.install(2, default_profile(200, 6000.0)).await; // EVSE 2 only

        // EVSE 2 has its own default → wins over the wildcard.
        assert_eq!(store.effective_for(2).await.expect("evse-2").id, 200);
        // EVSE 1 has none → falls back to the station-wide wildcard.
        assert_eq!(store.effective_for(1).await.expect("wildcard").id, 100);
        // Querying key 0 resolves the wildcard itself.
        assert_eq!(store.effective_for(0).await.expect("wildcard").id, 100);
    }

    #[tokio::test]
    async fn effective_for_none_when_no_default_and_no_wildcard() {
        let store = V201TxDefaultProfileStore::new();
        store.install(2, default_profile(200, 6000.0)).await;
        // EVSE 1 has no default and there is no `0` wildcard → nothing in force.
        assert!(store.effective_for(1).await.is_none());
    }

    #[tokio::test]
    async fn effective_for_negative_evse_never_panics() {
        let store = V201TxDefaultProfileStore::new();
        // No wildcard installed → a negative (never-chargeable) EVSE resolves None.
        assert!(store.effective_for(-1).await.is_none());
        assert!(store.effective_for(i32::MIN).await.is_none());
        // With a wildcard, a negative EVSE falls through to it (never panics).
        store.install(0, default_profile(100, 3000.0)).await;
        assert_eq!(store.effective_for(-1).await.expect("wildcard").id, 100);
        assert_eq!(
            store.effective_for(i32::MAX).await.expect("wildcard").id,
            100
        );
    }

    #[tokio::test]
    async fn clear_removes_and_is_idempotent() {
        let store = V201TxDefaultProfileStore::new();
        store.install(1, default_profile(10, 6000.0)).await;
        assert_eq!(store.clear(1).await.expect("removed").id, 10);
        assert!(store.clear(1).await.is_none(), "clearing again is a no-op");
        assert!(store.is_empty().await);
    }

    #[tokio::test]
    async fn snapshot_is_a_detached_clone() {
        let store = V201TxDefaultProfileStore::new();
        store.install(0, default_profile(100, 3000.0)).await;
        store.install(1, default_profile(10, 6000.0)).await;
        let mut snap = store.snapshot().await;
        snap.sort_by_key(|(evse, _)| *evse);
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].0, 0);
        assert_eq!(snap[1].0, 1);
        // Mutating the snapshot does not touch the store.
        snap.clear();
        assert_eq!(store.len().await, 2);
    }
}
