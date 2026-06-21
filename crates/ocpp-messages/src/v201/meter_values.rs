//! `MeterValues` — the Charging Station pushes sampled meter readings for an
//! EVSE outside of (or alongside) the `TransactionEvent` flow.
//!
//! Ports `ocpp.v201.call.MeterValues` / `ocpp.v201.call_result.MeterValues`.
//! This is the 2.0.1 successor to 1.6J `MeterValues`. The heavy lifting — the
//! [`MeterValueType`] / `SampledValueType` tree and the measurement enums —
//! already lives in [`ocpp_types::v201`] (ported for `TransactionEvent`), so
//! this is a thin request/response wrapper around it.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, MeterValueType};
use serde::{Deserialize, Serialize};

/// `MeterValues.req` — sent by the Charging Station to report sampled meter
/// values for one EVSE.
///
/// Ports `ocpp.v201.call.MeterValues`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterValuesRequest {
    /// The EVSE the readings belong to. A number `> 0` designates an EVSE of
    /// the Charging Station; `0` designates the station's main power meter.
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// The sampled meter readings; the schema requires at least one
    /// [`MeterValueType`], each with a non-empty `sampledValue` list.
    #[serde(rename = "meterValue")]
    pub meter_value: Vec<MeterValueType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for MeterValuesRequest {
    const ACTION_NAME: &'static str = "MeterValues";
    type Response = MeterValuesResponse;
}

/// `MeterValues.conf` — the CSMS acknowledgement. The payload is empty in the
/// spec (it carries no fields beyond the optional vendor extension), so it
/// serializes to `{}` unless `customData` is present.
///
/// Ports `ocpp.v201.call_result.MeterValues`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeterValuesResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for MeterValuesResponse {
    const ACTION_NAME: &'static str = "MeterValuesResponse";
    type Response = Self;
}

impl OcppResponse for MeterValuesResponse {}
