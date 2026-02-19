//! Production learning loop — incident-to-code mapping and pattern extraction.
//!
//! Links production incidents to causal code changes, tracks which patterns
//! are risky, and provides feedback integration for improving scan accuracy.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A record linking an incident to a code change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentCodeMapping {
    /// Incident identifier.
    pub incident_id: String,
    /// Related commit hashes.
    pub commit_hashes: Vec<String>,
    /// Files that were changed.
    pub changed_files: Vec<String>,
    /// Functions that were modified.
    pub changed_functions: Vec<String>,
    /// When the causal change was made.
    pub change_timestamp: DateTime<Utc>,
    /// When the incident occurred.
    pub incident_timestamp: DateTime<Utc>,
    /// Time between change and incident.
    pub time_to_incident_hours: f64,
    /// Severity of the incident.
    pub severity: String,
    /// Root cause category.
    pub root_cause: Option<String>,
}

/// A risky code pattern learned from incidents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskyPattern {
    /// Pattern identifier.
    pub id: String,
    /// Description of the risky pattern.
    pub description: String,
    /// Files commonly associated with incidents.
    pub hotspot_files: Vec<String>,
    /// Functions commonly associated with incidents.
    pub hotspot_functions: Vec<String>,
    /// How many incidents this pattern has been involved in.
    pub incident_count: usize,
    /// Confidence that this is a genuine risky pattern (0.0-1.0).
    pub confidence: f32,
    /// When this pattern was last updated.
    pub last_updated: DateTime<Utc>,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Feedback on a security finding (accepted, rejected, false positive).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingFeedback {
    /// Finding ID.
    pub finding_id: String,
    /// Scanner that produced the finding.
    pub scanner: String,
    /// Rule that triggered.
    pub rule_id: Option<String>,
    /// Feedback type.
    pub feedback_type: FeedbackType,
    /// Who provided the feedback.
    pub provided_by: String,
    /// When feedback was provided.
    pub timestamp: DateTime<Utc>,
    /// Additional notes.
    pub notes: Option<String>,
}

/// Type of feedback on a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    /// Finding was accurate and led to a fix.
    TruePositive,
    /// Finding was accurate but not actionable.
    TruePositiveNotActionable,
    /// Finding was inaccurate.
    FalsePositive,
    /// Real issue that was missed.
    FalseNegative,
}

impl std::fmt::Display for FeedbackType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeedbackType::TruePositive => write!(f, "True Positive"),
            FeedbackType::TruePositiveNotActionable => write!(f, "True Positive (Not Actionable)"),
            FeedbackType::FalsePositive => write!(f, "False Positive"),
            FeedbackType::FalseNegative => write!(f, "False Negative"),
        }
    }
}

/// Accuracy metrics for a scanner/rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccuracyMetrics {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
}

impl AccuracyMetrics {
    /// Calculate precision, recall, and F1 from counts.
    pub fn calculate(&mut self) {
        let tp = self.true_positives as f64;
        let fp = self.false_positives as f64;
        let fn_ = self.false_negatives as f64;

        self.precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };

        self.recall = if tp + fn_ > 0.0 { tp / (tp + fn_) } else { 0.0 };

        self.f1_score = if self.precision + self.recall > 0.0 {
            2.0 * (self.precision * self.recall) / (self.precision + self.recall)
        } else {
            0.0
        };
    }
}

/// Learning engine that tracks patterns and feedback.
pub struct LearningEngine {
    /// Incident-to-code mappings.
    mappings: Vec<IncidentCodeMapping>,
    /// Extracted risky patterns.
    patterns: Vec<RiskyPattern>,
    /// Finding feedback history.
    feedback: Vec<FindingFeedback>,
    /// File hotspot scores (file path -> incident count).
    hotspot_scores: HashMap<String, usize>,
    /// Function hotspot scores (function name -> incident count).
    function_hotspot_scores: HashMap<String, usize>,
}

