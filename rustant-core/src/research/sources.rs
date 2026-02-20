//! Research source tracking and claim extraction.
//!
//! Manages `ResearchSource` entries with reliability scoring and
//! tracks individual `Claim` extractions for cross-reference.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A source of information discovered during research.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSource {
    /// Unique identifier.
    pub id: Uuid,
    /// Source title.
    pub title: String,
    /// Source URL or identifier.
    pub url: Option<String>,
    /// Type of source.
    pub source_type: SourceType,
    /// Reliability score (0.0-1.0).
    pub reliability: f64,
    /// When this source was discovered.
    pub discovered_at: DateTime<Utc>,
    /// The tool used to discover this source.
    pub discovered_via: String,
    /// Brief summary of the source content.
    pub summary: Option<String>,
}

/// Type of research source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// Academic paper (arXiv, Semantic Scholar, etc.).
    AcademicPaper,
    /// Documentation or technical reference.
    Documentation,
    /// Blog post or article.
    BlogPost,
    /// Forum or Q&A (Stack Overflow, etc.).
    Forum,
    /// Official API documentation.
    ApiDocs,
    /// News article.
    News,
    /// Book or textbook.
    Book,
    /// Other/unknown source type.
    Other,
}

/// A claim extracted from a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    /// Unique claim ID.
    pub id: Uuid,
    /// The claim text.
    pub text: String,
    /// Source this claim was extracted from.
    pub source_id: Uuid,
    /// Confidence in the claim's accuracy (0.0-1.0).
    pub confidence: f64,
    /// Categories/tags for the claim.
    pub tags: Vec<String>,
    /// Whether this claim has been verified by another source.
    pub verified: bool,
    /// IDs of claims that contradict this one.
    pub contradicted_by: Vec<Uuid>,
}

/// Tracks all sources and claims discovered during research.
pub struct SourceTracker {
    sources: Vec<ResearchSource>,
    claims: Vec<Claim>,
}

impl SourceTracker {
    /// Create a new source tracker.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            claims: Vec::new(),
        }
    }

    /// Add a research source.
    pub fn add_source(&mut self, source: ResearchSource) -> Uuid {
        let id = source.id;
        self.sources.push(source);
        id
    }

    /// Add a claim extracted from a source.
    pub fn add_claim(&mut self, claim: Claim) -> Uuid {
        let id = claim.id;
        self.claims.push(claim);
        id
    }

    /// Get a source by ID.
    pub fn get_source(&self, id: &Uuid) -> Option<&ResearchSource> {
        self.sources.iter().find(|s| s.id == *id)
    }

    /// Get all sources.
    pub fn sources(&self) -> &[ResearchSource] {
        &self.sources
    }

    /// Get all claims.
    pub fn claims(&self) -> &[Claim] {
        &self.claims
    }

    /// Get claims from a specific source.
    pub fn claims_from_source(&self, source_id: &Uuid) -> Vec<&Claim> {
        self.claims
            .iter()
            .filter(|c| c.source_id == *source_id)
            .collect()
    }

    /// Get unverified claims.
    pub fn unverified_claims(&self) -> Vec<&Claim> {
        self.claims.iter().filter(|c| !c.verified).collect()
    }

    /// Mark a claim as verified.
    pub fn verify_claim(&mut self, claim_id: &Uuid) {
        if let Some(claim) = self.claims.iter_mut().find(|c| c.id == *claim_id) {
            claim.verified = true;
        }
    }

    /// Record a contradiction between two claims.
    pub fn record_contradiction(&mut self, claim_a: &Uuid, claim_b: &Uuid) {
        if let Some(a) = self.claims.iter_mut().find(|c| c.id == *claim_a) {
            if !a.contradicted_by.contains(claim_b) {
                a.contradicted_by.push(*claim_b);
            }
        }
        if let Some(b) = self.claims.iter_mut().find(|c| c.id == *claim_b) {
            if !b.contradicted_by.contains(claim_a) {
                b.contradicted_by.push(*claim_a);
            }
        }
    }

    /// Average reliability score across all sources.
    pub fn average_reliability(&self) -> f64 {
        if self.sources.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.sources.iter().map(|s| s.reliability).sum();
        sum / self.sources.len() as f64
    }

    /// Number of sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Number of claims.
    pub fn claim_count(&self) -> usize {
        self.claims.len()
    }
}

impl Default for SourceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_source(title: &str, reliability: f64) -> ResearchSource {
        ResearchSource {
            id: Uuid::new_v4(),
            title: title.to_string(),
            url: Some("https://example.com".to_string()),
            source_type: SourceType::AcademicPaper,
            reliability,
            discovered_at: Utc::now(),
            discovered_via: "web_search".to_string(),
            summary: None,
        }
    }

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

    #[test]
    fn test_add_source_and_claim() {
        let mut tracker = SourceTracker::new();
        let source = make_source("Test Paper", 0.9);
        let source_id = tracker.add_source(source);

        let claim = make_claim(source_id, "This is a test claim");
        tracker.add_claim(claim);

        assert_eq!(tracker.source_count(), 1);
        assert_eq!(tracker.claim_count(), 1);
    }

    #[test]
    fn test_claims_from_source() {
        let mut tracker = SourceTracker::new();
        let source = make_source("Paper A", 0.9);
        let sid = tracker.add_source(source);

        tracker.add_claim(make_claim(sid, "Claim 1"));
        tracker.add_claim(make_claim(sid, "Claim 2"));
        tracker.add_claim(make_claim(Uuid::new_v4(), "Claim from other source"));

        assert_eq!(tracker.claims_from_source(&sid).len(), 2);
    }

    #[test]
    fn test_contradiction() {
        let mut tracker = SourceTracker::new();
        let sid = Uuid::new_v4();
        let c1 = make_claim(sid, "X is true");
        let c2 = make_claim(sid, "X is false");
        let c1_id = c1.id;
        let c2_id = c2.id;
        tracker.add_claim(c1);
        tracker.add_claim(c2);

        tracker.record_contradiction(&c1_id, &c2_id);

        let claim1 = tracker.claims.iter().find(|c| c.id == c1_id).unwrap();
        assert!(claim1.contradicted_by.contains(&c2_id));
    }

    #[test]
    fn test_average_reliability() {
        let mut tracker = SourceTracker::new();
        tracker.add_source(make_source("A", 0.8));
        tracker.add_source(make_source("B", 0.6));
        assert!((tracker.average_reliability() - 0.7).abs() < f64::EPSILON);
    }
}
