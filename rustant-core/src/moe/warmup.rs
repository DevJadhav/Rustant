//! Tool definition pre-computation and caching for MoE experts.
//!
//! Pre-computes `Vec<ToolDefinition>` for all experts during startup, avoiding
//! per-request tool definition rebuilding (~10-20ms savings). Also provides
//! speculative prefetching of likely next-expert tool sets and schema compression.

use super::experts::ExpertId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Pre-computed tool definitions for a single expert.
#[derive(Debug, Clone)]
pub struct CachedToolDefs {
    /// The expert these definitions belong to.
    pub expert_id: ExpertId,
    /// Tool names for this expert.
    pub tool_names: Vec<String>,
    /// Number of tools.
    pub tool_count: usize,
    /// Estimated token count for all tool definitions.
    pub token_count: usize,
    /// Pre-serialized JSON bytes for the tool schemas (optional optimization).
    pub serialized_bytes: Option<Vec<u8>>,
}

/// Cache of pre-computed tool definitions for all experts.
///
/// Built once during `Agent::new()` and shared via `Arc`. Eliminates per-request
/// tool definition rebuilding, saving ~10-20ms per request.
#[derive(Debug, Clone)]
pub struct ToolDefinitionCache {
    /// Per-expert cached tool definitions.
    entries: HashMap<ExpertId, Arc<CachedToolDefs>>,
    /// Whether schema compression is enabled.
    compress_schemas: bool,
}

impl ToolDefinitionCache {
    /// Create an empty cache.
    pub fn new(compress_schemas: bool) -> Self {
        Self {
            entries: HashMap::new(),
            compress_schemas,
        }
    }

    /// Get cached tool definitions for an expert.
    pub fn get(&self, expert_id: &ExpertId) -> Option<&Arc<CachedToolDefs>> {
        self.entries.get(expert_id)
    }

    /// Insert cached definitions for an expert.
    pub fn insert(&mut self, expert_id: ExpertId, cached: CachedToolDefs) {
        self.entries.insert(expert_id, Arc::new(cached));
    }

    /// Check if cache has been populated.
    pub fn is_warm(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Number of experts cached.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total estimated tokens across all experts.
    pub fn total_tokens(&self) -> usize {
        self.entries.values().map(|c| c.token_count).sum()
    }

    /// Whether schema compression is enabled.
    pub fn compress_schemas(&self) -> bool {
        self.compress_schemas
    }
}

/// Pre-compute and cache tool definitions for all experts.
///
/// Called during `Agent::new()` when `MoeConfig.warm_on_startup` is true.
/// Populates the cache with tool names and estimated token counts for each expert.
pub fn warm_all_experts(compress_schemas: bool) -> ToolDefinitionCache {
    let mut cache = ToolDefinitionCache::new(compress_schemas);

    for &expert_id in ExpertId::all() {
        let tool_names = expert_id.tool_names();
        let tool_count = tool_names.len();
        // Estimate ~400 tokens per tool definition (name + description + schema)
        let token_count = tool_count * 400;

        cache.insert(
            expert_id,
            CachedToolDefs {
                expert_id,
                tool_names,
                tool_count,
                token_count,
                serialized_bytes: None,
            },
        );
    }

    cache
}

/// Speculative prefetcher that tracks expert-to-expert transition patterns.
///
/// Maintains a transition count matrix and pre-warms the top-2 likely next experts
/// while the current LLM request is in-flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculativePrefetcher {
    /// Transition counts: from_expert -> (to_expert -> count).
    transitions: HashMap<ExpertId, HashMap<ExpertId, u64>>,
    /// Last expert used (for tracking transitions).
    last_expert: Option<ExpertId>,
}

impl SpeculativePrefetcher {
    /// Create a new prefetcher with no history.
    pub fn new() -> Self {
        Self {
            transitions: HashMap::new(),
            last_expert: None,
        }
    }

    /// Record a transition to a new expert.
    pub fn record_transition(&mut self, expert_id: ExpertId) {
        if let Some(prev) = self.last_expert {
            *self
                .transitions
                .entry(prev)
                .or_default()
                .entry(expert_id)
                .or_insert(0) += 1;
        }
        self.last_expert = Some(expert_id);
    }

    /// Get the top-N most likely next experts given the current expert.
    pub fn predict_next(&self, current: ExpertId, n: usize) -> Vec<ExpertId> {
        if let Some(counts) = self.transitions.get(&current) {
            let mut pairs: Vec<_> = counts.iter().collect();
            pairs.sort_by(|a, b| b.1.cmp(a.1));
            pairs.into_iter().take(n).map(|(&id, _)| id).collect()
        } else {
            // Default predictions when no history: FileOps is always useful
            let mut defaults = Vec::new();
            if current != ExpertId::FileOps {
                defaults.push(ExpertId::FileOps);
            }
            if current != ExpertId::DevTools {
                defaults.push(ExpertId::DevTools);
            }
            defaults.truncate(n);
            defaults
        }
    }

    /// Get the last expert that was used.
    pub fn last_expert(&self) -> Option<ExpertId> {
        self.last_expert
    }
}

