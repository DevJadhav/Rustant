//! Voice command session — background listen→transcribe→respond loop.
//!
//! Runs as a `tokio::spawn` task with graceful shutdown via `watch` channel.

use crate::config::AgentConfig;
use crate::error::VoiceError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// A non-blocking voice command session that listens for speech,
/// transcribes it, and delivers the text via a callback.
pub struct VoiceCommandSession {
    cancel_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
    active: Arc<AtomicBool>,
}

impl VoiceCommandSession {
    /// Start a new voice command session in the background.
    ///
    /// The session records audio in chunks, detects speech via VAD,
    /// transcribes via OpenAI Whisper, and sends the transcription text
    /// through `on_transcription`. The response text can be spoken back
    /// via `on_response`.
    pub async fn start(
        config: AgentConfig,
        _workspace: PathBuf,
        on_transcription: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Self, VoiceError> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| VoiceError::AuthFailed {
            provider: "openai".into(),
        })?;

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let active = Arc::new(AtomicBool::new(true));
        let active_clone = active.clone();

        let voice_config = config.voice.clone().unwrap_or_default();
        let sample_rate: u32 = 16000;
        let chunk_duration: f32 = 3.0;

        let handle = tokio::spawn(async move {
            voice_loop(
                cancel_rx,
                active_clone,
                api_key,
                voice_config,
                sample_rate,
                chunk_duration,
                on_transcription,
            )
            .await;
        });

        info!("Voice command session started");
        Ok(Self {
            cancel_tx,
            handle,
            active,
        })
    }

    /// Gracefully stop the voice command session.
    pub async fn stop(self) -> Result<(), VoiceError> {
        info!("Stopping voice command session");
        let _ = self.cancel_tx.send(true);
        self.active.store(false, Ordering::SeqCst);
        // Wait for the background task to finish (with timeout).
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), self.handle).await;
        info!("Voice command session stopped");
        Ok(())
    }

    /// Check if the session is currently active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}

/// The main voice command loop.
async fn voice_loop(
    mut cancel_rx: watch::Receiver<bool>,
    active: Arc<AtomicBool>,
    api_key: String,
    voice_config: crate::config::VoiceConfig,
    sample_rate: u32,
    chunk_duration: f32,
    on_transcription: Arc<dyn Fn(String) + Send + Sync>,
) {
    use crate::voice::audio_io::record_audio_chunk;
    use crate::voice::stt::{OpenAiSttProvider, SttProvider};
    use crate::voice::vad::{VadEvent, VoiceActivityDetector};

    let stt = OpenAiSttProvider::new(&api_key);
    let vad_threshold = voice_config.vad_threshold;
    let mut vad = VoiceActivityDetector::new(vad_threshold);
    let mut speech_buffer: Vec<f32> = Vec::new();
    let mut is_collecting = false;
    let max_collect_secs = voice_config.max_listen_secs as f32;
    let max_collect_samples = (max_collect_secs * sample_rate as f32) as usize;

    info!(
        threshold = vad_threshold,
        max_listen = voice_config.max_listen_secs,
        "Voice loop started"
    );

    loop {
        // Check for cancellation.
        if *cancel_rx.borrow() {
            debug!("Voice loop cancelled");
            break;
        }

        // Record a chunk.
        let chunk = tokio::select! {
            result = record_audio_chunk(chunk_duration, sample_rate) => {
                match result {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(error = %e, "Voice loop: failed to record chunk");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                }
            }
            _ = cancel_rx.changed() => {
                debug!("Voice loop cancelled during recording");
                break;
            }
        };

        // Process through VAD.
        let event = vad.process_chunk(&chunk);

        match event {
            VadEvent::SpeechStart => {
                debug!("Speech detected — collecting audio");
                is_collecting = true;
                speech_buffer.clear();
                speech_buffer.extend_from_slice(&chunk.samples);
            }
            VadEvent::SpeechEnd if is_collecting => {
                // Add final chunk and transcribe.
                speech_buffer.extend_from_slice(&chunk.samples);
                is_collecting = false;

                let audio = crate::voice::types::AudioChunk::new(
                    std::mem::take(&mut speech_buffer),
                    sample_rate,
                    1,
                );

                debug!(
                    duration = audio.duration_secs(),
                    "Speech ended — transcribing"
                );

                match stt.transcribe(&audio).await {
                    Ok(result) if !result.text.trim().is_empty() => {
                        info!(text = %result.text, "Transcription received");
                        on_transcription(result.text);
                    }
                    Ok(_) => debug!("Empty transcription — ignoring"),
                    Err(e) => warn!(error = %e, "Transcription failed"),
                }
            }
            VadEvent::NoChange if is_collecting => {
                speech_buffer.extend_from_slice(&chunk.samples);
                // Enforce max collection length.
                if speech_buffer.len() >= max_collect_samples {
                    is_collecting = false;
                    let audio = crate::voice::types::AudioChunk::new(
                        std::mem::take(&mut speech_buffer),
                        sample_rate,
                        1,
                    );
                    debug!(
                        duration = audio.duration_secs(),
                        "Max listen reached — transcribing"
                    );
                    match stt.transcribe(&audio).await {
                        Ok(result) if !result.text.trim().is_empty() => {
                            info!(text = %result.text, "Transcription received (max length)");
                            on_transcription(result.text);
                        }
                        Ok(_) => debug!("Empty transcription — ignoring"),
                        Err(e) => warn!(error = %e, "Transcription failed"),
                    }
                }
            }
            _ => {
                // Silence or speech continuing without change — do nothing special.
            }
        }
    }

    active.store(false, Ordering::SeqCst);
    info!("Voice loop exited");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_active_flag() {
        let active = Arc::new(AtomicBool::new(true));
        assert!(active.load(Ordering::SeqCst));
        active.store(false, Ordering::SeqCst);
        assert!(!active.load(Ordering::SeqCst));
    }
}
