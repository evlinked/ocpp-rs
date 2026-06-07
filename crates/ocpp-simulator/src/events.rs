//! # Simulator Events
//!
//! This module provides event handling and streaming capabilities for the OCPP simulator,
//! including event types, handlers, and real-time event streaming via WebSocket.

use ocpp_types::v16j::ChargePointErrorCode;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};

/// Maximum number of events to keep in history
const MAX_EVENT_HISTORY: usize = 1000;

/// Simulator event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub enum SimulatorEvent {
    /// Simulator started
    SimulatorStarted {
        charge_point_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Simulator stopped
    SimulatorStopped {
        charge_point_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Connected to central system
    Connected {
        central_system_url: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Disconnected from central system
    Disconnected {
        reason: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Boot notification sent
    BootNotificationSent {
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Boot notification response received
    BootNotificationReceived {
        status: String,
        interval: i32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Heartbeat sent
    HeartbeatSent {
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Heartbeat response received
    HeartbeatReceived {
        current_time: chrono::DateTime<chrono::Utc>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Connector plugged in
    ConnectorPluggedIn {
        connector_id: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Connector plugged out
    ConnectorPluggedOut {
        connector_id: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Connector status changed
    ConnectorStatusChanged {
        connector_id: u32,
        old_status: String,
        new_status: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Transaction started
    TransactionStarted {
        connector_id: u32,
        transaction_id: i32,
        id_tag: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Transaction stopped
    TransactionStopped {
        connector_id: u32,
        transaction_id: i32,
        reason: String,
        energy_delivered_wh: Option<i32>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Charging started
    ChargingStarted {
        connector_id: u32,
        transaction_id: i32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Charging stopped
    ChargingStopped {
        connector_id: u32,
        transaction_id: i32,
        reason: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Charging suspended
    ChargingSuspended {
        connector_id: u32,
        transaction_id: i32,
        suspended_by: String, // "EV" or "EVSE"
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Charging resumed
    ChargingResumed {
        connector_id: u32,
        transaction_id: i32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Meter values updated
    MeterValuesUpdated {
        connector_id: u32,
        transaction_id: Option<i32>,
        energy_wh: f64,
        power_w: f64,
        voltage_v: f64,
        current_a: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Status notification sent
    StatusNotificationSent {
        connector_id: u32,
        status: String,
        error_code: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Fault injected
    FaultInjected {
        connector_id: u32,
        error_code: ChargePointErrorCode,
        info: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Fault cleared
    FaultCleared {
        connector_id: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Availability changed
    AvailabilityChanged {
        connector_id: u32,
        available: bool,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Remote command received
    RemoteCommandReceived {
        command: String,
        connector_id: Option<u32>,
        parameters: serde_json::Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Remote command executed
    RemoteCommandExecuted {
        command: String,
        connector_id: Option<u32>,
        success: bool,
        result: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Configuration changed
    ConfigurationChanged {
        key: String,
        old_value: Option<String>,
        new_value: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Scenario started
    ScenarioStarted {
        scenario: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Scenario completed
    ScenarioCompleted {
        scenario: String,
        success: bool,
        duration_seconds: u64,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Scenario step executed
    ScenarioStepExecuted {
        scenario: String,
        step: String,
        success: bool,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Error occurred
    Error {
        error: String,
        context: Option<String>,
        severity: ErrorSeverity,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Warning issued
    Warning {
        message: String,
        context: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Debug information
    Debug {
        message: String,
        context: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Performance metric
    PerformanceMetric {
        metric_name: String,
        value: f64,
        unit: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Custom event
    Custom {
        event_type: String,
        data: serde_json::Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

/// Error severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl SimulatorEvent {
    /// Get event timestamp
    pub fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Self::SimulatorStarted { timestamp, .. }
            | Self::SimulatorStopped { timestamp, .. }
            | Self::Connected { timestamp, .. }
            | Self::Disconnected { timestamp, .. }
            | Self::BootNotificationSent { timestamp, .. }
            | Self::BootNotificationReceived { timestamp, .. }
            | Self::HeartbeatSent { timestamp, .. }
            | Self::HeartbeatReceived { timestamp, .. }
            | Self::ConnectorPluggedIn { timestamp, .. }
            | Self::ConnectorPluggedOut { timestamp, .. }
            | Self::ConnectorStatusChanged { timestamp, .. }
            | Self::TransactionStarted { timestamp, .. }
            | Self::TransactionStopped { timestamp, .. }
            | Self::ChargingStarted { timestamp, .. }
            | Self::ChargingStopped { timestamp, .. }
            | Self::ChargingSuspended { timestamp, .. }
            | Self::ChargingResumed { timestamp, .. }
            | Self::MeterValuesUpdated { timestamp, .. }
            | Self::StatusNotificationSent { timestamp, .. }
            | Self::FaultInjected { timestamp, .. }
            | Self::FaultCleared { timestamp, .. }
            | Self::AvailabilityChanged { timestamp, .. }
            | Self::RemoteCommandReceived { timestamp, .. }
            | Self::RemoteCommandExecuted { timestamp, .. }
            | Self::ConfigurationChanged { timestamp, .. }
            | Self::ScenarioStarted { timestamp, .. }
            | Self::ScenarioCompleted { timestamp, .. }
            | Self::ScenarioStepExecuted { timestamp, .. }
            | Self::Error { timestamp, .. }
            | Self::Warning { timestamp, .. }
            | Self::Debug { timestamp, .. }
            | Self::PerformanceMetric { timestamp, .. }
            | Self::Custom { timestamp, .. } => *timestamp,
        }
    }

    /// Get event type name
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::SimulatorStarted { .. } => "simulator_started",
            Self::SimulatorStopped { .. } => "simulator_stopped",
            Self::Connected { .. } => "connected",
            Self::Disconnected { .. } => "disconnected",
            Self::BootNotificationSent { .. } => "boot_notification_sent",
            Self::BootNotificationReceived { .. } => "boot_notification_received",
            Self::HeartbeatSent { .. } => "heartbeat_sent",
            Self::HeartbeatReceived { .. } => "heartbeat_received",
            Self::ConnectorPluggedIn { .. } => "connector_plugged_in",
            Self::ConnectorPluggedOut { .. } => "connector_plugged_out",
            Self::ConnectorStatusChanged { .. } => "connector_status_changed",
            Self::TransactionStarted { .. } => "transaction_started",
            Self::TransactionStopped { .. } => "transaction_stopped",
            Self::ChargingStarted { .. } => "charging_started",
            Self::ChargingStopped { .. } => "charging_stopped",
            Self::ChargingSuspended { .. } => "charging_suspended",
            Self::ChargingResumed { .. } => "charging_resumed",
            Self::MeterValuesUpdated { .. } => "meter_values_updated",
            Self::StatusNotificationSent { .. } => "status_notification_sent",
            Self::FaultInjected { .. } => "fault_injected",
            Self::FaultCleared { .. } => "fault_cleared",
            Self::AvailabilityChanged { .. } => "availability_changed",
            Self::RemoteCommandReceived { .. } => "remote_command_received",
            Self::RemoteCommandExecuted { .. } => "remote_command_executed",
            Self::ConfigurationChanged { .. } => "configuration_changed",
            Self::ScenarioStarted { .. } => "scenario_started",
            Self::ScenarioCompleted { .. } => "scenario_completed",
            Self::ScenarioStepExecuted { .. } => "scenario_step_executed",
            Self::Error { .. } => "error",
            Self::Warning { .. } => "warning",
            Self::Debug { .. } => "debug",
            Self::PerformanceMetric { .. } => "performance_metric",
            Self::Custom { .. } => "custom",
        }
    }

    /// Get connector ID if applicable
    pub fn connector_id(&self) -> Option<u32> {
        match self {
            Self::ConnectorPluggedIn { connector_id, .. }
            | Self::ConnectorPluggedOut { connector_id, .. }
            | Self::ConnectorStatusChanged { connector_id, .. }
            | Self::TransactionStarted { connector_id, .. }
            | Self::TransactionStopped { connector_id, .. }
            | Self::ChargingStarted { connector_id, .. }
            | Self::ChargingStopped { connector_id, .. }
            | Self::ChargingSuspended { connector_id, .. }
            | Self::ChargingResumed { connector_id, .. }
            | Self::MeterValuesUpdated { connector_id, .. }
            | Self::StatusNotificationSent { connector_id, .. }
            | Self::FaultInjected { connector_id, .. }
            | Self::FaultCleared { connector_id, .. }
            | Self::AvailabilityChanged { connector_id, .. } => Some(*connector_id),
            Self::RemoteCommandReceived { connector_id, .. }
            | Self::RemoteCommandExecuted { connector_id, .. } => *connector_id,
            _ => None,
        }
    }

    /// Check if event is an error
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// Check if event is a warning
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Warning { .. })
    }

    /// Get event severity (for filtering)
    pub fn severity(&self) -> EventSeverity {
        match self {
            Self::Debug { .. } => EventSeverity::Debug,
            Self::Warning { .. } | Self::FaultInjected { .. } | Self::ChargingSuspended { .. } => {
                EventSeverity::Warning
            }
            Self::Error { severity, .. } => match severity {
                ErrorSeverity::Low => EventSeverity::Warning,
                ErrorSeverity::Medium => EventSeverity::Error,
                ErrorSeverity::High | ErrorSeverity::Critical => EventSeverity::Critical,
            },
            Self::Disconnected { .. }
            | Self::TransactionStopped { .. }
            | Self::ChargingStopped { .. } => EventSeverity::Error,
            _ => EventSeverity::Info,
        }
    }
}

/// Event severity levels for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventSeverity {
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
    Critical = 4,
}

/// Event filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Minimum severity level
    pub min_severity: EventSeverity,
    /// Include only specific event types
    pub event_types: Option<Vec<String>>,
    /// Include only specific connectors
    pub connector_ids: Option<Vec<u32>>,
    /// Time range filter
    #[serde(skip)]
    pub time_range: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
}

impl Default for EventFilter {
    fn default() -> Self {
        Self {
            min_severity: EventSeverity::Info,
            event_types: None,
            connector_ids: None,
            time_range: None,
        }
    }
}

impl EventFilter {
    /// Check if event passes the filter
    pub fn matches(&self, event: &SimulatorEvent) -> bool {
        // Check severity
        if event.severity() < self.min_severity {
            return false;
        }

        // Check event types
        if let Some(ref types) = self.event_types {
            if !types.contains(&event.event_type().to_string()) {
                return false;
            }
        }

        // Check connector IDs
        if let Some(ref connector_ids) = self.connector_ids {
            if let Some(event_connector_id) = event.connector_id() {
                if !connector_ids.contains(&event_connector_id) {
                    return false;
                }
            } else {
                // Event has no connector ID, but filter requires specific connectors
                return false;
            }
        }

        // Check time range
        if let Some((start, end)) = self.time_range {
            let event_time = event.timestamp();
            if event_time < start || event_time > end {
                return false;
            }
        }

        true
    }
}

/// Event handler trait
#[async_trait::async_trait]
pub trait SimulatorEventHandler: Send + Sync {
    /// Handle a simulator event
    async fn handle_event(&self, event: SimulatorEvent);
}

/// Event store for managing event history and streaming
#[derive(Clone)]
pub struct EventStore {
    /// Event history
    history: Arc<RwLock<VecDeque<SimulatorEvent>>>,
    /// Event broadcaster for real-time streaming
    broadcaster: broadcast::Sender<SimulatorEvent>,
    /// Event handlers
    handlers: Arc<RwLock<Vec<Box<dyn SimulatorEventHandler>>>>,
}

impl EventStore {
    /// Create a new event store
    pub fn new() -> Self {
        let (broadcaster, _) = broadcast::channel(1000);

        Self {
            history: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_EVENT_HISTORY))),
            broadcaster,
            handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add an event to the store
    pub async fn add_event(&self, mut event: SimulatorEvent) {
        // Ensure event has timestamp
        match &event {
            SimulatorEvent::SimulatorStarted { timestamp, .. }
                if timestamp == &chrono::DateTime::<chrono::Utc>::default() =>
            {
                if let SimulatorEvent::SimulatorStarted {
                    ref mut timestamp, ..
                } = event
                {
                    *timestamp = chrono::Utc::now();
                }
            }
            _ => {}
        }

        debug!("Adding event: {:?}", event.event_type());

        // Add to history
        {
            let mut history = self.history.write().await;
            if history.len() >= MAX_EVENT_HISTORY {
                history.pop_front();
            }
            history.push_back(event.clone());
        }

        // Broadcast to subscribers
        let _ = self.broadcaster.send(event.clone());

        // Notify handlers
        let handlers = self.handlers.read().await;
        for handler in handlers.iter() {
            handler.handle_event(event.clone()).await;
        }
    }

    /// Get event history with optional filter
    pub async fn get_history(&self, filter: Option<EventFilter>) -> Vec<SimulatorEvent> {
        let history = self.history.read().await;

        if let Some(filter) = filter {
            history
                .iter()
                .filter(|event| filter.matches(event))
                .cloned()
                .collect()
        } else {
            history.iter().cloned().collect()
        }
    }

    /// Get recent events (last N events)
    pub async fn get_recent(&self, count: usize) -> Vec<SimulatorEvent> {
        let history = self.history.read().await;
        history.iter().rev().take(count).rev().cloned().collect()
    }

    /// Subscribe to real-time events
    pub fn subscribe(&self) -> broadcast::Receiver<SimulatorEvent> {
        self.broadcaster.subscribe()
    }

    /// Add event handler
    pub async fn add_handler(&self, handler: Box<dyn SimulatorEventHandler>) {
        let mut handlers = self.handlers.write().await;
        handlers.push(handler);
    }

    /// Clear event history
    pub async fn clear_history(&self) {
        let mut history = self.history.write().await;
        history.clear();
    }

    /// Get event statistics
    pub async fn get_statistics(&self) -> EventStatistics {
        let history = self.history.read().await;
        let total_events = history.len();

        let mut event_type_counts = std::collections::HashMap::new();
        let mut connector_counts = std::collections::HashMap::new();
        let mut severity_counts = std::collections::HashMap::new();
        let mut error_count = 0;
        let mut warning_count = 0;

        for event in history.iter() {
            // Count by event type
            *event_type_counts
                .entry(event.event_type().to_string())
                .or_insert(0) += 1;

            // Count by connector
            if let Some(connector_id) = event.connector_id() {
                *connector_counts.entry(connector_id).or_insert(0) += 1;
            }

            // Count by severity
            let severity_key = match event.severity() {
                EventSeverity::Debug => "debug",
                EventSeverity::Info => "info",
                EventSeverity::Warning => "warning",
                EventSeverity::Error => "error",
                EventSeverity::Critical => "critical",
            };
            *severity_counts.entry(severity_key.to_string()).or_insert(0) += 1;

            // Count errors and warnings
            if event.is_error() {
                error_count += 1;
            }
            if event.is_warning() {
                warning_count += 1;
            }
        }

        EventStatistics {
            total_events,
            event_type_counts,
            connector_counts,
            severity_counts,
            error_count,
            warning_count,
            oldest_event: history.front().map(|e| e.timestamp()),
            newest_event: history.back().map(|e| e.timestamp()),
        }
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Event statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStatistics {
    /// Total number of events
    pub total_events: usize,
    /// Count by event type
    pub event_type_counts: std::collections::HashMap<String, usize>,
    /// Count by connector
    pub connector_counts: std::collections::HashMap<u32, usize>,
    /// Count by severity
    pub severity_counts: std::collections::HashMap<String, usize>,
    /// Total error count
    pub error_count: usize,
    /// Total warning count
    pub warning_count: usize,
    /// Timestamp of oldest event
    pub oldest_event: Option<chrono::DateTime<chrono::Utc>>,
    /// Timestamp of newest event
    pub newest_event: Option<chrono::DateTime<chrono::Utc>>,
}

/// Console event handler for logging events
pub struct ConsoleEventHandler {
    /// Minimum severity to log
    min_severity: EventSeverity,
}

impl ConsoleEventHandler {
    /// Create a new console event handler
    pub fn new(min_severity: EventSeverity) -> Self {
        Self { min_severity }
    }
}

#[async_trait::async_trait]
impl SimulatorEventHandler for ConsoleEventHandler {
    async fn handle_event(&self, event: SimulatorEvent) {
        if event.severity() < self.min_severity {
            return;
        }

        let timestamp = event.timestamp().format("%Y-%m-%d %H:%M:%S%.3f");
        let event_type = event.event_type();

        match event.severity() {
            EventSeverity::Debug => {
                debug!("[{}] {}: {:?}", timestamp, event_type, event);
            }
            EventSeverity::Info => {
                info!("[{}] {}: {:?}", timestamp, event_type, event);
            }
            EventSeverity::Warning | EventSeverity::Error | EventSeverity::Critical => {
                tracing::warn!("[{}] {}: {:?}", timestamp, event_type, event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_properties() {
        let event = SimulatorEvent::ConnectorPluggedIn {
            connector_id: 1,
            timestamp: chrono::Utc::now(),
        };

        assert_eq!(event.event_type(), "connector_plugged_in");
        assert_eq!(event.connector_id(), Some(1));
        assert!(!event.is_error());
        assert!(!event.is_warning());
        assert_eq!(event.severity(), EventSeverity::Info);
    }

    #[test]
    fn test_event_filter() {
        // Filter for high-severity errors (no connector constraint — Error events are global)
        let filter = EventFilter {
            min_severity: EventSeverity::Warning,
            event_types: Some(vec!["error".to_string()]),
            connector_ids: None,
            time_range: None,
        };

        let event1 = SimulatorEvent::Error {
            error: "Test error".to_string(),
            context: None,
            severity: ErrorSeverity::High,
            timestamp: chrono::Utc::now(),
        };

        let event2 = SimulatorEvent::ConnectorPluggedIn {
            connector_id: 1,
            timestamp: chrono::Utc::now(),
        };

        assert!(filter.matches(&event1)); // Matches: severity OK, event type matches
        assert!(!filter.matches(&event2)); // Wrong event type (connector_plugged_in != error)
    }

    #[tokio::test]
    async fn test_event_store() {
        let store = EventStore::new();

        let event = SimulatorEvent::ConnectorPluggedIn {
            connector_id: 1,
            timestamp: chrono::Utc::now(),
        };

        store.add_event(event.clone()).await;

        let history = store.get_history(None).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_type(), event.event_type());

        let recent = store.get_recent(10).await;
        assert_eq!(recent.len(), 1);

        let stats = store.get_statistics().await;
        assert_eq!(stats.total_events, 1);
        assert!(stats.event_type_counts.contains_key("connector_plugged_in"));
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let store = EventStore::new();
        let mut receiver = store.subscribe();

        let event = SimulatorEvent::ConnectorPluggedIn {
            connector_id: 1,
            timestamp: chrono::Utc::now(),
        };

        // Add event in a separate task to avoid blocking
        let store_clone = store.clone();
        let event_clone = event.clone();
        tokio::spawn(async move {
            store_clone.add_event(event_clone).await;
        });

        // Should receive the event
        let received_event = receiver.recv().await.unwrap();
        assert_eq!(received_event.event_type(), event.event_type());
    }

    #[test]
    fn test_error_severity_ordering() {
        assert!(EventSeverity::Debug < EventSeverity::Info);
        assert!(EventSeverity::Info < EventSeverity::Warning);
        assert!(EventSeverity::Warning < EventSeverity::Error);
        assert!(EventSeverity::Error < EventSeverity::Critical);
    }
}
