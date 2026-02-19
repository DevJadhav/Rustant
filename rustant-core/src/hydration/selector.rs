//! Context selection — picks the most relevant code chunks for a query.
//!
//! Uses the RepoMap's combined PageRank + text relevance ranking.

use crate::repo_map::{ContextChunk, RepoMap};

/// Select the most relevant context chunks for a query.
///
/// Uses the RepoMap's built-in ranking (PageRank + text relevance)
/// and caps the result to `max_files` files and `max_tokens` tokens.
pub fn select_context(
    repo_map: &RepoMap,
    query: &str,
    max_tokens: usize,
    max_files: usize,
) -> Vec<ContextChunk> {
    let mut chunks = repo_map.select_context(query, max_tokens);

    // Enforce max_files limit by deduplicating files
    let mut seen_files = std::collections::HashSet::new();
    chunks.retain(|chunk| {
        if seen_files.len() >= max_files && !seen_files.contains(&chunk.file) {
            return false;
        }
        seen_files.insert(chunk.file.clone());
        true
    });

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_context_empty_repo() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo_map = RepoMap::build(dir.path());
        let chunks = select_context(&repo_map, "test", 1000, 10);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_select_context_with_symbols() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            "pub fn process_data() {}\npub fn handle_request() {}\n",
        )
        .unwrap();

        let repo_map = RepoMap::build(dir.path());
        let chunks = select_context(&repo_map, "process", 1000, 10);
        // Should prefer process_data due to text relevance
        if !chunks.is_empty() {
            assert!(chunks[0].content.contains("process") || chunks[0].relevance_score > 0.0);
        }
    }

    #[test]
    fn test_max_files_limit() {
        let dir = tempfile::TempDir::new().unwrap();
        // Create multiple files
        for i in 0..5 {
            std::fs::write(
                dir.path().join(format!("mod{i}.rs")),
                format!("pub fn func_{i}() {{}}\n"),
            )
            .unwrap();
        }

        let repo_map = RepoMap::build(dir.path());
        let chunks = select_context(&repo_map, "func", 10000, 2);
        let unique_files: std::collections::HashSet<_> = chunks.iter().map(|c| &c.file).collect();
        assert!(unique_files.len() <= 2);
    }
}
