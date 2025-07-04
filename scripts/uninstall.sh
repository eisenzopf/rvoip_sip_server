#!/bin/bash

# rvoip SIP Server Uninstallation Script
# This script removes the rvoip SIP server from the system

set -e

# Configuration
PROJECT_NAME="rvoip-sip-server"
SERVICE_USER="rvoip"
SERVICE_GROUP="rvoip"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/rvoip-sip-server"
LOG_DIR="/var/log/rvoip-sip-server"
SYSTEMD_DIR="/etc/systemd/system"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[UNINSTALL]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    log_error "This script must be run as root (use sudo)"
    exit 1
fi

log "Starting uninstallation of $PROJECT_NAME"

# Ask for confirmation
echo -n "Are you sure you want to completely remove $PROJECT_NAME? [y/N]: "
read -r response
if [[ ! "$response" =~ ^[Yy]$ ]]; then
    log "Uninstallation cancelled by user"
    exit 0
fi

# Stop services
log "Stopping services..."
if systemctl is-active --quiet rvoip-sip-server; then
    systemctl stop rvoip-sip-server
    log_success "Stopped rvoip-sip-server service"
fi

if systemctl is-active --quiet rvoip-health-monitor; then
    systemctl stop rvoip-health-monitor
    log_success "Stopped rvoip-health-monitor service"
fi

# Disable services
log "Disabling services..."
if systemctl is-enabled --quiet rvoip-sip-server 2>/dev/null; then
    systemctl disable rvoip-sip-server
    log_success "Disabled rvoip-sip-server service"
fi

if systemctl is-enabled --quiet rvoip-health-monitor 2>/dev/null; then
    systemctl disable rvoip-health-monitor
    log_success "Disabled rvoip-health-monitor service"
fi

# Remove systemd service files
log "Removing systemd service files..."
if [ -f "$SYSTEMD_DIR/rvoip-sip-server.service" ]; then
    rm -f "$SYSTEMD_DIR/rvoip-sip-server.service"
    log_success "Removed rvoip-sip-server.service"
fi

if [ -f "$SYSTEMD_DIR/rvoip-health-monitor.service" ]; then
    rm -f "$SYSTEMD_DIR/rvoip-health-monitor.service"
    log_success "Removed rvoip-health-monitor.service"
fi

# Reload systemd
log "Reloading systemd..."
systemctl daemon-reload

# Remove binaries
log "Removing binaries..."
if [ -f "$INSTALL_DIR/sip-server" ]; then
    rm -f "$INSTALL_DIR/sip-server"
    log_success "Removed sip-server binary"
fi

if [ -f "$INSTALL_DIR/health-monitor" ]; then
    rm -f "$INSTALL_DIR/health-monitor"
    log_success "Removed health-monitor binary"
fi

if [ -f "$INSTALL_DIR/health-check.sh" ]; then
    rm -f "$INSTALL_DIR/health-check.sh"
    log_success "Removed health-check script"
fi

# Ask about configuration files
echo -n "Do you want to remove configuration files? [y/N]: "
read -r response
if [[ "$response" =~ ^[Yy]$ ]]; then
    if [ -d "$CONFIG_DIR" ]; then
        rm -rf "$CONFIG_DIR"
        log_success "Removed configuration directory: $CONFIG_DIR"
    fi
else
    log "Configuration files kept in: $CONFIG_DIR"
fi

# Ask about log files
echo -n "Do you want to remove log files? [y/N]: "
read -r response
if [[ "$response" =~ ^[Yy]$ ]]; then
    if [ -d "$LOG_DIR" ]; then
        rm -rf "$LOG_DIR"
        log_success "Removed log directory: $LOG_DIR"
    fi
else
    log "Log files kept in: $LOG_DIR"
fi

# Remove logrotate configuration
log "Removing logrotate configuration..."
if [ -f "/etc/logrotate.d/rvoip-sip-server" ]; then
    rm -f "/etc/logrotate.d/rvoip-sip-server"
    log_success "Removed logrotate configuration"
fi

# Remove rsyslog configuration
log "Removing rsyslog configuration..."
if [ -f "/etc/rsyslog.d/49-rvoip-sip-server.conf" ]; then
    rm -f "/etc/rsyslog.d/49-rvoip-sip-server.conf"
    log_success "Removed rsyslog configuration"
    
    # Restart rsyslog
    log "Restarting rsyslog..."
    systemctl restart rsyslog
fi

# Ask about service user
echo -n "Do you want to remove the service user ($SERVICE_USER)? [y/N]: "
read -r response
if [[ "$response" =~ ^[Yy]$ ]]; then
    if getent passwd "$SERVICE_USER" >/dev/null 2>&1; then
        userdel "$SERVICE_USER"
        log_success "Removed user: $SERVICE_USER"
    fi
    
    if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
        groupdel "$SERVICE_GROUP"
        log_success "Removed group: $SERVICE_GROUP"
    fi
else
    log "Service user and group kept"
fi

# Remove PID file if it exists
if [ -f "/var/run/rvoip-sip-server.pid" ]; then
    rm -f "/var/run/rvoip-sip-server.pid"
    log_success "Removed PID file"
fi

# Remove firewall rules (if UFW is installed)
if command_exists ufw; then
    log "Removing firewall rules..."
    ufw --force delete allow 5060/udp >/dev/null 2>&1 || true
    ufw --force delete allow 5060/tcp >/dev/null 2>&1 || true
    ufw --force delete allow 10000:20000/udp >/dev/null 2>&1 || true
    ufw --force delete allow 8080/tcp >/dev/null 2>&1 || true
    log_success "Removed firewall rules"
fi

# Clean up any remaining files
log "Cleaning up remaining files..."

# Remove any remaining service files
find /etc/systemd/system -name "*rvoip*" -delete 2>/dev/null || true

# Remove any remaining configuration files
find /etc -name "*rvoip*" -type f -delete 2>/dev/null || true

# Remove any remaining log files in other locations
find /var/log -name "*rvoip*" -delete 2>/dev/null || true

log_success "Uninstallation completed successfully!"
log ""
log "The following may still exist and can be manually removed if desired:"
log "- Any custom configuration backups you made"
log "- Any custom scripts or cron jobs you created"
log "- Any additional log files in non-standard locations"
log ""

# Show what was kept
if [ -d "$CONFIG_DIR" ]; then
    log "Configuration files kept in: $CONFIG_DIR"
fi

if [ -d "$LOG_DIR" ]; then
    log "Log files kept in: $LOG_DIR"
fi

if getent passwd "$SERVICE_USER" >/dev/null 2>&1; then
    log "Service user kept: $SERVICE_USER"
fi

log ""
log_success "Uninstallation complete!"
log "Thank you for using $PROJECT_NAME" 