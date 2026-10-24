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

use ocpp_types::v201::PublishFirmwareStatusEnumType;
use std::collections::HashSet;
use tokio::sync::RwLock;

/// The most recent publish-firmware status the station reported, retained so a
/// `TriggerMessage(PublishFirmwareStatusNotification)` can re-report it on demand
/// (Issue #585) — the publish-to-local-cache twin of
/// [`V201FirmwareStatusReport`](crate::v201_firmware_update::V201FirmwareStatusReport).
///
/// A station reports publish progress asynchronously
/// (`PublishFirmwareStatusNotification(Idle → … → Published)`), but a CSMS may
/// ask for the *current* status at any point via `TriggerMessage`. This snapshot
/// is the answer: [`Idle`](PublishFirmwareStatusEnumType::Idle) with no
/// `request_id` and no `location` until the first `PublishFirmware` progress step
/// is emitted, then the latest `(status, location, requestId)` thereafter —
/// retained even after the in-flight marker is cleared, so a settled `Published`
/// (or a terminal failure) is still reportable.
///
/// Unlike the single-slot firmware-update snapshot this carries `location`: the
/// terminal [`Published`](PublishFirmwareStatusEnumType::Published) advertises the
/// cached image's download URIs, so a faithful re-report must reproduce them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V201PublishFirmwareStatusReport {
    /// The latest reported stage of the firmware-publish lifecycle.
    pub status: PublishFirmwareStatusEnumType,
    /// The URIs the published image can be downloaded from — `Some` only when the
    /// latest reported `status` was [`Published`](PublishFirmwareStatusEnumType::Published),
    /// `None` for every intermediate state and the initial `Idle` (mirroring what
    /// the async progress stream itself carries per state).
    pub location: Option<Vec<String>>,
    /// The `requestId` of the `PublishFirmware` that stage belongs to, or `None`
    /// for the initial [`Idle`](PublishFirmwareStatusEnumType::Idle) (no publish
    /// has run, so the status is not tied to a specific request — the 2.0.1 schema
    /// omits `requestId` in that case).
    pub request_id: Option<i32>,
}

impl Default for V201PublishFirmwareStatusReport {
    /// A station that has never run a publish:
    /// [`Idle`](PublishFirmwareStatusEnumType::Idle), no `location`, no
    /// correlating `requestId`.
    fn default() -> Self {
        Self {
            status: PublishFirmwareStatusEnumType::Idle,
            location: None,
            request_id: None,
        }
    }
}

/// Tracks the set of `PublishFirmware` progress streams currently in flight, by
/// their `requestId`, plus the latest publish-firmware status the station has
/// reported.
///
/// For the in-flight set: each `requestId` is CSMS-supplied and stored as an
/// opaque `i32` — only ever inserted, compared, and removed, never parsed or
/// indexed — so no wire value (including `i32::MIN`/`MAX`) can panic here.
///
/// Separately, [`last_reported`](Self::last_reported) retains the most recent
/// [`V201PublishFirmwareStatusReport`] the station emitted (via
/// [`record_reported`](Self::record_reported)), so a
/// `TriggerMessage(PublishFirmwareStatusNotification)` can re-report the current
/// status without re-running a publish. It is deliberately *not* cleared when an
/// in-flight id settles ([`complete`](Self::complete)): a finished publish leaves
/// the station's last status (`Published`, or a terminal failure) as the truthful
/// thing to report. With independent per-id streams the "latest" is simply the
/// most recent emit across all of them — the station's current publish status.
#[derive(Debug, Default)]
pub struct V201PublishFirmwareStore {
    /// The `requestId`s whose progress streams are currently in flight.
    in_flight: RwLock<HashSet<i32>>,
    /// The most recent publish-firmware status the station reported. `Idle` (no
    /// `location`/`requestId`) until the first progress step; see
    /// [`V201PublishFirmwareStatusReport`].
    last_reported: RwLock<V201PublishFirmwareStatusReport>,
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

