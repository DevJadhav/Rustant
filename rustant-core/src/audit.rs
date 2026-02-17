//! Audit trail — unified execution trace, querying, export, and analytics.
//!
//! Provides a complete audit system that unifies safety audit events,
//! tool execution records, and token/cost tracking into a single
//! queryable, exportable trace.

use crate::merkle::{MerkleChain, VerificationResult};
use crate::safety::AuditEvent;
use crate::types::{CostEstimate, RiskLevel, TokenUsage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TraceEvent + TraceEventKind
// ---------------------------------------------------------------------------

/// A single event within an execution trace, tagged with a monotonically
/// increasing sequence number and a wall-clock timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Monotonically increasing sequence number within the trace.
    pub sequence: usize,
    /// Wall-clock time at which the event was recorded.
    pub timestamp: DateTime<Utc>,
    /// The payload describing what happened.
    pub kind: TraceEventKind,
}

/// Discriminated union of all event kinds that may appear in a trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceEventKind {
    TaskStarted {
        task_id: Uuid,
        goal: String,
    },
    TaskCompleted {
        task_id: Uuid,
        success: bool,
        iterations: usize,
    },
    ToolRequested {
        tool: String,
        risk_level: RiskLevel,
        args_summary: String,
    },
    ToolApproved {
        tool: String,
    },
    ToolDenied {
        tool: String,
        reason: String,
    },
    ApprovalRequested {
        tool: String,
        context: String,
    },
    ApprovalDecision {
        tool: String,
        approved: bool,
    },
    ToolExecuted {
        tool: String,
        success: bool,
        duration_ms: u64,
        output_preview: String,
    },
    LlmCall {
        model: String,
        input_tokens: usize,
        output_tokens: usize,
        cost: f64,
    },
    StatusChange {
        from: String,
        to: String,
    },
    Error {
        message: String,
    },
}

impl TraceEventKind {
    /// Convert a safety-layer [`AuditEvent`] into the corresponding
    /// [`TraceEventKind`].
    pub fn from_audit_event(event: &AuditEvent) -> Self {
        match event {
            AuditEvent::ActionRequested {
                tool,
                risk_level,
                description,
            } => TraceEventKind::ToolRequested {
                tool: tool.clone(),
                risk_level: *risk_level,
                args_summary: description.clone(),
            },
            AuditEvent::ActionApproved { tool } => {
                TraceEventKind::ToolApproved { tool: tool.clone() }
            }
            AuditEvent::ActionDenied { tool, reason } => TraceEventKind::ToolDenied {
                tool: tool.clone(),
                reason: reason.clone(),
            },
            AuditEvent::ActionExecuted {
                tool,
                success,
                duration_ms,
            } => TraceEventKind::ToolExecuted {
                tool: tool.clone(),
                success: *success,
                duration_ms: *duration_ms,
                output_preview: String::new(),
            },
            AuditEvent::ApprovalRequested { tool, context } => TraceEventKind::ApprovalRequested {
                tool: tool.clone(),
                context: context.clone(),
            },
            AuditEvent::ApprovalDecision { tool, approved } => TraceEventKind::ApprovalDecision {
                tool: tool.clone(),
                approved: *approved,
            },
        }
    }

