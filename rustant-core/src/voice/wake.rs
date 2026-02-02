//! Wake word detection trait and implementations.
//!
//! `WakeWordDetector` trait, `MockWakeDetector`, and `SttWakeDetector` are
//! always available. `PorcupineWakeDetector` requires the `voice` feature.

use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::VoiceError;
use super::types::AudioChunk;
use super::stt::SttProvider;

/// Trait for wake word detectors.
#[async_trait]
pub trait WakeWordDetector: Send + Sync {
    /// Process an audio chunk and check for wake word.
    /// Returns `Some(detected_word)` if a wake word was detected.
    async fn detect(&self, audio: &AudioChunk) -> Result<Option<String>, VoiceError>;

    /// Get the list of wake words being listened for.
    fn wake_words(&self) -> &[String];

    /// Reset the detector state.
    fn reset(&mut self);
}

/// A mock wake word detector for testing.
pub struct MockWakeDetector {
    words: Vec<String>,
    trigger_after_calls: usize,
    call_count: AtomicUsize,
}

impl MockWakeDetector {
    /// Create a mock that triggers after `n` calls.
    pub fn new(wake_words: Vec<String>, trigger_after_calls: usize) -> Self {
        Self {
            words: wake_words,
            trigger_after_calls,
            call_count: AtomicUsize::new(0),
        }
    }

    /// Number of times `detect` was called.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl WakeWordDetector for MockWakeDetector {
    async fn detect(&self, _audio: &AudioChunk) -> Result<Option<String>, VoiceError> {
        let count = self.call_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.trigger_after_calls && !self.words.is_empty() {
            Ok(Some(self.words[0].clone()))
        } else {
            Ok(None)
        }
    }

    fn wake_words(&self) -> &[String] {
        &self.words
    }

    fn reset(&mut self) {
        self.call_count.store(0, Ordering::Relaxed);
    }
}

/// Wake word detection using any STT provider as a fallback.
///
/// Transcribes audio chunks and checks if the transcription contains
/// any of the configured wake words (case-insensitive).
pub struct SttWakeDetector {
    stt: Box<dyn SttProvider>,
    words: Vec<String>,
    sensitivity: f32,
}

impl SttWakeDetector {
    /// Create a new STT-based wake word detector.
    pub fn new(
        stt: Box<dyn SttProvider>,
        wake_words: Vec<String>,
        sensitivity: f32,
    ) -> Self {
        // Store wake words in lowercase for case-insensitive matching
        let words = wake_words.iter().map(|w| w.to_lowercase()).collect();
        Self {
            stt,
            words,
            sensitivity: sensitivity.clamp(0.0, 1.0),
        }
    }
}

#[async_trait]
impl WakeWordDetector for SttWakeDetector {
    async fn detect(&self, audio: &AudioChunk) -> Result<Option<String>, VoiceError> {
        let result = self.stt.transcribe(audio).await?;
        let text_lower = result.text.to_lowercase();

        // Check if the transcription contains any wake word
        for word in &self.words {
            if text_lower.contains(word.as_str()) {
                // Only trigger if confidence meets sensitivity threshold
                if result.confidence >= self.sensitivity {
                    return Ok(Some(word.clone()));
                }
            }
        }
        Ok(None)
    }

    fn wake_words(&self) -> &[String] {
        &self.words
    }

    fn reset(&mut self) {
        // No state to reset for STT-based detection
    }
}

/// Picovoice Porcupine wake word detector (requires `voice` feature).
#[cfg(feature = "voice")]
pub struct PorcupineWakeDetector {
    words: Vec<String>,
    sensitivity: f32,
}

#[cfg(feature = "voice")]
impl PorcupineWakeDetector {
    /// Create a new Porcupine wake word detector.
    pub fn new(wake_words: Vec<String>, sensitivity: f32) -> Self {
        Self {
            words: wake_words,
            sensitivity,
        }
    }
}

#[cfg(feature = "voice")]
#[async_trait]
impl WakeWordDetector for PorcupineWakeDetector {
    async fn detect(&self, _audio: &AudioChunk) -> Result<Option<String>, VoiceError> {
        // Real implementation would use pv_porcupine to process audio frames.
        Err(VoiceError::WakeWordError {
            message: "Porcupine SDK not initialized".into(),
        })
    }

    fn wake_words(&self) -> &[String] {
        &self.words
    }

    fn reset(&mut self) {
        // Reset Porcupine internal state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::types::TranscriptionResult;
    use crate::voice::stt::MockSttProvider;

    #[tokio::test]
    async fn test_mock_wake_triggers_after_n_calls() {
        let detector = MockWakeDetector::new(
            vec!["hey rustant".to_string()],
            3,
        );
        let chunk = AudioChunk::silence(16000, 1, 480);

        // First two calls: no detection
        assert!(detector.detect(&chunk).await.unwrap().is_none());
        assert!(detector.detect(&chunk).await.unwrap().is_none());

        // Third call: triggers
        let result = detector.detect(&chunk).await.unwrap();
        assert_eq!(result, Some("hey rustant".to_string()));
    }

    #[tokio::test]
    async fn test_mock_wake_reset() {
        let mut detector = MockWakeDetector::new(
            vec!["hey rustant".to_string()],
            2,
        );
        let chunk = AudioChunk::silence(16000, 1, 480);

        detector.detect(&chunk).await.unwrap();
        assert_eq!(detector.call_count(), 1);

        detector.reset();
        assert_eq!(detector.call_count(), 0);

        // After reset, should need 2 calls again
        assert!(detector.detect(&chunk).await.unwrap().is_none());
        let result = detector.detect(&chunk).await.unwrap();
        assert_eq!(result, Some("hey rustant".to_string()));
    }

    #[tokio::test]
    async fn test_stt_wake_detector_detects_keyword() {
        let mock_stt = MockSttProvider::with_responses(vec![TranscriptionResult {
            text: "hey rustant what time is it".into(),
            confidence: 0.95,
            language: Some("en".into()),
            segments: Vec::new(),
            duration_secs: 2.0,
        }]);

        let detector = SttWakeDetector::new(
            Box::new(mock_stt),
            vec!["hey rustant".to_string()],
            0.5,
        );
        let chunk = AudioChunk::silence(16000, 1, 480);

        let result = detector.detect(&chunk).await.unwrap();
        assert_eq!(result, Some("hey rustant".to_string()));
    }

    #[tokio::test]
    async fn test_stt_wake_detector_no_match() {
        let mock_stt = MockSttProvider::with_responses(vec![TranscriptionResult {
            text: "hello world".into(),
            confidence: 0.9,
            language: None,
            segments: Vec::new(),
            duration_secs: 1.0,
        }]);

        let detector = SttWakeDetector::new(
            Box::new(mock_stt),
            vec!["hey rustant".to_string()],
            0.5,
        );
        let chunk = AudioChunk::silence(16000, 1, 480);

        let result = detector.detect(&chunk).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_wake_word_case_insensitive() {
        let mock_stt = MockSttProvider::with_responses(vec![TranscriptionResult {
            text: "Hey Rustant please help".into(),
            confidence: 0.9,
            language: None,
            segments: Vec::new(),
            duration_secs: 1.5,
        }]);

        let detector = SttWakeDetector::new(
            Box::new(mock_stt),
            vec!["hey rustant".to_string()],
            0.5,
        );
        let chunk = AudioChunk::silence(16000, 1, 480);

        let result = detector.detect(&chunk).await.unwrap();
        assert_eq!(result, Some("hey rustant".to_string()));
    }
}
