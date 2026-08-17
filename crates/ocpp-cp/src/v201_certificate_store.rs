//! v201 trust-anchor store — the root/CA certificates a CSMS installs into the
//! Charging Station via `InstallCertificate` (OCPP 2.0.1 Part 2, A02 / M03–M05).
//!
//! `InstallCertificate` is the **write** side of the certificate-*management*
//! family (`InstallCertificate` / `DeleteCertificate` / `GetInstalledCertificateIds`),
//! which manages the *root/trust* anchors a station uses to validate TLS peers
//! and signed material — distinct from the certificate-*provisioning* pair
//! (`SignCertificate` / `CertificateSigned`), which gets the station's *own*
//! certificate signed.
//!
//! Each install carries an [`InstallCertificateUseEnumType`] selecting which trust
//! anchor it is (V2G / MO / CSMS / manufacturer root); this store keys installed
//! PEMs by that use, so:
//!
//! - a re-`InstallCertificate` under the same use **replaces** the anchor
//!   (upsert, not a second copy) — an operator rotating a root overwrites it in
//!   place; and
//! - a different use installs independently, never disturbing another anchor.
//!
//! This is the **foundational** slice of the certificate-management family
//! (Issue #518): the store it introduces is the single source of truth the
//! follow-up `GetInstalledCertificateIds` (enumerate) and `DeleteCertificate`
//! (remove) handlers will read/mutate, so it unblocks those two — mirroring how
//! the display-message family landed its [`V201DisplayMessageStore`] before the
//! `GetDisplayMessages` / `ClearDisplayMessage` handlers that read it.
//!
//! Interior-mutable behind an [`RwLock`] so a single `Arc<V201CertificateStore>`
//! can be shared across the charge point's tasks, exactly like the sibling v201
//! stores. Deciding the
//! [`InstallCertificateStatusEnumType`](ocpp_types::v201::InstallCertificateStatusEnumType)
//! an `InstallCertificate` answers is deliberately *not* this store's job — that
//! pure decision lives in
//! [`v201_install_certificate_status`](crate::v201_command::v201_install_certificate_status),
//! and the handler installs here only once it has decided `Accepted`.
//!
//! [`V201DisplayMessageStore`]: crate::v201_display_message::V201DisplayMessageStore

use std::collections::HashMap;

use ocpp_types::v201::InstallCertificateUseEnumType;
use tokio::sync::RwLock;

/// A store of root/CA certificates installed by `InstallCertificate`, keyed by the
/// [`InstallCertificateUseEnumType`] trust anchor each serves.
///
/// The use is the natural key: at most one certificate is held per anchor (V2G /
/// MO / CSMS / manufacturer root), so an install under a use already present
/// replaces it (upsert, never a duplicate), a future `DeleteCertificate` removes
/// by it, and a future `GetInstalledCertificateIds` enumerates the anchors. The
/// stored value is the opaque PEM string as delivered — this store neither parses
/// nor validates it (that boundary is the handler's; the simulator does no X.509
/// parse). Populated by the `InstallCertificate` handler only once its pure
/// decision returns `Accepted`.
#[derive(Debug, Default)]
pub struct V201CertificateStore {
    /// trust anchor → the PEM currently installed for it.
    by_use: RwLock<HashMap<InstallCertificateUseEnumType, String>>,
}

impl V201CertificateStore {
    /// A new, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install `certificate` (a PEM string) under the trust anchor `use_`,
    /// replacing any certificate already installed for that anchor.
    ///
    /// Returns the PEM that was displaced, if any — so a caller (and a test) can
    /// tell a fresh install (`None`) from a rotation (`Some(previous)`). There is
    /// at most one certificate per anchor, so a same-use re-install is a
    /// deliberate replace, never a second copy. The store is bounded to the four
    /// [`InstallCertificateUseEnumType`] variants, so it cannot grow without
    /// bound.
    pub async fn install(
        &self,
        use_: InstallCertificateUseEnumType,
        certificate: String,
    ) -> Option<String> {
        self.by_use.write().await.insert(use_, certificate)
    }

