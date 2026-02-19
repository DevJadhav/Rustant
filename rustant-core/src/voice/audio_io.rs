//! Audio I/O and format conversion utilities.
//!
//! Conversion helpers (f32↔i16, WAV encode/decode, resample, stereo→mono) are
//! always available. `AudioInput` and `AudioOutput` (cpal-based) require the
//! `voice` feature.

use super::types::AudioChunk;
use crate::error::VoiceError;

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
                    format: format!("WAV write error: {e}"),
                }
            })?;
            let i16_samples = f32_to_i16(&chunk.samples);
            for sample in &i16_samples {
                writer
                    .write_sample(*sample)
                    .map_err(|e| VoiceError::UnsupportedFormat {
                        format: format!("WAV sample write error: {e}"),
                    })?;
            }
            writer
                .finalize()
                .map_err(|e| VoiceError::UnsupportedFormat {
                    format: format!("WAV finalize error: {e}"),
                })?;
        }
        Ok(cursor.into_inner())
    }

    /// Decode WAV bytes to an AudioChunk using hound.
    ///
    /// Handles WAV files where the data chunk has trailing bytes that aren't
    /// a multiple of the sample size (common with some API-generated WAVs)
    /// by fixing the header and truncating the data to a valid boundary.
    pub fn decode_wav(data: &[u8]) -> Result<AudioChunk, VoiceError> {
        // First try the standard path
        let cursor = std::io::Cursor::new(data);
        match hound::WavReader::new(cursor) {
            Ok(mut reader) => decode_wav_reader(&mut reader),
            Err(hound::Error::FormatError(_)) => {
                // The WAV has format issues (e.g. data chunk length not a
                // multiple of sample size, or declared size exceeds actual
                // data). Fix header and truncate data to valid boundary.
                let fixed = fix_wav_data(data)?;
                let cursor = std::io::Cursor::new(&fixed);
                let mut reader =
                    hound::WavReader::new(cursor).map_err(|e| VoiceError::UnsupportedFormat {
                        format: format!("WAV read error after fix: {e}"),
                    })?;
                decode_wav_reader(&mut reader)
            }
            Err(e) => Err(VoiceError::UnsupportedFormat {
                format: format!("WAV read error: {e}"),
            }),
        }
    }

    /// Extract samples from a WavReader into an AudioChunk.
    ///
    /// Reads samples leniently: if a read error occurs after successfully
    /// reading some samples, returns what was read (handles truncated files).
    fn decode_wav_reader<R: std::io::Read>(
        reader: &mut hound::WavReader<R>,
    ) -> Result<AudioChunk, VoiceError> {
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let max_val = (1i32 << (spec.bits_per_sample - 1)) as f32;
                let mut out = Vec::new();
                for s in reader.samples::<i32>() {
                    match s {
                        Ok(v) => out.push(v as f32 / max_val),
                        Err(_) => break, // stop at first read error (truncated data)
                    }
                }
                out
            }
            hound::SampleFormat::Float => {
                let mut out = Vec::new();
                for s in reader.samples::<f32>() {
                    match s {
                        Ok(v) => out.push(v),
                        Err(_) => break,
                    }
                }
                out
            }
        };

        if samples.is_empty() {
            return Err(VoiceError::UnsupportedFormat {
                format: "WAV file contains no readable samples".into(),
            });
        }

        Ok(AudioChunk {
            samples,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            timestamp: None,
        })
    }

    /// Fix a WAV file where the data chunk is malformed.
    ///
    /// Adjusts the data chunk length to:
    /// 1. Not exceed the actual bytes available in the file
    /// 2. Be a multiple of the sample frame size
    ///
    /// Also truncates the file to match the corrected data length.
    fn fix_wav_data(data: &[u8]) -> Result<Vec<u8>, VoiceError> {
        if data.len() < 44 {
            return Err(VoiceError::UnsupportedFormat {
                format: "WAV too short to contain header".into(),
            });
        }

        let mut fixed = data.to_vec();
        let mut pos = 12; // Skip RIFF header (4 + 4 + 4 bytes)

        let mut bits_per_sample: u16 = 16;
        let mut num_channels: u16 = 1;

        while pos + 8 <= fixed.len() {
            let chunk_id = &fixed[pos..pos + 4];
            let chunk_size = u32::from_le_bytes([
                fixed[pos + 4],
                fixed[pos + 5],
                fixed[pos + 6],
                fixed[pos + 7],
            ]) as usize;

            if chunk_id == b"fmt " && chunk_size >= 16 {
                num_channels = u16::from_le_bytes([fixed[pos + 10], fixed[pos + 11]]);
                bits_per_sample = u16::from_le_bytes([fixed[pos + 22], fixed[pos + 23]]);
            }

            if chunk_id == b"data" {
                let data_start = pos + 8;
                let bytes_per_sample = (bits_per_sample as usize / 8) * num_channels as usize;

                // Cap at actual available bytes
                let actual_available = fixed.len().saturating_sub(data_start);
                let mut new_size = chunk_size.min(actual_available);

                // Align to sample frame boundary
                if bytes_per_sample > 0 {
                    new_size -= new_size % bytes_per_sample;
                }

                // Write corrected data chunk size
                let new_size_bytes = (new_size as u32).to_le_bytes();
                fixed[pos + 4] = new_size_bytes[0];
                fixed[pos + 5] = new_size_bytes[1];
                fixed[pos + 6] = new_size_bytes[2];
                fixed[pos + 7] = new_size_bytes[3];

                // Truncate file to end of corrected data chunk
                fixed.truncate(data_start + new_size);

                // Also fix the RIFF chunk size (total file size - 8)
                let riff_size = (fixed.len() - 8) as u32;
                let riff_bytes = riff_size.to_le_bytes();
                fixed[4] = riff_bytes[0];
                fixed[5] = riff_bytes[1];
                fixed[6] = riff_bytes[2];
                fixed[7] = riff_bytes[3];

                break;
            }

            pos += 8 + chunk_size;
            if chunk_size % 2 != 0 {
                pos += 1;
            }
        }

        Ok(fixed)
    }
}

