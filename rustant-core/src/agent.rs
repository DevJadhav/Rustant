//! Agent orchestrator implementing the Think → Act → Observe event loop.
//!
//! The `Agent` struct ties together the Brain, ToolRegistry, Memory, and Safety
//! Guardian to autonomously execute tasks through LLM-powered reasoning.

use crate::brain::{Brain, LlmProvider};
use crate::config::{AgentConfig, MessagePriority};
use crate::error::{AgentError, LlmError, RustantError, ToolError};
use crate::explanation::{DecisionExplanation, DecisionType, ExplanationBuilder, FactorInfluence};
use crate::memory::MemorySystem;
use crate::safety::{
    ActionDetails, ActionRequest, ApprovalContext, ApprovalDecision, ContractCheckResult,
    PermissionResult, ReversibilityInfo, SafetyGuardian,
};
use crate::scheduler::{CronScheduler, HeartbeatManager, JobManager};
use crate::summarizer::ContextSummarizer;
use crate::types::{
    AgentState, AgentStatus, CompletionResponse, Content, CostEstimate, Message, ProgressUpdate,
    RiskLevel, Role, StreamEvent, TaskClassification, TokenUsage, ToolDefinition, ToolOutput,
};
use std::collections::{HashMap, HashSet};
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
        /// Actionable hint for the user (e.g. "Use /compact to compress context").
        hint: String,
    },
    /// Context usage is critical (>= 90%).
    Critical {
        usage_percent: u8,
        total_tokens: usize,
        context_window: usize,
        /// Actionable hint for the user.
        hint: String,
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

    /// Called at the start of each ReAct loop iteration with the current iteration
    /// number and the configured maximum. Used by the TUI sidebar to show live progress.
    /// Default is a no-op for backward compatibility.
    async fn on_iteration_start(&self, _iteration: usize, _max_iterations: usize) {}

    /// Called before an LLM call with estimated token count and cost.
    /// Only called when estimated cost exceeds $0.05 to avoid noise.
    /// Default is a no-op for backward compatibility.
    async fn on_cost_prediction(&self, _estimated_tokens: usize, _estimated_cost: f64) {}

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

    // --- Plan mode callbacks ---

    /// Called when plan generation starts.
    /// Default is a no-op for backward compatibility.
    async fn on_plan_generating(&self, _goal: &str) {}

    /// Called when a plan is ready for user review.
    /// Returns the user's decision on the plan.
    /// Default auto-approves for backward compatibility.
    async fn on_plan_review(
        &self,
        _plan: &crate::plan::ExecutionPlan,
    ) -> crate::plan::PlanDecision {
        crate::plan::PlanDecision::Approve
    }

    /// Called when a plan step starts executing.
    /// Default is a no-op for backward compatibility.
    async fn on_plan_step_start(&self, _step_index: usize, _step: &crate::plan::PlanStep) {}

    /// Called when a plan step finishes (success or failure).
    /// Default is a no-op for backward compatibility.
    async fn on_plan_step_complete(&self, _step_index: usize, _step: &crate::plan::PlanStep) {}
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
    /// Lazily initialized on first use to avoid constructing an LLM-backed
    /// summarizer when the agent never triggers context compression.
    summarizer: Option<ContextSummarizer>,
    /// Token budget manager for cost control.
    budget: crate::brain::TokenBudgetManager,
    /// Cross-session knowledge distiller for learning from corrections/facts.
    knowledge: crate::memory::KnowledgeDistiller,
    /// Per-tool token usage tracking for budget breakdown.
    tool_token_usage: HashMap<String, usize>,
    /// Optional cron scheduler for time-based task triggers.
    cron_scheduler: Option<CronScheduler>,
    /// Optional heartbeat manager for periodic task triggers.
    heartbeat_manager: Option<HeartbeatManager>,
    /// Background job manager for long-running tasks.
    job_manager: JobManager,
    /// Consecutive failure tracker: (tool_name, failure_count).
    /// Resets when a different tool succeeds or a different tool is called.
    consecutive_failures: (String, usize),
    /// Recent decision explanations for transparency (capped at 50).
    recent_explanations: std::collections::VecDeque<DecisionExplanation>,
    /// Optional output redactor for stripping secrets before storage.
    /// Set by CLI when security features are enabled.
    output_redactor: Option<crate::redact::SharedRedactor>,
    /// Whether plan mode is active (generate plan before executing).
    plan_mode: bool,
    /// The current plan being generated, reviewed, or executed.
    current_plan: Option<crate::plan::ExecutionPlan>,
    /// Adaptive persona resolver for modulating agent behavior.
    persona_resolver: Option<crate::personas::PersonaResolver>,
    /// Most recent task classification (for persona auto-detection).
    last_classification: Option<crate::types::TaskClassification>,
    /// Cached tool definitions keyed by classification, avoiding repeated clones.
    cached_tool_defs: HashMap<Option<crate::types::TaskClassification>, Arc<Vec<ToolDefinition>>>,
    /// Optional MoE router for expert-based tool filtering.
    moe_router: Option<crate::moe::MoeRouter>,
    /// Agent decision log for interpretability (tracks reasoning behind decisions).
    decision_log: crate::decision_log::DecisionLog,
    /// Data flow tracker for transparency (records data movements through the agent).
    data_flow_tracker: crate::data_flow::DataFlowTracker,
    /// Optional consent manager for tracking user consent per scope (provider, storage, etc.).
    consent_manager: Option<crate::consent::ConsentManager>,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        config: AgentConfig,
        callback: Arc<dyn AgentCallback>,
    ) -> Self {
        let mut brain = Brain::new(provider, crate::brain::DEFAULT_SYSTEM_PROMPT);

        // Bridge rate limits from config → Brain's rate limiter
        if let Some(ref limits) = config.llm.rate_limits {
            let rl_config = crate::providers::rate_limiter::RateLimitConfig {
                itpm: limits.input_tokens_per_minute,
                otpm: limits.output_tokens_per_minute,
                rpm: limits.requests_per_minute,
            };
            if rl_config.itpm > 0 || rl_config.otpm > 0 || rl_config.rpm > 0 {
                brain.set_rate_limits(rl_config);
                debug!(
                    itpm = limits.input_tokens_per_minute,
                    otpm = limits.output_tokens_per_minute,
                    rpm = limits.requests_per_minute,
                    "Initialized rate limiter from config"
                );
            }
        }

        let memory = MemorySystem::new(config.memory.window_size);
        let safety = SafetyGuardian::new(config.safety.clone());
        let max_iter = config.safety.max_iterations;
        let budget = crate::brain::TokenBudgetManager::new(config.budget.as_ref());
        let knowledge = crate::memory::KnowledgeDistiller::new(config.knowledge.as_ref());

        let cron_scheduler = config.scheduler.as_ref().and_then(|sc| {
            if sc.enabled {
                let mut scheduler = CronScheduler::new();
                for job_config in &sc.cron_jobs {
                    if let Err(e) = scheduler.add_job(job_config.clone()) {
                        warn!("Failed to add cron job '{}': {}", job_config.name, e);
                    }
                }
                Some(scheduler)
            } else {
                None
            }
        });
        let heartbeat_manager = config.scheduler.as_ref().and_then(|sc| {
            sc.heartbeat
                .as_ref()
                .map(|hb| HeartbeatManager::new(hb.clone()))
        });
        let max_bg_jobs = config
            .scheduler
            .as_ref()
            .map(|sc| sc.max_background_jobs)
            .unwrap_or(10);
        let job_manager = JobManager::new(max_bg_jobs);
        let plan_mode_enabled = config.plan.as_ref().map(|p| p.enabled).unwrap_or(false);

        // Initialize persona resolver if enabled
        let persona_resolver = {
            let persona_config = config.persona.as_ref();
            let disabled = persona_config.as_ref().is_some_and(|p| !p.enabled);
            if disabled {
                None
            } else {
                Some(crate::personas::PersonaResolver::new(persona_config))
            }
        };

        // Initialize MoE router if configured and enabled
        let moe_router = config
            .moe
            .as_ref()
            .filter(|m| m.enabled)
            .map(|m| crate::moe::MoeRouter::new(m.clone()));

        // Initialize consent manager if consent framework is enabled
        let consent_manager = config.consent.as_ref().filter(|c| c.enabled).map(|c| {
            let policy = if c.require_explicit_provider_consent {
                crate::consent::DefaultConsentPolicy::RequireExplicit
            } else {
                crate::consent::DefaultConsentPolicy::ImpliedGrant
            };
            // Try to load persisted consent records from ~/.rustant/consent.json
            let persist_path = std::env::var("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_default()
                .join(".rustant")
                .join("consent.json");
            let mut mgr = crate::consent::ConsentManager::with_persistence(persist_path);
            mgr.default_policy = policy;
            let _ = mgr.load();
            mgr
        });

        Self {
            brain,
            memory,
            safety,
            tools: HashMap::new(),
            state: AgentState::new(max_iter),
            config,
            cancellation: CancellationToken::new(),
            callback,
            summarizer: None,
            budget,
            knowledge,
            tool_token_usage: HashMap::new(),
            cron_scheduler,
            heartbeat_manager,
            job_manager,
            consecutive_failures: (String::new(), 0),
            recent_explanations: std::collections::VecDeque::new(),
            output_redactor: None,
            plan_mode: plan_mode_enabled,
            current_plan: None,
            persona_resolver,
            last_classification: None,
            cached_tool_defs: HashMap::new(),
            moe_router,
            decision_log: crate::decision_log::DecisionLog::new(),
            data_flow_tracker: crate::data_flow::DataFlowTracker::new(),
            consent_manager,
        }
    }

    /// Register a tool with the agent.
    pub fn register_tool(&mut self, tool: RegisteredTool) {
        self.tools.insert(tool.definition.name.clone(), tool);
        self.invalidate_tool_def_cache();
    }

    /// Map a task classification to the set of tool names relevant for that task.
    ///
    /// Returns `None` for `General` and `Workflow(_)` classifications, meaning
    /// all tools should be sent.  For specific classifications, returns a set
    /// containing core tools (always needed) plus task-specific tools, reducing
    /// the number of tool definitions sent per LLM request from ~67 to ~12-25.
    fn tools_for_classification(
        classification: &TaskClassification,
    ) -> Option<HashSet<&'static str>> {
        // Core tools — always included regardless of classification
        let core: [&str; 10] = [
            "ask_user",
            "echo",
            "datetime",
            "calculator",
            "shell_exec",
            "file_read",
            "file_write",
            "file_list",
            "file_search",
            "web_search",
        ];

        let extra: &[&str] = match classification {
            TaskClassification::General
            | TaskClassification::Workflow(_)
            | TaskClassification::DeepResearch => return None,
            TaskClassification::FileOperation => &[
                "file_patch",
                "smart_edit",
                "codebase_search",
                "document_read",
            ],
            TaskClassification::GitOperation => &[
                "git_status",
                "git_diff",
                "git_commit",
                "file_patch",
                "smart_edit",
            ],
            TaskClassification::CodeAnalysis => &[
                "code_intelligence",
                "codebase_search",
                "smart_edit",
                "git_status",
                "git_diff",
            ],
            TaskClassification::Search => &["codebase_search", "web_fetch", "smart_edit"],
            TaskClassification::WebSearch => &["web_fetch"],
            TaskClassification::WebFetch => &["web_fetch", "http_api"],
            TaskClassification::Calendar => &["macos_calendar", "macos_notification"],
            TaskClassification::Reminders => &["macos_reminders", "macos_notification"],
            TaskClassification::Notes => &["macos_notes"],
            TaskClassification::Email => &["macos_mail", "macos_notification"],
            TaskClassification::Music => &["macos_music"],
            TaskClassification::AppControl => &[
                "macos_app_control",
                "macos_gui_scripting",
                "macos_accessibility",
                "macos_screen_analyze",
            ],
            TaskClassification::Clipboard => &["macos_clipboard"],
            TaskClassification::Screenshot => &["macos_screenshot"],
            TaskClassification::SystemInfo => &["macos_system_info"],
            TaskClassification::Contacts => &["macos_contacts", "imessage_contacts"],
            TaskClassification::Safari => &["macos_safari", "web_fetch"],
            TaskClassification::HomeKit => &["homekit"],
            TaskClassification::Photos => &["photos"],
            TaskClassification::Voice => &["macos_say"],
            TaskClassification::Meeting => &[
                "macos_meeting_recorder",
                "macos_notes",
                "macos_notification",
            ],
            TaskClassification::DailyBriefing => &[
                "macos_daily_briefing",
                "macos_calendar",
                "macos_reminders",
                "macos_mail",
                "macos_notes",
            ],
            TaskClassification::GuiScripting => &[
                "macos_gui_scripting",
                "macos_accessibility",
                "macos_screen_analyze",
                "macos_app_control",
            ],
            TaskClassification::Accessibility => &[
                "macos_accessibility",
                "macos_gui_scripting",
                "macos_screen_analyze",
            ],
            TaskClassification::Browser => &[
                "browser_navigate",
                "browser_click",
                "browser_type",
                "browser_screenshot",
                "web_fetch",
            ],
            TaskClassification::Messaging => {
                &["imessage_read", "imessage_send", "imessage_contacts"]
            }
            TaskClassification::Slack => &["slack"],
            TaskClassification::ArxivResearch => {
                &["arxiv_research", "knowledge_graph", "web_fetch"]
            }
            TaskClassification::KnowledgeGraph => &["knowledge_graph"],
            TaskClassification::ExperimentTracking => &["experiment_tracker"],
            TaskClassification::CodeIntelligence => {
                &["code_intelligence", "codebase_search", "smart_edit"]
            }
            TaskClassification::ContentEngine => &["content_engine"],
            TaskClassification::SkillTracker => &["skill_tracker"],
            TaskClassification::CareerIntel => &["career_intel"],
            TaskClassification::SystemMonitor => &["system_monitor"],
            TaskClassification::LifePlanner => &["life_planner"],
            TaskClassification::PrivacyManager => &["privacy_manager"],
            TaskClassification::SelfImprovement => &["self_improvement"],
            TaskClassification::Notification => &["macos_notification"],
            TaskClassification::Spotlight => &["macos_spotlight"],
            TaskClassification::FocusMode => &["macos_focus_mode"],
            TaskClassification::Finder => &["macos_finder"],
        };

        let mut set: HashSet<&str> = core.into_iter().collect();
        set.extend(extra.iter().copied());
        Some(set)
    }

    /// Get tool definitions for the LLM, optionally filtered by task classification.
    ///
    /// When a classification is provided (and is not `General`/`Workflow`), only
    /// tools relevant to that task type are returned, reducing token overhead by
    /// 60-82% (~25K-35K tokens down to ~5K-12K).
    pub fn tool_definitions(
        &self,
        classification: Option<&TaskClassification>,
    ) -> Vec<ToolDefinition> {
        // Check the cache first — avoids re-cloning all tool definitions per LLM call.
        let cache_key = classification.cloned();
        if let Some(cached) = self.cached_tool_defs.get(&cache_key) {
            return (**cached).clone();
        }

        let allowed = classification.and_then(Self::tools_for_classification);

        let mut defs: Vec<ToolDefinition> = if let Some(ref allowed_set) = allowed {
            self.tools
                .values()
                .filter(|t| allowed_set.contains(t.definition.name.as_str()))
                .map(|t| t.definition.clone())
                .collect()
        } else {
            self.tools.values().map(|t| t.definition.clone()).collect()
        };

        let tool_count = defs.len();
        let total_registered = self.tools.len();
        if tool_count < total_registered {
            debug!(
                filtered = tool_count,
                total = total_registered,
                classification = ?classification,
                "Filtered tool definitions for LLM request"
            );
        }

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

    /// Warm the tool definition cache for the current classification.
    /// Call this after tools are registered or when classification changes.
    pub fn warm_tool_def_cache(&mut self, classification: Option<&TaskClassification>) {
        let cache_key = classification.cloned();
        let defs = {
            let allowed = classification.and_then(Self::tools_for_classification);
            let mut defs: Vec<ToolDefinition> = if let Some(ref allowed_set) = allowed {
                self.tools
                    .values()
                    .filter(|t| allowed_set.contains(t.definition.name.as_str()))
                    .map(|t| t.definition.clone())
                    .collect()
            } else {
                self.tools.values().map(|t| t.definition.clone()).collect()
            };
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
        };
        self.cached_tool_defs.insert(cache_key, Arc::new(defs));
    }

    /// Invalidate the tool definition cache (e.g., when tools are added/removed).
    pub fn invalidate_tool_def_cache(&mut self) {
        self.cached_tool_defs.clear();
    }

    /// Process a user task through the agent loop.
    pub async fn process_task(&mut self, task: &str) -> Result<TaskResult, RustantError> {
        // Plan mode: generate and review plan before executing
        if self.plan_mode {
            return self.process_task_with_plan(task).await;
        }

        let task_id = Uuid::new_v4();
        info!(task_id = %task_id, task = task, "Starting task processing");

        self.state.start_task(task);
        self.state.task_id = Some(task_id);
        self.memory.start_new_task(task);
        self.budget.reset_task();
        self.tool_token_usage.clear();

        // Run knowledge distillation from long-term memory and inject into brain
        self.knowledge.distill(&self.memory.long_term);
        let mut knowledge_addendum = self.knowledge.rules_for_prompt();

        // Inject a tool-routing hint based on the cached task classification.
        // Appended to the knowledge addendum (system prompt) instead of persisted
        // in memory, so it never gets displaced by compression and can't end up
        // between tool_call and tool_result messages.
        if let Some(ref classification) = self.state.task_classification {
            // Inject persona addendum based on task classification
            if let Some(ref resolver) = self.persona_resolver {
                let addendum = resolver.prompt_addendum(Some(classification));
                if !addendum.is_empty() {
                    knowledge_addendum.push_str("\n\n");
                    knowledge_addendum.push_str(&addendum);
                }
            }
            self.last_classification = Some(classification.clone());

            // Inject a tool-routing hint
            if let Some(hint) = Self::tool_routing_hint_from_classification(classification) {
                knowledge_addendum.push_str("\n\n");
                knowledge_addendum.push_str(&hint);
            }
        }
        // Warm the tool definition cache for this classification so that
        // tool_definitions() (which is &self) can serve cached results.
        let classification_for_cache = self.state.task_classification.clone();
        self.warm_tool_def_cache(classification_for_cache.as_ref());

        // MoE: inject expert-specific system prompt addendum and precision hints
        if let Some(ref mut router) = self.moe_router {
            let route_result = router.route(task);
            debug!(
                expert = ?route_result.primary_expert(),
                experts = route_result.selected_experts.len(),
                tools = route_result.all_tool_names().len(),
                tokens = route_result.total_tool_tokens,
                cache_hit = route_result.cache_hit,
                "MoE sparse-routed task to expert(s)"
            );
            knowledge_addendum.push_str("\n\n");
            knowledge_addendum.push_str(&route_result.system_prompt_addendum);

            // Pass precision hints to Brain for Anthropic Tool Search integration.
            // Half/Quarter precision tools will be deferred (discovered on demand via
            // tool_search_tool), while Full precision tools stay in the prompt.
            let mut hints = std::collections::HashMap::new();
            for (tool_name, precision) in &route_result.routed_tools {
                hints.insert(tool_name.clone(), *precision);
            }
            // Mark shared tools as Full precision (never deferred)
            for tool_name in &route_result.shared_tools {
                hints.insert(tool_name.clone(), crate::moe::ToolPrecision::Full);
            }
            self.brain.set_tool_precision_hints(hints);

            // Expert-aware prompt stripping: remove irrelevant system prompt sections
            // based on the primary expert's domain (saves 500-1500 tokens per request).
            let primary = route_result.primary_expert();
            let exclusions = primary.system_prompt_exclusions();
            self.brain.apply_expert_prompt_exclusions(exclusions);

            // Log routing decision for interpretability (Pillar 5d)
            let alternatives: Vec<String> = crate::moe::ExpertId::all()
                .iter()
                .filter(|&&e| !route_result.selected_experts.iter().any(|(id, _)| id == &e))
                .take(3)
                .map(|e| e.display_name().to_string())
                .collect();
            let decision_id = self.decision_log.record(
                self.state.iteration,
                format!("moe_route → {}", primary.display_name()),
                &route_result.routing_reasoning,
                "low",
                crate::decision_log::DecisionOutcome::AutoApproved,
            );
            if let Some(entry) = self.decision_log.get_mut(decision_id) {
                entry.alternatives = alternatives;
                entry.expert = Some(primary.display_name().to_string());
                entry.confidence = route_result
                    .selected_experts
                    .first()
                    .map(|(_, score)| *score);
            }
        }

        // Hydration: inject relevant codebase context if configured
        if let Some(ref hydration_config) = self.config.hydration
            && hydration_config.enabled
        {
            let workspace = std::env::current_dir().unwrap_or_default();
            let pipeline = crate::hydration::HydrationPipeline::new(hydration_config.clone());
            if pipeline.should_hydrate(&workspace) {
                let result = pipeline.hydrate(&workspace, task);
                if !result.context_text.is_empty() {
                    knowledge_addendum.push_str("\n\n## Relevant Codebase Context\n");
                    knowledge_addendum.push_str(&result.context_text);
                    debug!(
                        files = result.file_count,
                        tokens = result.estimated_tokens,
                        "Hydration injected context"
                    );
                }
            }
        }

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

            // Notify about iteration progress for live UI updates
            self.callback
                .on_iteration_start(self.state.iteration, self.state.max_iterations)
                .await;

            // --- THINK ---
            self.state.status = AgentStatus::Thinking;
            self.callback.on_status_change(AgentStatus::Thinking).await;

            let conversation = self.memory.context_messages();
            let tools = Some(self.tool_definitions(self.state.task_classification.as_ref()));

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
                            hint: "Context nearly full — auto-compression imminent. Use /pin to protect important messages.".to_string(),
                        })
                        .await;
                } else if usage_percent >= 70 {
                    self.callback
                        .on_context_health(&ContextHealthEvent::Warning {
                            usage_percent,
                            total_tokens: breakdown.total_tokens,
                            context_window: breakdown.context_window,
                            hint: "Context filling up. Use /compact to compress now, or /pin to protect key messages.".to_string(),
                        })
                        .await;
                }
            }

            // Pre-call budget check (includes tool definition token overhead)
            let estimated_tokens = self
                .brain
                .estimate_tokens_with_tools(&conversation, tools.as_deref());
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
                        format!("{message}. Top consumers: {top}")
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
                        format!("{message}. Top consumers: {top}")
                    };
                    self.callback
                        .on_budget_warning(&enriched, BudgetSeverity::Warning)
                        .await;
                    debug!("Budget warning: {}", enriched);
                }
                crate::brain::BudgetCheckResult::Ok => {}
            }

            // Consent check: verify provider consent before sending data
            if let Some(ref consent_mgr) = self.consent_manager {
                let provider_name = self.brain.provider().model_name().to_string();
                let scope = crate::consent::ConsentScope::Provider {
                    provider: provider_name.clone(),
                };
                if !consent_mgr.check(&scope) {
                    debug!(
                        provider = provider_name,
                        "Provider consent not granted, auto-granting for session"
                    );
                    // Auto-grant for the session (backward-compatible behavior).
                    // In strict mode, this would prompt the user via ask_user.
                    if let Some(ref mut mgr) = self.consent_manager {
                        mgr.grant(
                            scope,
                            "Auto-granted for session",
                            Some(24), // 24-hour TTL
                        );
                        let _ = mgr.persist();
                    }
                }
            }

            // Cost prediction before LLM call
            {
                let est_tokens = estimated_tokens + 500; // +500 for expected response
                let est_cost = est_tokens as f64 * input_rate;
                if est_cost > 0.05 {
                    self.callback.on_cost_prediction(est_tokens, est_cost).await;
                }
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
                    ..Default::default()
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

                    self.handle_tool_call(id, name, arguments).await;

                    // Check context compression
                    self.check_and_compress().await;

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
                                self.handle_tool_call(id, name, arguments).await;
                            }
                            _ => {}
                        }
                    }

                    if !has_tool_call {
                        break; // Only text, we're done
                    }

                    // Check context compression after multipart tool calls
                    self.check_and_compress().await;

                    // Continue loop — agent needs to observe and think again
                }
                Content::ToolResult { .. } => {
                    // Shouldn't happen from LLM directly, but handle gracefully
                    warn!("Received unexpected ToolResult from LLM");
                    break;
                }
                Content::Thinking { thinking, .. } => {
                    // Thinking-only response: display and continue
                    info!(task_id = %task_id, "Agent produced thinking block");
                    self.callback
                        .on_assistant_message(&format!(
                            "[Thinking] {}",
                            &thinking[..thinking.len().min(200)]
                        ))
                        .await;
                    self.memory.add_message(response.message.clone());
                    // Thinking alone doesn't complete the task; continue loop
                }
                Content::Image { .. }
                | Content::Citation { .. }
                | Content::CodeExecution { .. }
                | Content::SearchResult { .. } => {
                    // Extended content types from LLM — treat as informational text
                    let summary = format!("{:?}", &response.message.content);
                    self.callback
                        .on_assistant_message(&summary[..summary.len().min(500)])
                        .await;
                    self.memory.add_message(response.message.clone());
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
    /// Includes retry logic with exponential backoff for transient errors
    /// (rate limits, timeouts, connection failures).
    async fn think_streaming(
        &mut self,
        conversation: &[Message],
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResponse, LlmError> {
        const MAX_RETRIES: usize = 3;
        let mut last_error: Option<LlmError> = None;

        for attempt in 0..=MAX_RETRIES {
            match self.think_streaming_once(conversation, tools.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) if Self::is_streaming_retryable(&e) => {
                    if attempt < MAX_RETRIES {
                        let backoff_secs = std::cmp::min(1u64 << attempt, 32);
                        let wait = match &e {
                            LlmError::RateLimited { retry_after_secs } => {
                                std::cmp::max(*retry_after_secs, backoff_secs)
                            }
                            _ => backoff_secs,
                        };
                        info!(
                            attempt = attempt + 1,
                            max_retries = MAX_RETRIES,
                            backoff_secs = wait,
                            error = %e,
                            "Retrying streaming after transient error"
                        );
                        self.callback
                            .on_token(&format!("\n[Retrying in {wait}s due to: {e}]\n"))
                            .await;
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_error.unwrap_or(LlmError::Connection {
            message: "Max streaming retries exceeded".to_string(),
        }))
    }

    /// Check if a streaming error is transient and should be retried.
    fn is_streaming_retryable(error: &LlmError) -> bool {
        if Brain::is_retryable(error) {
            return true;
        }
        // Streaming errors may wrap retryable conditions as strings
        if let LlmError::Streaming { message } = error {
            let msg = message.to_lowercase();
            return msg.contains("rate limit")
                || msg.contains("429")
                || msg.contains("timeout")
                || msg.contains("timed out")
                || msg.contains("connection")
                || msg.contains("temporarily unavailable")
                || msg.contains("503")
                || msg.contains("502");
        }
        false
    }

    /// Single attempt at streaming think — extracted for retry wrapping.
    async fn think_streaming_once(
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
            ..Default::default()
        };

        // Run the streaming completion in a background task so the producer
        // (complete_streaming) and consumer (rx.recv loop) run concurrently.
        // Without this, awaiting complete_streaming drops the tx sender before
        // the consumer reads any events, resulting in empty text.
        let provider = self.brain.provider_arc();
        let producer = tokio::spawn(async move { provider.complete_streaming(request, tx).await });

        // Consume events from the channel concurrently with the producer
        let mut text_parts = String::new();
        let mut usage = TokenUsage::default();
        // Track streaming tool calls: id -> (name, accumulated_arguments)
        let mut tool_calls: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();
        let mut tool_call_order: Vec<String> = Vec::new(); // preserve order
        // Raw provider-specific function call data (e.g., Gemini thought_signature)
        let mut raw_function_calls: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(token) => {
                    self.callback.on_token(&token).await;
                    text_parts.push_str(&token);
                }
                StreamEvent::ToolCallStart {
                    id,
                    name,
                    raw_function_call,
                } => {
                    tool_call_order.push(id.clone());
                    tool_calls.insert(id.clone(), (name, String::new()));
                    if let Some(raw_fc) = raw_function_call {
                        raw_function_calls.insert(id, raw_fc);
                    }
                }
                StreamEvent::ToolCallDelta {
                    id,
                    arguments_delta,
                } => {
                    if let Some((_, args)) = tool_calls.get_mut(&id) {
                        args.push_str(&arguments_delta);
                    }
                }
                StreamEvent::ToolCallEnd { id: _ } => {
                    // Tool call complete — arguments are now fully accumulated
                }
                StreamEvent::ThinkingDelta(delta) => {
                    // Accumulate thinking text (displayed in verbose mode by callback)
                    self.callback.on_token(&format!("[Thinking] {delta}")).await;
                }
                StreamEvent::ThinkingComplete { .. } => {
                    // Thinking phase complete — no action needed for streaming
                }
                StreamEvent::CitationBlock(_) | StreamEvent::CodeExecutionResult { .. } => {
                    // These are handled at response parse time, not during streaming
                }
                StreamEvent::Done { usage: u } => {
                    usage = u;
                    break;
                }
                StreamEvent::Error(e) => {
                    return Err(LlmError::Streaming { message: e });
                }
            }
        }

        // Wait for the producer to finish and propagate errors
        producer.await.map_err(|e| LlmError::Streaming {
            message: format!("Streaming task panicked: {e}"),
        })??;

        // Track usage in brain
        self.brain.track_usage(&usage);

        // Build raw provider-specific parts (e.g., Gemini thought_signature) BEFORE
        // consuming text_parts, since we need to reference it for the raw parts array.
        let raw_parts_metadata = if !raw_function_calls.is_empty() {
            let mut raw_parts = Vec::new();
            if !text_parts.is_empty() {
                raw_parts.push(serde_json::json!({"text": &text_parts}));
            }
            for id in &tool_call_order {
                if let Some(raw_fc) = raw_function_calls.get(id) {
                    raw_parts.push(raw_fc.clone());
                }
            }
            Some(serde_json::Value::Array(raw_parts))
        } else {
            None
        };

        // Build the response content based on what was streamed
        let content = if !tool_call_order.is_empty() {
            // Use the first tool call (single tool call is most common)
            let first_id = &tool_call_order[0];
            if let Some((name, args_str)) = tool_calls.get(first_id) {
                let arguments: serde_json::Value =
                    serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                if text_parts.is_empty() {
                    Content::tool_call(first_id, name, arguments)
                } else {
                    Content::MultiPart {
                        parts: vec![
                            Content::text(&text_parts),
                            Content::tool_call(first_id, name, arguments),
                        ],
                    }
                }
            } else {
                Content::text(text_parts)
            }
        } else {
            Content::text(text_parts)
        };
        let finish_reason = if tool_call_order.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };

        let mut message = Message::new(Role::Assistant, content);

        // Attach raw provider-specific function call data (e.g., Gemini thought_signature)
        // so the provider can echo it back in subsequent requests.
        if let Some(raw_parts) = raw_parts_metadata {
            message = message.with_metadata("gemini_raw_parts", raw_parts);
        }

        Ok(CompletionResponse {
            message,
            usage,
            model: self.brain.model_name().to_string(),
            finish_reason: Some(finish_reason.to_string()),
            rate_limit_headers: None,
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

        // Look up the tool — extract risk_level once (Copy) to avoid repeated HashMap lookups.
        let risk_level = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound {
                name: tool_name.to_string(),
            })?
            .risk_level;

        // Build rich approval context from action details
        let details = Self::parse_action_details(tool_name, arguments);
        let approval_context = Self::build_approval_context(tool_name, &details, risk_level);

        // Build action request with rich context
        let action = SafetyGuardian::create_rich_action_request(
            tool_name,
            risk_level,
            format!("Execute tool: {tool_name}"),
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
                    error: format!("Permission denied for tool '{tool_name}'"),
                    strategy: "Returning error to LLM for re-planning".to_string(),
                });
                builder.add_reasoning_step(format!("Denied: {reason}"), None);
                builder.set_confidence(1.0);
                let explanation = builder.build();
                self.callback.on_decision_explanation(&explanation).await;
                self.record_explanation(explanation);

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
                            .add_session_allowlist(tool_name.to_string(), risk_level);
                        info!(
                            tool = tool_name,
                            risk = %risk_level,
                            "Added tool to session allowlist (approve all similar)"
                        );
                    }
                    ApprovalDecision::Deny => {
                        // Emit explanation for user denial decision
                        let mut builder = ExplanationBuilder::new(DecisionType::ErrorRecovery {
                            error: format!("User denied approval for tool '{tool_name}'"),
                            strategy: "Returning error to LLM for re-planning".to_string(),
                        });
                        builder.add_reasoning_step(
                            "User rejected the action in approval dialog".to_string(),
                            None,
                        );
                        builder.set_confidence(1.0);
                        let explanation = builder.build();
                        self.callback.on_decision_explanation(&explanation).await;
                        self.record_explanation(explanation);

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

        // Check safety contract pre-conditions (risk_level already extracted above)
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
                error: format!("Contract violation: {contract_result:?}"),
                strategy: "Returning error to LLM for re-planning".to_string(),
            });
            builder.set_confidence(1.0);
            let explanation = builder.build();
            self.callback.on_decision_explanation(&explanation).await;
            self.record_explanation(explanation);

            return Err(ToolError::PermissionDenied {
                name: tool_name.to_string(),
                reason: format!("Safety contract violation: {contract_result:?}"),
            });
        }

        // Execute the tool
        self.state.status = AgentStatus::Executing;
        self.callback.on_status_change(AgentStatus::Executing).await;
        self.callback.on_tool_start(tool_name, arguments).await;

        let start = Instant::now();

        // Borrow executor separately (all &mut self calls above have completed)
        let result = {
            let executor = &self.tools[tool_name].executor;
            (executor)(arguments.clone()).await
        };
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
                    // Redact secrets before storing in long-term memory
                    let redacted_summary = self.redact_output(&summary);
                    self.memory.long_term.add_fact(
                        crate::memory::Fact::new(
                            format!("Tool '{tool_name}' result: {redacted_summary}"),
                            format!("tool:{tool_name}"),
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

    /// Handle a single tool call: auto-correction, execution, observation, failure tracking,
    /// token usage, and verification hook. Extracted to eliminate duplication between the
    /// single-ToolCall and MultiPart code paths.
    async fn handle_tool_call(&mut self, id: &str, name: &str, arguments: &serde_json::Value) {
        // Build and emit decision explanation
        let explanation = self.build_decision_explanation(name, arguments);
        self.callback.on_decision_explanation(&explanation).await;
        self.record_explanation(explanation);

        // Auto-correction: reroute wrong tool calls
        let (actual_name, actual_args) =
            if let Some((cn, ca)) = Self::auto_correct_tool_call(name, arguments, &self.state) {
                if cn != *name {
                    info!(
                        original_tool = name,
                        corrected_tool = cn,
                        "Auto-routing to correct tool"
                    );
                    self.callback
                        .on_assistant_message(&format!("[Routed: {name} → {cn}]"))
                        .await;
                    (cn, ca)
                } else {
                    (name.to_string(), arguments.clone())
                }
            } else {
                (name.to_string(), arguments.clone())
            };

        // Execute tool
        let result = self.execute_tool(id, &actual_name, &actual_args).await;
        if let Err(ref e) = result {
            debug!(tool = %actual_name, error = %e, "Tool execution failed");
        }

        // Observe: store result in memory
        let result_tokens = match &result {
            Ok(output) => {
                let msg = Message::tool_result(id, &output.content, false);
                let tokens = output.content.len() / 4;
                self.memory.add_message(msg);
                tokens
            }
            Err(e) => {
                let error_msg = format!("Tool error: {e}");
                let tokens = error_msg.len() / 4;
                let msg = Message::tool_result(id, &error_msg, true);
                self.memory.add_message(msg);
                tokens
            }
        };
        *self.tool_token_usage.entry(name.to_string()).or_insert(0) += result_tokens;

        // Track consecutive failures for circuit breaker
        if result.is_err() {
            if self.consecutive_failures.0 == name {
                self.consecutive_failures.1 += 1;
            } else {
                self.consecutive_failures = (name.to_string(), 1);
            }
        } else {
            self.consecutive_failures = (String::new(), 0);
        }

        // Verification hook: after file writes, optionally run lint/test/typecheck
        if result.is_ok()
            && (actual_name == "file_write"
                || actual_name == "file_patch"
                || actual_name == "smart_edit")
            && let Some(ref verify_config) = self.config.verification
            && verify_config.run_on_file_write
        {
            let workspace = std::env::current_dir().unwrap_or_default();
            let verify_result =
                crate::verification::runner::run_verification(&workspace, verify_config).await;
            if !verify_result.passed {
                let feedback = crate::verification::feedback::format_feedback(&verify_result);
                let feedback_msg =
                    Message::tool_result(id, format!("[Verification failed]\n{feedback}"), true);
                self.memory.add_message(feedback_msg);
                debug!(
                    errors = verify_result.error_count(),
                    "Verification failed after file write"
                );
            }
        }
    }

    /// Record a decision explanation, capping at 50 entries.
    fn record_explanation(&mut self, explanation: DecisionExplanation) {
        if self.recent_explanations.len() >= 50 {
            self.recent_explanations.pop_front();
        }
        // Also log to the decision log for interpretability
        let iteration = self.state.iteration;
        let action = match &explanation.decision_type {
            crate::explanation::DecisionType::ToolSelection { selected_tool } => {
                selected_tool.clone()
            }
            crate::explanation::DecisionType::ParameterChoice { tool, parameter } => {
                format!("{tool}:{parameter}")
            }
            crate::explanation::DecisionType::TaskDecomposition { .. } => {
                "task_decomposition".to_string()
            }
            crate::explanation::DecisionType::ErrorRecovery { strategy, .. } => {
                format!("error_recovery:{strategy}")
            }
            crate::explanation::DecisionType::ModelSelection { selected_model, .. } => {
                format!("model_selection:{selected_model}")
            }
            crate::explanation::DecisionType::RetrievalStrategy { strategy, .. } => {
                format!("retrieval:{strategy}")
            }
            crate::explanation::DecisionType::SafetyOverride { rule, .. } => {
                format!("safety_override:{rule}")
            }
            crate::explanation::DecisionType::EvaluationJudgement { evaluator, .. } => {
                format!("eval:{evaluator}")
            }
        };
        let reasoning = explanation
            .reasoning_chain
            .iter()
            .map(|s| s.description.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        let risk_level_str = if explanation.confidence < 0.3 {
            "high"
        } else if explanation.confidence < 0.7 {
            "medium"
        } else {
            "low"
        };
        self.decision_log.record(
            iteration,
            &action,
            &reasoning,
            risk_level_str,
            crate::decision_log::DecisionOutcome::Pending,
        );
        self.recent_explanations.push_back(explanation);
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
                    .with_reasoning(format!("Executing shell command: {command}"))
                    .with_consequence("Shell command will run in the agent workspace".to_string());
                if risk_level >= RiskLevel::Execute {
                    ctx = ctx.with_consequence(
                        "Command may modify system state or produce side effects".to_string(),
                    );
                }
            }
            ActionDetails::NetworkRequest { host, method } => {
                ctx = ctx
                    .with_reasoning(format!("Making {method} request to {host}"))
                    .with_consequence(format!("Network request will be sent to {host}"));
            }
            ActionDetails::GitOperation { operation } => {
                ctx = ctx
                    .with_reasoning(format!("Git operation: {operation}"))
                    .with_reversibility(ReversibilityInfo {
                        is_reversible: true,
                        undo_description: Some(
                            "Git operations are generally reversible via reflog".to_string(),
                        ),
                        undo_window: None,
                    });
            }
            _ => {
                ctx = ctx.with_reasoning(format!("Executing {tool_name} tool"));
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
                    operation: format!("commit: {truncated}"),
                }
            }
            // macOS native tools
            "macos_calendar" | "macos_reminders" | "macos_notes" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list");
                let title = arguments
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                ActionDetails::Other {
                    info: format!("{tool_name} {action} {title}").trim().to_string(),
                }
            }
            "macos_app_control" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list_running");
                let app = arguments
                    .get("app_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                ActionDetails::ShellCommand {
                    command: format!("{action} {app}").trim().to_string(),
                }
            }
            "macos_clipboard" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("read");
                ActionDetails::Other {
                    info: format!("clipboard {action}"),
                }
            }
            "macos_screenshot" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("screenshot.png");
                ActionDetails::FileWrite {
                    path: path.into(),
                    size_bytes: 0,
                }
            }
            "macos_finder" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("reveal");
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                if action == "trash" {
                    ActionDetails::FileDelete { path: path.into() }
                } else {
                    ActionDetails::Other {
                        info: format!("Finder: {action} {path}"),
                    }
                }
            }
            "macos_notification" | "macos_system_info" | "macos_spotlight" => {
                ActionDetails::Other {
                    info: arguments
                        .as_object()
                        .map(|o| {
                            o.iter()
                                .map(|(k, v)| {
                                    format!("{}={}", k, v.as_str().unwrap_or(&v.to_string()))
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default(),
                }
            }
            "macos_mail" => {
                let action = arguments["action"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                if action == "send" {
                    let to = arguments["to"].as_str().unwrap_or("unknown").to_string();
                    let subject = arguments["subject"]
                        .as_str()
                        .unwrap_or("(no subject)")
                        .to_string();
                    ActionDetails::Other {
                        info: format!("SEND EMAIL to {to} — subject: {subject}"),
                    }
                } else {
                    ActionDetails::Other {
                        info: format!("macos_mail: {action}"),
                    }
                }
            }
            "macos_safari" => {
                let action = arguments["action"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                if action == "run_javascript" {
                    ActionDetails::ShellCommand {
                        command: format!(
                            "Safari JS: {}",
                            arguments["script"].as_str().unwrap_or("(unknown)")
                        ),
                    }
                } else if action == "navigate" {
                    ActionDetails::BrowserAction {
                        action: "navigate".to_string(),
                        url: arguments["url"].as_str().map(|s| s.to_string()),
                        selector: None,
                    }
                } else {
                    ActionDetails::Other {
                        info: format!("macos_safari: {action}"),
                    }
                }
            }
            "macos_screen_analyze" => {
                let action = arguments["action"].as_str().unwrap_or("ocr").to_string();
                let app = arguments["app_name"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "screen".to_string());
                ActionDetails::GuiAction {
                    app_name: app,
                    action,
                    element: None,
                }
            }
            "macos_contacts" => {
                let action = arguments["action"].as_str().unwrap_or("search").to_string();
                let query = arguments["query"]
                    .as_str()
                    .or_else(|| arguments["name"].as_str())
                    .map(|q| format!("'{q}'"))
                    .unwrap_or_default();
                ActionDetails::Other {
                    info: format!("Contacts: {action} {query}"),
                }
            }
            "macos_gui_scripting" | "macos_accessibility" => {
                let app_name = arguments["app_name"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let action = arguments["action"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let element = arguments["element_description"]
                    .as_str()
                    .map(|s| s.to_string());
                ActionDetails::GuiAction {
                    app_name,
                    action,
                    element,
                }
            }
            // Browser automation tools → BrowserAction for rich approval context.
            name if name.starts_with("browser_") => {
                let action = name.strip_prefix("browser_").unwrap_or(name).to_string();
                let url = arguments["url"].as_str().map(|s| s.to_string());
                let selector = arguments["selector"]
                    .as_str()
                    .or_else(|| arguments["ref"].as_str())
                    .map(|s| s.to_string());
                ActionDetails::BrowserAction {
                    action,
                    url,
                    selector,
                }
            }
            // Web tools → NetworkRequest for approval context.
            "web_search" | "web_fetch" => {
                let host = if tool_name == "web_search" {
                    "api.duckduckgo.com".to_string()
                } else {
                    // Extract hostname from URL for web_fetch
                    let url_str = arguments["url"].as_str().unwrap_or("unknown URL");
                    url_str
                        .strip_prefix("https://")
                        .or_else(|| url_str.strip_prefix("http://"))
                        .and_then(|s| s.split('/').next())
                        .unwrap_or(url_str)
                        .to_string()
                };
                ActionDetails::NetworkRequest {
                    host,
                    method: if tool_name == "web_search" {
                        "SEARCH".to_string()
                    } else {
                        "GET".to_string()
                    },
                }
            }
            // iMessage send → ChannelReply for approval gating.
            "imessage_send" => {
                let recipient = arguments["recipient"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let preview = arguments["message"]
                    .as_str()
                    .map(|s| {
                        if s.len() > 100 {
                            format!("{}...", &s[..97])
                        } else {
                            s.to_string()
                        }
                    })
                    .unwrap_or_default();
                ActionDetails::ChannelReply {
                    channel: "iMessage".to_string(),
                    recipient,
                    preview,
                    priority: MessagePriority::Normal,
                }
            }
            // Slack tool → ChannelReply for send/reply, Other for reads.
            "slack" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("send_message");
                match action {
                    "send_message" | "reply_thread" => {
                        let recipient = arguments["channel"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string();
                        let preview = arguments["message"]
                            .as_str()
                            .map(|s| {
                                if s.len() > 100 {
                                    format!("{}...", &s[..97])
                                } else {
                                    s.to_string()
                                }
                            })
                            .unwrap_or_default();
                        ActionDetails::ChannelReply {
                            channel: "Slack".to_string(),
                            recipient,
                            preview,
                            priority: MessagePriority::Normal,
                        }
                    }
                    "add_reaction" => ActionDetails::ChannelReply {
                        channel: "Slack".to_string(),
                        recipient: arguments["channel"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                        preview: format!(":{}:", arguments["emoji"].as_str().unwrap_or("?")),
                        priority: MessagePriority::Normal,
                    },
                    _ => ActionDetails::Other {
                        info: format!("slack:{action}"),
                    },
                }
            }
            // ArXiv research → NetworkRequest for search/fetch, FileWrite for save.
            "arxiv_research" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("search");
                match action {
                    "save" | "remove" | "collections" | "digest_config" | "reindex" => {
                        ActionDetails::FileWrite {
                            path: ".rustant/arxiv/library.json".into(),
                            size_bytes: 0,
                        }
                    }
                    "semantic_search" | "summarize" | "citation_graph" | "blueprint" => {
                        ActionDetails::NetworkRequest {
                            host: "api.semanticscholar.org".to_string(),
                            method: "GET".to_string(),
                        }
                    }
                    _ => ActionDetails::NetworkRequest {
                        host: "export.arxiv.org".to_string(),
                        method: "GET".to_string(),
                    },
                }
            }
            // Knowledge graph — write actions modify state file
            "knowledge_graph" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list");
                match action {
                    "add_node" | "update_node" | "remove_node" | "add_edge" | "remove_edge"
                    | "import_arxiv" => ActionDetails::FileWrite {
                        path: ".rustant/knowledge/graph.json".into(),
                        size_bytes: 0,
                    },
                    _ => ActionDetails::FileRead {
                        path: ".rustant/knowledge/graph.json".into(),
                    },
                }
            }
            // Experiment tracker — write actions modify state file
            "experiment_tracker" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list_experiments");
                match action {
                    "add_hypothesis"
                    | "update_hypothesis"
                    | "add_experiment"
                    | "start_experiment"
                    | "complete_experiment"
                    | "fail_experiment"
                    | "record_evidence" => ActionDetails::FileWrite {
                        path: ".rustant/experiments/tracker.json".into(),
                        size_bytes: 0,
                    },
                    _ => ActionDetails::FileRead {
                        path: ".rustant/experiments/tracker.json".into(),
                    },
                }
            }
            // Code intelligence — read-only analysis tool
            "code_intelligence" => {
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                ActionDetails::FileRead { path: path.into() }
            }
            // Content engine — write actions modify state file
            "content_engine" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list");
                match action {
                    "create" | "update" | "set_status" | "delete" | "schedule" | "calendar_add"
                    | "calendar_remove" => ActionDetails::FileWrite {
                        path: ".rustant/content/library.json".into(),
                        size_bytes: 0,
                    },
                    _ => ActionDetails::FileRead {
                        path: ".rustant/content/library.json".into(),
                    },
                }
            }
            // Skill tracker — write actions modify state file
            "skill_tracker" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list_skills");
                match action {
                    "add_skill" | "log_practice" | "learning_path" => ActionDetails::FileWrite {
                        path: ".rustant/skills/tracker.json".into(),
                        size_bytes: 0,
                    },
                    _ => ActionDetails::FileRead {
                        path: ".rustant/skills/tracker.json".into(),
                    },
                }
            }
            // Career intel — write actions modify state file
            "career_intel" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("progress_report");
                match action {
                    "set_goal" | "log_achievement" | "add_portfolio" | "network_note" => {
                        ActionDetails::FileWrite {
                            path: ".rustant/career/intel.json".into(),
                            size_bytes: 0,
                        }
                    }
                    _ => ActionDetails::FileRead {
                        path: ".rustant/career/intel.json".into(),
                    },
                }
            }
            // System monitor — health_check uses network, others modify state
            "system_monitor" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list_services");
                match action {
                    "health_check" => ActionDetails::NetworkRequest {
                        host: "service health check".to_string(),
                        method: "GET".to_string(),
                    },
                    "add_service" | "log_incident" => ActionDetails::FileWrite {
                        path: ".rustant/monitoring/topology.json".into(),
                        size_bytes: 0,
                    },
                    _ => ActionDetails::FileRead {
                        path: ".rustant/monitoring/topology.json".into(),
                    },
                }
            }
            // Life planner — write actions modify state file
            "life_planner" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("daily_plan");
                match action {
                    "set_energy_profile" | "add_deadline" | "log_habit" | "context_switch_log" => {
                        ActionDetails::FileWrite {
                            path: ".rustant/life/planner.json".into(),
                            size_bytes: 0,
                        }
                    }
                    _ => ActionDetails::FileRead {
                        path: ".rustant/life/planner.json".into(),
                    },
                }
            }
            // Privacy manager — delete_data is destructive, others vary
            "privacy_manager" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list_boundaries");
                match action {
                    "delete_data" => {
                        let domain = arguments
                            .get("domain")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        ActionDetails::FileDelete {
                            path: format!(".rustant/{domain}/").into(),
                        }
                    }
                    "set_boundary" | "encrypt_store" => ActionDetails::FileWrite {
                        path: ".rustant/privacy/config.json".into(),
                        size_bytes: 0,
                    },
                    _ => ActionDetails::FileRead {
                        path: ".rustant/privacy/config.json".into(),
                    },
                }
            }
            // Self-improvement — some actions write, others read
            "self_improvement" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("analyze_patterns");
                match action {
                    "set_preference" | "feedback" | "reset_baseline" => ActionDetails::FileWrite {
                        path: ".rustant/meta/improvement.json".into(),
                        size_bytes: 0,
                    },
                    _ => ActionDetails::FileRead {
                        path: ".rustant/meta/improvement.json".into(),
                    },
                }
            }
            // ML data pipeline tools → DataPipeline
            name if name.starts_with("ml_data_") => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let dataset = arguments
                    .get("dataset")
                    .or_else(|| arguments.get("dataset_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let contains_pii = arguments
                    .get("contains_pii")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                ActionDetails::DataPipeline {
                    action,
                    dataset,
                    contains_pii,
                }
            }
            // ML feature tools → DataPipeline
            name if name.starts_with("ml_feature_") => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let feature = arguments
                    .get("name")
                    .or_else(|| arguments.get("feature_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                ActionDetails::DataPipeline {
                    action,
                    dataset: feature,
                    contains_pii: false,
                }
            }
            // ML training tools → ModelTraining
            "ml_train" | "ml_experiment" | "ml_hyperparams" | "ml_checkpoint" | "ml_metrics" => {
                let framework = arguments
                    .get("framework")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let model_type = arguments
                    .get("model_type")
                    .or_else(|| arguments.get("model"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let dataset_id = arguments
                    .get("dataset_id")
                    .or_else(|| arguments.get("dataset"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                ActionDetails::ModelTraining {
                    framework,
                    model_type,
                    dataset_id,
                }
            }
            // Also match ml_train_* prefix variants (e.g. ml_train_distributed)
            name if name.starts_with("ml_train") => {
                let framework = arguments
                    .get("framework")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let model_type = arguments
                    .get("model_type")
                    .or_else(|| arguments.get("model"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let dataset_id = arguments
                    .get("dataset_id")
                    .or_else(|| arguments.get("dataset"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                ActionDetails::ModelTraining {
                    framework,
                    model_type,
                    dataset_id,
                }
            }
            // ML model management, finetune, quantize, eval, adapter, chat dataset → ModelInference
            "ml_model_registry" | "ml_model_export" | "ml_model_compare" | "ml_finetune"
            | "ml_quantize" | "ml_eval" | "ml_adapter" | "ml_chat_dataset" => {
                let model_name = arguments
                    .get("model_name")
                    .or_else(|| arguments.get("model"))
                    .or_else(|| arguments.get("model_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let backend = arguments
                    .get("backend")
                    .or_else(|| arguments.get("framework"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                ActionDetails::ModelInference {
                    model_name,
                    backend,
                    action,
                }
            }
            // Catch-all for ml_model_* prefix
            name if name.starts_with("ml_model_") => {
                let model_name = arguments
                    .get("model_name")
                    .or_else(|| arguments.get("model"))
                    .or_else(|| arguments.get("model_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let backend = arguments
                    .get("backend")
                    .or_else(|| arguments.get("framework"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                ActionDetails::ModelInference {
                    model_name,
                    backend,
                    action,
                }
            }
            // RAG tools → RagQuery
            name if name.starts_with("rag_") => {
                let query_type = arguments
                    .get("query_type")
                    .or_else(|| arguments.get("action"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let collection = arguments
                    .get("collection")
                    .or_else(|| arguments.get("index"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();
                let top_k = arguments
                    .get("top_k")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                ActionDetails::RagQuery {
                    query_type,
                    collection,
                    top_k,
                }
            }
            // Eval tools → EvaluationRun
            name if name.starts_with("eval_") => {
                let evaluator = arguments
                    .get("evaluator")
                    .or_else(|| arguments.get("metric"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let scope = arguments
                    .get("scope")
                    .or_else(|| arguments.get("dataset"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let traces_count = arguments
                    .get("traces_count")
                    .or_else(|| arguments.get("num_samples"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                ActionDetails::EvaluationRun {
                    evaluator,
                    scope,
                    traces_count,
                }
            }
            // Research tools → ResearchAction
            name if name.starts_with("research_") => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let source = arguments
                    .get("source")
                    .or_else(|| arguments.get("provider"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let query = arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                ActionDetails::ResearchAction {
                    action,
                    source,
                    query,
                }
            }
            // Inference tools → ModelInference
            name if name.starts_with("inference_") => {
                let model_name = arguments
                    .get("model_name")
                    .or_else(|| arguments.get("model"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let backend = arguments
                    .get("backend")
                    .or_else(|| arguments.get("runtime"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or(name.strip_prefix("inference_").unwrap_or("unknown"))
                    .to_string();
                ActionDetails::ModelInference {
                    model_name,
                    backend,
                    action,
                }
            }
            // AI tools — dispatch by sub-prefix
            name if name.starts_with("ai_") => {
                let suffix = name.strip_prefix("ai_").unwrap_or(name);
                if suffix.starts_with("safety")
                    || suffix.starts_with("pii")
                    || suffix.starts_with("bias")
                    || suffix.starts_with("alignment")
                {
                    // Safety/compliance AI tools → EvaluationRun
                    let evaluator = arguments
                        .get("evaluator")
                        .or_else(|| arguments.get("check_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(suffix)
                        .to_string();
                    let scope = arguments
                        .get("scope")
                        .or_else(|| arguments.get("target"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let traces_count = arguments
                        .get("traces_count")
                        .or_else(|| arguments.get("num_samples"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    ActionDetails::EvaluationRun {
                        evaluator,
                        scope,
                        traces_count,
                    }
                } else if suffix.starts_with("red_team")
                    || suffix.starts_with("adversarial")
                    || suffix.starts_with("provenance")
                {
                    // Adversarial/provenance AI tools → ModelInference
                    let model_name = arguments
                        .get("model_name")
                        .or_else(|| arguments.get("model"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let backend = arguments
                        .get("backend")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let action = arguments
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or(suffix)
                        .to_string();
                    ActionDetails::ModelInference {
                        model_name,
                        backend,
                        action,
                    }
                } else if suffix.starts_with("explain")
                    || suffix.starts_with("data_lineage")
                    || suffix.starts_with("source")
                {
                    // Explainability/lineage AI tools → ResearchAction
                    let action = arguments
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or(suffix)
                        .to_string();
                    let source = arguments
                        .get("source")
                        .or_else(|| arguments.get("model"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let query = arguments
                        .get("query")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    ActionDetails::ResearchAction {
                        action,
                        source,
                        query,
                    }
                } else if suffix.starts_with("attention")
                    || suffix.starts_with("feature_importance")
                    || suffix.starts_with("counterfactual")
                {
                    // Interpretability AI tools → EvaluationRun
                    let evaluator = arguments
                        .get("evaluator")
                        .or_else(|| arguments.get("method"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(suffix)
                        .to_string();
                    let scope = arguments
                        .get("scope")
                        .or_else(|| arguments.get("model"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let traces_count = arguments
                        .get("traces_count")
                        .or_else(|| arguments.get("num_samples"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    ActionDetails::EvaluationRun {
                        evaluator,
                        scope,
                        traces_count,
                    }
                } else {
                    // Fallback for unrecognized ai_* tools
                    ActionDetails::Other {
                        info: format!("ai_tool: {name} {arguments}"),
                    }
                }
            }
            // --- Fullstack development tools ---
            "scaffold" => {
                let template = arguments
                    .get("template")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let target_dir = arguments
                    .get("name")
                    .or_else(|| arguments.get("target_dir"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string();
                let framework = arguments
                    .get("framework")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                ActionDetails::Scaffold {
                    template,
                    target_dir,
                    framework,
                }
            }
            "dev_server" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("status")
                    .to_string();
                let port = arguments
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .map(|p| p as u16);
                ActionDetails::DevServer { action, port }
            }
            "database" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("status")
                    .to_string();
                let database = arguments
                    .get("database")
                    .or_else(|| arguments.get("db_path"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let reversible = matches!(action.as_str(), "query" | "schema" | "status");
                ActionDetails::DatabaseOperation {
                    action,
                    database,
                    reversible,
                }
            }
            "test_runner" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("run_all")
                    .to_string();
                let scope = arguments
                    .get("file")
                    .or_else(|| arguments.get("test_name"))
                    .and_then(|v| v.as_str())
                    .map(|s| format!("{action}: {s}"))
                    .unwrap_or_else(|| action.clone());
                let framework = arguments
                    .get("framework")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                ActionDetails::TestExecution { scope, framework }
            }
            "lint" => {
                let action = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("check")
                    .to_string();
                let auto_fix = matches!(action.as_str(), "fix" | "format");
                ActionDetails::LintCheck { action, auto_fix }
            }
            // --- Security scanning tools ---
            "security_scan"
            | "sast_scan"
            | "sca_scan"
            | "secrets_scan"
            | "secrets_validate"
            | "container_scan"
            | "dockerfile_lint"
            | "iac_scan"
            | "terraform_check"
            | "k8s_lint"
            | "supply_chain_check"
            | "vulnerability_check" => {
                let scanner = tool_name
                    .replace("_scan", "")
                    .replace("_check", "")
                    .replace("_lint", "");
                let target = arguments
                    .get("path")
                    .or_else(|| arguments.get("image"))
                    .or_else(|| arguments.get("package"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string();
                let scope = arguments
                    .get("scanners")
                    .or_else(|| arguments.get("scope"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("all")
                    .to_string();
                ActionDetails::SecurityScan {
                    scanner,
                    target,
                    scope,
                }
            }
            // --- Code review & quality tools ---
            "analyze_diff" | "code_review" | "suggest_fix" | "quality_score"
            | "complexity_check" | "dead_code_detect" | "duplicate_detect" | "tech_debt_report" => {
                let target = arguments
                    .get("path")
                    .or_else(|| arguments.get("diff"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string();
                ActionDetails::SecurityScan {
                    scanner: tool_name.to_string(),
                    target,
                    scope: "review".to_string(),
                }
            }
            // --- Apply fix (write action) ---
            "apply_fix" => {
                let path = arguments
                    .get("file")
                    .or_else(|| arguments.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let size = arguments
                    .get("replacement")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                ActionDetails::FileWrite {
                    path: path.into(),
                    size_bytes: size,
                }
            }
            // --- Compliance tools ---
            "license_check" | "sbom_generate" | "sbom_diff" | "policy_check"
            | "compliance_report" | "audit_export" | "risk_score" => {
                let framework = arguments
                    .get("framework")
                    .or_else(|| arguments.get("format"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("general")
                    .to_string();
                let scope = arguments
                    .get("path")
                    .or_else(|| arguments.get("scope"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("workspace")
                    .to_string();
                ActionDetails::ComplianceCheck { framework, scope }
            }
            // --- Incident response tools ---
            "threat_detect" | "log_analyze" | "alert_triage" => {
                let scanner = tool_name.to_string();
                let target = arguments
                    .get("path")
                    .or_else(|| arguments.get("log_file"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string();
                ActionDetails::SecurityScan {
                    scanner,
                    target,
                    scope: "incident".to_string(),
                }
            }
            "incident_respond" => {
                let action_type = arguments
                    .get("playbook")
                    .and_then(|v| v.as_str())
                    .unwrap_or("respond")
                    .to_string();
                let target = arguments
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let mode = arguments
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("dry_run");
                ActionDetails::IncidentAction {
                    action_type,
                    target,
                    reversible: mode == "dry_run",
                    incident_id: arguments
                        .get("incident_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                }
            }
            "alert_status" => {
                let action_type = arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("update")
                    .to_string();
                let target = arguments
                    .get("alert_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                ActionDetails::IncidentAction {
                    action_type,
                    target,
                    reversible: true,
                    incident_id: None,
                }
            }
            _ => ActionDetails::Other {
                info: arguments.to_string(),
            },
        }
    }

    /// Provide a tool-routing hint based on the cached task classification.
    /// Returns Some(hint) if the classification maps to a specific tool or workflow.
    /// This prevents the LLM from choosing generic tools (shell_exec, document_read)
    /// for tasks that have purpose-built tools.
    ///
    /// Uses the pre-computed `TaskClassification` from `AgentState` instead of
    /// running ~300 `.contains()` calls on every invocation.
    ///
    /// Match a cached task classification to a workflow template routing hint.
    /// This is platform-independent (workflows work on all platforms).
    fn workflow_routing_hint(classification: &TaskClassification) -> Option<String> {
        let workflow = match classification {
            TaskClassification::Workflow(name) => name.as_str(),
            _ => return None,
        };

        // ML-specific workflow routing with tailored tool guidance
        let ml_hint = match workflow {
            "ml_data_pipeline" => Some(
                "WORKFLOW ROUTING: For this data pipeline task, use ml_data_* tools \
                 (ml_data_ingest, ml_data_transform, ml_data_validate, ml_data_split). \
                 Start with data ingestion, then clean/transform, validate schema, \
                 and split into train/val/test sets.",
            ),
            "ml_training_experiment" => Some(
                "WORKFLOW ROUTING: For this training experiment, use ml_train, \
                 ml_experiment, ml_hyperparams, ml_metrics, and ml_checkpoint tools. \
                 Set up the experiment first, configure hyperparameters, start training, \
                 checkpoint periodically, and log metrics.",
            ),
            "ml_rag_setup" => Some(
                "WORKFLOW ROUTING: For this RAG setup task, use rag_* tools \
                 (rag_ingest, rag_index, rag_query, rag_evaluate). \
                 Ingest documents, build the vector index, then test retrieval quality.",
            ),
            "ml_llm_finetune" => Some(
                "WORKFLOW ROUTING: For this fine-tuning task, use ml_finetune, \
                 ml_chat_dataset, ml_adapter, ml_eval, and ml_quantize tools. \
                 Prepare the dataset, configure LoRA/QLoRA adapters, run fine-tuning, \
                 evaluate, and optionally quantize the output model.",
            ),
            "ai_safety_audit" => Some(
                "WORKFLOW ROUTING: For this AI safety audit, use ai_safety_*, ai_pii_*, \
                 ai_bias_*, and ai_alignment_* tools. Check for PII leakage, test for \
                 bias across demographic groups, validate alignment properties, and \
                 generate a compliance report.",
            ),
            _ => None,
        };

        if let Some(hint) = ml_hint {
            return Some(hint.to_string());
        }

        Some(format!(
            "WORKFLOW ROUTING: For this task, run the '{workflow}' workflow. \
             Use shell_exec to run: `rustant workflow run {workflow}` — or accomplish \
             the task directly step by step using available tools."
        ))
    }

    #[cfg(target_os = "macos")]
    fn tool_routing_hint_from_classification(
        classification: &TaskClassification,
    ) -> Option<String> {
        // ML tool routing — intercept ML-related workflows with tool-specific hints
        // before generic workflow routing
        if let TaskClassification::Workflow(name) = classification {
            let ml_tool_hint = match name.as_str() {
                n if n.contains("train") || n.contains("experiment") => Some(
                    "TOOL ROUTING: For model training tasks, use 'ml_train' to start training runs, \
                     'ml_experiment' to manage experiments, 'ml_hyperparams' for hyperparameter tuning, \
                     'ml_checkpoint' for saving/loading checkpoints, and 'ml_metrics' to log metrics. \
                     Use 'ml_finetune' for fine-tuning pre-trained models.",
                ),
                n if n.contains("rag") || n.contains("retrieval") || n.contains("ingest") => Some(
                    "TOOL ROUTING: For RAG tasks, use 'rag_ingest' to load documents, \
                     'rag_index' to build/update the vector index, 'rag_query' to perform \
                     retrieval-augmented queries, and 'rag_evaluate' to assess retrieval quality. \
                     Do NOT use web_fetch or shell_exec for document ingestion.",
                ),
                n if n.contains("eval") || n.contains("benchmark") || n.contains("judge") => Some(
                    "TOOL ROUTING: For evaluation tasks, use 'eval_run' to execute evaluations, \
                     'eval_compare' to compare model outputs, 'eval_report' to generate reports, \
                     and 'eval_dataset' to manage evaluation datasets. Use 'ai_bias_check' for \
                     fairness evaluation.",
                ),
                n if n.contains("inference")
                    || n.contains("serve")
                    || n.contains("deploy_model") =>
                {
                    Some(
                        "TOOL ROUTING: For model serving/inference tasks, use 'inference_serve' to start \
                     a model server, 'inference_predict' for batch predictions, 'inference_benchmark' \
                     for latency testing, and 'ml_quantize' for model optimization.",
                    )
                }
                n if n.contains("research") || n.contains("literature") || n.contains("paper") => {
                    Some(
                        "TOOL ROUTING: For ML research tasks, use 'research_search' to find papers, \
                     'research_summarize' for paper summaries, 'research_compare' to compare approaches, \
                     and 'research_implement' to generate code from papers. Also consider 'arxiv_research' \
                     for ArXiv-specific searches.",
                    )
                }
                n if n.contains("safety")
                    || n.contains("pii")
                    || n.contains("bias")
                    || n.contains("compliance") =>
                {
                    Some(
                        "TOOL ROUTING: For AI safety tasks, use 'ai_safety_check' for general safety evaluation, \
                     'ai_pii_scan' for PII detection, 'ai_bias_check' for fairness testing, and \
                     'ai_alignment_eval' for alignment verification. Generate reports with 'eval_report'.",
                    )
                }
                _ => None,
            };
            if let Some(hint) = ml_tool_hint {
                return Some(hint.to_string());
            }
        }

        // Workflow routing (platform-independent, checked first)
        if let Some(hint) = Self::workflow_routing_hint(classification) {
            return Some(hint);
        }

        let tool_hint = match classification {
            TaskClassification::Clipboard => {
                "For this task, call the 'macos_clipboard' tool with {\"action\":\"read\"} to read the clipboard or {\"action\":\"write\",\"content\":\"...\"} to write to it."
            }
            TaskClassification::SystemInfo => {
                "For this task, call the 'macos_system_info' tool with the appropriate action: \"battery\", \"disk\", \"memory\", \"cpu\", \"network\", or \"version\"."
            }
            TaskClassification::AppControl => {
                "For this task, call the 'macos_app_control' tool with the appropriate action: \"list_running\", \"open\", \"quit\", or \"activate\"."
            }
            TaskClassification::Meeting => {
                "For this task, call 'macos_meeting_recorder'. Use action 'record_and_transcribe' to start (announces via TTS, records with silence detection, auto-transcribes to Notes.app). Use 'stop' to stop manually. Use 'status' to check state."
            }
            TaskClassification::Calendar => {
                "For this task, call the 'macos_calendar' tool with the appropriate action."
            }
            TaskClassification::Reminders => {
                "For this task, call the 'macos_reminders' tool with the appropriate action."
            }
            TaskClassification::Notes => {
                "For this task, call the 'macos_notes' tool with the appropriate action."
            }
            TaskClassification::Screenshot => {
                "For this task, call the 'macos_screenshot' tool with the appropriate action."
            }
            TaskClassification::Notification => {
                "For this task, call the 'macos_notification' tool."
            }
            TaskClassification::Spotlight => {
                "For this task, call the 'macos_spotlight' tool to search files using Spotlight."
            }
            TaskClassification::FocusMode => "For this task, call the 'macos_focus_mode' tool.",
            TaskClassification::Music => {
                "For this task, call the 'macos_music' tool with the appropriate action."
            }
            TaskClassification::Email => {
                "For this task, call the 'macos_mail' tool with the appropriate action."
            }
            TaskClassification::Finder => {
                "For this task, call the 'macos_finder' tool with the appropriate action."
            }
            TaskClassification::Contacts => {
                "For this task, call the 'macos_contacts' tool with the appropriate action."
            }
            TaskClassification::WebSearch => {
                "For this task, call the 'web_search' tool with {\"query\": \"your search terms\"}. Do NOT use macos_safari or shell_exec for web searches — use the dedicated web_search tool which queries DuckDuckGo."
            }
            TaskClassification::WebFetch => {
                "For this task, call the 'web_fetch' tool with {\"url\": \"https://...\"} to retrieve page content. Do NOT use macos_safari or shell_exec — use the dedicated web_fetch tool."
            }
            TaskClassification::Safari => {
                "For this task, call the 'macos_safari' tool with the appropriate action. Note: for simple web searches use 'web_search' instead, and for fetching page content use 'web_fetch' instead."
            }
            TaskClassification::Slack => {
                "For this task, call the 'slack' tool with the appropriate action (send_message, read_messages, list_channels, reply_thread, list_users, add_reaction). Do NOT use macos_gui_scripting or macos_app_control to interact with Slack."
            }
            TaskClassification::Messaging => {
                "For this task, call the appropriate iMessage tool: 'imessage_read', 'imessage_send', or 'imessage_contacts'."
            }
            TaskClassification::ArxivResearch => {
                "For this task, call the 'arxiv_research' tool. Actions: search, fetch, analyze, compare, trending, save/library/remove, export_bibtex, collections, digest_config, paper_to_code, paper_to_notebook, implement, setup_env, verify, implementation_status, semantic_search, summarize, citation_graph, blueprint, reindex. Do NOT use macos_safari, shell_exec, or curl."
            }
            TaskClassification::KnowledgeGraph => {
                "For this task, call the 'knowledge_graph' tool. Actions: add_node, get_node, update_node, remove_node, add_edge, remove_edge, neighbors, search, list, path, stats, import_arxiv, export_dot."
            }
            TaskClassification::ExperimentTracking => {
                "For this task, call the 'experiment_tracker' tool. Actions: add_hypothesis, update_hypothesis, list_hypotheses, get_hypothesis, add_experiment, start_experiment, complete_experiment, fail_experiment, get_experiment, list_experiments, record_evidence, compare_experiments, summary, export_markdown."
            }
            TaskClassification::CodeIntelligence => {
                "For this task, call the 'code_intelligence' tool. Actions: analyze_architecture, detect_patterns, translate_snippet, compare_implementations, tech_debt_report, api_surface, dependency_map."
            }
            TaskClassification::ContentEngine => {
                "For this task, call the 'content_engine' tool. Actions: create, update, set_status, get, list, search, delete, schedule, calendar_add, calendar_list, calendar_remove, stats, adapt, export_markdown."
            }
            TaskClassification::SkillTracker => {
                "For this task, call the 'skill_tracker' tool. Actions: add_skill, log_practice, assess, list_skills, knowledge_gaps, learning_path, progress_report, daily_practice."
            }
            TaskClassification::CareerIntel => {
                "For this task, call the 'career_intel' tool. Actions: set_goal, log_achievement, add_portfolio, gap_analysis, market_scan, network_note, progress_report, strategy_review."
            }
            TaskClassification::SystemMonitor => {
                "For this task, call the 'system_monitor' tool. Actions: add_service, topology, health_check, log_incident, correlate, generate_runbook, impact_analysis, list_services."
            }
            TaskClassification::LifePlanner => {
                "For this task, call the 'life_planner' tool. Actions: set_energy_profile, add_deadline, log_habit, daily_plan, weekly_review, context_switch_log, balance_report, optimize_schedule."
            }
            TaskClassification::PrivacyManager => {
                "For this task, call the 'privacy_manager' tool. Actions: set_boundary, list_boundaries, audit_access, compliance_check, export_data, delete_data, encrypt_store, privacy_report."
            }
            TaskClassification::SelfImprovement => {
                "For this task, call the 'self_improvement' tool. Actions: analyze_patterns, performance_report, suggest_improvements, set_preference, get_preferences, cognitive_load, feedback, reset_baseline."
            }
            _ => return None,
        };

        Some(format!("TOOL ROUTING: {tool_hint}"))
    }

    /// Non-macOS fallback — workflow routing + cross-platform tool routing.
    #[cfg(not(target_os = "macos"))]
    fn tool_routing_hint_from_classification(
        classification: &TaskClassification,
    ) -> Option<String> {
        // ML tool routing — intercept ML-related workflows with tool-specific hints
        // before generic workflow routing
        if let TaskClassification::Workflow(name) = classification {
            let ml_tool_hint = match name.as_str() {
                n if n.contains("train") || n.contains("experiment") => Some(
                    "TOOL ROUTING: For model training tasks, use 'ml_train' to start training runs, \
                     'ml_experiment' to manage experiments, 'ml_hyperparams' for hyperparameter tuning, \
                     'ml_checkpoint' for saving/loading checkpoints, and 'ml_metrics' to log metrics. \
                     Use 'ml_finetune' for fine-tuning pre-trained models.",
                ),
                n if n.contains("rag") || n.contains("retrieval") || n.contains("ingest") => Some(
                    "TOOL ROUTING: For RAG tasks, use 'rag_ingest' to load documents, \
                     'rag_index' to build/update the vector index, 'rag_query' to perform \
                     retrieval-augmented queries, and 'rag_evaluate' to assess retrieval quality. \
                     Do NOT use web_fetch or shell_exec for document ingestion.",
                ),
                n if n.contains("eval") || n.contains("benchmark") || n.contains("judge") => Some(
                    "TOOL ROUTING: For evaluation tasks, use 'eval_run' to execute evaluations, \
                     'eval_compare' to compare model outputs, 'eval_report' to generate reports, \
                     and 'eval_dataset' to manage evaluation datasets. Use 'ai_bias_check' for \
                     fairness evaluation.",
                ),
                n if n.contains("inference")
                    || n.contains("serve")
                    || n.contains("deploy_model") =>
                {
                    Some(
                        "TOOL ROUTING: For model serving/inference tasks, use 'inference_serve' to start \
                     a model server, 'inference_predict' for batch predictions, 'inference_benchmark' \
                     for latency testing, and 'ml_quantize' for model optimization.",
                    )
                }
                n if n.contains("research") || n.contains("literature") || n.contains("paper") => {
                    Some(
                        "TOOL ROUTING: For ML research tasks, use 'research_search' to find papers, \
                     'research_summarize' for paper summaries, 'research_compare' to compare approaches, \
                     and 'research_implement' to generate code from papers. Also consider 'arxiv_research' \
                     for ArXiv-specific searches.",
                    )
                }
                n if n.contains("safety")
                    || n.contains("pii")
                    || n.contains("bias")
                    || n.contains("compliance") =>
                {
                    Some(
                        "TOOL ROUTING: For AI safety tasks, use 'ai_safety_check' for general safety evaluation, \
                     'ai_pii_scan' for PII detection, 'ai_bias_check' for fairness testing, and \
                     'ai_alignment_eval' for alignment verification. Generate reports with 'eval_report'.",
                    )
                }
                _ => None,
            };
            if let Some(hint) = ml_tool_hint {
                return Some(hint.to_string());
            }
        }

        // Workflow routing (platform-independent, checked first)
        if let Some(hint) = Self::workflow_routing_hint(classification) {
            return Some(hint);
        }

        let tool_hint = match classification {
            TaskClassification::WebSearch => {
                "For this task, call the 'web_search' tool with {\"query\": \"your search terms\"}. Do NOT use shell_exec for web searches — use the dedicated web_search tool which queries DuckDuckGo."
            }
            TaskClassification::WebFetch => {
                "For this task, call the 'web_fetch' tool with {\"url\": \"https://...\"} to retrieve page content. Do NOT use shell_exec — use the dedicated web_fetch tool."
            }
            TaskClassification::Slack => {
                "For this task, call the 'slack' tool with the appropriate action (send_message, read_messages, list_channels, reply_thread, list_users, add_reaction). Do NOT use shell_exec to interact with Slack."
            }
            TaskClassification::ArxivResearch => {
                "For this task, call the 'arxiv_research' tool. Actions: search, fetch, analyze, compare, trending, save/library/remove, export_bibtex, collections, digest_config, paper_to_code, paper_to_notebook, implement, setup_env, verify, implementation_status, semantic_search, summarize, citation_graph, blueprint, reindex. Do NOT use shell_exec or curl."
            }
            TaskClassification::KnowledgeGraph => {
                "For this task, call the 'knowledge_graph' tool. Actions: add_node, get_node, update_node, remove_node, add_edge, remove_edge, neighbors, search, list, path, stats, import_arxiv, export_dot."
            }
            TaskClassification::ExperimentTracking => {
                "For this task, call the 'experiment_tracker' tool. Actions: add_hypothesis, update_hypothesis, list_hypotheses, get_hypothesis, add_experiment, start_experiment, complete_experiment, fail_experiment, get_experiment, list_experiments, record_evidence, compare_experiments, summary, export_markdown."
            }
            TaskClassification::CodeIntelligence => {
                "For this task, call the 'code_intelligence' tool. Actions: analyze_architecture, detect_patterns, translate_snippet, compare_implementations, tech_debt_report, api_surface, dependency_map."
            }
            TaskClassification::ContentEngine => {
                "For this task, call the 'content_engine' tool. Actions: create, update, set_status, get, list, search, delete, schedule, calendar_add, calendar_list, calendar_remove, stats, adapt, export_markdown."
            }
            TaskClassification::SkillTracker => {
                "For this task, call the 'skill_tracker' tool. Actions: add_skill, log_practice, assess, list_skills, knowledge_gaps, learning_path, progress_report, daily_practice."
            }
            TaskClassification::CareerIntel => {
                "For this task, call the 'career_intel' tool. Actions: set_goal, log_achievement, add_portfolio, gap_analysis, market_scan, network_note, progress_report, strategy_review."
            }
            TaskClassification::SystemMonitor => {
                "For this task, call the 'system_monitor' tool. Actions: add_service, topology, health_check, log_incident, correlate, generate_runbook, impact_analysis, list_services."
            }
            TaskClassification::LifePlanner => {
                "For this task, call the 'life_planner' tool. Actions: set_energy_profile, add_deadline, log_habit, daily_plan, weekly_review, context_switch_log, balance_report, optimize_schedule."
            }
            TaskClassification::PrivacyManager => {
                "For this task, call the 'privacy_manager' tool. Actions: set_boundary, list_boundaries, audit_access, compliance_check, export_data, delete_data, encrypt_store, privacy_report."
            }
            TaskClassification::SelfImprovement => {
                "For this task, call the 'self_improvement' tool. Actions: analyze_patterns, performance_report, suggest_improvements, set_preference, get_preferences, cognitive_load, feedback, reset_baseline."
            }
            _ => return None,
        };

        Some(format!("TOOL ROUTING: {}", tool_hint))
    }

    /// Auto-correct a tool call when the LLM is stuck calling the wrong tool.
    /// Returns Some((corrected_name, corrected_args)) if a correction is possible.
    /// Uses the cached `TaskClassification` from `AgentState` for O(1) matching.
    ///
    /// Cross-platform: common corrections (Slack, ArXiv, WebSearch) work everywhere.
    /// macOS-specific corrections (Clipboard, SystemInfo, AppControl) are gated by cfg.
    fn auto_correct_tool_call(
        failed_tool: &str,
        _args: &serde_json::Value,
        state: &AgentState,
    ) -> Option<(String, serde_json::Value)> {
        let classification = state.task_classification.as_ref()?;
        let task = state.current_goal.as_deref().unwrap_or("");

        // Cross-platform corrections
        match classification {
            TaskClassification::Slack
                if matches!(
                    failed_tool,
                    "shell_exec" | "web_fetch" | "macos_gui_scripting" | "macos_app_control"
                ) =>
            {
                return Some((
                    "slack".to_string(),
                    serde_json::json!({"action": "send_message"}),
                ));
            }
            TaskClassification::ArxivResearch
                if matches!(
                    failed_tool,
                    "macos_safari" | "shell_exec" | "web_fetch" | "web_search"
                ) =>
            {
                return Some((
                    "arxiv_research".to_string(),
                    serde_json::json!({"action": "search", "query": task, "max_results": 10}),
                ));
            }
            TaskClassification::WebSearch
                if matches!(failed_tool, "macos_safari" | "shell_exec") =>
            {
                return Some(("web_search".to_string(), serde_json::json!({"query": task})));
            }
            _ => {}
        }

        // macOS-specific corrections
        #[cfg(target_os = "macos")]
        match classification {
            TaskClassification::Clipboard
                if matches!(failed_tool, "document_read" | "file_read" | "shell_exec") =>
            {
                return Some((
                    "macos_clipboard".to_string(),
                    serde_json::json!({"action": "read"}),
                ));
            }
            TaskClassification::SystemInfo
                if matches!(failed_tool, "document_read" | "file_read" | "shell_exec") =>
            {
                let lower = task.to_lowercase();
                let action = if lower.contains("battery") {
                    "battery"
                } else if lower.contains("disk") {
                    "disk"
                } else if lower.contains("cpu") || lower.contains("processor") {
                    "cpu"
                } else if lower.contains("memory") || lower.contains("ram") {
                    "memory"
                } else {
                    "version"
                };
                return Some((
                    "macos_system_info".to_string(),
                    serde_json::json!({"action": action}),
                ));
            }
            TaskClassification::AppControl
                if matches!(failed_tool, "document_read" | "file_read" | "shell_exec") =>
            {
                return Some((
                    "macos_app_control".to_string(),
                    serde_json::json!({"action": "list_running"}),
                ));
            }
            _ => {}
        }

        None
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
            format!("Selected tool '{tool_name}' (risk: {risk_level})"),
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
            builder.add_context_factor(&format!("Current goal: {goal}"), FactorInfluence::Positive);
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

        // Add active persona as context
        if let Some(ref resolver) = self.persona_resolver {
            let persona = resolver.active_persona(self.last_classification.as_ref());
            builder.set_persona(
                &persona.to_string(),
                "Auto-detected from task classification",
            );
        }

        // Improved confidence scoring using multiple signals
        let mut confidence = self.calculate_tool_confidence(tool_name, risk_level);

        // Apply persona confidence modifier
        if let Some(ref resolver) = self.persona_resolver {
            let persona = resolver.active_persona(self.last_classification.as_ref());
            if let Some(profile) = resolver.profile(&persona) {
                confidence = (confidence + profile.confidence_modifier).clamp(0.0, 1.0);
            }
        }

        builder.set_confidence(confidence);

        builder.build()
    }

    /// Calculate confidence score for a tool call based on multiple factors.
    ///
    /// Considers risk level, prior usage in this session, and iteration depth.
    fn calculate_tool_confidence(&self, tool_name: &str, risk_level: RiskLevel) -> f32 {
        // Base confidence from risk level
        let mut confidence: f32 = match risk_level {
            RiskLevel::ReadOnly => 0.90,
            RiskLevel::Write => 0.75,
            RiskLevel::Execute => 0.65,
            RiskLevel::Network => 0.70,
            RiskLevel::Destructive => 0.45,
        };

        // +0.05 if tool has been used successfully before in this session
        if self.tool_token_usage.contains_key(tool_name) {
            confidence += 0.05;
        }

        // -0.1 if iteration count is high (>10), suggesting the agent may be looping
        if self.state.iteration > 10 {
            confidence -= 0.1;
        }

        // -0.05 if approaching iteration limit (>80% of max)
        if self.state.max_iterations > 0
            && (self.state.iteration as f64 / self.state.max_iterations as f64) > 0.8
        {
            confidence -= 0.05;
        }

        confidence.clamp(0.0, 1.0)
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

    /// Reset the cancellation token so the agent can process another task.
    /// Must be called before `process_task()` if a previous task was cancelled.
    pub fn reset_cancellation(&mut self) {
        self.cancellation = CancellationToken::new();
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

    /// Get a reference to the cron scheduler (if enabled).
    pub fn cron_scheduler(&self) -> Option<&CronScheduler> {
        self.cron_scheduler.as_ref()
    }

    /// Get a mutable reference to the cron scheduler (if enabled).
    pub fn cron_scheduler_mut(&mut self) -> Option<&mut CronScheduler> {
        self.cron_scheduler.as_mut()
    }

    /// Get a reference to the job manager.
    pub fn job_manager(&self) -> &JobManager {
        &self.job_manager
    }

    /// Get a mutable reference to the job manager.
    pub fn job_manager_mut(&mut self) -> &mut JobManager {
        &mut self.job_manager
    }

    /// Check scheduler for due tasks and return their task strings.
    pub fn check_scheduler(&mut self) -> Vec<String> {
        let mut due_tasks = Vec::new();

        // Check cron scheduler
        if let Some(ref scheduler) = self.cron_scheduler {
            let due_jobs: Vec<String> = scheduler
                .due_jobs()
                .iter()
                .map(|j| j.config.name.clone())
                .collect();
            for name in &due_jobs {
                if let Some(ref scheduler) = self.cron_scheduler
                    && let Some(job) = scheduler.get_job(name)
                {
                    due_tasks.push(job.config.task.clone());
                }
            }
            // Mark them executed
            if let Some(ref mut scheduler) = self.cron_scheduler {
                for name in &due_jobs {
                    let _ = scheduler.mark_executed(name);
                }
            }
        }

        // Check heartbeat tasks
        if let Some(ref mut heartbeat) = self.heartbeat_manager {
            let ready: Vec<(String, String)> = heartbeat
                .ready_tasks()
                .iter()
                .map(|t| (t.name.clone(), t.action.clone()))
                .collect();
            for (name, action) in &ready {
                if let Some(ref task_condition) = heartbeat
                    .config()
                    .tasks
                    .iter()
                    .find(|t| t.name == *name)
                    .and_then(|t| t.condition.clone())
                {
                    if HeartbeatManager::check_condition(task_condition) {
                        due_tasks.push(action.clone());
                        heartbeat.mark_executed(name);
                    }
                } else {
                    due_tasks.push(action.clone());
                    heartbeat.mark_executed(name);
                }
            }
        }

        due_tasks
    }

    /// Save scheduler state (cron jobs + background jobs) to the given directory.
    pub fn save_scheduler_state(
        &self,
        state_dir: &std::path::Path,
    ) -> Result<(), crate::error::SchedulerError> {
        if let Some(ref scheduler) = self.cron_scheduler {
            crate::scheduler::save_state(scheduler, &self.job_manager, state_dir)
        } else {
            // Nothing to save when scheduler is disabled
            Ok(())
        }
    }

    /// Load scheduler state from disk and replace current scheduler/job_manager.
    pub fn load_scheduler_state(&mut self, state_dir: &std::path::Path) {
        if self.cron_scheduler.is_some() {
            let (loaded_scheduler, loaded_jm) = crate::scheduler::load_state(state_dir);
            if !loaded_scheduler.is_empty() {
                self.cron_scheduler = Some(loaded_scheduler);
                info!("Restored cron scheduler state from {:?}", state_dir);
            }
            if !loaded_jm.is_empty() {
                self.job_manager = loaded_jm;
                info!("Restored job manager state from {:?}", state_dir);
            }
        }
    }

    /// Get recent decision explanations for transparency.
    pub fn recent_explanations(&self) -> Vec<&DecisionExplanation> {
        self.recent_explanations.iter().collect()
    }

    /// Get a reference to the decision log for interpretability queries.
    pub fn decision_log(&self) -> &crate::decision_log::DecisionLog {
        &self.decision_log
    }

    /// Get a mutable reference to the data flow tracker.
    pub fn data_flow_tracker_mut(&mut self) -> &mut crate::data_flow::DataFlowTracker {
        &mut self.data_flow_tracker
    }

    /// Get a reference to the data flow tracker.
    pub fn data_flow_tracker(&self) -> &crate::data_flow::DataFlowTracker {
        &self.data_flow_tracker
    }

    /// Get a reference to the consent manager, if initialized.
    pub fn consent_manager(&self) -> Option<&crate::consent::ConsentManager> {
        self.consent_manager.as_ref()
    }

    /// Get a mutable reference to the consent manager, if initialized.
    pub fn consent_manager_mut(&mut self) -> Option<&mut crate::consent::ConsentManager> {
        self.consent_manager.as_mut()
    }

    /// Get a reference to the persona resolver, if initialized.
    pub fn persona_resolver(&self) -> Option<&crate::personas::PersonaResolver> {
        self.persona_resolver.as_ref()
    }

    /// Get a mutable reference to the persona resolver, if initialized.
    pub fn persona_resolver_mut(&mut self) -> Option<&mut crate::personas::PersonaResolver> {
        self.persona_resolver.as_mut()
    }

    /// Get the most recent task classification.
    pub fn last_classification(&self) -> &Option<crate::types::TaskClassification> {
        &self.last_classification
    }

    /// Get a reference to the MoE router, if enabled.
    pub fn moe_router(&self) -> Option<&crate::moe::MoeRouter> {
        self.moe_router.as_ref()
    }

    /// Get a mutable reference to the MoE router, if enabled.
    pub fn moe_router_mut(&mut self) -> Option<&mut crate::moe::MoeRouter> {
        self.moe_router.as_mut()
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
                format!("{name} ({pct}%)")
            })
            .collect();
        top.join(", ")
    }

    /// Run a council deliberation if configured and the task is appropriate.
    ///
    /// Returns `Some(CouncilResult)` if council was used, `None` if skipped.
    /// Falls back gracefully if council fails.
    pub async fn think_with_council(
        &self,
        task: &str,
        council: &crate::council::PlanningCouncil,
    ) -> Option<crate::council::CouncilResult> {
        if !crate::council::should_use_council(task) {
            debug!(task, "Skipping council — task is not a planning task");
            return None;
        }

        info!(task, "Using council deliberation for planning task");
        match council.deliberate(task).await {
            Ok(result) => {
                info!(
                    responses = result.member_responses.len(),
                    reviews = result.peer_reviews.len(),
                    cost = format!("${:.4}", result.total_cost),
                    "Council deliberation succeeded"
                );
                Some(result)
            }
            Err(e) => {
                warn!(error = %e, "Council deliberation failed, falling back to single model");
                None
            }
        }
    }

    // --- Redaction ---

    /// Set the output redactor for stripping secrets from tool outputs before
    /// they are stored in long-term memory or audit trails.
    pub fn set_output_redactor(&mut self, redactor: crate::redact::SharedRedactor) {
        self.output_redactor = Some(redactor);
    }

    /// Apply redaction if a redactor is configured, otherwise return unchanged.
    fn redact_output(&self, text: &str) -> String {
        match &self.output_redactor {
            Some(redactor) => redactor.redact(text),
            None => text.to_string(),
        }
    }

    // --- Plan Mode ---

    /// Toggle plan mode on or off.
    pub fn set_plan_mode(&mut self, enabled: bool) {
        self.plan_mode = enabled;
    }

    /// Query whether plan mode is active.
    pub fn plan_mode(&self) -> bool {
        self.plan_mode
    }

    /// Access the current plan, if any.
    pub fn current_plan(&self) -> Option<&crate::plan::ExecutionPlan> {
        self.current_plan.as_ref()
    }

    /// Generate a structured execution plan for a task via the LLM.
    async fn generate_plan(
        &mut self,
        task: &str,
    ) -> Result<crate::plan::ExecutionPlan, RustantError> {
        use crate::plan::{PLAN_GENERATION_PROMPT, PlanStatus};

        // Build a prompt with available tools and the task
        // Plan generation needs all tools — don't filter by classification
        let tool_list: Vec<String> = self
            .tool_definitions(None)
            .iter()
            .map(|t| format!("- {} — {}", t.name, t.description))
            .collect();
        let tools_str = tool_list.join("\n");

        let plan_prompt =
            format!("{PLAN_GENERATION_PROMPT}\n\nAvailable tools:\n{tools_str}\n\nTask: {task}");

        // Use a temporary conversation for plan generation (don't pollute memory)
        let messages = vec![Message::system(&plan_prompt), Message::user(task)];

        let response = self
            .brain
            .think_with_retry(&messages, None, 3)
            .await
            .map_err(RustantError::Llm)?;

        // Record usage
        self.budget.record_usage(
            &response.usage,
            &CostEstimate {
                input_cost: 0.0,
                output_cost: 0.0,
                ..Default::default()
            },
        );

        let text = response.message.content.as_text().unwrap_or("").to_string();
        let mut plan = crate::plan::parse_plan_json(&text, task);

        // Enforce max_steps from config
        let max_steps = self.config.plan.as_ref().map(|p| p.max_steps).unwrap_or(20);
        if plan.steps.len() > max_steps {
            plan.steps.truncate(max_steps);
        }

        plan.status = PlanStatus::PendingReview;
        Ok(plan)
    }

    /// Execute an approved plan step by step.
    async fn execute_plan(
        &mut self,
        plan: &mut crate::plan::ExecutionPlan,
    ) -> Result<TaskResult, RustantError> {
        use crate::plan::{PlanStatus, StepStatus};

        plan.status = PlanStatus::Executing;
        let task_id = Uuid::new_v4();

        while let Some(step_idx) = plan.next_pending_step() {
            plan.current_step = Some(step_idx);
            let step = &plan.steps[step_idx];
            let step_desc = step.description.clone();
            let step_tool = step.tool.clone();
            let step_args = step.tool_args.clone();

            // Notify step start
            self.callback
                .on_plan_step_start(step_idx, &plan.steps[step_idx])
                .await;
            plan.steps[step_idx].status = StepStatus::InProgress;

            let result = if let Some(tool_name) = &step_tool {
                // If we have a tool and args, execute directly
                let args = step_args.unwrap_or(serde_json::json!({}));

                self.callback.on_tool_start(tool_name, &args).await;
                let start = std::time::Instant::now();
                let exec_result = self.execute_tool("plan", tool_name, &args).await;
                let duration_ms = start.elapsed().as_millis() as u64;

                match exec_result {
                    Ok(output) => {
                        self.callback
                            .on_tool_result(tool_name, &output, duration_ms)
                            .await;
                        Ok(output.content)
                    }
                    Err(e) => Err(format!("{e}")),
                }
            } else {
                // No specific tool — let the LLM handle this step
                // by running one Think iteration with the step as context
                let step_prompt = format!(
                    "Execute plan step {}: {}\n\nPrevious step results are in context.",
                    step_idx + 1,
                    step_desc
                );
                self.memory.add_message(Message::user(&step_prompt));

                let conversation = self.memory.context_messages();
                let tools = Some(self.tool_definitions(self.state.task_classification.as_ref()));
                let response = if self.config.llm.use_streaming {
                    self.think_streaming(&conversation, tools).await
                } else {
                    self.brain.think_with_retry(&conversation, tools, 3).await
                };

                match response {
                    Ok(resp) => {
                        let text = resp
                            .message
                            .content
                            .as_text()
                            .unwrap_or("(no output)")
                            .to_string();
                        self.callback.on_assistant_message(&text).await;
                        self.memory.add_message(resp.message);
                        Ok(text)
                    }
                    Err(e) => Err(format!("{e}")),
                }
            };

            match result {
                Ok(output) => {
                    plan.complete_step(step_idx, &output);
                }
                Err(error) => {
                    plan.fail_step(step_idx, &error);
                    // Notify step failure
                    self.callback
                        .on_plan_step_complete(step_idx, &plan.steps[step_idx])
                        .await;
                    // Stop execution on first failure
                    plan.status = PlanStatus::Failed;
                    break;
                }
            }

            // Notify step completion
            self.callback
                .on_plan_step_complete(step_idx, &plan.steps[step_idx])
                .await;
        }

        // Update overall status
        if plan.status != PlanStatus::Failed {
            let all_done = plan
                .steps
                .iter()
                .all(|s| s.status == StepStatus::Completed || s.status == StepStatus::Skipped);
            plan.status = if all_done {
                PlanStatus::Completed
            } else {
                PlanStatus::Failed
            };
        }

        let success = plan.status == PlanStatus::Completed;
        let response = plan.progress_summary();

        Ok(TaskResult {
            task_id,
            success,
            response,
            iterations: plan.steps.len(),
            total_usage: *self.brain.total_usage(),
            total_cost: *self.brain.total_cost(),
        })
    }

    /// Process a task in plan mode: generate → review → execute.
    async fn process_task_with_plan(&mut self, task: &str) -> Result<TaskResult, RustantError> {
        use crate::plan::{PlanDecision, PlanStatus};

        // 1. Generate the plan
        self.state.status = AgentStatus::Planning;
        self.callback.on_status_change(AgentStatus::Planning).await;
        self.callback.on_plan_generating(task).await;

        let mut plan = self.generate_plan(task).await?;

        // 2. Handle any clarifications
        for question in &plan.clarifications.clone() {
            let answer = self.callback.on_clarification_request(question).await;
            if !answer.is_empty() {
                // Add clarification answer to context for potential re-generation
                self.memory
                    .add_message(Message::user(format!("Q: {question} A: {answer}")));
            }
        }

        // 3. Review loop
        loop {
            let decision = self.callback.on_plan_review(&plan).await;
            match decision {
                PlanDecision::Approve => break,
                PlanDecision::Reject => {
                    plan.status = PlanStatus::Cancelled;
                    self.current_plan = Some(plan);
                    self.state.complete();
                    self.callback.on_status_change(AgentStatus::Complete).await;
                    let task_id = self.state.task_id.unwrap_or_else(Uuid::new_v4);
                    return Ok(TaskResult {
                        task_id,
                        success: false,
                        response: "Plan rejected by user.".to_string(),
                        iterations: 0,
                        total_usage: *self.brain.total_usage(),
                        total_cost: *self.brain.total_cost(),
                    });
                }
                PlanDecision::EditStep(idx, new_desc) => {
                    if let Some(step) = plan.steps.get_mut(idx) {
                        step.description = new_desc;
                        plan.updated_at = chrono::Utc::now();
                    }
                }
                PlanDecision::RemoveStep(idx) => {
                    if idx < plan.steps.len() {
                        plan.steps.remove(idx);
                        // Re-index remaining steps
                        for (i, step) in plan.steps.iter_mut().enumerate() {
                            step.index = i;
                        }
                        plan.updated_at = chrono::Utc::now();
                    }
                }
                PlanDecision::AddStep(idx, desc) => {
                    let new_step = crate::plan::PlanStep {
                        index: idx,
                        description: desc,
                        ..Default::default()
                    };
                    if idx <= plan.steps.len() {
                        plan.steps.insert(idx, new_step);
                    } else {
                        plan.steps.push(new_step);
                    }
                    // Re-index
                    for (i, step) in plan.steps.iter_mut().enumerate() {
                        step.index = i;
                    }
                    plan.updated_at = chrono::Utc::now();
                }
                PlanDecision::ReorderSteps(new_order) => {
                    let old_steps = plan.steps.clone();
                    plan.steps.clear();
                    for (i, &old_idx) in new_order.iter().enumerate() {
                        if let Some(mut step) = old_steps.get(old_idx).cloned() {
                            step.index = i;
                            plan.steps.push(step);
                        }
                    }
                    plan.updated_at = chrono::Utc::now();
                }
                PlanDecision::AskQuestion(question) => {
                    // Send question to LLM and display the answer
                    let messages = vec![
                        Message::system("Answer this question about the plan you generated."),
                        Message::user(&question),
                    ];
                    if let Ok(resp) = self.brain.think_with_retry(&messages, None, 1).await
                        && let Some(answer) = resp.message.content.as_text()
                    {
                        self.callback.on_assistant_message(answer).await;
                    }
                }
            }
        }

        // 4. Execute the approved plan
        self.current_plan = Some(plan.clone());
        let result = self.execute_plan(&mut plan).await?;
        self.current_plan = Some(plan);
        self.state.complete();
        self.callback.on_status_change(AgentStatus::Complete).await;

        Ok(result)
    }

    /// Check if context compression is needed and perform it.
    ///
    /// Extracted from the agent loop to avoid duplication between the single-ToolCall
    /// and MultiPart code paths.
    async fn check_and_compress(&mut self) {
        if !self.memory.short_term.needs_compression() {
            return;
        }

        // Aggressively mask stale observations first — any tool result more than
        // 2x window_size messages old is replaced with a compact summary regardless
        // of whether the assistant has consumed it. This catches long-lived tool
        // outputs that persist across many iterations.
        let stale_age = self.config.memory.window_size * 2;
        let stale_masked = self.memory.short_term.mask_stale_observations(stale_age, 200);
        if stale_masked > 0 {
            debug!(stale_masked, "Masked stale tool observations by age");
        }

        // Then mask consumed observations (tool results already used by assistant)
        // to reduce input to the summarizer and overall context size.
        let masked = self.memory.short_term.mask_consumed_observations(500);
        if masked > 0 {
            debug!(
                masked_count = masked,
                "Masked consumed tool observations before compression"
            );
        }

        let msgs_to_summarize: Vec<crate::types::Message> = self
            .memory
            .short_term
            .messages_to_summarize()
            .into_iter()
            .cloned()
            .collect();
        let msgs_count = msgs_to_summarize.len();
        let pinned_count = self.memory.short_term.pinned_count();

        // Lazily initialize the summarizer on first context compression.
        let summarizer = self
            .summarizer
            .get_or_insert_with(|| ContextSummarizer::new(self.brain.provider_arc()));

        let (summary_text, was_llm) = match summarizer.summarize(&msgs_to_summarize).await {
            Ok(result) => {
                info!(
                    messages_summarized = result.messages_summarized,
                    tokens_saved = result.tokens_saved,
                    "Context compression via LLM summarization"
                );
                (result.text, true)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "LLM summarization failed, falling back to truncation"
                );
                let text = crate::summarizer::smart_fallback_summary(&msgs_to_summarize, 500);
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
        let mut config = AgentConfig::default();
        // Use non-streaming for deterministic test behavior
        config.llm.use_streaming = false;
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
                    Ok(ToolOutput::text(format!("Echo: {text}")))
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
        // Queue many tool calls to exhaust iterations (more than max_iterations default of 50)
        for _ in 0..55 {
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
                assert_eq!(max, 50);
            }
            e => panic!("Expected MaxIterationsReached, got: {e:?}"),
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
            e => panic!("Expected Cancelled, got: {e:?}"),
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
            other => panic!("Expected ToolSelection, got {other:?}"),
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
                    Ok(ToolOutput::text(format!("Echo: {text}")))
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
        let mut config = AgentConfig::default();
        config.llm.use_streaming = false;
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
            other => panic!("Expected FileRead, got {other:?}"),
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
            other => panic!("Expected FileWrite, got {other:?}"),
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
            other => panic!("Expected ShellCommand, got {other:?}"),
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
                    "Expected 'commit' in '{operation}'"
                );
                assert!(
                    operation.contains("fix bug"),
                    "Expected 'fix bug' in '{operation}'"
                );
            }
            other => panic!("Expected GitOperation, got {other:?}"),
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
            "Reasoning should mention size: {reasoning}"
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
                    Ok(ToolOutput::text(format!("Echo: {text}")))
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
        config.llm.use_streaming = false;
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

    #[test]
    fn test_scheduler_fields_none_when_disabled() {
        let provider = Arc::new(MockLlmProvider::new());
        let (agent, _) = create_test_agent(provider);
        // Default config has no scheduler section, so fields should be None
        assert!(agent.cron_scheduler().is_none());
    }

    #[test]
    fn test_save_scheduler_state_noop_when_disabled() {
        let provider = Arc::new(MockLlmProvider::new());
        let (agent, _) = create_test_agent(provider);
        let dir = tempfile::TempDir::new().unwrap();
        // Should succeed silently when scheduler is disabled
        assert!(agent.save_scheduler_state(dir.path()).is_ok());
    }

    #[test]
    fn test_load_scheduler_state_noop_when_disabled() {
        let provider = Arc::new(MockLlmProvider::new());
        let (mut agent, _) = create_test_agent(provider);
        let dir = tempfile::TempDir::new().unwrap();
        // Should not panic when scheduler is disabled
        agent.load_scheduler_state(dir.path());
        assert!(agent.cron_scheduler().is_none());
    }

    #[test]
    fn test_save_load_scheduler_roundtrip() {
        let provider = Arc::new(MockLlmProvider::new());
        let callback = Arc::new(RecordingCallback::new());
        let mut config = AgentConfig::default();
        config.llm.use_streaming = false;
        config.scheduler = Some(crate::config::SchedulerConfig {
            enabled: true,
            cron_jobs: vec![crate::scheduler::CronJobConfig::new(
                "test_job",
                "0 0 9 * * * *",
                "do something",
            )],
            ..Default::default()
        });
        let agent = Agent::new(provider.clone(), config, callback);
        assert_eq!(agent.cron_scheduler().unwrap().len(), 1);

        let dir = tempfile::TempDir::new().unwrap();
        agent.save_scheduler_state(dir.path()).unwrap();

        // Create a new agent with an empty scheduler and load state
        let callback2 = Arc::new(RecordingCallback::new());
        let mut config2 = AgentConfig::default();
        config2.llm.use_streaming = false;
        config2.scheduler = Some(crate::config::SchedulerConfig {
            enabled: true,
            cron_jobs: vec![],
            ..Default::default()
        });
        let mut agent2 = Agent::new(provider, config2, callback2);
        assert_eq!(agent2.cron_scheduler().unwrap().len(), 0);

        agent2.load_scheduler_state(dir.path());
        assert_eq!(agent2.cron_scheduler().unwrap().len(), 1);
    }

    #[test]
    fn test_tools_for_classification_calendar() {
        let set = Agent::tools_for_classification(&TaskClassification::Calendar)
            .expect("Calendar should return Some");
        // Should include core tools
        assert!(set.contains("file_read"), "Missing core tool file_read");
        assert!(set.contains("ask_user"), "Missing core tool ask_user");
        assert!(set.contains("calculator"), "Missing core tool calculator");
        // Should include calendar-specific tools
        assert!(set.contains("macos_calendar"), "Missing macos_calendar");
        assert!(
            set.contains("macos_notification"),
            "Missing macos_notification"
        );
        // Should NOT include unrelated tools
        assert!(
            !set.contains("macos_music"),
            "Should not include macos_music"
        );
        assert!(!set.contains("git_status"), "Should not include git_status");
        // Total: 10 core + 2 extra = 12
        assert_eq!(set.len(), 12);
    }

    #[test]
    fn test_tools_for_classification_general_returns_none() {
        assert!(
            Agent::tools_for_classification(&TaskClassification::General).is_none(),
            "General classification should return None (all tools)"
        );
    }

    #[test]
    fn test_tools_for_classification_workflow_returns_none() {
        assert!(
            Agent::tools_for_classification(&TaskClassification::Workflow("security_scan".into()))
                .is_none(),
            "Workflow classification should return None (all tools)"
        );
    }

    #[test]
    fn test_tool_definitions_filtered() {
        let provider = Arc::new(MockLlmProvider::new());
        let (mut agent, _) = create_test_agent(provider);

        // Register some tools that span different categories
        for name in &[
            "echo",
            "file_read",
            "macos_calendar",
            "git_status",
            "macos_music",
        ] {
            agent.register_tool(RegisteredTool {
                definition: ToolDefinition {
                    name: name.to_string(),
                    description: format!("{name} tool"),
                    parameters: serde_json::json!({"type": "object"}),
                },
                risk_level: RiskLevel::ReadOnly,
                executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("ok")) })),
            });
        }

        // Unfiltered: should return all 5 registered + ask_user = 6
        let all_defs = agent.tool_definitions(None);
        assert_eq!(
            all_defs.len(),
            6,
            "Unfiltered should return all tools + ask_user"
        );

        // Calendar filter: should include echo, file_read, macos_calendar but NOT git_status, macos_music
        let calendar_defs = agent.tool_definitions(Some(&TaskClassification::Calendar));
        let names: Vec<&str> = calendar_defs.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"echo"),
            "Calendar should include core tool echo"
        );
        assert!(
            names.contains(&"file_read"),
            "Calendar should include core tool file_read"
        );
        assert!(
            names.contains(&"macos_calendar"),
            "Calendar should include macos_calendar"
        );
        assert!(
            names.contains(&"ask_user"),
            "Should always include ask_user"
        );
        assert!(
            !names.contains(&"git_status"),
            "Calendar should NOT include git_status"
        );
        assert!(
            !names.contains(&"macos_music"),
            "Calendar should NOT include macos_music"
        );

        // General filter: should return all tools
        let general_defs = agent.tool_definitions(Some(&TaskClassification::General));
        assert_eq!(general_defs.len(), 6, "General should return all tools");
    }
}
