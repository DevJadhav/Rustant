//! Project Context Auto-Indexer
//!
//! Background workspace indexer that walks the project directory, respects
//! `.gitignore`, extracts file paths, function signatures, and content summaries,
//! then indexes them into the `HybridSearchEngine` for semantic codebase search.

use crate::project_detect::{ProjectInfo, detect_project};
use crate::search::{HybridSearchEngine, SearchConfig, SearchResult};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Maximum file size to index (256 KB).
const MAX_FILE_SIZE: u64 = 256 * 1024;

/// Maximum number of files to index.
const MAX_FILES: usize = 5000;

/// File extensions considered indexable source code.
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs",
    "py",
    "js",
    "ts",
    "jsx",
    "tsx",
    "go",
    "java",
    "rb",
    "c",
    "cpp",
    "cc",
    "h",
    "hpp",
    "cs",
    "swift",
    "kt",
    "scala",
    "lua",
    "sh",
    "bash",
    "zsh",
    "toml",
    "yaml",
    "yml",
    "json",
    "xml",
    "html",
    "css",
    "scss",
    "sql",
    "md",
    "txt",
    "cfg",
    "ini",
    "env",
    "dockerfile",
    "makefile",
];

/// Result of indexing a workspace.
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Number of files indexed.
    pub files_indexed: usize,
    /// Number of entries (facts) written to the search engine.
    pub entries_indexed: usize,
    /// Number of files skipped (too large, binary, etc.).
    pub files_skipped: usize,
    /// Detected project info.
    pub project_info: Option<ProjectInfo>,
}

/// The project context indexer.
pub struct ProjectIndexer {
    workspace: PathBuf,
    engine: HybridSearchEngine,
    config: IndexerConfig,
}

/// Configuration for the indexer.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Maximum file size in bytes to index.
    pub max_file_size: u64,
    /// Maximum number of files to index.
    pub max_files: usize,
    /// Whether to index file content (not just paths).
    pub index_content: bool,
    /// Whether to extract and index function signatures.
    pub index_signatures: bool,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            max_file_size: MAX_FILE_SIZE,
            max_files: MAX_FILES,
            index_content: true,
            index_signatures: true,
        }
    }
}

impl ProjectIndexer {
    /// Create a new indexer for the given workspace.
    pub fn new(
        workspace: PathBuf,
        search_config: SearchConfig,
    ) -> Result<Self, crate::search::SearchError> {
        let engine = HybridSearchEngine::open(search_config)?;
        Ok(Self {
            workspace,
            engine,
            config: IndexerConfig::default(),
        })
    }

    /// Create a new indexer with custom configuration.
    pub fn with_config(
        workspace: PathBuf,
        search_config: SearchConfig,
        config: IndexerConfig,
    ) -> Result<Self, crate::search::SearchError> {
        let engine = HybridSearchEngine::open(search_config)?;
        Ok(Self {
            workspace,
            engine,
            config,
        })
    }

    /// Run the full indexing pass over the workspace.
    /// Returns statistics about what was indexed.
    pub fn index_workspace(&mut self) -> IndexStats {
        let project_info = detect_project(&self.workspace);
        info!(
            "Indexing workspace: {:?} (type: {:?})",
            self.workspace, project_info.project_type
        );

        // Index the project structure summary first
        let structure = self.build_structure_summary(&project_info);
        let _ = self.engine.index_fact("__project_structure__", &structure);

        let mut files_indexed = 0;
        let mut entries_indexed = 1; // structure summary counts as 1
        let mut files_skipped = 0;

        // Walk the workspace respecting .gitignore
        let walker = WalkBuilder::new(&self.workspace)
            .hidden(true) // respect hidden files
            .git_ignore(true) // respect .gitignore
            .git_global(true) // respect global gitignore
            .git_exclude(true) // respect .git/info/exclude
            .max_depth(Some(10))
            .build();

        for entry in walker.flatten() {
            if files_indexed >= self.config.max_files {
                debug!("Reached max files limit ({})", self.config.max_files);
                break;
            }

            let path = entry.path();

            // Skip directories and non-files
            if !path.is_file() {
                continue;
            }

            // Skip files that are too large
            if let Ok(meta) = path.metadata()
                && meta.len() > self.config.max_file_size {
                    files_skipped += 1;
                    continue;
                }

            // Check file extension
            if !is_indexable(path) {
                files_skipped += 1;
                continue;
            }

            // Get relative path
            let rel_path = path
                .strip_prefix(&self.workspace)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            // Index the file path as an entry
            let path_entry = format!("file: {}", rel_path);
            let fact_id = format!("file:{}", rel_path);
            if self.engine.index_fact(&fact_id, &path_entry).is_ok() {
                entries_indexed += 1;
            }

            // Optionally index file content
            if self.config.index_content
                && let Ok(content) = std::fs::read_to_string(path) {
                    // Index a content summary (first N lines + function signatures)
                    let summary = self.summarize_file(&rel_path, &content);
                    if !summary.is_empty() {
                        let content_id = format!("content:{}", rel_path);
                        if self.engine.index_fact(&content_id, &summary).is_ok() {
                            entries_indexed += 1;
                        }
                    }

                    // Extract and index function signatures
                    if self.config.index_signatures {
                        let signatures = extract_signatures(&content, &rel_path);
                        for (i, sig) in signatures.iter().enumerate() {
                            let sig_id = format!("sig:{}:{}", rel_path, i);
                            if self.engine.index_fact(&sig_id, sig).is_ok() {
                                entries_indexed += 1;
                            }
                        }
                    }
                }

            files_indexed += 1;
        }

        info!(
            "Indexing complete: {} files indexed, {} entries, {} skipped",
            files_indexed, entries_indexed, files_skipped
        );

        IndexStats {
            files_indexed,
            entries_indexed,
            files_skipped,
            project_info: Some(project_info),
        }
    }

