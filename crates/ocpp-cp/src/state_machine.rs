//! # State Machine for Connector State Management
//!
//! This module provides a comprehensive state machine implementation for managing
//! connector states according to OCPP 1.6J specification. It handles all valid
//! state transitions and ensures proper state management with event-driven updates.

use crate::error::ChargePointError;
use anyhow::Result;
use ocpp_types::v16j::{ChargePointErrorCode, ChargePointStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// State machine events that can trigger state transitions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StateMachineEvent {
    /// Cable plugged in
    CablePluggedIn,
    /// Cable unplugged
    CableUnplugged,
    /// Authorization successful
    Authorized { id_tag: String },
    /// Authorization failed
    AuthorizationFailed { reason: String },
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
    FaultOccurred { error_code: ChargePointErrorCode },
    /// Fault cleared
    FaultCleared,
    /// Connector made available
    MakeAvailable,
    /// Connector made unavailable
    MakeUnavailable,
    /// Connector reserved
    Reserved { id_tag: String },
    /// Reservation cancelled
    ReservationCancelled,
    /// Reset command received
    Reset,
    /// Remote start transaction
    RemoteStartTransaction { id_tag: String },
    /// Remote stop transaction
    RemoteStopTransaction,
    /// Emergency stop
    EmergencyStop,
}

/// State transition result
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// Previous state
    pub from_state: ChargePointStatus,
    /// New state
    pub to_state: ChargePointStatus,
    /// Event that triggered the transition
    pub event: StateMachineEvent,
    /// Whether the transition was successful
    pub success: bool,
    /// Additional information about the transition
    pub info: Option<String>,
}

impl StateTransition {
    /// Create a successful transition
    pub fn success(
        from_state: ChargePointStatus,
        to_state: ChargePointStatus,
        event: StateMachineEvent,
    ) -> Self {
        Self {
            from_state,
            to_state,
            event,
            success: true,
            info: None,
        }
    }

    /// Create a successful transition with info
    pub fn success_with_info(
        from_state: ChargePointStatus,
        to_state: ChargePointStatus,
        event: StateMachineEvent,
        info: String,
    ) -> Self {
        Self {
            from_state,
            to_state,
            event,
            success: true,
            info: Some(info),
        }
    }

    /// Create a failed transition (state remains unchanged)
    pub fn failed(state: ChargePointStatus, event: StateMachineEvent, reason: String) -> Self {
        Self {
            from_state: state,
            to_state: state,
            event,
            success: false,
            info: Some(reason),
        }
    }
}

/// State machine for managing connector states
#[derive(Debug)]
pub struct ConnectorStateMachine {
    /// Current state
    current_state: ChargePointStatus,
    /// Previous state (for rollback scenarios)
    previous_state: Option<ChargePointStatus>,
    /// State transition history
    transition_history: Vec<StateTransition>,
    /// Maximum history size
    max_history_size: usize,
    /// Valid state transitions
    valid_transitions: HashMap<(ChargePointStatus, StateMachineEvent), ChargePointStatus>,
}

impl ConnectorStateMachine {
    /// Create a new state machine with initial state
    pub fn new(initial_state: ChargePointStatus) -> Self {
        let mut state_machine = Self {
            current_state: initial_state,
            previous_state: None,
            transition_history: Vec::new(),
            max_history_size: 100,
            valid_transitions: HashMap::new(),
        };

        state_machine.initialize_transitions();
        state_machine
    }