    /// Return the event type as a human-readable tag (e.g. `"tool_requested"`).
    fn type_tag(&self) -> &'static str {
        match self {
            TraceEventKind::TaskStarted { .. } => "task_started",
            TraceEventKind::TaskCompleted { .. } => "task_completed",
            TraceEventKind::ToolRequested { .. } => "tool_requested",
            TraceEventKind::ToolApproved { .. } => "tool_approved",
            TraceEventKind::ToolDenied { .. } => "tool_denied",
            TraceEventKind::ApprovalRequested { .. } => "approval_requested",
            TraceEventKind::ApprovalDecision { .. } => "approval_decision",
            TraceEventKind::ToolExecuted { .. } => "tool_executed",
            TraceEventKind::LlmCall { .. } => "llm_call",
            TraceEventKind::StatusChange { .. } => "status_change",
            TraceEventKind::Error { .. } => "error",
        }
    }

    /// Return the tool name referenced by this event, if any.
    fn tool_name(&self) -> Option<&str> {
        match self {
            TraceEventKind::ToolRequested { tool, .. }
            | TraceEventKind::ToolApproved { tool }
            | TraceEventKind::ToolDenied { tool, .. }
            | TraceEventKind::ApprovalRequested { tool, .. }
            | TraceEventKind::ApprovalDecision { tool, .. }
            | TraceEventKind::ToolExecuted { tool, .. } => Some(tool),
            _ => None,
        }
    }

    /// Produce a short, single-line human-readable summary of the event.
    fn summary(&self) -> String {
        match self {
            TraceEventKind::TaskStarted { goal, .. } => format!("Task started: {}", goal),
            TraceEventKind::TaskCompleted {
                success,
                iterations,
                ..
            } => {
                let tag = if *success { "SUCCESS" } else { "FAILED" };
                format!("Task completed [{}] after {} iterations", tag, iterations)
            }
            TraceEventKind::ToolRequested {
                tool, risk_level, ..
            } => format!("Tool requested: {} (risk: {})", tool, risk_level),
            TraceEventKind::ToolApproved { tool } => format!("Tool approved: {}", tool),
            TraceEventKind::ToolDenied { tool, reason } => {
                format!("Tool denied: {} — {}", tool, reason)
            }
            TraceEventKind::ApprovalRequested { tool, .. } => {
                format!("Approval requested for: {}", tool)
            }
            TraceEventKind::ApprovalDecision { tool, approved } => {
                let decision = if *approved { "approved" } else { "denied" };
                format!("Approval decision for {}: {}", tool, decision)
            }
            TraceEventKind::ToolExecuted {
                tool,
                success,
                duration_ms,
                ..
            } => {
                let tag = if *success { "OK" } else { "ERR" };
                format!("Tool executed: {} [{}] ({}ms)", tool, tag, duration_ms)
            }
            TraceEventKind::LlmCall {
                model,
                input_tokens,
                output_tokens,
                cost,
            } => format!(
                "LLM call: {} ({}/{} tokens, ${:.4})",
                model, input_tokens, output_tokens, cost
            ),
            TraceEventKind::StatusChange { from, to } => {
                format!("Status: {} -> {}", from, to)
            }
            TraceEventKind::Error { message } => format!("Error: {}", message),
        }
    }

    /// Extract CSV detail string (tool name + extra info) for tabular export.
    fn csv_details(&self) -> (String, String) {
        match self {
            TraceEventKind::TaskStarted { goal, .. } => (String::new(), goal.clone()),
            TraceEventKind::TaskCompleted {
                success,
                iterations,
                ..
            } => (
                String::new(),
                format!("success={} iterations={}", success, iterations),
            ),
            TraceEventKind::ToolRequested {
                tool,
                risk_level,
                args_summary,
            } => (
                tool.clone(),
                format!("risk={} args={}", risk_level, args_summary),
            ),
            TraceEventKind::ToolApproved { tool } => (tool.clone(), String::new()),
            TraceEventKind::ToolDenied { tool, reason } => (tool.clone(), reason.clone()),
            TraceEventKind::ApprovalRequested { tool, context } => (tool.clone(), context.clone()),
            TraceEventKind::ApprovalDecision { tool, approved } => {
                (tool.clone(), format!("approved={}", approved))
            }
            TraceEventKind::ToolExecuted {
                tool,
                success,
                duration_ms,
                output_preview,
            } => (
                tool.clone(),
                format!(
                    "success={} duration_ms={} output={}",
                    success, duration_ms, output_preview
                ),
            ),
            TraceEventKind::LlmCall {
                model,
                input_tokens,
                output_tokens,
                cost,
            } => (
                String::new(),
                format!(
                    "model={} in={} out={} cost={:.6}",
                    model, input_tokens, output_tokens, cost
                ),
            ),
            TraceEventKind::StatusChange { from, to } => {
                (String::new(), format!("{} -> {}", from, to))
            }
            TraceEventKind::Error { message } => (String::new(), message.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// ExecutionTrace
// ---------------------------------------------------------------------------

/// The full execution history of a single task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub trace_id: Uuid,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub goal: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub success: Option<bool>,
    pub iterations: usize,
    pub events: Vec<TraceEvent>,
    pub total_usage: TokenUsage,
    pub total_cost: CostEstimate,
}

impl ExecutionTrace {
    /// Create a new, in-progress execution trace.
    pub fn new(session_id: Uuid, task_id: Uuid, goal: impl Into<String>) -> Self {
        let goal = goal.into();
        let now = Utc::now();
        let mut trace = Self {
            trace_id: Uuid::new_v4(),
            session_id,
            task_id,
            goal: goal.clone(),
            started_at: now,
            completed_at: None,
            success: None,
            iterations: 0,
            events: Vec::new(),
            total_usage: TokenUsage::default(),
            total_cost: CostEstimate::default(),
        };
        // Record the initial task-started event.
        trace.push_event(TraceEventKind::TaskStarted { task_id, goal });
        trace
    }

    /// Append an event to the trace with an auto-assigned sequence number.
    pub fn push_event(&mut self, kind: TraceEventKind) {
        let seq = self.events.len();
        self.events.push(TraceEvent {
            sequence: seq,
            timestamp: Utc::now(),
            kind,
        });
    }

    /// Mark the trace as completed.
    pub fn complete(&mut self, success: bool) {
        self.completed_at = Some(Utc::now());
        self.success = Some(success);
        self.push_event(TraceEventKind::TaskCompleted {
            task_id: self.task_id,
            success,
            iterations: self.iterations,
        });
    }

    /// Compute the wall-clock duration in milliseconds, if completed.
    pub fn duration_ms(&self) -> Option<u64> {
        self.completed_at.map(|end| {
            let dur = end - self.started_at;
            dur.num_milliseconds().max(0) as u64
        })
    }

    /// Return references to all tool-related events.
    pub fn tool_events(&self) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    TraceEventKind::ToolRequested { .. }
                        | TraceEventKind::ToolApproved { .. }
                        | TraceEventKind::ToolDenied { .. }
                        | TraceEventKind::ToolExecuted { .. }
                )
            })
            .collect()
    }

    /// Return references to all error events.
    pub fn error_events(&self) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| matches!(e.kind, TraceEventKind::Error { .. }))
            .collect()
    }

    /// Return references to all LLM call events.
    pub fn llm_events(&self) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| matches!(e.kind, TraceEventKind::LlmCall { .. }))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// AuditError
// ---------------------------------------------------------------------------

/// Errors that may occur during audit operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuditError {
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    #[error("io error: {0}")]
    IoError(String),
    #[error("store is empty")]
    EmptyStore,
    #[error("trace not found: {0}")]
    TraceNotFound(Uuid),
}

// ---------------------------------------------------------------------------
// AuditStore
// ---------------------------------------------------------------------------

/// Persistent, capacity-bounded store of execution traces.
pub struct AuditStore {
    traces: Vec<ExecutionTrace>,
    max_traces: usize,
    merkle_chain: Option<MerkleChain>,
}

impl AuditStore {
    /// Create a new store with the default capacity of 1 000 traces.
    pub fn new() -> Self {
        Self {
            traces: Vec::new(),
            max_traces: 1000,
            merkle_chain: None,
        }
    }

    /// Create a new store with Merkle chain integrity verification enabled.
    pub fn with_merkle_chain() -> Self {
        Self {
            traces: Vec::new(),
            max_traces: 1000,
            merkle_chain: Some(MerkleChain::new()),
        }
    }

    /// Add a trace to the store, evicting the oldest trace when capacity is
    /// reached. If the Merkle chain is enabled, the serialized trace is
    /// appended to the chain for tamper-evident integrity.
    pub fn add_trace(&mut self, trace: ExecutionTrace) {
        // Append to Merkle chain if enabled
        if let Some(ref mut chain) = self.merkle_chain
            && let Ok(serialized) = serde_json::to_vec(&trace) {
                chain.append(&serialized);
            }
        if self.traces.len() >= self.max_traces {
            self.traces.remove(0);
        }
        self.traces.push(trace);
    }

    /// Verify the integrity of the Merkle chain.
    ///
    /// Returns `Some(VerificationResult)` if the chain is enabled, or `None`.
    pub fn verify_integrity(&self) -> Option<VerificationResult> {
        self.merkle_chain.as_ref().map(|chain| chain.verify_chain())
    }

    /// Return the root hash of the Merkle chain, if enabled.
    pub fn merkle_root_hash(&self) -> Option<String> {
        self.merkle_chain
            .as_ref()
            .and_then(|chain| chain.root_hash().map(|h| h.to_string()))
    }

    /// Access the underlying Merkle chain, if present.
    pub fn merkle_chain(&self) -> Option<&MerkleChain> {
        self.merkle_chain.as_ref()
    }

