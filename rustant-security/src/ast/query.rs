//! S-expression query interface for AST pattern matching.
//!
//! Provides a simplified query interface for matching tree-sitter AST patterns.
//! Used by SAST scanners to define vulnerability detection rules.

use super::Language;
use crate::error::AstError;
use std::path::Path;

/// A query match result.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// The capture name (e.g., "@func", "@args").
    pub capture_name: String,
    /// The matched text.
    pub text: String,
    /// Start line (1-indexed).
    pub start_line: usize,
    /// End line (1-indexed).
    pub end_line: usize,
    /// Start column (0-indexed).
    pub start_col: usize,
    /// End column (0-indexed).
    pub end_col: usize,
}

/// A pattern match result grouping related captures.
#[derive(Debug, Clone)]
pub struct PatternMatch {
    /// All captures for this match.
    pub captures: Vec<QueryMatch>,
    /// The file where the match was found.
    pub file: std::path::PathBuf,
    /// The full matched text span.
    pub matched_text: String,
    /// Start line of the overall match.
    pub start_line: usize,
    /// End line of the overall match.
    pub end_line: usize,
}

/// A compiled AST query ready for execution.
pub struct AstQuery {
    /// The S-expression pattern string.
    pattern: String,
    /// Target language.
    language: Language,
}

impl AstQuery {
    /// Create a new query from an S-expression pattern.
    ///
    /// The pattern follows tree-sitter S-expression syntax:
    /// ```text
    /// (call_expression
    ///   function: (identifier) @func
    ///   arguments: (argument_list) @args)
    /// ```
    pub fn new(pattern: &str, language: Language) -> Self {
        Self {
            pattern: pattern.to_string(),
            language,
        }
    }

    /// Execute this query against source code.
    pub fn execute(&self, source: &str, file: &Path) -> Result<Vec<PatternMatch>, AstError> {
        if !self.language.has_grammar() {
            // Fallback to regex-based matching for unsupported languages
            return self.regex_fallback(source, file);
        }

        #[cfg(feature = "sast-rust")]
        if self.language == Language::Rust {
            return self.execute_tree_sitter(source, file, &tree_sitter_rust::LANGUAGE.into());
        }

        #[cfg(feature = "sast-python")]
        if self.language == Language::Python {
            return self.execute_tree_sitter(source, file, &tree_sitter_python::LANGUAGE.into());
        }

        #[cfg(feature = "sast-javascript")]
        if self.language == Language::JavaScript {
            return self.execute_tree_sitter(
                source,
                file,
                &tree_sitter_javascript::LANGUAGE.into(),
            );
        }

        #[cfg(feature = "sast-javascript")]
        if self.language == Language::TypeScript {
            return self.execute_tree_sitter(
                source,
                file,
                &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            );
        }

        #[cfg(feature = "sast-go")]
        if self.language == Language::Go {
            return self.execute_tree_sitter(source, file, &tree_sitter_go::LANGUAGE.into());
        }

        #[cfg(feature = "sast-java")]
        if self.language == Language::Java {
            return self.execute_tree_sitter(source, file, &tree_sitter_java::LANGUAGE.into());
        }

        self.regex_fallback(source, file)
    }

    /// Execute query using tree-sitter.
    fn execute_tree_sitter(
        &self,
        source: &str,
        file: &Path,
        ts_language: &tree_sitter::Language,
    ) -> Result<Vec<PatternMatch>, AstError> {
        use streaming_iterator::StreamingIterator;

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(ts_language)
            .map_err(|e| AstError::ParseError {
                file: file.display().to_string(),
                message: format!("Failed to set language: {e}"),
            })?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| AstError::ParseError {
                file: file.display().to_string(),
                message: "Failed to parse source".into(),
            })?;

        let query = tree_sitter::Query::new(ts_language, &self.pattern).map_err(|e| {
            AstError::QueryError {
                pattern: self.pattern.clone(),
                message: format!("Invalid query pattern: {e}"),
            }
        })?;

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

        let capture_names = query.capture_names();

        let mut results = Vec::new();

