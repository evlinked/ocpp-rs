//! Vendor-scoped routing for inbound `DataTransfer` requests (OCPP 1.6J §6.x).
//!
//! `DataTransfer` is the spec's escape hatch for vendor-specific extensions: a
//! CALL carries a mandatory `vendorId`, an optional `messageId`, and optional
//! free-form `data`. The receiver must answer with one of four
//! [`DataTransferStatus`] values:
//!
//! - `UnknownVendorId` — the `vendorId` is not implemented by this endpoint,
//! - `UnknownMessageId` — the `vendorId` is known but the `messageId` is not,
//! - `Rejected` — recognized but refused,
//! - `Accepted` — recognized and handled (optionally returning `data`).
//!
//! The Python reference routes `DataTransfer` like any other action through its
//! `@on("DataTransfer")` decorator and leaves the vendor/message dispatch to the
//! application
//! ([`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)).
//! This registry is the idiomatic Rust equivalent: embedders register a handler
//! per `(vendorId, messageId)`, and anything unregistered resolves to the
//! spec-faithful `UnknownVendorId` / `UnknownMessageId` automatically.
//!
//! # Default behaviour
//!
//! A [`ChargePoint`](crate::ChargePoint) starts with an **empty** registry, so
//! by default every inbound `DataTransfer` is answered `UnknownVendorId` — the
//! faithful answer for a vendor the CP does not implement (1.6J §6.x). Embedders
//! opt into specific vendors/messages via
//! [`ChargePoint::register_data_transfer_handler`](crate::ChargePoint::register_data_transfer_handler).

use std::collections::HashMap;
use std::sync::RwLock;

use ocpp_messages::v16j::{DataTransferRequest, DataTransferResponse};
use ocpp_types::v16j::DataTransferStatus;

/// A vendor handler: given the inbound request, decide the full response
/// ([`DataTransferStatus`] plus optional `data`).
///
/// Handlers are synchronous and must be cheap and non-blocking:
/// [`dispatch`](DataTransferRegistry::dispatch) invokes them while holding the
/// registry's read lock, so a handler must not call back into
/// [`register`](DataTransferRegistry::register) (it would deadlock on the write
/// lock).
pub type DataTransferHandler =
    Box<dyn Fn(&DataTransferRequest) -> DataTransferResponse + Send + Sync>;

/// Handlers registered under one `vendorId`.
#[derive(Default)]
struct VendorEntry {
    /// Handlers keyed by `messageId` (for requests that carry one).
    by_message: HashMap<String, DataTransferHandler>,
    /// Handler for requests from this vendor that carry **no** `messageId`
    /// (`messageId` is optional in the request).
    no_message: Option<DataTransferHandler>,
}

impl VendorEntry {
    /// Whether this vendor has at least one handler — i.e. the `vendorId` is
    /// "known". A vendor with no handlers is indistinguishable from an
    /// unregistered one and resolves to `UnknownVendorId`.
    fn is_empty(&self) -> bool {
        self.by_message.is_empty() && self.no_message.is_none()
    }
}

/// Thread-safe routing table for inbound `DataTransfer` requests.
///
/// Shared behind an `Arc` and captured by the default `DataTransfer` handler;
/// embedders mutate it through
/// [`ChargePoint::register_data_transfer_handler`](crate::ChargePoint::register_data_transfer_handler)
/// even after the dispatcher is built (the `Arc` is the same instance).
///
/// The table only ever grows by **explicit** registration — never from inbound
/// request data — so a malicious or chatty CSMS cannot make it grow unbounded.
#[derive(Default)]
pub struct DataTransferRegistry {
    vendors: RwLock<HashMap<String, VendorEntry>>,
}

