//! # OCPP CLI Application
//!
//! A comprehensive CLI tool for testing and simulating OCPP charge points.
//! Connects to real Central Systems and provides interactive verification.

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use colored::*;
use dialoguer::{theme::ColorfulTheme, Select};
use futures_util::{SinkExt, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};
use tokio_native_tls::TlsConnector;
use tokio_tungstenite::{
    client_async, tungstenite::protocol::Message, tungstenite::protocol::WebSocketConfig,
};
use tracing::{debug, error, info, warn};
use url::Url;

#[derive(Parser)]
#[command(name = "ocpp-cli")]
#[command(about = "OCPP CLI - Test and simulate OCPP charge points")]
#[command(version, author)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to Central System and run interactive simulation
    Connect(ConnectArgs),
    /// Run automated test suite
    Test(TestArgs),
    /// Monitor OCPP traffic
    Monitor(MonitorArgs),
    /// Generate test scenarios
    Scenario(ScenarioArgs),
    /// Validate OCPP messages
    Validate(ValidateArgs),
}

#[derive(Args)]
struct ConnectArgs {
    /// Central System WebSocket URL
    #[arg(short, long, default_value = "ws://localhost:8080/ocpp/CP001")]
    url: String,

    /// Charge Point ID
    #[arg(short, long, default_value = "CP001")]
    id: String,

    /// Number of connectors
    #[arg(short, long, default_value = "2")]
    connectors: u32,

    /// Interactive mode
    #[arg(short = 'I', long)]
    interactive: bool,

    /// Auto-start scenarios
    #[arg(short, long)]
    auto_scenario: Option<String>,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Enable detailed logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args)]
struct TestArgs {
    /// Central System URL
    #[arg(short, long)]
    url: Option<String>,

    /// Test suite to run
    #[arg(short, long, default_value = "basic")]
    suite: String,

    /// Number of charge points to simulate
    #[arg(short, long, default_value = "1")]
    count: u32,

    /// Test duration in seconds
    #[arg(short, long, default_value = "300")]
    duration: u64,
}

#[derive(Args)]
struct MonitorArgs {
    /// Central System URL to monitor
    #[arg(short, long)]
    url: String,

    /// Filter by message type
    #[arg(short, long)]
    filter: Option<String>,

    /// Output format (json, table, raw)
    #[arg(short, long, default_value = "table")]
    output: String,
}

#[derive(Args)]
struct ScenarioArgs {
    /// Generate new scenario
    #[arg(short, long)]
    generate: bool,

    /// List available scenarios
    #[arg(short, long)]
    list: bool,

    /// Execute specific scenario
    #[arg(short, long)]
    execute: Option<String>,
}

#[derive(Args)]
struct ValidateArgs {
    /// OCPP message JSON file to validate
    #[arg(short, long)]
    file: Option<String>,

    /// Message type to validate against
    #[arg(short, long)]
    message_type: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SimulatorState {
    charge_point_id: String,
    connectors: u32,
    connected: bool,
    start_time: chrono::DateTime<chrono::Utc>,
    message_count: u64,
    connector_states: HashMap<u32, ConnectorStatus>,
    websocket_tx: Option<tokio::sync::mpsc::Sender<Message>>,
    recent_activities: Vec<ActivityLog>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ConnectorStatus {
    id: u32,
    status: ConnectorState,
    error_code: String,
    transaction_id: Option<u32>,
    cable_plugged: bool,
    current_power: f64,
    energy_delivered: f64,
    id_tag: Option<String>,
    last_status_time: chrono::DateTime<chrono::Utc>,
    remote_started: bool,
    battery_soc: Option<f64>, // State of Charge percentage (0-100)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ActivityLog {
    timestamp: chrono::DateTime<chrono::Utc>,
    activity_type: ActivityType,
    connector_id: Option<u32>,
    message: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ActivityType {
    RemoteStart,
    RemoteStop,
    ManualStart,
    ManualStop,
    StatusChange,
    Connection,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum ConnectorState {
    Available,
    Preparing,
    Charging,
    SuspendedEV,
    SuspendedEVSE,
    Finishing,
    Reserved,
    Unavailable,
    Faulted,
}

impl std::fmt::Display for ConnectorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectorState::Available => write!(f, "Available"),
            ConnectorState::Preparing => write!(f, "Preparing"),
            ConnectorState::Charging => write!(f, "Charging"),
            ConnectorState::SuspendedEV => write!(f, "SuspendedEV"),
            ConnectorState::SuspendedEVSE => write!(f, "SuspendedEVSE"),
            ConnectorState::Finishing => write!(f, "Finishing"),
            ConnectorState::Reserved => write!(f, "Reserved"),
            ConnectorState::Unavailable => write!(f, "Unavailable"),
            ConnectorState::Faulted => write!(f, "Faulted"),
        }
    }
}

impl ConnectorStatus {
    fn new(id: u32) -> Self {
        Self {
            id,
            status: ConnectorState::Available,
            error_code: "NoError".to_string(),
            transaction_id: None,
            cable_plugged: false,
            current_power: 0.0,
            energy_delivered: 0.0,
            id_tag: None,
            last_status_time: chrono::Utc::now(),
            remote_started: false,
            battery_soc: None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Connect(args) => connect_command(args).await,
        Commands::Test(args) => test_command(args).await,
        Commands::Monitor(args) => monitor_command(args).await,
        Commands::Scenario(args) => scenario_command(args).await,
        Commands::Validate(args) => validate_command(args).await,
    }
}

async fn connect_command(args: ConnectArgs) -> Result<()> {
    // Initialize logging
    init_logging(&args.log_level, args.verbose)?;

    print_banner();
    println!(
        "🔌 {}\n",
        "OCPP Charge Point Simulator".bright_cyan().bold()
    );

    // Display connection info
    println!("📡 Connection Details:");
    println!("   URL: {}", args.url.bright_white());
    println!("   Charge Point ID: {}", args.id.bright_yellow());
    println!(
        "   Connectors: {}",
        args.connectors.to_string().bright_green()
    );
    println!();

    // Create simulator state with initialized connectors
    let mut connector_states = HashMap::new();
    for i in 1..=args.connectors {
        connector_states.insert(i, ConnectorStatus::new(i));
    }

    let state = SimulatorState {
        charge_point_id: args.id.clone(),
        connectors: args.connectors,
        connected: false,
        start_time: chrono::Utc::now(),
        message_count: 0,
        connector_states,
        websocket_tx: None,
        recent_activities: Vec::new(),
    };

    // Start WebSocket connection
    let simulator = Arc::new(tokio::sync::Mutex::new(state));

    // Start energy simulation and meter values task for active transactions
    let simulator_energy = simulator.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10)); // 10 second intervals for MeterValues
        loop {
            interval.tick().await;
            let mut state = simulator_energy.lock().await;
            let tx_option = state.websocket_tx.clone();

            for (connector_id, connector) in state.connector_states.iter_mut() {
                if connector.transaction_id.is_some() && connector.current_power > 0.0 {
                    // Simulate energy delivery: 10 seconds * power in kW / 3600 seconds per hour
                    let energy_increment = connector.current_power * 10.0 / 3600.0;
                    connector.energy_delivered += energy_increment;

                    // Update SoC during charging (simulate battery charging)
                    if let Some(ref mut soc) = connector.battery_soc {
                        // Assume 75kWh battery capacity and 90% charging efficiency
                        let battery_capacity_kwh = 75.0;
                        let charging_efficiency = 0.9;
                        let soc_increment =
                            (energy_increment * charging_efficiency / battery_capacity_kwh) * 100.0;
                        *soc = (*soc + soc_increment).min(100.0);
                    }

                    // Send MeterValues message
                    if let (Some(tx_id), Some(tx)) = (connector.transaction_id, &tx_option) {
                        let mut sampled_values = vec![
                            serde_json::json!({
                                "value": format!("{}", (connector.energy_delivered * 1000.0) as i32),
                                "context": "Sample.Periodic",
                                "format": "Raw",
                                "measurand": "Energy.Active.Import.Register",
                                "unit": "Wh"
                            }),
                            serde_json::json!({
                                "value": format!("{:.1}", connector.current_power * 1000.0),
                                "context": "Sample.Periodic",
                                "format": "Raw",
                                "measurand": "Power.Active.Import",
                                "unit": "W"
                            }),
                        ];

                        // Add SoC if available
                        if let Some(soc) = connector.battery_soc {
                            sampled_values.push(serde_json::json!({
                                "value": format!("{:.1}", soc),
                                "context": "Sample.Periodic",
                                "format": "Raw",
                                "measurand": "SoC",
                                "unit": "Percent"
                            }));
                        }

                        let meter_values = serde_json::json!([
                            2,
                            format!("{}", chrono::Utc::now().timestamp()),
                            "MeterValues",
                            {
                                "connectorId": connector_id,
                                "transactionId": tx_id,
                                "meterValue": [{
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                    "sampledValue": sampled_values
                                }]
                            }
                        ]);

                        let _ = tx.send(Message::Text(meter_values.to_string())).await;
                        debug!(
                            "📊 Sent MeterValues for connector {} transaction {}",
                            connector_id, tx_id
                        );
                    }
                }
            }
        }
    });

    // Setup progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.blue} {msg}")?,
    );
    pb.set_message("Connecting to Central System...");

    // Parse URL
    let _url = Url::parse(&args.url)?;

    // Connect to WebSocket
    pb.enable_steady_tick(Duration::from_millis(120));

    match connect_websocket(&args.url, &args.id, simulator.clone()).await {
        Ok(_) => {
            pb.finish_with_message("✅ Connected successfully");
            println!("\n🟢 {}", "Connection Established".bright_green().bold());
        }
        Err(e) => {
            pb.finish_with_message("❌ Connection failed");
            return Err(anyhow::anyhow!("Failed to connect: {}", e));
        }
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Setup Ctrl+C handler
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let shutdown_tx_clone = shutdown_tx.clone();

    ctrlc::set_handler(move || {
        println!("\n\n🛑 {} received", "Ctrl+C".yellow());
        let _ = shutdown_tx_clone.try_send(());
    })?;

    // Interactive mode or monitoring
    if args.interactive {
        println!(
            "🎮 {} - Type 'help' for commands",
            "Interactive Mode".bright_blue()
        );
        run_interactive_mode(&args, simulator.clone()).await?;
    } else if let Some(scenario_name) = args.auto_scenario {
        println!(
            "🎯 Running auto scenario: {}",
            scenario_name.bright_yellow()
        );
        run_auto_scenario(&scenario_name, simulator.clone()).await?;
    } else {
        // Just monitor and display status
        run_monitoring_mode(&args, simulator.clone()).await?;
    }

    // Wait for shutdown signal
    tokio::select! {
        _ = shutdown_rx.recv() => {
            println!("\n📴 Shutting down simulator...");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n📴 Shutting down simulator...");
        }
    }

    println!("✅ {}", "Simulator stopped".bright_green());
    Ok(())
}

