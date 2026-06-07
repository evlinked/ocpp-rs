//! # Transaction Management
//!
//! This module provides transaction lifecycle management for OCPP charge points,
//! including transaction state tracking, meter value recording, and transaction
//! data management according to OCPP 1.6J specification.

use anyhow::Result;
use ocpp_types::{ConnectorId, TransactionId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{debug, info, warn};

/// Transaction state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionState {
    /// Transaction is starting
    Starting,
    /// Transaction is active (charging)
    Active,
    /// Transaction is suspended
    Suspended,
    /// Transaction is stopping
    Stopping,
    /// Transaction has stopped
    Stopped,
    /// Transaction failed
    Failed,
}

/// Transaction stop reason
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StopReason {
    /// Emergency stop
    EmergencyStop,
    /// EV disconnected
    EVDisconnected,
    /// Hard reset
    HardReset,
    /// Local command
    Local,
    /// Other reason
    Other,
    /// Power loss
    PowerLoss,
    /// Reboot
    Reboot,
    /// Remote command
    Remote,
    /// Soft reset
    SoftReset,
    /// Unlock command
    UnlockCommand,
    /// De-authorized
    DeAuthorized,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopReason::EmergencyStop => write!(f, "EmergencyStop"),
            StopReason::EVDisconnected => write!(f, "EVDisconnected"),
            StopReason::HardReset => write!(f, "HardReset"),
            StopReason::Local => write!(f, "Local"),
            StopReason::Other => write!(f, "Other"),
            StopReason::PowerLoss => write!(f, "PowerLoss"),
            StopReason::Reboot => write!(f, "Reboot"),
            StopReason::Remote => write!(f, "Remote"),
            StopReason::SoftReset => write!(f, "SoftReset"),
            StopReason::UnlockCommand => write!(f, "UnlockCommand"),
            StopReason::DeAuthorized => write!(f, "DeAuthorized"),
        }
    }
}

impl From<String> for StopReason {
    fn from(s: String) -> Self {
        match s.as_str() {
            "EmergencyStop" => StopReason::EmergencyStop,
            "EVDisconnected" => StopReason::EVDisconnected,
            "HardReset" => StopReason::HardReset,
            "Local" => StopReason::Local,
            "PowerLoss" => StopReason::PowerLoss,
            "Reboot" => StopReason::Reboot,
            "Remote" => StopReason::Remote,
            "SoftReset" => StopReason::SoftReset,
            "UnlockCommand" => StopReason::UnlockCommand,
            "DeAuthorized" => StopReason::DeAuthorized,
            _ => StopReason::Other,
        }
    }
}

/// Meter value record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterValue {
    /// Timestamp of the meter value
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Energy reading in Wh
    pub energy_wh: i32,
    /// Power reading in W
    pub power_w: Option<f32>,
    /// Voltage reading in V
    pub voltage_v: Option<f32>,
    /// Current reading in A
    pub current_a: Option<f32>,
    /// Temperature in Celsius
    pub temperature_c: Option<f32>,
    /// State of charge percentage (0-100)
    pub soc_percent: Option<f32>,
}

impl MeterValue {
    /// Create a new meter value with current timestamp
    pub fn new(energy_wh: i32) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            energy_wh,
            power_w: None,
            voltage_v: None,
            current_a: None,
            temperature_c: None,
            soc_percent: None,
        }
    }

    /// Builder pattern for adding optional values
    pub fn with_power(mut self, power_w: f32) -> Self {
        self.power_w = Some(power_w);
        self
    }

    pub fn with_voltage(mut self, voltage_v: f32) -> Self {
        self.voltage_v = Some(voltage_v);
        self
    }

    pub fn with_current(mut self, current_a: f32) -> Self {
        self.current_a = Some(current_a);
        self
    }

    pub fn with_temperature(mut self, temperature_c: f32) -> Self {
        self.temperature_c = Some(temperature_c);
        self
    }

    pub fn with_soc(mut self, soc_percent: f32) -> Self {
        self.soc_percent = Some(soc_percent);
        self
    }
}

/// Transaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionData {
    /// Transaction ID
    pub transaction_id: TransactionId,
    /// Connector ID
    pub connector_id: ConnectorId,
    /// ID tag used for authorization
    pub id_tag: String,
    /// Start timestamp
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Stop timestamp
    pub stop_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Meter start value in Wh
    pub meter_start: i32,
    /// Meter stop value in Wh
    pub meter_stop: Option<i32>,
    /// Current state
    pub state: TransactionState,
    /// Stop reason
    pub stop_reason: Option<StopReason>,
    /// Reservation ID if started from reservation
    pub reservation_id: Option<i32>,
    /// Meter values recorded during transaction
    pub meter_values: VecDeque<MeterValue>,
    /// Maximum number of meter values to keep
    max_meter_values: usize,
}

impl TransactionData {
    /// Create new transaction data
    pub fn new(
        transaction_id: TransactionId,
        connector_id: ConnectorId,
        id_tag: String,
        meter_start: i32,
    ) -> Self {
        Self {
            transaction_id,
            connector_id,
            id_tag,
            start_time: chrono::Utc::now(),
            stop_time: None,
            meter_start,
            meter_stop: None,
            state: TransactionState::Starting,
            stop_reason: None,
            reservation_id: None,
            meter_values: VecDeque::new(),
            max_meter_values: 1000, // Keep last 1000 meter values
        }
    }

    /// Add meter value
    pub fn add_meter_value(&mut self, meter_value: MeterValue) {
        // Maintain maximum number of meter values
        if self.meter_values.len() >= self.max_meter_values {
            self.meter_values.pop_front();
        }
        self.meter_values.push_back(meter_value);
    }

    /// Get latest meter value
    pub fn latest_meter_value(&self) -> Option<&MeterValue> {
        self.meter_values.back()
    }

    /// Get energy consumed so far
    pub fn energy_consumed(&self) -> i32 {
        if let Some(latest) = self.latest_meter_value() {
            latest.energy_wh - self.meter_start
        } else {
            0
        }
    }

    /// Get transaction duration
    pub fn duration(&self) -> chrono::Duration {
        let end_time = self.stop_time.unwrap_or_else(chrono::Utc::now);
        end_time.signed_duration_since(self.start_time)
    }

    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            TransactionState::Active | TransactionState::Suspended
        )
    }

    /// Check if transaction is finished
    pub fn is_finished(&self) -> bool {
        matches!(
            self.state,
            TransactionState::Stopped | TransactionState::Failed
        )
    }
}

