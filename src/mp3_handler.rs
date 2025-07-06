use anyhow::{Context, Result};
use log::{info, warn};
use std::path::Path;
use std::fs::File;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;
use hound::{WavSpec, WavWriter};

const MP3_FILENAME: &str = "jocofullinterview41.mp3";
const MP3_URL: &str = "https://archive.org/download/NeverGonnaGiveYouUp/jocofullinterview41.mp3";
const WAV_FILENAME: &str = "jocofullinterview41.wav";

pub struct Mp3Handler {
    mp3_path: String,
    wav_path: String,
    telephony_processor: TelephonyAudioProcessor,
}

impl Mp3Handler {
    pub fn new() -> Self {
        Self {
            mp3_path: MP3_FILENAME.to_string(),
            wav_path: WAV_FILENAME.to_string(),
            telephony_processor: TelephonyAudioProcessor::new(8000.0),
        }
    }

    /// Download the MP3 file if it doesn't exist
    pub async fn ensure_mp3_downloaded(&self) -> Result<()> {
        if Path::new(&self.mp3_path).exists() {
            info!("🎵 MP3 file already exists: {}", self.mp3_path);
            return Ok(());
        }

        info!("📥 Downloading MP3 file from: {}", MP3_URL);
        
        let response = reqwest::get(MP3_URL)
            .await
            .context("Failed to download MP3 file")?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to download MP3: HTTP {}", response.status()));
        }

        let bytes = response.bytes()
            .await
            .context("Failed to read MP3 response body")?;

        let mut file = File::create(&self.mp3_path)
            .context("Failed to create MP3 file")?;
        
        use std::io::Write;
        file.write_all(&bytes)
            .context("Failed to write MP3 file")?;

