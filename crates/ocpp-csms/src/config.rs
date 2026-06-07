//! Configuration management for OCPP CSMS

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CSMS configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Metrics configuration
    pub metrics: MetricsConfig,
    /// OCPP protocol configuration
    pub ocpp: OcppConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server bind address
    pub bind_address: String,
    /// WebSocket port
    pub websocket_port: u16,
    /// HTTP API port
    pub http_port: u16,
    /// Metrics port
    pub metrics_port: u16,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Enable TLS
    pub tls_enabled: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<PathBuf>,
    /// TLS private key path
    pub tls_key_path: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            websocket_port: 8080,
            http_port: 3000,
            metrics_port: 9090,
            max_connections: 1000,
            connection_timeout: 30,
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url: String,
    /// Maximum connection pool size
    pub max_connections: u32,
    /// Minimum connection pool size
    pub min_connections: u32,
    /// Connection timeout in seconds
    pub connect_timeout: u64,
    /// Query timeout in seconds
    pub query_timeout: u64,
    /// Enable automatic migrations
    pub auto_migrate: bool,
    /// Enable SQL statement logging
    pub log_statements: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://ocpp_user:ocpp_password@localhost:5432/ocpp_dev".to_string(),
            max_connections: 16,
            min_connections: 1,
            connect_timeout: 30,
            query_timeout: 60,
            auto_migrate: true,
            log_statements: false,
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Enable authentication
    pub enabled: bool,
    /// JWT secret key
    pub jwt_secret: String,
    /// JWT expiration time in seconds
    pub jwt_expiry: u64,
    /// Enable API key authentication
    pub api_key_enabled: bool,
    /// Enable rate limiting
    pub rate_limiting_enabled: bool,
    /// Rate limit: requests per minute
    pub rate_limit_rpm: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            jwt_secret: "your-super-secret-jwt-key-change-this-in-production".to_string(),
            jwt_expiry: 86400, // 24 hours
            api_key_enabled: false,
            rate_limiting_enabled: true,
            rate_limit_rpm: 1000,
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log format (json, pretty, compact)
    pub format: String,
    /// Enable file logging
    pub file_enabled: bool,
    /// Log file path
    pub file_path: Option<PathBuf>,
    /// Maximum log file size in MB
    pub max_file_size: u64,
    /// Number of log files to keep
    pub max_files: u32,
    /// Enable console logging
    pub console_enabled: bool,
    /// Enable structured logging
    pub structured: bool,
    /// Enable tracing
    pub tracing_enabled: bool,
    /// Jaeger endpoint for tracing
    pub jaeger_endpoint: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            file_enabled: true,
            file_path: Some(PathBuf::from("./logs/ocpp-csms.log")),
            max_file_size: 100,
            max_files: 10,
            console_enabled: true,
            structured: false,
            tracing_enabled: true,
            jaeger_endpoint: Some("http://localhost:14268/api/traces".to_string()),
        }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable Prometheus metrics
    pub enabled: bool,
    /// Metrics namespace
    pub namespace: String,
    /// Collection interval in seconds
    pub collection_interval: u64,
    /// Enable detailed metrics
    pub detailed_metrics: bool,
    /// Histogram buckets for latency metrics
    pub latency_buckets: Vec<f64>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            namespace: "ocpp_csms".to_string(),
            collection_interval: 15,
            detailed_metrics: true,
            latency_buckets: vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
        }
    }
}

/// OCPP protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcppConfig {
    /// OCPP version
    pub version: String,
    /// Supported sub-protocols
    pub supported_protocols: Vec<String>,
    /// Message timeout in seconds
    pub message_timeout: u64,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Enable message validation
    pub validate_messages: bool,
    /// Enable strict schema validation
    pub strict_validation: bool,
    /// Supported feature profiles
    pub supported_features: Vec<String>,
}

impl Default for OcppConfig {
    fn default() -> Self {
        Self {
            version: "1.6J".to_string(),
            supported_protocols: vec!["ocpp1.6".to_string()],
            message_timeout: 30,
            heartbeat_interval: 60,
            max_message_size: 65536,
            validate_messages: true,
            strict_validation: true,
            supported_features: vec![
                "Core".to_string(),
                "FirmwareManagement".to_string(),
                "LocalAuthListManagement".to_string(),
                "Reservation".to_string(),
                "SmartCharging".to_string(),
                "RemoteTrigger".to_string(),
            ],
        }
    }
}

