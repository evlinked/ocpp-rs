//! # CLI Configuration Module
//!
//! This module provides configuration structures and utilities for the OCPP CLI tool.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// CLI configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliConfig {
    /// Connection settings
    pub connection: ConnectionConfig,
    /// Test settings
    pub test: TestConfig,
    /// Logging settings
    pub logging: LoggingConfig,
}

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Central System WebSocket URL
    pub central_system_url: String,
    /// Charge Point identifier
    pub charge_point_id: String,
    /// Connection timeout in seconds
    pub timeout_seconds: u64,
    /// Number of reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Reconnect delay in seconds
    pub reconnect_delay_seconds: u64,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            central_system_url: "ws://localhost:8080/ocpp/CP001".to_string(),
            charge_point_id: "CP001".to_string(),
            timeout_seconds: 30,
            max_reconnect_attempts: 5,
            reconnect_delay_seconds: 5,
        }
    }
}

/// Test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Default test suite
    pub default_suite: String,
    /// Test duration in seconds
    pub duration_seconds: u64,
    /// Number of concurrent charge points for load testing
    pub concurrent_charge_points: u32,
    /// Enable fault injection testing
    pub enable_fault_injection: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            default_suite: "basic".to_string(),
            duration_seconds: 300,
            concurrent_charge_points: 1,
            enable_fault_injection: false,
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Enable verbose logging
    pub verbose: bool,
    /// Log format
    pub format: String,
    /// Log to file
    pub log_to_file: bool,
    /// Log file path
    pub log_file_path: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            verbose: false,
            format: "compact".to_string(),
            log_to_file: false,
            log_file_path: None,
        }
    }
}

impl CliConfig {
    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = if content.trim().starts_with('{') {
            serde_json::from_str::<Self>(&content)?
        } else {
            toml::from_str::<Self>(&content)?
        };
        Ok(config)
    }

    /// Save configuration to file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let content = if path.extension().and_then(|s| s.to_str()) == Some("json") {
            serde_json::to_string_pretty(self)?
        } else {
            toml::to_string_pretty(self)?
        };
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.connection.charge_point_id.is_empty() {
            return Err(anyhow::anyhow!("Charge point ID cannot be empty"));
        }

        if !self.connection.central_system_url.starts_with("ws://")
            && !self.connection.central_system_url.starts_with("wss://")
        {
            return Err(anyhow::anyhow!(
                "Central system URL must be a WebSocket URL"
            ));
        }

        if self.connection.timeout_seconds == 0 {
            return Err(anyhow::anyhow!("Connection timeout must be greater than 0"));
        }

        if !["trace", "debug", "info", "warn", "error"].contains(&self.logging.level.as_str()) {
            return Err(anyhow::anyhow!("Invalid log level: {}", self.logging.level));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert_eq!(config.connection.charge_point_id, "CP001");
        assert_eq!(config.test.default_suite, "basic");
        assert_eq!(config.logging.level, "info");
    }

    #[test]
    fn test_config_validation() {
        let mut config = CliConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Empty charge point ID should fail
        config.connection.charge_point_id = "".to_string();
        assert!(config.validate().is_err());
        config.connection.charge_point_id = "TEST".to_string();

        // Invalid URL should fail
        config.connection.central_system_url = "http://example.com".to_string();
        assert!(config.validate().is_err());
        config.connection.central_system_url = "wss://example.com".to_string();

        // Zero timeout should fail
        config.connection.timeout_seconds = 0;
        assert!(config.validate().is_err());
        config.connection.timeout_seconds = 30;

        // Invalid log level should fail
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_serialization() {
        let config = CliConfig::default();

        // Test TOML serialization
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: CliConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            config.connection.charge_point_id,
            deserialized.connection.charge_point_id
        );

        // Test JSON serialization
        let json_str = serde_json::to_string(&config).unwrap();
        let deserialized: CliConfig = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            config.connection.charge_point_id,
            deserialized.connection.charge_point_id
        );
    }
}
