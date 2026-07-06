//! `Get15118EVCertificate` — a Charging Station relays an ISO 15118 EV
//! certificate request to the CSMS.
//!
//! Ports `ocpp.v201.call.Get15118EVCertificate` /
//! `ocpp.v201.call_result.Get15118EVCertificate`. This is the Charging Station →
//! CSMS leg of the ISO 15118 Plug-and-Charge certificate exchange: during a
//! 15118 session the EV emits a raw EXI `CertificateInstallationReq`, which the
//! station cannot interpret and simply forwards up to the CSMS (which in turn
//! relays it to the contract-certificate backend / V2G root). The CSMS answers
//! with an [`Iso15118EVCertificateStatusEnumType`] and, on success, the EXI
//! `CertificateInstallationRes` in `exiResponse`, which the station passes back
//! down to the EV untouched.
//!
//! The station is a transparent relay: `exiRequest` / `exiResponse` are opaque
//! base64-EXI blobs that are neither parsed nor validated here beyond the
//! schema's `maxLength` caps. This is the query side of the 15118 certificate
//! family, complementing the certificate-management messages
//! (`InstallCertificate` / `DeleteCertificate` / `GetInstalledCertificateIds` /
//! `CertificateSigned` / `GetCertificateStatus`).

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    CertificateActionEnumType, CustomDataType, Iso15118EVCertificateStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `Get15118EVCertificate.req` — sent by a Charging Station to have the CSMS
/// process an EV's ISO 15118 certificate-installation/-update request.
///
/// Ports `ocpp.v201.call.Get15118EVCertificate`. All three fields are required.
/// `exiRequest` is the EV's raw `CertificateInstallationReq`, base64-encoded;
/// the schema caps it at 5600 characters (enforced at the schema layer,
/// consistent with the rest of v201) and it is relayed verbatim — the station
/// does not decode it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Get15118EVCertificateRequest {
    /// The ISO 15118 schema version in use for the EV↔station session, needed by
    /// the CSMS to parse the EXI stream (max length 50).
    #[serde(rename = "iso15118SchemaVersion")]
    pub iso15118_schema_version: String,
    /// Whether the EV's certificate should be installed or updated.
    pub action: CertificateActionEnumType,
    /// The EV's raw `CertificateInstallationReq`, base64-encoded (max length
    /// 5600). Relayed opaquely.
    #[serde(rename = "exiRequest")]
    pub exi_request: String,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for Get15118EVCertificateRequest {
    const ACTION_NAME: &'static str = "Get15118EVCertificate";
    type Response = Get15118EVCertificateResponse;
}

/// `Get15118EVCertificate.conf` — the CSMS's reply carrying the processing
/// outcome and the EXI response to hand back to the EV.
///
/// Ports `ocpp.v201.call_result.Get15118EVCertificate`. `status` and
/// `exiResponse` are both required — even on `Failed` the schema requires an
/// `exiResponse` string (an empty string is used when the CSMS produced no
/// response). `exiResponse` is the base64-encoded `CertificateInstallationRes`;
/// the schema caps it at 7500 characters (enforced at the schema layer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Get15118EVCertificateResponse {
    /// Whether the CSMS could process the relayed certificate request.
    pub status: Iso15118EVCertificateStatusEnumType,
    /// The EV's `CertificateInstallationRes`, base64-encoded (max length 7500).
    /// Relayed opaquely back to the EV.
    #[serde(rename = "exiResponse")]
    pub exi_response: String,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for Get15118EVCertificateResponse {
    const ACTION_NAME: &'static str = "Get15118EVCertificateResponse";
    type Response = Self;
}

impl OcppResponse for Get15118EVCertificateResponse {}
