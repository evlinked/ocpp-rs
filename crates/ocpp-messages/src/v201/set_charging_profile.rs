//! `SetChargingProfile` — the CSMS installs a charging profile on an EVSE,
//! bounding the charging power/current over time (smart charging).
//!
//! Ports `ocpp.v201.call.SetChargingProfile` /
//! `ocpp.v201.call_result.SetChargingProfile`. The heavy lifting — the
//! [`ChargingProfileType`] datatype and its nested schedule tree — already
//! landed with `RequestStartTransaction` (#161); this message reuses it
//! verbatim. The only genuinely new surface is the
//! [`ChargingProfileStatusEnumType`] returned by the station.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ChargingProfileStatusEnumType, ChargingProfileType, CustomDataType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `SetChargingProfile.req` — sent by the CSMS to install a charging profile on
/// an EVSE.
///
/// Ports `ocpp.v201.call.SetChargingProfile`. `evse_id` selects the target: a
/// non-zero id addresses one EVSE, while `0` applies a `TxDefaultProfile` to
/// every EVSE or carries a station-wide limit for the
/// `ChargingStationMaxProfile` / `ChargingStationExternalConstraints`
/// purposes. The `charging_profile` itself is the already-ported
/// [`ChargingProfileType`], whose schedule tree is unchanged here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetChargingProfileRequest {
    /// The EVSE to install the profile on (`0` = station-wide / all EVSEs).
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// The charging profile to install, including its schedule tree.
    #[serde(rename = "chargingProfile")]
    pub charging_profile: ChargingProfileType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetChargingProfileRequest {
    const ACTION_NAME: &'static str = "SetChargingProfile";
    type Response = SetChargingProfileResponse;
}

/// `SetChargingProfile.conf` — the Charging Station's reply, stating whether it
/// was able to process the profile.
///
/// Ports `ocpp.v201.call_result.SetChargingProfile`. An `Accepted` status does
/// not guarantee the schedule is followed to the letter — the station may apply
/// further local constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetChargingProfileResponse {
    /// Whether the profile was accepted (`Accepted`) or refused (`Rejected`).
    pub status: ChargingProfileStatusEnumType,
    /// Optional detail about the result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetChargingProfileResponse {
    const ACTION_NAME: &'static str = "SetChargingProfileResponse";
    type Response = Self;
}

impl OcppResponse for SetChargingProfileResponse {}
