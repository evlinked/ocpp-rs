//! # Simulator Configuration
//!
//! This module provides configuration structures and utilities for the OCPP simulator,
//! including charge point settings, scenario parameters, and API configuration.

use ocpp_transport::TransportConfig;
use ocpp_types::v16j::ChargePointVendorInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main simulator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorConfig {
    /// Charge point identifier
    pub charge_point_id: String,
    /// Central system WebSocket URL
    pub central_system_url: String,
    /// Charge point vendor information
    pub vendor_info: ChargePointVendorInfo,
    /// Number of connectors
    pub connector_count: u32,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Meter values sample interval in seconds
    pub meter_values_interval: u64,
    /// Connection retry interval in seconds
    pub connection_retry_interval: u64,
    /// Maximum connection retry attempts
    pub max_connection_retries: u32,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    /// Transport configuration
    #[serde(skip)]
    pub transport_config: TransportConfig,
    /// API server configuration
    pub api_config: ApiConfig,
    /// Meter simulation configuration
    pub meter_config: MeterConfig,
    /// Scenario configuration
    pub scenario_config: ScenarioConfig,
    /// Fault injection configuration
    pub fault_config: FaultConfig,
    /// Logging configuration
    pub logging_config: LoggingConfig,
}

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API server bind address
    pub bind_address: String,
    /// API server port
    pub port: u16,
    /// Enable CORS
    pub enable_cors: bool,
    /// API key for authentication (optional)
    pub api_key: Option<String>,
    /// Enable WebSocket events streaming
    pub enable_websocket_events: bool,
    /// WebSocket events port
    pub websocket_events_port: u16,
}

/// Meter simulation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterConfig {
    /// Initial energy reading in Wh
    pub initial_energy_wh: f64,
    /// Base power consumption in W (when not charging)
    pub base_power_w: f64,
    /// Maximum charging power in W
    pub max_charging_power_w: f64,
    /// Voltage level in V
    pub voltage_v: f64,
    /// Power variation percentage (0.0 to 1.0)
    pub power_variation: f64,
    /// Meter reading update interval in seconds
    pub update_interval_seconds: u64,
    /// Include realistic meter noise
    pub include_noise: bool,
    /// Noise amplitude as percentage of reading
    pub noise_amplitude: f64,
    /// Simulate meter drift
    pub simulate_drift: bool,
    /// Drift rate per hour as percentage
    pub drift_rate_per_hour: f64,
}

/// Scenario configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    /// Enable automatic scenario execution
    pub auto_execute: bool,
    /// Default scenario to run on startup
    pub default_scenario: Option<String>,
    /// Scenario execution interval in seconds
    pub execution_interval_seconds: u64,
    /// Available scenarios
    pub scenarios: HashMap<String, ScenarioDefinition>,
}

/// Individual scenario definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioDefinition {
    /// Scenario name
    pub name: String,
    /// Description
    pub description: String,
    /// Scenario steps
    pub steps: Vec<ScenarioStep>,
    /// Repeat scenario
    pub repeat: bool,
    /// Repeat interval in seconds
    pub repeat_interval_seconds: Option<u64>,
    /// Scenario parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Individual scenario step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStep {
    /// Step name
    pub name: String,
    /// Action to perform
    pub action: ScenarioAction,
    /// Delay before executing step in seconds
    pub delay_seconds: u64,
    /// Connector ID (if applicable)
    pub connector_id: Option<u32>,
    /// Parameters for the action
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Available scenario actions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioAction {
    /// Plug in cable
    PlugIn,
    /// Plug out cable
    PlugOut,
    /// Start transaction
    StartTransaction,
    /// Stop transaction
    StopTransaction,
    /// Inject fault
    InjectFault,
    /// Clear fault
    ClearFault,
    /// Suspend charging (EV side)
    SuspendChargingEV,
    /// Suspend charging (EVSE side)
    SuspendChargingEVSE,
    /// Resume charging
    ResumeCharging,
    /// Set availability
    SetAvailability,
    /// Wait for duration
    Wait,
    /// Send heartbeat
    SendHeartbeat,
    /// Update meter values
    UpdateMeterValues,
    /// Connect to central system
    Connect,
    /// Disconnect from central system
    Disconnect,
    /// Reset charge point
    Reset,
}

