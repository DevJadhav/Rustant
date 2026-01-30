//! TuiCallback: bridges AgentCallback to TUI event channel.
//!
//! The Agent runs in a background tokio task. Each AgentCallback method
//! sends a TuiEvent through an unbounded mpsc channel, which the TUI
//! main loop polls with tokio::select!.
//!
//! For approval requests, a oneshot channel is used so the agent
//! suspends until the user responds in the TUI.

use rustant_core::safety::ActionRequest;
use rustant_core::types::{AgentStatus, ToolOutput};
use rustant_core::AgentCallback;
use tokio::sync::{mpsc, oneshot};

/// Events sent from the Agent (via TuiCallback) to the TUI event loop.
#[derive(Debug)]
pub enum TuiEvent {
    /// The assistant produced a complete message.
    AssistantMessage(String),
    /// A single streaming token arrived.
    StreamToken(String),
    /// An approval is needed. The TUI must send the decision via the oneshot.
    ApprovalRequest {
        action: ActionRequest,
        reply: oneshot::Sender<bool>,
    },
    /// A tool started executing.
    ToolStart {
        name: String,
        args: serde_json::Value,
    },
    /// A tool finished executing.
    ToolResult {
        name: String,
        output: ToolOutput,
        duration_ms: u64,
    },
    /// The agent status changed.
    StatusChange(AgentStatus),
}

/// Implements AgentCallback by forwarding events through an mpsc channel.
pub struct TuiCallback {
    tx: mpsc::UnboundedSender<TuiEvent>,
}

impl TuiCallback {
    /// Create a new TuiCallback and its corresponding receiver.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<TuiEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Create from an existing sender (useful for testing).
    #[allow(dead_code)]
    pub fn from_sender(tx: mpsc::UnboundedSender<TuiEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl AgentCallback for TuiCallback {
    async fn on_assistant_message(&self, message: &str) {
        let _ = self
            .tx
            .send(TuiEvent::AssistantMessage(message.to_string()));
    }

    async fn on_token(&self, token: &str) {
        let _ = self.tx.send(TuiEvent::StreamToken(token.to_string()));
    }

    async fn request_approval(&self, action: &ActionRequest) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self.tx.send(TuiEvent::ApprovalRequest {
            action: action.clone(),
            reply: reply_tx,
        });
        if sent.is_err() {
            return false;
        }
        // Block (async) until the TUI sends a decision.
        reply_rx.await.unwrap_or(false)
    }

    async fn on_tool_start(&self, tool_name: &str, args: &serde_json::Value) {
        let _ = self.tx.send(TuiEvent::ToolStart {
            name: tool_name.to_string(),
            args: args.clone(),
        });
    }

    async fn on_tool_result(&self, tool_name: &str, output: &ToolOutput, duration_ms: u64) {
        let _ = self.tx.send(TuiEvent::ToolResult {
            name: tool_name.to_string(),
            output: output.clone(),
            duration_ms,
        });
    }

    async fn on_status_change(&self, status: AgentStatus) {
        let _ = self.tx.send(TuiEvent::StatusChange(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustant_core::safety::ActionDetails;
    use rustant_core::types::RiskLevel;

    #[tokio::test]
    async fn test_callback_sends_assistant_message() {
        let (callback, mut rx) = TuiCallback::new();
        callback.on_assistant_message("hello").await;
        match rx.recv().await.unwrap() {
            TuiEvent::AssistantMessage(msg) => assert_eq!(msg, "hello"),
            other => panic!("Expected AssistantMessage, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_sends_stream_token() {
        let (callback, mut rx) = TuiCallback::new();
        callback.on_token("tok").await;
        match rx.recv().await.unwrap() {
            TuiEvent::StreamToken(t) => assert_eq!(t, "tok"),
            other => panic!("Expected StreamToken, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_sends_tool_start() {
        let (callback, mut rx) = TuiCallback::new();
        let args = serde_json::json!({"path": "foo.rs"});
        callback.on_tool_start("file_read", &args).await;
        match rx.recv().await.unwrap() {
            TuiEvent::ToolStart { name, args: a } => {
                assert_eq!(name, "file_read");
                assert_eq!(a["path"], "foo.rs");
            }
            other => panic!("Expected ToolStart, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_sends_status_change() {
        let (callback, mut rx) = TuiCallback::new();
        callback.on_status_change(AgentStatus::Thinking).await;
        match rx.recv().await.unwrap() {
            TuiEvent::StatusChange(s) => assert_eq!(s, AgentStatus::Thinking),
            other => panic!("Expected StatusChange, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_approval_approved() {
        let (callback, mut rx) = TuiCallback::new();

        let approval_task = tokio::spawn(async move {
            let action = rustant_core::SafetyGuardian::create_action_request(
                "file_write",
                RiskLevel::Write,
                "Write to test.rs",
                ActionDetails::FileWrite {
                    path: "test.rs".into(),
                    size_bytes: 100,
                },
            );
            callback.request_approval(&action).await
        });

        // Simulate the TUI resolving the approval
        match rx.recv().await.unwrap() {
            TuiEvent::ApprovalRequest { reply, .. } => {
                reply.send(true).unwrap();
            }
            other => panic!("Expected ApprovalRequest, got {:?}", other),
        }

        let result = approval_task.await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_callback_approval_denied() {
        let (callback, mut rx) = TuiCallback::new();

        let approval_task = tokio::spawn(async move {
            let action = rustant_core::SafetyGuardian::create_action_request(
                "shell_exec",
                RiskLevel::Execute,
                "Run rm -rf",
                ActionDetails::ShellCommand {
                    command: "rm -rf /".into(),
                },
            );
            callback.request_approval(&action).await
        });

        match rx.recv().await.unwrap() {
            TuiEvent::ApprovalRequest { reply, .. } => {
                reply.send(false).unwrap();
            }
            other => panic!("Expected ApprovalRequest, got {:?}", other),
        }

        let result = approval_task.await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_callback_sends_tool_result() {
        let (callback, mut rx) = TuiCallback::new();
        let output = ToolOutput::text("file content here");
        callback.on_tool_result("file_read", &output, 42).await;
        match rx.recv().await.unwrap() {
            TuiEvent::ToolResult {
                name,
                output: o,
                duration_ms,
            } => {
                assert_eq!(name, "file_read");
                assert_eq!(o.content, "file content here");
                assert_eq!(duration_ms, 42);
            }
            other => panic!("Expected ToolResult, got {:?}", other),
        }
    }
}
