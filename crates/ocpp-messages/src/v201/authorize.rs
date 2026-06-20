//! `Authorize` — a Charging Station asks the CSMS whether an `idToken` may
//! start/stop charging.
//!
//! Ports `ocpp.v201.call.Authorize` / `ocpp.v201.call_result.Authorize`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    AuthorizeCertificateStatusEnumType, CustomDataType, IdTokenInfoType, IdTokenType,
    OCSPRequestDataType,
};
use serde::{Deserialize, Serialize};

/// `Authorize.req` — a Charging Station asks the CSMS whether an `idToken` is
/// authorized to start/stop charging.
///
/// Ports `ocpp.v201.call.Authorize`. Unlike 1.6J (a bare `idTag` string), 2.0.1
/// carries the richer [`IdTokenType`].
///
/// The optional ISO 15118 plug-and-charge certificate path is modelled here:
/// `certificate` carries the EV's contract certificate (PEM, max length 5500)
/// and `iso15118_certificate_hash_data` carries 1..=4 [`OCSPRequestDataType`]
/// entries for OCSP status checking. Both are omitted from the wire when absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeRequest {
    /// The identifier being authorized.
    #[serde(rename = "idToken")]
    pub id_token: IdTokenType,
    /// The X.509 contract certificate presented by the EV, PEM-encoded
    /// (max length 5500). Part of the ISO 15118 plug-and-charge path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<String>,
    /// OCSP request data for the contract certificate chain (1..=4 entries).
    /// Omitted entirely when absent; the schema requires at least one item and
    /// at most four when present.
    #[serde(
        rename = "iso15118CertificateHashData",
        skip_serializing_if = "Option::is_none"
    )]
    pub iso15118_certificate_hash_data: Option<Vec<OCSPRequestDataType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for AuthorizeRequest {
    const ACTION_NAME: &'static str = "Authorize";
    type Response = AuthorizeResponse;
}

/// `Authorize.conf` — the CSMS's authorization decision.
///
/// Ports `ocpp.v201.call_result.Authorize`. The [`IdTokenInfoType`] payload is
/// reused by the 2.0.1 transaction model.
///
/// The optional `certificate_status` reports the outcome of validating the
/// contract certificate supplied along the ISO 15118 plug-and-charge path; it
/// is omitted from the wire when absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeResponse {
    /// Status information about the identifier.
    #[serde(rename = "idTokenInfo")]
    pub id_token_info: IdTokenInfoType,
    /// Result of validating the ISO 15118 contract certificate, when one was
    /// presented in the request.
    #[serde(rename = "certificateStatus", skip_serializing_if = "Option::is_none")]
    pub certificate_status: Option<AuthorizeCertificateStatusEnumType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for AuthorizeResponse {
    const ACTION_NAME: &'static str = "AuthorizeResponse";
    type Response = Self;
}

impl OcppResponse for AuthorizeResponse {}