impl LearningEngine {
    pub fn new() -> Self {
        Self {
            mappings: Vec::new(),
            patterns: Vec::new(),
            feedback: Vec::new(),
            hotspot_scores: HashMap::new(),
            function_hotspot_scores: HashMap::new(),
        }
    }

    /// Record an incident-to-code mapping.
    pub fn record_mapping(&mut self, mapping: IncidentCodeMapping) {
        // Update file hotspot scores
        for file in &mapping.changed_files {
            *self.hotspot_scores.entry(file.clone()).or_insert(0) += 1;
        }
        // Update function hotspot scores
        for func in &mapping.changed_functions {
            *self
                .function_hotspot_scores
                .entry(func.clone())
                .or_insert(0) += 1;
        }
        self.mappings.push(mapping);
    }

    /// Record feedback on a finding.
    pub fn record_feedback(&mut self, feedback: FindingFeedback) {
        self.feedback.push(feedback);
    }

    /// Extract risky patterns from accumulated mappings.
    pub fn extract_patterns(&mut self) -> Vec<RiskyPattern> {
        let mut file_incidents: HashMap<String, Vec<&IncidentCodeMapping>> = HashMap::new();

        for mapping in &self.mappings {
            for file in &mapping.changed_files {
                file_incidents
                    .entry(file.clone())
                    .or_default()
                    .push(mapping);
            }
        }

        let mut new_patterns = Vec::new();

        for (file, incidents) in &file_incidents {
            if incidents.len() >= 2 {
                // File has been involved in 2+ incidents
                let pattern = RiskyPattern {
                    id: format!("hotspot-{}", file.replace(['/', '.'], "-")),
                    description: format!(
                        "File '{}' has been involved in {} incidents",
                        file,
                        incidents.len()
                    ),
                    hotspot_files: vec![file.clone()],
                    hotspot_functions: Vec::new(),
                    incident_count: incidents.len(),
                    confidence: (incidents.len() as f32 / self.mappings.len() as f32).min(1.0),
                    last_updated: Utc::now(),
                    tags: vec!["hotspot".to_string()],
                };
                new_patterns.push(pattern);
            }
        }

        self.patterns.extend(new_patterns.clone());
        new_patterns
    }

    /// Get files associated with multiple incidents, sorted by incident count descending.
    pub fn get_risky_files(&self) -> Vec<(&str, usize)> {
        let mut risky: Vec<(&str, usize)> = self
            .hotspot_scores
            .iter()
            .filter(|(_, count)| **count >= 2)
            .map(|(file, count)| (file.as_str(), *count))
            .collect();
        risky.sort_by(|a, b| b.1.cmp(&a.1));
        risky
    }

    /// Get functions associated with multiple incidents, sorted by incident count descending.
    pub fn get_risky_functions(&self) -> Vec<(&str, usize)> {
        let mut risky: Vec<(&str, usize)> = self
            .function_hotspot_scores
            .iter()
            .filter(|(_, count)| **count >= 2)
            .map(|(func, count)| (func.as_str(), *count))
            .collect();
        risky.sort_by(|a, b| b.1.cmp(&a.1));
        risky
    }

    /// Update the confidence of a pattern by ID based on feedback.
    /// Positive feedback (true positive) increases confidence, negative (false positive) decreases.
    pub fn update_confidence(&mut self, pattern_id: &str, positive: bool) -> Option<f32> {
        let pattern = self.patterns.iter_mut().find(|p| p.id == pattern_id)?;

        if positive {
            pattern.confidence = (pattern.confidence + 0.1).min(1.0);
        } else {
            pattern.confidence = (pattern.confidence - 0.15).max(0.0);
        }
        pattern.last_updated = Utc::now();

        Some(pattern.confidence)
    }