    /// Initialize all valid state transitions according to OCPP 1.6J specification
    fn initialize_transitions(&mut self) {
        use ChargePointStatus::*;
        use StateMachineEvent::*;

        let transitions = vec![
            // From Available
            ((Available, CablePluggedIn), Preparing),
            (
                (
                    Available,
                    StateMachineEvent::Reserved {
                        id_tag: String::new(),
                    },
                ),
                ChargePointStatus::Reserved,
            ),
            ((Available, MakeUnavailable), Unavailable),
            (
                (
                    Available,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((Available, Reset), Available),
            // From Preparing
            (
                (
                    Preparing,
                    Authorized {
                        id_tag: String::new(),
                    },
                ),
                Charging,
            ),
            (
                (
                    Preparing,
                    AuthorizationFailed {
                        reason: String::new(),
                    },
                ),
                Preparing,
            ),
            ((Preparing, CableUnplugged), Available),
            (
                (
                    Preparing,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((Preparing, MakeUnavailable), Unavailable),
            ((Preparing, TransactionFinished), Finishing),
            ((Preparing, Reset), Available),
            // From Charging
            ((Charging, ChargingStoppedByEV), SuspendedEV),
            ((Charging, ChargingStoppedByEVSE), SuspendedEVSE),
            ((Charging, TransactionFinished), Finishing),
            (
                (
                    Charging,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((Charging, RemoteStopTransaction), Finishing),
            ((Charging, EmergencyStop), Finishing),
            ((Charging, CableUnplugged), Available), // Emergency unplug
            ((Charging, Reset), Available),
            // From SuspendedEV
            ((SuspendedEV, ChargingResumed), Charging),
            ((SuspendedEV, TransactionFinished), Finishing),
            (
                (
                    SuspendedEV,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((SuspendedEV, RemoteStopTransaction), Finishing),
            ((SuspendedEV, CableUnplugged), Available),
            ((SuspendedEV, Reset), Available),
            // From SuspendedEVSE
            ((SuspendedEVSE, ChargingResumed), Charging),
            ((SuspendedEVSE, TransactionFinished), Finishing),
            (
                (
                    SuspendedEVSE,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((SuspendedEVSE, RemoteStopTransaction), Finishing),
            ((SuspendedEVSE, CableUnplugged), Available),
            ((SuspendedEVSE, Reset), Available),
            // From Finishing
            ((Finishing, CableUnplugged), Available),
            (
                (
                    Finishing,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((Finishing, MakeUnavailable), Unavailable),
            ((Finishing, Reset), Available),
            // Timeout transitions (handled externally)
            ((Finishing, TransactionFinished), Available),
            // From Reserved
            ((Reserved, CablePluggedIn), Preparing),
            ((Reserved, ReservationCancelled), Available),
            (
                (
                    Reserved,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((Reserved, MakeUnavailable), Unavailable),
            (
                (
                    Reserved,
                    RemoteStartTransaction {
                        id_tag: String::new(),
                    },
                ),
                Preparing,
            ),
            ((Reserved, Reset), Available),
            // From Faulted
            ((Faulted, FaultCleared), Available), // Or Preparing if cable is plugged
            ((Faulted, Reset), Available),
            ((Faulted, MakeUnavailable), Unavailable),
            // From Unavailable
            ((Unavailable, MakeAvailable), Available),
            (
                (
                    Unavailable,
                    FaultOccurred {
                        error_code: ChargePointErrorCode::NoError,
                    },
                ),
                Faulted,
            ),
            ((Unavailable, Reset), Available),
        ];

        for ((from_state, event), to_state) in transitions {
            // For events with data, we store them with default/empty data as keys
            // Actual matching will be done by event type in the transition logic
            self.valid_transitions.insert((from_state, event), to_state);
        }
    }

    /// Get current state
    pub fn current_state(&self) -> ChargePointStatus {
        self.current_state
    }

    /// Get previous state
    pub fn previous_state(&self) -> Option<ChargePointStatus> {
        self.previous_state
    }

    /// Check if a transition is valid
    pub fn is_valid_transition(&self, event: &StateMachineEvent) -> bool {
        self.get_next_state(event).is_some()
    }

    /// Get the next state for a given event without changing current state
    fn get_next_state(&self, event: &StateMachineEvent) -> Option<ChargePointStatus> {
        use ChargePointStatus::*;
        use StateMachineEvent::*;

        // Handle special cases and complex logic
        match (self.current_state, event) {
            // Cable plugged in transitions
            (Available, CablePluggedIn) => Some(Preparing),
            (Reserved, CablePluggedIn) => Some(Preparing),
            (Faulted, CablePluggedIn) => None,     // Stay faulted
            (Unavailable, CablePluggedIn) => None, // Stay unavailable

            // Cable unplugged transitions
            (Preparing, CableUnplugged) => Some(Available),
            (Finishing, CableUnplugged) => Some(Available),
            (Charging, CableUnplugged) => Some(Available), // Emergency unplug
            (SuspendedEV, CableUnplugged) => Some(Available),
            (SuspendedEVSE, CableUnplugged) => Some(Available),
            (Faulted, CableUnplugged) => Some(Available), // Clear physical connection issue
            (Available, CableUnplugged) => None,          // Already unplugged

            // Authorization transitions
            (Preparing, Authorized { .. }) => Some(Charging),
            (Reserved, Authorized { .. }) => Some(Charging), // If cable was plugged first

            // Authorization failed - stay in current state
            (Preparing, AuthorizationFailed { .. }) => None,

            // Charging control transitions
            (Charging, ChargingStoppedByEV) => Some(SuspendedEV),
            (Charging, ChargingStoppedByEVSE) => Some(SuspendedEVSE),
            (SuspendedEV, ChargingResumed) => Some(Charging),
            (SuspendedEVSE, ChargingResumed) => Some(Charging),

            // Transaction control
            (Charging, TransactionFinished) => Some(Finishing),
            (SuspendedEV, TransactionFinished) => Some(Finishing),
            (SuspendedEVSE, TransactionFinished) => Some(Finishing),
            (Preparing, TransactionFinished) => Some(Finishing),
            (Finishing, TransactionFinished) => Some(Available), // Timeout or confirmation

            // Remote control
            (Charging, RemoteStopTransaction) => Some(Finishing),
            (SuspendedEV, RemoteStopTransaction) => Some(Finishing),
            (SuspendedEVSE, RemoteStopTransaction) => Some(Finishing),
            (Available, RemoteStartTransaction { .. }) => Some(Preparing),
            (Reserved, RemoteStartTransaction { .. }) => Some(Preparing),

            // Fault handling
            (_, FaultOccurred { .. }) => Some(Faulted),
            (Faulted, FaultCleared) => {
                // Determine target state based on context
                // For now, default to Available - connector should determine actual state
                Some(Available)
            }

            // Availability control
            (Available, MakeUnavailable) => Some(Unavailable),
            (Preparing, MakeUnavailable) => Some(Unavailable),
            (Finishing, MakeUnavailable) => Some(Unavailable),
            (Reserved, MakeUnavailable) => Some(Unavailable),
            (Faulted, MakeUnavailable) => Some(Unavailable),
            (Unavailable, MakeAvailable) => Some(Available),

            // Reservation handling
            (Available, StateMachineEvent::Reserved { .. }) => Some(ChargePointStatus::Reserved),
            (ChargePointStatus::Reserved, ReservationCancelled) => Some(Available),

            // Emergency stop
            (Charging, EmergencyStop) => Some(Finishing),
            (SuspendedEV, EmergencyStop) => Some(Finishing),
            (SuspendedEVSE, EmergencyStop) => Some(Finishing),

            // Reset handling
            (_, Reset) => Some(Available),

            // Invalid or no-op transitions
            _ => None,
        }
    }

    /// Process an event and transition to new state if valid
    pub fn process_event(&mut self, event: StateMachineEvent) -> Result<StateTransition> {
        let from_state = self.current_state;

        debug!(
            "Processing state machine event: {:?} in state {:?}",
            event, from_state
        );

        // Check if transition is valid
        if let Some(to_state) = self.get_next_state(&event) {
            // Perform additional validation based on event and context
            if let Err(e) = self.validate_transition(&from_state, &to_state, &event) {
                warn!(
                    "State transition validation failed: {} -> {} on event {:?}: {}",
                    from_state, to_state, event, e
                );
                let failed_transition = StateTransition::failed(from_state, event, e.to_string());
                self.add_to_history(failed_transition.clone());
                return Ok(failed_transition);
            }

            // Execute the transition
            self.previous_state = Some(self.current_state);
            self.current_state = to_state;

            let transition = StateTransition::success(from_state, to_state, event);

            info!(
                "State transition: {} -> {} (event: {:?})",
                from_state, to_state, transition.event
            );

            self.add_to_history(transition.clone());
            Ok(transition)
        } else {
            let reason = format!(
                "Invalid state transition: {} with event {:?}",
                from_state, event
            );
            warn!("{}", reason);

            let failed_transition = StateTransition::failed(from_state, event, reason);
            self.add_to_history(failed_transition.clone());
            Ok(failed_transition)
        }
    }

    /// Validate a state transition with additional business logic
    fn validate_transition(
        &self,
        from_state: &ChargePointStatus,
        to_state: &ChargePointStatus,
        event: &StateMachineEvent,
    ) -> Result<()> {
        use ChargePointStatus::*;
        use StateMachineEvent::*;

        // Add custom validation logic here
        match (from_state, to_state, event) {
            // Validate authorization events
            (Preparing, Charging, Authorized { id_tag }) => {
                if id_tag.trim().is_empty() {
                    return Err(
                        ChargePointError::validation("id_tag", "ID tag cannot be empty").into(),
                    );
                }
            }

            // Validate reservation events
            (Available, ChargePointStatus::Reserved, StateMachineEvent::Reserved { id_tag }) => {
                if id_tag.trim().is_empty() {
                    return Err(ChargePointError::validation(
                        "id_tag",
                        "Reservation ID tag cannot be empty",
                    )
                    .into());
                }
            }

            // Validate remote start
            (_, Preparing, RemoteStartTransaction { id_tag }) => {
                if id_tag.trim().is_empty() {
                    return Err(ChargePointError::validation(
                        "id_tag",
                        "Remote start ID tag cannot be empty",
                    )
                    .into());
                }
            }

            // Validate fault events
            (_, Faulted, FaultOccurred { error_code }) => {
                if *error_code == ChargePointErrorCode::NoError {
                    return Err(ChargePointError::validation(
                        "error_code",
                        "Cannot fault with NoError code",
                    )
                    .into());
                }
            }

            _ => {} // No additional validation needed
        }

        Ok(())
    }

    /// Add transition to history
    fn add_to_history(&mut self, transition: StateTransition) {
        // Maintain maximum history size
        if self.transition_history.len() >= self.max_history_size {
            self.transition_history.remove(0);
        }
        self.transition_history.push(transition);
    }

    /// Get transition history
    pub fn transition_history(&self) -> &[StateTransition] {
        &self.transition_history
    }

    /// Get last transition
    pub fn last_transition(&self) -> Option<&StateTransition> {
        self.transition_history.last()
    }

    /// Clear transition history
    pub fn clear_history(&mut self) {
        self.transition_history.clear();
    }

    /// Force set state (for recovery scenarios)
    pub fn force_state(&mut self, new_state: ChargePointStatus, reason: String) -> StateTransition {
        let from_state = self.current_state;
        self.previous_state = Some(from_state);
        self.current_state = new_state;

        let transition = StateTransition::success_with_info(
            from_state,
            new_state,
            StateMachineEvent::Reset, // Use Reset as generic force event
            reason,
        );

        warn!(
            "Force state transition: {} -> {} (reason: {})",
            from_state,
            new_state,
            transition.info.as_ref().unwrap()
        );

        self.add_to_history(transition.clone());
        transition
    }

    /// Check if connector is in an operational state
    pub fn is_operational(&self) -> bool {
        !matches!(
            self.current_state,
            ChargePointStatus::Faulted | ChargePointStatus::Unavailable
        )
    }

    /// Check if connector is available for new transactions
    pub fn is_available_for_transaction(&self) -> bool {
        matches!(
            self.current_state,
            ChargePointStatus::Available | ChargePointStatus::Reserved
        )
    }

    /// Check if connector has an active transaction
    pub fn has_active_transaction(&self) -> bool {
        matches!(
            self.current_state,
            ChargePointStatus::Charging
                | ChargePointStatus::SuspendedEV
                | ChargePointStatus::SuspendedEVSE
                | ChargePointStatus::Finishing
        )
    }

    /// Get valid events for current state
    pub fn get_valid_events(&self) -> Vec<StateMachineEvent> {
        use ChargePointStatus::*;
        use StateMachineEvent::*;

        match self.current_state {
            Available => vec![
                CablePluggedIn,
                StateMachineEvent::Reserved {
                    id_tag: "".to_string(),
                },
                MakeUnavailable,
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                RemoteStartTransaction {
                    id_tag: "".to_string(),
                },
                Reset,
            ],
            Preparing => vec![
                Authorized {
                    id_tag: "".to_string(),
                },
                AuthorizationFailed {
                    reason: "".to_string(),
                },
                CableUnplugged,
                TransactionFinished,
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                MakeUnavailable,
                Reset,
            ],
            Charging => vec![
                ChargingStoppedByEV,
                ChargingStoppedByEVSE,
                TransactionFinished,
                RemoteStopTransaction,
                EmergencyStop,
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                CableUnplugged,
                Reset,
            ],
            SuspendedEV => vec![
                ChargingResumed,
                TransactionFinished,
                RemoteStopTransaction,
                EmergencyStop,
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                CableUnplugged,
                Reset,
            ],
            SuspendedEVSE => vec![
                ChargingResumed,
                TransactionFinished,
                RemoteStopTransaction,
                EmergencyStop,
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                CableUnplugged,
                Reset,
            ],
            Finishing => vec![
                CableUnplugged,
                TransactionFinished,
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                MakeUnavailable,
                Reset,
            ],
            Reserved => vec![
                CablePluggedIn,
                ReservationCancelled,
                RemoteStartTransaction {
                    id_tag: "".to_string(),
                },
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                MakeUnavailable,
                Reset,
            ],
            Faulted => vec![FaultCleared, MakeUnavailable, CableUnplugged, Reset],
            Unavailable => vec![
                MakeAvailable,
                FaultOccurred {
                    error_code: ChargePointErrorCode::NoError,
                },
                Reset,
            ],
        }
    }
}

impl Default for ConnectorStateMachine {
    fn default() -> Self {
        Self::new(ChargePointStatus::Available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_creation() {
        let sm = ConnectorStateMachine::new(ChargePointStatus::Available);
        assert_eq!(sm.current_state(), ChargePointStatus::Available);
        assert_eq!(sm.previous_state(), None);
        assert_eq!(sm.transition_history().len(), 0);
    }

    #[test]
    fn test_basic_transitions() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Available);

        // Available -> Preparing (cable plugged)
        let transition = sm.process_event(StateMachineEvent::CablePluggedIn).unwrap();
        assert!(transition.success);
        assert_eq!(transition.from_state, ChargePointStatus::Available);
        assert_eq!(transition.to_state, ChargePointStatus::Preparing);
        assert_eq!(sm.current_state(), ChargePointStatus::Preparing);

        // Preparing -> Charging (authorized)
        let transition = sm
            .process_event(StateMachineEvent::Authorized {
                id_tag: "user123".to_string(),
            })
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Charging);

        // Charging -> Finishing (transaction finished)
        let transition = sm
            .process_event(StateMachineEvent::TransactionFinished)
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Finishing);

        // Finishing -> Available (cable unplugged)
        let transition = sm.process_event(StateMachineEvent::CableUnplugged).unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Available);
    }

    #[test]
    fn test_charging_suspension() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Charging);

        // Charging -> SuspendedEV
        let transition = sm
            .process_event(StateMachineEvent::ChargingStoppedByEV)
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::SuspendedEV);

        // SuspendedEV -> Charging (resumed)
        let transition = sm
            .process_event(StateMachineEvent::ChargingResumed)
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Charging);

        // Charging -> SuspendedEVSE
        let transition = sm
            .process_event(StateMachineEvent::ChargingStoppedByEVSE)
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::SuspendedEVSE);

        // SuspendedEVSE -> Charging (resumed)
        let transition = sm
            .process_event(StateMachineEvent::ChargingResumed)
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Charging);
    }

    #[test]
    fn test_fault_handling() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Charging);

        // Any state -> Faulted
        let transition = sm
            .process_event(StateMachineEvent::FaultOccurred {
                error_code: ChargePointErrorCode::OverCurrentFailure,
            })
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Faulted);

        // Faulted -> Available (fault cleared)
        let transition = sm.process_event(StateMachineEvent::FaultCleared).unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Available);
    }

    #[test]
    fn test_reservation_flow() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Available);

