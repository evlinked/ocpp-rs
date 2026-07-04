//! `NotifyDisplayMessages` — the Charging Station streams the *actual*
//! display-message data that a [`GetDisplayMessages`] request asked for.
//!
//! Ports `ocpp.v201.call.NotifyDisplayMessages` /
//! `ocpp.v201.call_result.NotifyDisplayMessages`. It is the asynchronous data
//! half of the OCPP 2.0.1 display-message **query** flow: after a station
//! answers `GetDisplayMessages` synchronously with `Accepted`, it sends one or
//! more `NotifyDisplayMessages.req` messages — paged via `tbc` — each
//! correlated back to the triggering request by `request_id`. The near-twin of
//! [`NotifyMonitoringReport`] (paged-carrier shape) but its payload embeds
//! installed [`MessageInfoType`] entries rather than monitoring data, and it
//! carries no `seq_no`/`generated_at`. The response is empty.
//!
//! [`GetDisplayMessages`]: super::GetDisplayMessagesRequest
//! [`NotifyMonitoringReport`]: super::NotifyMonitoringReportRequest
//! [`MessageInfoType`]: ocpp_types::v201::MessageInfoType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, MessageInfoType};
use serde::{Deserialize, Serialize};

/// `NotifyDisplayMessages.req` — a single (possibly partial) page of the
/// display messages a station currently has installed.
///
/// Ports `ocpp.v201.call.NotifyDisplayMessages`. `request_id` echoes the
/// `GetDisplayMessages` that triggered the report so the CSMS can correlate the
/// pages; `tbc` ("to be continued") is `true` while more pages follow. Only
/// `request_id` is required — a station with no matching messages may report an
/// empty page. `message_info` carries the actual data and, per the schema,
/// holds at least one item when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyDisplayMessagesRequest {
    /// The id of the `GetDisplayMessages` request that requested this report.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// The installed display messages for this page; absent when the page
    /// carries none. The schema requires at least one item when present.
    #[serde(rename = "messageInfo", skip_serializing_if = "Option::is_none")]
    pub message_info: Option<Vec<MessageInfoType>>,
    /// "To be continued" — `true` when another page follows in an upcoming
    /// `NotifyDisplayMessages`. Absent means the default `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbc: Option<bool>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyDisplayMessagesRequest {
    const ACTION_NAME: &'static str = "NotifyDisplayMessages";
    type Response = NotifyDisplayMessagesResponse;
}

/// `NotifyDisplayMessages.conf` — the CSMS's empty acknowledgement.
///
/// Ports `ocpp.v201.call_result.NotifyDisplayMessages`. It carries no fields
/// beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NotifyDisplayMessagesResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyDisplayMessagesResponse {
    const ACTION_NAME: &'static str = "NotifyDisplayMessagesResponse";
    type Response = Self;
}

impl OcppResponse for NotifyDisplayMessagesResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        MessageContentType, MessageFormatEnumType, MessagePriorityEnumType, MessageStateEnumType,
    };
    use serde_json::json;

    fn sample_message_info() -> MessageInfoType {
        MessageInfoType {
            id: 7,
            priority: MessagePriorityEnumType::AlwaysFront,
            message: MessageContentType {
                format: MessageFormatEnumType::Utf8,
                language: Some("en".to_string()),
                content: "Welcome".to_string(),
                custom_data: None,
            },
            state: Some(MessageStateEnumType::Idle),
            start_date_time: Some("2022-01-01T10:00:00Z".to_string()),
            end_date_time: Some("2022-01-02T10:00:00Z".to_string()),
            transaction_id: None,
            display: None,
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = NotifyDisplayMessagesRequest {
            request_id: 42,
            message_info: None,
            tbc: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional `messageInfo` / `tbc` / `customData` stay off the wire.
        assert_eq!(value, json!({ "requestId": 42 }));
        let parsed: NotifyDisplayMessagesRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_message_info() {
        let req = NotifyDisplayMessagesRequest {
            request_id: 7,
            message_info: Some(vec![sample_message_info()]),
            tbc: Some(true),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["messageInfo"][0]["id"], json!(7));
        assert_eq!(value["messageInfo"][0]["priority"], json!("AlwaysFront"));
        assert_eq!(
            value["messageInfo"][0]["message"]["content"],
            json!("Welcome")
        );
        assert_eq!(value["messageInfo"][0]["message"]["format"], json!("UTF8"));
        assert_eq!(value["messageInfo"][0]["state"], json!("Idle"));
        assert_eq!(value["tbc"], json!(true));
        let parsed: NotifyDisplayMessagesRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let value = serde_json::to_value(NotifyDisplayMessagesRequest {
            request_id: -3,
            message_info: Some(vec![sample_message_info()]),
            tbc: Some(false),
            custom_data: None,
        })
        .unwrap();
        assert!(value["requestId"].is_i64());
        assert!(value["tbc"].is_boolean());
        assert!(value["messageInfo"].is_array());
        assert!(value["messageInfo"][0]["id"].is_i64());
    }

    #[test]
    fn response_round_trips_empty() {
        let resp = NotifyDisplayMessagesResponse::default();
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({}));
        let parsed: NotifyDisplayMessagesResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn request_missing_required_field_fails() {
        // `requestId` is required.
        let err = serde_json::from_value::<NotifyDisplayMessagesRequest>(json!({ "tbc": true }))
            .unwrap_err();
        assert!(err.to_string().contains("requestId"));
    }

    #[test]
    fn request_rejects_unknown_message_priority() {
        let err = serde_json::from_value::<NotifyDisplayMessagesRequest>(json!({
            "requestId": 1,
            "messageInfo": [{
                "id": 1,
                "priority": "Whenever",
                "message": { "format": "UTF8", "content": "hi" }
            }]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("Whenever") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            NotifyDisplayMessagesRequest::ACTION_NAME,
            "NotifyDisplayMessages"
        );
        assert_eq!(
            NotifyDisplayMessagesResponse::ACTION_NAME,
            "NotifyDisplayMessagesResponse"
        );
    }
}
