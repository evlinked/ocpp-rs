//! Running-cost store for the OCPP 2.0.1 `CostUpdated` handler.
//!
//! `CostUpdated` is how a CSMS pushes the **running total cost** of an ongoing
//! transaction to the Charging Station so it can be shown to the driver (e.g.
//! on a display). The message ports
//! [`ocpp.v201.call.CostUpdated`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)
//! (`totalCost: f64`, `transactionId: String`) with an **empty** response —
//! OCPP 2.0.1 Part 2, K (Tariff & Cost) defines no rejection status for it, so
//! the station simply records the latest figure and acknowledges.
//!
//! This module holds the small store that records the latest cost per
//! transaction id, mirroring the `Arc`-shared, async-`RwLock` shape of
//! [`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore).
//! The pure (empty) response builder lives in
//! [`v201_command`](crate::v201_command); the runtime wiring that upserts an
//! inbound cost and answers is in [`crate`]'s `ChargePoint`.

use std::collections::HashMap;

use tokio::sync::RwLock;

/// The latest running total cost the CSMS has pushed per transaction id.
///
/// Keyed by the **wire** `transactionId` string exactly as it arrives (the
/// simulator renders its own live-transaction ids as their decimal spelling, so
/// a cost for a live transaction lands under the same key the transaction is
/// known by). Because `CostUpdated` has no rejection status, a cost is recorded
/// **unconditionally** — including for a `transactionId` the station is not (or
/// not yet) running: OCPP places no ordering guarantee between `CostUpdated` and
/// the station's own view of transaction liveness, so retaining it lets a reader
/// join against [`active_transactions`](crate::ChargePoint) to distinguish a
/// live figure from a stale/pending one, rather than dropping a figure that may
/// belong to a transaction the station is about to (or just did) run.
///
/// Held as `Arc<V201CostStore>` and shared across the charge point's tasks, the
/// same discipline as [`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore).
#[derive(Debug, Default)]
pub struct V201CostStore {
    /// `transactionId` → its latest recorded running total cost.
    by_transaction: RwLock<HashMap<String, f64>>,
}

impl V201CostStore {
    /// A new, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `total_cost` as the latest running cost for `transaction_id`,
    /// returning the cost it displaced if one was already recorded.
    ///
    /// An upsert: a later `CostUpdated` for the same transaction replaces the
    /// earlier figure (the running total only moves forward over a session, and
    /// the station shows the newest value), so the store never stacks or grows a
    /// second entry for the same id. The value is stored **verbatim** — the
    /// decoded [`f64`] with no rounding (see #411 for the float-fidelity
    /// direction; recording the decoded value is correct regardless of it).
    pub async fn update(&self, transaction_id: &str, total_cost: f64) -> Option<f64> {
        self.by_transaction
            .write()
            .await
            .insert(transaction_id.to_string(), total_cost)
    }

    /// The latest cost recorded for `transaction_id`, if any.
    ///
    /// `None` for a transaction the CSMS has never pushed a cost for (the common
    /// case for a session the CSMS is not metering a running price on).
    pub async fn get(&self, transaction_id: &str) -> Option<f64> {
        self.by_transaction
            .read()
            .await
            .get(transaction_id)
            .copied()
    }

    /// A cloned `(transaction_id, total_cost)` snapshot of every recorded cost.
    ///
    /// An owned copy the caller inspects without holding the store lock. The
    /// order is unspecified (a `HashMap` walk); callers key off the id, never
    /// position.
    pub async fn snapshot(&self) -> Vec<(String, f64)> {
        self.by_transaction
            .read()
            .await
            .iter()
            .map(|(id, cost)| (id.clone(), *cost))
            .collect()
    }

    /// The number of transactions with a recorded cost.
    pub async fn len(&self) -> usize {
        self.by_transaction.read().await.len()
    }

    /// Whether no cost has been recorded yet.
    pub async fn is_empty(&self) -> bool {
        self.by_transaction.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_and_reads_back_exactly() {
        let store = V201CostStore::new();
        assert!(store.is_empty().await);
        // A value exactly representable in binary floating point round-trips
        // bit-for-bit through the store.
        assert_eq!(store.update("7", 12.5).await, None);
        assert_eq!(store.get("7").await, Some(12.5));
        assert_eq!(store.len().await, 1);
        assert!(!store.is_empty().await);
    }

    #[tokio::test]
    async fn a_later_update_overwrites_and_returns_the_displaced_cost() {
        let store = V201CostStore::new();
        assert_eq!(store.update("7", 12.0).await, None);
        // The running total moves forward; the newer figure replaces the older
        // and the displaced value is handed back, and the store does not grow a
        // second entry for the same id.
        assert_eq!(store.update("7", 18.25).await, Some(12.0));
        assert_eq!(store.get("7").await, Some(18.25));
        assert_eq!(store.len().await, 1);
    }

    #[tokio::test]
    async fn get_on_an_unrecorded_id_is_none() {
        let store = V201CostStore::new();
        assert_eq!(store.get("nope").await, None);
    }

    #[tokio::test]
    async fn distinct_ids_are_independent() {
        let store = V201CostStore::new();
        store.update("1", 1.0).await;
        store.update("2", 2.0).await;
        assert_eq!(store.get("1").await, Some(1.0));
        assert_eq!(store.get("2").await, Some(2.0));
        assert_eq!(store.len().await, 2);
    }

    #[tokio::test]
    async fn any_id_string_is_accepted_and_never_panics() {
        // `transactionId` is an opaque wire string, never parsed. Empty,
        // whitespace, oversized, and non-numeric ids are all recorded and read
        // back verbatim — the store keys on the exact string.
        let store = V201CostStore::new();
        for id in ["", " 7 ", "not-a-number", &"x".repeat(36)] {
            assert_eq!(store.update(id, 0.0).await, None);
            assert_eq!(store.get(id).await, Some(0.0));
        }
        // Exact-string keying: " 7 " and "7" are distinct keys.
        store.update("7", 9.0).await;
        assert_eq!(store.get("7").await, Some(9.0));
        assert_eq!(store.get(" 7 ").await, Some(0.0));
    }

    #[tokio::test]
    async fn snapshot_is_a_detached_clone() {
        let store = V201CostStore::new();
        store.update("7", 3.5).await;
        let snap = store.snapshot().await;
        assert_eq!(snap, vec![("7".to_string(), 3.5)]);
        // Mutating the store after the snapshot does not change the snapshot.
        store.update("7", 4.5).await;
        assert_eq!(snap, vec![("7".to_string(), 3.5)]);
        assert_eq!(store.get("7").await, Some(4.5));
    }
}