    /// Check if a file or function change is risky based on learned patterns.
    /// Returns a risk score (0.0 = safe, 1.0 = very risky) and matching pattern descriptions.
    pub fn is_risky_change(&self, files: &[&str], functions: &[&str]) -> (f64, Vec<String>) {
        let mut max_risk = 0.0f64;
        let mut reasons = Vec::new();

        // Check file hotspots
        for file in files {
            if let Some(&count) = self.hotspot_scores.get(*file)
                && count >= 2
            {
                let risk = (count as f64 / self.mappings.len().max(1) as f64).min(1.0);
                if risk > max_risk {
                    max_risk = risk;
                }
                reasons.push(format!(
                    "File '{file}' has been involved in {count} incidents"
                ));
            }
        }

        // Check function hotspots
        for func in functions {
            if let Some(&count) = self.function_hotspot_scores.get(*func)
                && count >= 2
            {
                let risk = (count as f64 / self.mappings.len().max(1) as f64).min(1.0);
                if risk > max_risk {
                    max_risk = risk;
                }
                reasons.push(format!(
                    "Function '{func}' has been involved in {count} incidents"
                ));
            }
        }

        // Also check against extracted patterns
        for pattern in &self.patterns {
            for file in files {
                if pattern.hotspot_files.contains(&file.to_string()) {
                    let risk = pattern.confidence as f64;
                    if risk > max_risk {
                        max_risk = risk;
                    }
                    if !reasons.iter().any(|r| r.contains(*file)) {
                        reasons.push(format!("Pattern '{}': {}", pattern.id, pattern.description));
                    }
                }
            }
            for func in functions {
                if pattern.hotspot_functions.contains(&func.to_string()) {
                    let risk = pattern.confidence as f64;
                    if risk > max_risk {
                        max_risk = risk;
                    }
                    if !reasons.iter().any(|r| r.contains(*func)) {
                        reasons.push(format!("Pattern '{}': {}", pattern.id, pattern.description));
                    }
                }
            }
        }

        (max_risk, reasons)
    }

    /// Get function-level hotspot scores.
    pub fn function_hotspots(&self, limit: usize) -> Vec<(&str, usize)> {
        let mut hotspots: Vec<(&str, usize)> = self
            .function_hotspot_scores
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        hotspots.sort_by(|a, b| b.1.cmp(&a.1));
        hotspots.truncate(limit);
        hotspots
    }

    /// Calculate accuracy metrics for a specific scanner.
    pub fn accuracy_for_scanner(&self, scanner: &str) -> AccuracyMetrics {
        let mut metrics = AccuracyMetrics::default();

        for fb in &self.feedback {
            if fb.scanner == scanner {
                match fb.feedback_type {
                    FeedbackType::TruePositive | FeedbackType::TruePositiveNotActionable => {
                        metrics.true_positives += 1;
                    }
                    FeedbackType::FalsePositive => {
                        metrics.false_positives += 1;
                    }
                    FeedbackType::FalseNegative => {
                        metrics.false_negatives += 1;
                    }
                }
            }
        }

        metrics.calculate();
        metrics
    }

    /// Calculate accuracy metrics for a specific rule.
    pub fn accuracy_for_rule(&self, rule_id: &str) -> AccuracyMetrics {
        let mut metrics = AccuracyMetrics::default();

        for fb in &self.feedback {
            if fb.rule_id.as_deref() == Some(rule_id) {
                match fb.feedback_type {
                    FeedbackType::TruePositive | FeedbackType::TruePositiveNotActionable => {
                        metrics.true_positives += 1;
                    }
                    FeedbackType::FalsePositive => {
                        metrics.false_positives += 1;
                    }
                    FeedbackType::FalseNegative => {
                        metrics.false_negatives += 1;
                    }
                }
            }
        }

        metrics.calculate();
        metrics
    }

    /// Get top hotspot files (most incident-prone).
    pub fn top_hotspots(&self, limit: usize) -> Vec<(&str, usize)> {
        let mut hotspots: Vec<(&str, usize)> = self
            .hotspot_scores
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        hotspots.sort_by(|a, b| b.1.cmp(&a.1));
        hotspots.truncate(limit);
        hotspots
    }