    /// Get a slice of all stored traces.
    pub fn traces(&self) -> &[ExecutionTrace] {
        &self.traces
    }

    /// Look up a trace by its `trace_id`.
    pub fn get_trace(&self, trace_id: Uuid) -> Option<&ExecutionTrace> {
        self.traces.iter().find(|t| t.trace_id == trace_id)
    }

    /// Run a structured query against the store and return matching traces.
    pub fn query(&self, query: &AuditQuery) -> Vec<&ExecutionTrace> {
        self.traces.iter().filter(|t| query.matches(t)).collect()
    }

    /// Return the *n* most recently added traces (most recent last).
    pub fn latest(&self, n: usize) -> Vec<&ExecutionTrace> {
        let start = self.traces.len().saturating_sub(n);
        self.traces[start..].iter().collect()
    }

    /// Persist the store to a JSON file at the given `path`.
    pub fn save(&self, path: &Path) -> Result<(), AuditError> {
        let json = serde_json::to_string_pretty(&self.traces)
            .map_err(|e| AuditError::SerializationFailed(e.to_string()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AuditError::IoError(e.to_string()))?;
        }

        std::fs::write(path, json).map_err(|e| AuditError::IoError(e.to_string()))?;
        Ok(())
    }

    /// Load a store from a JSON file at the given `path`.
    pub fn load(path: &Path) -> Result<Self, AuditError> {
        let json = std::fs::read_to_string(path).map_err(|e| AuditError::IoError(e.to_string()))?;

        let traces: Vec<ExecutionTrace> = serde_json::from_str(&json)
            .map_err(|e| AuditError::SerializationFailed(e.to_string()))?;

        Ok(Self {
            max_traces: 1000,
            traces,
            merkle_chain: None,
        })
    }

    /// Load a store from a JSON file and rebuild the Merkle chain from
    /// the loaded traces.
    pub fn load_with_merkle(path: &Path) -> Result<Self, AuditError> {
        let mut store = Self::load(path)?;
        let mut chain = MerkleChain::new();
        for trace in &store.traces {
            if let Ok(serialized) = serde_json::to_vec(trace) {
                chain.append(&serialized);
            }
        }
        store.merkle_chain = Some(chain);
        Ok(store)
    }

    /// Return the number of stored traces.
    pub fn len(&self) -> usize {
        self.traces.len()
    }

    /// Check whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }
}

impl Default for AuditStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AuditQuery
// ---------------------------------------------------------------------------

/// Filtering criteria for querying the [`AuditStore`].
///
/// Uses a builder pattern so callers can compose predicates fluently:
/// ```ignore
/// let q = AuditQuery::new()
///     .for_session(session_id)
///     .min_risk(RiskLevel::Execute)
///     .successful();
/// ```
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    pub session_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub tool_name: Option<String>,
    pub risk_level_min: Option<RiskLevel>,
    pub success_only: Option<bool>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

impl AuditQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_session(mut self, id: Uuid) -> Self {
        self.session_id = Some(id);
        self
    }

    pub fn for_task(mut self, id: Uuid) -> Self {
        self.task_id = Some(id);
        self
    }

    pub fn for_tool(mut self, name: impl Into<String>) -> Self {
        self.tool_name = Some(name.into());
        self
    }

    pub fn min_risk(mut self, level: RiskLevel) -> Self {
        self.risk_level_min = Some(level);
        self
    }

    pub fn successful(mut self) -> Self {
        self.success_only = Some(true);
        self
    }

    pub fn failed(mut self) -> Self {
        self.success_only = Some(false);
        self
    }

    pub fn since(mut self, dt: DateTime<Utc>) -> Self {
        self.since = Some(dt);
        self
    }

    pub fn until(mut self, dt: DateTime<Utc>) -> Self {
        self.until = Some(dt);
        self
    }

    /// Determine whether a given [`ExecutionTrace`] satisfies every non-`None`
    /// predicate in this query.
    fn matches(&self, trace: &ExecutionTrace) -> bool {
        if let Some(sid) = self.session_id
            && trace.session_id != sid {
                return false;
            }
        if let Some(tid) = self.task_id
            && trace.task_id != tid {
                return false;
            }
        if let Some(ref tool) = self.tool_name {
            let has_tool = trace
                .events
                .iter()
                .any(|e| e.kind.tool_name() == Some(tool.as_str()));
            if !has_tool {
                return false;
            }
        }
        if let Some(min_risk) = self.risk_level_min {
            let has_risk = trace.events.iter().any(|e| {
                if let TraceEventKind::ToolRequested { risk_level, .. } = &e.kind {
                    *risk_level >= min_risk
                } else {
                    false
                }
            });
            if !has_risk {
                return false;
            }
        }
        if let Some(want_success) = self.success_only {
            match trace.success {
                Some(s) if s == want_success => {}
                _ => return false,
            }
        }
        if let Some(since) = self.since
            && trace.started_at < since {
                return false;
            }
        if let Some(until) = self.until
            && trace.started_at > until {
                return false;
            }
        true
    }
}

// ---------------------------------------------------------------------------
// AuditExporter
// ---------------------------------------------------------------------------

/// Stateless exporter capable of rendering execution traces in several
/// formats.
pub struct AuditExporter;

impl AuditExporter {
    /// Export trace(s) to pretty-printed JSON.
    pub fn to_json(traces: &[&ExecutionTrace]) -> Result<String, AuditError> {
        serde_json::to_string_pretty(traces)
            .map_err(|e| AuditError::SerializationFailed(e.to_string()))
    }

    /// Export trace(s) to JSON Lines format (one JSON object per line).
    pub fn to_jsonl(traces: &[&ExecutionTrace]) -> Result<String, AuditError> {
        let mut buf = String::new();
        for trace in traces {
            let line = serde_json::to_string(trace)
                .map_err(|e| AuditError::SerializationFailed(e.to_string()))?;
            buf.push_str(&line);
            buf.push('\n');
        }
        Ok(buf)
    }

