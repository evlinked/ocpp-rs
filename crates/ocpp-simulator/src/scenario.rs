//! # Scenario Execution Module
//!
//! This module provides scenario execution capabilities for the OCPP simulator,
//! allowing automated testing and realistic charge point behavior simulation.

use crate::{
    config::{ScenarioAction, ScenarioDefinition, ScenarioStep},
    error::SimulatorError,
    events::SimulatorEvent,
};
use ocpp_types::v16j::ChargePointErrorCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant};
use tracing::{debug, error, info, warn};

/// Scenario execution status
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScenarioStatus {
    /// Scenario is idle/not running
    Idle,
    /// Scenario is currently running
    Running,
    /// Scenario completed successfully
    Completed,
    /// Scenario failed
    Failed,
    /// Scenario was cancelled
    Cancelled,
    /// Scenario is paused
    Paused,
}

/// Scenario execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// Scenario name
    pub scenario_name: String,
    /// Execution status
    pub status: ScenarioStatus,
    /// Start timestamp
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// End timestamp (if completed)
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Total execution duration
    pub duration: Option<Duration>,
    /// Step results
    pub step_results: Vec<StepResult>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution parameters used
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Individual step execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name
    pub step_name: String,
    /// Step action
    pub action: String,
    /// Execution status
    pub success: bool,
    /// Start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Execution duration
    pub duration: Duration,
    /// Error message if failed
    pub error: Option<String>,
    /// Connector ID involved (if applicable)
    pub connector_id: Option<u32>,
}

/// Scenario execution context
pub struct ScenarioContext {
    /// Current scenario being executed
    pub scenario: ScenarioDefinition,
    /// Execution parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Current step index
    pub current_step: usize,
    /// Execution status
    pub status: ScenarioStatus,
    /// Start time
    pub start_time: Instant,
    /// Step results
    pub step_results: Vec<StepResult>,
    /// Variable storage for scenario
    pub variables: HashMap<String, serde_json::Value>,
}

impl ScenarioContext {
    /// Create new scenario context
    pub fn new(
        scenario: ScenarioDefinition,
        parameters: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            scenario,
            parameters,
            current_step: 0,
            status: ScenarioStatus::Idle,
            start_time: Instant::now(),
            step_results: Vec::new(),
            variables: HashMap::new(),
        }
    }

    /// Check if scenario has more steps
    pub fn has_next_step(&self) -> bool {
        self.current_step < self.scenario.steps.len()
    }

    /// Get current step
    pub fn current_step(&self) -> Option<&ScenarioStep> {
        self.scenario.steps.get(self.current_step)
    }

    /// Move to next step
    pub fn next_step(&mut self) {
        if self.has_next_step() {
            self.current_step += 1;
        }
    }

    /// Get execution duration
    pub fn duration(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Set variable
    pub fn set_variable(&mut self, key: String, value: serde_json::Value) {
        self.variables.insert(key, value);
    }

    /// Get variable
    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.variables.get(key)
    }
}

/// Scenario executor for running automated scenarios
pub struct ScenarioExecutor {
    /// Event sender for scenario events
    event_sender: Option<mpsc::UnboundedSender<SimulatorEvent>>,
}

impl ScenarioExecutor {
    /// Create new scenario executor
    pub fn new(event_sender: Option<mpsc::UnboundedSender<SimulatorEvent>>) -> Self {
        Self { event_sender }
    }

    /// Execute a scenario
    pub async fn execute_scenario(
        &self,
        scenario: ScenarioDefinition,
        parameters: HashMap<String, serde_json::Value>,
    ) -> Result<ScenarioResult, SimulatorError> {
        let mut context = ScenarioContext::new(scenario, parameters);
        context.status = ScenarioStatus::Running;

        info!("Starting scenario execution: {}", context.scenario.name);

        // Send scenario started event
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(SimulatorEvent::ScenarioStarted {
                scenario: context.scenario.name.clone(),
                timestamp: chrono::Utc::now(),
            });
        }

        let start_time = chrono::Utc::now();
        let mut overall_success = true;

