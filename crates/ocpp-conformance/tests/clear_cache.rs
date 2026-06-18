//! End-to-end CS→CP ClearCache test (Issue #60).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the M5 `ClearCache` command (OCPP 1.6J §5.2) through the
//! `OcppServer::clear_cache` helper, asserting the CP empties its authorization
//! cache and answers `Accepted`.
//!
//! The cache effect is proven **black-box**, without reaching into the CP's
//! private `auth_cache`: the CSMS `Authorize` handler returns `Accepted` on the
//! *first* round-trip and `Blocked` on every subsequent one. So:
//!
//!   1. `authorize(TAG)` → CSMS round-trip #1 → `Accepted`, now cached.
//!   2. `authorize(TAG)` again → served from the cache (`Accepted`) with no
//!      round-trip — if it had hit the CSMS it would have seen `Blocked`.
//!   3. `clear_cache` → `Accepted`.
//!   4. `authorize(TAG)` again → cache empty, so a fresh CSMS round-trip → now
//!      `Blocked`, proving the cache was cleared.
//!
//! Rust counterpart of the Python reference's central system driving
//! `ClearCache`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against the default `@on('ClearCache')` charge point.

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
use ocpp_types::v16j::ClearCacheStatus;

/// A CSMS dispatcher whose `Authorize` handler accepts the **first** id-tag
/// lookup and blocks every one after it. The flip lets the test distinguish a
/// cache hit (still `Accepted`) from a fresh round-trip (now `Blocked`) purely
/// from the CP's public `authorize` result.
fn csms_dispatcher(authorize_calls: Arc<AtomicUsize>) -> ActionDispatcher {
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
        async move {
            // First lookup accepted (and cached CP-side); later lookups blocked.
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
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        ..ChargePointConfig::default()
    }
}

#[tokio::test]
async fn csms_clear_cache_empties_cp_authorization_cache() {
    let cp_id = "CP_CLEARCACHE_01";
    let id_tag = "TAG-CLR-01";
    let authorize_calls = Arc::new(AtomicUsize::new(0));

    let (mut server, addr) = start_csms(csms_dispatcher(authorize_calls.clone())).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // 1. First authorization → real CSMS round-trip → Accepted, cached CP-side.
    let first = cp
        .authorize(id_tag)
        .await
        .expect("first authorize resolves");
    assert_eq!(
        first.status,
        AuthorizationStatus::Accepted,
        "the CSMS accepts the first lookup"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        1,
        "the first authorize must hit the CSMS"
    );

    // 2. Second authorization → served from the cache, no round-trip. If it had
    //    reached the CSMS it would now be Blocked, so Accepted proves the hit.
    let cached = cp
        .authorize(id_tag)
        .await
        .expect("cached authorize resolves");
    assert_eq!(
        cached.status,
        AuthorizationStatus::Accepted,
        "the cached Accepted entry short-circuits the CSMS"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        1,
        "a cache hit must not produce a second CSMS round-trip"
    );

    // 3. CSMS clears the CP's authorization cache (§5.2).
    let status = server
        .clear_cache(cp_id)
        .await
        .expect("clear_cache resolves");
    assert_eq!(
        status,
        ClearCacheStatus::Accepted,
        "the CP accepts ClearCache and empties its cache"
    );

    // 4. Third authorization → cache empty → fresh CSMS round-trip → now Blocked.
    let after_clear = cp
        .authorize(id_tag)
        .await
        .expect("post-clear authorize resolves");
    assert_eq!(
        after_clear.status,
        AuthorizationStatus::Blocked,
        "after ClearCache the lookup must round-trip and see the CSMS's new answer"
    );
    assert_eq!(
        authorize_calls.load(Ordering::SeqCst),
        2,
        "the post-clear authorize must hit the CSMS again (cache was emptied)"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
