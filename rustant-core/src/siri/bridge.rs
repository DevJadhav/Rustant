//! Siri ↔ Daemon bridge.
//!
//! Core connector that manages activation state and routes Siri commands
//! to the daemon via IPC.

use crate::daemon::ipc::IpcMessage;
use std::path::PathBuf;

/// The Siri bridge manages activation state and command routing.
pub struct SiriBridge {
    base_dir: PathBuf,
}

impl SiriBridge {
    /// Create a new Siri bridge.
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Activate Siri mode — starts daemon if needed, sets active flag.
    pub fn activate(&self) -> Result<String, SiriError> {
        // Set the active flag
        let flag_path = self.base_dir.join("siri_active");
        crate::persistence::atomic_write(&flag_path, b"1")
            .map_err(|e| SiriError::IoError(e.to_string()))?;

        Ok("Rustant is now active. What can I help you with?".to_string())
    }

    /// Deactivate Siri mode — clears active flag, optionally stops daemon.
    pub fn deactivate(&self, stop_daemon: bool) -> Result<String, SiriError> {
        let flag_path = self.base_dir.join("siri_active");
        if flag_path.exists() {
            std::fs::remove_file(&flag_path).map_err(|e| SiriError::IoError(e.to_string()))?;
        }

        if stop_daemon {
            // Send shutdown message to daemon
            let _ = self.send_to_daemon(IpcMessage::Shutdown);
        }

        Ok("Rustant deactivated.".to_string())
    }

    /// Check if Siri mode is currently active.
    ///
    /// Fast filesystem check — no IPC needed.
    pub fn is_active(&self) -> bool {
        let flag_path = self.base_dir.join("siri_active");
        flag_path.exists()
    }

    /// Send a command to the daemon and get a response.
    pub fn send_to_daemon(&self, _msg: IpcMessage) -> Result<IpcMessage, SiriError> {
        // In a real implementation, this would connect to the Unix socket
        // and send/receive JSON messages. For now, return a stub response.
        Ok(IpcMessage::CommandResult {
            success: true,
            response: "Command received".into(),
            audio_text: Some("Command received".into()),
            needs_confirmation: false,
            confirmation_prompt: None,
            session_id: None,
        })
    }

    /// Send a voice command to the daemon for execution.
    pub fn send_command(&self, command: &str) -> Result<String, SiriError> {
        if !self.is_active() {
            return Err(SiriError::NotActive);
        }

        let msg = IpcMessage::ExecuteCommand {
            command: command.to_string(),
            source: "siri".to_string(),
            timeout_secs: 30,
        };

        match self.send_to_daemon(msg)? {
            IpcMessage::CommandResult {
                response,
                audio_text,
                needs_confirmation,
                confirmation_prompt,
                session_id,
                ..
            } => {
                if needs_confirmation {
                    // Return the confirmation prompt for Siri to speak
                    let prompt =
                        confirmation_prompt.unwrap_or_else(|| "Should I proceed?".to_string());
                    let sid = session_id.unwrap_or_default();
                    Ok(format!("CONFIRM:{sid}:{prompt}"))
                } else {
                    Ok(audio_text.unwrap_or(response))
                }
            }
            _ => Err(SiriError::UnexpectedResponse),
        }
    }

    /// Confirm or deny a pending action.
    pub fn handle_approval(&self, session_id: &str, confirmed: bool) -> Result<String, SiriError> {
        if !self.is_active() {
            return Err(SiriError::NotActive);
        }

        let msg = IpcMessage::ConfirmAction {
            session_id: session_id.to_string(),
            confirmed,
        };

        match self.send_to_daemon(msg)? {
            IpcMessage::CommandResult { response, .. } => Ok(response),
            _ => Err(SiriError::UnexpectedResponse),
        }
    }

    /// Check if the daemon is running.
    pub fn ensure_daemon_running(&self) -> bool {
        crate::daemon::lifecycle::check_daemon_running(&self.base_dir).is_some()
    }
}

/// Siri-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum SiriError {
    #[error("Rustant is not activated. Say 'Hey Siri, activate Rustant' first.")]
    NotActive,
    #[error("Daemon is not running")]
    DaemonNotRunning,
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Unexpected response from daemon")]
    UnexpectedResponse,
    #[error("Command timed out")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activate_deactivate() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = SiriBridge::new(tmp.path().to_path_buf());

        assert!(!bridge.is_active());

        bridge.activate().unwrap();
        assert!(bridge.is_active());

        bridge.deactivate(false).unwrap();
        assert!(!bridge.is_active());
    }

    #[test]
    fn test_send_command_when_inactive() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = SiriBridge::new(tmp.path().to_path_buf());

        let result = bridge.send_command("check calendar");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SiriError::NotActive));
    }

    #[test]
    fn test_send_command_when_active() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = SiriBridge::new(tmp.path().to_path_buf());

        bridge.activate().unwrap();
        let result = bridge.send_command("check calendar");
        assert!(result.is_ok());
    }
}
