//! Technical debt tracker — detects, scores, and tracks debt indicators.
//!
//! Identifies TODO/FIXME/HACK comments, high-complexity functions,
//! duplicated logic, deprecated API usage, and missing tests.
//! Persists to `.rustant/security/tech_debt.json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A single technical debt indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtIndicator {
    /// File containing the debt.
    pub file: PathBuf,
    /// Line number (1-indexed).
    pub line: usize,
    /// Category of debt.
    pub category: DebtCategory,
    /// Description of the debt item.
    pub description: String,
    /// Severity/weight (1-10). Higher = more impactful debt.
    pub weight: u8,
    /// The raw text that triggered detection (e.g., the TODO comment).
    pub source_text: Option<String>,
}

/// Category of technical debt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DebtCategory {
    /// TODO/FIXME/HACK/XXX/OPTIMIZE comments.
    MarkerComment,
    /// Function with high cyclomatic complexity.
    HighComplexity,
    /// Duplicated code blocks.
    Duplication,
    /// Deprecated API or pattern usage.
    DeprecatedUsage,
    /// Missing tests for complex code.
    MissingTests,
    /// Outdated dependency.
    OutdatedDependency,
    /// Large file (>500 lines).
    LargeFile,
    /// Long function (>50 lines).
    LongFunction,
}

impl std::fmt::Display for DebtCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DebtCategory::MarkerComment => write!(f, "marker comment"),
            DebtCategory::HighComplexity => write!(f, "high complexity"),
            DebtCategory::Duplication => write!(f, "duplication"),
            DebtCategory::DeprecatedUsage => write!(f, "deprecated usage"),
            DebtCategory::MissingTests => write!(f, "missing tests"),
            DebtCategory::OutdatedDependency => write!(f, "outdated dependency"),
            DebtCategory::LargeFile => write!(f, "large file"),
            DebtCategory::LongFunction => write!(f, "long function"),
        }
    }
}

/// Result of a tech debt analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechDebtReport {
    /// All detected debt indicators.
    pub indicators: Vec<DebtIndicator>,
    /// Total debt score (sum of weights).
    pub total_score: u32,
    /// Breakdown by category.
    pub category_counts: Vec<(String, usize)>,
    /// Files with the most debt (hotspots).
    pub hotspots: Vec<(PathBuf, u32)>,
}

impl TechDebtReport {
    /// Build a report from indicators.
    pub fn from_indicators(indicators: Vec<DebtIndicator>) -> Self {
        let total_score: u32 = indicators.iter().map(|i| i.weight as u32).sum();

        // Category breakdown
        let mut cat_map = std::collections::HashMap::new();
        for ind in &indicators {
            *cat_map.entry(ind.category.to_string()).or_insert(0usize) += 1;
        }
        let mut category_counts: Vec<(String, usize)> = cat_map.into_iter().collect();
        category_counts.sort_by(|a, b| b.1.cmp(&a.1));

        // Hotspots
        let mut file_scores: std::collections::HashMap<PathBuf, u32> =
            std::collections::HashMap::new();
        for ind in &indicators {
            *file_scores.entry(ind.file.clone()).or_insert(0) += ind.weight as u32;
        }
        let mut hotspots: Vec<(PathBuf, u32)> = file_scores.into_iter().collect();
        hotspots.sort_by(|a, b| b.1.cmp(&a.1));

        Self {
            indicators,
            total_score,
            category_counts,
            hotspots,
        }
    }

    /// Get indicators for a specific file.
    pub fn indicators_for_file(&self, path: &Path) -> Vec<&DebtIndicator> {
        self.indicators.iter().filter(|i| i.file == path).collect()
    }

    /// Get indicators by category.
    pub fn indicators_by_category(&self, category: DebtCategory) -> Vec<&DebtIndicator> {
        self.indicators
            .iter()
            .filter(|i| i.category == category)
            .collect()
    }
}

