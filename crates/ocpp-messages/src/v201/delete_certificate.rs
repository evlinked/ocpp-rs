//! `DeleteCertificate` — the CSMS removes a previously installed certificate
//! from a Charging Station's trust store.
//!
//! Ports `ocpp.v201.call.DeleteCertificate` /
//! `ocpp.v201.call_result.DeleteCertificate`. It is the removal side of the
//! OCPP 2.0.1 certificate-*management* family
//! ([`InstallCertificate`](crate::v201) installs a root,
//! `GetInstalledCertificateIds` queries them), distinct from the
//! certificate-*provisioning* pair [`SignCertificate`](crate::v201) /
//! [`CertificateSigned`](crate::v201) that gets the station's own certificate
//! signed. The CSMS identifies the certificate to remove by its
//! [`CertificateHashDataType`] (issuer-name / issuer-key / serial-number hash
//! triple) rather than by transmitting the certificate itself, and the station
//! answers with a [`DeleteCertificateStatusEnumType`] reporting whether it was
//! found and removed.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CertificateHashDataType, CustomDataType, DeleteCertificateStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `DeleteCertificate.req` — sent by the CSMS to remove one installed
/// certificate, identified by its hash.
///
/// Ports `ocpp.v201.call.DeleteCertificate`. `certificate_hash_data` is the only
/// required field; it names the certificate to delete without carrying the
/// certificate contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteCertificateRequest {
    /// The hash identifying the certificate to remove.
    #[serde(rename = "certificateHashData")]
    pub certificate_hash_data: CertificateHashDataType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for DeleteCertificateRequest {
    const ACTION_NAME: &'static str = "DeleteCertificate";
    type Response = DeleteCertificateResponse;
}

/// `DeleteCertificate.conf` — the station's synchronous report of whether it
/// removed the certificate.
///
/// Ports `ocpp.v201.call_result.DeleteCertificate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteCertificateResponse {
    /// Whether the certificate was found and removed.
    pub status: DeleteCertificateStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for DeleteCertificateResponse {
    const ACTION_NAME: &'static str = "DeleteCertificateResponse";
    type Response = Self;
}

impl OcppResponse for DeleteCertificateResponse {}
