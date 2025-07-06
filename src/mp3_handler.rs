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
use crate::config::{AudioProcessingConfig, CompressorBandConfig};

const MP3_FILENAME: &str = "jocofullinterview41.mp3";
const MP3_URL: &str = "https://archive.org/download/NeverGonnaGiveYouUp/jocofullinterview41.mp3";
const WAV_FILENAME: &str = "jocofullinterview41.wav";

pub struct Mp3Handler {
    mp3_path: String,
    wav_path: String,
    telephony_processor: TelephonyAudioProcessor,
}

impl Mp3Handler {
    pub fn new(audio_config: &AudioProcessingConfig) -> Self {
        Self {
            mp3_path: MP3_FILENAME.to_string(),
            wav_path: WAV_FILENAME.to_string(),
            telephony_processor: TelephonyAudioProcessor::new(8000.0, audio_config.clone()),
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
    config: AudioProcessingConfig,
    // Preemphasis filter state
    preemphasis_prev: f32,
    // Bandpass filter states (2nd order Butterworth)
    bandpass_x1: f32,
    bandpass_x2: f32,
    bandpass_y1: f32,
    bandpass_y2: f32,
    // 3-band compressor components
    band_filters: BandSplitFilters,
    band1_compressor: BandCompressor,
    band2_compressor: BandCompressor,
    band3_compressor: BandCompressor,
}

/// Band-splitting filters for 3-band processing
struct BandSplitFilters {
    // Low-pass filter for band 1 (low-mid)
    lowpass1_x1: f32,
    lowpass1_x2: f32,
    lowpass1_y1: f32,
    lowpass1_y2: f32,
    // High-pass filter for band 3 (high-mid)
    highpass2_x1: f32,
    highpass2_x2: f32,
    highpass2_y1: f32,
    highpass2_y2: f32,
    // Bandpass filter for band 2 (mid)
    bandpass2_x1: f32,
    bandpass2_x2: f32,
    bandpass2_y1: f32,
    bandpass2_y2: f32,
}

/// Individual compressor for each band
struct BandCompressor {
    envelope: f32,
}

impl BandSplitFilters {
    fn new() -> Self {
        Self {
            lowpass1_x1: 0.0,
            lowpass1_x2: 0.0,
            lowpass1_y1: 0.0,
            lowpass1_y2: 0.0,
            highpass2_x1: 0.0,
            highpass2_x2: 0.0,
            highpass2_y1: 0.0,
            highpass2_y2: 0.0,
            bandpass2_x1: 0.0,
            bandpass2_x2: 0.0,
            bandpass2_y1: 0.0,
            bandpass2_y2: 0.0,
        }
    }
}

impl BandCompressor {
    fn new() -> Self {
        Self {
            envelope: 0.0,
        }
    }
}

impl TelephonyAudioProcessor {
    pub fn new(sample_rate: f32, config: AudioProcessingConfig) -> Self {
        Self {
            sample_rate,
            config,
            preemphasis_prev: 0.0,
            bandpass_x1: 0.0,
            bandpass_x2: 0.0,
            bandpass_y1: 0.0,
            bandpass_y2: 0.0,
            band_filters: BandSplitFilters::new(),
            band1_compressor: BandCompressor::new(),
            band2_compressor: BandCompressor::new(),
            band3_compressor: BandCompressor::new(),
        }
    }
    
    /// Process audio sample through the telephony pipeline
    pub fn process_sample(&mut self, input: f32) -> f32 {
        // Step 1: Preemphasis filter (boost high frequencies)
        let preemphasized = self.preemphasis_filter(input);
        
        // Step 2: Bandpass filter (300-3400Hz for telephony)
        let bandpassed = self.bandpass_filter(preemphasized);
        
        // Step 3: 3-band dynamic range compression
        let compressed = self.three_band_compressor(bandpassed);
        
        // Step 4: Noise gate
        let gated = self.noise_gate(compressed);
        
        // Step 5: Final limiting to prevent clipping
        self.soft_limiter(gated)
    }
    
    /// Preemphasis filter - boosts high frequencies for better telephony transmission
    fn preemphasis_filter(&mut self, input: f32) -> f32 {
        // Simple first-order high-pass filter with configurable alpha
        let alpha = self.config.preemphasis_alpha;
        let output = input - alpha * self.preemphasis_prev;
        self.preemphasis_prev = input;
        output
    }
    
    /// Bandpass filter (configurable telephony bandwidth)
    fn bandpass_filter(&mut self, input: f32) -> f32 {
        // 2nd order Butterworth bandpass filter coefficients with configurable frequencies
        let low_freq: f32 = self.config.bandpass_low_freq;
        let high_freq: f32 = self.config.bandpass_high_freq;
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
    
    /// Apply low-pass filter for band splitting
    fn apply_lowpass_filter(input: f32, cutoff_freq: f32, sample_rate: f32, x1: &mut f32, x2: &mut f32, y1: &mut f32, y2: &mut f32) -> f32 {
        let nyquist = sample_rate / 2.0;
        let wc = cutoff_freq / nyquist;
        let wc_pre = (std::f32::consts::PI * wc / 2.0).tan();
        
        // 2nd order Butterworth low-pass coefficients
        let norm = 1.0 + std::f32::consts::SQRT_2 * wc_pre + wc_pre * wc_pre;
        let b0 = wc_pre * wc_pre / norm;
        let b1 = 2.0 * b0;
        let b2 = b0;
        let a1 = (2.0 * (wc_pre * wc_pre - 1.0)) / norm;
        let a2 = (1.0 - std::f32::consts::SQRT_2 * wc_pre + wc_pre * wc_pre) / norm;
        
        // Apply filter
        let output = b0 * input + b1 * *x1 + b2 * *x2 - a1 * *y1 - a2 * *y2;
        
        // Update state
        *x2 = *x1;
        *x1 = input;
        *y2 = *y1;
        *y1 = output;
        
        if output.is_finite() { output } else { 0.0 }
    }
    
    /// Apply high-pass filter for band splitting
    fn apply_highpass_filter(input: f32, cutoff_freq: f32, sample_rate: f32, x1: &mut f32, x2: &mut f32, y1: &mut f32, y2: &mut f32) -> f32 {
        let nyquist = sample_rate / 2.0;
        let wc = cutoff_freq / nyquist;
        let wc_pre = (std::f32::consts::PI * wc / 2.0).tan();
        
        // 2nd order Butterworth high-pass coefficients
        let norm = 1.0 + std::f32::consts::SQRT_2 * wc_pre + wc_pre * wc_pre;
        let b0 = 1.0 / norm;
        let b1 = -2.0 * b0;
        let b2 = b0;
        let a1 = (2.0 * (wc_pre * wc_pre - 1.0)) / norm;
        let a2 = (1.0 - std::f32::consts::SQRT_2 * wc_pre + wc_pre * wc_pre) / norm;
        
        // Apply filter
        let output = b0 * input + b1 * *x1 + b2 * *x2 - a1 * *y1 - a2 * *y2;
        
        // Update state
        *x2 = *x1;
        *x1 = input;
        *y2 = *y1;
        *y1 = output;
        
        if output.is_finite() { output } else { 0.0 }
    }
    
    /// Apply band-pass filter for band splitting
    fn apply_bandpass_filter(input: f32, low_freq: f32, high_freq: f32, sample_rate: f32, x1: &mut f32, x2: &mut f32, y1: &mut f32, y2: &mut f32) -> f32 {
        let nyquist = sample_rate / 2.0;
        let wc1 = low_freq / nyquist;
        let wc2 = high_freq / nyquist;
        
        let wc1_pre = (std::f32::consts::PI * wc1 / 2.0).tan();
        let wc2_pre = (std::f32::consts::PI * wc2 / 2.0).tan();
        
        // Bandpass filter design
        let bw = wc2_pre - wc1_pre;
        let wc = (wc1_pre * wc2_pre).sqrt();
        
        // 2nd order bandpass coefficients
        let norm = 1.0 + bw + wc * wc;
        let b0 = bw / norm;
        let b1 = 0.0;
        let b2 = -bw / norm;
        let a1 = (2.0 * (wc * wc - 1.0)) / norm;
        let a2 = (1.0 - bw + wc * wc) / norm;
        
        // Apply filter
        let output = b0 * input + b1 * *x1 + b2 * *x2 - a1 * *y1 - a2 * *y2;
        
        // Update state
        *x2 = *x1;
        *x1 = input;
        *y2 = *y1;
        *y1 = output;
        
        if output.is_finite() { output } else { 0.0 }
    }
    
    /// 3-band dynamic range compressor for consistent volume levels
    fn three_band_compressor(&mut self, input: f32) -> f32 {
        // Split the input into 3 frequency bands
        let (band1, band2, band3) = self.split_into_bands(input);
        
        // Extract needed values to avoid borrowing conflicts
        let sample_rate = self.sample_rate;
        let band1_config = self.config.band1_compressor.clone();
        let band2_config = self.config.band2_compressor.clone();
        let band3_config = self.config.band3_compressor.clone();
        
        // Apply compression to each band independently
        let compressed_band1 = Self::compress_band(band1, &band1_config, &mut self.band1_compressor, sample_rate);
        let compressed_band2 = Self::compress_band(band2, &band2_config, &mut self.band2_compressor, sample_rate);
        let compressed_band3 = Self::compress_band(band3, &band3_config, &mut self.band3_compressor, sample_rate);
        
        // Combine the bands back together
        let combined = compressed_band1 + compressed_band2 + compressed_band3;
        
        // Prevent NaN/Inf propagation
        if combined.is_finite() { combined } else { 0.0 }
    }
    
    /// Split audio into 3 frequency bands
    fn split_into_bands(&mut self, input: f32) -> (f32, f32, f32) {
        let nyquist = self.sample_rate / 2.0;
        let split_freq_1 = self.config.band_split_freq_1.min(nyquist * 0.95);
        let split_freq_2 = self.config.band_split_freq_2.min(nyquist * 0.95);
        let sample_rate = self.sample_rate;
        
        // Band 1: Low-pass filter (300Hz - split_freq_1)
        let band1 = Self::apply_lowpass_filter(input, split_freq_1, sample_rate,
            &mut self.band_filters.lowpass1_x1, &mut self.band_filters.lowpass1_x2,
            &mut self.band_filters.lowpass1_y1, &mut self.band_filters.lowpass1_y2);
        
        // Band 3: High-pass filter (split_freq_2 - 3400Hz)
        let band3 = Self::apply_highpass_filter(input, split_freq_2, sample_rate,
            &mut self.band_filters.highpass2_x1, &mut self.band_filters.highpass2_x2,
            &mut self.band_filters.highpass2_y1, &mut self.band_filters.highpass2_y2);
        
        // Band 2: Bandpass filter (split_freq_1 - split_freq_2)
        let band2 = Self::apply_bandpass_filter(input, split_freq_1, split_freq_2, sample_rate,
            &mut self.band_filters.bandpass2_x1, &mut self.band_filters.bandpass2_x2,
            &mut self.band_filters.bandpass2_y1, &mut self.band_filters.bandpass2_y2);
        
        (band1, band2, band3)
    }
    
    /// Apply compression to a single band
    fn compress_band(input: f32, config: &CompressorBandConfig, compressor: &mut BandCompressor, sample_rate: f32) -> f32 {
        if !config.enabled {
            return input;
        }
        
        let input_level = input.abs();
        let target_level = config.target_level;
        let attack_time = config.attack_time;
        let release_time = config.release_time;
        
        let attack_coeff = (-1.0 / (attack_time * sample_rate)).exp();
        let release_coeff = (-1.0 / (release_time * sample_rate)).exp();
        
        // Envelope follower with proper attack/release
        if input_level > compressor.envelope {
            compressor.envelope = attack_coeff * compressor.envelope + (1.0 - attack_coeff) * input_level;
        } else {
            compressor.envelope = release_coeff * compressor.envelope + (1.0 - release_coeff) * input_level;
        }
        
        // Professional compressor with proper knee
        let ratio = config.ratio;
        let threshold = target_level * config.threshold_factor;
        let knee_width = config.knee_width;
        
        let gain = if compressor.envelope > threshold {
            let excess = compressor.envelope - threshold;
            
            // Soft knee compression
            let knee_ratio = if excess < knee_width {
                1.0 + (ratio - 1.0) * (excess / knee_width).powi(2)
            } else {
                ratio
            };
            
            let compressed_excess = excess / knee_ratio;
            let compressed_level = threshold + compressed_excess;
            
            // Calculate gain reduction
            if compressor.envelope > 1e-10 {
                compressed_level / compressor.envelope
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
        
        if input_level < self.config.noise_gate_threshold {
            input * self.config.noise_gate_ratio
        } else {
            input
        }
    }
    
    /// Soft limiter to prevent clipping
    fn soft_limiter(&self, input: f32) -> f32 {
        let threshold = self.config.soft_limiter_threshold;
        
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
        
        // Reset band filter states
        self.band_filters.lowpass1_x1 = 0.0;
        self.band_filters.lowpass1_x2 = 0.0;
        self.band_filters.lowpass1_y1 = 0.0;
        self.band_filters.lowpass1_y2 = 0.0;
        self.band_filters.highpass2_x1 = 0.0;
        self.band_filters.highpass2_x2 = 0.0;
        self.band_filters.highpass2_y1 = 0.0;
        self.band_filters.highpass2_y2 = 0.0;
        self.band_filters.bandpass2_x1 = 0.0;
        self.band_filters.bandpass2_x2 = 0.0;
        self.band_filters.bandpass2_y1 = 0.0;
        self.band_filters.bandpass2_y2 = 0.0;
        
        // Reset compressor states
        self.band1_compressor.envelope = 0.0;
        self.band2_compressor.envelope = 0.0;
        self.band3_compressor.envelope = 0.0;
    }
}

 