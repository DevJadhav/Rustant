//! Call graph constructor — builds function-level call graphs from source code.
//!
//! MVP: single-file call graph using regex-based call detection.
//! Full: cross-file call graph via `SymbolTable` + `ProjectIndexer` integration.

use super::{CallEdge, Language, SymbolKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A call graph representing function-level dependencies.
#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    /// All nodes (function names) with their file locations.
    nodes: HashMap<String, CallGraphNode>,
    /// Edges: caller → set of callees.
    edges: HashMap<String, Vec<CallGraphEdge>>,
    /// Reverse edges: callee → set of callers.
    reverse_edges: HashMap<String, Vec<String>>,
}

/// A node in the call graph.
#[derive(Debug, Clone)]
pub struct CallGraphNode {
    pub name: String,
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Language,
    pub is_public: bool,
}

/// An edge in the call graph.
#[derive(Debug, Clone)]
pub struct CallGraphEdge {
    pub callee: String,
    pub call_site_line: usize,
    pub file: PathBuf,
}

impl CallGraph {
    /// Create a new empty call graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a call graph from extracted call edges and a symbol table.
    pub fn from_edges(symbols: &super::symbols::SymbolTable, edges: &[CallEdge]) -> Self {
        let mut graph = Self::new();

        // Add nodes from symbols
        for sym in symbols.iter() {
            if matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
                graph.nodes.insert(
                    sym.name.clone(),
                    CallGraphNode {
                        name: sym.name.clone(),
                        file: sym.file.clone(),
                        start_line: sym.start_line,
                        end_line: sym.end_line,
                        language: sym.language,
                        is_public: sym.visibility == super::Visibility::Public,
                    },
                );
            }
        }

        // Add edges
        for edge in edges {
            graph
                .edges
                .entry(edge.caller.clone())
                .or_default()
                .push(CallGraphEdge {
                    callee: edge.callee.clone(),
                    call_site_line: edge.call_site_line,
                    file: edge.file.clone(),
                });

            graph
                .reverse_edges
                .entry(edge.callee.clone())
                .or_default()
                .push(edge.caller.clone());
        }

        graph
    }

    /// Get all direct callees of a function.
    pub fn callees(&self, function: &str) -> Vec<&str> {
        self.edges
            .get(function)
            .map(|edges| edges.iter().map(|e| e.callee.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get all direct callers of a function.
    pub fn callers(&self, function: &str) -> Vec<&str> {
        self.reverse_edges
            .get(function)
            .map(|callers| callers.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get transitive callees (all functions reachable from the given function).
    pub fn transitive_callees(&self, function: &str) -> HashSet<String> {
        let mut visited = HashSet::new();
        let mut stack = vec![function.to_string()];

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            for callee in self.callees(&current) {
                if !visited.contains(callee) {
                    stack.push(callee.to_string());
                }
            }
        }

        visited.remove(function);
        visited
    }

    /// Get transitive callers (all functions that can reach the given function).
    pub fn transitive_callers(&self, function: &str) -> HashSet<String> {
        let mut visited = HashSet::new();
        let mut stack = vec![function.to_string()];

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            for caller in self.callers(&current) {
                if !visited.contains(caller) {
                    stack.push(caller.to_string());
                }
            }
        }

        visited.remove(function);
        visited
    }

    /// Check if a function is reachable from any public entry point.
    pub fn is_reachable_from_public(&self, function: &str) -> bool {
        let callers = self.transitive_callers(function);
        // The function itself might be public
        if let Some(node) = self.nodes.get(function)
            && node.is_public
        {
            return true;
        }
        // Check if any transitive caller is public
        callers
            .iter()
            .any(|c| self.nodes.get(c).is_some_and(|n| n.is_public))
    }

    /// Find potential dead code (functions not reachable from any public function).
    pub fn unreachable_functions(&self) -> Vec<&str> {
        self.nodes
            .keys()
            .filter(|name| !self.is_reachable_from_public(name))
            .map(|s| s.as_str())
            .collect()
    }

    /// Detect cycles in the call graph.
    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut on_stack = HashSet::new();
        let mut path = Vec::new();

        for node in self.nodes.keys() {
            if !visited.contains(node.as_str()) {
                self.dfs_cycles(node, &mut visited, &mut on_stack, &mut path, &mut cycles);
            }
        }

        cycles
    }

    fn dfs_cycles(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        on_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        on_stack.insert(node.to_string());
        path.push(node.to_string());

        for callee in self.callees(node) {
            if !visited.contains(callee) {
                self.dfs_cycles(callee, visited, on_stack, path, cycles);
            } else if on_stack.contains(callee) {
                // Found a cycle — extract it
                if let Some(start_idx) = path.iter().position(|n| n == callee) {
                    let cycle: Vec<String> = path[start_idx..].to_vec();
                    if cycle.len() > 1 {
                        cycles.push(cycle);
                    }
                }
            }
        }

        path.pop();
        on_stack.remove(node);
    }

    /// Get the number of nodes in the call graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges in the call graph.
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|v| v.len()).sum()
    }

    /// Get all node names.
    pub fn node_names(&self) -> Vec<&str> {
        self.nodes.keys().map(|s| s.as_str()).collect()
    }

    /// Get a node by name.
    pub fn get_node(&self, name: &str) -> Option<&CallGraphNode> {
        self.nodes.get(name)
    }
}

