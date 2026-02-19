//! Symbol table builder — aggregates extracted symbols with lookup capabilities.
//!
//! Builds a structured symbol table from AST-extracted symbols, providing
//! efficient lookup by name, kind, file, and scope.

use super::{CallEdge, Language, Symbol, SymbolKind, Visibility};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A structured symbol table for a set of source files.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    /// All symbols indexed by a unique key (file:name:kind).
    symbols: Vec<Symbol>,
    /// Name → indices for fast lookup.
    by_name: HashMap<String, Vec<usize>>,
    /// File → indices for per-file queries.
    by_file: HashMap<PathBuf, Vec<usize>>,
    /// Kind → indices for filtering by symbol type.
    by_kind: HashMap<SymbolKind, Vec<usize>>,
}

impl SymbolTable {
    /// Create a new empty symbol table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a symbol table from a collection of symbols.
    pub fn from_symbols(symbols: Vec<Symbol>) -> Self {
        let mut table = Self::new();
        for symbol in symbols {
            table.add(symbol);
        }
        table
    }

    /// Add a symbol to the table.
    pub fn add(&mut self, symbol: Symbol) {
        let idx = self.symbols.len();

        self.by_name
            .entry(symbol.name.clone())
            .or_default()
            .push(idx);
        self.by_file
            .entry(symbol.file.clone())
            .or_default()
            .push(idx);
        self.by_kind.entry(symbol.kind).or_default().push(idx);

        self.symbols.push(symbol);
    }

    /// Look up symbols by name.
    pub fn lookup(&self, name: &str) -> Vec<&Symbol> {
        self.by_name
            .get(name)
            .map(|indices| indices.iter().map(|&i| &self.symbols[i]).collect())
            .unwrap_or_default()
    }

    /// Get all symbols in a specific file.
    pub fn symbols_in_file(&self, file: &Path) -> Vec<&Symbol> {
        self.by_file
            .get(file)
            .map(|indices| indices.iter().map(|&i| &self.symbols[i]).collect())
            .unwrap_or_default()
    }

    /// Get all symbols of a specific kind.
    pub fn symbols_of_kind(&self, kind: SymbolKind) -> Vec<&Symbol> {
        self.by_kind
            .get(&kind)
            .map(|indices| indices.iter().map(|&i| &self.symbols[i]).collect())
            .unwrap_or_default()
    }

    /// Get all public symbols.
    pub fn public_symbols(&self) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| s.visibility == Visibility::Public)
            .collect()
    }

    /// Get all functions/methods in the table.
    pub fn functions(&self) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .collect()
    }

    /// Get all type definitions (struct, class, enum, interface, trait).
    pub fn types(&self) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    SymbolKind::Struct
                        | SymbolKind::Class
                        | SymbolKind::Enum
                        | SymbolKind::Interface
                        | SymbolKind::Trait
                        | SymbolKind::Type
                )
            })
            .collect()
    }

    /// Get all files referenced in the symbol table.
    pub fn files(&self) -> Vec<&Path> {
        self.by_file.keys().map(|p| p.as_path()).collect()
    }

    /// Total number of symbols.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Find symbols at a specific line in a file.
    pub fn symbols_at_line(&self, file: &Path, line: usize) -> Vec<&Symbol> {
        self.symbols_in_file(file)
            .into_iter()
            .filter(|s| s.start_line <= line && line <= s.end_line)
            .collect()
    }

    /// Find the enclosing function/method for a given line.
    pub fn enclosing_function(&self, file: &Path, line: usize) -> Option<&Symbol> {
        self.symbols_in_file(file)
            .into_iter()
            .filter(|s| {
                matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                    && s.start_line <= line
                    && line <= s.end_line
            })
            .min_by_key(|s| s.end_line - s.start_line) // Innermost function
    }

    /// Get a summary of the symbol table.
    pub fn summary(&self) -> SymbolTableSummary {
        SymbolTableSummary {
            total_symbols: self.symbols.len(),
            functions: self.symbols_of_kind(SymbolKind::Function).len()
                + self.symbols_of_kind(SymbolKind::Method).len(),
            types: self.types().len(),
            files: self.by_file.len(),
            public: self.public_symbols().len(),
            languages: self
                .symbols
                .iter()
                .map(|s| s.language)
                .collect::<std::collections::HashSet<_>>()
                .len(),
        }
    }

    /// Iterate over all symbols.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }
}

/// Summary statistics for a symbol table.
#[derive(Debug, Clone)]
pub struct SymbolTableSummary {
    pub total_symbols: usize,
    pub functions: usize,
    pub types: usize,
    pub files: usize,
    pub public: usize,
    pub languages: usize,
}

