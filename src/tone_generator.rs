use anyhow::Result;
use log::{debug, info};
use std::f32::consts::PI;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ToneConfig {
    pub frequency: f32,
    pub amplitude: f32,
    pub sample_rate: u32,
    pub duration_seconds: f32,
}

impl Default for ToneConfig {
    fn default() -> Self {
        Self {
            frequency: 440.0,  // A4 note
            amplitude: 0.5,    // 50% amplitude
            sample_rate: 8000, // Standard telephony sample rate
            duration_seconds: 30.0,
        }
    }
}

#[derive(Debug)]
pub struct ToneGenerator {
    config: Arc<RwLock<ToneConfig>>,
    is_generating: Arc<RwLock<bool>>,
}

impl ToneGenerator {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(ToneConfig::default())),
            is_generating: Arc::new(RwLock::new(false)),
        }
    }

    pub fn new_with_config(config: ToneConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            is_generating: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn set_config(&self, config: ToneConfig) {
        let mut current_config = self.config.write().await;
        *current_config = config;
        debug!("Tone generator configuration updated");
    }

    pub async fn get_config(&self) -> ToneConfig {
        let config = self.config.read().await;
        config.clone()
    }

    pub async fn is_generating(&self) -> bool {
        let generating = self.is_generating.read().await;
        *generating
    }

    /// Generate a tone as PCM samples
    pub async fn generate_tone(&self) -> Result<Vec<i16>> {
        let config = self.get_config().await;
        
        {
            let mut generating = self.is_generating.write().await;
            *generating = true;
        }

        info!("Generating tone: {}Hz, {}s duration, {}Hz sample rate", 
              config.frequency, config.duration_seconds, config.sample_rate);

        let samples = self.generate_pcm_samples(&config).await?;

        {
            let mut generating = self.is_generating.write().await;
            *generating = false;
        }

        Ok(samples)
    }

    /// Generate a continuous tone that can be streamed
    pub async fn generate_streaming_tone(&self, duration_ms: u64) -> Result<Vec<i16>> {
        let config = self.get_config().await;
        
        let duration_seconds = duration_ms as f32 / 1000.0;
        let mut streaming_config = config.clone();
        streaming_config.duration_seconds = duration_seconds;

        self.generate_pcm_samples(&streaming_config).await
    }

    async fn generate_pcm_samples(&self, config: &ToneConfig) -> Result<Vec<i16>> {
        let total_samples = (config.sample_rate as f32 * config.duration_seconds) as usize;
        let mut samples = Vec::with_capacity(total_samples);

        let angular_frequency = 2.0 * PI * config.frequency;
        let sample_duration = 1.0 / config.sample_rate as f32;

        for i in 0..total_samples {
            let time = i as f32 * sample_duration;
            let sample_value = config.amplitude * (angular_frequency * time).sin();
            
            // Convert to 16-bit PCM
            let pcm_value = (sample_value * i16::MAX as f32) as i16;
            samples.push(pcm_value);
        }

        debug!("Generated {} PCM samples", samples.len());
        Ok(samples)
    }

    /// Generate DTMF tone (dual tone multi-frequency)
    pub async fn generate_dtmf_tone(&self, digit: char, duration_ms: u64) -> Result<Vec<i16>> {
        let (freq1, freq2) = match digit {
            '1' => (697.0, 1209.0),
            '2' => (697.0, 1336.0),
            '3' => (697.0, 1477.0),
            'A' => (697.0, 1633.0),
            '4' => (770.0, 1209.0),
            '5' => (770.0, 1336.0),
            '6' => (770.0, 1477.0),
            'B' => (770.0, 1633.0),
            '7' => (852.0, 1209.0),
            '8' => (852.0, 1336.0),
            '9' => (852.0, 1477.0),
            'C' => (852.0, 1633.0),
            '*' => (941.0, 1209.0),
            '0' => (941.0, 1336.0),
            '#' => (941.0, 1477.0),
            'D' => (941.0, 1633.0),
            _ => return Err(anyhow::anyhow!("Invalid DTMF digit: {}", digit)),
        };

        let config = self.get_config().await;
        let duration_seconds = duration_ms as f32 / 1000.0;
        let total_samples = (config.sample_rate as f32 * duration_seconds) as usize;
        let mut samples = Vec::with_capacity(total_samples);

        let angular_freq1 = 2.0 * PI * freq1;
        let angular_freq2 = 2.0 * PI * freq2;
        let sample_duration = 1.0 / config.sample_rate as f32;

        for i in 0..total_samples {
            let time = i as f32 * sample_duration;
            let sample1 = (angular_freq1 * time).sin();
            let sample2 = (angular_freq2 * time).sin();
            let combined_sample = (sample1 + sample2) * 0.5 * config.amplitude;
            
            // Convert to 16-bit PCM
            let pcm_value = (combined_sample * i16::MAX as f32) as i16;
            samples.push(pcm_value);
        }

        info!("Generated DTMF tone for digit '{}': {:.1}Hz + {:.1}Hz, {}ms", 
              digit, freq1, freq2, duration_ms);
        Ok(samples)
    }

    /// Generate a comfort noise tone (for silence periods)
    pub async fn generate_comfort_noise(&self, duration_ms: u64) -> Result<Vec<i16>> {
        let config = self.get_config().await;
        let duration_seconds = duration_ms as f32 / 1000.0;
        let total_samples = (config.sample_rate as f32 * duration_seconds) as usize;
        let mut samples = Vec::with_capacity(total_samples);

        // Generate low-level white noise
        let mut seed = 12345u64;
        let noise_amplitude = 0.01; // Very low amplitude

        for _ in 0..total_samples {
            // Simple linear congruential generator for noise
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let noise_value = (seed as f32 / u64::MAX as f32 - 0.5) * noise_amplitude;
            
            let pcm_value = (noise_value * i16::MAX as f32) as i16;
            samples.push(pcm_value);
        }

        debug!("Generated comfort noise: {}ms", duration_ms);
        Ok(samples)
    }

    /// Convert PCM samples to μ-law encoding (commonly used in telephony)
    pub fn pcm_to_mulaw(&self, pcm_samples: &[i16]) -> Vec<u8> {
        pcm_samples.iter().map(|&sample| {
            self.linear_to_mulaw(sample)
        }).collect()
    }

    /// Convert PCM samples to A-law encoding (commonly used in telephony)
    pub fn pcm_to_alaw(&self, pcm_samples: &[i16]) -> Vec<u8> {
        pcm_samples.iter().map(|&sample| {
            self.linear_to_alaw(sample)
        }).collect()
    }

    fn linear_to_mulaw(&self, sample: i16) -> u8 {
        // μ-law encoding implementation
        const BIAS: i16 = 0x84;
        const CLIP: i16 = 32635;

        let sign = if sample < 0 { 0x80 } else { 0 };
        let sample = if sample < 0 { -sample } else { sample };
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

    fn linear_to_alaw(&self, sample: i16) -> u8 {
        // A-law encoding implementation
        const ALAW_MAX: i16 = 0x7FFF;
        
        let sign = if sample < 0 { 0x80 } else { 0 };
        let sample = if sample < 0 { -sample } else { sample };
        let sample = if sample > ALAW_MAX { ALAW_MAX } else { sample };

        let exponent = if sample >= 0x4000 { 7 }
        else if sample >= 0x2000 { 6 }
        else if sample >= 0x1000 { 5 }
        else if sample >= 0x0800 { 4 }
        else if sample >= 0x0400 { 3 }
        else if sample >= 0x0200 { 2 }
        else if sample >= 0x0100 { 1 }
        else { 0 };

        let mantissa = if exponent == 0 {
            sample >> 4
        } else {
            (sample >> (exponent + 3)) & 0x0F
        };

        let alaw = sign | (exponent << 4) | mantissa;
        alaw as u8 ^ 0x55
    }

    /// Stop any ongoing tone generation
    pub async fn stop_generation(&self) {
        let mut generating = self.is_generating.write().await;
        *generating = false;
        info!("Tone generation stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[tokio::test]
    async fn test_tone_generation() {
        let generator = ToneGenerator::new();
        let config = ToneConfig {
            frequency: 440.0,
            amplitude: 0.5,
            sample_rate: 8000,
            duration_seconds: 0.1, // 100ms
        };
        
        generator.set_config(config).await;
        let samples = generator.generate_tone().await.unwrap();
        
        // Should have 800 samples for 100ms at 8kHz
        assert_eq!(samples.len(), 800);
        
        // Samples should not all be zero
        assert!(samples.iter().any(|&s| s != 0));
    }

    #[tokio::test]
    async fn test_dtmf_generation() {
        let generator = ToneGenerator::new();
        let samples = generator.generate_dtmf_tone('5', 100).await.unwrap();
        
        // Should have 800 samples for 100ms at 8kHz
        assert_eq!(samples.len(), 800);
        
        // Samples should not all be zero
        assert!(samples.iter().any(|&s| s != 0));
    }

    #[tokio::test]
    async fn test_comfort_noise() {
        let generator = ToneGenerator::new();
        let samples = generator.generate_comfort_noise(100).await.unwrap();
        
        // Should have 800 samples for 100ms at 8kHz
        assert_eq!(samples.len(), 800);
        
        // Samples should be low amplitude noise
        let max_sample = samples.iter().map(|&s| s.abs()).max().unwrap();
        assert!(max_sample < 1000); // Should be low amplitude
    }

    #[test]
    fn test_mulaw_encoding() {
        let generator = ToneGenerator::new();
        let pcm_samples = vec![0, 1000, -1000, 32000, -32000];
        let mulaw_samples = generator.pcm_to_mulaw(&pcm_samples);
        
        assert_eq!(mulaw_samples.len(), pcm_samples.len());
        
        // μ-law of 0 should be 0xFF
        assert_eq!(mulaw_samples[0], 0xFF);
    }

    #[test]
    fn test_alaw_encoding() {
        let generator = ToneGenerator::new();
        let pcm_samples = vec![0, 1000, -1000, 32000, -32000];
        let alaw_samples = generator.pcm_to_alaw(&pcm_samples);
        
        assert_eq!(alaw_samples.len(), pcm_samples.len());
        
        // A-law of 0 should be 0x55
        assert_eq!(alaw_samples[0], 0x55);
    }
} 