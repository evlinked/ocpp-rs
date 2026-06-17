//! End-to-end CS→CP configuration-management test (Issue #58).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the two M5 configuration commands through the
//! `OcppServer::get_configuration` / `change_configuration` helpers, asserting
//! the CP answers from its `ConfigurationStore` with faithful semantics
//! (OCPP 1.6J §5.3 / §5.8): all-key reads, targeted reads that split known
//! from unknown keys, a write→read round-trip, and read-only rejection.
//!
//! Rust counterpart of the Python reference's central system driving
//! `GetConfiguration` / `ChangeConfiguration`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against the default `@on('GetConfiguration')` / `@on('ChangeConfiguration')`
//! charge point.

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
use ocpp_types::v16j::ConfigurationStatus;

/// A CSMS dispatcher that accepts the CP-originated actions the boot handshake
/// needs, so the CP reaches the connected state this test drives commands into.
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
        // No background reconnect storm racing the assertions.
        auto_reconnect: false,
        // Keep the meter sampler quiet so it doesn't interleave with the asserts.
        meter_values_interval: 3600,
        ..ChargePointConfig::default()
    }
}

#[tokio::test]
async fn csms_drives_get_and_change_configuration() {
    let cp_id = "CP_CONFIG_01";
    let (mut server, addr) = start_csms(csms_dispatcher()).await;

    // Connect a real charge point and run its boot handshake.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );

    // 1. GetConfiguration with no keys → the CP returns its whole key set
    //    (no unknown keys), including the well-known HeartbeatInterval (§5.8).
    let resp = server
        .get_configuration(cp_id, None)
        .await
        .expect("get_configuration(all) resolves");
    let all_keys = resp
        .configuration_keys
        .expect("a full read returns configurationKey");
    assert!(
        all_keys.iter().any(|k| k.key == "HeartbeatInterval"),
        "the default store advertises HeartbeatInterval"
    );
    assert!(
        resp.unknown_keys.is_none(),
        "a full read reports no unknown keys"
    );
    // NumberOfConnectors is a read-only key in the default store.
    let num_connectors = all_keys
        .iter()
        .find(|k| k.key == "NumberOfConnectors")
        .expect("NumberOfConnectors is a default key");
    assert_eq!(
        num_connectors.readonly,
        Some(true),
        "NumberOfConnectors is advertised read-only"
    );

    // 2. Targeted read splits known from unknown keys (§5.8).
    let resp = server
        .get_configuration(
            cp_id,
            Some(vec![
                "HeartbeatInterval".to_string(),
                "NoSuchKey".to_string(),
            ]),
        )
        .await
        .expect("get_configuration(targeted) resolves");
    let known = resp.configuration_keys.expect("known keys present");
    assert_eq!(known.len(), 1, "only HeartbeatInterval is known");
    assert_eq!(known[0].key, "HeartbeatInterval");
    assert_eq!(
        resp.unknown_keys.as_deref(),
        Some(&["NoSuchKey".to_string()][..]),
        "the bogus key is reported as unknown"
    );

    // 3. Write → read round-trip: change a writable key, then read it back (§5.3).
    let status = server
        .change_configuration(cp_id, "HeartbeatInterval", "120")
        .await
        .expect("change_configuration resolves");
    assert_eq!(
        status,
        ConfigurationStatus::Accepted,
        "a writable key accepts the change"
    );
    let resp = server
        .get_configuration(cp_id, Some(vec!["HeartbeatInterval".to_string()]))
        .await
        .expect("read-back resolves");
    let key = resp
        .configuration_keys
        .and_then(|mut k| k.pop())
        .expect("HeartbeatInterval read back");
    assert_eq!(
        key.value.as_deref(),
        Some("120"),
        "the new value is persisted in the CP's ConfigurationStore"
    );

    // 4. A read-only key rejects the change (§5.3).
    let status = server
        .change_configuration(cp_id, "NumberOfConnectors", "9")
        .await
        .expect("change_configuration resolves");
    assert_eq!(
        status,
        ConfigurationStatus::Rejected,
        "a read-only key rejects the change"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
