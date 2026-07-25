//! WebSocket server implementation for OCPP Central System Management System (CSMS)

use crate::{
    error::{TransportError, TransportResult},
    pending::PendingCallMap,
    ConnectionInfo, ConnectionState, MessageHandler, TransportConfig, TransportEvent,
};
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use ocpp_messages::v16j::{
    BootNotificationRequest, CancelReservationRequest, ChangeConfigurationRequest,
    ClearCacheRequest, ClearChargingProfileRequest, GetCompositeScheduleRequest,
    GetCompositeScheduleResponse, GetConfigurationRequest, GetConfigurationResponse,
    GetDiagnosticsRequest, GetDiagnosticsResponse, GetLocalListVersionRequest, MeterValuesRequest,
    RemoteStartTransactionRequest, RemoteStopTransactionRequest, ReserveNowRequest, ResetRequest,
    SendLocalListRequest, SetChargingProfileRequest, StartTransactionRequest,
    StartTransactionResponse, StatusNotificationRequest, StopTransactionRequest,
    TriggerMessageRequest, UnlockConnectorRequest, UpdateFirmwareRequest, UpdateFirmwareResponse,
};
use ocpp_messages::{CallMessage, Message, MessageType, OcppAction, SchemaValidator};
use ocpp_types::v16j::{
    AuthorizationData, CancelReservationStatus, ChargingProfile, ChargingProfilePurposeType,
    ChargingProfileStatus, ChargingRateUnitType, ClearCacheStatus, ClearChargingProfileStatus,
    ConfigurationStatus, MessageTrigger, RemoteStartStopStatus, ReservationStatus, ResetStatus,
    ResetType, TriggerMessageStatus, UnlockStatus, UpdateStatus, UpdateType,
};
use ocpp_types::{CallErrorCode, DateTime, OcppError, OcppResult, SchemaKeyword, Utc};
use std::{net::SocketAddr, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Per-charge-point handle used to drive CSMS-initiated CALLs.
///
/// Cloned out of the `cp_handles` map by [`OcppServer::call`] so a frame can be
/// written to the CP's WebSocket sink (`outbound`) and the matching
/// CALLRESULT/CALLERROR correlated back (`pending`) — the server-side mirror of
/// the [`WebSocketClient`](crate::WebSocketClient) machinery.
#[derive(Clone)]
struct CpHandle {
    /// Per-session identity token (the owning connection's `connection_id`).
    ///
    /// OCPP `cp_id`s are unique, so at most one entry per `cp_id` lives in
    /// `cp_handles` — but on a racy reconnect a *new* session can overwrite the
    /// entry while the *old* session is still tearing down. Teardown does a
    /// compare-and-remove against this token so a stale session can only evict
    /// its own handle, never the newer session's (see issue #50).
    connection_id: Uuid,
    /// Writes text frames to this CP's WebSocket sink via the per-connection
    /// receive loop, which owns the sink.
    outbound: mpsc::UnboundedSender<WsMessage>,
    /// In-flight CALL correlation map for CALLs the *server* initiated to this CP.
    pending: Arc<PendingCallMap>,
}

/// State shared across all per-CP axum handler invocations.
struct ServerState {
    connections: Arc<DashMap<Uuid, ConnectionInfo>>,
    /// Routing handles keyed by charge-point id, used by [`OcppServer::call`].
    cp_handles: Arc<DashMap<String, CpHandle>>,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    config: TransportConfig,
    message_handler: Arc<dyn MessageHandler>,
}

/// WebSocket server for OCPP Central System (CSMS).
///
/// Call `start()` to bind and begin accepting connections, then `stop()` to shut down.
pub struct OcppServer {
    config: TransportConfig,
    connections: Arc<DashMap<Uuid, ConnectionInfo>>,
    cp_handles: Arc<DashMap<String, CpHandle>>,
    event_tx: mpsc::UnboundedSender<TransportEvent>,
    state: ConnectionState,
    message_handler: Arc<dyn MessageHandler>,
    serve_handle: Option<JoinHandle<()>>,
    local_addr: Option<SocketAddr>,
    /// Optional schema validator for the CSMS-initiated [`call`](Self::call)
    /// path. When `Some`, `call()` validates the outbound CALL before sending
    /// and the inbound CALLRESULT before deserializing, mirroring the CP-side
    /// [`ChargePoint::call`](../../ocpp_cp/struct.ChargePoint.html) and the
    /// reference's side-agnostic `charge_point.py::call`. `None` (the default)
    /// is the analog of the reference's `skip_schema_validation=True`.
    validator: Option<Arc<SchemaValidator>>,
}

impl OcppServer {
    /// Create a new OCPP server.
    ///
    /// Returns the server and the receive end of the transport-event channel.
    pub fn new(
        config: TransportConfig,
        message_handler: Arc<dyn MessageHandler>,
    ) -> (Self, mpsc::UnboundedReceiver<TransportEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let server = Self {
            config,
            connections: Arc::new(DashMap::new()),
            cp_handles: Arc::new(DashMap::new()),
            event_tx,
            state: ConnectionState::Closed,
            message_handler,
            serve_handle: None,
            local_addr: None,
            validator: None,
        };
        (server, event_rx)
    }

    /// Attach a [`SchemaValidator`] to the CSMS-initiated [`call`](Self::call)
    /// path, so outbound CALLs and inbound CALLRESULTs are schema-validated —
    /// the CSMS-side analog of the CP's `config.validate_payloads` and of the
    /// reference's `skip_schema_validation=False`.
    ///
    /// Opt-in (rather than a hard-wired `v16j()` default) because
    /// [`call`](Self::call) is version-generic: the caller supplies the
    /// validator matching the OCPP version they speak — e.g.
    /// `SchemaValidator::v16j()` for 1.6J or `SchemaValidator::v201()` for
    /// 2.0.1. An unknown action for the attached validator surfaces as
    /// [`OcppError::NotSupported`] (same as the CP side).
    ///
    /// ```ignore
    /// let (server, events) = OcppServer::new(config, handler);
    /// let server = server.with_validator(Arc::new(SchemaValidator::v16j()));
    /// ```
    #[must_use]
    pub fn with_validator(mut self, validator: Arc<SchemaValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Bind to `bind_addr` and start accepting WebSocket connections.
    ///
    /// The serve loop runs in a background Tokio task; this method returns as soon as the TCP
    /// socket is bound. Use `local_addr()` to discover the actual port (useful with `:0`).
    pub async fn start(&mut self, bind_addr: &str) -> TransportResult<()> {
        let listener = tokio::net::TcpListener::bind(bind_addr)
            .await
            .map_err(|e| TransportError::IoError {
                message: e.to_string(),
            })?;

        let local_addr = listener.local_addr().map_err(|e| TransportError::IoError {
            message: e.to_string(),
        })?;

        let shared = Arc::new(ServerState {
            connections: Arc::clone(&self.connections),
            cp_handles: Arc::clone(&self.cp_handles),
            event_tx: self.event_tx.clone(),
            config: self.config.clone(),
            message_handler: Arc::clone(&self.message_handler),
        });

        let app = Router::new()
            .route("/ocpp/:charge_point_id", get(ws_handler))
            .with_state(shared);

        self.local_addr = Some(local_addr);
        self.state = ConnectionState::Connected;
        info!("OCPP server listening on {}", local_addr);

        let handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("OCPP server error: {}", e);
            }
        });
        self.serve_handle = Some(handle);

        Ok(())
    }

    /// Stop the server, abort the serve task, and drop all connection tracking.
    pub async fn stop(&mut self) -> TransportResult<()> {
        info!("Stopping OCPP server");
        self.state = ConnectionState::Closing;

        if let Some(handle) = self.serve_handle.take() {
            handle.abort();
        }

        // Cancel any in-flight server-initiated CALLs so their futures resolve
        // with a transport error instead of hanging until timeout.
        for entry in self.cp_handles.iter() {
            entry.value().pending.cancel_all();
        }
        self.cp_handles.clear();
        self.connections.clear();
        self.local_addr = None;
        self.state = ConnectionState::Closed;
        Ok(())
    }

    /// Address the server is bound to, or `None` if not yet started.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    /// Number of charge points currently connected.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Connection info for a specific CP, if present.
    pub fn get_connection(&self, connection_id: &Uuid) -> Option<ConnectionInfo> {
        self.connections.get(connection_id).map(|c| c.clone())
    }

    /// Info for every connected CP.
    pub fn get_all_connections(&self) -> Vec<ConnectionInfo> {
        self.connections.iter().map(|c| c.clone()).collect()
    }

    /// Whether a charge point with `cp_id` currently has a live session.
    pub fn is_cp_connected(&self, cp_id: &str) -> bool {
        self.cp_handles.contains_key(cp_id)
    }

    /// Ids of every charge point currently connected and routable via [`call`](Self::call).
    pub fn connected_cp_ids(&self) -> Vec<String> {
        self.cp_handles.iter().map(|e| e.key().clone()).collect()
    }

    /// Send a typed CALL to a specific connected charge point and await its CALLRESULT.
    ///
    /// This is the CSMS half of the bidirectional OCPP conversation and the
    /// server-side mirror of [`ChargePoint::call`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
    /// It is used to dispatch CS→CP actions such as `RemoteStartTransaction`,
    /// `Reset`, or `ChangeAvailability`.
    ///
    /// 0. Serialize + (if a validator is attached via [`with_validator`](Self::with_validator))
    ///    schema-validate the outbound CALL, before the connection check
    /// 1. Resolve the per-CP routing handle by `cp_id`
    /// 2. Register the `unique_id` in the CP's `PendingCallMap` *before* sending
    ///    (race-free, identical to the CP-side `call()`)
    /// 3. Write the CALL frame to the CP's WebSocket sink
    /// 4. Await the response, bounded by `config.call_timeout`
    /// 5. (if a validator is attached) schema-validate the CALLRESULT, then
    ///    deserialize the payload into `Req::Response`
    ///
    /// # Errors
    /// - [`OcppError::SchemaViolation`] if a validator is attached and the
    ///   outbound CALL or inbound CALLRESULT violates its schema
    /// - [`OcppError::CpNotConnected`] if `cp_id` has no live session (checked
    ///   before registering, and again if the sink channel has closed mid-flight)
    /// - [`OcppError::CallError`] if the CP replies with a CALLERROR frame
    /// - [`OcppError::Timeout`] if no response arrives within `config.call_timeout`
    /// - [`OcppError::Transport`] if the connection closes while the CALL is in flight
    ///
    /// Concurrent calls — to the same CP or to different CPs — are independent:
    /// each registers a distinct `unique_id`, and each CP owns its own pending map,
    /// so there is no cross-CP or cross-call correlation.
    pub async fn call<Req: OcppAction>(
        &self,
        cp_id: &str,
        request: Req,
    ) -> OcppResult<Req::Response> {
        // 0. Serialize the typed request up front and validate the OUTGOING CALL
        //    against its `{action}` schema *before* the connection check or any
        //    network I/O, mirroring the CP-side `ChargePoint::call` and the
        //    reference's `call()` in charge_point.py (which runs
        //    `validate_payload(call)` prior to `_send`). A strongly-typed
        //    request can still be schema-invalid (e.g. a `String` field
        //    exceeding its `maxLength`); the reference rejects it locally rather
        //    than putting a malformed CALL on the wire. Gated by an attached
        //    validator, whose absence is the analog of the reference's
        //    `skip_schema_validation=True`. Ordering it before the connection
        //    check means a schema-invalid request surfaces as `SchemaViolation`
        //    regardless of link state, matching the reference's
        //    validate-before-`_send` ordering.
        let payload = serde_json::to_value(&request).map_err(OcppError::from)?;
        if let Some(validator) = &self.validator {
            validator.validate_call(Req::ACTION_NAME, &payload)?;
        }

        // 1. Resolve the routing handle. Clone it out so we don't hold a DashMap
        //    guard across the await points below (which could deadlock writers).
        let handle = self
            .cp_handles
            .get(cp_id)
            .map(|h| h.clone())
            .ok_or_else(|| OcppError::CpNotConnected {
                cp_id: cp_id.to_string(),
            })?;

        let unique_id = Uuid::new_v4().to_string();

        // 2. Register before sending to avoid the race where the CALLRESULT
        //    arrives before the receiver is in the map.
        let rx = handle.pending.register(unique_id.clone());

        // 3. Build and send the CALL frame, reusing the payload serialized (and
        //    validated) in step 0.
        let call_msg = CallMessage {
            message_type: MessageType::Call,
            unique_id: unique_id.clone(),
            action: Req::ACTION_NAME.to_string(),
            payload,
        };
        let text = serde_json::to_string(&Message::Call(call_msg)).map_err(OcppError::from)?;
        if handle.outbound.send(WsMessage::Text(text)).is_err() {
            // The per-CP receive loop has exited (CP disconnected) and dropped the
            // sink half. Tidy up the now-orphaned pending entry.
            handle.pending.reject(
                &unique_id,
                OcppError::CpNotConnected {
                    cp_id: cp_id.to_string(),
                },
            );
            return Err(OcppError::CpNotConnected {
                cp_id: cp_id.to_string(),
            });
        }

        // 4. Await the CALLRESULT/CALLERROR with the configured timeout.
        let raw = match tokio::time::timeout(self.config.call_timeout, rx).await {
            Ok(result) => result.map_err(|_| OcppError::Transport {
                // oneshot RecvError → sender dropped, i.e. the CP disconnected.
                message: format!("connection to '{cp_id}' closed while awaiting CALLRESULT"),
            })?,
            Err(_) => {
                // Drop the now-useless pending entry so the map doesn't leak a
                // dead receiver for a response that may never arrive.
                handle.pending.reject(
                    &unique_id,
                    OcppError::Timeout {
                        operation: Req::ACTION_NAME.to_string(),
                    },
                );
                return Err(OcppError::Timeout {
                    operation: format!("{} call to {}", Req::ACTION_NAME, cp_id),
                });
            }
        };

        // 5. Propagate any CALLERROR, then validate + deserialize the success
        //    payload.
        let payload = raw?;

        // Validate the CALLRESULT against the `{action}Response` schema before
        // deserializing, mirroring `_handle_call_result()` in charge_point.py
        // and the CP-side `ChargePoint::call`. A CP's response is a trust
        // boundary: a frame that violates the schema but still deserializes into
        // `Req::Response` — an over-`maxLength` string, or an extra property
        // under the schema's `additionalProperties: false` (serde ignores
        // unknown fields) — is rejected explicitly rather than silently
        // accepted.
        if let Some(validator) = &self.validator {
            validator.validate_call_result(Req::ACTION_NAME, &payload)?;
        }

        serde_json::from_value::<Req::Response>(payload).map_err(OcppError::from)
    }

    /// Send a `Reset` command (Soft or Hard) to a connected charge point and
    /// return the CP's [`ResetStatus`].
    ///
    /// Thin typed wrapper over [`call`](Self::call) — the CSMS half of the
    /// OCPP 1.6J Reset use case (§5.13). Mirrors how the Python reference
    /// drives the command from
    /// [`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py).
    ///
    /// `Accepted` means the CP has committed to *attempting* the reset; the
    /// reset itself (graceful transaction shutdown for Soft, a full reboot for
    /// Hard) then runs CP-side. Surfaces the same errors as [`call`](Self::call)
    /// (e.g. [`OcppError::CpNotConnected`], [`OcppError::Timeout`]).
    pub async fn reset(&self, cp_id: &str, reset_type: ResetType) -> OcppResult<ResetStatus> {
        let response = self.call(cp_id, ResetRequest { reset_type }).await?;
        Ok(response.status)
    }

    /// Ask a connected charge point to start a transaction remotely.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `RemoteStartTransaction` command (§5.11), mirroring how the Python
    /// reference's central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `connector_id` is optional: `None` lets the charge point pick a connector.
    /// Returns the CP's [`RemoteStartStopStatus`] — `Accepted` if the CP will act
    /// on the request, `Rejected` otherwise. Errors propagate from [`call`](Self::call)
    /// (e.g. [`OcppError::CpNotConnected`], [`OcppError::Timeout`]).
    pub async fn remote_start_transaction(
        &self,
        cp_id: &str,
        id_tag: impl Into<String>,
        connector_id: Option<u32>,
    ) -> OcppResult<RemoteStartStopStatus> {
        let resp = self
            .call(
                cp_id,
                RemoteStartTransactionRequest {
                    connector_id,
                    id_tag: id_tag.into(),
                    charging_profile: None,
                },
            )
            .await?;
        Ok(resp.status)
    }

    /// Ask a connected charge point to stop an ongoing transaction remotely.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `RemoteStopTransaction` command (§5.12). `transaction_id` is the
    /// CSMS-assigned id returned by the original `StartTransaction`.
    ///
    /// Returns the CP's [`RemoteStartStopStatus`] — `Accepted` if the CP knows
    /// the transaction and will stop it, `Rejected` if it does not. Errors
    /// propagate from [`call`](Self::call).
    pub async fn remote_stop_transaction(
        &self,
        cp_id: &str,
        transaction_id: i32,
    ) -> OcppResult<RemoteStartStopStatus> {
        let resp = self
            .call(cp_id, RemoteStopTransactionRequest { transaction_id })
            .await?;
        Ok(resp.status)
    }

    /// Ask a connected charge point to unlock a connector's cable (OCPP 1.6J
    /// §5.21), mirroring how the Python reference's central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// A typed convenience wrapper over [`call`](Self::call). Per the spec the CP
    /// stops any ongoing transaction on the connector (reason `UnlockCommand`)
    /// before releasing the cable. Returns the CP's [`UnlockStatus`] — `Unlocked`
    /// when the connector was (or already is) unlocked, `UnlockFailed` when it
    /// could not be unlocked (mechanical fault, or an unknown/out-of-range
    /// connector), or `NotSupported` when the connector has no controllable lock.
    /// Errors propagate from [`call`](Self::call) (e.g.
    /// [`OcppError::CpNotConnected`], [`OcppError::Timeout`]).
    pub async fn unlock_connector(
        &self,
        cp_id: &str,
        connector_id: u32,
    ) -> OcppResult<UnlockStatus> {
        let resp = self
            .call(cp_id, UnlockConnectorRequest { connector_id })
            .await?;
        Ok(resp.status)
    }

    /// Read configuration keys from a connected charge point.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `GetConfiguration` command (§5.8), mirroring how the Python reference's
    /// central system reads CP configuration
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `keys` is optional: `None` requests *all* configuration keys, while
    /// `Some(list)` requests only the named keys. The full
    /// [`GetConfigurationResponse`] is returned — unlike the
    /// [`RemoteStartStopStatus`] helpers — because both halves carry
    /// information the CSMS needs: `configuration_keys` (the keys the CP knows,
    /// with their values and read-only flags) and `unknown_keys` (requested
    /// keys the CP does not recognise). Errors propagate from [`call`](Self::call)
    /// (e.g. [`OcppError::CpNotConnected`], [`OcppError::Timeout`]).
    pub async fn get_configuration(
        &self,
        cp_id: &str,
        keys: Option<Vec<String>>,
    ) -> OcppResult<GetConfigurationResponse> {
        self.call(cp_id, GetConfigurationRequest { key: keys })
            .await
    }

    /// Change a single configuration key on a connected charge point.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `ChangeConfiguration` command (§5.3). Returns the CP's
    /// [`ConfigurationStatus`]: `Accepted` when the key was updated,
    /// `Rejected` for a read-only key, `RebootRequired` when the change takes
    /// effect only after a reboot, or `NotSupported` for a key the CP does not
    /// accept. Errors propagate from [`call`](Self::call).
    pub async fn change_configuration(
        &self,
        cp_id: &str,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> OcppResult<ConfigurationStatus> {
        let resp = self
            .call(
                cp_id,
                ChangeConfigurationRequest {
                    key: key.into(),
                    value: value.into(),
                },
            )
            .await?;
        Ok(resp.status)
    }

    /// Ask a connected charge point to proactively send a specific message now.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `TriggerMessage` command (§4.x), mirroring how the Python reference's
    /// central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `connector_id` is optional and scopes connector-specific messages (e.g.
    /// `StatusNotification`); `None` is not connector-specific. Returns the CP's
    /// [`TriggerMessageStatus`] — `Accepted` when the CP will send the requested
    /// message, `NotImplemented` for a message it does not support, or
    /// `Rejected` if it cannot honor the request right now. Errors propagate
    /// from [`call`](Self::call) (e.g. [`OcppError::CpNotConnected`],
    /// [`OcppError::Timeout`]).
    pub async fn trigger_message(
        &self,
        cp_id: &str,
        requested_message: MessageTrigger,
        connector_id: Option<i32>,
    ) -> OcppResult<TriggerMessageStatus> {
        let resp = self
            .call(
                cp_id,
                TriggerMessageRequest {
                    requested_message,
                    connector_id,
                },
            )
            .await?;
        Ok(resp.status)
    }

    /// Ask a connected charge point to clear its authorization cache.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `ClearCache` command (§5.2), mirroring how the Python reference's central
    /// system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `ClearCache` carries no request fields, so the helper takes only `cp_id`.
    /// Returns the CP's [`ClearCacheStatus`] — `Accepted` once the local
    /// authorization cache has been emptied, `Rejected` if the CP declines.
    /// Errors propagate from [`call`](Self::call) (e.g.
    /// [`OcppError::CpNotConnected`], [`OcppError::Timeout`]).
    pub async fn clear_cache(&self, cp_id: &str) -> OcppResult<ClearCacheStatus> {
        let resp = self.call(cp_id, ClearCacheRequest {}).await?;
        Ok(resp.status)
    }

    /// Ask a connected charge point for the version of its Local Authorization
    /// List.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `GetLocalListVersion` command (§5.x), mirroring how the Python
    /// reference's central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `GetLocalListVersion` carries no request fields. Returns the list version
    /// — `0` for an empty list, `-1` if the CP does not support the feature, or
    /// the `listVersion` of the last accepted [`send_local_list`](Self::send_local_list).
    /// Errors propagate from [`call`](Self::call).
    pub async fn get_local_list_version(&self, cp_id: &str) -> OcppResult<i32> {
        let resp = self.call(cp_id, GetLocalListVersionRequest {}).await?;
        Ok(resp.list_version)
    }

    /// Push a Local Authorization List update to a connected charge point.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `SendLocalList` command (§5.x), mirroring how the Python reference's
    /// central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `list_version` is the version the list will carry after this update;
    /// `update_type` selects a [`UpdateType::Full`] replace or a
    /// [`UpdateType::Differential`] delta; `local_authorization_list` carries the
    /// entries (an entry with no `idTagInfo` in a differential update deletes that
    /// id tag). Returns the CP's [`UpdateStatus`] — `Accepted`, `Failed`,
    /// `NotSupported`, or `VersionMismatch`. Errors propagate from
    /// [`call`](Self::call).
    pub async fn send_local_list(
        &self,
        cp_id: &str,
        list_version: i32,
        update_type: UpdateType,
        local_authorization_list: Vec<AuthorizationData>,
    ) -> OcppResult<UpdateStatus> {
        let resp = self
            .call(
                cp_id,
                SendLocalListRequest {
                    list_version,
                    update_type,
                    local_authorization_list,
                },
            )
            .await?;
        Ok(resp.status)
    }

    /// Reserve a connector on a connected charge point until `expiry_date`.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `ReserveNow` command (§5.14), mirroring how the Python reference's
    /// central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `connector_id` is the connector to reserve; `reservation_id` is the
    /// caller-chosen id used later to [`cancel_reservation`](Self::cancel_reservation).
    /// Returns the CP's [`ReservationStatus`] — `Accepted` when the connector is
    /// held, or `Occupied` / `Faulted` / `Unavailable` / `Rejected` per the
    /// connector's state. Errors propagate from [`call`](Self::call) (e.g.
    /// [`OcppError::CpNotConnected`], [`OcppError::Timeout`]).
    #[allow(clippy::too_many_arguments)]
    pub async fn reserve_now(
        &self,
        cp_id: &str,
        connector_id: i32,
        id_tag: impl Into<String>,
        expiry_date: DateTime<Utc>,
        reservation_id: i32,
        parent_id_tag: Option<String>,
    ) -> OcppResult<ReservationStatus> {
        let resp = self
            .call(
                cp_id,
                ReserveNowRequest {
                    connector_id,
                    expiry_date,
                    id_tag: id_tag.into(),
                    reservation_id,
                    parent_id_tag,
                },
            )
            .await?;
        Ok(resp.status)
    }

    /// Cancel a reservation on a connected charge point by `reservation_id`.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `CancelReservation` command (§5.4). Returns the CP's
    /// [`CancelReservationStatus`] — `Accepted` if the CP held that reservation
    /// (the connector is freed), `Rejected` if the id is unknown. Errors
    /// propagate from [`call`](Self::call).
    pub async fn cancel_reservation(
        &self,
        cp_id: &str,
        reservation_id: i32,
    ) -> OcppResult<CancelReservationStatus> {
        let resp = self
            .call(cp_id, CancelReservationRequest { reservation_id })
            .await?;
        Ok(resp.status)
    }

    /// Install a Smart Charging profile on a connected charge point.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `SetChargingProfile` command (§5.16), mirroring how the Python reference's
    /// central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `connector_id` is the connector the profile targets (`0` =
    /// charge-point-wide). Returns the CP's [`ChargingProfileStatus`] —
    /// `Accepted` when the profile is installed, `Rejected` for an invalid
    /// placement (e.g. a `ChargePointMaxProfile` at a real connector or an
    /// unknown connector). Errors propagate from [`call`](Self::call).
    pub async fn set_charging_profile(
        &self,
        cp_id: &str,
        connector_id: i32,
        cs_charging_profiles: ChargingProfile,
    ) -> OcppResult<ChargingProfileStatus> {
        let resp = self
            .call(
                cp_id,
                SetChargingProfileRequest {
                    connector_id,
                    cs_charging_profiles,
                },
            )
            .await?;
        Ok(resp.status)
    }

    /// Clear Smart Charging profiles on a connected charge point.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `ClearChargingProfile` command (§5.2). Each filter is optional and a
    /// `None` matches anything, so an all-`None` call clears every installed
    /// profile. Returns the CP's [`ClearChargingProfileStatus`] — `Accepted` if
    /// at least one profile matched and was cleared, `Unknown` otherwise. Errors
    /// propagate from [`call`](Self::call).
    pub async fn clear_charging_profile(
        &self,
        cp_id: &str,
        id: Option<i32>,
        connector_id: Option<i32>,
        charging_profile_purpose: Option<ChargingProfilePurposeType>,
        stack_level: Option<i32>,
    ) -> OcppResult<ClearChargingProfileStatus> {
        let resp = self
            .call(
                cp_id,
                ClearChargingProfileRequest {
                    id,
                    connector_id,
                    charging_profile_purpose,
                    stack_level,
                },
            )
            .await?;
        Ok(resp.status)
    }

    /// Ask a connected charge point for the composite charging schedule of a
    /// connector.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `GetCompositeSchedule` command (§5.x). `connector_id` is the connector the
    /// schedule is requested for (`0` = entire charge point); `duration` is the
    /// length of the reported window in seconds; `charging_rate_unit` optionally
    /// requests the unit (`W`/`A`) of the returned schedule. The full
    /// [`GetCompositeScheduleResponse`] is returned because the caller needs the
    /// `status`, the `connectorId`, the `scheduleStart`, and the computed
    /// `chargingSchedule` together. Errors propagate from [`call`](Self::call).
    pub async fn get_composite_schedule(
        &self,
        cp_id: &str,
        connector_id: i32,
        duration: i32,
        charging_rate_unit: Option<ChargingRateUnitType>,
    ) -> OcppResult<GetCompositeScheduleResponse> {
        self.call(
            cp_id,
            GetCompositeScheduleRequest {
                connector_id,
                duration,
                charging_rate_unit,
            },
        )
        .await
    }

    /// Ask a connected charge point to upload a diagnostics archive to `location`.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `GetDiagnostics` command (§4.x, firmware-management profile), mirroring
    /// how the Python reference's central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `location` is the URL the CP uploads to. `retries` / `retry_interval`
    /// bound the CP's upload attempts; `start_time` / `stop_time` scope which
    /// log entries to collect — all optional, matching the spec. The full
    /// [`GetDiagnosticsResponse`] is returned because its `file_name` (the name
    /// of the archive the CP will produce, present only when the CP accepts the
    /// request) is information the CSMS needs to correlate the subsequent
    /// `DiagnosticsStatusNotification` progress. Errors propagate from
    /// [`call`](Self::call) (e.g. [`OcppError::CpNotConnected`],
    /// [`OcppError::Timeout`]).
    pub async fn get_diagnostics(
        &self,
        cp_id: &str,
        location: impl Into<String>,
        retries: Option<i32>,
        retry_interval: Option<i32>,
        start_time: Option<DateTime<Utc>>,
        stop_time: Option<DateTime<Utc>>,
    ) -> OcppResult<GetDiagnosticsResponse> {
        self.call(
            cp_id,
            GetDiagnosticsRequest {
                location: location.into(),
                retries,
                retry_interval,
                start_time,
                stop_time,
            },
        )
        .await
    }

    /// Instruct a connected charge point to download and install firmware from
    /// `location` at/after `retrieve_date`.
    ///
    /// A typed convenience wrapper over [`call`](Self::call) for the OCPP 1.6J
    /// `UpdateFirmware` command (§4.x, firmware-management profile), mirroring
    /// how the Python reference's central system drives it
    /// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
    ///
    /// `location` is the URL the CP downloads from; `retrieve_date` is the
    /// earliest time the CP should start the download. `retries` /
    /// `retry_interval` bound the CP's download attempts — both optional,
    /// matching the spec. `UpdateFirmware.conf` carries no fields per the spec,
    /// so the returned [`UpdateFirmwareResponse`] is empty; the CP reports its
    /// progress out-of-band via `FirmwareStatusNotification` CALLs. Errors
    /// propagate from [`call`](Self::call) (e.g. [`OcppError::CpNotConnected`],
    /// [`OcppError::Timeout`]).
    pub async fn update_firmware(
        &self,
        cp_id: &str,
        location: impl Into<String>,
        retrieve_date: DateTime<Utc>,
        retries: Option<i32>,
        retry_interval: Option<i32>,
    ) -> OcppResult<UpdateFirmwareResponse> {
        self.call(
            cp_id,
            UpdateFirmwareRequest {
                location: location.into(),
                retries,
                retrieve_date,
                retry_interval,
            },
        )
        .await
    }

    /// Evict connections that have been idle for more than 2× the keep-alive interval.
    pub async fn cleanup_idle_connections(&self) -> TransportResult<usize> {
        let timeout = self.config.keep_alive_interval * 2;
        let idle: Vec<Uuid> = self
            .connections
            .iter()
            .filter_map(|e| e.is_idle(timeout).then_some(*e.key()))
            .collect();

        for id in &idle {
            self.connections.remove(id);
            let _ = self.event_tx.send(TransportEvent::Disconnected {
                connection_id: *id,
                reason: "Idle timeout".to_string(),
            });
        }

        let removed = idle.len();
        if removed > 0 {
            info!("Cleaned up {} idle connections", removed);
        }
        Ok(removed)
    }

    /// Basic server statistics.
    pub fn get_stats(&self) -> ServerStats {
        ServerStats {
            active_connections: self.connection_count(),
        }
    }
}

