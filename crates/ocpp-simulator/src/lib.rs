//! # OCPP Simulator
//!
//! This crate provides a comprehensive charge point simulator with full API
//! to control and simulate real-world charging scenarios. It supports:
//!
//! - Complete charge point lifecycle simulation
//! - Connector state management and transitions
//! - Transaction simulation with realistic scenarios
//! - Fault injection and error simulation
//! - Meter value generation with configurable patterns
//! - WebSocket connection to real Central Systems
//! - REST API for external control
//! - Real-time event streaming
//! - Scenario playbook execution

pub mod api;
pub mod config;
pub mod error;
pub mod events;
pub mod fault_injector;
pub mod meter_simulator;
pub mod scenario;
pub mod simulator;
pub mod websocket;

pub use config::SimulatorConfig;
pub use error::SimulatorError;
pub use events::{SimulatorEvent, SimulatorEventHandler};
pub use simulator::Simulator;

use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use ocpp_cp::{ChargePoint, ChargePointConfig};
use ocpp_types::v16j::{ChargePointErrorCode, ChargePointStatus};
use ocpp_types::ConnectorId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

/// Simulator statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorStats {
    /// Total runtime in seconds
    pub uptime_seconds: u64,
    /// Number of connectors
    pub connector_count: u32,
    /// Current connector states
    pub connector_states: HashMap<u32, ChargePointStatus>,
    /// Active transactions
    pub active_transactions: Vec<TransactionInfo>,
    /// Total transactions started
    pub total_transactions: u64,
    /// Total energy delivered in Wh
    pub total_energy_wh: u64,
    /// Connection status
    pub connected: bool,
    /// Last event timestamp
    pub last_event: Option<chrono::DateTime<chrono::Utc>>,
    /// Error count
    pub error_count: u64,
}

/// Transaction information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    /// Transaction ID
    pub transaction_id: i32,
    /// Connector ID
    pub connector_id: u32,
    /// ID tag
    pub id_tag: String,
    /// Start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Current energy consumed in Wh
    pub energy_wh: i32,
    /// Current duration in seconds
    pub duration_seconds: i64,
    /// Current power in W
    pub current_power_w: f32,
}

/// Connector status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorInfo {
    /// Connector ID
    pub connector_id: u32,
    /// Current status
    pub status: ChargePointStatus,
    /// Error code
    pub error_code: ChargePointErrorCode,
    /// Error info
    pub error_info: Option<String>,
    /// Is cable plugged
    pub cable_plugged: bool,
    /// Current transaction ID if any
    pub transaction_id: Option<i32>,
    /// Last meter reading
    pub last_meter_reading: MeterReading,
    /// Reserved for ID tag
    pub reserved_for: Option<String>,
}

/// Meter reading data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterReading {
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Energy in Wh
    pub energy_wh: f64,
    /// Power in W
    pub power_w: f64,
    /// Voltage in V
    pub voltage_v: f64,
    /// Current in A
    pub current_a: f64,
    /// Temperature in Celsius
    pub temperature_c: Option<f64>,
}

/// Control command for connector operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorCommand {
    /// Command type
    pub command: String,
    /// Parameters
    pub params: Option<serde_json::Value>,
}

/// Fault injection request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultRequest {
    /// Error code to inject
    pub error_code: ChargePointErrorCode,
    /// Additional info
    pub info: Option<String>,
    /// Duration in seconds (None for permanent)
    pub duration_seconds: Option<u64>,
}

/// Scenario execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioRequest {
    /// Scenario name
    pub scenario: String,
    /// Scenario parameters
    pub parameters: Option<HashMap<String, serde_json::Value>>,
}

/// Simulator response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorResponse<T = ()> {
    /// Success flag
    pub success: bool,
    /// Response data
    pub data: Option<T>,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl<T> SimulatorResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            timestamp: chrono::Utc::now(),
        }
    }
}

