//! # OCPP Central System Management System (CSMS)
//!
//! This crate provides a complete implementation of an OCPP Central System Management System (CSMS)
//! for managing electric vehicle charging infrastructure. It supports OCPP 1.6J protocol
//! with plans for OCPP 2.0.1 support.
//!
//! ## Features
//!
//! - **Core Profile**: Complete implementation of OCPP 1.6J Core Profile
//! - **Transaction Management**: Handle charging sessions with full lifecycle support
//! - **Authentication**: ID tag management and authorization
//! - **Configuration Management**: Remote configuration of charge points
//! - **Firmware Management**: Remote firmware updates
//! - **Smart Charging**: Charging profile management
//! - **Reservations**: Connector reservation system
//! - **Diagnostics**: Remote diagnostics and monitoring
//! - **REST API**: HTTP API for external integration
//! - **WebSocket Server**: High-performance WebSocket server for charge point connections
//! - **Database Integration**: PostgreSQL support with migrations
//! - **Metrics & Monitoring**: Prometheus metrics and health checks
//! - **Distributed Tracing**: OpenTelemetry integration

pub mod auth;
pub mod config;
pub mod database;
pub mod error;
pub mod handlers;
pub mod manager;
pub mod metrics;
pub mod server;
pub mod state;

pub use error::*;
pub use manager::*;

use ocpp_messages::Message;
use ocpp_transport::{MessageHandler, TransportEvent};
use ocpp_types::OcppResult;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// CSMS configuration
#[derive(Debug, Clone, Default)]
pub struct CsmsConfig {
    /// Database configuration
    pub database: database::DatabaseConfig,
    /// Server configuration
    pub server: server::ServerConfig,
    /// Authentication configuration
    pub auth: auth::AuthConfig,
    /// Metrics configuration
    pub metrics: metrics::MetricsConfig,
}

/// Central System Management System
pub struct Csms {
    /// Configuration
    config: CsmsConfig,
    /// Charge point manager
    manager: Arc<RwLock<ChargePointManager>>,
    /// Database pool
    database: Arc<database::DatabasePool>,
    /// Metrics registry
    metrics: Arc<metrics::MetricsRegistry>,
    /// Server handle
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Csms {
    /// Create a new CSMS instance
    pub async fn new(config: CsmsConfig) -> CsmsResult<Self> {
        info!("Initializing OCPP CSMS");

        // Initialize database
        let database = Arc::new(database::DatabasePool::new(&config.database).await?);

        // Run migrations
        database.migrate().await?;

        // Initialize metrics
        let metrics = Arc::new(metrics::MetricsRegistry::new(&config.metrics)?);

        // Initialize charge point manager
        let manager = Arc::new(RwLock::new(ChargePointManager::new(
            database.clone(),
            metrics.clone(),
        )));

        Ok(Self {
            config,
            manager,
            database,
            metrics,
            server_handle: None,
        })
    }

    /// Start the CSMS server
    pub async fn start(&mut self) -> CsmsResult<()> {
        info!(
            "Starting OCPP CSMS server on {}",
            self.config.server.bind_address
        );

        let server = server::CsmsServer::new(
            self.config.server.clone(),
            self.manager.clone(),
            self.database.clone(),
            self.metrics.clone(),
        );

        let handle = tokio::spawn(async move {
            if let Err(e) = server.run().await {
                error!("CSMS server error: {}", e);
            }
        });

        self.server_handle = Some(handle);
        info!("CSMS server started successfully");
        Ok(())
    }

    /// Stop the CSMS server
    pub async fn stop(&mut self) -> CsmsResult<()> {
        info!("Stopping OCPP CSMS server");

        if let Some(handle) = self.server_handle.take() {
            handle.abort();
            if let Err(e) = handle.await {
                if !e.is_cancelled() {
                    warn!("Error stopping server: {}", e);
                }
            }
        }

        // Close database connections
        self.database.close().await;

        info!("CSMS server stopped");
        Ok(())
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> CsmsStats {
        let manager = self.manager.read().await;
        let db_stats = self.database.get_stats().await;
        let metrics_stats = self.metrics.get_stats();

        CsmsStats {
            active_charge_points: manager.active_count(),
            total_charge_points: manager.total_count(),
            active_transactions: manager.active_transactions(),
            database_connections: db_stats.active_connections,
            uptime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default(),
            metrics_collected: metrics_stats.metrics_count,
        }
    }

    /// Get charge point manager
    pub fn manager(&self) -> Arc<RwLock<ChargePointManager>> {
        self.manager.clone()
    }

    /// Get database pool
    pub fn database(&self) -> Arc<database::DatabasePool> {
        self.database.clone()
    }

    /// Get metrics registry
    pub fn metrics(&self) -> Arc<metrics::MetricsRegistry> {
        self.metrics.clone()
    }

    /// Health check
    pub async fn health_check(&self) -> CsmsResult<HealthStatus> {
        let mut status = HealthStatus {
            healthy: true,
            checks: Vec::new(),
        };

        // Database health check
        match self.database.health_check().await {
            Ok(()) => status.checks.push(HealthCheck {
                name: "database".to_string(),
                healthy: true,
                message: "Database connection healthy".to_string(),
            }),
            Err(e) => {
                status.healthy = false;
                status.checks.push(HealthCheck {
                    name: "database".to_string(),
                    healthy: false,
                    message: format!("Database connection failed: {}", e),
                });
            }
        }

        // Manager health check
        let manager = self.manager.read().await;
        if manager.is_healthy() {
            status.checks.push(HealthCheck {
                name: "manager".to_string(),
                healthy: true,
                message: "Charge point manager healthy".to_string(),
            });
        } else {
            status.healthy = false;
            status.checks.push(HealthCheck {
                name: "manager".to_string(),
                healthy: false,
                message: "Charge point manager unhealthy".to_string(),
            });
        }

        // Metrics health check
        if self.metrics.is_healthy() {
            status.checks.push(HealthCheck {
                name: "metrics".to_string(),
                healthy: true,
                message: "Metrics system healthy".to_string(),
            });
        } else {
            status.healthy = false;
            status.checks.push(HealthCheck {
                name: "metrics".to_string(),
                healthy: false,
                message: "Metrics system unhealthy".to_string(),
            });
        }

        Ok(status)
    }
}

/// CSMS statistics
#[derive(Debug, Clone)]
pub struct CsmsStats {
    /// Number of active charge points
    pub active_charge_points: usize,
    /// Total number of registered charge points
    pub total_charge_points: usize,
    /// Number of active transactions
    pub active_transactions: usize,
    /// Number of active database connections
    pub database_connections: usize,
    /// Server uptime
    pub uptime: std::time::Duration,
    /// Number of metrics collected
    pub metrics_collected: usize,
}

/// Health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// Overall health status
    pub healthy: bool,
    /// Individual health checks
    pub checks: Vec<HealthCheck>,
}

/// Individual health check result
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// Check name
    pub name: String,
    /// Health status
    pub healthy: bool,
    /// Status message
    pub message: String,
}

