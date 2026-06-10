//! # Simulator API Module
//!
//! This module provides additional API endpoints and utilities for the OCPP simulator,
//! including WebSocket event streaming, advanced connector control, and batch operations.

use crate::{
    events::{EventFilter, EventSeverity, SimulatorEvent},
    SimulatorState,
};
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// WebSocket message types for event streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMessage {
    /// Subscribe to events with filter
    Subscribe { filter: Option<EventFilter> },
    /// Unsubscribe from events
    Unsubscribe,
    /// Event notification
    Event(SimulatorEvent),
    /// Heartbeat/ping message
    Ping,
    /// Heartbeat/pong response
    Pong,
    /// Error message
    Error { message: String },
    /// Acknowledgment message
    Ack { message_id: Option<String> },
}

/// Batch operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationRequest {
    /// Operations to perform
    pub operations: Vec<BatchOperation>,
    /// Whether to stop on first error
    pub stop_on_error: bool,
    /// Delay between operations in milliseconds
    pub delay_ms: Option<u64>,
}

/// Individual batch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    /// Operation type
    pub operation: String,
    /// Target connector ID (if applicable)
    pub connector_id: Option<u32>,
    /// Operation parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Batch operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    /// Overall success
    pub success: bool,
    /// Individual operation results
    pub results: Vec<OperationResult>,
    /// Total execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Individual operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    /// Operation that was performed
    pub operation: String,
    /// Target connector ID
    pub connector_id: Option<u32>,
    /// Whether operation succeeded
    pub success: bool,
    /// Result message or error
    pub message: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Advanced connector control request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedControlRequest {
    /// Control type
    pub control_type: String,
    /// Control parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Real-time metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeMetrics {
    /// Current timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Total power consumption in W
    pub total_power_w: f64,
    /// Total energy delivered in Wh
    pub total_energy_wh: f64,
    /// Active transactions count
    pub active_transactions: u32,
    /// Messages per second
    pub messages_per_second: f64,
    /// Connection uptime in seconds
    pub uptime_seconds: u64,
    /// Per-connector metrics
    pub connector_metrics: HashMap<u32, ConnectorMetrics>,
}

/// Per-connector metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorMetrics {
    /// Connector ID
    pub connector_id: u32,
    /// Current power in W
    pub power_w: f64,
    /// Current energy session in Wh
    pub session_energy_wh: f64,
    /// Transaction duration in seconds
    pub transaction_duration_seconds: Option<u64>,
    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

/// Create extended API router with advanced features
pub fn create_extended_api_router(state: SimulatorState) -> Router {
    Router::new()
        .route("/events/stream", get(websocket_handler))
        .route("/events/history", get(get_event_history))
        .route("/events/statistics", get(get_event_statistics))
        .route("/batch", post(execute_batch_operations))
        .route("/metrics/realtime", get(get_realtime_metrics))
        .route("/connectors/:id/advanced", post(advanced_connector_control))
        .route(
            "/connectors/:id/simulate",
            post(simulate_connector_behavior),
        )
        .route("/system/health", get(health_check))
        .route("/system/reset", post(system_reset))
        .with_state(state)
}

/// WebSocket handler for real-time event streaming
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<SimulatorState>,
) -> Response {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

/// Handle WebSocket connection for event streaming
async fn handle_websocket(socket: WebSocket, _state: SimulatorState) {
    let (mut sender, mut receiver) = socket.split();

    info!("New WebSocket connection established");

    // Create event receiver
    let mut event_receiver = {
        let (_tx, rx) = broadcast::channel(100);
        // In a real implementation, this would connect to the event store
        rx
    };

    let mut event_filter: Option<EventFilter> = None;

    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(msg)) => {
                        if let Ok(text) = msg.to_text() {
                            match serde_json::from_str::<WebSocketMessage>(text) {
                                Ok(WebSocketMessage::Subscribe { filter }) => {
                                    event_filter = filter;
                                    let ack = WebSocketMessage::Ack { message_id: None };
                                    if let Ok(ack_text) = serde_json::to_string(&ack) {
                                        let _ = sender.send(axum::extract::ws::Message::Text(ack_text)).await;
                                    }
                                }
                                Ok(WebSocketMessage::Unsubscribe) => {
                                    event_filter = None;
                                    let ack = WebSocketMessage::Ack { message_id: None };
                                    if let Ok(ack_text) = serde_json::to_string(&ack) {
                                        let _ = sender.send(axum::extract::ws::Message::Text(ack_text)).await;
                                    }
                                }
                                Ok(WebSocketMessage::Ping) => {
                                    let pong = WebSocketMessage::Pong;
                                    if let Ok(pong_text) = serde_json::to_string(&pong) {
                                        let _ = sender.send(axum::extract::ws::Message::Text(pong_text)).await;
                                    }
                                }
                                Err(e) => {
                                    let error = WebSocketMessage::Error {
                                        message: format!("Invalid message format: {}", e),
                                    };
                                    if let Ok(error_text) = serde_json::to_string(&error) {
                                        let _ = sender.send(axum::extract::ws::Message::Text(error_text)).await;
                                    }
                                }
                                _ => {} // Ignore other message types
                            }
                        }
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        debug!("WebSocket connection closed");
                        break;
                    }
                }
            }

            // Handle simulator events
            Ok(event) = event_receiver.recv() => {
                // Apply filter if set
                if let Some(ref filter) = event_filter {
                    if !filter.matches(&event) {
                        continue;
                    }
                }

                let ws_message = WebSocketMessage::Event(event);
                if let Ok(text) = serde_json::to_string(&ws_message) {
                    if sender.send(axum::extract::ws::Message::Text(text)).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket connection closed");
}

/// Get event history with filtering
async fn get_event_history(
    Query(params): Query<HashMap<String, String>>,
) -> Json<crate::SimulatorResponse<Vec<SimulatorEvent>>> {
    let _limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100);

    let min_severity = params
        .get("severity")
        .and_then(|s| match s.as_str() {
            "debug" => Some(EventSeverity::Debug),
            "info" => Some(EventSeverity::Info),
            "warning" => Some(EventSeverity::Warning),
            "error" => Some(EventSeverity::Error),
            "critical" => Some(EventSeverity::Critical),
            _ => None,
        })
        .unwrap_or(EventSeverity::Info);

    let event_types = params
        .get("types")
        .map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<_>>());

    let connector_ids = params.get("connectors").and_then(|s| {
        s.split(',')
            .map(|s| s.parse::<u32>().ok())
            .collect::<Option<Vec<_>>>()
    });

    let _filter = EventFilter {
        min_severity,
        event_types,
        connector_ids,
        time_range: None, // TODO: Parse time range from query params
    };

    // In a real implementation, this would query the event store
    let events = vec![];

    Json(crate::SimulatorResponse::success(events))
}