        info!("✅ MP3 file downloaded successfully: {} ({} bytes)", self.mp3_path, bytes.len());
        Ok(())
    }

    /// Convert MP3 to WAV format with specified parameters and proper resampling
    pub fn convert_mp3_to_wav(&mut self, target_sample_rate: u32, channels: u16) -> Result<()> {
        if Path::new(&self.wav_path).exists() {
            info!("🎵 WAV file already exists: {}", self.wav_path);
            return Ok(());
        }

        info!("🔄 Converting MP3 to WAV format ({}Hz, {} channels) with telephony processing", target_sample_rate, channels);

        let file = File::open(&self.mp3_path)
            .context("Failed to open MP3 file")?;
        
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();
        
        let probed = get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .context("Failed to probe MP3 file")?;
        
        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .context("No valid audio track found")?;
        
        let track_id = track.id;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: false })
            .context("Failed to create decoder")?;
        
        // Get source sample rate from the MP3
        let source_sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        info!("🎼 Source MP3 sample rate: {}Hz, target: {}Hz", source_sample_rate, target_sample_rate);
        
        let spec = WavSpec {
            channels,
            sample_rate: target_sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        
        let mut writer = WavWriter::create(&self.wav_path, spec)
            .context("Failed to create WAV writer")?;
        
        let mut sample_count = 0;
        let max_samples = target_sample_rate as usize * 30; // 30 seconds at target rate
        let mut resampler = SimpleResampler::new(source_sample_rate, target_sample_rate);
        
        // Reset telephony processor for fresh start
        self.telephony_processor.reset();
        
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::ResetRequired) => {
                    // The track list has been changed. Re-examine it and create a new set of decoders,
                    // then restart the decode loop. This is an advanced feature that most applications
                    // do not need.
                    break;
                }
                Err(SymphoniaError::IoError(_)) => {
                    // The packet reader has reached the end of the file.
                    break;
                }
                Err(err) => {
                    // A unrecoverable error occurred, halt decoding.
                    return Err(err.into());
                }
            };
            
            if packet.track_id() != track_id {
                continue;
            }
            
            let audio_buf = decoder.decode(&packet)
                .context("Failed to decode audio packet")?;
            
            // Convert to the target format and write samples
            match audio_buf {
                AudioBufferRef::F32(buf) => {
                    // Process samples with resampling and telephony processing
                    for &sample in buf.chan(0) {
                        if sample_count >= max_samples {
                            break;
                        }
                        
                        // Resample if needed
                        let resampled_samples = if source_sample_rate != target_sample_rate {
                            resampler.process_sample(sample)
                        } else {
                            vec![sample]
                        };
                        
                        for resampled_sample in resampled_samples {
                            if sample_count >= max_samples {
                                break;
                            }
                            
                            // Apply telephony processing for better phone call quality
                            let processed_sample = self.telephony_processor.process_sample(resampled_sample);
                            
                            let sample_i16 = (processed_sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            writer.write_sample(sample_i16)
                                .context("Failed to write sample")?;
                            sample_count += 1;
                        }
                    }
                }
                AudioBufferRef::F64(buf) => {
                    // Process samples with resampling and telephony processing
                    for &sample in buf.chan(0) {
                        if sample_count >= max_samples {
                            break;
                        }
                        
                        let sample_f32 = sample as f32;
                        
                        // Resample if needed
                        let resampled_samples = if source_sample_rate != target_sample_rate {
                            resampler.process_sample(sample_f32)
                        } else {
                            vec![sample_f32]
                        };
                        
                        for resampled_sample in resampled_samples {
                            if sample_count >= max_samples {
                                break;
                            }
                            
                            // Apply telephony processing for better phone call quality
                            let processed_sample = self.telephony_processor.process_sample(resampled_sample);
                            
                            let sample_i16 = (processed_sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            writer.write_sample(sample_i16)
                                .context("Failed to write sample")?;
                            sample_count += 1;
                        }
                    }
                }
                _ => {
                    warn!("Unsupported audio buffer format");
                }
            }
            
            if sample_count >= max_samples {
                break;
            }
        }
        
        writer.finalize()
            .context("Failed to finalize WAV file")?;
        
        info!("✅ MP3 converted to WAV with telephony processing: {} ({} samples at {}Hz)", 
              self.wav_path, sample_count, target_sample_rate);
        Ok(())
    }

    /// Read WAV file samples for streaming
    pub fn read_wav_samples(&self) -> Result<Vec<i16>> {
        let mut reader = hound::WavReader::open(&self.wav_path)
            .context("Failed to open WAV file")?;
        
        let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
        let samples = samples.context("Failed to read WAV samples")?;
        
        info!("📊 Loaded {} samples from WAV file", samples.len());
        Ok(samples)
    }
    
    /// Convert PCM samples to μ-law for PCMU codec
    pub fn pcm_to_mulaw(&self, pcm_samples: &[i16]) -> Vec<u8> {
        pcm_samples.iter().map(|&sample| {
            self.linear_to_mulaw(sample)
        }).collect()
    }
    
    /// Convert linear PCM to μ-law (G.711)
    fn linear_to_mulaw(&self, pcm: i16) -> u8 {
        const BIAS: i16 = 0x84;
        const CLIP: i16 = 32635;

        let sign = if pcm < 0 { 0x80 } else { 0 };
        let sample = if pcm < 0 { -pcm } else { pcm };
        let sample = if sample > CLIP { CLIP } else { sample };
        let sample = sample + BIAS;

        let exponent = if sample >= 0x7FFF { 7 }
        else if sample >= 0x4000 { 6 }
        else if sample >= 0x2000 { 5 }
        else if sample >= 0x1000 { 4 }
        else if sample >= 0x0800 { 3 }
        else if sample >= 0x0400 { 2 }
        else if sample >= 0x0200 { 1 }
        else { 0 };

        let mantissa = (sample >> (exponent + 3)) & 0x0F;
        let mulaw = sign | (exponent << 4) | mantissa;
        !mulaw as u8
    }
}

/// Simple linear resampler for basic sample rate conversion
struct SimpleResampler {
    source_rate: u32,
    target_rate: u32,
    position: f64,
    last_sample: f32,
}

