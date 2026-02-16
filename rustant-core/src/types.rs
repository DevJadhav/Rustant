//! Core type definitions for the Rustant agent.
//!
//! Defines the fundamental data structures used throughout the system:
//! messages, tool calls, content types, and agent state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Represents a participant role in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

/// Content within a message — text, tool call, or tool result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        call_id: String,
        output: String,
        is_error: bool,
    },
    MultiPart {
        parts: Vec<Content>,
    },
}

impl Content {
    /// Create a simple text content.
    pub fn text(text: impl Into<String>) -> Self {
        Content::Text { text: text.into() }
    }

    /// Create a tool call content.
    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Content::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// Create a tool result content.
    pub fn tool_result(
        call_id: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Content::ToolResult {
            call_id: call_id.into(),
            output: output.into(),
            is_error,
        }
    }

    /// Returns the text representation of this content.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// A single message in the conversation history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: Role,
    pub content: Content,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    /// Create a new message with auto-generated ID and current timestamp.
    pub fn new(role: Role, content: Content) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create a system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, Content::text(text))
    }

    /// Create a user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, Content::text(text))
    }

    /// Create an assistant message.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, Content::text(text))
    }

    /// Create a tool result message.
    pub fn tool_result(
        call_id: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::new(Role::Tool, Content::tool_result(call_id, output, is_error))
    }

    /// Add metadata to this message.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Approximate character length of the message content.
    pub fn content_length(&self) -> usize {
        match &self.content {
            Content::Text { text } => text.len(),
            Content::ToolCall {
                name, arguments, ..
            } => name.len() + arguments.to_string().len(),
            Content::ToolResult { output, .. } => output.len(),
            Content::MultiPart { parts } => parts.iter().map(content_char_len).sum(),
        }
    }
}

/// Helper to compute char length for a Content variant.
fn content_char_len(c: &Content) -> usize {
    match c {
        Content::Text { text } => text.len(),
        Content::ToolCall {
            name, arguments, ..
        } => name.len() + arguments.to_string().len(),
        Content::ToolResult { output, .. } => output.len(),
        Content::MultiPart { parts } => parts.iter().map(content_char_len).sum(),
    }
}

/// A definition describing a tool for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// The risk level of a tool operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Read-only operations (level 0).
    ReadOnly = 0,
    /// Reversible write operations (level 1).
    Write = 1,
    /// Shell command execution (level 2).
    Execute = 2,
    /// Network operations (level 3).
    Network = 3,
    /// Destructive / irreversible operations (level 4).
    Destructive = 4,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::ReadOnly => write!(f, "read-only"),
            RiskLevel::Write => write!(f, "write"),
            RiskLevel::Execute => write!(f, "execute"),
            RiskLevel::Network => write!(f, "network"),
            RiskLevel::Destructive => write!(f, "destructive"),
        }
    }
}

/// Output produced by a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ToolOutput {
    /// Create a simple text output.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            artifacts: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Create an error output.
    pub fn error(message: impl Into<String>) -> Self {
        let mut output = Self::text(message);
        output
            .metadata
            .insert("is_error".into(), serde_json::Value::Bool(true));
        output
    }

    /// Add an artifact to this output.
    pub fn with_artifact(mut self, artifact: Artifact) -> Self {
        self.artifacts.push(artifact);
        self
    }
}

/// An artifact produced by a tool (file created, data generated, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Artifact {
    FileCreated { path: PathBuf },
    FileModified { path: PathBuf, diff: String },
    FileDeleted { path: PathBuf },
    Data { mime_type: String, data: String },
}

/// Progress update from a running tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressUpdate {
    /// A tool execution is at a particular stage.
    ToolProgress {
        tool: String,
        stage: String,
        /// Optional completion percentage (0.0 to 1.0).
        percent: Option<f32>,
    },
    /// A file operation is in progress.
    FileOperation {
        path: PathBuf,
        operation: String,
        bytes_processed: Option<u64>,
    },
    /// A line of shell output arrived.
    ShellOutput { line: String, is_stderr: bool },
}

