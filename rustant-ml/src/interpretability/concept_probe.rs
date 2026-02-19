//! Concept probing — test for learned representations.

use serde::{Deserialize, Serialize};

/// Concept probe result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptProbeResult {
    pub concept: String,
    pub layer: usize,
    pub accuracy: f64,
    pub is_encoded: bool,
}

/// Concept prober.
pub struct ConceptProber;

impl Default for ConceptProber {
    fn default() -> Self {
        Self::new()
    }
}

impl ConceptProber {
    pub fn new() -> Self {
        Self
    }

    pub fn probe(&self, concept: &str, _representations: &[Vec<f64>]) -> ConceptProbeResult {
        ConceptProbeResult {
            concept: concept.to_string(),
            layer: 0,
            accuracy: 0.0,
            is_encoded: false,
        }
    }
}
