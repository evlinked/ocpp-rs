//! `InstallCertificate` — the CSMS installs a new root (CA) certificate into a
//! Charging Station's trust store.
//!
//! Ports `ocpp.v201.call.InstallCertificate` /
//! `ocpp.v201.call_result.InstallCertificate`. This opens the OCPP 2.0.1
//! certificate-*management* family (`InstallCertificate` / `DeleteCertificate` /
//! `GetInstalledCertificateIds`), which manages the *root/trust* certificates a
//! station holds — distinct from the certificate-*provisioning* pair
//! [`SignCertificate`](crate::v201) / [`CertificateSigned`](crate::v201), which
//! gets the station's *own* certificate signed. The CSMS hands over a PEM-encoded
//! X.509 root certificate together with an [`InstallCertificateUseEnumType`]
//! selecting which trust anchor it is (V2G / MO / CSMS / manufacturer root), and
//! the station answers with an [`InstallCertificateStatusEnumType`] reporting
//! whether it installed it.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CustomDataType, InstallCertificateStatusEnumType, InstallCertificateUseEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `InstallCertificate.req` — sent by the CSMS to install a root certificate into
/// a Charging Station's trust store.
///
/// Ports `ocpp.v201.call.InstallCertificate`. `certificate` is a PEM-encoded
/// X.509 root certificate; the schema caps it at 5500 characters, enforced at the
/// schema layer consistent with the rest of v201.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallCertificateRequest {
    /// Which trust anchor the certificate is (V2G / MO / CSMS / manufacturer root).
    #[serde(rename = "certificateType")]
    pub certificate_type: InstallCertificateUseEnumType,
    /// The PEM-encoded X.509 root certificate to install.
    pub certificate: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for InstallCertificateRequest {
    const ACTION_NAME: &'static str = "InstallCertificate";
    type Response = InstallCertificateResponse;
}

/// `InstallCertificate.conf` — the station's synchronous acknowledgement of
/// whether it installed the delivered root certificate.
///
/// Ports `ocpp.v201.call_result.InstallCertificate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallCertificateResponse {
    /// Whether the station installed the certificate.
    pub status: InstallCertificateStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for InstallCertificateResponse {
    const ACTION_NAME: &'static str = "InstallCertificateResponse";
    type Response = Self;
}

impl OcppResponse for InstallCertificateResponse {}
