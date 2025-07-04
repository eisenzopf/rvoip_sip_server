use anyhow::{Context, Result};
use clap::{Arg, Command};
use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub server_binary_path: String,
    pub server_config_path: String,
    pub server_pid_file: String,
    pub server_log_file: String,
    pub health_check_url: String,
    pub health_check_interval_seconds: u64,
    pub health_check_timeout_seconds: u64,
    pub max_restart_attempts: u32,
    pub restart_delay_seconds: u64,
    pub monitor_log_file: String,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            server_binary_path: "/usr/local/bin/sip-server".to_string(),
            server_config_path: "/etc/rvoip-sip-server/config.toml".to_string(),
            server_pid_file: "/var/run/rvoip-sip-server.pid".to_string(),
            server_log_file: "/var/log/rvoip-sip-server/server.log".to_string(),
            health_check_url: "http://localhost:8080/health".to_string(),
            health_check_interval_seconds: 30,
            health_check_timeout_seconds: 10,
            max_restart_attempts: 3,
            restart_delay_seconds: 5,
            monitor_log_file: "/var/log/rvoip-sip-server/monitor.log".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub uptime_seconds: u64,
    pub active_calls: u32,
    pub total_calls: u64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
}

#[derive(Debug)]
pub struct HealthMonitor {
    config: HealthConfig,
    client: Client,
    restart_attempts: u32,
    last_restart_time: Option<Instant>,
    server_start_time: Option<Instant>,
}

