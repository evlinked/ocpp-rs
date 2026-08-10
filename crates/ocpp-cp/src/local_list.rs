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
use ocpp_messages::v201::SendLocalListRequest as V201SendLocalListRequest;
use ocpp_types::common::{AuthorizationStatus, IdTagInfo};
use ocpp_types::v16j::{AuthorizationData, UpdateStatus, UpdateType};
use ocpp_types::v201::{AuthorizationStatusEnumType, SendLocalListStatusEnumType, UpdateEnumType};

/// Default capacity for the Local Authorization List — the value reported for
/// the read-only `LocalAuthListMaxLength` standard configuration key (OCPP 1.6J
/// §9) and the limit enforced by [`LocalAuthList::apply`]. Chosen to match the
/// simulator's other capacity knobs (e.g. `GetConfigurationMaxKeys`); a real
/// charge point would report its hardware limit here.
pub const DEFAULT_LOCAL_AUTH_LIST_MAX_LENGTH: usize = 100;

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
    /// Maximum number of entries the list may hold, modelling the read-only
    /// `LocalAuthListMaxLength` configuration key (OCPP 1.6J §9). `None` leaves
    /// the list unbounded — the behavior of a plain [`new`](Self::new) /
    /// [`Default`] list, so existing callers are unaffected. A bounded list
    /// (see [`with_max_length`](Self::with_max_length)) rejects any
    /// `SendLocalList` that would push the entry count over this cap.
    max_length: Option<usize>,
}

impl LocalAuthList {
    /// Create an empty, **unbounded** list at version `0`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty list at version `0` bounded to at most `max_length`
    /// entries — the capacity reported as `LocalAuthListMaxLength` and enforced
    /// by [`apply`](Self::apply). A `max_length` of `0` rejects every non-empty
    /// update (only an empty `Full` "clear" is accepted).
    pub fn with_max_length(max_length: usize) -> Self {
        Self {
            inner: Mutex::default(),
            max_length: Some(max_length),
        }
    }

