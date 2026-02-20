//! Research synthesis — merges sub-query results into coherent findings.
//!
//! Handles contradiction resolution, evidence weighting, and gap identification.

use super::contradiction::Contradiction;
use super::decomposition::SubQuery;
use super::sources::SourceTracker;
use serde::{Deserialize, Serialize};

/// Result of synthesizing research findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    /// The synthesized answer to the research question.
    pub answer: String,
    /// Confidence in the synthesis (0.0-1.0).
    pub confidence: f64,
    /// Key findings organized by theme.
    pub findings: Vec<Finding>,
    /// Identified contradictions.
    pub contradictions: Vec<Contradiction>,
    /// Gaps in the research that need additional queries.
    pub gaps: Vec<ResearchGap>,
    /// Number of sources used.
    pub sources_used: usize,
}

/// A key finding from the research.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// The finding text.
    pub text: String,
    /// Evidence strength (0.0-1.0).
    pub evidence_strength: f64,
    /// Number of sources supporting this finding.
    pub supporting_sources: usize,
    /// Tags/categories.
    pub tags: Vec<String>,
}

/// An identified gap in the research.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchGap {
    /// Description of what's missing.
    pub description: String,
    /// Suggested query to fill this gap.
    pub suggested_query: String,
    /// Priority of filling this gap (1 = highest).
    pub priority: u32,
}

/// Synthesizes research results from sub-queries and sources.
pub struct ResearchSynthesizer;

impl ResearchSynthesizer {
    /// Create a new synthesizer.
    pub fn new() -> Self {
        Self
    }

    /// Synthesize results from completed sub-queries.
    pub fn synthesize(
        &self,
        question: &str,
        sub_queries: &[SubQuery],
        tracker: &SourceTracker,
        contradictions: &[Contradiction],
    ) -> SynthesisResult {
        let completed: Vec<&SubQuery> = sub_queries.iter().filter(|q| q.completed).collect();

        // Collect all results
        let mut all_results = Vec::new();
        for query in &completed {
            if let Some(ref result) = query.result {
                all_results.push(result.as_str());
            }
        }

        // Build findings from results
        let mut findings = Vec::new();
        for result in &all_results {
            findings.push(Finding {
                text: result.to_string(),
                evidence_strength: 0.7,
                supporting_sources: 1,
                tags: vec![],
            });
        }

        // Identify gaps
        let gaps = self.identify_gaps(question, sub_queries, tracker);

        // Calculate confidence based on source reliability and completeness
        let avg_reliability = tracker.average_reliability();
        let completion_rate = if sub_queries.is_empty() {
            0.0
        } else {
            completed.len() as f64 / sub_queries.len() as f64
        };
        let contradiction_penalty = contradictions.len() as f64 * 0.05;
        let confidence =
            (avg_reliability * 0.4 + completion_rate * 0.6 - contradiction_penalty).clamp(0.0, 1.0);

        // Build answer
        let answer = if all_results.is_empty() {
            format!("Unable to find sufficient information to answer: {question}")
        } else {
            all_results.join("\n\n")
        };

        SynthesisResult {
            answer,
            confidence,
            findings,
            contradictions: contradictions.to_vec(),
            gaps,
            sources_used: tracker.source_count(),
        }
    }

    /// Identify gaps in the research coverage.
    fn identify_gaps(
        &self,
        question: &str,
        sub_queries: &[SubQuery],
        tracker: &SourceTracker,
    ) -> Vec<ResearchGap> {
        let mut gaps = Vec::new();

        // Check for incomplete sub-queries
        let incomplete: Vec<&SubQuery> = sub_queries.iter().filter(|q| !q.completed).collect();
        for query in &incomplete {
            gaps.push(ResearchGap {
                description: format!("Sub-query not completed: {}", query.question),
                suggested_query: query.question.clone(),
                priority: query.priority,
            });
        }

        // Check source diversity
        if tracker.source_count() < 3 {
            gaps.push(ResearchGap {
                description: "Limited source diversity (fewer than 3 sources)".to_string(),
                suggested_query: format!("Find additional perspectives on: {question}"),
                priority: 3,
            });
        }

        // Check for unverified claims
        let unverified = tracker.unverified_claims();
        if unverified.len() > 3 {
            gaps.push(ResearchGap {
                description: format!("{} claims remain unverified", unverified.len()),
                suggested_query: "Verify key claims with additional sources".to_string(),
                priority: 4,
            });
        }

        gaps
    }
}

impl Default for ResearchSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_synthesize_basic() {
        let synthesizer = ResearchSynthesizer::new();
        let tracker = SourceTracker::new();

        let queries = vec![SubQuery {
            id: Uuid::new_v4(),
            question: "What is X?".into(),
            depends_on: vec![],
            suggested_tools: vec![],
            output_type: "factual".into(),
            priority: 1,
            completed: true,
            result: Some("X is a method for Y.".into()),
        }];

        let result = synthesizer.synthesize("What is X?", &queries, &tracker, &[]);
        assert!(result.confidence > 0.0);
        assert!(!result.answer.is_empty());
        assert_eq!(result.findings.len(), 1);
    }

    #[test]
    fn test_gaps_identified() {
        let synthesizer = ResearchSynthesizer::new();
        let tracker = SourceTracker::new();

        let queries = vec![SubQuery {
            id: Uuid::new_v4(),
            question: "Q1".into(),
            depends_on: vec![],
            suggested_tools: vec![],
            output_type: "factual".into(),
            priority: 1,
            completed: false,
            result: None,
        }];

        let result = synthesizer.synthesize("Test?", &queries, &tracker, &[]);
        // Should identify incomplete sub-query and low source diversity
        assert!(!result.gaps.is_empty());
    }
}
