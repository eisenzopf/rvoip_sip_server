use anyhow::Result;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration, Instant};
use uuid::Uuid;

use crate::tone_generator::ToneGenerator;

#[derive(Debug, Clone)]
pub struct CallInfo {
    pub call_id: String,
    pub caller_id: String,
    pub called_number: String,
    pub start_time: Instant,
    pub status: CallStatus,
    pub duration_seconds: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallStatus {
    Ringing,
    Answered,
    InProgress,
    Hanging,
    Terminated,
}

#[derive(Debug)]
pub struct CallStatistics {
    pub total_calls: u64,
    pub answered_calls: u64,
    pub failed_calls: u64,
    pub average_call_duration: f64,
    pub active_calls: u32,
}

#[derive(Debug)]
pub struct CallHandler {
    tone_generator: Arc<ToneGenerator>,
    active_calls: Arc<RwLock<HashMap<String, CallInfo>>>,
    statistics: Arc<RwLock<CallStatistics>>,
    auto_answer_delay_ms: u64,
    max_concurrent_calls: u32,
}

impl CallHandler {
    pub fn new(tone_generator: Arc<ToneGenerator>) -> Self {
        Self {
            tone_generator,
            active_calls: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(CallStatistics::default())),
            auto_answer_delay_ms: 1000, // Default 1 second delay
            max_concurrent_calls: 100,
        }
    }

