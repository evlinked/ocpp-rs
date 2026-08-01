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
//!
//! The 2.0.1 twin (Issue #418) mirrors this for a `for_version(V201)` CP against
//! a `central_system_service_v201` CSMS, exercising the version-aware runtime
//! (2.0.1 `BootNotification` / `StatusNotification`, response normalization)
//! landed in #419 end to end.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::RegistrationStatus;
use ocpp_messages::v201::{
    BootNotificationRequest as V201BootNotificationRequest,
    BootNotificationResponse as V201BootNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{
    central_system_dispatcher, central_system_dispatcher_with, central_system_service_v201,
    CentralSystemConfig, CentralSystemConfigV201, DispatchHandler, TransportConfig,
};
use ocpp_types::v201::{BootReasonEnumType, RegistrationStatusEnumType};
use ocpp_types::OcppVersion;

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

// ---------------------------------------------------------------------------
// OCPP 2.0.1 twin (Issue #418): the same boot handshake, but a
// `for_version(V201)` ChargePoint against a `central_system_service_v201` CSMS.
// Both directions are `SchemaValidator::v201()`-backed, so a green `connect()`
// proves the CP negotiated `ocpp2.0.1`, emitted a schema-valid 2.0.1
// `BootNotification`, parsed the 2.0.1 `BootNotificationResponse`, and announced
// its connectors with the 2.0.1 `StatusNotification` shape — end to end. This is
// the regression guard for the version-aware runtime landed in #419, which had
// no CP<->CSMS integration coverage of its own.
// ---------------------------------------------------------------------------

/// Start an in-process 2.0.1 CSMS (`central_system_service_v201`, both
/// directions `SchemaValidator::v201()`-backed) and return it with its bound
/// address (random free port). `customize` runs against the default lifecycle
/// dispatcher before it is shared into the handler; pass `|_| {}` for the
/// pure-defaults CSMS.
async fn start_v201_csms(
    customize: impl FnOnce(&mut ActionDispatcher),
) -> (OcppServer, SocketAddr) {
    let (mut server, _events) = central_system_service_v201(
        CentralSystemConfigV201::default(),
        TransportConfig::default(),
        customize,
    );
    server
        .start("127.0.0.1:0")
        .await
        .expect("v201 server start");
    let addr = server.local_addr().expect("v201 server local addr");
    (server, addr)
}

/// A `for_version(V201)` config pointed at `addr` (so it offers `ocpp2.0.1` and
/// speaks the 2.0.1 runtime), with background reconnect disabled to keep the
/// test deterministic.
fn v201_cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    ChargePointConfig {
        charge_point_id: id.to_string(),
        central_system_url: format!("ws://{addr}"),
        auto_reconnect: false,
        ..ChargePointConfig::for_version(OcppVersion::V201)
    }
}

#[tokio::test]
async fn v201_charge_point_boots_against_default_v201_central_system() {
    let (mut server, addr) = start_v201_csms(|_| {}).await;

    let cp = ChargePoint::new(v201_cp_config(addr, "CP201_BOOT")).expect("build v201 charge point");

    // connect() drives the full 2.0.1 boot handshake internally — negotiate
    // `ocpp2.0.1`, send the v201 `BootNotification`, process the v201
    // `BootNotificationResponse` (Accepted -> start heartbeat), then announce
    // every connector as Available via the 2.0.1 `StatusNotification` shape. It
    // only returns Ok once the CSMS has accepted the CP *and* acknowledged the
    // connector announcements, so a green connect exercises the whole path.
    cp.connect().await.expect("v201 connect + boot sequence");

    assert!(
        cp.is_connected().await,
        "v201 CP should be connected after boot"
    );
    assert_eq!(
        cp.registration_status().await,
        // boot_sequence normalizes the 2.0.1 `RegistrationStatusEnumType` onto
        // the canonical 1.6J `RegistrationStatus`, so Accepted reads through.
        RegistrationStatus::Accepted,
        "default v201 CSMS must accept the 2.0.1 BootNotification"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn v201_boot_notification_carries_spec_shape_over_the_wire() {
    // Capture the `BootNotification` the CSMS actually receives, so we assert the
    // *runtime* emits the correct 2.0.1 wire shape end to end — not just the
    // slice-1 builder in isolation. The request reaching this handler has already
    // passed the server's inbound `SchemaValidator::v201()`, so it is schema-valid
    // by construction; here we pin the identity mapping and the boot reason.
    let received: Arc<Mutex<Option<V201BootNotificationRequest>>> = Arc::new(Mutex::new(None));

    let received_for_handler = received.clone();
    let (mut server, addr) = start_v201_csms(move |dispatcher| {
        let received = received_for_handler;
        dispatcher.on(move |req: V201BootNotificationRequest| {
            let received = received.clone();
            async move {
                *received.lock().expect("capture mutex not poisoned") = Some(req);
                Ok(V201BootNotificationResponse {
                    // Mirror the default handler's timestamp format so the
                    // server's outbound v201 validation is satisfied.
                    current_time: chrono::Utc::now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    interval: 300,
                    status: RegistrationStatusEnumType::Accepted,
                    status_info: None,
                    custom_data: None,
                })
            }
        });
    })
    .await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP201_SHAPE")).expect("build v201 charge point");
    cp.connect().await.expect("v201 connect + boot sequence");

    // connect() only returns after the boot CALLRESULT, which the capturing
    // handler produces *after* storing the request, so this is populated with no
    // timing dependency (no sleep / poll).
    let boot = received
        .lock()
        .expect("capture mutex not poisoned")
        .clone()
        .expect("CSMS should have received a 2.0.1 BootNotification");

    assert_eq!(
        boot.reason,
        BootReasonEnumType::PowerUp,
        "a fresh-boot simulator announces reason = PowerUp"
    );
    // Identity maps from the default 1.6J `ChargePointVendorInfo` onto the 2.0.1
    // `ChargingStationType` (see `ChargePointConfig::v201_boot_notification_request`).
    assert_eq!(boot.charging_station.vendor_name, "OCPP-RS");
    assert_eq!(boot.charging_station.model, "Simulator");
    assert_eq!(
        boot.charging_station.serial_number.as_deref(),
        Some("SIM001")
    );
    assert_eq!(
        boot.charging_station.firmware_version.as_deref(),
        Some("1.0.0")
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
