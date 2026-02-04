//! Agent orchestrator implementing the Think → Act → Observe event loop.
//!
//! The `Agent` struct ties together the Brain, ToolRegistry, Memory, and Safety
//! Guardian to autonomously execute tasks through LLM-powered reasoning.

use crate::brain::{Brain, LlmProvider};
use crate::config::AgentConfig;
use crate::error::{AgentError, LlmError, RustantError, ToolError};
use crate::explanation::{DecisionExplanation, DecisionType, ExplanationBuilder, FactorInfluence};
use crate::memory::MemorySystem;
use crate::safety::{
    ActionDetails, ActionRequest, ApprovalContext, ApprovalDecision, ContractCheckResult,
    PermissionResult, ReversibilityInfo, SafetyGuardian,
};
use crate::summarizer::ContextSummarizer;
use crate::types::{
    AgentState, AgentStatus, CompletionResponse, Content, CostEstimate, Message, ProgressUpdate,
    RiskLevel, Role, StreamEvent, TokenUsage, ToolDefinition, ToolOutput,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Truncate a string to at most `max_chars` characters, respecting UTF-8 boundaries.
fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Messages sent to the agent loop via the handle.
pub enum AgentMessage {
    ProcessTask {
        task: String,
        reply: oneshot::Sender<TaskResult>,
    },
    Cancel {
        task_id: Uuid,
    },
    GetStatus {
        reply: oneshot::Sender<AgentStatus>,
    },
    Shutdown,
}

/// The result of a completed task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: Uuid,
    pub success: bool,
    pub response: String,
    pub iterations: usize,
    pub total_usage: TokenUsage,
    pub total_cost: CostEstimate,
}

/// Severity of a budget warning or exceeded condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSeverity {
    /// Budget usage is approaching the limit.
    Warning,
    /// Budget limit has been exceeded.
    Exceeded,
}

/// Event emitted for context window health monitoring.
#[derive(Debug, Clone)]
pub enum ContextHealthEvent {
    /// Context usage is approaching the limit (>= 70%).
    Warning {
        usage_percent: u8,
        total_tokens: usize,
        context_window: usize,
    },
    /// Context usage is critical (>= 90%).
    Critical {
        usage_percent: u8,
        total_tokens: usize,
        context_window: usize,
    },
    /// Context compression just occurred.
    Compressed {
        messages_compressed: usize,
        was_llm_summarized: bool,
        pinned_preserved: usize,
    },
}

/// Callback trait for user interaction (approval, display).
#[async_trait::async_trait]
pub trait AgentCallback: Send + Sync {
    /// Display a message from the assistant to the user.
    async fn on_assistant_message(&self, message: &str);

    /// Display a streaming token from the assistant.
    async fn on_token(&self, token: &str);

    /// Request approval for an action. Returns the user's decision.
    async fn request_approval(&self, action: &ActionRequest) -> ApprovalDecision;

    /// Notify about a tool execution.
    async fn on_tool_start(&self, tool_name: &str, args: &serde_json::Value);

    /// Notify about a tool result.
    async fn on_tool_result(&self, tool_name: &str, output: &ToolOutput, duration_ms: u64);

    /// Notify about agent status changes.
    async fn on_status_change(&self, status: AgentStatus);

    /// Notify about token usage and cost after each LLM call.
    async fn on_usage_update(&self, usage: &TokenUsage, cost: &CostEstimate);

    /// Notify about a decision explanation for a tool selection.
    async fn on_decision_explanation(&self, explanation: &DecisionExplanation);

    /// Notify about a budget warning or exceeded condition.
    /// Default is a no-op for backward compatibility.
    async fn on_budget_warning(&self, _message: &str, _severity: BudgetSeverity) {}

    /// Notify about progress during tool execution (streaming output, file operations, etc.).
    /// Default is a no-op for backward compatibility.
    async fn on_progress(&self, _progress: &ProgressUpdate) {}

    /// Request clarification from the user. Returns the user's answer.
    /// Called when the agent needs more information to proceed.
    /// Default returns empty string for backward compatibility.
    async fn on_clarification_request(&self, _question: &str) -> String {
        String::new()
    }

    /// Notify about context window health changes (warnings, compression events).
    /// Default is a no-op for backward compatibility.
    async fn on_context_health(&self, _event: &ContextHealthEvent) {}

    /// A channel digest has been generated and is ready for review.
    /// Called when the digest system completes a summary for the configured period.
    /// Default is a no-op for backward compatibility.
    async fn on_channel_digest(&self, _digest: &serde_json::Value) {}

    /// A message on a channel needs immediate user attention (escalation).
    ///
    /// Called when the intelligence layer classifies a message at or above the
    /// escalation threshold. Uses `&str` parameters rather than `ClassifiedMessage`
    /// to keep the callback trait decoupled from the classification system — callers
    /// can format the alert data however they choose.
    ///
    /// Default is a no-op for backward compatibility.
    async fn on_channel_alert(&self, _channel: &str, _sender: &str, _summary: &str) {}

    /// A scheduled follow-up reminder has been triggered.
    /// Called when a cron-scheduled reminder fires for a previously classified
    /// message that requires follow-up.
    /// Default is a no-op for backward compatibility.
    async fn on_reminder(&self, _reminder: &serde_json::Value) {}
}