/// Technical debt scanner for source code.
pub struct TechDebtScanner {
    /// Patterns to detect marker comments.
    marker_patterns: Vec<(&'static str, u8)>,
    /// Threshold for "large file" (lines).
    large_file_threshold: usize,
    /// Threshold for "long function" (lines).
    long_function_threshold: usize,
    /// Threshold for "high complexity" (cyclomatic).
    /// Used by external callers integrating with QualityScorer.
    pub high_complexity_threshold: usize,
}

impl Default for TechDebtScanner {
    fn default() -> Self {
        Self {
            marker_patterns: vec![
                ("TODO", 3),
                ("FIXME", 5),
                ("HACK", 6),
                ("XXX", 4),
                ("OPTIMIZE", 3),
                ("WORKAROUND", 5),
                ("TEMP", 4),
                ("DEPRECATED", 4),
            ],
            large_file_threshold: 500,
            long_function_threshold: 50,
            high_complexity_threshold: 15,
        }
    }
}

impl TechDebtScanner {
    /// Create a scanner with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Scan a source file for debt indicators.
    pub fn scan_file(&self, source: &str, file: &Path) -> Vec<DebtIndicator> {
        let mut indicators = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Check for large file
        if lines.len() > self.large_file_threshold {
            indicators.push(DebtIndicator {
                file: file.to_path_buf(),
                line: 1,
                category: DebtCategory::LargeFile,
                description: format!(
                    "File has {} lines (threshold: {})",
                    lines.len(),
                    self.large_file_threshold
                ),
                weight: 4,
                source_text: None,
            });
        }

        // Scan for marker comments
        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            for &(marker, weight) in &self.marker_patterns {
                if contains_marker(trimmed, marker) {
                    indicators.push(DebtIndicator {
                        file: file.to_path_buf(),
                        line: line_num + 1,
                        category: DebtCategory::MarkerComment,
                        description: format!("{marker} comment found"),
                        weight,
                        source_text: Some(trimmed.to_string()),
                    });
                }
            }

            // Check for deprecated attribute patterns
            if trimmed.starts_with("#[deprecated")
                || trimmed.starts_with("@Deprecated")
                || trimmed.starts_with("@deprecated")
            {
                indicators.push(DebtIndicator {
                    file: file.to_path_buf(),
                    line: line_num + 1,
                    category: DebtCategory::DeprecatedUsage,
                    description: "Deprecated item".to_string(),
                    weight: 4,
                    source_text: Some(trimmed.to_string()),
                });
            }
        }

        // Detect long functions using simple brace counting
        indicators.extend(self.detect_long_functions(source, file));

        indicators
    }

    /// Detect long functions via brace matching.
    fn detect_long_functions(&self, source: &str, file: &Path) -> Vec<DebtIndicator> {
        let mut indicators = Vec::new();
        let lines: Vec<&str> = source.lines().collect();
        let mut brace_depth = 0i32;
        let mut fn_start: Option<(usize, String)> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Simple function detection
            if brace_depth == 0 && is_function_declaration(trimmed) {
                let name = extract_function_name(trimmed);
                fn_start = Some((i + 1, name));
            }

            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            if let Some((start, ref name)) = fn_start {
                                let length = (i + 1) - start + 1;
                                if length > self.long_function_threshold {
                                    indicators.push(DebtIndicator {
                                        file: file.to_path_buf(),
                                        line: start,
                                        category: DebtCategory::LongFunction,
                                        description: format!(
                                            "Function '{}' is {} lines (threshold: {})",
                                            name, length, self.long_function_threshold
                                        ),
                                        weight: 5,
                                        source_text: None,
                                    });
                                }
                            }
                            fn_start = None;
                        }
                    }
                    _ => {}
                }
            }
        }

        indicators
    }
}

/// Check if a line contains a marker comment (case-insensitive for the marker).
fn contains_marker(line: &str, marker: &str) -> bool {
    // Look for markers in comments
    let comment_start = line
        .find("//")
        .or_else(|| line.find('#'))
        .or_else(|| line.find("/*"));

    if let Some(pos) = comment_start {
        let comment_text = &line[pos..];
        comment_text.to_uppercase().contains(marker)
    } else {
        false
    }
}

