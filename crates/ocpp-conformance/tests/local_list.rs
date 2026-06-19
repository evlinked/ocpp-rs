//! End-to-end CS→CP Local Authorization List test (Issue #93).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the M5 Local Authorization List Management profile
//! (OCPP 1.6J §5.x) — `GetLocalListVersion` and `SendLocalList` — through the
//! `OcppServer::get_local_list_version` / `OcppServer::send_local_list` helpers,
//! asserting the CP reports the right version and applies `Full`/`Differential`
//! updates faithfully.
//!
//! The version flow and the returned `UpdateStatus` are proven black-box over
//! the wire; the resulting list *contents* (which add/replace/remove succeeded)
//! are checked through the CP's public [`ChargePoint::local_list`] accessor,
//! since the in-process CP is the one under test.
//!
//! Rust counterpart of the Python reference's central system driving
//! `SendLocalList` / `GetLocalListVersion`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against the default `@on` charge point.

use std::net::SocketAddr;
use std::sync::Arc;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    RegistrationStatus, StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::common::{AuthorizationStatus, IdTagInfo};
use ocpp_types::v16j::{AuthorizationData, ConfigurationStatus, UpdateStatus, UpdateType};

/// A minimal CSMS dispatcher: just enough to boot a CP and keep it quiet. The
/// Local Authorization List commands flow CS→CP, so the CSMS needs no extra
/// handlers.
fn csms_dispatcher() -> ActionDispatcher {
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

fn entry(id: &str, status: AuthorizationStatus) -> AuthorizationData {
    AuthorizationData {
        id_tag: id.to_string(),
        id_tag_info: Some(IdTagInfo {
            status,
            parent_id_tag: None,
            expiry_date: None,
        }),
    }
}

fn delete(id: &str) -> AuthorizationData {
    AuthorizationData {
        id_tag: id.to_string(),
        id_tag_info: None,
    }
}

/// A capacity-bounded CP reports its `LocalAuthListMaxLength` (read-only) and
/// rejects any `SendLocalList` that would push the list over that limit
/// (OCPP 1.6J §9), leaving the list untouched on rejection — proven black-box
/// over a real CP↔CSMS loop.
#[tokio::test]
async fn csms_local_list_capacity_is_bounded_and_reported() {
    let cp_id = "CP_LOCALLIST_CAP";
    let (mut server, addr) = start_csms(csms_dispatcher()).await;

    // A CP whose Local Authorization List holds at most two entries.
    let cp = ChargePoint::new(ChargePointConfig {
        local_auth_list_max_length: 2,
        ..cp_config(addr, cp_id)
    })
    .expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(server.is_cp_connected(cp_id));

    // 1. GetConfiguration reports the capacity, read-only.
    let resp = server
        .get_configuration(cp_id, Some(vec!["LocalAuthListMaxLength".to_string()]))
        .await
        .expect("GetConfiguration resolves");
    let keys = resp.configuration_keys.expect("known key returned");
    let max_len = keys
        .iter()
        .find(|k| k.key == "LocalAuthListMaxLength")
        .expect("LocalAuthListMaxLength is reported");
    assert_eq!(
        max_len.value.as_deref(),
        Some("2"),
        "the reported limit matches the configured capacity"
    );
    assert_eq!(
        max_len.readonly,
        Some(true),
        "LocalAuthListMaxLength is a read-only capability"
    );

    // 2. A full update over the limit is rejected and changes nothing.
    let over = server
        .send_local_list(
            cp_id,
            1,
            UpdateType::Full,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
                entry("TAG-C", AuthorizationStatus::Accepted),
            ],
        )
        .await
        .expect("SendLocalList(over-capacity Full) resolves");
    assert_eq!(
        over,
        UpdateStatus::Failed,
        "a full update exceeding LocalAuthListMaxLength is rejected"
    );
    assert_eq!(
        server.get_local_list_version(cp_id).await.unwrap(),
        0,
        "a rejected update leaves the version at 0"
    );
    assert!(cp.local_list().is_empty(), "nothing was applied");

    // 3. A full update at the limit is accepted.
    let at = server
        .send_local_list(
            cp_id,
            2,
            UpdateType::Full,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Accepted),
            ],
        )
        .await
        .expect("SendLocalList(at-capacity Full) resolves");
    assert_eq!(
        at,
        UpdateStatus::Accepted,
        "a full update at the limit fits"
    );
    assert_eq!(cp.local_list().len(), 2);

    // 4. A differential that would add a third entry is rejected, untouched.
    let diff_over = server
        .send_local_list(
            cp_id,
            3,
            UpdateType::Differential,
            vec![entry("TAG-C", AuthorizationStatus::Accepted)],
        )
        .await
        .expect("SendLocalList(over-capacity Differential) resolves");
    assert_eq!(
        diff_over,
        UpdateStatus::Failed,
        "a differential update over the limit is rejected"
    );
    assert_eq!(
        server.get_local_list_version(cp_id).await.unwrap(),
        2,
        "the rejected differential did not advance the version"
    );
    assert_eq!(cp.local_list().len(), 2, "nothing partially applied");
    assert_eq!(cp.local_list().get("TAG-C"), None);

    // 5. A read-only LocalAuthListMaxLength cannot be changed by the CSMS.
    let change = server
        .change_configuration(cp_id, "LocalAuthListMaxLength", "9")
        .await
        .expect("ChangeConfiguration resolves");
    assert_eq!(
        change,
        ConfigurationStatus::Rejected,
        "LocalAuthListMaxLength is read-only"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn csms_manages_cp_local_authorization_list() {
    let cp_id = "CP_LOCALLIST_01";
    let (mut server, addr) = start_csms(csms_dispatcher()).await;

    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // 1. A fresh CP reports an empty list at version 0.
    let v0 = server
        .get_local_list_version(cp_id)
        .await
        .expect("GetLocalListVersion resolves");
    assert_eq!(v0, 0, "a fresh CP has an empty list at version 0");

    // 2. Full update installs two entries and bumps the version.
    let full = server
        .send_local_list(
            cp_id,
            5,
            UpdateType::Full,
            vec![
                entry("TAG-A", AuthorizationStatus::Accepted),
                entry("TAG-B", AuthorizationStatus::Blocked),
            ],
        )
        .await
        .expect("SendLocalList(Full) resolves");
    assert_eq!(
        full,
        UpdateStatus::Accepted,
        "a well-formed full update is accepted"
    );

    let v1 = server
        .get_local_list_version(cp_id)
        .await
        .expect("GetLocalListVersion resolves");
    assert_eq!(v1, 5, "the full update set the version to its listVersion");
    assert_eq!(
        cp.local_list().len(),
        2,
        "the full update installed two entries"
    );
    assert_eq!(
        cp.local_list().get("TAG-A").map(|i| i.status),
        Some(AuthorizationStatus::Accepted)
    );
    assert_eq!(
        cp.local_list().get("TAG-B").map(|i| i.status),
        Some(AuthorizationStatus::Blocked)
    );

    // 3. Differential update: replace TAG-A, add TAG-C, remove TAG-B.
    let diff = server
        .send_local_list(
            cp_id,
            6,
            UpdateType::Differential,
            vec![
                entry("TAG-A", AuthorizationStatus::Blocked),
                entry("TAG-C", AuthorizationStatus::Accepted),
                delete("TAG-B"),
            ],
        )
        .await
        .expect("SendLocalList(Differential) resolves");
    assert_eq!(
        diff,
        UpdateStatus::Accepted,
        "a forward differential update is accepted"
    );

    let v2 = server
        .get_local_list_version(cp_id)
        .await
        .expect("GetLocalListVersion resolves");
    assert_eq!(v2, 6, "the differential update advanced the version");
    assert_eq!(cp.local_list().len(), 2, "TAG-B removed, TAG-C added");
    assert_eq!(
        cp.local_list().get("TAG-A").map(|i| i.status),
        Some(AuthorizationStatus::Blocked),
        "TAG-A was replaced"
    );
    assert_eq!(cp.local_list().get("TAG-B"), None, "TAG-B was removed");
    assert!(cp.local_list().get("TAG-C").is_some(), "TAG-C was added");

    // 4. A stale differential update (version not advancing) is rejected with
    //    VersionMismatch and leaves the list untouched.
    let stale = server
        .send_local_list(
            cp_id,
            6,
            UpdateType::Differential,
            vec![entry("TAG-Z", AuthorizationStatus::Accepted)],
        )
        .await
        .expect("SendLocalList(stale) resolves");
    assert_eq!(
        stale,
        UpdateStatus::VersionMismatch,
        "a non-advancing differential update is a version mismatch"
    );

    let v3 = server
        .get_local_list_version(cp_id)
        .await
        .expect("GetLocalListVersion resolves");
    assert_eq!(v3, 6, "a rejected update does not change the version");
    assert_eq!(
        cp.local_list().get("TAG-Z"),
        None,
        "a rejected update applies nothing"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
