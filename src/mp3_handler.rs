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
}

impl Mp3Handler {
    pub fn new() -> Self {
        Self {
            mp3_path: MP3_FILENAME.to_string(),
            wav_path: WAV_FILENAME.to_string(),
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

    /// Convert MP3 to WAV format with specified parameters
    pub fn convert_mp3_to_wav(&self, sample_rate: u32, channels: u16) -> Result<()> {
        if Path::new(&self.wav_path).exists() {
            info!("🎵 WAV file already exists: {}", self.wav_path);
            return Ok(());
        }

        info!("🔄 Converting MP3 to WAV format ({}Hz, {} channels)", sample_rate, channels);

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
        
        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        
        let mut writer = WavWriter::create(&self.wav_path, spec)
            .context("Failed to create WAV writer")?;
        
        let mut sample_count = 0;
        let max_samples = sample_rate as usize * 30; // 30 seconds
        
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
                    for &sample in buf.chan(0) {
                        if sample_count >= max_samples {
                            break;
                        }
                        let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        writer.write_sample(sample_i16)
                            .context("Failed to write sample")?;
                        sample_count += 1;
                    }
                }
                AudioBufferRef::F64(buf) => {
                    for &sample in buf.chan(0) {
                        if sample_count >= max_samples {
                            break;
                        }
                        let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        writer.write_sample(sample_i16)
                            .context("Failed to write sample")?;
                        sample_count += 1;
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
        
        info!("✅ MP3 converted to WAV: {} ({} samples)", self.wav_path, sample_count);
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
}

 