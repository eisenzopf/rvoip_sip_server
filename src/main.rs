use anyhow::{Context, Result};
use clap::{Arg, Command};
use daemonize::Daemonize;
use log::{error, info};
use std::net::UdpSocket;

use signal_hook::consts::SIGTERM;
use signal_hook_tokio::{Handle, Signals};
use tokio_stream::StreamExt;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

mod config;
mod tone_generator;
mod logger;
mod call_handler;

use config::ServerConfig;
use tone_generator::ToneGenerator;
use call_handler::CallHandler;

const DEFAULT_CONFIG_PATH: &str = "/etc/rvoip-sip-server/config.toml";
const DEFAULT_LOG_PATH: &str = "/var/log/rvoip-sip-server/server.log";
const DEFAULT_PID_PATH: &str = "/var/run/rvoip-sip-server.pid";

#[derive(Debug)]
pub struct SipServerState {
    pub config: ServerConfig,
    pub tone_generator: Arc<ToneGenerator>,
    pub call_handler: Arc<CallHandler>,
    pub running: Arc<RwLock<bool>>,
}

impl SipServerState {
    pub fn new(config: ServerConfig) -> Self {
        let tone_generator = Arc::new(ToneGenerator::new());
        let call_handler = Arc::new(CallHandler::new(tone_generator.clone()));
        
        Self {
            config,
            tone_generator,
            call_handler,
            running: Arc::new(RwLock::new(false)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("rvoip-sip-server")
        .version("0.1.0")
        .about("Auto-answering SIP server with tone generation")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
                .default_value(DEFAULT_CONFIG_PATH),
        )
        .arg(
            Arg::new("daemon")
                .short('d')
                .long("daemon")
                .help("Run as daemon")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("log-file")
                .short('l')
                .long("log-file")
                .value_name("FILE")
                .help("Log file path")
                .default_value(DEFAULT_LOG_PATH),
        )
        .arg(
            Arg::new("pid-file")
                .short('p')
                .long("pid-file")
                .value_name("FILE")
                .help("PID file path")
                .default_value(DEFAULT_PID_PATH),
        )
        .get_matches();

    let config_path = matches.get_one::<String>("config").unwrap();
    let log_file = matches.get_one::<String>("log-file").unwrap();
    let pid_file = matches.get_one::<String>("pid-file").unwrap();
    let daemon_mode = matches.get_flag("daemon");

    // Initialize logging
    logger::init_logger(log_file, daemon_mode)?;

    // Load configuration
    let config = ServerConfig::load_from_file(config_path)
        .with_context(|| format!("Failed to load config from {}", config_path))?;

    info!("Starting rvoip SIP server v0.1.0");
    info!("Configuration loaded from: {}", config_path);

    // Validate configuration
    config.validate()?;

    if daemon_mode {
        info!("Starting in daemon mode");
        
        // Create necessary directories
        if let Some(parent) = Path::new(log_file).parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = Path::new(pid_file).parent() {
            fs::create_dir_all(parent)?;
        }

        let daemonize = Daemonize::new()
            .pid_file(pid_file)
            .chown_pid_file(true)
            .working_directory("/")
            .umask(0o027)
            .privileged_action(|| "Executed before drop privileges");

        match daemonize.start() {
            Ok(_) => info!("Daemon started successfully"),
            Err(e) => {
                error!("Failed to daemonize: {}", e);
                return Err(e.into());
            }
        }
    }

    // Initialize server state
    let server_state = Arc::new(SipServerState::new(config));

    // Set up signal handlers
    let mut signals = Signals::new(&[SIGTERM])?;
    let handle = signals.handle();
    
    let server_state_clone = server_state.clone();
    let signal_task = tokio::spawn(async move {
        handle_signals(&mut signals, server_state_clone, handle).await;
    });

    // Start the SIP server
    let server_result = run_sip_server(server_state.clone()).await;

    // Wait for signal handling to complete
    signal_task.abort();

    match server_result {
        Ok(_) => {
            info!("SIP server shutdown gracefully");
            Ok(())
        }
        Err(e) => {
            error!("SIP server error: {}", e);
            Err(e)
        }
    }
}

async fn run_sip_server(server_state: Arc<SipServerState>) -> Result<()> {
    let config = &server_state.config;
    
    info!("Initializing SIP server on {}:{}", config.sip.bind_address, config.sip.port);

    // Create a basic UDP socket for SIP communication
    let bind_addr = format!("{}:{}", config.sip.bind_address, config.sip.port);
    let _socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("Failed to bind to {}", bind_addr))?;
    
    info!("SIP server started successfully");
    info!("Listening on {}:{}", config.sip.bind_address, config.sip.port);
    info!("Domain: {}", config.sip.domain);
    info!("Auto-answer enabled: {}", config.behavior.auto_answer);
    info!("Tone duration: {}s", config.behavior.tone_duration_seconds);

    // Mark server as running
    {
        let mut running = server_state.running.write().await;
        *running = true;
    }

    // Main server loop - simplified for demonstration
    loop {
        {
            let running = server_state.running.read().await;
            if !*running {
                break;
            }
        }

        // Simulate call handling
        if let Err(e) = handle_incoming_calls(server_state.clone()).await {
            error!("Error handling incoming calls: {}", e);
            sleep(Duration::from_secs(1)).await;
        }

        // Small delay to prevent busy waiting
        sleep(Duration::from_millis(100)).await;
    }

    info!("Stopping SIP server");
    Ok(())
}

async fn handle_incoming_calls(server_state: Arc<SipServerState>) -> Result<()> {
    // This is a simplified call handling - in a real implementation,
    // you would need to implement proper SIP message parsing and handling
    // For now, we'll just log that we're ready to handle calls and simulate
    // some basic call processing
    
    // Simulate handling a call every 30 seconds for demonstration
    use std::sync::Mutex;
    use std::time::Instant;
    
    static LAST_CALL_TIME: Mutex<Option<Instant>> = Mutex::new(None);
    
    let should_simulate_call = {
        let mut last_time = LAST_CALL_TIME.lock().unwrap();
        match *last_time {
            None => {
                *last_time = Some(Instant::now());
                true
            }
            Some(time) if time.elapsed() > Duration::from_secs(30) => {
                *last_time = Some(Instant::now());
                true
            }
            _ => false,
        }
    };
    
    if should_simulate_call {
        // Simulate an incoming call
        if let Err(e) = server_state.call_handler
            .handle_incoming_call("sip:test@example.com", "sip:server@localhost")
            .await
        {
            error!("Failed to handle simulated call: {}", e);
        }
    }
    
    Ok(())
}

async fn handle_signals(
    signals: &mut Signals,
    server_state: Arc<SipServerState>,
    handle: Handle,
) {
    while let Some(signal) = signals.next().await {
        match signal {
            SIGTERM => {
                info!("Received SIGTERM, shutting down gracefully");
                let mut running = server_state.running.write().await;
                *running = false;
                break;
            }
            _ => {}
        }
    }
    
    handle.close();
} 