    /// Export to a human-readable text summary.
    pub fn to_text(traces: &[&ExecutionTrace]) -> String {
        let mut buf = String::new();
        for trace in traces {
            buf.push_str(&format!(
                "Trace {} | Task: {}\n",
                trace.trace_id, trace.goal
            ));
            buf.push_str(&format!(
                "Started: {} | Completed: {} | Duration: {}ms\n",
                trace.started_at.to_rfc3339(),
                trace
                    .completed_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "in-progress".to_string()),
                trace
                    .duration_ms()
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
            ));
            buf.push_str(&format!(
                "Iterations: {} | Tokens: {}/{} | Cost: ${:.4}\n",
                trace.iterations,
                trace.total_usage.input_tokens,
                trace.total_usage.output_tokens,
                trace.total_cost.total(),
            ));
            buf.push_str("Events:\n");
            for event in &trace.events {
                buf.push_str(&format!(
                    "  [{}] {} {}\n",
                    event.sequence,
                    event.timestamp.to_rfc3339(),
                    event.kind.summary()
                ));
            }
            buf.push_str("---\n");
        }
        buf
    }

    /// Export to CSV (one row per trace event).
    ///
    /// Columns: `trace_id,sequence,timestamp,event_type,tool,details`
    pub fn to_csv(traces: &[&ExecutionTrace]) -> String {
        let mut buf = String::from("trace_id,sequence,timestamp,event_type,tool,details\n");
        for trace in traces {
            for event in &trace.events {
                let (tool, details) = event.kind.csv_details();
                buf.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    trace.trace_id,
                    event.sequence,
                    event.timestamp.to_rfc3339(),
                    event.kind.type_tag(),
                    csv_escape(&tool),
                    csv_escape(&details),
                ));
            }
        }
        buf
    }
}

/// Minimal CSV field escaping: wrap the value in double-quotes if it contains
/// a comma, newline, or double-quote, doubling any embedded double-quotes.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        let escaped = value.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        value.to_string()
    }
}

// ---------------------------------------------------------------------------
// Analytics helpers
// ---------------------------------------------------------------------------

/// Summary of tool usage across a set of execution traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageSummary {
    pub tool_counts: HashMap<String, usize>,
    pub tool_success_rates: HashMap<String, f64>,
    pub tool_avg_duration_ms: HashMap<String, f64>,
    pub most_used: Option<String>,
    pub most_denied: Option<String>,
}

/// Per-model cost and token breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCostEntry {
    pub calls: usize,
    pub total_tokens: usize,
    pub total_cost: f64,
}

/// Aggregate cost breakdown across all models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub total_cost: f64,
    pub total_tokens: usize,
    pub by_model: HashMap<String, ModelCostEntry>,
}

/// A detected pattern or anomaly in the audit data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub kind: PatternKind,
    pub description: String,
    pub occurrences: usize,
}

/// Enumeration of detectable pattern kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    FrequentDenial,
    ApprovalBottleneck,
    HighCostTool,
    RepeatedError,
    SlowTool,
}

/// Stateless analytics engine operating over slices of execution traces.
pub struct Analytics;

impl Analytics {
    /// Compute a summary of tool usage across the provided traces.
    pub fn tool_usage_summary(traces: &[&ExecutionTrace]) -> ToolUsageSummary {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut successes: HashMap<String, usize> = HashMap::new();
        let mut exec_counts: HashMap<String, usize> = HashMap::new();
        let mut durations: HashMap<String, Vec<u64>> = HashMap::new();
        let mut denials: HashMap<String, usize> = HashMap::new();

        for trace in traces {
            for event in &trace.events {
                match &event.kind {
                    TraceEventKind::ToolRequested { tool, .. } => {
                        *counts.entry(tool.clone()).or_insert(0) += 1;
                    }
                    TraceEventKind::ToolExecuted {
                        tool,
                        success,
                        duration_ms,
                        ..
                    } => {
                        *exec_counts.entry(tool.clone()).or_insert(0) += 1;
                        if *success {
                            *successes.entry(tool.clone()).or_insert(0) += 1;
                        }
                        durations
                            .entry(tool.clone())
                            .or_default()
                            .push(*duration_ms);
                    }
                    TraceEventKind::ToolDenied { tool, .. } => {
                        *denials.entry(tool.clone()).or_insert(0) += 1;
                    }
                    _ => {}
                }
            }
        }

        let tool_success_rates: HashMap<String, f64> = exec_counts
            .iter()
            .map(|(tool, &total)| {
                let ok = *successes.get(tool).unwrap_or(&0);
                let rate = if total > 0 {
                    ok as f64 / total as f64
                } else {
                    0.0
                };
                (tool.clone(), rate)
            })
            .collect();

        let tool_avg_duration_ms: HashMap<String, f64> = durations
            .iter()
            .map(|(tool, durs)| {
                let avg = if durs.is_empty() {
                    0.0
                } else {
                    durs.iter().sum::<u64>() as f64 / durs.len() as f64
                };
                (tool.clone(), avg)
            })
            .collect();

        let most_used = counts
            .iter()
            .max_by_key(|&(_, &c)| c)
            .map(|(t, _)| t.clone());

        let most_denied = denials
            .iter()
            .max_by_key(|&(_, &c)| c)
            .map(|(t, _)| t.clone());

        ToolUsageSummary {
            tool_counts: counts,
            tool_success_rates,
            tool_avg_duration_ms,
            most_used,
            most_denied,
        }
    }

    /// Compute cost breakdown by model.
    pub fn cost_breakdown(traces: &[&ExecutionTrace]) -> CostBreakdown {
        let mut by_model: HashMap<String, ModelCostEntry> = HashMap::new();
        let mut total_cost = 0.0_f64;
        let mut total_tokens = 0_usize;

        for trace in traces {
            for event in &trace.events {
                if let TraceEventKind::LlmCall {
                    model,
                    input_tokens,
                    output_tokens,
                    cost,
                } = &event.kind
                {
                    let tokens = input_tokens + output_tokens;
                    total_cost += cost;
                    total_tokens += tokens;

                    let entry = by_model.entry(model.clone()).or_insert(ModelCostEntry {
                        calls: 0,
                        total_tokens: 0,
                        total_cost: 0.0,
                    });
                    entry.calls += 1;
                    entry.total_tokens += tokens;
                    entry.total_cost += cost;
                }
            }
        }

        CostBreakdown {
            total_cost,
            total_tokens,
            by_model,
        }
    }

