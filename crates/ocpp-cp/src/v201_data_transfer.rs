//! Adapter that serves an inbound OCPP 2.0.1 `DataTransfer` from the same
//! vendor-scoped routing table the 1.6J CP uses.
//!
//! `DataTransfer` is the spec's vendor-extension escape hatch, and it survives
//! essentially unchanged from 1.6J to 2.0.1: a required `vendorId`, an optional
//! `messageId`, free-form `data`, and a four-value status
//! (`Accepted` / `Rejected` / `UnknownMessageId` / `UnknownVendorId`). The CP
//! already ships a faithful, well-tested router for the 1.6J message —
//! [`DataTransferRegistry`] (Issue #101) — so rather than stand up a parallel
//! 2.0.1 registry with its own
//! registration API, the V201 dispatcher arm routes through **that same
//! registry** and this module adapts the request/response at the boundary.
//! Embedders register handlers exactly once, via
//! [`ChargePoint::register_data_transfer_handler`](crate::ChargePoint::register_data_transfer_handler),
//! and both dialects observe them.
//!
//! Ports `ocpp.v201.call.DataTransfer` / `ocpp.v201.call_result.DataTransfer`
//! and the `@on(Action.data_transfer)` dispatch shape from
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
//!
//! # The one representational gap
//!
//! The 1.6J message models `data` as an opaque `Option<String>`; the 2.0.1
//! message models it as `Option<serde_json::Value>` (the reference's
//! `Optional[Any]`). The adapter bridges the two by serializing the v201
//! `Value` to its JSON text on the way in and parsing the handler's returned
//! text back to a `Value` on the way out, so a structured payload
//! (object / array / scalar) round-trips through a handler that merely echoes
//! `data` — the JSON text a handler sees for `{"k":1}` is `{"k":1}`, and it
//! parses straight back. A handler that returns text which is not valid JSON
//! (e.g. an opaque token) is preserved verbatim as a JSON string rather than
//! dropped, so no payload is ever lost.
//!
//! Both conversions are total — no `unwrap`/`expect`/`panic` on the inbound
//! CSMS payload — so a hostile or malformed `data` can neither crash the
//! handler nor bypass the router.

use crate::data_transfer::DataTransferRegistry;
use ocpp_messages::v16j::{
    DataTransferRequest as V16jDataTransferRequest,
    DataTransferResponse as V16jDataTransferResponse,
};
use ocpp_messages::v201::{
    DataTransferRequest as V201DataTransferRequest,
    DataTransferResponse as V201DataTransferResponse,
};
use ocpp_types::v16j::DataTransferStatus as V16jStatus;
use ocpp_types::v201::DataTransferStatusEnumType as V201Status;
use serde_json::Value;

/// Map the 1.6J status onto its 2.0.1 twin. The two enums are value-identical
/// (`Accepted` / `Rejected` / `UnknownMessageId` / `UnknownVendorId`), so this
/// is a total 1:1 correspondence with no default arm.
fn v201_status(status: V16jStatus) -> V201Status {
    match status {
        V16jStatus::Accepted => V201Status::Accepted,
        V16jStatus::Rejected => V201Status::Rejected,
        V16jStatus::UnknownMessageId => V201Status::UnknownMessageId,
        V16jStatus::UnknownVendorId => V201Status::UnknownVendorId,
    }
}

/// The 2.0.1 `data` (`Option<Value>`) rendered as the 1.6J `data`
/// (`Option<String>`) the registry's handlers expect: each `Value` becomes its
/// compact JSON text. Serialization of a `serde_json::Value` cannot fail, but
/// the result is threaded through `.ok()` so there is no panic path — a
/// (theoretical) failure degrades to `None`, never a crash.
fn data_to_v16j(data: &Option<Value>) -> Option<String> {
    data.as_ref().and_then(|v| serde_json::to_string(v).ok())
}

/// The 1.6J `data` (`Option<String>`) a handler returned, parsed back to the
/// 2.0.1 `data` (`Option<Value>`). Handler text that is valid JSON round-trips
/// to the original structure; text that is not valid JSON is preserved as a
/// JSON string so an opaque payload is never lost.
fn data_from_v16j(data: Option<String>) -> Option<Value> {
    data.map(|s| serde_json::from_str(&s).unwrap_or(Value::String(s)))
}

