//! End-to-end CSMS→CP `DataTransfer` routing test (Issue #101).
//!
//! Wires a real `OcppServer` (CSMS) and `ChargePoint` together over a loopback
//! WebSocket and drives the OCPP 1.6J `DataTransfer` command (§6.x) through the
//! server's generic `call`, asserting the CP routes each request through its
//! vendor handler registry to the faithful [`DataTransferStatus`]:
//!
//! - an **unregistered** `vendorId` → `UnknownVendorId`,
//! - a **known** `vendorId` with an unregistered `messageId` → `UnknownMessageId`,
//! - a **registered** handler → `Accepted` (echoing `data`) or `Rejected`,
//! - a CP with **no** handlers registered → `UnknownVendorId` for everything.
//!
//! Rust counterpart of the Python reference's central system driving
//! `DataTransfer`
//! ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py))
//! against a charge point that supplies a `@on('DataTransfer')` handler.

use std::net::SocketAddr;
use std::sync::Arc;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, DataTransferRequest, DataTransferResponse,
    HeartbeatRequest, HeartbeatResponse, RegistrationStatus, StatusNotificationRequest,
    StatusNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::v16j::DataTransferStatus;

const VENDOR: &str = "com.evlinked.test";

/// Minimal CSMS dispatcher — just enough to accept the CP's boot handshake and
/// heartbeats. `DataTransfer` flows CSMS→CP, so the CSMS needs no handler for it.
fn csms_dispatcher() -> ActionDispatcher {
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
    d
}

async fn start_csms() -> (OcppServer, SocketAddr) {
    let handler = Arc::new(DispatchHandler::new(Arc::new(csms_dispatcher())));
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

async fn connect_cp(cp: &ChargePoint, server: &OcppServer, cp_id: &str) {
    cp.connect().await.expect("connect + boot sequence");
    assert!(cp.is_connected().await, "CP should be connected after boot");
    assert!(
        server.is_cp_connected(cp_id),
        "the CSMS must be able to route CALLs to the booted CP"
    );
}

fn data_transfer(vendor: &str, message: Option<&str>, data: Option<&str>) -> DataTransferRequest {
    DataTransferRequest {
        vendor_id: vendor.to_string(),
        message_id: message.map(str::to_string),
        data: data.map(str::to_string),
    }
}

#[tokio::test]
async fn data_transfer_routes_through_vendor_registry() {
    let cp_id = "CP_DATATRANSFER_01";
    let (mut server, addr) = start_csms().await;

    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");

    // A "Ping" handler that accepts and echoes the request data, and a "Drop"
    // handler that rejects. Registered before connecting — the registry is the
    // same instance the live inbound handler consults.
    cp.register_data_transfer_handler(VENDOR, Some("Ping".into()), |req: &DataTransferRequest| {
        DataTransferResponse {
            status: DataTransferStatus::Accepted,
            data: req.data.clone(),
        }
    });
    cp.register_data_transfer_handler(VENDOR, Some("Drop".into()), |_req| DataTransferResponse {
        status: DataTransferStatus::Rejected,
        data: None,
    });

    connect_cp(&cp, &server, cp_id).await;

    // 1. Unregistered vendor → UnknownVendorId.
    let resp: DataTransferResponse = server
        .call(cp_id, data_transfer("com.unknown", Some("Ping"), None))
        .await
        .expect("call resolves");
    assert_eq!(resp.status, DataTransferStatus::UnknownVendorId);

    // 2. Known vendor, unregistered messageId → UnknownMessageId.
    let resp: DataTransferResponse = server
        .call(cp_id, data_transfer(VENDOR, Some("Nope"), None))
        .await
        .expect("call resolves");
    assert_eq!(resp.status, DataTransferStatus::UnknownMessageId);

    // 3. Registered handler accepts and echoes the data.
    let resp: DataTransferResponse = server
        .call(cp_id, data_transfer(VENDOR, Some("Ping"), Some("hello")))
        .await
        .expect("call resolves");
    assert_eq!(resp.status, DataTransferStatus::Accepted);
    assert_eq!(resp.data.as_deref(), Some("hello"));

    // 4. Registered handler rejects.
    let resp: DataTransferResponse = server
        .call(cp_id, data_transfer(VENDOR, Some("Drop"), None))
        .await
        .expect("call resolves");
    assert_eq!(resp.status, DataTransferStatus::Rejected);

    // 5. Known vendor but the request omits messageId, and no vendor-default
    //    handler was registered → UnknownMessageId.
    let resp: DataTransferResponse = server
        .call(cp_id, data_transfer(VENDOR, None, None))
        .await
        .expect("call resolves");
    assert_eq!(resp.status, DataTransferStatus::UnknownMessageId);

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn data_transfer_defaults_to_unknown_vendor_with_no_handlers() {
    let cp_id = "CP_DATATRANSFER_02";
    let (mut server, addr) = start_csms().await;

    // No handlers registered — the spec-faithful default for an unimplemented
    // vendor is UnknownVendorId, even for a request that carries no messageId.
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    connect_cp(&cp, &server, cp_id).await;

    let resp: DataTransferResponse = server
        .call(cp_id, data_transfer(VENDOR, Some("Ping"), Some("data")))
        .await
        .expect("call resolves");
    assert_eq!(resp.status, DataTransferStatus::UnknownVendorId);

    let resp: DataTransferResponse = server
        .call(cp_id, data_transfer(VENDOR, None, None))
        .await
        .expect("call resolves");
    assert_eq!(resp.status, DataTransferStatus::UnknownVendorId);

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