    /// Get all recorded patterns.
    pub fn patterns(&self) -> &[RiskyPattern] {
        &self.patterns
    }

    /// Get total number of feedback entries.
    pub fn feedback_count(&self) -> usize {
        self.feedback.len()
    }

    /// Get total number of mappings.
    pub fn mapping_count(&self) -> usize {
        self.mappings.len()
    }

    /// Generate a learning summary.
    pub fn summary(&self) -> LearningSummary {
        let mut scanner_metrics: HashMap<String, AccuracyMetrics> = HashMap::new();
        let scanners: Vec<String> = self
            .feedback
            .iter()
            .map(|f| f.scanner.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for scanner in &scanners {
            scanner_metrics.insert(scanner.clone(), self.accuracy_for_scanner(scanner));
        }

        LearningSummary {
            total_mappings: self.mappings.len(),
            total_patterns: self.patterns.len(),
            total_feedback: self.feedback.len(),
            hotspot_count: self.hotspot_scores.len(),
            scanner_metrics,
        }
    }
}

impl Default for LearningEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of learning engine state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSummary {
    pub total_mappings: usize,
    pub total_patterns: usize,
    pub total_feedback: usize,
    pub hotspot_count: usize,
    pub scanner_metrics: HashMap<String, AccuracyMetrics>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapping(files: Vec<&str>, incident_id: &str) -> IncidentCodeMapping {
        let now = Utc::now();
        IncidentCodeMapping {
            incident_id: incident_id.to_string(),
            commit_hashes: vec!["abc123".to_string()],
            changed_files: files.into_iter().map(|s| s.to_string()).collect(),
            changed_functions: Vec::new(),
            change_timestamp: now,
            incident_timestamp: now,
            time_to_incident_hours: 2.0,
            severity: "high".to_string(),
            root_cause: None,
        }
    }

    fn make_feedback(scanner: &str, feedback_type: FeedbackType) -> FindingFeedback {
        FindingFeedback {
            finding_id: "f1".to_string(),
            scanner: scanner.to_string(),
            rule_id: Some("rule-1".to_string()),
            feedback_type,
            provided_by: "analyst".to_string(),
            timestamp: Utc::now(),
            notes: None,
        }
    }

    #[test]
    fn test_record_mapping() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-001"));
        assert_eq!(engine.mapping_count(), 1);
        assert_eq!(engine.top_hotspots(5).len(), 1);
    }

    #[test]
    fn test_extract_patterns() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-001"));
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-002"));
        engine.record_mapping(make_mapping(vec!["src/db.rs"], "INC-003"));