    /// Record `status` (from `PublishFirmware` request `request_id`, carrying
    /// `location` only on the terminal `Published` state) as the latest
    /// publish-firmware status the station has reported.
    ///
    /// Called at the single emit choke point
    /// (`ChargePoint::send_v201_publish_firmware_status`) for every progress step,
    /// so the snapshot tracks the full lifecycle — interim (`DownloadScheduled` …
    /// `Downloaded`) and terminal (`Published` / a failure state). It is
    /// independent of the in-flight set: a completed publish clears its id but this
    /// retains the terminal status so a later
    /// `TriggerMessage(PublishFirmwareStatusNotification)` still re-reports it. With
    /// independent per-id streams the newest emit wins, which is the station's
    /// current publish status. `request_id` is only stored, never parsed or
    /// indexed, so no wire value can panic.
    pub async fn record_reported(
        &self,
        status: PublishFirmwareStatusEnumType,
        location: Option<Vec<String>>,
        request_id: i32,
    ) {
        *self.last_reported.write().await = V201PublishFirmwareStatusReport {
            status,
            location,
            request_id: Some(request_id),
        };
    }

    /// The most recent [`V201PublishFirmwareStatusReport`] the station has
    /// reported.
    ///
    /// The snapshot a `TriggerMessage(PublishFirmwareStatusNotification)`
    /// re-reports on demand. Defaults to [`Idle`](PublishFirmwareStatusEnumType::Idle)
    /// with no `location`/`requestId` on a station that has never run a publish.
    /// Returns a clone, so the caller decides without holding the store lock.
    pub async fn last_reported(&self) -> V201PublishFirmwareStatusReport {
        self.last_reported.read().await.clone()
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

    #[tokio::test]
    async fn a_new_store_reports_idle_with_no_location_or_request_id() {
        let store = V201PublishFirmwareStore::new();
        assert_eq!(
            store.last_reported().await,
            V201PublishFirmwareStatusReport {
                status: PublishFirmwareStatusEnumType::Idle,
                location: None,
                request_id: None,
            },
            "a station that has never run a publish reports Idle, no location/requestId"
        );
    }

    #[tokio::test]
    async fn record_reported_tracks_the_latest_status_and_request_id() {
        let store = V201PublishFirmwareStore::new();
        store
            .record_reported(PublishFirmwareStatusEnumType::Downloading, None, 42)
            .await;
        assert_eq!(
            store.last_reported().await,
            V201PublishFirmwareStatusReport {
                status: PublishFirmwareStatusEnumType::Downloading,
                location: None,
                request_id: Some(42),
            }
        );
        // The latest wins — the terminal Published step overwrites the earlier one
        // and carries the cached-image download URIs.
        let uris = vec!["http://lc.lan/fw.bin".to_string()];
        store
            .record_reported(
                PublishFirmwareStatusEnumType::Published,
                Some(uris.clone()),
                42,
            )
            .await;
        assert_eq!(
            store.last_reported().await,
            V201PublishFirmwareStatusReport {
                status: PublishFirmwareStatusEnumType::Published,
                location: Some(uris),
                request_id: Some(42),
            },
            "the terminal Published re-report retains its location URIs"
        );
    }

    #[tokio::test]
    async fn completing_the_stream_retains_the_last_reported_status() {
        // A settled publish clears the id from the in-flight set, but the terminal
        // status (with its location) must remain reportable for a later trigger.
        let store = V201PublishFirmwareStore::new();
        store.begin(9).await;
        let uris = vec!["ftp://lc.lan/fw.bin".to_string()];
        store
            .record_reported(
                PublishFirmwareStatusEnumType::Published,
                Some(uris.clone()),
                9,
            )
            .await;
        assert!(store.complete(9).await);
        assert_eq!(store.in_flight_count().await, 0, "in-flight marker cleared");
        assert_eq!(
            store.last_reported().await,
            V201PublishFirmwareStatusReport {
                status: PublishFirmwareStatusEnumType::Published,
                location: Some(uris),
                request_id: Some(9),
            },
            "the terminal status survives the in-flight clear"
        );
    }

    #[tokio::test]
    async fn record_reported_accepts_extreme_request_ids() {
        let store = V201PublishFirmwareStore::new();
        store
            .record_reported(
                PublishFirmwareStatusEnumType::DownloadFailed,
                None,
                i32::MIN,
            )
            .await;
        assert_eq!(store.last_reported().await.request_id, Some(i32::MIN));
        store
            .record_reported(PublishFirmwareStatusEnumType::PublishFailed, None, i32::MAX)
            .await;
        assert_eq!(store.last_reported().await.request_id, Some(i32::MAX));
    }
}
