//! Speech-to-text provider trait and implementations.
//!
//! The `SttProvider` trait, `MockSttProvider`, and `OpenAiSttProvider` are
//! always available. `WhisperLocalProvider` requires the `voice` feature.

use async_trait::async_trait;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::types::{AudioChunk, TranscriptionResult};
use crate::error::VoiceError;

/// Trait for speech-to-text providers.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe an audio chunk to text.
    async fn transcribe(&self, audio: &AudioChunk) -> Result<TranscriptionResult, VoiceError>;

    /// Provider name for logging.
    fn name(&self) -> &str;

    /// Whether this provider works offline (no network needed).
    fn is_offline(&self) -> bool;
}

/// A mock STT provider for testing.
pub struct MockSttProvider {
    responses: Mutex<Vec<TranscriptionResult>>,
    call_count: AtomicUsize,
}

impl MockSttProvider {
    /// Create a new mock that returns errors (no responses queued).
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(Vec::new()),
            call_count: AtomicUsize::new(0),
        }
    }

    /// Create a mock with pre-configured responses.
    pub fn with_responses(responses: Vec<TranscriptionResult>) -> Self {
        Self {
            responses: Mutex::new(responses),
            call_count: AtomicUsize::new(0),
        }
    }

    /// Number of times `transcribe` was called.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl Default for MockSttProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SttProvider for MockSttProvider {
    async fn transcribe(&self, _audio: &AudioChunk) -> Result<TranscriptionResult, VoiceError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Err(VoiceError::TranscriptionFailed {
                message: "no mock responses queued".into(),
            })
        } else {
            Ok(responses.remove(0))
        }
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn is_offline(&self) -> bool {
        true
    }
}

/// OpenAI Whisper API-based STT provider (HTTP, no native deps).
pub struct OpenAiSttProvider {
    /// API key for authentication.
    pub api_key: String,
    /// Model name (e.g., "whisper-1").
    pub model: String,
    /// Language hint (e.g., "en").
    pub language: String,
    /// Base URL for the API.
    pub base_url: String,
}

impl OpenAiSttProvider {
    /// Create a new OpenAI STT provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "whisper-1".to_string(),
            language: "en".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Set the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the language.
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Set a custom base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl SttProvider for OpenAiSttProvider {
    async fn transcribe(&self, audio: &AudioChunk) -> Result<TranscriptionResult, VoiceError> {
        use super::audio_io::audio_convert;

        if audio.is_empty() {
            return Ok(TranscriptionResult::default());
        }

        // Encode audio to WAV
        let wav_bytes = audio_convert::encode_wav(audio)?;

        // Build multipart form
        let part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| VoiceError::TranscriptionFailed {
                message: format!("MIME error: {e}"),
            })?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", self.model.clone())
            .text("language", self.language.clone())
            .text("response_format", "verbose_json".to_string());

        let url = format!("{}/audio/transcriptions", self.base_url);

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| VoiceError::TranscriptionFailed {
                message: format!("HTTP request failed: {e}"),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(VoiceError::TranscriptionFailed {
                message: format!("API returned {status}: {body}"),
            });
        }

        let json: serde_json::Value =
            response
                .json()
                .await
                .map_err(|e| VoiceError::TranscriptionFailed {
                    message: format!("JSON parse error: {e}"),
                })?;

        let text = json["text"].as_str().unwrap_or("").to_string();
        let language = json["language"].as_str().map(|s| s.to_string());
        let duration = json["duration"].as_f64().unwrap_or(0.0) as f32;

        Ok(TranscriptionResult {
            text,
            confidence: 1.0, // OpenAI doesn't return per-result confidence
            language,
            segments: Vec::new(),
            duration_secs: duration,
        })
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn is_offline(&self) -> bool {
        false
    }
}

/// Whisper-rs local STT provider (requires `voice` feature).
#[cfg(feature = "voice")]
pub struct WhisperLocalProvider {
    /// Path to the Whisper model file.
    pub model_path: String,
    /// Language hint.
    pub language: String,
}

#[cfg(feature = "voice")]
impl WhisperLocalProvider {
    /// Create a new local Whisper provider.
    pub fn new(model_path: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            model_path: model_path.into(),
            language: language.into(),
        }
    }
}

#[cfg(feature = "voice")]
#[async_trait]
impl SttProvider for WhisperLocalProvider {
    async fn transcribe(&self, audio: &AudioChunk) -> Result<TranscriptionResult, VoiceError> {
        if audio.is_empty() {
            return Ok(TranscriptionResult::default());
        }
        // In a real implementation, this would use whisper-rs to transcribe.
        // For now, return an error indicating the model is not loaded.
        Err(VoiceError::ModelNotFound {
            model: self.model_path.clone(),
        })
    }

    fn name(&self) -> &str {
        "whisper-local"
    }

    fn is_offline(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_stt_returns_responses() {
        let responses = vec![
            TranscriptionResult {
                text: "hello".into(),
                confidence: 0.95,
                language: Some("en".into()),
                segments: Vec::new(),
                duration_secs: 1.0,
            },
            TranscriptionResult {
                text: "world".into(),
                confidence: 0.90,
                language: None,
                segments: Vec::new(),
                duration_secs: 0.5,
            },
        ];
        let mock = MockSttProvider::with_responses(responses);
        let chunk = AudioChunk::silence(16000, 1, 480);

        let r1 = mock.transcribe(&chunk).await.unwrap();
        assert_eq!(r1.text, "hello");

        let r2 = mock.transcribe(&chunk).await.unwrap();
        assert_eq!(r2.text, "world");
    }

    #[tokio::test]
    async fn test_mock_stt_call_count() {
        let mock = MockSttProvider::with_responses(vec![
            TranscriptionResult::default(),
            TranscriptionResult::default(),
        ]);
        let chunk = AudioChunk::silence(16000, 1, 480);

        assert_eq!(mock.call_count(), 0);
        let _ = mock.transcribe(&chunk).await;
        assert_eq!(mock.call_count(), 1);
        let _ = mock.transcribe(&chunk).await;
        assert_eq!(mock.call_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_stt_empty_returns_error() {
        let mock = MockSttProvider::new();
        let chunk = AudioChunk::silence(16000, 1, 480);
        let result = mock.transcribe(&chunk).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("no mock responses queued"));
    }

    #[test]
    fn test_stt_provider_name() {
        let mock = MockSttProvider::new();
        assert_eq!(mock.name(), "mock");
        assert!(mock.is_offline());

        let openai = OpenAiSttProvider::new("test-key");
        assert_eq!(openai.name(), "openai");
        assert!(!openai.is_offline());
    }

    #[test]
    fn test_openai_stt_construction() {
        let provider = OpenAiSttProvider::new("sk-test")
            .with_model("whisper-1")
            .with_language("fr")
            .with_base_url("https://custom.api.com/v1");

        assert_eq!(provider.api_key, "sk-test");
        assert_eq!(provider.model, "whisper-1");
        assert_eq!(provider.language, "fr");
        assert_eq!(provider.base_url, "https://custom.api.com/v1");
    }

    #[test]
    fn test_audio_to_wav_encoding() {
        use super::super::audio_io::audio_convert;

        let chunk = AudioChunk::new(vec![0.0, 0.5, -0.5, 1.0], 16000, 1);
        let wav_bytes = audio_convert::encode_wav(&chunk).unwrap();
        assert!(!wav_bytes.is_empty());
        // WAV header check
        assert_eq!(&wav_bytes[0..4], b"RIFF");
        assert_eq!(&wav_bytes[8..12], b"WAVE");
    }
}
