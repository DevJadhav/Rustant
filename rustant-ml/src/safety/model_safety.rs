//! Model output safety validation.

use serde::{Deserialize, Serialize};

/// Output safety validation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSafetyResult {
    pub is_safe: bool,
    pub toxicity_score: f32,
    pub pii_detected: Vec<String>,
    pub bias_indicators: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Output validator.
pub struct OutputValidator {
    toxicity_threshold: f32,
}

impl OutputValidator {
    pub fn new(toxicity_threshold: f32) -> Self {
        Self { toxicity_threshold }
    }

    pub fn validate(&self, output: &str) -> OutputSafetyResult {
        let toxicity = super::content::ToxicityDetector::new(self.toxicity_threshold);
        let tox_result = toxicity.detect(output);
        let pii = super::content::PiiDetector::new();
        let pii_detections = pii.scan(output);

        OutputSafetyResult {
            is_safe: !tox_result.is_toxic && pii_detections.is_empty(),
            toxicity_score: tox_result.score,
            pii_detected: pii_detections.iter().map(|d| d.pii_type.clone()).collect(),
            bias_indicators: Vec::new(),
            recommendations: Vec::new(),
        }
    }
}

/// Detects refusal patterns in model outputs.
pub struct RefusalDetector {
    pub patterns: Vec<String>,
}

impl Default for RefusalDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl RefusalDetector {
    pub fn new() -> Self {
        Self {
            patterns: vec![
                "I cannot".into(),
                "I'm unable to".into(),
                "I am unable to".into(),
                "I'm not able to".into(),
                "I refuse to".into(),
                "I can't help with".into(),
                "I will not".into(),
                "As an AI".into(),
                "not appropriate for me".into(),
            ],
        }
    }

    /// Returns true if refusal patterns are detected in the text.
    pub fn detect(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.patterns
            .iter()
            .any(|p| lower.contains(&p.to_lowercase()))
    }
}

/// Enforces output constraints on model-generated text.
pub struct ConstraintEnforcer {
    pub max_output_length: usize,
    pub forbidden_patterns: Vec<String>,
}

impl Default for ConstraintEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintEnforcer {
    pub fn new() -> Self {
        Self {
            max_output_length: 100_000,
            forbidden_patterns: vec![
                "password".into(),
                "secret_key".into(),
                "BEGIN RSA PRIVATE KEY".into(),
                "BEGIN PRIVATE KEY".into(),
            ],
        }
    }

    /// Returns a list of constraint violations found in the text.
    pub fn enforce(&self, text: &str) -> Vec<String> {
        let mut violations = Vec::new();
        if text.len() > self.max_output_length {
            violations.push(format!(
                "Output length {} exceeds max {}",
                text.len(),
                self.max_output_length
            ));
        }
        let lower = text.to_lowercase();
        for pattern in &self.forbidden_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                violations.push(format!("Forbidden pattern detected: {pattern}"));
            }
        }
        violations
    }
}
