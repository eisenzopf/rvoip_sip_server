# rvoip SIP Server Test Application

A **simple test server** built using **rvoip's client-core library** to demonstrate **auto-answering SIP calls and MP3 audio playback**. This is a testing tool for the rvoip library, not a production system.

## ✨ What It Actually Does

When someone calls your server (e.g., via bandwidth.com or any SIP client):

1. **📞 Real SIP Auto-Answer**: Automatically answers incoming calls after configurable delay
2. **🎵 MP3 Audio Playback**: Downloads and plays `jocofullinterview41.mp3` to the caller
3. **📡 Full SIP Protocol**: Complete SIP protocol handling (INVITE → 180 Ringing → 200 OK → ACK)
4. **🔄 RTP Media Streams**: Actual audio transmission via RTP with μ-law encoding
5. **🎛️ Advanced Audio Processing**: 3-band compressor, bandpass filtering, and telephony optimization
6. **⏱️ Fixed Duration**: Plays MP3 audio for exactly 30 seconds
7. **📴 Clean Hangup**: Automatically hangs up after audio completion

**The caller will hear a 30-second excerpt from the MP3 file with telephony-optimized audio processing!**

## 🛠️ Technical Implementation

### Real rvoip client-core Usage
- **ClientManager**: rvoip's SIP client for handling calls
- **ClientEventHandler**: Auto-answer logic via `on_incoming_call` callback  
- **Auto-answering**: Uses `CallAction::Ignore` + async `client.answer_call()`
- **Audio Transmission**: Real RTP streams via `client.start_audio_transmission_with_custom_audio()`
- **MP3 Processing**: Downloads, converts, and processes MP3 to telephony-optimized audio

### Key Features
- ✅ **Actually auto-answers calls** - demonstrates rvoip auto-answer capabilities
- ✅ **Real MP3 audio playback** - streams pre-recorded audio content
- ✅ **Advanced Audio Processing** - 3-band compressor, bandpass filtering, noise gating
- ✅ **Test deployment** with systemd services and health monitoring
- ✅ **Comprehensive logging** with call statistics and duration tracking
- ✅ **Configurable audio parameters** for testing different telephony settings

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   SIP Client    │◄──►│  rvoip SIP Server │◄──►│ Health Monitor  │
│ (bandwidth.com) │    │ (client-core)     │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                │
                         ┌─────────────┐
                         │ Components  │
                         ├─────────────┤
                         │ • AutoAnswerHandler
                         │ • Mp3Handler  
                         │ • ClientManager   
                         │ • RTP Audio
                         │ • Call Statistics
                         └─────────────┘
```

### Core Components

- **🎯 AutoAnswerHandler**: Implements `ClientEventHandler` for automatic call processing
- **📡 ClientManager**: rvoip's main SIP client component  
- **🎵 Mp3Handler**: Downloads, converts, and processes MP3 files for telephony playback
- **🎛️ TelephonyAudioProcessor**: 3-band compressor with bandpass filtering and audio optimization
- **📊 CallStats**: Real-time call metrics and statistics tracking
- **⚕️ HealthMonitor**: Monitors server health and handles automatic restarts

## Quick Start

### Prerequisites

- **Rust 1.70+** with Cargo
- **Ubuntu 20.04+** for production deployment  
- Network access for SIP communication (UDP port 5060)
- **rvoip library**: Uses published `rvoip = "0.1.10"` crate

### Setup

1. **Development Build**:
```bash
git clone <this-repo>
cd rvoip_sip_server
cargo build --release
```

2. **Test Locally**:
```bash
# Run in development mode
./target/release/sip-server --config config.toml

# Test with any SIP client pointed to localhost:5060
   # The server will auto-answer and play MP3 audio
```

3. **Package for Deployment**:
```bash
./scripts/build.sh
```

4. **Deploy to Ubuntu Server**:
```bash
# Copy the generated .tar.gz to your server
scp rvoip-sip-server-*.tar.gz user@server:/tmp/
ssh user@server
cd /tmp
tar -xzf rvoip-sip-server-*.tar.gz
sudo ./scripts/install.sh
```

## Configuration

### Main Server Configuration (`config.toml`)

```toml
[sip]
bind_address = "0.0.0.0"        # IP address to bind to
port = 5060                     # SIP port
domain = "example.com"          # SIP domain
transport = "udp"               # Transport protocol

[behavior]
auto_answer = true              # Enable auto-answer
auto_answer_delay_ms = 1000     # Delay before answering (ms)
max_concurrent_calls = 100      # Maximum concurrent calls

[audio_processing]
# 3-band compressor settings for telephony optimization
band_split_freq_1 = 800.0       # Split between low-mid and mid band (Hz)
band_split_freq_2 = 2500.0      # Split between mid and high-mid band (Hz)
# Individual compressor settings for each frequency band
# See config.toml for complete audio processing configuration

