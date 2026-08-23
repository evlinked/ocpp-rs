//! v201 firmware-update tracker — the single in-flight `UpdateFirmware` request a
//! Charging Station is currently serving (OCPP 2.0.1 Part 2, firmware management,
//! L01–L03, Issue #532).
//!
//! `UpdateFirmware` is how a CSMS asks a station to fetch and install a new
//! firmware image from a URL, identifying the rollout by a `requestId`. The
//! station acks *synchronously* with an
//! [`UpdateFirmwareStatusEnumType`](ocpp_types::v201::UpdateFirmwareStatusEnumType),
//! then reports fetch/install progress *asynchronously* via
//! `FirmwareStatusNotification.req`, correlated by that `requestId`. A station
//! runs one firmware update at a time, so it needs to remember which `requestId`
//! is in flight to answer a second `UpdateFirmware` deterministically:
//!
//! - an `UpdateFirmware` while **nothing** is in flight starts a fresh update
//!   ([`Accepted`](ocpp_types::v201::UpdateFirmwareStatusEnumType::Accepted));
//! - an `UpdateFirmware` carrying the **same** `requestId` as the in-flight one is
//!   a retry — idempotently the same answer, no second update;
//! - an `UpdateFirmware` carrying a **different** `requestId` supersedes the
//!   in-flight update
//!   ([`AcceptedCanceled`](ocpp_types::v201::UpdateFirmwareStatusEnumType::AcceptedCanceled)):
//!   the previous update is canceled to serve the new one.
//!
//! This store is the direct sibling of the
//! [`V201LogUploadStore`](crate::v201_log_upload::V201LogUploadStore) — the same
//! single-in-flight-`requestId` supersede model the `GetLog` upload tracker uses.
//! It keeps *only* the in-flight `requestId`; deciding the
//! [`UpdateFirmwareStatusEnumType`](ocpp_types::v201::UpdateFirmwareStatusEnumType)
//! an `UpdateFirmware` answers is deliberately **not** its job — that pure
//! decision lives in
//! [`v201_update_firmware_decision`](crate::v201_command::v201_update_firmware_decision),
//! and the handler calls [`begin`](V201FirmwareUpdateStore::begin) only once it has
//! decided to accept. [`complete`](V201FirmwareUpdateStore::complete) is the
//! completion seam the async `FirmwareStatusNotification(Downloading → … →
//! Installed)` flow (the analogue of the `GetLog` → `LogStatusNotification` async
//! split, #534) calls when the update finishes (or permanently fails), returning
//! the station to idle.
//!
//! Interior-mutable behind an [`RwLock`] so a single `Arc<V201FirmwareUpdateStore>`
//! can be shared across the charge point's tasks, exactly like the
//! [`V201LogUploadStore`](crate::v201_log_upload::V201LogUploadStore).

use tokio::sync::RwLock;

/// Tracks the single `UpdateFirmware` rollout a station is currently serving, by
/// its `requestId`.
///
/// `None` means idle (no update in flight); `Some(request_id)` names the request
/// whose update is underway. The `requestId` is CSMS-supplied and stored as an
/// opaque `i32` — never parsed or indexed — so no wire value (including
/// `i32::MIN`/`MAX`) can panic here.
#[derive(Debug, Default)]
pub struct V201FirmwareUpdateStore {
    /// The `requestId` of the firmware update currently in flight, or `None` when
    /// idle.
    in_flight: RwLock<Option<i32>>,
}

impl V201FirmwareUpdateStore {
    /// A new, idle store (no firmware update in flight).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The `requestId` of the firmware update currently in flight, or `None` when
    /// idle.
    ///
    /// A cheap read the `UpdateFirmware` handler takes before deciding: the pure
    /// decision
    /// ([`v201_update_firmware_decision`](crate::v201_command::v201_update_firmware_decision))
    /// keys off whether — and which — request is in flight. Returns a copied
    /// `Option<i32>`, so the caller decides without holding the store lock.
    pub async fn in_flight(&self) -> Option<i32> {
        *self.in_flight.read().await
    }

    /// Whether no firmware update is currently in flight (the station is idle).
    pub async fn is_idle(&self) -> bool {
        self.in_flight.read().await.is_none()
    }

