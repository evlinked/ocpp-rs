//! v201 pending-CSR tracker — the single outstanding `SignCertificate` request a
//! Charging Station has originated but not yet seen answered (OCPP 2.0.1 Part 2,
//! A02 — certificate provisioning).
//!
//! The certificate-provisioning loop is CP-initiated: the station submits a CSR
//! via `SignCertificate.req` ([`ChargePoint::request_sign_certificate`]), the
//! CSMS acks synchronously with a
//! [`GenericStatusEnumType`](ocpp_types::v201::GenericStatusEnumType), and the
//! operator's CA later delivers the signed chain *out-of-band* via a
//! `CertificateSigned` CALL. The two halves land in different code paths — the
//! request is originated from a driver hook, the delivery arrives at the inbound
//! `CertificateSigned` handler — so without a shared marker the delivery cannot
//! know it is *this station's* answer.
//!
//! This store is that marker: a single slot recording the CSR the station is
//! currently waiting on (its [`CertificateSigningUseEnumType`] and a monotonic
//! request sequence for observability). It lets the `CertificateSigned` handler
//! correlate an accepted delivery back to the request that started the flow:
//!
//! - an accepted `SignCertificate.req` [`record`](V201PendingCsrStore::record)s
//!   the pending CSR (a new request supersedes an unanswered one — a station
//!   drives one provisioning at a time);
//! - an inbound `CertificateSigned` the station *accepts*
//!   [`correlate_accepted`](V201PendingCsrStore::correlate_accepted)s against the
//!   slot: [`Solicited`](CsrCorrelation::Solicited) when it clears a pending CSR
//!   (the CA answered the request), [`Unsolicited`](CsrCorrelation::Unsolicited)
//!   when nothing was outstanding (the delivery is accepted on chain-shape alone,
//!   the documented backward-compatible behavior — a station may legitimately
//!   receive a chain it did not track in memory, e.g. after a restart);
//! - a *rejected* `CertificateSigned` (an unusable chain) leaves the marker in
//!   place: the CSR is still outstanding, awaiting a usable chain.
//!
//! Deciding *whether* a delivered chain is installable is deliberately **not**
//! this store's job — that pure predicate lives in
//! [`v201_certificate_signed_status`](crate::v201_command::v201_certificate_signed_status),
//! and the handler correlates only once it has decided to accept. Nor does this
//! store do any PKI: the recorded [`CertificateSigningUseEnumType`] is a closed
//! enum and the CSR/chain never reach it, so no attacker-influenced wire value is
//! parsed here (the same "no X.509 parse" boundary the certificate predicates set).
//!
//! Interior-mutable behind an [`RwLock`] (plus an [`AtomicU64`] sequence) so a
//! single `Arc<V201PendingCsrStore>` can be shared across the charge point's
//! tasks, exactly like the sibling single-slot
//! [`V201LogUploadStore`](crate::v201_log_upload::V201LogUploadStore) and
//! [`V201FirmwareUpdateStore`](crate::v201_firmware_update::V201FirmwareUpdateStore).
//!
//! [`ChargePoint::request_sign_certificate`]: crate::ChargePoint::request_sign_certificate

use std::sync::atomic::{AtomicU64, Ordering};

use ocpp_types::v201::CertificateSigningUseEnumType;
use tokio::sync::RwLock;

/// A recorded outstanding `SignCertificate` request the station is waiting to see
/// answered by a `CertificateSigned` delivery.
///
/// `certificate_type` is the [`CertificateSigningUseEnumType`] the CSR was
/// submitted for (`None` when the request omitted it, applying to both the ISO
/// 15118 and the Charging-Station-to-CSMS connections). `seq` is a
/// station-assigned monotonic request number — never sent on the wire — that
/// makes each originated request individually observable (an operator or a test
/// can tell a re-recorded supersede apart from the request it displaced).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingCsr {
    /// Which certificate the outstanding CSR is for, or `None` when the request
    /// omitted `certificateType`.
    pub certificate_type: Option<CertificateSigningUseEnumType>,
    /// Station-assigned monotonic request sequence, for observability only.
    pub seq: u64,
}

