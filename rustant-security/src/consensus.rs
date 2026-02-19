//! Multi-model consensus engine for security finding validation.
//!
//! Composes the existing PlanningCouncil from rustant-core, adding
//! structured Finding output, per-finding model agreement tracking,
//! and cost-aware provider selection.

use crate::finding::{ConsensusProvenance, Finding, FindingSeverity};
use serde::{Deserialize, Serialize};

/// Configuration for a consensus validation session.
#[derive(Debug, Clone)]
pub struct ConsensusRequest {
    /// Code snippet to analyze.
    pub code: String,
    /// Context about the code (language, file path, function name).
    pub context: AnalysisContext,
    /// Hint about expected severity (for cost-aware provider selection).
    pub severity_hint: Option<FindingSeverity>,
}

/// Context provided for consensus analysis.
#[derive(Debug, Clone)]
pub struct AnalysisContext {
    /// Programming language.
    pub language: String,
    /// File path.
    pub file_path: String,
    /// Function or method name.
    pub function_name: Option<String>,
    /// Surrounding code context.
    pub surrounding_context: Option<String>,
}

/// Result of a consensus validation.
#[derive(Debug, Clone)]
pub struct ConsensusResult {
    /// Validated findings from the consensus process.
    pub findings: Vec<Finding>,
    /// Raw model responses before voting.
    pub model_responses: Vec<ModelResponse>,
    /// Overall agreement ratio across all findings.
    pub overall_agreement: f32,
    /// Total cost of the consensus call.
    pub total_cost: f64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// A single model's response in the consensus process.
#[derive(Debug, Clone)]
pub struct ModelResponse {
    /// Provider/model identifier.
    pub model: String,
    /// Findings identified by this model.
    pub findings: Vec<Finding>,
    /// Confidence score from this model.
    pub confidence: f32,
    /// Cost of this model's call.
    pub cost: f64,
    /// Latency in milliseconds.
    pub latency_ms: u64,
}

/// Weighted vote result for a single finding.
#[derive(Debug, Clone)]
pub struct VoteResult {
    /// Whether the finding was accepted by consensus.
    pub accepted: bool,
    /// Final confidence after voting.
    pub confidence: f32,
    /// Provenance recording which models agreed/disagreed.
    pub provenance: ConsensusProvenance,
}

/// The security consensus engine.
///
/// In a full implementation, this wraps PlanningCouncil from rustant-core.
/// For now, it provides the interface and basic voting logic.
pub struct SecurityConsensus {
    /// Minimum agreement ratio for a finding to be accepted.
    min_agreement: f32,
    /// Whether to include minority findings (with lower confidence).
    include_minority: bool,
    /// Timeout for consensus calls (seconds).
    timeout_secs: u64,
}

impl SecurityConsensus {
    pub fn new(min_agreement: f32) -> Self {
        Self {
            min_agreement,
            include_minority: false,
            timeout_secs: 30,
        }
    }

    /// Create from approval mode (maps mode to agreement threshold).
    pub fn for_approval_mode(mode: &str) -> Self {
        let min_agreement = match mode {
            "paranoid" => 1.0,  // Unanimous
            "cautious" => 0.67, // 2-of-3
            "safe" => 0.5,      // Majority with confidence weighting
            "yolo" => 0.33,     // Any model
            _ => 0.67,
        };
        Self::new(min_agreement)
    }