impl SimulatorResponse<()> {
    pub fn success_empty() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Main simulator state
#[derive(Clone)]
pub struct SimulatorState {
    /// Charge point instance
    pub charge_point: Arc<ChargePoint>,
    /// Simulator configuration
    pub config: Arc<SimulatorConfig>,
    /// Statistics
    pub stats: Arc<RwLock<SimulatorStats>>,
    /// Event sender
    pub event_sender: mpsc::UnboundedSender<SimulatorEvent>,
    /// Start time
    pub start_time: chrono::DateTime<chrono::Utc>,
}

/// Create REST API router
pub fn create_api_router(state: SimulatorState) -> Router {
    Router::new()
        .route("/", get(get_status))
        .route("/status", get(get_status))
        .route("/stats", get(get_stats))
        .route("/connectors", get(get_connectors))
        .route("/connectors/:id", get(get_connector))
        .route("/connectors/:id/command", post(execute_connector_command))
        .route("/connectors/:id/plug-in", post(plug_in_connector))
        .route("/connectors/:id/plug-out", post(plug_out_connector))
        .route("/connectors/:id/start-transaction", post(start_transaction))
        .route("/connectors/:id/stop-transaction", post(stop_transaction))
        .route("/connectors/:id/fault", post(inject_fault))
        .route("/connectors/:id/clear-fault", post(clear_fault))
        .route("/connectors/:id/availability", put(set_availability))
        .route("/connection/connect", post(connect_to_central_system))
        .route(
            "/connection/disconnect",
            post(disconnect_from_central_system),
        )
        .route("/scenarios", get(list_scenarios))
        .route("/scenarios/:name", post(execute_scenario))
        .route("/events", get(get_events))
        .route("/config", get(get_config))
        .route("/config", put(update_config))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the simulator with REST API
pub async fn start_simulator(config: SimulatorConfig, bind_address: SocketAddr) -> Result<()> {
    info!("Starting OCPP Simulator on {}", bind_address);

    // Create charge point
    let cp_config = ChargePointConfig {
        charge_point_id: config.charge_point_id.clone(),
        central_system_url: config.central_system_url.clone(),
        vendor_info: config.vendor_info.clone(),
        connector_count: config.connector_count,
        heartbeat_interval: config.heartbeat_interval,
        meter_values_interval: config.meter_values_interval,
        connection_retry_interval: config.connection_retry_interval,
        max_connection_retries: config.max_connection_retries,
        auto_reconnect: config.auto_reconnect,
        transport_config: config.transport_config.clone(),
    };

    let charge_point = Arc::new(ChargePoint::new(cp_config)?);

    // Create event channel
    let (event_sender, mut event_receiver) = mpsc::unbounded_channel();

    // Initialize stats
    let start_time = chrono::Utc::now();
    let stats = Arc::new(RwLock::new(SimulatorStats {
        uptime_seconds: 0,
        connector_count: config.connector_count,
        connector_states: HashMap::new(),
        active_transactions: Vec::new(),
        total_transactions: 0,
        total_energy_wh: 0,
        connected: false,
        last_event: None,
        error_count: 0,
    }));

    // Create simulator state
    let state = SimulatorState {
        charge_point: charge_point.clone(),
        config: Arc::new(config.clone()),
        stats: stats.clone(),
        event_sender: event_sender.clone(),
        start_time,
    };

    // Start charge point
    charge_point.start().await?;

    // Spawn statistics updater
    let stats_updater = stats.clone();
    let cp_for_stats = charge_point.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            update_statistics(&stats_updater, &cp_for_stats).await;
        }
    });

    // Spawn event processor
    let event_stats = stats.clone();
    tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            process_simulator_event(&event_stats, event).await;
        }
    });

    // Create and start web server
    let app = create_api_router(state);

    info!("OCPP Simulator API available at http://{}", bind_address);
    info!("WebSocket connection: {}", config.central_system_url);
    info!("Charge Point ID: {}", config.charge_point_id);
    info!("Number of connectors: {}", config.connector_count);

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// API Handlers

/// Get simulator status
async fn get_status(
    State(state): State<SimulatorState>,
) -> Json<SimulatorResponse<SimulatorStats>> {
    let stats = state.stats.read().await.clone();
    Json(SimulatorResponse::success(stats))
}

/// Get detailed statistics
async fn get_stats(State(state): State<SimulatorState>) -> Json<SimulatorResponse<SimulatorStats>> {
    let mut stats = state.stats.write().await;
    stats.uptime_seconds = chrono::Utc::now()
        .signed_duration_since(state.start_time)
        .num_seconds() as u64;
    let stats_clone = stats.clone();
    Json(SimulatorResponse::success(stats_clone))
}

