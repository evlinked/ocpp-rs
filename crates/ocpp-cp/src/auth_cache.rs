//! Authorization ID-tag cache for offline authorization (OCPP 1.6J §3.1).
//!
//! A charge point caches the `IdTagInfo` results returned by the CSMS so it can
//! authorize an id tag locally when the CSMS is unreachable, and so repeated
//! authorizations of the same tag don't require a round-trip. This mirrors the
//! Authorization Cache concept in the OCPP 1.6J specification and the
//! cache-then-call behavior in the Python reference
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
//!
//! Entries expire either at the `expiryDate` carried in the cached `IdTagInfo`
//! (when the CSMS supplies one) or, failing that, after a configurable TTL
//! ([`crate::ChargePointConfig::auth_cache_ttl`]).

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use ocpp_types::common::IdTagInfo;
use std::time::Duration;

/// A cached authorization result together with the instant it becomes stale.
#[derive(Debug, Clone)]
struct CachedAuth {
    id_tag_info: IdTagInfo,
    /// Absolute time at which this entry is considered expired. Derived from
    /// `IdTagInfo.expiry_date` when present, otherwise `cached_at + ttl`.
    expires_at: DateTime<Utc>,
}

impl CachedAuth {
    fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

/// Thread-safe authorization cache keyed by id tag.
///
/// Cheap to share across tasks: [`DashMap`] provides interior mutability, so the
/// cache is used behind a plain `Arc<AuthCache>` rather than a lock.
#[derive(Debug)]
pub struct AuthCache {
    entries: DashMap<String, CachedAuth>,
    /// Fallback time-to-live applied when a cached `IdTagInfo` carries no
    /// `expiryDate`.
    ttl: Duration,
}

impl AuthCache {
    /// Create an empty cache whose entries live for `ttl` when the CSMS does not
    /// supply an explicit `expiryDate`.
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            ttl,
        }
    }

    /// Look up a *fresh* cached authorization result.
    ///
    /// Returns `None` if the tag is unknown or its entry has expired. This is a
    /// pure read: an expired entry is **not** evicted, so a subsequent
    /// [`AuthCache::get_stale`] can still honor it for offline authorization.
    /// Stale rows are cleaned up when the tag is refreshed (an `insert`
    /// overwrites), explicitly [`evict`](AuthCache::evict)ed, or
    /// [`clear`](AuthCache::clear)ed.
    pub fn get(&self, id_tag: &str) -> Option<IdTagInfo> {
        let now = Utc::now();
        let entry = self.entries.get(id_tag)?;
        if entry.is_expired_at(now) {
            None
        } else {
            Some(entry.id_tag_info.clone())
        }
    }

    /// Look up a cached authorization result *ignoring expiry*.
    ///
    /// Used for offline fallback: when the CSMS is unreachable and the operator
    /// has opted into [`crate::ChargePointConfig::offline_auth_stale_ok`], a
    /// stale-but-previously-accepted tag may still be honored.
    pub fn get_stale(&self, id_tag: &str) -> Option<IdTagInfo> {
        self.entries.get(id_tag).map(|e| e.id_tag_info.clone())
    }

    /// Store an authorization result.
    ///
    /// The entry expires at `id_tag_info.expiry_date` when set; otherwise it
    /// lives for the cache's configured TTL from now.
    pub fn insert(&self, id_tag: &str, id_tag_info: IdTagInfo) {
        let expires_at = id_tag_info.expiry_date.unwrap_or_else(|| {
            // For our bounded TTLs this is exact; for a pathologically large
            // configured TTL, saturate to `DateTime::MAX_UTC` instead of risking
            // an arithmetic-overflow panic from `now + duration`.
            chrono::Duration::from_std(self.ttl)
                .ok()
                .and_then(|d| Utc::now().checked_add_signed(d))
                .unwrap_or(DateTime::<Utc>::MAX_UTC)
        });
        self.entries.insert(
            id_tag.to_string(),
            CachedAuth {
                id_tag_info,
                expires_at,
            },
        );
    }

    /// Remove a single entry (e.g. after remote de-authorization).
    pub fn evict(&self, id_tag: &str) {
        self.entries.remove(id_tag);
    }

    /// Remove every entry (e.g. on a CSMS `ClearCache` command).
    pub fn clear(&self) {
        self.entries.clear();
    }

    /// Number of entries currently held (expired entries included until they are
    /// looked up or cleared). Primarily useful for tests and diagnostics.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::common::AuthorizationStatus;

    fn info(status: AuthorizationStatus, expiry: Option<DateTime<Utc>>) -> IdTagInfo {
        IdTagInfo {
            status,
            parent_id_tag: None,
            expiry_date: expiry,
        }
    }

    fn accepted() -> IdTagInfo {
        info(AuthorizationStatus::Accepted, None)
    }

    #[test]
    fn get_returns_inserted_entry() {
        let cache = AuthCache::new(Duration::from_secs(3600));
        cache.insert("TAG001", accepted());
        let got = cache.get("TAG001").expect("entry should be present");
        assert_eq!(got.status, AuthorizationStatus::Accepted);
    }

    #[test]
    fn get_returns_none_for_missing_tag() {
        let cache = AuthCache::new(Duration::from_secs(3600));
        assert!(cache.get("UNKNOWN").is_none());
    }

    #[test]
    fn respects_expiry_date_in_the_past() {
        let cache = AuthCache::new(Duration::from_secs(86_400));
        let past = Utc::now() - chrono::Duration::seconds(10);
        cache.insert("TAG001", info(AuthorizationStatus::Accepted, Some(past)));
        // Expired by its own expiryDate even though the TTL would keep it alive.
        assert!(cache.get("TAG001").is_none());
        // get() is a pure read — the row survives for a possible stale lookup.
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn honors_future_expiry_date() {
        let cache = AuthCache::new(Duration::from_secs(1));
        let future = Utc::now() + chrono::Duration::seconds(3600);
        cache.insert("TAG001", info(AuthorizationStatus::Accepted, Some(future)));
        // expiryDate (1h) overrides the short 1s TTL.
        assert!(cache.get("TAG001").is_some());
    }

    #[test]
    fn ttl_fallback_expires_entry_without_expiry_date() {
        // Zero TTL → entry is immediately stale when no expiryDate is supplied.
        let cache = AuthCache::new(Duration::from_secs(0));
        cache.insert("TAG001", accepted());
        assert!(cache.get("TAG001").is_none());
    }

    #[test]
    fn ttl_fallback_keeps_fresh_entry() {
        let cache = AuthCache::new(Duration::from_secs(3600));
        cache.insert("TAG001", accepted());
        assert!(cache.get("TAG001").is_some());
    }

    #[test]
    fn get_stale_returns_expired_entry() {
        let cache = AuthCache::new(Duration::from_secs(86_400));
        let past = Utc::now() - chrono::Duration::seconds(10);
        cache.insert("TAG001", info(AuthorizationStatus::Accepted, Some(past)));
        // get() rejects the expired entry, but get_stale still returns it.
        assert!(cache.get("TAG001").is_none());
        let stale = cache.get_stale("TAG001").expect("stale lookup should hit");
        assert_eq!(stale.status, AuthorizationStatus::Accepted);
    }

    #[test]
    fn evict_removes_single_entry() {
        let cache = AuthCache::new(Duration::from_secs(3600));
        cache.insert("TAG001", accepted());
        cache.insert("TAG002", accepted());
        cache.evict("TAG001");
        assert!(cache.get("TAG001").is_none());
        assert!(cache.get("TAG002").is_some());
    }

    #[test]
    fn clear_empties_all_entries() {
        let cache = AuthCache::new(Duration::from_secs(3600));
        cache.insert("TAG001", accepted());
        cache.insert("TAG002", accepted());
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }
}
