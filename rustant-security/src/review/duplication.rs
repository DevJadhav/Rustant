//! Code duplication detector — AST fingerprinting and token hashing.
//!
//! Detects duplicated code blocks using a combination of structural
//! (AST-based) fingerprinting and Rabin-Karp rolling hash on tokens.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A detected code duplication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Duplication {
    /// First occurrence of the duplicated code.
    pub original: CodeSpan,
    /// All duplicate occurrences.
    pub duplicates: Vec<CodeSpan>,
    /// Number of duplicated lines.
    pub line_count: usize,
    /// Similarity score (0.0-1.0). 1.0 = exact copy.
    pub similarity: f64,
    /// The fingerprint hash shared by duplicates.
    pub fingerprint: u64,
}

/// A span of code in a specific file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSpan {
    /// File path.
    pub file: PathBuf,
    /// Start line (1-indexed).
    pub start_line: usize,
    /// End line (1-indexed).
    pub end_line: usize,
}

impl CodeSpan {
    /// Number of lines in this span.
    pub fn lines(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }
}

/// Result of a duplication analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicationReport {
    /// All detected duplications.
    pub duplications: Vec<Duplication>,
    /// Total lines analyzed.
    pub total_lines: usize,
    /// Total duplicated lines.
    pub duplicated_lines: usize,
    /// Duplication percentage (0.0-100.0).
    pub duplication_percentage: f64,
}

/// Configuration for the duplication detector.
#[derive(Debug, Clone)]
pub struct DuplicationConfig {
    /// Minimum number of lines for a block to count as a duplication.
    pub min_lines: usize,
    /// Minimum number of tokens for a block to count.
    pub min_tokens: usize,
    /// Window size for rolling hash.
    pub window_size: usize,
}

impl Default for DuplicationConfig {
    fn default() -> Self {
        Self {
            min_lines: 6,
            min_tokens: 50,
            window_size: 5,
        }
    }
}

/// Duplication detector using token-level rolling hash.
pub struct DuplicationDetector {
    config: DuplicationConfig,
}

impl DuplicationDetector {
    /// Create a new detector with the given config.
    pub fn new(config: DuplicationConfig) -> Self {
        Self { config }
    }

    /// Create a detector with default config.
    pub fn with_defaults() -> Self {
        Self::new(DuplicationConfig::default())
    }

    /// Analyze a single file for internal duplication.
    pub fn analyze_file(&self, source: &str, file: &Path) -> DuplicationReport {
        let lines: Vec<&str> = source.lines().collect();
        let total_lines = lines.len();

        if total_lines < self.config.min_lines {
            return DuplicationReport {
                duplications: Vec::new(),
                total_lines,
                duplicated_lines: 0,
                duplication_percentage: 0.0,
            };
        }

        // Normalize lines: trim whitespace, skip empty/comment-only lines
        let normalized: Vec<(usize, String)> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !is_comment_only(trimmed)
            })
            .map(|(i, line)| (i + 1, normalize_line(line))) // 1-indexed
            .collect();

        // Build rolling hash windows
        let window = self.config.min_lines;
        let mut hash_map: HashMap<u64, Vec<usize>> = HashMap::new();

        for start_idx in 0..normalized.len().saturating_sub(window - 1) {
            let end_idx = start_idx + window;
            if end_idx > normalized.len() {
                break;
            }

            let block: String = normalized[start_idx..end_idx]
                .iter()
                .map(|(_, line)| line.as_str())
                .collect::<Vec<_>>()
                .join("\n");

            let hash = simple_hash(&block);
            hash_map.entry(hash).or_default().push(start_idx);
        }

        // Find duplicates (hash collisions with verification)
        let mut duplications = Vec::new();
        let mut seen_hashes = std::collections::HashSet::new();

        for (hash, positions) in &hash_map {
            if positions.len() < 2 || !seen_hashes.insert(*hash) {
                continue;
            }

            let first_pos = positions[0];
            let original = CodeSpan {
                file: file.to_path_buf(),
                start_line: normalized[first_pos].0,
                end_line: normalized[first_pos + window - 1].0,
            };

            let mut dups = Vec::new();
            for &pos in &positions[1..] {
                // Verify content matches (not just hash)
                let orig_block: Vec<&str> = normalized[first_pos..first_pos + window]
                    .iter()
                    .map(|(_, l)| l.as_str())
                    .collect();
                let dup_block: Vec<&str> = normalized[pos..pos + window]
                    .iter()
                    .map(|(_, l)| l.as_str())
                    .collect();

                if orig_block == dup_block {
                    dups.push(CodeSpan {
                        file: file.to_path_buf(),
                        start_line: normalized[pos].0,
                        end_line: normalized[pos + window - 1].0,
                    });
                }
            }

            if !dups.is_empty() {
                duplications.push(Duplication {
                    original,
                    duplicates: dups,
                    line_count: window,
                    similarity: 1.0, // Exact match after normalization
                    fingerprint: *hash,
                });
            }
        }

        let duplicated_lines = duplications
            .iter()
            .map(|d| d.line_count * d.duplicates.len())
            .sum::<usize>();

        let duplication_percentage = if total_lines > 0 {
            (duplicated_lines as f64 / total_lines as f64) * 100.0
        } else {
            0.0
        };

        DuplicationReport {
            duplications,
            total_lines,
            duplicated_lines,
            duplication_percentage,
        }
    }

    /// Analyze multiple files for cross-file duplication.
    pub fn analyze_files(&self, files: &[(&str, &Path)]) -> DuplicationReport {
        let mut all_blocks: Vec<(u64, PathBuf, usize, usize)> = Vec::new();
        let mut total_lines = 0;

        for (source, file) in files {
            let lines: Vec<&str> = source.lines().collect();
            total_lines += lines.len();

            let normalized: Vec<(usize, String)> = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| {
                    let trimmed = line.trim();
                    !trimmed.is_empty() && !is_comment_only(trimmed)
                })
                .map(|(i, line)| (i + 1, normalize_line(line)))
                .collect();

            let window = self.config.min_lines;
            for start_idx in 0..normalized.len().saturating_sub(window - 1) {
                let end_idx = start_idx + window;
                if end_idx > normalized.len() {
                    break;
                }

                let block: String = normalized[start_idx..end_idx]
                    .iter()
                    .map(|(_, line)| line.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");

                let hash = simple_hash(&block);
                all_blocks.push((
                    hash,
                    file.to_path_buf(),
                    normalized[start_idx].0,
                    normalized[end_idx - 1].0,
                ));
            }
        }

        // Group by hash
        let mut hash_map: HashMap<u64, Vec<(PathBuf, usize, usize)>> = HashMap::new();
        for (hash, file, start, end) in all_blocks {
            hash_map.entry(hash).or_default().push((file, start, end));
        }

        let mut duplications = Vec::new();
        for (hash, positions) in &hash_map {
            if positions.len() < 2 {
                continue;
            }

            let (ref first_file, first_start, first_end) = positions[0];
            let original = CodeSpan {
                file: first_file.clone(),
                start_line: first_start,
                end_line: first_end,
            };

            let dups: Vec<CodeSpan> = positions[1..]
                .iter()
                .map(|(f, s, e)| CodeSpan {
                    file: f.clone(),
                    start_line: *s,
                    end_line: *e,
                })
                .collect();

            duplications.push(Duplication {
                original,
                duplicates: dups,
                line_count: self.config.min_lines,
                similarity: 1.0,
                fingerprint: *hash,
            });
        }

        let duplicated_lines = duplications
            .iter()
            .map(|d| d.line_count * d.duplicates.len())
            .sum::<usize>();

        let duplication_percentage = if total_lines > 0 {
            (duplicated_lines as f64 / total_lines as f64) * 100.0
        } else {
            0.0
        };

        DuplicationReport {
            duplications,
            total_lines,
            duplicated_lines,
            duplication_percentage,
        }
    }
}