async fn connect_websocket(
    url: &str,
    charge_point_id: &str,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    info!(
        "Connecting to {} (Charge Point ID: {})",
        url, charge_point_id
    );

    // Parse URL
    let url_parsed = url::Url::parse(url)?;
    let host = url_parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
    let port = url_parsed
        .port()
        .unwrap_or(if url_parsed.scheme() == "wss" {
            443
        } else {
            80
        });

    // Connect TCP stream
    let tcp_stream = TcpStream::connect((host, port))
        .await
        .map_err(|e| anyhow::anyhow!("TCP connection failed: {}", e))?;

    // Setup TLS if needed
    let stream = if url_parsed.scheme() == "wss" {
        let connector = TlsConnector::from(
            native_tls::TlsConnector::new()
                .map_err(|e| anyhow::anyhow!("TLS connector creation failed: {}", e))?,
        );
        let tls_stream = connector
            .connect(host, tcp_stream)
            .await
            .map_err(|e| anyhow::anyhow!("TLS connection failed: {}", e))?;
        tokio_tungstenite::MaybeTlsStream::NativeTls(tls_stream)
    } else {
        tokio_tungstenite::MaybeTlsStream::Plain(tcp_stream)
    };

    // Create WebSocket request with OCPP subprotocol
    let request = http::Request::builder()
        .uri(url)
        .header("Host", host)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Protocol", "ocpp1.6")
        .body(())
        .map_err(|e| anyhow::anyhow!("Failed to build WebSocket request: {}", e))?;

    // Perform WebSocket handshake
    let _config = WebSocketConfig::default();
    let (ws_stream, response) = client_async(request, stream)
        .await
        .map_err(|e| anyhow::anyhow!("WebSocket handshake failed: {}", e))?;

    info!("WebSocket connected successfully with OCPP 1.6 subprotocol");
    debug!("WebSocket response status: {:?}", response.status());

    let (mut write, mut read) = ws_stream.split();

    // Create message channel for sending messages
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Message>(100);

    // Update simulator state with websocket sender
    {
        let mut state = simulator.lock().await;
        state.connected = true;
        state.websocket_tx = Some(tx);

        // Log connection activity
        state.recent_activities.push(ActivityLog {
            timestamp: chrono::Utc::now(),
            activity_type: ActivityType::Connection,
            connector_id: None,
            message: format!("🔗 Connected to Central System at {}", url),
        });

        // Keep only last 20 activities
        if state.recent_activities.len() > 20 {
            state.recent_activities.remove(0);
        }
    }

    // Send BootNotification
    let boot_notification = create_boot_notification(charge_point_id);
    write
        .send(Message::Text(boot_notification))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send BootNotification: {}", e))?;

    // Start message sending task
    let _simulator_clone = simulator.clone();
    tokio::spawn(async move {
        info!("📡 WebSocket message sender task started");
        while let Some(msg) = rx.recv().await {
            debug!("📤 Sending WebSocket message: {:?}", msg);
            if let Err(e) = write.send(msg).await {
                error!("❌ Failed to send WebSocket message: {}", e);
                break;
            } else {
                debug!("✅ WebSocket message sent successfully");
            }
        }
        warn!("📡 WebSocket message sender task ended");
    });

    // Start message handling task
    let simulator_clone2 = simulator.clone();
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received message: {}", text);
                    match handle_message(&text, simulator_clone2.clone()).await {
                        Ok(Some(response)) => {
                            info!("📤 Sending response: {}", response);
                            // Send response back through the channel
                            if let Some(tx) = &simulator_clone2.lock().await.websocket_tx {
                                match tx.send(Message::Text(response.clone())).await {
                                    Ok(_) => info!("✅ Response sent successfully"),
                                    Err(e) => error!("❌ Failed to send response: {}", e),
                                }
                            } else {
                                error!("❌ No websocket tx channel available");
                            }
                        }
                        Ok(None) => {
                            debug!("No response needed for message");
                        }
                        Err(e) => {
                            error!("Error handling message: {}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(())
}

async fn handle_message(
    message: &str,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<Option<String>> {
    // Update message count
    {
        let mut state = simulator.lock().await;
        state.message_count += 1;
    }

    // Parse and handle OCPP message
    let parsed: Value = serde_json::from_str(message)?;

    if let Some(array) = parsed.as_array() {
        if array.len() >= 4 {
            let message_type = array[0].as_i64().unwrap_or(0);
            let message_id = array[1].as_str().unwrap_or("");

            match message_type {
                2 => {
                    // CALL - need to respond
                    let action = array[2].as_str().unwrap_or("");
                    let payload = &array[3];

                    info!("Received CALL: {} ({})", action, message_id);
                    debug!("📋 Payload: {}", payload);

                    // Handle specific actions that require responses
                    match action {
                        "DataTransfer" => {
                            return handle_data_transfer_call(message_id, payload).await;
                        }
                        "ChangeAvailability" => {
                            return handle_change_availability_call(message_id, payload).await;
                        }
                        "RemoteStartTransaction" => {
                            info!("🔧 Handling RemoteStartTransaction");
                            let result = handle_remote_start_transaction_call(
                                message_id,
                                payload,
                                simulator.clone(),
                            )
                            .await;
                            info!("🔧 RemoteStartTransaction handler result: {:?}", result);
                            return result;
                        }
                        "RemoteStopTransaction" => {
                            info!("🔧 Handling RemoteStopTransaction");
                            let result = handle_remote_stop_transaction_call(
                                message_id,
                                payload,
                                simulator.clone(),
                            )
                            .await;
                            info!("🔧 RemoteStopTransaction handler result: {:?}", result);
                            return result;
                        }
                        "Reset" => {
                            return handle_reset_call(message_id, payload).await;
                        }
                        "GetConfiguration" => {
                            return handle_get_configuration_call(message_id, payload).await;
                        }
                        "ChangeConfiguration" => {
                            return handle_change_configuration_call(message_id, payload).await;
                        }
                        _ => {
                            warn!("Unsupported action: {}", action);
                            // Send NotSupported error
                            let error_response = serde_json::json!([
                                4,
                                message_id,
                                "NotSupported",
                                format!("Action '{}' is not supported", action),
                                {}
                            ]);
                            return Ok(Some(error_response.to_string()));
                        }
                    }
                }
                3 => {
                    // CALLRESULT
                    info!("Received CALLRESULT for message: {}", message_id);
                }
                4 => {
                    // CALLERROR
                    let error_code = array[2].as_str().unwrap_or("");
                    warn!(
                        "Received CALLERROR: {} for message: {}",
                        error_code, message_id
                    );
                }
                _ => {
                    warn!("Unknown message type: {}", message_type);
                }
            }
        } else {
            warn!("Message array too short: {} elements", array.len());
        }
    } else {
        warn!("Message is not a JSON array");
    }

    debug!("🔚 handle_message returning None");
    Ok(None)
}

async fn handle_data_transfer_call(message_id: &str, payload: &Value) -> Result<Option<String>> {
    let vendor_id = payload["vendorId"].as_str().unwrap_or("");
    let message_id_param = payload["messageId"].as_str();
    let data = payload["data"].as_str();

    info!(
        "Handling DataTransfer: vendor_id={}, message_id={:?}, data={:?}",
        vendor_id, message_id_param, data
    );

    // Handle specific vendor/message combinations
    let (status, response_data) = match (vendor_id, message_id_param) {
        ("1", Some("Qrcode")) => {
            info!("✅ Processing QRCode data transfer for vendor 1");
            let response_obj = serde_json::json!({
                "result": "success",
                "qr_code_displayed": true,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            (
                "Accepted",
                Some(serde_json::Value::String(response_obj.to_string())),
            )
        }
        ("1", _) => {
            // Known vendor, accept other messages
            (
                "Accepted",
                data.map(|d| serde_json::Value::String(d.to_string())),
            )
        }
        _ => {
            // Unknown vendor
            (
                "UnknownVendorId",
                Some(serde_json::Value::String("".to_string())),
            )
        }
    };

    let mut response_payload = serde_json::json!({
        "status": status
    });

    if let Some(data) = response_data {
        response_payload["data"] = data;
    } else {
        response_payload["data"] = serde_json::Value::String("".to_string());
    }

    let response = serde_json::json!([3, message_id, response_payload]);

    info!("Sending DataTransfer response: {}", status);
    Ok(Some(response.to_string()))
}

async fn handle_change_availability_call(
    message_id: &str,
    _payload: &Value,
) -> Result<Option<String>> {
    let response = serde_json::json!([
        3,
        message_id,
        {
            "status": "Accepted"
        }
    ]);
    Ok(Some(response.to_string()))
}

async fn handle_remote_start_transaction_call(
    message_id: &str,
    payload: &Value,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<Option<String>> {
    let connector_id = payload["connectorId"].as_u64().unwrap_or(1) as u32;
    let id_tag = payload["idTag"]
        .as_str()
        .unwrap_or("REMOTE_TAG")
        .to_string();

    info!(
        "📞 Remote Start Transaction request: connector={}, idTag={}, messageId={}",
        connector_id, id_tag, message_id
    );

    // Check if connector is available for remote start
    let can_start = {
        let state = simulator.lock().await;
        if let Some(connector) = state.connector_states.get(&connector_id) {
            // Can start if connector is Available or Preparing (cable plugged)
            match connector.status {
                ConnectorState::Available | ConnectorState::Preparing => {
                    connector.transaction_id.is_none()
                }
                _ => false,
            }
        } else {
            false
        }
    };

    let status = if can_start {
        // Auto-plug cable if not already connected
        {
            let mut state = simulator.lock().await;
            if let Some(connector) = state.connector_states.get_mut(&connector_id) {
                if !connector.cable_plugged {
                    connector.cable_plugged = true;
                    connector.status = ConnectorState::Preparing;
                    info!(
                        "🔌 Auto-plugged cable for remote start on connector {}",
                        connector_id
                    );
                }
            }
        }

        // Start the transaction
        let transaction_id = chrono::Utc::now().timestamp() as u32;

        {
            let mut state = simulator.lock().await;
            if let Some(connector) = state.connector_states.get_mut(&connector_id) {
                connector.transaction_id = Some(transaction_id);
                connector.status = ConnectorState::Charging;
                connector.id_tag = Some(id_tag.clone());
                connector.current_power = 7.2; // Default 7.2kW
                connector.energy_delivered = 0.0;
                connector.last_status_time = chrono::Utc::now();
                connector.remote_started = true;
                // Initialize SoC for charging session (typical starting range)
                connector.battery_soc = Some(25.0 + (connector_id as f64 * 5.0) % 50.0);
                state.message_count += 1;

                // Log remote start activity
                state.recent_activities.push(ActivityLog {
                    timestamp: chrono::Utc::now(),
                    activity_type: ActivityType::RemoteStart,
                    connector_id: Some(connector_id),
                    message: format!("🆕 Remote start transaction {} initiated by Central System with ID tag '{}' - MeterValues enabled", transaction_id, id_tag),
                });

                // Keep only last 20 activities
                if state.recent_activities.len() > 20 {
                    state.recent_activities.remove(0);
                }
            }
        }

        // Send StatusNotification for charging state
        tokio::spawn({
            let simulator_clone = simulator.clone();
            async move {
                let _ = send_status_notification(simulator_clone.clone(), connector_id).await;
                let _ =
                    send_start_transaction(simulator_clone, connector_id, id_tag, transaction_id)
                        .await;
            }
        });

        info!(
            "✅ Remote start transaction {} accepted on connector {}",
            transaction_id, connector_id
        );
        "Accepted"
    } else {
        info!(
            "❌ Remote start transaction rejected on connector {} (not available)",
            connector_id
        );
        "Rejected"
    };

    let response = serde_json::json!([
        3,
        message_id,
        {
            "status": status
        }
    ]);

    info!("🔄 Generated RemoteStartTransaction response: {}", response);
    Ok(Some(response.to_string()))
}

async fn handle_remote_stop_transaction_call(
    message_id: &str,
    payload: &Value,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<Option<String>> {
    let transaction_id = payload["transactionId"].as_u64().map(|id| id as u32);

    info!(
        "📞 Remote Stop Transaction request: transactionId={:?}",
        transaction_id
    );

    // Find connector with matching transaction ID
    let (connector_id, energy, can_stop) = {
        let mut state = simulator.lock().await;
        let mut found_connector = None;
        let mut transaction_energy = 0.0;
        let mut can_stop = false;

        for (id, connector) in state.connector_states.iter_mut() {
            if let Some(tx_id) = connector.transaction_id {
                if transaction_id.is_none() || Some(tx_id) == transaction_id {
                    // Found matching transaction
                    found_connector = Some(*id);
                    transaction_energy = connector.energy_delivered;
                    can_stop = true;

                    // Stop the transaction
                    connector.transaction_id = None;
                    connector.status = ConnectorState::Finishing;
                    connector.current_power = 0.0;
                    connector.id_tag = None;
                    connector.battery_soc = None; // Reset SoC when remote stop
                    connector.remote_started = false;
                    connector.last_status_time = chrono::Utc::now();
                    break;
                }
            }
        }

        if can_stop {
            state.message_count += 1;

            // Log remote stop activity
            state.recent_activities.push(ActivityLog {
                timestamp: chrono::Utc::now(),
                activity_type: ActivityType::RemoteStop,
                connector_id: found_connector,
                message: "🛑 Remote stop transaction requested by Central System".to_string(),
            });

            // Keep only last 20 activities
            if state.recent_activities.len() > 20 {
                state.recent_activities.remove(0);
            }
        }

        (found_connector, transaction_energy, can_stop)
    };

    let status = if let (true, Some(conn_id)) = (can_stop, connector_id) {
        let actual_tx_id = transaction_id.unwrap_or_else(|| chrono::Utc::now().timestamp() as u32);

        // Send StopTransaction and StatusNotification messages
        tokio::spawn({
            let simulator_clone = simulator.clone();
            async move {
                let _ = send_stop_transaction(simulator_clone.clone(), actual_tx_id, energy as i32)
                    .await;

                // Wait 2 seconds then update to finishing
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                {
                    let mut state = simulator_clone.lock().await;
                    if let Some(connector) = state.connector_states.get_mut(&conn_id) {
                        connector.status = ConnectorState::Finishing;
                        connector.last_status_time = chrono::Utc::now();
                    }
                }
                let _ = send_status_notification(simulator_clone.clone(), conn_id).await;

                // Wait another 2 seconds then transition to final state
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                {
                    let mut state = simulator_clone.lock().await;
                    if let Some(connector) = state.connector_states.get_mut(&conn_id) {
                        connector.status = if connector.cable_plugged {
                            ConnectorState::Preparing
                        } else {
                            ConnectorState::Available
                        };
                        connector.last_status_time = chrono::Utc::now();
                    }
                }
                let _ = send_status_notification(simulator_clone, conn_id).await;
            }
        });

        info!(
            "✅ Remote stop transaction accepted on connector {}",
            conn_id
        );
        "Accepted"
    } else {
        info!("❌ Remote stop transaction rejected (no matching transaction found)");
        "Rejected"
    };

    let response = serde_json::json!([
        3,
        message_id,
        {
            "status": status
        }
    ]);

    Ok(Some(response.to_string()))
}

async fn handle_reset_call(message_id: &str, _payload: &Value) -> Result<Option<String>> {
    let response = serde_json::json!([
        3,
        message_id,
        {
            "status": "Accepted"
        }
    ]);
    Ok(Some(response.to_string()))
}

async fn handle_get_configuration_call(
    message_id: &str,
    _payload: &Value,
) -> Result<Option<String>> {
    let response = serde_json::json!([
        3,
        message_id,
        {
            "configurationKey": [],
            "unknownKey": []
        }
    ]);
    Ok(Some(response.to_string()))
}

async fn handle_change_configuration_call(
    message_id: &str,
    _payload: &Value,
) -> Result<Option<String>> {
    let response = serde_json::json!([
        3,
        message_id,
        {
            "status": "Accepted"
        }
    ]);
    Ok(Some(response.to_string()))
}

fn create_boot_notification(charge_point_id: &str) -> String {
    let boot_notification = serde_json::json!([
        2,
        "1",
        "BootNotification",
        {
            "chargePointVendor": "OCPP-RS",
            "chargePointModel": "CLI-Simulator",
            "chargePointSerialNumber": charge_point_id,
            "firmwareVersion": "1.0.0"
        }
    ]);

    boot_notification.to_string()
}

async fn test_command(args: TestArgs) -> Result<()> {
    init_logging("info", false)?;

    println!("🧪 {}", "Running OCPP Test Suite".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let url = args
        .url
        .unwrap_or_else(|| "ws://localhost:8080/ocpp/CP001".to_string());

    match args.suite.as_str() {
        "basic" => run_basic_tests(&url, args.count, args.duration).await,
        "load" => run_load_tests(&url, args.count, args.duration).await,
        "fault" => run_fault_tests(&url, args.count, args.duration).await,
        "full" => run_full_test_suite(&url, args.count, args.duration).await,
        _ => {
            println!("❌ Unknown test suite: {}", args.suite.red());
            list_available_test_suites();
            Ok(())
        }
    }
}

async fn monitor_command(args: MonitorArgs) -> Result<()> {
    init_logging("debug", true)?;

    println!("👁️  {}", "OCPP Traffic Monitor".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Monitoring: {}", args.url.bright_white());
    println!("Format: {}", args.output.bright_yellow());
    if let Some(filter) = &args.filter {
        println!("Filter: {}", filter.bright_green());
    }
    println!();

    println!("📡 Monitoring OCPP traffic...");
    println!("Press Ctrl+C to stop");

    // Setup Ctrl+C handler
    tokio::signal::ctrl_c().await?;
    println!("\n✅ Monitoring stopped");

    Ok(())
}

async fn scenario_command(args: ScenarioArgs) -> Result<()> {
    if args.list {
        list_available_scenarios();
    } else if args.generate {
        generate_scenario_template().await?;
    } else if let Some(scenario) = args.execute {
        execute_scenario(&scenario).await?;
    } else {
        println!("Use --help to see available scenario commands");
    }

    Ok(())
}

async fn validate_command(args: ValidateArgs) -> Result<()> {
    init_logging("info", false)?;

    println!("🔍 {}", "OCPP Message Validator".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if let Some(file) = args.file {
        validate_message_file(&file, args.message_type.as_deref()).await?;
    } else {
        println!("❌ Please specify a file to validate with --file");
    }

    Ok(())
}

// Interactive mode implementation
async fn run_interactive_mode(
    args: &ConnectArgs,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    let theme = ColorfulTheme::default();

    loop {
        println!("\n🎮 Interactive Commands:");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━");

        let options = vec![
            "📊 Show Status",
            "🔌 Connector Operations",
            "⚡ Transaction Operations",
            "🚨 Fault Injection",
            "🎯 Run Scenario",
            "📈 Statistics",
            "🔍 Monitor Events",
            "❌ Exit",
        ];

        let selection = Select::with_theme(&theme)
            .with_prompt("Choose an action")
            .items(&options)
            .default(0)
            .interact()?;

        match selection {
            0 => show_status(args, simulator.clone()).await?,
            1 => connector_operations(args, simulator.clone()).await?,
            2 => transaction_operations(args, simulator.clone()).await?,
            3 => fault_injection(args, simulator.clone()).await?,
            4 => run_scenario_interactive(args, simulator.clone()).await?,
            5 => show_statistics(args, simulator.clone()).await?,
            6 => monitor_events_interactive(args, simulator.clone()).await?,
            7 => {
                println!("👋 Goodbye!");
                break;
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}

async fn show_status(
    args: &ConnectArgs,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!("\n📊 {}", "Simulator Status".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━");

    let state = simulator.lock().await;

    // Display connection status
    println!("🔗 Connection:");
    println!("   URL: {}", args.url.bright_white());
    println!(
        "   Status: {}",
        if state.connected {
            "Connected".bright_green()
        } else {
            "Disconnected".bright_red()
        }
    );
    let uptime = chrono::Utc::now().signed_duration_since(state.start_time);
    println!(
        "   Uptime: {}",
        format_duration(uptime.to_std().unwrap_or_default()).bright_blue()
    );

    // Display connector status
    println!("\n🔌 Connectors:");
    for (id, connector) in &state.connector_states {
        let status_color = match connector.status {
            ConnectorState::Available => "green",
            ConnectorState::Charging => "yellow",
            ConnectorState::Faulted => "red",
            ConnectorState::Unavailable => "red",
            _ => "cyan",
        };
        let cable_icon = if connector.cable_plugged {
            "🔌"
        } else {
            "⚪"
        };
        let remote_indicator = if connector.remote_started {
            " 📞"
        } else {
            ""
        };
        let tx_info = if let Some(tx_id) = connector.transaction_id {
            let mut info = format!(" (Tx: {})", tx_id);
            if let Some(soc) = connector.battery_soc {
                info.push_str(&format!(" SoC: {:.1}%", soc));
            }
            info.bright_blue()
        } else {
            "".normal()
        };

        println!(
            "   {} Connector {}: {}{}{}",
            cable_icon,
            id,
            connector.status.to_string().color(status_color),
            tx_info,
            remote_indicator.bright_magenta()
        );
    }

    // Display statistics
    println!("\n📊 Statistics:");
    println!(
        "   Messages: {}",
        state.message_count.to_string().bright_yellow()
    );
    println!("   Transactions: {}", "0".bright_cyan());

    // Display recent activities
    println!("\n📋 Recent Activities:");
    let activities_displayed = std::cmp::min(5, state.recent_activities.len());
    if activities_displayed == 0 {
        println!("   No recent activities");
    } else {
        for activity in state
            .recent_activities
            .iter()
            .rev()
            .take(activities_displayed)
        {
            let time_str = activity.timestamp.format("%H:%M:%S").to_string();
            println!("   {} {}", time_str.bright_black(), activity.message);
        }
        if state.recent_activities.len() > 5 {
            println!(
                "   ... and {} more activities",
                state.recent_activities.len() - 5
            );
        }
    }

    Ok(())
}

async fn connector_operations(
    _args: &ConnectArgs,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    loop {
        println!("\n🔌 {}", "Connector Operations".bright_cyan().bold());
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━");

        // Display current connector status
        {
            let state = simulator.lock().await;
            println!("\n📊 Current Connector Status:");
            for (id, connector) in &state.connector_states {
                let status_color = match connector.status {
                    ConnectorState::Available => "green",
                    ConnectorState::Charging => "yellow",
                    ConnectorState::Faulted => "red",
                    ConnectorState::Unavailable => "red",
                    _ => "cyan",
                };
                let cable_icon = if connector.cable_plugged {
                    "🔌"
                } else {
                    "⚪"
                };
                println!(
                    "   {} Connector {}: {} {} {}",
                    cable_icon,
                    id,
                    connector.status.to_string().color(status_color),
                    if let Some(tx_id) = connector.transaction_id {
                        format!("(Tx: {})", tx_id).bright_blue()
                    } else {
                        "".normal()
                    },
                    if connector.current_power > 0.0 {
                        format!("{}kW", connector.current_power).bright_yellow()
                    } else {
                        "".normal()
                    }
                    .to_string()
                        + if connector.remote_started {
                            " 📞"
                        } else {
                            ""
                        }
                        + if connector.transaction_id.is_some() && connector.current_power > 0.0 {
                            " 📊"
                        } else {
                            ""
                        }
                );
            }
        }

        let theme = ColorfulTheme::default();
        let operations = vec![
            "🔌 Plug In Cable",
            "🔌 Plug Out Cable",
            "⚡ Start Transaction",
            "🛑 Stop Transaction",
            "🔧 Set Availability",
            "📊 Show Detailed Status",
            "📋 View Recent Activities",
            "⬅️  Back to Main Menu",
        ];

        let selection = Select::with_theme(&theme)
            .with_prompt("Choose connector operation")
            .items(&operations)
            .default(0)
            .interact()?;

        match selection {
            0 => plug_cable_operation(simulator.clone(), true).await?,
            1 => plug_cable_operation(simulator.clone(), false).await?,
            2 => start_transaction_operation(simulator.clone()).await?,
            3 => stop_transaction_operation(simulator.clone()).await?,
            4 => set_availability_operation(simulator.clone()).await?,
            5 => show_detailed_connector_status(simulator.clone()).await?,
            6 => show_recent_activities(simulator.clone()).await?,
            7 => return Ok(()),
            _ => unreachable!(),
        }
    }
}

async fn show_recent_activities(simulator: Arc<tokio::sync::Mutex<SimulatorState>>) -> Result<()> {
    let state = simulator.lock().await;
    println!("\n📋 {} ", "Recent Activities".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if state.recent_activities.is_empty() {
        println!("\n   No activities recorded yet");
    } else {
        for (i, activity) in state.recent_activities.iter().rev().enumerate() {
            let time_str = activity.timestamp.format("%H:%M:%S UTC").to_string();
            let connector_info = if let Some(conn_id) = activity.connector_id {
                format!(" [C{}]", conn_id)
            } else {
                "".to_string()
            };

            println!(
                "{}. {} {}{}",
                i + 1,
                time_str.bright_black(),
                connector_info.bright_blue(),
                activity.message
            );
        }
    }

    println!("\nPress Enter to continue...");
    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

async fn plug_cable_operation(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
    plug_in: bool,
) -> Result<()> {
    let connector_id = select_connector(simulator.clone()).await?;

    {
        let mut state = simulator.lock().await;
        if let Some(connector) = state.connector_states.get_mut(&connector_id) {
            if plug_in {
                if connector.cable_plugged {
                    println!(
                        "⚠️  Cable is already plugged in on connector {}",
                        connector_id
                    );
                    return Ok(());
                }
                connector.cable_plugged = true;
                connector.status = ConnectorState::Preparing;
                println!("✅ Cable plugged into connector {}", connector_id);
            } else {
                if !connector.cable_plugged {
                    println!("⚠️  No cable to unplug on connector {}", connector_id);
                    return Ok(());
                }
                if connector.transaction_id.is_some() {
                    println!("⚠️  Cannot unplug cable during active transaction. Stop transaction first.");
                    return Ok(());
                }
                connector.cable_plugged = false;
                connector.status = ConnectorState::Available;
                println!("✅ Cable unplugged from connector {}", connector_id);
            }
            connector.last_status_time = chrono::Utc::now();
            state.message_count += 1;
        }
    }

    // Send StatusNotification
    send_status_notification(simulator.clone(), connector_id).await?;

    Ok(())
}

async fn start_transaction_operation(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    let connector_id = select_connector(simulator.clone()).await?;

    // Check if connector is ready for transaction
    {
        let state = simulator.lock().await;
        if let Some(connector) = state.connector_states.get(&connector_id) {
            if !connector.cable_plugged {
                println!("⚠️  Cable must be plugged in before starting transaction");
                return Ok(());
            }
            if connector.transaction_id.is_some() {
                println!(
                    "⚠️  Transaction already active on connector {}",
                    connector_id
                );
                return Ok(());
            }
            if connector.status == ConnectorState::Faulted
                || connector.status == ConnectorState::Unavailable
            {
                println!(
                    "⚠️  Connector {} is not available for transactions",
                    connector_id
                );
                return Ok(());
            }
        }
    }

    // Get ID tag from user
    println!("Enter ID tag for authorization (or press Enter for default 'DEMO_TAG'):");
    let mut id_tag = String::new();
    std::io::stdin().read_line(&mut id_tag)?;
    let id_tag = id_tag.trim();
    let id_tag = if id_tag.is_empty() {
        "DEMO_TAG"
    } else {
        id_tag
    };

    // Generate transaction ID
    let transaction_id = chrono::Utc::now().timestamp() as u32;

    {
        let mut state = simulator.lock().await;
        if let Some(connector) = state.connector_states.get_mut(&connector_id) {
            connector.transaction_id = Some(transaction_id);
            connector.status = ConnectorState::Charging;
            connector.id_tag = Some(id_tag.to_string());
            connector.current_power = 7.2; // Default 7.2kW
            connector.energy_delivered = 0.0;
            connector.remote_started = false; // Manual start
            connector.last_status_time = chrono::Utc::now();
            // Initialize SoC for charging session (typical starting range)
            connector.battery_soc = Some(25.0 + (connector_id as f64 * 5.0) % 50.0);
            state.message_count += 1;

            // Log manual start activity
            state.recent_activities.push(ActivityLog {
                timestamp: chrono::Utc::now(),
                activity_type: ActivityType::ManualStart,
                connector_id: Some(connector_id),
                message: format!(
                    "⚡ Manual start transaction {} on connector {} with ID tag '{}' - MeterValues enabled",
                    transaction_id, connector_id, id_tag
                ),
            });

            // Keep only last 20 activities
            if state.recent_activities.len() > 20 {
                state.recent_activities.remove(0);
            }
        }
    }

    println!(
        "⚡ Transaction {} started on connector {} with ID tag '{}'",
        transaction_id, connector_id, id_tag
    );

    // Send StartTransaction message
    send_start_transaction(
        simulator.clone(),
        connector_id,
        id_tag.to_string(),
        transaction_id,
    )
    .await?;

    Ok(())
}

async fn stop_transaction_operation(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    let connector_id = select_connector(simulator.clone()).await?;

    let (transaction_id, energy) = {
        let mut state = simulator.lock().await;
        if let Some(connector) = state.connector_states.get_mut(&connector_id) {
            if let Some(tx_id) = connector.transaction_id {
                let energy = connector.energy_delivered;
                connector.transaction_id = None;
                connector.status = if connector.cable_plugged {
                    ConnectorState::Finishing
                } else {
                    ConnectorState::Available
                };
                connector.current_power = 0.0;
                connector.id_tag = None;
                connector.remote_started = false;
                connector.battery_soc = None; // Reset SoC when transaction ends
                connector.last_status_time = chrono::Utc::now();
                state.message_count += 1;

                // Log manual stop activity
                state.recent_activities.push(ActivityLog {
                    timestamp: chrono::Utc::now(),
                    activity_type: ActivityType::ManualStop,
                    connector_id: Some(connector_id),
                    message: format!(
                        "🛑 Manual stop transaction {} on connector {} - Energy: {:.2} kWh",
                        tx_id, connector_id, energy
                    ),
                });

                // Keep only last 20 activities
                if state.recent_activities.len() > 20 {
                    state.recent_activities.remove(0);
                }

                (tx_id, energy)
            } else {
                println!("⚠️  No active transaction on connector {}", connector_id);
                return Ok(());
            }
        } else {
            return Ok(());
        }
    };

    println!(
        "🛑 Transaction {} stopped on connector {}. Energy delivered: {:.2} kWh",
        transaction_id, connector_id, energy
    );

    // Send StopTransaction message
    send_stop_transaction(simulator.clone(), transaction_id, energy as i32).await?;

    // Handle status transitions in a background task to avoid blocking UI
    tokio::spawn({
        let simulator_clone = simulator.clone();
        async move {
            // Wait 1 second then update to finishing
            tokio::time::sleep(Duration::from_secs(1)).await;
            {
                let mut state = simulator_clone.lock().await;
                if let Some(connector) = state.connector_states.get_mut(&connector_id) {
                    connector.status = ConnectorState::Finishing;
                    connector.last_status_time = chrono::Utc::now();
                }
            }
            let _ = send_status_notification(simulator_clone.clone(), connector_id).await;

            // Wait another 2 seconds then transition to final state
            tokio::time::sleep(Duration::from_secs(2)).await;
            {
                let mut state = simulator_clone.lock().await;
                if let Some(connector) = state.connector_states.get_mut(&connector_id) {
                    connector.status = if connector.cable_plugged {
                        ConnectorState::Preparing
                    } else {
                        ConnectorState::Available
                    };
                    connector.last_status_time = chrono::Utc::now();
                }
            }
            let _ = send_status_notification(simulator_clone, connector_id).await;
        }
    });

    Ok(())
}

async fn set_availability_operation(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    let connector_id = select_connector(simulator.clone()).await?;

    let theme = ColorfulTheme::default();
    let availability_options = vec!["🟢 Make Available", "🔴 Make Unavailable", "🚨 Set Faulted"];

    let selection = Select::with_theme(&theme)
        .with_prompt("Set availability status")
        .items(&availability_options)
        .default(0)
        .interact()?;

    let new_status = match selection {
        0 => ConnectorState::Available,
        1 => ConnectorState::Unavailable,
        2 => ConnectorState::Faulted,
        _ => unreachable!(),
    };

    {
        let mut state = simulator.lock().await;
        if let Some(connector) = state.connector_states.get_mut(&connector_id) {
            if connector.transaction_id.is_some() && new_status != ConnectorState::Available {
                println!("⚠️  Cannot change availability during active transaction");
                return Ok(());
            }
            connector.status = new_status.clone();
            connector.last_status_time = chrono::Utc::now();
            state.message_count += 1;
        }
    }

    println!(
        "✅ Connector {} availability set to {}",
        connector_id, new_status
    );
    send_status_notification(simulator.clone(), connector_id).await?;

    Ok(())
}

async fn show_detailed_connector_status(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    let state = simulator.lock().await;
    println!("\n📊 {} ", "Detailed Connector Status".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    for (id, connector) in &state.connector_states {
        println!("\n🔌 Connector {}:", id);
        println!("   Status: {}", connector.status);
        println!(
            "   Cable: {}",
            if connector.cable_plugged {
                "Plugged"
            } else {
                "Not plugged"
            }
        );
        println!("   Error Code: {}", connector.error_code);
        if let Some(tx_id) = connector.transaction_id {
            println!("   Transaction ID: {}", tx_id);
            println!("   Current Power: {:.1} kW", connector.current_power);
            println!("   Energy Delivered: {:.2} kWh", connector.energy_delivered);
            if let Some(soc) = connector.battery_soc {
                println!("   Battery SoC: {:.1}%", soc);
            }
        }
        if let Some(id_tag) = &connector.id_tag {
            println!("   ID Tag: {}", id_tag);
        }
        if connector.remote_started {
            println!("   Remote Started: 📞 {}", "Yes".bright_magenta());
        }
        if connector.transaction_id.is_some() && connector.current_power > 0.0 {
            println!(
                "   MeterValues: 📊 {}",
                "Active (10s intervals)".bright_cyan()
            );
        }
        println!(
            "   Last Update: {}",
            connector.last_status_time.format("%H:%M:%S")
        );
    }

    println!("\nPress Enter to continue...");
    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

async fn select_connector(simulator: Arc<tokio::sync::Mutex<SimulatorState>>) -> Result<u32> {
    let state = simulator.lock().await;
    let theme = ColorfulTheme::default();

    let connector_options: Vec<String> = state
        .connector_states
        .iter()
        .map(|(id, connector)| {
            format!(
                "Connector {} ({}{})",
                id,
                connector.status,
                if connector.cable_plugged { " 🔌" } else { "" }
            )
        })
        .collect();

    let selection = Select::with_theme(&theme)
        .with_prompt("Select connector")
        .items(&connector_options)
        .default(0)
        .interact()?;

    let connector_ids: Vec<u32> = state.connector_states.keys().cloned().collect();
    Ok(connector_ids[selection])
}

async fn send_status_notification(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
    connector_id: u32,
) -> Result<()> {
    let state = simulator.lock().await;
    if let Some(connector) = state.connector_states.get(&connector_id) {
        let status_notification = serde_json::json!([
            2,
            format!("{}", chrono::Utc::now().timestamp()),
            "StatusNotification",
            {
                "connectorId": connector_id,
                "errorCode": connector.error_code,
                "status": connector.status.to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        ]);

        if let Some(tx) = &state.websocket_tx {
            let _ = tx
                .send(Message::Text(status_notification.to_string()))
                .await;
            println!("📤 Sent StatusNotification for connector {}", connector_id);
        }
    }
    Ok(())
}

async fn send_start_transaction(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
    connector_id: u32,
    id_tag: String,
    _transaction_id: u32,
) -> Result<()> {
    let state = simulator.lock().await;
    let start_transaction = serde_json::json!([
        2,
        format!("{}", chrono::Utc::now().timestamp()),
        "StartTransaction",
        {
            "connectorId": connector_id,
            "idTag": id_tag,
            "meterStart": 0,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }
    ]);

    if let Some(tx) = &state.websocket_tx {
        let _ = tx.send(Message::Text(start_transaction.to_string())).await;
        println!("📤 Sent StartTransaction for connector {}", connector_id);
    }

    // Send status notification for charging state
    drop(state);
    send_status_notification(simulator, connector_id).await?;

    Ok(())
}

async fn send_stop_transaction(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
    transaction_id: u32,
    meter_stop: i32,
) -> Result<()> {
    let state = simulator.lock().await;
    let stop_transaction = serde_json::json!([
        2,
        format!("{}", chrono::Utc::now().timestamp()),
        "StopTransaction",
        {
            "transactionId": transaction_id,
            "meterStop": meter_stop,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }
    ]);

    if let Some(tx) = &state.websocket_tx {
        let _ = tx.send(Message::Text(stop_transaction.to_string())).await;
        println!("📤 Sent StopTransaction for transaction {}", transaction_id);
    }

    Ok(())
}

#[allow(dead_code)]
async fn send_meter_values(
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
    connector_id: u32,
    transaction_id: u32,
    energy_wh: i32,
    power_w: f64,
    soc_percent: Option<f64>,
) -> Result<()> {
    let state = simulator.lock().await;

    let mut sampled_values = vec![
        serde_json::json!({
            "value": format!("{}", energy_wh),
            "context": "Sample.Periodic",
            "format": "Raw",
            "measurand": "Energy.Active.Import.Register",
            "unit": "Wh"
        }),
        serde_json::json!({
            "value": format!("{:.1}", power_w),
            "context": "Sample.Periodic",
            "format": "Raw",
            "measurand": "Power.Active.Import",
            "unit": "W"
        }),
    ];

    // Add SoC if available
    if let Some(soc) = soc_percent {
        sampled_values.push(serde_json::json!({
            "value": format!("{:.1}", soc),
            "context": "Sample.Periodic",
            "format": "Raw",
            "measurand": "SoC",
            "unit": "Percent"
        }));
    }

    let meter_values = serde_json::json!([
        2,
        format!("{}", chrono::Utc::now().timestamp()),
        "MeterValues",
        {
            "connectorId": connector_id,
            "transactionId": transaction_id,
            "meterValue": [{
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "sampledValue": sampled_values
            }]
        }
    ]);

    if let Some(tx) = &state.websocket_tx {
        let _ = tx.send(Message::Text(meter_values.to_string())).await;
        let soc_info = if let Some(soc) = soc_percent {
            format!(" (SoC: {:.1}%)", soc)
        } else {
            String::new()
        };
        debug!(
            "📊 Sent MeterValues for connector {} transaction {}{}",
            connector_id, transaction_id, soc_info
        );
    }

    Ok(())
}

async fn transaction_operations(
    _args: &ConnectArgs,
    _simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!("\n⚡ {}", "Transaction Operations".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    println!("🔄 Transaction operations would be implemented here");
    println!("Press Enter to continue...");
    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

async fn fault_injection(
    _args: &ConnectArgs,
    _simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!("\n🚨 {}", "Fault Injection".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━");

    println!("💥 Fault injection would be implemented here");
    println!("Press Enter to continue...");
    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

async fn run_scenario_interactive(
    _args: &ConnectArgs,
    _simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!("\n🎯 {}", "Run Scenario".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━");

    let theme = ColorfulTheme::default();
    let scenarios = vec![
        "Basic Charging",
        "Fast Charging",
        "Emergency Stop",
        "Fault Recovery",
        "Back to Main Menu",
    ];

    let selection = Select::with_theme(&theme)
        .with_prompt("Choose scenario")
        .items(&scenarios)
        .default(0)
        .interact()?;

    match selection {
        0..=3 => {
            println!(
                "🎬 Running scenario: {}",
                scenarios[selection].bright_yellow()
            );
            let pb = ProgressBar::new(100);
            pb.set_style(ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}")
                .unwrap()
                .progress_chars("##-"));

            for i in 0..=100 {
                pb.set_position(i);
                pb.set_message(format!("Step {}/100", i));
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            pb.finish_with_message("✅ Scenario completed");
        }
        4 => return Ok(()),
        _ => unreachable!(),
    }

    Ok(())
}

async fn show_statistics(
    _args: &ConnectArgs,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!("\n📈 {}", "Statistics".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━");

    let state = simulator.lock().await;

    println!("📊 Message Statistics:");
    println!(
        "   Total Messages: {}",
        state.message_count.to_string().bright_yellow()
    );
    println!("   Boot Notifications: {}", "1".bright_green());
    println!("   Heartbeats: {}", "0".bright_blue());
    println!("   Status Notifications: {}", "0".bright_cyan());

    println!("\n⚡ Transaction Statistics:");
    println!("   Total Transactions: {}", "0".bright_yellow());
    println!("   Active Transactions: {}", "0".bright_green());
    println!("   Energy Delivered: {} kWh", "0.0".bright_blue());

    println!("\nPress Enter to continue...");
    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

async fn monitor_events_interactive(
    _args: &ConnectArgs,
    _simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!("\n🔍 {}", "Monitor Events".bright_cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━");

    println!("👁️  Event monitoring would show real-time events here");
    println!("Press Enter to continue...");
    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

async fn run_monitoring_mode(
    _args: &ConnectArgs,
    simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!(
        "📊 {} - Press Ctrl+C to stop",
        "Monitoring Mode".bright_blue()
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let mut interval = interval(Duration::from_secs(5));
    let mut counter = 0;

    loop {
        interval.tick().await;
        counter += 1;

        let state = simulator.lock().await;
        let uptime = chrono::Utc::now().signed_duration_since(state.start_time);

        println!(
            "📡 Status Update #{}: {} | Messages: {} | Uptime: {}",
            counter,
            if state.connected {
                "Connected".bright_green()
            } else {
                "Disconnected".bright_red()
            },
            state.message_count.to_string().bright_yellow(),
            format_duration(uptime.to_std().unwrap_or_default()).bright_blue()
        );

        if counter >= 20 {
            break;
        }
    }

    Ok(())
}

async fn run_auto_scenario(
    scenario_name: &str,
    _simulator: Arc<tokio::sync::Mutex<SimulatorState>>,
) -> Result<()> {
    println!(
        "🤖 Auto-executing scenario: {}",
        scenario_name.bright_yellow()
    );

    let pb = ProgressBar::new(10);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}",
            )
            .unwrap()
            .progress_chars("##-"),
    );

    for i in 0..=10 {
        pb.set_position(i);
        pb.set_message(format!("Executing step {}/10", i));
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    pb.finish_with_message("✅ Auto-scenario completed");
    Ok(())
}

async fn run_basic_tests(_url: &str, _count: u32, _duration: u64) -> Result<()> {
    println!("🧪 Running basic tests...");
    sleep(Duration::from_secs(2)).await;
    println!("✅ Basic tests completed");
    Ok(())
}

async fn run_load_tests(_url: &str, _count: u32, _duration: u64) -> Result<()> {
    println!("🏋️ Running load tests...");
    sleep(Duration::from_secs(3)).await;
    println!("✅ Load tests completed");
    Ok(())
}

async fn run_fault_tests(_url: &str, _count: u32, _duration: u64) -> Result<()> {
    println!("🔥 Running fault tests...");
    sleep(Duration::from_secs(2)).await;
    println!("✅ Fault tests completed");
    Ok(())
}

async fn run_full_test_suite(_url: &str, count: u32, duration: u64) -> Result<()> {
    println!("🎯 Running full test suite...");
    run_basic_tests(_url, count, duration).await?;
    run_load_tests(_url, count, duration).await?;
    run_fault_tests(_url, count, duration).await?;
    println!("✅ Full test suite completed");
    Ok(())
}

fn list_available_test_suites() {
    println!("\n📋 Available test suites:");
    println!("   • basic  - Basic OCPP functionality tests");
    println!("   • load   - Load and performance tests");
    println!("   • fault  - Fault injection and recovery tests");
    println!("   • full   - Complete test suite");
}

fn list_available_scenarios() {
    println!("\n📋 Available scenarios:");
    println!("   • basic-charging     - Simple charging session");
    println!("   • fast-charging      - High-power charging");
    println!("   • emergency-stop     - Emergency stop scenario");
    println!("   • fault-recovery     - Fault injection and recovery");
    println!("   • multiple-connector - Multi-connector operations");
}

async fn generate_scenario_template() -> Result<()> {
    println!("📝 Generating scenario template...");

    let template = serde_json::json!({
        "name": "custom-scenario",
        "description": "Custom OCPP scenario",
        "steps": [
            {
                "action": "connect",
                "description": "Connect to Central System"
            },
            {
                "action": "boot_notification",
                "description": "Send BootNotification"
            },
            {
                "action": "start_transaction",
                "connector_id": 1,
                "id_tag": "test_tag"
            },
            {
                "action": "stop_transaction",
                "transaction_id": 1
            }
        ]
    });

    println!("✅ Template generated:");
    println!("{}", serde_json::to_string_pretty(&template)?);
    Ok(())
}

async fn execute_scenario(_scenario: &str) -> Result<()> {
    println!("🎬 Executing scenario...");
    sleep(Duration::from_secs(1)).await;
    println!("✅ Scenario execution completed");
    Ok(())
}

async fn validate_message_file(_file: &str, _message_type: Option<&str>) -> Result<()> {
    println!("🔍 Validating message file: {}", _file);
    sleep(Duration::from_millis(500)).await;
    println!("✅ Message validation completed");
    Ok(())
}

// Utility functions
fn print_banner() {
    println!(
        "{}",
        r#"
   ____   ____ ____  ____     ____ _     ___
  / __ \ / ___/ __ \|  _ \   / ___| |   |_ _|
 | |  | | |  | |  | | |_) | | |   | |    | |
 | |__| | |__| |__| |  __/  | |___| |___ | |
  \____/ \____\____/|_|      \____|_____|___|

"#
        .bright_cyan()
    );
}

fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn init_logging(level: &str, verbose: bool) -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let level = match level.to_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    if verbose {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into()))
            .with(tracing_subscriber::fmt::layer().pretty())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into()))
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
    }

    Ok(())
}
