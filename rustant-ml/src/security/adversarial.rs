//! Adversarial input detection.

use serde::{Deserialize, Serialize};

/// Adversarial input detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialDetectionResult {
    pub is_adversarial: bool,
    pub confidence: f64,
    pub attack_type: Option<String>,
    pub indicators: Vec<String>,
}

/// Adversarial input detector.
pub struct AdversarialInputDetector;

impl Default for AdversarialInputDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl AdversarialInputDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect(&self, input: &str) -> AdversarialDetectionResult {
        let mut indicators = Vec::new();
        if input.len() > 10000 {
            indicators.push("Unusually long input".into());
        }
        if input.chars().filter(|c| !c.is_ascii()).count() > input.len() / 2 {
            indicators.push("High non-ASCII ratio".into());
        }

        AdversarialDetectionResult {
            is_adversarial: !indicators.is_empty(),
            confidence: if indicators.is_empty() { 0.0 } else { 0.5 },
            attack_type: indicators.first().map(|_| "suspicious_input".into()),
            indicators,
        }
    }
}
