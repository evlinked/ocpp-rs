//! `ReservationStatusUpdate` — the Charging Station tells the CSMS that a
//! previously-made reservation is no longer valid.
//!
//! Ports `ocpp.v201.call.ReservationStatusUpdate` /
//! `ocpp.v201.call_result.ReservationStatusUpdate`. This completes the 2.0.1
//! reservation message family alongside the already-ported
//! [`ReserveNow`](crate::v201::ReserveNowRequest) and
//! [`CancelReservation`](crate::v201::CancelReservationRequest): one of the
//! smallest 2.0.1 notifications — a `reservationId` plus a
//! [`ReservationUpdateStatusEnumType`] in, an empty response out.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, ReservationUpdateStatusEnumType};
use serde::{Deserialize, Serialize};

/// `ReservationStatusUpdate.req` — sent by the Charging Station when a
/// reservation has `Expired` or been `Removed`.
///
/// Ports `ocpp.v201.call.ReservationStatusUpdate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReservationStatusUpdateRequest {
    /// The id of the reservation whose status changed.
    #[serde(rename = "reservationId")]
    pub reservation_id: i32,
    /// The new state of the reservation (`Expired` / `Removed`).
    #[serde(rename = "reservationUpdateStatus")]
    pub reservation_update_status: ReservationUpdateStatusEnumType,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ReservationStatusUpdateRequest {
    const ACTION_NAME: &'static str = "ReservationStatusUpdate";
    type Response = ReservationStatusUpdateResponse;
}

/// `ReservationStatusUpdate.conf` — the CSMS acknowledgement. The 2.0.1 schema
/// carries no fields beyond the optional vendor extension, so it serializes to
/// an empty object `{}`.
///
/// Ports `ocpp.v201.call_result.ReservationStatusUpdate`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReservationStatusUpdateResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ReservationStatusUpdateResponse {
    const ACTION_NAME: &'static str = "ReservationStatusUpdateResponse";
    type Response = Self;
}

impl OcppResponse for ReservationStatusUpdateResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = ReservationStatusUpdateRequest {
            reservation_id: 42,
            reservation_update_status: ReservationUpdateStatusEnumType::Expired,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `customData` is omitted when `None`.
        assert_eq!(
            value,
            json!({ "reservationId": 42, "reservationUpdateStatus": "Expired" })
        );
        let parsed: ReservationStatusUpdateRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_serializes_custom_data() {
        let req = ReservationStatusUpdateRequest {
            reservation_id: 7,
            reservation_update_status: ReservationUpdateStatusEnumType::Removed,
            custom_data: Some(CustomDataType {
                vendor_id: "ACME".to_string(),
                extra: Default::default(),
            }),
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["reservationId"], json!(7));
        assert_eq!(value["reservationUpdateStatus"], json!("Removed"));
        assert_eq!(value["customData"]["vendorId"], json!("ACME"));
    }

    #[test]
    fn request_missing_reservation_id_fails() {
        let err = serde_json::from_value::<ReservationStatusUpdateRequest>(
            json!({ "reservationUpdateStatus": "Expired" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("reservationId"));
    }

    #[test]
    fn request_missing_status_fails() {
        let err =
            serde_json::from_value::<ReservationStatusUpdateRequest>(json!({ "reservationId": 1 }))
                .unwrap_err();
        assert!(err.to_string().contains("reservationUpdateStatus"));
    }

    #[test]
    fn status_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(ReservationUpdateStatusEnumType::Expired).unwrap(),
            json!("Expired")
        );
        assert_eq!(
            serde_json::to_value(ReservationUpdateStatusEnumType::Removed).unwrap(),
            json!("Removed")
        );
    }

    #[test]
    fn request_rejects_unknown_status() {
        let err = serde_json::from_value::<ReservationStatusUpdateRequest>(
            json!({ "reservationId": 1, "reservationUpdateStatus": "Cancelled" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Cancelled") || err.to_string().contains("variant"));
    }

    #[test]
    fn response_is_empty_object_on_wire() {
        let resp = ReservationStatusUpdateResponse::default();
        assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
        let parsed: ReservationStatusUpdateResponse = serde_json::from_value(json!({})).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            ReservationStatusUpdateRequest::ACTION_NAME,
            "ReservationStatusUpdate"
        );
        assert_eq!(
            ReservationStatusUpdateResponse::ACTION_NAME,
            "ReservationStatusUpdateResponse"
        );
    }
}
