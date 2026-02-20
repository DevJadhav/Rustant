//! Daemon process management.

use super::ipc::IpcMessage;
use super::lifecycle::DaemonState;
use crate::config::DaemonConfig;
use std::path::PathBuf;

/// The main Rustant background daemon.
pub struct RustantDaemon {
    /// Configuration.
    _config: DaemonConfig,
    /// Current daemon state.
    state: DaemonState,
    /// PID file path.
    pid_file: PathBuf,
    /// Base directory for state files.
    base_dir: PathBuf,
    /// Whether Siri mode is active.
    siri_active: bool,
}

impl RustantDaemon {
    /// Create a new daemon instance.
    pub fn new(config: DaemonConfig, base_dir: PathBuf) -> Self {
        let pid_file = config
            .pid_file_path
            .clone()
            .unwrap_or_else(|| base_dir.join("daemon.pid"));

        Self {
            _config: config,
            state: DaemonState::Starting,
            pid_file,
            base_dir,
            siri_active: false,
        }
    }

    /// Get current daemon state.
    pub fn state(&self) -> &DaemonState {
        &self.state
    }

    /// Check if Siri mode is active.
    pub fn is_siri_active(&self) -> bool {
        self.siri_active
    }

    /// Start the daemon.
    ///
    /// 1. Writes PID file
    /// 2. Starts IPC listener
    /// 3. Optionally starts gateway server
    /// 4. Warms MoE cache if configured
    pub async fn start(&mut self) -> Result<(), DaemonError> {
        // Check for existing daemon
        if self.is_running() {
            return Err(DaemonError::AlreadyRunning);
        }

        // Write PID file
        let pid = std::process::id();
        crate::persistence::atomic_write(&self.pid_file, pid.to_string().as_bytes())
            .map_err(|e| DaemonError::IoError(e.to_string()))?;

        self.state = DaemonState::Running;
        tracing::info!("Daemon started (PID {})", pid);

        Ok(())
    }

    /// Stop the daemon gracefully.
    pub async fn stop(&mut self) -> Result<(), DaemonError> {
        self.state = DaemonState::ShuttingDown;

        // Remove PID file
        if self.pid_file.exists() {
            let _ = std::fs::remove_file(&self.pid_file);
        }

        // Clear siri active flag
        let siri_flag = self.base_dir.join("siri_active");
        if siri_flag.exists() {
            let _ = std::fs::remove_file(&siri_flag);
        }

        self.state = DaemonState::Stopped;
        tracing::info!("Daemon stopped");

        Ok(())
    }

    /// Enable Siri mode.
    pub fn activate_siri(&mut self) -> Result<(), DaemonError> {
        let flag_path = self.base_dir.join("siri_active");
        crate::persistence::atomic_write(&flag_path, b"1")
            .map_err(|e| DaemonError::IoError(e.to_string()))?;
        self.siri_active = true;
        tracing::info!("Siri mode activated");
        Ok(())
    }

    /// Disable Siri mode.
    pub fn deactivate_siri(&mut self) -> Result<(), DaemonError> {
        let flag_path = self.base_dir.join("siri_active");
        if flag_path.exists() {
            std::fs::remove_file(&flag_path).map_err(|e| DaemonError::IoError(e.to_string()))?;
        }
        self.siri_active = false;
        tracing::info!("Siri mode deactivated");
        Ok(())
    }

    /// Check if another daemon instance is already running.
    pub fn is_running(&self) -> bool {
        if !self.pid_file.exists() {
            return false;
        }
        if let Ok(pid_str) = std::fs::read_to_string(&self.pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                // Check if process is alive
                return is_process_alive(pid);
            }
        }
        false
    }

    /// Get the daemon's base directory.
    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    /// Handle an incoming IPC message.
    pub async fn handle_message(&mut self, msg: IpcMessage) -> IpcMessage {
        match msg {
            IpcMessage::ExecuteCommand {
                command, source, ..
            } => {
                tracing::info!("Received command from {}: {}", source, command);
                IpcMessage::CommandResult {
                    success: true,
                    response: format!("Received: {command}"),
                    audio_text: None,
                    needs_confirmation: false,
                    confirmation_prompt: None,
                    session_id: None,
                }
            }
            IpcMessage::StatusQuery => IpcMessage::StatusResponse {
                state: format!("{:?}", self.state),
                uptime_secs: 0, // TODO: track actual uptime
                active_tasks: 0,
                expert: None,
                siri_active: self.siri_active,
            },
            IpcMessage::Shutdown => {
                let _ = self.stop().await;
                IpcMessage::CommandResult {
                    success: true,
                    response: "Daemon shutting down".into(),
                    audio_text: None,
                    needs_confirmation: false,
                    confirmation_prompt: None,
                    session_id: None,
                }
            }
            _ => IpcMessage::CommandResult {
                success: false,
                response: "Unknown message type".into(),
                audio_text: None,
                needs_confirmation: false,
                confirmation_prompt: None,
                session_id: None,
            },
        }
    }
}

/// Check if a process with the given PID is alive.
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Signal 0 checks if process exists without sending a signal.
        // Use extern "C" directly to avoid libc crate dependency.
        unsafe {
            unsafe extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            kill(pid as i32, 0) == 0
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Daemon-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("Daemon is already running")]
    AlreadyRunning,
    #[error("Daemon is not running")]
    NotRunning,
    #[error("IO error: {0}")]
    IoError(String),
    #[error("IPC error: {0}")]
    IpcError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_new() {
        let config = DaemonConfig::default();
        let daemon = RustantDaemon::new(config, PathBuf::from("/tmp/test-rustant"));
        assert!(matches!(daemon.state(), DaemonState::Starting));
        assert!(!daemon.is_siri_active());
    }
}
