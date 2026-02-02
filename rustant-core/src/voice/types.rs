//! Core audio data types for the voice module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported audio formats.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    Pcm16,
    Float32,
    #[default]
    Wav,
    Mp3,
    Opus,
}

/// A chunk of audio data. Internal representation is always f32 samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioChunk {
    /// Audio samples in f32 format (-1.0 to 1.0).
    pub samples: Vec<f32>,
    /// Sample rate in Hz (e.g., 16000, 44100).
    pub sample_rate: u32,
    /// Number of audio channels (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Optional timestamp for when this chunk was captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
}

impl AudioChunk {
    /// Create a new audio chunk from samples.
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
            timestamp: None,
        }
    }

    /// Create a silent audio chunk with the given parameters.
    pub fn silence(sample_rate: u32, channels: u16, num_samples: usize) -> Self {
        Self {
            samples: vec![0.0; num_samples],
            sample_rate,
            channels,
            timestamp: None,
        }
    }

    /// Duration of this chunk in seconds.
    pub fn duration_secs(&self) -> f32 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
    }

    /// Root mean square energy of the audio.
    pub fn rms_energy(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = self.samples.iter().map(|s| s * s).sum();
        (sum_sq / self.samples.len() as f32).sqrt()
    }

    /// Whether this chunk contains no samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Number of audio frames (samples / channels).
    pub fn num_frames(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    /// Append another chunk's samples. Panics if sample rate or channels differ.
    pub fn append(&mut self, other: &AudioChunk) {
        assert_eq!(self.sample_rate, other.sample_rate, "sample rate mismatch");
        assert_eq!(self.channels, other.channels, "channel count mismatch");
        self.samples.extend_from_slice(&other.samples);
    }

    /// Split this chunk at the given sample index, returning (left, right).
    pub fn split_at(&self, sample_index: usize) -> (AudioChunk, AudioChunk) {
        let idx = sample_index.min(self.samples.len());
        let left = AudioChunk {
            samples: self.samples[..idx].to_vec(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            timestamp: self.timestamp,
        };
        let right = AudioChunk {
            samples: self.samples[idx..].to_vec(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            timestamp: None,
        };
        (left, right)
    }
}

/// A segment within a transcription, with timing info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    /// Transcribed text for this segment.
    pub text: String,
    /// Start time in seconds.
    pub start_secs: f32,
    /// End time in seconds.
    pub end_secs: f32,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
}

/// Result of a speech-to-text transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// The full transcribed text.
    pub text: String,
    /// Overall confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// Detected language code (e.g., "en").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Word- or sentence-level segments with timing.
    #[serde(default)]
    pub segments: Vec<TranscriptionSegment>,
    /// Duration of the transcribed audio in seconds.
    pub duration_secs: f32,
}

impl Default for TranscriptionResult {
    fn default() -> Self {
        Self {
            text: String::new(),
            confidence: 0.0,
            language: None,
            segments: Vec::new(),
            duration_secs: 0.0,
        }
    }
}

/// A request to synthesize speech from text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisRequest {
    /// Text to synthesize.
    pub text: String,
    /// Optional voice name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// Speech speed multiplier (1.0 = normal).
    pub speed: f32,
    /// Desired output format.
    pub format: AudioFormat,
}

impl SynthesisRequest {
    /// Create a new synthesis request with defaults.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice: None,
            speed: 1.0,
            format: AudioFormat::Wav,
        }
    }

    /// Set the voice name.
    pub fn with_voice(mut self, voice: impl Into<String>) -> Self {
        self.voice = Some(voice.into());
        self
    }

    /// Set the speech speed.
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }
}