/// Transaction implementation
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Transaction data
    data: TransactionData,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(connector_id: ConnectorId, id_tag: String, meter_start: i32) -> Result<Self> {
        // Generate unique transaction ID (in real implementation, this would come from Central System)
        let transaction_id = TransactionId::new(chrono::Utc::now().timestamp_millis() as i32);

        let data = TransactionData::new(transaction_id, connector_id, id_tag, meter_start);

        Ok(Self { data })
    }

    /// Create transaction with specific ID (for responses from Central System)
    pub fn with_id(
        transaction_id: TransactionId,
        connector_id: ConnectorId,
        id_tag: String,
        meter_start: i32,
    ) -> Self {
        let data = TransactionData::new(transaction_id, connector_id, id_tag, meter_start);
        Self { data }
    }

    /// Get transaction ID
    pub fn id(&self) -> TransactionId {
        self.data.transaction_id
    }

    /// Get connector ID
    pub fn connector_id(&self) -> ConnectorId {
        self.data.connector_id
    }

    /// Get ID tag
    pub fn id_tag(&self) -> &str {
        &self.data.id_tag
    }

    /// Get start time
    pub fn start_time(&self) -> chrono::DateTime<chrono::Utc> {
        self.data.start_time
    }

    /// Get stop time
    pub fn stop_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.data.stop_time
    }

    /// Get meter start value
    pub fn meter_start(&self) -> i32 {
        self.data.meter_start
    }

    /// Get meter stop value
    pub fn meter_stop(&self) -> Option<i32> {
        self.data.meter_stop
    }

    /// Get current state
    pub fn state(&self) -> TransactionState {
        self.data.state.clone()
    }

    /// Get stop reason
    pub fn stop_reason(&self) -> Option<StopReason> {
        self.data.stop_reason.clone()
    }

    /// Get reservation ID
    pub fn reservation_id(&self) -> Option<i32> {
        self.data.reservation_id
    }

    /// Set reservation ID
    pub fn set_reservation_id(&mut self, reservation_id: Option<i32>) {
        self.data.reservation_id = reservation_id;
    }

    /// Start the transaction (transition to Active state)
    pub async fn start(&mut self) -> Result<()> {
        if self.data.state != TransactionState::Starting {
            return Err(anyhow::anyhow!(
                "Cannot start transaction in state: {:?}",
                self.data.state
            ));
        }

        self.data.state = TransactionState::Active;

        info!(
            "Transaction {} started on connector {} for user {}",
            self.data.transaction_id,
            self.data.connector_id.value(),
            self.data.id_tag
        );

        Ok(())
    }

    /// Suspend the transaction
    pub async fn suspend(&mut self) -> Result<()> {
        if self.data.state != TransactionState::Active {
            return Err(anyhow::anyhow!(
                "Cannot suspend transaction in state: {:?}",
                self.data.state
            ));
        }

        self.data.state = TransactionState::Suspended;

        debug!(
            "Transaction {} suspended on connector {}",
            self.data.transaction_id,
            self.data.connector_id.value()
        );

        Ok(())
    }

    /// Resume the transaction
    pub async fn resume(&mut self) -> Result<()> {
        if self.data.state != TransactionState::Suspended {
            return Err(anyhow::anyhow!(
                "Cannot resume transaction in state: {:?}",
                self.data.state
            ));
        }

        self.data.state = TransactionState::Active;

        debug!(
            "Transaction {} resumed on connector {}",
            self.data.transaction_id,
            self.data.connector_id.value()
        );

        Ok(())
    }

    /// Stop the transaction
    pub async fn stop(&mut self, reason: String) -> Result<()> {
        if self.data.state == TransactionState::Stopped
            || self.data.state == TransactionState::Failed
        {
            warn!(
                "Attempted to stop already finished transaction {}",
                self.data.transaction_id
            );
            return Ok(());
        }

        self.data.state = TransactionState::Stopping;
        self.data.stop_time = Some(chrono::Utc::now());
        self.data.stop_reason = Some(StopReason::from(reason));

        // Set meter stop value from latest meter reading
        if let Some(latest_meter) = self.data.latest_meter_value() {
            self.data.meter_stop = Some(latest_meter.energy_wh);
        } else {
            self.data.meter_stop = Some(self.data.meter_start);
        }

        self.data.state = TransactionState::Stopped;

        info!(
            "Transaction {} stopped on connector {} (reason: {:?}, energy: {} Wh)",
            self.data.transaction_id,
            self.data.connector_id.value(),
            self.data.stop_reason,
            self.data.energy_consumed()
        );

        Ok(())
    }

    /// Mark transaction as failed
    pub async fn fail(&mut self, reason: String) -> Result<()> {
        self.data.state = TransactionState::Failed;
        self.data.stop_time = Some(chrono::Utc::now());
        self.data.stop_reason = Some(StopReason::from(reason));

        warn!(
            "Transaction {} failed on connector {} (reason: {:?})",
            self.data.transaction_id,
            self.data.connector_id.value(),
            self.data.stop_reason
        );

        Ok(())
    }

    /// Update meter reading
    pub async fn update_meter_reading(&mut self, energy_wh: i32) -> Result<()> {
        let meter_value = MeterValue::new(energy_wh);
        self.data.add_meter_value(meter_value);

        debug!(
            "Meter reading updated for transaction {}: {} Wh",
            self.data.transaction_id, energy_wh
        );

        Ok(())
    }

    /// Add detailed meter value
    pub async fn add_meter_value(&mut self, meter_value: MeterValue) -> Result<()> {
        let energy_wh = meter_value.energy_wh;
        self.data.add_meter_value(meter_value);

        debug!(
            "Detailed meter value added for transaction {}: {} Wh",
            self.data.transaction_id, energy_wh
        );

        Ok(())
    }

    /// Get energy consumed
    pub fn energy_consumed(&self) -> i32 {
        self.data.energy_consumed()
    }

    /// Get transaction duration
    pub fn duration(&self) -> chrono::Duration {
        self.data.duration()
    }

    /// Get latest meter value
    pub fn latest_meter_value(&self) -> Option<&MeterValue> {
        self.data.latest_meter_value()
    }

    /// Get all meter values
    pub fn meter_values(&self) -> &VecDeque<MeterValue> {
        &self.data.meter_values
    }

    /// Get meter values in time range
    pub fn meter_values_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Vec<&MeterValue> {
        self.data
            .meter_values
            .iter()
            .filter(|mv| mv.timestamp >= start && mv.timestamp <= end)
            .collect()
    }

    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        self.data.is_active()
    }

    /// Check if transaction is finished
    pub fn is_finished(&self) -> bool {
        self.data.is_finished()
    }

    /// Get transaction data (for serialization)
    pub fn data(&self) -> &TransactionData {
        &self.data
    }

    /// Get mutable transaction data
    pub fn data_mut(&mut self) -> &mut TransactionData {
        &mut self.data
    }
}