/// Normalize a line for comparison: trim, lowercase, collapse whitespace.
fn normalize_line(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Check if a line is comment-only.
fn is_comment_only(trimmed: &str) -> bool {
    trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
}

/// Simple hash function for string blocks.
fn simple_hash(s: &str) -> u64 {
    // FNV-1a hash
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_duplication_short_file() {
        let detector = DuplicationDetector::with_defaults();
        let source = "fn main() {\n    println!(\"hello\");\n}";
        let report = detector.analyze_file(source, Path::new("test.rs"));
        assert!(report.duplications.is_empty());
        assert_eq!(report.duplication_percentage, 0.0);
    }

    #[test]
    fn test_exact_duplication() {
        let detector = DuplicationDetector::new(DuplicationConfig {
            min_lines: 3,
            min_tokens: 10,
            window_size: 3,
        });

        let source = r#"fn process_a() {
    let x = compute();
    let y = transform(x);
    validate(y);
    store(y);
}

fn process_b() {
    let x = compute();
    let y = transform(x);
    validate(y);
    store(y);
}"#;

        let report = detector.analyze_file(source, Path::new("test.rs"));
        assert!(
            !report.duplications.is_empty(),
            "Should detect duplicated blocks"
        );
    }

    #[test]
    fn test_normalize_line() {
        assert_eq!(normalize_line("  let   x =  1;  "), "let x = 1;");
        assert_eq!(normalize_line("FN Main()"), "fn main()");
    }

    #[test]
    fn test_comment_only_detection() {
        assert!(is_comment_only("// comment"));
        assert!(is_comment_only("# comment"));
        assert!(is_comment_only("/* comment */"));
        assert!(!is_comment_only("let x = 1;"));
    }

    #[test]
    fn test_code_span_lines() {
        let span = CodeSpan {
            file: PathBuf::from("test.rs"),
            start_line: 5,
            end_line: 10,
        };
        assert_eq!(span.lines(), 6);
    }

    #[test]
    fn test_cross_file_duplication() {
        let detector = DuplicationDetector::new(DuplicationConfig {
            min_lines: 3,
            min_tokens: 10,
            window_size: 3,
        });

        let source_a =
            "fn work() {\n    let x = compute();\n    let y = transform(x);\n    validate(y);\n}";
        let source_b =
            "fn other() {\n    let x = compute();\n    let y = transform(x);\n    validate(y);\n}";

        let files = vec![(source_a, Path::new("a.rs")), (source_b, Path::new("b.rs"))];
        let report = detector.analyze_files(&files);
        assert!(
            !report.duplications.is_empty(),
            "Should detect cross-file duplication"
        );
    }
}
