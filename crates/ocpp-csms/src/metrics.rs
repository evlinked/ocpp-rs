//! Metrics module for OCPP CSMS

use crate::{CsmsError, CsmsResult};
use prometheus::{Gauge, Histogram, IntCounter, IntGauge, Registry};
use serde::{Deserialize, Serialize};
use tracing::info;

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

/// Metrics registry for OCPP CSMS
pub struct MetricsRegistry {
    /// Prometheus registry
    registry: Registry,
    /// Configuration
    config: MetricsConfig,
    /// OCPP metrics
    ocpp_metrics: OcppMetrics,
    /// System metrics
    system_metrics: SystemMetrics,
}

impl MetricsRegistry {
    /// Create new metrics registry
    pub fn new(config: &MetricsConfig) -> CsmsResult<Self> {
        if !config.enabled {
            info!("Metrics collection disabled");
        } else {
            info!("Initializing metrics with namespace: {}", config.namespace);
        }

        let registry = Registry::new();
        let ocpp_metrics = OcppMetrics::new(&config.namespace, &config.latency_buckets)?;
        let system_metrics = SystemMetrics::new(&config.namespace)?;

        // Register metrics
        if config.enabled {
            registry.register(Box::new(ocpp_metrics.messages_received.clone()))?;
            registry.register(Box::new(ocpp_metrics.messages_sent.clone()))?;
            registry.register(Box::new(ocpp_metrics.active_connections.clone()))?;
            registry.register(Box::new(ocpp_metrics.message_processing_duration.clone()))?;
            registry.register(Box::new(system_metrics.uptime_seconds.clone()))?;
        }

        Ok(Self {
            registry,
            config: config.clone(),
            ocpp_metrics,
            system_metrics,
        })
    }

    /// Record message received
    pub fn record_message_received(&self, _action: &str) {
        if self.config.enabled {
            self.ocpp_metrics.messages_received.inc();
        }
    }

    /// Record message sent
    pub fn record_message_sent(&self, _action: &str) {
        if self.config.enabled {
            self.ocpp_metrics.messages_sent.inc();
        }
    }

    /// Record active connections
    pub fn set_active_connections(&self, count: i64) {
        if self.config.enabled {
            self.ocpp_metrics.active_connections.set(count);
        }
    }

    /// Record message processing duration
    pub fn record_message_processing_duration(&self, _action: &str, duration_seconds: f64) {
        if self.config.enabled {
            self.ocpp_metrics
                .message_processing_duration
                .observe(duration_seconds);
        }
    }

    /// Update system uptime
    pub fn update_uptime(&self, uptime_seconds: f64) {
        if self.config.enabled {
            self.system_metrics.uptime_seconds.set(uptime_seconds);
        }
    }

    /// Get metrics in Prometheus format
    pub fn gather(&self) -> Result<Vec<prometheus::proto::MetricFamily>, prometheus::Error> {
        if self.config.enabled {
            Ok(self.registry.gather())
        } else {
            Ok(Vec::new())
        }
    }

    /// Check if metrics system is healthy
    pub fn is_healthy(&self) -> bool {
        // TODO: Implement actual health checks
        true
    }

    /// Get metrics statistics
    pub fn get_stats(&self) -> MetricsStats {
        MetricsStats {
            enabled: self.config.enabled,
            metrics_count: if self.config.enabled {
                self.registry.gather().len()
            } else {
                0
            },
            namespace: self.config.namespace.clone(),
        }
    }
}

/// OCPP-specific metrics
struct OcppMetrics {
    /// Number of messages received by action
    messages_received: IntCounter,
    /// Number of messages sent by action
    messages_sent: IntCounter,
    /// Number of active WebSocket connections
    active_connections: IntGauge,
    /// Message processing duration histogram
    message_processing_duration: Histogram,
}

impl OcppMetrics {
    fn new(namespace: &str, latency_buckets: &[f64]) -> CsmsResult<Self> {
        Ok(Self {
            messages_received: IntCounter::new(
                format!("{}_messages_received_total", namespace),
                "Total number of OCPP messages received",
            )?,
            messages_sent: IntCounter::new(
                format!("{}_messages_sent_total", namespace),
                "Total number of OCPP messages sent",
            )?,
            active_connections: IntGauge::new(
                format!("{}_active_connections", namespace),
                "Number of active WebSocket connections",
            )?,
            message_processing_duration: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    format!("{}_message_processing_duration_seconds", namespace),
                    "Time spent processing OCPP messages",
                )
                .buckets(latency_buckets.to_vec()),
            )?,
        })
    }
}

/// System-level metrics
struct SystemMetrics {
    /// System uptime in seconds
    uptime_seconds: Gauge,
}

impl SystemMetrics {
    fn new(namespace: &str) -> CsmsResult<Self> {
        Ok(Self {
            uptime_seconds: Gauge::new(
                format!("{}_uptime_seconds", namespace),
                "System uptime in seconds",
            )?,
        })
    }
}

/// Metrics statistics
#[derive(Debug, Clone)]
pub struct MetricsStats {
    /// Whether metrics are enabled
    pub enabled: bool,
    /// Number of metrics being tracked
    pub metrics_count: usize,
    /// Metrics namespace
    pub namespace: String,
}

impl From<prometheus::Error> for CsmsError {
    fn from(err: prometheus::Error) -> Self {
        CsmsError::Internal {
            message: format!("Metrics error: {}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_config_default() {
        let config = MetricsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.namespace, "ocpp_csms");
        assert!(!config.latency_buckets.is_empty());
    }

    #[test]
    fn test_metrics_registry_creation() {
        let config = MetricsConfig::default();
        let registry = MetricsRegistry::new(&config);
        assert!(registry.is_ok());
    }

    #[test]
    fn test_metrics_registry_disabled() {
        let config = MetricsConfig {
            enabled: false,
            ..MetricsConfig::default()
        };
        let registry = MetricsRegistry::new(&config).unwrap();
        let stats = registry.get_stats();
        assert!(!stats.enabled);
    }

    #[test]
    fn test_metrics_operations() {
        let config = MetricsConfig::default();
        let registry = MetricsRegistry::new(&config).unwrap();

        // Test recording metrics
        registry.record_message_received("Heartbeat");
        registry.record_message_sent("HeartbeatResponse");
        registry.set_active_connections(5);
        registry.record_message_processing_duration("Authorize", 0.05);
        registry.update_uptime(3600.0);

        // Test gathering metrics
        let metrics = registry.gather();
        assert!(metrics.is_ok());
    }
}