        // Execute each step
        while context.has_next_step() && context.status == ScenarioStatus::Running {
            let step = context.current_step().unwrap().clone();

            debug!(
                "Executing step {}: {} (action: {:?})",
                context.current_step + 1,
                step.name,
                step.action
            );

            // Apply delay before step
            if step.delay_seconds > 0 {
                debug!(
                    "Waiting {} seconds before step execution",
                    step.delay_seconds
                );
                sleep(Duration::from_secs(step.delay_seconds)).await;
            }

            // Execute step
            let step_start = Instant::now();
            let step_start_time = chrono::Utc::now();

            let step_result = self.execute_step(&step, &mut context).await;
            let step_duration = step_start.elapsed();

            let success = step_result.is_ok();
            let step_error = step_result.err().map(|e| e.to_string());
            if step_error.is_some() {
                overall_success = false;
            }

            // Record step result
            let result = StepResult {
                step_name: step.name.clone(),
                action: format!("{:?}", step.action),
                success,
                start_time: step_start_time,
                duration: step_duration,
                error: step_error.clone(),
                connector_id: step.connector_id,
            };

            context.step_results.push(result.clone());

            // Send step executed event
            if let Some(sender) = &self.event_sender {
                let _ = sender.send(SimulatorEvent::ScenarioStepExecuted {
                    scenario: context.scenario.name.clone(),
                    step: step.name.clone(),
                    success,
                    timestamp: chrono::Utc::now(),
                });
            }

            if !success {
                error!(
                    "Step '{}' failed: {}",
                    step.name,
                    step_error.as_deref().unwrap_or("unknown error")
                );
                context.status = ScenarioStatus::Failed;
                break;
            }

            context.next_step();
        }

        // Determine final status
        let final_status = if context.status == ScenarioStatus::Running {
            if overall_success {
                ScenarioStatus::Completed
            } else {
                ScenarioStatus::Failed
            }
        } else {
            context.status
        };

        let end_time = chrono::Utc::now();
        let duration = end_time.signed_duration_since(start_time);

        let result = ScenarioResult {
            scenario_name: context.scenario.name.clone(),
            status: final_status,
            start_time,
            end_time: Some(end_time),
            duration: Some(duration.to_std().unwrap_or(Duration::ZERO)),
            step_results: context.step_results,
            error: if final_status == ScenarioStatus::Failed {
                Some("One or more steps failed".to_string())
            } else {
                None
            },
            parameters: context.parameters,
        };