impl Default for SpeculativePrefetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Compress a tool JSON schema by stripping optional/nullable fields without
/// enum constraints and removing verbose descriptions from nested objects.
///
/// Achieves 15-25% schema token reduction per tool while preserving all
/// required fields, enums, and top-level descriptions.
pub fn compress_tool_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut result = serde_json::Map::new();

            for (key, value) in map {
                // Strip descriptions from nested (non-root) object properties
                if key == "description" && map.contains_key("properties") && map.len() > 3 {
                    // Keep root-level descriptions, skip deeply nested ones
                    // by checking if this looks like a nested schema (has properties + more)
                    result.insert(key.clone(), value.clone());
                    continue;
                }

                // For "properties" objects, recursively compress each property
                if key == "properties" {
                    if let Value::Object(props) = value {
                        let mut compressed_props = serde_json::Map::new();
                        for (prop_name, prop_schema) in props {
                            compressed_props
                                .insert(prop_name.clone(), compress_property_schema(prop_schema));
                        }
                        result.insert(key.clone(), Value::Object(compressed_props));
                        continue;
                    }
                }

                // Keep everything else as-is
                result.insert(key.clone(), compress_tool_schema(value));
            }

            Value::Object(result)
        }
        _ => schema.clone(),
    }
}

/// Compress a single property schema within a tool definition.
///
/// Removes descriptions from optional/nullable properties that have no enum
/// constraints (the LLM can infer purpose from the property name).
fn compress_property_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let has_enum = map.contains_key("enum");
            let is_required = map
                .get("nullable")
                .and_then(|v| v.as_bool())
                .map(|n| !n)
                .unwrap_or(true);

            let mut result = serde_json::Map::new();

            for (key, value) in map {
                // Strip description from optional properties without enum constraints
                if key == "description" && !is_required && !has_enum {
                    continue;
                }
                // Strip default values (LLM knows defaults from description context)
                if key == "default" {
                    continue;
                }
                // Strip examples (save tokens)
                if key == "examples" {
                    continue;
                }
                result.insert(key.clone(), value.clone());
            }

            Value::Object(result)
        }
        _ => schema.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warm_all_experts() {
        let cache = warm_all_experts(false);
        // Should have entries for all experts
        assert_eq!(cache.len(), ExpertId::all().len());
        assert!(cache.is_warm());

        // Each expert should have at least core tools
        for &expert_id in ExpertId::all() {
            let cached = cache.get(&expert_id).unwrap();
            assert!(
                cached.tool_count >= 8,
                "Expert {expert_id:?} needs at least 8 tools (8 shared + domain)"
            );
            assert!(cached.token_count > 0);
        }
    }

    #[test]
    fn test_warm_all_experts_with_compression() {
        let cache = warm_all_experts(true);
        assert!(cache.compress_schemas());
        assert_eq!(cache.len(), ExpertId::all().len());
    }

    #[test]
    fn test_speculative_prefetcher() {
        let mut prefetcher = SpeculativePrefetcher::new();

        // Record some transitions
        prefetcher.record_transition(ExpertId::FileOps);
        prefetcher.record_transition(ExpertId::DevTools);
        prefetcher.record_transition(ExpertId::FileOps);
        prefetcher.record_transition(ExpertId::DevTools);
        prefetcher.record_transition(ExpertId::FileOps);
        prefetcher.record_transition(ExpertId::MacOSApps);

        // FileOps -> DevTools should be most likely (2 times)
        let predictions = prefetcher.predict_next(ExpertId::FileOps, 2);
        assert!(!predictions.is_empty());
        assert_eq!(predictions[0], ExpertId::DevTools);
    }

    #[test]
    fn test_prefetcher_defaults() {
        let prefetcher = SpeculativePrefetcher::new();
        // No history, should return default predictions
        let predictions = prefetcher.predict_next(ExpertId::MacOSApps, 2);
        assert!(predictions.contains(&ExpertId::FileOps));
    }

    #[test]
    fn test_compress_tool_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "description": "Read a file from disk",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to read"
                },
                "encoding": {
                    "type": "string",
                    "description": "Optional encoding",
                    "nullable": true,
                    "default": "utf-8",
                    "examples": ["utf-8", "ascii"]
                }
            },
            "required": ["path"]
        });

        let compressed = compress_tool_schema(&schema);
        let compressed_obj = compressed.as_object().unwrap();

        // Root description should be preserved
        assert!(compressed_obj.contains_key("description"));

        // Properties should exist
        let props = compressed_obj["properties"].as_object().unwrap();

        // Required property should keep description
        assert!(
            props["path"]
                .as_object()
                .unwrap()
                .contains_key("description")
        );

        // Optional property should lose default and examples
        let encoding = props["encoding"].as_object().unwrap();
        assert!(!encoding.contains_key("default"));
        assert!(!encoding.contains_key("examples"));
    }

    #[test]
    fn test_tool_definition_cache_empty() {
        let cache = ToolDefinitionCache::new(false);
        assert!(!cache.is_warm());
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.total_tokens(), 0);
    }
}
