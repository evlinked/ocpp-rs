//! Database module for OCPP CSMS

use crate::{CsmsError, CsmsResult};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::time::Duration;
use tracing::{debug, info};

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

/// Database pool wrapper
pub struct DatabasePool {
    /// SQLx connection pool
    pool: Pool<Postgres>,
    /// Configuration
    config: DatabaseConfig,
}

impl DatabasePool {
    /// Create new database pool
    pub async fn new(config: &DatabaseConfig) -> CsmsResult<Self> {
        info!("Connecting to database: {}", mask_url(&config.url));

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.connect_timeout))
            .connect(&config.url)
            .await?;

        Ok(Self {
            pool,
            config: config.clone(),
        })
    }

    /// Run database migrations
    pub async fn migrate(&self) -> CsmsResult<()> {
        if self.config.auto_migrate {
            info!("Running database migrations");
            // TODO: Implement actual migrations
            // sqlx::migrate!("./migrations").run(&self.pool).await?;
        }
        Ok(())
    }

    /// Get database pool reference
    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    /// Health check
    pub async fn health_check(&self) -> CsmsResult<()> {
        debug!("Performing database health check");
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CsmsError::Database {
                message: format!("Health check failed: {}", e),
            })?;
        Ok(())
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> DatabaseStats {
        DatabaseStats {
            active_connections: self.pool.size() as usize,
            idle_connections: self.pool.num_idle(),
            total_connections: self.config.max_connections as usize,
        }
    }

    /// Close database connections
    pub async fn close(&self) {
        info!("Closing database connections");
        self.pool.close().await;
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    /// Number of active connections
    pub active_connections: usize,
    /// Number of idle connections
    pub idle_connections: usize,
    /// Total configured connections
    pub total_connections: usize,
}

/// Mask sensitive information in database URL
fn mask_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            format!(
                "postgres://***:***@{}/{}",
                host,
                parsed.path().trim_start_matches('/')
            )
        } else {
            "postgres://***:***@***/**".to_string()
        }
    } else {
        "***".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_config_default() {
        let config = DatabaseConfig::default();
        assert!(!config.url.is_empty());
        assert!(config.max_connections > 0);
    }

    #[test]
    fn test_mask_url() {
        let url = "postgres://user:password@localhost:5432/dbname";
        let masked = mask_url(url);
        assert!(!masked.contains("password"));
        assert!(masked.contains("localhost"));
    }
}