        // Send scenario completed event
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(SimulatorEvent::ScenarioCompleted {
                scenario: context.scenario.name.clone(),
                success: final_status == ScenarioStatus::Completed,
                duration_seconds: duration.num_seconds() as u64,
                timestamp: chrono::Utc::now(),
            });
        }

        info!(
            "Scenario '{}' completed with status: {:?} (duration: {:?})",
            context.scenario.name, final_status, duration
        );

        Ok(result)
    }

    /// Execute a single scenario step
    async fn execute_step(
        &self,
        step: &ScenarioStep,
        context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        match &step.action {
            ScenarioAction::PlugIn => self.execute_plug_in(step, context).await,
            ScenarioAction::PlugOut => self.execute_plug_out(step, context).await,
            ScenarioAction::StartTransaction => self.execute_start_transaction(step, context).await,
            ScenarioAction::StopTransaction => self.execute_stop_transaction(step, context).await,
            ScenarioAction::InjectFault => self.execute_inject_fault(step, context).await,
            ScenarioAction::ClearFault => self.execute_clear_fault(step, context).await,
            ScenarioAction::SuspendChargingEV => {
                self.execute_suspend_charging_ev(step, context).await
            }
            ScenarioAction::SuspendChargingEVSE => {
                self.execute_suspend_charging_evse(step, context).await
            }
            ScenarioAction::ResumeCharging => self.execute_resume_charging(step, context).await,
            ScenarioAction::SetAvailability => self.execute_set_availability(step, context).await,
            ScenarioAction::Wait => self.execute_wait(step, context).await,
            ScenarioAction::SendHeartbeat => self.execute_send_heartbeat(step, context).await,
            ScenarioAction::UpdateMeterValues => {
                self.execute_update_meter_values(step, context).await
            }
            ScenarioAction::Connect => self.execute_connect(step, context).await,
            ScenarioAction::Disconnect => self.execute_disconnect(step, context).await,
            ScenarioAction::Reset => self.execute_reset(step, context).await,
        }
    }

    // Step execution implementations

    async fn execute_plug_in(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(&step.name, "Missing connector_id for plug_in")
        })?;

        debug!("Executing plug_in for connector {}", connector_id);

        // In a real implementation, this would call the charge point
        // For now, just log the action

        Ok(())
    }

    async fn execute_plug_out(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(&step.name, "Missing connector_id for plug_out")
        })?;

        debug!("Executing plug_out for connector {}", connector_id);

        Ok(())
    }

    async fn execute_start_transaction(
        &self,
        step: &ScenarioStep,
        context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(
                &step.name,
                "Missing connector_id for start_transaction",
            )
        })?;

        let id_tag = step
            .parameters
            .get("id_tag")
            .and_then(|v| v.as_str())
            .unwrap_or("scenario_user");

        debug!(
            "Executing start_transaction for connector {} with id_tag: {}",
            connector_id, id_tag
        );

        // Store transaction info in context variables
        context.set_variable(
            "last_transaction_connector".to_string(),
            connector_id.into(),
        );
        context.set_variable("last_transaction_id_tag".to_string(), id_tag.into());

        Ok(())
    }

    async fn execute_stop_transaction(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(
                &step.name,
                "Missing connector_id for stop_transaction",
            )
        })?;

        let reason = step
            .parameters
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Local");

        debug!(
            "Executing stop_transaction for connector {} with reason: {}",
            connector_id, reason
        );

        Ok(())
    }

    async fn execute_inject_fault(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(&step.name, "Missing connector_id for inject_fault")
        })?;

        let error_code_str = step
            .parameters
            .get("error_code")
            .and_then(|v| v.as_str())
            .unwrap_or("OtherError");

        let error_code = match error_code_str {
            "OverCurrentFailure" => ChargePointErrorCode::OverCurrentFailure,
            "OverVoltage" => ChargePointErrorCode::OverVoltage,
            "UnderVoltage" => ChargePointErrorCode::UnderVoltage,
            "HighTemperature" => ChargePointErrorCode::HighTemperature,
            "ConnectorLockFailure" => ChargePointErrorCode::ConnectorLockFailure,
            "PowerMeterFailure" => ChargePointErrorCode::PowerMeterFailure,
            _ => ChargePointErrorCode::OtherError,
        };

        let info = step.parameters.get("info").and_then(|v| v.as_str());

        debug!(
            "Executing inject_fault for connector {} with error_code: {:?}",
            connector_id, error_code
        );

        Ok(())
    }

    async fn execute_clear_fault(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(&step.name, "Missing connector_id for clear_fault")
        })?;

        debug!("Executing clear_fault for connector {}", connector_id);

        Ok(())
    }

    async fn execute_suspend_charging_ev(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(
                &step.name,
                "Missing connector_id for suspend_charging_ev",
            )
        })?;

        debug!(
            "Executing suspend_charging_ev for connector {}",
            connector_id
        );

        Ok(())
    }

    async fn execute_suspend_charging_evse(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(
                &step.name,
                "Missing connector_id for suspend_charging_evse",
            )
        })?;

        debug!(
            "Executing suspend_charging_evse for connector {}",
            connector_id
        );

        Ok(())
    }

    async fn execute_resume_charging(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(
                &step.name,
                "Missing connector_id for resume_charging",
            )
        })?;

        debug!("Executing resume_charging for connector {}", connector_id);

        Ok(())
    }

    async fn execute_set_availability(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id.ok_or_else(|| {
            SimulatorError::scenario_execution(
                &step.name,
                "Missing connector_id for set_availability",
            )
        })?;

        let available = step
            .parameters
            .get("available")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        debug!(
            "Executing set_availability for connector {} to: {}",
            connector_id, available
        );

        Ok(())
    }

    async fn execute_wait(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let duration = step
            .parameters
            .get("duration")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);

        debug!("Executing wait for {} seconds", duration);

        sleep(Duration::from_secs(duration)).await;

        Ok(())
    }

    async fn execute_send_heartbeat(
        &self,
        _step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        debug!("Executing send_heartbeat");

        Ok(())
    }

    async fn execute_update_meter_values(
        &self,
        step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let connector_id = step.connector_id;

        debug!(
            "Executing update_meter_values for connector: {:?}",
            connector_id
        );

        Ok(())
    }

    async fn execute_connect(
        &self,
        _step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        debug!("Executing connect to central system");

        Ok(())
    }

    async fn execute_disconnect(
        &self,
        _step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        debug!("Executing disconnect from central system");

        Ok(())
    }

    async fn execute_reset(
        &self,
        _step: &ScenarioStep,
        _context: &mut ScenarioContext,
    ) -> Result<(), SimulatorError> {
        let reset_type = _step
            .parameters
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("Soft");

        debug!("Executing reset with type: {}", reset_type);

        Ok(())
    }
}

/// Scenario manager for handling multiple concurrent scenarios
pub struct ScenarioManager {
    /// Available scenarios
    scenarios: HashMap<String, ScenarioDefinition>,
    /// Currently running scenarios
    running_scenarios: HashMap<String, ScenarioStatus>,
    /// Scenario executor
    executor: ScenarioExecutor,
}

impl ScenarioManager {
    /// Create new scenario manager
    pub fn new(event_sender: Option<mpsc::UnboundedSender<SimulatorEvent>>) -> Self {
        Self {
            scenarios: HashMap::new(),
            running_scenarios: HashMap::new(),
            executor: ScenarioExecutor::new(event_sender),
        }
    }

