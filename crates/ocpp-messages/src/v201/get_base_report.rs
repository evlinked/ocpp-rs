//! `GetBaseReport` — the CSMS asks a Charging Station for a snapshot of its
//! device model: the writable configuration only, or the full / summarized
//! inventory of components and variables.
//!
//! Ports `ocpp.v201.call.GetBaseReport` / `ocpp.v201.call_result.GetBaseReport`.
//! The station acknowledges synchronously with a
//! [`GenericDeviceModelStatusEnumType`] and then streams the actual data
//! asynchronously via later `NotifyReport` messages, correlated back to this
//! request by `requestId`.
//!
//! This is a foundational device-model message: its response status enum is
//! shared across the whole report/monitor family (`GetReport`,
//! `GetMonitoringReport`, `SetMonitoringBase`, `SetMonitoringLevel`,
//! `SetNetworkProfile`).

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, GenericDeviceModelStatusEnumType, ReportBaseEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetBaseReport.req` — sent by the CSMS to request a device-model report.
///
/// Ports `ocpp.v201.call.GetBaseReport`. `request_id` is the correlation id the
/// station echoes back on the asynchronous `NotifyReport` messages;
/// `report_base` selects which slice of the device model to report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetBaseReportRequest {
    /// The id of the request, echoed by the station on the asynchronous
    /// `NotifyReport` messages that carry the actual report data.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Which slice of the device model to report (configuration / full /
    /// summary inventory).
    #[serde(rename = "reportBase")]
    pub report_base: ReportBaseEnumType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetBaseReportRequest {
    const ACTION_NAME: &'static str = "GetBaseReport";
    type Response = GetBaseReportResponse;
}

/// `GetBaseReport.conf` — the Charging Station's synchronous acknowledgement.
///
/// Ports `ocpp.v201.call_result.GetBaseReport`. `status` reports whether the
/// station will produce the report; the actual data follows asynchronously in
/// `NotifyReport` messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetBaseReportResponse {
    /// Whether the station can produce the requested report (`Accepted`,
    /// `Rejected`, `NotSupported`, or `EmptyResultSet` when nothing matched).
    pub status: GenericDeviceModelStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetBaseReportResponse {
    const ACTION_NAME: &'static str = "GetBaseReportResponse";
    type Response = Self;
}

impl OcppResponse for GetBaseReportResponse {}
