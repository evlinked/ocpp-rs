//! End-to-end conformance for SetChargingProfile / ClearChargingProfile
//! (Issue #94, OCPP 1.6J Smart Charging profile §5.16 / §5.2).
//!
//! `SetChargingProfile` installs a [`ChargingProfile`] against a connector (`0`
//! = charge-point-wide); `ClearChargingProfile` removes profiles matching
//! optional filters. These tests drive both from a real CSMS over a loopback to
//! a real charge point and assert the faithful status semantics *and* the CP
//! side effect (the profile actually appears in / disappears from the CP's
//! installed-profile store).
//!
//! Rust counterpart of the Python reference's `SetChargingProfile` /
//! `ClearChargingProfile`
//! ([`ocpp/v16/call.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/call.py),
//! [`ocpp/v16/enums.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v16/enums.py)).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    MeterValuesRequest, MeterValuesResponse, RegistrationStatus, StartTransactionRequest,
    StartTransactionResponse, StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::common::{AuthorizationStatus, IdTagInfo};
use ocpp_types::v16j::{
    ChargingProfile, ChargingProfileKindType, ChargingProfilePurposeType, ChargingProfileStatus,
    ChargingRateUnitType, ChargingSchedule, ChargingSchedulePeriod, ClearChargingProfileStatus,
};
use ocpp_types::ConnectorId;

/// A minimal CSMS dispatcher — the profile commands flow CSMS→CP, so the CSMS
/// needs only enough to let the CP boot and to drive a transaction (so a
/// `TxProfile`'s transaction-scoped acceptance can be exercised). It assigns a
/// fixed `transactionId` and acknowledges the periodic `MeterValues` the CP
/// emits while charging.
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
    d.on(|_req: StartTransactionRequest| async move {
        Ok(StartTransactionResponse {
            id_tag_info: IdTagInfo {
                status: AuthorizationStatus::Accepted,
                parent_id_tag: None,
                expiry_date: None,
            },
            transaction_id: 4242,
        })
    });
    d.on(|_req: MeterValuesRequest| async move { Ok(MeterValuesResponse {}) });
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

/// Build a charging profile with a single-period schedule capping current at
/// `limit` amps.
fn profile(
    id: i32,
    stack_level: i32,
    purpose: ChargingProfilePurposeType,
    limit: f64,
) -> ChargingProfile {
    ChargingProfile {
        charging_profile_id: id,
        transaction_id: None,
        stack_level,
        charging_profile_purpose: purpose,
        charging_profile_kind: ChargingProfileKindType::Absolute,
        recurrency_kind: None,
        valid_from: None,
        valid_to: None,
        charging_schedule: ChargingSchedule {
            duration: None,
            start_schedule: None,
            charging_rate_unit: ChargingRateUnitType::A,
            charging_schedule_period: vec![ChargingSchedulePeriod {
                start_period: 0,
                limit,
                number_phases: None,
            }],
            min_charging_rate: None,
        },
    }
}

const TIMEOUT: Duration = Duration::from_secs(5);

