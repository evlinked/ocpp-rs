//! v201 station-ceiling store — the two station-wide *ceiling* charging-profile
//! purposes a `SetChargingProfile` may install (OCPP 2.0.1 Part 2, §K01 Smart
//! Charging; Issue #511):
//!
//! * [`ChargingStationMaxProfile`](ocpp_types::v201::ChargingProfilePurposeEnumType::ChargingStationMaxProfile)
//!   — a station-wide ceiling (`evseId = 0` carries an overall limit for the whole
//!   Charging Station) that caps the sum across EVSEs.
//! * [`ChargingStationExternalConstraints`](ocpp_types::v201::ChargingProfilePurposeEnumType::ChargingStationExternalConstraints)
//!   — externally-imposed limits (e.g. a grid/DSO signal) the station must respect;
//!   the **outermost** cap.
//!
//! ## Why a distinct store from `V201TxDefaultProfileStore`
//!
//! A `TxDefaultProfile` *substitutes* for the resolved limit when no `TxProfile`
//! is in force. A station ceiling instead **caps** whatever limit was resolved:
//! the effective metering limit is `min(resolved, ceiling)`, and a ceiling binds
//! even when no `TxProfile`/`TxDefaultProfile` is in force at all (a station-wide
//! limit still caps the connector's natural rate). It composes differently and
//! carries two distinct purposes, so it earns its own store rather than crowding
//! the fallback store.
//!
//! ## Keying and the `evseId = 0` wildcard
//!
//! Keyed by `(kind, evseId)`, mirroring the `evseId = 0` wildcard rule the
//! `TxDefaultProfile` store applies: a `ChargingStationMaxProfile` at `evseId = 0`
//! is the whole-station ceiling, while a per-EVSE `ChargingStationMaxProfile`
//! (`evseId >= 1`) is also valid and wins for that EVSE.
//! [`effective_for`](V201StationCeilingStore::effective_for) resolves the ceiling
//! in force for a concrete EVSE by that precedence: an EVSE-specific ceiling
//! (`>= 1`) wins over the `0` whole-station ceiling, which is the fallback for
//! every EVSE lacking its own.
//!
//! ## Precedence between the two ceilings
//!
//! Both are `min` caps, so their **numeric** composition is order-independent —
//! `min(a, min(b, x)) == min(min(a, b), x)`. `ChargingStationExternalConstraints`
//! is nonetheless the semantic outermost cap (an externally-imposed limit the
//! station must always respect); applying both as a `min` honors it regardless of
//! order. See [`crate::v201_charging_profiles::bounded_power_w_capped`] for the
//! composition itself — this module is a pure typed store.

use std::collections::HashMap;

use ocpp_types::v201::{ChargingProfilePurposeEnumType, ChargingProfileType};
use tokio::sync::RwLock;

/// Which station-wide ceiling purpose a stored profile carries — the two
/// `ChargingProfilePurposeEnumType` variants that cap (rather than substitute
/// for) a resolved limit.
///
/// A dedicated key type rather than reusing `ChargingProfilePurposeEnumType`
/// directly: it is closed over exactly the two ceiling purposes (so the store can
/// never hold a `TxProfile`/`TxDefaultProfile`), and it derives `Hash`/`Eq` for
/// use as a `HashMap` key, which the wire enum does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CeilingKind {
    /// `ChargingStationMaxProfile` — the station's own overall ceiling.
    Max,
    /// `ChargingStationExternalConstraints` — an externally-imposed ceiling.
    External,
}

impl CeilingKind {
    /// The ceiling kind for a `ChargingProfilePurposeEnumType`, or `None` for the
    /// two substitutive purposes (`TxProfile` / `TxDefaultProfile`) this store
    /// does not hold. Lets the wiring layer route an accepted install to this
    /// store only for a genuine ceiling purpose.
    #[must_use]
    pub fn from_purpose(purpose: ChargingProfilePurposeEnumType) -> Option<Self> {
        match purpose {
            ChargingProfilePurposeEnumType::ChargingStationMaxProfile => Some(Self::Max),
            ChargingProfilePurposeEnumType::ChargingStationExternalConstraints => {
                Some(Self::External)
            }
            ChargingProfilePurposeEnumType::TxProfile
            | ChargingProfilePurposeEnumType::TxDefaultProfile => None,
        }
    }
}

/// A v201-typed store of installed station-ceiling profiles, keyed by
/// `(kind, evseId)` (with `evseId = 0` the whole-station ceiling that applies to
/// every EVSE lacking its own).
///
/// Interior-mutable behind an [`RwLock`] so a single
/// `Arc<V201StationCeilingStore>` can be shared across the charge point's tasks
/// (the dispatcher that installs, the metering sampler and `GetCompositeSchedule`
/// handler that read), the same discipline as
/// [`V201TxDefaultProfileStore`](crate::v201_tx_default_profile::V201TxDefaultProfileStore).
#[derive(Debug, Default)]
pub struct V201StationCeilingStore {
    /// `(kind, evseId)` → the ceiling currently in force for it; `evseId = 0` is
    /// the whole-station ceiling applied to every EVSE that has no ceiling of its
    /// own for that kind.
    by_key: RwLock<HashMap<(CeilingKind, i32), ChargingProfileType>>,
}