/// Fault injection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultConfig {
    /// Enable random fault injection
    pub enable_random_faults: bool,
    /// Random fault probability (0.0 to 1.0)
    pub random_fault_probability: f64,
    /// Random fault check interval in seconds
    pub random_fault_interval_seconds: u64,
    /// Fault recovery time range in seconds
    pub recovery_time_range: (u64, u64),
    /// Available fault types with probabilities
    pub fault_types: HashMap<String, f64>,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log to file
    pub log_to_file: bool,
    /// Log file path
    pub log_file_path: Option<String>,
    /// Log to console
    pub log_to_console: bool,
    /// Log format
    pub log_format: LogFormat,
    /// Enable structured logging (JSON)
    pub structured: bool,
}

/// Log format options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    /// Human readable format
    Pretty,
    /// JSON format
    Json,
    /// Compact format
    Compact,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            charge_point_id: "SIM001".to_string(),
            central_system_url: "ws://localhost:8080".to_string(),
            vendor_info: ChargePointVendorInfo {
                charge_point_vendor: "OCPP-RS".to_string(),
                charge_point_model: "Simulator".to_string(),
                charge_point_serial_number: Some("SIM001".to_string()),
                charge_box_serial_number: Some("CB001".to_string()),
                firmware_version: Some("1.0.0".to_string()),
                iccid: None,
                imsi: None,
                meter_type: Some("Energy".to_string()),
                meter_serial_number: Some("MT001".to_string()),
            },
            connector_count: 2,
            heartbeat_interval: 300,
            meter_values_interval: 60,
            connection_retry_interval: 30,
            max_connection_retries: 10,
            auto_reconnect: true,
            transport_config: TransportConfig::default(),
            api_config: ApiConfig::default(),
            meter_config: MeterConfig::default(),
            scenario_config: ScenarioConfig::default(),
            fault_config: FaultConfig::default(),
            logging_config: LoggingConfig::default(),
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8081,
            enable_cors: true,
            api_key: None,
            enable_websocket_events: true,
            websocket_events_port: 8082,
        }
    }
}

impl Default for MeterConfig {
    fn default() -> Self {
        Self {
            initial_energy_wh: 0.0,
            base_power_w: 10.0,           // Standby power
            max_charging_power_w: 7360.0, // 32A * 230V
            voltage_v: 230.0,
            power_variation: 0.1, // 10% variation
            update_interval_seconds: 5,
            include_noise: true,
            noise_amplitude: 0.01, // 1% noise
            simulate_drift: true,
            drift_rate_per_hour: 0.1, // 0.1% per hour
        }
    }
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        let mut scenarios = HashMap::new();

        // Basic charging scenario
        scenarios.insert(
            "basic_charging".to_string(),
            ScenarioDefinition {
                name: "Basic Charging".to_string(),
                description: "Complete charging cycle from plug-in to plug-out".to_string(),
                steps: vec![
                    ScenarioStep {
                        name: "Plug in cable".to_string(),
                        action: ScenarioAction::PlugIn,
                        delay_seconds: 0,
                        connector_id: Some(1),
                        parameters: HashMap::new(),
                    },
                    ScenarioStep {
                        name: "Start transaction".to_string(),
                        action: ScenarioAction::StartTransaction,
                        delay_seconds: 2,
                        connector_id: Some(1),
                        parameters: [("id_tag".to_string(), "user123".into())].into(),
                    },
                    ScenarioStep {
                        name: "Charging duration".to_string(),
                        action: ScenarioAction::Wait,
                        delay_seconds: 0,
                        connector_id: None,
                        parameters: [("duration".to_string(), 30.into())].into(),
                    },
                    ScenarioStep {
                        name: "Stop transaction".to_string(),
                        action: ScenarioAction::StopTransaction,
                        delay_seconds: 0,
                        connector_id: Some(1),
                        parameters: HashMap::new(),
                    },
                    ScenarioStep {
                        name: "Plug out cable".to_string(),
                        action: ScenarioAction::PlugOut,
                        delay_seconds: 2,
                        connector_id: Some(1),
                        parameters: HashMap::new(),
                    },
                ],
                repeat: false,
                repeat_interval_seconds: None,
                parameters: HashMap::new(),
            },
        );