/// Build a symbol table from source files by extracting symbols from each.
pub fn build_symbol_table(files: &[(PathBuf, String, Language)]) -> SymbolTable {
    let mut table = SymbolTable::new();

    for (path, source, lang) in files {
        match super::extract_symbols(source, *lang, path) {
            Ok(symbols) => {
                for sym in symbols {
                    table.add(sym);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to extract symbols from {}: {}", path.display(), e);
            }
        }
    }

    table
}

/// Extract call edges from source code (regex-based fallback).
///
/// This is a best-effort approach that looks for function call patterns.
/// For more accurate results, use tree-sitter based extraction.
pub fn extract_call_edges_regex(
    source: &str,
    language: Language,
    file: &Path,
    symbols: &SymbolTable,
) -> Vec<CallEdge> {
    let mut edges = Vec::new();

    // Get all functions in this file
    let file_functions = symbols.symbols_in_file(file);
    let function_names: Vec<&str> = symbols
        .functions()
        .iter()
        .map(|s| s.name.as_str())
        .collect();

    // Simple call pattern: identifier followed by `(`
    let call_pattern = match language {
        Language::Rust
        | Language::Go
        | Language::Java
        | Language::Kotlin
        | Language::CSharp
        | Language::Cpp
        | Language::C => regex::Regex::new(r"\b(\w+)\s*\(").ok(),
        Language::Python | Language::Ruby => regex::Regex::new(r"\b(\w+)\s*\(").ok(),
        Language::JavaScript | Language::TypeScript => regex::Regex::new(r"\b(\w+)\s*\(").ok(),
        _ => None,
    };

    if let Some(pattern) = call_pattern {
        for (line_num, line) in source.lines().enumerate() {
            let line_1indexed = line_num + 1;

            // Find the enclosing function for this line
            let caller = file_functions.iter().find(|s| {
                matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                    && s.start_line <= line_1indexed
                    && line_1indexed <= s.end_line
            });

            if let Some(caller) = caller {
                for cap in pattern.captures_iter(line) {
                    if let Some(callee_name) = cap.get(1) {
                        let callee = callee_name.as_str();
                        // Only record edges to known symbols
                        if function_names.contains(&callee) && callee != caller.name {
                            edges.push(CallEdge {
                                caller: caller.name.clone(),
                                callee: callee.to_string(),
                                call_site_line: line_1indexed,
                                file: file.to_path_buf(),
                            });
                        }
                    }
                }
            }
        }
    }

    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symbol(name: &str, kind: SymbolKind, file: &str, start: usize, end: usize) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            language: Language::Rust,
            file: PathBuf::from(file),
            start_line: start,
            end_line: end,
            visibility: Visibility::Public,
            parameters: Vec::new(),
            return_type: None,
        }
    }

    #[test]
    fn test_symbol_table_lookup() {
        let mut table = SymbolTable::new();
        table.add(make_symbol("foo", SymbolKind::Function, "a.rs", 1, 5));
        table.add(make_symbol("bar", SymbolKind::Function, "a.rs", 7, 10));
        table.add(make_symbol("foo", SymbolKind::Function, "b.rs", 1, 3));

        assert_eq!(table.lookup("foo").len(), 2);
        assert_eq!(table.lookup("bar").len(), 1);
        assert_eq!(table.lookup("baz").len(), 0);
    }

    #[test]
    fn test_symbol_table_by_file() {
        let mut table = SymbolTable::new();
        table.add(make_symbol("foo", SymbolKind::Function, "a.rs", 1, 5));
        table.add(make_symbol("bar", SymbolKind::Struct, "a.rs", 7, 10));
        table.add(make_symbol("baz", SymbolKind::Function, "b.rs", 1, 3));

        assert_eq!(table.symbols_in_file(Path::new("a.rs")).len(), 2);
        assert_eq!(table.symbols_in_file(Path::new("b.rs")).len(), 1);
    }

    #[test]
    fn test_symbol_table_by_kind() {
        let mut table = SymbolTable::new();
        table.add(make_symbol("foo", SymbolKind::Function, "a.rs", 1, 5));
        table.add(make_symbol("Bar", SymbolKind::Struct, "a.rs", 7, 10));
        table.add(make_symbol("baz", SymbolKind::Function, "b.rs", 1, 3));

        assert_eq!(table.symbols_of_kind(SymbolKind::Function).len(), 2);
        assert_eq!(table.symbols_of_kind(SymbolKind::Struct).len(), 1);
    }

    #[test]
    fn test_enclosing_function() {
        let mut table = SymbolTable::new();
        table.add(make_symbol("outer", SymbolKind::Function, "a.rs", 1, 20));
        table.add(make_symbol("inner", SymbolKind::Function, "a.rs", 5, 10));

        // Line 7 should be inside "inner" (innermost match)
        let enc = table.enclosing_function(Path::new("a.rs"), 7);
        assert!(enc.is_some());
        assert_eq!(enc.unwrap().name, "inner");

        // Line 15 should be inside "outer" only
        let enc = table.enclosing_function(Path::new("a.rs"), 15);
        assert!(enc.is_some());
        assert_eq!(enc.unwrap().name, "outer");
    }

    #[test]
    fn test_symbol_table_summary() {
        let mut table = SymbolTable::new();
        table.add(make_symbol("foo", SymbolKind::Function, "a.rs", 1, 5));
        table.add(make_symbol("Bar", SymbolKind::Struct, "a.rs", 7, 10));
        table.add(make_symbol("baz", SymbolKind::Method, "b.rs", 1, 3));

        let summary = table.summary();
        assert_eq!(summary.total_symbols, 3);
        assert_eq!(summary.functions, 2);
        assert_eq!(summary.types, 1);
        assert_eq!(summary.files, 2);
    }
}