impl V201StationCeilingStore {
    /// A new, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install `profile` against `(kind, evse_id)`, replacing (upsert) any ceiling
    /// already installed there and returning the displaced one.
    ///
    /// A ceiling is station configuration, not transaction-scoped, so a re-install
    /// on the same key **replaces** rather than stacks — the store holds one
    /// ceiling per `(kind, evseId)` (the last accepted install wins).
    /// `evse_id = 0` installs/replaces the whole-station ceiling for that kind.
    pub async fn install(
        &self,
        kind: CeilingKind,
        evse_id: i32,
        profile: ChargingProfileType,
    ) -> Option<ChargingProfileType> {
        self.by_key.write().await.insert((kind, evse_id), profile)
    }

    /// The ceiling installed under the exact key `(kind, evse_id)`, if any (a
    /// cloned snapshot). Does **not** apply the `0`-wildcard fallback — use
    /// [`effective_for`](Self::effective_for) for the ceiling *in force* for a
    /// concrete EVSE.
    pub async fn get(&self, kind: CeilingKind, evse_id: i32) -> Option<ChargingProfileType> {
        self.by_key.read().await.get(&(kind, evse_id)).cloned()
    }

    /// The ceiling of `kind` in force for a concrete `evse_id`, applying the
    /// `evseId = 0` wildcard: an EVSE-specific ceiling (`>= 1`) wins; absent that,
    /// the whole-station ceiling under key `0` applies; absent both, `None`.
    ///
    /// A cloned snapshot, taken under a single read lock. An `evse_id` of `0`
    /// resolves to the whole-station ceiling itself, and a negative `evse_id` —
    /// never a chargeable EVSE — falls straight through to the `0` wildcard, never
    /// panicking.
    pub async fn effective_for(
        &self,
        kind: CeilingKind,
        evse_id: i32,
    ) -> Option<ChargingProfileType> {
        let by_key = self.by_key.read().await;
        by_key
            .get(&(kind, evse_id))
            .or_else(|| by_key.get(&(kind, 0)))
            .cloned()
    }

    /// Remove the ceiling installed under `(kind, evse_id)`, returning it if one
    /// was present. Idempotent — clearing a key that holds no ceiling is a no-op
    /// returning `None`.
    pub async fn clear(&self, kind: CeilingKind, evse_id: i32) -> Option<ChargingProfileType> {
        self.by_key.write().await.remove(&(kind, evse_id))
    }

    /// A cloned `(kind, evse_id, profile)` snapshot of every installed ceiling. The
    /// order is unspecified (a `HashMap` walk); callers key off `(kind, evse_id)`,
    /// never position.
    pub async fn snapshot(&self) -> Vec<(CeilingKind, i32, ChargingProfileType)> {
        self.by_key
            .read()
            .await
            .iter()
            .map(|((kind, evse_id), profile)| (*kind, *evse_id, profile.clone()))
            .collect()
    }

    /// The number of installed ceilings (distinct `(kind, evseId)` keys).
    pub async fn len(&self) -> usize {
        self.by_key.read().await.len()
    }

    /// Whether no ceiling is installed.
    pub async fn is_empty(&self) -> bool {
        self.by_key.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        ChargingProfileKindEnumType, ChargingRateUnitEnumType, ChargingSchedulePeriodType,
        ChargingScheduleType,
    };

