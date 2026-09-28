//! v201 log-upload tracker — the single in-flight `GetLog` request a Charging
//! Station is currently serving (OCPP 2.0.1 Part 2, security profile).
//!
//! `GetLog` is how a CSMS asks the station to collect a diagnostics or security
//! log and upload it to a remote location. The station acks *synchronously* with
//! a [`LogStatusEnumType`](ocpp_types::v201::LogStatusEnumType), then reports
//! upload progress *asynchronously* via `LogStatusNotification.req`, correlated
//! by the request's `requestId`. A station uploads one log at a time, so it
//! needs to remember which `requestId` is in flight to answer a second `GetLog`
//! deterministically:
//!
//! - a `GetLog` while **nothing** is in flight starts a fresh upload
//!   ([`Accepted`](ocpp_types::v201::LogStatusEnumType::Accepted));
//! - a `GetLog` carrying the **same** `requestId` as the in-flight one is a
//!   retry — idempotently the same answer, no second upload;
//! - a `GetLog` carrying a **different** `requestId` supersedes the in-flight
//!   upload ([`AcceptedCanceled`](ocpp_types::v201::LogStatusEnumType::AcceptedCanceled)):
//!   the previous upload is canceled to serve the new one.
//!
//! This store keeps *only* the in-flight `requestId`; deciding the
//! [`LogStatusEnumType`](ocpp_types::v201::LogStatusEnumType) a `GetLog` answers
//! is deliberately **not** its job — that pure decision lives in
//! [`v201_get_log_decision`](crate::v201_command::v201_get_log_decision), and the
//! handler calls [`begin`](V201LogUploadStore::begin) only once it has decided to
//! accept. [`clear`](V201LogUploadStore::clear) is the completion seam a future
//! async `LogStatusNotification(Uploaded)` slice will call when the upload
//! finishes (or permanently fails), returning the station to idle.
//!
//! Interior-mutable behind an [`RwLock`] so a single `Arc<V201LogUploadStore>`
//! can be shared across the charge point's tasks, exactly like the v201
//! [`V201DisplayMessageStore`](crate::v201_display_message::V201DisplayMessageStore).

use ocpp_types::v201::UploadLogStatusEnumType;
use tokio::sync::RwLock;

/// The most recent log-upload status the station reported, retained so a
/// `TriggerMessage(LogStatusNotification)` can re-report it on demand (Issue
/// #584) — the OCPP 2.0.1 twin of the firmware
/// [`V201FirmwareStatusReport`](crate::v201_firmware_update::V201FirmwareStatusReport)
/// and of the 1.6J `TriggerMessage(DiagnosticsStatusNotification)` re-report.
///
/// A station reports upload progress asynchronously
/// (`LogStatusNotification(Uploading → Uploaded)`), but a CSMS may ask for the
/// *current* status at any point via `TriggerMessage`. This snapshot is the
/// answer: [`Idle`](UploadLogStatusEnumType::Idle) with no `request_id` until the
/// first `GetLog` progress step is emitted, then the latest `(status, requestId)`
/// thereafter — retained even after the in-flight slot is cleared, so a settled
/// `Uploaded` (or a terminal `UploadFailure` / `AcceptedCanceled`) stays
/// reportable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V201LogStatusReport {
    /// The latest reported stage of the log-upload lifecycle.
    pub status: UploadLogStatusEnumType,
    /// The `requestId` of the `GetLog` that stage belongs to, or `None` for the
    /// initial [`Idle`](UploadLogStatusEnumType::Idle) (no upload has run, so the
    /// status is not tied to a specific request — the 2.0.1 schema omits
    /// `requestId` in that case).
    pub request_id: Option<i32>,
}

impl Default for V201LogStatusReport {
    /// A station that has never run an upload:
    /// [`Idle`](UploadLogStatusEnumType::Idle), no correlating `requestId`.
    fn default() -> Self {
        Self {
            status: UploadLogStatusEnumType::Idle,
            request_id: None,
        }
    }
}

