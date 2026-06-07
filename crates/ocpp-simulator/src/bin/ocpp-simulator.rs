//! # OCPP Simulator CLI
//!
//! Command-line interface for the OCPP charge point simulator.
//! Provides easy configuration and control of simulated charge points.

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use ocpp_simulator::{
    config::SimulatorConfig, events::EventSeverity, start_simulator,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "ocpp-simulator")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the simulator
    Start(StartArgs),
    /// Generate a default configuration file
    Config(ConfigArgs),
    /// Validate a configuration file
    Validate(ValidateArgs),
    /// Run a specific scenario
    Scenario(ScenarioArgs),
    /// Show version information
    Version,
}

#[derive(Args)]
struct StartArgs {
    /// Configuration file path
    #[arg(short, long, default_value = "simulator.toml")]
    config: PathBuf,

    /// Override bind address
    #[arg(short, long)]
    bind: Option<SocketAddr>,

    /// Override central system URL
    #[arg(short = 'u', long)]
    url: Option<String>,

    /// Override charge point ID
    #[arg(short = 'i', long)]
    id: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable JSON logging
    #[arg(long)]
    json_logs: bool,
}

#[derive(Args)]
struct ConfigArgs {
    /// Output file path
    #[arg(short, long, default_value = "simulator.toml")]
    output: PathBuf,

    /// Output format (toml, json)
    #[arg(short, long, default_value = "toml")]
    format: String,

    /// Number of connectors
    #[arg(short = 'n', long, default_value = "2")]
    connectors: u32,

    /// Central system URL
    #[arg(short, long, default_value = "ws://localhost:8080")]
    url: String,

    /// Charge point ID
    #[arg(short, long, default_value = "SIM001")]
    id: String,
}

#[derive(Args)]
struct ValidateArgs {
    /// Configuration file path
    #[arg(short, long)]
    config: PathBuf,
}

#[derive(Args)]
struct ScenarioArgs {
    /// Configuration file path
    #[arg(short, long, default_value = "simulator.toml")]
    config: PathBuf,

    /// Scenario name to run
    #[arg(short, long)]
    scenario: String,

    /// Scenario parameters (JSON format)
    #[arg(short, long)]
    params: Option<String>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start(args) => start_command(args).await,
        Commands::Config(args) => config_command(args),
        Commands::Validate(args) => validate_command(args),
        Commands::Scenario(args) => scenario_command(args).await,
        Commands::Version => version_command(),
    }
}

async fn start_command(args: StartArgs) -> Result<()> {
    // Initialize logging
    init_logging(&args.log_level, args.json_logs)?;

    info!("Starting OCPP Simulator");

    // Load configuration
    let mut config = if args.config.exists() {
        info!("Loading configuration from: {}", args.config.display());
        SimulatorConfig::from_file(args.config.to_str().unwrap())?
    } else {
        warn!("Configuration file not found, using defaults");
        SimulatorConfig::default()
    };

    // Apply CLI overrides
    if let Some(bind_addr) = args.bind {
        config.api_config.bind_address = bind_addr.ip().to_string();
        config.api_config.port = bind_addr.port();
    }

    if let Some(url) = args.url {
        config.central_system_url = url;
    }

    if let Some(id) = args.id {
        config.charge_point_id = id;
    }

    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("Configuration validation failed: {}", e))?;

    info!("Configuration:");
    info!("  Charge Point ID: {}", config.charge_point_id);
    info!("  Central System: {}", config.central_system_url);
    info!("  Connectors: {}", config.connector_count);
    info!(
        "  API Server: {}:{}",
        config.api_config.bind_address, config.api_config.port
    );

    // Determine bind address
    let bind_addr = SocketAddr::new(
        config
            .api_config
            .bind_address
            .parse()
            .unwrap_or_else(|_| "127.0.0.1".parse().unwrap()),
        config.api_config.port,
    );

    info!("Starting simulator...");

    // Start the simulator in a separate task
    let config_clone = config.clone();
    let simulator_task = tokio::spawn(async move {
        if let Err(e) = start_simulator(config_clone, bind_addr).await {
            error!("Simulator error: {}", e);
        }
    });

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
        _ = simulator_task => {
            warn!("Simulator task completed unexpectedly");
        }
    }

    info!("Simulator stopped");
    Ok(())
}

