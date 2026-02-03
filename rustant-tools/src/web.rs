//! Web tools: search, fetch, and document reading.
//!
//! Lightweight web access tools that work without browser automation.
//! - `web_search`: Search the web using DuckDuckGo instant answers (privacy-first).
//! - `web_fetch`: Fetch a URL and extract readable text content.
//! - `document_read`: Read PDF and text documents from the local filesystem.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------------------
// WebSearchTool
// ---------------------------------------------------------------------------

/// Search the web using DuckDuckGo instant answers API.
///
/// Returns structured results with titles, snippets, and URLs.
/// Privacy-first: queries go directly to DuckDuckGo, never through a third party.
#[derive(Default)]
pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns titles, snippets, and URLs from search results. \
         Use this to look up documentation, find solutions to errors, or research topics."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, max: 10)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(15)
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "web_search".into(),
                reason: "Missing required parameter: query".into(),
            }
        })?;

        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(10) as usize;

        // Use DuckDuckGo HTML search (no API key required, privacy-first)
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("Rustant/1.0")
            .build()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "web_search".into(),
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        // Use DuckDuckGo instant answer API
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query)
        );

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "web_search".into(),
                message: format!("Search request failed: {}", e),
            })?;

        let body: serde_json::Value =
            response
                .json()
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "web_search".into(),
                    message: format!("Failed to parse search response: {}", e),
                })?;

        let mut results = Vec::new();

        // Extract abstract (main answer)
        if let Some(abstract_text) = body.get("AbstractText").and_then(|v| v.as_str()) {
            if !abstract_text.is_empty() {
                let source = body
                    .get("AbstractSource")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let url = body
                    .get("AbstractURL")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                results.push(format!("[{}] {}\n  URL: {}", source, abstract_text, url));
            }
        }

        // Extract related topics
        if let Some(topics) = body.get("RelatedTopics").and_then(|v| v.as_array()) {
            for topic in topics
                .iter()
                .take(max_results.saturating_sub(results.len()))
            {
                if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                    let url = topic.get("FirstURL").and_then(|v| v.as_str()).unwrap_or("");
                    results.push(format!("- {}\n  URL: {}", text, url));
                }
            }
        }

        // Extract results from Results array
        if let Some(res_array) = body.get("Results").and_then(|v| v.as_array()) {
            for result in res_array
                .iter()
                .take(max_results.saturating_sub(results.len()))
            {
                if let Some(text) = result.get("Text").and_then(|v| v.as_str()) {
                    let url = result
                        .get("FirstURL")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    results.push(format!("- {}\n  URL: {}", text, url));
                }
            }
        }

        let content = if results.is_empty() {
            format!(
                "No instant answers found for \"{}\". Try refining your query or use web_fetch with a specific URL.",
                query
            )
        } else {
            format!(
                "Search results for \"{}\":\n\n{}",
                query,
                results.join("\n\n")
            )
        };

        Ok(ToolOutput::text(content))
    }
}

// ---------------------------------------------------------------------------
// WebFetchTool
// ---------------------------------------------------------------------------

/// Fetch a URL and extract readable text content.
///
/// Strips HTML tags and returns clean text. Much lighter than browser
/// automation — no Chrome required.
#[derive(Default)]
pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a web page URL and extract its text content. Returns the readable text \
         from the page, stripped of HTML. Use this to read documentation, articles, or \
         API references from the web."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum characters of content to return (default: 5000)",
                    "default": 5000
                }
            },
            "required": ["url"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "web_fetch".into(),
                reason: "Missing required parameter: url".into(),
            }
        })?;

        let max_length = args
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000) as usize;

        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ToolError::InvalidArguments {
                name: "web_fetch".into(),
                reason: "URL must start with http:// or https://".into(),
            });
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("Rustant/1.0")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "web_fetch".into(),
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "web_fetch".into(),
                message: format!("Fetch failed: {}", e),
            })?;

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolOutput::text(format!(
                "HTTP {} for URL: {}",
                status, url
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "web_fetch".into(),
                message: format!("Failed to read response body: {}", e),
            })?;

        // Extract text from HTML
        let text =
            if content_type.contains("text/html") || content_type.contains("application/xhtml") {
                extract_text_from_html(&body)
            } else {
                // Plain text or other formats — return as-is
                body
            };

        // Truncate if needed
        let text = if text.len() > max_length {
            format!(
                "{}...\n\n[Truncated at {} characters. Use max_length to see more.]",
                &text[..max_length],
                max_length
            )
        } else {
            text
        };

        let content = format!("Content from {}:\n\n{}", url, text);

        Ok(ToolOutput::text(content))
    }
}

