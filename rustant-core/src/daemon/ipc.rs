//! IPC protocol for daemon communication.
//!
//! JSON-encoded request/response messages over Unix socket (macOS/Linux)
//! or TCP fallback.

use serde::{Deserialize, Serialize};

/// IPC message protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcMessage {
    /// Execute a command.
    ExecuteCommand {
        command: String,
        source: String,
        timeout_secs: u32,
    },
    /// Result of a command execution.
    CommandResult {
        success: bool,
        response: String,
        audio_text: Option<String>,
        needs_confirmation: bool,
        confirmation_prompt: Option<String>,
        session_id: Option<String>,
    },
    /// Confirm or deny a pending action.
    ConfirmAction { session_id: String, confirmed: bool },
    /// Query daemon status.
    StatusQuery,
    /// Daemon status response.
    StatusResponse {
        state: String,
        uptime_secs: u64,
        active_tasks: usize,
        expert: Option<String>,
        siri_active: bool,
    },
    /// Request daemon shutdown.
    Shutdown,
}

/// IPC server listening on Unix socket.
pub struct IpcServer {
    socket_path: std::path::PathBuf,
}

impl IpcServer {
    /// Create a new IPC server.
    pub fn new(socket_path: std::path::PathBuf) -> Self {
        Self { socket_path }
    }

    /// Get the socket path.
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    /// Parse an incoming JSON message.
    pub fn parse_message(data: &[u8]) -> Result<IpcMessage, serde_json::Error> {
        serde_json::from_slice(data)
    }

    /// Serialize a response message.
    pub fn serialize_response(msg: &IpcMessage) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_roundtrip() {
        let msg = IpcMessage::ExecuteCommand {
            command: "check calendar".into(),
            source: "siri".into(),
            timeout_secs: 30,
        };

        let serialized = serde_json::to_vec(&msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_slice(&serialized).unwrap();

        if let IpcMessage::ExecuteCommand {
            command,
            source,
            timeout_secs,
        } = deserialized
        {
            assert_eq!(command, "check calendar");
            assert_eq!(source, "siri");
            assert_eq!(timeout_secs, 30);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_status_query_roundtrip() {
        let msg = IpcMessage::StatusQuery;
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, IpcMessage::StatusQuery));
    }

    #[test]
    fn test_command_result_with_confirmation() {
        let msg = IpcMessage::CommandResult {
            success: true,
            response: "Need confirmation".into(),
            audio_text: Some("Should I proceed?".into()),
            needs_confirmation: true,
            confirmation_prompt: Some("Delete 3 files?".into()),
            session_id: Some("sess-123".into()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();

        if let IpcMessage::CommandResult {
            needs_confirmation, ..
        } = parsed
        {
            assert!(needs_confirmation);
        } else {
            panic!("Wrong variant");
        }
    }
}