/// Tracks the single `GetLog` upload a station is currently serving, by its
/// `requestId`, plus the latest log-upload status it has reported.
///
/// For the in-flight slot: `None` means idle (no upload in flight);
/// `Some(request_id)` names the request whose upload is underway. The `requestId`
/// is CSMS-supplied and stored as an opaque `i32` — never parsed or indexed — so
/// no wire value (including `i32::MIN`/`MAX`) can panic here.
///
/// Separately, [`last_reported`](Self::last_reported) retains the most recent
/// [`V201LogStatusReport`] the station emitted (via
/// [`record_reported`](Self::record_reported)), so a
/// `TriggerMessage(LogStatusNotification)` can re-report the current status
/// without re-running the upload. It is deliberately *not* cleared when the
/// in-flight slot is ([`complete`](Self::complete) / [`clear`](Self::clear)): a
/// finished upload leaves the station idle but its last status (`Uploaded`, or a
/// terminal failure) remains the truthful thing to report.
#[derive(Debug, Default)]
pub struct V201LogUploadStore {
    /// The `requestId` of the upload currently in flight, or `None` when idle.
    in_flight: RwLock<Option<i32>>,
    /// The most recent log-upload status the station reported. `Idle`/`None`
    /// until the first progress step; see [`V201LogStatusReport`].
    last_reported: RwLock<V201LogStatusReport>,
}

impl V201LogUploadStore {
    /// A new, idle store (no upload in flight).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The `requestId` of the upload currently in flight, or `None` when idle.
    ///
    /// A cheap read the `GetLog` handler takes before deciding: the pure decision
    /// ([`v201_get_log_decision`](crate::v201_command::v201_get_log_decision))
    /// keys off whether — and which — request is in flight. Returns a copied
    /// `Option<i32>`, so the caller decides without holding the store lock.
    pub async fn in_flight(&self) -> Option<i32> {
        *self.in_flight.read().await
    }

    /// Whether no upload is currently in flight (the station is idle).
    pub async fn is_idle(&self) -> bool {
        self.in_flight.read().await.is_none()
    }

    /// Record `request_id` as the upload now in flight, returning the `requestId`
    /// it displaced (if any).
    ///
    /// Called by the `GetLog` handler once its pure decision has accepted the
    /// request. A `Some(previous)` return where `previous != request_id` is a
    /// supersede (the decision answered `AcceptedCanceled`); `Some(previous)`
    /// where `previous == request_id` is an idempotent retry of the same request;
    /// `None` is a fresh start from idle.
    pub async fn begin(&self, request_id: i32) -> Option<i32> {
        self.in_flight.write().await.replace(request_id)
    }

    /// Return the station to idle, yielding the `requestId` that was in flight (if
    /// any).
    ///
    /// The unconditional reset seam. Idempotent — clearing an already-idle store
    /// is a no-op returning `None`. Prefer [`complete`](Self::complete) from an
    /// async upload task, which only settles when the caller still owns the
    /// in-flight slot (so a completing upload cannot wipe a newer one that
    /// superseded it).
    pub async fn clear(&self) -> Option<i32> {
        self.in_flight.write().await.take()
    }

