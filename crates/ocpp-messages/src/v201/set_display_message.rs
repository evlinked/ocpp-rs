//! `SetDisplayMessage` — the CSMS **installs (or replaces) a display message**
//! on a Charging Station's display.
//!
//! Ports `ocpp.v201.call.SetDisplayMessage` /
//! `ocpp.v201.call_result.SetDisplayMessage`. The request carries a single
//! [`MessageInfoType`] describing the message, its priority, and optionally when
//! / where / in which state it should be shown. The station answers
//! synchronously with a [`DisplayMessageStatusEnumType`] reporting whether it
//! accepted the message and, if not, why (unsupported format, priority, or
//! state, unknown transaction, or a plain rejection). It is the install side of
//! the device-model **display-message** family whose removal side is
//! `ClearDisplayMessage`.
//!
//! [`MessageInfoType`]: ocpp_types::v201::MessageInfoType
//! [`DisplayMessageStatusEnumType`]: ocpp_types::v201::DisplayMessageStatusEnumType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, DisplayMessageStatusEnumType, MessageInfoType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `SetDisplayMessage.req` — sent by the CSMS to install one display message on
/// a Charging Station.
///
/// Ports `ocpp.v201.call.SetDisplayMessage`. `message` is the only required
/// field and fully describes the message to display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetDisplayMessageRequest {
    /// The display message to install on the Charging Station.
    pub message: MessageInfoType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetDisplayMessageRequest {
    const ACTION_NAME: &'static str = "SetDisplayMessage";
    type Response = SetDisplayMessageResponse;
}

/// `SetDisplayMessage.conf` — the Charging Station's synchronous
/// acknowledgement.
///
/// Ports `ocpp.v201.call_result.SetDisplayMessage`. `status` reports whether the
/// station accepted the message (`Accepted`) or rejected it, with the specific
/// reason encoded in the enum (e.g. `NotSupportedMessageFormat`,
/// `NotSupportedPriority`, `NotSupportedState`, `UnknownTransaction`, or a plain
/// `Rejected`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetDisplayMessageResponse {
    /// Whether the station accepted the message, and if not, why.
    pub status: DisplayMessageStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SetDisplayMessageResponse {
    const ACTION_NAME: &'static str = "SetDisplayMessageResponse";
    type Response = Self;
}

