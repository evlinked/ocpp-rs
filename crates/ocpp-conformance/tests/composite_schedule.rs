//! End-to-end conformance for GetCompositeSchedule (Issue #95, OCPP 1.6J Smart
//! Charging profile §5.x).
//!
//! `GetCompositeSchedule` asks the CP to compute the *effective* charging
//! schedule for a connector over a requested window, combining the profiles
//! installed via `SetChargingProfile` per the 1.6J stacking rules. These tests
//! drive a real CSMS over a loopback to a real charge point: install profiles
//! with `SetChargingProfile`, then assert the composite the CP computes and
//! returns (status, reported connector, and the combined/capped schedule).
//!
//! The Python reference ships only the wire types (its example CP returns a
//! canned response), so the composite computation follows the 1.6J spec.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_messages::v16j::{
    BootNotificationRequest, BootNotificationResponse, HeartbeatRequest, HeartbeatResponse,
    RegistrationStatus, StatusNotificationRequest, StatusNotificationResponse,
};
use ocpp_messages::ActionDispatcher;
use ocpp_transport::server::OcppServer;
use ocpp_transport::{DispatchHandler, TransportConfig};
use ocpp_types::v16j::{
    ChargingProfile, ChargingProfileKindType, ChargingProfilePurposeType, ChargingRateUnitType,
    ChargingSchedule, ChargingSchedulePeriod, GetCompositeScheduleStatus, RecurrencyKindType,
};

/// A minimal CSMS dispatcher — the Smart Charging commands flow CSMS→CP, so the
/// CSMS needs no handlers beyond enough to let the CP boot.
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

/// A single-period profile capping the rate at `limit` in `unit`.
fn profile(
    id: i32,
    stack_level: i32,
    purpose: ChargingProfilePurposeType,
    limit: f64,
    unit: ChargingRateUnitType,
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
            charging_rate_unit: unit,
            charging_schedule_period: vec![ChargingSchedulePeriod {
                start_period: 0,
                limit,
                number_phases: None,
            }],
            min_charging_rate: None,
        },
    }
}

/// A `Recurring` (Daily) profile with the given periods, anchored at the window
/// start (no `startSchedule`) so the composite is deterministic regardless of
/// the wall-clock time at which the test runs.
fn daily_profile(
    id: i32,
    purpose: ChargingProfilePurposeType,
    periods: Vec<ChargingSchedulePeriod>,
) -> ChargingProfile {
    ChargingProfile {
        charging_profile_id: id,
        transaction_id: None,
        stack_level: 0,
        charging_profile_purpose: purpose,
        charging_profile_kind: ChargingProfileKindType::Recurring,
        recurrency_kind: Some(RecurrencyKindType::Daily),
        valid_from: None,
        valid_to: None,
        charging_schedule: ChargingSchedule {
            duration: None,
            start_schedule: None,
            charging_rate_unit: ChargingRateUnitType::A,
            charging_schedule_period: periods,
            min_charging_rate: None,
        },
    }
}

const TIMEOUT: Duration = Duration::from_secs(5);

