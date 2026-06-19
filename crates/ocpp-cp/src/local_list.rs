//! Local Authorization List management for the charge point (OCPP 1.6J §5.x).
//!
//! The Local Authorization List is a **CSMS-managed, versioned** set of id tags
//! (each with an [`IdTagInfo`]) that the charge point may authorize *offline*,
//! without a round-trip to the CSMS. Unlike the Authorization Cache
//! ([`crate::auth_cache`]), which the CP populates opportunistically from
//! `Authorize`/`StartTransaction` results, this list is pushed wholesale by the
//! CSMS via `SendLocalList` and queried with `GetLocalListVersion`, so the CSMS
//! can tell whether the CP is up to date.
//!
//! Ports the `GetLocalListVersion` / `SendLocalList` messages from the Python
//! reference
//! ([`ocpp/v16/call.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/call.py),
//! [`ocpp/v16/call_result.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/call_result.py))
//! with semantics from the OCPP 1.6J specification (§5.x). The Python library is
//! transport/routing only and leaves the list logic to the application, so the
//! `Full`/`Differential` and version-ordering rules below are taken from the
//! spec.
//!
//! This module owns **list management only**. Whether [`crate::ChargePoint`]'s
//! `authorize()` consults the list for offline authorization is tracked
//! separately (see the PR for issue #93); the list is exposed via
//! [`LocalAuthList::get`] so that integration can read it.

use std::collections::HashMap;
use std::sync::Mutex;

use ocpp_messages::v16j::SendLocalListRequest;
use ocpp_types::common::IdTagInfo;
use ocpp_types::v16j::{UpdateStatus, UpdateType};

/// The mutable state behind the list: a version number and the id-tag entries.
///
/// Held together under one lock so a `SendLocalList` update mutates the version
/// and the entries atomically — a reader can never observe a bumped version with
/// stale entries (or vice versa).
#[derive(Debug, Default)]
struct Inner {
    /// Version of the current list. `0` means the list is empty (and the feature
    /// is supported); the CP never reports `-1` ("not supported") because it
    /// implements the profile.
    version: i32,
    /// Authorized id tags keyed by `idTag`.
    entries: HashMap<String, IdTagInfo>,
}

/// Thread-safe Local Authorization List.
///
/// Cheap to share across tasks behind a plain `Arc<LocalAuthList>`: all
/// mutation happens under an internal [`Mutex`], and no lock is ever held across
/// an `.await`, so the synchronous `std` mutex is sufficient.
#[derive(Debug, Default)]
pub struct LocalAuthList {
    inner: Mutex<Inner>,
}

impl LocalAuthList {
    /// Create an empty list at version `0`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current list version, as reported by `GetLocalListVersion`.
    ///
    /// `0` for an empty list (the feature is supported), otherwise the
    /// `listVersion` of the last accepted `SendLocalList` update.
    pub fn version(&self) -> i32 {
        self.inner
            .lock()
            .expect("local list mutex poisoned")
            .version
    }