        // Fault during charging scenario
        scenarios.insert(
            "fault_during_charging".to_string(),
            ScenarioDefinition {
                name: "Fault During Charging".to_string(),
                description: "Charging session interrupted by fault".to_string(),
                steps: vec![
                    ScenarioStep {
                        name: "Plug in cable".to_string(),
                        action: ScenarioAction::PlugIn,
                        delay_seconds: 0,
                        connector_id: Some(1),
                        parameters: HashMap::new(),
                    },
                    ScenarioStep {
                        name: "Start transaction".to_string(),
                        action: ScenarioAction::StartTransaction,
                        delay_seconds: 2,
                        connector_id: Some(1),
                        parameters: [("id_tag".to_string(), "user123".into())].into(),
                    },
                    ScenarioStep {
                        name: "Charging for a while".to_string(),
                        action: ScenarioAction::Wait,
                        delay_seconds: 0,
                        connector_id: None,
                        parameters: [("duration".to_string(), 10.into())].into(),
                    },
                    ScenarioStep {
                        name: "Inject overcurrent fault".to_string(),
                        action: ScenarioAction::InjectFault,
                        delay_seconds: 0,
                        connector_id: Some(1),
                        parameters: [
                            ("error_code".to_string(), "OverCurrentFailure".into()),
                            ("info".to_string(), "Simulated overcurrent".into()),
                        ]
                        .into(),
                    },
                    ScenarioStep {
                        name: "Wait for fault handling".to_string(),
                        action: ScenarioAction::Wait,
                        delay_seconds: 0,
                        connector_id: None,
                        parameters: [("duration".to_string(), 5.into())].into(),
                    },
                    ScenarioStep {
                        name: "Clear fault".to_string(),
                        action: ScenarioAction::ClearFault,
                        delay_seconds: 0,
                        connector_id: Some(1),
                        parameters: HashMap::new(),
                    },
                    ScenarioStep {
                        name: "Plug out cable".to_string(),
                        action: ScenarioAction::PlugOut,
                        delay_seconds: 2,
                        connector_id: Some(1),
                        parameters: HashMap::new(),
                    },
                ],
                repeat: false,
                repeat_interval_seconds: None,
                parameters: HashMap::new(),
            },
        );

        Self {
            auto_execute: false,
            default_scenario: None,
            execution_interval_seconds: 60,
            scenarios,
        }
    }
}

impl Default for FaultConfig {
    fn default() -> Self {
        let mut fault_types = HashMap::new();
        fault_types.insert("OverCurrentFailure".to_string(), 0.3);
        fault_types.insert("OverVoltage".to_string(), 0.2);
        fault_types.insert("UnderVoltage".to_string(), 0.2);
        fault_types.insert("HighTemperature".to_string(), 0.1);
        fault_types.insert("ConnectorLockFailure".to_string(), 0.1);
        fault_types.insert("PowerMeterFailure".to_string(), 0.05);
        fault_types.insert("OtherError".to_string(), 0.05);

        Self {
            enable_random_faults: false,
            random_fault_probability: 0.01, // 1% chance per check
            random_fault_interval_seconds: 60,
            recovery_time_range: (30, 300), // 30 seconds to 5 minutes
            fault_types,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            log_to_file: false,
            log_file_path: Some("simulator.log".to_string()),
            log_to_console: true,
            log_format: LogFormat::Pretty,
            structured: false,
        }
    }
}

