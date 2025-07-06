use std::fs;
use std::path::Path;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Arg, Command};
use daemonize::Daemonize;
use log::{info, error};
use signal_hook::consts::SIGTERM;
use signal_hook_tokio::Signals;
use tokio::sync::{RwLock, Mutex};
use tokio::time::Instant;
use tokio_stream::StreamExt;

// rvoip client imports
use rvoip::client_core::{
    ClientBuilder, ClientManager, ClientEventHandler, 
    CallId, CallState, CallStatusInfo, RegistrationStatusInfo, MediaEventInfo,
    CallAction, ClientError, IncomingCallInfo
};

mod config;
mod logger;
mod mp3_handler;

use config::ServerConfig;
use mp3_handler::Mp3Handler;

const DEFAULT_CONFIG_PATH: &str = "/etc/rvoip-sip-server/config.toml";
const DEFAULT_LOG_PATH: &str = "/var/log/rvoip-sip-server/server.log";
const DEFAULT_PID_PATH: &str = "/var/run/rvoip-sip-server.pid";

/// Auto-answering SIP server handler
#[derive(Clone)]
struct AutoAnswerHandler {
    client_manager: Arc<RwLock<Option<Arc<ClientManager>>>>,
    mp3_handler: Arc<Mp3Handler>,
    server_config: Arc<ServerConfig>,
    active_calls: Arc<Mutex<std::collections::HashMap<CallId, tokio::time::Instant>>>,
    call_stats: Arc<Mutex<CallStats>>,
    // Pre-converted μ-law samples for MP3 playback
    audio_samples: Arc<Mutex<Option<Vec<u8>>>>,
}

#[derive(Debug, Default)]
struct CallStats {
    total_calls: u64,
    answered_calls: u64,
    failed_calls: u64,
    active_calls: u32,
}

impl AutoAnswerHandler {
    pub fn new(mp3_handler: Arc<Mp3Handler>, server_config: Arc<ServerConfig>) -> Self {
        Self {
            client_manager: Arc::new(RwLock::new(None)),
            mp3_handler,
            server_config,
            active_calls: Arc::new(Mutex::new(std::collections::HashMap::new())),
            call_stats: Arc::new(Mutex::new(CallStats::default())),
            audio_samples: Arc::new(Mutex::new(None)),
        }
    }
    
    pub async fn set_client_manager(&self, client: Arc<ClientManager>) {
        *self.client_manager.write().await = Some(client);
    }

    /// Set up event handling with the client
    pub async fn set_event_handler(&self) {
        if let Some(_client) = self.client_manager.read().await.as_ref() {
            // The handler is already set up through ClientEventHandler trait implementation
            info!("📡 Event handler configured for client");
        }
    }

    /// Prepare audio samples for transmission (called during initialization)
    pub async fn prepare_audio_samples(&self) -> Result<(), anyhow::Error> {
        info!("📡 Preparing MP3 audio samples for transmission...");
        
        // Load PCM samples from WAV file
        let pcm_samples = self.mp3_handler.read_wav_samples()?;
        
        // Convert PCM samples to μ-law for PCMU codec
        let mulaw_samples = self.mp3_handler.pcm_to_mulaw(&pcm_samples);
        
        info!("🔄 Converted {} PCM samples to {} μ-law samples for RTP transmission", 
              pcm_samples.len(), mulaw_samples.len());
        
        // Store the samples for later use
        *self.audio_samples.lock().await = Some(mulaw_samples);
        
        info!("✅ Audio samples prepared and ready for transmission");
        Ok(())
    }

    /// Start custom audio transmission using pre-converted μ-law samples
    async fn start_custom_audio_transmission(&self, call_id: &CallId) -> Result<(), anyhow::Error> {
        info!("🎵 Starting custom audio transmission for call {}", call_id);
        
        // Get the pre-converted audio samples
        let samples = {
            let audio_samples_guard = self.audio_samples.lock().await;
            match audio_samples_guard.as_ref() {
                Some(samples) => samples.clone(),
                None => {
                    anyhow::bail!("Audio samples not prepared. Call prepare_audio_samples() first.");
                }
            }
        };
        
        info!("📡 Using {} pre-converted μ-law samples for call {}", samples.len(), call_id);
        
        // Use the new rvoip API to start custom audio transmission
        if let Some(client) = self.client_manager.read().await.as_ref() {
            client.start_audio_transmission_with_custom_audio(call_id, samples, false).await
                .context("Failed to start custom audio transmission")?;
                
            info!("✅ Custom audio transmission started successfully for call {}", call_id);
            
            // Schedule call hangup after MP3 duration (30 seconds)
            let call_id = call_id.clone();
            let client_ref = Arc::clone(&self.client_manager);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                
                if let Some(client) = client_ref.read().await.as_ref() {
                    info!("📴 Hanging up call {} after MP3 completion", call_id);
                    match client.hangup_call(&call_id).await {
                        Ok(_) => info!("✅ Call {} hung up successfully after MP3 playback", call_id),
                        Err(e) => error!("❌ Failed to hang up call {}: {}", call_id, e),
                    }
                }
            });
        } else {
            anyhow::bail!("Client manager not available");
        }
        
        Ok(())
    }
}