/// A tool executor function type. The agent holds tool executors and their definitions.
pub type ToolExecutor = Box<
    dyn Fn(
            serde_json::Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<ToolOutput, ToolError>> + Send>,
        > + Send
        + Sync,
>;

/// A registered tool with its definition and executor.
pub struct RegisteredTool {
    pub definition: ToolDefinition,
    pub risk_level: RiskLevel,
    pub executor: ToolExecutor,
}

/// The Agent orchestrator running the Think → Act → Observe loop.
pub struct Agent {
    brain: Brain,
    memory: MemorySystem,
    safety: SafetyGuardian,
    tools: HashMap<String, RegisteredTool>,
    state: AgentState,
    #[allow(dead_code)]
    config: AgentConfig,
    cancellation: CancellationToken,
    callback: Arc<dyn AgentCallback>,
    /// LLM-based context summarizer for intelligent compression.
    summarizer: ContextSummarizer,
    /// Token budget manager for cost control.
    budget: crate::brain::TokenBudgetManager,
    /// Cross-session knowledge distiller for learning from corrections/facts.
    knowledge: crate::memory::KnowledgeDistiller,
    /// Per-tool token usage tracking for budget breakdown.
    tool_token_usage: HashMap<String, usize>,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        config: AgentConfig,
        callback: Arc<dyn AgentCallback>,
    ) -> Self {
        let summarizer = ContextSummarizer::new(Arc::clone(&provider));
        let brain = Brain::new(provider, crate::brain::DEFAULT_SYSTEM_PROMPT);
        let memory = MemorySystem::new(config.memory.window_size);
        let safety = SafetyGuardian::new(config.safety.clone());
        let max_iter = config.safety.max_iterations;
        let budget = crate::brain::TokenBudgetManager::new(config.budget.as_ref());
        let knowledge = crate::memory::KnowledgeDistiller::new(config.knowledge.as_ref());

        Self {
            brain,
            memory,
            safety,
            tools: HashMap::new(),
            state: AgentState::new(max_iter),
            config,
            cancellation: CancellationToken::new(),
            callback,
            summarizer,
            budget,
            knowledge,
            tool_token_usage: HashMap::new(),
        }
    }

    /// Register a tool with the agent.
    pub fn register_tool(&mut self, tool: RegisteredTool) {
        self.tools.insert(tool.definition.name.clone(), tool);
    }

    /// Get tool definitions for the LLM.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> =
            self.tools.values().map(|t| t.definition.clone()).collect();

        // Add the ask_user pseudo-tool so the LLM knows it can ask clarifying questions.
        defs.push(ToolDefinition {
            name: "ask_user".to_string(),
            description: "Ask the user a clarifying question when you need more information to proceed. Use this when the task is ambiguous or you need to confirm something before taking action.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to ask the user"
                    }
                },
                "required": ["question"]
            }),
        });

        defs
    }

    /// Process a user task through the agent loop.
    pub async fn process_task(&mut self, task: &str) -> Result<TaskResult, RustantError> {
        let task_id = Uuid::new_v4();
        info!(task_id = %task_id, task = task, "Starting task processing");

        self.state.start_task(task);
        self.state.task_id = Some(task_id);
        self.memory.start_new_task(task);
        self.budget.reset_task();
        self.tool_token_usage.clear();

        // Run knowledge distillation from long-term memory and inject into brain
        self.knowledge.distill(&self.memory.long_term);
        let knowledge_addendum = self.knowledge.rules_for_prompt();
        self.brain.set_knowledge_addendum(knowledge_addendum);

        self.memory.add_message(Message::user(task));
        self.callback.on_status_change(AgentStatus::Thinking).await;

        let mut final_response = String::new();

        loop {
            // Check cancellation
            if self.cancellation.is_cancelled() {
                self.state.set_error();
                return Err(RustantError::Agent(AgentError::Cancelled));
            }

            // Check iteration limit
            if !self.state.increment_iteration() {
                warn!(
                    task_id = %task_id,
                    iterations = self.state.iteration,
                    "Maximum iterations reached"
                );
                self.state.set_error();
                return Err(RustantError::Agent(AgentError::MaxIterationsReached {
                    max: self.state.max_iterations,
                }));
            }

            debug!(
                task_id = %task_id,
                iteration = self.state.iteration,
                "Agent loop iteration"
            );

            // --- THINK ---
            self.state.status = AgentStatus::Thinking;
            self.callback.on_status_change(AgentStatus::Thinking).await;

            let conversation = self.memory.context_messages();
            let tools = Some(self.tool_definitions());

            // Context health check before LLM call
            {
                let context_window = self.brain.provider().context_window();
                let breakdown = self.memory.context_breakdown(context_window);
                let usage_percent = (breakdown.usage_ratio() * 100.0) as u8;
                if usage_percent >= 90 {
                    self.callback
                        .on_context_health(&ContextHealthEvent::Critical {
                            usage_percent,
                            total_tokens: breakdown.total_tokens,
                            context_window: breakdown.context_window,
                        })
                        .await;
                } else if usage_percent >= 70 {
                    self.callback
                        .on_context_health(&ContextHealthEvent::Warning {
                            usage_percent,
                            total_tokens: breakdown.total_tokens,
                            context_window: breakdown.context_window,
                        })
                        .await;
                }
            }

            // Pre-call budget check
            let estimated_tokens = self.brain.estimate_tokens(&conversation);
            let (input_rate, output_rate) = self.brain.provider_cost_rates();
            let budget_result = self
                .budget
                .check_budget(estimated_tokens, input_rate, output_rate);
            match &budget_result {
                crate::brain::BudgetCheckResult::Exceeded { message } => {
                    let top = self.top_tool_consumers(3);
                    let enriched = if top.is_empty() {
                        message.clone()
                    } else {
                        format!("{}. Top consumers: {}", message, top)
                    };
                    self.callback
                        .on_budget_warning(&enriched, BudgetSeverity::Exceeded)
                        .await;
                    if self.budget.should_halt_on_exceed() {
                        warn!("Budget exceeded, halting: {}", enriched);
                        return Err(RustantError::Agent(AgentError::BudgetExceeded {
                            message: enriched,
                        }));
                    }
                    warn!("Budget warning (soft limit): {}", enriched);
                }
                crate::brain::BudgetCheckResult::Warning { message, .. } => {
                    let top = self.top_tool_consumers(3);
                    let enriched = if top.is_empty() {
                        message.clone()
                    } else {
                        format!("{}. Top consumers: {}", message, top)
                    };
                    self.callback
                        .on_budget_warning(&enriched, BudgetSeverity::Warning)
                        .await;
                    debug!("Budget warning: {}", enriched);
                }
                crate::brain::BudgetCheckResult::Ok => {}
            }

            let response = if self.config.llm.use_streaming {
                self.think_streaming(&conversation, tools).await?
            } else {
                self.brain.think_with_retry(&conversation, tools, 3).await?
            };

            // Record usage in budget manager and emit live update
            self.budget.record_usage(
                &response.usage,
                &CostEstimate {
                    input_cost: response.usage.input_tokens as f64 * input_rate,
                    output_cost: response.usage.output_tokens as f64 * output_rate,
                },
            );
            self.callback
                .on_usage_update(self.brain.total_usage(), self.brain.total_cost())
                .await;

            // --- DECIDE ---
            self.state.status = AgentStatus::Deciding;
            match &response.message.content {
                Content::Text { text } => {
                    // LLM produced a text response — task may be complete
                    info!(task_id = %task_id, "Agent produced text response");
                    self.callback.on_assistant_message(text).await;
                    self.memory.add_message(response.message.clone());
                    final_response = text.clone();
                    // Text response means the agent is done thinking
                    break;
                }
                Content::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    // LLM wants to call a tool
                    info!(
                        task_id = %task_id,
                        tool = name,
                        "Agent requesting tool execution"
                    );
                    self.memory.add_message(response.message.clone());

                    // Build and emit decision explanation
                    let explanation = self.build_decision_explanation(name, arguments);
                    self.callback.on_decision_explanation(&explanation).await;

                    // --- ACT ---
                    let result = self.execute_tool(id, name, arguments).await;

                    // --- OBSERVE ---
                    let result_tokens = match &result {
                        Ok(output) => {
                            let result_msg = Message::tool_result(id, &output.content, false);
                            let tokens = output.content.len() / 4; // rough estimate
                            self.memory.add_message(result_msg);
                            tokens
                        }
                        Err(e) => {
                            let error_msg = format!("Tool error: {}", e);
                            let tokens = error_msg.len() / 4;
                            let result_msg = Message::tool_result(id, &error_msg, true);
                            self.memory.add_message(result_msg);
                            tokens
                        }
                    };
                    *self.tool_token_usage.entry(name.to_string()).or_insert(0) += result_tokens;

                    // Check context compression
                    if self.memory.short_term.needs_compression() {
                        debug!("Triggering LLM-based context compression");
                        let msgs_to_summarize: Vec<Message> = self
                            .memory
                            .short_term
                            .messages_to_summarize()
                            .into_iter()
                            .cloned()
                            .collect();
                        let msgs_count = msgs_to_summarize.len();
                        let pinned_count = self.memory.short_term.pinned_count();

                        let (summary_text, was_llm) = match self
                            .summarizer
                            .summarize(&msgs_to_summarize)
                            .await
                        {
                            Ok(result) => {
                                info!(
                                    messages_summarized = result.messages_summarized,
                                    tokens_saved = result.tokens_saved,
                                    "Context compression via LLM summarization"
                                );
                                (result.text, true)
                            }
                            Err(e) => {
                                warn!(error = %e, "LLM summarization failed, falling back to truncation");
                                // Fallback: smart structured summary preserving tool names
                                // and first/last messages instead of naive truncation
                                let text = crate::summarizer::smart_fallback_summary(
                                    &msgs_to_summarize,
                                    500,
                                );
                                (text, false)
                            }
                        };

                        self.memory.short_term.compress(summary_text);

                        // Notify about compression
                        self.callback
                            .on_context_health(&ContextHealthEvent::Compressed {
                                messages_compressed: msgs_count,
                                was_llm_summarized: was_llm,
                                pinned_preserved: pinned_count,
                            })
                            .await;
                    }

                    // Continue loop — agent needs to observe and think again
                }
                Content::MultiPart { parts } => {
                    // Handle multi-part responses (text + tool calls)
                    self.memory.add_message(response.message.clone());

                    let mut has_tool_call = false;
                    for part in parts {
                        match part {
                            Content::Text { text } => {
                                self.callback.on_assistant_message(text).await;
                                final_response = text.clone();
                            }
                            Content::ToolCall {
                                id,
                                name,
                                arguments,
                            } => {
                                has_tool_call = true;

                                // Build and emit decision explanation (same as single ToolCall path)
                                let explanation = self.build_decision_explanation(name, arguments);
                                self.callback.on_decision_explanation(&explanation).await;

                                let result = self.execute_tool(id, name, arguments).await;
                                // Note: fact recording for cross-session learning is handled
                                // inside execute_tool() — no need to duplicate it here.
                                let result_tokens = match &result {
                                    Ok(output) => {
                                        let msg = Message::tool_result(id, &output.content, false);
                                        let tokens = output.content.len() / 4;
                                        self.memory.add_message(msg);
                                        tokens
                                    }
                                    Err(e) => {
                                        let error_msg = format!("Tool error: {}", e);
                                        let tokens = error_msg.len() / 4;
                                        let msg = Message::tool_result(id, &error_msg, true);
                                        self.memory.add_message(msg);
                                        tokens
                                    }
                                };

                                // Track per-tool token usage (same as single ToolCall path)
                                *self.tool_token_usage.entry(name.to_string()).or_insert(0) +=
                                    result_tokens;
                            }
                            _ => {}
                        }
                    }

                    if !has_tool_call {
                        break; // Only text, we're done
                    }

                    // Check context compression after multipart tool calls
                    if self.memory.short_term.needs_compression() {
                        debug!("Triggering LLM-based context compression (multipart)");
                        let msgs_to_summarize: Vec<Message> = self
                            .memory
                            .short_term
                            .messages_to_summarize()
                            .into_iter()
                            .cloned()
                            .collect();
                        let msgs_count = msgs_to_summarize.len();
                        let pinned_count = self.memory.short_term.pinned_count();

                        let (summary_text, was_llm) = match self
                            .summarizer
                            .summarize(&msgs_to_summarize)
                            .await
                        {
                            Ok(result) => {
                                info!(
                                    messages_summarized = result.messages_summarized,
                                    tokens_saved = result.tokens_saved,
                                    "Context compression via LLM summarization (multipart)"
                                );
                                (result.text, true)
                            }
                            Err(e) => {
                                warn!(error = %e, "LLM summarization failed, falling back to truncation");
                                let text = crate::summarizer::smart_fallback_summary(
                                    &msgs_to_summarize,
                                    500,
                                );
                                (text, false)
                            }
                        };

                        self.memory.short_term.compress(summary_text);

                        self.callback
                            .on_context_health(&ContextHealthEvent::Compressed {
                                messages_compressed: msgs_count,
                                was_llm_summarized: was_llm,
                                pinned_preserved: pinned_count,
                            })
                            .await;
                    }

                    // Continue loop — agent needs to observe and think again
                }
                Content::ToolResult { .. } => {
                    // Shouldn't happen from LLM directly, but handle gracefully
                    warn!("Received unexpected ToolResult from LLM");
                    break;
                }
            }
        }

        self.state.complete();
        self.callback.on_status_change(AgentStatus::Complete).await;

        info!(
            task_id = %task_id,
            iterations = self.state.iteration,
            total_tokens = self.brain.total_usage().total(),
            total_cost = format!("${:.4}", self.brain.total_cost().total()),
            "Task completed"
        );

        Ok(TaskResult {
            task_id,
            success: true,
            response: final_response,
            iterations: self.state.iteration,
            total_usage: *self.brain.total_usage(),
            total_cost: *self.brain.total_cost(),
        })
    }

    /// Perform a streaming think operation, sending tokens to the callback as they arrive.
    /// Returns a CompletionResponse equivalent to the non-streaming path.
    async fn think_streaming(
        &mut self,
        conversation: &[Message],
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResponse, LlmError> {
        let (tx, mut rx) = mpsc::channel(64);

        // Build messages and request manually to avoid double borrow
        let messages = self.brain.build_messages(conversation);
        let token_estimate = self.brain.provider().estimate_tokens(&messages);
        let context_limit = self.brain.provider().context_window();

        if token_estimate > context_limit {
            return Err(LlmError::ContextOverflow {
                used: token_estimate,
                limit: context_limit,
            });
        }

        let request = crate::types::CompletionRequest {
            messages,
            tools,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: Vec::new(),
            model: None,
        };

        // Run the streaming completion
        self.brain
            .provider()
            .complete_streaming(request, tx)
            .await?;

        // Consume events from the channel
        let mut text_parts = String::new();
        let mut usage = TokenUsage::default();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(token) => {
                    self.callback.on_token(&token).await;
                    text_parts.push_str(&token);
                }
                StreamEvent::Done { usage: u } => {
                    usage = u;
                    break;
                }
                StreamEvent::Error(e) => {
                    return Err(LlmError::Streaming { message: e });
                }
                _ => {}
            }
        }

        // Track usage in brain
        self.brain.track_usage(&usage);

        let message = Message::new(Role::Assistant, Content::text(text_parts));
        Ok(CompletionResponse {
            message,
            usage,
            model: self.brain.model_name().to_string(),
            finish_reason: Some("stop".to_string()),
        })
    }

    /// Execute a tool with safety checks.
    async fn execute_tool(
        &mut self,
        _call_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        // Handle ask_user pseudo-tool before regular tool lookup.
        // This bypasses safety checks since it's read-only user interaction.
        if tool_name == "ask_user" {
            self.state.status = AgentStatus::WaitingForClarification;
            self.callback
                .on_status_change(AgentStatus::WaitingForClarification)
                .await;
            let question = arguments
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("Can you provide more details?");
            let answer = self.callback.on_clarification_request(question).await;
            self.state.status = AgentStatus::Executing;
            self.callback.on_status_change(AgentStatus::Executing).await;
            return Ok(ToolOutput::text(answer));
        }

        // Look up the tool
        let tool = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound {
                name: tool_name.to_string(),
            })?;

        // Build rich approval context from action details
        let details = Self::parse_action_details(tool_name, arguments);
        let approval_context = Self::build_approval_context(tool_name, &details, tool.risk_level);

        // Build action request with rich context
        let action = SafetyGuardian::create_rich_action_request(
            tool_name,
            tool.risk_level,
            format!("Execute tool: {}", tool_name),
            details,
            approval_context,
        );

        // Check permissions
        let perm = self.safety.check_permission(&action);
        match perm {
            PermissionResult::Allowed => {
                // Proceed
            }
            PermissionResult::Denied { reason } => {
                // Emit explanation for safety denial decision
                let mut builder = ExplanationBuilder::new(DecisionType::ErrorRecovery {
                    error: format!("Permission denied for tool '{}'", tool_name),
                    strategy: "Returning error to LLM for re-planning".to_string(),
                });
                builder.add_reasoning_step(format!("Denied: {}", reason), None);
                builder.set_confidence(1.0);
                let explanation = builder.build();
                self.callback.on_decision_explanation(&explanation).await;

                return Err(ToolError::PermissionDenied {
                    name: tool_name.to_string(),
                    reason,
                });
            }
            PermissionResult::RequiresApproval { context: _ } => {
                self.state.status = AgentStatus::WaitingForApproval;
                self.callback
                    .on_status_change(AgentStatus::WaitingForApproval)
                    .await;

                let decision = self.callback.request_approval(&action).await;
                let approved = decision != ApprovalDecision::Deny;
                self.safety.log_approval_decision(tool_name, approved);

                match decision {
                    ApprovalDecision::Approve => {
                        // Single approval, proceed
                    }
                    ApprovalDecision::ApproveAllSimilar => {
                        // Add to session allowlist for future auto-approval
                        self.safety
                            .add_session_allowlist(tool_name.to_string(), tool.risk_level);
                        info!(
                            tool = tool_name,
                            risk = %tool.risk_level,
                            "Added tool to session allowlist (approve all similar)"
                        );
                    }
                    ApprovalDecision::Deny => {
                        // Emit explanation for user denial decision
                        let mut builder = ExplanationBuilder::new(DecisionType::ErrorRecovery {
                            error: format!("User denied approval for tool '{}'", tool_name),
                            strategy: "Returning error to LLM for re-planning".to_string(),
                        });
                        builder.add_reasoning_step(
                            "User rejected the action in approval dialog".to_string(),
                            None,
                        );
                        builder.set_confidence(1.0);
                        let explanation = builder.build();
                        self.callback.on_decision_explanation(&explanation).await;

                        // Record correction for cross-session learning:
                        // the agent's proposed action was rejected by the user.
                        self.memory.long_term.add_correction(
                            format!(
                                "Attempted tool '{}' with args: {}",
                                tool_name,
                                arguments.to_string().chars().take(200).collect::<String>()
                            ),
                            "User denied this action".to_string(),
                            format!(
                                "Tool '{}' denied by user; goal: {:?}",
                                tool_name, self.memory.working.current_goal
                            ),
                        );

                        return Err(ToolError::PermissionDenied {
                            name: tool_name.to_string(),
                            reason: "User rejected the action".to_string(),
                        });
                    }
                }
            }
        }

        // Check safety contract pre-conditions
        let tool_entry = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound {
                name: tool_name.to_string(),
            })?;
        let risk_level = tool_entry.risk_level;
        let contract_result = self
            .safety
            .contract_enforcer_mut()
            .check_pre(tool_name, risk_level, arguments);
        if contract_result != ContractCheckResult::Satisfied {
            warn!(
                tool = tool_name,
                result = ?contract_result,
                "Safety contract violation (pre-check)"
            );

            // Emit explanation for contract violation
            let mut builder = ExplanationBuilder::new(DecisionType::ErrorRecovery {
                error: format!("Contract violation: {:?}", contract_result),
                strategy: "Returning error to LLM for re-planning".to_string(),
            });
            builder.set_confidence(1.0);
            let explanation = builder.build();
            self.callback.on_decision_explanation(&explanation).await;

            return Err(ToolError::PermissionDenied {
                name: tool_name.to_string(),
                reason: format!("Safety contract violation: {:?}", contract_result),
            });
        }

        // Execute the tool
        self.state.status = AgentStatus::Executing;
        self.callback.on_status_change(AgentStatus::Executing).await;
        self.callback.on_tool_start(tool_name, arguments).await;

        let start = Instant::now();

        // Re-fetch the executor (borrow checker requires separate borrow from the one above)
        let executor = &self
            .tools
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound {
                name: tool_name.to_string(),
            })?
            .executor;
        let result = (executor)(arguments.clone()).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        // Record execution in contract enforcer
        self.safety
            .contract_enforcer_mut()
            .record_execution(risk_level, 0.0);

        match &result {
            Ok(output) => {
                self.safety.log_execution(tool_name, true, duration_ms);
                self.safety
                    .record_behavioral_outcome(tool_name, risk_level, true);
                self.callback
                    .on_tool_result(tool_name, output, duration_ms)
                    .await;

                // Record fact from successful tool execution for cross-session learning.
                // Only record non-trivial (>10 chars) and non-huge (<5000 chars) outputs
                // to avoid noise and memory bloat.
                if output.content.len() > 10 && output.content.len() < 5000 {
                    let summary = if output.content.chars().count() > 200 {
                        format!("{}...", truncate_str(&output.content, 200))
                    } else {
                        output.content.clone()
                    };
                    self.memory.long_term.add_fact(
                        crate::memory::Fact::new(
                            format!("Tool '{}' result: {}", tool_name, summary),
                            format!("tool:{}", tool_name),
                        )
                        .with_tags(vec!["tool_result".to_string(), tool_name.to_string()]),
                    );
                }
            }
            Err(e) => {
                self.safety.log_execution(tool_name, false, duration_ms);
                self.safety
                    .record_behavioral_outcome(tool_name, risk_level, false);
                let error_output = ToolOutput::error(e.to_string());
                self.callback
                    .on_tool_result(tool_name, &error_output, duration_ms)
                    .await;
            }
        }

        result
    }

    /// Build rich approval context from action details, providing users with
    /// reasoning, consequences, and reversibility information.
    fn build_approval_context(
        tool_name: &str,
        details: &ActionDetails,
        risk_level: RiskLevel,
    ) -> ApprovalContext {
        let mut ctx = ApprovalContext::new();

        // Derive consequences from action details
        match details {
            ActionDetails::FileWrite { path, size_bytes } => {
                ctx = ctx
                    .with_reasoning(format!(
                        "Writing {} bytes to {}",
                        size_bytes,
                        path.display()
                    ))
                    .with_consequence(format!(
                        "File '{}' will be created or overwritten",
                        path.display()
                    ))
                    .with_reversibility(ReversibilityInfo {
                        is_reversible: true,
                        undo_description: Some(
                            "Revert via git checkout or checkpoint restore".to_string(),
                        ),
                        undo_window: None,
                    });
            }
            ActionDetails::FileDelete { path } => {
                ctx = ctx
                    .with_reasoning(format!("Deleting file {}", path.display()))
                    .with_consequence(format!(
                        "File '{}' will be permanently removed",
                        path.display()
                    ))
                    .with_reversibility(ReversibilityInfo {
                        is_reversible: true,
                        undo_description: Some(
                            "Restore via git checkout or checkpoint".to_string(),
                        ),
                        undo_window: None,
                    });
            }
            ActionDetails::ShellCommand { command } => {
                ctx = ctx
                    .with_reasoning(format!("Executing shell command: {}", command))
                    .with_consequence("Shell command will run in the agent workspace".to_string());
                if risk_level >= RiskLevel::Execute {
                    ctx = ctx.with_consequence(
                        "Command may modify system state or produce side effects".to_string(),
                    );
                }
            }
            ActionDetails::NetworkRequest { host, method } => {
                ctx = ctx
                    .with_reasoning(format!("Making {} request to {}", method, host))
                    .with_consequence(format!("Network request will be sent to {}", host));
            }
            ActionDetails::GitOperation { operation } => {
                ctx = ctx
                    .with_reasoning(format!("Git operation: {}", operation))
                    .with_reversibility(ReversibilityInfo {
                        is_reversible: true,
                        undo_description: Some(
                            "Git operations are generally reversible via reflog".to_string(),
                        ),
                        undo_window: None,
                    });
            }
            _ => {
                ctx = ctx.with_reasoning(format!("Executing {} tool", tool_name));
            }
        }

        // Add preview for destructive tools
        ctx = ctx.with_preview_from_tool(tool_name, details);

        ctx
    }

    /// Parse tool arguments into a specific `ActionDetails` variant based on tool name.
    /// This enables `build_approval_context()` to produce rich reasoning, consequences,
    /// and reversibility info instead of always falling through to the `Other` catch-all.
    fn parse_action_details(tool_name: &str, arguments: &serde_json::Value) -> ActionDetails {
        match tool_name {
            "file_read" | "file_list" | "file_search" => {
                if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
                    ActionDetails::FileRead { path: path.into() }
                } else {
                    ActionDetails::Other {
                        info: arguments.to_string(),
                    }
                }
            }
            "file_write" | "file_patch" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let size = arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                ActionDetails::FileWrite {
                    path: path.into(),
                    size_bytes: size,
                }
            }
            "shell_exec" => {
                let cmd = arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                ActionDetails::ShellCommand {
                    command: cmd.to_string(),
                }
            }
            "git_status" | "git_diff" => ActionDetails::GitOperation {
                operation: tool_name.to_string(),
            },
            "git_commit" => {
                let msg = arguments
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let truncated = truncate_str(msg, 80);
                ActionDetails::GitOperation {
                    operation: format!("commit: {}", truncated),
                }
            }
            _ => ActionDetails::Other {
                info: arguments.to_string(),
            },
        }
    }

    /// Build a decision explanation for a tool selection.
    fn build_decision_explanation(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> DecisionExplanation {
        let risk_level = self
            .tools
            .get(tool_name)
            .map(|t| t.risk_level)
            .unwrap_or(RiskLevel::Execute);

        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: tool_name.to_string(),
        });

        // Add reasoning based on the tool and arguments
        builder.add_reasoning_step(
            format!("Selected tool '{}' (risk: {})", tool_name, risk_level),
            None,
        );

        // Add argument summary as evidence
        if let Some(obj) = arguments.as_object() {
            let param_keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
            if !param_keys.is_empty() {
                builder.add_reasoning_step(
                    format!("Parameters: {}", param_keys.join(", ")),
                    Some(&arguments.to_string()),
                );
            }
        }

        // Context factors from memory and safety state
        if let Some(goal) = &self.memory.working.current_goal {
            builder.add_context_factor(
                &format!("Current goal: {}", goal),
                FactorInfluence::Positive,
            );
        }

        builder.add_context_factor(
            &format!("Approval mode: {}", self.safety.approval_mode()),
            FactorInfluence::Neutral,
        );

        builder.add_context_factor(
            &format!(
                "Iteration {}/{}",
                self.state.iteration, self.state.max_iterations
            ),
            if self.state.iteration as f64 / self.state.max_iterations as f64 > 0.8 {
                FactorInfluence::Negative
            } else {
                FactorInfluence::Neutral
            },
        );

        // List other available tools as considered alternatives
        for (name, tool) in &self.tools {
            if name != tool_name && tool.risk_level <= risk_level {
                builder.add_alternative(name, "Not selected by LLM for this step", tool.risk_level);
            }
        }

        // Set confidence based on risk level
        let confidence = match risk_level {
            RiskLevel::ReadOnly => 0.95,
            RiskLevel::Write => 0.80,
            RiskLevel::Execute => 0.70,
            RiskLevel::Network => 0.75,
            RiskLevel::Destructive => 0.50,
        };
        builder.set_confidence(confidence);

        builder.build()
    }

    /// Get the current agent state.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Get a cancellation token for this agent.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Cancel the current task.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Get the brain reference (for usage stats).
    pub fn brain(&self) -> &Brain {
        &self.brain
    }

    /// Get the safety guardian reference (for audit log).
    pub fn safety(&self) -> &SafetyGuardian {
        &self.safety
    }

    /// Get a mutable reference to the safety guardian (for contract setup).
    pub fn safety_mut(&mut self) -> &mut SafetyGuardian {
        &mut self.safety
    }

    /// Get the memory system reference.
    pub fn memory(&self) -> &MemorySystem {
        &self.memory
    }

    /// Get a mutable reference to the memory system.
    pub fn memory_mut(&mut self) -> &mut MemorySystem {
        &mut self.memory
    }

    /// Get a reference to the agent configuration.
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get a mutable reference to the agent configuration.
    pub fn config_mut(&mut self) -> &mut AgentConfig {
        &mut self.config
    }

    /// Get per-tool token usage breakdown (tool_name -> estimated tokens).
    pub fn tool_token_breakdown(&self) -> &HashMap<String, usize> {
        &self.tool_token_usage
    }

    /// Format top token consumers as a summary string.
    pub fn top_tool_consumers(&self, n: usize) -> String {
        if self.tool_token_usage.is_empty() {
            return String::new();
        }
        let total: usize = self.tool_token_usage.values().sum();
        if total == 0 {
            return String::new();
        }
        let mut sorted: Vec<_> = self.tool_token_usage.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        let top: Vec<String> = sorted
            .iter()
            .take(n)
            .map(|(name, tokens)| {
                let pct = (**tokens as f64 / total as f64 * 100.0) as u8;
                format!("{} ({}%)", name, pct)
            })
            .collect();
        top.join(", ")
    }

    /// Compact the conversation context by summarizing older messages.
    /// Returns (messages_before, messages_after).
    pub fn compact(&mut self) -> (usize, usize) {
        let before = self.memory.short_term.len();
        if before <= 2 {
            return (before, before);
        }
        let msgs: Vec<crate::types::Message> =
            self.memory.short_term.messages().iter().cloned().collect();
        let summary = crate::summarizer::smart_fallback_summary(&msgs, 500);
        self.memory.short_term.compress(summary);
        let after = self.memory.short_term.len();
        (before, after)
    }
}

