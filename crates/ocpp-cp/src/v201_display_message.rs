//! v201 display-message store — the messages a CSMS installs on the Charging
//! Station's display via `SetDisplayMessage` (OCPP 2.0.1 Part 2, E05–E08).
//!
//! `SetDisplayMessage` is how a CSMS pushes a message for the station to show on
//! its display (a tariff, a "charging paused" notice, a promotional line). Each
//! message is a [`MessageInfoType`] carrying a **station-unique `id`**
//! (`ocpp_types::v201::MessageInfoType::id`); this store keys installed messages
//! by that id, so:
//!
//! - a re-`SetDisplayMessage` with the same id **replaces** the message (upsert,
//!   not a duplicate) — the "master resource identifier" semantics the spec gives
//!   `MessageInfoType.id`;
//! - a later `ClearDisplayMessage` removes a message **by id**
//!   ([`remove`](V201DisplayMessageStore::remove)); and
//! - a `GetDisplayMessages` query reads the installed set
//!   ([`snapshot`](V201DisplayMessageStore::snapshot)).
//!
//! This is the **foundational** slice of the display-message family (Issue #505):
//! the store it introduces is the single source of truth the `GetDisplayMessages`
//! and `ClearDisplayMessage` handlers will read/mutate, so it unblocks those two
//! follow-ups — mirroring how the variable-monitoring family landed its store
//! (#494) before the report/clear handlers that read it.
//!
//! Interior-mutable behind an [`RwLock`] so a single `Arc<V201DisplayMessageStore>`
//! can be shared across the charge point's tasks, exactly like the v201 `TxProfile`
//! store ([`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore)).
//! Deciding the [`DisplayMessageStatusEnumType`](ocpp_types::v201::DisplayMessageStatusEnumType)
//! a `SetDisplayMessage` answers is
//! deliberately *not* this store's job — that pure decision lives in
//! [`v201_set_display_message_status`](crate::v201_command::v201_set_display_message_status),
//! and the handler installs here only once it has decided `Accepted`.

use std::collections::HashMap;

use ocpp_types::v201::MessageInfoType;
use tokio::sync::RwLock;

/// A store of display messages installed by `SetDisplayMessage`, keyed by
/// [`MessageInfoType::id`].
///
/// The id is the message's "master resource identifier" (station-unique), so it
/// is the natural key: an install with an id already present replaces it (upsert),
/// a `ClearDisplayMessage` removes by it, and a `GetDisplayMessages` enumerates
/// the values. Populated by the `SetDisplayMessage` handler (only once its pure
/// decision returns `Accepted`) and drained by a future `ClearDisplayMessage`.
#[derive(Debug, Default)]
pub struct V201DisplayMessageStore {
    /// `MessageInfoType.id` → the display message currently installed under it.
    by_id: RwLock<HashMap<i32, MessageInfoType>>,
}

impl V201DisplayMessageStore {
    /// A new, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install `message`, keyed by its [`id`](MessageInfoType::id), replacing any
    /// message already installed under that id.
    ///
    /// Returns the message that was displaced, if any — so a caller (and a test)
    /// can tell a fresh install (`None`) from an upsert (`Some(previous)`). The id
    /// is the spec's master resource identifier, so a same-id re-install is a
    /// deliberate replace, never a second copy.
    pub async fn install(&self, message: MessageInfoType) -> Option<MessageInfoType> {
        self.by_id.write().await.insert(message.id, message)
    }

    /// The message currently installed under `id`, if any.
    ///
    /// A cloned snapshot: the caller inspects it without holding the store lock.
    pub async fn get(&self, id: i32) -> Option<MessageInfoType> {
        self.by_id.read().await.get(&id).cloned()
    }

    /// Remove the message installed under `id`, returning it if one was present.
    ///
    /// The read a future `ClearDisplayMessage` needs. Idempotent — removing an id
    /// that holds no message is a no-op returning `None` (and cannot panic on any
    /// wire id: an unknown, negative, or `i32::MIN`/`MAX` id simply fails to
    /// match).
    pub async fn remove(&self, id: i32) -> Option<MessageInfoType> {
        self.by_id.write().await.remove(&id)
    }

    /// A cloned snapshot of every installed message.
    ///
    /// The read a future `GetDisplayMessages` needs: its selector is resolved
    /// against the store's current contents by a pure decision. Returning an owned
    /// snapshot lets that matching run without holding the store lock. The order is
    /// unspecified (a `HashMap` walk); callers key off
    /// [`id`](MessageInfoType::id), never position.
    pub async fn snapshot(&self) -> Vec<MessageInfoType> {
        self.by_id.read().await.values().cloned().collect()
    }

    /// How many messages are currently installed. A cheap read used by tests and a
    /// future `GetDisplayMessages` "how many matched" answer.
    pub async fn len(&self) -> usize {
        self.by_id.read().await.len()
    }

