//! End-to-end 2.0.1 provisioning handshake: a `V201`-configured
//! [`ChargePoint`] boots against a real [`OcppServer`] wired with the
//! batteries-included 2.0.1 CSMS ([`central_system_service_v201`], backed by
//! `SchemaValidator::v201()`).
//!
//! Ports the connect→boot→heartbeat lifecycle of the mobilityhouse/ocpp
//! reference client
//! [`examples/v201/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v201/charge_point.py),
//! which offers the `ocpp2.0.1` subprotocol and sends a 2.0.1 `BootNotification`
//! (`chargingStation` identity + `reason = "PowerUp"`) before starting its
//! heartbeat.
//!
//! ## Why an e2e test (issue #418, slice 2)
//!
//! Slice 1 (#417/#419) added the *foundation* for a 2.0.1-speaking `ocpp-cp`
//! (the `protocol_version` field, `for_version()`, a schema-valid
//! `v201_boot_notification_request()` builder) but left the live runtime loop
//! speaking 1.6J. Slice 2 wires `protocol_version` through the boot/status
//! runtime. Unit tests in `ocpp-cp` pin the pure mappings; this test proves the
//! whole path end-to-end: a `V201` CP's `connect()` negotiates `ocpp2.0.1`,
//! frames and sends a **schema-valid** 2.0.1 `BootNotification` (validated on
//! *both* sides — the CP's own outgoing validator and the CSMS's
//! `SchemaValidator::v201()`), processes the 2.0.1 `BootNotificationResponse`,
//! and reaches `Accepted`.
//!
//! This mirrors the existing 1.6J CP↔CSMS loopback e2e
//! (`charge_point_call_e2e.rs`) and the 2.0.1 server round-trip from #342, but
//! drives the **charge-point** side of a 2.0.1 boot for the first time.

use std::net::SocketAddr;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::RegistrationStatus;
use ocpp_transport::{central_system_service_v201, CentralSystemConfigV201, TransportConfig};
use ocpp_types::OcppVersion;

/// Stand up an in-process 2.0.1 CSMS on a loopback port. The default handler set
/// answers `BootNotification` with `Accepted` + a long heartbeat interval (so no
/// background `Heartbeat` races the assertions) and validates every frame
/// against `SchemaValidator::v201()`.
async fn start_v201_csms() -> (ocpp_transport::server::OcppServer, SocketAddr) {
    let cs_config = CentralSystemConfigV201 {
        // Long interval: the CP starts its heartbeat task on Accepted; a big
        // value keeps it dormant for the life of the test.
        boot_interval: 3600,
        ..CentralSystemConfigV201::default()
    };
    let (mut server, _events) =
        central_system_service_v201(cs_config, TransportConfig::default(), |_d| {});
    server
        .start("127.0.0.1:0")
        .await
        .expect("v201 server start");
    let addr = server.local_addr().expect("v201 server local addr");
    (server, addr)
}

fn v201_cp_config(addr: SocketAddr, id: &str) -> ChargePointConfig {
    // Start from `for_version(V201)` so `protocol_version` AND the offered
    // `ocpp2.0.1` subprotocol are set consistently, then fill in the connection
    // details. NOTE: do not `..Default::default()` here — that would reset the
    // offered subprotocol back to `ocpp1.6`.
    let mut config = ChargePointConfig::for_version(OcppVersion::V201);
    config.charge_point_id = id.to_string();
    config.central_system_url = format!("ws://{addr}");
    config.connector_count = 1;
    config.call_timeout = 5;
    // Deterministic: no background reconnect storm racing the assertions.
    config.auto_reconnect = false;
    config
}

/// A `V201`-configured CP connects, negotiates `ocpp2.0.1`, sends a schema-valid
/// 2.0.1 `BootNotification`, processes the `Accepted` `BootNotificationResponse`,
/// and reaches the `Accepted` registration state — the full slice-2 acceptance
/// criterion.
#[tokio::test]
async fn v201_charge_point_boots_against_v201_csms() {
    let (mut server, addr) = start_v201_csms().await;

    let cp =
        ChargePoint::new(v201_cp_config(addr, "CP_V201_BOOT")).expect("build V201 charge point");

    // `connect()` runs the whole provisioning sequence: WS handshake (offering
    // ocpp2.0.1) → 2.0.1 BootNotification via `call()` (which validates the
    // outgoing request against SchemaValidator::v201()) → parse the 2.0.1
    // BootNotificationResponse → announce connectors (2.0.1 StatusNotification)
    // → start heartbeat. Any schema mismatch on either side fails here.
    cp.connect()
        .await
        .expect("V201 connect + 2.0.1 boot handshake must succeed");

    assert!(
        cp.is_connected().await,
        "CP should be connected after a successful 2.0.1 boot"
    );
    assert_eq!(
        cp.registration_status().await,
        RegistrationStatus::Accepted,
        "the 2.0.1 BootNotificationResponse (Accepted) must be normalized onto \
         the canonical registration state"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("v201 server stop");
}