        while let Some(m) = matches.next() {
            let mut captures = Vec::new();
            let mut match_start = usize::MAX;
            let mut match_end = 0;

            for capture in m.captures {
                let node = capture.node;
                let text = &source[node.byte_range()];
                let name = capture_names
                    .get(capture.index as usize)
                    .copied()
                    .unwrap_or("unknown");

                let start_line = node.start_position().row + 1;
                let end_line = node.end_position().row + 1;

                match_start = match_start.min(start_line);
                match_end = match_end.max(end_line);

                captures.push(QueryMatch {
                    capture_name: name.to_string(),
                    text: text.to_string(),
                    start_line,
                    end_line,
                    start_col: node.start_position().column,
                    end_col: node.end_position().column,
                });
            }

            if !captures.is_empty() {
                let matched_text = captures
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                results.push(PatternMatch {
                    captures,
                    file: file.to_path_buf(),
                    matched_text,
                    start_line: match_start,
                    end_line: match_end,
                });
            }
        }

        Ok(results)
    }

    /// Regex-based fallback for languages without tree-sitter support.
    fn regex_fallback(&self, _source: &str, _file: &Path) -> Result<Vec<PatternMatch>, AstError> {
        // S-expression patterns cannot be directly translated to regex.
        // Return empty results for unsupported languages.
        Ok(Vec::new())
    }
}

/// Convenience function to find all function calls matching a name pattern.
pub fn find_calls(
    source: &str,
    language: Language,
    file: &Path,
    function_pattern: &str,
) -> Vec<PatternMatch> {
    let query_str = match language {
        Language::Rust => format!(
            r#"(call_expression function: (identifier) @func (#match? @func "{function_pattern}"))"#
        ),
        Language::Python => {
            format!(r#"(call function: (identifier) @func (#match? @func "{function_pattern}"))"#)
        }
        Language::JavaScript | Language::TypeScript => format!(
            r#"(call_expression function: (identifier) @func (#match? @func "{function_pattern}"))"#
        ),
        Language::Go => format!(
            r#"(call_expression function: (identifier) @func (#match? @func "{function_pattern}"))"#
        ),
        Language::Java => format!(
            r#"(method_invocation name: (identifier) @func (#match? @func "{function_pattern}"))"#
        ),
        _ => return Vec::new(),
    };

    let query = AstQuery::new(&query_str, language);
    query.execute(source, file).unwrap_or_default()
}

/// Find all string literals in source code.
pub fn find_string_literals(source: &str, language: Language, file: &Path) -> Vec<PatternMatch> {
    let query_str = match language {
        Language::Rust => "(string_literal) @str",
        Language::Python => "(string) @str",
        Language::JavaScript | Language::TypeScript => "(string) @str",
        Language::Go => "(interpreted_string_literal) @str",
        Language::Java => "(string_literal) @str",
        _ => return Vec::new(),
    };

    let query = AstQuery::new(query_str, language);
    query.execute(source, file).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_query_creation() {
        let query = AstQuery::new("(function_item name: (identifier) @name)", Language::Rust);
        assert_eq!(query.language, Language::Rust);
        assert!(!query.pattern.is_empty());
    }

    #[test]
    fn test_regex_fallback_returns_empty() {
        let query = AstQuery::new("(some_pattern) @cap", Language::Unknown);
        let result = query.execute("fn main() {}", Path::new("test.rs"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[cfg(feature = "sast-rust")]
    #[test]
    fn test_rust_query_function_items() {
        let source = r#"
fn hello() {
    println!("Hello");
}

fn world() {
    println!("World");
}
"#;
        let query = AstQuery::new("(function_item name: (identifier) @name)", Language::Rust);
        let results = query.execute(source, Path::new("test.rs")).unwrap();
        assert_eq!(results.len(), 2);

        let names: Vec<&str> = results
            .iter()
            .flat_map(|m| m.captures.iter())
            .filter(|c| c.capture_name == "name")
            .map(|c| c.text.as_str())
            .collect();
        assert!(names.contains(&"hello"));
        assert!(names.contains(&"world"));
    }

    #[cfg(feature = "sast-rust")]
    #[test]
    fn test_rust_query_unsafe_blocks() {
        let source = r#"
fn safe_fn() {
    let x = 1;
}

fn unsafe_fn() {
    unsafe {
        std::ptr::null::<i32>();
    }
}
"#;
        let query = AstQuery::new("(unsafe_block) @unsafe", Language::Rust);
        let results = query.execute(source, Path::new("test.rs")).unwrap();
        assert_eq!(results.len(), 1);
    }
}