fn config_command(args: ConfigArgs) -> Result<()> {
    let mut config = SimulatorConfig::default();

    // Apply CLI overrides
    config.connector_count = args.connectors;
    config.central_system_url = args.url;
    config.charge_point_id = args.id;

    // Save configuration
    config.to_file(args.output.to_str().unwrap())?;

    println!("Configuration file generated: {}", args.output.display());
    println!("Edit this file to customize your simulator settings.");
    println!();
    println!("Key settings:");
    println!("  charge_point_id: {}", config.charge_point_id);
    println!("  central_system_url: {}", config.central_system_url);
    println!("  connector_count: {}", config.connector_count);
    println!("  api_config.port: {}", config.api_config.port);

    Ok(())
}

fn validate_command(args: ValidateArgs) -> Result<()> {
    if !args.config.exists() {
        anyhow::bail!("Configuration file not found: {}", args.config.display());
    }

    println!("Validating configuration: {}", args.config.display());

    let config = SimulatorConfig::from_file(args.config.to_str().unwrap())?;

    match config.validate() {
        Ok(()) => {
            println!("✓ Configuration is valid");
            println!();
            println!("Configuration summary:");
            println!("  Charge Point ID: {}", config.charge_point_id);
            println!("  Central System: {}", config.central_system_url);
            println!("  Connectors: {}", config.connector_count);
            println!("  Heartbeat Interval: {}s", config.heartbeat_interval);
            println!("  Meter Values Interval: {}s", config.meter_values_interval);
            println!("  API Port: {}", config.api_config.port);
            println!(
                "  Available Scenarios: {}",
                config.scenario_config.scenarios.len()
            );

            for scenario_name in config.scenario_config.scenarios.keys() {
                println!("    - {}", scenario_name);
            }
        }
        Err(e) => {
            println!("✗ Configuration validation failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn scenario_command(args: ScenarioArgs) -> Result<()> {
    // Initialize logging
    init_logging(&args.log_level, false)?;

    info!("Running scenario: {}", args.scenario);

    // Load configuration
    let config = if args.config.exists() {
        SimulatorConfig::from_file(args.config.to_str().unwrap())?
    } else {
        SimulatorConfig::default()
    };

    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("Configuration validation failed: {}", e))?;

    // Check if scenario exists
    if !config
        .scenario_config
        .scenarios
        .contains_key(&args.scenario)
    {
        anyhow::bail!("Scenario '{}' not found in configuration", args.scenario);
    }

    let scenario = config.get_scenario(&args.scenario).unwrap();

    info!("Scenario: {}", scenario.name);
    info!("Description: {}", scenario.description);
    info!("Steps: {}", scenario.steps.len());

    // Parse parameters if provided
    let _params = if let Some(params_str) = args.params {
        Some(serde_json::from_str::<serde_json::Value>(&params_str)?)
    } else {
        None
    };

    // For now, just display the scenario steps
    println!("Scenario steps:");
    for (i, step) in scenario.steps.iter().enumerate() {
        println!("  {}. {} (action: {:?})", i + 1, step.name, step.action);
        if step.delay_seconds > 0 {
            println!("     Wait: {}s", step.delay_seconds);
        }
        if let Some(connector_id) = step.connector_id {
            println!("     Connector: {}", connector_id);
        }
        if !step.parameters.is_empty() {
            println!("     Parameters: {:?}", step.parameters);
        }
    }

    println!();
    println!("Note: Scenario execution is not yet implemented in this CLI.");
    println!("Use the REST API to execute scenarios programmatically.");

    Ok(())
}

fn version_command() -> Result<()> {
    println!("OCPP Simulator {}", env!("CARGO_PKG_VERSION"));
    println!("Built with Rust {}", env!("CARGO_PKG_RUST_VERSION"));
    println!();
    println!("Features:");
    println!("  - OCPP 1.6J protocol support");
    println!("  - Multiple connector simulation");
    println!("  - Real-time WebSocket connection to Central System");
    println!("  - REST API for external control");
    println!("  - Configurable charging scenarios");
    println!("  - Fault injection and error simulation");
    println!("  - Realistic meter value generation");
    println!("  - Event streaming and logging");

    Ok(())
}

fn init_logging(level: &str, json_logs: bool) -> Result<()> {
    let level = match level.to_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    if json_logs {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into()))
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into()))
            .with(tracing_subscriber::fmt::layer().pretty())
            .init();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_default_config_generation() {
        let config = SimulatorConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.charge_point_id, "SIM001");
        assert_eq!(config.connector_count, 2);
        assert_eq!(config.api_config.port, 8081);
    }
}
