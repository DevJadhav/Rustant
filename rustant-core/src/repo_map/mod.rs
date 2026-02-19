//! Repository Map — combines AST extraction, code graph, and PageRank ranking.
//!
//! Provides structural code intelligence for context hydration and symbol search.

pub mod graph;
pub mod ranking;

use crate::ast::{AstEngine, Symbol};
use std::path::Path;

/// A chunk of context selected for injection into the LLM.
#[derive(Debug, Clone)]
pub struct ContextChunk {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub relevance_score: f64,
}

/// The repository map: symbols, references, and their graph.
pub struct RepoMap {
    engine: AstEngine,
    pub code_graph: graph::CodeGraph,
    pub symbols: Vec<Symbol>,
}

impl RepoMap {
    /// Build a repo map by walking a workspace and extracting symbols + refs.
    pub fn build(workspace: &Path) -> Self {
        let engine = AstEngine::new();
        let mut code_graph = graph::CodeGraph::new();
        let mut all_symbols = Vec::new();

        let walker = ignore::WalkBuilder::new(workspace)
            .hidden(true)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if !is_code_file(ext) {
                continue;
            }

            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let symbols = engine.extract_symbols(path, &source);
            let references = engine.extract_references(path, &source);

            // Add symbols as nodes
            for sym in &symbols {
                code_graph.add_symbol(sym);
            }

            // Add references as edges
            for reference in &references {
                code_graph.add_reference(reference);
            }

            all_symbols.extend(symbols);
        }

        Self {
            engine,
            code_graph,
            symbols: all_symbols,
        }
    }

    /// Select context chunks relevant to a query, fitting within a token budget.
    pub fn select_context(&self, query: &str, token_budget: usize) -> Vec<ContextChunk> {
        ranking::select_ranked_context(&self.symbols, &self.code_graph, query, token_budget)
    }

    /// Get the AST engine reference.
    pub fn engine(&self) -> &AstEngine {
        &self.engine
    }

    /// Get symbols for a specific file.
    pub fn symbols_in_file(&self, file: &str) -> Vec<&Symbol> {
        self.symbols.iter().filter(|s| s.file == file).collect()
    }

    /// Search symbols by name (case-insensitive substring match).
    pub fn search_symbols(&self, query: &str) -> Vec<&Symbol> {
        let query_lower = query.to_lowercase();
        self.symbols
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// Find all references to a given symbol name.
    pub fn find_references(&self, symbol_name: &str) -> Vec<crate::ast::Reference> {
        // Re-derive from graph edges that point to this symbol
        self.code_graph.references_to(symbol_name)
    }

    /// Get a formatted string representation of the repo map.
    pub fn format_summary(&self) -> String {
        let file_count = {
            let mut files = std::collections::HashSet::new();
            for sym in &self.symbols {
                files.insert(&sym.file);
            }
            files.len()
        };

        let mut output = format!(
            "Repository Map: {} symbols across {} files\n\n",
            self.symbols.len(),
            file_count
        );

        // Group by file
        let mut by_file: std::collections::BTreeMap<&str, Vec<&Symbol>> =
            std::collections::BTreeMap::new();
        for sym in &self.symbols {
            by_file.entry(&sym.file).or_default().push(sym);
        }

        for (file, symbols) in &by_file {
            output.push_str(&format!("{file}:\n"));
            for sym in symbols {
                output.push_str(&format!(
                    "  L{}: {} {} - {}\n",
                    sym.start_line, sym.kind, sym.name, sym.signature
                ));
            }
        }

        output
    }
}

fn is_code_file(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "rb"
            | "swift"
            | "kt"
            | "scala"
            | "cs"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_repo_map() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("main.rs"),
            "pub fn hello() {}\nstruct Foo {}\n",
        )
        .unwrap();

        let map = RepoMap::build(dir.path());
        assert!(map.symbols.len() >= 2);
    }

    #[test]
    fn test_search_symbols() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("main.rs"),
            "pub fn hello_world() {}\nfn goodbye() {}\n",
        )
        .unwrap();

        let map = RepoMap::build(dir.path());
        let results = map.search_symbols("hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "hello_world");
    }

    #[test]
    fn test_format_summary() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            "pub fn foo() {}\npub struct Bar {}\n",
        )
        .unwrap();

        let map = RepoMap::build(dir.path());
        let summary = map.format_summary();
        assert!(summary.contains("foo"));
        assert!(summary.contains("Bar"));
    }
}
