//! `GetCompositeSchedule` — the CSMS asks a Charging Station to compute and
//! return the *net* charging schedule it will enforce for an EVSE over a
//! requested window, after stacking every applicable charging profile.
//!
//! Ports `ocpp.v201.call.GetCompositeSchedule` /
//! `ocpp.v201.call_result.GetCompositeSchedule`. The request names the
//! `duration` (seconds) and the `evseId` (`0` for the whole grid connection),
//! optionally forcing a [`ChargingRateUnitEnumType`]. The station replies with a
//! [`GenericStatusEnumType`] and, only when `Accepted`, the computed
//! [`CompositeScheduleType`]. This is the read/query side of the OCPP 2.0.1
//! smart-charging family, completing the CSMS command trio alongside
//! `SetChargingProfile` and `ClearChargingProfile`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ChargingRateUnitEnumType, CompositeScheduleType, CustomDataType, GenericStatusEnumType,
    StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetCompositeSchedule.req` — sent by the CSMS to query the resulting
/// composite charging schedule for an EVSE.
///
/// Ports `ocpp.v201.call.GetCompositeSchedule`. `duration` and `evse_id` are
/// required; `charging_rate_unit` optionally forces the unit the returned limits
/// are expressed in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCompositeScheduleRequest {
    /// Length of the requested schedule, in seconds.
    pub duration: i32,
    /// Force the returned schedule into a specific rate unit; the station picks
    /// when omitted.
    #[serde(rename = "chargingRateUnit", skip_serializing_if = "Option::is_none")]
    pub charging_rate_unit: Option<ChargingRateUnitEnumType>,
    /// EVSE the schedule is requested for; `0` computes the expected
    /// consumption for the whole grid connection.
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetCompositeScheduleRequest {
    const ACTION_NAME: &'static str = "GetCompositeSchedule";
    type Response = GetCompositeScheduleResponse;
}

/// `GetCompositeSchedule.conf` — the Charging Station's reply, carrying the
/// computed schedule when it could honour the request.
///
/// Ports `ocpp.v201.call_result.GetCompositeSchedule`. `status` is `Accepted`
/// when the station could compute the schedule (in which case `schedule` is
/// present) or `Rejected` otherwise (in which case `schedule` is omitted).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCompositeScheduleResponse {
    /// Whether the station could compute the requested schedule.
    pub status: GenericStatusEnumType,
    /// The computed composite schedule; present only when `status` is
    /// `Accepted`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<CompositeScheduleType>,
    /// Optional detail about the result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetCompositeScheduleResponse {
    const ACTION_NAME: &'static str = "GetCompositeScheduleResponse";
    type Response = Self;
}

impl OcppResponse for GetCompositeScheduleResponse {}
