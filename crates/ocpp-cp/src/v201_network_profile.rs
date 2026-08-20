//! v201 network-connection-profile store — the connectivity settings a CSMS
//! provisions into the Charging Station via `SetNetworkProfile` (OCPP 2.0.1
//! Part 2, provisioning, B09/B10).
//!
//! `SetNetworkProfile` writes a [`NetworkConnectionProfileType`] — the OCPP
//! transport/version, message timeout, security profile, network interface, and
//! the underlying cellular (APN) or VPN bearer the station uses to reach the
//! CSMS — into a numbered `configurationSlot`. A station holds several slots so
//! it can fall back between connections, so this store keys profiles by that
//! slot:
//!
//! - a re-`SetNetworkProfile` for the same slot **replaces** the profile
//!   (upsert, not a second copy) — an operator re-provisioning slot 1 overwrites
//!   it in place (last-writer-wins); and
//! - a different slot stores independently, never disturbing another slot's
//!   profile.
//!
//! `SetNetworkProfile` is a self-contained configuration command with no async
//! follow-up — the store it needs is the whole state surface, unlike the
//! `GetLog` / `CustomerInformation` families that also stream progress back.
//!
//! Interior-mutable behind an [`RwLock`] so a single `Arc<V201NetworkProfileStore>`
//! can be shared across the charge point's tasks, exactly like the sibling v201
//! stores ([`V201CertificateStore`](crate::v201_certificate_store::V201CertificateStore),
//! [`V201DisplayMessageStore`](crate::v201_display_message::V201DisplayMessageStore)).
//! Deciding the
//! [`SetNetworkProfileStatusEnumType`](ocpp_types::v201::SetNetworkProfileStatusEnumType)
//! a `SetNetworkProfile` answers is deliberately *not* this store's job — that
//! pure decision lives in
//! [`v201_set_network_profile_decision`](crate::v201_command::v201_set_network_profile_decision),
//! and the handler upserts here only once it has decided `Accepted`.

use std::collections::HashMap;

use ocpp_types::v201::NetworkConnectionProfileType;
use tokio::sync::RwLock;

/// A store of network connection profiles installed by `SetNetworkProfile`, keyed
/// by the numbered `configurationSlot` each occupies.
///
/// The slot is the natural key: at most one profile is held per slot, so a
/// `SetNetworkProfile` for a slot already present replaces it (upsert, never a
/// duplicate). The stored value is the [`NetworkConnectionProfileType`] as
/// delivered — this store neither dials nor validates it (the simulator never
/// actually connects with the profile; that boundary is the handler's).
/// Populated by the `SetNetworkProfile` handler only once its pure decision
/// returns `Accepted`.
#[derive(Debug, Default)]
pub struct V201NetworkProfileStore {
    /// configuration slot → the profile currently stored in it.
    by_slot: RwLock<HashMap<i32, NetworkConnectionProfileType>>,
}

impl V201NetworkProfileStore {
    /// A new, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Store `profile` in `slot`, replacing any profile already held there.
    ///
    /// Returns the profile that was displaced, if any — so a caller (and a test)
    /// can tell a fresh store (`None`) from a rotation (`Some(previous)`). There
    /// is at most one profile per slot, so a same-slot re-store is a deliberate
    /// replace (last-writer-wins), never a second copy. The store grows only with
    /// distinct `configurationSlot` values the CSMS provisions; a re-store of an
    /// existing slot does not grow it.
    pub async fn upsert(
        &self,
        slot: i32,
        profile: NetworkConnectionProfileType,
    ) -> Option<NetworkConnectionProfileType> {
        self.by_slot.write().await.insert(slot, profile)
    }

    /// The profile currently stored in `slot`, if any.
    ///
    /// A cloned snapshot: the caller inspects it without holding the store lock.
    /// The read path that makes an accepted `SetNetworkProfile` observable in
    /// tests and to an operator.
    pub async fn get(&self, slot: i32) -> Option<NetworkConnectionProfileType> {
        self.by_slot.read().await.get(&slot).cloned()
    }