[media]
rtp_port_range_start = 10000    # RTP port range start
rtp_port_range_end = 20000      # RTP port range end
preferred_codecs = ["PCMU", "PCMA"]  # Preferred audio codecs
enable_dtmf = true              # Enable DTMF detection
audio_sample_rate = 8000        # Audio sample rate (Hz)
```

## Real Call Flow

When a caller dials your server:

### 📞 SIP Protocol Sequence

```
Caller ──INVITE──► rvoip Server
       ◄──100 Trying──────────
       ◄──180 Ringing─────────
       ◄──200 OK──────────────  ← Auto-answer after delay
       ──ACK─────►
       ◄═══RTP Media Stream═══► ← Real audio flow
       ◄─── MP3 Audio ───────── ← Caller hears processed audio
       ──BYE─────►              ← After 30 seconds  
       ◄──200 OK──────────────
```

### 🔄 Server Processing Steps

1. **📡 SIP INVITE Received**: Client receives incoming call
2. **⏱️ Auto-Answer Delay**: Waits configured delay (1 second default)
3. **📞 Call Answered**: `client.answer_call()` sends 200 OK
4. **📥 MP3 Download**: Downloads `jocofullinterview41.mp3` if not cached
5. **🎛️ Audio Processing**: Converts MP3 to WAV with 3-band compression and telephony optimization
6. **🎵 Audio Session**: `client.start_audio_transmission_with_custom_audio()` starts RTP
7. **📊 Media Info**: Log RTP ports and codec information
8. **⏰ Duration**: Play processed MP3 audio for 30 seconds
9. **📴 Hangup**: `client.hangup_call()` terminates gracefully

## Usage Examples

### Testing with SIP Clients

1. **Softphone** (Linphone, Zoiper, etc.):
   ```
   Server: your-server-ip:5060
   Make call to: any-extension@your-server-ip
   Result: Auto-answers, plays MP3 audio for 30s
   ```

2. **SIPp Load Testing**:
   ```bash
   # Basic call test
   sipp -sn uac your-server-ip:5060
   
   # Load test with 10 concurrent calls
   sipp -sn uac -l 10 your-server-ip:5060
   ```

3. **Command Line Testing**:
   ```bash
   # Check if server is listening
   sudo netstat -tulpn | grep :5060
   
   # Monitor real-time logs  
   tail -f /var/log/rvoip-sip-server/server.log
   ```

## What You'll See in Logs

```bash
📞 Incoming call: abc123 from sip:+1234567890@bandwidth.com to sip:server@yourserver.com
⏱️ Auto-answering call abc123 in 1000ms
📞 Auto-answering call: abc123
✅ Call abc123 answered successfully  
🎉 Call abc123 connected! Starting audio session...
📥 MP3 file already exists: jocofullinterview41.mp3
🔄 Converting MP3 to WAV format (8000Hz, 1 channels) with telephony processing
✅ MP3 converted to WAV with telephony processing: jocofullinterview41.wav (240000 samples at 8000Hz)
🎵 Preparing MP3 audio samples for transmission...
🔄 Converted 240000 PCM samples to 240000 μ-law samples for RTP transmission
✅ Custom audio transmission started successfully for call abc123
📊 Media info for call abc123 - Local RTP: 10000, Remote RTP: 5004, Codec: PCMU
📴 Hanging up call abc123 after MP3 completion
✅ Call abc123 hung up successfully after MP3 playback
📴 Call abc123 terminated
⏱️ Call abc123 duration: 30.2s
📊 Server Statistics: 📞 Calls: 1 total, 0 active, 1 answered, 0 failed
```

## Monitoring and Management

### Service Management
```bash
# Check service status
sudo systemctl status rvoip-sip-server
sudo systemctl status rvoip-health-monitor

# View real-time logs
sudo journalctl -u rvoip-sip-server -f
tail -f /var/log/rvoip-sip-server/server.log

# Health check
curl http://localhost:8080/health
```

### Call Statistics
The server reports statistics every 30 seconds:
- **Total calls**: Lifetime call count
- **Active calls**: Currently connected calls
- **Answered calls**: Successfully answered calls  
- **Failed calls**: Failed or rejected calls
- **Call durations**: Individual call timing

## Network Requirements

### Firewall Rules
```bash
# SIP signaling
sudo ufw allow 5060/udp comment "SIP UDP"
sudo ufw allow 5060/tcp comment "SIP TCP"

# RTP media
sudo ufw allow 10000:20000/udp comment "RTP Media"

