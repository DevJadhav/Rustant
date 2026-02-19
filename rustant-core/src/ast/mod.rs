//! AST engine for structural code analysis using tree-sitter.
//!
//! Feature-gated: requires one of `ast-rust`, `ast-python`, `ast-javascript`,
//! `ast-go`, `ast-java`, or `ast-all` features. Falls back to regex-based
//! extraction when no tree-sitter features are enabled.

pub mod languages;
pub mod references;

use std::path::Path;

/// A code symbol extracted from source.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
}

/// The kind of code symbol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Interface,
    Trait,
    Enum,
    Constant,
    Module,
    TypeAlias,
    Import,
    Other(String),
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::Constant => write!(f, "constant"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::TypeAlias => write!(f, "type_alias"),
            SymbolKind::Import => write!(f, "import"),
            SymbolKind::Other(s) => write!(f, "{s}"),
        }
    }
}

/// A reference from one symbol to another.
#[derive(Debug, Clone)]
pub struct Reference {
    pub from_file: String,
    pub from_line: usize,
    pub to_name: String,
    pub kind: ReferenceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceKind {
    Call,
    Import,
    TypeRef,
}

/// AST engine that can parse files and extract symbols/references.
pub struct AstEngine {
    #[cfg(any(
        feature = "ast-rust",
        feature = "ast-python",
        feature = "ast-javascript",
        feature = "ast-go",
        feature = "ast-java"
    ))]
    parsers: std::sync::Mutex<std::collections::HashMap<String, tree_sitter::Parser>>,
}

impl AstEngine {
    pub fn new() -> Self {
        Self {
            #[cfg(any(
                feature = "ast-rust",
                feature = "ast-python",
                feature = "ast-javascript",
                feature = "ast-go",
                feature = "ast-java"
            ))]
            parsers: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Extract symbols from a source file.
    pub fn extract_symbols(&self, path: &Path, source: &str) -> Vec<Symbol> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        #[cfg(any(
            feature = "ast-rust",
            feature = "ast-python",
            feature = "ast-javascript",
            feature = "ast-go",
            feature = "ast-java"
        ))]
        {
            if let Some(tree) = self.parse(ext, source) {
                return languages::extract_symbols_from_tree(
                    &tree,
                    source,
                    path.to_string_lossy().as_ref(),
                    ext,
                );
            }
        }

        // Fallback: regex-based extraction
        self.extract_symbols_regex(path, source)
    }

    /// Extract references from a source file.
    pub fn extract_references(&self, path: &Path, source: &str) -> Vec<Reference> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        #[cfg(any(
            feature = "ast-rust",
            feature = "ast-python",
            feature = "ast-javascript",
            feature = "ast-go",
            feature = "ast-java"
        ))]
        {
            if let Some(tree) = self.parse(ext, source) {
                return references::extract_references_from_tree(
                    &tree,
                    source,
                    path.to_string_lossy().as_ref(),
                    ext,
                );
            }
        }

        let _ = ext;
        Vec::new()
    }

    #[cfg(any(
        feature = "ast-rust",
        feature = "ast-python",
        feature = "ast-javascript",
        feature = "ast-go",
        feature = "ast-java"
    ))]
    fn parse(&self, ext: &str, source: &str) -> Option<tree_sitter::Tree> {
        let language = languages::language_for_extension(ext)?;
        let mut parsers = self.parsers.lock().ok()?;
        let parser = parsers.entry(ext.to_string()).or_insert_with(|| {
            let mut p = tree_sitter::Parser::new();
            let _ = p.set_language(&language);
            p
        });
        parser.parse(source, None)
    }

    fn extract_symbols_regex(&self, path: &Path, source: &str) -> Vec<Symbol> {
        let file = path.to_string_lossy().to_string();
        let mut symbols = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();

            // Rust patterns
            if trimmed.starts_with("pub fn ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("pub(crate) fn ")
            {
                if let Some(name) = extract_name_after(trimmed, "fn ") {
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Function,
                        file: file.clone(),
                        start_line: line_num + 1,
                        end_line: line_num + 1,
                        signature: trimmed.to_string(),
                    });
                }
            } else if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                if let Some(name) = extract_name_after(trimmed, "struct ") {
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Struct,
                        file: file.clone(),
                        start_line: line_num + 1,
                        end_line: line_num + 1,
                        signature: trimmed.to_string(),
                    });
                }
            } else if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
                if let Some(name) = extract_name_after(trimmed, "trait ") {
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Trait,
                        file: file.clone(),
                        start_line: line_num + 1,
                        end_line: line_num + 1,
                        signature: trimmed.to_string(),
                    });
                }
            } else if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
                if let Some(name) = extract_name_after(trimmed, "enum ") {
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Enum,
                        file: file.clone(),
                        start_line: line_num + 1,
                        end_line: line_num + 1,
                        signature: trimmed.to_string(),
                    });
                }
            }
            // Python/JS patterns
            else if trimmed.starts_with("def ") {
                if let Some(name) = extract_name_after(trimmed, "def ") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        file: file.clone(),
                        start_line: line_num + 1,
                        end_line: line_num + 1,
                        signature: trimmed.to_string(),
                    });
                }
            } else if trimmed.starts_with("class ") {
                if let Some(name) = extract_name_after(trimmed, "class ") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Class,
                        file: file.clone(),
                        start_line: line_num + 1,
                        end_line: line_num + 1,
                        signature: trimmed.to_string(),
                    });
                }
            } else if trimmed.starts_with("function ") || trimmed.starts_with("export function ") {
                let prefix = if trimmed.starts_with("export function ") {
                    "export function "
                } else {
                    "function "
                };
                if let Some(name) = extract_name_after(trimmed, prefix) {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        file: file.clone(),
                        start_line: line_num + 1,
                        end_line: line_num + 1,
                        signature: trimmed.to_string(),
                    });
                }
            }
        }

        symbols
    }
}

impl Default for AstEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_name_after(line: &str, prefix: &str) -> Option<String> {
    let after = line.split(prefix).nth(1)?;
    let name: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_symbols_regex_rust() {
        let engine = AstEngine::new();
        let source = r#"
pub fn hello() {}
struct Foo {}
pub trait Bar {}
enum Baz {}
"#;
        let symbols = engine.extract_symbols(Path::new("test.rs"), source);
        assert_eq!(symbols.len(), 4);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[1].name, "Foo");
        assert_eq!(symbols[1].kind, SymbolKind::Struct);
    }

    #[test]
    fn test_extract_symbols_regex_python() {
        let engine = AstEngine::new();
        let source = r#"
def hello():
    pass

class Foo:
    pass
"#;
        let symbols = engine.extract_symbols(Path::new("test.py"), source);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[1].name, "Foo");
    }

    #[test]
    fn test_extract_name_after() {
        assert_eq!(
            extract_name_after("fn hello()", "fn "),
            Some("hello".into())
        );
        assert_eq!(
            extract_name_after("struct Foo {", "struct "),
            Some("Foo".into())
        );
        assert_eq!(extract_name_after("no match", "fn "), None);
    }
}
