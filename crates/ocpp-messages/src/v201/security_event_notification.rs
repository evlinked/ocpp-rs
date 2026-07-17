//! `SecurityEventNotification` — the Charging Station reports a
//! security-relevant event (tamper detection, failed firmware signature,
//! invalid credentials, reboot, …) to the CSMS.
//!
//! Ports `ocpp.v201.call.SecurityEventNotification` /
//! `ocpp.v201.call_result.SecurityEventNotification`. The request carries a
//! free-form security-event `type` plus the `timestamp` at which it occurred
//! and optional vendor-specific `techInfo`; the response is empty (only the
//! optional vendor extension), so it serializes to `{}`.
//!
//! The `type` field is an open string in the FINAL schema — the predefined
//! security-event names are not a closed `enum` — so it is modelled as a
//! [`String`], not an enum. The 20 standardized event names (OCPP 2.0.1 Part 2,
//! Appendix 1) are available as the recommendation vocabulary
//! [`ocpp_types::v201::SecurityEventType`] for callers that want to avoid
//! stringly-typed typos; it does not constrain the wire field.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::CustomDataType;
use serde::{Deserialize, Serialize};

/// `SecurityEventNotification.req` — security-event report sent by the Charging
/// Station.
///
/// Ports `ocpp.v201.call.SecurityEventNotification`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityEventNotificationRequest {
    /// Type of the security event. Named `event_type` because `type` is a Rust
    /// keyword; serialized as `"type"` to match the wire format. An open string
    /// (the standard security-event names are a recommendation, not a closed
    /// schema `enum`); the schema bounds it at `maxLength: 50`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// ISO-8601 date-time at which the event occurred. Consistent with every
    /// other v201 timestamp in the crate (the schema enforces
    /// `format: date-time`).
    pub timestamp: String,
    /// Additional vendor-specific information about the event. The schema
    /// bounds it at `maxLength: 255`.
    #[serde(rename = "techInfo", skip_serializing_if = "Option::is_none")]
    pub tech_info: Option<String>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SecurityEventNotificationRequest {
    const ACTION_NAME: &'static str = "SecurityEventNotification";
    type Response = SecurityEventNotificationResponse;
}

/// `SecurityEventNotification.conf` — the CSMS's acknowledgement.
///
/// Ports `ocpp.v201.call_result.SecurityEventNotification`. The response
/// carries no fields beyond the optional vendor extension, so it serializes to
/// `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SecurityEventNotificationResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for SecurityEventNotificationResponse {
    const ACTION_NAME: &'static str = "SecurityEventNotificationResponse";
    type Response = Self;
}

impl OcppResponse for SecurityEventNotificationResponse {}
