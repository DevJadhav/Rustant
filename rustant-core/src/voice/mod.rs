//! Voice & Audio Module for Rustant.
//!
//! Provides speech-to-text, text-to-speech, voice activity detection,
//! wake word detection, and audio pipeline orchestration.
//!
//! ## Feature Gating
//!
//! Core types, traits, mocks, HTTP providers, and VAD are always available.
//! Native audio I/O (cpal), local Whisper STT, and Porcupine wake word
//! detection require the `voice` feature flag.

pub mod audio_io;
pub mod stt;
pub mod tts;
pub mod types;
pub mod vad;
pub mod wake;

#[cfg(feature = "voice")]
pub mod pipeline;

// Re-export core types (always available)
pub use audio_io::{audio_convert, play_audio, record_audio_chunk};
pub use stt::{MockSttProvider, OpenAiSttProvider, SttProvider};
pub use tts::{MockTtsProvider, OpenAiTtsProvider, TtsProvider};
pub use types::{
    AudioChunk, AudioFormat, SynthesisRequest, SynthesisResult, TranscriptionResult,
    TranscriptionSegment,
};
pub use vad::{VadEvent, VoiceActivityDetector};
pub use wake::{MockWakeDetector, SttWakeDetector, WakeWordDetector};

// Feature-gated re-exports
#[cfg(feature = "voice")]
pub use audio_io::{AudioInput, AudioOutput};
#[cfg(feature = "voice")]
pub use pipeline::{VoicePipeline, VoicePipelineEvent};
#[cfg(feature = "voice")]
pub use stt::WhisperLocalProvider;
#[cfg(feature = "voice")]
pub use wake::PorcupineWakeDetector;
