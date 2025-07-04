use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub sip: SipConfig,
    pub behavior: BehaviorConfig,
    pub media: MediaConfig,
    pub logging: LoggingConfig,
    pub health: HealthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipConfig {
    pub bind_address: String,
    pub port: u16,
    pub domain: String,
    pub user_agent: String,
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub auto_answer: bool,
    pub auto_answer_delay_ms: u64,
    pub tone_duration_seconds: u64,
    pub tone_frequency: f32,
    pub call_timeout_seconds: u64,
    pub max_concurrent_calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaConfig {
    pub rtp_port_range_start: u16,
    pub rtp_port_range_end: u16,
    pub preferred_codecs: Vec<String>,
    pub enable_dtmf: bool,
    pub audio_sample_rate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub enable_file_logging: bool,
    pub enable_syslog: bool,
    pub log_file_path: String,
    pub max_log_size_mb: u64,
    pub max_log_files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub enable_health_check: bool,
    pub health_check_port: u16,
    pub health_check_interval_seconds: u64,
    pub restart_on_failure: bool,
    pub max_restart_attempts: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            sip: SipConfig::default(),
            behavior: BehaviorConfig::default(),
            media: MediaConfig::default(),
            logging: LoggingConfig::default(),
            health: HealthConfig::default(),
        }
    }
}

impl Default for SipConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 5060,
            domain: "localhost".to_string(),
            user_agent: "rvoip-sip-server/0.1.0".to_string(),
            transport: "udp".to_string(),
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            auto_answer: true,
            auto_answer_delay_ms: 1000,
            tone_duration_seconds: 30,
            tone_frequency: 440.0, // A4 note
            call_timeout_seconds: 300, // 5 minutes
            max_concurrent_calls: 100,
        }
    }
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            rtp_port_range_start: 10000,
            rtp_port_range_end: 20000,
            preferred_codecs: vec!["PCMU".to_string(), "PCMA".to_string()],
            enable_dtmf: true,
            audio_sample_rate: 8000,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            enable_file_logging: true,
            enable_syslog: true,
            log_file_path: "/var/log/rvoip-sip-server/server.log".to_string(),
            max_log_size_mb: 100,
            max_log_files: 10,
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enable_health_check: true,
            health_check_port: 8080,
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
        }
    }
}

impl ServerConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        if !path.exists() {
            log::warn!("Configuration file {} does not exist, creating default config", path.display());
            let default_config = Self::default();
            default_config.save_to_file(path)?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: ServerConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config to TOML")?;

        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // Validate bind address
        if self.sip.bind_address.parse::<IpAddr>().is_err() {
            return Err(anyhow::anyhow!("Invalid bind address: {}", self.sip.bind_address));
        }

        // Validate port range (port 0 is invalid for binding)
        if self.sip.port == 0 {
            return Err(anyhow::anyhow!("Invalid SIP port: {} (port 0 is not allowed)", self.sip.port));
        }

        // Validate RTP port range
        if self.media.rtp_port_range_start >= self.media.rtp_port_range_end {
            return Err(anyhow::anyhow!("Invalid RTP port range: start {} >= end {}", 
                self.media.rtp_port_range_start, self.media.rtp_port_range_end));
        }

        // Validate tone frequency
        if self.behavior.tone_frequency <= 0.0 || self.behavior.tone_frequency > 20000.0 {
            return Err(anyhow::anyhow!("Invalid tone frequency: {}", self.behavior.tone_frequency));
        }

        // Validate log level
        match self.logging.level.to_lowercase().as_str() {
            "error" | "warn" | "info" | "debug" | "trace" => {},
            _ => return Err(anyhow::anyhow!("Invalid log level: {}", self.logging.level)),
        }

        // Validate domain
        if self.sip.domain.is_empty() {
            return Err(anyhow::anyhow!("Domain cannot be empty"));
        }

        // Validate transport
        match self.sip.transport.to_lowercase().as_str() {
            "udp" | "tcp" | "tls" | "ws" | "wss" => {},
            _ => return Err(anyhow::anyhow!("Invalid transport: {}", self.sip.transport)),
        }

        log::info!("Configuration validation passed");
        Ok(())
    }
} 