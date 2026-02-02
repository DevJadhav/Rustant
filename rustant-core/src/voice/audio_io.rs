//! Audio I/O and format conversion utilities.
//!
//! Conversion helpers (f32↔i16, WAV encode/decode, resample, stereo→mono) are
//! always available. `AudioInput` and `AudioOutput` (cpal-based) require the
//! `voice` feature.

use crate::error::VoiceError;
use super::types::AudioChunk;

/// Audio conversion utilities — always available (no feature gate).
pub mod audio_convert {
    use super::*;

    /// Convert f32 samples (-1.0..1.0) to i16 samples.
    pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
        samples
            .iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * i16::MAX as f32) as i16
            })
            .collect()
    }

    /// Convert i16 samples to f32 samples (-1.0..1.0).
    pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
        samples
            .iter()
            .map(|&s| s as f32 / i16::MAX as f32)
            .collect()
    }

    /// Resample audio using linear interpolation.
    pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate || samples.is_empty() {
            return samples.to_vec();
        }
        let ratio = from_rate as f64 / to_rate as f64;
        let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let src_pos = i as f64 * ratio;
            let idx = src_pos as usize;
            let frac = (src_pos - idx as f64) as f32;
            if idx + 1 < samples.len() {
                out.push(samples[idx] * (1.0 - frac) + samples[idx + 1] * frac);
            } else if idx < samples.len() {
                out.push(samples[idx]);
            }
        }
        out
    }

    /// Convert stereo interleaved samples to mono by averaging channels.
    pub fn stereo_to_mono(samples: &[f32]) -> Vec<f32> {
        samples
            .chunks(2)
            .map(|pair| {
                if pair.len() == 2 {
                    (pair[0] + pair[1]) / 2.0
                } else {
                    pair[0]
                }
            })
            .collect()
    }

    /// Encode an AudioChunk to WAV bytes using hound.
    pub fn encode_wav(chunk: &AudioChunk) -> Result<Vec<u8>, VoiceError> {
        let spec = hound::WavSpec {
            channels: chunk.channels,
            sample_rate: chunk.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
                VoiceError::UnsupportedFormat {
                    format: format!("WAV write error: {}", e),
                }
            })?;
            let i16_samples = f32_to_i16(&chunk.samples);
            for sample in &i16_samples {
                writer.write_sample(*sample).map_err(|e| {
                    VoiceError::UnsupportedFormat {
                        format: format!("WAV sample write error: {}", e),
                    }
                })?;
            }
            writer.finalize().map_err(|e| {
                VoiceError::UnsupportedFormat {
                    format: format!("WAV finalize error: {}", e),
                }
            })?;
        }
        Ok(cursor.into_inner())
    }

    /// Decode WAV bytes to an AudioChunk using hound.
    pub fn decode_wav(data: &[u8]) -> Result<AudioChunk, VoiceError> {
        let cursor = std::io::Cursor::new(data);
        let mut reader = hound::WavReader::new(cursor).map_err(|e| {
            VoiceError::UnsupportedFormat {
                format: format!("WAV read error: {}", e),
            }
        })?;
        let spec = reader.spec();

        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let max_val = (1i32 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / max_val))
                    .collect::<std::result::Result<Vec<f32>, _>>()
                    .map_err(|e| VoiceError::UnsupportedFormat {
                        format: format!("WAV sample read error: {}", e),
                    })?
            }
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .collect::<std::result::Result<Vec<f32>, _>>()
                .map_err(|e| VoiceError::UnsupportedFormat {
                    format: format!("WAV float sample read error: {}", e),
                })?,
        };

        Ok(AudioChunk {
            samples,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            timestamp: None,
        })
    }
}

// Feature-gated AudioInput / AudioOutput stubs.
// Real implementation would use cpal; we provide struct definitions
// behind the feature flag so they can be referenced.

/// Configuration for audio input capture.
#[cfg(feature = "voice")]
pub struct AudioInput {
    /// Optional device name (None = system default).
    pub device_name: Option<String>,
    /// Sample rate to request.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u16,
}

#[cfg(feature = "voice")]
impl AudioInput {
    /// Create a new audio input configuration.
    pub fn new(device_name: Option<String>) -> Self {
        Self {
            device_name,
            sample_rate: 16000,
            channels: 1,
        }
    }
}

/// Configuration for audio output playback.
#[cfg(feature = "voice")]
pub struct AudioOutput {
    /// Optional device name (None = system default).
    pub device_name: Option<String>,
}

#[cfg(feature = "voice")]
impl AudioOutput {
    /// Create a new audio output configuration.
    pub fn new(device_name: Option<String>) -> Self {
        Self { device_name }
    }
}

#[cfg(test)]
mod tests {
    use super::audio_convert::*;
    use super::*;

    #[test]
    fn test_f32_to_i16_conversion() {
        let samples = vec![0.0, 1.0, -1.0, 0.5, -0.5];
        let i16s = f32_to_i16(&samples);
        assert_eq!(i16s[0], 0);
        assert_eq!(i16s[1], i16::MAX);
        assert_eq!(i16s[2], -i16::MAX); // -1.0 * MAX = -MAX (not MIN due to asymmetry)
        assert!(i16s[3] > 0);
        assert!(i16s[4] < 0);
    }

    #[test]
    fn test_i16_to_f32_conversion() {
        let samples = vec![0i16, i16::MAX, i16::MIN, 16383, -16383];
        let f32s = i16_to_f32(&samples);
        assert!((f32s[0] - 0.0).abs() < 0.001);
        assert!((f32s[1] - 1.0).abs() < 0.001);
        assert!((f32s[2] - (-1.0)).abs() < 0.01); // MIN / MAX is approximately -1.0
        assert!(f32s[3] > 0.0);
        assert!(f32s[4] < 0.0);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = vec![0.0f32, 0.5, -0.5, 0.25, -0.25];
        let i16s = f32_to_i16(&original);
        let restored = i16_to_f32(&i16s);
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 0.001, "expected {}, got {}", a, b);
        }
    }

    #[test]
    fn test_stereo_to_mono() {
        let stereo = vec![0.4, 0.6, 0.2, 0.8, -0.5, 0.5];
        let mono = stereo_to_mono(&stereo);
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - 0.5).abs() < 0.001);
        assert!((mono[1] - 0.5).abs() < 0.001);
        assert!((mono[2] - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_resample_upsample() {
        // 4 samples at 8kHz → should produce ~8 samples at 16kHz
        let samples = vec![0.0, 0.5, 1.0, 0.5];
        let resampled = resample(&samples, 8000, 16000);
        assert!(resampled.len() >= 7); // roughly double
        // First sample should be the same
        assert!((resampled[0] - 0.0).abs() < 0.01);
        // Values should be interpolated
        assert!(resampled[1] > 0.0 && resampled[1] < 0.5);
    }

    #[test]
    fn test_wav_encode_decode_roundtrip() {
        let original = AudioChunk::new(
            vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.25],
            16000,
            1,
        );
        let wav_bytes = encode_wav(&original).unwrap();
        assert!(!wav_bytes.is_empty());
        // WAV files start with RIFF header
        assert_eq!(&wav_bytes[0..4], b"RIFF");

        let decoded = decode_wav(&wav_bytes).unwrap();
        assert_eq!(decoded.sample_rate, 16000);
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.samples.len(), original.samples.len());
        // Check samples are close (i16 quantization introduces small errors)
        for (a, b) in original.samples.iter().zip(decoded.samples.iter()) {
            assert!((a - b).abs() < 0.001, "expected {}, got {}", a, b);
        }
    }
}
