//! Dependency graph engine — builds and queries dependency trees from lockfiles.
//!
//! Supports 12 ecosystems via lockfile parsers.

pub mod cargo;
pub mod dart;
pub mod elixir;
pub mod go;
pub mod jvm;
pub mod npm;
pub mod php;
pub mod python;
pub mod registry;
pub mod ruby;
pub mod swift;

use crate::error::DepGraphError;
use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepNode {
    /// Package name.
    pub name: String,
    /// Resolved version.
    pub version: String,
    /// Ecosystem (npm, cargo, pypi, etc.).
    pub ecosystem: String,
    /// Whether this is a direct dependency.
    pub is_direct: bool,
    /// Whether this is a dev/test-only dependency.
    pub is_dev: bool,
    /// License identifier, if known.
    pub license: Option<String>,
    /// Source/registry URL.
    pub source: Option<String>,
}

/// An edge in the dependency graph.
#[derive(Debug, Clone)]
pub struct DepEdge {
    /// Version constraint used by the parent.
    pub version_req: Option<String>,
    /// Whether this is an optional dependency.
    pub optional: bool,
}

/// The dependency graph data structure.
pub struct DependencyGraph {
    graph: DiGraph<DepNode, DepEdge>,
    name_index: HashMap<String, NodeIndex>,
}

/// Blast radius of a vulnerability in a dependency.
#[derive(Debug, Clone)]
pub struct BlastRadius {
    /// The vulnerable package.
    pub package: String,
    /// Direct dependents (packages that directly depend on the vulnerable one).
    pub direct_dependents: Vec<String>,
    /// Total number of packages affected (transitive).
    pub total_affected: usize,
    /// Whether any production (non-dev) path is affected.
    pub affects_production: bool,
}