    /// The `configurationSlot`s that currently hold a profile, in ascending order.
    ///
    /// Sorted so callers (and tests) get a deterministic enumeration; the store
    /// itself makes no ordering promise about its underlying `HashMap`.
    pub async fn slots(&self) -> Vec<i32> {
        let mut slots: Vec<i32> = self.by_slot.read().await.keys().copied().collect();
        slots.sort_unstable();
        slots
    }

    /// How many slots currently hold a profile. A cheap read used by tests.
    pub async fn len(&self) -> usize {
        self.by_slot.read().await.len()
    }

    /// Whether the store currently holds no profiles.
    pub async fn is_empty(&self) -> bool {
        self.by_slot.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{OCPPInterfaceEnumType, OCPPTransportEnumType, OCPPVersionEnumType};

    /// A minimal, well-formed profile whose `ocppCsmsUrl` embeds `tag` so tests
    /// can tell one stored profile from another.
    fn profile(tag: &str) -> NetworkConnectionProfileType {
        NetworkConnectionProfileType {
            ocpp_version: OCPPVersionEnumType::Ocpp20,
            ocpp_transport: OCPPTransportEnumType::Json,
            ocpp_csms_url: format!("wss://csms.example.com/{tag}"),
            message_timeout: 30,
            security_profile: 2,
            ocpp_interface: OCPPInterfaceEnumType::Wireless0,
            apn: None,
            vpn: None,
            custom_data: None,
        }
    }

    #[tokio::test]
    async fn get_returns_none_for_an_unused_slot() {
        let store = V201NetworkProfileStore::new();
        assert_eq!(store.get(1).await, None);
        assert!(store.is_empty().await);
        assert!(store.slots().await.is_empty());
    }

    #[tokio::test]
    async fn upsert_then_read_round_trips_the_profile() {
        let store = V201NetworkProfileStore::new();
        assert_eq!(
            store.upsert(1, profile("a")).await,
            None,
            "a fresh store displaces nothing"
        );
        assert_eq!(store.get(1).await, Some(profile("a")));
        // Scoped to its slot — a sibling slot is unaffected.
        assert_eq!(store.get(2).await, None);
        assert_eq!(store.len().await, 1);
        assert_eq!(store.slots().await, vec![1]);
    }

    #[tokio::test]
    async fn a_second_slot_stores_independently() {
        let store = V201NetworkProfileStore::new();
        store.upsert(1, profile("a")).await;
        store.upsert(2, profile("b")).await;

        // Both slots hold their own profile; neither disturbed the other.
        assert_eq!(store.get(1).await, Some(profile("a")));
        assert_eq!(store.get(2).await, Some(profile("b")));
        assert_eq!(store.len().await, 2);
        assert_eq!(store.slots().await, vec![1, 2]);
    }

    #[tokio::test]
    async fn restoring_the_same_slot_replaces_the_previous_profile() {
        let store = V201NetworkProfileStore::new();
        store.upsert(1, profile("old")).await;

        assert_eq!(
            store.upsert(1, profile("new")).await,
            Some(profile("old")),
            "rotating a slot returns the profile it displaced"
        );
        assert_eq!(
            store.get(1).await,
            Some(profile("new")),
            "the same slot upserts — the last store wins, no duplicate"
        );
        assert_eq!(store.len().await, 1, "a rotation does not grow the store");
    }

    #[tokio::test]
    async fn extreme_slots_are_ordinary_keys() {
        let store = V201NetworkProfileStore::new();
        store.upsert(i32::MIN, profile("min")).await;
        store.upsert(i32::MAX, profile("max")).await;
        store.upsert(0, profile("zero")).await;

        assert_eq!(store.get(i32::MIN).await, Some(profile("min")));
        assert_eq!(store.get(i32::MAX).await, Some(profile("max")));
        assert_eq!(store.get(0).await, Some(profile("zero")));
        // `slots()` sorts as signed integers.
        assert_eq!(store.slots().await, vec![i32::MIN, 0, i32::MAX]);
    }

    #[tokio::test]
    async fn get_is_a_detached_clone() {
        let store = V201NetworkProfileStore::new();
        store.upsert(1, profile("a")).await;

        // A taken snapshot is independent of a later mutation of the same slot.
        let taken = store.get(1).await.unwrap();
        store.upsert(1, profile("b")).await;
        assert_eq!(taken, profile("a"));
        assert_eq!(store.get(1).await, Some(profile("b")));
    }
}
