//! `GetCertificateStatus` — a Charging Station asks the CSMS for the OCSP status
//! of a certificate.
//!
//! Ports `ocpp.v201.call.GetCertificateStatus` /
//! `ocpp.v201.call_result.GetCertificateStatus`. During ISO 15118
//! plug-and-charge the station must validate a contract/sub-CA certificate but
//! cannot (or should not) reach the OCSP responder itself. It hands the CSMS an
//! [`OCSPRequestDataType`] — the issuer-name / issuer-key / serial-number hash
//! triple plus the responder URL — and the CSMS performs the OCSP lookup on its
//! behalf, returning a [`GetCertificateStatusEnumType`] and, on success, the
//! DER-then-base64-encoded OCSP response in `ocspResult`. This completes the OCSP
//! query side of the certificate family (`InstallCertificate` /
//! `DeleteCertificate` / `GetInstalledCertificateIds` / `SignCertificate` /
//! `CertificateSigned`).

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, GetCertificateStatusEnumType, OCSPRequestDataType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetCertificateStatus.req` — sent by a Charging Station to have the CSMS
/// retrieve the OCSP status of a certificate.
///
/// Ports `ocpp.v201.call.GetCertificateStatus`. `ocspRequestData` is the only
/// required field; it reuses the shared [`OCSPRequestDataType`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCertificateStatusRequest {
    /// The OCSP request identifying the certificate whose status is sought.
    #[serde(rename = "ocspRequestData")]
    pub ocsp_request_data: OCSPRequestDataType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetCertificateStatusRequest {
    const ACTION_NAME: &'static str = "GetCertificateStatus";
    type Response = GetCertificateStatusResponse;
}

/// `GetCertificateStatus.conf` — the CSMS's reply carrying the OCSP lookup
/// outcome and, when successful, the OCSP response itself.
///
/// Ports `ocpp.v201.call_result.GetCertificateStatus`. `ocspResult` is the
/// DER-encoded OCSP response, base64-encoded; the schema caps it at 5500
/// characters (enforced at the schema layer, consistent with the rest of v201)
/// and it may only be omitted when `status` is not `Accepted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCertificateStatusResponse {
    /// Whether the CSMS could retrieve the OCSP certificate status.
    pub status: GetCertificateStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// The base64-encoded, DER-encoded OCSP response (max length 5500). Present
    /// only when the lookup succeeded.
    #[serde(rename = "ocspResult", skip_serializing_if = "Option::is_none")]
    pub ocsp_result: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetCertificateStatusResponse {
    const ACTION_NAME: &'static str = "GetCertificateStatusResponse";
    type Response = Self;
}

impl OcppResponse for GetCertificateStatusResponse {}
