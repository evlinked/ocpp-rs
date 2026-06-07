//! # Meter Simulator Module
//!
//! This module provides realistic meter value simulation for the OCPP simulator,
//! including energy consumption patterns, power variations, and meter noise simulation.

use crate::{config::MeterConfig, error::SimulatorError};
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;
use tracing::{debug, trace};

/// Meter reading with all standard measurements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterReading {
    /// Timestamp of reading
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Cumulative energy in Wh
    pub energy_wh: f64,
    /// Current power in W
    pub power_w: f64,
    /// Voltage in V
    pub voltage_v: f64,
    /// Current in A
    pub current_a: f64,
    /// Power factor
    pub power_factor: Option<f64>,
    /// Frequency in Hz
    pub frequency_hz: Option<f64>,
    /// Temperature in Celsius
    pub temperature_c: Option<f64>,
    /// State of charge percentage (0-100) for EV battery
    pub soc_percent: Option<f64>,
}

impl MeterReading {
    /// Create a new meter reading with basic values
    pub fn new(energy_wh: f64, power_w: f64, voltage_v: f64, current_a: f64) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            energy_wh,
            power_w,
            voltage_v,
            current_a,
            power_factor: Some(0.95),  // Typical PF for EV charging
            frequency_hz: Some(50.0),  // European standard
            temperature_c: Some(25.0), // Room temperature
            soc_percent: None,
        }
    }

    /// Set state of charge
    pub fn with_soc(mut self, soc_percent: f64) -> Self {
        self.soc_percent = Some(soc_percent.clamp(0.0, 100.0));
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature_c: f64) -> Self {
        self.temperature_c = Some(temperature_c);
        self
    }

    /// Calculate apparent power (VA)
    pub fn apparent_power_va(&self) -> f64 {
        self.voltage_v * self.current_a
    }

    /// Calculate reactive power (VAR)
    pub fn reactive_power_var(&self) -> f64 {
        let pf = self.power_factor.unwrap_or(1.0);
        let apparent_power = self.apparent_power_va();
        (apparent_power * apparent_power - self.power_w * self.power_w).sqrt()
    }
}

/// Charging behavior patterns
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChargingPattern {
    /// Constant power charging
    ConstantPower,
    /// Constant current charging
    ConstantCurrent,
    /// Tapered charging (reduces power as battery fills)
    TaperedCharging,
    /// Variable power based on grid conditions
    VariablePower,
}

/// Meter simulator for realistic energy consumption simulation
pub struct MeterSimulator {
    /// Configuration
    config: MeterConfig,
    /// Current energy reading in Wh
    current_energy_wh: f64,
    /// Base energy at start of simulation
    base_energy_wh: f64,
    /// Current power consumption in W
    current_power_w: f64,
    /// Target power for gradual changes
    target_power_w: f64,
    /// Charging pattern
    charging_pattern: ChargingPattern,
    /// Start time of current session
    session_start_time: Option<Instant>,
    /// Drift accumulator
    drift_accumulator: f64,
    /// Last update timestamp
    last_update: Instant,
    /// Session energy at start
    session_start_energy: f64,
    /// Simulated EV battery state
    battery_soc: f64,
    /// Battery capacity in Wh
    battery_capacity_wh: f64,
}

impl MeterSimulator {
    /// Create a new meter simulator
    pub fn new(config: MeterConfig) -> Self {
        Self {
            base_energy_wh: config.initial_energy_wh,
            current_energy_wh: config.initial_energy_wh,
            current_power_w: config.base_power_w,
            target_power_w: config.base_power_w,
            charging_pattern: ChargingPattern::ConstantPower,
            session_start_time: None,
            config,
            drift_accumulator: 0.0,
            last_update: Instant::now(),
            session_start_energy: 0.0,
            battery_soc: 50.0,            // Start with 50% charge
            battery_capacity_wh: 75000.0, // 75 kWh typical EV battery
        }
    }