    /// Record `request_id` as the firmware update now in flight, returning the
    /// `requestId` it displaced (if any).
    ///
    /// Called by the `UpdateFirmware` handler once its pure decision has accepted
    /// the request. A `Some(previous)` return where `previous != request_id` is a
    /// supersede (the decision answered `AcceptedCanceled`); `Some(previous)` where
    /// `previous == request_id` is an idempotent retry of the same request; `None`
    /// is a fresh start from idle.
    pub async fn begin(&self, request_id: i32) -> Option<i32> {
        self.in_flight.write().await.replace(request_id)
    }

    /// Return the station to idle, yielding the `requestId` that was in flight (if
    /// any).
    ///
    /// The unconditional reset seam. Idempotent — clearing an already-idle store
    /// is a no-op returning `None`. Prefer [`complete`](Self::complete) from an
    /// async update task, which only settles when the caller still owns the
    /// in-flight slot (so a completing update cannot wipe a newer one that
    /// superseded it).
    pub async fn clear(&self) -> Option<i32> {
        self.in_flight.write().await.take()
    }

    /// Compare-and-clear the in-flight slot: return the station to idle **only if**
    /// `request_id` is still the update in flight, reporting whether it was.
    ///
    /// The completion seam the async
    /// `FirmwareStatusNotification(Downloading→…→Installed)` flow (#534) calls when
    /// an update settles. A station runs one firmware update at a time, but the
    /// CALL-path handler that records a supersede ([`begin`](Self::begin)) runs
    /// *concurrently* with the previous update's async task: while that task
    /// works, a second `UpdateFirmware` can install a new `requestId`. An
    /// unconditional [`clear`](Self::clear) at the end of the first task would then
    /// wipe the *second* update's marker. This guards against exactly that:
    ///
    /// - **`true`** — `request_id` was still in flight; it has been cleared and the
    ///   station is idle (unless another update begins). The update settled as the
    ///   owner: report its terminal `Installed` / `InstallationFailed`.
    /// - **`false`** — a different `requestId` is now in flight (a newer
    ///   `UpdateFirmware` superseded this one, or the slot was already cleared).
    ///   Nothing is changed — the newer update keeps its slot — and this update
    ///   should report the canceled outcome rather than a completion.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_new_store_is_idle() {
        let store = V201FirmwareUpdateStore::new();
        assert!(store.is_idle().await);
        assert_eq!(store.in_flight().await, None);
    }

    #[tokio::test]
    async fn begin_records_in_flight_and_returns_the_previous() {
        let store = V201FirmwareUpdateStore::new();
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
        let store = V201FirmwareUpdateStore::new();
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
        let store = V201FirmwareUpdateStore::new();
        assert_eq!(store.begin(i32::MIN).await, None);
        assert_eq!(store.in_flight().await, Some(i32::MIN));
        assert_eq!(store.begin(i32::MAX).await, Some(i32::MIN));
        assert_eq!(store.in_flight().await, Some(i32::MAX));
    }

    #[tokio::test]
    async fn complete_settles_only_when_the_id_still_owns_the_slot() {
        let store = V201FirmwareUpdateStore::new();

        // The owner completing returns to idle and reports it did.
        store.begin(1).await;
        assert!(
            store.complete(1).await,
            "the in-flight update settles as owner"
        );
        assert!(store.is_idle().await);

        // A completion for an id that no longer owns the slot (superseded) leaves
        // the newer update untouched and reports it did not settle.
        store.begin(1).await;
        store.begin(2).await; // 2 supersedes 1; the slot is now 2's.
        assert!(
            !store.complete(1).await,
            "a superseded update does not settle"
        );
        assert_eq!(
            store.in_flight().await,
            Some(2),
            "the superseding update keeps the slot"
        );

        // 2 can still settle as the current owner.
        assert!(store.complete(2).await);
        assert!(store.is_idle().await);
    }

    #[tokio::test]
    async fn complete_on_an_idle_store_is_a_noop() {
        let store = V201FirmwareUpdateStore::new();
        assert!(
            !store.complete(7).await,
            "completing an idle store settles nothing"
        );
        assert!(store.is_idle().await);
        // Extreme ids compare, never index — no panic.
        assert!(!store.complete(i32::MIN).await);
        assert!(!store.complete(i32::MAX).await);
    }
}
