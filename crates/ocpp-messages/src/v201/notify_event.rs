//! `NotifyEvent` — the Charging Station streams device-model **events** to the
//! CSMS.
//!
//! Ports `ocpp.v201.call.NotifyEvent` / `ocpp.v201.call_result.NotifyEvent`. It
//! is the asynchronous event carrier of the OCPP 2.0.1 monitoring subsystem:
//! whenever a monitored variable crosses a threshold, a component changes
//! state, or a delta/periodic monitor fires, the station reports it here. Like
//! the other report carriers (e.g. [`NotifyMonitoringReport`]) events are paged
//! via `seq_no` / `tbc`. It is the runtime counterpart to the monitoring
//! *configuration* messages ([`SetVariableMonitoring`], [`SetMonitoringBase`],
//! [`SetMonitoringLevel`], [`ClearVariableMonitoring`]). The response is empty.
//!
//! [`NotifyMonitoringReport`]: super::NotifyMonitoringReportRequest
//! [`SetVariableMonitoring`]: super::SetVariableMonitoringRequest
//! [`SetMonitoringBase`]: super::SetMonitoringBaseRequest
//! [`SetMonitoringLevel`]: super::SetMonitoringLevelRequest
//! [`ClearVariableMonitoring`]: super::ClearVariableMonitoringRequest

use crate::{OcppAction, OcppResponse};
use ocpp_types::v201::{CustomDataType, EventDataType};
use serde::{Deserialize, Serialize};

/// `NotifyEvent.req` — a single (possibly partial) page of device-model events
/// sent by the Charging Station.
///
/// Ports `ocpp.v201.call.NotifyEvent`. `seq_no` numbers the pages (first is 0)
/// and `tbc` ("to be continued") is `true` while more pages follow. `event_data`
/// carries the events and, per the schema, holds at least one item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyEventRequest {
    /// Timestamp (RFC 3339 / ISO 8601) of the moment this message was generated
    /// at the Charging Station.
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    /// Sequence number of this message; the first page starts at 0.
    #[serde(rename = "seqNo")]
    pub seq_no: i32,
    /// The events reported in this page. The schema requires at least one item.
    #[serde(rename = "eventData")]
    pub event_data: Vec<EventDataType>,
    /// "To be continued" — `true` when another page follows in an upcoming
    /// `NotifyEvent`. Absent means the default `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbc: Option<bool>,
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyEventRequest {
    const ACTION_NAME: &'static str = "NotifyEvent";
    type Response = NotifyEventResponse;
}

/// `NotifyEvent.conf` — the CSMS's empty acknowledgement.
///
/// Ports `ocpp.v201.call_result.NotifyEvent`. It carries no fields beyond the
/// optional vendor extension, so it serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NotifyEventResponse {
    /// Vendor extension.
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<CustomDataType>,
}

impl OcppAction for NotifyEventResponse {
    const ACTION_NAME: &'static str = "NotifyEventResponse";
    type Response = Self;
}

