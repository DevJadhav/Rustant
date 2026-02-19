//! Extended prompt injection and jailbreak detection.

use serde::{Deserialize, Serialize};

/// Enhanced injection detector (extends core injection.rs patterns).
pub struct EnhancedInjectionDetector {
    patterns: Vec<AttackPattern>,
}

/// Attack pattern definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPattern {
    pub name: String,
    pub pattern: String,
    pub category: String,
    pub severity: String,
}

impl Default for EnhancedInjectionDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl EnhancedInjectionDetector {
    pub fn new() -> Self {
        Self {
            patterns: vec![
                AttackPattern {
                    name: "benchmark_manipulation".into(),
                    pattern: "ignore.*rubric|give.*maximum.*score".into(),
                    category: "benchmark".into(),
                    severity: "high".into(),
                },
                AttackPattern {
                    name: "model_exfiltration".into(),
                    pattern: "output.*weights|training.*data.*extract".into(),
                    category: "exfiltration".into(),
                    severity: "critical".into(),
                },
                AttackPattern {
                    name: "data_poisoning".into(),
                    pattern: "inject.*training|poison.*dataset".into(),
                    category: "poisoning".into(),
                    severity: "high".into(),
                },
            ],
        }
    }

    pub fn scan(&self, text: &str) -> Vec<&AttackPattern> {
        let text_lower = text.to_lowercase();
        self.patterns
            .iter()
            .filter(|p| {
                regex::Regex::new(&p.pattern)
                    .ok()
                    .is_some_and(|re| re.is_match(&text_lower))
            })
            .collect()
    }
}

/// Jailbreak detector.
pub struct JailbreakDetector {
    known_patterns: Vec<String>,
    /// Whether to use LLM-based detection in addition to pattern matching.
    pub llm_based_detection: bool,
}

impl Default for JailbreakDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl JailbreakDetector {
    pub fn new() -> Self {
        Self {
            known_patterns: vec![
                "DAN".into(),
                "do anything now".into(),
                "ignore all previous".into(),
                "pretend you".into(),
                "bypass safety".into(),
                "jailbreak".into(),
            ],
            llm_based_detection: false,
        }
    }

    pub fn detect(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.known_patterns
            .iter()
            .any(|p| lower.contains(&p.to_lowercase()))
    }
}