    /// Compare-and-clear the in-flight slot: return the station to idle **only
    /// if** `request_id` is still the upload in flight, reporting whether it was.
    ///
    /// The completion seam the async `LogStatusNotification(Uploading→Uploaded)`
    /// flow (#526) calls when an upload settles. A station uploads one log at a
    /// time, but the CALL-path handler that records a supersede
    /// ([`begin`](Self::begin)) runs *concurrently* with the previous upload's
    /// async task: while that task sleeps, a second `GetLog` can install a new
    /// `requestId`. An unconditional [`clear`](Self::clear) at the end of the
    /// first task would then wipe the *second* upload's marker. This guards
    /// against exactly that:
    ///
    /// - **`true`** — `request_id` was still in flight; it has been cleared and
    ///   the station is idle (unless another upload begins). The upload settled
    ///   as the owner: report its terminal `Uploaded` / `UploadFailure`.
    /// - **`false`** — a different `requestId` is now in flight (a newer `GetLog`
    ///   superseded this one, or the slot was already cleared). Nothing is
    ///   changed — the newer upload keeps its slot — and this upload should report
    ///   the canceled outcome rather than a completion.
    ///
    /// `request_id` is only compared, never parsed or indexed, so no wire value
    /// (including `i32::MIN`/`MAX`) can panic.
    pub async fn complete(&self, request_id: i32) -> bool {
        let mut in_flight = self.in_flight.write().await;
        if *in_flight == Some(request_id) {
            *in_flight = None;
            true
        } else {
            false
        }
    }

    /// Record `status` (from `GetLog` request `request_id`) as the latest
    /// log-upload status the station has reported.
    ///
    /// Called at the single emit choke point
    /// (`ChargePoint::send_v201_log_status`) for every progress step, so the
    /// snapshot tracks the full lifecycle — the interim `Uploading` and the
    /// terminal `Uploaded` / `UploadFailure` / `AcceptedCanceled`. It is
    /// independent of the in-flight slot: a completed upload clears
    /// [`in_flight`](Self::in_flight) but this retains the terminal status so a
    /// later `TriggerMessage(LogStatusNotification)` still re-reports it.
    /// `request_id` is only stored, never parsed or indexed.
    pub async fn record_reported(&self, status: UploadLogStatusEnumType, request_id: i32) {
        *self.last_reported.write().await = V201LogStatusReport {
            status,
            request_id: Some(request_id),
        };
    }