    /// Detect patterns such as frequent denials, approval bottlenecks, slow
    /// tools, repeated errors, and high-cost tools.
    pub fn detect_patterns(traces: &[&ExecutionTrace]) -> Vec<Pattern> {
        let mut patterns = Vec::new();

        // --- Frequent denial ---
        let mut denial_counts: HashMap<String, usize> = HashMap::new();
        // --- Approval bottleneck ---
        let mut approval_counts: HashMap<String, usize> = HashMap::new();
        // --- Slow tools ---
        let mut durations: HashMap<String, Vec<u64>> = HashMap::new();
        // --- Repeated errors ---
        let mut error_counts: HashMap<String, usize> = HashMap::new();
        // --- High-cost tools ---
        let mut tool_costs: HashMap<String, f64> = HashMap::new();

        for trace in traces {
            for event in &trace.events {
                match &event.kind {
                    TraceEventKind::ToolDenied { tool, .. } => {
                        *denial_counts.entry(tool.clone()).or_insert(0) += 1;
                    }
                    TraceEventKind::ApprovalRequested { tool, .. } => {
                        *approval_counts.entry(tool.clone()).or_insert(0) += 1;
                    }
                    TraceEventKind::ToolExecuted {
                        tool, duration_ms, ..
                    } => {
                        durations
                            .entry(tool.clone())
                            .or_default()
                            .push(*duration_ms);
                    }
                    TraceEventKind::Error { message } => {
                        *error_counts.entry(message.clone()).or_insert(0) += 1;
                    }
                    TraceEventKind::LlmCall { cost, .. } => {
                        // Attribute LLM costs to the most recent tool in
                        // context (simplified heuristic).
                        *tool_costs.entry("_llm".to_string()).or_insert(0.0) += cost;
                    }
                    _ => {}
                }
            }
        }

        // Emit patterns that exceed reasonable thresholds.
        let threshold = 3_usize;

        for (tool, count) in &denial_counts {
            if *count >= threshold {
                patterns.push(Pattern {
                    kind: PatternKind::FrequentDenial,
                    description: format!("Tool '{}' was denied {} times", tool, count),
                    occurrences: *count,
                });
            }
        }

        for (tool, count) in &approval_counts {
            if *count >= threshold {
                patterns.push(Pattern {
                    kind: PatternKind::ApprovalBottleneck,
                    description: format!("Tool '{}' required approval {} times", tool, count),
                    occurrences: *count,
                });
            }
        }

        let slow_threshold_ms = 5000_u64;
        for (tool, durs) in &durations {
            let slow = durs.iter().filter(|&&d| d >= slow_threshold_ms).count();
            if slow >= threshold {
                patterns.push(Pattern {
                    kind: PatternKind::SlowTool,
                    description: format!(
                        "Tool '{}' was slow (>={}ms) {} times",
                        tool, slow_threshold_ms, slow
                    ),
                    occurrences: slow,
                });
            }
        }

        for (message, count) in &error_counts {
            if *count >= threshold {
                let preview = if message.len() > 60 {
                    format!("{}...", &message[..60])
                } else {
                    message.clone()
                };
                patterns.push(Pattern {
                    kind: PatternKind::RepeatedError,
                    description: format!("Error '{}' occurred {} times", preview, count),
                    occurrences: *count,
                });
            }
        }

        let cost_threshold = 1.0_f64;
        for (label, cost) in &tool_costs {
            if *cost >= cost_threshold {
                patterns.push(Pattern {
                    kind: PatternKind::HighCostTool,
                    description: format!("'{}' accumulated ${:.4} in costs", label, cost),
                    occurrences: 1,
                });
            }
        }

        patterns
    }

    /// Compute the fraction of traces that completed successfully.
    ///
    /// Traces without a `success` outcome are excluded from the denominator.
    pub fn success_rate(traces: &[&ExecutionTrace]) -> f64 {
        let completed: Vec<_> = traces.iter().filter(|t| t.success.is_some()).collect();
        if completed.is_empty() {
            return 0.0;
        }
        let ok = completed.iter().filter(|t| t.success == Some(true)).count();
        ok as f64 / completed.len() as f64
    }

