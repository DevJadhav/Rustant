//! Groundedness checking — verify claims against retrieved context.

use crate::rag::chunk::Chunk;
use serde::{Deserialize, Serialize};

/// Groundedness report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundednessReport {
    pub claims: Vec<ClaimVerification>,
    pub overall_score: f64,
    pub ungrounded_claims: Vec<String>,
    pub hallucination_risk: f64,
}

/// Verification result for a single claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimVerification {
    pub claim: String,
    pub supported: bool,
    pub supporting_evidence: Option<String>,
    pub confidence: f64,
}

/// Groundedness checker.
pub struct GroundednessChecker {
    #[allow(dead_code)]
    threshold: f64,
}

impl GroundednessChecker {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }

    /// Check groundedness of an answer against context chunks.
    pub fn check(&self, answer: &str, _context: &[Chunk]) -> GroundednessReport {
        // Simple heuristic: extract sentences as claims
        let claims: Vec<&str> = answer
            .split(". ")
            .filter(|s| !s.trim().is_empty())
            .collect();

        let verifications: Vec<ClaimVerification> = claims
            .iter()
            .map(|claim| {
                ClaimVerification {
                    claim: claim.to_string(),
                    supported: true, // Stub: real implementation would use NLI or LLM
                    supporting_evidence: None,
                    confidence: 0.8,
                }
            })
            .collect();

        let supported_count = verifications.iter().filter(|v| v.supported).count();
        let overall_score = if verifications.is_empty() {
            1.0
        } else {
            supported_count as f64 / verifications.len() as f64
        };
        let ungrounded: Vec<String> = verifications
            .iter()
            .filter(|v| !v.supported)
            .map(|v| v.claim.clone())
            .collect();
        let hallucination_risk = 1.0 - overall_score;

        GroundednessReport {
            claims: verifications,
            overall_score,
            ungrounded_claims: ungrounded,
            hallucination_risk,
        }
    }
}
