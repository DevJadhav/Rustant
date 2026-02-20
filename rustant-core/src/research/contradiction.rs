//! Contradiction detection across research claims.
//!
//! Finds conflicting claims from different sources using keyword overlap
//! analysis and negation detection.

use super::sources::{Claim, SourceTracker};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A detected contradiction between two claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// First claim.
    pub claim_a_id: Uuid,
    /// Second claim.
    pub claim_b_id: Uuid,
    /// Text of the first claim.
    pub claim_a_text: String,
    /// Text of the second claim.
    pub claim_b_text: String,
    /// Type of contradiction.
    pub contradiction_type: ContradictionType,
    /// Confidence that this is a real contradiction (0.0-1.0).
    pub confidence: f64,
    /// Resolution status.
    pub resolved: bool,
    /// Resolution text, if resolved.
    pub resolution: Option<String>,
}

/// Type of contradiction between claims.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionType {
    /// Direct negation (X is true vs X is false).
    DirectNegation,
    /// Conflicting statistics or numbers.
    NumericDisagreement,
    /// Different conclusions from similar premises.
    InterpretiveDifference,
    /// Temporal inconsistency (claims from different time periods).
    TemporalConflict,
}

/// Detects contradictions between claims.
pub struct ContradictionDetector {
    /// Minimum keyword overlap to consider claims as potentially contradictory.
    min_overlap: f64,
}

impl ContradictionDetector {
    /// Create a new detector with default settings.
    pub fn new() -> Self {
        Self { min_overlap: 0.3 }
    }

    /// Detect contradictions across all claims in the tracker.
    pub fn detect(&self, tracker: &SourceTracker) -> Vec<Contradiction> {
        let claims = tracker.claims();
        let mut contradictions = Vec::new();

        for i in 0..claims.len() {
            for j in (i + 1)..claims.len() {
                // Skip claims from the same source
                if claims[i].source_id == claims[j].source_id {
                    continue;
                }

                if let Some(contradiction) = self.check_pair(&claims[i], &claims[j]) {
                    contradictions.push(contradiction);
                }
            }
        }

        contradictions
    }

    /// Check if two claims contradict each other.
    fn check_pair(&self, a: &Claim, b: &Claim) -> Option<Contradiction> {
        let words_a = self.extract_keywords(&a.text);
        let words_b = self.extract_keywords(&b.text);

        // Check keyword overlap
        let overlap = self.keyword_overlap(&words_a, &words_b);
        if overlap < self.min_overlap {
            return None; // Not about the same topic
        }

        // Check for negation patterns
        let a_lower = a.text.to_lowercase();
        let b_lower = b.text.to_lowercase();

        let negation_words = [
            "not", "no", "never", "neither", "without", "lack", "doesn't", "don't", "isn't",
            "aren't", "wasn't", "weren't", "won't", "cannot",
        ];
        let a_has_negation = negation_words.iter().any(|w| a_lower.contains(w));
        let b_has_negation = negation_words.iter().any(|w| b_lower.contains(w));

        if a_has_negation != b_has_negation && overlap > 0.4 {
            return Some(Contradiction {
                claim_a_id: a.id,
                claim_b_id: b.id,
                claim_a_text: a.text.clone(),
                claim_b_text: b.text.clone(),
                contradiction_type: ContradictionType::DirectNegation,
                confidence: overlap * 0.8,
                resolved: false,
                resolution: None,
            });
        }

        // Check for numeric disagreement
        let nums_a = self.extract_numbers(&a.text);
        let nums_b = self.extract_numbers(&b.text);
        if !nums_a.is_empty() && !nums_b.is_empty() && overlap > 0.3 {
            // If they discuss the same topic but have different numbers
            let any_mismatch = nums_a.iter().any(|na| {
                nums_b.iter().any(|nb| {
                    (na - nb).abs() > f64::EPSILON && (na - nb).abs() / na.abs().max(1.0) > 0.1
                })
            });
            if any_mismatch {
                return Some(Contradiction {
                    claim_a_id: a.id,
                    claim_b_id: b.id,
                    claim_a_text: a.text.clone(),
                    claim_b_text: b.text.clone(),
                    contradiction_type: ContradictionType::NumericDisagreement,
                    confidence: overlap * 0.6,
                    resolved: false,
                    resolution: None,
                });
            }
        }

        None
    }

