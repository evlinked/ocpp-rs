//! `DataTransfer` — the 2.0.1 bidirectional vendor-extension escape hatch.
//!
//! Ports `ocpp.v201.call.DataTransfer` / `ocpp.v201.call_result.DataTransfer`.
//! This is the 2.0.1 successor to 1.6J `DataTransfer`: either side sends it to
//! exchange data that no standard message covers. A required `vendorId` scopes
//! the exchange; an optional `messageId` and free-form `data` carry the
//! payload, and the reply returns a [`DataTransferStatusEnumType`] plus its own
//! optional `data`.
//!
//! The `data` field is `Optional[Any]` in the reference, so it is modelled as
//! [`serde_json::Value`] to round-trip arbitrary JSON (object, array, string,
//! number, bool) without loss. The one edge is a *bare* top-level `null`:
//! `Some(Value::Null)` serializes to `"data": null` but serde reads a JSON
//! `null` back into an `Option` field as `None`, so an explicit null and an
//! omitted `data` are indistinguishable after a read — semantically equivalent
//! for OCPP. The freedom is *inside* `data`: the FINAL schema still pins
//! `additionalProperties: false` at the message root, so unexpected top-level
//! keys are rejected.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, DataTransferStatusEnumType, StatusInfoType};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `DataTransfer.req` — sent by either peer to exchange vendor-specific data
/// that no standard message covers.
///
/// Ports `ocpp.v201.call.DataTransfer`. `vendor_id` is required and identifies
/// the vendor-specific implementation; `message_id` and `data` are optional and
/// their meaning is agreed out-of-band by both parties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataTransferRequest {
    /// Identifies the vendor-specific implementation (required).
    #[serde(rename = "vendorId")]
    pub vendor_id: String,
    /// May indicate a specific message or implementation within the vendor's
    /// namespace.
    #[serde(rename = "messageId", skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Free-form payload; format is agreed out-of-band by both parties. Any
    /// JSON value (object, array, scalar) round-trips without loss.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for DataTransferRequest {
    const ACTION_NAME: &'static str = "DataTransfer";
    type Response = DataTransferResponse;
}

/// `DataTransfer.conf` — the receiver's reply, reporting whether it accepted the
/// transfer and optionally returning data of its own.
///
/// Ports `ocpp.v201.call_result.DataTransfer`. `UnknownMessageId` /
/// `UnknownVendorId` let the receiver pinpoint which part of the request it did
/// not recognise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataTransferResponse {
    /// Whether the data transfer succeeded, and if not, why.
    pub status: DataTransferStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Free-form payload returned in response to the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for DataTransferResponse {
    const ACTION_NAME: &'static str = "DataTransferResponse";
    type Response = Self;
}

impl OcppResponse for DataTransferResponse {}
