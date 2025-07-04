#!/bin/bash

# rvoip SIP Server Installation Script
# This script installs the rvoip SIP server on Ubuntu

set -e

# Configuration
PROJECT_NAME="rvoip-sip-server"
SERVICE_USER="rvoip"
SERVICE_GROUP="rvoip"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/rvoip-sip-server"
LOG_DIR="/var/log/rvoip-sip-server"
RUN_DIR="/var/run"
SYSTEMD_DIR="/etc/systemd/system"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[INSTALL]${NC} $1"
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

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

log "Starting installation of $PROJECT_NAME"
log "Package directory: $PACKAGE_DIR"

# Check if this is Ubuntu
if [ -f /etc/os-release ]; then
    . /etc/os-release
    if [ "$ID" != "ubuntu" ]; then
        log_warning "This installer is designed for Ubuntu. Detected: $ID"
        log_warning "Installation may not work correctly on other distributions."
    fi
else
    log_warning "Cannot detect OS distribution. Proceeding anyway..."
fi

# Check prerequisites
log "Checking prerequisites..."

if ! command_exists systemctl; then
    log_error "systemctl not found. This installer requires systemd."
    exit 1
fi

# Update package list
log "Updating package list..."
apt-get update -qq

# Install dependencies
log "Installing dependencies..."
apt-get install -y \
    curl \
    wget \
    ca-certificates \
    gnupg \
    lsb-release \
    logrotate \
    rsyslog

# Create service user and group
log "Creating service user and group..."
if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    groupadd --system "$SERVICE_GROUP"
    log_success "Created group: $SERVICE_GROUP"
fi

if ! getent passwd "$SERVICE_USER" >/dev/null 2>&1; then
    useradd --system --gid "$SERVICE_GROUP" --home-dir /nonexistent --shell /bin/false "$SERVICE_USER"
    log_success "Created user: $SERVICE_USER"
fi

# Create directories
log "Creating directories..."
mkdir -p "$INSTALL_DIR"
mkdir -p "$CONFIG_DIR"
mkdir -p "$LOG_DIR"
mkdir -p "$RUN_DIR"

# Set proper ownership and permissions
chown "$SERVICE_USER:$SERVICE_GROUP" "$LOG_DIR"
chmod 755 "$LOG_DIR"
chmod 755 "$RUN_DIR"

# Install binaries
log "Installing binaries..."
if [ -f "$PACKAGE_DIR/bin/sip-server" ]; then
    cp "$PACKAGE_DIR/bin/sip-server" "$INSTALL_DIR/"
    chmod 755 "$INSTALL_DIR/sip-server"
    log_success "Installed sip-server to $INSTALL_DIR"
else
    log_error "sip-server binary not found in package"
    exit 1
fi

if [ -f "$PACKAGE_DIR/bin/health-monitor" ]; then
    cp "$PACKAGE_DIR/bin/health-monitor" "$INSTALL_DIR/"
    chmod 755 "$INSTALL_DIR/health-monitor"
    log_success "Installed health-monitor to $INSTALL_DIR"
else
    log_error "health-monitor binary not found in package"
    exit 1
fi

# Install configuration files
log "Installing configuration files..."
if [ -f "$PACKAGE_DIR/config/config.toml" ]; then
    if [ ! -f "$CONFIG_DIR/config.toml" ]; then
        cp "$PACKAGE_DIR/config/config.toml" "$CONFIG_DIR/"
        log_success "Installed config.toml to $CONFIG_DIR"
    else
        cp "$PACKAGE_DIR/config/config.toml" "$CONFIG_DIR/config.toml.new"
        log_warning "Existing config.toml found, new config saved as config.toml.new"
    fi
else
    log_error "config.toml not found in package"
    exit 1
fi

if [ -f "$PACKAGE_DIR/config/monitor.toml" ]; then
    if [ ! -f "$CONFIG_DIR/monitor.toml" ]; then
        cp "$PACKAGE_DIR/config/monitor.toml" "$CONFIG_DIR/"
        log_success "Installed monitor.toml to $CONFIG_DIR"
    else
        cp "$PACKAGE_DIR/config/monitor.toml" "$CONFIG_DIR/monitor.toml.new"
        log_warning "Existing monitor.toml found, new config saved as monitor.toml.new"
    fi