    /// Compute the average number of iterations across all traces.
    pub fn avg_iterations(traces: &[&ExecutionTrace]) -> f64 {
        if traces.is_empty() {
            return 0.0;
        }
        let total: usize = traces.iter().map(|t| t.iterations).sum();
        total as f64 / traces.len() as f64
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// Helper: create a minimal execution trace.
    fn make_trace(goal: &str) -> ExecutionTrace {
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        let mut trace = ExecutionTrace::new(session, task, goal);
        trace.iterations = 3;
        trace
    }

    /// Helper: create a trace with tool events.
    fn make_trace_with_tools() -> ExecutionTrace {
        let mut trace = make_trace("test task");
        trace.push_event(TraceEventKind::ToolRequested {
            tool: "file_read".into(),
            risk_level: RiskLevel::ReadOnly,
            args_summary: "reading main.rs".into(),
        });
        trace.push_event(TraceEventKind::ToolApproved {
            tool: "file_read".into(),
        });
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_read".into(),
            success: true,
            duration_ms: 42,
            output_preview: "fn main() ...".into(),
        });
        trace.push_event(TraceEventKind::LlmCall {
            model: "claude-opus-4-5-20251101".into(),
            input_tokens: 1000,
            output_tokens: 500,
            cost: 0.05,
        });
        trace.push_event(TraceEventKind::Error {
            message: "timeout".into(),
        });
        trace
    }

    // 1
    #[test]
    fn test_trace_event_creation() {
        let mut trace = make_trace("goal");
        // The constructor pushes a TaskStarted event at sequence 0.
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.events[0].sequence, 0);

        trace.push_event(TraceEventKind::StatusChange {
            from: "idle".into(),
            to: "thinking".into(),
        });
        assert_eq!(trace.events[1].sequence, 1);

        trace.push_event(TraceEventKind::Error {
            message: "oops".into(),
        });
        assert_eq!(trace.events[2].sequence, 2);
    }

    // 2
    #[test]
    fn test_execution_trace_new() {
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        let trace = ExecutionTrace::new(session, task, "my goal");

        assert_eq!(trace.session_id, session);
        assert_eq!(trace.task_id, task);
        assert_eq!(trace.goal, "my goal");
        assert!(trace.completed_at.is_none());
        assert!(trace.success.is_none());
        assert_eq!(trace.iterations, 0);
        assert_eq!(trace.total_usage.total(), 0);
        assert!((trace.total_cost.total() - 0.0).abs() < f64::EPSILON);
        // The constructor auto-pushes a TaskStarted event.
        assert_eq!(trace.events.len(), 1);
        assert!(matches!(
            &trace.events[0].kind,
            TraceEventKind::TaskStarted { goal, .. } if goal == "my goal"
        ));
    }

    // 3
    #[test]
    fn test_execution_trace_push_event() {
        let mut trace = make_trace("push test");
        let initial_len = trace.events.len();

        trace.push_event(TraceEventKind::ToolRequested {
            tool: "shell_exec".into(),
            risk_level: RiskLevel::Execute,
            args_summary: "cargo test".into(),
        });
        assert_eq!(trace.events.len(), initial_len + 1);
        assert_eq!(trace.events.last().unwrap().sequence, initial_len);
    }

    // 4
    #[test]
    fn test_execution_trace_complete() {
        let mut trace = make_trace("complete test");
        assert!(trace.completed_at.is_none());
        assert!(trace.success.is_none());

        trace.complete(true);
        assert!(trace.completed_at.is_some());
        assert_eq!(trace.success, Some(true));

        // A TaskCompleted event should have been appended.
        let last = trace.events.last().unwrap();
        assert!(matches!(
            &last.kind,
            TraceEventKind::TaskCompleted { success: true, .. }
        ));
    }

    // 5
    #[test]
    fn test_execution_trace_duration() {
        let mut trace = make_trace("duration test");
        // Not yet completed — duration is None.
        assert!(trace.duration_ms().is_none());

        trace.complete(true);
        // Now duration should be >= 0.
        let dur = trace.duration_ms().unwrap();
        assert!(dur < 5000); // sanity: should be effectively instant in tests
    }

    // 6
    #[test]
    fn test_execution_trace_tool_events() {
        let trace = make_trace_with_tools();
        let tool_evts = trace.tool_events();
        // ToolRequested, ToolApproved, ToolExecuted
        assert_eq!(tool_evts.len(), 3);
        assert!(matches!(
            &tool_evts[0].kind,
            TraceEventKind::ToolRequested { .. }
        ));
    }

    // 7
    #[test]
    fn test_execution_trace_error_events() {
        let trace = make_trace_with_tools();
        let errs = trace.error_events();
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, TraceEventKind::Error { message } if message == "timeout"));
    }

    // 8
    #[test]
    fn test_execution_trace_llm_events() {
        let trace = make_trace_with_tools();
        let llm = trace.llm_events();
        assert_eq!(llm.len(), 1);
        assert!(matches!(
            &llm[0].kind,
            TraceEventKind::LlmCall { model, .. } if model == "claude-opus-4-5-20251101"
        ));
    }

    // 9
    #[test]
    fn test_audit_store_add_and_get() {
        let mut store = AuditStore::new();
        let trace = make_trace("store test");
        let id = trace.trace_id;

        store.add_trace(trace);
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());

        let found = store.get_trace(id).unwrap();
        assert_eq!(found.goal, "store test");

        // Non-existent ID.
        assert!(store.get_trace(Uuid::new_v4()).is_none());
    }

    // 10
    #[test]
    fn test_audit_store_capacity() {
        let mut store = AuditStore {
            traces: Vec::new(),
            max_traces: 3,
            merkle_chain: None,
        };

        for i in 0..5 {
            store.add_trace(make_trace(&format!("trace {}", i)));
        }

        assert_eq!(store.len(), 3);
        // The oldest traces (0 and 1) should have been evicted.
        let goals: Vec<&str> = store.traces().iter().map(|t| t.goal.as_str()).collect();
        assert_eq!(goals, vec!["trace 2", "trace 3", "trace 4"]);
    }

    // 11
    #[test]
    fn test_audit_store_latest() {
        let mut store = AuditStore::new();
        for i in 0..5 {
            store.add_trace(make_trace(&format!("trace {}", i)));
        }

        let latest = store.latest(2);
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].goal, "trace 3");
        assert_eq!(latest[1].goal, "trace 4");

        // Requesting more than available.
        let all = store.latest(100);
        assert_eq!(all.len(), 5);
    }

    // 12
    #[test]
    fn test_audit_store_query_by_session() {
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();

        let mut store = AuditStore::new();

        let mut t1 = make_trace("a1");
        t1.session_id = session_a;
        let mut t2 = make_trace("b1");
        t2.session_id = session_b;
        let mut t3 = make_trace("a2");
        t3.session_id = session_a;

        store.add_trace(t1);
        store.add_trace(t2);
        store.add_trace(t3);

        let results = store.query(&AuditQuery::new().for_session(session_a));
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.session_id == session_a));
    }

    // 13
    #[test]
    fn test_audit_store_query_by_tool() {
        let mut store = AuditStore::new();

        let mut t1 = make_trace("with tool");
        t1.push_event(TraceEventKind::ToolRequested {
            tool: "file_read".into(),
            risk_level: RiskLevel::ReadOnly,
            args_summary: "src/main.rs".into(),
        });

        let t2 = make_trace("no tool");

        store.add_trace(t1);
        store.add_trace(t2);

        let results = store.query(&AuditQuery::new().for_tool("file_read"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].goal, "with tool");
    }

    // 14
    #[test]
    fn test_audit_store_query_by_risk() {
        let mut store = AuditStore::new();

        let mut low = make_trace("low risk");
        low.push_event(TraceEventKind::ToolRequested {
            tool: "ls".into(),
            risk_level: RiskLevel::ReadOnly,
            args_summary: ".".into(),
        });

        let mut high = make_trace("high risk");
        high.push_event(TraceEventKind::ToolRequested {
            tool: "rm".into(),
            risk_level: RiskLevel::Destructive,
            args_summary: "tmp/".into(),
        });

        store.add_trace(low);
        store.add_trace(high);

        let results = store.query(&AuditQuery::new().min_risk(RiskLevel::Execute));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].goal, "high risk");
    }

    // 15
    #[test]
    fn test_audit_store_query_success_only() {
        let mut store = AuditStore::new();

        let mut ok = make_trace("good");
        ok.complete(true);

        let mut fail = make_trace("bad");
        fail.complete(false);

        store.add_trace(ok);
        store.add_trace(fail);

        let successes = store.query(&AuditQuery::new().successful());
        assert_eq!(successes.len(), 1);
        assert_eq!(successes[0].goal, "good");

        let failures = store.query(&AuditQuery::new().failed());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].goal, "bad");
    }

    // 16
    #[test]
    fn test_audit_store_query_time_range() {
        let mut store = AuditStore::new();

        let now = Utc::now();
        let one_hour_ago = now - Duration::hours(1);
        let two_hours_ago = now - Duration::hours(2);

        let mut old = make_trace("old");
        old.started_at = two_hours_ago;

        let mut recent = make_trace("recent");
        recent.started_at = now;

        store.add_trace(old);
        store.add_trace(recent);

        let results = store.query(&AuditQuery::new().since(one_hour_ago));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].goal, "recent");

        let results = store.query(&AuditQuery::new().until(one_hour_ago));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].goal, "old");
    }

    // 17
    #[test]
    fn test_audit_store_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.json");

        let mut store = AuditStore::new();
        let mut trace = make_trace_with_tools();
        trace.complete(true);
        let id = trace.trace_id;
        store.add_trace(trace);

        store.save(&path).unwrap();
        assert!(path.exists());

        let loaded = AuditStore::load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        let t = loaded.get_trace(id).unwrap();
        assert_eq!(t.goal, "test task");
    }

    // 18
    #[test]
    fn test_exporter_json() {
        let trace = make_trace_with_tools();
        let refs = vec![&trace];

        let json = AuditExporter::to_json(&refs).unwrap();
        assert!(json.contains("test task"));
        assert!(json.contains("file_read"));

        // Should be valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
    }

    // 19
    #[test]
    fn test_exporter_jsonl() {
        let t1 = make_trace("one");
        let t2 = make_trace("two");
        let refs = vec![&t1, &t2];

        let jsonl = AuditExporter::to_jsonl(&refs).unwrap();
        let lines: Vec<&str> = jsonl.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON.
        for line in &lines {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }

    // 20
    #[test]
    fn test_exporter_text() {
        let mut trace = make_trace_with_tools();
        trace.total_usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
        };
        trace.total_cost = CostEstimate {
            input_cost: 0.01,
            output_cost: 0.03,
        };
        trace.complete(true);
        let refs = vec![&trace];

        let text = AuditExporter::to_text(&refs);
        assert!(text.contains("Trace"));
        assert!(text.contains("test task"));
        assert!(text.contains("Tokens: 1000/500"));
        assert!(text.contains("$0.0400"));
        assert!(text.contains("Events:"));
        assert!(text.contains("---"));
    }

    // 21
    #[test]
    fn test_exporter_csv() {
        let trace = make_trace_with_tools();
        let refs = vec![&trace];

        let csv = AuditExporter::to_csv(&refs);
        let lines: Vec<&str> = csv.trim().split('\n').collect();

        // Header + events.
        assert!(lines[0].starts_with("trace_id,sequence,timestamp,event_type,tool,details"));
        assert!(lines.len() > 1);
        // First data row should be task_started.
        assert!(lines[1].contains("task_started"));
    }

    // 22
    #[test]
    fn test_analytics_tool_usage() {
        let trace = make_trace_with_tools();
        let refs = vec![&trace];

        let summary = Analytics::tool_usage_summary(&refs);
        assert_eq!(summary.tool_counts.get("file_read"), Some(&1));
        assert_eq!(summary.most_used, Some("file_read".to_string()));

        let rate = summary.tool_success_rates.get("file_read").unwrap();
        assert!((rate - 1.0).abs() < f64::EPSILON);

        let avg = summary.tool_avg_duration_ms.get("file_read").unwrap();
        assert!((avg - 42.0).abs() < f64::EPSILON);
    }

    // 23
    #[test]
    fn test_analytics_cost_breakdown() {
        let mut trace = make_trace("cost test");
        trace.push_event(TraceEventKind::LlmCall {
            model: "gpt-4".into(),
            input_tokens: 1000,
            output_tokens: 500,
            cost: 0.06,
        });
        trace.push_event(TraceEventKind::LlmCall {
            model: "gpt-4".into(),
            input_tokens: 2000,
            output_tokens: 1000,
            cost: 0.12,
        });
        trace.push_event(TraceEventKind::LlmCall {
            model: "claude-sonnet".into(),
            input_tokens: 500,
            output_tokens: 200,
            cost: 0.02,
        });

        let refs = vec![&trace];
        let breakdown = Analytics::cost_breakdown(&refs);

        assert!((breakdown.total_cost - 0.20).abs() < 1e-9);
        assert_eq!(breakdown.total_tokens, 1000 + 500 + 2000 + 1000 + 500 + 200);

        let gpt4 = breakdown.by_model.get("gpt-4").unwrap();
        assert_eq!(gpt4.calls, 2);
        assert!((gpt4.total_cost - 0.18).abs() < 1e-9);

        let claude = breakdown.by_model.get("claude-sonnet").unwrap();
        assert_eq!(claude.calls, 1);
    }

    // 24
    #[test]
    fn test_analytics_detect_patterns() {
        let mut trace = make_trace("pattern test");

        // Three denials for the same tool -> FrequentDenial
        for _ in 0..4 {
            trace.push_event(TraceEventKind::ToolDenied {
                tool: "rm_rf".into(),
                reason: "too dangerous".into(),
            });
        }

        // Three approval requests -> ApprovalBottleneck
        for _ in 0..3 {
            trace.push_event(TraceEventKind::ApprovalRequested {
                tool: "deploy".into(),
                context: "production".into(),
            });
        }

        // Three repeated errors -> RepeatedError
        for _ in 0..3 {
            trace.push_event(TraceEventKind::Error {
                message: "connection reset".into(),
            });
        }

        let refs = vec![&trace];
        let patterns = Analytics::detect_patterns(&refs);

        let kinds: Vec<_> = patterns.iter().map(|p| &p.kind).collect();
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, PatternKind::FrequentDenial))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, PatternKind::ApprovalBottleneck))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, PatternKind::RepeatedError))
        );
    }

    // 25
    #[test]
    fn test_analytics_success_rate() {
        let mut t1 = make_trace("ok");
        t1.complete(true);
        let mut t2 = make_trace("ok2");
        t2.complete(true);
        let mut t3 = make_trace("fail");
        t3.complete(false);
        let t4 = make_trace("in-progress");

        let refs = vec![&t1, &t2, &t3, &t4];
        let rate = Analytics::success_rate(&refs);
        // 2 successes out of 3 completed (t4 excluded from denominator).
        assert!((rate - 2.0 / 3.0).abs() < 1e-9);
    }

    // 26
    #[test]
    fn test_analytics_avg_iterations() {
        let mut t1 = make_trace("a");
        t1.iterations = 5;
        let mut t2 = make_trace("b");
        t2.iterations = 10;
        let mut t3 = make_trace("c");
        t3.iterations = 0;

        let refs = vec![&t1, &t2, &t3];
        let avg = Analytics::avg_iterations(&refs);
        assert!((avg - 5.0).abs() < 1e-9);
    }

    // 27
    #[test]
    fn test_trace_event_kind_from_audit_event() {
        let requested = AuditEvent::ActionRequested {
            tool: "file_write".into(),
            risk_level: RiskLevel::Write,
            description: "writing config".into(),
        };
        let kind = TraceEventKind::from_audit_event(&requested);
        assert!(matches!(
            kind,
            TraceEventKind::ToolRequested {
                ref tool,
                risk_level: RiskLevel::Write,
                ..
            } if tool == "file_write"
        ));

        let approved = AuditEvent::ActionApproved {
            tool: "file_write".into(),
        };
        let kind = TraceEventKind::from_audit_event(&approved);
        assert!(matches!(kind, TraceEventKind::ToolApproved { ref tool } if tool == "file_write"));

        let denied = AuditEvent::ActionDenied {
            tool: "rm".into(),
            reason: "denied".into(),
        };
        let kind = TraceEventKind::from_audit_event(&denied);
        assert!(matches!(kind, TraceEventKind::ToolDenied { ref tool, .. } if tool == "rm"));

        let executed = AuditEvent::ActionExecuted {
            tool: "grep".into(),
            success: true,
            duration_ms: 99,
        };
        let kind = TraceEventKind::from_audit_event(&executed);
        assert!(
            matches!(kind, TraceEventKind::ToolExecuted { ref tool, success: true, duration_ms: 99, .. } if tool == "grep")
        );

        let approval_req = AuditEvent::ApprovalRequested {
            tool: "deploy".into(),
            context: "prod".into(),
        };
        let kind = TraceEventKind::from_audit_event(&approval_req);
        assert!(
            matches!(kind, TraceEventKind::ApprovalRequested { ref tool, ref context } if tool == "deploy" && context == "prod")
        );

        let decision = AuditEvent::ApprovalDecision {
            tool: "deploy".into(),
            approved: false,
        };
        let kind = TraceEventKind::from_audit_event(&decision);
        assert!(
            matches!(kind, TraceEventKind::ApprovalDecision { ref tool, approved: false } if tool == "deploy")
        );
    }

    // 28
    #[test]
    fn test_audit_query_builder() {
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        let since = Utc::now() - Duration::hours(1);
        let until = Utc::now();

        let q = AuditQuery::new()
            .for_session(session)
            .for_task(task)
            .for_tool("file_read")
            .min_risk(RiskLevel::Execute)
            .successful()
            .since(since)
            .until(until);

        assert_eq!(q.session_id, Some(session));
        assert_eq!(q.task_id, Some(task));
        assert_eq!(q.tool_name.as_deref(), Some("file_read"));
        assert_eq!(q.risk_level_min, Some(RiskLevel::Execute));
        assert_eq!(q.success_only, Some(true));
        assert_eq!(q.since, Some(since));
        assert_eq!(q.until, Some(until));

        // Also test the `failed()` variant.
        let q2 = AuditQuery::new().failed();
        assert_eq!(q2.success_only, Some(false));
    }

    // 29
    #[test]
    fn test_execution_trace_serde_roundtrip() {
        let mut trace = make_trace_with_tools();
        trace.total_usage = TokenUsage {
            input_tokens: 1234,
            output_tokens: 567,
        };
        trace.total_cost = CostEstimate {
            input_cost: 0.01,
            output_cost: 0.03,
        };
        trace.complete(true);

        let json = serde_json::to_string(&trace).unwrap();
        let restored: ExecutionTrace = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.trace_id, trace.trace_id);
        assert_eq!(restored.goal, trace.goal);
        assert_eq!(restored.events.len(), trace.events.len());
        assert_eq!(restored.total_usage.input_tokens, 1234);
        assert_eq!(restored.total_usage.output_tokens, 567);
        assert!((restored.total_cost.total() - 0.04).abs() < f64::EPSILON);
        assert_eq!(restored.success, Some(true));
    }

    // --- Merkle chain integration tests ---

    // 30
    #[test]
    fn test_audit_store_with_merkle_chain() {
        let store = AuditStore::with_merkle_chain();
        assert!(store.merkle_chain().is_some());
        assert!(store.is_empty());
    }

    // 31
    #[test]
    fn test_audit_store_merkle_appends_on_add() {
        let mut store = AuditStore::with_merkle_chain();
        store.add_trace(make_trace("trace 1"));
        store.add_trace(make_trace("trace 2"));

        let chain = store.merkle_chain().unwrap();
        assert_eq!(chain.len(), 2);
    }

    // 32
    #[test]
    fn test_audit_store_verify_integrity_valid() {
        let mut store = AuditStore::with_merkle_chain();
        store.add_trace(make_trace("a"));
        store.add_trace(make_trace("b"));
        store.add_trace(make_trace("c"));

        let result = store.verify_integrity().unwrap();
        assert!(result.is_valid);
        assert_eq!(result.checked_nodes, 3);
        assert!(result.first_invalid.is_none());
    }

    // 33
    #[test]
    fn test_audit_store_verify_integrity_without_merkle() {
        let store = AuditStore::new();
        assert!(store.verify_integrity().is_none());
    }

    // 34
    #[test]
    fn test_audit_store_merkle_root_hash_changes() {
        let mut store = AuditStore::with_merkle_chain();
        assert!(store.merkle_root_hash().is_none()); // empty chain

        store.add_trace(make_trace("first"));
        let hash1 = store.merkle_root_hash().unwrap();

        store.add_trace(make_trace("second"));
        let hash2 = store.merkle_root_hash().unwrap();

        assert_ne!(hash1, hash2);
    }

    // 35
    #[test]
    fn test_audit_store_load_with_merkle_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit_merkle.json");

        // Save without merkle
        let mut store = AuditStore::new();
        store.add_trace(make_trace("alpha"));
        store.add_trace(make_trace("beta"));
        store.save(&path).unwrap();

        // Load with merkle — should rebuild chain
        let loaded = AuditStore::load_with_merkle(&path).unwrap();
        assert!(loaded.merkle_chain().is_some());
        assert_eq!(loaded.merkle_chain().unwrap().len(), 2);

        let result = loaded.verify_integrity().unwrap();
        assert!(result.is_valid);
    }

    // 36
    #[test]
    fn test_audit_store_no_merkle_by_default() {
        let store = AuditStore::new();
        assert!(store.merkle_chain().is_none());
        assert!(store.merkle_root_hash().is_none());
    }

    // 37
    #[test]
    fn test_audit_error_display() {
        let err = AuditError::SerializationFailed("bad json".into());
        assert_eq!(err.to_string(), "serialization failed: bad json");

        let err = AuditError::IoError("file not found".into());
        assert_eq!(err.to_string(), "io error: file not found");

        let err = AuditError::EmptyStore;
        assert_eq!(err.to_string(), "store is empty");

        let id = Uuid::new_v4();
        let err = AuditError::TraceNotFound(id);
        assert_eq!(err.to_string(), format!("trace not found: {}", id));
    }
}
