//! `NotifyCustomerInformation` — the Charging Station streams the stored
//! customer data that a `CustomerInformation` request asked it to report.
//!
//! Ports `ocpp.v201.call.NotifyCustomerInformation` /
//! `ocpp.v201.call_result.NotifyCustomerInformation`. It is the asynchronous
//! data half of the OCPP 2.0.1 customer-information (privacy / GDPR) flow: when
//! a `CustomerInformation` request arrives with `report: true`, the station
//! acks it synchronously and then streams the stored data back as one or more
//! `NotifyCustomerInformation.req` pages — plain human-readable text, paged via
//! `seq_no` / `tbc`, each correlated to the triggering request by `request_id`.
//! The flat-text near-twin of [`NotifyReport`] (same paged-carrier shape) but
//! its payload is a `data` string rather than a report graph, so it embeds no
//! datatypes. The response is empty.
//!
//! (`CustomerInformation`, the synchronous trigger, is a separate message; it
//! is not linked here to keep this module's docs independent of merge order.)
//!
//! [`NotifyReport`]: super::NotifyReportRequest

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::CustomDataType;
use serde::{Deserialize, Serialize};

/// `NotifyCustomerInformation.req` — a single (possibly partial) page of the
/// stored customer data a station is reporting back.
///
/// Ports `ocpp.v201.call.NotifyCustomerInformation`. `request_id` echoes the
/// `CustomerInformation` request that triggered the report so the CSMS can
/// correlate the pages; `seq_no` numbers the pages (first is 0) and `tbc`
/// ("to be continued") is `true` while more pages follow. `data` carries the
/// (part of the) requested data — no format is specified beyond that it should
/// be human readable (the schema caps it at 512 characters).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyCustomerInformationRequest {
    /// (Part of) the requested data. No format is specified; it should be human
    /// readable. The schema caps this at `maxLength` 512.
    pub data: String,
    /// "To be continued" — `true` when another page follows in an upcoming
    /// `NotifyCustomerInformation`. Absent means the default `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbc: Option<bool>,
    /// Sequence number of this message; the first page starts at 0.
    #[serde(rename = "seqNo")]
    pub seq_no: i32,
    /// Timestamp (RFC 3339 / ISO 8601) of the moment this message was generated
    /// at the Charging Station.
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    /// The id of the `CustomerInformation` request that requested this report.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyCustomerInformationRequest {
    const ACTION_NAME: &'static str = "NotifyCustomerInformation";
    type Response = NotifyCustomerInformationResponse;
}

/// `NotifyCustomerInformation.conf` — the CSMS's empty acknowledgement.
///
/// Ports `ocpp.v201.call_result.NotifyCustomerInformation`. It carries no
/// fields beyond the optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NotifyCustomerInformationResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyCustomerInformationResponse {
    const ACTION_NAME: &'static str = "NotifyCustomerInformationResponse";
    type Response = Self;
}

impl OcppResponse for NotifyCustomerInformationResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = NotifyCustomerInformationRequest {
            data: "name=Jane Doe".to_string(),
            tbc: None,
            seq_no: 0,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            request_id: 42,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional `tbc` / `customData` stay off the wire.
        assert_eq!(
            value,
            json!({
                "data": "name=Jane Doe",
                "seqNo": 0,
                "generatedAt": "2022-01-01T10:00:00Z",
                "requestId": 42
            })
        );
        let parsed: NotifyCustomerInformationRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_full() {
        let req = NotifyCustomerInformationRequest {
            data: "idToken=ABC123; sessions=17".to_string(),
            tbc: Some(true),
            seq_no: 3,
            generated_at: "2022-01-01T10:05:00Z".to_string(),
            request_id: 7,
            custom_data: Some(CustomDataType {
                vendor_id: "com.example".to_string(),
                extra: Default::default(),
            }),
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["data"], json!("idToken=ABC123; sessions=17"));
        assert_eq!(value["tbc"], json!(true));
        assert_eq!(value["seqNo"], json!(3));
        assert_eq!(value["requestId"], json!(7));
        assert_eq!(value["customData"]["vendorId"], json!("com.example"));
        let parsed: NotifyCustomerInformationRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let value = serde_json::to_value(NotifyCustomerInformationRequest {
            data: "x".to_string(),
            tbc: Some(false),
            seq_no: -1,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            request_id: -3,
            custom_data: None,
        })
        .unwrap();
        assert!(value["data"].is_string());
        assert!(value["tbc"].is_boolean());
        assert!(value["seqNo"].is_i64());
        assert!(value["requestId"].is_i64());
        assert!(value["generatedAt"].is_string());
    }

    /// A representative multi-page sequence: a non-final page (`tbc: true`)
    /// followed by a final page (`tbc` omitted) sharing one `request_id`.
    #[test]
    fn multi_page_sequence_round_trips() {
        let page0 = NotifyCustomerInformationRequest {
            data: "part 1 of 2".to_string(),
            tbc: Some(true),
            seq_no: 0,
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            request_id: 99,
            custom_data: None,
        };
        let page1 = NotifyCustomerInformationRequest {
            data: "part 2 of 2".to_string(),
            tbc: None,
            seq_no: 1,
            generated_at: "2022-01-01T10:00:01Z".to_string(),
            request_id: 99,
            custom_data: None,
        };
        for page in [page0, page1] {
            let value = serde_json::to_value(&page).unwrap();
            let parsed: NotifyCustomerInformationRequest = serde_json::from_value(value).unwrap();
            assert_eq!(parsed, page);
        }
    }

    #[test]
    fn response_round_trips_empty() {
        let resp = NotifyCustomerInformationResponse::default();
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({}));
        let parsed: NotifyCustomerInformationResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn request_missing_required_field_fails() {
        // `data` is required.
        let err = serde_json::from_value::<NotifyCustomerInformationRequest>(json!({
            "seqNo": 0,
            "generatedAt": "2022-01-01T10:00:00Z",
            "requestId": 1
        }))
        .unwrap_err();
        assert!(err.to_string().contains("data"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            NotifyCustomerInformationRequest::ACTION_NAME,
            "NotifyCustomerInformation"
        );
        assert_eq!(
            NotifyCustomerInformationResponse::ACTION_NAME,
            "NotifyCustomerInformationResponse"
        );
    }
}
