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
    pub audio_processing: AudioProcessingConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioProcessingConfig {
    pub preemphasis_alpha: f32,
    pub bandpass_low_freq: f32,
    pub bandpass_high_freq: f32,
    // 3-band compressor settings
    pub band_split_freq_1: f32,      // Split between band 1 and 2
    pub band_split_freq_2: f32,      // Split between band 2 and 3
    pub band1_compressor: CompressorBandConfig,  // Low-Mid band (300-800Hz)
    pub band2_compressor: CompressorBandConfig,  // Mid band (800-2500Hz)
    pub band3_compressor: CompressorBandConfig,  // High-Mid band (2500-3400Hz)
    pub noise_gate_threshold: f32,
    pub noise_gate_ratio: f32,
    pub soft_limiter_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorBandConfig {
    pub target_level: f32,
    pub attack_time: f32,
    pub release_time: f32,
    pub ratio: f32,
    pub threshold_factor: f32,
    pub knee_width: f32,
    pub enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            sip: SipConfig::default(),
            behavior: BehaviorConfig::default(),
            media: MediaConfig::default(),
            logging: LoggingConfig::default(),
            health: HealthConfig::default(),
            audio_processing: AudioProcessingConfig::default(),
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

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            preemphasis_alpha: 0.95,
            bandpass_low_freq: 300.0,
            bandpass_high_freq: 3400.0,
            // 3-band compressor crossover frequencies
            band_split_freq_1: 800.0,   // Split between low-mid and mid
            band_split_freq_2: 2500.0,  // Split between mid and high-mid
            // Band 1: Low-Mid (300-800Hz) - more aggressive for bass control
            band1_compressor: CompressorBandConfig {
                target_level: 0.4,
                attack_time: 0.010,     // Slower attack for musical content
                release_time: 0.15,     // Longer release
                ratio: 4.0,             // More aggressive for bass control
                threshold_factor: 0.6,
                knee_width: 0.15,
                enabled: true,
            },
            // Band 2: Mid (800-2500Hz) - gentler for vocal clarity
            band2_compressor: CompressorBandConfig {
                target_level: 0.6,
                attack_time: 0.020,     // Even slower for speech preservation
                release_time: 0.08,     // Faster release for speech
                ratio: 2.5,             // Gentler for vocals
                threshold_factor: 0.75,
                knee_width: 0.2,
                enabled: true,
            },
            // Band 3: High-Mid (2500-3400Hz) - minimal for presence
            band3_compressor: CompressorBandConfig {
                target_level: 0.7,
                attack_time: 0.005,     // Fast for transient control
                release_time: 0.05,     // Quick release for clarity
                ratio: 2.0,             // Gentle for presence
                threshold_factor: 0.8,
                knee_width: 0.1,
                enabled: true,
            },
            noise_gate_threshold: 0.01,
            noise_gate_ratio: 0.1,
            soft_limiter_threshold: 0.9,
        }
    }
}

impl Default for CompressorBandConfig {
    fn default() -> Self {
        Self {
            target_level: 0.5,
            attack_time: 0.010,
            release_time: 0.1,
            ratio: 3.0,
            threshold_factor: 0.7,
            knee_width: 0.1,
            enabled: true,
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

        // Validate audio processing parameters
        if self.audio_processing.preemphasis_alpha < 0.0 || self.audio_processing.preemphasis_alpha > 1.0 {
            return Err(anyhow::anyhow!("Invalid preemphasis alpha: {} (must be between 0.0 and 1.0)", 
                self.audio_processing.preemphasis_alpha));
        }

        if self.audio_processing.bandpass_low_freq >= self.audio_processing.bandpass_high_freq {
            return Err(anyhow::anyhow!("Invalid bandpass filter frequencies: low {} >= high {}", 
                self.audio_processing.bandpass_low_freq, self.audio_processing.bandpass_high_freq));
        }

        // Validate 3-band compressor frequencies
        if self.audio_processing.band_split_freq_1 <= self.audio_processing.bandpass_low_freq ||
           self.audio_processing.band_split_freq_1 >= self.audio_processing.bandpass_high_freq {
            return Err(anyhow::anyhow!("Invalid band split frequency 1: {} (must be between {} and {})", 
                self.audio_processing.band_split_freq_1, self.audio_processing.bandpass_low_freq, self.audio_processing.bandpass_high_freq));
        }

        if self.audio_processing.band_split_freq_2 <= self.audio_processing.band_split_freq_1 ||
           self.audio_processing.band_split_freq_2 >= self.audio_processing.bandpass_high_freq {
            return Err(anyhow::anyhow!("Invalid band split frequency 2: {} (must be between {} and {})", 
                self.audio_processing.band_split_freq_2, self.audio_processing.band_split_freq_1, self.audio_processing.bandpass_high_freq));
        }

        // Validate each compressor band
        self.validate_compressor_band(&self.audio_processing.band1_compressor, "Band 1")?;
        self.validate_compressor_band(&self.audio_processing.band2_compressor, "Band 2")?;
        self.validate_compressor_band(&self.audio_processing.band3_compressor, "Band 3")?;

        log::info!("Configuration validation passed");
        Ok(())
    }

    fn validate_compressor_band(&self, band: &CompressorBandConfig, band_name: &str) -> Result<()> {
        if band.target_level <= 0.0 || band.target_level > 1.0 {
            return Err(anyhow::anyhow!("Invalid {} target level: {} (must be between 0.0 and 1.0)", 
                band_name, band.target_level));
        }

        if band.attack_time <= 0.0 || band.attack_time > 1.0 {
            return Err(anyhow::anyhow!("Invalid {} attack time: {} (must be between 0.0 and 1.0)", 
                band_name, band.attack_time));
        }

        if band.release_time <= 0.0 || band.release_time > 5.0 {
            return Err(anyhow::anyhow!("Invalid {} release time: {} (must be between 0.0 and 5.0)", 
                band_name, band.release_time));
        }

        if band.ratio < 1.0 || band.ratio > 20.0 {
            return Err(anyhow::anyhow!("Invalid {} ratio: {} (must be between 1.0 and 20.0)", 
                band_name, band.ratio));
        }

        if band.threshold_factor < 0.0 || band.threshold_factor > 1.0 {
            return Err(anyhow::anyhow!("Invalid {} threshold factor: {} (must be between 0.0 and 1.0)", 
                band_name, band.threshold_factor));
        }

        if band.knee_width < 0.0 || band.knee_width > 1.0 {
            return Err(anyhow::anyhow!("Invalid {} knee width: {} (must be between 0.0 and 1.0)", 
                band_name, band.knee_width));
        }

        Ok(())
    }
} 