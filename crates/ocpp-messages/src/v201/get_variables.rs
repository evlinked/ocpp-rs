//! `GetVariables` — the CSMS reads one or more component-variable attributes.
//!
//! Ports `ocpp.v201.call.GetVariables` / `ocpp.v201.call_result.GetVariables`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, GetVariableDataType, GetVariableResultType};
use serde::{Deserialize, Serialize};

/// `GetVariables.req` — sent by the CSMS to read one or more
/// component-variable attributes from a Charging Station.
///
/// Ports `ocpp.v201.call.GetVariables`. The 2.0.1 device-model replacement for
/// 1.6J `GetConfiguration`: instead of flat string keys, each entry names a
/// `component`/`variable` pair (see [`GetVariableDataType`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetVariablesRequest {
    /// The variables (and attributes) to read. Per the schema at least one
    /// entry must be present.
    #[serde(rename = "getVariableData")]
    pub get_variable_data: Vec<GetVariableDataType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetVariablesRequest {
    const ACTION_NAME: &'static str = "GetVariables";
    type Response = GetVariablesResponse;
}

/// `GetVariables.conf` — the Charging Station's reply, one result per requested
/// variable (order corresponds to the request).
///
/// Ports `ocpp.v201.call_result.GetVariables`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetVariablesResponse {
    /// One result per requested variable.
    #[serde(rename = "getVariableResult")]
    pub get_variable_result: Vec<GetVariableResultType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetVariablesResponse {
    const ACTION_NAME: &'static str = "GetVariablesResponse";
    type Response = Self;
}

impl OcppResponse for GetVariablesResponse {}