    /// Search the indexed codebase.
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, crate::search::SearchError> {
        self.engine.search(query)
    }

    /// Get the number of indexed entries.
    pub fn indexed_count(&self) -> usize {
        self.engine.indexed_count()
    }

    /// Get a reference to the underlying search engine.
    pub fn engine(&self) -> &HybridSearchEngine {
        &self.engine
    }

    /// Build a project structure summary for the system prompt.
    pub fn build_structure_summary(&self, info: &ProjectInfo) -> String {
        let mut summary = String::new();

        summary.push_str(&format!("Project type: {:?}\n", info.project_type));

        if let Some(ref framework) = info.framework {
            summary.push_str(&format!("Framework: {}\n", framework));
        }
        if let Some(ref pm) = info.package_manager {
            summary.push_str(&format!("Package manager: {}\n", pm));
        }

        if !info.source_dirs.is_empty() {
            summary.push_str(&format!(
                "Source directories: {}\n",
                info.source_dirs.join(", ")
            ));
        }

        // Add directory tree (top-level)
        summary.push_str("\nTop-level structure:\n");
        if let Ok(entries) = std::fs::read_dir(&self.workspace) {
            let mut dirs: Vec<String> = Vec::new();
            let mut files: Vec<String> = Vec::new();

            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if entry.path().is_dir() {
                    dirs.push(format!("  {}/", name));
                } else {
                    files.push(format!("  {}", name));
                }
            }

            dirs.sort();
            files.sort();

            for d in &dirs {
                summary.push_str(d);
                summary.push('\n');
            }
            for f in &files {
                summary.push_str(f);
                summary.push('\n');
            }
        }

        summary
    }

    /// Summarize a file's content for indexing.
    fn summarize_file(&self, path: &str, content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Take first few lines (imports, module declaration)
        let head: Vec<&str> = lines.iter().take(20).copied().collect();

        // Build summary
        let mut summary = format!("{} ({} lines)\n{}", path, total_lines, head.join("\n"));

        // If file is longer, add a note
        if total_lines > 20 {
            summary.push_str(&format!("\n... ({} more lines)", total_lines - 20));
        }

        summary
    }
}

/// Check if a file is indexable based on its extension.
fn is_indexable(path: &Path) -> bool {
    // Handle files without extension (Makefile, Dockerfile, etc.)
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if ["makefile", "dockerfile", "rakefile", "gemfile", "procfile"].contains(&name.as_str()) {
        return true;
    }

    // Check extension
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Extract function/method/class signatures from source code.
fn extract_signatures(content: &str, path: &str) -> Vec<String> {
    let mut signatures = Vec::new();
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let sig = match ext {
            "rs" => extract_rust_signature(trimmed),
            "py" => extract_python_signature(trimmed),
            "js" | "jsx" | "ts" | "tsx" => extract_js_signature(trimmed),
            "go" => extract_go_signature(trimmed),
            "java" | "kt" | "scala" => extract_java_signature(trimmed),
            "rb" => extract_ruby_signature(trimmed),
            "c" | "cpp" | "cc" | "h" | "hpp" => extract_c_signature(trimmed),
            _ => None,
        };

        if let Some(sig_text) = sig {
            signatures.push(format!("{}:{} {}", path, i + 1, sig_text));
        }
    }

    signatures
}

