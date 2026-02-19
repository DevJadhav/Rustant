//! Reasoning trace capture.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A step in a reasoning trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub step_number: usize,
    pub action: String,
    pub rationale: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub confidence: f64,
    pub timestamp: DateTime<Utc>,
}

/// A complete reasoning trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTrace {
    pub id: Uuid,
    pub task: String,
    pub steps: Vec<ReasoningStep>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Reasoning tracer for capturing decision steps.
pub struct ReasoningTracer {
    trace: ReasoningTrace,
}

impl ReasoningTracer {
    pub fn new(task: &str) -> Self {
        Self {
            trace: ReasoningTrace {
                id: Uuid::new_v4(),
                task: task.to_string(),
                steps: Vec::new(),
                started_at: Utc::now(),
                completed_at: None,
            },
        }
    }

    pub fn add_step(&mut self, action: &str, rationale: &str, confidence: f64) {
        let step_num = self.trace.steps.len() + 1;
        self.trace.steps.push(ReasoningStep {
            step_number: step_num,
            action: action.to_string(),
            rationale: rationale.to_string(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            confidence,
            timestamp: Utc::now(),
        });
    }

    pub fn complete(mut self) -> ReasoningTrace {
        self.trace.completed_at = Some(Utc::now());
        self.trace
    }

    /// Alias for `add_step` — capture a reasoning step.
    pub fn capture_step(&mut self, action: &str, rationale: &str, confidence: f64) {
        self.add_step(action, rationale, confidence);
    }

    /// Alias for `complete` — generate the final reasoning trace.
    pub fn generate_trace(self) -> ReasoningTrace {
        self.complete()
    }
}
