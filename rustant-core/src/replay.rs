//! Session replay engine — step-by-step playback of execution traces.
//!
//! Enables reviewing past agent executions by stepping through recorded
//! trace events, providing context at each step about what happened
//! and why.

use crate::audit::{AuditStore, ExecutionTrace, TraceEvent, TraceEventKind};
use crate::types::{CostEstimate, TokenUsage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ReplayError
// ---------------------------------------------------------------------------

/// Errors that can occur during replay operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReplayError {
    #[error("trace not found: {0}")]
    TraceNotFound(Uuid),
    #[error("position {position} out of bounds (total: {total})")]
    OutOfBounds { position: usize, total: usize },
    #[error("bookmark index {0} out of bounds")]
    BookmarkNotFound(usize),
    #[error("empty trace: no events to replay")]
    EmptyTrace,
}

// ---------------------------------------------------------------------------
// Bookmark
// ---------------------------------------------------------------------------

/// A saved position in the replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub position: usize,
    pub label: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ReplaySnapshot
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of the replay state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySnapshot {
    pub trace_id: Uuid,
    pub position: usize,
    pub total_events: usize,
    /// Progress percentage from 0.0 to 100.0.
    pub progress_pct: f64,
    pub current_event: Option<TraceEvent>,
    /// Milliseconds elapsed from the trace start to the current event.
    pub elapsed_from_start: Option<u64>,
    pub cumulative_usage: TokenUsage,
    pub cumulative_cost: CostEstimate,
    pub tools_executed_so_far: Vec<String>,
    pub errors_so_far: usize,
}

// ---------------------------------------------------------------------------
// TimelineEntry
// ---------------------------------------------------------------------------

/// One entry in the timeline view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub sequence: usize,
    pub timestamp: DateTime<Utc>,
    /// Milliseconds elapsed from the trace start.
    pub elapsed_ms: u64,
    pub description: String,
    /// Whether this entry is the current replay position.
    pub is_current: bool,
    /// Whether this entry has a bookmark.
    pub is_bookmarked: bool,
}

// ---------------------------------------------------------------------------
// ReplayEngine
// ---------------------------------------------------------------------------

/// The main replay controller — provides step-by-step playback of an
/// execution trace.
pub struct ReplayEngine {
    trace: ExecutionTrace,
    /// Current event index (0-based).
    position: usize,
    bookmarks: Vec<Bookmark>,
}

impl ReplayEngine {
    /// Create a new replay from an execution trace.
    pub fn new(trace: ExecutionTrace) -> Self {
        Self {
            trace,
            position: 0,
            bookmarks: Vec::new(),
        }
    }

    /// Create a replay engine from a trace in the audit store.
    pub fn from_store(store: &AuditStore, trace_id: Uuid) -> Result<Self, ReplayError> {
        let trace = store
            .get_trace(trace_id)
            .ok_or(ReplayError::TraceNotFound(trace_id))?;
        Ok(Self::new(trace.clone()))
    }

    /// Get the current position (0-based event index).
    pub fn position(&self) -> usize {
        self.position
    }

    /// Get total number of events.
    pub fn total_events(&self) -> usize {
        self.trace.events.len()
    }

    /// Is the replay at the beginning?
    pub fn is_at_start(&self) -> bool {
        self.position == 0
    }

    /// Is the replay at the end?
    pub fn is_at_end(&self) -> bool {
        self.trace.events.is_empty() || self.position >= self.trace.events.len() - 1
    }

    /// Step forward one event. Returns the event at the new position, or
    /// `None` if already at the end.
    pub fn step_forward(&mut self) -> Option<&TraceEvent> {
        if self.position + 1 < self.trace.events.len() {
            self.position += 1;
            self.trace.events.get(self.position)
        } else {
            None
        }
    }

    /// Step backward one event. Returns the event at the new position, or
    /// `None` if already at the start.
    pub fn step_backward(&mut self) -> Option<&TraceEvent> {
        if self.position > 0 {
            self.position -= 1;
            self.trace.events.get(self.position)
        } else {
            None
        }
    }

    /// Jump to a specific position.
    pub fn seek(&mut self, position: usize) -> Result<&TraceEvent, ReplayError> {
        if position >= self.trace.events.len() {
            return Err(ReplayError::OutOfBounds {
                position,
                total: self.trace.events.len(),
            });
        }
        self.position = position;
        Ok(&self.trace.events[self.position])
    }

    /// Go to the start.
    pub fn rewind(&mut self) {
        self.position = 0;
    }

    /// Go to the end.
    pub fn fast_forward(&mut self) {
        if !self.trace.events.is_empty() {
            self.position = self.trace.events.len() - 1;
        }
    }

    /// Get the current event.
    pub fn current_event(&self) -> Option<&TraceEvent> {
        self.trace.events.get(self.position)
    }