    /// Start a charging session
    pub fn start_charging_session(&mut self, initial_soc: Option<f64>) {
        self.session_start_time = Some(Instant::now());
        self.session_start_energy = self.current_energy_wh;

        if let Some(soc) = initial_soc {
            self.battery_soc = soc.clamp(0.0, 100.0);
        }

        // Set charging pattern based on SoC
        self.charging_pattern = if self.battery_soc < 80.0 {
            ChargingPattern::ConstantPower
        } else {
            ChargingPattern::TaperedCharging
        };

        self.target_power_w = self.config.max_charging_power_w;

        debug!(
            "Started charging session with SoC: {:.1}%, pattern: {:?}",
            self.battery_soc, self.charging_pattern
        );
    }

    /// Stop charging session
    pub fn stop_charging_session(&mut self) {
        self.session_start_time = None;
        self.target_power_w = self.config.base_power_w;
        self.charging_pattern = ChargingPattern::ConstantPower;

        debug!("Stopped charging session");
    }

    /// Update meter readings
    pub fn update(&mut self) -> MeterReading {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        self.last_update = now;

        // Update power based on charging pattern
        self.update_power();

        // Calculate energy increment
        let hours = elapsed.as_secs_f64() / 3600.0;
        let energy_increment = self.current_power_w * hours;

        // Apply drift if enabled
        if self.config.simulate_drift {
            let drift_rate_per_hour = self.config.drift_rate_per_hour / 100.0;
            let drift_increment = energy_increment * drift_rate_per_hour * hours;
            self.drift_accumulator += drift_increment;
        }

        // Update energy with drift
        self.current_energy_wh += energy_increment + self.drift_accumulator;

        // Update battery SoC if charging
        if self.is_charging() {
            let energy_to_battery = energy_increment * 0.9; // 90% efficiency
            let soc_increment = (energy_to_battery / self.battery_capacity_wh) * 100.0;
            self.battery_soc = (self.battery_soc + soc_increment).min(100.0);
        }

        // Generate voltage and current
        let voltage = self.generate_voltage();
        let current = if voltage > 0.0 {
            self.current_power_w / voltage
        } else {
            0.0
        };

        // Apply noise if enabled
        let (final_energy, final_power, final_voltage, final_current) = if self.config.include_noise
        {
            self.apply_noise(
                self.current_energy_wh,
                self.current_power_w,
                voltage,
                current,
            )
        } else {
            (
                self.current_energy_wh,
                self.current_power_w,
                voltage,
                current,
            )
        };

        // Generate temperature based on power
        let temperature = self.generate_temperature();

        let reading = MeterReading::new(final_energy, final_power, final_voltage, final_current)
            .with_temperature(temperature)
            .with_soc(self.battery_soc);

        trace!(
            "Generated meter reading: {:.0} Wh, {:.0} W",
            reading.energy_wh,
            reading.power_w
        );

        reading
    }

    /// Set charging pattern
    pub fn set_charging_pattern(&mut self, pattern: ChargingPattern) {
        self.charging_pattern = pattern;
        debug!("Changed charging pattern to: {:?}", pattern);
    }

    /// Set target power
    pub fn set_target_power(&mut self, power_w: f64) {
        self.target_power_w = power_w.max(0.0).min(self.config.max_charging_power_w);
        debug!("Set target power to: {:.0} W", self.target_power_w);
    }

    /// Get current power
    pub fn current_power(&self) -> f64 {
        self.current_power_w
    }

    /// Get session energy
    pub fn session_energy(&self) -> f64 {
        if self.session_start_time.is_some() {
            self.current_energy_wh - self.session_start_energy
        } else {
            0.0
        }
    }

    /// Get session duration
    pub fn session_duration(&self) -> Duration {
        if let Some(start_time) = self.session_start_time {
            start_time.elapsed()
        } else {
            Duration::ZERO
        }
    }

    /// Check if currently charging
    pub fn is_charging(&self) -> bool {
        self.session_start_time.is_some() && self.current_power_w > self.config.base_power_w * 2.0
    }

    /// Get battery state of charge
    pub fn battery_soc(&self) -> f64 {
        self.battery_soc
    }

    /// Set battery SoC
    pub fn set_battery_soc(&mut self, soc: f64) {
        self.battery_soc = soc.clamp(0.0, 100.0);
    }

