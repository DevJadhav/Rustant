//! Text-to-speech provider trait and implementations.
//!
//! The `TtsProvider` trait, `MockTtsProvider`, and `OpenAiTtsProvider` are
//! always available (no feature gate).

use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::VoiceError;
use super::types::{AudioChunk, SynthesisRequest, SynthesisResult};

/// Trait for text-to-speech providers.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Synthesize speech from a request.
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<SynthesisResult, VoiceError>;

    /// Provider name for logging.
    fn name(&self) -> &str;

    /// Whether this provider works offline.
    fn is_offline(&self) -> bool;

    /// List available voices.
    fn available_voices(&self) -> Vec<String>;
}

/// A mock TTS provider for testing. Generates a simple sine wave.
pub struct MockTtsProvider {
    call_count: AtomicUsize,
    voices: Vec<String>,
}

impl MockTtsProvider {
    /// Create a new mock TTS provider.
    pub fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            voices: vec![
                "mock-voice-1".to_string(),
                "mock-voice-2".to_string(),
            ],
        }
    }

    /// Number of times `synthesize` was called.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl Default for MockTtsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TtsProvider for MockTtsProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<SynthesisResult, VoiceError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);

        // Generate a simple 440Hz sine wave for the text length
        let sample_rate = 16000u32;
        let duration_secs = (request.text.len() as f32 * 0.05).max(0.1);
        let num_samples = (sample_rate as f32 * duration_secs) as usize;

        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5
            })
            .collect();

        let audio = AudioChunk::new(samples, sample_rate, 1);

        Ok(SynthesisResult {
            audio,
            duration_secs,
            characters_used: request.text.len(),
        })
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn is_offline(&self) -> bool {
        true
    }

    fn available_voices(&self) -> Vec<String> {
        self.voices.clone()
    }
}

/// OpenAI TTS provider (HTTP, no native deps).
pub struct OpenAiTtsProvider {
    /// API key for authentication.
    pub api_key: String,
    /// Model name (e.g., "tts-1", "tts-1-hd").
    pub model: String,
    /// Default voice name.
    pub voice: String,
    /// Base URL for the API.
    pub base_url: String,
}

impl OpenAiTtsProvider {
    /// Create a new OpenAI TTS provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "tts-1".to_string(),
            voice: "alloy".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Set the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the default voice.
    pub fn with_voice(mut self, voice: impl Into<String>) -> Self {
        self.voice = voice.into();
        self
    }

    /// Set a custom base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl TtsProvider for OpenAiTtsProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<SynthesisResult, VoiceError> {
        let voice = request.voice.as_deref().unwrap_or(&self.voice);
        let url = format!("{}/audio/speech", self.base_url);

        let body = serde_json::json!({
            "model": self.model,
            "input": request.text,
            "voice": voice,
            "speed": request.speed,
            "response_format": "wav",
        });

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| VoiceError::SynthesisFailed {
                message: format!("HTTP request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(VoiceError::SynthesisFailed {
                message: format!("API returned {}: {}", status, body),
            });
        }

        let bytes = response.bytes().await.map_err(|e| VoiceError::SynthesisFailed {
            message: format!("Failed to read response: {}", e),
        })?;

        // Decode the WAV response
        let audio = super::audio_io::audio_convert::decode_wav(&bytes)?;
        let duration_secs = audio.duration_secs();

        Ok(SynthesisResult {
            audio,
            duration_secs,
            characters_used: request.text.len(),
        })
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn is_offline(&self) -> bool {
        false
    }

    fn available_voices(&self) -> Vec<String> {
        vec![
            "alloy".to_string(),
            "echo".to_string(),
            "fable".to_string(),
            "onyx".to_string(),
            "nova".to_string(),
            "shimmer".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_tts_generates_audio() {
        let mock = MockTtsProvider::new();
        let request = SynthesisRequest::new("Hello, world!");
        let result = mock.synthesize(&request).await.unwrap();
        assert!(!result.audio.is_empty());
        assert!(result.duration_secs > 0.0);
        assert_eq!(result.characters_used, 13);
        assert_eq!(result.audio.sample_rate, 16000);
        assert_eq!(result.audio.channels, 1);
    }

    #[tokio::test]
    async fn test_mock_tts_call_count() {
        let mock = MockTtsProvider::new();
        assert_eq!(mock.call_count(), 0);

        let request = SynthesisRequest::new("test");
        let _ = mock.synthesize(&request).await;
        assert_eq!(mock.call_count(), 1);

        let _ = mock.synthesize(&request).await;
        assert_eq!(mock.call_count(), 2);
    }

    #[test]
    fn test_tts_provider_name() {
        let mock = MockTtsProvider::new();
        assert_eq!(mock.name(), "mock");
        assert!(mock.is_offline());

        let openai = OpenAiTtsProvider::new("test-key");
        assert_eq!(openai.name(), "openai");
        assert!(!openai.is_offline());
    }

    #[test]
    fn test_openai_tts_construction() {
        let provider = OpenAiTtsProvider::new("sk-test")
            .with_model("tts-1-hd")
            .with_voice("nova")
            .with_base_url("https://custom.api.com/v1");

        assert_eq!(provider.api_key, "sk-test");
        assert_eq!(provider.model, "tts-1-hd");
        assert_eq!(provider.voice, "nova");
        assert_eq!(provider.base_url, "https://custom.api.com/v1");
    }

    #[test]
    fn test_tts_available_voices() {
        let mock = MockTtsProvider::new();
        let voices = mock.available_voices();
        assert_eq!(voices.len(), 2);
        assert!(voices.contains(&"mock-voice-1".to_string()));

        let openai = OpenAiTtsProvider::new("key");
        let voices = openai.available_voices();
        assert!(voices.len() >= 6);
        assert!(voices.contains(&"alloy".to_string()));
        assert!(voices.contains(&"nova".to_string()));
    }
}
