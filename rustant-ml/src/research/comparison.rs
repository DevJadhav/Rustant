//! Multi-paper comparison.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Side-by-side paper comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperComparison {
    pub papers: Vec<PaperSummary>,
    pub dimensions: Vec<ComparisonDimension>,
    pub matrix: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperSummary {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub key_contribution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonDimension {
    pub name: String,
    pub description: String,
}

/// Type alias for backward compatibility.
pub type MultiPaperComparison = PaperComparison;
