//! Pending OCPP call correlation map.
//!
//! Tracks in-flight CALLs and resolves or rejects them when the matching
//! CALLRESULT or CALLERROR arrives — mirroring `_pending_futures` in the
//! Python reference (`ocpp/charge_point.py`, `call()`,
//! `_handle_call_result()`, `_handle_call_error()`).

use dashmap::DashMap;
use ocpp_types::{OcppError, OcppResult};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::oneshot;

type PendingTx = oneshot::Sender<OcppResult<Value>>;

/// Thread-safe map of `unique_id` → response channel for in-flight OCPP CALLs.
///
/// Wrap in `Arc` and share between the outgoing-call path and the
/// incoming WebSocket receive loop.
pub struct PendingCallMap {
    inner: DashMap<String, PendingTx>,
}

impl PendingCallMap {
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Register a new in-flight call; returns the receiver half.
    ///
    /// **Must be called before sending the CALL frame** to eliminate the race
    /// where a CALLRESULT arrives before the entry is registered.
    pub fn register(&self, unique_id: String) -> oneshot::Receiver<OcppResult<Value>> {
        let (tx, rx) = oneshot::channel();
        self.inner.insert(unique_id, tx);
        rx
    }

    /// Register a new in-flight call and return the receiver **plus an RAII
    /// guard** that prunes the entry from the map when dropped.
    ///
    /// Use this on the client `call()` path so a CALL that times out — or bails
    /// on a transport error, or whose future is cancelled — does not leave a
    /// stale `oneshot::Sender` behind (Issue #323). On the happy path the recv
    /// loop's [`resolve`](Self::resolve)/[`reject`](Self::reject) has already
    /// removed the entry, so the guard's drop-time removal is a harmless no-op.
    ///
    /// **Must be called before sending the CALL frame**, exactly like
    /// [`register`](Self::register), to eliminate the register-after-response
    /// race.
    pub fn register_guarded(
        self: &Arc<Self>,
        unique_id: String,
    ) -> (oneshot::Receiver<OcppResult<Value>>, PendingGuard) {
        let rx = self.register(unique_id.clone());
        let guard = PendingGuard {
            map: Arc::clone(self),
            unique_id,
        };
        (rx, guard)
    }

    /// Remove a pending entry without resolving or rejecting it.
    ///
    /// Returns `true` if an entry was present. Used by [`PendingGuard`] to prune
    /// a call that will never be answered; a later `resolve`/`reject` for the
    /// same id then returns `false` (an unknown-id no-op).
    pub fn remove(&self, unique_id: &str) -> bool {
        self.inner.remove(unique_id).is_some()
    }

    /// Resolve a pending call with a successful CALLRESULT payload.
    ///
    /// Returns `true` if the `unique_id` was found and resolved; `false`
    /// if already timed out, cancelled, or unknown.
    pub fn resolve(&self, unique_id: &str, payload: Value) -> bool {
        if let Some((_, tx)) = self.inner.remove(unique_id) {
            tx.send(Ok(payload)).is_ok()
        } else {
            false
        }
    }

    /// Reject a pending call with an error from a CALLERROR frame.
    ///
    /// Returns `true` if the `unique_id` was found; `false` if unknown.
    pub fn reject(&self, unique_id: &str, error: OcppError) -> bool {
        if let Some((_, tx)) = self.inner.remove(unique_id) {
            tx.send(Err(error)).is_ok()
        } else {
            false
        }
    }