impl OcppResponse for NotifyEventResponse {}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::{
        ComponentType, EventNotificationEnumType, EventTriggerEnumType, VariableType,
    };
    use serde_json::json;

    fn sample_event() -> EventDataType {
        EventDataType {
            event_id: 42,
            timestamp: "2022-01-01T10:00:00Z".to_string(),
            trigger: EventTriggerEnumType::Alerting,
            actual_value: "85".to_string(),
            event_notification_type: EventNotificationEnumType::HardWiredMonitor,
            component: ComponentType {
                name: "EVSE".to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: VariableType {
                name: "Temperature".to_string(),
                instance: None,
                custom_data: None,
            },
            cause: None,
            tech_code: None,
            tech_info: None,
            cleared: None,
            transaction_id: None,
            variable_monitoring_id: None,
            custom_data: None,
        }
    }

    #[test]
    fn request_round_trips_minimal() {
        let req = NotifyEventRequest {
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            seq_no: 0,
            event_data: vec![sample_event()],
            tbc: None,
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        // Optional `tbc` / `customData` stay off the wire, as do the event's
        // absent optionals.
        assert_eq!(
            value,
            json!({
                "generatedAt": "2022-01-01T10:00:00Z",
                "seqNo": 0,
                "eventData": [{
                    "eventId": 42,
                    "timestamp": "2022-01-01T10:00:00Z",
                    "trigger": "Alerting",
                    "actualValue": "85",
                    "eventNotificationType": "HardWiredMonitor",
                    "component": { "name": "EVSE" },
                    "variable": { "name": "Temperature" }
                }]
            })
        );
        let parsed: NotifyEventRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn request_round_trips_full_event_and_paging() {
        let mut event = sample_event();
        event.cause = Some(7);
        event.tech_code = Some("E42".to_string());
        event.tech_info = Some("over temperature".to_string());
        event.cleared = Some(false);
        event.transaction_id = Some("txn-1".to_string());
        event.variable_monitoring_id = Some(3);
        let req = NotifyEventRequest {
            generated_at: "2022-01-01T10:05:00Z".to_string(),
            seq_no: 2,
            event_data: vec![event],
            tbc: Some(true),
            custom_data: None,
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["tbc"], json!(true));
        let ev = &value["eventData"][0];
        assert_eq!(ev["cause"], json!(7));
        assert_eq!(ev["techCode"], json!("E42"));
        assert_eq!(ev["techInfo"], json!("over temperature"));
        assert_eq!(ev["cleared"], json!(false));
        assert_eq!(ev["transactionId"], json!("txn-1"));
        assert_eq!(ev["variableMonitoringId"], json!(3));
        let parsed: NotifyEventRequest = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn scalar_fields_round_trip_as_their_json_types() {
        let mut event = sample_event();
        event.cause = Some(-1);
        event.cleared = Some(true);
        event.variable_monitoring_id = Some(9);
        let value = serde_json::to_value(NotifyEventRequest {
            generated_at: "2022-01-01T10:00:00Z".to_string(),
            seq_no: 12,
            event_data: vec![event],
            tbc: Some(false),
            custom_data: None,
        })
        .unwrap();
        assert!(value["seqNo"].is_i64());
        assert!(value["tbc"].is_boolean());
        assert!(value["generatedAt"].is_string());
        let ev = &value["eventData"][0];
        assert!(ev["eventId"].is_i64());
        assert!(ev["cause"].is_i64());
        assert!(ev["cleared"].is_boolean());
        assert!(ev["variableMonitoringId"].is_i64());
        assert!(ev["timestamp"].is_string());
    }

    #[test]
    fn trigger_enum_serializes_to_exact_wire_values() {
        for (variant, wire) in [
            (EventTriggerEnumType::Alerting, "Alerting"),
            (EventTriggerEnumType::Delta, "Delta"),
            (EventTriggerEnumType::Periodic, "Periodic"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: EventTriggerEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn notification_enum_serializes_to_exact_wire_values() {
        for (variant, wire) in [
            (
                EventNotificationEnumType::HardWiredNotification,
                "HardWiredNotification",
            ),
            (
                EventNotificationEnumType::HardWiredMonitor,
                "HardWiredMonitor",
            ),
            (
                EventNotificationEnumType::PreconfiguredMonitor,
                "PreconfiguredMonitor",
            ),
            (EventNotificationEnumType::CustomMonitor, "CustomMonitor"),
        ] {
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            let back: EventNotificationEnumType = serde_json::from_value(json!(wire)).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn response_round_trips_empty() {
        let resp = NotifyEventResponse::default();
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value, json!({}));
        let parsed: NotifyEventResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn request_missing_required_field_fails() {
        // `eventData` is required.
        let err = serde_json::from_value::<NotifyEventRequest>(
            json!({ "generatedAt": "2022-01-01T10:00:00Z", "seqNo": 0 }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("eventData"));
    }

    #[test]
    fn request_rejects_unknown_trigger() {
        let err = serde_json::from_value::<NotifyEventRequest>(json!({
            "generatedAt": "2022-01-01T10:00:00Z",
            "seqNo": 0,
            "eventData": [{
                "eventId": 1,
                "timestamp": "2022-01-01T10:00:00Z",
                "trigger": "Hourly",
                "actualValue": "1",
                "eventNotificationType": "HardWiredMonitor",
                "component": { "name": "EVSE" },
                "variable": { "name": "Temperature" }
            }]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("Hourly") || err.to_string().contains("variant"));
    }

    #[test]
    fn action_names_are_stable() {
        assert_eq!(NotifyEventRequest::ACTION_NAME, "NotifyEvent");
        assert_eq!(NotifyEventResponse::ACTION_NAME, "NotifyEventResponse");
    }
}
