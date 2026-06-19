//! End-to-end CS→CP test: the Local Authorization List drives `authorize()`
//! for offline authorization (Issue #104).
//!
//! PR #103 (Issue #93) added Local Authorization List *management*
//! (`GetLocalListVersion` / `SendLocalList`) but deliberately left the list
//! disconnected from the CP's authorization decision. This test proves the gap
//! is closed: once the CSMS pushes a list, [`ChargePoint::authorize`] consults
//! it **before** any `Authorize` round-trip, with the precedence and authority
//! the OCPP 1.6J spec (§4.1.3) gives the list:
//!
//! * a tag present (and non-expired) in the list authorizes **with no** CSMS
//!   round-trip — proven black-box with a CSMS whose `Authorize` answer differs
//!   from the list's, so the returned status reveals which source decided;
//! * the list is **authoritative**: a `Blocked` list entry rejects locally,
//!   without round-tripping (unlike the opportunistic cache, which re-checks
//!   non-`Accepted` results);
//! * a tag *absent* from the list falls through to the CSMS;
//! * an entry past its `expiryDate` is **not** honored and falls through;
//! * the list takes **precedence over the cache**: a tag cached `Accepted` is
//!   still rejected once the list says `Blocked`, with no fresh round-trip.
//!
//! Rust counterpart of the Python reference's `@on('Authorize')` flow in
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py),
//! driven from
//! [`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    AuthorizeRequest, AuthorizeResponse, BootNotificationRequest, BootNotificationResponse,
    HeartbeatRequest, HeartbeatResponse, RegistrationStatus, StatusNotificationRequest,
    StatusNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::common::{AuthorizationStatus, IdTagInfo};
use ocpp_types::v16j::{AuthorizationData, UpdateStatus, UpdateType};

/// A CSMS dispatcher whose `Authorize` handler counts how many times it is hit
/// and always answers with `fallback`. Because the answer is a fixed sentinel
/// distinct from what the Local Authorization List holds, the status returned by
/// `cp.authorize()` reveals whether the *list* (local) or the *CSMS* (round-trip)
/// decided, and the counter confirms whether a round-trip happened at all.
fn csms_dispatcher(
    authorize_calls: Arc<AtomicUsize>,
    fallback: AuthorizationStatus,
) -> ActionDispatcher {
    let mut d = ActionDispatcher::new();
    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: chrono::Utc::now(),
            // A long interval keeps stray heartbeats from racing the assertions.
            interval: 300,
            status: RegistrationStatus::Accepted,
        })
    });
    d.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: chrono::Utc::now(),
        })
    });
    d.on(|_req: StatusNotificationRequest| async move { Ok(StatusNotificationResponse {}) });
    d.on(move |_req: AuthorizeRequest| {
        let authorize_calls = authorize_calls.clone();
        let fallback = fallback.clone();
        async move {
            authorize_calls.fetch_add(1, Ordering::SeqCst);
            Ok(AuthorizeResponse {
                id_tag_info: IdTagInfo {
                    status: fallback,
                    parent_id_tag: None,
                    expiry_date: None,
                },
            })
        }
    });
    d
}

/// A CSMS `Authorize` handler that accepts the **first** lookup and blocks every
/// one after it — the same flip used by `clear_cache.rs` to distinguish a cached
/// answer from a fresh round-trip.
fn flip_dispatcher(authorize_calls: Arc<AtomicUsize>) -> ActionDispatcher {
    let mut d = ActionDispatcher::new();
    d.on(|_req: BootNotificationRequest| async move {
        Ok(BootNotificationResponse {
            current_time: chrono::Utc::now(),
            interval: 300,
            status: RegistrationStatus::Accepted,
        })
    });
    d.on(|_req: HeartbeatRequest| async move {
        Ok(HeartbeatResponse {
            current_time: chrono::Utc::now(),
        })
    });
    d.on(|_req: StatusNotificationRequest| async move { Ok(StatusNotificationResponse {}) });
    d.on(move |_req: AuthorizeRequest| {
        let authorize_calls = authorize_calls.clone();
        async move {
            let status = if authorize_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                AuthorizationStatus::Accepted
            } else {
                AuthorizationStatus::Blocked
            };
            Ok(AuthorizeResponse {
                id_tag_info: IdTagInfo {
                    status,
                    parent_id_tag: None,
                    expiry_date: None,
                },
            })
        }
    });
    d
}

async fn start_csms(dispatcher: ActionDispatcher) -> (OcppServer, SocketAddr) {
    let handler = Arc::new(DispatchHandler::new(Arc::new(dispatcher)));
    let (mut server, _events) = OcppServer::new(TransportConfig::default(), handler);
    server.start("127.0.0.1:0").await.expect("server start");
    let addr = server.local_addr().expect("server local addr");
    (server, addr)
}

fn cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        connector_count: 1,
        auto_reconnect: false,
        meter_values_interval: 3600,
        ..ChargePointConfig::default()
    }
}

