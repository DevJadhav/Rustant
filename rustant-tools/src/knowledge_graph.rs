//! Knowledge graph tool — local graph of concepts, papers, methods, and relationships.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum NodeType {
    Paper,
    Concept,
    Method,
    Dataset,
    Person,
    Organization,
    Experiment,
    Methodology,
    Result,
    Hypothesis,
    Benchmark,
}

impl NodeType {
    fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "paper" => Some(Self::Paper),
            "concept" => Some(Self::Concept),
            "method" => Some(Self::Method),
            "dataset" => Some(Self::Dataset),
            "person" => Some(Self::Person),
            "organization" => Some(Self::Organization),
            "experiment" => Some(Self::Experiment),
            "methodology" => Some(Self::Methodology),
            "result" => Some(Self::Result),
            "hypothesis" => Some(Self::Hypothesis),
            "benchmark" => Some(Self::Benchmark),
            _ => None,
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Paper => "Paper",
            Self::Concept => "Concept",
            Self::Method => "Method",
            Self::Dataset => "Dataset",
            Self::Person => "Person",
            Self::Organization => "Organization",
            Self::Experiment => "Experiment",
            Self::Methodology => "Methodology",
            Self::Result => "Result",
            Self::Hypothesis => "Hypothesis",
            Self::Benchmark => "Benchmark",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum RelationshipType {
    Cites,
    Implements,
    Extends,
    Contradicts,
    BuildsOn,
    AuthoredBy,
    UsesDataset,
    RelatedTo,
    Reproduces,
    Refines,
    Validates,
    SupportsHypothesis,
    RefutesHypothesis,
}

impl RelationshipType {
    fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cites" => Some(Self::Cites),
            "implements" => Some(Self::Implements),
            "extends" => Some(Self::Extends),
            "contradicts" => Some(Self::Contradicts),
            "builds_on" => Some(Self::BuildsOn),
            "authored_by" => Some(Self::AuthoredBy),
            "uses_dataset" => Some(Self::UsesDataset),
            "related_to" => Some(Self::RelatedTo),
            "reproduces" => Some(Self::Reproduces),
            "refines" => Some(Self::Refines),
            "validates" => Some(Self::Validates),
            "supports_hypothesis" => Some(Self::SupportsHypothesis),
            "refutes_hypothesis" => Some(Self::RefutesHypothesis),
            _ => None,
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Cites => "Cites",
            Self::Implements => "Implements",
            Self::Extends => "Extends",
            Self::Contradicts => "Contradicts",
            Self::BuildsOn => "BuildsOn",
            Self::AuthoredBy => "AuthoredBy",
            Self::UsesDataset => "UsesDataset",
            Self::RelatedTo => "RelatedTo",
            Self::Reproduces => "Reproduces",
            Self::Refines => "Refines",
            Self::Validates => "Validates",
            Self::SupportsHypothesis => "SupportsHypothesis",
            Self::RefutesHypothesis => "RefutesHypothesis",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphNode {
    id: String,
    name: String,
    node_type: NodeType,
    description: String,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Edge {
    source_id: String,
    target_id: String,
    relationship_type: RelationshipType,
    strength: f64,
    notes: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct KnowledgeGraphState {
    nodes: Vec<GraphNode>,
    edges: Vec<Edge>,
    next_auto_id: usize,
}

/// Minimal struct for deserializing papers from the ArXiv library.
#[derive(Debug, Deserialize)]
struct ArxivLibraryFile {
    entries: Vec<ArxivLibraryEntry>,
}

#[derive(Debug, Deserialize)]
struct ArxivLibraryEntry {
    paper: ArxivPaperRef,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ArxivPaperRef {
    #[serde(alias = "arxiv_id")]
    id: String,
    title: String,
    authors: Vec<String>,
    #[serde(alias = "summary")]
    abstract_text: String,
}

pub struct KnowledgeGraphTool {
    workspace: PathBuf,
}

impl KnowledgeGraphTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("knowledge")
            .join("graph.json")
    }

    fn load_state(&self) -> KnowledgeGraphState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            KnowledgeGraphState::default()
        }
    }

    fn save_state(&self, state: &KnowledgeGraphState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "knowledge_graph".to_string(),
                message: format!("Failed to create state dir: {e}"),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "knowledge_graph".to_string(),
            message: format!("Failed to serialize state: {e}"),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "knowledge_graph".to_string(),
            message: format!("Failed to write state: {e}"),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "knowledge_graph".to_string(),
            message: format!("Failed to rename state file: {e}"),
        })?;
        Ok(())
    }

    fn slug(name: &str) -> String {
        name.to_lowercase().replace(' ', "-")
    }

    fn arxiv_library_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("arxiv")
            .join("library.json")
    }
}

#[async_trait]
impl Tool for KnowledgeGraphTool {
    fn name(&self) -> &str {
        "knowledge_graph"
    }