/// The current state of the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Planning,
    Thinking,
    Deciding,
    Executing,
    WaitingForApproval,
    WaitingForClarification,
    Complete,
    Error,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Idle => write!(f, "idle"),
            AgentStatus::Planning => write!(f, "planning"),
            AgentStatus::Thinking => write!(f, "thinking"),
            AgentStatus::Deciding => write!(f, "deciding"),
            AgentStatus::Executing => write!(f, "executing"),
            AgentStatus::WaitingForApproval => write!(f, "waiting for approval"),
            AgentStatus::WaitingForClarification => write!(f, "waiting for clarification"),
            AgentStatus::Complete => write!(f, "complete"),
            AgentStatus::Error => write!(f, "error"),
        }
    }
}

/// Cached classification of the current task, computed once at task start.
///
/// Used by `tool_routing_hint()` and `auto_correct_tool_call()` to avoid
/// repeated `.contains()` string matching on every tool call (~300 calls
/// per iteration without caching).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskClassification {
    Calendar,
    Reminders,
    Notes,
    Email,
    Music,
    AppControl,
    Clipboard,
    Screenshot,
    SystemInfo,
    Contacts,
    Safari,
    HomeKit,
    Photos,
    Voice,
    Meeting,
    DailyBriefing,
    GuiScripting,
    Accessibility,
    Browser,
    Messaging,
    Slack,
    FileOperation,
    GitOperation,
    Search,
    WebSearch,
    WebFetch,
    CodeAnalysis,
    ArxivResearch,
    KnowledgeGraph,
    ExperimentTracking,
    CodeIntelligence,
    ContentEngine,
    SkillTracker,
    CareerIntel,
    SystemMonitor,
    LifePlanner,
    PrivacyManager,
    SelfImprovement,
    Notification,
    Spotlight,
    FocusMode,
    Finder,
    Workflow(String),
    General,
}

