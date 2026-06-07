//! # Fault Injector Module
//!
//! This module provides fault injection capabilities for the OCPP simulator,
//! allowing simulation of various error conditions and hardware failures.

use crate::error::SimulatorError;
use ocpp_types::v16j::ChargePointErrorCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;
use tracing::{debug, info, warn};

/// Fault injection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultInjectionConfig {
    /// Enable random fault injection
    pub enable_random: bool,
    /// Probability of random faults (0.0 to 1.0)
    pub random_probability: f64,
    /// Check interval for random faults
    pub check_interval_seconds: u64,
    /// Fault type probabilities
    pub fault_probabilities: HashMap<ChargePointErrorCode, f64>,
    /// Recovery time range (min, max) in seconds
    pub recovery_time_range: (u64, u64),
}

impl Default for FaultInjectionConfig {
    fn default() -> Self {
        let mut fault_probabilities = HashMap::new();
        fault_probabilities.insert(ChargePointErrorCode::OverCurrentFailure, 0.3);
        fault_probabilities.insert(ChargePointErrorCode::OverVoltage, 0.2);
        fault_probabilities.insert(ChargePointErrorCode::UnderVoltage, 0.2);
        fault_probabilities.insert(ChargePointErrorCode::HighTemperature, 0.1);
        fault_probabilities.insert(ChargePointErrorCode::ConnectorLockFailure, 0.1);
        fault_probabilities.insert(ChargePointErrorCode::PowerMeterFailure, 0.05);
        fault_probabilities.insert(ChargePointErrorCode::OtherError, 0.05);

        Self {
            enable_random: false,
            random_probability: 0.01,
            check_interval_seconds: 60,
            fault_probabilities,
            recovery_time_range: (30, 300),
        }
    }
}

/// Active fault information
#[derive(Debug, Clone)]
pub struct ActiveFault {
    /// Error code
    pub error_code: ChargePointErrorCode,
    /// Error description
    pub description: Option<String>,
    /// Fault injection timestamp
    pub injected_at: Instant,
    /// Recovery time (if auto-recovery is enabled)
    pub recovery_time: Option<Instant>,
    /// Whether fault was manually injected
    pub manual: bool,
}

/// Fault injector for managing error simulation
pub struct FaultInjector {
    /// Configuration
    config: FaultInjectionConfig,
    /// Active faults by connector ID
    active_faults: HashMap<u32, ActiveFault>,
}

impl FaultInjector {
    /// Create a new fault injector
    pub fn new(config: FaultInjectionConfig) -> Self {
        Self {
            config,
            active_faults: HashMap::new(),
        }
    }

    /// Inject a specific fault on a connector
    pub fn inject_fault(
        &mut self,
        connector_id: u32,
        error_code: ChargePointErrorCode,
        description: Option<String>,
        auto_recovery: bool,
    ) -> Result<(), SimulatorError> {
        info!(
            "Injecting fault on connector {}: {:?} - {:?}",
            connector_id, error_code, description
        );

        let recovery_time = if auto_recovery {
            let recovery_seconds = self.generate_recovery_time();
            Some(Instant::now() + Duration::from_secs(recovery_seconds))
        } else {
            None
        };

        let fault = ActiveFault {
            error_code,
            description,
            injected_at: Instant::now(),
            recovery_time,
            manual: true,
        };

        self.active_faults.insert(connector_id, fault);
        Ok(())
    }

    /// Clear fault on a connector
    pub fn clear_fault(&mut self, connector_id: u32) -> Result<(), SimulatorError> {
        if let Some(fault) = self.active_faults.remove(&connector_id) {
            info!(
                "Clearing fault on connector {}: {:?}",
                connector_id, fault.error_code
            );
            Ok(())
        } else {
            warn!("No active fault to clear on connector {}", connector_id);
            Err(SimulatorError::fault_injection(
                connector_id,
                "No active fault to clear",
            ))
        }
    }

    /// Check if connector has active fault
    pub fn has_fault(&self, connector_id: u32) -> bool {
        self.active_faults.contains_key(&connector_id)
    }

    /// Get active fault for connector
    pub fn get_fault(&self, connector_id: u32) -> Option<&ActiveFault> {
        self.active_faults.get(&connector_id)
    }

    /// Get all active faults
    pub fn get_all_faults(&self) -> &HashMap<u32, ActiveFault> {
        &self.active_faults
    }

    /// Process random fault injection
    pub fn process_random_faults(
        &mut self,
        connector_ids: &[u32],
    ) -> Vec<(u32, ChargePointErrorCode)> {
        if !self.config.enable_random {
            return Vec::new();
        }

        let mut injected_faults = Vec::new();

        for &connector_id in connector_ids {
            // Skip if connector already has a fault
            if self.has_fault(connector_id) {
                continue;
            }

            // Check if we should inject a random fault
            if self.should_inject_random_fault() {
                if let Some(error_code) = self.select_random_fault() {
                    let description = Some(format!("Random fault simulation: {:?}", error_code));

                    if self
                        .inject_fault(connector_id, error_code, description, true)
                        .is_ok()
                    {
                        injected_faults.push((connector_id, error_code));
                        debug!(
                            "Random fault injected on connector {}: {:?}",
                            connector_id, error_code
                        );
                    }
                }
            }
        }

        injected_faults
    }

    /// Process automatic fault recovery
    pub fn process_auto_recovery(&mut self) -> Vec<u32> {
        let mut recovered_connectors = Vec::new();
        let now = Instant::now();

        let mut to_remove = Vec::new();

        for (&connector_id, fault) in &self.active_faults {
            if let Some(recovery_time) = fault.recovery_time {
                if now >= recovery_time {
                    to_remove.push(connector_id);
                }
            }
        }

        for connector_id in to_remove {
            self.active_faults.remove(&connector_id);
            recovered_connectors.push(connector_id);
            info!("Auto-recovery completed for connector {}", connector_id);
        }

        recovered_connectors
    }

