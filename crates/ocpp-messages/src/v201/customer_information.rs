//! `CustomerInformation` — the CSMS asks a Charging Station to report and/or
//! clear the customer data it has stored.
//!
//! Ports `ocpp.v201.call.CustomerInformation` /
//! `ocpp.v201.call_result.CustomerInformation`. This is the OCPP 2.0.1
//! privacy/GDPR command (the "right to access" / "right to be forgotten" flow):
//! the CSMS identifies a customer by one of three optional selectors — a hashed
//! customer certificate ([`CertificateHashDataType`]), an authorization token
//! ([`IdTokenType`]), or a free-form `customerIdentifier` string — and asks the
//! station to `report` the stored data, `clear` it, or both. The station answers
//! synchronously with a [`CustomerInformationStatusEnumType`] accept/reject; when
//! `report` is set, the actual report data is streamed back asynchronously via
//! its counterpart `NotifyCustomerInformation` (a paged text carrier, tracked as
//! a follow-up), correlated by `requestId`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CertificateHashDataType, CustomDataType, CustomerInformationStatusEnumType, IdTokenType,
    StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `CustomerInformation.req` — sent by the CSMS to have a station report and/or
/// clear stored customer data.
///
/// Ports `ocpp.v201.call.CustomerInformation`. `request_id`, `report`, and
/// `clear` are required; the three selectors are optional — the spec expects at
/// least one of `customer_certificate` / `id_token` / `customer_identifier` to
/// be present, but the schema does not enforce that, so neither do we.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomerInformationRequest {
    /// Correlates the (asynchronous) `NotifyCustomerInformation` report pages
    /// back to this request.
    #[serde(rename = "requestId")]
    pub request_id: i32,
    /// Whether the station should return the stored customer information (via
    /// `NotifyCustomerInformation`).
    pub report: bool,
    /// Whether the station should clear all information about the customer.
    pub clear: bool,
    /// Customer selector: the hash of a customer certificate.
    #[serde(
        rename = "customerCertificate",
        skip_serializing_if = "Option::is_none"
    )]
    pub customer_certificate: Option<CertificateHashDataType>,
    /// Customer selector: an authorization token.
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<IdTokenType>,
    /// Customer selector: a vendor-specific identifier other than an idToken or
    /// certificate (max length 64, enforced at the schema layer).
    #[serde(rename = "customerIdentifier", skip_serializing_if = "Option::is_none")]
    pub customer_identifier: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CustomerInformationRequest {
    const ACTION_NAME: &'static str = "CustomerInformation";
    type Response = CustomerInformationResponse;
}

/// `CustomerInformation.conf` — the station's synchronous accept/reject of the
/// request.
///
/// Ports `ocpp.v201.call_result.CustomerInformation`. Only `status` is required.
/// A positive `status` acknowledges the command; the report data itself (when
/// `report` was set) arrives later via `NotifyCustomerInformation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomerInformationResponse {
    /// Whether the station accepted the request.
    pub status: CustomerInformationStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CustomerInformationResponse {
    const ACTION_NAME: &'static str = "CustomerInformationResponse";
    type Response = Self;
}

impl OcppResponse for CustomerInformationResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{HashAlgorithmEnumType, IdTokenEnumType};
    use serde_json::json;

    fn sample_certificate() -> CertificateHashDataType {
        CertificateHashDataType {
            hash_algorithm: HashAlgorithmEnumType::Sha256,
            issuer_name_hash: "a1".to_string(),
            issuer_key_hash: "b2".to_string(),
            serial_number: "c3".to_string(),
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = CustomerInformationRequest {
            request_id: 42,
            report: true,
            clear: false,
            customer_certificate: None,
            id_token: None,
            customer_identifier: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Only the three required fields hit the wire; every absent selector is
        // omitted.
        assert_eq!(
            value,
            json!({
                "requestId": 42,
                "report": true,
                "clear": false
            })
        );
        let parsed: CustomerInformationRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_all_selectors() {
        let req = CustomerInformationRequest {
            request_id: 7,
            report: false,
            clear: true,
            customer_certificate: Some(sample_certificate()),
            id_token: Some(IdTokenType {
                id_token: "RFID-1234".to_string(),
                kind: IdTokenEnumType::Iso14443,
                additional_info: None,
                custom_data: None,
            }),
            customer_identifier: Some("customer-abc".to_string()),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(
            value,
            json!({
                "requestId": 7,
                "report": false,
                "clear": true,
                "customerCertificate": {
                    "hashAlgorithm": "SHA256",
                    "issuerNameHash": "a1",
                    "issuerKeyHash": "b2",
                    "serialNumber": "c3"
                },
                "idToken": {
                    "idToken": "RFID-1234",
                    "type": "ISO14443"
                },
                "customerIdentifier": "customer-abc"
            })
        );
        let parsed: CustomerInformationRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let value = serde_json::to_value(CustomerInformationRequest {
            request_id: -3,
            report: true,
            clear: true,
            customer_certificate: None,
            id_token: None,
            customer_identifier: None,
            custom_data: None,
        })
        .unwrap();
        assert!(value["requestId"].is_i64());
        assert!(value["report"].is_boolean());
        assert!(value["clear"].is_boolean());
    }

    #[test]
    fn response_round_trips_minimal_and_full() {
        let resp = CustomerInformationResponse {
            status: CustomerInformationStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: CustomerInformationResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);

        let full = CustomerInformationResponse {
            status: CustomerInformationStatusEnumType::Rejected,
            status_info: Some(StatusInfoType {
                reason_code: "UnknownCustomer".to_string(),
                additional_info: Some("no data for selector".to_string()),
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&full).unwrap();
        assert_eq!(
            value,
            json!({
                "status": "Rejected",
                "statusInfo": {
                    "reasonCode": "UnknownCustomer",
                    "additionalInfo": "no data for selector"
                }
            })
        );
        let parsed: CustomerInformationResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, full);
    }

    #[test]
    fn status_enum_wire_spellings_and_unknown_rejected() {
        for (variant, wire) in [
            (CustomerInformationStatusEnumType::Accepted, "Accepted"),
            (CustomerInformationStatusEnumType::Rejected, "Rejected"),
            (CustomerInformationStatusEnumType::Invalid, "Invalid"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let parsed: CustomerInformationStatusEnumType =
                serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(parsed, variant);
        }
        assert!(
            serde_json::from_value::<CustomerInformationStatusEnumType>(json!("Maybe")).is_err()
        );
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            CustomerInformationRequest::ACTION_NAME,
            "CustomerInformation"
        );
        assert_eq!(
            CustomerInformationResponse::ACTION_NAME,
            "CustomerInformationResponse"
        );
    }
}
