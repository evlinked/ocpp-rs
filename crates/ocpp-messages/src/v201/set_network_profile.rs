//! `SetNetworkProfile` — the CSMS provisions the connectivity settings a
//! Charging Station uses to reach it: the OCPP transport/version, the message
//! timeout and security profile, the network interface, and the underlying
//! cellular (APN) or VPN bearer.
//!
//! Ports `ocpp.v201.call.SetNetworkProfile` /
//! `ocpp.v201.call_result.SetNetworkProfile`. The CSMS writes the
//! [`NetworkConnectionProfileType`] into a numbered `configurationSlot` so a
//! station can hold several fallback connections; the station acks
//! synchronously with a [`SetNetworkProfileStatusEnumType`] (`Accepted` /
//! `Rejected` / `Failed`). A self-contained configuration command with no async
//! follow-up. Pulls in a small tree of new datatypes ([`APNType`], [`VPNType`],
//! [`NetworkConnectionProfileType`]) and their enums; reuses [`StatusInfoType`].
//!
//! [`NetworkConnectionProfileType`]: ocpp_types::v201::NetworkConnectionProfileType
//! [`SetNetworkProfileStatusEnumType`]: ocpp_types::v201::SetNetworkProfileStatusEnumType
//! [`APNType`]: ocpp_types::v201::APNType
//! [`VPNType`]: ocpp_types::v201::VPNType
//! [`StatusInfoType`]: ocpp_types::v201::StatusInfoType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, NetworkConnectionProfileType, SetNetworkProfileStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `SetNetworkProfile.req` — sent by the CSMS to configure a Charging Station's
/// network connection profile.
///
/// Ports `ocpp.v201.call.SetNetworkProfile`. `configurationSlot` selects the
/// numbered slot the profile is stored in; `connectionData` carries the profile
/// itself. Both are required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetNetworkProfileRequest {
    /// Slot in which the configuration should be stored.
    #[serde(rename = "configurationSlot")]
    pub configuration_slot: i32,
    /// The network connection profile to store.
    #[serde(rename = "connectionData")]
    pub connection_data: NetworkConnectionProfileType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetNetworkProfileRequest {
    const ACTION_NAME: &'static str = "SetNetworkProfile";
    type Response = SetNetworkProfileResponse;
}

/// `SetNetworkProfile.conf` — the Charging Station's synchronous
/// acknowledgement.
///
/// Ports `ocpp.v201.call_result.SetNetworkProfile`. `status` reports whether the
/// station accepted (`Accepted`), refused (`Rejected`), or accepted but could
/// not apply (`Failed`) the profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetNetworkProfileResponse {
    /// Result of storing the profile.
    pub status: SetNetworkProfileStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetNetworkProfileResponse {
    const ACTION_NAME: &'static str = "SetNetworkProfileResponse";
    type Response = Self;
}

impl OcppResponse for SetNetworkProfileResponse {}