// ─── axum WebSocket handlers ──────────────────────────────────────────────────

/// Validate the WebSocket upgrade request (subprotocol + CP-ID) then hand off.
async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(charge_point_id): Path<String>,
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    // The connect path is an attacker-influenceable trust boundary: whatever we
    // accept here becomes the routing key for every subsequent CALL on this
    // socket. Mirror the hardened pure parser `websocket::server::extract_charge_point_id`
    // (ported from mobilityhouse/ocpp `charge_point.py`), which trims surrounding
    // whitespace and refuses an empty / whitespace-only segment — so the live
    // handshake and the pinned pure-function contract agree (issue #330). The
    // axum `Path` param is percent-decoded, so `/ocpp/%20%20` arrives here as
    // `"   "`; trimming collapses it to empty and the id is rejected rather than
    // registered as a whitespace-only routing key.
    let charge_point_id = charge_point_id.trim();
    // OCPP 1.6J §3.1 — charge-point IDs are ≤ 48 characters (CiString48).
    if charge_point_id.is_empty() || charge_point_id.len() > 48 {
        warn!(
            "Rejected connection: invalid charge_point_id '{}'",
            charge_point_id
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    // A space-padded but non-empty id (e.g. `/ocpp/%20CP001%20`) is accepted as
    // its trimmed form `CP001` — matching the pure parser — so the routing key
    // stored in `cp_handles` is exactly what the CSMS `call()` API addresses.
    let charge_point_id = charge_point_id.to_string();

    // Negotiate the OCPP-J subprotocol against the server's configured accepted
    // set (`TransportConfig::sub_protocols`, e.g. `["ocpp1.6", "ocpp2.0.1"]`) —
    // no hardcoded `ocpp1.6` literal. A station offering `ocpp2.0.1` is accepted
    // and echoed back `ocpp2.0.1`; one offering nothing supported (or a bogus
    // token like `ocpp2.0`) is rejected with HTTP 400.
    let offered = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok());
    let Some(sub_protocol) = negotiate_subprotocol(offered, &state.config.sub_protocols) else {
        warn!(
            "Rejected '{}': no supported Sec-WebSocket-Protocol offered (server supports {:?}, client offered {:?})",
            charge_point_id, state.config.sub_protocols, offered
        );
        return Err(StatusCode::BAD_REQUEST);
    };

    Ok(ws
        .protocols([sub_protocol.clone()])
        .on_upgrade(move |socket| handle_cp_socket(socket, charge_point_id, sub_protocol, state)))
}

