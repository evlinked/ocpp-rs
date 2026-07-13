//! WebSocket connect-path contract — conformance suite.
//!
//! A faithful port of the connect-path half of the mobilityhouse/ocpp
//! reference's
//! [`tests/test_charge_point_connection.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point_connection.py)
//! (`TestExtractChargePointId`), which pins how a charge-point identity is
//! recovered from the WebSocket connect path.
//!
//! The connect path is an **attacker-influenceable trust boundary**: whatever
//! this returns becomes the routing key for every subsequent CALL from that
//! socket. The reference's `extract_charge_point_id` (`ocpp/charge_point.py`)
//! is ported to `ocpp_transport::websocket::server::extract_charge_point_id`,
//! and this suite exercises it through the crate's **public** API — the same
//! entry point a downstream embedder of the CSMS (e.g. issue #66's charge-hub
//! adapter) relies on. The transport crate carries its own in-crate unit
//! tests, but those cannot guard the item's *visibility*: pinning the contract
//! from a separate crate keeps `extract_charge_point_id` a stable public
//! surface, not an internal detail that could silently become private.
//!
//! ## Divergence from the reference — pinned, not dropped
//!
//! Python's `extract_charge_point_id(None)` returns `None`. Rust has no
//! nullable `&str`, so the reference's `None`-input row maps onto the
//! empty-string row here — both yield `None` (the socket is refused rather than
//! given a live routing key). This is the same mapping documented on the port
//! itself; the row is pinned below (`root_and_empty_paths_are_refused`), not
//! silently omitted.

use ocpp_transport::websocket::server::extract_charge_point_id;

/// One row per reference assertion in `TestExtractChargePointId`, keyed by the
/// reference method name so a failure names the exact reference case.
///
/// `(connect path, expected charge-point id)` — `None` means "refuse this
/// path" (no usable, non-whitespace segment ⇒ never a routing key).
const REFERENCE_ROWS: &[(&str, &str, Option<&str>)] = &[
    ("simple_path", "/CP001", Some("CP001")),
    ("nested_path", "/ocpp/CP001", Some("CP001")),
    ("deeply_nested_path", "/api/v1/ocpp/CP001", Some("CP001")),
    ("trailing_slash", "/CP001/", Some("CP001")),
    ("root_path_returns_none", "/", None),
    // Python's `None` input maps here: no nullable &str, same `None` result.
    ("empty_string_returns_none", "", None),
    ("path_without_leading_slash", "CP001", Some("CP001")),
    // Query strings are stripped (the `urlparse(...).path` component) so a
    // `?token=…` credential can never leak into the routing key.
    (
        "path_with_query_string",
        "/CP001?token=abc123",
        Some("CP001"),
    ),
    // Fragments are stripped for the same reason.
    ("path_with_fragment", "/CP001#section", Some("CP001")),
    ("whitespace_only_segment", "/   ", None),
    (
        "charge_point_id_with_special_chars",
        "/CP-001_v2",
        Some("CP-001_v2"),
    ),
    (
        "charge_point_id_with_dots",
        "/EVB-P12354.00.01",
        Some("EVB-P12354.00.01"),
    ),
    ("multiple_slashes", "///CP001", Some("CP001")),
];

#[test]
fn extract_charge_point_id_matches_reference_table() {
    for &(case, path, expected) in REFERENCE_ROWS {
        assert_eq!(
            extract_charge_point_id(path),
            expected,
            "test_charge_point_connection.py::TestExtractChargePointId::test_{case} \
             — path {path:?} should extract {expected:?}",
        );
    }
}

/// Trust-boundary focus: paths with no usable, non-whitespace segment must be
/// refused so they can never become a live routing key. Groups the reference's
/// `None`-yielding rows (root, empty, whitespace-only) into one explicit
/// negative assertion.
#[test]
fn root_and_empty_paths_are_refused() {
    for path in ["/", "", "   ", "/   ", "/\t/", "///"] {
        assert_eq!(
            extract_charge_point_id(path),
            None,
            "path {path:?} has no usable charge-point id and must be refused, \
             never returned as an (empty/whitespace) routing key",
        );
    }
}

/// Trust-boundary focus: query strings and fragments are stripped before the
/// id is taken, so neither a `?token=…` credential nor a `#fragment` can leak
/// into — or masquerade as — the charge-point identity.
#[test]
fn query_and_fragment_never_leak_into_the_id() {
    assert_eq!(
        extract_charge_point_id("/CP001?token=secret"),
        Some("CP001")
    );
    assert_eq!(extract_charge_point_id("/CP001#frag"), Some("CP001"));
    // Both present, in URL order (query then fragment)…
    assert_eq!(
        extract_charge_point_id("/CP001?token=secret#frag"),
        Some("CP001"),
    );
    // …and, defensively, in the reverse order too (a `#` before a `?`): the id
    // is whatever precedes the first of either delimiter, so both are stripped.
    assert_eq!(
        extract_charge_point_id("/CP001#frag?token=secret"),
        Some("CP001"),
    );
    assert_eq!(extract_charge_point_id("/ocpp/CP001?a=1"), Some("CP001"));
}