impl OcppResponse for SetDisplayMessageResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        MessageContentType, MessageFormatEnumType, MessagePriorityEnumType, MessageStateEnumType,
    };
    use serde_json::json;

    fn minimal_message() -> MessageInfoType {
        MessageInfoType {
            id: 1,
            priority: MessagePriorityEnumType::NormalCycle,
            message: MessageContentType {
                format: MessageFormatEnumType::Utf8,
                content: "Welcome".to_string(),
                language: None,
                custom_data: None,
            },
            state: None,
            start_date_time: None,
            end_date_time: None,
            transaction_id: None,
            display: None,
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = SetDisplayMessageRequest {
            message: minimal_message(),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Only `id`, `priority`, `message` present; all optionals omitted.
        assert_eq!(
            value,
            json!({
                "message": {
                    "id": 1,
                    "priority": "NormalCycle",
                    "message": { "format": "UTF8", "content": "Welcome" }
                }
            })
        );
        let parsed: SetDisplayMessageRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_full_message_info() {
        let req = SetDisplayMessageRequest {
            message: MessageInfoType {
                id: 7,
                priority: MessagePriorityEnumType::AlwaysFront,
                message: MessageContentType {
                    format: MessageFormatEnumType::Html,
                    content: "<b>Charging</b>".to_string(),
                    language: Some("en".to_string()),
                    custom_data: None,
                },
                state: Some(MessageStateEnumType::Charging),
                start_date_time: Some("2026-07-04T00:00:00Z".to_string()),
                end_date_time: Some("2026-07-05T00:00:00Z".to_string()),
                transaction_id: Some("txn-123".to_string()),
                display: None,
                custom_data: None,
            },
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["message"]["state"], json!("Charging"));
        assert_eq!(
            value["message"]["startDateTime"],
            json!("2026-07-04T00:00:00Z")
        );
        assert_eq!(
            value["message"]["endDateTime"],
            json!("2026-07-05T00:00:00Z")
        );
        assert_eq!(value["message"]["transactionId"], json!("txn-123"));
        let parsed: SetDisplayMessageRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn message_id_round_trips_as_integer() {
        let req = SetDisplayMessageRequest {
            message: minimal_message(),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["message"]["id"], json!(1));
        assert!(value["message"]["id"].is_i64());
    }

    #[test]
    fn request_missing_message_fails() {
        let err = serde_json::from_value::<SetDisplayMessageRequest>(json!({})).unwrap_err();
        assert!(err.to_string().contains("message"));
    }

    #[test]
    fn message_info_missing_priority_fails() {
        let err = serde_json::from_value::<SetDisplayMessageRequest>(json!({
            "message": { "id": 1, "message": { "format": "UTF8", "content": "hi" } }
        }))
        .unwrap_err();
        assert!(err.to_string().contains("priority"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = SetDisplayMessageResponse {
            status: DisplayMessageStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        // `statusInfo` and `customData` are omitted when `None`.
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: SetDisplayMessageResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = SetDisplayMessageResponse {
            status: DisplayMessageStatusEnumType::NotSupportedMessageFormat,
            status_info: Some(StatusInfoType {
                reason_code: "BadFormat".to_string(),
                additional_info: None,
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("NotSupportedMessageFormat"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("BadFormat"));
        let parsed: SetDisplayMessageResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn priority_enum_serializes_to_wire_values() {
        assert_eq!(
            serde_json::to_value(MessagePriorityEnumType::AlwaysFront).unwrap(),
            json!("AlwaysFront")
        );
        assert_eq!(
            serde_json::to_value(MessagePriorityEnumType::InFront).unwrap(),
            json!("InFront")
        );
        assert_eq!(
            serde_json::to_value(MessagePriorityEnumType::NormalCycle).unwrap(),
            json!("NormalCycle")
        );
    }

    #[test]
    fn state_enum_serializes_to_wire_values() {
        for (variant, wire) in [
            (MessageStateEnumType::Charging, "Charging"),
            (MessageStateEnumType::Faulted, "Faulted"),
            (MessageStateEnumType::Idle, "Idle"),
            (MessageStateEnumType::Unavailable, "Unavailable"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
        }
    }

    #[test]
    fn status_enum_serializes_to_wire_values() {
        for (variant, wire) in [
            (DisplayMessageStatusEnumType::Accepted, "Accepted"),
            (
                DisplayMessageStatusEnumType::NotSupportedMessageFormat,
                "NotSupportedMessageFormat",
            ),
            (DisplayMessageStatusEnumType::Rejected, "Rejected"),
            (
                DisplayMessageStatusEnumType::NotSupportedPriority,
                "NotSupportedPriority",
            ),
            (
                DisplayMessageStatusEnumType::NotSupportedState,
                "NotSupportedState",
            ),
            (
                DisplayMessageStatusEnumType::UnknownTransaction,
                "UnknownTransaction",
            ),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
        }
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err = serde_json::from_value::<SetDisplayMessageResponse>(json!({ "status": "Maybe" }))
            .unwrap_err();
        assert!(err.to_string().contains("Maybe") || err.to_string().contains("variant"));
    }

    #[test]
    fn request_rejects_unknown_priority() {
        let err = serde_json::from_value::<SetDisplayMessageRequest>(json!({
            "message": {
                "id": 1,
                "priority": "Whenever",
                "message": { "format": "UTF8", "content": "hi" }
            }
        }))
        .unwrap_err();
        assert!(err.to_string().contains("Whenever") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(SetDisplayMessageRequest::ACTION_NAME, "SetDisplayMessage");
        assert_eq!(
            SetDisplayMessageResponse::ACTION_NAME,
            "SetDisplayMessageResponse"
        );
    }
}