fn extract_rust_signature(line: &str) -> Option<String> {
    if line.starts_with("pub fn ")
        || line.starts_with("fn ")
        || line.starts_with("pub async fn ")
        || line.starts_with("async fn ")
        || line.starts_with("pub struct ")
        || line.starts_with("struct ")
        || line.starts_with("pub enum ")
        || line.starts_with("enum ")
        || line.starts_with("pub trait ")
        || line.starts_with("trait ")
        || line.starts_with("impl ")
        || line.starts_with("pub mod ")
        || line.starts_with("mod ")
    {
        Some(line.trim_end_matches('{').trim().to_string())
    } else {
        None
    }
}

fn extract_python_signature(line: &str) -> Option<String> {
    if line.starts_with("def ") || line.starts_with("async def ") || line.starts_with("class ") {
        Some(line.trim_end_matches(':').trim().to_string())
    } else {
        None
    }
}

fn extract_js_signature(line: &str) -> Option<String> {
    if line.starts_with("function ")
        || line.starts_with("async function ")
        || line.starts_with("export function ")
        || line.starts_with("export async function ")
        || line.starts_with("export default function ")
        || line.starts_with("class ")
        || line.starts_with("export class ")
        || line.contains("=> {")
    {
        Some(line.trim_end_matches('{').trim().to_string())
    } else {
        None
    }
}

fn extract_go_signature(line: &str) -> Option<String> {
    if line.starts_with("func ") || line.starts_with("type ") {
        Some(line.trim_end_matches('{').trim().to_string())
    } else {
        None
    }
}

fn extract_java_signature(line: &str) -> Option<String> {
    let keywords = [
        "public ",
        "private ",
        "protected ",
        "static ",
        "abstract ",
        "final ",
    ];
    let is_declaration = keywords.iter().any(|k| line.starts_with(k))
        && (line.contains('(') || line.contains("class ") || line.contains("interface "));

    if is_declaration || line.starts_with("class ") || line.starts_with("interface ") {
        Some(line.trim_end_matches('{').trim().to_string())
    } else {
        None
    }
}

fn extract_ruby_signature(line: &str) -> Option<String> {
    if line.starts_with("def ") || line.starts_with("class ") || line.starts_with("module ") {
        Some(line.trim().to_string())
    } else {
        None
    }
}