/// Serve an inbound 2.0.1 `DataTransfer` by routing it through the shared 1.6J
/// [`DataTransferRegistry`].
///
/// The request is adapted to the 1.6J shape, dispatched (an unimplemented
/// vendor/message resolves to the faithful `UnknownVendorId` / `UnknownMessageId`
/// with no handler run), and the outcome adapted back to the 2.0.1 response.
/// `statusInfo` and `customData` are not produced by the registry, so they are
/// `None`; the free-form `data` round-trips (see the module docs).
pub fn dispatch(
    registry: &DataTransferRegistry,
    req: &V201DataTransferRequest,
) -> V201DataTransferResponse {
    let v16j_req = V16jDataTransferRequest {
        vendor_id: req.vendor_id.clone(),
        message_id: req.message_id.clone(),
        data: data_to_v16j(&req.data),
    };

    let V16jDataTransferResponse { status, data } = registry.dispatch(&v16j_req);

    V201DataTransferResponse {
        status: v201_status(status),
        status_info: None,
        data: data_from_v16j(data),
        custom_data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v201_req(
        vendor: &str,
        message: Option<&str>,
        data: Option<Value>,
    ) -> V201DataTransferRequest {
        V201DataTransferRequest {
            vendor_id: vendor.to_string(),
            message_id: message.map(str::to_string),
            data,
            custom_data: None,
        }
    }

    fn accepted_echo(reg: &DataTransferRegistry, vendor: &str, message: Option<&str>) {
        // A handler that echoes the request's data verbatim — the canonical
        // round-trip vendor handler.
        reg.register(vendor.to_string(), message.map(str::to_string), |r| {
            V16jDataTransferResponse {
                status: V16jStatus::Accepted,
                data: r.data.clone(),
            }
        });
    }

    #[test]
    fn status_map_is_one_to_one() {
        assert_eq!(v201_status(V16jStatus::Accepted), V201Status::Accepted);
        assert_eq!(v201_status(V16jStatus::Rejected), V201Status::Rejected);
        assert_eq!(
            v201_status(V16jStatus::UnknownMessageId),
            V201Status::UnknownMessageId
        );
        assert_eq!(
            v201_status(V16jStatus::UnknownVendorId),
            V201Status::UnknownVendorId
        );
    }

    #[test]
    fn empty_registry_reports_unknown_vendor() {
        let reg = DataTransferRegistry::new();
        let resp = dispatch(&reg, &v201_req("com.acme", Some("Ping"), None));
        assert_eq!(resp.status, V201Status::UnknownVendorId);
        assert_eq!(resp.data, None);
    }

    #[test]
    fn known_vendor_unknown_message_reports_unknown_message() {
        let reg = DataTransferRegistry::new();
        accepted_echo(&reg, "com.acme", Some("Ping"));
        let resp = dispatch(&reg, &v201_req("com.acme", Some("Pong"), None));
        assert_eq!(resp.status, V201Status::UnknownMessageId);
    }

    #[test]
    fn registered_handler_accepts_and_a_vendor_default_serves_no_message_id() {
        let reg = DataTransferRegistry::new();
        accepted_echo(&reg, "com.acme", Some("Ping"));
        accepted_echo(&reg, "com.acme", None);
        assert_eq!(
            dispatch(&reg, &v201_req("com.acme", Some("Ping"), None)).status,
            V201Status::Accepted
        );
        assert_eq!(
            dispatch(&reg, &v201_req("com.acme", None, None)).status,
            V201Status::Accepted
        );
    }

    #[test]
    fn handler_may_reject() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme".to_string(), Some("Drop".to_string()), |_| {
            V16jDataTransferResponse {
                status: V16jStatus::Rejected,
                data: None,
            }
        });
        let resp = dispatch(&reg, &v201_req("com.acme", Some("Drop"), None));
        assert_eq!(resp.status, V201Status::Rejected);
    }

    #[test]
    fn structured_data_round_trips_in_both_directions() {
        let reg = DataTransferRegistry::new();
        accepted_echo(&reg, "com.acme", Some("Echo"));
        // An object, an array, a bare string, an integer, and a bool each survive
        // request -> handler -> response without loss.
        for payload in [
            json!({"soc": 80, "phases": [1, 2, 3]}),
            json!([1, "two", true]),
            json!("just-a-string"),
            json!(42),
            json!(true),
        ] {
            let resp = dispatch(
                &reg,
                &v201_req("com.acme", Some("Echo"), Some(payload.clone())),
            );
            assert_eq!(resp.status, V201Status::Accepted);
            assert_eq!(resp.data, Some(payload), "data must round-trip unchanged");
        }
    }

    #[test]
    fn non_json_handler_text_is_preserved_as_a_string() {
        // A handler that returns opaque, non-JSON text must not have its payload
        // dropped — it surfaces as a JSON string rather than being lost.
        let reg = DataTransferRegistry::new();
        reg.register("com.acme".to_string(), Some("Token".to_string()), |_| {
            V16jDataTransferResponse {
                status: V16jStatus::Accepted,
                data: Some("opaque-not-json".to_string()),
            }
        });
        let resp = dispatch(&reg, &v201_req("com.acme", Some("Token"), None));
        assert_eq!(
            resp.data,
            Some(Value::String("opaque-not-json".to_string()))
        );
    }

    #[test]
    fn a_message_scoped_handler_does_not_serve_a_request_without_message_id() {
        // The routing contract is the registry's, unchanged: a messageId-scoped
        // handler must not absorb a request that omits messageId.
        let reg = DataTransferRegistry::new();
        accepted_echo(&reg, "com.acme", Some("Ping"));
        let resp = dispatch(&reg, &v201_req("com.acme", None, None));
        assert_eq!(resp.status, V201Status::UnknownMessageId);
    }
}