    /// Number of entries currently in the list.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("local list mutex poisoned")
            .entries
            .len()
    }

    /// Whether the list has no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Look up the [`IdTagInfo`] the CSMS pushed for `id_tag`, if present.
    ///
    /// This is the read side an offline-authorization path would consult.
    pub fn get(&self, id_tag: &str) -> Option<IdTagInfo> {
        self.inner
            .lock()
            .expect("local list mutex poisoned")
            .entries
            .get(id_tag)
            .cloned()
    }

    /// Apply a `SendLocalList` request and return the resulting [`UpdateStatus`].
    ///
    /// Semantics (OCPP 1.6J §5.x):
    ///
    /// * **Duplicate `idTag`s** within the request are rejected with
    ///   [`UpdateStatus::Failed`] and the list is left untouched.
    /// * **`Full`** replaces the entire list with the request's entries and sets
    ///   the version to `listVersion`. Every entry in a full update must carry an
    ///   `idTagInfo` (a bare `idTag` only makes sense as a *delete*, which has no
    ///   meaning in a full replace); a missing one yields [`UpdateStatus::Failed`]
    ///   with no mutation.
    /// * **`Differential`** requires `listVersion` to be strictly greater than
    ///   the current version, otherwise [`UpdateStatus::VersionMismatch`] is
    ///   returned and nothing changes. Each entry with an `idTagInfo` is
    ///   added/replaced; each bare `idTag` (no `idTagInfo`) removes that entry.
    ///
    /// On success the version is set to `listVersion` and [`UpdateStatus::Accepted`]
    /// is returned. The mutation is performed under the lock so concurrent
    /// updates are serialized and a rejected update never partially applies.
    pub fn apply(&self, request: &SendLocalListRequest) -> UpdateStatus {
        // Reject duplicate id tags before taking the lock — a malformed request
        // shouldn't be partially applied.
        if has_duplicate_id_tags(request) {
            return UpdateStatus::Failed;
        }

        let mut inner = self.inner.lock().expect("local list mutex poisoned");

        match request.update_type {
            UpdateType::Full => {
                // Build the replacement off to the side; only commit once we know
                // every entry is well-formed, so a bad entry leaves the list as-is.
                let mut replacement =
                    HashMap::with_capacity(request.local_authorization_list.len());
                for entry in &request.local_authorization_list {
                    match &entry.id_tag_info {
                        Some(info) => {
                            replacement.insert(entry.id_tag.clone(), info.clone());
                        }
                        // A delete in a full update is meaningless.
                        None => return UpdateStatus::Failed,
                    }
                }
                inner.entries = replacement;
                inner.version = request.list_version;
                UpdateStatus::Accepted
            }
            UpdateType::Differential => {
                // A differential update must advance the version monotonically.
                if request.list_version <= inner.version {
                    return UpdateStatus::VersionMismatch;
                }
                for entry in &request.local_authorization_list {
                    match &entry.id_tag_info {
                        Some(info) => {
                            inner.entries.insert(entry.id_tag.clone(), info.clone());
                        }
                        None => {
                            // Bare idTag => remove (idempotent if absent).
                            inner.entries.remove(&entry.id_tag);
                        }
                    }
                }
                inner.version = request.list_version;
                UpdateStatus::Accepted
            }
        }
    }
}