    /// Add scenario
    pub fn add_scenario(&mut self, scenario: ScenarioDefinition) {
        self.scenarios.insert(scenario.name.clone(), scenario);
    }

    /// Remove scenario
    pub fn remove_scenario(&mut self, name: &str) -> Option<ScenarioDefinition> {
        self.scenarios.remove(name)
    }

    /// Get scenario
    pub fn get_scenario(&self, name: &str) -> Option<&ScenarioDefinition> {
        self.scenarios.get(name)
    }

    /// List all scenarios
    pub fn list_scenarios(&self) -> Vec<&str> {
        self.scenarios.keys().map(|s| s.as_str()).collect()
    }

    /// Execute scenario
    pub async fn execute_scenario(
        &mut self,
        name: &str,
        parameters: HashMap<String, serde_json::Value>,
    ) -> Result<ScenarioResult, SimulatorError> {
        let scenario = self
            .scenarios
            .get(name)
            .ok_or_else(|| SimulatorError::resource_not_found("scenario", name))?
            .clone();

        // Check if scenario is already running
        if let Some(status) = self.running_scenarios.get(name) {
            if *status == ScenarioStatus::Running {
                return Err(SimulatorError::concurrent_operation(format!(
                    "Scenario '{}' is already running",
                    name
                )));
            }
        }

        self.running_scenarios
            .insert(name.to_string(), ScenarioStatus::Running);

        let result = self.executor.execute_scenario(scenario, parameters).await;

        // Update status
        if let Ok(ref res) = result {
            self.running_scenarios.insert(name.to_string(), res.status);
        } else {
            self.running_scenarios
                .insert(name.to_string(), ScenarioStatus::Failed);
        }

        result
    }

    /// Get scenario status
    pub fn get_scenario_status(&self, name: &str) -> Option<ScenarioStatus> {
        self.running_scenarios.get(name).copied()
    }

    /// Cancel running scenario
    pub fn cancel_scenario(&mut self, name: &str) -> Result<(), SimulatorError> {
        if let Some(status) = self.running_scenarios.get_mut(name) {
            if *status == ScenarioStatus::Running {
                *status = ScenarioStatus::Cancelled;
                Ok(())
            } else {
                Err(SimulatorError::invalid_state(
                    "cancel_scenario",
                    format!("{:?}", status),
                ))
            }
        } else {
            Err(SimulatorError::resource_not_found("scenario", name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ScenarioAction, ScenarioDefinition, ScenarioStep};

    #[test]
    fn test_scenario_context() {
        let scenario = ScenarioDefinition {
            name: "test_scenario".to_string(),
            description: "Test scenario".to_string(),
            steps: vec![ScenarioStep {
                name: "step1".to_string(),
                action: ScenarioAction::Wait,
                delay_seconds: 0,
                connector_id: None,
                parameters: HashMap::new(),
            }],
            repeat: false,
            repeat_interval_seconds: None,
            parameters: HashMap::new(),
        };

        let mut context = ScenarioContext::new(scenario, HashMap::new());

        assert_eq!(context.status, ScenarioStatus::Idle);
        assert!(context.has_next_step());
        assert_eq!(context.current_step, 0);

        context.next_step();
        assert!(!context.has_next_step());
        assert_eq!(context.current_step, 1);
    }

    #[test]
    fn test_scenario_manager() {
        let mut manager = ScenarioManager::new(None);

        let scenario = ScenarioDefinition {
            name: "test_scenario".to_string(),
            description: "Test scenario".to_string(),
            steps: vec![],
            repeat: false,
            repeat_interval_seconds: None,
            parameters: HashMap::new(),
        };

        manager.add_scenario(scenario.clone());

        assert_eq!(manager.list_scenarios(), vec!["test_scenario"]);
        assert!(manager.get_scenario("test_scenario").is_some());

        let removed = manager.remove_scenario("test_scenario");
        assert!(removed.is_some());
        assert!(manager.get_scenario("test_scenario").is_none());
    }

    #[tokio::test]
    async fn test_wait_step_execution() {
        let executor = ScenarioExecutor::new(None);

        let step = ScenarioStep {
            name: "wait_step".to_string(),
            action: ScenarioAction::Wait,
            delay_seconds: 0,
            connector_id: None,
            parameters: [("duration".to_string(), 0.into())].into(),
        };

        let scenario = ScenarioDefinition {
            name: "test".to_string(),
            description: "Test".to_string(),
            steps: vec![],
            repeat: false,
            repeat_interval_seconds: None,
            parameters: HashMap::new(),
        };

        let mut context = ScenarioContext::new(scenario, HashMap::new());

        let result = executor.execute_wait(&step, &mut context).await;
        assert!(result.is_ok());
    }
}