    pub fn new_with_config(
        tone_generator: Arc<ToneGenerator>,
        auto_answer_delay_ms: u64,
        max_concurrent_calls: u32,
    ) -> Self {
        Self {
            tone_generator,
            active_calls: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(CallStatistics::default())),
            auto_answer_delay_ms,
            max_concurrent_calls,
        }
    }

    /// Handle a new incoming call
    pub async fn handle_incoming_call(
        &self,
        caller_id: &str,
        called_number: &str,
    ) -> Result<String> {
        let call_id = Uuid::new_v4().to_string();
        
        info!("Incoming call from {} to {} (Call ID: {})", 
              caller_id, called_number, call_id);

        // Check if we have capacity for more calls
        {
            let active_calls = self.active_calls.read().await;
            if active_calls.len() >= self.max_concurrent_calls as usize {
                error!("Maximum concurrent calls reached ({}), rejecting call", 
                       self.max_concurrent_calls);
                self.update_statistics(false, 0).await;
                return Err(anyhow::anyhow!("Maximum concurrent calls reached"));
            }
        }

        // Create call info
        let call_info = CallInfo {
            call_id: call_id.clone(),
            caller_id: caller_id.to_string(),
            called_number: called_number.to_string(),
            start_time: Instant::now(),
            status: CallStatus::Ringing,
            duration_seconds: 0,
        };

        // Add to active calls
        {
            let mut active_calls = self.active_calls.write().await;
            active_calls.insert(call_id.clone(), call_info);
        }

        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.total_calls += 1;
            stats.active_calls += 1;
        }

        // Auto-answer the call after delay
        let call_handler = self.clone();
        let call_id_clone = call_id.clone();
        tokio::spawn(async move {
            if let Err(e) = call_handler.auto_answer_call(&call_id_clone).await {
                error!("Failed to auto-answer call {}: {}", call_id_clone, e);
            }
        });

        Ok(call_id)
    }

    /// Auto-answer a call after the configured delay
    async fn auto_answer_call(&self, call_id: &str) -> Result<()> {
        info!("Auto-answering call {} in {}ms", call_id, self.auto_answer_delay_ms);
        
        // Wait for auto-answer delay
        sleep(Duration::from_millis(self.auto_answer_delay_ms)).await;

        // Check if call is still active
        let call_exists = {
            let active_calls = self.active_calls.read().await;
            active_calls.contains_key(call_id)
        };

        if !call_exists {
            warn!("Call {} no longer exists, cannot auto-answer", call_id);
            return Ok(());
        }

        // Answer the call
        self.answer_call(call_id).await?;

        // Start playing tone
        self.start_tone_playback(call_id).await?;

        Ok(())
    }

    /// Answer a call
    pub async fn answer_call(&self, call_id: &str) -> Result<()> {
        info!("Answering call {}", call_id);

        // Update call status
        {
            let mut active_calls = self.active_calls.write().await;
            if let Some(call_info) = active_calls.get_mut(call_id) {
                call_info.status = CallStatus::Answered;
                info!("Call {} answered", call_id);
            } else {
                return Err(anyhow::anyhow!("Call {} not found", call_id));
            }
        }

        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.answered_calls += 1;
        }

        // In a real implementation, you would send SIP 200 OK response here
        // For now, we'll just log the action
        info!("SIP 200 OK sent for call {}", call_id);

        Ok(())
    }

    /// Start playing tone for a call
    async fn start_tone_playback(&self, call_id: &str) -> Result<()> {
        info!("Starting tone playback for call {}", call_id);

        // Update call status
        {
            let mut active_calls = self.active_calls.write().await;
            if let Some(call_info) = active_calls.get_mut(call_id) {
                call_info.status = CallStatus::InProgress;
            } else {
                return Err(anyhow::anyhow!("Call {} not found", call_id));
            }
        }

        // Generate and play tone
        let tone_samples = self.tone_generator.generate_tone().await?;
        info!("Generated {} tone samples for call {}", tone_samples.len(), call_id);

        // Convert to μ-law for SIP/RTP transmission
        let mulaw_samples = self.tone_generator.pcm_to_mulaw(&tone_samples);
        info!("Converted to {} μ-law samples for call {}", mulaw_samples.len(), call_id);

        // In a real implementation, you would send these samples via RTP
        // For now, we'll simulate the playback duration
        let config = self.tone_generator.get_config().await;
        let playback_duration = Duration::from_secs(config.duration_seconds as u64);
        
        info!("Playing tone for {}s on call {}", config.duration_seconds, call_id);
        sleep(playback_duration).await;

        // After tone playback, hang up the call
        self.hangup_call(call_id, "Tone playback completed").await?;

        Ok(())
    }

    /// Hang up a call
    pub async fn hangup_call(&self, call_id: &str, reason: &str) -> Result<()> {
        info!("Hanging up call {} - Reason: {}", call_id, reason);

        let call_duration = {
            let mut active_calls = self.active_calls.write().await;
            if let Some(call_info) = active_calls.get_mut(call_id) {
                call_info.status = CallStatus::Hanging;
                call_info.duration_seconds = call_info.start_time.elapsed().as_secs();
                call_info.duration_seconds
            } else {
                return Err(anyhow::anyhow!("Call {} not found", call_id));
            }
        };

        // In a real implementation, you would send SIP BYE request here
        info!("SIP BYE sent for call {}", call_id);

        // Remove from active calls
        {
            let mut active_calls = self.active_calls.write().await;
            if let Some(call_info) = active_calls.remove(call_id) {
                info!("Call {} terminated after {}s", call_id, call_info.duration_seconds);
            }
        }

        // Update statistics
        self.update_statistics(true, call_duration).await;

        Ok(())
    }

    /// Handle DTMF input during a call
    pub async fn handle_dtmf(&self, call_id: &str, digit: char) -> Result<()> {
        info!("Received DTMF digit '{}' on call {}", digit, call_id);

        // Check if call exists
        let call_exists = {
            let active_calls = self.active_calls.read().await;
            active_calls.contains_key(call_id)
        };

        if !call_exists {
            return Err(anyhow::anyhow!("Call {} not found", call_id));
        }

        // Generate DTMF tone
        let dtmf_samples = self.tone_generator.generate_dtmf_tone(digit, 200).await?;
        let _mulaw_samples = self.tone_generator.pcm_to_mulaw(&dtmf_samples);

        info!("Generated DTMF tone for digit '{}' on call {}", digit, call_id);

        // In a real implementation, you would send the DTMF tone via RTP
        // For now, we'll just log the action
        info!("DTMF tone sent for digit '{}' on call {}", digit, call_id);

        Ok(())
    }

    /// Get information about an active call
    pub async fn get_call_info(&self, call_id: &str) -> Option<CallInfo> {
        let active_calls = self.active_calls.read().await;
        active_calls.get(call_id).cloned()
    }

    /// Get list of all active calls
    pub async fn get_active_calls(&self) -> Vec<CallInfo> {
        let active_calls = self.active_calls.read().await;
        active_calls.values().cloned().collect()
    }

    /// Get call statistics
    pub async fn get_statistics(&self) -> CallStatistics {
        let stats = self.statistics.read().await;
        CallStatistics {
            total_calls: stats.total_calls,
            answered_calls: stats.answered_calls,
            failed_calls: stats.failed_calls,
            average_call_duration: stats.average_call_duration,
            active_calls: stats.active_calls,
        }
    }

    /// Update call statistics
    async fn update_statistics(&self, call_completed: bool, duration_seconds: u64) {
        let mut stats = self.statistics.write().await;
        
        if call_completed {
            // Update average call duration
            let total_completed = stats.answered_calls;
            if total_completed > 0 {
                stats.average_call_duration = (stats.average_call_duration * (total_completed - 1) as f64 
                    + duration_seconds as f64) / total_completed as f64;
            } else {
                stats.average_call_duration = duration_seconds as f64;
            }
        } else {
            stats.failed_calls += 1;
        }
        
        stats.active_calls = stats.active_calls.saturating_sub(1);
    }

    /// Cleanup terminated calls (housekeeping)
    pub async fn cleanup_terminated_calls(&self) {
        let mut active_calls = self.active_calls.write().await;
        let mut to_remove = Vec::new();

        for (call_id, call_info) in active_calls.iter() {
            if call_info.status == CallStatus::Terminated {
                to_remove.push(call_id.clone());
            }
        }

        for call_id in to_remove {
            active_calls.remove(&call_id);
            debug!("Cleaned up terminated call {}", call_id);
        }
    }
}

