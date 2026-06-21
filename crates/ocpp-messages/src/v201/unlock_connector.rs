//! `UnlockConnector` — the CSMS asks a Charging Station to mechanically unlock a
//! specific connector on a specific EVSE.
//!
//! Ports `ocpp.v201.call.UnlockConnector` /
//! `ocpp.v201.call_result.UnlockConnector`. This is the 2.0.1 successor to 1.6J
//! `UnlockConnector`: where 1.6J addressed a flat `connectorId`, 2.0.1 names
//! both the `evseId` and the `connectorId` within it (e.g. so a driver can
//! retrieve a stuck cable). The reply carries an [`UnlockStatusEnumType`].

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, StatusInfoType, UnlockStatusEnumType};
use serde::{Deserialize, Serialize};

/// `UnlockConnector.req` — sent by the CSMS to unlock one connector on one EVSE.
///
/// Ports `ocpp.v201.call.UnlockConnector`. Both ids are required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnlockConnectorRequest {
    /// Identifier of the EVSE that owns the connector to unlock.
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// Identifier of the connector to unlock, within the EVSE above.
    #[serde(rename = "connectorId")]
    pub connector_id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for UnlockConnectorRequest {
    const ACTION_NAME: &'static str = "UnlockConnector";
    type Response = UnlockConnectorResponse;
}

/// `UnlockConnector.conf` — the Charging Station's reply, stating whether it
/// unlocked the connector.
///
/// Ports `ocpp.v201.call_result.UnlockConnector`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnlockConnectorResponse {
    /// The result of the unlock attempt.
    pub status: UnlockStatusEnumType,
    /// Optional detail about the result.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for UnlockConnectorResponse {
    const ACTION_NAME: &'static str = "UnlockConnectorResponse";
    type Response = Self;
}

impl OcppResponse for UnlockConnectorResponse {}
