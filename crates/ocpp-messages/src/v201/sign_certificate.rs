//! `SignCertificate` — a Charging Station submits a Certificate Signing Request
//! (CSR) to the CSMS to have the operator's CA sign it.
//!
//! Ports `ocpp.v201.call.SignCertificate` / `ocpp.v201.call_result.SignCertificate`.
//! It is the entry point of the OCPP 2.0.1 certificate-provisioning flow: the
//! station generates a key pair, sends the PEM-encoded `csr` here, and the CSMS
//! later returns the signed chain via the paired
//! [`CertificateSigned`](crate::v201) command. The optional
//! [`CertificateSigningUseEnumType`] selects which certificate the CSR is for;
//! when omitted it applies to both the ISO 15118 connection and the
//! Charging-Station-to-CSMS connection. The response is a single
//! [`GenericStatusEnumType`] acknowledging whether the CSMS can process the
//! request.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CertificateSigningUseEnumType, CustomDataType, GenericStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `SignCertificate.req` — sent by a Charging Station to have its CSR signed by
/// the operator's CA.
///
/// Ports `ocpp.v201.call.SignCertificate`. `csr` is the PEM-encoded Certificate
/// Signing Request (RFC 2986); the schema caps it at 5500 characters, enforced
/// at the schema layer consistent with the rest of v201.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignCertificateRequest {
    /// The PEM-encoded Certificate Signing Request the station wants signed.
    pub csr: String,
    /// Which certificate the CSR is for. Absent applies the request to both the
    /// ISO 15118 connection and the Charging-Station-to-CSMS connection.
    #[serde(rename = "certificateType", skip_serializing_if = "Option::is_none")]
    pub certificate_type: Option<CertificateSigningUseEnumType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SignCertificateRequest {
    const ACTION_NAME: &'static str = "SignCertificate";
    type Response = SignCertificateResponse;
}

/// `SignCertificate.conf` — the CSMS's synchronous acknowledgement of whether it
/// can process the signing request.
///
/// Ports `ocpp.v201.call_result.SignCertificate`. The signed certificate chain
/// itself is delivered later, asynchronously, via the paired `CertificateSigned`
/// command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignCertificateResponse {
    /// Whether the CSMS can process the signing request.
    pub status: GenericStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SignCertificateResponse {
    const ACTION_NAME: &'static str = "SignCertificateResponse";
    type Response = Self;
}

impl OcppResponse for SignCertificateResponse {}