// Implement Clone for CallHandler to allow spawning async tasks
impl Clone for CallHandler {
    fn clone(&self) -> Self {
        Self {
            tone_generator: self.tone_generator.clone(),
            active_calls: self.active_calls.clone(),
            statistics: self.statistics.clone(),
            auto_answer_delay_ms: self.auto_answer_delay_ms,
            max_concurrent_calls: self.max_concurrent_calls,
        }
    }
}

impl Default for CallStatistics {
    fn default() -> Self {
        Self {
            total_calls: 0,
            answered_calls: 0,
            failed_calls: 0,
            average_call_duration: 0.0,
            active_calls: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[tokio::test]
    async fn test_call_handling() {
        let tone_generator = Arc::new(ToneGenerator::new());
        let call_handler = CallHandler::new(tone_generator);

        // Test incoming call
        let call_id = call_handler
            .handle_incoming_call("sip:alice@example.com", "sip:bob@example.com")
            .await
            .unwrap();

        // Verify call was created
        let call_info = call_handler.get_call_info(&call_id).await.unwrap();
        assert_eq!(call_info.caller_id, "sip:alice@example.com");
        assert_eq!(call_info.called_number, "sip:bob@example.com");
        assert_eq!(call_info.status, CallStatus::Ringing);

        // Test statistics
        let stats = call_handler.get_statistics().await;
        assert_eq!(stats.total_calls, 1);
        assert_eq!(stats.active_calls, 1);
    }

    #[tokio::test]
    async fn test_call_answer() {
        let tone_generator = Arc::new(ToneGenerator::new());
        let call_handler = CallHandler::new(tone_generator);

        let call_id = call_handler
            .handle_incoming_call("sip:alice@example.com", "sip:bob@example.com")
            .await
            .unwrap();

        // Answer the call
        call_handler.answer_call(&call_id).await.unwrap();

        // Verify call status
        let call_info = call_handler.get_call_info(&call_id).await.unwrap();
        assert_eq!(call_info.status, CallStatus::Answered);

        // Test statistics
        let stats = call_handler.get_statistics().await;
        assert_eq!(stats.answered_calls, 1);
    }

    #[tokio::test]
    async fn test_call_hangup() {
        let tone_generator = Arc::new(ToneGenerator::new());
        let call_handler = CallHandler::new(tone_generator);

        let call_id = call_handler
            .handle_incoming_call("sip:alice@example.com", "sip:bob@example.com")
            .await
            .unwrap();

        // Hang up the call
        call_handler.hangup_call(&call_id, "Test hangup").await.unwrap();

        // Verify call is no longer active
        let call_info = call_handler.get_call_info(&call_id).await;
        assert!(call_info.is_none());

        // Test statistics
        let stats = call_handler.get_statistics().await;
        assert_eq!(stats.active_calls, 0);
    }

    #[tokio::test]
    async fn test_dtmf_handling() {
        let tone_generator = Arc::new(ToneGenerator::new());
        let call_handler = CallHandler::new(tone_generator);

        let call_id = call_handler
            .handle_incoming_call("sip:alice@example.com", "sip:bob@example.com")
            .await
            .unwrap();

        // Test DTMF handling
        let result = call_handler.handle_dtmf(&call_id, '5').await;
        assert!(result.is_ok());
    }
} 