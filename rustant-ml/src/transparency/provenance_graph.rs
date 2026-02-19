//! Provenance graph for full data/model flow visualization.

use serde::{Deserialize, Serialize};

/// A provenance event in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEvent {
    pub id: String,
    pub event_type: String,
    pub entity_id: String,
    pub entity_type: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub details: std::collections::HashMap<String, String>,
}

/// Provenance graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvenanceGraph {
    pub events: Vec<ProvenanceEvent>,
}

impl ProvenanceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, event_type: &str, entity_id: &str, entity_type: &str) {
        self.events.push(ProvenanceEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type.to_string(),
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            timestamp: chrono::Utc::now(),
            details: std::collections::HashMap::new(),
        });
    }
}