/// SetChargingProfile installs a valid profile (→ Accepted, CP stores it);
/// re-setting the same (purpose, stackLevel) replaces it rather than
/// duplicating; an invalid placement is rejected and nothing is stored.
#[tokio::test]
async fn set_charging_profile_installs_and_validates() {
    let cp_id = "CP_SCP_01";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(server.is_cp_connected(cp_id), "CSMS must route to the CP");

    // 1. Valid TxDefaultProfile on connector 1 → Accepted, stored.
    let status = tokio::time::timeout(
        TIMEOUT,
        server.set_charging_profile(
            cp_id,
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile, 16.0),
        ),
    )
    .await
    .expect("set resolves in time")
    .expect("set resolves");
    assert_eq!(status, ChargingProfileStatus::Accepted);
    let installed = cp.charging_profiles().profiles_for(1);
    assert_eq!(installed.len(), 1, "the profile is stored on connector 1");
    assert_eq!(installed[0].charging_profile_id, 10);

    // 2. A second profile, same (purpose, stackLevel), different id → replaces.
    let status = server
        .set_charging_profile(
            cp_id,
            1,
            profile(11, 0, ChargingProfilePurposeType::TxDefaultProfile, 8.0),
        )
        .await
        .expect("set resolves");
    assert_eq!(status, ChargingProfileStatus::Accepted);
    let installed = cp.charging_profiles().profiles_for(1);
    assert_eq!(
        installed.len(),
        1,
        "the (purpose, stackLevel) slot is replaced"
    );
    assert_eq!(installed[0].charging_profile_id, 11);

    // 3. ChargePointMaxProfile at a real connector → Rejected, not stored.
    let status = server
        .set_charging_profile(
            cp_id,
            1,
            profile(
                12,
                1,
                ChargingProfilePurposeType::ChargePointMaxProfile,
                32.0,
            ),
        )
        .await
        .expect("set resolves");
    assert_eq!(
        status,
        ChargingProfileStatus::Rejected,
        "ChargePointMaxProfile may only target connector 0"
    );
    assert_eq!(
        cp.charging_profiles().profiles_for(1).len(),
        1,
        "the rejected profile is not stored"
    );

    // 4. Any profile at an unknown connector id → Rejected.
    let status = server
        .set_charging_profile(
            cp_id,
            7,
            profile(13, 0, ChargingProfilePurposeType::TxDefaultProfile, 16.0),
        )
        .await
        .expect("set resolves");
    assert_eq!(
        status,
        ChargingProfileStatus::Rejected,
        "an unknown connector id is rejected"
    );
    assert!(cp.charging_profiles().profiles_for(7).is_empty());

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// A `TxProfile` is transaction-scoped (OCPP 1.6J §5.16.1): the CP SHALL reject
/// it on a connector with no ongoing transaction, and accept it once a
/// transaction is running there.
#[tokio::test]
async fn set_tx_profile_requires_active_transaction() {
    let cp_id = "CP_SCP_03";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(server.is_cp_connected(cp_id), "CSMS must route to the CP");

    // 1. TxProfile on idle connector 1 (no transaction) → Rejected, not stored.
    let status = tokio::time::timeout(
        TIMEOUT,
        server.set_charging_profile(
            cp_id,
            1,
            profile(20, 0, ChargingProfilePurposeType::TxProfile, 16.0),
        ),
    )
    .await
    .expect("set resolves in time")
    .expect("set resolves");
    assert_eq!(
        status,
        ChargingProfileStatus::Rejected,
        "a TxProfile on a connector with no active transaction is rejected"
    );
    assert!(
        cp.charging_profiles().profiles_for(1).is_empty(),
        "the rejected TxProfile is not stored"
    );

    // Start a transaction on connector 1 (the CSMS accepts it, assigning a
    // transactionId), so the connector now has an ongoing transaction.
    let connector = ConnectorId::new(1).expect("connector 1");
    let transaction_id = cp
        .start_transaction(connector, "TAG_SCP_03", 0)
        .await
        .expect("transaction starts");
    assert_eq!(transaction_id, 4242);

    // 2. TxProfile on connector 1 with an active transaction → Accepted, stored.
    let status = server
        .set_charging_profile(
            cp_id,
            1,
            profile(21, 0, ChargingProfilePurposeType::TxProfile, 8.0),
        )
        .await
        .expect("set resolves");
    assert_eq!(
        status,
        ChargingProfileStatus::Accepted,
        "a TxProfile on a connector with an active transaction is accepted"
    );
    let installed = cp.charging_profiles().profiles_for(1);
    assert_eq!(installed.len(), 1, "the accepted TxProfile is stored");
    assert_eq!(installed[0].charging_profile_id, 21);

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// ClearChargingProfile removes profiles matching the filters (→ Accepted) and
/// reports Unknown when nothing matches.
#[tokio::test]
async fn clear_charging_profile_filters_and_unknown() {
    let cp_id = "CP_SCP_02";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");

    // Install a CP-wide max profile and two connector-1 profiles. Both
    // connector-1 profiles are TxDefaultProfiles (at distinct stack levels) so
    // they are legitimately Accepted on an idle connector — a TxProfile would be
    // Rejected here since no transaction is active (§5.16.1; see the dedicated
    // `set_tx_profile_requires_active_transaction` test).
    for (cid, p) in [
        (
            0,
            profile(
                1,
                0,
                ChargingProfilePurposeType::ChargePointMaxProfile,
                32.0,
            ),
        ),
        (
            1,
            profile(10, 0, ChargingProfilePurposeType::TxDefaultProfile, 16.0),
        ),
        (
            1,
            profile(11, 1, ChargingProfilePurposeType::TxDefaultProfile, 8.0),
        ),
    ] {
        let status = server
            .set_charging_profile(cp_id, cid, p)
            .await
            .expect("set resolves");
        assert_eq!(status, ChargingProfileStatus::Accepted);
    }
    assert_eq!(cp.charging_profiles().len(), 3);

    // 1. Clear by id → only that profile goes.
    let status = server
        .clear_charging_profile(cp_id, Some(10), None, None, None)
        .await
        .expect("clear resolves");
    assert_eq!(status, ClearChargingProfileStatus::Accepted);
    assert_eq!(cp.charging_profiles().len(), 2);
    assert!(cp
        .charging_profiles()
        .profiles_for(1)
        .iter()
        .all(|p| p.charging_profile_id != 10));

    // 2. Clear with no match → Unknown, nothing removed.
    let status = server
        .clear_charging_profile(cp_id, Some(999), None, None, None)
        .await
        .expect("clear resolves");
    assert_eq!(status, ClearChargingProfileStatus::Unknown);
    assert_eq!(cp.charging_profiles().len(), 2);

    // 3. Clear all remaining (empty filter) → Accepted, store emptied.
    let status = server
        .clear_charging_profile(cp_id, None, None, None, None)
        .await
        .expect("clear resolves");
    assert_eq!(status, ClearChargingProfileStatus::Accepted);
    assert!(cp.charging_profiles().is_empty());

    // 4. Clearing an already-empty store → Unknown.
    let status = server
        .clear_charging_profile(cp_id, None, None, None, None)
        .await
        .expect("clear resolves");
    assert_eq!(status, ClearChargingProfileStatus::Unknown);

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