    pub fn with_include_minority(mut self, include: bool) -> Self {
        self.include_minority = include;
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Perform weighted voting on model responses to determine finding consensus.
    pub fn weighted_vote(&self, responses: &[ModelResponse]) -> Vec<VoteResult> {
        if responses.is_empty() {
            return Vec::new();
        }

        // Collect all unique finding content hashes across models
        let mut finding_votes: std::collections::HashMap<String, Vec<(String, f32)>> =
            std::collections::HashMap::new();

        for response in responses {
            for finding in &response.findings {
                finding_votes
                    .entry(finding.content_hash.clone())
                    .or_default()
                    .push((response.model.clone(), response.confidence));
            }
        }

        let total_models = responses.len() as f32;

        finding_votes
            .into_values()
            .map(|votes| {
                let models_agreed: Vec<String> = votes.iter().map(|(m, _)| m.clone()).collect();
                let models_disagreed: Vec<String> = responses
                    .iter()
                    .filter(|r| !models_agreed.contains(&r.model))
                    .map(|r| r.model.clone())
                    .collect();

                // Weighted agreement: sum of confidences / total models
                let weighted_agreement: f32 =
                    votes.iter().map(|(_, c)| c).sum::<f32>() / total_models;
                let simple_agreement = models_agreed.len() as f32 / total_models;

                let agreement = (weighted_agreement + simple_agreement) / 2.0;
                let accepted = agreement >= self.min_agreement;

                VoteResult {
                    accepted,
                    confidence: agreement,
                    provenance: ConsensusProvenance {
                        models_queried: responses.iter().map(|r| r.model.clone()).collect(),
                        models_agreed,
                        models_disagreed,
                        agreement_ratio: agreement,
                    },
                }
            })
            .collect()
    }

    /// Select providers based on severity hint (cost-aware).
    pub fn select_providers_for_severity(
        &self,
        severity: FindingSeverity,
        available: &[String],
    ) -> Vec<String> {
        match severity {
            // For critical/high: use all available providers
            FindingSeverity::Critical | FindingSeverity::High => available.to_vec(),
            // For medium: use up to 2 providers (prefer cheaper)
            FindingSeverity::Medium => available.iter().take(2).cloned().collect(),
            // For low/info: use 1 provider (cheapest)
            FindingSeverity::Low | FindingSeverity::Info => {
                available.iter().take(1).cloned().collect()
            }
        }
    }
}

impl Default for SecurityConsensus {
    fn default() -> Self {
        Self::new(0.67)
    }
}

/// Configuration for the consensus engine, loadable from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusEngineConfig {
    pub enabled: bool,
    pub threshold: String,
    pub providers: Vec<String>,
    pub timeout_secs: u64,
}

impl Default for ConsensusEngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: "2-of-3".into(),
            providers: vec!["openai".into(), "anthropic".into(), "gemini".into()],
            timeout_secs: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::*;

    fn make_finding(title: &str) -> Finding {
        Finding::new(
            title,
            "description",
            FindingSeverity::High,
            FindingCategory::Security,
            FindingProvenance::new("sast", 0.9),
        )
    }

    fn make_response(model: &str, findings: Vec<Finding>, confidence: f32) -> ModelResponse {
        ModelResponse {
            model: model.into(),
            findings,
            confidence,
            cost: 0.01,
            latency_ms: 100,
        }
    }

    #[test]
    fn test_unanimous_agreement() {
        let consensus = SecurityConsensus::new(0.67);
        let finding = make_finding("SQLi");

        let responses = vec![
            make_response("model_a", vec![finding.clone()], 0.95),
            make_response("model_b", vec![finding.clone()], 0.90),
            make_response("model_c", vec![finding.clone()], 0.85),
        ];

        let results = consensus.weighted_vote(&responses);
        assert_eq!(results.len(), 1);
        assert!(results[0].accepted);
        assert!(results[0].confidence > 0.8);
    }

    #[test]
    fn test_minority_finding_rejected() {
        let consensus = SecurityConsensus::new(0.67);
        let finding = make_finding("XSS");

        let responses = vec![
            make_response("model_a", vec![finding.clone()], 0.5),
            make_response("model_b", vec![], 0.0),
            make_response("model_c", vec![], 0.0),
        ];

        let results = consensus.weighted_vote(&responses);
        assert_eq!(results.len(), 1);
        assert!(!results[0].accepted);
    }

    #[test]
    fn test_approval_mode_thresholds() {
        let paranoid = SecurityConsensus::for_approval_mode("paranoid");
        assert_eq!(paranoid.min_agreement, 1.0);

        let yolo = SecurityConsensus::for_approval_mode("yolo");
        assert_eq!(yolo.min_agreement, 0.33);
    }

    #[test]
    fn test_provider_selection_by_severity() {
        let consensus = SecurityConsensus::default();
        let providers = vec!["openai".into(), "anthropic".into(), "gemini".into()];

        let critical =
            consensus.select_providers_for_severity(FindingSeverity::Critical, &providers);
        assert_eq!(critical.len(), 3);

        let low = consensus.select_providers_for_severity(FindingSeverity::Low, &providers);
        assert_eq!(low.len(), 1);
    }

    #[test]
    fn test_empty_responses() {
        let consensus = SecurityConsensus::default();
        let results = consensus.weighted_vote(&[]);
        assert!(results.is_empty());
    }
}
