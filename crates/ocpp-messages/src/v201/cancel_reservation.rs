//! `CancelReservation` — the CSMS asks a Charging Station to drop a previously
//! made reservation.
//!
//! Ports `ocpp.v201.call.CancelReservation` /
//! `ocpp.v201.call_result.CancelReservation`. This is the 2.0.1 successor to
//! 1.6J `CancelReservation`: one of the smallest command pairs — a single
//! required `reservationId` in, a status out.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CancelReservationStatusEnumType, CustomDataType, StatusInfoType};
use serde::{Deserialize, Serialize};

/// `CancelReservation.req` — sent by the CSMS to cancel the reservation
/// identified by `reservationId`.
///
/// Ports `ocpp.v201.call.CancelReservation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelReservationRequest {
    /// Id of the reservation to cancel.
    #[serde(rename = "reservationId")]
    pub reservation_id: i32,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CancelReservationRequest {
    const ACTION_NAME: &'static str = "CancelReservation";
    type Response = CancelReservationResponse;
}

/// `CancelReservation.conf` — the Charging Station's reply, stating whether it
/// cancelled the reservation.
///
/// Ports `ocpp.v201.call_result.CancelReservation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelReservationResponse {
    /// `Accepted` if the reservation was cancelled, otherwise `Rejected`.
    pub status: CancelReservationStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for CancelReservationResponse {
    const ACTION_NAME: &'static str = "CancelReservationResponse";
    type Response = Self;
}

impl OcppResponse for CancelReservationResponse {}
