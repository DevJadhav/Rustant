//! Tree-sitter AST engine — language parsing, symbol extraction, and pattern matching.
//!
//! Provides a thread-safe, LRU-cached AST layer for use by SAST scanners,
//! code review, and quality analysis.

pub mod cache;
pub mod call_graph;
pub mod languages;
pub mod query;
pub mod symbols;

use crate::error::AstError;
use std::path::Path;

/// Detected programming language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Kotlin,
    CSharp,
    Cpp,
    C,
    Ruby,
    Php,
    Swift,
    Bash,
    Sql,
    Hcl,
    Yaml,
    Toml,
    Json,
    Html,
    Css,
    Dockerfile,
    GraphQL,
    Dart,
    Elixir,
    Scala,
    Unknown,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" | "pyw" => Language::Python,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" | "mts" => Language::TypeScript,
            "go" => Language::Go,
            "java" => Language::Java,
            "kt" | "kts" => Language::Kotlin,
            "cs" => Language::CSharp,
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "h" => Language::Cpp,
            "c" => Language::C,
            "rb" | "erb" => Language::Ruby,
            "php" => Language::Php,
            "swift" => Language::Swift,
            "sh" | "bash" | "zsh" => Language::Bash,
            "sql" => Language::Sql,
            "tf" | "hcl" => Language::Hcl,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            "json" => Language::Json,
            "html" | "htm" => Language::Html,
            "css" | "scss" | "less" => Language::Css,
            "graphql" | "gql" => Language::GraphQL,
            "dart" => Language::Dart,
            "ex" | "exs" => Language::Elixir,
            "scala" | "sc" => Language::Scala,
            _ => Language::Unknown,
        }
    }

    /// Detect language from a file path.
    pub fn from_path(path: &Path) -> Self {
        // Special case: Dockerfile
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (name == "Dockerfile" || name.starts_with("Dockerfile."))
        {
            return Language::Dockerfile;
        }

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(Self::from_extension)
            .unwrap_or(Language::Unknown)
    }

    /// Get the canonical name for this language.
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::Java => "java",
            Language::Kotlin => "kotlin",
            Language::CSharp => "csharp",
            Language::Cpp => "cpp",
            Language::C => "c",
            Language::Ruby => "ruby",
            Language::Php => "php",
            Language::Swift => "swift",
            Language::Bash => "bash",
            Language::Sql => "sql",
            Language::Hcl => "hcl",
            Language::Yaml => "yaml",
            Language::Toml => "toml",
            Language::Json => "json",
            Language::Html => "html",
            Language::Css => "css",
            Language::Dockerfile => "dockerfile",
            Language::GraphQL => "graphql",
            Language::Dart => "dart",
            Language::Elixir => "elixir",
            Language::Scala => "scala",
            Language::Unknown => "unknown",
        }
    }

    /// Check if this language has tree-sitter grammar support compiled in.
    pub fn has_grammar(&self) -> bool {
        languages::is_supported(*self)
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A symbol extracted from an AST (function, class, method, etc.).
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub language: Language,
    pub file: std::path::PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub visibility: Visibility,
    pub parameters: Vec<String>,
    pub return_type: Option<String>,
}

/// Kinds of symbols that can be extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Module,
    Constant,
    Variable,
    Import,
    Type,
}

/// Visibility of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
    Unknown,
}

/// A call relationship between two symbols.
#[derive(Debug, Clone)]
pub struct CallEdge {
    pub caller: String,
    pub callee: String,
    pub call_site_line: usize,
    pub file: std::path::PathBuf,
}

/// Parse source code and extract symbols.
pub fn extract_symbols(
    source: &str,
    language: Language,
    file: &Path,
) -> Result<Vec<Symbol>, AstError> {
    if !language.has_grammar() {
        // Fallback to regex-based extraction for unsupported languages
        return Ok(regex_extract_symbols(source, language, file));
    }

    // Tree-sitter based extraction (when grammar is compiled in)
    #[cfg(feature = "sast-rust")]
    if language == Language::Rust {
        return languages::extract_rust_symbols(source, file);
    }

    #[cfg(feature = "sast-python")]
    if language == Language::Python {
        return languages::extract_python_symbols(source, file);
    }

    #[cfg(feature = "sast-javascript")]
    if matches!(language, Language::JavaScript | Language::TypeScript) {
        return languages::extract_js_symbols(source, language, file);
    }

    #[cfg(feature = "sast-go")]
    if language == Language::Go {
        return languages::extract_go_symbols(source, file);
    }

    #[cfg(feature = "sast-java")]
    if language == Language::Java {
        return languages::extract_java_symbols(source, file);
    }

    // Fall back to regex for any unsupported combo
    Ok(regex_extract_symbols(source, language, file))
}