/// Simple HTML-to-text extraction.
///
/// Strips HTML tags and extracts readable content. Handles common elements
/// like paragraphs, headings, list items, and code blocks.
fn extract_text_from_html(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut building_tag = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            building_tag = true;
            tag_name.clear();
            continue;
        }
        if ch == '>' {
            in_tag = false;
            building_tag = false;

            let tag_lower = tag_name.to_lowercase();
            if tag_lower == "script" {
                in_script = true;
            } else if tag_lower == "/script" {
                in_script = false;
            } else if tag_lower == "style" {
                in_style = true;
            } else if tag_lower == "/style" {
                in_style = false;
            }

            // Add newlines for block elements
            if tag_lower.starts_with("p")
                || tag_lower.starts_with("/p")
                || tag_lower.starts_with("br")
                || tag_lower.starts_with("div")
                || tag_lower.starts_with("/div")
                || tag_lower.starts_with("h1")
                || tag_lower.starts_with("h2")
                || tag_lower.starts_with("h3")
                || tag_lower.starts_with("h4")
                || tag_lower.starts_with("h5")
                || tag_lower.starts_with("h6")
                || tag_lower.starts_with("/h")
                || tag_lower.starts_with("li")
                || tag_lower.starts_with("tr")
            {
                text.push('\n');
            }

            continue;
        }
        if in_tag {
            if building_tag && (ch.is_alphanumeric() || ch == '/') {
                tag_name.push(ch);
            } else {
                building_tag = false;
            }
            continue;
        }
        if in_script || in_style {
            continue;
        }
        text.push(ch);
    }

    // Decode common HTML entities
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Clean up whitespace: collapse multiple blank lines
    let mut lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    lines.dedup_by(|a, b| a.is_empty() && b.is_empty());
    let result: String = lines
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    result
}

// ---------------------------------------------------------------------------
// DocumentReadTool
// ---------------------------------------------------------------------------

/// Read documents from the local filesystem (plain text and common formats).
///
/// Supports: .txt, .md, .csv, .json, .yaml, .toml, .xml, .log files.
/// For PDF support, the `pdf-extract` crate would be needed (not included
/// by default to keep dependencies minimal).
pub struct DocumentReadTool {
    workspace: PathBuf,
}

impl DocumentReadTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf, ToolError> {
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.workspace.join(path)
        };

        let canonical = resolved
            .canonicalize()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "document_read".into(),
                message: format!("Path resolution failed: {}", e),
            })?;

        // Allow reading outside workspace for documents (e.g., ~/Downloads/*.pdf)
        // but still validate the path exists
        if !canonical.exists() {
            return Err(ToolError::ExecutionFailed {
                name: "document_read".into(),
                message: format!("File not found: {}", path),
            });
        }

        Ok(canonical)
    }
}

#[async_trait]
impl Tool for DocumentReadTool {
    fn name(&self) -> &str {
        "document_read"
    }