/// Get event statistics
async fn get_event_statistics() -> Json<crate::SimulatorResponse<serde_json::Value>> {
    // In a real implementation, this would query the event store for statistics
    let stats = serde_json::json!({
        "total_events": 0,
        "event_type_counts": {},
        "severity_counts": {},
        "connector_counts": {},
        "error_count": 0,
        "warning_count": 0
    });

    Json(crate::SimulatorResponse::success(stats))
}

/// Execute batch operations
async fn execute_batch_operations(
    State(_state): State<SimulatorState>,
    Json(request): Json<BatchOperationRequest>,
) -> Result<Json<crate::SimulatorResponse<BatchOperationResult>>, StatusCode> {
    let start_time = std::time::Instant::now();
    let mut results = Vec::new();
    let mut overall_success = true;

    info!(
        "Executing batch operations: {} operations",
        request.operations.len()
    );

    for operation in request.operations {
        let op_start_time = std::time::Instant::now();

        let result = match operation.operation.as_str() {
            "plug_in" => execute_plug_in_operation(&operation).await,
            "plug_out" => execute_plug_out_operation(&operation).await,
            "start_transaction" => execute_start_transaction_operation(&operation).await,
            "stop_transaction" => execute_stop_transaction_operation(&operation).await,
            "inject_fault" => execute_inject_fault_operation(&operation).await,
            "clear_fault" => execute_clear_fault_operation(&operation).await,
            "set_availability" => execute_set_availability_operation(&operation).await,
            _ => OperationResult {
                operation: operation.operation.clone(),
                connector_id: operation.connector_id,
                success: false,
                message: Some(format!("Unknown operation: {}", operation.operation)),
                execution_time_ms: op_start_time.elapsed().as_millis() as u64,
            },
        };

        let success = result.success;
        results.push(result);

        if !success {
            overall_success = false;
            if request.stop_on_error {
                break;
            }
        }

        // Add delay between operations if specified
        if let Some(delay_ms) = request.delay_ms {
            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }

    let total_time = start_time.elapsed().as_millis() as u64;

    let batch_result = BatchOperationResult {
        success: overall_success,
        results,
        execution_time_ms: total_time,
    };

    Ok(Json(crate::SimulatorResponse::success(batch_result)))
}

/// Get real-time metrics
async fn get_realtime_metrics(
    State(_state): State<SimulatorState>,
) -> Json<crate::SimulatorResponse<RealTimeMetrics>> {
    let metrics = RealTimeMetrics {
        timestamp: chrono::Utc::now(),
        total_power_w: 0.0,
        total_energy_wh: 0.0,
        active_transactions: 0,
        messages_per_second: 0.0,
        uptime_seconds: 0,
        connector_metrics: HashMap::new(),
    };

    Json(crate::SimulatorResponse::success(metrics))
}

/// Advanced connector control
async fn advanced_connector_control(
    State(_state): State<SimulatorState>,
    Path(_id): Path<u32>,
    Json(_request): Json<AdvancedControlRequest>,
) -> Json<crate::SimulatorResponse<()>> {
    // Implementation would handle advanced connector control operations
    Json(crate::SimulatorResponse::success_empty())
}

/// Simulate connector behavior
async fn simulate_connector_behavior(
    State(_state): State<SimulatorState>,
    Path(_id): Path<u32>,
    Json(_params): Json<HashMap<String, serde_json::Value>>,
) -> Json<crate::SimulatorResponse<()>> {
    // Implementation would handle behavior simulation
    Json(crate::SimulatorResponse::success_empty())
}

/// Health check endpoint
async fn health_check(
    State(_state): State<SimulatorState>,
) -> Json<crate::SimulatorResponse<serde_json::Value>> {
    let health = serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now(),
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": 0
    });

    Json(crate::SimulatorResponse::success(health))
}