fn extract_c_signature(line: &str) -> Option<String> {
    // Simplified: look for function-like declarations
    if (line.contains('(') && !line.starts_with("//") && !line.starts_with('#'))
        || line.starts_with("struct ")
        || line.starts_with("class ")
        || line.starts_with("typedef ")
    {
        // Skip preprocessor and comments
        if line.starts_with('#') || line.starts_with("//") || line.starts_with("/*") {
            return None;
        }
        // Skip simple statements (assignments, returns, etc.)
        if line.contains('=') && !line.contains("==") && !line.contains("!=") {
            return None;
        }
        Some(line.trim_end_matches('{').trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_workspace() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Create source files
        fs::create_dir_all(path.join("src")).unwrap();
        fs::write(
            path.join("src/main.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n\npub fn helper() -> bool {\n    true\n}\n",
        )
        .unwrap();
        fs::write(
            path.join("src/lib.rs"),
            "pub mod utils;\n\npub struct Config {\n    pub name: String,\n}\n\nimpl Config {\n    pub fn new() -> Self {\n        Self { name: String::new() }\n    }\n}\n",
        )
        .unwrap();
        fs::write(
            path.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            path.join("README.md"),
            "# Test Project\n\nA test project.\n",
        )
        .unwrap();

        // Create .gitignore
        fs::write(path.join(".gitignore"), "target/\n*.tmp\n").unwrap();

        // Create a file that should be ignored
        fs::create_dir_all(path.join("target")).unwrap();
        fs::write(path.join("target/debug.rs"), "should be ignored").unwrap();

        // Create a binary file (should be skipped)
        fs::write(path.join("image.png"), [0x89, 0x50, 0x4E, 0x47]).unwrap();

        (dir, path)
    }

    #[test]
    fn test_is_indexable() {
        assert!(is_indexable(Path::new("src/main.rs")));
        assert!(is_indexable(Path::new("app.py")));
        assert!(is_indexable(Path::new("index.js")));
        assert!(is_indexable(Path::new("Makefile")));
        assert!(is_indexable(Path::new("Dockerfile")));
        assert!(!is_indexable(Path::new("image.png")));
        assert!(!is_indexable(Path::new("archive.zip")));
        assert!(!is_indexable(Path::new("binary.exe")));
    }

    #[test]
    fn test_extract_rust_signatures() {
        let content = "use std::io;\n\npub fn process(data: &[u8]) -> Result<(), Error> {\n    Ok(())\n}\n\nstruct Config {\n    name: String,\n}\n\nimpl Config {\n    fn new() -> Self { todo!() }\n}\n";
        let sigs = extract_signatures(content, "lib.rs");
        assert!(sigs.iter().any(|s| s.contains("pub fn process")));
        assert!(sigs.iter().any(|s| s.contains("struct Config")));
        assert!(sigs.iter().any(|s| s.contains("impl Config")));
        assert!(sigs.iter().any(|s| s.contains("fn new")));
    }

    #[test]
    fn test_extract_python_signatures() {
        let content = "import os\n\nclass Handler:\n    def process(self, data):\n        pass\n\nasync def fetch(url):\n    pass\n";
        let sigs = extract_signatures(content, "handler.py");
        assert!(sigs.iter().any(|s| s.contains("class Handler")));
        assert!(sigs.iter().any(|s| s.contains("def process")));
        assert!(sigs.iter().any(|s| s.contains("async def fetch")));
    }

    #[test]
    fn test_extract_js_signatures() {
        let content = "const x = 1;\n\nfunction handleRequest(req) {\n    return null;\n}\n\nexport class Server {\n}\n";
        let sigs = extract_signatures(content, "server.js");
        assert!(sigs.iter().any(|s| s.contains("function handleRequest")));
        assert!(sigs.iter().any(|s| s.contains("export class Server")));
    }

    #[test]
    fn test_index_workspace() {
        let (_dir, path) = setup_test_workspace();

        let search_config = SearchConfig {
            index_path: path.join(".rustant/search_index"),
            db_path: path.join(".rustant/vectors.db"),
            ..Default::default()
        };

        let mut indexer = ProjectIndexer::new(path, search_config).unwrap();
        let stats = indexer.index_workspace();

        // Should have indexed some files
        assert!(stats.files_indexed > 0, "Should index at least one file");
        assert!(
            stats.entries_indexed > 0,
            "Should create at least one entry"
        );

        // Project info should be detected
        assert!(stats.project_info.is_some());
    }

    #[test]
    fn test_search_indexed_workspace() {
        let (_dir, path) = setup_test_workspace();

        let search_config = SearchConfig {
            index_path: path.join(".rustant/search_index"),
            db_path: path.join(".rustant/vectors.db"),
            ..Default::default()
        };

        let mut indexer = ProjectIndexer::new(path, search_config).unwrap();
        indexer.index_workspace();

        // Search for something we know is in the workspace
        let results = indexer.search("main function").unwrap();
        assert!(
            !results.is_empty(),
            "Should find results for 'main function'"
        );

        // At least one result should reference main.rs
        let has_main = results.iter().any(|r| r.content.contains("main"));
        assert!(has_main, "Should find main.rs related content");
    }

    #[test]
    fn test_indexer_config() {
        let config = IndexerConfig::default();
        assert_eq!(config.max_file_size, MAX_FILE_SIZE);
        assert_eq!(config.max_files, MAX_FILES);
        assert!(config.index_content);
        assert!(config.index_signatures);
    }

    #[test]
    fn test_indexer_with_custom_config() {
        let (_dir, path) = setup_test_workspace();

        let search_config = SearchConfig {
            index_path: path.join(".rustant/search_index"),
            db_path: path.join(".rustant/vectors.db"),
            ..Default::default()
        };

        let custom = IndexerConfig {
            max_files: 2,
            index_content: false,
            index_signatures: false,
            ..Default::default()
        };

        let mut indexer = ProjectIndexer::with_config(path, search_config, custom).unwrap();
        let stats = indexer.index_workspace();

        // Should respect max_files limit
        assert!(stats.files_indexed <= 2);
    }

    #[test]
    fn test_build_structure_summary() {
        let (_dir, path) = setup_test_workspace();

        let search_config = SearchConfig {
            index_path: path.join(".rustant/search_index"),
            db_path: path.join(".rustant/vectors.db"),
            ..Default::default()
        };

        let indexer = ProjectIndexer::new(path.clone(), search_config).unwrap();
        let info = detect_project(&path);
        let summary = indexer.build_structure_summary(&info);

        assert!(summary.contains("Project type:"));
        assert!(summary.contains("Top-level structure:"));
    }

    #[test]
    fn test_ignored_files_not_indexed() {
        let (_dir, path) = setup_test_workspace();

        // Initialize a git repo so .gitignore is respected by the `ignore` crate
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&path)
            .output()
            .expect("git init");

        let search_config = SearchConfig {
            index_path: path.join(".rustant/search_index"),
            db_path: path.join(".rustant/vectors.db"),
            ..Default::default()
        };

        let mut indexer = ProjectIndexer::new(path, search_config).unwrap();
        indexer.index_workspace();

        // Search for content that should have been ignored
        let results = indexer.search("should be ignored").unwrap();
        let has_target = results
            .iter()
            .any(|r| r.content.contains("target/debug.rs"));
        assert!(
            !has_target,
            "Files in target/ should be ignored by .gitignore"
        );
    }
}