    /// Get a snapshot of the current replay state.
    pub fn snapshot(&self) -> ReplaySnapshot {
        let total_events = self.trace.events.len();
        let current_event = self.trace.events.get(self.position).cloned();

        let elapsed_from_start = current_event.as_ref().map(|e| {
            (e.timestamp - self.trace.started_at)
                .num_milliseconds()
                .max(0) as u64
        });

        let progress_pct = if total_events == 0 {
            0.0
        } else if total_events == 1 {
            100.0
        } else {
            (self.position as f64 / (total_events - 1) as f64) * 100.0
        };

        let end = if total_events == 0 {
            0
        } else {
            self.position + 1
        };

        let tools_executed_so_far: Vec<String> = self
            .trace
            .events
            .iter()
            .take(end)
            .filter_map(|e| {
                if let TraceEventKind::ToolExecuted { ref tool, .. } = e.kind {
                    Some(tool.clone())
                } else {
                    None
                }
            })
            .collect();

        let errors_so_far = self
            .trace
            .events
            .iter()
            .take(end)
            .filter(|e| matches!(&e.kind, TraceEventKind::Error { .. }))
            .count();

        ReplaySnapshot {
            trace_id: self.trace.trace_id,
            position: self.position,
            total_events,
            progress_pct,
            current_event,
            elapsed_from_start,
            cumulative_usage: self.cumulative_usage(),
            cumulative_cost: self.cumulative_cost(),
            tools_executed_so_far,
            errors_so_far,
        }
    }

    /// Get the current step as a formatted description.
    pub fn describe_current(&self) -> String {
        match self.current_event() {
            Some(event) => format!(
                "[{}/{}] {}",
                self.position + 1,
                self.total_events(),
                describe_event(&event.kind)
            ),
            None => "No events".to_string(),
        }
    }

    /// Get a full timeline description of all events.
    pub fn timeline(&self) -> Vec<TimelineEntry> {
        let bookmark_positions: HashSet<usize> =
            self.bookmarks.iter().map(|b| b.position).collect();

        self.trace
            .events
            .iter()
            .map(|event| {
                let elapsed_ms = (event.timestamp - self.trace.started_at)
                    .num_milliseconds()
                    .max(0) as u64;

                TimelineEntry {
                    sequence: event.sequence,
                    timestamp: event.timestamp,
                    elapsed_ms,
                    description: describe_event(&event.kind),
                    is_current: event.sequence == self.position,
                    is_bookmarked: bookmark_positions.contains(&event.sequence),
                }
            })
            .collect()
    }

    /// Add a bookmark at the current position.
    pub fn add_bookmark(&mut self, label: impl Into<String>) {
        self.bookmarks.push(Bookmark {
            position: self.position,
            label: label.into(),
            created_at: Utc::now(),
        });
    }

    /// Get all bookmarks.
    pub fn bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Jump to a bookmark by index.
    pub fn goto_bookmark(&mut self, index: usize) -> Result<&TraceEvent, ReplayError> {
        let position = self
            .bookmarks
            .get(index)
            .ok_or(ReplayError::BookmarkNotFound(index))?
            .position;
        self.seek(position)
    }

    /// Get the trace being replayed.
    pub fn trace(&self) -> &ExecutionTrace {
        &self.trace
    }

    /// Skip forward to the next event of a tool-related kind
    /// (`ToolRequested`, `ToolApproved`, `ToolDenied`, or `ToolExecuted`).
    pub fn skip_to_next_tool_event(&mut self) -> Option<&TraceEvent> {
        let start = self.position + 1;
        for i in start..self.trace.events.len() {
            match &self.trace.events[i].kind {
                TraceEventKind::ToolRequested { .. }
                | TraceEventKind::ToolApproved { .. }
                | TraceEventKind::ToolDenied { .. }
                | TraceEventKind::ToolExecuted { .. } => {
                    self.position = i;
                    return self.trace.events.get(i);
                }
                _ => continue,
            }
        }
        None
    }

    /// Get cumulative token usage up to and including the current position.
    pub fn cumulative_usage(&self) -> TokenUsage {
        let mut usage = TokenUsage::default();
        let end = self.position + 1;
        for event in self.trace.events.iter().take(end) {
            if let TraceEventKind::LlmCall {
                input_tokens,
                output_tokens,
                ..
            } = &event.kind
            {
                usage.input_tokens += input_tokens;
                usage.output_tokens += output_tokens;
            }
        }
        usage
    }

    /// Get cumulative cost up to and including the current position.
    ///
    /// The per-call cost is split proportionally between input and output
    /// based on token counts.
    pub fn cumulative_cost(&self) -> CostEstimate {
        let mut estimate = CostEstimate::default();
        let end = self.position + 1;
        for event in self.trace.events.iter().take(end) {
            if let TraceEventKind::LlmCall {
                cost,
                input_tokens,
                output_tokens,
                ..
            } = &event.kind
            {
                let total_tokens = input_tokens + output_tokens;
                if total_tokens > 0 {
                    estimate.input_cost += cost * (*input_tokens as f64 / total_tokens as f64);
                    estimate.output_cost += cost * (*output_tokens as f64 / total_tokens as f64);
                }
            }
        }
        estimate
    }
}

// ---------------------------------------------------------------------------
// describe_event helper
// ---------------------------------------------------------------------------