    fn description(&self) -> &str {
        "Local knowledge graph of concepts, papers, methods, and relationships. Actions: add_node, get_node, update_node, remove_node, add_edge, remove_edge, neighbors, search, list, path, stats, import_arxiv, export_dot."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "add_node", "get_node", "update_node", "remove_node",
                        "add_edge", "remove_edge", "neighbors", "search",
                        "list", "path", "stats", "import_arxiv", "export_dot"
                    ],
                    "description": "Action to perform"
                },
                "id": { "type": "string", "description": "Node ID" },
                "name": { "type": "string", "description": "Node name" },
                "node_type": {
                    "type": "string",
                    "enum": ["paper", "concept", "method", "dataset", "person", "organization", "experiment", "methodology", "result", "hypothesis", "benchmark"],
                    "description": "Type of node"
                },
                "description": { "type": "string", "description": "Node description" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for the node"
                },
                "metadata": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Key-value metadata for the node"
                },
                "source_id": { "type": "string", "description": "Source node ID for edge" },
                "target_id": { "type": "string", "description": "Target node ID for edge" },
                "relationship_type": {
                    "type": "string",
                    "enum": ["cites", "implements", "extends", "contradicts", "builds_on", "authored_by", "uses_dataset", "related_to", "reproduces", "refines", "validates", "supports_hypothesis", "refutes_hypothesis"],
                    "description": "Type of relationship"
                },
                "strength": {
                    "type": "number",
                    "description": "Edge strength 0.0-1.0 (default 0.5)"
                },
                "notes": { "type": "string", "description": "Edge notes" },
                "depth": {
                    "type": "integer",
                    "description": "Traversal depth for neighbors (1-3, default 1)"
                },
                "query": { "type": "string", "description": "Search query" },
                "tag": { "type": "string", "description": "Filter by tag" },
                "from_id": { "type": "string", "description": "Path start node ID" },
                "to_id": { "type": "string", "description": "Path end node ID" },
                "arxiv_id": { "type": "string", "description": "ArXiv paper ID for import" },
                "filter_type": {
                    "type": "string",
                    "enum": ["paper", "concept", "method", "dataset", "person", "organization", "experiment", "methodology", "result", "hypothesis", "benchmark"],
                    "description": "Filter by node type"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let mut state = self.load_state();

        match action {
            "add_node" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if name.is_empty() {
                    return Ok(ToolOutput::text("Missing required parameter 'name'."));
                }

                let node_type_str = args.get("node_type").and_then(|v| v.as_str()).unwrap_or("");
                let node_type = match NodeType::from_str_loose(node_type_str) {
                    Some(nt) => nt,
                    None => {
                        return Ok(ToolOutput::text(format!(
                            "Invalid node_type '{node_type_str}'. Use: paper, concept, method, dataset, person, organization, experiment, methodology, result, hypothesis, benchmark."
                        )));
                    }
                };

                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| Self::slug(name));

                // Check for duplicate id
                if state.nodes.iter().any(|n| n.id == id) {
                    return Ok(ToolOutput::text(format!(
                        "Node with id '{id}' already exists."
                    )));
                }

                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let tags: Vec<String> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let metadata: HashMap<String, String> = args
                    .get("metadata")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();

                let now = Utc::now();
                state.nodes.push(GraphNode {
                    id: id.clone(),
                    name: name.to_string(),
                    node_type,
                    description,
                    tags,
                    metadata,
                    created_at: now,
                    updated_at: now,
                });
                state.next_auto_id += 1;
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!("Added node '{name}' (id: {id}).")))
            }

            "get_node" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if id.is_empty() {
                    return Ok(ToolOutput::text("Missing required parameter 'id'."));
                }

                let node = match state.nodes.iter().find(|n| n.id == id) {
                    Some(n) => n,
                    None => {
                        return Ok(ToolOutput::text(format!("Node '{id}' not found.")));
                    }
                };

                let connected_edges: Vec<&Edge> = state
                    .edges
                    .iter()
                    .filter(|e| e.source_id == id || e.target_id == id)
                    .collect();

                let mut output = format!(
                    "Node: {} ({})\n  ID: {}\n  Type: {}\n  Description: {}\n  Tags: {}\n  Created: {}\n  Updated: {}",
                    node.name,
                    node.node_type.as_str(),
                    node.id,
                    node.node_type.as_str(),
                    if node.description.is_empty() {
                        "(none)"
                    } else {
                        &node.description
                    },
                    if node.tags.is_empty() {
                        "(none)".to_string()
                    } else {
                        node.tags.join(", ")
                    },
                    node.created_at.format("%Y-%m-%d %H:%M"),
                    node.updated_at.format("%Y-%m-%d %H:%M"),
                );

                if !node.metadata.is_empty() {
                    output.push_str("\n  Metadata:");
                    for (k, v) in &node.metadata {
                        output.push_str(&format!("\n    {k}: {v}"));
                    }
                }

                if !connected_edges.is_empty() {
                    output.push_str(&format!("\n  Edges ({}):", connected_edges.len()));
                    for edge in &connected_edges {
                        let direction = if edge.source_id == id {
                            format!(
                                "--[{}]--> {}",
                                edge.relationship_type.as_str(),
                                edge.target_id
                            )
                        } else {
                            format!(
                                "<--[{}]-- {}",
                                edge.relationship_type.as_str(),
                                edge.source_id
                            )
                        };
                        output.push_str(&format!(
                            "\n    {} (strength: {:.2})",
                            direction, edge.strength
                        ));
                    }
                }

                Ok(ToolOutput::text(output))
            }

            "update_node" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if id.is_empty() {
                    return Ok(ToolOutput::text("Missing required parameter 'id'."));
                }

                let node = match state.nodes.iter_mut().find(|n| n.id == id) {
                    Some(n) => n,
                    None => {
                        return Ok(ToolOutput::text(format!("Node '{id}' not found.")));
                    }
                };

                if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                    node.name = name.to_string();
                }
                if let Some(desc) = args.get("description").and_then(|v| v.as_str()) {
                    node.description = desc.to_string();
                }
                if let Some(tags) = args.get("tags").and_then(|v| v.as_array()) {
                    node.tags = tags
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                }
                node.updated_at = Utc::now();
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!("Updated node '{id}'.")))
            }

            "remove_node" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if id.is_empty() {
                    return Ok(ToolOutput::text("Missing required parameter 'id'."));
                }

                let before_nodes = state.nodes.len();
                state.nodes.retain(|n| n.id != id);
                if state.nodes.len() == before_nodes {
                    return Ok(ToolOutput::text(format!("Node '{id}' not found.")));
                }

                // Cascade-delete edges referencing this node
                let before_edges = state.edges.len();
                state
                    .edges
                    .retain(|e| e.source_id != id && e.target_id != id);
                let edges_removed = before_edges - state.edges.len();

                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Removed node '{id}' and {edges_removed} connected edge(s)."
                )))
            }

            "add_edge" => {
                let source_id = args.get("source_id").and_then(|v| v.as_str()).unwrap_or("");
                let target_id = args.get("target_id").and_then(|v| v.as_str()).unwrap_or("");
                let rel_str = args
                    .get("relationship_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if source_id.is_empty() || target_id.is_empty() {
                    return Ok(ToolOutput::text(
                        "Missing required parameters 'source_id' and 'target_id'.",
                    ));
                }

                // Validate nodes exist
                if !state.nodes.iter().any(|n| n.id == source_id) {
                    return Ok(ToolOutput::text(format!(
                        "Source node '{source_id}' not found."
                    )));
                }
                if !state.nodes.iter().any(|n| n.id == target_id) {
                    return Ok(ToolOutput::text(format!(
                        "Target node '{target_id}' not found."
                    )));
                }

                let relationship_type = match RelationshipType::from_str_loose(rel_str) {
                    Some(rt) => rt,
                    None => {
                        return Ok(ToolOutput::text(format!(
                            "Invalid relationship_type '{rel_str}'. Use: cites, implements, extends, contradicts, builds_on, authored_by, uses_dataset, related_to, reproduces, refines, validates, supports_hypothesis, refutes_hypothesis."
                        )));
                    }
                };

                let strength = args
                    .get("strength")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.5)
                    .clamp(0.0, 1.0);

                let notes = args
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                state.edges.push(Edge {
                    source_id: source_id.to_string(),
                    target_id: target_id.to_string(),
                    relationship_type,
                    strength,
                    notes,
                    created_at: Utc::now(),
                });

                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Added edge {source_id} --[{rel_str}]--> {target_id} (strength: {strength:.2})."
                )))
            }

            "remove_edge" => {
                let source_id = args.get("source_id").and_then(|v| v.as_str()).unwrap_or("");
                let target_id = args.get("target_id").and_then(|v| v.as_str()).unwrap_or("");

                if source_id.is_empty() || target_id.is_empty() {
                    return Ok(ToolOutput::text(
                        "Missing required parameters 'source_id' and 'target_id'.",
                    ));
                }

                let rel_filter = args
                    .get("relationship_type")
                    .and_then(|v| v.as_str())
                    .and_then(RelationshipType::from_str_loose);

                let before = state.edges.len();
                state.edges.retain(|e| {
                    if e.source_id == source_id && e.target_id == target_id {
                        if let Some(ref rt) = rel_filter {
                            return &e.relationship_type != rt;
                        }
                        return false;
                    }
                    true
                });
                let removed = before - state.edges.len();

                if removed == 0 {
                    return Ok(ToolOutput::text("No matching edge(s) found."));
                }

                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Removed {removed} edge(s) from '{source_id}' to '{target_id}'."
                )))
            }

            "neighbors" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if id.is_empty() {
                    return Ok(ToolOutput::text("Missing required parameter 'id'."));
                }

                if !state.nodes.iter().any(|n| n.id == id) {
                    return Ok(ToolOutput::text(format!("Node '{id}' not found.")));
                }

                let max_depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1)
                    .clamp(1, 3) as usize;

                let rel_filter = args
                    .get("relationship_type")
                    .and_then(|v| v.as_str())
                    .and_then(RelationshipType::from_str_loose);

                // BFS traversal
                let mut visited: HashSet<String> = HashSet::new();
                visited.insert(id.to_string());
                let mut queue: VecDeque<(String, usize)> = VecDeque::new();
                queue.push_back((id.to_string(), 0));
                let mut found_nodes: Vec<(String, usize)> = Vec::new();

                while let Some((current_id, depth)) = queue.pop_front() {
                    if depth >= max_depth {
                        continue;
                    }

                    for edge in &state.edges {
                        if let Some(ref rt) = rel_filter
                            && &edge.relationship_type != rt
                        {
                            continue;
                        }

                        let neighbor_id = if edge.source_id == current_id {
                            &edge.target_id
                        } else if edge.target_id == current_id {
                            &edge.source_id
                        } else {
                            continue;
                        };

                        if !visited.contains(neighbor_id) {
                            visited.insert(neighbor_id.clone());
                            found_nodes.push((neighbor_id.clone(), depth + 1));
                            queue.push_back((neighbor_id.clone(), depth + 1));
                        }
                    }
                }

                if found_nodes.is_empty() {
                    return Ok(ToolOutput::text(format!(
                        "No neighbors found for '{id}' within depth {max_depth}."
                    )));
                }

                let mut output = format!("Neighbors of '{id}' (depth {max_depth}):\n");
                for (nid, depth) in &found_nodes {
                    if let Some(node) = state.nodes.iter().find(|n| n.id == *nid) {
                        output.push_str(&format!(
                            "  [depth {}] {} — {} ({})\n",
                            depth,
                            node.id,
                            node.name,
                            node.node_type.as_str()
                        ));
                    }
                }

                Ok(ToolOutput::text(output.trim_end().to_string()))
            }

            "search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                if query.is_empty() {
                    return Ok(ToolOutput::text("Missing required parameter 'query'."));
                }

                let type_filter = args
                    .get("node_type")
                    .and_then(|v| v.as_str())
                    .and_then(NodeType::from_str_loose);

                let matches: Vec<&GraphNode> = state
                    .nodes
                    .iter()
                    .filter(|n| {
                        if let Some(ref nt) = type_filter
                            && &n.node_type != nt
                        {
                            return false;
                        }
                        n.name.to_lowercase().contains(&query)
                            || n.description.to_lowercase().contains(&query)
                            || n.tags.iter().any(|t| t.to_lowercase().contains(&query))
                    })
                    .collect();

                if matches.is_empty() {
                    return Ok(ToolOutput::text(format!("No nodes matching '{query}'.")));
                }

                let mut output = format!("Found {} node(s):\n", matches.len());
                for node in &matches {
                    output.push_str(&format!(
                        "  {} — {} ({})\n",
                        node.id,
                        node.name,
                        node.node_type.as_str()
                    ));
                }

                Ok(ToolOutput::text(output.trim_end().to_string()))
            }

            "list" => {
                let type_filter = args
                    .get("node_type")
                    .and_then(|v| v.as_str())
                    .and_then(NodeType::from_str_loose);

                let tag_filter = args.get("tag").and_then(|v| v.as_str());

                let filtered: Vec<&GraphNode> = state
                    .nodes
                    .iter()
                    .filter(|n| {
                        if let Some(ref nt) = type_filter
                            && &n.node_type != nt
                        {
                            return false;
                        }
                        if let Some(tag) = tag_filter
                            && !n.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
                        {
                            return false;
                        }
                        true
                    })
                    .collect();

                if filtered.is_empty() {
                    return Ok(ToolOutput::text("No nodes found."));
                }

                let mut output = format!("Nodes ({}):\n", filtered.len());
                for node in &filtered {
                    output.push_str(&format!(
                        "  {} — {} ({})\n",
                        node.id,
                        node.name,
                        node.node_type.as_str()
                    ));
                }

                Ok(ToolOutput::text(output.trim_end().to_string()))
            }

            "path" => {
                let from_id = args.get("from_id").and_then(|v| v.as_str()).unwrap_or("");
                let to_id = args.get("to_id").and_then(|v| v.as_str()).unwrap_or("");

                if from_id.is_empty() || to_id.is_empty() {
                    return Ok(ToolOutput::text(
                        "Missing required parameters 'from_id' and 'to_id'.",
                    ));
                }

                if !state.nodes.iter().any(|n| n.id == from_id) {
                    return Ok(ToolOutput::text(format!("Node '{from_id}' not found.")));
                }
                if !state.nodes.iter().any(|n| n.id == to_id) {
                    return Ok(ToolOutput::text(format!("Node '{to_id}' not found.")));
                }

                // BFS shortest path (bidirectional edges)
                let mut visited: HashSet<String> = HashSet::new();
                let mut parent: HashMap<String, String> = HashMap::new();
                let mut queue: VecDeque<String> = VecDeque::new();

                visited.insert(from_id.to_string());
                queue.push_back(from_id.to_string());

                let mut found = false;
                while let Some(current) = queue.pop_front() {
                    if current == to_id {
                        found = true;
                        break;
                    }

                    for edge in &state.edges {
                        let neighbor = if edge.source_id == current {
                            &edge.target_id
                        } else if edge.target_id == current {
                            &edge.source_id
                        } else {
                            continue;
                        };

                        if !visited.contains(neighbor) {
                            visited.insert(neighbor.clone());
                            parent.insert(neighbor.clone(), current.clone());
                            queue.push_back(neighbor.clone());
                        }
                    }
                }

                if !found {
                    return Ok(ToolOutput::text(format!(
                        "No path found from '{from_id}' to '{to_id}'."
                    )));
                }

                // Reconstruct path
                let mut path_ids: Vec<String> = Vec::new();
                let mut current = to_id.to_string();
                while current != from_id {
                    path_ids.push(current.clone());
                    current = parent.get(&current).unwrap().clone();
                }
                path_ids.push(from_id.to_string());
                path_ids.reverse();

                let mut output = format!(
                    "Path from '{}' to '{}' ({} hops):\n",
                    from_id,
                    to_id,
                    path_ids.len() - 1
                );
                for (i, pid) in path_ids.iter().enumerate() {
                    let node_name = state
                        .nodes
                        .iter()
                        .find(|n| n.id == *pid)
                        .map(|n| n.name.as_str())
                        .unwrap_or("?");
                    if i > 0 {
                        output.push_str("  -> ");
                    } else {
                        output.push_str("  ");
                    }
                    output.push_str(&format!("{pid} ({node_name})\n"));
                }

                Ok(ToolOutput::text(output.trim_end().to_string()))
            }

            "stats" => {
                let total_nodes = state.nodes.len();
                let total_edges = state.edges.len();

                if total_nodes == 0 {
                    return Ok(ToolOutput::text(
                        "Knowledge graph is empty. Use add_node to get started.",
                    ));
                }

                // Node counts by type
                let mut type_counts: HashMap<&str, usize> = HashMap::new();
                for node in &state.nodes {
                    *type_counts.entry(node.node_type.as_str()).or_insert(0) += 1;
                }

                // Edge counts by type
                let mut edge_type_counts: HashMap<&str, usize> = HashMap::new();
                for edge in &state.edges {
                    *edge_type_counts
                        .entry(edge.relationship_type.as_str())
                        .or_insert(0) += 1;
                }

                // Top-5 most connected nodes
                let mut connection_counts: HashMap<&str, usize> = HashMap::new();
                for edge in &state.edges {
                    *connection_counts.entry(&edge.source_id).or_insert(0) += 1;
                    *connection_counts.entry(&edge.target_id).or_insert(0) += 1;
                }
                let mut top_connected: Vec<(&&str, &usize)> = connection_counts.iter().collect();
                top_connected.sort_by(|a, b| b.1.cmp(a.1));
                top_connected.truncate(5);

                let mut output = format!(
                    "Knowledge Graph Stats:\n  Nodes: {total_nodes}\n  Edges: {total_edges}\n\n  Nodes by type:\n"
                );
                let mut sorted_types: Vec<_> = type_counts.iter().collect();
                sorted_types.sort_by_key(|(k, _)| *k);
                for (t, c) in &sorted_types {
                    output.push_str(&format!("    {t}: {c}\n"));
                }

                if !edge_type_counts.is_empty() {
                    output.push_str("\n  Edges by type:\n");
                    let mut sorted_etypes: Vec<_> = edge_type_counts.iter().collect();
                    sorted_etypes.sort_by_key(|(k, _)| *k);
                    for (t, c) in &sorted_etypes {
                        output.push_str(&format!("    {t}: {c}\n"));
                    }
                }

                if !top_connected.is_empty() {
                    output.push_str("\n  Most connected nodes:\n");
                    for (nid, count) in &top_connected {
                        let node_name = state
                            .nodes
                            .iter()
                            .find(|n| n.id == ***nid)
                            .map(|n| n.name.as_str())
                            .unwrap_or("?");
                        output
                            .push_str(&format!("    {nid} ({node_name}) — {count} connections\n"));
                    }
                }

                Ok(ToolOutput::text(output.trim_end().to_string()))
            }

            "import_arxiv" => {
                let arxiv_id = args.get("arxiv_id").and_then(|v| v.as_str()).unwrap_or("");
                if arxiv_id.is_empty() {
                    return Ok(ToolOutput::text("Missing required parameter 'arxiv_id'."));
                }

                let lib_path = self.arxiv_library_path();
                if !lib_path.exists() {
                    return Ok(ToolOutput::text(
                        "ArXiv library not found. Save papers with the arxiv_research tool first.",
                    ));
                }

                let lib_json =
                    std::fs::read_to_string(&lib_path).map_err(|e| ToolError::ExecutionFailed {
                        name: "knowledge_graph".to_string(),
                        message: format!("Failed to read arxiv library: {e}"),
                    })?;

                let library: ArxivLibraryFile =
                    serde_json::from_str(&lib_json).map_err(|e| ToolError::ExecutionFailed {
                        name: "knowledge_graph".to_string(),
                        message: format!("Failed to parse arxiv library: {e}"),
                    })?;

                let entry = match library.entries.iter().find(|e| e.paper.id == arxiv_id) {
                    Some(e) => e,
                    None => {
                        return Ok(ToolOutput::text(format!(
                            "Paper '{arxiv_id}' not found in arxiv library."
                        )));
                    }
                };

                let now = Utc::now();
                let paper_id = Self::slug(&entry.paper.title);
                let mut added_nodes = 0;
                let mut added_edges = 0;

                // Create Paper node if not exists
                if !state.nodes.iter().any(|n| n.id == paper_id) {
                    let mut metadata = HashMap::new();
                    metadata.insert("arxiv_id".to_string(), entry.paper.id.clone());
                    state.nodes.push(GraphNode {
                        id: paper_id.clone(),
                        name: entry.paper.title.clone(),
                        node_type: NodeType::Paper,
                        description: entry.paper.abstract_text.clone(),
                        tags: entry.tags.clone(),
                        metadata,
                        created_at: now,
                        updated_at: now,
                    });
                    added_nodes += 1;
                }

                // Create Person nodes for each author + AuthoredBy edges
                for author in &entry.paper.authors {
                    let author_id = Self::slug(author);
                    if !state.nodes.iter().any(|n| n.id == author_id) {
                        state.nodes.push(GraphNode {
                            id: author_id.clone(),
                            name: author.clone(),
                            node_type: NodeType::Person,
                            description: String::new(),
                            tags: Vec::new(),
                            metadata: HashMap::new(),
                            created_at: now,
                            updated_at: now,
                        });
                        added_nodes += 1;
                    }

                    // Add AuthoredBy edge (paper -> author)
                    let edge_exists = state.edges.iter().any(|e| {
                        e.source_id == paper_id
                            && e.target_id == author_id
                            && e.relationship_type == RelationshipType::AuthoredBy
                    });
                    if !edge_exists {
                        state.edges.push(Edge {
                            source_id: paper_id.clone(),
                            target_id: author_id,
                            relationship_type: RelationshipType::AuthoredBy,
                            strength: 1.0,
                            notes: String::new(),
                            created_at: now,
                        });
                        added_edges += 1;
                    }
                }

                state.next_auto_id += added_nodes;
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Imported '{}' from arxiv: {} node(s) and {} edge(s) added.",
                    entry.paper.title, added_nodes, added_edges
                )))
            }

            "export_dot" => {
                let type_filter = args
                    .get("filter_type")
                    .and_then(|v| v.as_str())
                    .and_then(NodeType::from_str_loose);

                let filtered_nodes: Vec<&GraphNode> = state
                    .nodes
                    .iter()
                    .filter(|n| {
                        type_filter
                            .as_ref()
                            .map(|nt| &n.node_type == nt)
                            .unwrap_or(true)
                    })
                    .collect();

                let node_ids: HashSet<&str> =
                    filtered_nodes.iter().map(|n| n.id.as_str()).collect();

                let mut dot = String::from("digraph KnowledgeGraph {\n");
                dot.push_str("  rankdir=LR;\n");
                dot.push_str("  node [shape=box];\n\n");

                for node in &filtered_nodes {
                    dot.push_str(&format!(
                        "  \"{}\" [label=\"{}\\n({})\"];\n",
                        node.id,
                        node.name.replace('"', "\\\""),
                        node.node_type.as_str()
                    ));
                }

                dot.push('\n');

                for edge in &state.edges {
                    if node_ids.contains(edge.source_id.as_str())
                        && node_ids.contains(edge.target_id.as_str())
                    {
                        dot.push_str(&format!(
                            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
                            edge.source_id,
                            edge.target_id,
                            edge.relationship_type.as_str()
                        ));
                    }
                }

                dot.push_str("}\n");

                Ok(ToolOutput::text(dot))
            }

            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{action}'. Use: add_node, get_node, update_node, remove_node, add_edge, remove_edge, neighbors, search, list, path, stats, import_arxiv, export_dot."
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (TempDir, KnowledgeGraphTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = KnowledgeGraphTool::new(workspace);
        (dir, tool)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "knowledge_graph");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert_eq!(tool.timeout(), Duration::from_secs(30));
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, tool) = make_tool();
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("action")));
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert_eq!(actions.len(), 13);
    }

    #[tokio::test]
    async fn test_add_node_basic() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "add_node",
                "name": "Attention Is All You Need",
                "node_type": "paper",
                "description": "Transformer architecture paper"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Added node"));
        assert!(result.content.contains("attention-is-all-you-need"));
    }

    #[tokio::test]
    async fn test_add_node_auto_id_from_slug() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "add_node",
                "name": "Deep Learning Basics",
                "node_type": "concept"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("deep-learning-basics"));

        // Verify the id was generated correctly
        let state = tool.load_state();
        assert_eq!(state.nodes[0].id, "deep-learning-basics");
        assert_eq!(state.nodes[0].name, "Deep Learning Basics");
    }

    #[tokio::test]
    async fn test_get_node_with_edges() {
        let (_dir, tool) = make_tool();
        // Add two nodes and an edge
        tool.execute(json!({
            "action": "add_node",
            "name": "Paper A",
            "node_type": "paper",
            "id": "paper-a"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "add_node",
            "name": "Concept B",
            "node_type": "concept",
            "id": "concept-b"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "add_edge",
            "source_id": "paper-a",
            "target_id": "concept-b",
            "relationship_type": "implements"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({ "action": "get_node", "id": "paper-a" }))
            .await
            .unwrap();
        assert!(result.content.contains("Paper A"));
        assert!(result.content.contains("Implements"));
        assert!(result.content.contains("concept-b"));
    }

    #[tokio::test]
    async fn test_update_node() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_node",
            "name": "Old Name",
            "node_type": "concept",
            "id": "test-node"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "update_node",
                "id": "test-node",
                "name": "New Name",
                "description": "Updated description",
                "tags": ["updated"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Updated node"));

        let state = tool.load_state();
        let node = state.nodes.iter().find(|n| n.id == "test-node").unwrap();
        assert_eq!(node.name, "New Name");
        assert_eq!(node.description, "Updated description");
        assert_eq!(node.tags, vec!["updated"]);
    }

    #[tokio::test]
    async fn test_remove_node_cascades_edges() {
        let (_dir, tool) = make_tool();
        // Add three nodes and edges
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "C", "node_type": "concept", "id": "c" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "a", "target_id": "b", "relationship_type": "related_to" }))
            .await.unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "b", "target_id": "c", "relationship_type": "related_to" }))
            .await.unwrap();

        let state = tool.load_state();
        assert_eq!(state.edges.len(), 2);

        // Remove node B — should cascade-delete both edges
        let result = tool
            .execute(json!({ "action": "remove_node", "id": "b" }))
            .await
            .unwrap();
        assert!(result.content.contains("Removed node 'b'"));
        assert!(result.content.contains("2 connected edge(s)"));

        let state = tool.load_state();
        assert_eq!(state.nodes.len(), 2);
        assert_eq!(state.edges.len(), 0);
    }

    #[tokio::test]
    async fn test_add_edge_validates_nodes() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();

        // Target node doesn't exist
        let result = tool
            .execute(json!({
                "action": "add_edge",
                "source_id": "a",
                "target_id": "nonexistent",
                "relationship_type": "related_to"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("not found"));

        // Source node doesn't exist
        let result = tool
            .execute(json!({
                "action": "add_edge",
                "source_id": "nonexistent",
                "target_id": "a",
                "relationship_type": "related_to"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_add_edge_strength_default() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_edge",
            "source_id": "a",
            "target_id": "b",
            "relationship_type": "related_to"
        }))
        .await
        .unwrap();

        let state = tool.load_state();
        assert_eq!(state.edges.len(), 1);
        assert!((state.edges[0].strength - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_remove_edge() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();
        tool.execute(json!({
            "action": "add_edge",
            "source_id": "a",
            "target_id": "b",
            "relationship_type": "related_to"
        }))
        .await
        .unwrap();

        assert_eq!(tool.load_state().edges.len(), 1);

        let result = tool
            .execute(json!({
                "action": "remove_edge",
                "source_id": "a",
                "target_id": "b"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Removed"));
        assert_eq!(tool.load_state().edges.len(), 0);
    }

    #[tokio::test]
    async fn test_neighbors_depth_1() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "C", "node_type": "concept", "id": "c" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "a", "target_id": "b", "relationship_type": "related_to" }))
            .await.unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "b", "target_id": "c", "relationship_type": "related_to" }))
            .await.unwrap();

        let result = tool
            .execute(json!({ "action": "neighbors", "id": "a", "depth": 1 }))
            .await
            .unwrap();
        assert!(result.content.contains("b"));
        // At depth 1, should NOT find C
        assert!(!result.content.contains(" c ") && !result.content.contains("\n  [depth 1] c"));
    }

    #[tokio::test]
    async fn test_neighbors_depth_2() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "C", "node_type": "concept", "id": "c" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "a", "target_id": "b", "relationship_type": "related_to" }))
            .await.unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "b", "target_id": "c", "relationship_type": "related_to" }))
            .await.unwrap();

        let result = tool
            .execute(json!({ "action": "neighbors", "id": "a", "depth": 2 }))
            .await
            .unwrap();
        // At depth 2, should find both B and C
        assert!(result.content.contains("b"));
        assert!(result.content.contains("c"));
    }

    #[tokio::test]
    async fn test_search_by_name() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_node",
            "name": "Transformer Architecture",
            "node_type": "method",
            "id": "transformer"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({ "action": "search", "query": "transformer" }))
            .await
            .unwrap();
        assert!(result.content.contains("Transformer Architecture"));
    }

    #[tokio::test]
    async fn test_search_by_tag() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_node",
            "name": "BERT",
            "node_type": "method",
            "id": "bert",
            "tags": ["nlp", "language-model"]
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({ "action": "search", "query": "nlp" }))
            .await
            .unwrap();
        assert!(result.content.contains("BERT"));
    }

    #[tokio::test]
    async fn test_search_filter_type() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({ "action": "add_node", "name": "ML Paper", "node_type": "paper", "id": "ml-paper" }))
            .await.unwrap();
        tool.execute(json!({ "action": "add_node", "name": "ML Concept", "node_type": "concept", "id": "ml-concept" }))
            .await.unwrap();

        let result = tool
            .execute(json!({ "action": "search", "query": "ml", "node_type": "paper" }))
            .await
            .unwrap();
        assert!(result.content.contains("ML Paper"));
        assert!(!result.content.contains("ML Concept"));
    }

    #[tokio::test]
    async fn test_list_all() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_node", "name": "B", "node_type": "paper", "id": "b" }))
            .await
            .unwrap();

        let result = tool.execute(json!({ "action": "list" })).await.unwrap();
        assert!(result.content.contains("Nodes (2)"));
        assert!(result.content.contains("a"));
        assert!(result.content.contains("b"));
    }

    #[tokio::test]
    async fn test_list_filter_type() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_node", "name": "B", "node_type": "paper", "id": "b" }))
            .await
            .unwrap();

        let result = tool
            .execute(json!({ "action": "list", "node_type": "concept" }))
            .await
            .unwrap();
        assert!(result.content.contains("Nodes (1)"));
        assert!(result.content.contains("a"));
        assert!(!result.content.contains(" b "));
    }

    #[tokio::test]
    async fn test_path_direct() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "a", "target_id": "b", "relationship_type": "related_to" }))
            .await.unwrap();

        let result = tool
            .execute(json!({ "action": "path", "from_id": "a", "to_id": "b" }))
            .await
            .unwrap();
        assert!(result.content.contains("1 hops"));
        assert!(result.content.contains("a"));
        assert!(result.content.contains("b"));
    }

    #[tokio::test]
    async fn test_path_indirect() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "C", "node_type": "concept", "id": "c" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "a", "target_id": "b", "relationship_type": "related_to" }))
            .await.unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "b", "target_id": "c", "relationship_type": "related_to" }))
            .await.unwrap();

        let result = tool
            .execute(json!({ "action": "path", "from_id": "a", "to_id": "c" }))
            .await
            .unwrap();
        assert!(result.content.contains("2 hops"));
        assert!(result.content.contains("a"));
        assert!(result.content.contains("b"));
        assert!(result.content.contains("c"));
    }

    #[tokio::test]
    async fn test_path_no_path() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "concept", "id": "b" }),
        )
        .await
        .unwrap();
        // No edges between them

        let result = tool
            .execute(json!({ "action": "path", "from_id": "a", "to_id": "b" }))
            .await
            .unwrap();
        assert!(result.content.contains("No path found"));
    }

    #[tokio::test]
    async fn test_stats() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "Paper 1", "node_type": "paper", "id": "p1" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_node", "name": "Concept 1", "node_type": "concept", "id": "c1" }))
            .await.unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "Author 1", "node_type": "person", "id": "a1" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "p1", "target_id": "c1", "relationship_type": "implements" }))
            .await.unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "p1", "target_id": "a1", "relationship_type": "authored_by" }))
            .await.unwrap();

        let result = tool.execute(json!({ "action": "stats" })).await.unwrap();
        assert!(result.content.contains("Nodes: 3"));
        assert!(result.content.contains("Edges: 2"));
        assert!(result.content.contains("Paper: 1"));
        assert!(result.content.contains("Concept: 1"));
        assert!(result.content.contains("Person: 1"));
        assert!(result.content.contains("Most connected"));
        assert!(result.content.contains("p1"));
    }

    #[tokio::test]
    async fn test_export_dot() {
        let (_dir, tool) = make_tool();
        tool.execute(
            json!({ "action": "add_node", "name": "A", "node_type": "concept", "id": "a" }),
        )
        .await
        .unwrap();
        tool.execute(
            json!({ "action": "add_node", "name": "B", "node_type": "method", "id": "b" }),
        )
        .await
        .unwrap();
        tool.execute(json!({ "action": "add_edge", "source_id": "a", "target_id": "b", "relationship_type": "implements" }))
            .await.unwrap();

        let result = tool
            .execute(json!({ "action": "export_dot" }))
            .await
            .unwrap();
        assert!(result.content.contains("digraph KnowledgeGraph"));
        assert!(result.content.contains("\"a\""));
        assert!(result.content.contains("\"b\""));
        assert!(result.content.contains("Implements"));
        assert!(result.content.contains("->"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();
        // Add node, save, reload and verify
        tool.execute(json!({
            "action": "add_node",
            "name": "Test Node",
            "node_type": "dataset",
            "id": "test-node",
            "description": "A test dataset",
            "tags": ["test", "data"],
            "metadata": { "source": "kaggle", "size": "1GB" }
        }))
        .await
        .unwrap();

        // Reload state from disk
        let state = tool.load_state();
        assert_eq!(state.nodes.len(), 1);
        let node = &state.nodes[0];
        assert_eq!(node.id, "test-node");
        assert_eq!(node.name, "Test Node");
        assert_eq!(node.node_type, NodeType::Dataset);
        assert_eq!(node.description, "A test dataset");
        assert_eq!(node.tags, vec!["test", "data"]);
        assert_eq!(node.metadata.get("source").unwrap(), "kaggle");
        assert_eq!(node.metadata.get("size").unwrap(), "1GB");
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let (_dir, tool) = make_tool();
        let result = tool.execute(json!({ "action": "foobar" })).await.unwrap();
        assert!(result.content.contains("Unknown action"));
        assert!(result.content.contains("foobar"));
    }
}