/// The outcome of correlating an *accepted* inbound `CertificateSigned` against
/// the pending-CSR slot.
///
/// A `CertificateSigned` the station decides to install is either the CA's answer
/// to a CSR the station itself submitted ([`Solicited`](Self::Solicited)) or a
/// delivery with no outstanding request behind it
/// ([`Unsolicited`](Self::Unsolicited)). Both are accepted (the acceptance
/// decision is made *before* correlation, on chain shape alone); the distinction
/// is surfaced for logging/observability, not to change the wire answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsrCorrelation {
    /// The delivery cleared a pending CSR — it is the answer to a request the
    /// station originated. Carries the marker that was cleared.
    Solicited(PendingCsr),
    /// No CSR was outstanding — the delivery was unsolicited (accepted on
    /// chain-shape alone, the backward-compatible behavior).
    Unsolicited,
}

/// Tracks the single outstanding `SignCertificate` request the station has
/// originated but not yet seen answered.
///
/// `None` (the pending slot empty) means no CSR is outstanding; `Some(marker)`
/// names the request awaiting its `CertificateSigned` delivery. Idle on the 1.6J
/// path (the CP-initiated `SignCertificate` hook is V201-only).
#[derive(Debug, Default)]
pub struct V201PendingCsrStore {
    /// The CSR currently awaiting a `CertificateSigned` delivery, or `None` when
    /// none is outstanding.
    pending: RwLock<Option<PendingCsr>>,
    /// Monotonic source of the [`PendingCsr::seq`] observability tag, bumped once
    /// per [`record`](Self::record).
    next_seq: AtomicU64,
}

impl V201PendingCsrStore {
    /// A new, idle store (no CSR outstanding).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The CSR currently outstanding, or `None` when none is.
    ///
    /// A cheap copied read the correlation seam and observability accessor take
    /// without holding the store lock.
    pub async fn pending(&self) -> Option<PendingCsr> {
        *self.pending.read().await
    }

    /// Whether no CSR is currently outstanding.
    pub async fn is_idle(&self) -> bool {
        self.pending.read().await.is_none()
    }

    /// Record `certificate_type` as the CSR now outstanding, returning the marker
    /// (with its freshly assigned [`seq`](PendingCsr::seq)).
    ///
    /// Called by [`request_sign_certificate`](crate::ChargePoint::request_sign_certificate)
    /// once the CSMS has *accepted* the `SignCertificate.req`. A station drives one
    /// provisioning at a time, so this replaces any prior unanswered marker — a
    /// second accepted request supersedes the first (whose delivery, if it ever
    /// arrives, then correlates as [`Unsolicited`](CsrCorrelation::Unsolicited)).
    /// The sequence is assigned before the slot is written, so every recorded
    /// marker carries a distinct, increasing `seq`.
    pub async fn record(
        &self,
        certificate_type: Option<CertificateSigningUseEnumType>,
    ) -> PendingCsr {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let marker = PendingCsr {
            certificate_type,
            seq,
        };
        *self.pending.write().await = Some(marker);
        marker
    }

    /// Correlate an *accepted* `CertificateSigned` delivery against the pending
    /// slot, clearing it, and report whether the delivery was solicited.
    ///
    /// The correlation seam the `CertificateSigned` handler calls **only after**
    /// its pure decision ([`v201_certificate_signed_status`](crate::v201_command::v201_certificate_signed_status))
    /// has accepted the chain. An accepted delivery installs the station's own
    /// certificate, ending the provisioning loop, so it clears the marker:
    ///
    /// - [`Solicited`](CsrCorrelation::Solicited) — a CSR was outstanding; it has
    ///   been cleared and the station is idle again. The carried marker lets the
    ///   caller log which request (and `certificateType`) the delivery answered.
    /// - [`Unsolicited`](CsrCorrelation::Unsolicited) — nothing was outstanding;
    ///   the slot was already idle and stays idle. The delivery is still accepted
    ///   (on chain shape), just not attributable to a tracked request.
    ///
    /// A *rejected* delivery must **not** call this: the chain was unusable, so
    /// the CSR is still outstanding and its marker is left in place for a later,
    /// usable `CertificateSigned`.
    pub async fn correlate_accepted(&self) -> CsrCorrelation {
        match self.pending.write().await.take() {
            Some(marker) => CsrCorrelation::Solicited(marker),
            None => CsrCorrelation::Unsolicited,
        }
    }

