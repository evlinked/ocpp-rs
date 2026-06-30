//! `CostUpdated` — the CSMS pushes the current running total cost of an
//! ongoing transaction to the Charging Station, so it can show the driver an
//! up-to-date price.
//!
//! Ports `ocpp.v201.call.CostUpdated` / `ocpp.v201.call_result.CostUpdated`.
//! The request carries the running `totalCost` (a JSON `number`, the first
//! non-integer numeric field in this v201 port) and the `transactionId` it
//! applies to; the response is empty (only the optional vendor extension), so
//! it serializes to `{}`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::CustomDataType;
use serde::{Deserialize, Serialize};

/// `CostUpdated.req` — running-cost update sent by the CSMS to the Charging
/// Station.
///
/// Ports `ocpp.v201.call.CostUpdated`. `total_cost` is the current total cost
/// of the transaction including taxes, in the CSMS-configured currency;
/// `transaction_id` identifies the transaction it applies to (schema
/// `maxLength: 36`, enforced at the schema layer).
///
/// `total_cost` is a JSON-Schema `number`, modelled as [`f64`]. The derived
/// [`PartialEq`] is reliable for the round-trip tests because they use values
/// that are exactly representable in binary floating point (e.g. `0.0`,
/// `12.0`, `12.5`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostUpdatedRequest {
    /// Current total cost of the transaction including taxes, in the currency
    /// configured at the CSMS.
    #[serde(rename = "totalCost")]
    pub total_cost: f64,
    /// Id of the transaction the cost applies to.
    #[serde(rename = "transactionId")]
    pub transaction_id: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CostUpdatedRequest {
    const ACTION_NAME: &'static str = "CostUpdated";
    type Response = CostUpdatedResponse;
}

/// `CostUpdated.conf` — the Charging Station's acknowledgement.
///
/// Ports `ocpp.v201.call_result.CostUpdated`. The response carries no fields
/// beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CostUpdatedResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CostUpdatedResponse {
    const ACTION_NAME: &'static str = "CostUpdatedResponse";
    type Response = Self;
}

impl OcppResponse for CostUpdatedResponse {}