/// Configuration builder for easier setup
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    /// Create a new config builder
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Set server configuration
    pub fn server(mut self, server: ServerConfig) -> Self {
        self.config.server = server;
        self
    }

    /// Set database configuration
    pub fn database(mut self, database: DatabaseConfig) -> Self {
        self.config.database = database;
        self
    }

    /// Set authentication configuration
    pub fn auth(mut self, auth: AuthConfig) -> Self {
        self.config.auth = auth;
        self
    }

    /// Set logging configuration
    pub fn logging(mut self, logging: LoggingConfig) -> Self {
        self.config.logging = logging;
        self
    }

    /// Set metrics configuration
    pub fn metrics(mut self, metrics: MetricsConfig) -> Self {
        self.config.metrics = metrics;
        self
    }

    /// Set OCPP configuration
    pub fn ocpp(mut self, ocpp: OcppConfig) -> Self {
        self.config.ocpp = ocpp;
        self
    }

    /// Build the configuration
    pub fn build(self) -> Config {
        self.config
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Load configuration from file
pub fn load_from_file(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

/// Load configuration from environment variables
pub fn load_from_env() -> Config {
    let mut config = Config::default();

    // Server configuration from environment
    if let Ok(addr) = std::env::var("OCPP_BIND_ADDRESS") {
        config.server.bind_address = addr;
    }
    if let Ok(port) = std::env::var("OCPP_WEBSOCKET_PORT") {
        if let Ok(port) = port.parse() {
            config.server.websocket_port = port;
        }
    }
    if let Ok(port) = std::env::var("OCPP_HTTP_PORT") {
        if let Ok(port) = port.parse() {
            config.server.http_port = port;
        }
    }

    // Database configuration from environment
    if let Ok(url) = std::env::var("DATABASE_URL") {
        config.database.url = url;
    }

    // Authentication configuration from environment
    if let Ok(secret) = std::env::var("JWT_SECRET") {
        config.auth.jwt_secret = secret;
    }

    config
}

/// Validate configuration
pub fn validate_config(config: &Config) -> Result<(), String> {
    // Validate server configuration
    if config.server.websocket_port == 0 {
        return Err("WebSocket port cannot be 0".to_string());
    }
    if config.server.http_port == 0 {
        return Err("HTTP port cannot be 0".to_string());
    }
    if config.server.max_connections == 0 {
        return Err("Max connections cannot be 0".to_string());
    }

    // Validate database configuration
    if config.database.url.is_empty() {
        return Err("Database URL cannot be empty".to_string());
    }
    if config.database.max_connections == 0 {
        return Err("Database max connections cannot be 0".to_string());
    }

    // Validate authentication configuration
    if config.auth.enabled && config.auth.jwt_secret.len() < 32 {
        return Err("JWT secret must be at least 32 characters long".to_string());
    }

    // Validate OCPP configuration
    if config.ocpp.supported_protocols.is_empty() {
        return Err("At least one OCPP protocol must be supported".to_string());
    }
    if config.ocpp.max_message_size == 0 {
        return Err("Max message size cannot be 0".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.websocket_port, 8080);
        assert_eq!(config.database.max_connections, 16);
        assert!(config.auth.enabled);
        assert_eq!(config.ocpp.version, "1.6J");
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .server(ServerConfig {
                websocket_port: 9000,
                ..Default::default()
            })
            .build();

        assert_eq!(config.server.websocket_port, 9000);
    }

    #[test]
    fn test_config_validation() {
        let config = Config::default();
        assert!(validate_config(&config).is_ok());

        let mut invalid_config = Config::default();
        invalid_config.server.websocket_port = 0;
        assert!(validate_config(&invalid_config).is_err());
    }

    #[test]
    fn test_load_from_env() {
        std::env::set_var("OCPP_WEBSOCKET_PORT", "9999");
        let config = load_from_env();
        assert_eq!(config.server.websocket_port, 9999);
        std::env::remove_var("OCPP_WEBSOCKET_PORT");
    }
}