impl std::fmt::Debug for DataTransferRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let vendors = self.vendors.read().unwrap_or_else(|p| p.into_inner());
        f.debug_struct("DataTransferRegistry")
            .field("vendors", &vendors.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl DataTransferRegistry {
    /// Create an empty registry. Until a handler is registered, every request
    /// resolves to `UnknownVendorId`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for a `(vendorId, messageId)` pair.
    ///
    /// `message_id = Some(id)` handles requests carrying that exact `messageId`;
    /// `message_id = None` handles requests from this vendor that carry **no**
    /// `messageId`. Registering again for the same key replaces the prior
    /// handler.
    pub fn register<F>(&self, vendor_id: impl Into<String>, message_id: Option<String>, handler: F)
    where
        F: Fn(&DataTransferRequest) -> DataTransferResponse + Send + Sync + 'static,
    {
        let mut vendors = self.vendors.write().unwrap_or_else(|p| p.into_inner());
        let entry = vendors.entry(vendor_id.into()).or_default();
        match message_id {
            Some(id) => {
                entry.by_message.insert(id, Box::new(handler));
            }
            None => entry.no_message = Some(Box::new(handler)),
        }
    }

    /// Route a request to the matching handler, or synthesize the faithful
    /// `Unknown*` response when none matches.
    ///
    /// - unknown `vendorId` → `UnknownVendorId`
    /// - known `vendorId`, `messageId` present but unregistered → `UnknownMessageId`
    /// - known `vendorId`, no `messageId` and no vendor default → `UnknownMessageId`
    /// - a matching handler → its `Accepted` / `Rejected` response
    pub fn dispatch(&self, req: &DataTransferRequest) -> DataTransferResponse {
        let vendors = self.vendors.read().unwrap_or_else(|p| p.into_inner());
        let Some(entry) = vendors.get(&req.vendor_id).filter(|e| !e.is_empty()) else {
            return unknown(DataTransferStatus::UnknownVendorId);
        };

        let handler = match &req.message_id {
            Some(id) => entry.by_message.get(id),
            None => entry.no_message.as_ref(),
        };

        match handler {
            Some(h) => h(req),
            None => unknown(DataTransferStatus::UnknownMessageId),
        }
    }
}

/// A status-only response with no `data`, used for the `Unknown*` outcomes.
fn unknown(status: DataTransferStatus) -> DataTransferResponse {
    DataTransferResponse { status, data: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(vendor: &str, message: Option<&str>, data: Option<&str>) -> DataTransferRequest {
        DataTransferRequest {
            vendor_id: vendor.to_string(),
            message_id: message.map(str::to_string),
            data: data.map(str::to_string),
        }
    }

    fn accepted(data: Option<String>) -> DataTransferResponse {
        DataTransferResponse {
            status: DataTransferStatus::Accepted,
            data,
        }
    }

    #[test]
    fn empty_registry_reports_unknown_vendor() {
        let reg = DataTransferRegistry::new();
        let resp = reg.dispatch(&req("com.acme", Some("Ping"), None));
        assert_eq!(resp.status, DataTransferStatus::UnknownVendorId);
        assert_eq!(resp.data, None);
    }

    #[test]
    fn unregistered_vendor_reports_unknown_vendor() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", Some("Ping".into()), |_| accepted(None));
        let resp = reg.dispatch(&req("com.other", Some("Ping"), None));
        assert_eq!(resp.status, DataTransferStatus::UnknownVendorId);
    }

    #[test]
    fn known_vendor_unknown_message_reports_unknown_message() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", Some("Ping".into()), |_| accepted(None));
        let resp = reg.dispatch(&req("com.acme", Some("Pong"), None));
        assert_eq!(resp.status, DataTransferStatus::UnknownMessageId);
    }

    #[test]
    fn registered_message_handler_runs_and_can_echo_data() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", Some("Ping".into()), |r| {
            accepted(r.data.clone())
        });
        let resp = reg.dispatch(&req("com.acme", Some("Ping"), Some("payload")));
        assert_eq!(resp.status, DataTransferStatus::Accepted);
        assert_eq!(resp.data.as_deref(), Some("payload"));
    }

    #[test]
    fn handler_may_reject() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", Some("Drop".into()), |_| DataTransferResponse {
            status: DataTransferStatus::Rejected,
            data: None,
        });
        let resp = reg.dispatch(&req("com.acme", Some("Drop"), None));
        assert_eq!(resp.status, DataTransferStatus::Rejected);
    }

    #[test]
    fn no_message_handler_serves_requests_without_message_id() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", None, |_| {
            accepted(Some("vendor-default".into()))
        });
        let resp = reg.dispatch(&req("com.acme", None, None));
        assert_eq!(resp.status, DataTransferStatus::Accepted);
        assert_eq!(resp.data.as_deref(), Some("vendor-default"));
    }

    #[test]
    fn no_message_handler_does_not_catch_a_specific_message_id() {
        // A vendor-default (no-messageId) handler must not absorb requests that
        // *do* carry an (unregistered) messageId — those are UnknownMessageId.
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", None, |_| accepted(None));
        let resp = reg.dispatch(&req("com.acme", Some("Ping"), None));
        assert_eq!(resp.status, DataTransferStatus::UnknownMessageId);
    }

    #[test]
    fn message_handler_does_not_catch_request_without_message_id() {
        // Symmetric to the above: a messageId-scoped handler must not serve a
        // request that omits messageId.
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", Some("Ping".into()), |_| accepted(None));
        let resp = reg.dispatch(&req("com.acme", None, None));
        assert_eq!(resp.status, DataTransferStatus::UnknownMessageId);
    }

    #[test]
    fn re_registering_replaces_the_handler() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", Some("Ping".into()), |_| {
            accepted(Some("v1".into()))
        });
        reg.register("com.acme", Some("Ping".into()), |_| {
            accepted(Some("v2".into()))
        });
        let resp = reg.dispatch(&req("com.acme", Some("Ping"), None));
        assert_eq!(resp.data.as_deref(), Some("v2"));
    }

    #[test]
    fn vendor_can_mix_specific_and_default_handlers() {
        let reg = DataTransferRegistry::new();
        reg.register("com.acme", Some("Ping".into()), |_| {
            accepted(Some("ping".into()))
        });
        reg.register("com.acme", None, |_| accepted(Some("default".into())));
        assert_eq!(
            reg.dispatch(&req("com.acme", Some("Ping"), None))
                .data
                .as_deref(),
            Some("ping")
        );
        assert_eq!(
            reg.dispatch(&req("com.acme", None, None)).data.as_deref(),
            Some("default")
        );
        assert_eq!(
            reg.dispatch(&req("com.acme", Some("Other"), None)).status,
            DataTransferStatus::UnknownMessageId
        );
    }
}
