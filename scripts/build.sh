#!/bin/bash

# rvoip SIP Server Build Script
# This script builds the project for release deployment

set -e

# Configuration
PROJECT_NAME="rvoip-sip-server"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$PROJECT_DIR/target/release"
PACKAGE_DIR="$PROJECT_DIR/package"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[BUILD]${NC} $1"
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

# Check prerequisites
log "Checking prerequisites..."

if ! command_exists cargo; then
    log_error "Cargo not found. Please install Rust and Cargo."
    exit 1
fi

if ! command_exists git; then
    log_error "Git not found. Please install Git."
    exit 1
fi

# Get version from Cargo.toml
VERSION=$(grep "^version" "$PROJECT_DIR/Cargo.toml" | head -1 | cut -d'"' -f2)
log "Building $PROJECT_NAME version $VERSION"

# Clean previous builds
log "Cleaning previous builds..."
cd "$PROJECT_DIR"
cargo clean

# Build release version
log "Building release version..."
cargo build --release

if [ $? -ne 0 ]; then
    log_error "Build failed"
    exit 1
fi

# Verify binaries
log "Verifying binaries..."
if [ ! -f "$BUILD_DIR/sip-server" ]; then
    log_error "sip-server binary not found"
    exit 1
fi

if [ ! -f "$BUILD_DIR/health-monitor" ]; then
    log_error "health-monitor binary not found"
    exit 1
fi

# Create package directory
log "Creating package directory..."
rm -rf "$PACKAGE_DIR"
mkdir -p "$PACKAGE_DIR"/{bin,config,systemd,scripts}

# Copy binaries
log "Copying binaries..."
cp "$BUILD_DIR/sip-server" "$PACKAGE_DIR/bin/"
cp "$BUILD_DIR/health-monitor" "$PACKAGE_DIR/bin/"

# Copy configuration files
log "Copying configuration files..."
cp "$PROJECT_DIR/systemd/"*.service "$PACKAGE_DIR/systemd/"
cp "$PROJECT_DIR/scripts/install.sh" "$PACKAGE_DIR/scripts/"
cp "$PROJECT_DIR/scripts/uninstall.sh" "$PACKAGE_DIR/scripts/"

# Create default configuration files
log "Creating default configuration files..."
cat > "$PACKAGE_DIR/config/config.toml" << EOF
[sip]
bind_address = "0.0.0.0"
port = 5060
domain = "localhost"
user_agent = "rvoip-sip-server/$VERSION"
transport = "udp"

[behavior]
auto_answer = true
auto_answer_delay_ms = 1000
tone_duration_seconds = 30
tone_frequency = 440.0
call_timeout_seconds = 300
max_concurrent_calls = 100

[media]
rtp_port_range_start = 10000
rtp_port_range_end = 20000
preferred_codecs = ["PCMU", "PCMA"]
enable_dtmf = true
audio_sample_rate = 8000

[logging]
level = "info"
enable_file_logging = true
enable_syslog = true
log_file_path = "/var/log/rvoip-sip-server/server.log"
max_log_size_mb = 100
max_log_files = 10

[health]
enable_health_check = true
health_check_port = 8080
health_check_interval_seconds = 30
restart_on_failure = true
max_restart_attempts = 3
EOF

cat > "$PACKAGE_DIR/config/monitor.toml" << EOF
server_binary_path = "/usr/local/bin/sip-server"
server_config_path = "/etc/rvoip-sip-server/config.toml"
server_pid_file = "/var/run/rvoip-sip-server.pid"
server_log_file = "/var/log/rvoip-sip-server/server.log"
health_check_url = "http://localhost:8080/health"
health_check_interval_seconds = 30
health_check_timeout_seconds = 10
max_restart_attempts = 3
restart_delay_seconds = 5
monitor_log_file = "/var/log/rvoip-sip-server/monitor.log"
EOF

# Create README
log "Creating README..."
cat > "$PACKAGE_DIR/README.md" << EOF
# rvoip SIP Server Package

This package contains the rvoip SIP server and health monitor.

## Installation

Run the installation script as root:

\`\`\`bash
sudo ./scripts/install.sh
\`\`\`

## Configuration

- Main server config: \`/etc/rvoip-sip-server/config.toml\`
- Health monitor config: \`/etc/rvoip-sip-server/monitor.toml\`

## Service Management

\`\`\`bash
# Start services
sudo systemctl start rvoip-sip-server
sudo systemctl start rvoip-health-monitor

# Enable services to start on boot
sudo systemctl enable rvoip-sip-server
sudo systemctl enable rvoip-health-monitor

# Check status
sudo systemctl status rvoip-sip-server
sudo systemctl status rvoip-health-monitor

# View logs
sudo journalctl -u rvoip-sip-server -f
sudo journalctl -u rvoip-health-monitor -f
\`\`\`

## Uninstallation

Run the uninstallation script as root:

\`\`\`bash
sudo ./scripts/uninstall.sh
\`\`\`

## Version

$VERSION
EOF

# Create tarball
log "Creating tarball..."
cd "$PROJECT_DIR"
tar -czf "$PROJECT_NAME-$VERSION.tar.gz" -C package .

log_success "Build completed successfully!"
log_success "Package created: $PROJECT_NAME-$VERSION.tar.gz"
log_success "Package directory: $PACKAGE_DIR"

# Display package contents
log "Package contents:"
ls -la "$PACKAGE_DIR"
find "$PACKAGE_DIR" -type f | sort 