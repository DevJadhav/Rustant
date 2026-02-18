//! Evaluation framework for agent execution traces.
//!
//! Provides structured evaluators that analyze execution traces to detect
//! errors, inefficiencies, and improvement opportunities. Built on top of
//! the existing `AuditStore`/`ExecutionTrace` infrastructure.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::audit::{ExecutionTrace, TraceEventKind};

/// Error category taxonomy for trace annotations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Tool selected incorrectly for the task.
    WrongToolSelection,
    /// Correct tool but wrong parameters.
    IncorrectParameters,
    /// Safety denial that blocked needed operation.
    FalsePositiveSafety,
    /// Unsafe operation that was not caught.
    FalseNegativeSafety,
    /// Agent looped without making progress.
    InfiniteLoop,
    /// Context window overflow or compression loss.
    ContextOverflow,
    /// Task misclassification leading to wrong routing.
    MisclassifiedTask,
    /// External dependency failure.
    ExternalFailure,
    /// Custom/user-defined category.
    Custom(String),
}

/// A single evaluation annotation on a trace event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceAnnotation {
    /// Index of the event in the trace being annotated.
    pub event_index: usize,
    /// Error category (if this is an error annotation).
    pub category: Option<ErrorCategory>,
    /// Human-readable note.
    pub note: String,
    /// Severity: 0 (info) to 3 (critical).
    pub severity: u8,
}

/// Metrics for binary classification evaluation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinaryMetrics {
    pub true_positives: usize,
    pub true_negatives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
}

impl BinaryMetrics {
    pub fn precision(&self) -> f64 {
        let denom = self.true_positives + self.false_positives;
        if denom == 0 {
            0.0
        } else {
            self.true_positives as f64 / denom as f64
        }
    }

    pub fn recall(&self) -> f64 {
        let denom = self.true_positives + self.false_negatives;
        if denom == 0 {
            0.0
        } else {
            self.true_positives as f64 / denom as f64
        }
    }

    pub fn f1(&self) -> f64 {
        let p = self.precision();
        let r = self.recall();
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    pub fn accuracy(&self) -> f64 {
        let total =
            self.true_positives + self.true_negatives + self.false_positives + self.false_negatives;
        if total == 0 {
            0.0
        } else {
            (self.true_positives + self.true_negatives) as f64 / total as f64
        }
    }
}

/// Trait for evaluators that analyze execution traces.
pub trait TraceEvaluator: Send + Sync {
    /// Evaluate a trace and return annotations.
    fn evaluate(&self, trace: &ExecutionTrace) -> Vec<TraceAnnotation>;
    /// Human-readable name of this evaluator.
    fn name(&self) -> &str;
}

/// Detects looping behavior (same tool called > N times consecutively).
pub struct LoopDetectionEvaluator {
    pub max_consecutive_same_tool: usize,
}

impl Default for LoopDetectionEvaluator {
    fn default() -> Self {
        Self {
            max_consecutive_same_tool: 3,
        }
    }
}

impl TraceEvaluator for LoopDetectionEvaluator {
    fn evaluate(&self, trace: &ExecutionTrace) -> Vec<TraceAnnotation> {
        let mut annotations = Vec::new();
        let mut consecutive_count = 0usize;
        let mut last_tool = String::new();

        for (i, event) in trace.events.iter().enumerate() {
            if let Some(tool_name) = Self::extract_tool_name(&event.kind) {
                if tool_name == last_tool {
                    consecutive_count += 1;
                    if consecutive_count >= self.max_consecutive_same_tool {
                        annotations.push(TraceAnnotation {
                            event_index: i,
                            category: Some(ErrorCategory::InfiniteLoop),
                            note: format!(
                                "Tool '{}' called {} times consecutively",
                                tool_name,
                                consecutive_count + 1
                            ),
                            severity: 2,
                        });
                    }
                } else {
                    consecutive_count = 0;
                    last_tool = tool_name.to_string();
                }
            }
        }
        annotations
    }