impl TaskClassification {
    /// Classify a task description once, returning a cached classification.
    ///
    /// This replaces the ~300 `.contains()` calls that previously ran on every
    /// tool call. Call once at task start and store the result in `AgentState`.
    pub fn classify(task: &str) -> Self {
        let lower = task.to_lowercase();

        // Workflow routing (checked first — platform-independent)
        if lower.contains("security scan")
            || lower.contains("security audit")
            || lower.contains("vulnerability")
        {
            return Self::Workflow("security_scan".into());
        }
        if lower.contains("code review") {
            return Self::Workflow("code_review".into());
        }
        if lower.contains("refactor") && !lower.contains("file") {
            return Self::Workflow("refactor".into());
        }
        if lower.contains("generate test")
            || lower.contains("write test")
            || lower.contains("test generation")
        {
            return Self::Workflow("test_generation".into());
        }
        if lower.contains("generate doc") || lower.contains("write docs") {
            return Self::Workflow("documentation".into());
        }
        if lower.contains("update dependenc") || lower.contains("dependency update") {
            return Self::Workflow("dependency_update".into());
        }
        if lower.contains("deploy") {
            return Self::Workflow("deployment".into());
        }
        if lower.contains("incident response") {
            return Self::Workflow("incident_response".into());
        }
        if lower.contains("morning briefing") || lower.contains("daily briefing") {
            return Self::Workflow("morning_briefing".into());
        }
        if lower.contains("pr review") || lower.contains("pull request review") {
            return Self::Workflow("pr_review".into());
        }
        if lower.contains("dependency audit") || lower.contains("audit dependenc") {
            return Self::Workflow("dependency_audit".into());
        }
        if lower.contains("changelog") || lower.contains("release notes") {
            return Self::Workflow("changelog".into());
        }
        if lower.contains("end of day") || lower.contains("eod summary") {
            return Self::Workflow("end_of_day_summary".into());
        }
        if lower.contains("email triage") || lower.contains("triage email") {
            return Self::Workflow("email_triage".into());
        }
        if lower.contains("meeting record")
            || lower.contains("record meeting")
            || lower.contains("record the meeting")
            || lower.contains("record my meeting")
            || lower.contains("meeting transcri")
        {
            return Self::Workflow("meeting_recorder".into());
        }
        if lower.contains("app automation") || lower.contains("automate app") {
            return Self::Workflow("app_automation".into());
        }
        if lower.contains("arxiv")
            || lower.contains("research paper")
            || lower.contains("academic paper")
            || lower.contains("literature review")
        {
            return Self::Workflow("arxiv_research".into());
        }
        if lower.contains("knowledge graph") || lower.contains("concept map") {
            return Self::Workflow("knowledge_graph".into());
        }
        if lower.contains("experiment") || lower.contains("hypothesis") {
            return Self::Workflow("experiment_tracking".into());
        }
        if lower.contains("code analysis") || lower.contains("architecture review") {
            return Self::Workflow("code_analysis".into());
        }
        if lower.contains("content strategy") || lower.contains("blog pipeline") {
            return Self::Workflow("content_pipeline".into());
        }
        if lower.contains("skill assessment") || lower.contains("learning plan") {
            return Self::Workflow("skill_development".into());
        }
        if lower.contains("career planning") || lower.contains("portfolio review") {
            return Self::Workflow("career_planning".into());
        }
        if lower.contains("service monitoring") || lower.contains("health check") {
            return Self::Workflow("system_monitoring".into());
        }
        if lower.contains("daily planning") || lower.contains("productivity review") {
            return Self::Workflow("life_planning".into());
        }
        if lower.contains("privacy audit") || lower.contains("data management") {
            return Self::Workflow("privacy_audit".into());
        }
        if lower.contains("self improvement") || lower.contains("performance analysis") {
            return Self::Workflow("self_improvement_loop".into());
        }

        // Tool-specific classifications (macOS + cross-platform)
        if lower.contains("slack")
            || lower.contains("send slack")
            || lower.contains("slack message")
            || lower.contains("slack channel")
            || lower.contains("post to slack")
        {
            return Self::Slack;
        }
        if lower.contains("clipboard")
            || lower.contains("paste")
            || (lower.contains("copy") && !lower.contains("file"))
        {
            return Self::Clipboard;
        }
        if lower.contains("battery")
            || (lower.contains("system") && lower.contains("info"))
            || lower.contains("disk space")
            || lower.contains("cpu")
            || lower.contains("ram")
            || lower.contains("memory usage")
        {
            return Self::SystemInfo;
        }
        if lower.contains("running app")
            || lower.contains("open app")
            || lower.contains("launch")
            || lower.contains("quit app")
            || lower.contains("close app")
        {
            return Self::AppControl;
        }
        if (lower.contains("record") && lower.contains("meeting"))
            || lower.contains("start recording")
            || lower.contains("stop recording")
            || lower.contains("stop the recording")
            || lower.contains("stop the meeting")
            || lower.contains("transcribe meeting")
            || lower.contains("meeting transcript")
            || lower.contains("meeting status")
            || lower.contains("recording status")
        {
            return Self::Meeting;
        }
        if lower.contains("calendar")
            || lower.contains("event")
            || (lower.contains("meeting")
                && !lower.contains("record")
                && !lower.contains("transcrib")
                && !lower.contains("stop"))
        {
            return Self::Calendar;
        }
        if lower.contains("reminder") || lower.contains("todo") || lower.contains("to-do") {
            return Self::Reminders;
        }
        if lower.contains("note") && !lower.contains("notification") {
            return Self::Notes;
        }
        if lower.contains("screenshot")
            || lower.contains("screen capture")
            || lower.contains("screen shot")
        {
            return Self::Screenshot;
        }
        if lower.contains("notification") || lower.contains("notify") || lower.contains("alert me")
        {
            return Self::Notification;
        }
        if lower.contains("spotlight")
            || lower.contains("find file")
            || lower.contains("search for file")
            || lower.contains("locate file")
        {
            return Self::Spotlight;
        }
        if lower.contains("do not disturb") || lower.contains("focus mode") || lower.contains("dnd")
        {
            return Self::FocusMode;
        }
        if lower.contains("music")
            || lower.contains("song")
            || lower.contains("play ")
            || lower.contains("pause")
            || lower.contains("now playing")
        {
            return Self::Music;
        }
        if lower.contains("mail") || lower.contains("email") || lower.contains("inbox") {
            return Self::Email;
        }
        if lower.contains("finder") || lower.contains("trash") || lower.contains("reveal in") {
            return Self::Finder;
        }
        if lower.contains("contact") && !lower.contains("file") {
            return Self::Contacts;
        }
        if lower.contains("search the web")
            || lower.contains("web search")
            || lower.contains("search online")
            || lower.contains("look up")
            || lower.contains("google")
            || (lower.contains("search") && lower.contains("internet"))
        {
            return Self::WebSearch;
        }
        if lower.contains("fetch")
            && (lower.contains("url")
                || lower.contains("http")
                || lower.contains("page")
                || lower.contains("website"))
        {
            return Self::WebFetch;
        }
        if lower.contains("safari") || (lower.contains("browser") && lower.contains("tab")) {
            return Self::Safari;
        }
        if lower.contains("imessage")
            || lower.contains("text message")
            || lower.contains("send message")
            || lower.contains("sms")
        {
            return Self::Messaging;
        }
        // ArXiv (tool-level, not workflow-level)
        if lower.contains("arxiv")
            || lower.contains("scientific paper")
            || lower.contains("paper search")
            || lower.contains("paper summary")
            || lower.contains("paper to code")
            || lower.contains("paper to notebook")
            || lower.contains("bibtex")
            || lower.contains("preprint")
            || (lower.contains("paper")
                && (lower.contains("search")
                    || lower.contains("find")
                    || lower.contains("top")
                    || lower.contains("latest")
                    || lower.contains("recent")
                    || lower.contains("trending")))
            || (lower.contains("papers")
                && (lower.contains("search")
                    || lower.contains("find")
                    || lower.contains("about")
                    || lower.contains("top")
                    || lower.contains("latest")
                    || lower.contains("recent")
                    || lower.contains("trending")))
        {
            return Self::ArxivResearch;
        }
        if lower.contains("knowledge graph")
            || lower.contains("concept")
            || lower.contains("citation")
            || lower.contains("paper relationship")
        {
            return Self::KnowledgeGraph;
        }
        if lower.contains("experiment")
            || lower.contains("hypothesis")
            || lower.contains("test result")
            || lower.contains("lab ")
        {
            return Self::ExperimentTracking;
        }
        if lower.contains("code architecture")
            || lower.contains("tech debt")
            || lower.contains("translate code")
            || lower.contains("api surface")
            || lower.contains("pattern detection")
        {
            return Self::CodeIntelligence;
        }
        if lower.contains("blog")
            || lower.contains("content")
            || lower.contains("article")
            || lower.contains("publish")
            || lower.contains("twitter")
            || lower.contains("linkedin")
            || lower.contains("newsletter")
        {
            return Self::ContentEngine;
        }
        if lower.contains("skill")
            || lower.contains("learning")
            || lower.contains("practice")
            || lower.contains("proficiency")
            || lower.contains("knowledge gap")
        {
            return Self::SkillTracker;
        }
        if lower.contains("career")
            || lower.contains("achievement")
            || lower.contains("portfolio")
            || lower.contains("job")
            || lower.contains("resume")
            || lower.contains("networking")
        {
            return Self::CareerIntel;
        }
        if lower.contains("service monitor")
            || lower.contains("health check")
            || lower.contains("incident")
            || lower.contains("topology")
            || lower.contains("runbook")
        {
            return Self::SystemMonitor;
        }
        if lower.contains("schedule")
            || lower.contains("deadline")
            || lower.contains("habit")
            || lower.contains("energy")
            || lower.contains("daily plan")
            || lower.contains("weekly review")
            || lower.contains("context switch")
        {
            return Self::LifePlanner;
        }
        if lower.contains("privacy")
            || lower.contains("data boundary")
            || lower.contains("encrypt")
            || lower.contains("delete data")
            || lower.contains("compliance")
            || lower.contains("audit access")
        {
            return Self::PrivacyManager;
        }
        if lower.contains("usage pattern")
            || lower.contains("performance")
            || lower.contains("cognitive load")
            || lower.contains("preference")
            || lower.contains("feedback")
            || lower.contains("self-improvement")
        {
            return Self::SelfImprovement;
        }

        Self::General
    }
}

