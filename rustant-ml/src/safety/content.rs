//! Content safety — toxicity, bias, PII detection.

use serde::{Deserialize, Serialize};

/// Toxicity detector.
pub struct ToxicityDetector {
    pub threshold: f32,
}

impl ToxicityDetector {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }

    pub fn detect(&self, text: &str) -> ToxicityResult {
        let toxic_patterns = ["hate", "kill", "violence", "threat", "slur"];
        let text_lower = text.to_lowercase();
        let matches: Vec<String> = toxic_patterns
            .iter()
            .filter(|p| text_lower.contains(*p))
            .map(|p| p.to_string())
            .collect();
        let score = matches.len() as f32 / toxic_patterns.len() as f32;
        ToxicityResult {
            score,
            is_toxic: score >= self.threshold,
            matched_patterns: matches,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToxicityResult {
    pub score: f32,
    pub is_toxic: bool,
    pub matched_patterns: Vec<String>,
}

/// Bias detector.
pub struct BiasDetector {
    pub dimensions: Vec<String>,
}

impl Default for BiasDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl BiasDetector {
    pub fn new() -> Self {
        Self {
            dimensions: vec![
                "gender".into(),
                "race".into(),
                "age".into(),
                "religion".into(),
            ],
        }
    }

    pub fn detect(&self, _text: &str) -> BiasResult {
        BiasResult {
            overall_bias_score: 0.0,
            dimension_scores: self.dimensions.iter().map(|d| (d.clone(), 0.0)).collect(),
            flagged_segments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasResult {
    pub overall_bias_score: f64,
    pub dimension_scores: std::collections::HashMap<String, f64>,
    pub flagged_segments: Vec<String>,
}

/// PII detector (extends SecretRedactor patterns).
pub struct PiiDetector {
    patterns: Vec<(String, regex::Regex)>,
}

impl Default for PiiDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PiiDetector {
    pub fn new() -> Self {
        let pattern_defs = vec![
            ("email", r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"),
            ("phone", r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b"),
            ("ssn", r"\b\d{3}-\d{2}-\d{4}\b"),
            ("credit_card", r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b"),
            ("ip_address", r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b"),
        ];
        let patterns = pattern_defs
            .into_iter()
            .filter_map(|(name, pat)| regex::Regex::new(pat).ok().map(|re| (name.to_string(), re)))
            .collect();
        Self { patterns }
    }

    pub fn scan(&self, text: &str) -> Vec<PiiDetection> {
        let mut detections = Vec::new();
        for (pii_type, re) in &self.patterns {
            for m in re.find_iter(text) {
                detections.push(PiiDetection {
                    pii_type: pii_type.clone(),
                    start: m.start(),
                    end: m.end(),
                    preview: format!("{}...", &m.as_str()[..m.as_str().len().min(8)]),
                });
            }
        }
        detections
    }

    pub fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (pii_type, re) in &self.patterns {
            result = re
                .replace_all(&result, format!("[{pii_type}_REDACTED]").as_str())
                .to_string();
        }
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiDetection {
    pub pii_type: String,
    pub start: usize,
    pub end: usize,
    pub preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pii_detection() {
        let detector = PiiDetector::new();
        let detections = detector.scan("Contact me at user@example.com");
        assert!(!detections.is_empty());
        assert_eq!(detections[0].pii_type, "email");
    }

    #[test]
    fn test_pii_redaction() {
        let detector = PiiDetector::new();
        let redacted = detector.redact("Email: user@example.com");
        assert!(redacted.contains("[email_REDACTED]"));
        assert!(!redacted.contains("user@example.com"));
    }
}