    /// Reset meter to initial state
    pub fn reset(&mut self) {
        self.current_energy_wh = self.config.initial_energy_wh;
        self.base_energy_wh = self.config.initial_energy_wh;
        self.current_power_w = self.config.base_power_w;
        self.target_power_w = self.config.base_power_w;
        self.session_start_time = None;
        self.drift_accumulator = 0.0;
        self.session_start_energy = 0.0;
        self.battery_soc = 50.0;
    }

    /// Update configuration
    pub fn update_config(&mut self, config: MeterConfig) {
        self.config = config;
        debug!("Updated meter simulator configuration");
    }

    // Private helper methods

    fn update_power(&mut self) {
        match self.charging_pattern {
            ChargingPattern::ConstantPower => {
                // Gradually approach target power
                let power_diff = self.target_power_w - self.current_power_w;
                let change_rate = power_diff * 0.1; // 10% per update
                self.current_power_w += change_rate;
            }
            ChargingPattern::ConstantCurrent => {
                // Maintain constant current, vary power with voltage
                let voltage = self.generate_voltage();
                let target_current = self.target_power_w / self.config.voltage_v;
                self.current_power_w = voltage * target_current;
            }
            ChargingPattern::TaperedCharging => {
                // Reduce power as battery fills up
                let taper_factor = if self.battery_soc > 80.0 {
                    1.0 - ((self.battery_soc - 80.0) / 20.0) * 0.7 // Reduce to 30% at 100%
                } else {
                    1.0
                };
                let tapered_target = self.target_power_w * taper_factor;
                let power_diff = tapered_target - self.current_power_w;
                self.current_power_w += power_diff * 0.05; // Slower change for tapering
            }
            ChargingPattern::VariablePower => {
                // Add some randomness to simulate grid conditions
                let mut rng = thread_rng();
                let variation = rng.gen_range(-0.2..=0.2); // ±20% variation
                let variable_target =
                    self.target_power_w * (1.0 + variation * self.config.power_variation);
                let power_diff = variable_target - self.current_power_w;
                self.current_power_w += power_diff * 0.2;
            }
        }

        // Apply power variation
        if self.config.power_variation > 0.0 {
            let mut rng = thread_rng();
            let variation =
                rng.gen_range(-self.config.power_variation..=self.config.power_variation);
            self.current_power_w *= 1.0 + variation;
        }

        // Clamp to valid range
        self.current_power_w = self
            .current_power_w
            .max(0.0)
            .min(self.config.max_charging_power_w);
    }

    fn generate_voltage(&self) -> f64 {
        let base_voltage = self.config.voltage_v;

        // Add small variations based on load
        let load_factor = self.current_power_w / self.config.max_charging_power_w;
        let voltage_drop = load_factor * 5.0; // Up to 5V drop under full load

        let voltage = base_voltage - voltage_drop;

        // Add small random variation
        if self.config.include_noise {
            let noise = thread_rng().gen_range(-1.0..=1.0);
            voltage + noise
        } else {
            voltage
        }
    }

    fn generate_temperature(&self) -> f64 {
        let base_temp = 25.0; // Room temperature

        // Temperature rises with power
        let power_factor = self.current_power_w / self.config.max_charging_power_w;
        let temp_rise = power_factor * 15.0; // Up to 15°C rise at full power

        // Add session duration effect
        let session_temp_rise = if let Some(start_time) = self.session_start_time {
            let minutes = start_time.elapsed().as_secs_f64() / 60.0;
            (minutes / 30.0).min(1.0) * 5.0 // Up to 5°C additional after 30 minutes
        } else {
            0.0
        };

        let temperature = base_temp + temp_rise + session_temp_rise;

        // Add noise
        if self.config.include_noise {
            let noise = thread_rng().gen_range(-0.5..=0.5);
            temperature + noise
        } else {
            temperature
        }
    }