    /// The configured maximum number of entries, or `None` if unbounded. This is
    /// the value a CP reports for the `LocalAuthListMaxLength` configuration key.
    pub fn max_length(&self) -> Option<usize> {
        self.max_length
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
    /// * **Over capacity** — when the list is bounded (see
    ///   [`with_max_length`](Self::with_max_length)), an update whose *resulting*
    ///   entry count would exceed `LocalAuthListMaxLength` is rejected with
    ///   [`UpdateStatus::Failed`] and the list is left untouched (OCPP 1.6J §9).
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
                // Reject an over-capacity replacement before committing, so the
                // list is left untouched (OCPP 1.6J §9, LocalAuthListMaxLength).
                if self.exceeds_capacity(replacement.len()) {
                    return UpdateStatus::Failed;
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
                // Project the resulting entry count without mutating, and reject
                // an over-capacity result so nothing partially applies. Duplicate
                // idTags are already rejected above, so each idTag appears once
                // and its net effect on the size is independent of the others:
                // an add of a new tag is +1, a delete of a present tag is -1,
                // and replaces / no-op deletes are 0.
                if let Some(max) = self.max_length {
                    let mut projected = inner.entries.len();
                    for entry in &request.local_authorization_list {
                        match &entry.id_tag_info {
                            Some(_) if !inner.entries.contains_key(&entry.id_tag) => {
                                projected += 1;
                            }
                            None if inner.entries.contains_key(&entry.id_tag) => {
                                projected -= 1;
                            }
                            _ => {}
                        }
                    }
                    if projected > max {
                        return UpdateStatus::Failed;
                    }
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

    /// Whether a resulting list of `len` entries would exceed the configured
    /// `LocalAuthListMaxLength`. Always `false` for an unbounded list.
    fn exceeds_capacity(&self, len: usize) -> bool {
        self.max_length.is_some_and(|max| len > max)
    }

    /// Apply an **OCPP 2.0.1** `SendLocalList` request against this same store,
    /// returning the resulting [`SendLocalListStatusEnumType`].
    ///
    /// The list is a single source of truth across dialects: a `for_version`
    /// station shares one version counter, one capacity, and one set of entries
    /// regardless of whether the CSMS speaks 1.6J or 2.0.1. So rather than a
    /// parallel 2.0.1 store, the 2.0.1 request is translated onto the store's
    /// entry model (`v16j_request_from_v201`) and run through the same
    /// [`apply`](Self::apply) decision — Full replace, Differential version-gate,
    /// duplicate rejection, and the over-capacity guard all behave identically —
    /// and the resulting [`UpdateStatus`] is mapped back to the 2.0.1 status
    /// (`update_status_to_v201`).
    ///
    /// Ports `ocpp.v201.call.SendLocalList` / `ocpp.v201.call_result.SendLocalList`.
    /// A given [`ChargePoint`](crate::ChargePoint) only ever speaks one dialect,
    /// so the shared entry map is never populated from both at once.
    ///
    /// **Trust boundary:** a malformed request — an empty or absent
    /// `localAuthorizationList`, duplicate `idToken`s, a Differential update with
    /// a stale `versionNumber`, or a bare `idToken` (delete) inside a Full update
    /// — resolves to a faithful `Failed` / `VersionMismatch`, never a panic, and
    /// leaves the stored list untouched.
    pub fn apply_v201(&self, request: &V201SendLocalListRequest) -> SendLocalListStatusEnumType {
        update_status_to_v201(self.apply(&v16j_request_from_v201(request)))
    }
}

/// Translate an OCPP 2.0.1 `SendLocalList` request onto the store's entry model
/// so it can run through the shared [`LocalAuthList::apply`] decision.
///
/// The mapping is faithful to what the store keys and tracks: the 2.0.1
/// `idToken.idToken` string becomes the list key (as `idTag` does in 1.6J), a
/// present `idTokenInfo` becomes an add/replace and an absent one a delete (the
/// same `Some`/`None` discriminator `apply` already relies on), the
/// `versionNumber` and `updateType` carry straight across, and an absent
/// `localAuthorizationList` becomes the empty list (a Full "clear"). The
/// `groupIdToken`, when present, is preserved as the 1.6J `parentIdTag`.
///
/// `cacheExpiryDateTime` (a 2.0.1 caching hint) is intentionally dropped: it is
/// an RFC 3339 string with no bearing on list *management* (version/capacity),
/// and no 2.0.1 read path consults the stored expiry yet.
fn v16j_request_from_v201(request: &V201SendLocalListRequest) -> SendLocalListRequest {
    SendLocalListRequest {
        list_version: request.version_number,
        update_type: match request.update_type {
            UpdateEnumType::Full => UpdateType::Full,
            UpdateEnumType::Differential => UpdateType::Differential,
        },
        local_authorization_list: request
            .local_authorization_list
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|entry| AuthorizationData {
                id_tag: entry.id_token.id_token.clone(),
                id_tag_info: entry.id_token_info.as_ref().map(|info| IdTagInfo {
                    status: v201_status_to_v16j(info.status),
                    parent_id_tag: info.group_id_token.as_ref().map(|g| g.id_token.clone()),
                    expiry_date: None,
                }),
            })
            .collect(),
    }
}

/// Map a 2.0.1 [`AuthorizationStatusEnumType`] onto the 1.6J
/// [`AuthorizationStatus`] the store's entry model carries.
///
/// The five 1.6J statuses map 1:1. The 2.0.1-only refusals (`NoCredit`,
/// `NotAllowedTypeEVSE`, `NotAtThisLocation`, `NotAtThisTime`, `Unknown`) have no
/// 1.6J equivalent; they all mean "not a usable token", which 1.6J expresses as
/// `Invalid`. This only affects the *stored* status a future offline-auth path
/// would read; it has no bearing on the `SendLocalList` accept/reject decision,
/// which turns solely on version, capacity, and structure.
fn v201_status_to_v16j(status: AuthorizationStatusEnumType) -> AuthorizationStatus {
    match status {
        AuthorizationStatusEnumType::Accepted => AuthorizationStatus::Accepted,
        AuthorizationStatusEnumType::Blocked => AuthorizationStatus::Blocked,
        AuthorizationStatusEnumType::Expired => AuthorizationStatus::Expired,
        AuthorizationStatusEnumType::ConcurrentTx => AuthorizationStatus::ConcurrentTx,
        AuthorizationStatusEnumType::Invalid
        | AuthorizationStatusEnumType::NoCredit
        | AuthorizationStatusEnumType::NotAllowedTypeEvse
        | AuthorizationStatusEnumType::NotAtThisLocation
        | AuthorizationStatusEnumType::NotAtThisTime
        | AuthorizationStatusEnumType::Unknown => AuthorizationStatus::Invalid,
    }
}

/// Map the store's 1.6J [`UpdateStatus`] onto the 2.0.1
/// [`SendLocalListStatusEnumType`].
///
/// `Accepted` and `VersionMismatch` are shared 1:1. The store never returns
/// `NotSupported` (the CP implements the Local Authorization List profile), so it
/// and `Failed` both fold to the faithful 2.0.1 `Failed`.
fn update_status_to_v201(status: UpdateStatus) -> SendLocalListStatusEnumType {
    match status {
        UpdateStatus::Accepted => SendLocalListStatusEnumType::Accepted,
        UpdateStatus::VersionMismatch => SendLocalListStatusEnumType::VersionMismatch,
        UpdateStatus::Failed | UpdateStatus::NotSupported => SendLocalListStatusEnumType::Failed,
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

    #[test]
    fn new_list_is_unbounded() {
        let list = LocalAuthList::new();
        assert_eq!(list.max_length(), None);
        // A large full update is accepted on an unbounded list.
        let big: Vec<_> = (0..1_000)
            .map(|i| entry(&format!("TAG-{i}"), AuthorizationStatus::Accepted))
            .collect();
        assert_eq!(list.apply(&full(1, big)), UpdateStatus::Accepted);
        assert_eq!(list.len(), 1_000);
    }

    #[test]
    fn full_update_at_capacity_is_accepted() {
        let list = LocalAuthList::with_max_length(2);
        assert_eq!(list.max_length(), Some(2));
        let status = list.apply(&full(
            1,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
            ],
        ));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn full_update_over_capacity_is_rejected_without_mutation() {
        let list = LocalAuthList::with_max_length(2);
        // Seed an at-capacity list so we can prove the rejected update leaves it
        // entirely untouched.
        list.apply(&full(
            3,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
            ],
        ));

        let status = list.apply(&full(
            4,
            vec![
                entry("TAG-X", AuthorizationStatus::Accepted),
                entry("TAG-Y", AuthorizationStatus::Accepted),
                entry("TAG-Z", AuthorizationStatus::Accepted),
            ],
        ));
        assert_eq!(status, UpdateStatus::Failed);
        // Untouched: still the previous two entries at the previous version.
        assert_eq!(list.version(), 3);
        assert_eq!(list.len(), 2);
        assert!(list.get("TAG-A").is_some());
        assert_eq!(list.get("TAG-X"), None);
    }

    #[test]
    fn differential_over_capacity_is_rejected_without_mutation() {
        let list = LocalAuthList::with_max_length(2);
        list.apply(&full(
            1,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
            ],
        ));

        // Adding a third distinct tag would make the list size 3 > 2.
        let status = list.apply(&differential(
            2,
            vec![entry("TAG-C", AuthorizationStatus::Accepted)],
        ));
        assert_eq!(status, UpdateStatus::Failed);
        assert_eq!(
            list.version(),
            1,
            "a rejected update does not bump the version"
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list.get("TAG-C"), None, "nothing partially applied");
    }

    #[test]
    fn differential_replacing_within_capacity_is_accepted() {
        let list = LocalAuthList::with_max_length(2);
        list.apply(&full(
            1,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
            ],
        ));
        // Replacing an existing tag keeps the size at 2 (at capacity) — accepted.
        let status = list.apply(&differential(
            2,
            vec![entry("TAG-A", AuthorizationStatus::Blocked)],
        ));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.len(), 2);
        assert_eq!(
            list.get("TAG-A").map(|i| i.status),
            Some(AuthorizationStatus::Blocked)
        );
    }

    #[test]
    fn differential_freeing_room_then_adding_is_accepted() {
        let list = LocalAuthList::with_max_length(2);
        list.apply(&full(
            1,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
            ],
        ));
        // Remove one and add one in the same update: net size unchanged (2 ≤ 2).
        let status = list.apply(&differential(
            2,
            vec![
                delete("TAG-B"),
                entry("TAG-C", AuthorizationStatus::Accepted),
            ],
        ));
        assert_eq!(status, UpdateStatus::Accepted);
        assert_eq!(list.len(), 2);
        assert_eq!(list.get("TAG-B"), None);
        assert!(list.get("TAG-C").is_some());
    }

    #[test]
    fn zero_capacity_accepts_only_an_empty_clear() {
        let list = LocalAuthList::with_max_length(0);
        // Any non-empty update is rejected.
        assert_eq!(
            list.apply(&full(
                1,
                vec![entry("TAG-A", AuthorizationStatus::Accepted)]
            )),
            UpdateStatus::Failed
        );
        assert!(list.is_empty());
        // An empty full update (a "clear") is still accepted.
        assert_eq!(list.apply(&full(2, vec![])), UpdateStatus::Accepted);
        assert_eq!(list.version(), 2);
    }

    /// OCPP 2.0.1 `SendLocalList` applied against the same shared store via
    /// [`LocalAuthList::apply_v201`]. Mirrors the 1.6J cases above to prove the
    /// 2.0.1 translation reuses the same accept/reject decision.
    mod v201 {
        use super::*;
        use ocpp_types::v201::{
            AuthorizationData as V201AuthorizationData, IdTokenEnumType, IdTokenInfoType,
            IdTokenType,
        };

        fn token(id: &str) -> IdTokenType {
            IdTokenType {
                id_token: id.to_string(),
                kind: IdTokenEnumType::Iso14443,
                additional_info: None,
                custom_data: None,
            }
        }

        fn v201_info(status: AuthorizationStatusEnumType) -> IdTokenInfoType {
            IdTokenInfoType {
                status,
                cache_expiry_date_time: None,
                charging_priority: None,
                language1: None,
                evse_id: None,
                language2: None,
                group_id_token: None,
                personal_message: None,
                custom_data: None,
            }
        }

        fn v201_entry(id: &str, status: AuthorizationStatusEnumType) -> V201AuthorizationData {
            V201AuthorizationData {
                id_token: token(id),
                id_token_info: Some(v201_info(status)),
                custom_data: None,
            }
        }

        /// A bare `idToken` (no `idTokenInfo`) — a differential delete.
        fn v201_delete(id: &str) -> V201AuthorizationData {
            V201AuthorizationData {
                id_token: token(id),
                id_token_info: None,
                custom_data: None,
            }
        }

        fn v201_full(
            version: i32,
            entries: Vec<V201AuthorizationData>,
        ) -> V201SendLocalListRequest {
            V201SendLocalListRequest {
                version_number: version,
                update_type: UpdateEnumType::Full,
                local_authorization_list: Some(entries),
                custom_data: None,
            }
        }

        fn v201_differential(
            version: i32,
            entries: Vec<V201AuthorizationData>,
        ) -> V201SendLocalListRequest {
            V201SendLocalListRequest {
                version_number: version,
                update_type: UpdateEnumType::Differential,
                local_authorization_list: Some(entries),
                custom_data: None,
            }
        }

        #[test]
        fn full_update_is_accepted_bumps_version_and_stores_entries() {
            let list = LocalAuthList::new();
            let status = list.apply_v201(&v201_full(
                7,
                vec![
                    v201_entry("TAG-A", AuthorizationStatusEnumType::Accepted),
                    v201_entry("TAG-B", AuthorizationStatusEnumType::Blocked),
                ],
            ));
            assert_eq!(status, SendLocalListStatusEnumType::Accepted);
            assert_eq!(list.version(), 7);
            assert_eq!(list.len(), 2);
            // Entries are stored under the `idToken` string key, with the 2.0.1
            // status mapped onto the store's 1.6J model.
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
        fn full_update_with_absent_list_clears_and_sets_version() {
            let list = LocalAuthList::new();
            list.apply_v201(&v201_full(
                3,
                vec![v201_entry("TAG-A", AuthorizationStatusEnumType::Accepted)],
            ));
            assert_eq!(list.len(), 1);

            // A Full update whose `localAuthorizationList` is omitted entirely is a
            // clear — the translation maps `None` to the empty list.
            let status = list.apply_v201(&V201SendLocalListRequest {
                version_number: 4,
                update_type: UpdateEnumType::Full,
                local_authorization_list: None,
                custom_data: None,
            });
            assert_eq!(status, SendLocalListStatusEnumType::Accepted);
            assert_eq!(list.version(), 4);
            assert!(list.is_empty());
        }

        #[test]
        fn differential_adds_replaces_and_removes() {
            let list = LocalAuthList::new();
            list.apply_v201(&v201_full(
                1,
                vec![
                    v201_entry("TAG-A", AuthorizationStatusEnumType::Accepted),
                    v201_entry("TAG-B", AuthorizationStatusEnumType::Accepted),
                ],
            ));

            let status = list.apply_v201(&v201_differential(
                2,
                vec![
                    v201_entry("TAG-A", AuthorizationStatusEnumType::Blocked), // replace
                    v201_entry("TAG-C", AuthorizationStatusEnumType::Accepted), // add
                    v201_delete("TAG-B"),                                      // remove
                ],
            ));
            assert_eq!(status, SendLocalListStatusEnumType::Accepted);
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
        fn differential_with_stale_version_is_version_mismatch_without_mutation() {
            let list = LocalAuthList::new();
            list.apply_v201(&v201_full(
                5,
                vec![v201_entry("TAG-A", AuthorizationStatusEnumType::Accepted)],
            ));

            // Equal version is not strictly greater => mismatch, nothing changes.
            let status = list.apply_v201(&v201_differential(
                5,
                vec![v201_entry("TAG-Z", AuthorizationStatusEnumType::Accepted)],
            ));
            assert_eq!(status, SendLocalListStatusEnumType::VersionMismatch);
            assert_eq!(list.version(), 5);
            assert_eq!(list.len(), 1);
            assert_eq!(list.get("TAG-Z"), None);
        }

        #[test]
        fn duplicate_id_tokens_fail_without_mutation() {
            let list = LocalAuthList::new();
            let status = list.apply_v201(&v201_full(
                1,
                vec![
                    v201_entry("DUP", AuthorizationStatusEnumType::Accepted),
                    v201_entry("DUP", AuthorizationStatusEnumType::Blocked),
                ],
            ));
            assert_eq!(status, SendLocalListStatusEnumType::Failed);
            assert_eq!(list.version(), 0);
            assert!(list.is_empty());
        }

        #[test]
        fn full_update_with_bare_id_token_delete_fails() {
            // A delete (no `idTokenInfo`) is meaningless in a Full replace and is
            // rejected, leaving the list untouched — the same rule the 1.6J path
            // enforces.
            let list = LocalAuthList::new();
            list.apply_v201(&v201_full(
                2,
                vec![v201_entry("TAG-A", AuthorizationStatusEnumType::Accepted)],
            ));
            let status = list.apply_v201(&v201_full(
                5,
                vec![
                    v201_entry("TAG-B", AuthorizationStatusEnumType::Accepted),
                    v201_delete("TAG-C"),
                ],
            ));
            assert_eq!(status, SendLocalListStatusEnumType::Failed);
            assert_eq!(list.version(), 2);
            assert_eq!(list.len(), 1);
            assert!(list.get("TAG-A").is_some());
        }

        #[test]
        fn over_capacity_full_update_is_rejected_without_mutation() {
            let list = LocalAuthList::with_max_length(2);
            list.apply_v201(&v201_full(
                3,
                vec![
                    v201_entry("TAG-A", AuthorizationStatusEnumType::Accepted),
                    v201_entry("TAG-B", AuthorizationStatusEnumType::Accepted),
                ],
            ));
            let status = list.apply_v201(&v201_full(
                4,
                vec![
                    v201_entry("TAG-X", AuthorizationStatusEnumType::Accepted),
                    v201_entry("TAG-Y", AuthorizationStatusEnumType::Accepted),
                    v201_entry("TAG-Z", AuthorizationStatusEnumType::Accepted),
                ],
            ));
            assert_eq!(status, SendLocalListStatusEnumType::Failed);
            assert_eq!(list.version(), 3);
            assert_eq!(list.len(), 2);
        }

        #[test]
        fn group_id_token_is_preserved_as_parent_id_tag() {
            let list = LocalAuthList::new();
            let mut info = v201_info(AuthorizationStatusEnumType::Accepted);
            info.group_id_token = Some(token("PARENT"));
            list.apply_v201(&v201_full(
                1,
                vec![V201AuthorizationData {
                    id_token: token("TAG-A"),
                    id_token_info: Some(info),
                    custom_data: None,
                }],
            ));
            assert_eq!(
                list.get("TAG-A").and_then(|i| i.parent_id_tag),
                Some("PARENT".to_string())
            );
        }

        #[test]
        fn v201_only_refusal_statuses_map_to_invalid() {
            // The 2.0.1-only refusals collapse to the 1.6J `Invalid` in the store.
            for s in [
                AuthorizationStatusEnumType::NoCredit,
                AuthorizationStatusEnumType::NotAllowedTypeEvse,
                AuthorizationStatusEnumType::NotAtThisLocation,
                AuthorizationStatusEnumType::NotAtThisTime,
                AuthorizationStatusEnumType::Unknown,
            ] {
                assert_eq!(v201_status_to_v16j(s), AuthorizationStatus::Invalid);
            }
        }
    }
}