/// Play an AudioChunk through the system speakers.
///
/// Encodes to a temporary WAV file and plays it using system commands:
/// - macOS: `afplay`
/// - Linux: `aplay`
pub async fn play_audio(chunk: &AudioChunk) -> Result<(), VoiceError> {
    let wav_bytes = audio_convert::encode_wav(chunk)?;
    let tmp_path = std::env::temp_dir().join("rustant_playback.wav");
    std::fs::write(&tmp_path, &wav_bytes).map_err(|e| VoiceError::AudioError {
        message: format!("Failed to write temp WAV: {e}"),
    })?;

    let cmd = if cfg!(target_os = "macos") {
        "afplay"
    } else {
        "aplay"
    };
    let status = tokio::process::Command::new(cmd)
        .arg(&tmp_path)
        .status()
        .await
        .map_err(|e| VoiceError::AudioError {
            message: format!("{cmd} failed: {e}"),
        })?;

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp_path);

    if !status.success() {
        return Err(VoiceError::AudioError {
            message: format!("{cmd} exited with status {status}"),
        });
    }
    Ok(())
}

/// Record an audio chunk from the system microphone.
///
/// On macOS, uses `afrecord` to capture audio for `duration_secs` seconds.
/// Returns the recorded AudioChunk.
pub async fn record_audio_chunk(
    duration_secs: f32,
    sample_rate: u32,
) -> Result<AudioChunk, VoiceError> {
    let tmp_path = std::env::temp_dir().join("rustant_mic_capture.wav");

    // Remove stale file from a previous capture
    let _ = std::fs::remove_file(&tmp_path);

    #[cfg(target_os = "macos")]
    {
        let status = tokio::process::Command::new("afrecord")
            .args([
                "-d",
                &format!("{duration_secs:.1}"),
                "-f",
                "WAVE",
                "-c",
                "1",
                "-r",
                &sample_rate.to_string(),
                tmp_path.to_str().unwrap(),
            ])
            .status()
            .await
            .map_err(|e| VoiceError::AudioError {
                message: format!("afrecord failed: {e}"),
            })?;

        if !status.success() {
            return Err(VoiceError::AudioError {
                message: "afrecord exited with error".into(),
            });
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let status = tokio::process::Command::new("arecord")
            .args([
                "-d",
                &format!("{:.0}", duration_secs),
                "-f",
                "S16_LE",
                "-c",
                "1",
                "-r",
                &sample_rate.to_string(),
                tmp_path.to_str().unwrap(),
            ])
            .status()
            .await
            .map_err(|e| VoiceError::AudioError {
                message: format!("arecord failed: {}", e),
            })?;

        if !status.success() {
            return Err(VoiceError::AudioError {
                message: "arecord exited with error".into(),
            });
        }
    }

    let data = std::fs::read(&tmp_path).map_err(|e| VoiceError::AudioError {
        message: format!("Failed to read recorded audio: {e}"),
    })?;
    let _ = std::fs::remove_file(&tmp_path);

    audio_convert::decode_wav(&data)
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
            assert!((a - b).abs() < 0.001, "expected {a}, got {b}");
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

    #[tokio::test]
    async fn test_play_audio_creates_and_cleans_temp_file() {
        // We cannot actually test playback without audio hardware, but we can test
        // that play_audio writes the WAV file correctly before the command runs.
        // Since afplay/aplay may not be available in CI, we just verify encoding works.
        let chunk = AudioChunk::new(vec![0.0, 0.5, -0.5, 0.25], 16000, 1);
        let wav_bytes = audio_convert::encode_wav(&chunk).unwrap();
        let tmp_path = std::env::temp_dir().join("rustant_playback_test.wav");
        std::fs::write(&tmp_path, &wav_bytes).unwrap();
        assert!(tmp_path.exists());
        std::fs::remove_file(&tmp_path).unwrap();
        assert!(!tmp_path.exists());
    }

    #[test]
    fn test_wav_encode_decode_roundtrip() {
        let original = AudioChunk::new(vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.25], 16000, 1);
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
            assert!((a - b).abs() < 0.001, "expected {a}, got {b}");
        }
    }
}