/// Result of a text-to-speech synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    /// The synthesized audio.
    pub audio: AudioChunk,
    /// Duration of the output audio in seconds.
    pub duration_secs: f32,
    /// Number of characters processed.
    pub characters_used: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_chunk_silence() {
        let chunk = AudioChunk::silence(16000, 1, 480);
        assert_eq!(chunk.samples.len(), 480);
        assert_eq!(chunk.sample_rate, 16000);
        assert_eq!(chunk.channels, 1);
        assert!(!chunk.is_empty());
        assert!((chunk.rms_energy() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_audio_chunk_duration() {
        // 16000 samples at 16kHz mono = 1 second
        let chunk = AudioChunk::silence(16000, 1, 16000);
        assert!((chunk.duration_secs() - 1.0).abs() < 0.001);

        // 32000 samples at 16kHz stereo = 1 second (32000 / (16000 * 2))
        let stereo = AudioChunk::silence(16000, 2, 32000);
        assert!((stereo.duration_secs() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_audio_chunk_rms_energy() {
        // Silence has 0 energy
        let silence = AudioChunk::silence(16000, 1, 100);
        assert!((silence.rms_energy() - 0.0).abs() < f32::EPSILON);

        // Known signal: all 0.5 -> RMS = 0.5
        let chunk = AudioChunk::new(vec![0.5; 100], 16000, 1);
        assert!((chunk.rms_energy() - 0.5).abs() < 0.001);

        // Known signal: alternating 1.0 and -1.0 -> RMS = 1.0
        let alternating: Vec<f32> = (0..100)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let chunk = AudioChunk::new(alternating, 16000, 1);
        assert!((chunk.rms_energy() - 1.0).abs() < 0.001);

        // Empty chunk has 0 energy
        let empty = AudioChunk::new(vec![], 16000, 1);
        assert!((empty.rms_energy() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_audio_chunk_append() {
        let mut a = AudioChunk::new(vec![0.1, 0.2], 16000, 1);
        let b = AudioChunk::new(vec![0.3, 0.4], 16000, 1);
        a.append(&b);
        assert_eq!(a.samples, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn test_audio_chunk_split_at() {
        let chunk = AudioChunk::new(vec![1.0, 2.0, 3.0, 4.0], 16000, 1);
        let (left, right) = chunk.split_at(2);
        assert_eq!(left.samples, vec![1.0, 2.0]);
        assert_eq!(right.samples, vec![3.0, 4.0]);
        assert_eq!(left.sample_rate, 16000);
        assert_eq!(right.sample_rate, 16000);

        // Split at 0
        let (l, r) = chunk.split_at(0);
        assert!(l.samples.is_empty());
        assert_eq!(r.samples.len(), 4);

        // Split beyond length
        let (l, r) = chunk.split_at(100);
        assert_eq!(l.samples.len(), 4);
        assert!(r.samples.is_empty());
    }

    #[test]
    fn test_audio_format_serde() {
        let formats = vec![
            AudioFormat::Pcm16,
            AudioFormat::Float32,
            AudioFormat::Wav,
            AudioFormat::Mp3,
            AudioFormat::Opus,
        ];
        for fmt in &formats {
            let json = serde_json::to_string(fmt).unwrap();
            let deserialized: AudioFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(*fmt, deserialized);
        }
    }

    #[test]
    fn test_transcription_result_serde() {
        let result = TranscriptionResult {
            text: "hello world".to_string(),
            confidence: 0.95,
            language: Some("en".to_string()),
            segments: vec![TranscriptionSegment {
                text: "hello".to_string(),
                start_secs: 0.0,
                end_secs: 0.5,
                confidence: 0.97,
            }],
            duration_secs: 1.2,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TranscriptionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text, "hello world");
        assert!((deserialized.confidence - 0.95).abs() < f32::EPSILON);
        assert_eq!(deserialized.language, Some("en".to_string()));
        assert_eq!(deserialized.segments.len(), 1);
    }

    #[test]
    fn test_synthesis_request_creation() {
        let req = SynthesisRequest::new("Hello world")
            .with_voice("alloy")
            .with_speed(1.5);
        assert_eq!(req.text, "Hello world");
        assert_eq!(req.voice, Some("alloy".to_string()));
        assert!((req.speed - 1.5).abs() < f32::EPSILON);
        assert_eq!(req.format, AudioFormat::Wav);
    }
}
