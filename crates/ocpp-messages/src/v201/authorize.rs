//! `Authorize` — a Charging Station asks the CSMS whether an `idToken` may
//! start/stop charging.
//!
//! Ports `ocpp.v201.call.Authorize` / `ocpp.v201.call_result.Authorize`.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, IdTokenInfoType, IdTokenType};
use serde::{Deserialize, Serialize};

/// `Authorize.req` — a Charging Station asks the CSMS whether an `idToken` is
/// authorized to start/stop charging.
///
/// Ports `ocpp.v201.call.Authorize`. Unlike 1.6J (a bare `idTag` string), 2.0.1
/// carries the richer [`IdTokenType`].
///
/// **Deferred:** the ISO 15118 plug-and-charge certificate path — the request's
/// optional `certificate` (PEM) and `iso15118CertificateHashData`
/// (`OCSPRequestDataType` list) — is not yet modelled here; it is tracked as a
/// follow-up. The bundled `Authorize.json` schema still validates those fields
/// when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeRequest {
    /// The identifier being authorized.
    #[serde(rename = "idToken")]
    pub id_token: IdTokenType,
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
/// **Deferred:** the optional `certificateStatus`
/// (`AuthorizeCertificateStatusEnumType`) field, part of the same ISO 15118
/// certificate path as the request-side certificate fields, is not yet
/// modelled. The bundled schema still validates it when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeResponse {
    /// Status information about the identifier.
    #[serde(rename = "idTokenInfo")]
    pub id_token_info: IdTokenInfoType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for AuthorizeResponse {
    const ACTION_NAME: &'static str = "AuthorizeResponse";
    type Response = Self;
}

impl OcppResponse for AuthorizeResponse {}