impl HealthMonitor {
    pub fn new(config: HealthConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.health_check_timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            restart_attempts: 0,
            last_restart_time: None,
            server_start_time: None,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Health monitor starting");
        info!("Monitoring server at: {}", self.config.health_check_url);
        info!("Check interval: {}s", self.config.health_check_interval_seconds);
        info!("Max restart attempts: {}", self.config.max_restart_attempts);

        // Initial server start
        self.start_server().await?;

        loop {
            sleep(Duration::from_secs(self.config.health_check_interval_seconds)).await;

            match self.check_health().await {
                Ok(health_status) => {
                    info!("Health check passed - Status: {}, Active calls: {}, Uptime: {}s", 
                          health_status.status, health_status.active_calls, health_status.uptime_seconds);
                    
                    // Reset restart attempts on successful health check
                    self.restart_attempts = 0;
                }
                Err(e) => {
                    error!("Health check failed: {}", e);
                    
                    if self.should_restart() {
                        match self.restart_server().await {
                            Ok(_) => {
                                info!("Server restarted successfully (attempt {}/{})", 
                                      self.restart_attempts, self.config.max_restart_attempts);
                            }
                            Err(restart_error) => {
                                error!("Failed to restart server: {}", restart_error);
                            }
                        }
                    } else {
                        error!("Maximum restart attempts reached ({}), giving up", 
                               self.config.max_restart_attempts);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn check_health(&self) -> Result<HealthStatus> {
        // Check if server process is running
        if !self.is_server_running() {
            return Err(anyhow::anyhow!("Server process is not running"));
        }

        // Check health endpoint
        let response = self.client
            .get(&self.config.health_check_url)
            .send()
            .await
            .context("Failed to send health check request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Health check returned status: {}", response.status()));
        }

        let health_status: HealthStatus = response
            .json()
            .await
            .context("Failed to parse health check response")?;

        Ok(health_status)
    }

    fn is_server_running(&self) -> bool {
        if !Path::new(&self.config.server_pid_file).exists() {
            return false;
        }

        match fs::read_to_string(&self.config.server_pid_file) {
            Ok(pid_str) => {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    self.is_process_running(pid)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    fn is_process_running(&self, pid: u32) -> bool {
        // Check if process is running by sending signal 0
        let output = ProcessCommand::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output();

        matches!(output, Ok(output) if output.status.success())
    }

    async fn start_server(&mut self) -> Result<()> {
        info!("Starting SIP server");

        let mut command = ProcessCommand::new(&self.config.server_binary_path);
        command
            .arg("--daemon")
            .arg("--config")
            .arg(&self.config.server_config_path)
            .arg("--log-file")
            .arg(&self.config.server_log_file)
            .arg("--pid-file")
            .arg(&self.config.server_pid_file)
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let output = command
            .spawn()
            .context("Failed to start server process")?
            .wait()
            .context("Failed to wait for server process")?;

        if !output.success() {
            return Err(anyhow::anyhow!("Server failed to start with exit code: {:?}", output.code()));
        }

        self.server_start_time = Some(Instant::now());
        info!("Server started successfully");

        // Wait a bit for server to initialize
        sleep(Duration::from_secs(2)).await;

        Ok(())
    }

    async fn restart_server(&mut self) -> Result<()> {
        info!("Restarting SIP server");

        // Stop server first
        self.stop_server().await?;

        // Wait before restarting
        sleep(Duration::from_secs(self.config.restart_delay_seconds)).await;

        // Start server
        self.start_server().await?;

        self.restart_attempts += 1;
        self.last_restart_time = Some(Instant::now());

        Ok(())
    }

    async fn stop_server(&self) -> Result<()> {
        info!("Stopping SIP server");

        if let Ok(pid_str) = fs::read_to_string(&self.config.server_pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                // Send SIGTERM to gracefully shutdown
                let output = ProcessCommand::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .output()
                    .context("Failed to send SIGTERM to server")?;

                if !output.status.success() {
                    warn!("Failed to send SIGTERM, trying SIGKILL");
                    
                    // If SIGTERM fails, try SIGKILL
                    let kill_output = ProcessCommand::new("kill")
                        .arg("-KILL")
                        .arg(pid.to_string())
                        .output()
                        .context("Failed to send SIGKILL to server")?;

                    if !kill_output.status.success() {
                        return Err(anyhow::anyhow!("Failed to kill server process"));
                    }
                }

                // Wait for process to exit
                let mut wait_count = 0;
                while self.is_process_running(pid) && wait_count < 10 {
                    sleep(Duration::from_millis(500)).await;
                    wait_count += 1;
                }

                if self.is_process_running(pid) {
                    return Err(anyhow::anyhow!("Server process did not exit after SIGKILL"));
                }
            }
        }

        // Remove PID file
        if Path::new(&self.config.server_pid_file).exists() {
            fs::remove_file(&self.config.server_pid_file)
                .context("Failed to remove PID file")?;
        }

        info!("Server stopped successfully");
        Ok(())
    }

    fn should_restart(&self) -> bool {
        if self.restart_attempts >= self.config.max_restart_attempts {
            return false;
        }

        // Check if we've recently restarted (avoid restart loops)
        if let Some(last_restart) = self.last_restart_time {
            if last_restart.elapsed() < Duration::from_secs(60) {
                warn!("Recent restart detected, waiting before attempting another restart");
                return false;
            }
        }

        true
    }

    fn load_config<P: AsRef<Path>>(path: P) -> Result<HealthConfig> {
        let path = path.as_ref();
        
        if !path.exists() {
            warn!("Health monitor config file {} does not exist, creating default config", path.display());
            let default_config = HealthConfig::default();
            default_config.save_to_file(path)?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: HealthConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }
}

impl HealthConfig {
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("rvoip-health-monitor")
        .version("0.1.0")
        .about("Health monitor for rvoip SIP server")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Health monitor configuration file")
                .default_value("/etc/rvoip-sip-server/monitor.toml"),
        )
        .arg(
            Arg::new("daemon")
                .short('d')
                .long("daemon")
                .help("Run as daemon")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let config_path = matches.get_one::<String>("config").unwrap();
    let daemon_mode = matches.get_flag("daemon");

    // Initialize logging
    env_logger::Builder::from_default_env()
        .format_timestamp_secs()
        .init();

    // Load configuration
    let config = HealthMonitor::load_config(config_path)
        .with_context(|| format!("Failed to load config from {}", config_path))?;

    info!("Health monitor configuration loaded from: {}", config_path);

    if daemon_mode {
        info!("Running in daemon mode");
        // In a real implementation, you would daemonize here
        // For simplicity, we'll just run in the foreground
    }

    // Create and run health monitor
    let mut monitor = HealthMonitor::new(config);
    monitor.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("monitor.toml");
        
        let config = HealthConfig::default();
        config.save_to_file(&config_path).unwrap();
        
        let loaded_config = HealthMonitor::load_config(&config_path).unwrap();
        assert_eq!(config.health_check_interval_seconds, loaded_config.health_check_interval_seconds);
        assert_eq!(config.max_restart_attempts, loaded_config.max_restart_attempts);
    }

    #[test]
    fn test_should_restart_logic() {
        let config = HealthConfig::default();
        let mut monitor = HealthMonitor::new(config);
        
        // Should restart initially
        assert!(monitor.should_restart());
        
        // Should not restart after max attempts
        monitor.restart_attempts = monitor.config.max_restart_attempts;
        assert!(!monitor.should_restart());
        
        // Should not restart if recently restarted
        monitor.restart_attempts = 0;
        monitor.last_restart_time = Some(Instant::now());
        assert!(!monitor.should_restart());
    }
} 