        // Available -> Reserved
        let transition = sm
            .process_event(StateMachineEvent::Reserved {
                id_tag: "user456".to_string(),
            })
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Reserved);

        // Reserved -> Preparing (cable plugged)
        let transition = sm.process_event(StateMachineEvent::CablePluggedIn).unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Preparing);

        // Start over with reservation cancellation
        sm.force_state(ChargePointStatus::Reserved, "Test reset".to_string());

        // Reserved -> Available (reservation cancelled)
        let transition = sm
            .process_event(StateMachineEvent::ReservationCancelled)
            .unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Available);
    }

    #[test]
    fn test_invalid_transitions() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Available);

        // Try to unplug when nothing is plugged
        let transition = sm.process_event(StateMachineEvent::CableUnplugged).unwrap();
        assert!(!transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Available);

        // Try to authorize without cable
        let transition = sm
            .process_event(StateMachineEvent::Authorized {
                id_tag: "user123".to_string(),
            })
            .unwrap();
        assert!(!transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Available);
    }

    #[test]
    fn test_emergency_unplug() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Charging);

        // Emergency unplug during charging
        let transition = sm.process_event(StateMachineEvent::CableUnplugged).unwrap();
        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Available);
    }

    #[test]
    fn test_state_queries() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Available);

        assert!(sm.is_operational());
        assert!(sm.is_available_for_transaction());
        assert!(!sm.has_active_transaction());

        // Move to charging state
        sm.process_event(StateMachineEvent::CablePluggedIn).unwrap();
        sm.process_event(StateMachineEvent::Authorized {
            id_tag: "user123".to_string(),
        })
        .unwrap();

        assert!(sm.is_operational());
        assert!(!sm.is_available_for_transaction());
        assert!(sm.has_active_transaction());

        // Move to faulted state
        sm.process_event(StateMachineEvent::FaultOccurred {
            error_code: ChargePointErrorCode::OverCurrentFailure,
        })
        .unwrap();

        assert!(!sm.is_operational());
        assert!(!sm.is_available_for_transaction());
        assert!(!sm.has_active_transaction());
    }

    #[test]
    fn test_valid_events() {
        let sm = ConnectorStateMachine::new(ChargePointStatus::Available);
        let valid_events = sm.get_valid_events();

        assert!(!valid_events.is_empty());
        assert!(valid_events
            .iter()
            .any(|e| matches!(e, StateMachineEvent::CablePluggedIn)));
    }

    #[test]
    fn test_validation_failures() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Preparing);

        // Try to authorize with empty ID tag
        let transition = sm
            .process_event(StateMachineEvent::Authorized {
                id_tag: "".to_string(),
            })
            .unwrap();
        assert!(!transition.success);
        assert!(transition
            .info
            .as_ref()
            .unwrap()
            .contains("ID tag cannot be empty"));
    }

    #[test]
    fn test_history_management() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Available);

        // Perform several transitions
        sm.process_event(StateMachineEvent::CablePluggedIn).unwrap();
        sm.process_event(StateMachineEvent::Authorized {
            id_tag: "user123".to_string(),
        })
        .unwrap();
        sm.process_event(StateMachineEvent::TransactionFinished)
            .unwrap();

        assert_eq!(sm.transition_history().len(), 3);
        assert!(sm.last_transition().is_some());

        sm.clear_history();
        assert_eq!(sm.transition_history().len(), 0);
    }

    #[test]
    fn test_force_state() {
        let mut sm = ConnectorStateMachine::new(ChargePointStatus::Available);

        let transition =
            sm.force_state(ChargePointStatus::Faulted, "Emergency recovery".to_string());

        assert!(transition.success);
        assert_eq!(sm.current_state(), ChargePointStatus::Faulted);
        assert_eq!(sm.previous_state(), Some(ChargePointStatus::Available));
        assert!(transition
            .info
            .as_ref()
            .unwrap()
            .contains("Emergency recovery"));
    }
}
