//! Meeting recording session manager with graceful shutdown.
//!
//! Wraps meeting recording start/stop/status into a session manager
//! using `tokio::sync::watch` for cancellation — matching the existing
//! `SILENCE_MONITOR_STOP` pattern in `meeting.rs`.
//!
//! Uses core's own voice APIs and direct `afrecord` process management
//! to avoid circular dependency with `rustant-tools`.

use crate::config::MeetingConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
#[allow(unused_imports)]
use tracing::{debug, info, warn};

/// Result returned when a meeting recording is stopped.
#[derive(Debug, Clone)]
pub struct MeetingResult {
    /// Full transcript text (empty if transcription unavailable).
    pub transcript: String,
    /// Summary text (empty if summarization unavailable).
    pub summary: String,
    /// Whether the transcript was saved to Notes.app.
    pub notes_saved: bool,
    /// Duration of the recording in seconds.
    pub duration_secs: u64,
    /// Path to the recorded audio file.
    pub audio_path: String,
}

/// Snapshot of the current recording state.
#[derive(Debug, Clone)]
pub struct MeetingStatus {
    /// Whether recording is currently active.
    pub is_recording: bool,
    /// Recording start time (ISO 8601).
    pub started_at: Option<String>,
    /// Meeting title.
    pub title: Option<String>,
    /// Path to the audio file being recorded.
    pub audio_path: Option<String>,
    /// How long the recording has been running (seconds).
    pub elapsed_secs: u64,
}

/// A meeting recording session with background silence monitoring
/// and graceful shutdown.
pub struct MeetingRecordingSession {
    #[allow(dead_code)]
    cancel_tx: watch::Sender<bool>,
    #[allow(dead_code)]
    monitor_handle: Option<JoinHandle<()>>,
    active: Arc<AtomicBool>,
    state: Arc<Mutex<SessionState>>,
}

#[derive(Debug, Clone)]
struct SessionState {
    #[allow(dead_code)]
    pid: u32,
    audio_path: String,
    title: String,
    started_at: chrono::DateTime<chrono::Utc>,
    #[allow(dead_code)]
    config: MeetingConfig,
}