/// Tracks the full state of the agent during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub task_id: Option<Uuid>,
    pub status: AgentStatus,
    pub current_goal: Option<String>,
    pub iteration: usize,
    pub max_iterations: usize,
    pub checkpoints: Vec<String>,
    /// Cached task classification, computed once at task start.
    #[serde(default)]
    pub task_classification: Option<TaskClassification>,
}

impl AgentState {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            task_id: None,
            status: AgentStatus::Idle,
            current_goal: None,
            iteration: 0,
            max_iterations,
            checkpoints: Vec::new(),
            task_classification: None,
        }
    }

    pub fn start_task(&mut self, goal: impl Into<String>) {
        let goal_str = goal.into();
        self.task_id = Some(Uuid::new_v4());
        self.status = AgentStatus::Thinking;
        self.task_classification = Some(TaskClassification::classify(&goal_str));
        self.current_goal = Some(goal_str);
        self.iteration = 0;
        self.checkpoints.clear();
    }

    pub fn increment_iteration(&mut self) -> bool {
        self.iteration += 1;
        self.iteration <= self.max_iterations
    }

    pub fn complete(&mut self) {
        self.status = AgentStatus::Complete;
    }

    pub fn set_error(&mut self) {
        self.status = AgentStatus::Error;
    }

    pub fn reset(&mut self) {
        self.task_id = None;
        self.status = AgentStatus::Idle;
        self.current_goal = None;
        self.iteration = 0;
        self.checkpoints.clear();
        self.task_classification = None;
    }
}