/// Transaction manager for handling multiple transactions
#[derive(Debug)]
pub struct TransactionManager {
    /// Active transactions by connector ID
    transactions: std::collections::HashMap<ConnectorId, Transaction>,
    /// Transaction history
    history: VecDeque<TransactionData>,
    /// Maximum history size
    max_history_size: usize,
    /// Next transaction ID counter (for simulation)
    next_transaction_id: i32,
}

impl TransactionManager {
    /// Create new transaction manager
    pub fn new() -> Self {
        Self {
            transactions: std::collections::HashMap::new(),
            history: VecDeque::new(),
            max_history_size: 1000,
            next_transaction_id: 1,
        }
    }

    /// Create new transaction
    pub async fn create_transaction(
        &mut self,
        connector_id: ConnectorId,
        id_tag: String,
        meter_start: i32,
        reservation_id: Option<i32>,
    ) -> Result<TransactionId> {
        if self.transactions.contains_key(&connector_id) {
            return Err(anyhow::anyhow!(
                "Transaction already active on connector {}",
                connector_id.value()
            ));
        }

        let transaction_id = TransactionId::new(self.next_transaction_id);
        self.next_transaction_id += 1;

        let mut transaction =
            Transaction::with_id(transaction_id, connector_id, id_tag, meter_start);
        transaction.set_reservation_id(reservation_id);

        self.transactions.insert(connector_id, transaction);

        info!(
            "Created transaction {} on connector {}",
            transaction_id,
            connector_id.value()
        );

        Ok(transaction_id)
    }

    /// Get active transaction for connector
    pub fn get_transaction(&self, connector_id: ConnectorId) -> Option<&Transaction> {
        self.transactions.get(&connector_id)
    }

    /// Get mutable transaction for connector
    pub fn get_transaction_mut(&mut self, connector_id: ConnectorId) -> Option<&mut Transaction> {
        self.transactions.get_mut(&connector_id)
    }