/// Choose the WebSocket subprotocol to answer an incoming CSMS handshake with.
///
/// OCPP-J negotiation: the charging station offers one or more
/// `Sec-WebSocket-Protocol` tokens (comma-separated) and the CSMS replies with
/// the single subprotocol it selected. This mirrors the mobilityhouse/ocpp
/// example servers, which pass the version's identifier to
/// `websockets.serve(subprotocols=[...])` while the connection handling itself
/// stays version-agnostic — the same shape as this crate's version-generic
/// [`DispatchHandler`](crate::DispatchHandler) + `ActionDispatcher`.
///
/// Selection is by **server preference order**: the first entry in `supported`
/// that the station also offered. This makes the outcome deterministic when a
/// station offers several (e.g. `ocpp1.6, ocpp2.0.1` → `ocpp1.6`, since the
/// default `supported` lists 1.6 first).
///
/// Returns `None` — meaning "reject the upgrade with HTTP 400" — when the header
/// is absent/empty or the offered∩supported intersection is empty (a bogus
/// token such as `ocpp2.0`, which is *not* a valid OCPP-J identifier, is thus
/// rejected). `supported` is the configured accepted set, never a literal.
fn negotiate_subprotocol(header: Option<&str>, supported: &[String]) -> Option<String> {
    let offered: Vec<&str> = header?
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    supported
        .iter()
        .find(|proto| offered.contains(&proto.as_str()))
        .cloned()
}