    /// Update configuration
    pub fn update_config(&mut self, config: FaultInjectionConfig) {
        self.config = config;
        debug!("Fault injection configuration updated");
    }

    /// Get statistics
    pub fn get_statistics(&self) -> FaultInjectionStatistics {
        let active_fault_count = self.active_faults.len();

        let mut fault_type_counts = HashMap::new();
        let mut manual_faults = 0;
        let mut auto_faults = 0;

        for fault in self.active_faults.values() {
            *fault_type_counts.entry(fault.error_code).or_insert(0) += 1;
            if fault.manual {
                manual_faults += 1;
            } else {
                auto_faults += 1;
            }
        }

        FaultInjectionStatistics {
            active_fault_count,
            fault_type_counts,
            manual_faults,
            auto_faults,
            random_injection_enabled: self.config.enable_random,
            random_probability: self.config.random_probability,
        }
    }

    // Private helper methods

    fn should_inject_random_fault(&mut self) -> bool {
        use rand::Rng as _;
        rand::thread_rng().gen::<f64>() < self.config.random_probability
    }

    fn select_random_fault(&mut self) -> Option<ChargePointErrorCode> {
        use rand::Rng as _;

        if self.config.fault_probabilities.is_empty() {
            return None;
        }

        let total_weight: f64 = self.config.fault_probabilities.values().sum();
        if total_weight <= 0.0 {
            return None;
        }

        let mut random_value = rand::thread_rng().gen::<f64>() * total_weight;

        for (error_code, probability) in &self.config.fault_probabilities {
            random_value -= probability;
            if random_value <= 0.0 {
                return Some(*error_code);
            }
        }

        // Fallback to first fault type
        self.config.fault_probabilities.keys().next().copied()
    }

    fn generate_recovery_time(&mut self) -> u64 {
        use rand::Rng as _;
        let (min_time, max_time) = self.config.recovery_time_range;
        rand::thread_rng().gen_range(min_time..=max_time)
    }
}

/// Fault injection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultInjectionStatistics {
    /// Number of active faults
    pub active_fault_count: usize,
    /// Count of faults by type
    pub fault_type_counts: HashMap<ChargePointErrorCode, usize>,
    /// Number of manually injected faults
    pub manual_faults: usize,
    /// Number of automatically injected faults
    pub auto_faults: usize,
    /// Whether random injection is enabled
    pub random_injection_enabled: bool,
    /// Random injection probability
    pub random_probability: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fault_injector_creation() {
        let config = FaultInjectionConfig::default();
        let injector = FaultInjector::new(config);
        assert_eq!(injector.active_faults.len(), 0);
    }

    #[test]
    fn test_fault_injection_and_clearing() {
        let config = FaultInjectionConfig::default();
        let mut injector = FaultInjector::new(config);

        // Inject fault
        let result = injector.inject_fault(
            1,
            ChargePointErrorCode::OverCurrentFailure,
            Some("Test fault".to_string()),
            false,
        );
        assert!(result.is_ok());
        assert!(injector.has_fault(1));

        // Check fault details
        let fault = injector.get_fault(1).unwrap();
        assert_eq!(fault.error_code, ChargePointErrorCode::OverCurrentFailure);
        assert_eq!(fault.description, Some("Test fault".to_string()));
        assert!(fault.manual);

        // Clear fault
        let result = injector.clear_fault(1);
        assert!(result.is_ok());
        assert!(!injector.has_fault(1));

        // Try to clear non-existent fault
        let result = injector.clear_fault(1);
        assert!(result.is_err());
    }

    #[test]
    fn test_auto_recovery() {
        let config = FaultInjectionConfig::default();
        let mut injector = FaultInjector::new(config);

        // Inject fault with auto-recovery
        let result = injector.inject_fault(1, ChargePointErrorCode::OverCurrentFailure, None, true);
        assert!(result.is_ok());
        assert!(injector.has_fault(1));

        // Check that recovery time is set
        let fault = injector.get_fault(1).unwrap();
        assert!(fault.recovery_time.is_some());

        // Process auto-recovery (should not recover immediately)
        let recovered = injector.process_auto_recovery();
        assert!(recovered.is_empty());
        assert!(injector.has_fault(1));
    }

    #[test]
    fn test_statistics() {
        let config = FaultInjectionConfig::default();
        let mut injector = FaultInjector::new(config);

        // Inject some faults
        injector
            .inject_fault(1, ChargePointErrorCode::OverCurrentFailure, None, false)
            .unwrap();
        injector
            .inject_fault(2, ChargePointErrorCode::HighTemperature, None, true)
            .unwrap();

        let stats = injector.get_statistics();
        assert_eq!(stats.active_fault_count, 2);
        assert_eq!(stats.manual_faults, 2); // Both are manual initially
        assert!(!stats.random_injection_enabled);
    }

    #[test]
    fn test_fault_probabilities() {
        let mut config = FaultInjectionConfig::default();
        config.enable_random = true;
        config.random_probability = 1.0; // Always inject

        let mut injector = FaultInjector::new(config);
        let connector_ids = vec![1, 2, 3];

        // Process random faults multiple times
        let mut total_injected = 0;
        for _ in 0..10 {
            let injected = injector.process_random_faults(&connector_ids);
            total_injected += injected.len();

            // Clear all faults to allow re-injection
            for &connector_id in &connector_ids {
                let _ = injector.clear_fault(connector_id);
            }
        }

        // With 100% probability, we should have injected some faults
        assert!(total_injected > 0);
    }
}
