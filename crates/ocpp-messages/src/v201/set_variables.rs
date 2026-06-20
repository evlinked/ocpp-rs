//! `SetVariables` — the CSMS writes one or more component-variable attributes.
//!
//! Ports `ocpp.v201.call.SetVariables` / `ocpp.v201.call_result.SetVariables`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, SetVariableDataType, SetVariableResultType};
use serde::{Deserialize, Serialize};

/// `SetVariables.req` — sent by the CSMS to write one or more
/// component-variable attributes on a Charging Station.
///
/// Ports `ocpp.v201.call.SetVariables`. The 2.0.1 device-model replacement for
/// 1.6J `ChangeConfiguration`: instead of a single flat key/value pair, each
/// entry names a `component`/`variable` pair and the value to assign (see
/// [`SetVariableDataType`]). The write-path counterpart to
/// [`GetVariablesRequest`](super::GetVariablesRequest).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVariablesRequest {
    /// The variables (and attributes) to write. Per the schema at least one
    /// entry must be present.
    #[serde(rename = "setVariableData")]
    pub set_variable_data: Vec<SetVariableDataType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetVariablesRequest {
    const ACTION_NAME: &'static str = "SetVariables";
    type Response = SetVariablesResponse;
}

/// `SetVariables.conf` — the Charging Station's reply, one result per requested
/// variable (order corresponds to the request).
///
/// Ports `ocpp.v201.call_result.SetVariables`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVariablesResponse {
    /// One result per requested variable.
    #[serde(rename = "setVariableResult")]
    pub set_variable_result: Vec<SetVariableResultType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetVariablesResponse {
    const ACTION_NAME: &'static str = "SetVariablesResponse";
    type Response = Self;
}

impl OcppResponse for SetVariablesResponse {}
