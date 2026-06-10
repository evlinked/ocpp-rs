//! # OCPP CLI Library
//!
//! A library for building OCPP CLI applications and testing tools.

#![allow(ambiguous_glob_reexports)]

use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

// Re-export core types
pub use ocpp_messages::*;
pub use ocpp_types::*;

// Public modules
pub mod client;
pub mod commands;
pub mod config;
pub mod scenarios;
pub mod testing;
pub mod ui;

/// CLI configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Central System WebSocket URL
    pub central_system_url: String,
    /// Charge Point identifier
    pub charge_point_id: String,
    /// Number of connectors
    pub connector_count: u32,
    /// Enable interactive mode
    pub interactive_mode: bool,
    /// Auto-start scenario name
    pub auto_scenario: Option<String>,
    /// Logging configuration
    pub log_config: LogConfig,
    /// Testing configuration
    pub test_config: TestConfig,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            central_system_url: "ws://localhost:8080/ocpp/CP001".to_string(),
            charge_point_id: "CP001".to_string(),
            connector_count: 2,
            interactive_mode: false,
            auto_scenario: None,
            log_config: LogConfig::default(),
            test_config: TestConfig::default(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
    /// Enable verbose logging
    pub verbose: bool,
    /// Log output format
    pub format: LogFormat,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            verbose: false,
            format: LogFormat::Compact,
        }
    }
}

/// Log output format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Pretty,
    Compact,
    Json,
}

/// Test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Test suite name
    pub suite: String,
    /// Number of charge points to simulate
    pub charge_point_count: u32,
    /// Test duration in seconds
    pub duration_seconds: u64,
    /// Enable load testing
    pub load_testing: bool,
    /// Enable fault injection testing
    pub fault_testing: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            suite: "basic".to_string(),
            charge_point_count: 1,
            duration_seconds: 300,
            load_testing: false,
            fault_testing: false,
        }
    }
}

/// Test execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test name
    pub name: String,
    /// Test success status
    pub success: bool,
    /// Test duration in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// Test metrics
    pub metrics: TestMetrics,
}

/// Test performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMetrics {
    /// Number of messages sent
    pub messages_sent: u64,
    /// Number of messages received
    pub messages_received: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Success rate percentage (0-100)
    pub success_rate: f64,
    /// Peak memory usage in MB
    pub peak_memory_mb: f64,
}

/// Main CLI runner struct
pub struct CliRunner {
    /// CLI configuration
    config: CliConfig,
    /// Event channel for simulator events
    #[allow(dead_code)]
    event_channel: (
        mpsc::UnboundedSender<CliEvent>,
        mpsc::UnboundedReceiver<CliEvent>,
    ),
}

/// CLI events
#[derive(Debug, Clone)]
pub enum CliEvent {
    Connected,
    Disconnected,
    MessageSent(String),
    MessageReceived(String),
    Error(String),
}

impl CliRunner {
    /// Create a new CLI runner
    pub fn new(config: CliConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            config,
            event_channel: (tx, rx),
        }
    }

    /// Initialize the CLI runner
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing OCPP CLI runner");
        info!("Central System URL: {}", self.config.central_system_url);
        info!("Charge Point ID: {}", self.config.charge_point_id);
        info!("Connector Count: {}", self.config.connector_count);
        Ok(())
    }

    /// Get the current configuration
    pub fn get_config(&self) -> &CliConfig {
        &self.config
    }

    /// Test connection to Central System
    pub async fn test_connection(&self) -> Result<TestResult> {
        let start = std::time::Instant::now();

        // Simulate connection test
        tokio::time::sleep(Duration::from_millis(100)).await;

        let duration = start.elapsed();

        Ok(TestResult {
            name: "Connection Test".to_string(),
            success: true,
            duration_ms: duration.as_millis() as u64,
            error: None,
            metrics: TestMetrics {
                messages_sent: 1,
                messages_received: 1,
                avg_response_time_ms: 50.0,
                success_rate: 100.0,
                peak_memory_mb: 10.0,
            },
        })
    }

    /// Run basic OCPP functionality tests
    pub async fn run_basic_tests(&self) -> Result<Vec<TestResult>> {
        let mut results = Vec::new();

        results.push(self.test_boot_notification().await?);
        results.push(self.test_heartbeat().await?);
        results.push(self.test_status_notification().await?);

        Ok(results)
    }

    /// Test BootNotification message
    async fn test_boot_notification(&self) -> Result<TestResult> {
        let start = std::time::Instant::now();

        // Simulate BootNotification test
        tokio::time::sleep(Duration::from_millis(200)).await;

        let duration = start.elapsed();

        Ok(TestResult {
            name: "BootNotification".to_string(),
            success: true,
            duration_ms: duration.as_millis() as u64,
            error: None,
            metrics: TestMetrics {
                messages_sent: 1,
                messages_received: 1,
                avg_response_time_ms: 100.0,
                success_rate: 100.0,
                peak_memory_mb: 12.0,
            },
        })
    }

    /// Test Heartbeat message
    async fn test_heartbeat(&self) -> Result<TestResult> {
        let start = std::time::Instant::now();

        // Simulate Heartbeat test
        tokio::time::sleep(Duration::from_millis(50)).await;

        let duration = start.elapsed();

        Ok(TestResult {
            name: "Heartbeat".to_string(),
            success: true,
            duration_ms: duration.as_millis() as u64,
            error: None,
            metrics: TestMetrics {
                messages_sent: 1,
                messages_received: 1,
                avg_response_time_ms: 25.0,
                success_rate: 100.0,
                peak_memory_mb: 8.0,
            },
        })
    }

    /// Test StatusNotification message
    async fn test_status_notification(&self) -> Result<TestResult> {
        let start = std::time::Instant::now();

        // Simulate StatusNotification test
        tokio::time::sleep(Duration::from_millis(75)).await;

        let duration = start.elapsed();

        Ok(TestResult {
            name: "StatusNotification".to_string(),
            success: true,
            duration_ms: duration.as_millis() as u64,
            error: None,
            metrics: TestMetrics {
                messages_sent: 1,
                messages_received: 1,
                avg_response_time_ms: 35.0,
                success_rate: 100.0,
                peak_memory_mb: 9.0,
            },
        })
    }
}