    /// The PEM currently installed for the trust anchor `use_`, if any.
    ///
    /// A cloned snapshot: the caller inspects it without holding the store lock.
    /// The read a future `GetInstalledCertificateIds` needs, and the path that
    /// makes an accepted install observable in tests.
    pub async fn installed(&self, use_: InstallCertificateUseEnumType) -> Option<String> {
        self.by_use.read().await.get(&use_).cloned()
    }

    /// Remove the certificate installed for `use_`, returning it if one was
    /// present.
    ///
    /// The read a future `DeleteCertificate` needs. Idempotent — removing an
    /// anchor that holds no certificate is a no-op returning `None`.
    pub async fn remove(&self, use_: InstallCertificateUseEnumType) -> Option<String> {
        self.by_use.write().await.remove(&use_)
    }

    /// How many trust anchors currently hold a certificate. A cheap read used by
    /// tests and a future `GetInstalledCertificateIds` "how many installed" answer.
    pub async fn len(&self) -> usize {
        self.by_use.read().await.len()
    }

    /// Whether the store currently holds no certificates.
    pub async fn is_empty(&self) -> bool {
        self.by_use.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSMS_ROOT: InstallCertificateUseEnumType =
        InstallCertificateUseEnumType::CSMSRootCertificate;
    const V2G_ROOT: InstallCertificateUseEnumType =
        InstallCertificateUseEnumType::V2GRootCertificate;

    #[tokio::test]
    async fn installed_returns_none_for_an_uninstalled_anchor() {
        let store = V201CertificateStore::new();
        assert_eq!(store.installed(CSMS_ROOT).await, None);
        assert!(store.is_empty().await);
    }

    #[tokio::test]
    async fn install_then_read_round_trips_the_pem() {
        let store = V201CertificateStore::new();
        assert_eq!(
            store.install(CSMS_ROOT, "pem-csms".to_string()).await,
            None,
            "a fresh install displaces nothing"
        );
        assert_eq!(
            store.installed(CSMS_ROOT).await,
            Some("pem-csms".to_string())
        );
        // Scoped to its anchor — a sibling use is unaffected.
        assert_eq!(store.installed(V2G_ROOT).await, None);
        assert_eq!(store.len().await, 1);
    }

    #[tokio::test]
    async fn a_second_anchor_installs_independently() {
        let store = V201CertificateStore::new();
        store.install(CSMS_ROOT, "pem-csms".to_string()).await;
        store.install(V2G_ROOT, "pem-v2g".to_string()).await;

        // Both anchors hold their own PEM; neither disturbed the other.
        assert_eq!(
            store.installed(CSMS_ROOT).await,
            Some("pem-csms".to_string())
        );
        assert_eq!(store.installed(V2G_ROOT).await, Some("pem-v2g".to_string()));
        assert_eq!(store.len().await, 2);
    }

    #[tokio::test]
    async fn reinstalling_the_same_anchor_replaces_the_previous_pem() {
        let store = V201CertificateStore::new();
        store.install(CSMS_ROOT, "pem-old".to_string()).await;

        assert_eq!(
            store.install(CSMS_ROOT, "pem-new".to_string()).await,
            Some("pem-old".to_string()),
            "rotating an anchor returns the PEM it displaced"
        );
        assert_eq!(
            store.installed(CSMS_ROOT).await,
            Some("pem-new".to_string()),
            "the same anchor upserts — the last install wins, no duplicate"
        );
        assert_eq!(store.len().await, 1, "a rotation does not grow the store");
    }

    #[tokio::test]
    async fn remove_deletes_and_returns_the_pem_then_is_a_noop() {
        let store = V201CertificateStore::new();
        store.install(V2G_ROOT, "pem-v2g".to_string()).await;
        assert_eq!(
            store.remove(V2G_ROOT).await,
            Some("pem-v2g".to_string()),
            "remove returns what it deleted"
        );
        assert_eq!(
            store.installed(V2G_ROOT).await,
            None,
            "the anchor is empty after remove"
        );
        assert_eq!(
            store.remove(V2G_ROOT).await,
            None,
            "removing an absent anchor is a no-op"
        );
    }
}