impl SimpleResampler {
    fn new(source_rate: u32, target_rate: u32) -> Self {
        Self {
            source_rate,
            target_rate,
            position: 0.0,
            last_sample: 0.0,
        }
    }
    
    fn process_sample(&mut self, input_sample: f32) -> Vec<f32> {
        let mut output_samples = Vec::new();
        
        // For downsampling, advance position by target_rate/source_rate  
        self.position += self.target_rate as f64 / self.source_rate as f64;
        
        // When position >= 1.0, output a sample
        if self.position >= 1.0 {
            // Use linear interpolation for better quality
            let interpolated = self.last_sample + (input_sample - self.last_sample) * 0.5;
            output_samples.push(interpolated);
            self.position -= 1.0;
        }
        
        self.last_sample = input_sample;
        output_samples
    }
}

/// Telephony-optimized audio processor for 8000Hz phone calls
pub struct TelephonyAudioProcessor {
    sample_rate: f32,
    // Preemphasis filter state
    preemphasis_prev: f32,
    // Bandpass filter states (2nd order Butterworth)
    bandpass_x1: f32,
    bandpass_x2: f32,
    bandpass_y1: f32,
    bandpass_y2: f32,
    // Dynamic range compressor
    compressor_envelope: f32,
    // Noise gate
    noise_gate_threshold: f32,
    noise_gate_ratio: f32,
}

