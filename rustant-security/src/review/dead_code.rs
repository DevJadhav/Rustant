//! Dead code detector — reachability-based analysis via AST call graph.
//!
//! Identifies unreachable functions, unused imports, and dead modules
//! using call graph analysis from the AST engine.

use crate::ast::Language;
use crate::ast::call_graph::{CallGraph, build_call_graph_for_file};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A dead code finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeItem {
    /// File containing the dead code.
    pub file: PathBuf,
    /// Name of the unreachable symbol.
    pub name: String,
    /// Start line (1-indexed).
    pub start_line: usize,
    /// End line (1-indexed).
    pub end_line: usize,
    /// Kind of dead code.
    pub kind: DeadCodeKind,
    /// Confidence (0.0-1.0). Lower for items that might be used via
    /// reflection, macros, or external crates.
    pub confidence: f32,
}

/// Classification of dead code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeadCodeKind {
    /// Function with no callers reachable from public API.
    UnreachableFunction,
    /// Function defined but never called at all.
    UncalledFunction,
    /// Test helper only used in commented-out tests.
    UnusedTestHelper,
}

impl std::fmt::Display for DeadCodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeadCodeKind::UnreachableFunction => write!(f, "unreachable function"),
            DeadCodeKind::UncalledFunction => write!(f, "uncalled function"),
            DeadCodeKind::UnusedTestHelper => write!(f, "unused test helper"),
        }
    }
}

/// Result of a dead code analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeReport {
    /// All detected dead code items.
    pub items: Vec<DeadCodeItem>,
    /// Total functions analyzed.
    pub total_functions: usize,
    /// Number of reachable functions.
    pub reachable_count: usize,
    /// Percentage of dead code (0.0-100.0).
    pub dead_percentage: f64,
}

impl DeadCodeReport {
    /// Get items for a specific file.
    pub fn items_for_file(&self, path: &Path) -> Vec<&DeadCodeItem> {
        self.items.iter().filter(|i| i.file == path).collect()
    }

    /// Get items sorted by confidence (highest first).
    pub fn items_by_confidence(&self) -> Vec<&DeadCodeItem> {
        let mut sorted: Vec<&DeadCodeItem> = self.items.iter().collect();
        sorted.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted
    }
}

/// Dead code detector using call graph reachability analysis.
pub struct DeadCodeDetector;

impl DeadCodeDetector {
    /// Analyze a single file for dead code.
    pub fn analyze_file(source: &str, language: Language, file: &Path) -> DeadCodeReport {
        let graph = build_call_graph_for_file(source, language, file);
        Self::report_from_graph(&graph, file)
    }

    /// Build a report from a pre-built call graph.
    pub fn report_from_graph(graph: &CallGraph, file: &Path) -> DeadCodeReport {
        let total_functions = graph.node_count();
        let unreachable = graph.unreachable_functions();
        let dead_count = unreachable.len();

        let items: Vec<DeadCodeItem> = unreachable
            .into_iter()
            .map(|name| {
                let (start_line, end_line) = graph
                    .get_node(name)
                    .map(|n| (n.start_line, n.end_line))
                    .unwrap_or((0, 0));

                // Lower confidence for test functions and conventionally-named functions
                let confidence = if is_test_function(name) {
                    0.3
                } else if is_conventional_entry_point(name) {
                    0.4
                } else {
                    0.8
                };

                let kind = if graph.callers(name).is_empty() {
                    DeadCodeKind::UncalledFunction
                } else {
                    DeadCodeKind::UnreachableFunction
                };

                DeadCodeItem {
                    file: file.to_path_buf(),
                    name: name.to_string(),
                    start_line,
                    end_line,
                    kind,
                    confidence,
                }
            })
            .collect();

        let reachable_count = total_functions.saturating_sub(dead_count);
        let dead_percentage = if total_functions > 0 {
            (dead_count as f64 / total_functions as f64) * 100.0
        } else {
            0.0
        };

        DeadCodeReport {
            items,
            total_functions,
            reachable_count,
            dead_percentage,
        }
    }
}

/// Check if a function name looks like a test function.
fn is_test_function(name: &str) -> bool {
    name.starts_with("test_")
        || name.starts_with("test")
        || name.ends_with("_test")
        || name.contains("_test_")
}

/// Check if a function name matches conventional entry points that may be
/// called externally (e.g., via frameworks, macros, or serialization).
fn is_conventional_entry_point(name: &str) -> bool {
    matches!(
        name,
        "main"
            | "new"
            | "default"
            | "from"
            | "into"
            | "drop"
            | "clone"
            | "fmt"
            | "serialize"
            | "deserialize"
            | "init"
            | "setup"
            | "teardown"
    ) || name.starts_with("on_")
        || name.starts_with("handle_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::call_graph::CallGraph;
    use crate::ast::{CallEdge, Symbol, SymbolKind, Visibility};

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
    fn test_dead_code_detection() {
        let table = crate::ast::symbols::SymbolTable::from_symbols(vec![
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
        let report = DeadCodeDetector::report_from_graph(&graph, Path::new("test.rs"));

        assert_eq!(report.total_functions, 3);
        assert_eq!(report.reachable_count, 2);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].name, "unused");
        assert_eq!(report.items[0].kind, DeadCodeKind::UncalledFunction);
    }

    #[test]
    fn test_no_dead_code() {
        let table = crate::ast::symbols::SymbolTable::from_symbols(vec![
            make_sym("main", true, 1, 5),
            make_sym("helper", false, 7, 10),
        ]);

        let edges = vec![CallEdge {
            caller: "main".into(),
            callee: "helper".into(),
            call_site_line: 3,
            file: PathBuf::from("test.rs"),
        }];

        let graph = CallGraph::from_edges(&table, &edges);
        let report = DeadCodeDetector::report_from_graph(&graph, Path::new("test.rs"));

        assert_eq!(report.items.len(), 0);
        assert_eq!(report.dead_percentage, 0.0);
    }

    #[test]
    fn test_test_function_lower_confidence() {
        let table = crate::ast::symbols::SymbolTable::from_symbols(vec![make_sym(
            "test_something",
            false,
            1,
            5,
        )]);

        let graph = CallGraph::from_edges(&table, &[]);
        let report = DeadCodeDetector::report_from_graph(&graph, Path::new("test.rs"));

        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].confidence, 0.3);
    }

    #[test]
    fn test_dead_code_report_filtering() {
        let table = crate::ast::symbols::SymbolTable::from_symbols(vec![
            make_sym("unused_a", false, 1, 5),
            make_sym("unused_b", false, 7, 10),
        ]);

        let graph = CallGraph::from_edges(&table, &[]);
        let report = DeadCodeDetector::report_from_graph(&graph, Path::new("test.rs"));

        assert_eq!(report.items_for_file(Path::new("test.rs")).len(), 2);
        assert_eq!(report.items_for_file(Path::new("other.rs")).len(), 0);
        assert_eq!(report.items_by_confidence().len(), 2);
    }
}
