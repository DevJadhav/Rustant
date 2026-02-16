//! Shared toggle state for voice commands and meeting recording.
//!
//! Provides a single `ToggleState` that all interfaces (REPL, TUI,
//! Gateway, Dashboard) share to start/stop voice and meeting sessions.

use super::meeting_session::{MeetingRecordingSession, MeetingResult, MeetingStatus};
use super::session::VoiceCommandSession;
use crate::config::{AgentConfig, MeetingConfig};
use crate::error::VoiceError;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Shared state container for voice command and meeting recording toggles.
///
/// Wrapped in `Arc` and passed to REPL, TUI, Gateway, and Dashboard
/// so all interfaces share a single source of truth.
pub struct ToggleState {
    voice_session: Mutex<Option<VoiceCommandSession>>,
    meeting_session: Mutex<Option<MeetingRecordingSession>>,
}

impl ToggleState {
    /// Create a new toggle state (no sessions active).
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            voice_session: Mutex::new(None),
            meeting_session: Mutex::new(None),
        })
    }

    // ── Voice Commands ──────────────────────────────────────────────

    /// Start the voice command session.
    pub async fn voice_start(
        &self,
        config: AgentConfig,
        workspace: PathBuf,
        on_transcription: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<(), VoiceError> {
        let mut guard = self.voice_session.lock().await;
        if guard.as_ref().is_some_and(|s| s.is_active()) {
            return Err(VoiceError::PipelineError {
                message: "Voice command session is already active".into(),
            });
        }

        let session = VoiceCommandSession::start(config, workspace, on_transcription).await?;
        *guard = Some(session);
        info!("Voice command session toggled ON");
        Ok(())
    }

    /// Stop the voice command session.
    pub async fn voice_stop(&self) -> Result<(), VoiceError> {
        let mut guard = self.voice_session.lock().await;
        match guard.take() {
            Some(session) => {
                session.stop().await?;
                info!("Voice command session toggled OFF");
                Ok(())
            }
            None => Err(VoiceError::PipelineError {
                message: "No active voice command session".into(),
            }),
        }
    }

    /// Check if the voice command session is active.
    pub async fn voice_active(&self) -> bool {
        let guard = self.voice_session.lock().await;
        guard.as_ref().is_some_and(|s| s.is_active())
    }

    // ── Meeting Recording ───────────────────────────────────────────

    /// Start a meeting recording session.
    pub async fn meeting_start(
        &self,
        config: MeetingConfig,
        title: Option<String>,
    ) -> Result<(), String> {
        let mut guard = self.meeting_session.lock().await;
        if guard.as_ref().is_some_and(|s| s.is_active()) {
            return Err("Meeting recording is already active".into());
        }

        let session = MeetingRecordingSession::start(config, title).await?;
        *guard = Some(session);
        info!("Meeting recording toggled ON");
        Ok(())
    }

    /// Stop the meeting recording and return results.
    pub async fn meeting_stop(&self) -> Result<MeetingResult, String> {
        let mut guard = self.meeting_session.lock().await;
        match guard.take() {
            Some(session) => {
                let result = session.stop().await?;
                info!("Meeting recording toggled OFF");
                Ok(result)
            }
            None => Err("No active meeting recording".into()),
        }
    }

    /// Check if a meeting recording is active.
    pub async fn meeting_active(&self) -> bool {
        let guard = self.meeting_session.lock().await;
        guard.as_ref().is_some_and(|s| s.is_active())
    }

    /// Get the current meeting recording status.
    pub async fn meeting_status(&self) -> Option<MeetingStatus> {
        let guard = self.meeting_session.lock().await;
        match guard.as_ref() {
            Some(session) if session.is_active() => Some(session.status().await),
            _ => None,
        }
    }

    // ── Synchronous helpers (for TUI key handlers) ──────────────────

    /// Non-blocking check if voice session is active.
    /// Returns `None` if the mutex is contended.
    pub fn voice_session_active_sync(&self) -> Option<bool> {
        self.voice_session
            .try_lock()
            .ok()
            .map(|guard| guard.as_ref().is_some_and(|s| s.is_active()))
    }

    /// Non-blocking check if meeting session is active.
    /// Returns `None` if the mutex is contended.
    pub fn meeting_session_active_sync(&self) -> Option<bool> {
        self.meeting_session
            .try_lock()
            .ok()
            .map(|guard| guard.as_ref().is_some_and(|s| s.is_active()))
    }
}

impl Default for ToggleState {
    fn default() -> Self {
        Self {
            voice_session: Mutex::new(None),
            meeting_session: Mutex::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_toggle_state_initial() {
        let state = ToggleState::new();
        assert!(!state.voice_active().await);
        assert!(!state.meeting_active().await);
        assert!(state.meeting_status().await.is_none());
    }

    #[tokio::test]
    async fn test_voice_stop_without_start() {
        let state = ToggleState::new();
        let result = state.voice_stop().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_meeting_stop_without_start() {
        let state = ToggleState::new();
        let result = state.meeting_stop().await;
        assert!(result.is_err());
    }
}
