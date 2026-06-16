//! End-to-end: a real [`ChargePoint`] boots against an in-process CSMS backed
//! by the default central-system handler set (Issue #40).
//!
//! This wires the two halves the M2/M4 milestones care about: the
//! `central_system_dispatcher()` "batteries included" responders
//! (`crates/ocpp-transport/src/central_system.rs`) and the charge-point boot
//! sequence (`ChargePoint::connect`). It proves that a CP connecting to a CSMS
//! that only has the default handlers completes its BootNotification handshake
//! and ends up `Accepted` — i.e. the defaults are genuinely spec-valid and the
//! routing introduced in #39 carries the frames end to end.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::RegistrationStatus;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{
    central_system_dispatcher, central_system_dispatcher_with, CentralSystemConfig,
    DispatchHandler, TransportConfig,
};

/// Start an in-process CSMS whose dispatcher is built from `dispatcher` and
/// return it alongside the bound address (random free port).
async fn start_csms(dispatcher: ocpp_messages::ActionDispatcher) -> (OcppServer, SocketAddr) {
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
        // Keep the test deterministic: no background reconnect storm.
        auto_reconnect: false,
        ..ChargePointConfig::default()
    }
}

#[tokio::test]
async fn charge_point_boots_against_default_central_system() {
    let (mut server, addr) = start_csms(central_system_dispatcher()).await;

    let cp = ChargePoint::new(cp_config(addr, "CP_BOOT_01")).expect("build charge point");

    // connect() runs the boot sequence (BootNotification -> Accepted) internally
    // and only returns Ok once the CSMS has accepted the charge point.
    cp.connect().await.expect("connect + boot sequence");

    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert_eq!(
        cp.registration_status().await,
        RegistrationStatus::Accepted,
        "default CSMS must accept the BootNotification"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn charge_point_boot_rejected_when_csms_configured_to_reject() {
    // A CSMS that always Rejects, with a short retry interval so the CP's
    // bounded retry loop gives up quickly instead of sleeping for the default.
    let dispatcher = central_system_dispatcher_with(CentralSystemConfig {
        boot_interval: 1,
        registration_status: RegistrationStatus::Rejected,
    });
    let (mut server, addr) = start_csms(dispatcher).await;

    // max_boot_retries: 0 -> a single attempt, no waiting between retries.
    let config = ChargePointConfig {
        max_boot_retries: 0,
        ..cp_config(addr, "CP_BOOT_REJECT")
    };
    let cp = ChargePoint::new(config).expect("build charge point");

    // Boot is rejected, so connect() surfaces the failure rather than hanging.
    let result = tokio::time::timeout(Duration::from_secs(10), cp.connect()).await;
    let connect_result = result.expect("connect should resolve, not hang");
    assert!(
        connect_result.is_err(),
        "connect must fail when the CSMS rejects the BootNotification"
    );
    assert_eq!(
        cp.registration_status().await,
        RegistrationStatus::Rejected,
        "registration status should reflect the rejection"
    );

    server.stop().await.expect("server stop");
}