    fn description(&self) -> &str {
        "Read a document file and extract its text content. Supports text-based formats: \
         .txt, .md, .csv, .json, .yaml, .yml, .toml, .xml, .log, .cfg, .ini, .html. \
         Returns the file content as text."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the document file (relative to workspace or absolute)"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum characters to return (default: 10000)",
                    "default": 10000
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path_str = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "document_read".into(),
                reason: "Missing required parameter: path".into(),
            }
        })?;

        let max_length = args
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(10000) as usize;

        let path = self.resolve_path(path_str)?;

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Validate supported extensions
        let supported = [
            "txt",
            "md",
            "csv",
            "json",
            "yaml",
            "yml",
            "toml",
            "xml",
            "log",
            "cfg",
            "ini",
            "html",
            "htm",
            "rst",
            "adoc",
            "tex",
            "rtf",
            "conf",
            "properties",
            "env",
        ];

        if !supported.contains(&extension.as_str()) {
            return Err(ToolError::InvalidArguments {
                name: "document_read".into(),
                reason: format!(
                    "Unsupported file format '.{}'. Supported: {}",
                    extension,
                    supported.join(", ")
                ),
            });
        }

        // Read file
        let content = std::fs::read_to_string(&path).map_err(|e| ToolError::ExecutionFailed {
            name: "document_read".into(),
            message: format!("Failed to read file: {}", e),
        })?;

        // For HTML files, extract text
        let text = if extension == "html" || extension == "htm" {
            extract_text_from_html(&content)
        } else {
            content
        };

        // Truncate if needed
        let text = if text.len() > max_length {
            format!(
                "{}...\n\n[Truncated at {} characters. Use max_length to see more.]",
                &text[..max_length],
                max_length
            )
        } else {
            text
        };

        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        let content = format!(
            "Document: {} ({} bytes, .{}):\n\n{}",
            path.display(),
            file_size,
            extension,
            text
        );

        Ok(ToolOutput::text(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_text_from_html() {
        let html = r#"
        <html>
        <head><title>Test</title></head>
        <body>
            <h1>Hello World</h1>
            <p>This is a <b>test</b> paragraph.</p>
            <script>var x = 1;</script>
            <style>.foo { color: red; }</style>
            <ul>
                <li>Item 1</li>
                <li>Item 2</li>
            </ul>
        </body>
        </html>"#;

        let text = extract_text_from_html(html);
        assert!(text.contains("Hello World"));
        assert!(text.contains("This is a test paragraph."));
        assert!(text.contains("Item 1"));
        assert!(text.contains("Item 2"));
        assert!(!text.contains("var x = 1"));
        assert!(!text.contains("color: red"));
    }

    #[test]
    fn test_extract_text_html_entities() {
        let html = "<p>A &amp; B &lt; C &gt; D &quot;E&quot;</p>";
        let text = extract_text_from_html(html);
        assert!(text.contains("A & B < C > D \"E\""));
    }

    #[tokio::test]
    async fn test_web_search_tool_schema() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "web_search");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("query").is_some());
    }

    #[tokio::test]
    async fn test_web_fetch_tool_schema() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "web_fetch");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        let schema = tool.parameters_schema();
        assert!(schema["properties"].get("url").is_some());
    }

    #[tokio::test]
    async fn test_web_fetch_invalid_url() {
        let tool = WebFetchTool::new();
        let result = tool.execute(serde_json::json!({"url": "not-a-url"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_document_read_tool_schema() {
        let dir = TempDir::new().unwrap();
        let tool = DocumentReadTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "document_read");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_document_read_text_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, this is a test document.").unwrap();

        let tool = DocumentReadTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(result.content.contains("Hello, this is a test document."));
    }

    #[tokio::test]
    async fn test_document_read_markdown_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("readme.md");
        std::fs::write(&file_path, "# Title\n\nSome content.").unwrap();

        let tool = DocumentReadTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(result.content.contains("# Title"));
    }

    #[tokio::test]
    async fn test_document_read_unsupported_format() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("binary.exe");
        std::fs::write(&file_path, "fake binary").unwrap();

        let tool = DocumentReadTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_document_read_truncation() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("long.txt");
        let long_text = "a".repeat(20000);
        std::fs::write(&file_path, &long_text).unwrap();

        let tool = DocumentReadTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "max_length": 100
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Truncated"));
    }

    #[tokio::test]
    async fn test_document_read_html_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("page.html");
        std::fs::write(&file_path, "<h1>Title</h1><p>Content here.</p>").unwrap();

        let tool = DocumentReadTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(result.content.contains("Title"));
        assert!(result.content.contains("Content here."));
        // Should not contain HTML tags
        assert!(!result.content.contains("<h1>"));
    }

    #[tokio::test]
    async fn test_document_read_missing_param() {
        let dir = TempDir::new().unwrap();
        let tool = DocumentReadTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_web_search_missing_query() {
        let tool = WebSearchTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
