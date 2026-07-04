//! `CertificateSigned` — the CSMS delivers a signed certificate chain to a
//! Charging Station.
//!
//! Ports `ocpp.v201.call.CertificateSigned` /
//! `ocpp.v201.call_result.CertificateSigned`. It is the delivery half of the
//! OCPP 2.0.1 certificate-provisioning flow whose request half is
//! [`SignCertificate`](crate::v201): after the operator's CA signs the CSR the
//! station submitted, the CSMS pushes the resulting PEM-encoded chain here, and
//! the station installs it and answers with a
//! [`CertificateSignedStatusEnumType`] accept/reject status. The optional
//! [`CertificateSigningUseEnumType`] indicates which certificate the chain is
//! for, mirroring the `certificateType` the station may have sent on its
//! `SignCertificate` request.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CertificateSignedStatusEnumType, CertificateSigningUseEnumType, CustomDataType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `CertificateSigned.req` — sent by the CSMS to hand a signed certificate chain
/// to a Charging Station.
///
/// Ports `ocpp.v201.call.CertificateSigned`. `certificate_chain` is the signed,
/// PEM-encoded X.509 certificate, optionally bundled with the sub-CA chain (leaf
/// first); the schema caps it at 10000 characters, enforced at the schema layer
/// consistent with the rest of v201.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertificateSignedRequest {
    /// The signed, PEM-encoded X.509 certificate chain (leaf first).
    #[serde(rename = "certificateChain")]
    pub certificate_chain: String,
    /// Which certificate the chain is for. Absent applies to both the ISO 15118
    /// connection and the Charging-Station-to-CSMS connection.
    #[serde(rename = "certificateType", skip_serializing_if = "Option::is_none")]
    pub certificate_type: Option<CertificateSigningUseEnumType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CertificateSignedRequest {
    const ACTION_NAME: &'static str = "CertificateSigned";
    type Response = CertificateSignedResponse;
}

/// `CertificateSigned.conf` — the station's synchronous acknowledgement of
/// whether it installed the delivered certificate chain.
///
/// Ports `ocpp.v201.call_result.CertificateSigned`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertificateSignedResponse {
    /// Whether the station accepted and installed the certificate chain.
    pub status: CertificateSignedStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CertificateSignedResponse {
    const ACTION_NAME: &'static str = "CertificateSignedResponse";
    type Response = Self;
}

impl OcppResponse for CertificateSignedResponse {}
