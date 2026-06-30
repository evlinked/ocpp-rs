//! `ClearedChargingLimit` — the Charging Station tells the CSMS that an
//! externally-set charging limit is no longer in effect.
//!
//! Ports `ocpp.v201.call.ClearedChargingLimit` /
//! `ocpp.v201.call_result.ClearedChargingLimit`. Sent when a limit previously
//! imposed by an EMS, system operator or charging-station operator is removed:
//! a [`ChargingLimitSourceEnumType`] plus an optional `evseId` in, an empty
//! response out.

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{ChargingLimitSourceEnumType, CustomDataType};
use serde::{Deserialize, Serialize};

/// `ClearedChargingLimit.req` — sent by the Charging Station when an external
/// charging limit has been cleared.
///
/// Ports `ocpp.v201.call.ClearedChargingLimit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearedChargingLimitRequest {
    /// Source of the charging limit that was cleared.
    #[serde(rename = "chargingLimitSource")]
    pub charging_limit_source: ChargingLimitSourceEnumType,
    /// The EVSE the limit applied to. Absent when the limit was station-wide.
    #[serde(rename = "evseId", skip_serializing_if = "Option::is_none")]
    pub evse_id: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearedChargingLimitRequest {
    const ACTION_NAME: &'static str = "ClearedChargingLimit";
    type Response = ClearedChargingLimitResponse;
}

/// `ClearedChargingLimit.conf` — the CSMS acknowledgement. The 2.0.1 schema
/// carries no fields beyond the optional vendor extension, so it serializes to
/// an empty object `{}`.
///
/// Ports `ocpp.v201.call_result.ClearedChargingLimit`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClearedChargingLimitResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for ClearedChargingLimitResponse {
    const ACTION_NAME: &'static str = "ClearedChargingLimitResponse";
    type Response = Self;
}

impl OcppResponse for ClearedChargingLimitResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trips_minimal() {
        let req = ClearedChargingLimitRequest {
            charging_limit_source: ChargingLimitSourceEnumType::Ems,
            evse_id: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // `evseId` and `customData` are omitted when `None`.
        assert_eq!(value, json!({ "chargingLimitSource": "EMS" }));
        let parsed: ClearedChargingLimitRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_with_evse_id() {
        let req = ClearedChargingLimitRequest {
            charging_limit_source: ChargingLimitSourceEnumType::Cso,
            evse_id: Some(3),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value, json!({ "chargingLimitSource": "CSO", "evseId": 3 }));
        let parsed: ClearedChargingLimitRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_serializes_custom_data() {
        let req = ClearedChargingLimitRequest {
            charging_limit_source: ChargingLimitSourceEnumType::So,
            evse_id: None,
            custom_data: Some(CustomDataType {
                vendor_id: "ACME".to_string(),
                extra: Default::default(),
            }),
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["chargingLimitSource"], json!("SO"));
        assert_eq!(value["customData"]["vendorId"], json!("ACME"));
    }

    #[test]
    fn source_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(ChargingLimitSourceEnumType::Ems).unwrap(),
            json!("EMS")
        );
        assert_eq!(
            serde_json::to_value(ChargingLimitSourceEnumType::Other).unwrap(),
            json!("Other")
        );
        assert_eq!(
            serde_json::to_value(ChargingLimitSourceEnumType::So).unwrap(),
            json!("SO")
        );
        assert_eq!(
            serde_json::to_value(ChargingLimitSourceEnumType::Cso).unwrap(),
            json!("CSO")
        );
    }

    #[test]
    fn request_missing_source_fails() {
        let err = serde_json::from_value::<ClearedChargingLimitRequest>(json!({ "evseId": 1 }))
            .unwrap_err();
        assert!(err.to_string().contains("chargingLimitSource"));
    }

    #[test]
    fn request_rejects_unknown_source() {
        let err = serde_json::from_value::<ClearedChargingLimitRequest>(
            json!({ "chargingLimitSource": "DSO" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("DSO") || err.to_string().contains("variant"));
    }

    #[test]
    fn response_is_empty_object_on_wire() {
        let resp = ClearedChargingLimitResponse::default();
        assert_eq!(serde_json::to_value(&resp).unwrap(), json!({}));
        let parsed: ClearedChargingLimitResponse = serde_json::from_value(json!({})).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            ClearedChargingLimitRequest::ACTION_NAME,
            "ClearedChargingLimit"
        );
        assert_eq!(
            ClearedChargingLimitResponse::ACTION_NAME,
            "ClearedChargingLimitResponse"
        );
    }
}