/// Check if a line looks like a function declaration.
fn is_function_declaration(trimmed: &str) -> bool {
    // Rust: fn name(
    // Python: def name(
    // JS/TS: function name( or name(
    // Java/Go: func name( or type name(
    trimmed.starts_with("fn ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("pub(crate) fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("func ")
}

/// Extract function name from a declaration line.
fn extract_function_name(trimmed: &str) -> String {
    // Try to find pattern: keyword name(
    let search_after = if let Some(pos) = trimmed.find("fn ") {
        pos + 3
    } else if let Some(pos) = trimmed.find("def ") {
        pos + 4
    } else if let Some(pos) = trimmed.find("function ") {
        pos + 9
    } else if let Some(pos) = trimmed.find("func ") {
        pos + 5
    } else {
        0
    };

    let rest = &trimmed[search_after..];
    rest.split(|c: char| c == '(' || c == '<' || c.is_whitespace())
        .next()
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marker_comment_detection() {
        let scanner = TechDebtScanner::new();
        let source = r#"fn main() {
    // TODO: fix this later
    let x = 1;
    // FIXME: broken logic
    let y = x + 1;
    // This is a normal comment
    println!("{}", y);
}"#;

        let indicators = scanner.scan_file(source, Path::new("test.rs"));
        let markers: Vec<_> = indicators
            .iter()
            .filter(|i| i.category == DebtCategory::MarkerComment)
            .collect();

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].line, 2);
        assert_eq!(markers[1].line, 4);
    }

    #[test]
    fn test_large_file_detection() {
        let scanner = TechDebtScanner {
            large_file_threshold: 10,
            ..Default::default()
        };
        let source = (0..15)
            .map(|i| format!("let x{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");
        let indicators = scanner.scan_file(&source, Path::new("test.rs"));
        let large: Vec<_> = indicators
            .iter()
            .filter(|i| i.category == DebtCategory::LargeFile)
            .collect();
        assert_eq!(large.len(), 1);
    }

    #[test]
    fn test_long_function_detection() {
        let scanner = TechDebtScanner {
            long_function_threshold: 5,
            ..Default::default()
        };

        let mut source = String::from("fn long_function() {\n");
        for i in 0..10 {
            source.push_str(&format!("    let x{i} = {i};\n"));
        }
        source.push_str("}\n");

        let indicators = scanner.scan_file(&source, Path::new("test.rs"));
        let long: Vec<_> = indicators
            .iter()
            .filter(|i| i.category == DebtCategory::LongFunction)
            .collect();
        assert_eq!(long.len(), 1);
        assert!(long[0].description.contains("long_function"));
    }

    #[test]
    fn test_deprecated_detection() {
        let scanner = TechDebtScanner::new();
        let source = "#[deprecated]\nfn old() {}\n";
        let indicators = scanner.scan_file(source, Path::new("test.rs"));
        let deprecated: Vec<_> = indicators
            .iter()
            .filter(|i| i.category == DebtCategory::DeprecatedUsage)
            .collect();
        assert_eq!(deprecated.len(), 1);
    }

    #[test]
    fn test_report_from_indicators() {
        let indicators = vec![
            DebtIndicator {
                file: PathBuf::from("a.rs"),
                line: 1,
                category: DebtCategory::MarkerComment,
                description: "TODO found".into(),
                weight: 3,
                source_text: None,
            },
            DebtIndicator {
                file: PathBuf::from("a.rs"),
                line: 10,
                category: DebtCategory::HighComplexity,
                description: "Complex function".into(),
                weight: 7,
                source_text: None,
            },
            DebtIndicator {
                file: PathBuf::from("b.rs"),
                line: 5,
                category: DebtCategory::MarkerComment,
                description: "FIXME found".into(),
                weight: 5,
                source_text: None,
            },
        ];

        let report = TechDebtReport::from_indicators(indicators);
        assert_eq!(report.total_score, 15);
        assert_eq!(report.hotspots[0].0, PathBuf::from("a.rs"));
        assert_eq!(report.hotspots[0].1, 10);
        assert_eq!(
            report
                .indicators_by_category(DebtCategory::MarkerComment)
                .len(),
            2
        );
    }

    #[test]
    fn test_extract_function_name() {
        assert_eq!(extract_function_name("fn main() {"), "main");
        assert_eq!(extract_function_name("pub fn helper(x: i32) {"), "helper");
        assert_eq!(extract_function_name("def process(self):"), "process");
        assert_eq!(extract_function_name("function doWork() {"), "doWork");
    }

    #[test]
    fn test_contains_marker() {
        assert!(contains_marker("// TODO: fix this", "TODO"));
        assert!(contains_marker("# FIXME broken", "FIXME"));
        assert!(contains_marker("/* HACK */", "HACK"));
        assert!(!contains_marker("let todo = 1;", "TODO"));
    }
}
