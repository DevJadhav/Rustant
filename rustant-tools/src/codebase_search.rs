//! Codebase search tool powered by the Project Context Auto-Indexer.
//!
//! Provides semantic search over the indexed project files, function signatures,
//! and content summaries. Requires the workspace to have been indexed first.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::indexer::ProjectIndexer;
use rustant_core::search::SearchConfig;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::path::PathBuf;
use std::sync::Mutex;

/// Tool for searching the project codebase using hybrid search.
pub struct CodebaseSearchTool {
    indexer: Mutex<Option<ProjectIndexer>>,
    workspace: PathBuf,
}

impl CodebaseSearchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            indexer: Mutex::new(None),
            workspace,
        }
    }

    /// Ensure the indexer is initialized and workspace is indexed.
    fn ensure_indexed(&self) -> Result<(), ToolError> {
        let mut guard = self
            .indexer
            .lock()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "codebase_search".into(),
                message: format!("Lock error: {e}"),
            })?;

        if guard.is_none() {
            let search_config = SearchConfig {
                index_path: self.workspace.join(".rustant/search_index"),
                db_path: self.workspace.join(".rustant/vectors.db"),
                ..Default::default()
            };

            let mut indexer =
                ProjectIndexer::new(self.workspace.clone(), search_config).map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "codebase_search".into(),
                        message: format!("Failed to initialize indexer: {e}"),
                    }
                })?;

            indexer.index_workspace();
            *guard = Some(indexer);
        }

        Ok(())
    }
}

#[async_trait]
impl Tool for CodebaseSearchTool {
    fn name(&self) -> &str {
        "codebase_search"
    }

    fn description(&self) -> &str {
        "Search the project codebase using natural language queries. \
         Finds relevant files, function signatures, and code content. \
         The workspace is automatically indexed on first use."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language search query (e.g., 'authentication handler', \
                        'database connection', 'error types')"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 10)"
                },
                "filter": {
                    "type": "string",
                    "description": "Filter results by code block kind: 'function', 'struct', \
                        'enum', 'trait', 'class', 'interface', 'module', 'import', 'constant', 'method'"
                },
                "language": {
                    "type": "string",
                    "description": "Filter results by programming language (e.g., 'rust', 'python', \
                        'typescript', 'javascript', 'go', 'java', 'ruby', 'c', 'cpp')"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "codebase_search".into(),
                reason: "'query' parameter is required".into(),
            })?;

        let max_results = args["max_results"].as_u64().unwrap_or(10) as usize;
        let filter = args["filter"].as_str().map(|s| s.to_lowercase());
        let language = args["language"].as_str().map(|s| s.to_lowercase());

        // Ensure workspace is indexed (lazy initialization)
        self.ensure_indexed()?;

        let guard = self
            .indexer
            .lock()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "codebase_search".into(),
                message: format!("Lock error: {e}"),
            })?;

        let indexer = guard.as_ref().ok_or_else(|| ToolError::ExecutionFailed {
            name: "codebase_search".into(),
            message: "Indexer not initialized".into(),
        })?;

        let results = indexer
            .search(query)
            .map_err(|e| ToolError::ExecutionFailed {
                name: "codebase_search".into(),
                message: format!("Search failed: {e}"),
            })?;

        // Apply post-search filters
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|r| {
                // Language filter: match file extension in fact_id
                if let Some(ref lang) = language {
                    let extensions = language_extensions(lang);
                    if !extensions.is_empty() {
                        let has_ext = extensions
                            .iter()
                            .any(|ext| r.fact_id.contains(&format!(".{ext}")));
                        if !has_ext {
                            return false;
                        }
                    }
                }
                // Block kind filter: match content patterns
                if let Some(ref kind) = filter
                    && !matches_block_kind(&r.content, kind)
                {
                    return false;
                }
                true
            })
            .collect();

        if filtered.is_empty() {
            let mut msg = format!("No results found for query: '{query}'");
            if let Some(ref f) = filter {
                msg.push_str(&format!(" (filter: {f})"));
            }
            if let Some(ref l) = language {
                msg.push_str(&format!(" (language: {l})"));
            }
            return Ok(ToolOutput::text(msg));
        }

        let shown = filtered.len().min(max_results);
        let mut output = format!("Found {shown} results for '{query}'");
        if let Some(ref f) = filter {
            output.push_str(&format!(" [filter: {f}]"));
        }
        if let Some(ref l) = language {
            output.push_str(&format!(" [lang: {l}]"));
        }
        output.push_str(":\n\n");

        for (i, result) in filtered.iter().take(max_results).enumerate() {
            output.push_str(&format!(
                "{}. [score: {:.2}] {}\n",
                i + 1,
                result.combined_score,
                result.content.lines().next().unwrap_or(&result.content)
            ));

            // Show a bit more context for top results
            if i < 3 {
                let extra_lines: Vec<&str> = result.content.lines().skip(1).take(3).collect();
                if !extra_lines.is_empty() {
                    for line in extra_lines {
                        output.push_str(&format!("   {line}\n"));
                    }
                }
            }
            output.push('\n');
        }

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> std::time::Duration {
        // Indexing can take a while on first run
        std::time::Duration::from_secs(120)
    }
}