/// Regex-based symbol extraction fallback for languages without tree-sitter grammars.
fn regex_extract_symbols(source: &str, language: Language, file: &Path) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    // Language-agnostic function detection patterns
    let patterns: &[(&str, SymbolKind)] = match language {
        Language::Rust => &[
            (
                r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)",
                SymbolKind::Function,
            ),
            (r"(?m)^\s*(?:pub\s+)?struct\s+(\w+)", SymbolKind::Struct),
            (r"(?m)^\s*(?:pub\s+)?enum\s+(\w+)", SymbolKind::Enum),
            (r"(?m)^\s*(?:pub\s+)?trait\s+(\w+)", SymbolKind::Trait),
        ],
        Language::Python => &[
            (r"(?m)^\s*(?:async\s+)?def\s+(\w+)", SymbolKind::Function),
            (r"(?m)^\s*class\s+(\w+)", SymbolKind::Class),
        ],
        Language::JavaScript | Language::TypeScript => &[
            (
                r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)",
                SymbolKind::Function,
            ),
            (r"(?m)^\s*(?:export\s+)?class\s+(\w+)", SymbolKind::Class),
        ],
        Language::Go => &[
            (
                r"(?m)^func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)",
                SymbolKind::Function,
            ),
            (r"(?m)^type\s+(\w+)\s+struct", SymbolKind::Struct),
            (r"(?m)^type\s+(\w+)\s+interface", SymbolKind::Interface),
        ],
        Language::Java | Language::Kotlin => &[
            (
                r"(?m)\s+(?:public|private|protected)?\s*(?:static\s+)?(?:\w+\s+)(\w+)\s*\(",
                SymbolKind::Method,
            ),
            (r"(?m)^\s*(?:public\s+)?class\s+(\w+)", SymbolKind::Class),
            (
                r"(?m)^\s*(?:public\s+)?interface\s+(\w+)",
                SymbolKind::Interface,
            ),
        ],
        _ => &[],
    };

    for (pattern_str, kind) in patterns {
        if let Ok(re) = regex::Regex::new(pattern_str) {
            for cap in re.captures_iter(source) {
                if let Some(name_match) = cap.get(1) {
                    let line = source[..name_match.start()].lines().count();
                    symbols.push(Symbol {
                        name: name_match.as_str().to_string(),
                        kind: *kind,
                        language,
                        file: file.to_path_buf(),
                        start_line: line,
                        end_line: line,
                        visibility: Visibility::Unknown,
                        parameters: Vec::new(),
                        return_type: None,
                    });
                }
            }
        }
    }

    symbols
}

/// Calculate cyclomatic complexity for a function body.
pub fn cyclomatic_complexity(source: &str, language: Language) -> u32 {
    let mut complexity: u32 = 1; // Base complexity

    let keywords = match language {
        Language::Rust => vec![
            "if ", "else if", "while ", "for ", "loop ", "match ", "&&", "||", "?",
        ],
        Language::Python => vec!["if ", "elif ", "while ", "for ", "and ", "or ", "except "],
        Language::JavaScript | Language::TypeScript => vec![
            "if ", "else if", "while ", "for ", "switch ", "case ", "&&", "||", "??", "?.",
        ],
        Language::Go => vec![
            "if ", "else if", "for ", "switch ", "case ", "select ", "&&", "||",
        ],
        Language::Java | Language::Kotlin | Language::CSharp => vec![
            "if ", "else if", "while ", "for ", "switch ", "case ", "catch ", "&&", "||",
        ],
        _ => vec!["if ", "else if", "while ", "for ", "&&", "||"],
    };

    for keyword in keywords {
        complexity += source.matches(keyword).count() as u32;
    }

    complexity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("js"), Language::JavaScript);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("java"), Language::Java);
        assert_eq!(Language::from_extension("xyz"), Language::Unknown);
    }

    #[test]
    fn test_language_from_path() {
        assert_eq!(
            Language::from_path(Path::new("src/main.rs")),
            Language::Rust
        );
        assert_eq!(
            Language::from_path(Path::new("Dockerfile")),
            Language::Dockerfile
        );
        assert_eq!(
            Language::from_path(Path::new("Dockerfile.prod")),
            Language::Dockerfile
        );
    }

    #[test]
    fn test_regex_symbol_extraction() {
        let source = r#"
pub fn hello_world() {
    println!("Hello");
}

pub struct MyStruct {
    field: i32,
}

pub enum MyEnum {
    A,
    B,
}
"#;
        let symbols = regex_extract_symbols(source, Language::Rust, Path::new("test.rs"));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "hello_world" && s.kind == SymbolKind::Function)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyStruct" && s.kind == SymbolKind::Struct)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "MyEnum" && s.kind == SymbolKind::Enum)
        );
    }

    #[test]
    fn test_cyclomatic_complexity() {
        let simple_fn = "fn foo() { return 42; }";
        assert_eq!(cyclomatic_complexity(simple_fn, Language::Rust), 1);

        let complex_fn = "fn foo() { if a && b { while c { if d || e { } } } }";
        assert!(cyclomatic_complexity(complex_fn, Language::Rust) > 3);
    }
}
