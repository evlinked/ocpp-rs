//! `GetInstalledCertificateIds` — the CSMS asks a Charging Station which
//! certificates are installed in its trust store.
//!
//! Ports `ocpp.v201.call.GetInstalledCertificateIds` /
//! `ocpp.v201.call_result.GetInstalledCertificateIds`. It is the query side of
//! the OCPP 2.0.1 certificate-*management* family
//! ([`InstallCertificate`](crate::v201) installs a root,
//! [`DeleteCertificate`](crate::v201) removes one by hash), distinct from the
//! certificate-*provisioning* pair [`SignCertificate`](crate::v201) /
//! [`CertificateSigned`](crate::v201). The CSMS optionally narrows the query to
//! one or more [`GetCertificateIdUseEnumType`] categories; the station answers
//! with a [`GetInstalledCertificateStatusEnumType`] and, when any match, a chain
//! of [`CertificateHashDataChainType`] hash entries — it never transmits the
//! certificates themselves, only their identifying hashes.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CertificateHashDataChainType, CustomDataType, GetCertificateIdUseEnumType,
    GetInstalledCertificateStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `GetInstalledCertificateIds.req` — sent by the CSMS to enumerate the
/// certificates installed on a station.
///
/// Ports `ocpp.v201.call.GetInstalledCertificateIds`. Both fields are optional:
/// when `certificate_type` is omitted, the station reports every installed
/// certificate; when present, the schema requires at least one entry. An empty
/// request therefore serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetInstalledCertificateIdsRequest {
    /// Certificate categories to enumerate; when `None`, all types are
    /// requested. Non-empty when present (schema `minItems` 1).
    #[serde(rename = "certificateType", skip_serializing_if = "Option::is_none")]
    pub certificate_type: Option<Vec<GetCertificateIdUseEnumType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetInstalledCertificateIdsRequest {
    const ACTION_NAME: &'static str = "GetInstalledCertificateIds";
    type Response = GetInstalledCertificateIdsResponse;
}

/// `GetInstalledCertificateIds.conf` — the station's report of which
/// certificates are installed.
///
/// Ports `ocpp.v201.call_result.GetInstalledCertificateIds`. `status` is the
/// only required field; `certificate_hash_data_chain` carries the matching
/// entries (non-empty when present, schema `minItems` 1) and is absent when the
/// status is `NotFound` or nothing matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetInstalledCertificateIdsResponse {
    /// Whether the station could process the request.
    pub status: GetInstalledCertificateStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// The installed certificates matching the request, when any.
    #[serde(
        rename = "certificateHashDataChain",
        skip_serializing_if = "Option::is_none"
    )]
    pub certificate_hash_data_chain: Option<Vec<CertificateHashDataChainType>>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for GetInstalledCertificateIdsResponse {
    const ACTION_NAME: &'static str = "GetInstalledCertificateIdsResponse";
    type Response = Self;
}

impl OcppResponse for GetInstalledCertificateIdsResponse {}
