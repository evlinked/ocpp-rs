//! OCPP CSMS Binary
//!
//! Command-line interface for running the OCPP Central System Management System

use clap::{Arg, Command};
use ocpp_csms::{Csms, CsmsConfig};
use std::process;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    let matches = Command::new("ocpp-csms")
        .version("0.1.0")
        .about("OCPP Central System Management System")
        .subcommand(
            Command::new("start")
                .about("Start the CSMS server")
                .arg(
                    Arg::new("config")
                        .short('c')
                        .long("config")
                        .value_name("FILE")
                        .help("Configuration file path")
                        .default_value("config/default.toml"),
                )
                .arg(
                    Arg::new("bind")
                        .short('b')
                        .long("bind")
                        .value_name("ADDRESS")
                        .help("Bind address")
                        .default_value("0.0.0.0:8080"),
                ),
        )
        .subcommand(Command::new("health-check").about("Check CSMS health status"))
        .get_matches();

    // Initialize tracing
    if let Err(e) = init_tracing() {
        eprintln!("Failed to initialize tracing: {}", e);
        process::exit(1);
    }

    match matches.subcommand() {
        Some(("start", sub_matches)) => {
            let config_path = sub_matches.get_one::<String>("config").unwrap();
            let _bind_addr = sub_matches.get_one::<String>("bind").unwrap();

            info!("Starting OCPP CSMS with config: {}", config_path);

            let config = match load_config(config_path) {
                Ok(config) => config,
                Err(e) => {
                    error!("Failed to load configuration: {}", e);
                    process::exit(1);
                }
            };

            let mut csms = match Csms::new(config).await {
                Ok(csms) => csms,
                Err(e) => {
                    error!("Failed to create CSMS: {}", e);
                    process::exit(1);
                }
            };

            if let Err(e) = csms.start().await {
                error!("Failed to start CSMS: {}", e);
                process::exit(1);
            }

            // Wait for shutdown signal
            match tokio::signal::ctrl_c().await {
                Ok(()) => {
                    info!("Received shutdown signal, stopping CSMS...");
                    if let Err(e) = csms.stop().await {
                        error!("Error during shutdown: {}", e);
                        process::exit(1);
                    }
                }
                Err(err) => {
                    error!("Unable to listen for shutdown signal: {}", err);
                    process::exit(1);
                }
            }

            info!("CSMS shutdown complete");
        }
        Some(("health-check", _)) => {
            println!("Health check not implemented yet");
            process::exit(1);
        }
        _ => {
            eprintln!("No subcommand provided. Use --help for usage information.");
            process::exit(1);
        }
    }
}

fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ocpp_csms=info,ocpp=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

fn load_config(_path: &str) -> Result<CsmsConfig, Box<dyn std::error::Error>> {
    // For now, return default config
    // TODO: Implement actual config loading from file
    Ok(CsmsConfig::default())
}
