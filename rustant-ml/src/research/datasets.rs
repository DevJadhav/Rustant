//! Dataset discovery via Papers With Code API.

use serde::{Deserialize, Serialize};

/// A discovered dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDataset {
    pub name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub paper_count: usize,
    pub task: Option<String>,
    pub modality: Option<String>,
}

/// Dataset search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSearchResult {
    pub query: String,
    pub results: Vec<DiscoveredDataset>,
    pub total_count: usize,
}