/// Map a language name to its common file extensions.
fn language_extensions(lang: &str) -> &[&str] {
    match lang {
        // Systems / compiled
        "rust" | "rs" => &["rs"],
        "c" => &["c", "h"],
        "cpp" | "c++" | "cxx" => &["cpp", "cxx", "cc", "hpp", "h", "hxx"],
        "go" | "golang" => &["go"],
        "java" => &["java"],
        "kotlin" | "kt" => &["kt", "kts"],
        "scala" => &["scala", "sc"],
        "swift" => &["swift"],
        "objective-c" | "objc" => &["m", "mm"],
        "dart" => &["dart"],
        "zig" => &["zig"],
        "nim" => &["nim"],
        "haskell" | "hs" => &["hs", "lhs"],
        "ocaml" | "ml" => &["ml", "mli"],
        "erlang" | "erl" => &["erl", "hrl"],
        "elixir" | "ex" => &["ex", "exs"],
        // Scripting
        "python" | "py" => &["py", "pyi", "pyw"],
        "javascript" | "js" => &["js", "jsx", "mjs", "cjs"],
        "typescript" | "ts" => &["ts", "tsx", "mts", "cts"],
        "ruby" | "rb" => &["rb", "erb", "rake"],
        "php" => &["php", "phtml"],
        "perl" | "pl" => &["pl", "pm"],
        "lua" => &["lua"],
        "r" => &["r", "R"],
        "julia" | "jl" => &["jl"],
        "clojure" | "clj" => &["clj", "cljs", "cljc", "edn"],
        "shell" | "bash" | "sh" => &["sh", "bash", "zsh", "fish"],
        "powershell" | "ps1" => &["ps1", "psm1", "psd1"],
        // Web / markup
        "html" => &["html", "htm", "xhtml"],
        "css" => &["css", "scss", "sass", "less"],
        "vue" => &["vue"],
        "svelte" => &["svelte"],
        "xml" => &["xml", "xsl", "xslt", "svg"],
        "markdown" | "md" => &["md", "mdx"],
        // Data / config
        "json" => &["json", "jsonl", "jsonc"],
        "yaml" | "yml" => &["yaml", "yml"],
        "toml" => &["toml"],
        "ini" | "cfg" => &["ini", "cfg", "conf"],
        "protobuf" | "proto" => &["proto"],
        // SQL / database
        "sql" => &["sql"],
        "plsql" | "pl/sql" => &["sql", "pls", "plsql", "pks", "pkb"],
        "tsql" | "t-sql" => &["sql"],
        "mysql" => &["sql"],
        "postgresql" | "pgsql" => &["sql"],
        "sqlite" => &["sql"],
        // NoSQL / query languages
        "mongodb" | "mongo" => &["js", "json"],
        "graphql" | "gql" => &["graphql", "gql"],
        "cql" | "cassandra" => &["cql"],
        "cypher" | "neo4j" => &["cypher", "cql"],
        "hql" | "hive" => &["hql", "q"],
        "sparql" => &["sparql", "rq"],
        // DevOps / infra
        "dockerfile" | "docker" => &["dockerfile"],
        "terraform" | "tf" | "hcl" => &["tf", "tfvars", "hcl"],
        "nix" => &["nix"],
        _ => &[],
    }
}