/// Whether the request lists the same `idTag` more than once.
fn has_duplicate_id_tags(request: &SendLocalListRequest) -> bool {
    let mut seen = std::collections::HashSet::with_capacity(request.local_authorization_list.len());
    !request
        .local_authorization_list
        .iter()
        .all(|e| seen.insert(e.id_tag.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::common::AuthorizationStatus;
    use ocpp_types::v16j::AuthorizationData;

    fn info(status: AuthorizationStatus) -> IdTagInfo {
        IdTagInfo {
            status,
            parent_id_tag: None,
            expiry_date: None,
        }
    }

    fn entry(id: &str, status: AuthorizationStatus) -> AuthorizationData {
        AuthorizationData {
            id_tag: id.to_string(),
            id_tag_info: Some(info(status)),
        }
    }

    fn delete(id: &str) -> AuthorizationData {
        AuthorizationData {
            id_tag: id.to_string(),
            id_tag_info: None,
        }
    }

    fn full(version: i32, entries: Vec<AuthorizationData>) -> SendLocalListRequest {
        SendLocalListRequest {
            list_version: version,
            update_type: UpdateType::Full,
            local_authorization_list: entries,
        }
    }

    fn differential(version: i32, entries: Vec<AuthorizationData>) -> SendLocalListRequest {
        SendLocalListRequest {
            list_version: version,
            update_type: UpdateType::Differential,
            local_authorization_list: entries,
        }
    }

    #[test]
    fn fresh_list_is_empty_at_version_zero() {
        let list = LocalAuthList::new();
        assert_eq!(list.version(), 0);
        assert!(list.is_empty());
        assert_eq!(list.get("ANY"), None);
    }

    #[test]
    fn full_update_replaces_entries_and_sets_version() {
        let list = LocalAuthList::new();
        let status = list.apply(&full(
            7,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Blocked),
            ],
        ));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.version(), 7);
        assert_eq!(list.len(), 2);
        assert_eq!(
            list.get("TAG-A").map(|i| i.status),
            Some(AuthorizationStatus::Accepted)
        );
        assert_eq!(
            list.get("TAG-B").map(|i| i.status),
            Some(AuthorizationStatus::Blocked)
        );
    }

    #[test]
    fn full_update_with_empty_list_clears_and_sets_version() {
        let list = LocalAuthList::new();
        list.apply(&full(
            3,
            vec![entry("TAG-A", AuthorizationStatus::Accepted)],
        ));
        assert_eq!(list.len(), 1);

        let status = list.apply(&full(4, vec![]));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.version(), 4);
        assert!(
            list.is_empty(),
            "a full update with no entries empties the list"
        );
    }

    #[test]
    fn full_update_missing_id_tag_info_fails_without_mutation() {
        let list = LocalAuthList::new();
        list.apply(&full(
            2,
            vec![entry("TAG-A", AuthorizationStatus::Accepted)],
        ));

        // A bare idTag (delete) is meaningless in a full update.
        let status = list.apply(&full(
            5,
            vec![
                entry("TAG-B", AuthorizationStatus::Accepted),
                delete("TAG-C"),
            ],
        ));
        assert_eq!(status, UpdateStatus::Failed);
        // Untouched: still the previous list at the previous version.
        assert_eq!(list.version(), 2);
        assert_eq!(list.len(), 1);
        assert!(list.get("TAG-A").is_some());
        assert_eq!(list.get("TAG-B"), None);
    }

    #[test]
    fn differential_adds_replaces_and_removes() {
        let list = LocalAuthList::new();
        list.apply(&full(
            1,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
            ],
        ));

        let status = list.apply(&differential(
            2,
            vec![
                // replace TAG-A
                entry("TAG-A", AuthorizationStatus::Blocked),
                // add TAG-C
                entry("TAG-C", AuthorizationStatus::Accepted),
                // remove TAG-B
                delete("TAG-B"),
            ],
        ));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.version(), 2);
        assert_eq!(list.len(), 2);
        assert_eq!(
            list.get("TAG-A").map(|i| i.status),
            Some(AuthorizationStatus::Blocked)
        );
        assert_eq!(list.get("TAG-B"), None);
        assert!(list.get("TAG-C").is_some());
    }

    #[test]
    fn differential_removing_absent_tag_is_idempotent() {
        let list = LocalAuthList::new();
        list.apply(&full(
            1,
            vec![entry("TAG-A", AuthorizationStatus::Accepted)],
        ));
        let status = list.apply(&differential(2, vec![delete("GHOST")]));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.version(), 2);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn differential_with_stale_version_is_rejected_without_mutation() {
        let list = LocalAuthList::new();
        list.apply(&full(
            5,
            vec![entry("TAG-A", AuthorizationStatus::Accepted)],
        ));

        // Equal version is not strictly greater => mismatch.
        let same = list.apply(&differential(
            5,
            vec![entry("TAG-Z", AuthorizationStatus::Accepted)],
        ));
        assert_eq!(same, UpdateStatus::VersionMismatch);
        // Lower version => mismatch.
        let lower = list.apply(&differential(
            3,
            vec![entry("TAG-Z", AuthorizationStatus::Accepted)],
        ));
        assert_eq!(lower, UpdateStatus::VersionMismatch);

        // Nothing changed.
        assert_eq!(list.version(), 5);
        assert_eq!(list.len(), 1);
        assert_eq!(list.get("TAG-Z"), None);
    }

    #[test]
    fn duplicate_id_tags_fail_without_mutation() {
        let list = LocalAuthList::new();
        let status = list.apply(&full(
            1,
            vec![
                entry("DUP", AuthorizationStatus::Accepted),
                entry("DUP", AuthorizationStatus::Blocked),
            ],
        ));
        assert_eq!(status, UpdateStatus::Failed);
        assert_eq!(list.version(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn full_update_can_lower_version() {
        // A full update is a complete replace, so it is accepted regardless of
        // version ordering (only differential updates must advance the version).
        let list = LocalAuthList::new();
        list.apply(&full(
            10,
            vec![entry("TAG-A", AuthorizationStatus::Accepted)],
        ));
        let status = list.apply(&full(
            2,
            vec![entry("TAG-B", AuthorizationStatus::Accepted)],
        ));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.version(), 2);
        assert_eq!(list.get("TAG-A"), None);
        assert!(list.get("TAG-B").is_some());
    }
}
