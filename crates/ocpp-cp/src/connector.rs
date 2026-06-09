//! # Connector Implementation
//!
//! This module provides a comprehensive connector implementation that manages:
//! - Connector states according to OCPP 1.6J specification
//! - State transitions based on various events (plug/unplug, authorization, faults)
//! - Transaction lifecycle management
//! - Meter value generation and reporting
//! - Status notifications to Central System

use crate::error::ChargePointError;
use crate::transaction::Transaction;
use anyhow::Result;
use ocpp_types::v16j::{ChargePointErrorCode, ChargePointStatus};
use ocpp_types::ConnectorId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Connector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    /// Connector identifier
    pub connector_id: ConnectorId,
    /// Connector type (e.g., "Type2", "CCS", "CHAdeMO")
    pub connector_type: String,
    /// Maximum amperage
    pub max_amperage: f64,
    /// Maximum voltage
    pub max_voltage: f64,
    /// Maximum power in watts
    pub max_power: f64,
    /// Number of phases
    pub phases: u8,
    /// Energy meter serial number
    pub energy_meter_serial: Option<String>,
}

/// Connector physical state
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ConnectorPhysicalState {
    /// Cable not plugged
    Unplugged,
    /// Cable plugged but not locked
    Plugged,
    /// Cable plugged and locked
    PluggedAndLocked,
}

/// Connector events
#[derive(Debug, Clone)]
pub enum ConnectorEvent {
    /// Cable plugged in
    PluggedIn,
    /// Cable unplugged
    Unplugged,
    /// Authorization received
    Authorized { id_tag: String },
    /// Authorization failed
    AuthorizationFailed { id_tag: String, reason: String },
    /// Charging started
    ChargingStarted,
    /// Charging stopped by EV
    ChargingStoppedByEV,
    /// Charging stopped by EVSE
    ChargingStoppedByEVSE,
    /// Charging resumed
    ChargingResumed,
    /// Transaction finished
    TransactionFinished,
    /// Fault occurred
    FaultOccurred {
        error_code: ChargePointErrorCode,
        info: Option<String>,
    },
    /// Fault cleared
    FaultCleared,
    /// Connector made available
    MadeAvailable,
    /// Connector made unavailable
    MadeUnavailable,
    /// Reserved for user
    Reserved { id_tag: String },
    /// Reservation cancelled
    ReservationCancelled,
}

/// Meter reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterReading {
    /// Timestamp of reading
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Energy reading in Wh
    pub energy_wh: f64,
    /// Power reading in W
    pub power_w: f64,
    /// Voltage reading in V
    pub voltage_v: f64,
    /// Current reading in A
    pub current_a: f64,
    /// Temperature in Celsius
    pub temperature_c: Option<f64>,
}

impl MeterReading {
    /// Create a new meter reading with current timestamp
    pub fn new(energy_wh: f64, power_w: f64, voltage_v: f64, current_a: f64) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            energy_wh,
            power_w,
            voltage_v,
            current_a,
            temperature_c: None,
        }
    }

    /// Create with temperature
    pub fn with_temperature(mut self, temperature_c: f64) -> Self {
        self.temperature_c = Some(temperature_c);
        self
    }
}

/// Connector implementation
#[derive(Debug, Clone)]
pub struct Connector {
    /// Configuration
    config: ConnectorConfig,
    /// Current OCPP status
    status: Arc<RwLock<ChargePointStatus>>,
    /// Physical state
    physical_state: Arc<RwLock<ConnectorPhysicalState>>,
    /// Current error code
    error_code: Arc<RwLock<ChargePointErrorCode>>,
    /// Error info
    error_info: Arc<RwLock<Option<String>>>,
    /// Current transaction
    current_transaction: Arc<RwLock<Option<Transaction>>>,
    /// Reservation ID tag
    reserved_for: Arc<RwLock<Option<String>>>,
    /// Last meter reading
    last_meter_reading: Arc<RwLock<MeterReading>>,
    /// Event sender
    event_sender: Arc<RwLock<Option<mpsc::UnboundedSender<ConnectorEvent>>>>,
}