/// Build a call graph from source code (single-file MVP).
pub fn build_call_graph_for_file(source: &str, language: Language, file: &Path) -> CallGraph {
    // Extract symbols
    let symbols_vec = super::extract_symbols(source, language, file).unwrap_or_default();
    let table = super::symbols::SymbolTable::from_symbols(symbols_vec);

    // Extract call edges
    let edges = super::symbols::extract_call_edges_regex(source, language, file, &table);

    CallGraph::from_edges(&table, &edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Symbol, Visibility};

    fn make_sym(name: &str, public: bool, start: usize, end: usize) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: Language::Rust,
            file: PathBuf::from("test.rs"),
            start_line: start,
            end_line: end,
            visibility: if public {
                Visibility::Public
            } else {
                Visibility::Private
            },
            parameters: Vec::new(),
            return_type: None,
        }
    }

    #[test]
    fn test_call_graph_basic() {
        let table = super::super::symbols::SymbolTable::from_symbols(vec![
            make_sym("main", true, 1, 5),
            make_sym("helper", false, 7, 10),
            make_sym("unused", false, 12, 15),
        ]);

        let edges = vec![CallEdge {
            caller: "main".into(),
            callee: "helper".into(),
            call_site_line: 3,
            file: PathBuf::from("test.rs"),
        }];

        let graph = CallGraph::from_edges(&table, &edges);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.callees("main"), vec!["helper"]);
        assert_eq!(graph.callers("helper"), vec!["main"]);
    }

    #[test]
    fn test_transitive_callees() {
        let table = super::super::symbols::SymbolTable::from_symbols(vec![
            make_sym("a", true, 1, 5),
            make_sym("b", false, 7, 10),
            make_sym("c", false, 12, 15),
        ]);

        let edges = vec![
            CallEdge {
                caller: "a".into(),
                callee: "b".into(),
                call_site_line: 3,
                file: PathBuf::from("test.rs"),
            },
            CallEdge {
                caller: "b".into(),
                callee: "c".into(),
                call_site_line: 9,
                file: PathBuf::from("test.rs"),
            },
        ];

        let graph = CallGraph::from_edges(&table, &edges);
        let reachable = graph.transitive_callees("a");
        assert!(reachable.contains("b"));
        assert!(reachable.contains("c"));
    }

    #[test]
    fn test_reachability() {
        let table = super::super::symbols::SymbolTable::from_symbols(vec![
            make_sym("public_fn", true, 1, 5),
            make_sym("reachable", false, 7, 10),
            make_sym("unreachable", false, 12, 15),
        ]);

        let edges = vec![CallEdge {
            caller: "public_fn".into(),
            callee: "reachable".into(),
            call_site_line: 3,
            file: PathBuf::from("test.rs"),
        }];

        let graph = CallGraph::from_edges(&table, &edges);

        assert!(graph.is_reachable_from_public("public_fn"));
        assert!(graph.is_reachable_from_public("reachable"));
        assert!(!graph.is_reachable_from_public("unreachable"));
    }

    #[test]
    fn test_cycle_detection() {
        let table = super::super::symbols::SymbolTable::from_symbols(vec![
            make_sym("a", true, 1, 5),
            make_sym("b", false, 7, 10),
        ]);

        let edges = vec![
            CallEdge {
                caller: "a".into(),
                callee: "b".into(),
                call_site_line: 3,
                file: PathBuf::from("test.rs"),
            },
            CallEdge {
                caller: "b".into(),
                callee: "a".into(),
                call_site_line: 9,
                file: PathBuf::from("test.rs"),
            },
        ];

        let graph = CallGraph::from_edges(&table, &edges);
        let cycles = graph.find_cycles();
        assert!(!cycles.is_empty());
    }
}