impl TelephonyAudioProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            preemphasis_prev: 0.0,
            bandpass_x1: 0.0,
            bandpass_x2: 0.0,
            bandpass_y1: 0.0,
            bandpass_y2: 0.0,
            compressor_envelope: 0.0,
            noise_gate_threshold: 0.01, // -40dB threshold
            noise_gate_ratio: 0.1,
        }
    }
    
    /// Process audio sample through the telephony pipeline
    pub fn process_sample(&mut self, input: f32) -> f32 {
        // Step 1: Preemphasis filter (boost high frequencies)
        let preemphasized = self.preemphasis_filter(input);
        
        // Step 2: Bandpass filter (300-3400Hz for telephony)
        let bandpassed = self.bandpass_filter(preemphasized);
        
        // Step 3: Dynamic range compression
        let compressed = self.dynamic_range_compressor(bandpassed);
        
        // Step 4: Noise gate
        let gated = self.noise_gate(compressed);
        
        // Step 5: Final limiting to prevent clipping
        self.soft_limiter(gated)
    }
    
    /// Preemphasis filter - boosts high frequencies for better telephony transmission
    fn preemphasis_filter(&mut self, input: f32) -> f32 {
        // Simple first-order high-pass filter with alpha = 0.95
        let alpha = 0.95;
        let output = input - alpha * self.preemphasis_prev;
        self.preemphasis_prev = input;
        output
    }
    
    /// Bandpass filter 300-3400Hz (telephony bandwidth)
    fn bandpass_filter(&mut self, input: f32) -> f32 {
        // 2nd order Butterworth bandpass filter coefficients for 300-3400Hz @ 8000Hz
        let low_freq: f32 = 300.0;
        let high_freq: f32 = 3400.0;
        let nyquist = self.sample_rate / 2.0;
        
        // Ensure frequencies are within Nyquist limit
        let low_freq = low_freq.min(nyquist * 0.95);
        let high_freq = high_freq.min(nyquist * 0.95);
        
        // Normalized frequencies (0 to 1, where 1 is Nyquist)
        let wc1 = low_freq / nyquist;
        let wc2 = high_freq / nyquist;
        
        // Pre-warped frequencies for bilinear transform
        let wc1_pre = (std::f32::consts::PI * wc1 / 2.0).tan();
        let wc2_pre = (std::f32::consts::PI * wc2 / 2.0).tan();
        
        // Bandpass filter design using proper bilinear transform
        let bw = wc2_pre - wc1_pre;
        let wc = (wc1_pre * wc2_pre).sqrt();
        
        // Second-order bandpass coefficients
        let norm = 1.0 + bw + wc * wc;
        let b0 = bw / norm;
        let b1 = 0.0;
        let b2 = -bw / norm;
        let a1 = (2.0 * (wc * wc - 1.0)) / norm;
        let a2 = (1.0 - bw + wc * wc) / norm;
        
        // Apply filter (Direct Form II)
        let output = b0 * input + b1 * self.bandpass_x1 + b2 * self.bandpass_x2 
                   - a1 * self.bandpass_y1 - a2 * self.bandpass_y2;
        
        // Update state variables
        self.bandpass_x2 = self.bandpass_x1;
        self.bandpass_x1 = input;
        self.bandpass_y2 = self.bandpass_y1;
        self.bandpass_y1 = output;
        
        // Prevent NaN/Inf propagation
        if output.is_finite() { output } else { 0.0 }
    }
    
    /// Dynamic range compressor for consistent volume levels
    fn dynamic_range_compressor(&mut self, input: f32) -> f32 {
        let input_level = input.abs();
        let target_level = 0.5; // Target RMS level for consistent loudness
        let attack_time = 0.003; // 3ms attack (faster than 1ms to avoid artifacts)
        let release_time = 0.1;  // 100ms release
        
        let attack_coeff = (-1.0 / (attack_time * self.sample_rate)).exp();
        let release_coeff = (-1.0 / (release_time * self.sample_rate)).exp();
        
        // Envelope follower with proper attack/release
        if input_level > self.compressor_envelope {
            self.compressor_envelope = attack_coeff * self.compressor_envelope + (1.0 - attack_coeff) * input_level;
        } else {
            self.compressor_envelope = release_coeff * self.compressor_envelope + (1.0 - release_coeff) * input_level;
        }
        
        // Professional compressor with proper knee
        let ratio = 3.0; // 3:1 compression ratio
        let threshold = target_level * 0.7; // Threshold at 70% of target level
        let knee_width = 0.1; // Soft knee
        
        let gain = if self.compressor_envelope > threshold {
            let excess = self.compressor_envelope - threshold;
            
            // Soft knee compression
            let knee_ratio = if excess < knee_width {
                1.0 + (ratio - 1.0) * (excess / knee_width).powi(2)
            } else {
                ratio
            };
            
            let compressed_excess = excess / knee_ratio;
            let compressed_level = threshold + compressed_excess;
            
            // Calculate gain reduction
            if self.compressor_envelope > 1e-10 {
                compressed_level / self.compressor_envelope
            } else {
                1.0
            }
        } else {
            // Gentle makeup gain for quiet signals
            let makeup_gain = (target_level / (threshold + 1e-10)).min(1.2);
            makeup_gain
        };
        
        // Apply gain with safety limits
        let output = input * gain.clamp(0.1, 2.0);
        
        // Prevent NaN/Inf propagation
        if output.is_finite() { output } else { 0.0 }
    }
    
    /// Noise gate to reduce background noise
    fn noise_gate(&mut self, input: f32) -> f32 {
        let input_level = input.abs();
        
        if input_level < self.noise_gate_threshold {
            input * self.noise_gate_ratio
        } else {
            input
        }
    }
    
    /// Soft limiter to prevent clipping
    fn soft_limiter(&self, input: f32) -> f32 {
        let threshold = 0.9;
        
        if input.abs() > threshold {
            threshold * input.signum() * (1.0 - (-3.0 * (input.abs() - threshold)).exp())
        } else {
            input
        }
    }
    
    /// Reset all filter states
    pub fn reset(&mut self) {
        self.preemphasis_prev = 0.0;
        self.bandpass_x1 = 0.0;
        self.bandpass_x2 = 0.0;
        self.bandpass_y1 = 0.0;
        self.bandpass_y2 = 0.0;
        self.compressor_envelope = 0.0;
    }
}

 