    /// Unconditionally clear the pending slot, yielding the marker that was
    /// outstanding (if any).
    ///
    /// The idempotent reset seam — clearing an already-idle store is a no-op
    /// returning `None`. Distinct from [`correlate_accepted`](Self::correlate_accepted),
    /// which reports the solicited/unsolicited distinction; this is the plain
    /// "abandon the outstanding CSR" path (e.g. a future provisioning-timeout).
    pub async fn clear(&self) -> Option<PendingCsr> {
        self.pending.write().await.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_new_store_is_idle() {
        let store = V201PendingCsrStore::new();
        assert!(store.is_idle().await);
        assert_eq!(store.pending().await, None);
    }

    #[tokio::test]
    async fn record_marks_a_pending_csr_with_its_type() {
        let store = V201PendingCsrStore::new();
        let marker = store
            .record(Some(
                CertificateSigningUseEnumType::ChargingStationCertificate,
            ))
            .await;
        assert_eq!(
            marker.certificate_type,
            Some(CertificateSigningUseEnumType::ChargingStationCertificate)
        );
        assert!(!store.is_idle().await);
        assert_eq!(store.pending().await, Some(marker));
    }

    #[tokio::test]
    async fn record_omitted_type_is_preserved_as_none() {
        // A `SignCertificate.req` with no `certificateType` records a `None`
        // marker (the request applies to both connections) — still outstanding.
        let store = V201PendingCsrStore::new();
        let marker = store.record(None).await;
        assert_eq!(marker.certificate_type, None);
        assert_eq!(store.pending().await, Some(marker));
    }

    #[tokio::test]
    async fn each_record_gets_a_distinct_increasing_seq() {
        let store = V201PendingCsrStore::new();
        let first = store.record(None).await;
        let second = store
            .record(Some(CertificateSigningUseEnumType::V2GCertificate))
            .await;
        assert_eq!(first.seq, 0);
        assert_eq!(second.seq, 1);
        assert!(second.seq > first.seq);
    }

    #[tokio::test]
    async fn a_second_record_supersedes_an_unanswered_one() {
        // A station drives one provisioning at a time: a new accepted request
        // replaces the prior unanswered marker.
        let store = V201PendingCsrStore::new();
        store
            .record(Some(
                CertificateSigningUseEnumType::ChargingStationCertificate,
            ))
            .await;
        let second = store
            .record(Some(CertificateSigningUseEnumType::V2GCertificate))
            .await;
        assert_eq!(
            store.pending().await,
            Some(second),
            "the later request is the one now outstanding"
        );
        assert_eq!(
            store.pending().await.unwrap().certificate_type,
            Some(CertificateSigningUseEnumType::V2GCertificate)
        );
    }

    #[tokio::test]
    async fn correlate_accepted_clears_a_pending_csr_as_solicited() {
        let store = V201PendingCsrStore::new();
        let marker = store
            .record(Some(
                CertificateSigningUseEnumType::ChargingStationCertificate,
            ))
            .await;
        assert_eq!(
            store.correlate_accepted().await,
            CsrCorrelation::Solicited(marker),
            "an accepted delivery correlates to the outstanding request"
        );
        assert!(
            store.is_idle().await,
            "the provisioning loop is complete — the slot is cleared"
        );
    }

    #[tokio::test]
    async fn correlate_accepted_with_no_pending_csr_is_unsolicited() {
        let store = V201PendingCsrStore::new();
        assert_eq!(
            store.correlate_accepted().await,
            CsrCorrelation::Unsolicited,
            "a delivery with nothing outstanding is unsolicited"
        );
        assert!(store.is_idle().await);
    }

    #[tokio::test]
    async fn a_second_correlate_after_clearing_is_unsolicited() {
        // Only the first accepted delivery clears the marker; a redelivery (or a
        // duplicate) then correlates as unsolicited rather than double-clearing.
        let store = V201PendingCsrStore::new();
        store.record(None).await;
        assert!(matches!(
            store.correlate_accepted().await,
            CsrCorrelation::Solicited(_)
        ));
        assert_eq!(
            store.correlate_accepted().await,
            CsrCorrelation::Unsolicited
        );
    }

    #[tokio::test]
    async fn clear_returns_the_marker_and_is_a_noop_when_idle() {
        let store = V201PendingCsrStore::new();
        let marker = store.record(None).await;
        assert_eq!(
            store.clear().await,
            Some(marker),
            "clear yields the outstanding marker"
        );
        assert!(store.is_idle().await);
        assert_eq!(
            store.clear().await,
            None,
            "clearing an idle store is a no-op"
        );
    }
}
