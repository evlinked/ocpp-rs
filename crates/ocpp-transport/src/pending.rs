//! Pending OCPP call correlation map.
//!
//! Tracks in-flight CALLs and resolves or rejects them when the matching
//! CALLRESULT or CALLERROR arrives — mirroring `_pending_futures` in the
//! Python reference (`ocpp/charge_point.py`, `call()`,
//! `_handle_call_result()`, `_handle_call_error()`).

use dashmap::DashMap;
use ocpp_types::{OcppError, OcppResult};
use serde_json::Value;
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
