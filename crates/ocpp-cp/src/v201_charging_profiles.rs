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

use ocpp_types::v201::ChargingProfileType;
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
}