impl SimulatorConfig {
    /// Load configuration from file
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = if path.ends_with(".toml") {
            toml::from_str(&content)?
        } else if path.ends_with(".json") {
            serde_json::from_str(&content)?
        } else {
            anyhow::bail!("Unsupported configuration file format. Use .toml or .json");
        };
        Ok(config)
    }

    /// Save configuration to file
    pub fn to_file(&self, path: &str) -> anyhow::Result<()> {
        let content = if path.ends_with(".toml") {
            toml::to_string_pretty(self)?
        } else if path.ends_with(".json") {
            serde_json::to_string_pretty(self)?
        } else {
            anyhow::bail!("Unsupported configuration file format. Use .toml or .json");
        };
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Create configuration for multiple charge points
    pub fn create_multi_cp_config(base: &Self, count: u32) -> Vec<Self> {
        (1..=count)
            .map(|i| {
                let mut config = base.clone();
                config.charge_point_id = format!("{}_CP{:03}", base.charge_point_id, i);
                config.api_config.port = base.api_config.port + i as u16;
                config.api_config.websocket_events_port =
                    base.api_config.websocket_events_port + i as u16;
                config
            })
            .collect()
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.charge_point_id.is_empty() {
            return Err("Charge point ID cannot be empty".to_string());
        }

        if self.connector_count == 0 {
            return Err("Connector count must be greater than 0".to_string());
        }

        if self.connector_count > 100 {
            return Err("Connector count cannot exceed 100".to_string());
        }

        if self.heartbeat_interval == 0 {
            return Err("Heartbeat interval must be greater than 0".to_string());
        }

        if self.meter_values_interval == 0 {
            return Err("Meter values interval must be greater than 0".to_string());
        }

        if !self.central_system_url.starts_with("ws://")
            && !self.central_system_url.starts_with("wss://")
        {
            return Err("Central system URL must be a valid WebSocket URL".to_string());
        }

        if self.meter_config.max_charging_power_w <= 0.0 {
            return Err("Maximum charging power must be greater than 0".to_string());
        }

        if self.meter_config.voltage_v <= 0.0 {
            return Err("Voltage must be greater than 0".to_string());
        }

        if self.api_config.port == 0 {
            return Err("API port must be greater than 0".to_string());
        }

        Ok(())
    }

    /// Get scenario by name
    pub fn get_scenario(&self, name: &str) -> Option<&ScenarioDefinition> {
        self.scenario_config.scenarios.get(name)
    }

    /// Add scenario
    pub fn add_scenario(&mut self, scenario: ScenarioDefinition) {
        self.scenario_config
            .scenarios
            .insert(scenario.name.clone(), scenario);
    }

    /// Remove scenario
    pub fn remove_scenario(&mut self, name: &str) -> Option<ScenarioDefinition> {
        self.scenario_config.scenarios.remove(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SimulatorConfig::default();
        assert_eq!(config.charge_point_id, "SIM001");
        assert_eq!(config.connector_count, 2);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = SimulatorConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Empty charge point ID should fail
        config.charge_point_id = "".to_string();
        assert!(config.validate().is_err());
        config.charge_point_id = "TEST".to_string();

        // Zero connectors should fail
        config.connector_count = 0;
        assert!(config.validate().is_err());
        config.connector_count = 2;

        // Invalid URL should fail
        config.central_system_url = "http://example.com".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_multi_cp_config() {
        let base_config = SimulatorConfig::default();
        let configs = SimulatorConfig::create_multi_cp_config(&base_config, 3);

        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0].charge_point_id, "SIM001_CP001");
        assert_eq!(configs[1].charge_point_id, "SIM001_CP002");
        assert_eq!(configs[2].charge_point_id, "SIM001_CP003");

        assert_eq!(configs[0].api_config.port, 8082);
        assert_eq!(configs[1].api_config.port, 8083);
        assert_eq!(configs[2].api_config.port, 8084);
    }

    #[test]
    fn test_scenario_operations() {
        let mut config = SimulatorConfig::default();

        // Should have default scenarios
        assert!(config.get_scenario("basic_charging").is_some());

        // Test adding custom scenario
        let custom_scenario = ScenarioDefinition {
            name: "custom_test".to_string(),
            description: "Custom test scenario".to_string(),
            steps: vec![],
            repeat: false,
            repeat_interval_seconds: None,
            parameters: HashMap::new(),
        };

        config.add_scenario(custom_scenario.clone());
        assert!(config.get_scenario("custom_test").is_some());

        // Test removing scenario
        let removed = config.remove_scenario("custom_test");
        assert!(removed.is_some());
        assert!(config.get_scenario("custom_test").is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = SimulatorConfig::default();

        // Test JSON serialization
        let json_str = serde_json::to_string(&config).unwrap();
        let deserialized: SimulatorConfig = serde_json::from_str(&json_str).unwrap();
        assert_eq!(config.charge_point_id, deserialized.charge_point_id);

        // Test TOML serialization
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: SimulatorConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.charge_point_id, deserialized.charge_point_id);
    }
}