fn entry_with(
    id: &str,
    status: AuthorizationStatus,
    expiry: Option<chrono::DateTime<chrono::Utc>>,
) -> AuthorizationData {
    AuthorizationData {
        id_tag: id.to_string(),
        id_tag_info: Some(IdTagInfo {
            status,
            parent_id_tag: None,
            expiry_date: expiry,
        }),
    }
}

fn entry(id: &str, status: AuthorizationStatus) -> AuthorizationData {
    entry_with(id, status, None)
}

#[tokio::test]
async fn local_list_authorizes_offline_without_csms_roundtrip() {
    let cp_id = "CP_LL_AUTH_01";
    let authorize_calls = Arc::new(AtomicUsize::new(0));
    // The CSMS fallback is a sentinel (`Invalid`) that never appears in the list,
    // so any `Invalid` result means the CP round-tripped instead of using the list.
    let (mut server, addr) = start_csms(csms_dispatcher(
        authorize_calls.clone(),
        AuthorizationStatus::Invalid,
    ))
    .await;

    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "CSMS must be able to route CALLs"
    );

    // CSMS pushes a list: TAG-OK accepted, TAG-NO blocked, TAG-OLD accepted but
    // already expired.
    let past = chrono::Utc::now() - chrono::Duration::seconds(60);
    let status = server
        .send_local_list(
            cp_id,
            1,
            UpdateType::Full,
            vec![
                entry("TAG-OK", AuthorizationStatus::Accepted),
                entry("TAG-NO", AuthorizationStatus::Blocked),
                entry_with("TAG-OLD", AuthorizationStatus::Accepted, Some(past)),
            ],
        )
        .await
        .expect("SendLocalList resolves");
    assert_eq!(status, UpdateStatus::Accepted, "full update is accepted");

    // 1. A tag in the list authorizes locally — no CSMS round-trip.
    let ok = cp.authorize("TAG-OK").await.expect("authorize resolves");
    assert_eq!(
        ok.status,
        AuthorizationStatus::Accepted,
        "an Accepted list entry authorizes locally"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        0,
        "a list hit must not produce an Authorize round-trip"
    );

    // 2. The list is authoritative: a Blocked entry rejects locally, no round-trip.
    let no = cp.authorize("TAG-NO").await.expect("authorize resolves");
    assert_eq!(
        no.status,
        AuthorizationStatus::Blocked,
        "a Blocked list entry is honored verbatim (authoritative)"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        0,
        "a non-Accepted list entry still decides without a round-trip"
    );

    // 3. A tag absent from the list falls through to the CSMS.
    let unknown = cp.authorize("TAG-MISS").await.expect("authorize resolves");
    assert_eq!(
        unknown.status,
        AuthorizationStatus::Invalid,
        "an unknown tag round-trips and gets the CSMS answer"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        1,
        "the miss produced exactly one CSMS round-trip"
    );

    // 4. An expired list entry is not honored and falls through to the CSMS.
    let old = cp.authorize("TAG-OLD").await.expect("authorize resolves");
    assert_eq!(
        old.status,
        AuthorizationStatus::Invalid,
        "an entry past its expiryDate is ignored, so the CSMS decides"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        2,
        "the expired entry forced a second round-trip"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn local_list_takes_precedence_over_cache() {
    let cp_id = "CP_LL_AUTH_02";
    let id_tag = "TAG-PREC";
    let authorize_calls = Arc::new(AtomicUsize::new(0));
    // First lookup Accepted (and cached CP-side), every later one Blocked — so a
    // *fresh* round-trip after the first is observable as a status flip.
    let (mut server, addr) = start_csms(flip_dispatcher(authorize_calls.clone())).await;

    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(
        server.is_cp_connected(cp_id),
        "CSMS must be able to route CALLs"
    );

    // 1. Empty list → the first authorize round-trips → Accepted, cached CP-side.
    let first = cp.authorize(id_tag).await.expect("authorize resolves");
    assert_eq!(first.status, AuthorizationStatus::Accepted);
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        1,
        "the first lookup round-trips (empty list, cold cache)"
    );

    // 2. CSMS now pushes a list that Blocks the same tag.
    let status = server
        .send_local_list(
            cp_id,
            1,
            UpdateType::Full,
            vec![entry(id_tag, AuthorizationStatus::Blocked)],
        )
        .await
        .expect("SendLocalList resolves");
    assert_eq!(status, UpdateStatus::Accepted);

    // 3. The list outranks the cached Accepted: the tag is now Blocked, and with
    //    no round-trip. (A cache hit would still be Accepted; a round-trip would
    //    also be Blocked but would bump the counter — so Blocked + unchanged count
    //    proves the *list* decided.)
    let after = cp.authorize(id_tag).await.expect("authorize resolves");
    assert_eq!(
        after.status,
        AuthorizationStatus::Blocked,
        "the Local Authorization List outranks the cached Accepted entry"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        1,
        "the list decision short-circuits without a fresh round-trip"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