/// CSMS message handler implementation
#[async_trait::async_trait]
impl MessageHandler for Csms {
    async fn handle_message(&self, message: Message) -> OcppResult<Option<Message>> {
        debug!("Handling OCPP message: {}", message.unique_id());

        let manager = self.manager.read().await;
        manager.handle_message(message).await
    }

    async fn handle_event(&self, event: TransportEvent) {
        debug!("Handling transport event: {:?}", event);

        let manager = self.manager.write().await;
        manager.handle_event(event).await;
    }
}

/// Utility functions
pub mod utils {
    use super::*;

    /// Load configuration from file
    pub fn load_config(_path: &str) -> CsmsResult<CsmsConfig> {
        // For now, return default config
        // TODO: Implement actual config loading from file
        Ok(CsmsConfig::default())
    }

    /// Initialize tracing
    pub fn init_tracing(level: &str) -> CsmsResult<()> {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| format!("ocpp_csms={},ocpp={}", level, level).into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_csms_creation() {
        let config = CsmsConfig::default();
        // This will fail without a database, but tests the basic structure
        let result = Csms::new(config).await;
        // In real tests, we'd use a test database or mock
        assert!(result.is_err()); // Expected to fail without database
    }

    #[test]
    fn test_csms_stats_default() {
        let stats = CsmsStats {
            active_charge_points: 0,
            total_charge_points: 0,
            active_transactions: 0,
            database_connections: 0,
            uptime: std::time::Duration::from_secs(0),
            metrics_collected: 0,
        };

        assert_eq!(stats.active_charge_points, 0);
        assert_eq!(stats.total_charge_points, 0);
    }

    #[test]
    fn test_health_status() {
        let status = HealthStatus {
            healthy: true,
            checks: vec![HealthCheck {
                name: "test".to_string(),
                healthy: true,
                message: "OK".to_string(),
            }],
        };

        assert!(status.healthy);
        assert_eq!(status.checks.len(), 1);
        assert_eq!(status.checks[0].name, "test");
    }
}