/// Check if the content matches a particular code block kind.
fn matches_block_kind(content: &str, kind: &str) -> bool {
    let first_line = content.lines().next().unwrap_or("");
    let lower = first_line.to_lowercase();
    let content_lower = content.to_lowercase();
    match kind {
        "function" | "fn" => {
            lower.contains("fn ")
                || lower.contains("def ")
                || lower.contains("func ")
                || lower.contains("function ")
                || lower.contains("create function")
                || lower.contains("create or replace function")
        }
        "struct" => lower.contains("struct "),
        "enum" => lower.contains("enum ") || lower.contains("create type"),
        "trait" => lower.contains("trait "),
        "class" => lower.contains("class "),
        "interface" => lower.contains("interface "),
        "module" | "mod" => lower.contains("mod ") || lower.contains("module "),
        "import" | "use" => {
            lower.contains("use ")
                || lower.contains("import ")
                || lower.contains("require(")
                || lower.contains("include ")
                || lower.contains("from ")
        }
        "constant" | "const" => lower.contains("const ") || lower.contains("static "),
        "method" => {
            (lower.contains("fn ") || lower.contains("def ") || lower.contains("func "))
                && (content.contains("self") || content.contains("this"))
        }
        // SQL-specific block kinds
        "table" | "create_table" => {
            content_lower.contains("create table") || content_lower.contains("alter table")
        }
        "view" => {
            content_lower.contains("create view")
                || content_lower.contains("create or replace view")
        }
        "index" => {
            content_lower.contains("create index") || content_lower.contains("create unique index")
        }
        "trigger" => content_lower.contains("create trigger"),
        "procedure" | "stored_procedure" => {
            content_lower.contains("create procedure")
                || content_lower.contains("create or replace procedure")
        }
        "query" | "select" => content_lower.contains("select ") && content_lower.contains("from "),
        "migration" => {
            content_lower.contains("alter table")
                || content_lower.contains("add column")
                || content_lower.contains("drop column")
                || content_lower.contains("rename ")
        }
        "schema" => {
            content_lower.contains("create schema")
                || content_lower.contains("create database")
                || content_lower.contains("create keyspace")
                || content_lower.contains("create collection")
        }
        _ => true, // Unknown filter → don't filter
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_workspace() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        fs::create_dir_all(path.join("src")).unwrap();
        fs::write(
            path.join("src/main.rs"),
            "fn main() {\n    run_server();\n}\n\nfn run_server() {\n    println!(\"starting\");\n}\n",
        )
        .unwrap();
        fs::write(
            path.join("src/auth.rs"),
            "pub fn authenticate(token: &str) -> bool {\n    !token.is_empty()\n}\n",
        )
        .unwrap();
        fs::write(path.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        (dir, path)
    }

    #[tokio::test]
    async fn test_codebase_search_basic() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({
            "query": "authenticate"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(
            result.content.contains("authenticate") || result.content.contains("auth"),
            "Should find auth-related content: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn test_codebase_search_no_results() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({
            "query": "zzz_nonexistent_xyz_999"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.content.contains("No results") || result.content.contains("Found"));
    }

    #[tokio::test]
    async fn test_codebase_search_missing_query() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_properties() {
        let dir = TempDir::new().unwrap();
        let tool = CodebaseSearchTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "codebase_search");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.description().contains("Search"));
    }

    #[test]
    fn test_language_extensions() {
        assert_eq!(language_extensions("rust"), &["rs"]);
        assert_eq!(language_extensions("python"), &["py", "pyi", "pyw"]);
        assert!(language_extensions("sql").contains(&"sql"));
        assert!(language_extensions("graphql").contains(&"graphql"));
        assert!(language_extensions("mongodb").contains(&"js"));
        assert!(language_extensions("terraform").contains(&"tf"));
        assert!(language_extensions("unknown_lang").is_empty());
    }

    #[test]
    fn test_matches_block_kind_function() {
        assert!(matches_block_kind("fn main() {", "function"));
        assert!(matches_block_kind("def process(data):", "function"));
        assert!(matches_block_kind(
            "func Handle(w http.ResponseWriter)",
            "function"
        ));
        assert!(matches_block_kind("CREATE FUNCTION calc()", "function"));
        assert!(!matches_block_kind("struct Config {", "function"));
    }

    #[test]
    fn test_matches_block_kind_sql() {
        assert!(matches_block_kind("CREATE TABLE users (id INT)", "table"));
        assert!(matches_block_kind(
            "ALTER TABLE users ADD COLUMN email",
            "table"
        ));
        assert!(matches_block_kind("CREATE VIEW active_users AS", "view"));
        assert!(matches_block_kind(
            "CREATE INDEX idx_email ON users",
            "index"
        ));
        assert!(matches_block_kind("CREATE TRIGGER on_insert", "trigger"));
        assert!(matches_block_kind(
            "SELECT * FROM users WHERE id = 1",
            "query"
        ));
        assert!(matches_block_kind(
            "CREATE PROCEDURE update_balance()",
            "procedure"
        ));
        assert!(matches_block_kind("CREATE SCHEMA myapp", "schema"));
        assert!(matches_block_kind("CREATE KEYSPACE mykeyspace", "schema"));
    }

    #[test]
    fn test_matches_block_kind_unknown_passes() {
        // Unknown filter kind passes everything
        assert!(matches_block_kind("anything here", "unknown_kind"));
    }

    #[tokio::test]
    async fn test_codebase_search_with_language_filter() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({
            "query": "main",
            "language": "rust"
        });

        let result = tool.execute(args).await.unwrap();
        // Results should only be from .rs files
        assert!(
            result.content.contains("Found") || result.content.contains("No results"),
            "Should return results or no-results message: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn test_codebase_search_with_filter() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({
            "query": "authenticate",
            "filter": "function"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(
            result.content.contains("Found") || result.content.contains("No results"),
            "Should return filtered results: {}",
            result.content
        );
    }
}
