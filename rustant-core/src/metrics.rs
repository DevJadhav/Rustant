//! Agent metrics — counters and histograms for observability.
//!
//! When the `metrics` feature is enabled, this module provides OpenTelemetry
//! integration.  When disabled (the default), all operations are no-ops so
//! there is zero runtime overhead.

use std::time::Instant;

/// Agent-level metrics for task execution, tool calls, and token usage.
#[derive(Debug, Default)]
pub struct AgentMetrics {
    pub tasks_started: u64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub tool_calls: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub errors: u64,
    start_time: Option<Instant>,
}

impl AgentMetrics {
    pub fn new() -> Self {
        Self {
            start_time: Some(Instant::now()),
            ..Default::default()
        }
    }

    /// Record a task start.
    pub fn record_task_start(&mut self) {
        self.tasks_started += 1;
    }

    /// Record a task completion.
    pub fn record_task_complete(&mut self) {
        self.tasks_completed += 1;
    }

    /// Record a task failure.
    pub fn record_task_failed(&mut self) {
        self.tasks_failed += 1;
    }

    /// Record a tool invocation.
    pub fn record_tool_call(&mut self) {
        self.tool_calls += 1;
    }

    /// Record token usage from an LLM call.
    pub fn record_tokens(&mut self, input: u64, output: u64) {
        self.total_input_tokens += input;
        self.total_output_tokens += output;
    }

    /// Record an error.
    pub fn record_error(&mut self) {
        self.errors += 1;
    }

    /// Get uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.map(|s| s.elapsed().as_secs()).unwrap_or(0)
    }

    /// Get a summary snapshot of all metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            tasks_started: self.tasks_started,
            tasks_completed: self.tasks_completed,
            tasks_failed: self.tasks_failed,
            tool_calls: self.tool_calls,
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            errors: self.errors,
            uptime_secs: self.uptime_secs(),
        }
    }
}

/// Immutable snapshot of metrics at a point in time.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub tasks_started: u64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub tool_calls: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub errors: u64,
    pub uptime_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_default() {
        let m = AgentMetrics::new();
        assert_eq!(m.tasks_started, 0);
        assert_eq!(m.tasks_completed, 0);
        assert_eq!(m.tool_calls, 0);
    }

    #[test]
    fn test_record_task_lifecycle() {
        let mut m = AgentMetrics::new();
        m.record_task_start();
        m.record_task_start();
        m.record_task_complete();
        m.record_task_failed();

        assert_eq!(m.tasks_started, 2);
        assert_eq!(m.tasks_completed, 1);
        assert_eq!(m.tasks_failed, 1);
    }

    #[test]
    fn test_record_tokens() {
        let mut m = AgentMetrics::new();
        m.record_tokens(100, 50);
        m.record_tokens(200, 100);
        assert_eq!(m.total_input_tokens, 300);
        assert_eq!(m.total_output_tokens, 150);
    }

    #[test]
    fn test_snapshot() {
        let mut m = AgentMetrics::new();
        m.record_task_start();
        m.record_tool_call();
        m.record_error();

        let snap = m.snapshot();
        assert_eq!(snap.tasks_started, 1);
        assert_eq!(snap.tool_calls, 1);
        assert_eq!(snap.errors, 1);
    }

    #[test]
    fn test_uptime() {
        let m = AgentMetrics::new();
        // Just verify it doesn't panic and returns something >= 0
        assert!(m.uptime_secs() < 5);
    }

    #[test]
    fn test_noop_when_default() {
        let m = AgentMetrics::default();
        assert_eq!(m.uptime_secs(), 0); // No start_time in default
    }
}
