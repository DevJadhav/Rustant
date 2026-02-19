//! Data exfiltration detection.

use serde::{Deserialize, Serialize};

/// Data exfiltration detector.
pub struct DataExfiltrationDetector {
    sensitive_patterns: Vec<regex::Regex>,
}

impl Default for DataExfiltrationDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DataExfiltrationDetector {
    pub fn new() -> Self {
        let patterns = vec![
            r"(?i)model\s+weights?",
            r"(?i)training\s+data",
            r"(?i)system\s+prompt",
            r"(?i)api[_\s]?key",
            r"(?i)secret[_\s]?token",
        ];
        Self {
            sensitive_patterns: patterns
                .into_iter()
                .filter_map(|p| regex::Regex::new(p).ok())
                .collect(),
        }
    }

    pub fn scan(&self, text: &str) -> ExfiltrationResult {
        let matches: Vec<String> = self
            .sensitive_patterns
            .iter()
            .filter(|re| re.is_match(text))
            .filter_map(|re| re.find(text).map(|m| m.as_str().to_string()))
            .collect();

        let risk_level = if matches.is_empty() {
            "none".to_string()
        } else {
            "high".to_string()
        };
        ExfiltrationResult {
            risk_detected: !matches.is_empty(),
            matched_patterns: matches,
            risk_level,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExfiltrationResult {
    pub risk_detected: bool,
    pub matched_patterns: Vec<String>,
    pub risk_level: String,
}
