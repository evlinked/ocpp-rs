//! `NotifyEVChargingNeeds` — the Charging Station reports the charging needs an
//! EV has expressed over ISO 15118 (its requested energy-transfer mode,
//! departure time, and AC or DC charging parameters), so the CSMS can compute
//! and return a charging schedule (SASchedule).
//!
//! Ports `ocpp.v201.call.NotifyEVChargingNeeds` /
//! `ocpp.v201.call_result.NotifyEVChargingNeeds`. The station sends the EV's
//! [`ChargingNeedsType`] for a given `evseId`; the CSMS acks synchronously with
//! a [`NotifyEVChargingNeedsStatusEnumType`] — `Accepted` (a schedule will
//! follow), `Rejected` (service unavailable), or `Processing` (the CSMS is still
//! gathering information). Pulls in a small new datatype tree
//! ([`ChargingNeedsType`], [`ACChargingParametersType`],
//! [`DCChargingParametersType`]) and reuses [`StatusInfoType`].
//!
//! [`ChargingNeedsType`]: ocpp_types::v201::ChargingNeedsType
//! [`ACChargingParametersType`]: ocpp_types::v201::ACChargingParametersType
//! [`DCChargingParametersType`]: ocpp_types::v201::DCChargingParametersType
//! [`NotifyEVChargingNeedsStatusEnumType`]:
//!     ocpp_types::v201::NotifyEVChargingNeedsStatusEnumType
//! [`StatusInfoType`]: ocpp_types::v201::StatusInfoType

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{
    ChargingNeedsType, CustomDataType, NotifyEVChargingNeedsStatusEnumType, StatusInfoType,
};
use serde::{Deserialize, Serialize};

/// `NotifyEVChargingNeeds.req` — sent by the Charging Station to report an EV's
/// charging needs to the CSMS.
///
/// Ports `ocpp.v201.call.NotifyEVChargingNeeds`. `chargingNeeds` and `evseId`
/// are required; per the spec `evseId` may not be `0`. `maxScheduleTuples` is
/// the maximum number of schedule tuples the EV supports per schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyEVChargingNeedsRequest {
    /// The charging needs the EV communicated to the station.
    #[serde(rename = "chargingNeeds")]
    pub charging_needs: ChargingNeedsType,
    /// The EVSE the EV is connected to. Per the spec `evseId` may not be `0`.
    #[serde(rename = "evseId")]
    pub evse_id: i32,
    /// Maximum number of schedule tuples the EV supports per schedule.
    #[serde(rename = "maxScheduleTuples", skip_serializing_if = "Option::is_none")]
    pub max_schedule_tuples: Option<i32>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyEVChargingNeedsRequest {
    const ACTION_NAME: &'static str = "NotifyEVChargingNeeds";
    type Response = NotifyEVChargingNeedsResponse;
}

/// `NotifyEVChargingNeeds.conf` — the CSMS's synchronous acknowledgement.
///
/// Ports `ocpp.v201.call_result.NotifyEVChargingNeeds`. `status` reports only
/// whether the CSMS could process the message; per the spec it does **not**
/// imply that the EV's charging needs can be met with the current charging
/// profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyEVChargingNeedsResponse {
    /// Whether the CSMS processed the message (`Accepted` / `Rejected` /
    /// `Processing`); implies no guarantee the needs can be met.
    pub status: NotifyEVChargingNeedsStatusEnumType,
    /// Optional detail about the status.
    #[serde(rename = "statusInfo", skip_serializing_if = "Option::is_none")]
    pub status_info: Option<StatusInfoType>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyEVChargingNeedsResponse {
    const ACTION_NAME: &'static str = "NotifyEVChargingNeedsResponse";
    type Response = Self;
}