/// With a default profile capped by a CP-wide max profile, the composite
/// reflects the more restrictive ceiling, and is reported for the connector.
#[tokio::test]
async fn composite_combines_default_and_cp_max() {
    let cp_id = "CP_GCS_01";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");
    assert!(server.is_cp_connected(cp_id), "CSMS must route to the CP");

    // Connector 1 wants 32 A; the CP-wide ceiling (connector 0) is 16 A.
    server
        .set_charging_profile(
            cp_id,
            1,
            profile(
                10,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                32.0,
                ChargingRateUnitType::A,
            ),
        )
        .await
        .expect("set default");
    server
        .set_charging_profile(
            cp_id,
            0,
            profile(
                1,
                0,
                ChargingProfilePurposeType::ChargePointMaxProfile,
                16.0,
                ChargingRateUnitType::A,
            ),
        )
        .await
        .expect("set cp max");

    let resp = tokio::time::timeout(TIMEOUT, server.get_composite_schedule(cp_id, 1, 3600, None))
        .await
        .expect("resolves in time")
        .expect("resolves");

    assert_eq!(resp.status, GetCompositeScheduleStatus::Accepted);
    assert_eq!(resp.connector_id, Some(1));
    assert!(resp.schedule_start.is_some());
    let sched = resp.charging_schedule.expect("schedule present");
    assert_eq!(sched.charging_rate_unit, ChargingRateUnitType::A);
    assert_eq!(sched.charging_schedule_period.len(), 1);
    assert_eq!(sched.charging_schedule_period[0].limit, 16.0);

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// A requested `chargingRateUnit` of W is honored, converting an Amps profile.
#[tokio::test]
async fn composite_honors_requested_unit() {
    let cp_id = "CP_GCS_02";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");

    let mut p = profile(
        10,
        0,
        ChargingProfilePurposeType::TxDefaultProfile,
        16.0,
        ChargingRateUnitType::A,
    );
    // single phase so the nominal conversion is deterministic
    p.charging_schedule.charging_schedule_period[0].number_phases = Some(1);
    server
        .set_charging_profile(cp_id, 1, p)
        .await
        .expect("set default");

    let resp = server
        .get_composite_schedule(cp_id, 1, 3600, Some(ChargingRateUnitType::W))
        .await
        .expect("resolves");

    assert_eq!(resp.status, GetCompositeScheduleStatus::Accepted);
    let sched = resp.charging_schedule.expect("schedule present");
    assert_eq!(sched.charging_rate_unit, ChargingRateUnitType::W);
    // 16 A · 230 V · 1 phase = 3680 W.
    assert!((sched.charging_schedule_period[0].limit - 3680.0).abs() < 1.0);

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// No installed profile → Rejected with no schedule; an unknown connector is
/// also Rejected.
#[tokio::test]
async fn composite_rejects_when_no_profile_or_unknown_connector() {
    let cp_id = "CP_GCS_03";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");

    // No profiles installed yet → Rejected, no schedule.
    let resp = server
        .get_composite_schedule(cp_id, 1, 3600, None)
        .await
        .expect("resolves");
    assert_eq!(resp.status, GetCompositeScheduleStatus::Rejected);
    assert!(resp.charging_schedule.is_none());

    // Install a profile, but ask about a connector that does not exist.
    server
        .set_charging_profile(
            cp_id,
            1,
            profile(
                10,
                0,
                ChargingProfilePurposeType::TxDefaultProfile,
                16.0,
                ChargingRateUnitType::A,
            ),
        )
        .await
        .expect("set default");
    let resp = server
        .get_composite_schedule(cp_id, 7, 3600, None)
        .await
        .expect("resolves");
    assert_eq!(
        resp.status,
        GetCompositeScheduleStatus::Rejected,
        "an unknown connector id is rejected"
    );

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

#[tokio::test]
async fn composite_reports_cp_wide_schedule_for_connector_zero() {
    let cp_id = "CP_GCS_04";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");

    server
        .set_charging_profile(
            cp_id,
            0,
            profile(
                1,
                0,
                ChargingProfilePurposeType::ChargePointMaxProfile,
                20.0,
                ChargingRateUnitType::A,
            ),
        )
        .await
        .expect("set cp max");

    let resp = server
        .get_composite_schedule(cp_id, 0, 1800, None)
        .await
        .expect("resolves");
    assert_eq!(resp.status, GetCompositeScheduleStatus::Accepted);
    assert_eq!(resp.connector_id, Some(0));
    let sched = resp.charging_schedule.expect("schedule present");
    assert_eq!(sched.charging_schedule_period[0].limit, 20.0);
    assert_eq!(sched.duration, Some(1800));

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}

/// A `Recurring` (Daily) profile is unrolled across a multi-day window: the
/// CP repeats the daily pattern for every occurrence overlapping the request,
/// not just the first.
#[tokio::test]
async fn composite_unrolls_daily_recurring_profile() {
    let cp_id = "CP_GCS_05";
    let (mut server, addr) = start_csms().await;
    let cp = ChargePoint::new(cp_config(addr, cp_id)).expect("build charge point");
    cp.connect().await.expect("connect + boot sequence");

    // 16 A for the first 8 h of each day, 8 A for the rest — repeated daily.
    server
        .set_charging_profile(
            cp_id,
            1,
            daily_profile(
                20,
                ChargingProfilePurposeType::TxDefaultProfile,
                vec![
                    ChargingSchedulePeriod {
                        start_period: 0,
                        limit: 16.0,
                        number_phases: None,
                    },
                    ChargingSchedulePeriod {
                        start_period: 28_800,
                        limit: 8.0,
                        number_phases: None,
                    },
                ],
            ),
        )
        .await
        .expect("set recurring default");

    // Two full days: the daily pattern must appear twice, stepped by 86 400 s.
    let resp = tokio::time::timeout(
        TIMEOUT,
        server.get_composite_schedule(cp_id, 1, 172_800, None),
    )
    .await
    .expect("resolves in time")
    .expect("resolves");

    assert_eq!(resp.status, GetCompositeScheduleStatus::Accepted);
    let sched = resp.charging_schedule.expect("schedule present");
    let ps = &sched.charging_schedule_period;
    assert_eq!(ps.len(), 4, "the daily pattern repeats across both days");
    assert_eq!((ps[0].start_period, ps[0].limit), (0, 16.0));
    assert_eq!((ps[1].start_period, ps[1].limit), (28_800, 8.0));
    assert_eq!((ps[2].start_period, ps[2].limit), (86_400, 16.0));
    assert_eq!((ps[3].start_period, ps[3].limit), (115_200, 8.0));

    cp.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