/// Token usage statistics from an LLM call.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

impl TokenUsage {
    pub fn total(&self) -> usize {
        self.input_tokens + self.output_tokens
    }

    pub fn accumulate(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

/// Cost tracking for LLM usage.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CostEstimate {
    pub input_cost: f64,
    pub output_cost: f64,
}

impl CostEstimate {
    pub fn total(&self) -> f64 {
        self.input_cost + self.output_cost
    }

    pub fn accumulate(&mut self, other: &CostEstimate) {
        self.input_cost += other.input_cost;
        self.output_cost += other.output_cost;
    }
}

/// A stream event received during LLM response streaming.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ToolCallStart {
        id: String,
        name: String,
        /// Raw provider-specific function call data (e.g., Gemini's thought_signature).
        /// Stored in message metadata to echo back in subsequent requests.
        raw_function_call: Option<serde_json::Value>,
    },
    ToolCallDelta {
        id: String,
        arguments_delta: String,
    },
    ToolCallEnd {
        id: String,
    },
    Done {
        usage: TokenUsage,
    },
    Error(String),
}

/// The result of an LLM completion request.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: Message,
    pub usage: TokenUsage,
    pub model: String,
    pub finish_reason: Option<String>,
}

/// A request to the LLM for completion.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
    pub stop_sequences: Vec<String>,
    pub model: Option<String>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tools: None,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: Vec::new(),
            model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello, world!");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content.as_text(), Some("Hello, world!"));
        assert!(msg.metadata.is_empty());
    }

    #[test]
    fn test_message_with_metadata() {
        let msg = Message::assistant("Response").with_metadata("model", serde_json::json!("gpt-4"));
        assert_eq!(msg.metadata.get("model"), Some(&serde_json::json!("gpt-4")));
    }

    #[test]
    fn test_system_message() {
        let msg = Message::system("You are a helpful assistant.");
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.content.as_text(), Some("You are a helpful assistant."));
    }

    #[test]
    fn test_tool_result_message() {
        let msg = Message::tool_result("call-1", "file contents here", false);
        match &msg.content {
            Content::ToolResult {
                call_id,
                output,
                is_error,
            } => {
                assert_eq!(call_id, "call-1");
                assert_eq!(output, "file contents here");
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult content"),
        }
    }

    #[test]
    fn test_content_variants() {
        let text = Content::text("hello");
        assert_eq!(text.as_text(), Some("hello"));

        let tool_call = Content::tool_call("id1", "file_read", serde_json::json!({"path": "/tmp"}));
        assert_eq!(tool_call.as_text(), None);

        let tool_result = Content::tool_result("id1", "contents", false);
        assert_eq!(tool_result.as_text(), None);
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::System.to_string(), "system");
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Assistant.to_string(), "assistant");
        assert_eq!(Role::Tool.to_string(), "tool");
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::ReadOnly < RiskLevel::Write);
        assert!(RiskLevel::Write < RiskLevel::Execute);
        assert!(RiskLevel::Execute < RiskLevel::Network);
        assert!(RiskLevel::Network < RiskLevel::Destructive);
    }

    #[test]
    fn test_tool_output() {
        let output = ToolOutput::text("hello");
        assert_eq!(output.content, "hello");
        assert!(output.artifacts.is_empty());

        let output = ToolOutput::error("something went wrong");
        assert_eq!(output.content, "something went wrong");
        assert_eq!(
            output.metadata.get("is_error"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[test]
    fn test_tool_output_with_artifact() {
        let output = ToolOutput::text("created file").with_artifact(Artifact::FileCreated {
            path: "/tmp/test.rs".into(),
        });
        assert_eq!(output.artifacts.len(), 1);
    }

    #[test]
    fn test_agent_state_lifecycle() {
        let mut state = AgentState::new(25);
        assert_eq!(state.status, AgentStatus::Idle);
        assert!(state.task_id.is_none());

        state.start_task("refactor auth module");
        assert_eq!(state.status, AgentStatus::Thinking);
        assert!(state.task_id.is_some());
        assert_eq!(state.current_goal.as_deref(), Some("refactor auth module"));
        assert_eq!(state.iteration, 0);

        assert!(state.increment_iteration());
        assert_eq!(state.iteration, 1);

        state.complete();
        assert_eq!(state.status, AgentStatus::Complete);

        state.reset();
        assert_eq!(state.status, AgentStatus::Idle);
        assert!(state.task_id.is_none());
    }

    #[test]
    fn test_agent_state_max_iterations() {
        let mut state = AgentState::new(2);
        state.start_task("test");

        assert!(state.increment_iteration()); // 1 <= 2
        assert!(state.increment_iteration()); // 2 <= 2
        assert!(!state.increment_iteration()); // 3 > 2
    }

    #[test]
    fn test_token_usage() {
        let mut usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        assert_eq!(usage.total(), 150);

        let other = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
        };
        usage.accumulate(&other);
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 150);
        assert_eq!(usage.total(), 450);
    }

    #[test]
    fn test_cost_estimate() {
        let mut cost = CostEstimate {
            input_cost: 0.01,
            output_cost: 0.03,
        };
        assert!((cost.total() - 0.04).abs() < f64::EPSILON);

        let other = CostEstimate {
            input_cost: 0.02,
            output_cost: 0.06,
        };
        cost.accumulate(&other);
        assert!((cost.input_cost - 0.03).abs() < f64::EPSILON);
        assert!((cost.output_cost - 0.09).abs() < f64::EPSILON);
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = Message::user("test message");
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role::User);
        assert_eq!(deserialized.content.as_text(), Some("test message"));
    }

    #[test]
    fn test_tool_definition_serialization() {
        let def = ToolDefinition {
            name: "file_read".into(),
            description: "Read a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        };
        let json = serde_json::to_string(&def).unwrap();
        let deserialized: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "file_read");
    }

    #[test]
    fn test_completion_request_default() {
        let req = CompletionRequest::default();
        assert!(req.messages.is_empty());
        assert!(req.tools.is_none());
        assert!((req.temperature - 0.7).abs() < f32::EPSILON);
        assert!(req.max_tokens.is_none());
        assert!(req.stop_sequences.is_empty());
        assert!(req.model.is_none());
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Idle.to_string(), "idle");
        assert_eq!(AgentStatus::Planning.to_string(), "planning");
        assert_eq!(AgentStatus::Thinking.to_string(), "thinking");
        assert_eq!(
            AgentStatus::WaitingForApproval.to_string(),
            "waiting for approval"
        );
        assert_eq!(
            AgentStatus::WaitingForClarification.to_string(),
            "waiting for clarification"
        );
    }
}
