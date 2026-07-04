//! `GetLog` — the CSMS asks a Charging Station to collect a diagnostics or
//! security log and upload it to a remote location.
//!
//! Ports `ocpp.v201.call.GetLog` / `ocpp.v201.call_result.GetLog`. The request
//! names the log kind ([`LogEnumType`]), the upload target
//! ([`LogParametersType`], carrying the remote URI and an optional time window),
//! a `requestId` correlating the flow, and optional `retries` / `retryInterval`
//! hints. The station acks synchronously with a [`LogStatusEnumType`]
//! (`Accepted` / `Rejected` / `AcceptedCanceled`) and, when it will upload, the
//! `filename` it will produce; it then reports upload progress asynchronously via
//! `LogStatusNotification.req` (already ported) correlated by the same
//! `requestId`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, LogEnumType, LogParametersType, LogStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetLog.req` — sent by the CSMS to have a Charging Station upload a log.
///
/// Ports `ocpp.v201.call.GetLog`. `log`, `log_type` and `request_id` are
/// required; `retries` (how many upload attempts before giving up) and
/// `retry_interval` (seconds between attempts) are optional hints the station
/// may honour or decide for itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetLogRequest {
    /// Where and over what time window the log should be uploaded.
    pub log: LogParametersType,
    /// Which log the station should collect (diagnostics or security).
    #[serde(rename = "logType")]
    pub log_type: LogEnumType,
    /// Correlates this request with the station's `LogStatusNotification.req`
    /// progress reports.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// How many times the station must retry the upload before giving up.
    /// Omitted leaves the count to the station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i32>,
    /// Interval in seconds after which a retry may be attempted. Omitted leaves
    /// the wait to the station.
    #[serde(rename = "retryInterval", skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetLogRequest {
    const ACTION_NAME: &'static str = "GetLog";
    type Response = GetLogResponse;
}

/// `GetLog.conf` — the station's synchronous acknowledgement of a `GetLog.req`.
///
/// Ports `ocpp.v201.call_result.GetLog`. `status` is required; `filename` is the
/// name of the log file that will be uploaded and is absent when no logging
/// information is available (i.e. typically when `status` is not `Accepted`).
/// The `filename` `maxLength: 255` bound is enforced at the schema layer,
/// consistent with the rest of v201.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetLogResponse {
    /// Whether the station accepted the request to collect and upload the log.
    pub status: LogStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Name of the log file that will be uploaded (max length 255). Absent when
    /// no logging information is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetLogResponse {
    const ACTION_NAME: &'static str = "GetLogResponse";
    type Response = Self;
}

impl OcppResponse for GetLogResponse {}