else
    log_error "monitor.toml not found in package"
    exit 1
fi

# Set config file permissions
chmod 644 "$CONFIG_DIR"/*.toml*
chown root:root "$CONFIG_DIR"/*.toml*

# Install systemd service files
log "Installing systemd service files..."
if [ -f "$PACKAGE_DIR/systemd/rvoip-sip-server.service" ]; then
    cp "$PACKAGE_DIR/systemd/rvoip-sip-server.service" "$SYSTEMD_DIR/"
    log_success "Installed rvoip-sip-server.service"
else
    log_error "rvoip-sip-server.service not found in package"
    exit 1
fi

if [ -f "$PACKAGE_DIR/systemd/rvoip-health-monitor.service" ]; then
    cp "$PACKAGE_DIR/systemd/rvoip-health-monitor.service" "$SYSTEMD_DIR/"
    log_success "Installed rvoip-health-monitor.service"
else
    log_error "rvoip-health-monitor.service not found in package"
    exit 1
fi

# Set up logrotate
log "Setting up log rotation..."
cat > /etc/logrotate.d/rvoip-sip-server << EOF
$LOG_DIR/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 644 $SERVICE_USER $SERVICE_GROUP
    postrotate
        systemctl reload rvoip-sip-server >/dev/null 2>&1 || true
    endscript
}
EOF

# Set up rsyslog configuration
log "Setting up rsyslog configuration..."
cat > /etc/rsyslog.d/49-rvoip-sip-server.conf << EOF
# rvoip SIP Server logging
:programname,isequal,"rvoip-sip-server" $LOG_DIR/server.log
:programname,isequal,"rvoip-health-monitor" $LOG_DIR/monitor.log
& stop
EOF

# Reload systemd
log "Reloading systemd..."
systemctl daemon-reload

# Restart rsyslog
log "Restarting rsyslog..."
systemctl restart rsyslog

# Set up firewall rules (if UFW is installed)
if command_exists ufw; then
    log "Setting up firewall rules..."
    ufw allow 5060/udp comment "SIP Server"
    ufw allow 5060/tcp comment "SIP Server"
    ufw allow 10000:20000/udp comment "RTP Media"
    ufw allow 8080/tcp comment "Health Check"
    log_success "Firewall rules configured"
fi

# Create a simple health check script
log "Creating health check script..."
cat > "$INSTALL_DIR/health-check.sh" << 'EOF'
#!/bin/bash
# Simple health check script for rvoip SIP server

HEALTH_URL="http://localhost:8080/health"
TIMEOUT=10

if curl -s --max-time $TIMEOUT "$HEALTH_URL" | grep -q "status"; then
    echo "OK: SIP server is healthy"
    exit 0
else
    echo "ERROR: SIP server health check failed"
    exit 1
fi
EOF

chmod 755 "$INSTALL_DIR/health-check.sh"

# Installation complete
log_success "Installation completed successfully!"
log ""
log "Next steps:"
log "1. Review and customize the configuration files:"
log "   - Main server config: $CONFIG_DIR/config.toml"
log "   - Health monitor config: $CONFIG_DIR/monitor.toml"
log ""
log "2. Start the services:"
log "   sudo systemctl start rvoip-sip-server"
log "   sudo systemctl start rvoip-health-monitor"
log ""
log "3. Enable services to start on boot:"
log "   sudo systemctl enable rvoip-sip-server"
log "   sudo systemctl enable rvoip-health-monitor"
log ""
log "4. Check service status:"
log "   sudo systemctl status rvoip-sip-server"
log "   sudo systemctl status rvoip-health-monitor"
log ""
log "5. View logs:"
log "   sudo journalctl -u rvoip-sip-server -f"
log "   sudo journalctl -u rvoip-health-monitor -f"
log "   tail -f $LOG_DIR/server.log"
log "   tail -f $LOG_DIR/monitor.log"
log ""
log "6. Test health check:"
log "   $INSTALL_DIR/health-check.sh"
log ""
log "Configuration files are located in: $CONFIG_DIR"
log "Log files are located in: $LOG_DIR"
log "Service user: $SERVICE_USER"
log ""
log_success "Installation complete!" 