    /// A minimal schema-shaped ceiling profile carrying one flat-limit schedule,
    /// tagged with `id` so an upsert's displaced profile is identifiable.
    fn ceiling_profile(
        id: i32,
        purpose: ChargingProfilePurposeEnumType,
        limit: f64,
    ) -> ChargingProfileType {
        ChargingProfileType {
            id,
            stack_level: 0,
            charging_profile_purpose: purpose,
            charging_profile_kind: ChargingProfileKindEnumType::Absolute,
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

    fn max_profile(id: i32, limit: f64) -> ChargingProfileType {
        ceiling_profile(
            id,
            ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
            limit,
        )
    }

    #[test]
    fn from_purpose_maps_only_the_two_ceiling_purposes() {
        assert_eq!(
            CeilingKind::from_purpose(ChargingProfilePurposeEnumType::ChargingStationMaxProfile),
            Some(CeilingKind::Max)
        );
        assert_eq!(
            CeilingKind::from_purpose(
                ChargingProfilePurposeEnumType::ChargingStationExternalConstraints
            ),
            Some(CeilingKind::External)
        );
        assert_eq!(
            CeilingKind::from_purpose(ChargingProfilePurposeEnumType::TxProfile),
            None
        );
        assert_eq!(
            CeilingKind::from_purpose(ChargingProfilePurposeEnumType::TxDefaultProfile),
            None
        );
    }

    #[tokio::test]
    async fn install_then_read_back_is_kind_scoped() {
        let store = V201StationCeilingStore::new();
        assert!(store.is_empty().await);
        assert!(store
            .install(CeilingKind::Max, 1, max_profile(10, 6000.0))
            .await
            .is_none());
        assert_eq!(store.len().await, 1);
        assert_eq!(
            store.get(CeilingKind::Max, 1).await.expect("installed").id,
            10
        );
        // A different kind under the same evse is a distinct key.
        assert!(store.get(CeilingKind::External, 1).await.is_none());
        assert!(store.get(CeilingKind::Max, 2).await.is_none());
    }

    #[tokio::test]
    async fn install_is_upsert_returning_displaced() {
        let store = V201StationCeilingStore::new();
        assert!(store
            .install(CeilingKind::Max, 0, max_profile(10, 6000.0))
            .await
            .is_none());
        let displaced = store
            .install(CeilingKind::Max, 0, max_profile(11, 3000.0))
            .await
            .expect("displaced the first ceiling");
        assert_eq!(displaced.id, 10, "the earlier ceiling is returned");
        assert_eq!(store.len().await, 1, "the store does not grow on upsert");
        assert_eq!(
            store.get(CeilingKind::Max, 0).await.expect("installed").id,
            11
        );
    }

    #[tokio::test]
    async fn effective_for_prefers_evse_specific_over_wildcard() {
        let store = V201StationCeilingStore::new();
        store
            .install(CeilingKind::Max, 0, max_profile(100, 3000.0))
            .await; // whole-station
        store
            .install(CeilingKind::Max, 2, max_profile(200, 6000.0))
            .await; // EVSE 2 only

        assert_eq!(
            store
                .effective_for(CeilingKind::Max, 2)
                .await
                .expect("evse-2")
                .id,
            200
        );
        assert_eq!(
            store
                .effective_for(CeilingKind::Max, 1)
                .await
                .expect("wildcard")
                .id,
            100
        );
        assert_eq!(
            store
                .effective_for(CeilingKind::Max, 0)
                .await
                .expect("wildcard")
                .id,
            100
        );
    }

    #[tokio::test]
    async fn effective_for_is_kind_isolated() {
        let store = V201StationCeilingStore::new();
        store
            .install(CeilingKind::Max, 0, max_profile(100, 3000.0))
            .await;
        // A Max wildcard does not answer for the External kind.
        assert!(store
            .effective_for(CeilingKind::External, 1)
            .await
            .is_none());
        assert_eq!(
            store
                .effective_for(CeilingKind::Max, 1)
                .await
                .expect("max")
                .id,
            100
        );
    }

    #[tokio::test]
    async fn effective_for_negative_evse_never_panics() {
        let store = V201StationCeilingStore::new();
        assert!(store.effective_for(CeilingKind::Max, -1).await.is_none());
        assert!(store
            .effective_for(CeilingKind::Max, i32::MIN)
            .await
            .is_none());
        store
            .install(CeilingKind::Max, 0, max_profile(100, 3000.0))
            .await;
        assert_eq!(
            store
                .effective_for(CeilingKind::Max, -1)
                .await
                .expect("wildcard")
                .id,
            100
        );
        assert_eq!(
            store
                .effective_for(CeilingKind::Max, i32::MAX)
                .await
                .expect("wildcard")
                .id,
            100
        );
    }

    #[tokio::test]
    async fn clear_removes_and_is_idempotent() {
        let store = V201StationCeilingStore::new();
        store
            .install(
                CeilingKind::External,
                1,
                ceiling_profile(
                    10,
                    ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
                    6000.0,
                ),
            )
            .await;
        assert_eq!(
            store
                .clear(CeilingKind::External, 1)
                .await
                .expect("removed")
                .id,
            10
        );
        assert!(
            store.clear(CeilingKind::External, 1).await.is_none(),
            "clearing again is a no-op"
        );
        assert!(store.is_empty().await);
    }

    #[tokio::test]
    async fn snapshot_is_a_detached_clone() {
        let store = V201StationCeilingStore::new();
        store
            .install(CeilingKind::Max, 0, max_profile(100, 3000.0))
            .await;
        store
            .install(
                CeilingKind::External,
                1,
                ceiling_profile(
                    10,
                    ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
                    6000.0,
                ),
            )
            .await;
        let mut snap = store.snapshot().await;
        snap.sort_by_key(|(kind, evse, _)| (*kind as u8, *evse));
        assert_eq!(snap.len(), 2);
        snap.clear();
        assert_eq!(
            store.len().await,
            2,
            "mutating the snapshot does not touch the store"
        );
    }
}
