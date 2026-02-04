//! TuiCallback: bridges AgentCallback to TUI event channel.
//!
//! The Agent runs in a background tokio task. Each AgentCallback method
//! sends a TuiEvent through an unbounded mpsc channel, which the TUI
//! main loop polls with tokio::select!.
//!
//! For approval requests, a oneshot channel is used so the agent
//! suspends until the user responds in the TUI.

use rustant_core::explanation::DecisionExplanation;
use rustant_core::safety::{ActionRequest, ApprovalDecision};
use rustant_core::types::{AgentStatus, CostEstimate, ProgressUpdate, TokenUsage, ToolOutput};
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
        reply: oneshot::Sender<ApprovalDecision>,
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
    /// Token usage and cost update after an LLM call.
    UsageUpdate {
        usage: TokenUsage,
        cost: CostEstimate,
    },
    /// Decision explanation for a tool selection.
    DecisionExplanation(DecisionExplanation),
    /// Budget warning or exceeded notification.
    BudgetWarning {
        message: String,
        severity: rustant_core::BudgetSeverity,
    },
    /// Progress update during tool execution (streaming output, etc.).
    Progress(ProgressUpdate),
    /// Multi-agent status update for the task board.
    #[allow(dead_code)]
    MultiAgentUpdate(Vec<crate::tui::widgets::task_board::AgentSummary>),
    /// A clarification question from the agent. The TUI must send the answer via the oneshot.
    ClarificationRequest {
        question: String,
        reply: oneshot::Sender<String>,
    },
    /// Context window health notification (warnings, compression events).
    ContextHealth(rustant_core::ContextHealthEvent),
    /// A channel digest has been generated.
    ChannelDigest(serde_json::Value),
    /// A channel message needs immediate user attention.
    ChannelAlert {
        channel: String,
        sender: String,
        summary: String,
    },
    /// A scheduled follow-up reminder has been triggered.
    Reminder(serde_json::Value),
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

    async fn request_approval(&self, action: &ActionRequest) -> ApprovalDecision {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self.tx.send(TuiEvent::ApprovalRequest {
            action: action.clone(),
            reply: reply_tx,
        });
        if sent.is_err() {
            return ApprovalDecision::Deny;
        }
        // Block (async) until the TUI sends a decision.
        reply_rx.await.unwrap_or(ApprovalDecision::Deny)
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

    async fn on_usage_update(&self, usage: &TokenUsage, cost: &CostEstimate) {
        let _ = self.tx.send(TuiEvent::UsageUpdate {
            usage: *usage,
            cost: *cost,
        });
    }

    async fn on_decision_explanation(&self, explanation: &DecisionExplanation) {
        let _ = self
            .tx
            .send(TuiEvent::DecisionExplanation(explanation.clone()));
    }

    async fn on_budget_warning(&self, message: &str, severity: rustant_core::BudgetSeverity) {
        let _ = self.tx.send(TuiEvent::BudgetWarning {
            message: message.to_string(),
            severity,
        });
    }

    async fn on_progress(&self, progress: &ProgressUpdate) {
        let _ = self.tx.send(TuiEvent::Progress(progress.clone()));
    }

    async fn on_clarification_request(&self, question: &str) -> String {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self.tx.send(TuiEvent::ClarificationRequest {
            question: question.to_string(),
            reply: reply_tx,
        });
        if sent.is_err() {
            return String::new();
        }
        // Block (async) until the TUI sends the user's answer.
        reply_rx.await.unwrap_or_default()
    }

    async fn on_context_health(&self, event: &rustant_core::ContextHealthEvent) {
        let _ = self.tx.send(TuiEvent::ContextHealth(event.clone()));
    }

    async fn on_channel_digest(&self, digest: &serde_json::Value) {
        let _ = self.tx.send(TuiEvent::ChannelDigest(digest.clone()));
    }

    async fn on_channel_alert(&self, channel: &str, sender: &str, summary: &str) {
        let _ = self.tx.send(TuiEvent::ChannelAlert {
            channel: channel.to_string(),
            sender: sender.to_string(),
            summary: summary.to_string(),
        });
    }

    async fn on_reminder(&self, reminder: &serde_json::Value) {
        let _ = self.tx.send(TuiEvent::Reminder(reminder.clone()));
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
                reply.send(ApprovalDecision::Approve).unwrap();
            }
            other => panic!("Expected ApprovalRequest, got {:?}", other),
        }

        let result = approval_task.await.unwrap();
        assert_eq!(result, ApprovalDecision::Approve);
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
                reply.send(ApprovalDecision::Deny).unwrap();
            }
            other => panic!("Expected ApprovalRequest, got {:?}", other),
        }

        let result = approval_task.await.unwrap();
        assert_eq!(result, ApprovalDecision::Deny);
    }

    #[tokio::test]
    async fn test_callback_sends_clarification_request() {
        let (callback, mut rx) = TuiCallback::new();

        let clarify_task = tokio::spawn(async move {
            callback
                .on_clarification_request("Which environment?")
                .await
        });

        match rx.recv().await.unwrap() {
            TuiEvent::ClarificationRequest { question, reply } => {
                assert_eq!(question, "Which environment?");
                reply.send("production".to_string()).unwrap();
            }
            other => panic!("Expected ClarificationRequest, got {:?}", other),
        }

        let answer = clarify_task.await.unwrap();
        assert_eq!(answer, "production");
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

    #[tokio::test]
    async fn test_callback_sends_context_health_warning() {
        let (callback, mut rx) = TuiCallback::new();
        let event = rustant_core::ContextHealthEvent::Warning {
            usage_percent: 75,
            total_tokens: 6000,
            context_window: 8000,
        };
        callback.on_context_health(&event).await;
        match rx.recv().await.unwrap() {
            TuiEvent::ContextHealth(rustant_core::ContextHealthEvent::Warning {
                usage_percent,
                ..
            }) => {
                assert_eq!(usage_percent, 75);
            }
            other => panic!("Expected ContextHealth Warning, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_sends_channel_digest() {
        let (callback, mut rx) = TuiCallback::new();
        let digest = serde_json::json!({
            "summary": "47 messages across 3 channels",
            "total_messages": 47
        });
        callback.on_channel_digest(&digest).await;
        match rx.recv().await.unwrap() {
            TuiEvent::ChannelDigest(d) => {
                assert_eq!(d["total_messages"], 47);
            }
            other => panic!("Expected ChannelDigest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_sends_channel_alert() {
        let (callback, mut rx) = TuiCallback::new();
        callback
            .on_channel_alert("email", "boss@corp.com", "Urgent Q1 review")
            .await;
        match rx.recv().await.unwrap() {
            TuiEvent::ChannelAlert {
                channel,
                sender,
                summary,
            } => {
                assert_eq!(channel, "email");
                assert_eq!(sender, "boss@corp.com");
                assert_eq!(summary, "Urgent Q1 review");
            }
            other => panic!("Expected ChannelAlert, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_sends_reminder() {
        let (callback, mut rx) = TuiCallback::new();
        let reminder = serde_json::json!({
            "description": "Follow up on Q1 report",
            "source_channel": "email"
        });
        callback.on_reminder(&reminder).await;
        match rx.recv().await.unwrap() {
            TuiEvent::Reminder(r) => {
                assert_eq!(r["description"], "Follow up on Q1 report");
            }
            other => panic!("Expected Reminder, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_callback_sends_context_health_compressed() {
        let (callback, mut rx) = TuiCallback::new();
        let event = rustant_core::ContextHealthEvent::Compressed {
            messages_compressed: 12,
            was_llm_summarized: true,
            pinned_preserved: 2,
        };
        callback.on_context_health(&event).await;
        match rx.recv().await.unwrap() {
            TuiEvent::ContextHealth(rustant_core::ContextHealthEvent::Compressed {
                messages_compressed,
                was_llm_summarized,
                pinned_preserved,
            }) => {
                assert_eq!(messages_compressed, 12);
                assert!(was_llm_summarized);
                assert_eq!(pinned_preserved, 2);
            }
            other => panic!("Expected ContextHealth Compressed, got {:?}", other),
        }
    }
}