    fn name(&self) -> &str {
        "loop_detection"
    }
}

impl LoopDetectionEvaluator {
    fn extract_tool_name(kind: &TraceEventKind) -> Option<&str> {
        match kind {
            TraceEventKind::ToolExecuted { tool, .. } => Some(tool.as_str()),
            _ => None,
        }
    }
}

/// Detects safety false positives (tool denied then re-attempted and approved).
pub struct SafetyFalsePositiveEvaluator;

impl TraceEvaluator for SafetyFalsePositiveEvaluator {
    fn evaluate(&self, trace: &ExecutionTrace) -> Vec<TraceAnnotation> {
        let mut annotations = Vec::new();
        let mut denied_tools: HashMap<String, usize> = HashMap::new();

        for (i, event) in trace.events.iter().enumerate() {
            match &event.kind {
                TraceEventKind::ToolDenied { tool, .. } => {
                    denied_tools.insert(tool.clone(), i);
                }
                TraceEventKind::ToolApproved { tool } => {
                    if let Some(denied_idx) = denied_tools.remove(tool.as_str()) {
                        annotations.push(TraceAnnotation {
                            event_index: denied_idx,
                            category: Some(ErrorCategory::FalsePositiveSafety),
                            note: format!(
                                "Tool '{}' was denied at event {} but later approved at event {}",
                                tool, denied_idx, i
                            ),
                            severity: 1,
                        });
                    }
                }
                _ => {}
            }
        }
        annotations
    }

    fn name(&self) -> &str {
        "safety_false_positive"
    }
}

/// Evaluates cost efficiency (tokens per successful tool call).
pub struct CostEfficiencyEvaluator {
    pub max_tokens_per_tool_call: usize,
}

impl Default for CostEfficiencyEvaluator {
    fn default() -> Self {
        Self {
            max_tokens_per_tool_call: 5000,
        }
    }
}

impl TraceEvaluator for CostEfficiencyEvaluator {
    fn evaluate(&self, trace: &ExecutionTrace) -> Vec<TraceAnnotation> {
        let mut total_tokens = 0usize;
        let mut successful_tools = 0usize;

        for event in &trace.events {
            match &event.kind {
                TraceEventKind::LlmCall {
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    total_tokens += *input_tokens + *output_tokens;
                }
                TraceEventKind::ToolExecuted { success, .. } if *success => {
                    successful_tools += 1;
                }
                _ => {}
            }
        }

        let mut annotations = Vec::new();
        if successful_tools > 0 {
            let tokens_per_call = total_tokens / successful_tools;
            if tokens_per_call > self.max_tokens_per_tool_call {
                annotations.push(TraceAnnotation {
                    event_index: 0,
                    category: None,
                    note: format!(
                        "High token cost: {} tokens per successful tool call (threshold: {})",
                        tokens_per_call, self.max_tokens_per_tool_call
                    ),
                    severity: 1,
                });
            }
        }
        annotations
    }

    fn name(&self) -> &str {
        "cost_efficiency"
    }
}

/// Checks tool selection against task classification expectations.
pub struct ToolSelectionEvaluator;

impl TraceEvaluator for ToolSelectionEvaluator {
    fn evaluate(&self, _trace: &ExecutionTrace) -> Vec<TraceAnnotation> {
        // Placeholder -- requires TaskClassification context not available in trace alone.
        // Will be enhanced when traces include classification metadata.
        Vec::new()
    }

    fn name(&self) -> &str {
        "tool_selection"
    }
}

/// Aggregate evaluation results over a corpus of traces.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub total_traces: usize,
    pub successful_traces: usize,
    pub failed_traces: usize,
    pub error_distribution: HashMap<ErrorCategory, usize>,
    pub avg_iterations_per_task: f64,
    pub avg_cost_per_task: f64,
    pub tool_accuracy: BinaryMetrics,
    pub safety_metrics: BinaryMetrics,
}

/// Pipeline that runs multiple evaluators over a corpus of traces.
pub struct EvaluationPipeline {
    evaluators: Vec<Box<dyn TraceEvaluator>>,
}

impl EvaluationPipeline {
    pub fn new() -> Self {
        Self {
            evaluators: Vec::new(),
        }
    }

    pub fn add_evaluator(&mut self, evaluator: Box<dyn TraceEvaluator>) {
        self.evaluators.push(evaluator);
    }