    /// Whether the store currently holds no messages.
    pub async fn is_empty(&self) -> bool {
        self.by_id.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        MessageContentType, MessageFormatEnumType, MessagePriorityEnumType, MessageStateEnumType,
    };

    /// A minimal schema-shaped `MessageInfoType` with the given `id`, so tests can
    /// tell two installed messages apart. `content` is tagged with the id too, so a
    /// replace is observable in the stored body, not just the key.
    fn message(id: i32) -> MessageInfoType {
        MessageInfoType {
            id,
            priority: MessagePriorityEnumType::NormalCycle,
            message: MessageContentType {
                format: MessageFormatEnumType::Utf8,
                content: format!("message-{id}"),
                language: None,
                custom_data: None,
            },
            state: None,
            start_date_time: None,
            end_date_time: None,
            transaction_id: None,
            display: None,
            custom_data: None,
        }
    }

    #[tokio::test]
    async fn get_returns_none_for_an_unknown_id() {
        let store = V201DisplayMessageStore::new();
        assert_eq!(store.get(1).await, None);
        assert!(store.is_empty().await);
    }

    #[tokio::test]
    async fn install_then_get_round_trips_the_message() {
        let store = V201DisplayMessageStore::new();
        let msg = message(7);
        assert_eq!(
            store.install(msg.clone()).await,
            None,
            "a fresh install displaces nothing"
        );
        assert_eq!(store.get(7).await, Some(msg));
        // Scoped to its id — a sibling id is unaffected.
        assert_eq!(store.get(8).await, None);
        assert_eq!(store.len().await, 1);
    }

    #[tokio::test]
    async fn reinstalling_the_same_id_replaces_the_previous_message() {
        let store = V201DisplayMessageStore::new();
        store.install(message(1)).await;

        // A second message under a different content but the SAME id.
        let mut replacement = message(1);
        replacement.message.content = "replaced".to_string();
        replacement.priority = MessagePriorityEnumType::AlwaysFront;

        assert_eq!(
            store.install(replacement.clone()).await,
            Some(message(1)),
            "re-installing an id returns the message it displaced"
        );
        assert_eq!(
            store.get(1).await,
            Some(replacement),
            "the same id upserts — the last install wins, no duplicate"
        );
        assert_eq!(store.len().await, 1, "an upsert does not grow the store");
    }

    #[tokio::test]
    async fn remove_deletes_and_returns_the_message_then_is_a_noop() {
        let store = V201DisplayMessageStore::new();
        let msg = message(9);
        store.install(msg.clone()).await;
        assert_eq!(
            store.remove(9).await,
            Some(msg),
            "remove returns what it deleted"
        );
        assert_eq!(store.get(9).await, None, "the message is gone after remove");
        assert_eq!(
            store.remove(9).await,
            None,
            "removing an absent id is a no-op"
        );
    }

    #[tokio::test]
    async fn remove_tolerates_any_wire_id_without_panicking() {
        // A `ClearDisplayMessage` id is CSMS-supplied: an unknown or extreme id
        // must simply miss, never panic.
        let store = V201DisplayMessageStore::new();
        assert_eq!(store.remove(i32::MIN).await, None);
        assert_eq!(store.remove(i32::MAX).await, None);
        assert_eq!(store.remove(-1).await, None);
    }

    #[tokio::test]
    async fn snapshot_reflects_every_installed_message_by_id() {
        let store = V201DisplayMessageStore::new();
        assert!(
            store.snapshot().await.is_empty(),
            "an empty store snapshots to nothing"
        );

        let m1 = message(10);
        let m2 = message(20);
        store.install(m1.clone()).await;
        store.install(m2.clone()).await;

        // Order is a HashMap walk, so sort by id before comparing.
        let mut snap = store.snapshot().await;
        snap.sort_by_key(|m| m.id);
        assert_eq!(snap, vec![m1, m2]);

        // The snapshot is a detached clone: removing from the store afterward does
        // not mutate an already-taken snapshot.
        let taken = store.snapshot().await;
        store.remove(10).await;
        assert_eq!(
            taken.len(),
            2,
            "a taken snapshot is independent of the store"
        );
    }

    #[tokio::test]
    async fn a_message_bound_to_a_transaction_round_trips_its_transaction_id() {
        // A `transactionId`-scoped message stores like any other; whether to accept
        // it is the pure decision's job (v201_set_display_message_status), not the
        // store's.
        let store = V201DisplayMessageStore::new();
        let mut msg = message(3);
        msg.transaction_id = Some("txn-1".to_string());
        msg.state = Some(MessageStateEnumType::Charging);
        store.install(msg.clone()).await;
        assert_eq!(store.get(3).await, Some(msg));
    }
}