/// Get all connectors
async fn get_connectors(
    State(state): State<SimulatorState>,
) -> Json<SimulatorResponse<Vec<ConnectorInfo>>> {
    let mut connectors = Vec::new();

    for i in 1..=state.config.connector_count {
        if let Ok(connector_id) = ConnectorId::new(i) {
            if let Some(connector) = state.charge_point.get_connector(connector_id).await {
                let info = ConnectorInfo {
                    connector_id: i,
                    status: connector.status().await,
                    error_code: connector.error_code().await,
                    error_info: connector.error_info().await,
                    cable_plugged: matches!(
                        connector.physical_state().await,
                        ocpp_cp::connector::ConnectorPhysicalState::Plugged
                            | ocpp_cp::connector::ConnectorPhysicalState::PluggedAndLocked
                    ),
                    transaction_id: connector
                        .current_transaction()
                        .await
                        .map(|t| t.id().value()),
                    last_meter_reading: convert_meter_reading(
                        &connector.last_meter_reading().await,
                    ),
                    reserved_for: connector.reserved_for().await,
                };
                connectors.push(info);
            }
        }
    }

    Json(SimulatorResponse::success(connectors))
}

/// Get specific connector
async fn get_connector(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
) -> Result<Json<SimulatorResponse<ConnectorInfo>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;

    if let Some(connector) = state.charge_point.get_connector(connector_id).await {
        let info = ConnectorInfo {
            connector_id: id,
            status: connector.status().await,
            error_code: connector.error_code().await,
            error_info: connector.error_info().await,
            cable_plugged: matches!(
                connector.physical_state().await,
                ocpp_cp::connector::ConnectorPhysicalState::Plugged
                    | ocpp_cp::connector::ConnectorPhysicalState::PluggedAndLocked
            ),
            transaction_id: connector
                .current_transaction()
                .await
                .map(|t| t.id().value()),
            last_meter_reading: convert_meter_reading(&connector.last_meter_reading().await),
            reserved_for: connector.reserved_for().await,
        };
        Ok(Json(SimulatorResponse::success(info)))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Execute connector command
async fn execute_connector_command(
    State(_state): State<SimulatorState>,
    Path(id): Path<u32>,
    Json(command): Json<ConnectorCommand>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let _connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;

    match command.command.as_str() {
        "suspend_ev" => {
            // Implementation would call connector.suspend_by_ev()
            Ok(Json(SimulatorResponse::success_empty()))
        }
        "suspend_evse" => {
            // Implementation would call connector.suspend_by_evse()
            Ok(Json(SimulatorResponse::success_empty()))
        }
        "resume" => {
            // Implementation would call connector.resume_charging()
            Ok(Json(SimulatorResponse::success_empty()))
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Plug in connector
async fn plug_in_connector(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;

    match state.charge_point.plug_in(connector_id).await {
        Ok(()) => {
            let _ = state.event_sender.send(SimulatorEvent::ConnectorPluggedIn {
                connector_id: id,
                timestamp: chrono::Utc::now(),
            });
            Ok(Json(SimulatorResponse::success_empty()))
        }
        Err(e) => {
            error!("Failed to plug in connector {}: {}", id, e);
            Ok(Json(SimulatorResponse::error(e.to_string())))
        }
    }
}

/// Plug out connector
async fn plug_out_connector(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;

    match state.charge_point.plug_out(connector_id).await {
        Ok(()) => {
            let _ = state
                .event_sender
                .send(SimulatorEvent::ConnectorPluggedOut {
                    connector_id: id,
                    timestamp: chrono::Utc::now(),
                });
            Ok(Json(SimulatorResponse::success_empty()))
        }
        Err(e) => {
            error!("Failed to plug out connector {}: {}", id, e);
            Ok(Json(SimulatorResponse::error(e.to_string())))
        }
    }
}

/// Start transaction
async fn start_transaction(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let id_tag = params
        .get("id_tag")
        .cloned()
        .unwrap_or_else(|| format!("user_{}", id));

    match state
        .charge_point
        .start_transaction(connector_id, id_tag.clone())
        .await
    {
        Ok(()) => {
            let _ = state.event_sender.send(SimulatorEvent::TransactionStarted {
                connector_id: id,
                transaction_id: 0, // Would be set from actual transaction
                id_tag,
                timestamp: chrono::Utc::now(),
            });
            Ok(Json(SimulatorResponse::success_empty()))
        }
        Err(e) => {
            error!("Failed to start transaction on connector {}: {}", id, e);
            Ok(Json(SimulatorResponse::error(e.to_string())))
        }
    }
}

/// Stop transaction
async fn stop_transaction(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let reason = params.get("reason").cloned();

    match state
        .charge_point
        .stop_transaction(connector_id, reason)
        .await
    {
        Ok(()) => {
            let _ = state.event_sender.send(SimulatorEvent::TransactionStopped {
                connector_id: id,
                transaction_id: 0, // Would be set from actual transaction
                reason: "Manual".to_string(),
                energy_delivered_wh: None,
                timestamp: chrono::Utc::now(),
            });
            Ok(Json(SimulatorResponse::success_empty()))
        }
        Err(e) => {
            error!("Failed to stop transaction on connector {}: {}", id, e);
            Ok(Json(SimulatorResponse::error(e.to_string())))
        }
    }
}

/// Inject fault
async fn inject_fault(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
    Json(fault_req): Json<FaultRequest>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;

    match state
        .charge_point
        .set_fault(connector_id, fault_req.error_code, fault_req.info.clone())
        .await
    {
        Ok(()) => {
            let _ = state.event_sender.send(SimulatorEvent::FaultInjected {
                connector_id: id,
                error_code: fault_req.error_code,
                info: fault_req.info,
                timestamp: chrono::Utc::now(),
            });
            Ok(Json(SimulatorResponse::success_empty()))
        }
        Err(e) => {
            error!("Failed to inject fault on connector {}: {}", id, e);
            Ok(Json(SimulatorResponse::error(e.to_string())))
        }
    }
}

/// Clear fault
async fn clear_fault(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;

    match state.charge_point.clear_fault(connector_id).await {
        Ok(()) => {
            let _ = state.event_sender.send(SimulatorEvent::FaultCleared {
                connector_id: id,
                timestamp: chrono::Utc::now(),
            });
            Ok(Json(SimulatorResponse::success_empty()))
        }
        Err(e) => {
            error!("Failed to clear fault on connector {}: {}", id, e);
            Ok(Json(SimulatorResponse::error(e.to_string())))
        }
    }
}

/// Set connector availability
async fn set_availability(
    State(state): State<SimulatorState>,
    Path(id): Path<u32>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SimulatorResponse<()>>, StatusCode> {
    let connector_id = ConnectorId::new(id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let available = params
        .get("available")
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(true);

    match state
        .charge_point
        .set_availability(connector_id, available)
        .await
    {
        Ok(()) => {
            let _ = state
                .event_sender
                .send(SimulatorEvent::AvailabilityChanged {
                    connector_id: id,
                    available,
                    timestamp: chrono::Utc::now(),
                });
            Ok(Json(SimulatorResponse::success_empty()))
        }
        Err(e) => {
            error!("Failed to set availability for connector {}: {}", id, e);
            Ok(Json(SimulatorResponse::error(e.to_string())))
        }
    }
}

/// Connect to central system
async fn connect_to_central_system(
    State(state): State<SimulatorState>,
) -> Json<SimulatorResponse<()>> {
    match state.charge_point.connect().await {
        Ok(()) => Json(SimulatorResponse::success_empty()),
        Err(e) => {
            error!("Failed to connect to central system: {}", e);
            Json(SimulatorResponse::error(e.to_string()))
        }
    }
}

/// Disconnect from central system
async fn disconnect_from_central_system(
    State(state): State<SimulatorState>,
) -> Json<SimulatorResponse<()>> {
    match state.charge_point.disconnect().await {
        Ok(()) => Json(SimulatorResponse::success_empty()),
        Err(e) => {
            error!("Failed to disconnect from central system: {}", e);
            Json(SimulatorResponse::error(e.to_string()))
        }
    }
}

/// List available scenarios
async fn list_scenarios() -> Json<SimulatorResponse<Vec<String>>> {
    let scenarios = vec![
        "basic_charging".to_string(),
        "interrupted_charging".to_string(),
        "fault_during_charging".to_string(),
        "multiple_connectors".to_string(),
        "reservation_flow".to_string(),
        "emergency_stop".to_string(),
        "network_issues".to_string(),
    ];
    Json(SimulatorResponse::success(scenarios))
}

/// Execute scenario
async fn execute_scenario(
    State(state): State<SimulatorState>,
    Path(name): Path<String>,
    Json(_req): Json<ScenarioRequest>,
) -> Json<SimulatorResponse<()>> {
    info!("Executing scenario: {}", name);

    // Scenario execution would be implemented here
    // For now, just acknowledge the request
    let _ = state.event_sender.send(SimulatorEvent::ScenarioStarted {
        scenario: name.clone(),
        timestamp: chrono::Utc::now(),
    });

    Json(SimulatorResponse::success_empty())
}

/// Get recent events
async fn get_events(
    State(_state): State<SimulatorState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<SimulatorResponse<Vec<String>>> {
    let _limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100);

    // Would return actual event history
    let events = vec!["Simulator started".to_string()];
    Json(SimulatorResponse::success(events))
}

/// Get configuration
async fn get_config(
    State(state): State<SimulatorState>,
) -> Json<SimulatorResponse<SimulatorConfig>> {
    let config = (*state.config).clone();
    Json(SimulatorResponse::success(config))
}

/// Update configuration
async fn update_config(
    State(_state): State<SimulatorState>,
    Json(_config): Json<SimulatorConfig>,
) -> Json<SimulatorResponse<()>> {
    // Configuration update would be implemented here
    Json(SimulatorResponse::success_empty())
}

// Helper functions

/// Convert internal meter reading to API format
fn convert_meter_reading(reading: &ocpp_cp::connector::MeterReading) -> MeterReading {
    MeterReading {
        timestamp: reading.timestamp,
        energy_wh: reading.energy_wh,
        power_w: reading.power_w,
        voltage_v: reading.voltage_v,
        current_a: reading.current_a,
        temperature_c: reading.temperature_c,
    }
}

/// Update simulator statistics
async fn update_statistics(stats: &Arc<RwLock<SimulatorStats>>, charge_point: &Arc<ChargePoint>) {
    let mut stats_guard = stats.write().await;

    // Update connection status
    stats_guard.connected = charge_point.is_connected().await;

    // Update connector states
    for i in 1..=stats_guard.connector_count {
        if let Ok(connector_id) = ConnectorId::new(i) {
            if let Some(connector) = charge_point.get_connector(connector_id).await {
                stats_guard
                    .connector_states
                    .insert(i, connector.status().await);
            }
        }
    }

    // Update last event timestamp
    stats_guard.last_event = Some(chrono::Utc::now());
}

/// Process simulator event
async fn process_simulator_event(stats: &Arc<RwLock<SimulatorStats>>, event: SimulatorEvent) {
    let mut stats_guard = stats.write().await;

    match event {
        SimulatorEvent::TransactionStarted { .. } => {
            stats_guard.total_transactions += 1;
        }
        SimulatorEvent::Error { .. } => {
            stats_guard.error_count += 1;
        }
        _ => {}
    }

    stats_guard.last_event = Some(chrono::Utc::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulator_response_creation() {
        let success_resp = SimulatorResponse::success("test data".to_string());
        assert!(success_resp.success);
        assert_eq!(success_resp.data, Some("test data".to_string()));
        assert!(success_resp.error.is_none());

        let error_resp = SimulatorResponse::<String>::error("test error".to_string());
        assert!(!error_resp.success);
        assert!(error_resp.data.is_none());
        assert_eq!(error_resp.error, Some("test error".to_string()));
    }

    #[test]
    fn test_meter_reading_conversion() {
        let internal_reading = ocpp_cp::connector::MeterReading::new(1000.0, 2000.0, 230.0, 8.7);
        let api_reading = convert_meter_reading(&internal_reading);

        assert_eq!(api_reading.energy_wh, 1000.0);
        assert_eq!(api_reading.power_w, 2000.0);
        assert_eq!(api_reading.voltage_v, 230.0);
        assert_eq!(api_reading.current_a, 8.7);
    }

    #[tokio::test]
    async fn test_api_router_creation() {
        // Create minimal test state
        let config = SimulatorConfig::default();
        let cp_config = ChargePointConfig::default();
        let charge_point = Arc::new(ChargePoint::new(cp_config).unwrap());
        let (event_sender, _) = mpsc::unbounded_channel();
        let stats = Arc::new(RwLock::new(SimulatorStats {
            uptime_seconds: 0,
            connector_count: 2,
            connector_states: HashMap::new(),
            active_transactions: Vec::new(),
            total_transactions: 0,
            total_energy_wh: 0,
            connected: false,
            last_event: None,
            error_count: 0,
        }));

        let state = SimulatorState {
            charge_point,
            config: Arc::new(config),
            stats,
            event_sender,
            start_time: chrono::Utc::now(),
        };

        let _router = create_api_router(state);
    }
}