/// Format a [`TraceEventKind`] as a human-readable description.
pub fn describe_event(kind: &TraceEventKind) -> String {
    match kind {
        TraceEventKind::TaskStarted { goal, .. } => {
            format!("Task started: {goal}")
        }
        TraceEventKind::TaskCompleted {
            success,
            iterations,
            ..
        } => {
            format!(
                "Task {} after {} iterations",
                if *success {
                    "completed successfully"
                } else {
                    "failed"
                },
                iterations
            )
        }
        TraceEventKind::ToolRequested {
            tool, risk_level, ..
        } => {
            format!("Tool requested: {tool} (risk: {risk_level:?})")
        }
        TraceEventKind::ToolApproved { tool } => {
            format!("Tool approved: {tool}")
        }
        TraceEventKind::ToolDenied { tool, reason } => {
            format!("Tool denied: {tool} - {reason}")
        }
        TraceEventKind::ApprovalRequested { tool, .. } => {
            format!("Approval requested for: {tool}")
        }
        TraceEventKind::ApprovalDecision { tool, approved } => {
            format!(
                "Approval {}: {}",
                if *approved { "granted" } else { "rejected" },
                tool
            )
        }
        TraceEventKind::ToolExecuted {
            tool,
            success,
            duration_ms,
            ..
        } => {
            format!(
                "Tool executed: {} ({}, {}ms)",
                tool,
                if *success { "ok" } else { "failed" },
                duration_ms
            )
        }
        TraceEventKind::LlmCall {
            model,
            input_tokens,
            output_tokens,
            ..
        } => {
            format!("LLM call: {model} ({input_tokens}/{output_tokens} tokens)")
        }
        TraceEventKind::StatusChange { from, to } => {
            format!("Status: {from} -> {to}")
        }
        TraceEventKind::Error { message } => {
            format!("Error: {message}")
        }
        TraceEventKind::PersonaSwitched {
            from,
            to,
            rationale,
        } => {
            format!("Persona switched: {from} -> {to} ({rationale})")
        }
        TraceEventKind::CacheCreated { provider, tokens } => {
            format!("Cache created: {provider} ({tokens} tokens)")
        }
        TraceEventKind::CacheInvalidated { provider, reason } => {
            format!("Cache invalidated: {provider} ({reason})")
        }
        // AI/ML audit events
        TraceEventKind::ModelInferencePerformed { model, backend, .. } => {
            format!("Model inference: {model} ({backend})")
        }
        TraceEventKind::TrainingCompleted {
            experiment_id,
            epochs,
            ..
        } => {
            format!("Training completed: {experiment_id} ({epochs} epochs)")
        }
        TraceEventKind::RagQueryExecuted {
            collection,
            chunks_retrieved,
            ..
        } => {
            format!("RAG query: {collection} ({chunks_retrieved} chunks)")
        }
        TraceEventKind::AiSafetyCheckPerformed {
            check_type, result, ..
        } => {
            format!("AI safety check: {check_type} ({result})")
        }
        TraceEventKind::RedTeamAttackTested {
            attack_type,
            result,
        } => {
            format!("Red team test: {attack_type} ({result})")
        }
        TraceEventKind::DataPipelineAudited {
            action, dataset, ..
        } => {
            format!("Data pipeline: {action} on {dataset}")
        }
        // Security & compliance audit events
        TraceEventKind::SecurityScanCompleted {
            scanner,
            target,
            findings_count,
            ..
        } => {
            format!("Security scan: {scanner} on {target} ({findings_count} findings)")
        }
        TraceEventKind::FindingDetected {
            severity, title, ..
        } => {
            format!("Finding [{severity}]: {title}")
        }
        TraceEventKind::FindingSuppressed {
            finding_id, reason, ..
        } => {
            format!("Finding suppressed: {finding_id} ({reason})")
        }
        TraceEventKind::PolicyEvaluated {
            policy_id, result, ..
        } => {
            format!("Policy {policy_id}: {result}")
        }
        TraceEventKind::ComplianceReportGenerated {
            framework,
            compliance_rate,
            ..
        } => {
            format!(
                "Compliance report: {} ({:.0}%)",
                framework,
                compliance_rate * 100.0
            )
        }
        TraceEventKind::IncidentActionTaken {
            action_type,
            target,
            success,
            ..
        } => {
            format!(
                "Incident: {} on {} ({})",
                action_type,
                target,
                if *success { "ok" } else { "failed" }
            )
        }
    }
}

// ---------------------------------------------------------------------------
// ReplaySession
// ---------------------------------------------------------------------------

/// Manages multiple replay engines, with one active at a time.
pub struct ReplaySession {
    engines: Vec<ReplayEngine>,
    active_index: Option<usize>,
}

impl ReplaySession {
    /// Create a new empty replay session.
    pub fn new() -> Self {
        Self {
            engines: Vec::new(),
            active_index: None,
        }
    }

    /// Add a replay for the given execution trace. Returns its index.
    /// The first added replay is automatically set as active.
    pub fn add_replay(&mut self, trace: ExecutionTrace) -> usize {
        let index = self.engines.len();
        self.engines.push(ReplayEngine::new(trace));
        if self.active_index.is_none() {
            self.active_index = Some(index);
        }
        index
    }

    /// Set the active replay by index.
    pub fn set_active(&mut self, index: usize) -> Result<(), ReplayError> {
        if index >= self.engines.len() {
            return Err(ReplayError::OutOfBounds {
                position: index,
                total: self.engines.len(),
            });
        }
        self.active_index = Some(index);
        Ok(())
    }

    /// Get a reference to the active replay engine.
    pub fn active(&self) -> Option<&ReplayEngine> {
        self.active_index.and_then(|i| self.engines.get(i))
    }