    /// Drop all pending senders so each receiver sees a `RecvError`.
    ///
    /// Call on disconnect so in-flight `ChargePoint::call()` futures
    /// surface as `OcppError::Transport` instead of hanging until timeout.
    pub fn cancel_all(&self) {
        self.inner.clear();
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for PendingCallMap {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard returned by [`PendingCallMap::register_guarded`].
///
/// On drop it removes its `unique_id` from the map, guaranteeing that a CALL
/// which is never answered (timeout), errors out before awaiting, or whose
/// future is cancelled leaves no residue in the [`PendingCallMap`]. Hold it for
/// as long as the call is in flight — typically the whole body of `call()` —
/// and let it drop naturally when the call returns.
///
/// If the recv loop already resolved/rejected the entry, the drop-time removal
/// finds nothing and is a no-op, so the guard is safe to hold unconditionally.
pub struct PendingGuard {
    map: Arc<PendingCallMap>,
    unique_id: String,
}

impl PendingGuard {
    /// The `unique_id` this guard will prune on drop.
    pub fn unique_id(&self) -> &str {
        &self.unique_id
    }
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.map.remove(&self.unique_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn resolve_happy_path() {
        let map = PendingCallMap::new();
        let rx = map.register("id-1".to_string());

        let payload = json!({"status": "Accepted"});
        assert!(map.resolve("id-1", payload.clone()));

        let result = rx.await.expect("channel open").unwrap();
        assert_eq!(result, payload);
    }

    #[tokio::test]
    async fn reject_delivers_error() {
        let map = PendingCallMap::new();
        let rx = map.register("id-2".to_string());

        let err = OcppError::NotSupported {
            feature: "TestAction".to_string(),
        };
        assert!(map.reject("id-2", err.clone()));

        let result = rx.await.expect("channel open");
        assert_eq!(result.unwrap_err(), err);
    }

    #[tokio::test]
    async fn resolve_unknown_returns_false() {
        let map = PendingCallMap::new();
        assert!(!map.resolve("ghost", json!({})));
    }

    #[tokio::test]
    async fn reject_unknown_returns_false() {
        let map = PendingCallMap::new();
        assert!(!map.reject(
            "ghost",
            OcppError::NotSupported {
                feature: "x".to_string(),
            }
        ));
    }

    #[tokio::test]
    async fn cancel_all_drops_senders() {
        let map = PendingCallMap::new();
        let rx1 = map.register("a".to_string());
        let rx2 = map.register("b".to_string());

        assert_eq!(map.len(), 2);
        map.cancel_all();
        assert!(map.is_empty());

        assert!(rx1.await.is_err(), "sender dropped → RecvError");
        assert!(rx2.await.is_err(), "sender dropped → RecvError");
    }

    #[tokio::test]
    async fn resolve_only_first_succeeds() {
        let map = PendingCallMap::new();
        let _rx = map.register("dup".to_string());

        assert!(map.resolve("dup", json!(1)));
        assert!(
            !map.resolve("dup", json!(2)),
            "entry already removed on first resolve"
        );
    }

    #[tokio::test]
    async fn guard_prunes_entry_on_drop() {
        // Models the timeout path: a guarded registration whose reply never
        // arrives. Dropping the guard (as `call()` does on return) must remove
        // the entry so the map returns to its pre-call size (Issue #323).
        let map = Arc::new(PendingCallMap::new());
        assert_eq!(map.len(), 0);

        let (_rx, guard) = map.register_guarded("timeout-id".to_string());
        assert_eq!(map.len(), 1, "entry present while call is in flight");

        drop(guard);
        assert_eq!(map.len(), 0, "timed-out call must leave no residue");
    }

    #[tokio::test]
    async fn repeated_guard_drops_do_not_grow_map() {
        // No-reply timeouts in a loop must not accumulate entries.
        let map = Arc::new(PendingCallMap::new());
        for i in 0..100 {
            let (_rx, guard) = map.register_guarded(format!("call-{i}"));
            assert_eq!(map.len(), 1);
            drop(guard);
            assert_eq!(map.len(), 0);
        }
        assert!(map.is_empty(), "map returns to empty after every timeout");
    }

    #[tokio::test]
    async fn resolve_before_guard_drop_is_the_common_case() {
        // Happy path: the recv loop resolves the call, then the guard drops.
        // The resolve removed the entry, so the guard's drop is a no-op and the
        // delivered value is unaffected.
        let map = Arc::new(PendingCallMap::new());
        let (rx, guard) = map.register_guarded("resolved-id".to_string());

        assert!(map.resolve("resolved-id", json!({"status": "Accepted"})));
        assert_eq!(map.len(), 0, "resolve removed the entry");

        drop(guard); // no-op: entry already gone
        assert_eq!(map.len(), 0);

        let value = rx.await.expect("channel open").unwrap();
        assert_eq!(value, json!({"status": "Accepted"}));
    }

    #[tokio::test]
    async fn late_reply_after_guard_prune_is_a_harmless_noop() {
        // A reply that arrives *after* the guard pruned the entry (peer replied
        // just after we timed out) resolves nothing and returns false.
        let map = Arc::new(PendingCallMap::new());
        let (_rx, guard) = map.register_guarded("late-id".to_string());
        drop(guard);

        assert!(
            !map.resolve("late-id", json!(1)),
            "resolve after prune must be an unknown-id no-op"
        );
        assert!(!map.reject(
            "late-id",
            OcppError::NotSupported {
                feature: "x".to_string(),
            }
        ));
    }

    #[tokio::test]
    async fn remove_returns_presence() {
        let map = PendingCallMap::new();
        let _rx = map.register("id".to_string());
        assert!(map.remove("id"), "present entry removed");
        assert!(!map.remove("id"), "already gone → false");
        assert!(!map.remove("never"), "unknown id → false");
    }

    #[tokio::test]
    async fn concurrent_registrations() {
        use std::sync::Arc;

        let map = Arc::new(PendingCallMap::new());
        let handles: Vec<_> = (0u32..10)
            .map(|i| {
                let map = map.clone();
                tokio::spawn(async move {
                    let id = format!("call-{i}");
                    let rx = map.register(id.clone());
                    assert!(map.resolve(&id, json!(i)));
                    let val = rx.await.expect("channel open").expect("ok result");
                    assert_eq!(val, json!(i));
                })
            })
            .collect();

        for handle in handles {
            handle.await.unwrap();
        }
        assert!(map.is_empty());
    }
}
