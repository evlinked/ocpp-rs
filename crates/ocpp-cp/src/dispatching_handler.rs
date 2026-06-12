//! Bridge between `ActionDispatcher` and the transport `MessageHandler` trait.
//!
//! Ports `_handle_call()` from [`ocpp/charge_point.py`]:
//! incoming CALL → `ActionDispatcher::dispatch()` → CALLRESULT or CALLERROR.

use std::sync::Arc;

use async_trait::async_trait;
use ocpp_messages::ActionDispatcher;
use ocpp_transport::{MessageHandler as TransportMessageHandler, TransportEvent};
use ocpp_types::{CallErrorCode, CallErrorMessage, CallResultMessage, Message, OcppError};
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::error::ChargePointError;
use crate::ChargePointEvent;

/// Dispatches incoming OCPP CALL messages through an `ActionDispatcher` and
/// forwards transport events to the `ChargePoint` event channel.
pub(crate) struct DispatchingHandler {
    dispatcher: Arc<ActionDispatcher>,
    event_sender: mpsc::UnboundedSender<ChargePointEvent>,
}

impl DispatchingHandler {
    pub fn new(
        dispatcher: Arc<ActionDispatcher>,
        event_sender: mpsc::UnboundedSender<ChargePointEvent>,
    ) -> Self {
        Self {
            dispatcher,
            event_sender,
        }
    }
}

#[async_trait]
impl TransportMessageHandler for DispatchingHandler {
    async fn handle_message(&self, message: Message) -> ocpp_types::OcppResult<Option<Message>> {
        let Message::Call(call) = message else {
            // CALLRESULT and CALLERROR are resolved by PendingCallMap in the recv loop.
            return Ok(None);
        };

        let unique_id = call.unique_id.clone();

        let response = match self.dispatcher.dispatch(&call).await {
            Ok(payload) => {
                let result = CallResultMessage::new(unique_id, payload).map_err(|e| {
                    OcppError::Internal {
                        message: e.to_string(),
                    }
                })?;
                Message::CallResult(result)
            }
            Err(OcppError::NotSupported { feature }) => {
                warn!(action = %feature, "no handler registered for action");
                Message::CallError(CallErrorMessage::new(
                    unique_id,
                    CallErrorCode::NotSupported,
                    format!("Action '{}' is not supported", feature),
                    None,
                ))
            }
            Err(e) => {
                error!(error = %e, "handler returned error");
                Message::CallError(CallErrorMessage::new(
                    unique_id,
                    CallErrorCode::InternalError,
                    e.to_string(),
                    None,
                ))
            }
        };

        Ok(Some(response))
    }

    async fn handle_event(&self, event: TransportEvent) {
        match event {
            TransportEvent::Disconnected { reason, .. } => {
                let _ = self
                    .event_sender
                    .send(ChargePointEvent::Disconnected { reason });
            }
            TransportEvent::Error { error, .. } => {
                let _ = self.event_sender.send(ChargePointEvent::Error {
                    error: ChargePointError::TransportError(error.to_string()),
                });
            }
            _ => {}
        }
    }
}