/// Version constraint information.
#[derive(Debug, Clone)]
pub struct VersionConstraint {
    /// The package that imposes this constraint.
    pub from_package: String,
    /// The version requirement string.
    pub requirement: String,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            name_index: HashMap::new(),
        }
    }

    /// Build a dependency graph from the workspace root.
    ///
    /// Auto-detects lockfile formats based on files present.
    pub fn build(workspace: &Path) -> Result<Self, DepGraphError> {
        let mut graph = Self::new();

        // Try Cargo.lock
        let cargo_lock = workspace.join("Cargo.lock");
        if cargo_lock.exists() {
            let content = std::fs::read_to_string(&cargo_lock).map_err(DepGraphError::Io)?;
            cargo::parse_cargo_lock(&content, &mut graph)?;
        }

        // Try package-lock.json / yarn.lock / pnpm-lock.yaml
        let pkg_lock = workspace.join("package-lock.json");
        if pkg_lock.exists() {
            let content = std::fs::read_to_string(&pkg_lock).map_err(DepGraphError::Io)?;
            npm::parse_package_lock(&content, &mut graph)?;
        }

        let yarn_lock = workspace.join("yarn.lock");
        if yarn_lock.exists() {
            let content = std::fs::read_to_string(&yarn_lock).map_err(DepGraphError::Io)?;
            npm::parse_yarn_lock(&content, &mut graph)?;
        }

        // Try Python lockfiles
        let pip_req = workspace.join("requirements.txt");
        if pip_req.exists() {
            let content = std::fs::read_to_string(&pip_req).map_err(DepGraphError::Io)?;
            python::parse_requirements_txt(&content, &mut graph)?;
        }

        let poetry_lock = workspace.join("poetry.lock");
        if poetry_lock.exists() {
            let content = std::fs::read_to_string(&poetry_lock).map_err(DepGraphError::Io)?;
            python::parse_poetry_lock(&content, &mut graph)?;
        }

        // Try Go lockfiles
        let go_sum = workspace.join("go.sum");
        if go_sum.exists() {
            let content = std::fs::read_to_string(&go_sum).map_err(DepGraphError::Io)?;
            go::parse_go_sum(&content, &mut graph)?;
        }

        let go_mod = workspace.join("go.mod");
        if go_mod.exists() {
            let content = std::fs::read_to_string(&go_mod).map_err(DepGraphError::Io)?;
            go::parse_go_mod(&content, &mut graph)?;
        }

        // Try JVM lockfiles
        let build_gradle = workspace.join("build.gradle");
        if build_gradle.exists() {
            let content = std::fs::read_to_string(&build_gradle).map_err(DepGraphError::Io)?;
            jvm::parse_build_gradle(&content, &mut graph)?;
        }

        let build_gradle_kts = workspace.join("build.gradle.kts");
        if build_gradle_kts.exists() {
            let content = std::fs::read_to_string(&build_gradle_kts).map_err(DepGraphError::Io)?;
            jvm::parse_build_gradle(&content, &mut graph)?;
        }

        let pom_xml = workspace.join("pom.xml");
        if pom_xml.exists() {
            let content = std::fs::read_to_string(&pom_xml).map_err(DepGraphError::Io)?;
            jvm::parse_pom_xml(&content, &mut graph)?;
        }

        // Try Ruby lockfile
        let gemfile_lock = workspace.join("Gemfile.lock");
        if gemfile_lock.exists() {
            let content = std::fs::read_to_string(&gemfile_lock).map_err(DepGraphError::Io)?;
            ruby::parse_gemfile_lock(&content, &mut graph)?;
        }

        // Try PHP lockfile
        let composer_lock = workspace.join("composer.lock");
        if composer_lock.exists() {
            let content = std::fs::read_to_string(&composer_lock).map_err(DepGraphError::Io)?;
            php::parse_composer_lock(&content, &mut graph)?;
        }

        // Try Swift Package.resolved
        let swift_resolved = workspace.join("Package.resolved");
        if swift_resolved.exists() {
            let content = std::fs::read_to_string(&swift_resolved).map_err(DepGraphError::Io)?;
            swift::parse_package_resolved(&content, &mut graph)?;
        }

        // Try Dart pubspec.lock
        let pubspec_lock = workspace.join("pubspec.lock");
        if pubspec_lock.exists() {
            let content = std::fs::read_to_string(&pubspec_lock).map_err(DepGraphError::Io)?;
            dart::parse_pubspec_lock(&content, &mut graph)?;
        }

        // Try Elixir mix.lock
        let mix_lock = workspace.join("mix.lock");
        if mix_lock.exists() {
            let content = std::fs::read_to_string(&mix_lock).map_err(DepGraphError::Io)?;
            elixir::parse_mix_lock(&content, &mut graph)?;
        }

        // Try Python Pipfile.lock
        let pipfile_lock = workspace.join("Pipfile.lock");
        if pipfile_lock.exists() {
            let content = std::fs::read_to_string(&pipfile_lock).map_err(DepGraphError::Io)?;
            python::parse_pipfile_lock(&content, &mut graph)?;
        }

        // Try pnpm-lock.yaml
        let pnpm_lock = workspace.join("pnpm-lock.yaml");
        if pnpm_lock.exists() {
            let content = std::fs::read_to_string(&pnpm_lock).map_err(DepGraphError::Io)?;
            npm::parse_pnpm_lock(&content, &mut graph)?;
        }

        Ok(graph)
    }

    /// Add a dependency node to the graph.
    pub fn add_node(&mut self, node: DepNode) -> NodeIndex {
        let key = format!("{}@{}", node.name, node.version);
        if let Some(&idx) = self.name_index.get(&key) {
            return idx;
        }
        let idx = self.graph.add_node(node);
        let key_again = {
            let n = &self.graph[idx];
            format!("{}@{}", n.name, n.version)
        };
        self.name_index.insert(key_again, idx);
        idx
    }

    /// Add a dependency edge between two packages.
    pub fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, edge: DepEdge) {
        self.graph.add_edge(from, to, edge);
    }

    /// Look up a node index by package name (returns first match).
    pub fn find_node(&self, name: &str) -> Option<NodeIndex> {
        self.name_index
            .iter()
            .find(|(key, _)| key.starts_with(&format!("{name}@")))
            .map(|(_, &idx)| idx)
    }

    /// Get a node by index.
    pub fn get_node(&self, idx: NodeIndex) -> Option<&DepNode> {
        self.graph.node_weight(idx)
    }

    /// Get a mutable node by index.
    pub fn get_node_mut(&mut self, idx: NodeIndex) -> Option<&mut DepNode> {
        self.graph.node_weight_mut(idx)
    }

    /// Get all transitive dependencies of a package.
    pub fn transitive_deps(&self, name: &str) -> Vec<&DepNode> {
        let Some(start) = self.find_node(name) else {
            return Vec::new();
        };

        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start];
        let mut result = Vec::new();

        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            if node != start
                && let Some(dep) = self.graph.node_weight(node)
            {
                result.push(dep);
            }
            for neighbor in self.graph.neighbors_directed(node, Direction::Outgoing) {
                stack.push(neighbor);
            }
        }

        result
    }

    /// Get all packages that depend on the given package (reverse deps).
    pub fn reverse_deps(&self, name: &str) -> Vec<&DepNode> {
        let Some(start) = self.find_node(name) else {
            return Vec::new();
        };

        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start];
        let mut result = Vec::new();

        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            if node != start
                && let Some(dep) = self.graph.node_weight(node)
            {
                result.push(dep);
            }
            for neighbor in self.graph.neighbors_directed(node, Direction::Incoming) {
                stack.push(neighbor);
            }
        }

        result
    }

    /// Calculate the blast radius of a vulnerable package.
    pub fn blast_radius(&self, name: &str) -> BlastRadius {
        let reverse = self.reverse_deps(name);
        let direct_dependents: Vec<String> = if let Some(start) = self.find_node(name) {
            self.graph
                .neighbors_directed(start, Direction::Incoming)
                .filter_map(|n| self.graph.node_weight(n).map(|dep| dep.name.clone()))
                .collect()
        } else {
            Vec::new()
        };

        let affects_production = reverse.iter().any(|dep| !dep.is_dev);

        BlastRadius {
            package: name.to_string(),
            direct_dependents,
            total_affected: reverse.len(),
            affects_production,
        }
    }

    /// Get version constraints imposed on a package.
    pub fn version_constraints(&self, name: &str) -> Vec<VersionConstraint> {
        let Some(idx) = self.find_node(name) else {
            return Vec::new();
        };

        self.graph
            .edges_directed(idx, Direction::Incoming)
            .filter_map(|edge| {
                let from_idx = edge.source();
                let from_node = self.graph.node_weight(from_idx)?;
                let version_req = edge.weight().version_req.clone()?;
                Some(VersionConstraint {
                    from_package: from_node.name.clone(),
                    requirement: version_req,
                })
            })
            .collect()
    }

    /// Get all direct dependencies (non-transitive).
    pub fn direct_deps(&self) -> Vec<&DepNode> {
        self.graph.node_weights().filter(|n| n.is_direct).collect()
    }

    /// Total number of packages in the graph.
    pub fn package_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Total number of dependency relationships.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// List all packages.
    pub fn all_packages(&self) -> Vec<&DepNode> {
        self.graph.node_weights().collect()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let graph = DependencyGraph::new();
        assert_eq!(graph.package_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_add_and_find() {
        let mut graph = DependencyGraph::new();
        let idx = graph.add_node(DepNode {
            name: "serde".into(),
            version: "1.0.0".into(),
            ecosystem: "cargo".into(),
            is_direct: true,
            is_dev: false,
            license: Some("MIT OR Apache-2.0".into()),
            source: None,
        });

        assert_eq!(graph.package_count(), 1);
        assert!(graph.find_node("serde").is_some());
        assert!(graph.find_node("nonexistent").is_none());
        assert_eq!(graph.get_node(idx).unwrap().name, "serde");
    }

    #[test]
    fn test_transitive_deps() {
        let mut graph = DependencyGraph::new();
        let a = graph.add_node(DepNode {
            name: "app".into(),
            version: "1.0.0".into(),
            ecosystem: "cargo".into(),
            is_direct: true,
            is_dev: false,
            license: None,
            source: None,
        });
        let b = graph.add_node(DepNode {
            name: "lib-b".into(),
            version: "2.0.0".into(),
            ecosystem: "cargo".into(),
            is_direct: true,
            is_dev: false,
            license: None,
            source: None,
        });
        let c = graph.add_node(DepNode {
            name: "lib-c".into(),
            version: "3.0.0".into(),
            ecosystem: "cargo".into(),
            is_direct: false,
            is_dev: false,
            license: None,
            source: None,
        });

        graph.add_edge(
            a,
            b,
            DepEdge {
                version_req: Some("^2.0".into()),
                optional: false,
            },
        );
        graph.add_edge(
            b,
            c,
            DepEdge {
                version_req: Some("^3.0".into()),
                optional: false,
            },
        );

        let deps = graph.transitive_deps("app");
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn test_blast_radius() {
        let mut graph = DependencyGraph::new();
        let a = graph.add_node(DepNode {
            name: "app".into(),
            version: "1.0.0".into(),
            ecosystem: "cargo".into(),
            is_direct: true,
            is_dev: false,
            license: None,
            source: None,
        });
        let b = graph.add_node(DepNode {
            name: "vuln-lib".into(),
            version: "0.1.0".into(),
            ecosystem: "cargo".into(),
            is_direct: true,
            is_dev: false,
            license: None,
            source: None,
        });
        graph.add_edge(
            a,
            b,
            DepEdge {
                version_req: None,
                optional: false,
            },
        );

        let radius = graph.blast_radius("vuln-lib");
        assert_eq!(radius.direct_dependents.len(), 1);
        assert!(radius.affects_production);
    }
}