/// Classify an incoming text frame into a typed [`Message`].
///
/// The wire enum is `#[serde(untagged)]` and — because `MessageType` serialises
/// as an UPPERCASE string and `CallMessage` accepts any type tag — a CALLERROR
/// `["CALLERROR", id, code, desc, details]` would otherwise be mis-decoded as a
/// CALL. We disambiguate on the message-type discriminator (field `"0"`) so a
/// CP's CALLERROR response is correctly routed to the pending-call map instead
/// of being dispatched as an inbound request.
fn classify_frame(text: &str) -> Option<Message> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    match value.get("0").and_then(|t| t.as_str()) {
        Some("CALLERROR") => serde_json::from_value(value).ok().map(Message::CallError),
        Some("CALLRESULT") => serde_json::from_value(value).ok().map(Message::CallResult),
        Some("CALL") => serde_json::from_value(value).ok().map(Message::Call),
        // Unknown/absent discriminator: fall back to best-effort untagged decode.
        _ => serde_json::from_str(text).ok(),
    }
}

/// Per-charge-point send/receive loop.
///
/// A single task owns both halves of the socket and interleaves, via
/// `tokio::select!`:
/// - **outbound**: frames queued by [`OcppServer::call`] on the per-CP channel
///   (CSMS→CP CALLs), and
/// - **inbound**: frames from the CP — CALLs are dispatched through the
///   `MessageHandler` and answered with CALLRESULT/CALLERROR; CALLRESULT/CALLERROR
///   frames are correlated back to in-flight server CALLs via the `PendingCallMap`.
///
/// Owning the sink in one task (rather than behind a shared mutex) is the same
/// design used by [`WebSocketClient`](crate::WebSocketClient) and avoids a
/// send/receive deadlock. On close or any WS error the connection and its
/// routing handle are removed and pending CALLs are cancelled.
async fn handle_cp_socket(
    socket: WebSocket,
    charge_point_id: String,
    sub_protocol: String,
    state: Arc<ServerState>,
) {
    let mut info = ConnectionInfo::new(charge_point_id.clone(), "csms".to_string());
    // Record the *negotiated* subprotocol (e.g. `ocpp2.0.1` for a 2.0.1
    // station), not a hardcoded `ocpp1.6`, so `ConnectionInfo::sub_protocol`
    // reflects the version this session actually speaks.
    info.sub_protocol = Some(sub_protocol);
    let connection_id = info.id;

    // Outbound channel: `OcppServer::call` pushes CALL frames here; this loop
    // drains them to the sink. Pending map: correlates the CP's responses.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<WsMessage>();
    let pending = Arc::new(PendingCallMap::new());

    state.connections.insert(connection_id, info);
    state.cp_handles.insert(
        charge_point_id.clone(),
        CpHandle {
            connection_id,
            outbound: out_tx,
            pending: Arc::clone(&pending),
        },
    );
    let _ = state.event_tx.send(TransportEvent::Connected {
        connection_id,
        remote_addr: charge_point_id.clone(),
    });
    info!(
        "ChargePoint '{}' connected (id={})",
        charge_point_id, connection_id
    );

    let (mut sink, mut stream) = socket.split();

    'session: loop {
        tokio::select! {
            // ── Outbound: server-initiated CALL frames ────────────────────────
            outbound = out_rx.recv() => {
                match outbound {
                    Some(frame) => {
                        if sink.send(frame).await.is_err() {
                            break 'session;
                        }
                    }
                    // All `CpHandle`s dropped — only happens during teardown.
                    None => break 'session,
                }
            }
            // ── Inbound: frames from the charge point ─────────────────────────
            incoming = stream.next() => {
                let text = match incoming {
                    Some(Ok(WsMessage::Text(t))) => t,
                    Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => break 'session,
                    Some(Ok(_)) => continue, // ping/pong/binary handled elsewhere
                };

                if text.len() > state.config.max_message_size {
                    warn!(
                        "Message from '{}' exceeds {} bytes; closing",
                        charge_point_id, state.config.max_message_size
                    );
                    break 'session;
                }

                match classify_frame(&text) {
                    Some(Message::Call(call)) => {
                        let unique_id = call.unique_id.clone();
                        // `StatusNotification` is the one inbound action the CSMS
                        // surfaces as a *semantic* event (#47). The `cp_id` lives
                        // here at the connection layer, not in the payload the
                        // dispatcher sees, so we snapshot the payload before
                        // wrapping the frame and emit the event only after the
                        // handler accepts it (below) — keeping the empty CALLRESULT
                        // and avoiding an event for a frame the CSMS rejects.
                        let status_payload = (call.action
                            == StatusNotificationRequest::ACTION_NAME)
                            .then(|| call.payload.clone());
                        // Snapshot Start/StopTransaction requests the same way:
                        // the transaction-lifecycle events (#66) need the request
                        // fields, and — for StartTransaction — the CSMS-assigned
                        // `transactionId` from the *response*, so we emit only
                        // after an accepted CALLRESULT (below).
                        let txn_request = (call.action == StartTransactionRequest::ACTION_NAME
                            || call.action == StopTransactionRequest::ACTION_NAME)
                            .then(|| (call.action.clone(), call.payload.clone()));
                        // Snapshot BootNotification / MeterValues the same way
                        // (#345): both carry all their event fields in the
                        // *request*, but — like the transaction events — we emit
                        // only on an accepted CALLRESULT so a refused CALL
                        // surfaces nothing.
                        let lifecycle_request = (call.action
                            == BootNotificationRequest::ACTION_NAME
                            || call.action == MeterValuesRequest::ACTION_NAME)
                            .then(|| (call.action.clone(), call.payload.clone()));
                        let msg = Message::Call(call);

                        let _ = state.event_tx.send(TransportEvent::MessageReceived {
                            connection_id,
                            message: msg.clone(),
                        });

                        let dispatch_result = state.message_handler.handle_message(msg).await;

                        // Emit the connector-state event for an accepted
                        // StatusNotification. Deserialization (which validates the
                        // status/error enums) doubles as a guard against emitting
                        // for a malformed payload the dispatcher would also reject.
                        if dispatch_result.is_ok() {
                            if let Some(payload) = status_payload {
                                match serde_json::from_value::<StatusNotificationRequest>(payload) {
                                    Ok(req) => {
                                        info!(
                                            "ChargePoint '{}' connector {} -> {:?}",
                                            charge_point_id, req.connector_id, req.status
                                        );
                                        let _ = state.event_tx.send(
                                            TransportEvent::StatusNotification {
                                                cp_id: charge_point_id.clone(),
                                                connector_id: req.connector_id,
                                                status: req.status,
                                            },
                                        );
                                    }
                                    Err(e) => warn!(
                                        "Accepted StatusNotification from '{}' failed to decode for event: {}",
                                        charge_point_id, e
                                    ),
                                }
                            }
                        }

                        // Transaction-lifecycle events (#66): emit only when the
                        // handler *accepted* the CALL (a real CALLRESULT, not a
                        // CALLERROR), reading the CSMS-assigned `transactionId`
                        // out of the StartTransaction response. A rejected CALL —
                        // which comes back as `Ok(Some(Message::CallError))` from
                        // `DispatchHandler`, or `Err(..)` from a bespoke handler —
                        // produces no event.
                        if let (Some((action, req_payload)), Ok(Some(Message::CallResult(result)))) =
                            (txn_request, &dispatch_result)
                        {
                            emit_transaction_event(
                                &state.event_tx,
                                &charge_point_id,
                                &action,
                                &req_payload,
                                &result.payload,
                            );
                        }

                        // BootNotification / MeterValues events (#345): same
                        // acceptance gate as the transaction events — only a real
                        // CALLRESULT, never a CALLERROR, surfaces an event.
                        if let (Some((action, req_payload)), Ok(Some(Message::CallResult(_)))) =
                            (lifecycle_request, &dispatch_result)
                        {
                            emit_lifecycle_event(
                                &state.event_tx,
                                &charge_point_id,
                                &action,
                                &req_payload,
                            );
                        }

                        let response_text = match dispatch_result {
                            Ok(Some(response)) => serde_json::to_string(&response)
                                .map_err(|e| error!("Failed to serialize response: {}", e))
                                .ok(),
                            Ok(None) => None,
                            Err(e) => {
                                let callerror = build_call_error(&unique_id, &e);
                                serde_json::to_string(&callerror)
                                    .map_err(|e| error!("Failed to serialize CALLERROR: {}", e))
                                    .ok()
                            }
                        };

                        if let Some(resp) = response_text {
                            if sink.send(WsMessage::Text(resp)).await.is_err() {
                                break 'session;
                            }
                        }
                    }
                    Some(Message::CallResult(result)) => {
                        // Response to a CSMS-initiated CALL. If it doesn't match a
                        // pending call, surface it as an event for legacy observers.
                        if !pending.resolve(&result.unique_id, result.payload.clone()) {
                            let _ = state.event_tx.send(TransportEvent::MessageReceived {
                                connection_id,
                                message: Message::CallResult(result),
                            });
                        }
                    }
                    Some(Message::CallError(err)) => {
                        let unique_id = err.unique_id.clone();
                        let ocpp_err = OcppError::CallError {
                            code: err.error_code.clone(),
                            description: err.error_description.clone(),
                            details: err.error_details.clone(),
                        };
                        if !pending.reject(&unique_id, ocpp_err) {
                            warn!(
                                "CALLERROR from '{}' for unknown unique_id '{}'",
                                charge_point_id, unique_id
                            );
                        }
                    }
                    None => {
                        // Cannot correlate without a parseable frame — log and continue.
                        warn!("Malformed OCPP frame from '{}'", charge_point_id);
                    }
                }
            }
        }
    }

    // Teardown: remove routing state and fail any in-flight server CALLs so
    // their futures resolve instead of hanging until timeout.
    //
    // Compare-and-remove: only evict the routing handle if it still belongs to
    // *this* session. If the CP reconnected while we were tearing down, a newer
    // session has already overwritten the entry with its own `connection_id`, and
    // an unconditional `remove` here would wrongly evict the live session — making
    // a connected CP look like `CpNotConnected` to `OcppServer::call` (issue #50).
    state
        .cp_handles
        .remove_if(&charge_point_id, |_, h| h.connection_id == connection_id);
    pending.cancel_all();
    state.connections.remove(&connection_id);
    let _ = state.event_tx.send(TransportEvent::Disconnected {
        connection_id,
        reason: "Connection closed".to_string(),
    });
    info!("ChargePoint '{}' disconnected", charge_point_id);
}