impl MeetingRecordingSession {
    /// Start a new meeting recording session.
    ///
    /// Launches `afrecord` in the background and optionally starts
    /// a silence monitor task.
    #[cfg(target_os = "macos")]
    pub async fn start(config: MeetingConfig, title: Option<String>) -> Result<Self, String> {
        let title = title
            .unwrap_or_else(|| format!("Meeting {}", chrono::Utc::now().format("%Y-%m-%d %H:%M")));

        let sample_rate = config.sample_rate;
        let audio_dir = std::path::PathBuf::from(".rustant");
        std::fs::create_dir_all(&audio_dir)
            .map_err(|e| format!("Failed to create .rustant dir: {e}"))?;

        let audio_path = audio_dir
            .join(format!(
                "meeting-{}.wav",
                chrono::Utc::now().format("%Y%m%d-%H%M%S")
            ))
            .to_string_lossy()
            .to_string();

        // Start afrecord directly (same as meeting.rs:start_recording).
        let child = tokio::process::Command::new("afrecord")
            .args([
                "-f", "WAVE", "-d", "LEI16", "-c", "1", "-r",
                &sample_rate.to_string(),
                &audio_path,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start afrecord: {e}. Make sure Microphone access is granted in System Settings > Privacy & Security > Microphone."))?;

        let pid = child
            .id()
            .ok_or_else(|| "Failed to get afrecord PID".to_string())?;

        info!(pid = pid, path = %audio_path, "Meeting recording started");

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let active = Arc::new(AtomicBool::new(true));

        let state = Arc::new(Mutex::new(SessionState {
            pid,
            audio_path: audio_path.clone(),
            title: title.clone(),
            started_at: chrono::Utc::now(),
            config: config.clone(),
        }));

        // Optionally start silence monitor.
        let monitor_handle = if config.silence_timeout_secs > 0 {
            let active_clone = active.clone();
            let silence_timeout = config.silence_timeout_secs;

            let handle = tokio::spawn(async move {
                silence_monitor_loop(pid, silence_timeout, cancel_rx, active_clone).await;
            });
            Some(handle)
        } else {
            None
        };

        Ok(Self {
            cancel_tx,
            monitor_handle,
            active,
            state,
        })
    }

    /// Start a new meeting recording session (non-macOS stub).
    #[cfg(not(target_os = "macos"))]
    pub async fn start(_config: MeetingConfig, _title: Option<String>) -> Result<Self, String> {
        Err("Meeting recording is only available on macOS".into())
    }

    /// Gracefully stop the recording, transcribe, and return results.
    #[cfg(target_os = "macos")]
    pub async fn stop(self) -> Result<MeetingResult, String> {
        info!("Stopping meeting recording session");

        // Signal cancellation to silence monitor.
        let _ = self.cancel_tx.send(true);
        self.active.store(false, Ordering::SeqCst);

        // Wait for silence monitor to finish.
        if let Some(handle) = self.monitor_handle {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
        }

        let state = self.state.lock().await;

        // Stop the afrecord process via SIGINT.
        let _ = tokio::process::Command::new("kill")
            .args(["-SIGINT", &state.pid.to_string()])
            .output()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let elapsed = chrono::Utc::now()
            .signed_duration_since(state.started_at)
            .num_seconds() as u64;

        // Transcribe using core's own OpenAiSttProvider.
        let transcript = match std::env::var("OPENAI_API_KEY") {
            Ok(api_key) if std::path::Path::new(&state.audio_path).exists() => {
                match transcribe_file(&state.audio_path, &api_key).await {
                    Ok(text) => text,
                    Err(e) => {
                        warn!(error = %e, "Transcription failed");
                        String::new()
                    }
                }
            }
            _ => {
                warn!("OPENAI_API_KEY not set or audio file missing — skipping transcription");
                String::new()
            }
        };

        Ok(MeetingResult {
            transcript,
            summary: String::new(),
            notes_saved: false,
            duration_secs: elapsed,
            audio_path: state.audio_path.clone(),
        })
    }

    /// Stop recording (non-macOS stub).
    #[cfg(not(target_os = "macos"))]
    pub async fn stop(self) -> Result<MeetingResult, String> {
        Err("Meeting recording is only available on macOS".into())
    }

    /// Check if the session is currently active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Get the current recording status.
    pub async fn status(&self) -> MeetingStatus {
        let state = self.state.lock().await;
        let elapsed = chrono::Utc::now()
            .signed_duration_since(state.started_at)
            .num_seconds() as u64;

        MeetingStatus {
            is_recording: self.active.load(Ordering::SeqCst),
            started_at: Some(state.started_at.to_rfc3339()),
            title: Some(state.title.clone()),
            audio_path: Some(state.audio_path.clone()),
            elapsed_secs: elapsed,
        }
    }
}

/// Transcribe a WAV file using core's own `OpenAiSttProvider`.
#[cfg(target_os = "macos")]
async fn transcribe_file(audio_path: &str, api_key: &str) -> Result<String, String> {
    use crate::voice::audio_io::audio_convert;
    use crate::voice::stt::{OpenAiSttProvider, SttProvider};
    use crate::voice::types::AudioChunk;

    let wav_data =
        std::fs::read(audio_path).map_err(|e| format!("Failed to read audio file: {e}"))?;

    let chunk =
        audio_convert::decode_wav(&wav_data).map_err(|e| format!("Failed to decode WAV: {e}"))?;

    let provider = OpenAiSttProvider::new(api_key);

    // Chunk limit: 10 minutes at 16kHz mono.
    const CHUNK_SAMPLES: usize = 16000 * 600;

    if chunk.samples.len() <= CHUNK_SAMPLES {
        let result = provider
            .transcribe(&chunk)
            .await
            .map_err(|e| format!("Transcription failed: {e}"))?;
        return Ok(result.text);
    }

    // Multi-chunk transcription for long recordings.
    let mut full_transcript = String::new();
    let mut offset = 0;

    while offset < chunk.samples.len() {
        let end = (offset + CHUNK_SAMPLES).min(chunk.samples.len());
        let sub_chunk = AudioChunk::new(
            chunk.samples[offset..end].to_vec(),
            chunk.sample_rate,
            chunk.channels,
        );

        let result = provider
            .transcribe(&sub_chunk)
            .await
            .map_err(|e| format!("Transcription failed: {e}"))?;

        if !full_transcript.is_empty() {
            full_transcript.push(' ');
        }
        full_transcript.push_str(&result.text);
        offset = end;
    }

    Ok(full_transcript)
}

/// Background silence monitor that auto-stops recording after sustained silence.
#[cfg(target_os = "macos")]
async fn silence_monitor_loop(
    pid: u32,
    silence_timeout_secs: u64,
    mut cancel_rx: watch::Receiver<bool>,
    active: Arc<AtomicBool>,
) {
    let sample_duration_secs: f32 = 2.0;
    let check_interval = std::time::Duration::from_secs(5);
    let mut silence_duration_secs: u64 = 0;
    let mut vad = crate::voice::vad::VoiceActivityDetector::new(0.005);

    info!(
        pid = pid,
        timeout = silence_timeout_secs,
        "Meeting session silence monitor started"
    );

    loop {
        if *cancel_rx.borrow() {
            debug!("Meeting silence monitor cancelled");
            return;
        }

        tokio::select! {
            _ = tokio::time::sleep(check_interval) => {}
            _ = cancel_rx.changed() => {
                debug!("Meeting silence monitor cancelled during sleep");
                return;
            }
        }

        match crate::voice::audio_io::record_audio_chunk(sample_duration_secs, 16000).await {
            Ok(chunk) => {
                let event = vad.process_chunk(&chunk);
                match event {
                    crate::voice::vad::VadEvent::SpeechStart => {
                        silence_duration_secs = 0;
                    }
                    crate::voice::vad::VadEvent::SpeechEnd => {
                        silence_duration_secs += check_interval.as_secs();
                    }
                    crate::voice::vad::VadEvent::NoChange => {
                        if !vad.is_speaking() {
                            silence_duration_secs += check_interval.as_secs();
                        } else {
                            silence_duration_secs = 0;
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Meeting silence monitor: failed to record sample");
                continue;
            }
        }

        if silence_duration_secs >= silence_timeout_secs {
            info!(
                silence_secs = silence_duration_secs,
                "Silence timeout reached, auto-stopping meeting recording"
            );

            // Stop afrecord via SIGINT.
            let _ = tokio::process::Command::new("kill")
                .args(["-SIGINT", &pid.to_string()])
                .output()
                .await;

            active.store(false, Ordering::SeqCst);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meeting_result_fields() {
        let result = MeetingResult {
            transcript: "Hello".into(),
            summary: String::new(),
            notes_saved: false,
            duration_secs: 60,
            audio_path: "/tmp/test.wav".into(),
        };
        assert_eq!(result.duration_secs, 60);
        assert!(!result.notes_saved);
        assert_eq!(result.audio_path, "/tmp/test.wav");
    }

    #[test]
    fn test_meeting_status_fields() {
        let status = MeetingStatus {
            is_recording: false,
            started_at: None,
            title: None,
            audio_path: None,
            elapsed_secs: 0,
        };
        assert!(!status.is_recording);
        assert_eq!(status.elapsed_secs, 0);
    }
}