    /// Get a mutable reference to the active replay engine.
    pub fn active_mut(&mut self) -> Option<&mut ReplayEngine> {
        self.active_index.and_then(|i| self.engines.get_mut(i))
    }

    /// List all replays with summary information.
    pub fn list_replays(&self) -> Vec<ReplaySummary> {
        self.engines
            .iter()
            .enumerate()
            .map(|(i, engine)| ReplaySummary {
                index: i,
                trace_id: engine.trace().trace_id,
                goal: engine.trace().goal.clone(),
                event_count: engine.total_events(),
                is_active: self.active_index == Some(i),
            })
            .collect()
    }

    /// Get the number of replays in the session.
    pub fn len(&self) -> usize {
        self.engines.len()
    }

    /// Check whether the session has no replays.
    pub fn is_empty(&self) -> bool {
        self.engines.is_empty()
    }
}

impl Default for ReplaySession {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary information about a replay in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySummary {
    pub index: usize,
    pub trace_id: Uuid,
    pub goal: String,
    pub event_count: usize,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditStore, ExecutionTrace, TraceEventKind};
    use crate::types::RiskLevel;
    use uuid::Uuid;

    /// Build a sample execution trace with multiple event types.
    ///
    /// Events (0-based index):
    ///   0: TaskStarted          (auto-pushed by ExecutionTrace::new)
    ///   1: ToolRequested         (file_read, ReadOnly)
    ///   2: ToolApproved          (file_read)
    ///   3: ToolExecuted          (file_read, success, 42ms)
    ///   4: LlmCall               (gpt-4, 500/200 tokens, $0.021)
    ///   5: ToolRequested         (file_write, Write)
    ///   6: ToolDenied            (file_write, "path denied")
    ///   7: Error                 ("write denied")
    ///   8: TaskCompleted         (auto-pushed by complete())
    fn sample_trace() -> ExecutionTrace {
        let session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let mut trace = ExecutionTrace::new(session_id, task_id, "test task");

        // Events 1..7
        trace.push_event(TraceEventKind::ToolRequested {
            tool: "file_read".into(),
            risk_level: RiskLevel::ReadOnly,
            args_summary: "path=/src/main.rs".into(),
        });
        trace.push_event(TraceEventKind::ToolApproved {
            tool: "file_read".into(),
        });
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_read".into(),
            success: true,
            duration_ms: 42,
            output_preview: "fn main() {...}".into(),
        });
        trace.push_event(TraceEventKind::LlmCall {
            model: "gpt-4".into(),
            input_tokens: 500,
            output_tokens: 200,
            cost: 0.021,
        });
        trace.push_event(TraceEventKind::ToolRequested {
            tool: "file_write".into(),
            risk_level: RiskLevel::Write,
            args_summary: "path=/src/lib.rs".into(),
        });
        trace.push_event(TraceEventKind::ToolDenied {
            tool: "file_write".into(),
            reason: "path denied".into(),
        });
        trace.push_event(TraceEventKind::Error {
            message: "write denied".into(),
        });

        // Event 8 (auto-pushed by complete)
        trace.iterations = 3;
        trace.complete(true);
        trace
    }

    // -----------------------------------------------------------------------
    // 1. test_replay_engine_new
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_new() {
        let trace = sample_trace();
        let engine = ReplayEngine::new(trace);
        assert_eq!(engine.position(), 0);
        assert_eq!(engine.total_events(), 9);
        assert!(engine.bookmarks().is_empty());
    }

    // -----------------------------------------------------------------------
    // 2. test_replay_engine_step_forward
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_step_forward() {
        let mut engine = ReplayEngine::new(sample_trace());
        assert_eq!(engine.position(), 0);

        let event = engine.step_forward().unwrap();
        assert_eq!(event.sequence, 1);
        assert_eq!(engine.position(), 1);

        let event = engine.step_forward().unwrap();
        assert_eq!(event.sequence, 2);
        assert_eq!(engine.position(), 2);
    }

    // -----------------------------------------------------------------------
    // 3. test_replay_engine_step_backward
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_step_backward() {
        let mut engine = ReplayEngine::new(sample_trace());
        engine.seek(3).unwrap();
        assert_eq!(engine.position(), 3);

        let event = engine.step_backward().unwrap();
        assert_eq!(event.sequence, 2);
        assert_eq!(engine.position(), 2);

        let event = engine.step_backward().unwrap();
        assert_eq!(event.sequence, 1);
        assert_eq!(engine.position(), 1);
    }

    // -----------------------------------------------------------------------
    // 4. test_replay_engine_at_boundaries
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_at_boundaries() {
        let trace = sample_trace();
        let total = trace.events.len();
        let mut engine = ReplayEngine::new(trace);

        // At start
        assert!(engine.is_at_start());
        assert!(!engine.is_at_end());

        // step_backward at start returns None and position stays 0
        assert!(engine.step_backward().is_none());
        assert_eq!(engine.position(), 0);

        // Fast forward to end
        engine.fast_forward();
        assert_eq!(engine.position(), total - 1);
        assert!(!engine.is_at_start());
        assert!(engine.is_at_end());

        // step_forward at end returns None and position stays at end
        assert!(engine.step_forward().is_none());
        assert_eq!(engine.position(), total - 1);
    }

    // -----------------------------------------------------------------------
    // 5. test_replay_engine_seek
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_seek() {
        let mut engine = ReplayEngine::new(sample_trace());
        let event = engine.seek(4).unwrap();
        assert_eq!(event.sequence, 4);
        assert_eq!(engine.position(), 4);

        let event = engine.seek(0).unwrap();
        assert_eq!(event.sequence, 0);
        assert_eq!(engine.position(), 0);

        let event = engine.seek(8).unwrap();
        assert_eq!(event.sequence, 8);
        assert_eq!(engine.position(), 8);
    }

    // -----------------------------------------------------------------------
    // 6. test_replay_engine_seek_out_of_bounds
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_seek_out_of_bounds() {
        let mut engine = ReplayEngine::new(sample_trace());
        let result = engine.seek(100);
        assert!(matches!(
            result,
            Err(ReplayError::OutOfBounds {
                position: 100,
                total: 9
            })
        ));

        // Seeking to exactly total_events is also out of bounds
        let result = engine.seek(9);
        assert!(matches!(result, Err(ReplayError::OutOfBounds { .. })));
    }

    // -----------------------------------------------------------------------
    // 7. test_replay_engine_rewind
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_rewind() {
        let mut engine = ReplayEngine::new(sample_trace());
        engine.seek(5).unwrap();
        assert_eq!(engine.position(), 5);

        engine.rewind();
        assert_eq!(engine.position(), 0);
        assert!(engine.is_at_start());
    }

    // -----------------------------------------------------------------------
    // 8. test_replay_engine_fast_forward
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_fast_forward() {
        let trace = sample_trace();
        let total = trace.events.len();
        let mut engine = ReplayEngine::new(trace);
        assert_eq!(engine.position(), 0);

        engine.fast_forward();
        assert_eq!(engine.position(), total - 1);
        assert!(engine.is_at_end());
    }

    // -----------------------------------------------------------------------
    // 9. test_replay_engine_current_event
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_current_event() {
        let engine = ReplayEngine::new(sample_trace());
        let event = engine.current_event().unwrap();
        assert_eq!(event.sequence, 0);
        assert!(matches!(
            &event.kind,
            TraceEventKind::TaskStarted { goal, .. } if goal == "test task"
        ));
    }

    // -----------------------------------------------------------------------
    // 10. test_replay_engine_snapshot
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_snapshot() {
        let mut engine = ReplayEngine::new(sample_trace());

        // At position 0 — no tools executed, no errors
        let snap = engine.snapshot();
        assert_eq!(snap.trace_id, engine.trace().trace_id);
        assert_eq!(snap.position, 0);
        assert_eq!(snap.total_events, 9);
        assert!(snap.current_event.is_some());
        assert!(snap.elapsed_from_start.is_some());
        assert!(snap.tools_executed_so_far.is_empty());
        assert_eq!(snap.errors_so_far, 0);

        // Advance past the ToolExecuted event (index 3)
        engine.seek(3).unwrap();
        let snap = engine.snapshot();
        assert_eq!(snap.position, 3);
        assert_eq!(snap.tools_executed_so_far.len(), 1);
        assert_eq!(snap.tools_executed_so_far[0], "file_read");

        // Advance past the Error event (index 7)
        engine.seek(7).unwrap();
        let snap = engine.snapshot();
        assert_eq!(snap.errors_so_far, 1);
    }

    // -----------------------------------------------------------------------
    // 11. test_replay_engine_describe_current
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_describe_current() {
        let mut engine = ReplayEngine::new(sample_trace());
        let desc = engine.describe_current();
        assert!(desc.contains("[1/9]"));
        assert!(desc.contains("Task started"));
        assert!(desc.contains("test task"));

        engine.seek(4).unwrap();
        let desc = engine.describe_current();
        assert!(desc.contains("[5/9]"));
        assert!(desc.contains("LLM call"));
        assert!(desc.contains("gpt-4"));
    }

    // -----------------------------------------------------------------------
    // 12. test_replay_engine_timeline
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_timeline() {
        let trace = sample_trace();
        let total = trace.events.len();
        let mut engine = ReplayEngine::new(trace);

        let timeline = engine.timeline();
        assert_eq!(timeline.len(), total);

        // First entry should be current
        assert!(timeline[0].is_current);
        assert!(!timeline[1].is_current);

        // Step forward and verify current marker moves
        engine.step_forward();
        let timeline = engine.timeline();
        assert!(!timeline[0].is_current);
        assert!(timeline[1].is_current);

        // All entries should have descriptions
        for entry in &timeline {
            assert!(!entry.description.is_empty());
        }
    }

    // -----------------------------------------------------------------------
    // 13. test_replay_engine_bookmarks
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_bookmarks() {
        let mut engine = ReplayEngine::new(sample_trace());

        engine.add_bookmark("start");
        engine.step_forward();
        engine.step_forward();
        engine.add_bookmark("after two steps");

        assert_eq!(engine.bookmarks().len(), 2);
        assert_eq!(engine.bookmarks()[0].position, 0);
        assert_eq!(engine.bookmarks()[0].label, "start");
        assert_eq!(engine.bookmarks()[1].position, 2);
        assert_eq!(engine.bookmarks()[1].label, "after two steps");

        // Jump back to the first bookmark
        let event = engine.goto_bookmark(0).unwrap();
        assert_eq!(event.sequence, 0);
        assert_eq!(engine.position(), 0);

        // Timeline should reflect bookmarks
        let timeline = engine.timeline();
        assert!(timeline[0].is_bookmarked);
        assert!(!timeline[1].is_bookmarked);
        assert!(timeline[2].is_bookmarked);
    }

    // -----------------------------------------------------------------------
    // 14. test_replay_engine_bookmark_out_of_bounds
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_bookmark_out_of_bounds() {
        let mut engine = ReplayEngine::new(sample_trace());
        let result = engine.goto_bookmark(0);
        assert!(matches!(result, Err(ReplayError::BookmarkNotFound(0))));

        engine.add_bookmark("only one");
        let result = engine.goto_bookmark(5);
        assert!(matches!(result, Err(ReplayError::BookmarkNotFound(5))));
    }

    // -----------------------------------------------------------------------
    // 15. test_replay_engine_skip_to_tool
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_skip_to_tool() {
        let mut engine = ReplayEngine::new(sample_trace());

        // From position 0 (TaskStarted), skip to first tool event (index 1)
        let event = engine.skip_to_next_tool_event().unwrap();
        assert_eq!(event.sequence, 1);
        assert!(matches!(
            &event.kind,
            TraceEventKind::ToolRequested { tool, .. } if tool == "file_read"
        ));
        assert_eq!(engine.position(), 1);

        // Skip again — next tool event is ToolApproved at index 2
        let event = engine.skip_to_next_tool_event().unwrap();
        assert_eq!(event.sequence, 2);
        assert!(matches!(
            &event.kind,
            TraceEventKind::ToolApproved { tool } if tool == "file_read"
        ));

        // Skip to ToolExecuted at index 3
        let event = engine.skip_to_next_tool_event().unwrap();
        assert_eq!(event.sequence, 3);

        // Skip to second ToolRequested at index 5
        let event = engine.skip_to_next_tool_event().unwrap();
        assert_eq!(event.sequence, 5);

        // Skip to ToolDenied at index 6
        let event = engine.skip_to_next_tool_event().unwrap();
        assert_eq!(event.sequence, 6);

        // No more tool events after this
        assert!(engine.skip_to_next_tool_event().is_none());
    }

    // -----------------------------------------------------------------------
    // 16. test_replay_engine_cumulative_usage
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_cumulative_usage() {
        let mut engine = ReplayEngine::new(sample_trace());

        // At position 0 (TaskStarted), no LLM calls yet
        let usage = engine.cumulative_usage();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);

        // Seek to position 4 (LlmCall event)
        engine.seek(4).unwrap();
        let usage = engine.cumulative_usage();
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.total(), 700);

        // At end, should still be 500/200 (only one LLM call in the trace)
        engine.fast_forward();
        let usage = engine.cumulative_usage();
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.output_tokens, 200);
    }

    // -----------------------------------------------------------------------
    // 17. test_replay_engine_cumulative_cost
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_cumulative_cost() {
        let mut engine = ReplayEngine::new(sample_trace());

        // At start, no cost
        let cost = engine.cumulative_cost();
        assert!((cost.total() - 0.0).abs() < f64::EPSILON);

        // After LlmCall event at position 4
        engine.seek(4).unwrap();
        let cost = engine.cumulative_cost();
        assert!((cost.total() - 0.021).abs() < 0.001);
        // input_cost and output_cost should be proportional to tokens
        assert!(cost.input_cost > 0.0);
        assert!(cost.output_cost > 0.0);

        // At end, total cost remains the same
        engine.fast_forward();
        let cost = engine.cumulative_cost();
        assert!((cost.total() - 0.021).abs() < 0.001);
    }

    // -----------------------------------------------------------------------
    // 18. test_replay_engine_from_store
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_from_store() {
        let trace = sample_trace();
        let trace_id = trace.trace_id;
        let mut store = AuditStore::new();
        store.add_trace(trace);

        let engine = ReplayEngine::from_store(&store, trace_id).unwrap();
        assert_eq!(engine.trace().trace_id, trace_id);
        assert_eq!(engine.position(), 0);
        assert_eq!(engine.total_events(), 9);
    }

    // -----------------------------------------------------------------------
    // 19. test_replay_engine_from_store_not_found
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_engine_from_store_not_found() {
        let store = AuditStore::new();
        let missing_id = Uuid::new_v4();
        let result = ReplayEngine::from_store(&store, missing_id);
        assert!(matches!(result, Err(ReplayError::TraceNotFound(id)) if id == missing_id));
    }

    // -----------------------------------------------------------------------
    // 20. test_replay_session_new
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_session_new() {
        let session = ReplaySession::new();
        assert!(session.is_empty());
        assert_eq!(session.len(), 0);
        assert!(session.active().is_none());
        assert!(session.list_replays().is_empty());
    }

    // -----------------------------------------------------------------------
    // 21. test_replay_session_add_and_activate
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_session_add_and_activate() {
        let mut session = ReplaySession::new();

        let idx0 = session.add_replay(sample_trace());
        assert_eq!(idx0, 0);
        assert_eq!(session.len(), 1);
        assert!(!session.is_empty());

        // First replay is auto-activated
        assert!(session.active().is_some());
        assert_eq!(session.active().unwrap().position(), 0);

        let idx1 = session.add_replay(sample_trace());
        assert_eq!(idx1, 1);
        assert_eq!(session.len(), 2);

        // Active is still the first replay
        let summaries = session.list_replays();
        assert!(summaries[0].is_active);
        assert!(!summaries[1].is_active);

        // Switch to second replay
        session.set_active(1).unwrap();
        let summaries = session.list_replays();
        assert!(!summaries[0].is_active);
        assert!(summaries[1].is_active);

        // Invalid index
        assert!(session.set_active(10).is_err());
    }

    // -----------------------------------------------------------------------
    // 22. test_replay_session_list
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_session_list() {
        let mut session = ReplaySession::new();

        let t1 = sample_trace();
        let id1 = t1.trace_id;
        session.add_replay(t1);

        let t2 = sample_trace();
        let id2 = t2.trace_id;
        session.add_replay(t2);

        let list = session.list_replays();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].index, 0);
        assert_eq!(list[0].trace_id, id1);
        assert_eq!(list[0].goal, "test task");
        assert_eq!(list[0].event_count, 9);
        assert!(list[0].is_active);

        assert_eq!(list[1].index, 1);
        assert_eq!(list[1].trace_id, id2);
        assert!(!list[1].is_active);
    }

    // -----------------------------------------------------------------------
    // 23. test_describe_event_variants
    // -----------------------------------------------------------------------
    #[test]
    fn test_describe_event_variants() {
        // TaskStarted
        let desc = describe_event(&TraceEventKind::TaskStarted {
            task_id: Uuid::new_v4(),
            goal: "refactor auth".into(),
        });
        assert!(desc.contains("Task started"));
        assert!(desc.contains("refactor auth"));

        // TaskCompleted — success
        let desc = describe_event(&TraceEventKind::TaskCompleted {
            task_id: Uuid::new_v4(),
            success: true,
            iterations: 5,
        });
        assert!(desc.contains("completed successfully"));
        assert!(desc.contains("5 iterations"));

        // TaskCompleted — failure
        let desc = describe_event(&TraceEventKind::TaskCompleted {
            task_id: Uuid::new_v4(),
            success: false,
            iterations: 3,
        });
        assert!(desc.contains("failed"));
        assert!(desc.contains("3 iterations"));

        // ToolRequested
        let desc = describe_event(&TraceEventKind::ToolRequested {
            tool: "file_read".into(),
            risk_level: RiskLevel::ReadOnly,
            args_summary: "".into(),
        });
        assert!(desc.contains("Tool requested"));
        assert!(desc.contains("file_read"));
        assert!(desc.contains("ReadOnly"));

        // ToolApproved
        let desc = describe_event(&TraceEventKind::ToolApproved {
            tool: "shell_exec".into(),
        });
        assert!(desc.contains("Tool approved"));
        assert!(desc.contains("shell_exec"));

        // ToolDenied
        let desc = describe_event(&TraceEventKind::ToolDenied {
            tool: "file_write".into(),
            reason: "not allowed".into(),
        });
        assert!(desc.contains("Tool denied"));
        assert!(desc.contains("file_write"));
        assert!(desc.contains("not allowed"));

        // ApprovalRequested
        let desc = describe_event(&TraceEventKind::ApprovalRequested {
            tool: "deploy".into(),
            context: "production".into(),
        });
        assert!(desc.contains("Approval requested for"));
        assert!(desc.contains("deploy"));

        // ApprovalDecision — granted
        let desc = describe_event(&TraceEventKind::ApprovalDecision {
            tool: "deploy".into(),
            approved: true,
        });
        assert!(desc.contains("granted"));
        assert!(desc.contains("deploy"));

        // ApprovalDecision — rejected
        let desc = describe_event(&TraceEventKind::ApprovalDecision {
            tool: "deploy".into(),
            approved: false,
        });
        assert!(desc.contains("rejected"));
        assert!(desc.contains("deploy"));

        // ToolExecuted
        let desc = describe_event(&TraceEventKind::ToolExecuted {
            tool: "grep".into(),
            success: true,
            duration_ms: 42,
            output_preview: "match found".into(),
        });
        assert!(desc.contains("Tool executed"));
        assert!(desc.contains("grep"));
        assert!(desc.contains("ok"));
        assert!(desc.contains("42ms"));

        // ToolExecuted — failure
        let desc = describe_event(&TraceEventKind::ToolExecuted {
            tool: "grep".into(),
            success: false,
            duration_ms: 100,
            output_preview: "".into(),
        });
        assert!(desc.contains("failed"));
        assert!(desc.contains("100ms"));

        // LlmCall
        let desc = describe_event(&TraceEventKind::LlmCall {
            model: "gpt-4".into(),
            input_tokens: 1000,
            output_tokens: 500,
            cost: 0.05,
        });
        assert!(desc.contains("LLM call"));
        assert!(desc.contains("gpt-4"));
        assert!(desc.contains("1000/500 tokens"));

        // StatusChange
        let desc = describe_event(&TraceEventKind::StatusChange {
            from: "idle".into(),
            to: "thinking".into(),
        });
        assert!(desc.contains("Status"));
        assert!(desc.contains("idle"));
        assert!(desc.contains("thinking"));

        // Error
        let desc = describe_event(&TraceEventKind::Error {
            message: "something failed".into(),
        });
        assert!(desc.contains("Error"));
        assert!(desc.contains("something failed"));
    }

    // -----------------------------------------------------------------------
    // 24. test_replay_error_display
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_error_display() {
        let err = ReplayError::TraceNotFound(Uuid::nil());
        assert!(err.to_string().contains("trace not found"));

        let err = ReplayError::OutOfBounds {
            position: 10,
            total: 5,
        };
        let msg = err.to_string();
        assert!(msg.contains("10"));
        assert!(msg.contains("5"));

        let err = ReplayError::BookmarkNotFound(3);
        assert!(err.to_string().contains("3"));

        let err = ReplayError::EmptyTrace;
        assert!(err.to_string().contains("empty trace"));
    }

    // -----------------------------------------------------------------------
    // 25. test_replay_snapshot_progress
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_snapshot_progress() {
        let mut engine = ReplayEngine::new(sample_trace());

        // At start: 0%
        let snap = engine.snapshot();
        assert!((snap.progress_pct - 0.0).abs() < f64::EPSILON);

        // At midpoint (position 4 of 9 events = 4/8 = 50%)
        engine.seek(4).unwrap();
        let snap = engine.snapshot();
        assert!((snap.progress_pct - 50.0).abs() < f64::EPSILON);

        // At end: 100%
        engine.fast_forward();
        let snap = engine.snapshot();
        assert!((snap.progress_pct - 100.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // 26. test_replay_session_default
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_session_default() {
        let session = ReplaySession::default();
        assert!(session.is_empty());
        assert_eq!(session.len(), 0);
        assert!(session.active().is_none());
    }

    // -----------------------------------------------------------------------
    // Bonus tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_replay_session_active_mut() {
        let mut session = ReplaySession::new();
        session.add_replay(sample_trace());

        // Use active_mut to advance the engine
        let engine = session.active_mut().unwrap();
        engine.step_forward();
        assert_eq!(engine.position(), 1);

        // Verify position is persisted
        assert_eq!(session.active().unwrap().position(), 1);
    }

    #[test]
    fn test_replay_engine_empty_trace() {
        // Create a trace and manually clear its events to test empty behavior
        let mut trace = ExecutionTrace::new(Uuid::new_v4(), Uuid::new_v4(), "empty");
        trace.events.clear();

        let mut engine = ReplayEngine::new(trace);
        assert_eq!(engine.position(), 0);
        assert_eq!(engine.total_events(), 0);
        assert!(engine.is_at_start());
        assert!(engine.is_at_end());
        assert!(engine.current_event().is_none());
        assert!(engine.step_forward().is_none());
        assert!(engine.step_backward().is_none());
        assert!(engine.skip_to_next_tool_event().is_none());
        assert_eq!(engine.describe_current(), "No events");

        let snap = engine.snapshot();
        assert_eq!(snap.total_events, 0);
        assert!((snap.progress_pct - 0.0).abs() < f64::EPSILON);
        assert!(snap.current_event.is_none());
        assert!(snap.elapsed_from_start.is_none());

        let timeline = engine.timeline();
        assert!(timeline.is_empty());
    }

    #[test]
    fn test_replay_engine_walk_all_events() {
        let mut engine = ReplayEngine::new(sample_trace());
        let total = engine.total_events();
        let mut count = 1; // already at position 0

        while engine.step_forward().is_some() {
            count += 1;
        }

        assert_eq!(count, total);
        assert!(engine.is_at_end());

        // Walk backwards
        count = 1;
        while engine.step_backward().is_some() {
            count += 1;
        }

        assert_eq!(count, total);
        assert!(engine.is_at_start());
    }

    #[test]
    fn test_replay_snapshot_serialization() {
        let engine = ReplayEngine::new(sample_trace());
        let snap = engine.snapshot();

        let json = serde_json::to_string(&snap).unwrap();
        let restored: ReplaySnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.trace_id, snap.trace_id);
        assert_eq!(restored.position, snap.position);
        assert_eq!(restored.total_events, snap.total_events);
        assert_eq!(restored.errors_so_far, snap.errors_so_far);
    }

    #[test]
    fn test_timeline_entry_serialization() {
        let engine = ReplayEngine::new(sample_trace());
        let timeline = engine.timeline();

        let json = serde_json::to_string(&timeline).unwrap();
        let restored: Vec<TimelineEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), timeline.len());
        assert_eq!(restored[0].sequence, 0);
    }

    #[test]
    fn test_bookmark_serialization() {
        let bookmark = Bookmark {
            position: 5,
            label: "important point".into(),
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&bookmark).unwrap();
        let restored: Bookmark = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.position, 5);
        assert_eq!(restored.label, "important point");
    }

    #[test]
    fn test_replay_summary_serialization() {
        let summary = ReplaySummary {
            index: 0,
            trace_id: Uuid::new_v4(),
            goal: "test goal".into(),
            event_count: 42,
            is_active: true,
        };

        let json = serde_json::to_string(&summary).unwrap();
        let restored: ReplaySummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.index, 0);
        assert_eq!(restored.goal, "test goal");
        assert_eq!(restored.event_count, 42);
        assert!(restored.is_active);
    }
}
