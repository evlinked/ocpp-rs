//! v201 firmware-publish progress tracker — the set of in-flight
//! `PublishFirmware` progress streams a Local Controller is currently driving
//! (OCPP 2.0.1 Part 2, firmware management — the local firmware-cache trigger).
//!
//! `PublishFirmware` is how a CSMS tells a station acting as a Local Controller
//! to download a firmware image once and cache it locally, so the chargers
//! behind it can pull it over the LAN. The station acks *synchronously* with a
//! [`GenericStatusEnumType`](ocpp_types::v201::GenericStatusEnumType); when the
//! request was `Accepted` it then reports download/publish progress
//! *asynchronously* as one or more `PublishFirmwareStatusNotification.req`
//! messages, correlated by the request's `requestId`. This store remembers which
//! `requestId`s have a progress stream in flight so the handler can answer a
//! *retry* of the same request deterministically — **without** launching a
//! second, duplicate stream.
//!
//! Like the sibling [`V201CustomerInformationStore`] and unlike the single-
//! resource `GetLog` / `UpdateFirmware` trackers (a station uploads one log, and
//! runs one firmware rollout, at a time — so those keep a *single* in-flight
//! `requestId` and a *different* one supersedes it), a firmware *publish* is
//! modelled as independent per `requestId`: two different `requestId`s can each
//! have a stream in flight with neither cancelling the other. So this keeps a
//! **set** of in-flight ids rather than a single slot, and there is no
//! supersede / cancel notion — only "already publishing this id" vs. "not"
//! (`PublishFirmwareStatusEnumType` has no cancel value either).
//!
//! Deciding the synchronous [`GenericStatusEnumType`](ocpp_types::v201::GenericStatusEnumType)
//! a `PublishFirmware` answers is deliberately **not** this store's job — that
//! pure decision lives in
//! [`v201_publish_firmware_decision`](crate::v201_command::v201_publish_firmware_decision);
//! the handler calls [`begin`](V201PublishFirmwareStore::begin) only once it has
//! decided to accept the request.
//!
//! Interior-mutable behind an [`RwLock`] so a single
//! `Arc<V201PublishFirmwareStore>` can be shared across the charge point's
//! tasks, exactly like the sibling
//! [`V201CustomerInformationStore`](crate::v201_customer_information::V201CustomerInformationStore).
//!
//! [`V201CustomerInformationStore`]: crate::v201_customer_information::V201CustomerInformationStore

use std::collections::HashSet;
use tokio::sync::RwLock;

/// Tracks the set of `PublishFirmware` progress streams currently in flight, by
/// their `requestId`.
///
/// Each `requestId` is CSMS-supplied and stored as an opaque `i32` — only ever
/// inserted, compared, and removed, never parsed or indexed — so no wire value
/// (including `i32::MIN`/`MAX`) can panic here.
#[derive(Debug, Default)]
pub struct V201PublishFirmwareStore {
    /// The `requestId`s whose progress streams are currently in flight.
    in_flight: RwLock<HashSet<i32>>,
}

impl V201PublishFirmwareStore {
    /// A new, empty store (no progress stream in flight).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark `request_id` as having a progress stream in flight, reporting whether
    /// this newly started one.
    ///
    /// Called by the `PublishFirmware` handler once its pure decision has
    /// accepted the request. Returns:
    ///
    /// - **`true`** — `request_id` was not already in flight; it has been
    ///   recorded and the caller should queue a progress stream for it.
    /// - **`false`** — `request_id` already had a stream in flight (a retry of
    ///   an in-flight request); nothing changed and the caller must **not**
    ///   queue a second stream (that would double-report).
    pub async fn begin(&self, request_id: i32) -> bool {
        self.in_flight.write().await.insert(request_id)
    }

    /// Whether a progress stream for `request_id` is currently in flight.
    ///
    /// `request_id` is only compared, never parsed or indexed, so no wire value
    /// can panic.
    pub async fn is_publishing(&self, request_id: i32) -> bool {
        self.in_flight.read().await.contains(&request_id)
    }

    /// The number of progress streams currently in flight (0 when idle).
    pub async fn in_flight_count(&self) -> usize {
        self.in_flight.read().await.len()
    }

    /// Clear `request_id`'s in-flight marker, reporting whether it was set.
    ///
    /// The completion seam the async consumer
    /// ([`run_v201_publish_firmware_status`](crate::ChargePoint)) calls once a
    /// progress stream finishes, returning the id to the "not publishing" state
    /// so a later `PublishFirmware` with the same `requestId` can publish afresh.
    /// Idempotent — completing an id that is not in flight is a no-op returning
    /// `false`. `request_id` is only compared, never parsed or indexed, so no
    /// wire value can panic.
    pub async fn complete(&self, request_id: i32) -> bool {
        self.in_flight.write().await.remove(&request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_new_store_is_empty() {
        let store = V201PublishFirmwareStore::new();
        assert_eq!(store.in_flight_count().await, 0);
        assert!(!store.is_publishing(1).await);
    }

    #[tokio::test]
    async fn begin_records_and_reports_only_the_first_start() {
        let store = V201PublishFirmwareStore::new();
        // A fresh id starts a stream.
        assert!(store.begin(7).await, "a fresh requestId starts a stream");
        assert!(store.is_publishing(7).await);
        assert_eq!(store.in_flight_count().await, 1);

        // A retry of the same in-flight id does not start a second stream.
        assert!(
            !store.begin(7).await,
            "a retry of an in-flight requestId starts no second stream"
        );
        assert_eq!(store.in_flight_count().await, 1);

        // A different id is independent — it starts its own stream (no supersede).
        assert!(store.begin(8).await);
        assert!(store.is_publishing(7).await, "the first id stays in flight");
        assert!(store.is_publishing(8).await);
        assert_eq!(store.in_flight_count().await, 2);
    }

    #[tokio::test]
    async fn complete_clears_only_the_named_id() {
        let store = V201PublishFirmwareStore::new();
        store.begin(1).await;
        store.begin(2).await;

        assert!(
            store.complete(1).await,
            "completing an in-flight id clears it"
        );
        assert!(!store.is_publishing(1).await);
        assert!(store.is_publishing(2).await, "the other id is untouched");
        assert_eq!(store.in_flight_count().await, 1);

        // A completed id can publish afresh (a new request cycle, not a retry).
        assert!(store.begin(1).await, "a completed id can start again");
    }

    #[tokio::test]
    async fn complete_on_an_absent_id_is_a_noop() {
        let store = V201PublishFirmwareStore::new();
        assert!(
            !store.complete(9).await,
            "completing an absent id settles nothing"
        );
        store.begin(9).await;
        assert!(store.complete(9).await);
        assert!(!store.complete(9).await, "a second complete is a no-op");
    }

    #[tokio::test]
    async fn extreme_request_ids_do_not_panic() {
        // `requestId` is CSMS-supplied; extreme values are stored opaquely.
        let store = V201PublishFirmwareStore::new();
        assert!(store.begin(i32::MIN).await);
        assert!(store.begin(i32::MAX).await);
        assert!(store.is_publishing(i32::MIN).await);
        assert!(store.is_publishing(i32::MAX).await);
        assert!(
            !store.begin(i32::MIN).await,
            "extreme id retry is deduped too"
        );
        assert!(store.complete(i32::MIN).await);
        assert!(store.complete(i32::MAX).await);
        assert_eq!(store.in_flight_count().await, 0);
    }
}