impl Connector {
    /// Create a new connector
    pub fn new(config: ConnectorConfig) -> Result<Self> {
        Ok(Self {
            config,
            status: Arc::new(RwLock::new(ChargePointStatus::Available)),
            physical_state: Arc::new(RwLock::new(ConnectorPhysicalState::Unplugged)),
            error_code: Arc::new(RwLock::new(ChargePointErrorCode::NoError)),
            error_info: Arc::new(RwLock::new(None)),
            current_transaction: Arc::new(RwLock::new(None)),
            reserved_for: Arc::new(RwLock::new(None)),
            last_meter_reading: Arc::new(RwLock::new(MeterReading::new(0.0, 0.0, 0.0, 0.0))),
            event_sender: Arc::new(RwLock::new(None)),
        })
    }

    /// Set event sender
    pub async fn set_event_sender(&self, sender: mpsc::UnboundedSender<ConnectorEvent>) {
        *self.event_sender.write().await = Some(sender);
    }

    /// Send event
    async fn send_event(&self, event: ConnectorEvent) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            let _ = sender.send(event);
        }
    }

    /// Get connector ID
    pub fn connector_id(&self) -> ConnectorId {
        self.config.connector_id
    }

    /// Get current status
    pub async fn status(&self) -> ChargePointStatus {
        *self.status.read().await
    }

    /// Set status
    pub async fn set_status(&self, new_status: ChargePointStatus) -> Result<()> {
        let old_status = {
            let mut status = self.status.write().await;
            let old = *status;
            *status = new_status;
            old
        };

        if old_status != new_status {
            info!(
                "Connector {} status changed: {:?} -> {:?}",
                self.config.connector_id, old_status, new_status
            );
        }

        Ok(())
    }

    /// Get physical state
    pub async fn physical_state(&self) -> ConnectorPhysicalState {
        *self.physical_state.read().await
    }

    /// Get error code
    pub async fn error_code(&self) -> ChargePointErrorCode {
        *self.error_code.read().await
    }

    /// Get error info
    pub async fn error_info(&self) -> Option<String> {
        self.error_info.read().await.clone()
    }

    /// Check if connector is available for new transaction
    pub async fn is_available(&self) -> bool {
        matches!(
            *self.status.read().await,
            ChargePointStatus::Available | ChargePointStatus::Reserved
        )
    }

    /// Check if connector is charging
    pub async fn is_charging(&self) -> bool {
        matches!(
            *self.status.read().await,
            ChargePointStatus::Charging
                | ChargePointStatus::SuspendedEV
                | ChargePointStatus::SuspendedEVSE
        )
    }

    /// Check if connector has active transaction
    pub async fn has_active_transaction(&self) -> bool {
        self.current_transaction.read().await.is_some()
    }

    /// Get current transaction
    pub async fn current_transaction(&self) -> Option<Transaction> {
        self.current_transaction.read().await.clone()
    }

    /// Plug in cable
    pub async fn plug_in(&mut self) -> Result<()> {
        let current_status = self.status().await;

        debug!("Plugging in connector {}", self.config.connector_id);

        // Update physical state
        *self.physical_state.write().await = ConnectorPhysicalState::Plugged;

        // Handle status transitions based on current state
        match current_status {
            ChargePointStatus::Available => {
                // Available -> Preparing (waiting for authorization)
                self.set_status(ChargePointStatus::Preparing).await?;
                self.send_event(ConnectorEvent::PluggedIn).await;
            }
            ChargePointStatus::Reserved => {
                // Reserved -> Preparing
                self.set_status(ChargePointStatus::Preparing).await?;
                self.send_event(ConnectorEvent::PluggedIn).await;
            }
            ChargePointStatus::Unavailable => {
                // Stay unavailable but update physical state
                self.send_event(ConnectorEvent::PluggedIn).await;
            }
            ChargePointStatus::Faulted => {
                // Stay faulted but update physical state
                self.send_event(ConnectorEvent::PluggedIn).await;
            }
            _ => {
                warn!("Plug in event in unexpected status: {:?}", current_status);
            }
        }

        Ok(())
    }

    /// Plug out cable
    pub async fn plug_out(&mut self) -> Result<()> {
        let current_status = self.status().await;

        debug!("Plugging out connector {}", self.config.connector_id);

        // Update physical state
        *self.physical_state.write().await = ConnectorPhysicalState::Unplugged;

        // Handle status transitions based on current state
        match current_status {
            ChargePointStatus::Preparing => {
                // Preparing -> Available (cable removed before charging)
                self.set_status(ChargePointStatus::Available).await?;
                self.send_event(ConnectorEvent::Unplugged).await;
            }
            ChargePointStatus::Finishing => {
                // Finishing -> Available (normal end of transaction)
                self.set_status(ChargePointStatus::Available).await?;
                self.send_event(ConnectorEvent::Unplugged).await;

                // Clear transaction
                *self.current_transaction.write().await = None;
            }
            ChargePointStatus::Charging
            | ChargePointStatus::SuspendedEV
            | ChargePointStatus::SuspendedEVSE => {
                // Emergency unplug during charging
                warn!(
                    "Emergency unplug during charging on connector {}",
                    self.config.connector_id
                );

                // Stop transaction and go to Available
                if let Some(mut transaction) = self.current_transaction.write().await.take() {
                    transaction.stop("EmergencyStop".to_string()).await?;
                }

                self.set_status(ChargePointStatus::Available).await?;
                self.send_event(ConnectorEvent::Unplugged).await;
            }
            ChargePointStatus::Unavailable | ChargePointStatus::Faulted => {
                // Stay in current state but update physical state
                self.send_event(ConnectorEvent::Unplugged).await;
            }
            _ => {
                self.send_event(ConnectorEvent::Unplugged).await;
            }
        }

        Ok(())
    }

    /// Start transaction
    pub async fn start_transaction(&mut self, id_tag: String) -> Result<()> {
        let current_status = self.status().await;
        let physical_state = self.physical_state().await;

        debug!(
            "Starting transaction on connector {} with ID tag: {}",
            self.config.connector_id, id_tag
        );

        // Validate preconditions
        if physical_state == ConnectorPhysicalState::Unplugged {
            return Err(ChargePointError::InvalidOperation(
                "Cannot start transaction: cable not plugged".to_string(),
            )
            .into());
        }

        if self.has_active_transaction().await {
            return Err(ChargePointError::InvalidOperation(
                "Cannot start transaction: another transaction is active".to_string(),
            )
            .into());
        }

        // Check if reserved for different user
        if let Some(reserved_tag) = self.reserved_for.read().await.as_ref() {
            if *reserved_tag != id_tag {
                self.send_event(ConnectorEvent::AuthorizationFailed {
                    id_tag: id_tag.clone(),
                    reason: "Reserved for different user".to_string(),
                })
                .await;
                return Err(ChargePointError::AuthorizationFailed(
                    "Connector reserved for different user".to_string(),
                )
                .into());
            }
        }

        match current_status {
            ChargePointStatus::Preparing => {
                // Create and start transaction
                let transaction = Transaction::new(
                    self.config.connector_id,
                    id_tag.clone(),
                    self.last_meter_reading.read().await.energy_wh as i32,
                )?;

                *self.current_transaction.write().await = Some(transaction.clone());

                // Lock cable
                *self.physical_state.write().await = ConnectorPhysicalState::PluggedAndLocked;

                // Transition to Charging
                self.set_status(ChargePointStatus::Charging).await?;

                self.send_event(ConnectorEvent::Authorized { id_tag }).await;
                self.send_event(ConnectorEvent::ChargingStarted).await;

                // Clear reservation if any
                *self.reserved_for.write().await = None;

                info!(
                    "Transaction started on connector {} with ID: {}",
                    self.config.connector_id,
                    transaction.id()
                );
            }
            _ => {
                self.send_event(ConnectorEvent::AuthorizationFailed {
                    id_tag: id_tag.clone(),
                    reason: format!(
                        "Invalid status for starting transaction: {:?}",
                        current_status
                    ),
                })
                .await;
                return Err(ChargePointError::InvalidOperation(format!(
                    "Cannot start transaction in status: {:?}",
                    current_status
                ))
                .into());
            }
        }

        Ok(())
    }

    /// Stop transaction
    pub async fn stop_transaction(&mut self, reason: String) -> Result<()> {
        debug!(
            "Stopping transaction on connector {} with reason: {}",
            self.config.connector_id, reason
        );

        if let Some(mut transaction) = self.current_transaction.write().await.take() {
            // Stop the transaction
            transaction.stop(reason).await?;

            // Update status based on physical state
            let physical_state = self.physical_state().await;
            match physical_state {
                ConnectorPhysicalState::PluggedAndLocked | ConnectorPhysicalState::Plugged => {
                    // Cable still connected -> Finishing
                    self.set_status(ChargePointStatus::Finishing).await?;

                    // Unlock cable
                    *self.physical_state.write().await = ConnectorPhysicalState::Plugged;
                }
                ConnectorPhysicalState::Unplugged => {
                    // Cable already removed -> Available
                    self.set_status(ChargePointStatus::Available).await?;
                }
            }

            self.send_event(ConnectorEvent::TransactionFinished).await;

            info!(
                "Transaction stopped on connector {} with ID: {}",
                self.config.connector_id,
                transaction.id()
            );
        } else {
            warn!(
                "Attempted to stop transaction on connector {} but no active transaction found",
                self.config.connector_id
            );
        }

        Ok(())
    }

    /// Suspend charging (by EV)
    pub async fn suspend_by_ev(&mut self) -> Result<()> {
        let current_status = self.status().await;

        if current_status == ChargePointStatus::Charging {
            self.set_status(ChargePointStatus::SuspendedEV).await?;
            self.send_event(ConnectorEvent::ChargingStoppedByEV).await;

            info!(
                "Charging suspended by EV on connector {}",
                self.config.connector_id
            );
        }

        Ok(())
    }

    /// Suspend charging (by EVSE)
    pub async fn suspend_by_evse(&mut self) -> Result<()> {
        let current_status = self.status().await;

        if current_status == ChargePointStatus::Charging {
            self.set_status(ChargePointStatus::SuspendedEVSE).await?;
            self.send_event(ConnectorEvent::ChargingStoppedByEVSE).await;

            info!(
                "Charging suspended by EVSE on connector {}",
                self.config.connector_id
            );
        }

        Ok(())
    }

    /// Resume charging
    pub async fn resume_charging(&mut self) -> Result<()> {
        let current_status = self.status().await;

        match current_status {
            ChargePointStatus::SuspendedEV | ChargePointStatus::SuspendedEVSE => {
                self.set_status(ChargePointStatus::Charging).await?;
                self.send_event(ConnectorEvent::ChargingResumed).await;

                info!("Charging resumed on connector {}", self.config.connector_id);
            }
            _ => {
                warn!(
                    "Cannot resume charging on connector {} in status: {:?}",
                    self.config.connector_id, current_status
                );
            }
        }

        Ok(())
    }

    /// Set fault
    pub async fn set_fault(
        &mut self,
        error_code: ChargePointErrorCode,
        info: Option<String>,
    ) -> Result<()> {
        debug!(
            "Setting fault on connector {}: {:?} - {:?}",
            self.config.connector_id, error_code, info
        );

        *self.error_code.write().await = error_code;
        *self.error_info.write().await = info.clone();

        // Only change to Faulted if not NoError
        if error_code != ChargePointErrorCode::NoError {
            self.set_status(ChargePointStatus::Faulted).await?;
            self.send_event(ConnectorEvent::FaultOccurred { error_code, info })
                .await;

            // If we have an active transaction, stop it
            if let Some(mut transaction) = self.current_transaction.write().await.take() {
                transaction.stop("Fault".to_string()).await?;
            }

            info!(
                "Fault set on connector {}: {:?}",
                self.config.connector_id, error_code
            );
        }

        Ok(())
    }

    /// Clear fault
    pub async fn clear_fault(&mut self) -> Result<()> {
        debug!("Clearing fault on connector {}", self.config.connector_id);

        *self.error_code.write().await = ChargePointErrorCode::NoError;
        *self.error_info.write().await = None;

        // Determine appropriate status based on physical state
        let physical_state = self.physical_state().await;
        match physical_state {
            ConnectorPhysicalState::Unplugged => {
                self.set_status(ChargePointStatus::Available).await?;
            }
            ConnectorPhysicalState::Plugged | ConnectorPhysicalState::PluggedAndLocked => {
                // If cable is plugged, go to Preparing
                self.set_status(ChargePointStatus::Preparing).await?;
            }
        }

        self.send_event(ConnectorEvent::FaultCleared).await;

        info!("Fault cleared on connector {}", self.config.connector_id);

        Ok(())
    }

    /// Reserve connector for a user
    pub async fn reserve(&mut self, id_tag: String) -> Result<()> {
        let current_status = self.status().await;

        if current_status == ChargePointStatus::Available {
            *self.reserved_for.write().await = Some(id_tag.clone());
            self.set_status(ChargePointStatus::Reserved).await?;
            self.send_event(ConnectorEvent::Reserved {
                id_tag: id_tag.clone(),
            })
            .await;

            info!(
                "Connector {} reserved for user: {}",
                self.config.connector_id, id_tag
            );
        } else {
            return Err(ChargePointError::InvalidOperation(format!(
                "Cannot reserve connector in status: {:?}",
                current_status
            ))
            .into());
        }

        Ok(())
    }

    /// Cancel reservation
    pub async fn cancel_reservation(&mut self) -> Result<()> {
        *self.reserved_for.write().await = None;

        let current_status = self.status().await;
        if current_status == ChargePointStatus::Reserved {
            self.set_status(ChargePointStatus::Available).await?;
        }

        self.send_event(ConnectorEvent::ReservationCancelled).await;

        info!(
            "Reservation cancelled on connector {}",
            self.config.connector_id
        );

        Ok(())
    }

    /// Update meter reading
    pub async fn update_meter_reading(&self, reading: MeterReading) -> Result<()> {
        let energy_wh = reading.energy_wh as i32;
        *self.last_meter_reading.write().await = reading;

        if let Some(transaction) = self.current_transaction.write().await.as_mut() {
            transaction.update_meter_reading(energy_wh).await?;
        }

        Ok(())
    }

    /// Get last meter reading
    pub async fn last_meter_reading(&self) -> MeterReading {
        self.last_meter_reading.read().await.clone()
    }

    /// Get connector configuration
    pub fn config(&self) -> &ConnectorConfig {
        &self.config
    }

    /// Get reservation info
    pub async fn reserved_for(&self) -> Option<String> {
        self.reserved_for.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connector_creation() {
        let config = ConnectorConfig {
            connector_id: ConnectorId(1),
            connector_type: "Type2".to_string(),
            max_amperage: 32.0,
            max_voltage: 230.0,
            max_power: 7360.0,
            phases: 1,
            energy_meter_serial: Some("EM001".to_string()),
        };

        let connector = Connector::new(config.clone()).unwrap();

        assert_eq!(connector.connector_id(), ConnectorId(1));
        assert_eq!(connector.status().await, ChargePointStatus::Available);
        assert_eq!(
            connector.physical_state().await,
            ConnectorPhysicalState::Unplugged
        );
        assert!(!connector.has_active_transaction().await);
    }

    #[tokio::test]
    async fn test_plug_in_out_cycle() {
        let config = ConnectorConfig {
            connector_id: ConnectorId(1),
            connector_type: "Type2".to_string(),
            max_amperage: 32.0,
            max_voltage: 230.0,
            max_power: 7360.0,
            phases: 1,
            energy_meter_serial: Some("EM001".to_string()),
        };

        let mut connector = Connector::new(config).unwrap();

        // Initial state: Available, Unplugged
        assert_eq!(connector.status().await, ChargePointStatus::Available);
        assert_eq!(
            connector.physical_state().await,
            ConnectorPhysicalState::Unplugged
        );

        // Plug in: Available -> Preparing
        connector.plug_in().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Preparing);
        assert_eq!(
            connector.physical_state().await,
            ConnectorPhysicalState::Plugged
        );

        // Plug out: Preparing -> Available
        connector.plug_out().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Available);
        assert_eq!(
            connector.physical_state().await,
            ConnectorPhysicalState::Unplugged
        );
    }

    #[tokio::test]
    async fn test_transaction_lifecycle() {
        let config = ConnectorConfig {
            connector_id: ConnectorId(1),
            connector_type: "Type2".to_string(),
            max_amperage: 32.0,
            max_voltage: 230.0,
            max_power: 7360.0,
            phases: 1,
            energy_meter_serial: Some("EM001".to_string()),
        };

        let mut connector = Connector::new(config).unwrap();

        // Plug in cable
        connector.plug_in().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Preparing);

        // Start transaction
        connector
            .start_transaction("test_tag".to_string())
            .await
            .unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Charging);
        assert!(connector.has_active_transaction().await);
        assert_eq!(
            connector.physical_state().await,
            ConnectorPhysicalState::PluggedAndLocked
        );

        // Stop transaction
        connector
            .stop_transaction("Local".to_string())
            .await
            .unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Finishing);
        assert!(!connector.has_active_transaction().await);
        assert_eq!(
            connector.physical_state().await,
            ConnectorPhysicalState::Plugged
        );

        // Unplug cable
        connector.plug_out().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Available);
        assert_eq!(
            connector.physical_state().await,
            ConnectorPhysicalState::Unplugged
        );
    }

    #[tokio::test]
    async fn test_charging_suspension_and_resume() {
        let config = ConnectorConfig {
            connector_id: ConnectorId(1),
            connector_type: "Type2".to_string(),
            max_amperage: 32.0,
            max_voltage: 230.0,
            max_power: 7360.0,
            phases: 1,
            energy_meter_serial: Some("EM001".to_string()),
        };

        let mut connector = Connector::new(config).unwrap();

        // Setup charging state
        connector.plug_in().await.unwrap();
        connector
            .start_transaction("test_tag".to_string())
            .await
            .unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Charging);

        // Suspend by EV
        connector.suspend_by_ev().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::SuspendedEV);

        // Resume charging
        connector.resume_charging().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Charging);

        // Suspend by EVSE
        connector.suspend_by_evse().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::SuspendedEVSE);

        // Resume charging again
        connector.resume_charging().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Charging);
    }

    #[tokio::test]
    async fn test_fault_handling() {
        let config = ConnectorConfig {
            connector_id: ConnectorId(1),
            connector_type: "Type2".to_string(),
            max_amperage: 32.0,
            max_voltage: 230.0,
            max_power: 7360.0,
            phases: 1,
            energy_meter_serial: Some("EM001".to_string()),
        };

        let mut connector = Connector::new(config).unwrap();

        // Start with charging
        connector.plug_in().await.unwrap();
        connector
            .start_transaction("test_tag".to_string())
            .await
            .unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Charging);
        assert!(connector.has_active_transaction().await);

        // Set fault - should stop transaction and go to Faulted
        connector
            .set_fault(
                ChargePointErrorCode::OverCurrentFailure,
                Some("Test fault".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Faulted);
        assert_eq!(
            connector.error_code().await,
            ChargePointErrorCode::OverCurrentFailure
        );
        assert!(!connector.has_active_transaction().await);

        // Clear fault - should go to appropriate state based on physical state
        connector.clear_fault().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Preparing); // Cable still plugged
        assert_eq!(connector.error_code().await, ChargePointErrorCode::NoError);
    }

    #[tokio::test]
    async fn test_reservation() {
        let config = ConnectorConfig {
            connector_id: ConnectorId(1),
            connector_type: "Type2".to_string(),
            max_amperage: 32.0,
            max_voltage: 230.0,
            max_power: 7360.0,
            phases: 1,
            energy_meter_serial: Some("EM001".to_string()),
        };

        let mut connector = Connector::new(config).unwrap();

        // Reserve connector
        connector.reserve("user123".to_string()).await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Reserved);
        assert_eq!(connector.reserved_for().await, Some("user123".to_string()));

        // Cancel reservation
        connector.cancel_reservation().await.unwrap();
        assert_eq!(connector.status().await, ChargePointStatus::Available);
        assert_eq!(connector.reserved_for().await, None);
    }

    #[tokio::test]
    async fn test_meter_reading_update() {
        let config = ConnectorConfig {
            connector_id: ConnectorId(1),
            connector_type: "Type2".to_string(),
            max_amperage: 32.0,
            max_voltage: 230.0,
            max_power: 7360.0,
            phases: 1,
            energy_meter_serial: Some("EM001".to_string()),
        };

        let connector = Connector::new(config).unwrap();

        let reading = MeterReading::new(12500.0, 7360.0, 230.0, 32.0);
        connector.update_meter_reading(reading).await.unwrap();

        let last = connector.last_meter_reading().await;
        assert!((last.energy_wh - 12500.0).abs() < f64::EPSILON);
    }
}