/// A no-op callback for testing.
pub struct NoOpCallback;

#[async_trait::async_trait]
impl AgentCallback for NoOpCallback {
    async fn on_assistant_message(&self, _message: &str) {}
    async fn on_token(&self, _token: &str) {}
    async fn request_approval(&self, _action: &ActionRequest) -> ApprovalDecision {
        ApprovalDecision::Approve // auto-approve in tests
    }
    async fn on_tool_start(&self, _tool_name: &str, _args: &serde_json::Value) {}
    async fn on_tool_result(&self, _tool_name: &str, _output: &ToolOutput, _duration_ms: u64) {}
    async fn on_status_change(&self, _status: AgentStatus) {}
    async fn on_usage_update(&self, _usage: &TokenUsage, _cost: &CostEstimate) {}
    async fn on_decision_explanation(&self, _explanation: &DecisionExplanation) {}
}

/// A callback that records all events for test assertions.
pub struct RecordingCallback {
    messages: tokio::sync::Mutex<Vec<String>>,
    tool_calls: tokio::sync::Mutex<Vec<String>>,
    status_changes: tokio::sync::Mutex<Vec<AgentStatus>>,
    explanations: tokio::sync::Mutex<Vec<DecisionExplanation>>,
    budget_warnings: tokio::sync::Mutex<Vec<(String, BudgetSeverity)>>,
    context_health_events: tokio::sync::Mutex<Vec<ContextHealthEvent>>,
}