    /// Stop transaction
    pub async fn stop_transaction(
        &mut self,
        connector_id: ConnectorId,
        reason: String,
    ) -> Result<Option<TransactionData>> {
        if let Some(mut transaction) = self.transactions.remove(&connector_id) {
            transaction.stop(reason).await?;
            let data = transaction.data().clone();

            // Add to history
            if self.history.len() >= self.max_history_size {
                self.history.pop_front();
            }
            self.history.push_back(data.clone());

            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Get all active transactions
    pub fn active_transactions(&self) -> &std::collections::HashMap<ConnectorId, Transaction> {
        &self.transactions
    }

    /// Get transaction history
    pub fn history(&self) -> &VecDeque<TransactionData> {
        &self.history
    }

    /// Clear old history entries
    pub fn clear_old_history(&mut self, older_than: chrono::DateTime<chrono::Utc>) {
        self.history.retain(|tx| tx.start_time >= older_than);
    }

    /// Get transaction statistics
    pub fn statistics(&self) -> TransactionStatistics {
        let active_count = self.transactions.len();
        let total_count = self.history.len() + active_count;

        let total_energy: i32 = self.history.iter().map(|tx| tx.energy_consumed()).sum();

        let completed_count = self
            .history
            .iter()
            .filter(|tx| tx.state == TransactionState::Stopped)
            .count();

        let failed_count = self
            .history
            .iter()
            .filter(|tx| tx.state == TransactionState::Failed)
            .count();

        TransactionStatistics {
            active_transactions: active_count,
            total_transactions: total_count,
            completed_transactions: completed_count,
            failed_transactions: failed_count,
            total_energy_wh: total_energy,
        }
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Transaction statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionStatistics {
    /// Number of active transactions
    pub active_transactions: usize,
    /// Total number of transactions
    pub total_transactions: usize,
    /// Number of completed transactions
    pub completed_transactions: usize,
    /// Number of failed transactions
    pub failed_transactions: usize,
    /// Total energy consumed in Wh
    pub total_energy_wh: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transaction_creation() {
        let connector_id = ConnectorId::new(1).unwrap();
        let id_tag = "test_user".to_string();
        let meter_start = 1000;

        let transaction = Transaction::new(connector_id, id_tag.clone(), meter_start).unwrap();

        assert_eq!(transaction.connector_id(), connector_id);
        assert_eq!(transaction.id_tag(), id_tag);
        assert_eq!(transaction.meter_start(), meter_start);
        assert_eq!(transaction.state(), TransactionState::Starting);
        assert!(!transaction.is_active());
        assert!(!transaction.is_finished());
    }

    #[tokio::test]
    async fn test_transaction_lifecycle() {
        let connector_id = ConnectorId::new(1).unwrap();
        let mut transaction =
            Transaction::new(connector_id, "test_user".to_string(), 1000).unwrap();

        // Start transaction
        transaction.start().await.unwrap();
        assert_eq!(transaction.state(), TransactionState::Active);
        assert!(transaction.is_active());

        // Suspend transaction
        transaction.suspend().await.unwrap();
        assert_eq!(transaction.state(), TransactionState::Suspended);
        assert!(transaction.is_active());

        // Resume transaction
        transaction.resume().await.unwrap();
        assert_eq!(transaction.state(), TransactionState::Active);

        // Stop transaction
        transaction.stop("Local".to_string()).await.unwrap();
        assert_eq!(transaction.state(), TransactionState::Stopped);
        assert!(!transaction.is_active());
        assert!(transaction.is_finished());
        assert!(transaction.stop_time().is_some());
    }

    #[tokio::test]
    async fn test_meter_values() {
        let connector_id = ConnectorId::new(1).unwrap();
        let mut transaction =
            Transaction::new(connector_id, "test_user".to_string(), 1000).unwrap();

        // Add meter values
        transaction.update_meter_reading(1100).await.unwrap();
        transaction.update_meter_reading(1200).await.unwrap();

        let detailed_meter = MeterValue::new(1300)
            .with_power(5000.0)
            .with_voltage(230.0)
            .with_current(21.7);

        transaction.add_meter_value(detailed_meter).await.unwrap();

        assert_eq!(transaction.meter_values().len(), 3);
        assert_eq!(transaction.energy_consumed(), 300); // 1300 - 1000

        let latest = transaction.latest_meter_value().unwrap();
        assert_eq!(latest.energy_wh, 1300);
        assert_eq!(latest.power_w, Some(5000.0));
    }

    #[tokio::test]
    async fn test_transaction_manager() {
        let mut manager = TransactionManager::new();
        let connector_id = ConnectorId::new(1).unwrap();

        // Create transaction
        let tx_id = manager
            .create_transaction(connector_id, "test_user".to_string(), 1000, None)
            .await
            .unwrap();

        assert!(manager.get_transaction(connector_id).is_some());
        assert_eq!(manager.active_transactions().len(), 1);

        // Stop transaction
        let stopped_tx = manager
            .stop_transaction(connector_id, "Local".to_string())
            .await
            .unwrap();

        assert!(stopped_tx.is_some());
        assert_eq!(manager.active_transactions().len(), 0);
        assert_eq!(manager.history().len(), 1);

        let stats = manager.statistics();
        assert_eq!(stats.active_transactions, 0);
        assert_eq!(stats.total_transactions, 1);
        assert_eq!(stats.completed_transactions, 1);
    }

    #[test]
    fn test_stop_reason_conversion() {
        assert_eq!(StopReason::from("Local".to_string()), StopReason::Local);
        assert_eq!(StopReason::from("Remote".to_string()), StopReason::Remote);
        assert_eq!(StopReason::from("Unknown".to_string()), StopReason::Other);

        assert_eq!(StopReason::Local.to_string(), "Local");
        assert_eq!(StopReason::EmergencyStop.to_string(), "EmergencyStop");
    }

    #[test]
    fn test_meter_value_builder() {
        let meter_value = MeterValue::new(1500)
            .with_power(3680.0)
            .with_voltage(230.0)
            .with_current(16.0)
            .with_temperature(25.5)
            .with_soc(75.0);

        assert_eq!(meter_value.energy_wh, 1500);
        assert_eq!(meter_value.power_w, Some(3680.0));
        assert_eq!(meter_value.voltage_v, Some(230.0));
        assert_eq!(meter_value.current_a, Some(16.0));
        assert_eq!(meter_value.temperature_c, Some(25.5));
        assert_eq!(meter_value.soc_percent, Some(75.0));
    }
}