#[async_trait::async_trait]
impl ClientEventHandler for AutoAnswerHandler {
    async fn on_incoming_call(&self, call_info: IncomingCallInfo) -> CallAction {
        info!("📞 Incoming call: {} from {} to {}", call_info.call_id, call_info.caller_uri, call_info.callee_uri);
        
        // Track the call
        {
            let mut stats = self.call_stats.lock().await;
            stats.total_calls += 1;
            stats.active_calls += 1;
        }
        
        {
            let mut active_calls = self.active_calls.lock().await;
            active_calls.insert(call_info.call_id, Instant::now());
        }
        
        // Auto-answer if enabled
        if self.server_config.behavior.auto_answer {
            info!("⏱️ Auto-answering call {} in {}ms", call_info.call_id, self.server_config.behavior.auto_answer_delay_ms);
            
            let delay = Duration::from_millis(self.server_config.behavior.auto_answer_delay_ms);
            let client_manager = self.client_manager.clone();
            let call_id = call_info.call_id;
            
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                
                if let Some(client) = client_manager.read().await.as_ref() {
                    info!("📞 Auto-answering call: {}", call_id);
                    
                    match client.answer_call(&call_id).await {
                        Ok(_) => info!("✅ Call {} answered successfully", call_id),
                        Err(e) => error!("❌ Failed to answer call {}: {}", call_id, e),
                    }
                }
            });
        }
        
        CallAction::Ignore // Let the async auto-answer logic handle it
    }

    async fn on_call_state_changed(&self, status_info: CallStatusInfo) {
        let state_icon = match status_info.new_state {
            CallState::Initiating => "🔄",
            CallState::Ringing => "🔔", 
            CallState::Connected => "✅",
            CallState::Failed => "❌",
            CallState::Cancelled => "🚫",
            CallState::Terminated => "📴",
            CallState::Proceeding => "⏳",
            CallState::Terminating => "⏹️",
            CallState::IncomingPending => "📞",
        };
        
        info!("📱 Call {} state changed to {:?} {}", 
              status_info.call_id, status_info.new_state, state_icon);

        if status_info.new_state == CallState::Connected {
            info!("🎉 Call {} connected! Starting audio session...", status_info.call_id);
            
            // Get media info
            if let Some(client) = self.client_manager.read().await.as_ref() {
                if let Ok(media_info) = client.get_call_media_info(&status_info.call_id).await {
                    info!("📊 Media info for call {} - Local RTP: {:?}, Remote RTP: {:?}, Codec: {:?}",
                        status_info.call_id, media_info.local_rtp_port, media_info.remote_rtp_port, media_info.codec);
                }
                
                // Start custom MP3 audio transmission
                match self.start_custom_audio_transmission(&status_info.call_id).await {
                    Ok(_) => {
                        info!("✅ Started custom MP3 audio transmission for call {}", status_info.call_id);
                    }
                    Err(e) => {
                        error!("❌ Failed to start custom audio transmission: {}", e);
                        
                        // Fallback: try tone generation for testing
                        info!("🔄 Attempting fallback to tone generation...");
                        match client.start_audio_transmission_with_tone(&status_info.call_id).await {
                            Ok(_) => info!("✅ Fallback tone generation started for call {}", status_info.call_id),
                            Err(e2) => {
                                error!("❌ Fallback tone generation also failed: {}", e2);
                                
                                // Final fallback: try normal pass-through mode  
                                info!("🔄 Attempting final fallback to pass-through mode...");
                                match client.start_audio_transmission(&status_info.call_id).await {
                                    Ok(_) => info!("✅ Pass-through audio transmission started for call {}", status_info.call_id),
                                    Err(e3) => error!("❌ All audio transmission methods failed for call {}: {}", status_info.call_id, e3),
                                }
                            }
                        }
                    }
                }
            }
        } else if status_info.new_state == CallState::Terminated {
            info!("📴 Call {} terminated", status_info.call_id);
            
            // Remove from active calls and update statistics
            {
                let mut active_calls = self.active_calls.lock().await;
                if let Some(start_time) = active_calls.remove(&status_info.call_id) {
                    let duration = start_time.elapsed();
                    info!("⏱️ Call {} duration: {:?}", status_info.call_id, duration);
                }
            }
            
            {
                let mut stats = self.call_stats.lock().await;
                stats.active_calls = stats.active_calls.saturating_sub(1);
            }
        }
    }

    async fn on_media_event(&self, event: MediaEventInfo) {
        info!("🎵 Media event for call {}: {:?}", event.call_id, event.event_type);
    }

    async fn on_registration_status_changed(&self, _status_info: RegistrationStatusInfo) {
        // Not needed for auto-answering server
    }

    async fn on_client_error(&self, error: ClientError, call_id: Option<CallId>) {
        error!("❌ Client error on call {:?}: {}", call_id, error);
        
        if call_id.is_some() {
            let mut stats = self.call_stats.lock().await;
            stats.failed_calls += 1;
            stats.active_calls = stats.active_calls.saturating_sub(1);
        }
    }

    async fn on_network_event(&self, connected: bool, reason: Option<String>) {
        let status = if connected { "🌐 Connected" } else { "🔌 Disconnected" };
        info!("{} Network status changed", status);
        if let Some(reason) = reason {
            info!("💬 Reason: {}", reason);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("rvoip-sip-server")
        .version("0.1.0")
        .about("Auto-answering SIP server with tone generation using rvoip")
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
    let server_config = Arc::new(ServerConfig::load_from_file(config_path)
        .with_context(|| format!("Failed to load config from {}", config_path))?);

    info!("🚀 Starting rvoip auto-answering SIP server v0.1.0");
    info!("📁 Configuration loaded from: {}", config_path);

    // Validate configuration
    server_config.validate()?;

    if daemon_mode {
        info!("🔧 Starting in daemon mode");
        
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
            Ok(_) => info!("✅ Daemon started successfully"),
            Err(e) => {
                error!("❌ Failed to daemonize: {}", e);
                return Err(e.into());
            }
        }
    }

    // Create MP3 handler and initialize MP3 file
    let mut mp3_handler = Mp3Handler::new();
    
    info!("📥 Initializing MP3 file...");
    mp3_handler.ensure_mp3_downloaded().await
        .context("Failed to download MP3 file")?;
    
    mp3_handler.convert_mp3_to_wav(
        server_config.media.audio_sample_rate,
        1 // Mono channel for telephony
    ).context("Failed to convert MP3 to WAV")?;
    
    info!("✅ MP3 file ready for playback with telephony optimization");
    
    // Wrap in Arc after processing
    let mp3_handler = Arc::new(mp3_handler);
    
    log_server_configuration(&server_config);

    // Create rvoip client using updated API
    let sip_addr: SocketAddr = format!("{}:{}", server_config.sip.bind_address, server_config.sip.port)
        .parse()
        .context("Failed to parse SIP address")?;
    
    let media_addr: SocketAddr = format!("{}:0", server_config.sip.bind_address)
        .parse()
        .context("Failed to parse media address")?;
    
    // Health endpoint address
    let health_addr: SocketAddr = format!("127.0.0.1:{}", server_config.health.health_check_port)
        .parse()
        .context("Failed to parse health address")?;
    
    info!("⚙️ rvoip client configuration:");
    info!("   📡 SIP address: {}", sip_addr);
    info!("   🎵 Media address: {}", media_addr);
    info!("   🌐 Domain: {}", server_config.sip.domain);
    
    // Create handler and client using updated API
    let handler = Arc::new(AutoAnswerHandler::new(mp3_handler, server_config.clone()));
    
    // Prepare audio samples for transmission
    info!("🎵 Preparing audio samples for transmission...");
    handler.prepare_audio_samples().await
        .context("Failed to prepare audio samples")?;
    info!("✅ Audio samples ready for real-time transmission");
    
    let client = ClientBuilder::new()
        .local_address(sip_addr)         // SIP bind address
        .media_address(media_addr)       // Media bind address (0.0.0.0:0 for auto)
        .domain(&server_config.sip.domain)
        .build()
        .await
        .context("Failed to create client")?;

    handler.set_client_manager(client.clone()).await;
    
    // Set up the event handler with the client
    client.set_event_handler(handler.clone()).await;

    // Start health endpoint server (simple HTTP server)
    let health_addr_clone = health_addr;
    let handler_clone = handler.clone();
    let health_server = tokio::spawn(async move {
        use tokio::net::TcpListener;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        
        let listener = TcpListener::bind(&health_addr_clone).await.unwrap();
        info!("🏥 Health endpoint started on http://{}/health", health_addr_clone);
        
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                let handler = handler_clone.clone();
                tokio::spawn(async move {
                    let mut buf_reader = tokio::io::BufReader::new(&mut stream);
                    let mut request_line = String::new();
                    
                    if buf_reader.read_line(&mut request_line).await.is_ok() {
                        if request_line.contains("GET /health") {
                            let stats = handler.call_stats.lock().await;
                            
                            let health_response = format!(
                                r#"{{"status":"healthy","active_calls":{},"total_calls":{}}}"#,
                                stats.active_calls, stats.total_calls
                            );
                            
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                health_response.len(), health_response
                            );
                            
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else {
                            let response = "HTTP/1.1 404 Not Found\r\n\r\n";
                            let _ = stream.write_all(response.as_bytes()).await;
                        }
                    }
                });
            }
        }
    });

    client.start().await.context("Failed to start client")?;
    
    // Signal handling for graceful shutdown
    let mut signals = Signals::new(&[SIGTERM])?;
    let handle = signals.handle();
    let running = Arc::new(RwLock::new(true));
    
    let running_clone = Arc::clone(&running);
    let signal_task = tokio::spawn(async move {
        while let Some(signal) = signals.next().await {
            match signal {
                SIGTERM => {
                    info!("Received SIGTERM, shutting down gracefully...");
                    *running_clone.write().await = false;
                    break;
                }
                _ => {}
            }
        }
    });

    info!("✅ rvoip auto-answering SIP server started successfully!");
    info!("📞 Ready to auto-answer calls to: sip:*@{}", server_config.sip.domain);
    info!("🎵 Will play MP3 audio for 30 seconds on each call");
    info!("🎯 rvoip auto-answering SIP server is ready!");
    info!("🏥 Health endpoint started on http://{}:{}/health", health_addr.ip(), health_addr.port());

    // Main server loop
    while *running.read().await {
        tokio::time::sleep(Duration::from_secs(15)).await;
        let stats = handler.call_stats.lock().await;
        info!("📊 Server Statistics:");
        info!("  📞 Calls: {} total, {} active, {} answered, {} failed", 
              stats.total_calls, stats.active_calls, stats.answered_calls, stats.failed_calls);
        if stats.active_calls > 0 {
            info!("  🔄 Active calls: {}", stats.active_calls);
            for (call_id, start_time) in handler.active_calls.lock().await.iter() {
                let duration = start_time.elapsed();
                info!("    📞 {}: {:.6}s", call_id, duration.as_secs_f64());
            }
        }
    }
    
    info!("🛑 Shutting down rvoip SIP server...");
    handle.close();
    signal_task.abort();
    client.stop().await.context("Failed to stop client")?;
    health_server.abort();
    
    Ok(())
}

fn log_server_configuration(config: &ServerConfig) {
    info!("⚙️ rvoip server configuration:");
    info!("   📡 Listening: {}:{}", config.sip.bind_address, config.sip.port);
    info!("   🌐 Domain: {}", config.sip.domain);
    info!("   📞 Max concurrent calls: {}", config.behavior.max_concurrent_calls);
    info!("   🎵 Auto-answer enabled: {}", config.behavior.auto_answer);
    info!("   ⏱️ Auto-answer delay: {}ms", config.behavior.auto_answer_delay_ms);
    info!("   🎶 Audio: MP3 playback for {} seconds", 30);
    
    info!("⚙️ rvoip client configuration:");
    info!("   📡 SIP address: {}:{}", config.sip.bind_address, config.sip.port);
    info!("   🎵 Media address: {}:0", config.sip.bind_address);
    info!("   🌐 Domain: {}", config.sip.domain);
} 