/// Build the outgoing CALLERROR frame for an `OcppError` — its error code,
/// human-readable description, and `details` map.
///
/// Shared with [`DispatchHandler`](crate::dispatch_handler::DispatchHandler) so
/// server-side routing produces the same CALLERROR frames as the inline server
/// receive loop. Unrouted-action errors (`NotImplemented`/`NotSupported`) carry
/// the reference's spec-canonical description and a `{"cause": …}` detail; every
/// other variant keeps its `Display` text and empty details.
pub(crate) fn build_call_error(unique_id: &str, error: &OcppError) -> Message {
    // Port of `_raise_key_error` + `create_call_error` from
    // `ocpp/charge_point.py`: the two unrouted-action errors carry the
    // spec-canonical `default_description` (from `ocpp/exceptions.py`) and a
    // machine-readable `{"cause": …}` detail — the operator-facing reason a peer's
    // CALL was refused at the routing trust boundary. Every other variant keeps
    // its `Display` text and empty (`{}`) details, exactly as before.
    let (code, description, details): (CallErrorCode, String, Option<serde_json::Value>) =
        match error {
            // A known action for the negotiated version with no registered
            // handler — the `NotImplementedError` branch of `_raise_key_error`,
            // which raises `details={"cause": f"No handler for {action}
            // registered."}`. This variant is constructed *only* by
            // `ActionDispatcher::unrouted_action_error`, so `feature` is always
            // the bare action name and the cause is unambiguous.
            OcppError::NotImplemented { feature } => (
                CallErrorCode::NotImplemented,
                CallErrorCode::NotImplemented
                    .default_description()
                    .to_string(),
                Some(serde_json::json!({
                    "cause": format!("No handler for {feature} registered."),
                })),
            ),
            // An action the negotiated version does not define — the
            // `NotSupportedError` branch of `_raise_key_error`. The reference
            // cause embeds the OCPP version (`… not supported by OCPP2.0.1.`),
            // which isn't threaded to this layer, and the same variant is also
            // raised for a missing bundled schema; so we emit a version-agnostic
            // cause here (see issue #311). The canonical description matches.
            OcppError::NotSupported { feature } => (
                CallErrorCode::NotSupported,
                CallErrorCode::NotSupported
                    .default_description()
                    .to_string(),
                Some(serde_json::json!({
                    "cause": format!("{feature} not supported by receiver."),
                })),
            ),
            // A JSON-Schema validation failure. The code is the keyword-granular
            // one from `_validate_payload()` in `ocpp/messages.py`; the `details`
            // surface the triggering-message context the reference attaches to the
            // raised `OCPPError`
            // (`tests/test_exceptions.py::test_exception_show_triggered_*`).
            //
            // The reference stores the whole triggering `Call` under an
            // `ocpp_message` key (a Python `repr` that also echoes the payload).
            // Echoing a peer's own payload back is redundant and can be large, so
            // we surface just the offending `action` name plus a machine-readable
            // `cause` (the schema-violation message, the equivalent of the
            // reference's `e.message`) — the idiomatic port (see issue #313).
            // Faithful to the reference's per-keyword split, the `required` branch
            // — which raises `ProtocolError` with only a `{"cause": …}` detail and
            // *no* `ocpp_message` — omits the action.
            OcppError::SchemaViolation {
                keyword,
                message,
                action,
            } => {
                let details = match keyword {
                    SchemaKeyword::Required => serde_json::json!({ "cause": message }),
                    _ => serde_json::json!({ "action": action, "cause": message }),
                };
                (keyword.call_error_code(), error.to_string(), Some(details))
            }
            OcppError::ValidationError { .. } | OcppError::Json { .. } => {
                (CallErrorCode::FormationViolation, error.to_string(), None)
            }
            _ => (CallErrorCode::InternalError, error.to_string(), None),
        };
    Message::call_error(unique_id.to_string(), code, description, details)
}

/// Emit the semantic transaction-lifecycle [`TransportEvent`] for an accepted
/// `StartTransaction` / `StopTransaction` CALL.
///
/// Two pieces of context the version-generic dispatcher can't see are bridged
/// into a typed event for hosts that embed the CSMS (issue #66): the `cp_id`
/// (which lives at the connection layer, not the payload) and, for
/// `StartTransaction`, the CSMS-assigned `transactionId` (which lives in the
/// *response*, not the request). Called only for an accepted CALLRESULT, so a
/// rejected transaction never surfaces an event; a payload that nonetheless
/// fails to deserialize is skipped rather than panicking the receive loop.
///
/// Mirrors the reference's `@on('StartTransaction')` / `@on('StopTransaction')`
/// central-system handlers
/// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
fn emit_transaction_event(
    event_tx: &mpsc::UnboundedSender<TransportEvent>,
    cp_id: &str,
    action: &str,
    request_payload: &serde_json::Value,
    response_payload: &serde_json::Value,
) {
    if action == StartTransactionRequest::ACTION_NAME {
        let (Ok(req), Ok(resp)) = (
            serde_json::from_value::<StartTransactionRequest>(request_payload.clone()),
            serde_json::from_value::<StartTransactionResponse>(response_payload.clone()),
        ) else {
            warn!("Accepted StartTransaction from '{cp_id}' failed to decode for event");
            return;
        };
        info!(
            "ChargePoint '{}' started transaction {} on connector {}",
            cp_id, resp.transaction_id, req.connector_id
        );
        let _ = event_tx.send(TransportEvent::TransactionStarted {
            cp_id: cp_id.to_string(),
            connector_id: req.connector_id,
            id_tag: req.id_tag,
            meter_start: req.meter_start,
            timestamp: req.timestamp,
            transaction_id: resp.transaction_id,
        });
    } else if action == StopTransactionRequest::ACTION_NAME {
        let Ok(req) = serde_json::from_value::<StopTransactionRequest>(request_payload.clone())
        else {
            warn!("Accepted StopTransaction from '{cp_id}' failed to decode for event");
            return;
        };
        info!(
            "ChargePoint '{}' stopped transaction {} (meterStop {})",
            cp_id, req.transaction_id, req.meter_stop
        );
        let _ = event_tx.send(TransportEvent::TransactionStopped {
            cp_id: cp_id.to_string(),
            transaction_id: req.transaction_id,
            meter_stop: req.meter_stop,
            timestamp: req.timestamp,
            reason: req.reason,
            id_tag: req.id_tag,
        });
    }
}