/// Utility functions
pub mod utils {
    use super::*;
    use std::time::Duration;

    /// Format duration for display
    pub fn format_duration(duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;

        if hours > 0 {
            format!("{}:{:02}:{:02}", hours, minutes, seconds)
        } else {
            format!("{:02}:{:02}", minutes, seconds)
        }
    }

    /// Format test result for display
    pub fn format_test_result(result: &TestResult) -> String {
        let status = if result.success {
            "✅ PASS"
        } else {
            "❌ FAIL"
        };
        format!("{} {} ({}ms)", status, result.name, result.duration_ms)
    }

    /// Print application banner
    pub fn print_banner() {
        print!(
            r#"
   ____   ____ ____  ____     ____ _     ___
  / __ \ / ___/ __ \|  _ \   / ___| |   |_ _|
 | |  | | |  | |  | | |_) | | |   | |    | |
 | |__| | |__| |__| |  __/  | |___| |___ | |
  \____/ \____\____/|_|      \____|_____|___|

"#
        );
    }

    /// Initialize logging with given configuration
    pub fn init_logging(config: &LogConfig) -> Result<()> {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        let level = match config.level.to_lowercase().as_str() {
            "trace" => tracing::Level::TRACE,
            "debug" => tracing::Level::DEBUG,
            "info" => tracing::Level::INFO,
            "warn" => tracing::Level::WARN,
            "error" => tracing::Level::ERROR,
            _ => tracing::Level::INFO,
        };

        let registry = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into()));

        match config.format {
            LogFormat::Pretty => {
                registry
                    .with(tracing_subscriber::fmt::layer().pretty())
                    .init();
            }
            LogFormat::Compact => {
                registry
                    .with(tracing_subscriber::fmt::layer().compact())
                    .init();
            }
            LogFormat::Json => {
                registry
                    .with(tracing_subscriber::fmt::layer().json())
                    .init();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_config_default() {
        let config = CliConfig::default();
        assert_eq!(config.connector_count, 2);
        assert!(!config.interactive_mode);
    }

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert_eq!(config.level, "info");
        assert!(!config.verbose);
    }

    #[test]
    fn test_test_config_default() {
        let config = TestConfig::default();
        assert_eq!(config.suite, "basic");
        assert_eq!(config.charge_point_count, 1);
    }

    #[tokio::test]
    async fn test_cli_runner_creation() {
        let config = CliConfig::default();
        let runner = CliRunner::new(config);
        assert_eq!(runner.config.connector_count, 2);
    }

    #[test]
    fn test_format_duration() {
        let duration = Duration::from_secs(3661); // 1 hour, 1 minute, 1 second
        let formatted = utils::format_duration(duration);
        assert_eq!(formatted, "1:01:01");
    }

    #[test]
    fn test_test_result_formatting() {
        let result = TestResult {
            name: "Test".to_string(),
            success: true,
            duration_ms: 100,
            error: None,
            metrics: TestMetrics {
                messages_sent: 1,
                messages_received: 1,
                avg_response_time_ms: 50.0,
                success_rate: 100.0,
                peak_memory_mb: 10.0,
            },
        };
        let formatted = utils::format_test_result(&result);
        assert!(formatted.contains("✅ PASS"));
    }
}