impl OcppResponse for NotifyEVChargingNeedsResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        ACChargingParametersType, DCChargingParametersType, EnergyTransferModeEnumType,
    };
    use serde_json::json;

    fn minimal_needs() -> ChargingNeedsType {
        ChargingNeedsType {
            requested_energy_transfer: EnergyTransferModeEnumType::AcThreePhase,
            departure_time: None,
            ac_charging_parameters: None,
            dc_charging_parameters: None,
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = NotifyEVChargingNeedsRequest {
            charging_needs: minimal_needs(),
            evse_id: 1,
            max_schedule_tuples: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["evseId"], json!(1));
        assert_eq!(
            value["chargingNeeds"]["requestedEnergyTransfer"],
            json!("AC_three_phase")
        );
        // Optional fields omitted when `None`.
        assert!(!value.as_object().unwrap().contains_key("maxScheduleTuples"));
        assert!(!value.as_object().unwrap().contains_key("customData"));
        assert!(!value["chargingNeeds"]
            .as_object()
            .unwrap()
            .contains_key("acChargingParameters"));
        let parsed: NotifyEVChargingNeedsRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_with_ac_parameters_round_trips() {
        let req = NotifyEVChargingNeedsRequest {
            charging_needs: ChargingNeedsType {
                requested_energy_transfer: EnergyTransferModeEnumType::AcSinglePhase,
                departure_time: Some("2022-01-01T12:00:00Z".to_string()),
                ac_charging_parameters: Some(ACChargingParametersType {
                    energy_amount: 20000,
                    ev_min_current: 6,
                    ev_max_current: 32,
                    ev_max_voltage: 230,
                    custom_data: None,
                }),
                dc_charging_parameters: None,
                custom_data: None,
            },
            evse_id: 2,
            max_schedule_tuples: Some(4),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        let ac = &value["chargingNeeds"]["acChargingParameters"];
        assert_eq!(ac["energyAmount"], json!(20000));
        assert_eq!(ac["evMinCurrent"], json!(6));
        assert_eq!(ac["evMaxCurrent"], json!(32));
        assert_eq!(ac["evMaxVoltage"], json!(230));
        assert_eq!(value["maxScheduleTuples"], json!(4));
        let parsed: NotifyEVChargingNeedsRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_with_dc_parameters_round_trips() {
        let req = NotifyEVChargingNeedsRequest {
            charging_needs: ChargingNeedsType {
                requested_energy_transfer: EnergyTransferModeEnumType::Dc,
                departure_time: None,
                ac_charging_parameters: None,
                dc_charging_parameters: Some(DCChargingParametersType {
                    ev_max_current: 400,
                    ev_max_voltage: 900,
                    energy_amount: Some(60000),
                    ev_max_power: Some(150000),
                    state_of_charge: Some(20),
                    ev_energy_capacity: Some(80000),
                    full_soc: Some(100),
                    bulk_soc: Some(80),
                    custom_data: None,
                }),
                custom_data: None,
            },
            evse_id: 1,
            max_schedule_tuples: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        let dc = &value["chargingNeeds"]["dcChargingParameters"];
        assert_eq!(dc["evMaxCurrent"], json!(400));
        assert_eq!(dc["evMaxVoltage"], json!(900));
        assert_eq!(dc["fullSoC"], json!(100));
        assert_eq!(dc["bulkSoC"], json!(80));
        assert_eq!(dc["stateOfCharge"], json!(20));
        let parsed: NotifyEVChargingNeedsRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn integer_fields_round_trip_as_integers() {
        let req = NotifyEVChargingNeedsRequest {
            charging_needs: minimal_needs(),
            evse_id: 3,
            max_schedule_tuples: Some(7),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert!(value["evseId"].is_i64());
        assert!(value["maxScheduleTuples"].is_i64());
    }

    #[test]
    fn request_missing_charging_needs_fails() {
        let err = serde_json::from_value::<NotifyEVChargingNeedsRequest>(json!({ "evseId": 1 }))
            .unwrap_err();
        assert!(err.to_string().contains("chargingNeeds"));
    }

    #[test]
    fn response_round_trips_minimal() {
        let resp = NotifyEVChargingNeedsResponse {
            status: NotifyEVChargingNeedsStatusEnumType::Accepted,
            status_info: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({ "status": "Accepted" }));
        let parsed: NotifyEVChargingNeedsResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn response_serializes_status_info() {
        let resp = NotifyEVChargingNeedsResponse {
            status: NotifyEVChargingNeedsStatusEnumType::Rejected,
            status_info: Some(StatusInfoType {
                reason_code: "NotAvailable".to_string(),
                additional_info: None,
                custom_data: None,
            }),
            custom_data: None,
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["status"], json!("Rejected"));
        assert_eq!(value["statusInfo"]["reasonCode"], json!("NotAvailable"));
    }

    #[test]
    fn energy_transfer_mode_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(EnergyTransferModeEnumType::Dc).unwrap(),
            json!("DC")
        );
        assert_eq!(
            serde_json::to_value(EnergyTransferModeEnumType::AcSinglePhase).unwrap(),
            json!("AC_single_phase")
        );
        assert_eq!(
            serde_json::to_value(EnergyTransferModeEnumType::AcTwoPhase).unwrap(),
            json!("AC_two_phase")
        );
        assert_eq!(
            serde_json::to_value(EnergyTransferModeEnumType::AcThreePhase).unwrap(),
            json!("AC_three_phase")
        );
    }

    #[test]
    fn status_serializes_to_exact_wire_values() {
        assert_eq!(
            serde_json::to_value(NotifyEVChargingNeedsStatusEnumType::Accepted).unwrap(),
            json!("Accepted")
        );
        assert_eq!(
            serde_json::to_value(NotifyEVChargingNeedsStatusEnumType::Rejected).unwrap(),
            json!("Rejected")
        );
        assert_eq!(
            serde_json::to_value(NotifyEVChargingNeedsStatusEnumType::Processing).unwrap(),
            json!("Processing")
        );
    }

    #[test]
    fn rejects_unknown_enum_values() {
        let status_err =
            serde_json::from_value::<NotifyEVChargingNeedsResponse>(json!({ "status": "Maybe" }))
                .unwrap_err();
        assert!(
            status_err.to_string().contains("Maybe") || status_err.to_string().contains("variant")
        );
        let mode_err = serde_json::from_value::<EnergyTransferModeEnumType>(json!("AC_four_phase"))
            .unwrap_err();
        assert!(
            mode_err.to_string().contains("AC_four_phase")
                || mode_err.to_string().contains("variant")
        );
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(
            NotifyEVChargingNeedsRequest::ACTION_NAME,
            "NotifyEVChargingNeeds"
        );
        assert_eq!(
            NotifyEVChargingNeedsResponse::ACTION_NAME,
            "NotifyEVChargingNeedsResponse"
        );
    }
}
