//! CI/CD integration — quality gates, regression detection.

use serde::{Deserialize, Serialize};

/// Evaluation gate configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGate {
    pub name: String,
    pub metric: String,
    pub threshold: f64,
    pub comparison: GateComparison,
}

/// How to compare metric against threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateComparison {
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

/// Gate evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub gate_name: String,
    pub passed: bool,
    pub actual_value: f64,
    pub threshold: f64,
    pub message: String,
}

/// Evaluate a set of gates.
pub fn evaluate_gates(
    gates: &[EvalGate],
    metrics: &std::collections::HashMap<String, f64>,
) -> Vec<GateResult> {
    gates
        .iter()
        .map(|gate| {
            let actual = metrics.get(&gate.metric).copied().unwrap_or(0.0);
            let passed = match gate.comparison {
                GateComparison::GreaterThan => actual > gate.threshold,
                GateComparison::LessThan => actual < gate.threshold,
                GateComparison::GreaterThanOrEqual => actual >= gate.threshold,
                GateComparison::LessThanOrEqual => actual <= gate.threshold,
            };
            GateResult {
                gate_name: gate.name.clone(),
                passed,
                actual_value: actual,
                threshold: gate.threshold,
                message: if passed {
                    "PASS".into()
                } else {
                    format!(
                        "FAIL: {} = {actual:.4} (threshold: {:.4})",
                        gate.metric, gate.threshold
                    )
                },
            }
        })
        .collect()
}
