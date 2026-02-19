//! Context assembler — formats selected chunks into an LLM context addendum.

use crate::repo_map::ContextChunk;
use std::path::Path;

/// Assemble context chunks into a formatted string for LLM injection.
///
/// Groups chunks by file and formats them with file paths and line numbers.
pub fn assemble_context(chunks: &[ContextChunk], workspace: &Path) -> String {
    if chunks.is_empty() {
        return String::new();
    }

    let workspace_str = workspace.to_string_lossy();

    // Group by file
    let mut by_file: std::collections::BTreeMap<&str, Vec<&ContextChunk>> =
        std::collections::BTreeMap::new();
    for chunk in chunks {
        by_file.entry(&chunk.file).or_default().push(chunk);
    }

    let mut output = String::from("## Relevant Code Context\n\n");

    for (file, file_chunks) in &by_file {
        // Strip workspace prefix for cleaner display
        let display_path = file
            .strip_prefix(&*workspace_str)
            .unwrap_or(file)
            .trim_start_matches('/');

        output.push_str(&format!("### {display_path}\n```\n"));

        for chunk in file_chunks {
            if chunk.start_line > 0 {
                output.push_str(&format!("// L{}-{}\n", chunk.start_line, chunk.end_line));
            }
            output.push_str(&chunk.content);
            output.push('\n');
        }

        output.push_str("```\n\n");
    }

    output
}

/// Read the full content of a file and extract lines around a given range.
pub fn read_context_window(
    file_path: &Path,
    start_line: usize,
    end_line: usize,
    context_lines: usize,
) -> Option<String> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    let start = start_line.saturating_sub(context_lines + 1);
    let end = (end_line + context_lines).min(lines.len());

    Some(
        lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4} | {}", start + i + 1, line))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_empty() {
        let result = assemble_context(&[], Path::new("/tmp"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_assemble_single_chunk() {
        let chunks = vec![ContextChunk {
            file: "/tmp/project/src/main.rs".into(),
            start_line: 10,
            end_line: 15,
            content: "pub fn main() {}".into(),
            relevance_score: 0.9,
        }];

        let result = assemble_context(&chunks, Path::new("/tmp/project"));
        assert!(result.contains("Relevant Code Context"));
        assert!(result.contains("main.rs"));
        assert!(result.contains("pub fn main()"));
    }

    #[test]
    fn test_assemble_multiple_files() {
        let chunks = vec![
            ContextChunk {
                file: "a.rs".into(),
                start_line: 1,
                end_line: 3,
                content: "fn a() {}".into(),
                relevance_score: 0.8,
            },
            ContextChunk {
                file: "b.rs".into(),
                start_line: 1,
                end_line: 2,
                content: "fn b() {}".into(),
                relevance_score: 0.5,
            },
        ];

        let result = assemble_context(&chunks, Path::new("/workspace"));
        assert!(result.contains("a.rs"));
        assert!(result.contains("b.rs"));
    }

    #[test]
    fn test_read_context_window() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(
            &file,
            "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\n",
        )
        .unwrap();

        let result = read_context_window(&file, 3, 5, 1);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("line 2"));
        assert!(text.contains("line 3"));
        assert!(text.contains("line 5"));
        assert!(text.contains("line 6"));
    }
}