    /// Create a pipeline with all default evaluators.
    pub fn with_defaults() -> Self {
        let mut pipeline = Self::new();
        pipeline.add_evaluator(Box::new(LoopDetectionEvaluator::default()));
        pipeline.add_evaluator(Box::new(SafetyFalsePositiveEvaluator));
        pipeline.add_evaluator(Box::new(CostEfficiencyEvaluator::default()));
        pipeline.add_evaluator(Box::new(ToolSelectionEvaluator));
        pipeline
    }

    /// Evaluate a corpus of traces and produce an aggregate report.
    pub fn evaluate_corpus(&self, traces: &[ExecutionTrace]) -> EvaluationReport {
        let mut report = EvaluationReport {
            total_traces: traces.len(),
            ..Default::default()
        };

        for trace in traces {
            match trace.success {
                Some(true) => report.successful_traces += 1,
                Some(false) => report.failed_traces += 1,
                None => {}
            }

            for evaluator in &self.evaluators {
                let annotations = evaluator.evaluate(trace);
                for annotation in annotations {
                    if let Some(cat) = annotation.category {
                        *report.error_distribution.entry(cat).or_insert(0) += 1;
                    }
                }
            }
        }

        report
    }
}

impl Default for EvaluationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract behavioral rules from evaluation annotations.
/// Frequent error patterns become improvement suggestions.
pub fn distill_corrections(report: &EvaluationReport) -> Vec<String> {
    let mut rules = Vec::new();
    for (category, count) in &report.error_distribution {
        if *count >= 3 {
            let rule = match category {
                ErrorCategory::WrongToolSelection => {
                    "When uncertain about tool selection, prefer dedicated tools over shell_exec."
                }
                ErrorCategory::InfiniteLoop => {
                    "If the same tool fails twice, try an alternative approach instead of retrying."
                }
                ErrorCategory::FalsePositiveSafety => {
                    "Review safety denials for false positives; some read-only tools may be over-restricted."
                }
                _ => continue,
            };
            rules.push(rule.to_string());
        }
    }
    rules
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::ExecutionTrace;

    use uuid::Uuid;

    /// Helper: create a minimal completed execution trace.
    fn make_trace(goal: &str, success: bool) -> ExecutionTrace {
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        let mut trace = ExecutionTrace::new(session, task, goal);
        trace.complete(success);
        trace
    }

    /// Helper: create a trace with tool execution events (no auto-complete).
    fn make_trace_raw(goal: &str) -> ExecutionTrace {
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        ExecutionTrace::new(session, task, goal)
    }

    // -----------------------------------------------------------------------
    // BinaryMetrics tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_binary_metrics_precision() {
        let m = BinaryMetrics {
            true_positives: 8,
            true_negatives: 5,
            false_positives: 2,
            false_negatives: 1,
        };
        // precision = 8 / (8 + 2) = 0.8
        assert!((m.precision() - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_binary_metrics_recall() {
        let m = BinaryMetrics {
            true_positives: 8,
            true_negatives: 5,
            false_positives: 2,
            false_negatives: 2,
        };
        // recall = 8 / (8 + 2) = 0.8
        assert!((m.recall() - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_binary_metrics_f1() {
        let m = BinaryMetrics {
            true_positives: 8,
            true_negatives: 5,
            false_positives: 2,
            false_negatives: 2,
        };
        let p = m.precision(); // 8/10 = 0.8
        let r = m.recall(); // 8/10 = 0.8
        let expected_f1 = 2.0 * p * r / (p + r); // 0.8
        assert!((m.f1() - expected_f1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_binary_metrics_zero_division() {
        let m = BinaryMetrics::default();
        assert!((m.precision() - 0.0).abs() < f64::EPSILON);
        assert!((m.recall() - 0.0).abs() < f64::EPSILON);
        assert!((m.f1() - 0.0).abs() < f64::EPSILON);
        assert!((m.accuracy() - 0.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // LoopDetectionEvaluator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_loop_detection_evaluator() {
        let evaluator = LoopDetectionEvaluator {
            max_consecutive_same_tool: 3,
        };

        let mut trace = make_trace_raw("loop test");
        // Push 5 consecutive ToolExecuted events for the same tool.
        for _ in 0..5 {
            trace.push_event(TraceEventKind::ToolExecuted {
                tool: "file_read".into(),
                success: true,
                duration_ms: 10,
                output_preview: "data".into(),
            });
        }

        let annotations = evaluator.evaluate(&trace);
        // Should detect loop at events index 4 and 5 (3rd and 4th consecutive after first).
        // consecutive_count goes 0,1,2,3 at indices 2,3,4,5 (relative to ToolExecuted events).
        // The TaskStarted event at index 0 is skipped, so ToolExecuted starts at index 1.
        // Index 1: tool=file_read, last_tool="", reset -> last_tool="file_read", count=0
        // Index 2: tool=file_read, same -> count=1
        // Index 3: tool=file_read, same -> count=2
        // Index 4: tool=file_read, same -> count=3 >= 3 -> annotation
        // Index 5: tool=file_read, same -> count=4 >= 3 -> annotation
        assert!(!annotations.is_empty());
        assert!(
            annotations
                .iter()
                .all(|a| a.category == Some(ErrorCategory::InfiniteLoop))
        );
        assert_eq!(annotations.len(), 2);
    }

    #[test]
    fn test_loop_detection_no_loop() {
        let evaluator = LoopDetectionEvaluator::default();

        let mut trace = make_trace_raw("no loop test");
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_read".into(),
            success: true,
            duration_ms: 10,
            output_preview: "data".into(),
        });
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_write".into(),
            success: true,
            duration_ms: 20,
            output_preview: "ok".into(),
        });
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "shell_exec".into(),
            success: true,
            duration_ms: 30,
            output_preview: "done".into(),
        });

        let annotations = evaluator.evaluate(&trace);
        assert!(annotations.is_empty());
    }

    // -----------------------------------------------------------------------
    // SafetyFalsePositiveEvaluator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_safety_false_positive_evaluator() {
        let evaluator = SafetyFalsePositiveEvaluator;

        let mut trace = make_trace_raw("safety fp test");
        // Tool denied first...
        trace.push_event(TraceEventKind::ToolDenied {
            tool: "file_write".into(),
            reason: "unsafe".into(),
        });
        // ...some other event...
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_read".into(),
            success: true,
            duration_ms: 5,
            output_preview: "content".into(),
        });
        // ...then the same tool is approved.
        trace.push_event(TraceEventKind::ToolApproved {
            tool: "file_write".into(),
        });

        let annotations = evaluator.evaluate(&trace);
        assert_eq!(annotations.len(), 1);
        assert_eq!(
            annotations[0].category,
            Some(ErrorCategory::FalsePositiveSafety)
        );
        // The annotation should reference the denied event index (1, since 0 is TaskStarted).
        assert_eq!(annotations[0].event_index, 1);
        assert!(annotations[0].note.contains("file_write"));
    }

    // -----------------------------------------------------------------------
    // CostEfficiencyEvaluator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_cost_efficiency_evaluator() {
        let evaluator = CostEfficiencyEvaluator {
            max_tokens_per_tool_call: 100,
        };

        let mut trace = make_trace_raw("cost test");
        // One LLM call with 500 tokens total.
        trace.push_event(TraceEventKind::LlmCall {
            model: "gpt-4".into(),
            input_tokens: 400,
            output_tokens: 100,
            cost: 0.05,
        });
        // One successful tool call -> 500/1 = 500 tokens per call > threshold 100.
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_read".into(),
            success: true,
            duration_ms: 10,
            output_preview: "data".into(),
        });

        let annotations = evaluator.evaluate(&trace);
        assert_eq!(annotations.len(), 1);
        assert!(annotations[0].note.contains("High token cost"));
        assert!(annotations[0].note.contains("500"));
    }

    #[test]
    fn test_cost_efficiency_evaluator_under_threshold() {
        let evaluator = CostEfficiencyEvaluator {
            max_tokens_per_tool_call: 5000,
        };

        let mut trace = make_trace_raw("efficient");
        trace.push_event(TraceEventKind::LlmCall {
            model: "gpt-4".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost: 0.01,
        });
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_read".into(),
            success: true,
            duration_ms: 10,
            output_preview: "data".into(),
        });

        let annotations = evaluator.evaluate(&trace);
        assert!(annotations.is_empty());
    }

    // -----------------------------------------------------------------------
    // ToolSelectionEvaluator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_tool_selection_evaluator_name() {
        let evaluator = ToolSelectionEvaluator;
        assert_eq!(evaluator.name(), "tool_selection");
        // Placeholder evaluator returns empty annotations.
        let trace = make_trace("any", true);
        let annotations = evaluator.evaluate(&trace);
        assert!(annotations.is_empty());
    }

    // -----------------------------------------------------------------------
    // EvaluationPipeline tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_evaluation_pipeline_with_defaults() {
        let pipeline = EvaluationPipeline::with_defaults();
        assert_eq!(pipeline.evaluators.len(), 4);

        // Create a corpus with a mix of traces.
        let t1 = make_trace("success task", true);

        let mut t2 = make_trace_raw("loop task");
        for _ in 0..5 {
            t2.push_event(TraceEventKind::ToolExecuted {
                tool: "shell_exec".into(),
                success: false,
                duration_ms: 100,
                output_preview: "error".into(),
            });
        }
        t2.complete(false);

        let corpus = vec![t1, t2];
        let report = pipeline.evaluate_corpus(&corpus);

        assert_eq!(report.total_traces, 2);
        assert_eq!(report.successful_traces, 1);
        assert_eq!(report.failed_traces, 1);
        // Loop detection should have found InfiniteLoop errors.
        assert!(
            report
                .error_distribution
                .contains_key(&ErrorCategory::InfiniteLoop)
        );
    }

    #[test]
    fn test_evaluation_pipeline_empty_corpus() {
        let pipeline = EvaluationPipeline::with_defaults();
        let report = pipeline.evaluate_corpus(&[]);
        assert_eq!(report.total_traces, 0);
        assert_eq!(report.successful_traces, 0);
        assert_eq!(report.failed_traces, 0);
        assert!(report.error_distribution.is_empty());
    }

    // -----------------------------------------------------------------------
    // ErrorCategory serde tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_category_serde() {
        // Standard variants.
        let cat = ErrorCategory::InfiniteLoop;
        let json = serde_json::to_string(&cat).unwrap();
        assert_eq!(json, "\"infinite_loop\"");
        let restored: ErrorCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, ErrorCategory::InfiniteLoop);

        // Custom variant.
        let custom = ErrorCategory::Custom("my_error".into());
        let json = serde_json::to_string(&custom).unwrap();
        let restored: ErrorCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, ErrorCategory::Custom("my_error".into()));

        // All standard variants round-trip.
        let variants = vec![
            ErrorCategory::WrongToolSelection,
            ErrorCategory::IncorrectParameters,
            ErrorCategory::FalsePositiveSafety,
            ErrorCategory::FalseNegativeSafety,
            ErrorCategory::InfiniteLoop,
            ErrorCategory::ContextOverflow,
            ErrorCategory::MisclassifiedTask,
            ErrorCategory::ExternalFailure,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let restored: ErrorCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, v);
        }
    }

    // -----------------------------------------------------------------------
    // distill_corrections tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_distill_corrections_below_threshold() {
        // With fewer than 3 occurrences, no rules should be generated.
        let mut report = EvaluationReport::default();
        report
            .error_distribution
            .insert(ErrorCategory::InfiniteLoop, 2);
        report
            .error_distribution
            .insert(ErrorCategory::WrongToolSelection, 1);

        let rules = distill_corrections(&report);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_distill_corrections_above_threshold() {
        let mut report = EvaluationReport::default();
        report
            .error_distribution
            .insert(ErrorCategory::InfiniteLoop, 5);
        report
            .error_distribution
            .insert(ErrorCategory::WrongToolSelection, 3);
        report
            .error_distribution
            .insert(ErrorCategory::FalsePositiveSafety, 4);

        let rules = distill_corrections(&report);
        assert_eq!(rules.len(), 3);
        assert!(
            rules
                .iter()
                .any(|r| r.contains("alternative approach instead of retrying"))
        );
        assert!(rules.iter().any(|r| r.contains("prefer dedicated tools")));
        assert!(rules.iter().any(|r| r.contains("false positives")));
    }

    #[test]
    fn test_distill_corrections_ignores_unmatched_categories() {
        // Categories without specific rules (e.g., ExternalFailure) are skipped.
        let mut report = EvaluationReport::default();
        report
            .error_distribution
            .insert(ErrorCategory::ExternalFailure, 10);
        report
            .error_distribution
            .insert(ErrorCategory::ContextOverflow, 10);

        let rules = distill_corrections(&report);
        assert!(rules.is_empty());
    }
}
