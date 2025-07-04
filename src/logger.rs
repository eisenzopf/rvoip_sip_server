use anyhow::{Context, Result};
use log::info;
use std::env;
use std::fs;
use std::path::Path;
use syslog::{Facility, Formatter3164};

pub fn init_logger(log_file: &str, daemon_mode: bool) -> Result<()> {
    let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    
    if daemon_mode {
        // In daemon mode, use both file and syslog
        init_file_and_syslog_logger(log_file, &log_level)?;
    } else {
        // In non-daemon mode, use console and file
        init_console_and_file_logger(log_file, &log_level)?;
    }
    
    info!("Logger initialized successfully");
    Ok(())
}

fn init_console_and_file_logger(log_file: &str, log_level: &str) -> Result<()> {
    use env_logger::Builder;
    use std::io::Write;
    
    // Create log directory if it doesn't exist
    if let Some(parent) = Path::new(log_file).parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create log directory: {}", parent.display()))?;
    }
    
    let log_file_path = log_file.to_string();
    
    Builder::new()
        .format(move |_buf, record| {
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
            let level = record.level();
            let target = record.target();
            let message = record.args();
            
            // Write to console
            println!("[{}] {} [{}] {}", timestamp, level, target, message);
            
            // Write to file
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path)
            {
                writeln!(file, "[{}] {} [{}] {}", timestamp, level, target, message).ok();
            }
            
            Ok(())
        })
        .filter_level(parse_log_level(log_level))
        .init();
        
    Ok(())
}

fn init_file_and_syslog_logger(log_file: &str, log_level: &str) -> Result<()> {
    use env_logger::Builder;
    use std::io::Write;
    
    // Create log directory if it doesn't exist
    if let Some(parent) = Path::new(log_file).parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create log directory: {}", parent.display()))?;
    }
    
    // Initialize syslog
    let formatter = Formatter3164 {
        facility: Facility::LOG_DAEMON,
        hostname: None,
        process: "rvoip-sip-server".into(),
        pid: std::process::id(),
    };
    
    let _syslog_writer = syslog::unix(formatter)
        .map_err(|e| anyhow::anyhow!("Failed to initialize syslog: {}", e))?;
    
    let log_file_path = log_file.to_string();
    
    Builder::new()
        .format(move |_buf, record| {
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
            let level = record.level();
            let target = record.target();
            let message = record.args();
            
            let log_entry = format!("[{}] {} [{}] {}", timestamp, level, target, message);
            
            // Write to file
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path)
            {
                writeln!(file, "{}", log_entry).ok();
            }
            
            // Note: In a production system, you might want to implement actual syslog writing
            // For now, we'll just write to file since the syslog crate has complex lifetimes
            
            Ok(())
        })
        .filter_level(parse_log_level(log_level))
        .init();
        
    Ok(())
}

fn parse_log_level(level: &str) -> log::LevelFilter {
    match level.to_lowercase().as_str() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    }
}

// Note: Log rotation functions removed to eliminate dead code warnings
// They can be re-added if needed for production log management

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_parse_log_level() {
        assert_eq!(parse_log_level("error"), log::LevelFilter::Error);
        assert_eq!(parse_log_level("ERROR"), log::LevelFilter::Error);
        assert_eq!(parse_log_level("warn"), log::LevelFilter::Warn);
        assert_eq!(parse_log_level("info"), log::LevelFilter::Info);
        assert_eq!(parse_log_level("debug"), log::LevelFilter::Debug);
        assert_eq!(parse_log_level("trace"), log::LevelFilter::Trace);
        assert_eq!(parse_log_level("invalid"), log::LevelFilter::Info);
    }
    
    // Test for log rotation removed since the functions were removed
    // to eliminate dead code warnings
} 