# Health check
sudo ufw allow 8080/tcp comment "Health Check"
```

### Port Usage
- **5060**: SIP signaling (UDP/TCP)
- **10000-20000**: RTP media streams (UDP)
- **8080**: Health check HTTP endpoint

## Performance

### Testing Capabilities
- **Concurrent Calls**: Suitable for testing with multiple simultaneous calls
- **Memory Usage**: ~50MB baseline, ~1MB per active call
- **CPU Usage**: Low CPU usage on modern hardware
- **Audio Quality**: 8kHz sample rate, μ-law encoding with 3-band compression
- **MP3 Processing**: Real-time audio processing with telephony optimization

## Test Deployment

### Installation
```bash
# Ubuntu server deployment for testing
sudo ./scripts/install.sh

# Enables systemd services for testing:
# - rvoip-sip-server.service
# - rvoip-health-monitor.service
```

### Security Features (for test environments)
- **Dedicated User**: Runs as non-root `rvoip` user
- **Limited Privileges**: Only necessary capabilities granted
- **Resource Limits**: CPU and memory limits enforced for testing
- **Network Isolation**: Configurable firewall rules

## Troubleshooting

### Common Issues

1. **No Call Connection**:
   ```bash
   # Check if server is listening
   sudo netstat -tulpn | grep :5060
   
   # Check firewall
   sudo ufw status
   
   # Check logs
   tail -f /var/log/rvoip-sip-server/server.log
   ```

2. **No Audio**:
   ```bash
   # Check RTP port range is open
   sudo ufw status | grep 10000
   
   # Look for audio transmission messages in logs
   grep "Audio transmission" /var/log/rvoip-sip-server/server.log
   ```

3. **Service Won't Start**:
   ```bash
   # Check configuration
   ./target/release/sip-server --config config.toml
   
   # Check systemd logs
   sudo journalctl -u rvoip-sip-server -f
   ```

### Debug Mode
```bash
# Enable debug logging
export RUST_LOG=debug
./target/release/sip-server --config config.toml
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Test with real SIP clients
4. Submit a pull request

## License

This project is licensed under the MIT License.

## Acknowledgments

- Built with the [rvoip library](https://github.com/eisenzopf/rvoip)
- Uses rvoip's client-core for proper SIP handling
- Thanks to the Rust community for excellent crates

## How It Works

When a caller dials your bandwidth.com number (or any SIP client calls your server):

### 📞 Real SIP Call Flow

```
Caller ──INVITE──► rvoip Server ──100 Trying──► Caller
       ◄─────────           ──180 Ringing─► 
       ◄─────────           ──200 OK──────►
       ──ACK─────►          
       ◄═══RTP Media Stream═══════════════►
       ◄─── MP3 Audio for 30s ───────────►
       ──BYE─────►          ──200 OK──────►
```

### 🔄 Server Processing Steps

1. **📡 SIP INVITE Received**: rvoip handles the full SIP protocol stack
2. **🔄 Automatic Processing**: `AutoAnswerHandler.on_incoming_call()` triggered
3. **⏱️ Configurable Delay**: Server waits 1 second (configurable) before answering
4. **📋 SDP Negotiation**: Media parameters negotiated automatically via rvoip
5. **📞 Call Answered**: Server sends 200 OK with SDP answer
6. **📥 MP3 Processing**: Downloads and processes `jocofullinterview41.mp3` with telephony optimization
7. **🎛️ Audio Processing**: Applies 3-band compression, bandpass filtering, and noise gating
8. **🎵 Media Flow**: RTP stream established with processed μ-law audio samples
9. **📊 Monitoring**: Real-time call quality metrics collected
10. **⏰ Duration**: Plays for exactly 30 seconds
11. **📴 Clean Hangup**: Call terminated gracefully with final statistics

### 📊 What You'll See in Logs

```bash
📞 Auto-answering incoming call from sip:+1234567890@bandwidth.com to sip:server@yourserver.com
✅ Generated SDP answer successfully  
📡 Establishing media flow to 192.168.1.100:15004 with codec support: ["PCMU"]
📥 Downloading MP3 file from: https://archive.org/download/NeverGonnaGiveYouUp/jocofullinterview41.mp3
🔄 Converting MP3 to WAV format (8000Hz, 1 channels) with telephony processing
✅ Media flow established, starting MP3 audio transmission
🎵 Starting custom MP3 audio transmission for call abc123-def456
📊 Server Statistics: Calls: 1 received, 1 active, 1 accepted, 0 rejected
📴 Call abc123-def456 ended: Normal hangup
📊 Final call statistics: Duration: 30s, Packets: 1500 sent, 1480 received
```

The caller will hear a 30-second excerpt from the processed MP3 audio file!

---

**Note**: This is a **test server** designed to demonstrate rvoip library capabilities. It downloads and plays a specific MP3 file (`jocofullinterview41.mp3`) for testing SIP auto-answer and audio streaming functionality. Always test thoroughly in your specific environment. 