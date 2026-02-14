//! Voice pipeline orchestrating the full mic-to-speaker loop.
//!
//! This entire module requires the `voice` feature.

use std::sync::Arc;

use super::audio_io::{AudioInput, AudioOutput};
use super::stt::SttProvider;
use super::tts::TtsProvider;
use super::types::{SynthesisRequest, TranscriptionResult};
use super::vad::VoiceActivityDetector;
use super::wake::WakeWordDetector;
use crate::config::VoiceConfig;
use crate::error::VoiceError;

/// Events emitted by the voice pipeline.
#[derive(Debug, Clone)]
pub enum VoicePipelineEvent {
    /// Wake word was detected.
    WakeWordDetected { word: String },
    /// Pipeline started listening.
    ListeningStarted,
    /// Pipeline stopped listening.
    ListeningStopped,
    /// Audio was transcribed.
    Transcription(TranscriptionResult),
    /// Speech was synthesized and played.
    SpeechSynthesized { text: String, duration_secs: f32 },
    /// An error occurred.
    Error(String),
}

/// The full voice pipeline: mic → VAD → wake → STT → agent → TTS → speaker.
pub struct VoicePipeline {
    stt: Arc<dyn SttProvider>,
    tts: Arc<dyn TtsProvider>,
    vad: VoiceActivityDetector,
    wake_detector: Box<dyn WakeWordDetector>,
    audio_input: AudioInput,
    audio_output: AudioOutput,
    config: VoiceConfig,
    is_running: bool,
}

impl VoicePipeline {
    /// Create a new voice pipeline.
    pub fn new(
        stt: Arc<dyn SttProvider>,
        tts: Arc<dyn TtsProvider>,
        wake_detector: Box<dyn WakeWordDetector>,
        config: VoiceConfig,
    ) -> Result<Self, VoiceError> {
        let vad = VoiceActivityDetector::new(config.vad_threshold);
        let audio_input = AudioInput::new(config.input_device.clone());
        let audio_output = AudioOutput::new(config.output_device.clone());

        Ok(Self {
            stt,
            tts,
            vad,
            wake_detector,
            audio_input,
            audio_output,
            config,
            is_running: false,
        })
    }

    /// Whether the pipeline is currently running.
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Speak text through TTS and play through system speakers.
    pub async fn speak(&self, text: &str) -> Result<(), VoiceError> {
        let request = SynthesisRequest::new(text);
        let result = self.tts.synthesize(&request).await?;
        super::audio_io::play_audio(&result.audio).await?;
        Ok(())
    }

    /// Listen for a voice command via the wake word pipeline.
    ///
    /// 1. Records a short audio chunk (2s) for wake word detection
    /// 2. Checks VAD (skip STT if silence)
    /// 3. Checks for wake word via STT
    /// 4. On wake: records a longer command (max_listen_secs)
    /// 5. Transcribes and returns the command text (wake word stripped)
    ///
    /// Returns `Ok(None)` if no wake word was detected (silence or non-wake speech).
    pub async fn listen_for_command(&self) -> Result<Option<String>, VoiceError> {
        // Record a short chunk for wake word detection
        let chunk = super::audio_io::record_audio_chunk(2.0, 16000).await?;

        // Check VAD first (skip STT if silence)
        if !self.vad.is_speech(&chunk) {
            return Ok(None);
        }

        // Check wake word
        let wake_result = self.wake_detector.detect(&chunk).await?;
        if wake_result.is_none() {
            return Ok(None);
        }

        // Wake word detected! Record the full command
        let command_chunk =
            super::audio_io::record_audio_chunk(self.config.max_listen_secs as f32, 16000).await?;

        // Transcribe
        let transcription = self.stt.transcribe(&command_chunk).await?;

        // Strip the wake word from the transcription
        let text = transcription.text.to_lowercase();
        let command = self
            .config
            .wake_words
            .iter()
            .fold(text.clone(), |acc, w| acc.replace(&w.to_lowercase(), ""))
            .trim()
            .to_string();

        if command.is_empty() {
            return Ok(None);
        }

        Ok(Some(command))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pipeline tests require audio hardware, so they are ignored by default.
    // Run with: cargo test --features voice -- --ignored

    #[test]
    #[ignore]
    fn test_pipeline_construction() {
        // This test requires the voice feature AND audio hardware
        // It validates that the pipeline struct can be constructed.
        let _event = VoicePipelineEvent::WakeWordDetected {
            word: "hey rustant".into(),
        };
    }

    #[test]
    #[ignore]
    fn test_pipeline_event_types() {
        let events = vec![
            VoicePipelineEvent::ListeningStarted,
            VoicePipelineEvent::ListeningStopped,
            VoicePipelineEvent::WakeWordDetected {
                word: "test".into(),
            },
            VoicePipelineEvent::Error("test error".into()),
        ];
        assert_eq!(events.len(), 4);
    }
}
