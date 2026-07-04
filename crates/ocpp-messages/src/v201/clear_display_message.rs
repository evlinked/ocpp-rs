//! `ClearDisplayMessage` — the CSMS tells a Charging Station to **remove a
//! single display message** it previously installed, identified by its `id`.
//!
//! Ports `ocpp.v201.call.ClearDisplayMessage` /
//! `ocpp.v201.call_result.ClearDisplayMessage`. The station answers
//! synchronously with a [`ClearMessageStatusEnumType`]: `Accepted` when the
//! message was found and removed, `Unknown` when no message with that id
//! existed. It is the removal side of the device-model **display-message**
//! family (`SetDisplayMessage`, `GetDisplayMessages`, `NotifyDisplayMessages`).
//!
//! [`ClearMessageStatusEnumType`]: ocpp_types::v201::ClearMessageStatusEnumType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{ClearMessageStatusEnumType, CustomDataType, StatusInfoType};
use serde::{Deserialize, Serialize};

/// `ClearDisplayMessage.req` — sent by the CSMS to remove one previously
/// installed display message from a Charging Station.
///
/// Ports `ocpp.v201.call.ClearDisplayMessage`. `id` is the id of the message to
/// remove; it matches the `id` assigned when the message was installed via
/// `SetDisplayMessage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearDisplayMessageRequest {
    /// The id of the display message to remove from the Charging Station.
    pub id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearDisplayMessageRequest {
    const ACTION_NAME: &'static str = "ClearDisplayMessage";
    type Response = ClearDisplayMessageResponse;
}

/// `ClearDisplayMessage.conf` — the Charging Station's synchronous
/// acknowledgement.
///
/// Ports `ocpp.v201.call_result.ClearDisplayMessage`. `status` reports whether
/// the station found and removed the message (`Accepted`) or did not recognise
/// the id (`Unknown`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearDisplayMessageResponse {
    /// Whether the message was removed (`Accepted`) or its id was unknown
    /// (`Unknown`).
    pub status: ClearMessageStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearDisplayMessageResponse {
    const ACTION_NAME: &'static str = "ClearDisplayMessageResponse";
    type Response = Self;
}

impl OcppResponse for ClearDisplayMessageResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = ClearDisplayMessageRequest {
            id: 42,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`.
        assert_eq!(value, json!({ "id": 42 }));
        let parsed: ClearDisplayMessageRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn id_round_trips_as_integer() {
        let req = ClearDisplayMessageRequest {
            id: 0,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["id"], json!(0));
        assert!(value["id"].is_i64());
    }

    #[test]
    fn request_missing_id_fails() {
        let err = serde_json::from_value::<ClearDisplayMessageRequest>(json!({})).unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    #[test]
    fn request_rejects_non_integer_id() {
        let err = serde_json::from_value::<ClearDisplayMessageRequest>(json!({ "id": "42" }))
            .unwrap_err();
        assert!(err.to_string().contains("i32") || err.to_string().contains("integer"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = ClearDisplayMessageResponse {
            status: ClearMessageStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        // `statusInfo` and `customData` are omitted when `None`.
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: ClearDisplayMessageResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = ClearDisplayMessageResponse {
            status: ClearMessageStatusEnumType::Unknown,
            status_info: Some(StatusInfoType {
                reason_code: "NoSuchMessage".to_string(),
                additional_info: None,
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("Unknown"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("NoSuchMessage"));
        let parsed: ClearDisplayMessageResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn status_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(ClearMessageStatusEnumType::Accepted).unwrap(),
            json!("Accepted")
        );
        assert_eq!(
            serde_json::to_value(ClearMessageStatusEnumType::Unknown).unwrap(),
            json!("Unknown")
        );
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err =
            serde_json::from_value::<ClearDisplayMessageResponse>(json!({ "status": "Rejected" }))
                .unwrap_err();
        assert!(err.to_string().contains("Rejected") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            ClearDisplayMessageRequest::ACTION_NAME,
            "ClearDisplayMessage"
        );
        assert_eq!(
            ClearDisplayMessageResponse::ACTION_NAME,
            "ClearDisplayMessageResponse"
        );
    }
}