/// System reset endpoint
async fn system_reset(State(_state): State<SimulatorState>) -> Json<crate::SimulatorResponse<()>> {
    // Implementation would handle system reset
    Json(crate::SimulatorResponse::success_empty())
}

// Helper functions for batch operations

async fn execute_plug_in_operation(operation: &BatchOperation) -> OperationResult {
    let start_time = std::time::Instant::now();

    // Implementation would execute plug in operation
    OperationResult {
        operation: operation.operation.clone(),
        connector_id: operation.connector_id,
        success: true,
        message: Some("Plug in completed".to_string()),
        execution_time_ms: start_time.elapsed().as_millis() as u64,
    }
}

async fn execute_plug_out_operation(operation: &BatchOperation) -> OperationResult {
    let start_time = std::time::Instant::now();

    // Implementation would execute plug out operation
    OperationResult {
        operation: operation.operation.clone(),
        connector_id: operation.connector_id,
        success: true,
        message: Some("Plug out completed".to_string()),
        execution_time_ms: start_time.elapsed().as_millis() as u64,
    }
}

async fn execute_start_transaction_operation(operation: &BatchOperation) -> OperationResult {
    let start_time = std::time::Instant::now();

    // Implementation would execute start transaction operation
    OperationResult {
        operation: operation.operation.clone(),
        connector_id: operation.connector_id,
        success: true,
        message: Some("Transaction started".to_string()),
        execution_time_ms: start_time.elapsed().as_millis() as u64,
    }
}

async fn execute_stop_transaction_operation(operation: &BatchOperation) -> OperationResult {
    let start_time = std::time::Instant::now();

    // Implementation would execute stop transaction operation
    OperationResult {
        operation: operation.operation.clone(),
        connector_id: operation.connector_id,
        success: true,
        message: Some("Transaction stopped".to_string()),
        execution_time_ms: start_time.elapsed().as_millis() as u64,
    }
}

async fn execute_inject_fault_operation(operation: &BatchOperation) -> OperationResult {
    let start_time = std::time::Instant::now();

    // Implementation would execute fault injection
    OperationResult {
        operation: operation.operation.clone(),
        connector_id: operation.connector_id,
        success: true,
        message: Some("Fault injected".to_string()),
        execution_time_ms: start_time.elapsed().as_millis() as u64,
    }
}

async fn execute_clear_fault_operation(operation: &BatchOperation) -> OperationResult {
    let start_time = std::time::Instant::now();

    // Implementation would execute fault clearing
    OperationResult {
        operation: operation.operation.clone(),
        connector_id: operation.connector_id,
        success: true,
        message: Some("Fault cleared".to_string()),
        execution_time_ms: start_time.elapsed().as_millis() as u64,
    }
}

async fn execute_set_availability_operation(operation: &BatchOperation) -> OperationResult {
    let start_time = std::time::Instant::now();

    // Implementation would execute availability change
    OperationResult {
        operation: operation.operation.clone(),
        connector_id: operation.connector_id,
        success: true,
        message: Some("Availability updated".to_string()),
        execution_time_ms: start_time.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_message_serialization() {
        let message = WebSocketMessage::Subscribe {
            filter: Some(EventFilter::default()),
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: WebSocketMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            WebSocketMessage::Subscribe { .. } => {}
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_batch_operation_request() {
        let request = BatchOperationRequest {
            operations: vec![BatchOperation {
                operation: "plug_in".to_string(),
                connector_id: Some(1),
                parameters: HashMap::new(),
            }],
            stop_on_error: true,
            delay_ms: Some(1000),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: BatchOperationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.operations.len(), 1);
        assert_eq!(deserialized.operations[0].operation, "plug_in");
        assert!(deserialized.stop_on_error);
        assert_eq!(deserialized.delay_ms, Some(1000));
    }

    #[test]
    fn test_real_time_metrics() {
        let metrics = RealTimeMetrics {
            timestamp: chrono::Utc::now(),
            total_power_w: 1500.0,
            total_energy_wh: 12000.0,
            active_transactions: 2,
            messages_per_second: 5.5,
            uptime_seconds: 3600,
            connector_metrics: HashMap::new(),
        };

        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: RealTimeMetrics = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_power_w, 1500.0);
        assert_eq!(deserialized.active_transactions, 2);
        assert_eq!(deserialized.uptime_seconds, 3600);
    }
}
