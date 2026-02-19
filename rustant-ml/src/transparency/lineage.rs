//! Data and model lineage graphs.

use serde::{Deserialize, Serialize};

/// A node in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub id: String,
    pub node_type: LineageNodeType,
    pub name: String,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Type of lineage node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineageNodeType {
    Dataset,
    Transform,
    Model,
    Feature,
    Prediction,
    Evaluation,
}

/// An edge in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEdge {
    pub from: String,
    pub to: String,
    pub relationship: String,
}

/// Lineage graph for tracking data/model flow.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LineageGraph {
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
}

impl LineageGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: LineageNode) {
        self.nodes.push(node);
    }

    pub fn add_edge(&mut self, from: &str, to: &str, relationship: &str) {
        self.edges.push(LineageEdge {
            from: from.to_string(),
            to: to.to_string(),
            relationship: relationship.to_string(),
        });
    }

    pub fn trace(&self, node_id: &str) -> Vec<&LineageNode> {
        let mut result = Vec::new();
        let mut to_visit = vec![node_id.to_string()];
        let mut visited = std::collections::HashSet::new();

        while let Some(current) = to_visit.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(node) = self.nodes.iter().find(|n| n.id == current) {
                result.push(node);
            }
            for edge in &self.edges {
                if edge.to == current && !visited.contains(&edge.from) {
                    to_visit.push(edge.from.clone());
                }
            }
        }
        result
    }

    /// Trace the lineage of a dataset by its ID.
    pub fn trace_data(&self, dataset_id: &str) -> Vec<&LineageNode> {
        self.trace(dataset_id)
    }

    /// Trace the lineage of a model by its ID.
    pub fn trace_model(&self, model_id: &str) -> Vec<&LineageNode> {
        self.trace(model_id)
    }

    /// Export as DOT format for visualization.
    pub fn export_dot(&self) -> String {
        let mut dot = String::from("digraph lineage {\n");
        for node in &self.nodes {
            dot.push_str(&format!(
                "  \"{}\" [label=\"{}\\n({:?})\"];\n",
                node.id, node.name, node.node_type
            ));
        }
        for edge in &self.edges {
            dot.push_str(&format!(
                "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
                edge.from, edge.to, edge.relationship
            ));
        }
        dot.push_str("}\n");
        dot
    }
}