        let patterns = engine.extract_patterns();
        assert_eq!(patterns.len(), 1); // Only auth.rs has 2+ incidents
        assert!(
            patterns[0]
                .hotspot_files
                .contains(&"src/auth.rs".to_string())
        );
    }

    #[test]
    fn test_accuracy_metrics() {
        let mut engine = LearningEngine::new();
        engine.record_feedback(make_feedback("sast", FeedbackType::TruePositive));
        engine.record_feedback(make_feedback("sast", FeedbackType::TruePositive));
        engine.record_feedback(make_feedback("sast", FeedbackType::FalsePositive));

        let metrics = engine.accuracy_for_scanner("sast");
        assert_eq!(metrics.true_positives, 2);
        assert_eq!(metrics.false_positives, 1);
        assert!((metrics.precision - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_accuracy_for_rule() {
        let mut engine = LearningEngine::new();
        engine.record_feedback(make_feedback("sast", FeedbackType::TruePositive));
        engine.record_feedback(make_feedback("sast", FeedbackType::FalsePositive));

        let metrics = engine.accuracy_for_rule("rule-1");
        assert_eq!(metrics.true_positives, 1);
        assert_eq!(metrics.false_positives, 1);
        assert!((metrics.precision - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_top_hotspots() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/a.rs"], "1"));
        engine.record_mapping(make_mapping(vec!["src/a.rs"], "2"));
        engine.record_mapping(make_mapping(vec!["src/a.rs"], "3"));
        engine.record_mapping(make_mapping(vec!["src/b.rs"], "4"));
        engine.record_mapping(make_mapping(vec!["src/b.rs"], "5"));

        let hotspots = engine.top_hotspots(2);
        assert_eq!(hotspots.len(), 2);
        assert_eq!(hotspots[0].0, "src/a.rs");
        assert_eq!(hotspots[0].1, 3);
    }

    #[test]
    fn test_summary() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/a.rs"], "1"));
        engine.record_feedback(make_feedback("sast", FeedbackType::TruePositive));

        let summary = engine.summary();
        assert_eq!(summary.total_mappings, 1);
        assert_eq!(summary.total_feedback, 1);
        assert!(summary.scanner_metrics.contains_key("sast"));
    }

    #[test]
    fn test_feedback_type_display() {
        assert_eq!(FeedbackType::TruePositive.to_string(), "True Positive");
        assert_eq!(FeedbackType::FalsePositive.to_string(), "False Positive");
        assert_eq!(FeedbackType::FalseNegative.to_string(), "False Negative");
    }

    #[test]
    fn test_empty_accuracy() {
        let engine = LearningEngine::new();
        let metrics = engine.accuracy_for_scanner("nonexistent");
        assert_eq!(metrics.precision, 0.0);
        assert_eq!(metrics.recall, 0.0);
        assert_eq!(metrics.f1_score, 0.0);
    }

    #[test]
    fn test_accuracy_calculation() {
        let mut metrics = AccuracyMetrics {
            true_positives: 8,
            false_positives: 2,
            false_negatives: 1,
            ..Default::default()
        };
        metrics.calculate();

        assert!((metrics.precision - 0.8).abs() < 0.01);
        assert!((metrics.recall - 0.8889).abs() < 0.01);
        assert!(metrics.f1_score > 0.0);
    }

    fn make_mapping_with_functions(
        files: Vec<&str>,
        functions: Vec<&str>,
        incident_id: &str,
    ) -> IncidentCodeMapping {
        let now = Utc::now();
        IncidentCodeMapping {
            incident_id: incident_id.to_string(),
            commit_hashes: vec!["abc123".to_string()],
            changed_files: files.into_iter().map(|s| s.to_string()).collect(),
            changed_functions: functions.into_iter().map(|s| s.to_string()).collect(),
            change_timestamp: now,
            incident_timestamp: now,
            time_to_incident_hours: 2.0,
            severity: "high".to_string(),
            root_cause: None,
        }
    }

    #[test]
    fn test_get_risky_files() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/auth.rs", "src/db.rs"], "INC-001"));
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-002"));
        engine.record_mapping(make_mapping(vec!["src/api.rs"], "INC-003"));

        let risky = engine.get_risky_files();
        assert_eq!(risky.len(), 1); // Only auth.rs has 2+ incidents
        assert_eq!(risky[0].0, "src/auth.rs");
        assert_eq!(risky[0].1, 2);
    }

    #[test]
    fn test_get_risky_functions() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping_with_functions(
            vec!["src/auth.rs"],
            vec!["validate_token", "check_auth"],
            "INC-001",
        ));
        engine.record_mapping(make_mapping_with_functions(
            vec!["src/auth.rs"],
            vec!["validate_token"],
            "INC-002",
        ));
        engine.record_mapping(make_mapping_with_functions(
            vec!["src/db.rs"],
            vec!["run_query"],
            "INC-003",
        ));

        let risky_funcs = engine.get_risky_functions();
        assert_eq!(risky_funcs.len(), 1); // Only validate_token has 2+
        assert_eq!(risky_funcs[0].0, "validate_token");
        assert_eq!(risky_funcs[0].1, 2);
    }

    #[test]
    fn test_update_confidence() {
        let mut engine = LearningEngine::new();
        // Create enough mappings so that auth.rs has confidence < 1.0
        // auth.rs appears in 2 out of 5 mappings => confidence = 2/5 = 0.4
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-001"));
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-002"));
        engine.record_mapping(make_mapping(vec!["src/api.rs"], "INC-003"));
        engine.record_mapping(make_mapping(vec!["src/db.rs"], "INC-004"));
        engine.record_mapping(make_mapping(vec!["src/utils.rs"], "INC-005"));
        engine.extract_patterns();

        assert_eq!(engine.patterns().len(), 1);
        let pattern_id = engine.patterns()[0].id.clone();
        let initial = engine.patterns()[0].confidence;
        assert!(initial < 1.0, "Initial confidence should be < 1.0");

        // Positive feedback increases confidence
        let new_conf = engine.update_confidence(&pattern_id, true).unwrap();
        assert!(new_conf > initial);

        // Negative feedback decreases confidence
        let decreased = engine.update_confidence(&pattern_id, false).unwrap();
        assert!(decreased < new_conf);

        // Non-existent pattern returns None
        assert!(engine.update_confidence("nonexistent", true).is_none());
    }

    #[test]
    fn test_update_confidence_clamp() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-001"));
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-002"));
        engine.extract_patterns();

        let pattern_id = engine.patterns()[0].id.clone();

        // Increase many times — should clamp at 1.0
        for _ in 0..20 {
            engine.update_confidence(&pattern_id, true);
        }
        assert!((engine.patterns()[0].confidence - 1.0).abs() < f32::EPSILON);

        // Decrease many times — should clamp at 0.0
        for _ in 0..50 {
            engine.update_confidence(&pattern_id, false);
        }
        assert!(engine.patterns()[0].confidence >= 0.0);
    }

    #[test]
    fn test_is_risky_change_file() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-001"));
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-002"));
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-003"));

        let (risk, reasons) = engine.is_risky_change(&["src/auth.rs"], &[]);
        assert!(risk > 0.0);
        assert!(!reasons.is_empty());

        // Safe file
        let (risk, reasons) = engine.is_risky_change(&["src/new_file.rs"], &[]);
        assert!((risk - 0.0).abs() < f64::EPSILON);
        assert!(reasons.is_empty());
    }

    #[test]
    fn test_is_risky_change_function() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping_with_functions(
            vec!["src/auth.rs"],
            vec!["do_login"],
            "INC-001",
        ));
        engine.record_mapping(make_mapping_with_functions(
            vec!["src/auth.rs"],
            vec!["do_login"],
            "INC-002",
        ));

        let (risk, reasons) = engine.is_risky_change(&[], &["do_login"]);
        assert!(risk > 0.0);
        assert!(!reasons.is_empty());
    }

    #[test]
    fn test_is_risky_change_with_patterns() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-001"));
        engine.record_mapping(make_mapping(vec!["src/auth.rs"], "INC-002"));
        engine.extract_patterns();

        let (risk, reasons) = engine.is_risky_change(&["src/auth.rs"], &[]);
        assert!(risk > 0.0);
        assert!(!reasons.is_empty());
    }

    #[test]
    fn test_function_hotspots() {
        let mut engine = LearningEngine::new();
        engine.record_mapping(make_mapping_with_functions(
            vec![],
            vec!["func_a", "func_b"],
            "1",
        ));
        engine.record_mapping(make_mapping_with_functions(vec![], vec!["func_a"], "2"));
        engine.record_mapping(make_mapping_with_functions(vec![], vec!["func_a"], "3"));

        let hotspots = engine.function_hotspots(2);
        assert_eq!(hotspots.len(), 2);
        assert_eq!(hotspots[0].0, "func_a");
        assert_eq!(hotspots[0].1, 3);
    }
}