    fn apply_noise(
        &mut self,
        energy: f64,
        power: f64,
        voltage: f64,
        current: f64,
    ) -> (f64, f64, f64, f64) {
        let noise_factor = self.config.noise_amplitude;
        let mut rng = thread_rng();

        let energy_noise = rng.gen_range(-noise_factor..=noise_factor);
        let power_noise = rng.gen_range(-noise_factor..=noise_factor);
        let voltage_noise = rng.gen_range(-noise_factor..=noise_factor);
        let current_noise = rng.gen_range(-noise_factor..=noise_factor);

        (
            energy * (1.0 + energy_noise),
            power * (1.0 + power_noise),
            voltage * (1.0 + voltage_noise),
            current * (1.0 + current_noise),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meter_simulator_creation() {
        let config = MeterConfig::default();
        let simulator = MeterSimulator::new(config);

        assert_eq!(simulator.current_energy_wh, 0.0);
        assert_eq!(simulator.current_power_w, 10.0); // base power
        assert!(!simulator.is_charging());
    }

    #[test]
    fn test_charging_session() {
        let config = MeterConfig::default();
        let mut simulator = MeterSimulator::new(config);

        // Start charging session
        simulator.start_charging_session(Some(20.0));
        assert!(simulator.session_start_time.is_some());
        assert_eq!(simulator.battery_soc, 20.0);

        // Stop charging session
        simulator.stop_charging_session();
        assert!(simulator.session_start_time.is_none());
    }

    #[test]
    fn test_meter_reading_generation() {
        let config = MeterConfig::default();
        let mut simulator = MeterSimulator::new(config);

        let reading1 = simulator.update();
        assert!(reading1.energy_wh < 1.0); // Near-zero: tiny elapsed time since creation
        assert!(reading1.power_w > 0.0);
        assert!(reading1.voltage_v > 0.0);
        assert!(reading1.current_a >= 0.0);

        // Start charging and generate another reading
        simulator.start_charging_session(Some(50.0));
        simulator.set_target_power(5000.0);

        // Simulate some time passing
        std::thread::sleep(std::time::Duration::from_millis(10));

        let reading2 = simulator.update();
        assert!(reading2.energy_wh >= reading1.energy_wh);
        assert!(reading2.power_w > reading1.power_w);
    }

    #[test]
    fn test_charging_patterns() {
        let config = MeterConfig::default();
        let mut simulator = MeterSimulator::new(config);

        simulator.start_charging_session(Some(30.0));
        simulator.set_target_power(5000.0);

        // Test constant power
        simulator.set_charging_pattern(ChargingPattern::ConstantPower);
        let reading1 = simulator.update();

        // Test tapered charging
        simulator.set_charging_pattern(ChargingPattern::TaperedCharging);
        let reading2 = simulator.update();

        // Both should have valid readings
        assert!(reading1.power_w > 0.0);
        assert!(reading2.power_w > 0.0);
    }

    #[test]
    fn test_battery_soc_updates() {
        let config = MeterConfig::default();
        let mut simulator = MeterSimulator::new(config);

        simulator.start_charging_session(Some(50.0));
        simulator.set_target_power(7000.0);

        let initial_soc = simulator.battery_soc();

        // Simulate charging for a bit
        for _ in 0..10 {
            simulator.update();
        }

        // SoC should have increased (even slightly)
        assert!(simulator.battery_soc() >= initial_soc);
    }

    #[test]
    fn test_session_metrics() {
        let config = MeterConfig::default();
        let mut simulator = MeterSimulator::new(config);

        // No session initially
        assert_eq!(simulator.session_energy(), 0.0);
        assert_eq!(simulator.session_duration(), Duration::ZERO);

        // Start session
        simulator.start_charging_session(Some(40.0));
        let initial_energy = simulator.session_energy();

        // Generate some readings
        simulator.update();

        // Session energy should be available
        assert!(simulator.session_energy() >= initial_energy);
        assert!(simulator.session_duration() > Duration::ZERO);
    }

    #[test]
    fn test_meter_reset() {
        let config = MeterConfig::default();
        let mut simulator = MeterSimulator::new(config.clone());

        // Modify state
        simulator.start_charging_session(Some(70.0));
        simulator.set_target_power(6000.0);
        simulator.update();

        // Reset
        simulator.reset();

        // Should be back to initial state
        assert_eq!(simulator.current_energy_wh, config.initial_energy_wh);
        assert_eq!(simulator.current_power_w, config.base_power_w);
        assert!(simulator.session_start_time.is_none());
        assert_eq!(simulator.battery_soc, 50.0);
    }
}