    /// Extract keywords from text (lowercase, stop-words removed).
    fn extract_keywords(&self, text: &str) -> Vec<String> {
        let stop_words = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has",
            "had", "do", "does", "did", "will", "would", "shall", "should", "may", "might", "must",
            "can", "could", "of", "in", "to", "for", "with", "on", "at", "from", "by", "about",
            "as", "into", "through", "during", "before", "after", "above", "below", "between",
            "this", "that", "these", "those", "it", "its", "and", "but", "or",
        ];

        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2 && !stop_words.contains(w))
            .map(String::from)
            .collect()
    }

    /// Extract numeric values from text.
    fn extract_numbers(&self, text: &str) -> Vec<f64> {
        text.split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter_map(|s| s.parse::<f64>().ok())
            .collect()
    }

    /// Compute keyword overlap between two keyword sets (Jaccard similarity).
    fn keyword_overlap(&self, a: &[String], b: &[String]) -> f64 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }
        let set_a: std::collections::HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
        let set_b: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();
        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }
}

impl Default for ContradictionDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::sources::{ResearchSource, SourceType};
    use chrono::Utc;

    fn make_claim(source_id: Uuid, text: &str) -> Claim {
        Claim {
            id: Uuid::new_v4(),
            text: text.to_string(),
            source_id,
            confidence: 0.8,
            tags: vec![],
            verified: false,
            contradicted_by: vec![],
        }
    }

    fn make_source() -> ResearchSource {
        ResearchSource {
            id: Uuid::new_v4(),
            title: "Test".into(),
            url: None,
            source_type: SourceType::AcademicPaper,
            reliability: 0.8,
            discovered_at: Utc::now(),
            discovered_via: "test".into(),
            summary: None,
        }
    }

    #[test]
    fn test_detect_negation() {
        let mut tracker = SourceTracker::new();
        let s1 = make_source();
        let s2 = make_source();
        let s1_id = tracker.add_source(s1);
        let s2_id = tracker.add_source(s2);

        tracker.add_claim(make_claim(
            s1_id,
            "Prompt caching significantly reduces latency in LLM applications",
        ));
        tracker.add_claim(make_claim(
            s2_id,
            "Prompt caching does not significantly reduce latency in LLM applications",
        ));

        let detector = ContradictionDetector::new();
        let contradictions = detector.detect(&tracker);
        assert!(!contradictions.is_empty());
        assert_eq!(
            contradictions[0].contradiction_type,
            ContradictionType::DirectNegation
        );
    }

    #[test]
    fn test_no_contradiction_same_source() {
        let mut tracker = SourceTracker::new();
        let s1 = make_source();
        let s1_id = tracker.add_source(s1);

        tracker.add_claim(make_claim(s1_id, "X is true"));
        tracker.add_claim(make_claim(s1_id, "X is not true"));

        let detector = ContradictionDetector::new();
        let contradictions = detector.detect(&tracker);
        // Same source — not flagged as contradiction
        assert!(contradictions.is_empty());
    }

    #[test]
    fn test_unrelated_claims() {
        let mut tracker = SourceTracker::new();
        let s1 = make_source();
        let s2 = make_source();
        let s1_id = tracker.add_source(s1);
        let s2_id = tracker.add_source(s2);

        tracker.add_claim(make_claim(s1_id, "The sky is blue"));
        tracker.add_claim(make_claim(s2_id, "Python is a programming language"));

        let detector = ContradictionDetector::new();
        let contradictions = detector.detect(&tracker);
        assert!(contradictions.is_empty());
    }
}
