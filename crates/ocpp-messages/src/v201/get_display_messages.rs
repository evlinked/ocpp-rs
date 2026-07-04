//! `GetDisplayMessages` — the CSMS asks a Charging Station which display
//! messages it currently has installed.
//!
//! Ports `ocpp.v201.call.GetDisplayMessages` /
//! `ocpp.v201.call_result.GetDisplayMessages`. It is the query **trigger** of
//! the OCPP 2.0.1 display-message family: the install side is
//! [`SetDisplayMessage`](crate::v201) and the removal side is
//! [`ClearDisplayMessage`](crate::v201). The CSMS optionally narrows the query
//! by message id(s), [`MessagePriorityEnumType`], and/or
//! [`MessageStateEnumType`]; the station answers synchronously with a
//! [`GetDisplayMessagesStatusEnumType`] (`Accepted` when it has matches,
//! `Unknown` when it does not) and then streams the actual message data
//! asynchronously via one or more `NotifyDisplayMessages.req`, correlated back
//! to this request by `requestId`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, GetDisplayMessagesStatusEnumType, MessagePriorityEnumType,
    MessageStateEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetDisplayMessages.req` — sent by the CSMS to enumerate the display
/// messages installed on a station.
///
/// Ports `ocpp.v201.call.GetDisplayMessages`. Only `request_id` is required; it
/// correlates the asynchronous `NotifyDisplayMessages` report(s) back to this
/// query. The optional `id`, `priority`, and `state` filters narrow which
/// messages the station reports; when all are omitted the station reports every
/// installed message. `id` is non-empty when present (schema `minItems` 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetDisplayMessagesRequest {
    /// The id of this request, echoed back by the station on each
    /// `NotifyDisplayMessages` report so the CSMS can correlate them.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Restrict the report to display messages with these ids. Non-empty when
    /// present (schema `minItems` 1); when `None`, ids are not filtered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Vec<i32>>,
    /// Restrict the report to messages with this priority; when `None`,
    /// priority is not filtered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<MessagePriorityEnumType>,
    /// Restrict the report to messages shown in this station state; when
    /// `None`, state is not filtered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<MessageStateEnumType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetDisplayMessagesRequest {
    const ACTION_NAME: &'static str = "GetDisplayMessages";
    type Response = GetDisplayMessagesResponse;
}

/// `GetDisplayMessages.conf` — the station's synchronous acknowledgement of the
/// query.
///
/// Ports `ocpp.v201.call_result.GetDisplayMessages`. `status` is the only
/// required field: `Accepted` means the station has one or more matching
/// messages (streamed afterwards via `NotifyDisplayMessages`), `Unknown` means
/// it has none. This response carries no message data itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetDisplayMessagesResponse {
    /// Whether the station has display messages matching the request criteria.
    pub status: GetDisplayMessagesStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetDisplayMessagesResponse {
    const ACTION_NAME: &'static str = "GetDisplayMessagesResponse";
    type Response = Self;
}

impl OcppResponse for GetDisplayMessagesResponse {}
