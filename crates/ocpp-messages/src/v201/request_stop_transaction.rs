//! `RequestStopTransaction` — the CSMS asks a Charging Station to stop a
//! running transaction.
//!
//! Ports `ocpp.v201.call.RequestStopTransaction` /
//! `ocpp.v201.call_result.RequestStopTransaction`. This is the 2.0.1 successor
//! to 1.6J `RemoteStopTransaction`: the CSMS names the transaction to stop by
//! its `transactionId` and the station replies whether it accepts the request.
//! The smallest of the 2.0.1 command messages — a single required string in, a
//! status out.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, RequestStartStopStatusEnumType, StatusInfoType};
use serde::{Deserialize, Serialize};

/// `RequestStopTransaction.req` — sent by the CSMS to stop a running
/// transaction.
///
/// Ports `ocpp.v201.call.RequestStopTransaction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestStopTransactionRequest {
    /// The identifier of the transaction which the Charging Station is requested
    /// to stop.
    #[serde(rename = "transactionId")]
    pub transaction_id: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for RequestStopTransactionRequest {
    const ACTION_NAME: &'static str = "RequestStopTransaction";
    type Response = RequestStopTransactionResponse;
}

/// `RequestStopTransaction.conf` — the Charging Station's reply, stating whether
/// it accepts the request to stop the transaction.
///
/// Ports `ocpp.v201.call_result.RequestStopTransaction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestStopTransactionResponse {
    /// Whether the Charging Station accepts the request (`Accepted` /
    /// `Rejected`).
    pub status: RequestStartStopStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for RequestStopTransactionResponse {
    const ACTION_NAME: &'static str = "RequestStopTransactionResponse";
    type Response = Self;
}

impl OcppResponse for RequestStopTransactionResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::CustomDataType;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = RequestStopTransactionRequest {
            transaction_id: "txn-0001".to_string(),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`.
        assert_eq!(value, json!({ "transactionId": "txn-0001" }));
        let parsed: RequestStopTransactionRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_serializes_custom_data() {
        let req = RequestStopTransactionRequest {
            transaction_id: "txn-42".to_string(),
            custom_data: Some(CustomDataType {
                vendor_id: "ACME".to_string(),
                extra: Default::default(),
            }),
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["transactionId"], json!("txn-42"));
        assert_eq!(value["customData"]["vendorId"], json!("ACME"));
    }

    #[test]
    fn request_missing_transaction_id_fails() {
        let err = serde_json::from_value::<RequestStopTransactionRequest>(json!({})).unwrap_err();
        assert!(err.to_string().contains("transactionId"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = RequestStopTransactionResponse {
            status: RequestStartStopStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        // `statusInfo` omitted when `None`.
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: RequestStopTransactionResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = RequestStopTransactionResponse {
            status: RequestStartStopStatusEnumType::Rejected,
            status_info: Some(StatusInfoType {
                reason_code: "NoTransaction".to_string(),
                additional_info: Some("Unknown transactionId".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("Rejected"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("NoTransaction"));
        assert_eq!(
            value["statusInfo"]["additionalInfo"],
            json!("Unknown transactionId")
        );
    }

    #[test]
    fn response_status_serializes_pascal_case() {
        assert_eq!(
            serde_json::to_value(RequestStartStopStatusEnumType::Accepted).unwrap(),
            json!("Accepted")
        );
        assert_eq!(
            serde_json::to_value(RequestStartStopStatusEnumType::Rejected).unwrap(),
            json!("Rejected")
        );
    }

    #[test]
    fn response_rejects_unknown_status() {
        let err = serde_json::from_value::<RequestStopTransactionResponse>(
            json!({ "status": "Pending" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Pending") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            RequestStopTransactionRequest::ACTION_NAME,
            "RequestStopTransaction"
        );
        assert_eq!(
            RequestStopTransactionResponse::ACTION_NAME,
            "RequestStopTransactionResponse"
        );
    }
}