impl RecordingCallback {
    pub fn new() -> Self {
        Self {
            messages: tokio::sync::Mutex::new(Vec::new()),
            tool_calls: tokio::sync::Mutex::new(Vec::new()),
            status_changes: tokio::sync::Mutex::new(Vec::new()),
            explanations: tokio::sync::Mutex::new(Vec::new()),
            budget_warnings: tokio::sync::Mutex::new(Vec::new()),
            context_health_events: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    pub async fn messages(&self) -> Vec<String> {
        self.messages.lock().await.clone()
    }

    pub async fn tool_calls(&self) -> Vec<String> {
        self.tool_calls.lock().await.clone()
    }

    pub async fn status_changes(&self) -> Vec<AgentStatus> {
        self.status_changes.lock().await.clone()
    }

    pub async fn explanations(&self) -> Vec<DecisionExplanation> {
        self.explanations.lock().await.clone()
    }

    pub async fn budget_warnings(&self) -> Vec<(String, BudgetSeverity)> {
        self.budget_warnings.lock().await.clone()
    }

    pub async fn context_health_events(&self) -> Vec<ContextHealthEvent> {
        self.context_health_events.lock().await.clone()
    }
}

impl Default for RecordingCallback {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentCallback for RecordingCallback {
    async fn on_assistant_message(&self, message: &str) {
        self.messages.lock().await.push(message.to_string());
    }
    async fn on_token(&self, _token: &str) {}
    async fn request_approval(&self, _action: &ActionRequest) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
    async fn on_tool_start(&self, tool_name: &str, _args: &serde_json::Value) {
        self.tool_calls.lock().await.push(tool_name.to_string());
    }
    async fn on_tool_result(&self, _tool_name: &str, _output: &ToolOutput, _duration_ms: u64) {}
    async fn on_status_change(&self, status: AgentStatus) {
        self.status_changes.lock().await.push(status);
    }
    async fn on_usage_update(&self, _usage: &TokenUsage, _cost: &CostEstimate) {}
    async fn on_decision_explanation(&self, explanation: &DecisionExplanation) {
        self.explanations.lock().await.push(explanation.clone());
    }
    async fn on_budget_warning(&self, message: &str, severity: BudgetSeverity) {
        self.budget_warnings
            .lock()
            .await
            .push((message.to_string(), severity));
    }
    async fn on_context_health(&self, event: &ContextHealthEvent) {
        self.context_health_events.lock().await.push(event.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MockLlmProvider;

    fn create_test_agent(provider: Arc<MockLlmProvider>) -> (Agent, Arc<RecordingCallback>) {
        let callback = Arc::new(RecordingCallback::new());
        let config = AgentConfig::default();
        let agent = Agent::new(provider, config, callback.clone());
        (agent, callback)
    }

    #[tokio::test]
    async fn test_agent_simple_text_response() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::text_response("Hello! I can help you."));

        let (mut agent, callback) = create_test_agent(provider);
        let result = agent.process_task("Say hello").await.unwrap();

        assert!(result.success);
        assert_eq!(result.response, "Hello! I can help you.");
        assert_eq!(result.iterations, 1);

        let messages = callback.messages().await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], "Hello! I can help you.");
    }

    #[tokio::test]
    async fn test_agent_tool_call_then_response() {
        let provider = Arc::new(MockLlmProvider::new());

        // First response: tool call
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "test"}),
        ));
        // Second response after tool result: text
        provider.queue_response(MockLlmProvider::text_response(
            "I executed the echo tool successfully.",
        ));

        let (mut agent, callback) = create_test_agent(provider);

        // Register a simple echo tool
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo input text".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"]
                }),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|args: serde_json::Value| {
                Box::pin(async move {
                    let text = args["text"].as_str().unwrap_or("no text");
                    Ok(ToolOutput::text(format!("Echo: {}", text)))
                })
            }),
        });

        let result = agent.process_task("Test echo tool").await.unwrap();

        assert!(result.success);
        assert_eq!(result.iterations, 2);

        let tool_calls = callback.tool_calls().await;
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0], "echo");
    }

    #[tokio::test]
    async fn test_agent_tool_not_found() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::tool_call_response(
            "nonexistent_tool",
            serde_json::json!({}),
        ));
        // After tool error, agent should respond with text
        provider.queue_response(MockLlmProvider::text_response(
            "Sorry, that tool doesn't exist.",
        ));

        let (mut agent, _callback) = create_test_agent(provider);
        let result = agent.process_task("Use nonexistent tool").await.unwrap();

        // Agent should still complete (with the tool error in context)
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_agent_state_tracking() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::text_response("Done"));

        let (mut agent, callback) = create_test_agent(provider);

        assert_eq!(agent.state().status, AgentStatus::Idle);

        agent.process_task("Simple task").await.unwrap();

        assert_eq!(agent.state().status, AgentStatus::Complete);

        let statuses = callback.status_changes().await;
        assert!(statuses.contains(&AgentStatus::Thinking));
        assert!(statuses.contains(&AgentStatus::Complete));
    }

    #[tokio::test]
    async fn test_agent_max_iterations() {
        let provider = Arc::new(MockLlmProvider::new());
        // Queue many tool calls to exhaust iterations
        for _ in 0..30 {
            provider.queue_response(MockLlmProvider::tool_call_response(
                "echo",
                serde_json::json!({"text": "loop"}),
            ));
        }

        let (mut agent, _callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("echoed")) })),
        });

        let result = agent.process_task("Infinite loop test").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            RustantError::Agent(AgentError::MaxIterationsReached { max }) => {
                assert_eq!(max, 25);
            }
            e => panic!("Expected MaxIterationsReached, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_agent_cancellation() {
        let provider = Arc::new(MockLlmProvider::new());
        // Queue a tool call response so the agent enters the loop
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "test"}),
        ));

        let (mut agent, _callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("echoed")) })),
        });

        // Cancel before processing
        agent.cancel();
        let result = agent.process_task("Cancelled task").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            RustantError::Agent(AgentError::Cancelled) => {}
            e => panic!("Expected Cancelled, got: {:?}", e),
        }
    }

    #[test]
    fn test_no_op_callback() {
        // Just ensure it compiles and doesn't panic
        let _callback = NoOpCallback;
    }

    #[tokio::test]
    async fn test_agent_streaming_mode() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::text_response("streaming response"));

        let callback = Arc::new(RecordingCallback::new());
        let mut config = AgentConfig::default();
        config.llm.use_streaming = true;

        let mut agent = Agent::new(provider, config, callback.clone());
        let result = agent.process_task("Test streaming").await.unwrap();

        assert!(result.success);
        assert!(result.response.contains("streaming"));
        // Streaming should have triggered on_token callbacks
        // (MockLlmProvider splits on whitespace)
    }

    #[tokio::test]
    async fn test_recording_callback() {
        let callback = RecordingCallback::new();
        callback.on_assistant_message("hello").await;
        callback
            .on_tool_start("file_read", &serde_json::json!({}))
            .await;
        callback.on_status_change(AgentStatus::Thinking).await;

        assert_eq!(callback.messages().await, vec!["hello"]);
        assert_eq!(callback.tool_calls().await, vec!["file_read"]);
        assert_eq!(callback.status_changes().await, vec![AgentStatus::Thinking]);
    }

    // --- Gap 1: Explanation emission tests ---

    #[tokio::test]
    async fn test_recording_callback_records_explanations() {
        let callback = RecordingCallback::new();
        let explanation = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "echo".into(),
        })
        .build();
        callback.on_decision_explanation(&explanation).await;

        let explanations = callback.explanations().await;
        assert_eq!(explanations.len(), 1);
        match &explanations[0].decision_type {
            DecisionType::ToolSelection { selected_tool } => {
                assert_eq!(selected_tool, "echo");
            }
            other => panic!("Expected ToolSelection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_multipart_tool_call_emits_explanation() {
        let provider = Arc::new(MockLlmProvider::new());

        // First response: multipart (text + tool call)
        provider.queue_response(MockLlmProvider::multipart_response(
            "I'll echo for you",
            "echo",
            serde_json::json!({"text": "test"}),
        ));
        // Second response after tool result: text
        provider.queue_response(MockLlmProvider::text_response("Done."));

        let (mut agent, callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo input text".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"]
                }),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|args: serde_json::Value| {
                Box::pin(async move {
                    let text = args["text"].as_str().unwrap_or("no text");
                    Ok(ToolOutput::text(format!("Echo: {}", text)))
                })
            }),
        });

        agent.process_task("Echo test").await.unwrap();

        let explanations = callback.explanations().await;
        assert!(
            !explanations.is_empty(),
            "MultiPart tool calls should emit explanations"
        );
        // Verify the explanation is for the echo tool
        let has_echo = explanations.iter().any(|e| {
            matches!(&e.decision_type, DecisionType::ToolSelection { selected_tool } if selected_tool == "echo")
        });
        assert!(has_echo, "Should have explanation for echo tool selection");
    }

    #[tokio::test]
    async fn test_single_tool_call_emits_explanation() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "hi"}),
        ));
        provider.queue_response(MockLlmProvider::text_response("Done."));

        let (mut agent, callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("echoed")) })),
        });

        agent.process_task("Echo test").await.unwrap();

        let explanations = callback.explanations().await;
        assert!(
            !explanations.is_empty(),
            "Single tool calls should emit explanations"
        );
    }

    #[tokio::test]
    async fn test_contract_violation_emits_error_recovery_explanation() {
        use crate::safety::{Invariant, Predicate, SafetyContract};

        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "test"}),
        ));
        // After the contract violation error, LLM responds with text
        provider.queue_response(MockLlmProvider::text_response("OK, I'll skip that."));

        let callback = Arc::new(RecordingCallback::new());
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, config, callback.clone());
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("echoed")) })),
        });

        // Set a contract that blocks all tools
        agent.safety_mut().set_contract(SafetyContract {
            name: "deny-all".into(),
            invariants: vec![Invariant {
                description: "no tools allowed".into(),
                predicate: Predicate::AlwaysFalse,
            }],
            ..Default::default()
        });

        agent.process_task("Echo test").await.unwrap();

        let explanations = callback.explanations().await;
        let has_error_recovery = explanations.iter().any(|e| {
            matches!(
                &e.decision_type,
                DecisionType::ErrorRecovery { error, .. } if error.contains("Contract violation")
            )
        });
        assert!(
            has_error_recovery,
            "Contract violations should emit ErrorRecovery explanations, got: {:?}",
            explanations
                .iter()
                .map(|e| &e.decision_type)
                .collect::<Vec<_>>()
        );
    }

    // --- Gap 4: Budget warning tests ---

    #[tokio::test]
    async fn test_recording_callback_records_budget_warnings() {
        let callback = RecordingCallback::new();
        callback
            .on_budget_warning(
                "Session cost at 85% of $1.00 limit",
                BudgetSeverity::Warning,
            )
            .await;
        callback
            .on_budget_warning("Budget exceeded!", BudgetSeverity::Exceeded)
            .await;

        let warnings = callback.budget_warnings().await;
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].0.contains("85%"));
        assert_eq!(warnings[0].1, BudgetSeverity::Warning);
        assert_eq!(warnings[1].1, BudgetSeverity::Exceeded);
    }

    #[test]
    fn test_budget_severity_enum() {
        assert_ne!(BudgetSeverity::Warning, BudgetSeverity::Exceeded);
        assert_eq!(BudgetSeverity::Warning, BudgetSeverity::Warning);
    }

    // --- Gap 3: ActionDetails parsing tests ---

    #[test]
    fn test_parse_action_details_file_read() {
        let args = serde_json::json!({"path": "src/lib.rs"});
        let details = Agent::parse_action_details("file_read", &args);
        match details {
            ActionDetails::FileRead { path } => {
                assert_eq!(path, std::path::PathBuf::from("src/lib.rs"));
            }
            other => panic!("Expected FileRead, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_action_details_file_list() {
        let args = serde_json::json!({"path": "src/"});
        let details = Agent::parse_action_details("file_list", &args);
        assert!(matches!(details, ActionDetails::FileRead { .. }));
    }

    #[test]
    fn test_parse_action_details_file_write() {
        let args = serde_json::json!({"path": "x.rs", "content": "hello"});
        let details = Agent::parse_action_details("file_write", &args);
        match details {
            ActionDetails::FileWrite { path, size_bytes } => {
                assert_eq!(path, std::path::PathBuf::from("x.rs"));
                assert_eq!(size_bytes, 5); // "hello".len()
            }
            other => panic!("Expected FileWrite, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_action_details_shell_exec() {
        let args = serde_json::json!({"command": "cargo test"});
        let details = Agent::parse_action_details("shell_exec", &args);
        match details {
            ActionDetails::ShellCommand { command } => {
                assert_eq!(command, "cargo test");
            }
            other => panic!("Expected ShellCommand, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_action_details_git_commit() {
        let args = serde_json::json!({"message": "fix bug"});
        let details = Agent::parse_action_details("git_commit", &args);
        match details {
            ActionDetails::GitOperation { operation } => {
                assert!(
                    operation.contains("commit"),
                    "Expected 'commit' in '{}'",
                    operation
                );
                assert!(
                    operation.contains("fix bug"),
                    "Expected 'fix bug' in '{}'",
                    operation
                );
            }
            other => panic!("Expected GitOperation, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_action_details_git_status() {
        let args = serde_json::json!({});
        let details = Agent::parse_action_details("git_status", &args);
        assert!(matches!(details, ActionDetails::GitOperation { .. }));
    }

    #[test]
    fn test_parse_action_details_unknown_falls_back() {
        let args = serde_json::json!({"foo": "bar"});
        let details = Agent::parse_action_details("custom_tool", &args);
        assert!(matches!(details, ActionDetails::Other { .. }));
    }

    #[test]
    fn test_build_approval_context_file_write_has_reasoning() {
        let details = ActionDetails::FileWrite {
            path: "test.rs".into(),
            size_bytes: 100,
        };
        let ctx = Agent::build_approval_context("file_write", &details, RiskLevel::Write);
        assert!(
            ctx.reasoning.is_some(),
            "FileWrite should produce reasoning"
        );
        let reasoning = ctx.reasoning.unwrap();
        assert!(
            reasoning.contains("100 bytes"),
            "Reasoning should mention size: {}",
            reasoning
        );
        assert!(
            !ctx.consequences.is_empty(),
            "FileWrite should have consequences"
        );
    }

    #[test]
    fn test_build_approval_context_shell_command_has_reasoning() {
        let details = ActionDetails::ShellCommand {
            command: "rm -rf /tmp/test".to_string(),
        };
        let ctx = Agent::build_approval_context("shell_exec", &details, RiskLevel::Execute);
        assert!(ctx.reasoning.is_some());
        let reasoning = ctx.reasoning.unwrap();
        assert!(reasoning.contains("rm -rf"));
    }

    // --- Gap 5: Corrections/Facts production tests ---

    /// A test callback that denies specific tools but approves all others.
    struct SelectiveDenyCallback {
        deny_tools: Vec<String>,
    }

    impl SelectiveDenyCallback {
        fn new(deny_tools: Vec<String>) -> Self {
            Self { deny_tools }
        }
    }

    #[async_trait::async_trait]
    impl AgentCallback for SelectiveDenyCallback {
        async fn on_assistant_message(&self, _message: &str) {}
        async fn on_token(&self, _token: &str) {}
        async fn request_approval(&self, action: &ActionRequest) -> ApprovalDecision {
            if self.deny_tools.contains(&action.tool_name) {
                ApprovalDecision::Deny
            } else {
                ApprovalDecision::Approve
            }
        }
        async fn on_tool_start(&self, _tool_name: &str, _args: &serde_json::Value) {}
        async fn on_tool_result(&self, _tool_name: &str, _output: &ToolOutput, _duration_ms: u64) {}
        async fn on_status_change(&self, _status: AgentStatus) {}
        async fn on_usage_update(&self, _usage: &TokenUsage, _cost: &CostEstimate) {}
        async fn on_decision_explanation(&self, _explanation: &DecisionExplanation) {}
    }

    #[tokio::test]
    async fn test_successful_tool_execution_records_fact() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "important finding about the code"}),
        ));
        provider.queue_response(MockLlmProvider::text_response("Done."));

        let (mut agent, _callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo text".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|args: serde_json::Value| {
                Box::pin(async move {
                    let text = args["text"].as_str().unwrap_or("no text");
                    Ok(ToolOutput::text(format!("Echo: {}", text)))
                })
            }),
        });

        agent.process_task("Test echo").await.unwrap();

        assert!(
            !agent.memory().long_term.facts.is_empty(),
            "Successful tool execution should record a fact"
        );
        let fact = &agent.memory().long_term.facts[0];
        assert!(
            fact.content.contains("echo"),
            "Fact should mention tool name: {}",
            fact.content
        );
        assert!(
            fact.tags.contains(&"tool_result".to_string()),
            "Fact should have 'tool_result' tag"
        );
    }

    #[tokio::test]
    async fn test_short_tool_output_not_recorded() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "x"}),
        ));
        provider.queue_response(MockLlmProvider::text_response("Done."));

        let (mut agent, _callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            // Return very short output (< 10 chars)
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("ok")) })),
        });

        agent.process_task("Test").await.unwrap();

        assert!(
            agent.memory().long_term.facts.is_empty(),
            "Short tool output (<10 chars) should NOT be recorded as fact"
        );
    }

    #[tokio::test]
    async fn test_huge_tool_output_not_recorded() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "x"}),
        ));
        provider.queue_response(MockLlmProvider::text_response("Done."));

        let (mut agent, _callback) = create_test_agent(provider);
        let huge = "x".repeat(10_000);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(move |_| {
                let h = huge.clone();
                Box::pin(async move { Ok(ToolOutput::text(h)) })
            }),
        });

        agent.process_task("Test").await.unwrap();

        assert!(
            agent.memory().long_term.facts.is_empty(),
            "Huge tool output (>5000 chars) should NOT be recorded as fact"
        );
    }

    #[tokio::test]
    async fn test_user_denial_records_correction() {
        let provider = Arc::new(MockLlmProvider::new());
        // First: try a write tool (will require approval, gets denied)
        provider.queue_response(MockLlmProvider::tool_call_response(
            "file_write",
            serde_json::json!({"path": "test.rs", "content": "bad code"}),
        ));
        // After denial error, agent falls back to text
        provider.queue_response(MockLlmProvider::text_response("Understood, I won't write."));

        let callback = Arc::new(SelectiveDenyCallback::new(vec!["file_write".to_string()]));
        let mut config = AgentConfig::default();
        // Use Paranoid mode so ALL actions require approval
        config.safety.approval_mode = crate::config::ApprovalMode::Paranoid;

        let mut agent = Agent::new(provider, config, callback);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "file_write".to_string(),
                description: "Write file".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::Write,
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("written")) })),
        });

        agent.process_task("Write something").await.unwrap();

        assert!(
            !agent.memory().long_term.corrections.is_empty(),
            "User denial should record a correction"
        );
        let correction = &agent.memory().long_term.corrections[0];
        assert!(
            correction.original.contains("file_write"),
            "Correction original should mention denied tool: {}",
            correction.original
        );
        assert!(
            correction.context.contains("denied"),
            "Correction context should mention denial: {}",
            correction.context
        );
    }
}