    /// The most recent [`V201LogStatusReport`] the station has reported.
    ///
    /// The snapshot a `TriggerMessage(LogStatusNotification)` re-reports on
    /// demand. Defaults to [`Idle`](UploadLogStatusEnumType::Idle) with no
    /// `requestId` on a station that has never run an upload. Returns a copy, so
    /// the caller decides without holding the store lock.
    pub async fn last_reported(&self) -> V201LogStatusReport {
        *self.last_reported.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_new_store_is_idle() {
        let store = V201LogUploadStore::new();
        assert!(store.is_idle().await);
        assert_eq!(store.in_flight().await, None);
    }

    #[tokio::test]
    async fn begin_records_in_flight_and_returns_the_previous() {
        let store = V201LogUploadStore::new();
        // A fresh start from idle displaces nothing.
        assert_eq!(store.begin(7).await, None);
        assert_eq!(store.in_flight().await, Some(7));
        assert!(!store.is_idle().await);

        // A supersede returns the request it displaced and installs the new one.
        assert_eq!(
            store.begin(8).await,
            Some(7),
            "begin returns the requestId it superseded"
        );
        assert_eq!(store.in_flight().await, Some(8));

        // Re-beginning the same id is idempotent — returns itself, stays itself.
        assert_eq!(store.begin(8).await, Some(8));
        assert_eq!(store.in_flight().await, Some(8));
    }

    #[tokio::test]
    async fn clear_returns_to_idle_and_is_a_noop_when_already_idle() {
        let store = V201LogUploadStore::new();
        store.begin(3).await;
        assert_eq!(
            store.clear().await,
            Some(3),
            "clear yields what was in flight"
        );
        assert!(store.is_idle().await);
        assert_eq!(
            store.clear().await,
            None,
            "clearing an idle store is a no-op"
        );
    }

    #[tokio::test]
    async fn extreme_request_ids_do_not_panic() {
        // `requestId` is CSMS-supplied; an extreme value is stored opaquely.
        let store = V201LogUploadStore::new();
        assert_eq!(store.begin(i32::MIN).await, None);
        assert_eq!(store.in_flight().await, Some(i32::MIN));
        assert_eq!(store.begin(i32::MAX).await, Some(i32::MIN));
        assert_eq!(store.in_flight().await, Some(i32::MAX));
    }

    #[tokio::test]
    async fn complete_settles_only_when_the_id_still_owns_the_slot() {
        let store = V201LogUploadStore::new();

        // The owner completing returns to idle and reports it did.
        store.begin(1).await;
        assert!(
            store.complete(1).await,
            "the in-flight upload settles as owner"
        );
        assert!(store.is_idle().await);

        // A completion for an id that no longer owns the slot (superseded) leaves
        // the newer upload untouched and reports it did not settle.
        store.begin(1).await;
        store.begin(2).await; // 2 supersedes 1; the slot is now 2's.
        assert!(
            !store.complete(1).await,
            "a superseded upload does not settle"
        );
        assert_eq!(
            store.in_flight().await,
            Some(2),
            "the superseding upload keeps the slot"
        );

        // 2 can still settle as the current owner.
        assert!(store.complete(2).await);
        assert!(store.is_idle().await);
    }

    #[tokio::test]
    async fn complete_on_an_idle_store_is_a_noop() {
        let store = V201LogUploadStore::new();
        assert!(
            !store.complete(7).await,
            "completing an idle store settles nothing"
        );
        assert!(store.is_idle().await);
        // Extreme ids compare, never index — no panic.
        assert!(!store.complete(i32::MIN).await);
        assert!(!store.complete(i32::MAX).await);
    }

    #[tokio::test]
    async fn a_new_store_reports_idle_with_no_request_id() {
        let store = V201LogUploadStore::new();
        assert_eq!(
            store.last_reported().await,
            V201LogStatusReport {
                status: UploadLogStatusEnumType::Idle,
                request_id: None,
            },
            "a station that has never run an upload reports Idle, no requestId"
        );
    }

    #[tokio::test]
    async fn record_reported_tracks_the_latest_status_and_request_id() {
        let store = V201LogUploadStore::new();
        store
            .record_reported(UploadLogStatusEnumType::Uploading, 42)
            .await;
        assert_eq!(
            store.last_reported().await,
            V201LogStatusReport {
                status: UploadLogStatusEnumType::Uploading,
                request_id: Some(42),
            }
        );
        // The latest wins — a later step overwrites the earlier one.
        store
            .record_reported(UploadLogStatusEnumType::Uploaded, 42)
            .await;
        assert_eq!(
            store.last_reported().await,
            V201LogStatusReport {
                status: UploadLogStatusEnumType::Uploaded,
                request_id: Some(42),
            }
        );
    }

    #[tokio::test]
    async fn completing_the_upload_retains_the_last_reported_status() {
        // A settled upload returns the in-flight slot to idle, but the terminal
        // status must remain reportable for a later TriggerMessage.
        let store = V201LogUploadStore::new();
        store.begin(9).await;
        store
            .record_reported(UploadLogStatusEnumType::Uploaded, 9)
            .await;
        assert!(store.complete(9).await);
        assert!(store.is_idle().await, "in-flight slot cleared");
        assert_eq!(
            store.last_reported().await,
            V201LogStatusReport {
                status: UploadLogStatusEnumType::Uploaded,
                request_id: Some(9),
            },
            "the terminal status survives the in-flight clear"
        );
    }

    #[tokio::test]
    async fn record_reported_accepts_extreme_request_ids() {
        let store = V201LogUploadStore::new();
        store
            .record_reported(UploadLogStatusEnumType::UploadFailure, i32::MIN)
            .await;
        assert_eq!(store.last_reported().await.request_id, Some(i32::MIN));
        store
            .record_reported(UploadLogStatusEnumType::AcceptedCanceled, i32::MAX)
            .await;
        assert_eq!(store.last_reported().await.request_id, Some(i32::MAX));
    }
}
