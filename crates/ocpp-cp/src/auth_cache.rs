use chrono::Utc;
use ocpp_types::common::IdTagInfo;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

struct CachedAuth {
    id_tag_info: IdTagInfo,
    cached_at: chrono::DateTime<Utc>,
    ttl: Duration,
}

impl CachedAuth {
    fn is_expired(&self) -> bool {
        if let Some(expiry) = self.id_tag_info.expiry_date {
            return Utc::now() >= expiry;
        }
        match chrono::Duration::from_std(self.ttl) {
            Ok(ttl) => Utc::now().signed_duration_since(self.cached_at) >= ttl,
            Err(_) => false,
        }
    }
}

/// Thread-safe authorization result cache with per-entry expiry.
///
/// Ports the local authorization list checking from `ocpp/charge_point.py`.
/// Entries expire at `IdTagInfo.expiry_date` when set; otherwise they expire
/// `default_ttl` after insertion. On cache miss the caller should send an
/// `AuthorizeRequest` CALL and store the result via `insert`.
pub struct AuthCache {
    entries: Arc<RwLock<HashMap<String, CachedAuth>>>,
    default_ttl: Duration,
}

impl AuthCache {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Look up a cached auth result. Returns `None` if not found or expired.
    /// Expired entries are evicted on access.
    pub async fn get(&self, id_tag: &str) -> Option<IdTagInfo> {
        let mut entries = self.entries.write().await;
        match entries.get(id_tag) {
            Some(cached) if cached.is_expired() => {
                entries.remove(id_tag);
                None
            }
            Some(cached) => Some(cached.id_tag_info.clone()),
            None => None,
        }
    }

    /// Look up a cached auth result, returning even expired entries.
    /// Used for offline fallback (`offline_auth_stale_ok = true`) when the
    /// CSMS is unreachable and a fresh authorize CALL timed out.
    pub async fn get_stale(&self, id_tag: &str) -> Option<IdTagInfo> {
        self.entries
            .read()
            .await
            .get(id_tag)
            .map(|c| c.id_tag_info.clone())
    }

    /// Store an auth result. Expiry uses `IdTagInfo.expiry_date` when set,
    /// otherwise falls back to `default_ttl`.
    pub async fn insert(&self, id_tag: &str, id_tag_info: IdTagInfo) {
        self.entries.write().await.insert(
            id_tag.to_string(),
            CachedAuth {
                id_tag_info,
                cached_at: Utc::now(),
                ttl: self.default_ttl,
            },
        );
    }

    /// Evict a specific entry (e.g. after remote de-authorization).
    pub async fn evict(&self, id_tag: &str) {
        self.entries.write().await.remove(id_tag);
    }

    /// Clear all entries (called by the `ClearCache` CSMS command).
    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }

    /// Count of live (non-expired) entries. Runs in O(n).
    pub async fn len(&self) -> usize {
        self.entries
            .read()
            .await
            .values()
            .filter(|e| !e.is_expired())
            .count()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use ocpp_types::common::AuthorizationStatus;

    fn accepted_info() -> IdTagInfo {
        IdTagInfo {
            status: AuthorizationStatus::Accepted,
            parent_id_tag: None,
            expiry_date: None,
        }
    }

    fn expired_info() -> IdTagInfo {
        IdTagInfo {
            status: AuthorizationStatus::Accepted,
            parent_id_tag: None,
            // one second in the past → already expired
            expiry_date: Some(Utc::now() - ChronoDuration::seconds(1)),
        }
    }

    #[tokio::test]
    async fn auth_cache_hit_returns_cached_result() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("TAG001", accepted_info()).await;
        let result = cache.get("TAG001").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, AuthorizationStatus::Accepted);
    }

    #[tokio::test]
    async fn auth_cache_miss_returns_none() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        assert!(cache.get("UNKNOWN").await.is_none());
    }

    #[tokio::test]
    async fn auth_cache_expired_expiry_date_returns_none() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("TAG002", expired_info()).await;
        assert!(cache.get("TAG002").await.is_none());
    }

    #[tokio::test]
    async fn auth_cache_expired_entry_evicted_on_access() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("TAG003", expired_info()).await;
        // First access evicts
        let _ = cache.get("TAG003").await;
        // Second access: entry is gone
        assert!(cache.get("TAG003").await.is_none());
    }

    #[tokio::test]
    async fn auth_cache_clear_empties_all_entries() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("A", accepted_info()).await;
        cache.insert("B", accepted_info()).await;
        cache.clear().await;
        assert!(cache.get("A").await.is_none());
        assert!(cache.get("B").await.is_none());
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn auth_cache_evict_removes_single_entry() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("KEEP", accepted_info()).await;
        cache.insert("DROP", accepted_info()).await;
        cache.evict("DROP").await;
        assert!(cache.get("KEEP").await.is_some());
        assert!(cache.get("DROP").await.is_none());
    }

    #[tokio::test]
    async fn auth_cache_get_stale_returns_expired_entry() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("TAG004", expired_info()).await;
        // get() would return None, but get_stale() returns it
        assert!(cache.get_stale("TAG004").await.is_some());
    }

    #[tokio::test]
    async fn auth_cache_get_stale_returns_none_on_miss() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        assert!(cache.get_stale("MISSING").await.is_none());
    }

    #[tokio::test]
    async fn auth_cache_len_counts_live_entries_only() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("LIVE1", accepted_info()).await;
        cache.insert("LIVE2", accepted_info()).await;
        cache.insert("DEAD", expired_info()).await;
        assert_eq!(cache.len().await, 2);
    }

    #[tokio::test]
    async fn auth_cache_insert_overwrites_existing() {
        let cache = AuthCache::new(Duration::from_secs(86400));
        cache.insert("TAG005", accepted_info()).await;
        let blocked = IdTagInfo {
            status: AuthorizationStatus::Blocked,
            parent_id_tag: None,
            expiry_date: None,
        };
        cache.insert("TAG005", blocked).await;
        let result = cache.get("TAG005").await.unwrap();
        assert_eq!(result.status, AuthorizationStatus::Blocked);
    }

    #[tokio::test]
    async fn auth_cache_concurrent_access_is_safe() {
        let cache = Arc::new(AuthCache::new(Duration::from_secs(86400)));
        let mut handles = vec![];
        for i in 0..10u32 {
            let c = Arc::clone(&cache);
            handles.push(tokio::spawn(async move {
                let tag = format!("TAG{:03}", i);
                c.insert(&tag, accepted_info()).await;
                assert!(c.get(&tag).await.is_some());
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(cache.len().await, 10);
    }
}