/// Emit the semantic [`TransportEvent`] for an accepted `BootNotification` /
/// `MeterValues` CALL (issue #345, part of the charge-hub embeddable-CSMS
/// surface #66).
///
/// Both actions carry all their event fields in the *request* (unlike
/// `StartTransaction`, whose id lives in the response), but — like the
/// transaction-lifecycle events — the event is emitted only for an accepted
/// CALLRESULT, so a CALL the CSMS refuses surfaces nothing. The `cp_id` is
/// bridged from the connection layer. A payload that nonetheless fails to
/// deserialize is logged and skipped rather than panicking the receive loop.
///
/// Mirrors the reference's `@on('BootNotification')` / `@on('MeterValues')`
/// central-system handlers
/// ([`examples/v16/central_system.py`](https://github.com/mobilityhouse/ocpp/blob/master/examples/v16/central_system.py)).
fn emit_lifecycle_event(
    event_tx: &mpsc::UnboundedSender<TransportEvent>,
    cp_id: &str,
    action: &str,
    request_payload: &serde_json::Value,
) {
    if action == BootNotificationRequest::ACTION_NAME {
        let Ok(req) = serde_json::from_value::<BootNotificationRequest>(request_payload.clone())
        else {
            warn!("Accepted BootNotification from '{cp_id}' failed to decode for event");
            return;
        };
        info!(
            "ChargePoint '{}' booted (vendor '{}', model '{}')",
            cp_id, req.charge_point_vendor, req.charge_point_model
        );
        let _ = event_tx.send(TransportEvent::BootNotification {
            cp_id: cp_id.to_string(),
            vendor: req.charge_point_vendor,
            model: req.charge_point_model,
            serial_number: req.charge_point_serial_number,
            firmware_version: req.firmware_version,
        });
    } else if action == MeterValuesRequest::ACTION_NAME {
        let Ok(req) = serde_json::from_value::<MeterValuesRequest>(request_payload.clone()) else {
            warn!("Accepted MeterValues from '{cp_id}' failed to decode for event");
            return;
        };
        info!(
            "ChargePoint '{}' reported {} meter value(s) on connector {} (txn {:?})",
            cp_id,
            req.meter_values.len(),
            req.connector_id,
            req.transaction_id
        );
        let _ = event_tx.send(TransportEvent::MeterValues {
            cp_id: cp_id.to_string(),
            connector_id: req.connector_id,
            transaction_id: req.transaction_id,
            meter_values: req.meter_values,
        });
    }
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// Basic server statistics.
#[derive(Debug, Clone)]
pub struct ServerStats {
    /// Number of charge points currently connected.
    pub active_connections: usize,
}

/// Stub per-connection manager kept for API surface compatibility.
pub struct ConnectionManager {
    #[allow(dead_code)]
    connection_id: Uuid,
    #[allow(dead_code)]
    connection_info: ConnectionInfo,
    message_handler: Option<Arc<dyn MessageHandler>>,
}

impl ConnectionManager {
    pub fn new(connection_info: ConnectionInfo) -> Self {
        Self {
            connection_id: connection_info.id,
            connection_info,
            message_handler: None,
        }
    }

    pub fn set_message_handler(&mut self, handler: Arc<dyn MessageHandler>) {
        self.message_handler = Some(handler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use ocpp_types::{CallResultMessage, MessageType, OcppResult};
    use std::{net::SocketAddr, sync::Arc};
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{client::IntoClientRequest, http::Request, Message as WsMsg},
    };

    // ── Mock: returns an empty CALLRESULT for any CALL ──────────────────────

    struct EchoHandler;

    #[async_trait::async_trait]
    impl MessageHandler for EchoHandler {
        async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
            if let Message::Call(call) = message {
                Ok(Some(Message::CallResult(CallResultMessage {
                    message_type: MessageType::CallResult,
                    unique_id: call.unique_id,
                    payload: serde_json::json!({}),
                })))
            } else {
                Ok(None)
            }
        }

        async fn handle_event(&self, _: TransportEvent) {}
    }

    // ── Mock: always returns NotSupported ───────────────────────────────────

    struct RejectHandler;

    #[async_trait::async_trait]
    impl MessageHandler for RejectHandler {
        async fn handle_message(&self, _: Message) -> OcppResult<Option<Message>> {
            Err(OcppError::NotSupported {
                feature: "stub".to_string(),
            })
        }

        async fn handle_event(&self, _: TransportEvent) {}
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    async fn start_server(handler: Arc<dyn MessageHandler>) -> (OcppServer, SocketAddr) {
        let (mut server, _rx) = OcppServer::new(TransportConfig::default(), handler);
        server.start("127.0.0.1:0").await.expect("server start");
        let addr = server.local_addr().unwrap();
        (server, addr)
    }

    /// Build a WS upgrade request with `ocpp1.6` subprotocol.
    ///
    /// Using `into_client_request()` on the URL string ensures tungstenite adds the required
    /// `Sec-WebSocket-Key`, `Upgrade`, and `Connection` headers automatically.
    fn ocpp_request(addr: SocketAddr, cp_id: &str) -> Request<()> {
        let mut req = format!("ws://{}/ocpp/{}", addr, cp_id)
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("sec-websocket-protocol", "ocpp1.6".parse().unwrap());
        req
    }

    /// Concrete WebSocket stream type returned by `connect_async`.
    type CpWs = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// Connect a raw client as charge point `cp_id` and wait until the server has
    /// registered its routing handle (so `server.call(cp_id, …)` can find it).
    async fn connect_cp(server: &OcppServer, addr: SocketAddr, cp_id: &str) -> CpWs {
        let (ws, _) = connect_async(ocpp_request(addr, cp_id))
            .await
            .expect("CP connects");
        let start = tokio::time::Instant::now();
        while !server.is_cp_connected(cp_id) && start.elapsed() < Duration::from_millis(500) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(server.is_cp_connected(cp_id), "server registered {cp_id}");
        ws
    }

    /// Serialise a CALLRESULT frame the way the server's decoder expects.
    fn call_result_frame(unique_id: &str, payload: serde_json::Value) -> String {
        serde_json::to_string(&Message::CallResult(CallResultMessage {
            message_type: MessageType::CallResult,
            unique_id: unique_id.to_string(),
            payload,
        }))
        .unwrap()
    }

    /// Read one CALL frame and return its `(action, unique_id)`.
    async fn read_call(ws: &mut CpWs) -> (String, String) {
        let frame = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("CP receives a CALL")
            .expect("stream open")
            .expect("WS ok");
        let WsMsg::Text(text) = frame else {
            panic!("expected text CALL frame, got {frame:?}");
        };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        (
            v.get("2").and_then(|a| a.as_str()).unwrap().to_string(),
            v.get("1").and_then(|i| i.as_str()).unwrap().to_string(),
        )
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn server_accepts_valid_ocpp16_connection() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        connect_async(ocpp_request(addr, "CP001"))
            .await
            .expect("should connect with ocpp1.6 subprotocol");

        // Poll until the per-CP task inserts into the map (usually < 10 ms)
        let start = tokio::time::Instant::now();
        while server.connection_count() == 0 && start.elapsed() < Duration::from_millis(500) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(server.connection_count(), 1);

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_rejects_connection_without_subprotocol() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        let result = connect_async(format!("ws://{}/ocpp/CP001", addr)).await;
        assert!(
            result.is_err(),
            "expected rejection (HTTP 400) with no subprotocol"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_rejects_wrong_subprotocol() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        let mut req = format!("ws://{}/ocpp/CP001", addr)
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("sec-websocket-protocol", "ocpp2.0".parse().unwrap());

        let result = connect_async(req).await;
        assert!(
            result.is_err(),
            "expected rejection for unsupported subprotocol"
        );

        server.stop().await.unwrap();
    }

    /// Build a WS upgrade request offering an arbitrary `Sec-WebSocket-Protocol`
    /// value (may be a comma-separated list), for exercising negotiation.
    fn request_offering(addr: SocketAddr, cp_id: &str, offered: &str) -> Request<()> {
        let mut req = format!("ws://{}/ocpp/{}", addr, cp_id)
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("sec-websocket-protocol", offered.parse().unwrap());
        req
    }

    /// A station offering the spec `ocpp2.0.1` identifier is **accepted** (the
    /// M7 handshake blocker, issue #338): the upgrade succeeds, the response
    /// echoes `ocpp2.0.1`, and the recorded `ConnectionInfo.sub_protocol`
    /// reflects the negotiated version — not a hardcoded `ocpp1.6`.
    #[tokio::test]
    async fn server_accepts_ocpp201_subprotocol() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        let (_ws, response) = connect_async(request_offering(addr, "CP201", "ocpp2.0.1"))
            .await
            .expect("should connect with ocpp2.0.1 subprotocol");

        assert_eq!(
            response
                .headers()
                .get("sec-websocket-protocol")
                .and_then(|v| v.to_str().ok()),
            Some("ocpp2.0.1"),
            "server must echo the negotiated ocpp2.0.1 subprotocol"
        );

        // The per-CP task records the negotiated protocol in ConnectionInfo.
        let start = tokio::time::Instant::now();
        while server.connection_count() == 0 && start.elapsed() < Duration::from_millis(500) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let conns = server.get_all_connections();
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].sub_protocol.as_deref(), Some("ocpp2.0.1"));

        server.stop().await.unwrap();
    }

    /// A station offering **both** identifiers gets a deterministic,
    /// server-preferred choice: the default accepted set lists `ocpp1.6` first,
    /// so `ocpp1.6, ocpp2.0.1` negotiates `ocpp1.6`.
    #[tokio::test]
    async fn server_negotiates_server_preferred_when_both_offered() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        let (_ws, response) = connect_async(request_offering(addr, "CPBOTH", "ocpp1.6, ocpp2.0.1"))
            .await
            .expect("should connect when offering both");

        assert_eq!(
            response
                .headers()
                .get("sec-websocket-protocol")
                .and_then(|v| v.to_str().ok()),
            Some("ocpp1.6"),
            "server preference (1.6 first) must win when both are offered"
        );

        server.stop().await.unwrap();
    }

    /// An `ocpp1.6` connection still negotiates and records `ocpp1.6` — guards
    /// against a regression where the negotiated value was hardcoded.
    #[tokio::test]
    async fn server_reports_negotiated_ocpp16() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        let (_ws, response) = connect_async(ocpp_request(addr, "CP16"))
            .await
            .expect("should connect with ocpp1.6");

        assert_eq!(
            response
                .headers()
                .get("sec-websocket-protocol")
                .and_then(|v| v.to_str().ok()),
            Some("ocpp1.6"),
        );

        let start = tokio::time::Instant::now();
        while server.connection_count() == 0 && start.elapsed() < Duration::from_millis(500) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let conns = server.get_all_connections();
        assert_eq!(conns[0].sub_protocol.as_deref(), Some("ocpp1.6"));

        server.stop().await.unwrap();
    }

    #[test]
    fn negotiate_subprotocol_selects_and_rejects() {
        let supported = vec!["ocpp1.6".to_string(), "ocpp2.0.1".to_string()];

        // Single supported offer is selected.
        assert_eq!(
            negotiate_subprotocol(Some("ocpp2.0.1"), &supported).as_deref(),
            Some("ocpp2.0.1")
        );
        assert_eq!(
            negotiate_subprotocol(Some("ocpp1.6"), &supported).as_deref(),
            Some("ocpp1.6")
        );

        // Both offered → server preference order (1.6 listed first) wins,
        // regardless of the order the client lists them.
        assert_eq!(
            negotiate_subprotocol(Some("ocpp1.6, ocpp2.0.1"), &supported).as_deref(),
            Some("ocpp1.6")
        );
        assert_eq!(
            negotiate_subprotocol(Some("ocpp2.0.1,ocpp1.6"), &supported).as_deref(),
            Some("ocpp1.6")
        );

        // Bogus / absent / empty → reject (None). `ocpp2.0` is NOT a valid
        // OCPP-J identifier and must not be accepted.
        assert_eq!(negotiate_subprotocol(Some("ocpp2.0"), &supported), None);
        assert_eq!(negotiate_subprotocol(None, &supported), None);
        assert_eq!(negotiate_subprotocol(Some(""), &supported), None);
        assert_eq!(negotiate_subprotocol(Some("  , "), &supported), None);

        // A server configured for a single version rejects the other.
        let only16 = vec!["ocpp1.6".to_string()];
        assert_eq!(negotiate_subprotocol(Some("ocpp2.0.1"), &only16), None);
    }

    /// A percent-encoded **whitespace-only** charge-point id (`/ocpp/%20%20%20`,
    /// which axum decodes to `"   "`) is refused at the handshake with HTTP 400 —
    /// it must never become a live routing key. This pins the live-server side
    /// of the pure `extract_charge_point_id("/   ") == None` contract, closing
    /// the runtime/pure-function divergence (issue #330). The subprotocol header
    /// is present and valid, so the *only* reason for rejection is the id.
    #[tokio::test]
    async fn server_rejects_whitespace_only_charge_point_id() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        // `%20` is a space; the decoded id "   " trims to empty and is refused.
        let result = connect_async(ocpp_request(addr, "%20%20%20")).await;
        assert!(
            result.is_err(),
            "expected HTTP 400 for a whitespace-only charge-point id"
        );

        // Trust-boundary negative: give any erroneously-accepted per-CP task a
        // moment to register, then confirm nothing did.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            server.connection_count(),
            0,
            "a refused whitespace-only id must not register a connection"
        );
        assert!(
            server.connected_cp_ids().is_empty(),
            "a refused whitespace-only id must not become a routing key"
        );

        server.stop().await.unwrap();
    }

    /// A space-padded but non-empty id (`/ocpp/%20CP001%20` → `" CP001 "`) is
    /// accepted as its **trimmed** form `CP001`, matching the pure parser — so
    /// the routing key stored in `cp_handles` is exactly what the CSMS `call()`
    /// API addresses, with no surprising whitespace (issue #330).
    #[tokio::test]
    async fn server_trims_space_padded_charge_point_id() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;

        connect_async(ocpp_request(addr, "%20CP001%20"))
            .await
            .expect("a space-padded id should connect as its trimmed form");

        let start = tokio::time::Instant::now();
        while !server.is_cp_connected("CP001") && start.elapsed() < Duration::from_millis(500) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            server.is_cp_connected("CP001"),
            "the CP must be registered under the trimmed id 'CP001'"
        );
        assert_eq!(
            server.connected_cp_ids(),
            vec!["CP001".to_string()],
            "the routing key must be the trimmed id, not the padded one"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_round_trip_returns_callresult() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let (mut ws, _) = connect_async(ocpp_request(addr, "CP002")).await.unwrap();

        let call = Message::call("Heartbeat".to_string(), serde_json::json!({})).unwrap();
        let call_id = call.unique_id().to_string();
        ws.send(WsMsg::Text(serde_json::to_string(&call).unwrap()))
            .await
            .unwrap();

        let frame = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting for response")
            .expect("stream ended")
            .expect("WS error");

        if let WsMsg::Text(text) = frame {
            let msg: Message = serde_json::from_str(&text).unwrap();
            assert!(
                matches!(&msg, Message::CallResult(r) if r.unique_id == call_id),
                "expected CALLRESULT with unique_id={}, got {:?}",
                call_id,
                msg
            );
        } else {
            panic!("expected a text frame, got {:?}", frame);
        }

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_unknown_action_returns_callerror_not_supported() {
        let (mut server, addr) = start_server(Arc::new(RejectHandler)).await;
        let (mut ws, _) = connect_async(ocpp_request(addr, "CP003")).await.unwrap();

        let call = Message::call("UnknownAction".to_string(), serde_json::json!({})).unwrap();
        let call_id = call.unique_id().to_string();
        ws.send(WsMsg::Text(serde_json::to_string(&call).unwrap()))
            .await
            .unwrap();

        let frame = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting for CALLERROR")
            .expect("stream ended")
            .expect("WS error");

        // Parse directly as CallErrorMessage: the untagged Message enum is ambiguous for CALLERROR
        // (CallMessage accepts any MessageType including CallError), so we decode the specific type.
        if let WsMsg::Text(text) = frame {
            let err: ocpp_types::CallErrorMessage =
                serde_json::from_str(&text).unwrap_or_else(|e| {
                    panic!("expected CallErrorMessage, got parse error: {e}\nraw: {text}")
                });
            assert_eq!(err.unique_id, call_id);
            assert_eq!(err.error_code, CallErrorCode::NotSupported);
        } else {
            panic!("expected a text frame, got {:?}", frame);
        }

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_removes_connection_on_close() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let (ws, _) = connect_async(ocpp_request(addr, "CP004")).await.unwrap();

        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(server.connection_count(), 1);

        // Dropping the stream triggers a TCP close that the server detects.
        drop(ws);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            server.connection_count(),
            0,
            "map should be empty after close"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_cleanup_idle_connections() {
        let config = TransportConfig {
            keep_alive_interval: std::time::Duration::from_millis(1),
            ..Default::default()
        };
        let (server, _rx) = OcppServer::new(config, Arc::new(EchoHandler));

        let mut stale = ConnectionInfo::new("stale".to_string(), "csms".to_string());
        stale.last_activity = chrono::Utc::now() - chrono::Duration::seconds(10);
        server.connections.insert(stale.id, stale);

        let removed = server.cleanup_idle_connections().await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(server.connection_count(), 0);
    }

    #[test]
    fn connection_manager_stores_id() {
        let info = ConnectionInfo::new("192.168.1.1:9000".to_string(), "0.0.0.0:8080".to_string());
        let mgr = ConnectionManager::new(info.clone());
        assert_eq!(mgr.connection_id, info.id);
    }

    // ── CSMS-initiated CALL tests (Issue #30) ────────────────────────────────

    use ocpp_messages::v16j::RemoteStartTransactionRequest;
    use ocpp_types::v16j::RemoteStartStopStatus;

    fn remote_start(tag: &str) -> RemoteStartTransactionRequest {
        RemoteStartTransactionRequest {
            connector_id: Some(1),
            id_tag: tag.to_string(),
            charging_profile: None,
        }
    }

    #[tokio::test]
    async fn server_can_send_remote_start_to_connected_cp() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_RS").await;

        // CP side: answer the RemoteStartTransaction CALL with Accepted.
        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "RemoteStartTransaction");
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted" }),
            )))
            .await
            .unwrap();
            cp // keep the socket alive until the call resolves
        });

        let resp = server
            .call("CP_RS", remote_start("TAG_001"))
            .await
            .expect("RemoteStartTransaction resolves");
        assert_eq!(resp.status, RemoteStartStopStatus::Accepted);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_reset_helper_sends_reset_and_maps_status() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_RESET").await;

        // CP side: answer the Reset CALL with Accepted.
        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "Reset");
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted" }),
            )))
            .await
            .unwrap();
            cp // keep the socket alive until the call resolves
        });

        let status = server
            .reset("CP_RESET", ResetType::Hard)
            .await
            .expect("Reset resolves");
        assert_eq!(status, ResetStatus::Accepted);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn remote_start_transaction_helper_sends_action_and_maps_status() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_HELPER_RS").await;

        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "RemoteStartTransaction");
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted" }),
            )))
            .await
            .unwrap();
            cp
        });

        let status = server
            .remote_start_transaction("CP_HELPER_RS", "TAG_001", Some(1))
            .await
            .expect("remote_start_transaction resolves");
        assert_eq!(status, RemoteStartStopStatus::Accepted);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn remote_stop_transaction_helper_sends_action_and_maps_status() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_HELPER_RST").await;

        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "RemoteStopTransaction");
            // A CP that doesn't know the transaction id replies Rejected.
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Rejected" }),
            )))
            .await
            .unwrap();
            cp
        });

        let status = server
            .remote_stop_transaction("CP_HELPER_RST", 999)
            .await
            .expect("remote_stop_transaction resolves");
        assert_eq!(status, RemoteStartStopStatus::Rejected);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn get_configuration_helper_sends_action_and_returns_full_response() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_HELPER_GC").await;

        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "GetConfiguration");
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({
                    "configurationKey": [
                        { "key": "HeartbeatInterval", "readonly": false, "value": "86400" }
                    ],
                    "unknownKey": ["Bogus"]
                }),
            )))
            .await
            .unwrap();
            cp
        });

        let resp = server
            .get_configuration(
                "CP_HELPER_GC",
                Some(vec!["HeartbeatInterval".to_string(), "Bogus".to_string()]),
            )
            .await
            .expect("get_configuration resolves");
        let keys = resp.configuration_keys.expect("configurationKey present");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "HeartbeatInterval");
        assert_eq!(keys[0].value.as_deref(), Some("86400"));
        assert_eq!(
            resp.unknown_keys.as_deref(),
            Some(&["Bogus".to_string()][..])
        );

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn change_configuration_helper_sends_action_and_maps_status() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_HELPER_CC").await;

        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "ChangeConfiguration");
            // A read-only key is rejected by the CP.
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Rejected" }),
            )))
            .await
            .unwrap();
            cp
        });

        let status = server
            .change_configuration("CP_HELPER_CC", "NumberOfConnectors", "9")
            .await
            .expect("change_configuration resolves");
        assert_eq!(status, ConfigurationStatus::Rejected);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn trigger_message_helper_sends_action_and_maps_status() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_HELPER_TRG").await;

        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "TriggerMessage");
            // A CP that doesn't support the requested message replies NotImplemented.
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "NotImplemented" }),
            )))
            .await
            .unwrap();
            cp
        });

        let status = server
            .trigger_message("CP_HELPER_TRG", MessageTrigger::MeterValues, Some(1))
            .await
            .expect("trigger_message resolves");
        assert_eq!(status, TriggerMessageStatus::NotImplemented);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn clear_cache_helper_sends_action_and_maps_status() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_HELPER_CLR").await;

        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp).await;
            assert_eq!(action, "ClearCache");
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted" }),
            )))
            .await
            .unwrap();
            cp
        });

        let status = server
            .clear_cache("CP_HELPER_CLR")
            .await
            .expect("clear_cache resolves");
        assert_eq!(status, ClearCacheStatus::Accepted);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn trigger_message_helper_errors_when_cp_absent() {
        let (mut server, _addr) = start_server(Arc::new(EchoHandler)).await;

        let err = server
            .trigger_message("GHOST", MessageTrigger::Heartbeat, None)
            .await
            .expect_err("unknown CP must error");
        assert!(
            matches!(err, OcppError::CpNotConnected { ref cp_id } if cp_id == "GHOST"),
            "expected CpNotConnected, got {err:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn clear_cache_helper_errors_when_cp_absent() {
        let (mut server, _addr) = start_server(Arc::new(EchoHandler)).await;

        let err = server
            .clear_cache("GHOST")
            .await
            .expect_err("unknown CP must error");
        assert!(
            matches!(err, OcppError::CpNotConnected { ref cp_id } if cp_id == "GHOST"),
            "expected CpNotConnected, got {err:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn get_configuration_helper_errors_when_cp_absent() {
        let (mut server, _addr) = start_server(Arc::new(EchoHandler)).await;

        let err = server
            .get_configuration("GHOST", None)
            .await
            .expect_err("unknown CP must error");
        assert!(
            matches!(err, OcppError::CpNotConnected { ref cp_id } if cp_id == "GHOST"),
            "expected CpNotConnected, got {err:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_reset_to_unknown_cp_returns_not_connected() {
        let (mut server, _addr) = start_server(Arc::new(EchoHandler)).await;

        let err = server
            .reset("GHOST", ResetType::Soft)
            .await
            .expect_err("unknown CP must error");
        assert!(
            matches!(err, OcppError::CpNotConnected { ref cp_id } if cp_id == "GHOST"),
            "expected CpNotConnected, got {err:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn remote_start_transaction_helper_errors_when_cp_absent() {
        let (mut server, _addr) = start_server(Arc::new(EchoHandler)).await;

        let err = server
            .remote_start_transaction("GHOST", "TAG", None)
            .await
            .expect_err("unknown CP must error");
        assert!(
            matches!(err, OcppError::CpNotConnected { ref cp_id } if cp_id == "GHOST"),
            "expected CpNotConnected, got {err:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_to_unknown_cp_returns_not_connected() {
        let (mut server, _addr) = start_server(Arc::new(EchoHandler)).await;

        let err = server
            .call("GHOST", remote_start("TAG"))
            .await
            .expect_err("unknown CP must error");
        assert!(
            matches!(err, OcppError::CpNotConnected { ref cp_id } if cp_id == "GHOST"),
            "expected CpNotConnected, got {err:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_times_out_when_cp_does_not_respond() {
        let config = TransportConfig {
            call_timeout: Duration::from_millis(150),
            ..Default::default()
        };
        let (mut server, _rx) = OcppServer::new(config, Arc::new(EchoHandler));
        server.start("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();

        // CP connects but never answers the CALL.
        let _cp = connect_cp(&server, addr, "CP_SILENT").await;

        let err = server
            .call("CP_SILENT", remote_start("TAG"))
            .await
            .expect_err("silent CP must time out");
        assert!(
            matches!(err, OcppError::Timeout { .. }),
            "expected Timeout, got {err:?}"
        );

        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_surfaces_callerror() {
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_ERR").await;

        let responder = tokio::spawn(async move {
            let (_action, unique_id) = read_call(&mut cp).await;
            let frame =
                serde_json::to_string(&Message::CallError(ocpp_types::CallErrorMessage::new(
                    unique_id,
                    CallErrorCode::InternalError,
                    "boom".to_string(),
                    None,
                )))
                .unwrap();
            cp.send(WsMsg::Text(frame)).await.unwrap();
            cp
        });

        let err = server
            .call("CP_ERR", remote_start("TAG"))
            .await
            .expect_err("CALLERROR must surface as an error");
        assert!(
            matches!(err, OcppError::CallError { ref code, .. } if *code == CallErrorCode::InternalError),
            "expected CallError(InternalError), got {err:?}"
        );

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    /// Like [`start_server`] but attaches an OCPP 1.6J [`SchemaValidator`] to
    /// the CSMS-initiated `call()` path, so outbound CALLs and inbound
    /// CALLRESULTs are schema-validated.
    async fn start_server_validated(handler: Arc<dyn MessageHandler>) -> (OcppServer, SocketAddr) {
        let (server, _rx) = OcppServer::new(TransportConfig::default(), handler);
        let mut server = server.with_validator(Arc::new(SchemaValidator::v16j()));
        server.start("127.0.0.1:0").await.expect("server start");
        let addr = server.local_addr().unwrap();
        (server, addr)
    }

    #[tokio::test]
    async fn server_call_validates_outbound_rejects_overlong_idtag() {
        // A 21-char idTag exceeds RemoteStartTransaction's CiString20 maxLength:
        // serde builds it fine, but the schema rejects it. With a validator
        // attached, `call()` must reject it as a SchemaViolation *before* the
        // connection check — so even a call to an unconnected CP fails with
        // SchemaViolation, not CpNotConnected — matching the reference's
        // validate-before-`_send` ordering (`charge_point.py::call`).
        let (mut server, _addr) = start_server_validated(Arc::new(EchoHandler)).await;
        let err = server
            .call("NEVER_CONNECTED", remote_start(&"x".repeat(21)))
            .await
            .expect_err("overlong idTag must be rejected before send");
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "expected SchemaViolation (not CpNotConnected), got {err:?}"
        );
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_validates_callresult_rejects_additional_property() {
        // Primary trust-boundary regression: the CP replies with a valid
        // `status` plus an extra property the schema forbids
        // (`additionalProperties: false`). serde ignores unknown fields and
        // would accept this; the validator must reject it as a SchemaViolation,
        // mirroring `_handle_call_result` / `validate_payload(response)`.
        let (mut server, addr) = start_server_validated(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_VAL").await;

        let responder = tokio::spawn(async move {
            let (_action, unique_id) = read_call(&mut cp).await;
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted", "extra": true }),
            )))
            .await
            .unwrap();
            cp
        });

        let err = server
            .call("CP_VAL", remote_start("TAG"))
            .await
            .expect_err("schema-invalid CALLRESULT must be rejected");
        assert!(
            matches!(err, OcppError::SchemaViolation { .. }),
            "expected SchemaViolation for additionalProperties violation, got {err:?}"
        );

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_without_validator_accepts_unvalidated_callresult() {
        // Regression pinning the opt-in gate: a default server (no validator)
        // accepts the SAME additionalProperties-violating CALLRESULT, because
        // serde ignores the unknown `extra` field. Attaching a validator is
        // what adds the check; the default preserves prior behavior (the
        // reference's `skip_schema_validation=True`).
        let (mut server, addr) = start_server(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_NOVAL").await;

        let responder = tokio::spawn(async move {
            let (_action, unique_id) = read_call(&mut cp).await;
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted", "extra": true }),
            )))
            .await
            .unwrap();
            cp
        });

        let resp = server
            .call("CP_NOVAL", remote_start("TAG"))
            .await
            .expect("no validator ⇒ CALLRESULT accepted");
        assert_eq!(resp.status, RemoteStartStopStatus::Accepted);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_call_with_validator_accepts_valid_callresult() {
        // Happy path: with a validator attached, a conformant CALLRESULT still
        // round-trips to Ok — the validator doesn't break valid traffic.
        let (mut server, addr) = start_server_validated(Arc::new(EchoHandler)).await;
        let mut cp = connect_cp(&server, addr, "CP_OK").await;

        let responder = tokio::spawn(async move {
            let (_action, unique_id) = read_call(&mut cp).await;
            cp.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted" }),
            )))
            .await
            .unwrap();
            cp
        });

        let resp = server
            .call("CP_OK", remote_start("TAG"))
            .await
            .expect("valid CALLRESULT passes validation");
        assert_eq!(resp.status, RemoteStartStopStatus::Accepted);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn server_concurrent_calls_to_multiple_cps_correct() {
        let (server, addr) = start_server(Arc::new(EchoHandler)).await;

        // Five CPs; odd-numbered ones answer Accepted, even ones Rejected, so a
        // cross-CP correlation bug would surface as a wrong status.
        let mut responders = Vec::new();
        for i in 0..5u32 {
            let cp_id = format!("CP_{i}");
            let mut cp = connect_cp(&server, addr, &cp_id).await;
            let accepted = i % 2 == 1;
            responders.push(tokio::spawn(async move {
                let (_action, unique_id) = read_call(&mut cp).await;
                let status = if accepted { "Accepted" } else { "Rejected" };
                cp.send(WsMsg::Text(call_result_frame(
                    &unique_id,
                    serde_json::json!({ "status": status }),
                )))
                .await
                .unwrap();
                cp
            }));
        }

        let server = Arc::new(server);
        let calls = (0..5u32).map(|i| {
            let server = Arc::clone(&server);
            async move {
                let resp = server
                    .call(&format!("CP_{i}"), remote_start(&format!("TAG_{i}")))
                    .await
                    .expect("each call resolves");
                (i, resp.status)
            }
        });
        let results = futures_util::future::join_all(calls).await;

        for (i, status) in results {
            let expected = if i % 2 == 1 {
                RemoteStartStopStatus::Accepted
            } else {
                RemoteStartStopStatus::Rejected
            };
            assert_eq!(status, expected, "CP_{i} got the wrong response");
        }

        for r in responders {
            r.await.unwrap();
        }
        Arc::try_unwrap(server)
            .unwrap_or_else(|_| panic!("server still shared"))
            .stop()
            .await
            .unwrap();
    }

    /// Regression for issue #50: a charge point that reconnects while its
    /// previous session is still tearing down must stay routable. The stale
    /// session's teardown must not evict the *new* session's routing handle.
    ///
    /// The race window is reproduced deterministically. When session B connects
    /// under the same `cp_id`, its `cp_handles.insert` overwrites A's handle —
    /// which drops A's outbound sender, so A's session loop immediately observes
    /// the closed channel and runs teardown. That teardown therefore executes
    /// *after* B has taken ownership of the routing entry: exactly the race.
    /// We use the `Disconnected` event (emitted at the end of teardown, after the
    /// handle removal) as a happens-after barrier, then assert B is still
    /// routable. With the pre-fix unconditional `remove(cp_id)`, A's teardown
    /// evicts B's live handle and `server.call` wrongly returns `CpNotConnected`.
    #[tokio::test]
    async fn reconnect_teardown_does_not_evict_new_session_handle() {
        let (mut server, mut rx) =
            OcppServer::new(TransportConfig::default(), Arc::new(EchoHandler));
        server.start("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();

        // Session A connects first and registers the routing handle for CP_RECON.
        let _cp_a = connect_cp(&server, addr, "CP_RECON").await;

        // Session B reconnects under the *same* cp_id. Its insert overwrites A's
        // handle, dropping A's outbound sender and tripping A's teardown.
        let (mut cp_b, _) = connect_async(ocpp_request(addr, "CP_RECON"))
            .await
            .expect("B connects");

        // Wait for A's teardown to finish, signalled by its Disconnected event.
        // (Connected events for A and B are drained past on the way.)
        let mut saw_disconnect = false;
        let start = tokio::time::Instant::now();
        while !saw_disconnect && start.elapsed() < Duration::from_secs(2) {
            if let Ok(Some(ev)) = timeout(Duration::from_millis(200), rx.recv()).await {
                saw_disconnect = matches!(ev, TransportEvent::Disconnected { .. });
            }
        }
        assert!(
            saw_disconnect,
            "A's session should tear down once B overwrites its routing handle"
        );

        // B must still be routable: its handle outlived A's stale teardown.
        assert!(
            server.is_cp_connected("CP_RECON"),
            "B's routing handle must survive A's teardown"
        );

        // The decisive check: a CSMS-initiated CALL routes to the live B session
        // and resolves, rather than failing with CpNotConnected.
        let responder = tokio::spawn(async move {
            let (action, unique_id) = read_call(&mut cp_b).await;
            assert_eq!(action, "RemoteStartTransaction");
            cp_b.send(WsMsg::Text(call_result_frame(
                &unique_id,
                serde_json::json!({ "status": "Accepted" }),
            )))
            .await
            .unwrap();
            cp_b
        });

        let resp = server
            .call("CP_RECON", remote_start("TAG_RC"))
            .await
            .expect("call must route to the live (B) session, not CpNotConnected");
        assert_eq!(resp.status, RemoteStartStopStatus::Accepted);

        responder.await.unwrap();
        server.stop().await.unwrap();
    }

    // ── StatusNotification observability event tests (Issue #47) ─────────────

    use ocpp_types::v16j::ChargePointStatus;

    /// Drain every event currently queued on `rx`, stopping after a short period
    /// of silence. By the time the CP has received its CALLRESULT the server has
    /// already emitted any event for that frame (emission happens-before the
    /// reply is sent), so this captures them deterministically.
    async fn drain_events(rx: &mut mpsc::UnboundedReceiver<TransportEvent>) -> Vec<TransportEvent> {
        let mut out = Vec::new();
        while let Ok(Some(ev)) = timeout(Duration::from_millis(200), rx.recv()).await {
            out.push(ev);
        }
        out
    }

    fn status_notification_frame(connector_id: u32, status: &str) -> String {
        let call = Message::call(
            "StatusNotification".to_string(),
            serde_json::json!({
                "connectorId": connector_id,
                "errorCode": "NoError",
                "status": status,
            }),
        )
        .unwrap();
        serde_json::to_string(&call).unwrap()
    }

    /// A CP-sent StatusNotification produces exactly one
    /// `TransportEvent::StatusNotification` carrying the right `cp_id`,
    /// `connector_id` and `status`, while the CALLRESULT stays empty `{}`.
    #[tokio::test]
    async fn status_notification_emits_event_with_cp_id() {
        let (mut server, mut rx) =
            OcppServer::new(TransportConfig::default(), Arc::new(EchoHandler));
        server.start("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();

        let (mut ws, _) = connect_async(ocpp_request(addr, "CP_STATUS"))
            .await
            .unwrap();
        let call_text = status_notification_frame(1, "Charging");
        let call_id = {
            let v: serde_json::Value = serde_json::from_str(&call_text).unwrap();
            v.get("1").and_then(|i| i.as_str()).unwrap().to_string()
        };
        ws.send(WsMsg::Text(call_text)).await.unwrap();

        // The reply must be the empty StatusNotification CALLRESULT (no regression).
        let frame = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting for CALLRESULT")
            .expect("stream ended")
            .expect("WS error");
        let WsMsg::Text(text) = frame else {
            panic!("expected text frame, got {frame:?}");
        };
        let resp: Message = serde_json::from_str(&text).unwrap();
        match resp {
            Message::CallResult(r) => {
                assert_eq!(r.unique_id, call_id);
                assert_eq!(
                    r.payload,
                    serde_json::json!({}),
                    "CALLRESULT must stay empty"
                );
            }
            other => panic!("expected CALLRESULT, got {other:?}"),
        }

        let events = drain_events(&mut rx).await;
        let statuses: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                TransportEvent::StatusNotification {
                    cp_id,
                    connector_id,
                    status,
                } => Some((cp_id.clone(), *connector_id, *status)),
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses.len(),
            1,
            "expected exactly one StatusNotification event, got {statuses:?}"
        );
        assert_eq!(
            statuses[0],
            ("CP_STATUS".to_string(), 1, ChargePointStatus::Charging)
        );

        server.stop().await.unwrap();
    }

    /// A StatusNotification whose payload doesn't deserialize (unknown status
    /// enum) must NOT emit a connector-state event — the deserialization guard
    /// keeps the channel free of half-parsed transitions.
    #[tokio::test]
    async fn malformed_status_notification_emits_no_event() {
        let (mut server, mut rx) =
            OcppServer::new(TransportConfig::default(), Arc::new(EchoHandler));
        server.start("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();

        let (mut ws, _) = connect_async(ocpp_request(addr, "CP_BAD")).await.unwrap();
        // EchoHandler does no schema validation, so dispatch succeeds; only the
        // typed decode guards event emission. "Bogus" is not a ChargePointStatus.
        ws.send(WsMsg::Text(status_notification_frame(1, "Bogus")))
            .await
            .unwrap();

        // Drain the reply so the inbound frame is fully processed.
        let _ = timeout(Duration::from_secs(2), ws.next()).await;

        let events = drain_events(&mut rx).await;
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, TransportEvent::StatusNotification { .. })),
            "no StatusNotification event expected for an undecodable payload, got {events:?}"
        );

        server.stop().await.unwrap();
    }
}
