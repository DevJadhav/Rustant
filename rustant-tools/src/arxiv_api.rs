//! ArXiv API client — HTTP client, Atom XML parser, and data models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A paper from the ArXiv API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxivPaper {
    pub arxiv_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub summary: String,
    pub categories: Vec<String>,
    pub primary_category: String,
    pub published: String,
    pub updated: String,
    pub pdf_url: String,
    pub abs_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_ref: Option<String>,
}

/// Search parameters for the ArXiv API.
#[derive(Debug, Clone)]
pub struct ArxivSearchParams {
    pub query: String,
    pub category: Option<String>,
    pub max_results: usize,
    pub sort_by: ArxivSortBy,
    pub sort_order: ArxivSortOrder,
    pub start: usize,
}

impl Default for ArxivSearchParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            category: None,
            max_results: 10,
            sort_by: ArxivSortBy::Relevance,
            sort_order: ArxivSortOrder::Descending,
            start: 0,
        }
    }
}

/// Sort criteria for ArXiv search.
#[derive(Debug, Clone, Copy)]
pub enum ArxivSortBy {
    Relevance,
    LastUpdatedDate,
    SubmittedDate,
}

impl ArxivSortBy {
    pub fn as_api_str(&self) -> &str {
        match self {
            ArxivSortBy::Relevance => "relevance",
            ArxivSortBy::LastUpdatedDate => "lastUpdatedDate",
            ArxivSortBy::SubmittedDate => "submittedDate",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "date" | "submitted" | "submitteddate" => ArxivSortBy::SubmittedDate,
            "updated" | "lastupdateddate" => ArxivSortBy::LastUpdatedDate,
            _ => ArxivSortBy::Relevance,
        }
    }
}

/// Sort order for ArXiv search.
#[derive(Debug, Clone, Copy)]
pub enum ArxivSortOrder {
    Ascending,
    Descending,
}

impl ArxivSortOrder {
    pub fn as_api_str(&self) -> &str {
        match self {
            ArxivSortOrder::Ascending => "ascending",
            ArxivSortOrder::Descending => "descending",
        }
    }
}

/// Search result containing papers and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxivSearchResult {
    pub papers: Vec<ArxivPaper>,
    pub total_results: usize,
    pub start_index: usize,
    pub items_per_page: usize,
}

/// Depth for paper analysis.
#[derive(Debug, Clone, Copy)]
pub enum AnalysisDepth {
    Quick,
    Standard,
    Full,
}

impl AnalysisDepth {
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "quick" | "brief" => AnalysisDepth::Quick,
            "full" | "detailed" | "deep" => AnalysisDepth::Full,
            _ => AnalysisDepth::Standard,
        }
    }
}

/// A saved paper in the user's library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub paper: ArxivPaper,
    pub tags: Vec<String>,
    pub collection: Option<String>,
    pub notes: Option<String>,
    pub saved_at: DateTime<Utc>,
}

/// Persistent library state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArxivLibraryState {
    pub entries: Vec<LibraryEntry>,
    pub collections: Vec<String>,
    #[serde(default)]
    pub digest_config: Option<DigestConfig>,
    #[serde(default)]
    pub implementations: Vec<ImplementationRecord>,
}

/// Configuration for daily paper digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestConfig {
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub enabled: bool,
}

/// Configuration for a target programming language's project scaffold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    pub language: String,
    pub package_manager: String,
    pub test_framework: String,
    pub file_extension: String,
    pub common_ml_libraries: Vec<String>,
    /// Command to create an isolated environment (venv, etc.)
    pub env_setup_commands: Vec<String>,
    /// Command to activate the environment
    pub env_activate: String,
}

/// A file in a project scaffold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldFile {
    pub path: String,
    pub content: String,
    pub is_test: bool,
}

/// A complete project scaffold generated from a paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectScaffold {
    pub paper_id: String,
    pub project_name: String,
    pub language_config: LanguageConfig,
    pub directory_structure: Vec<String>,
    pub files: Vec<ScaffoldFile>,
    pub dependencies: Vec<String>,
    pub setup_commands: Vec<String>,
    pub test_commands: Vec<String>,
}

/// Implementation mode for paper-to-code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImplementationMode {
    StandaloneProject,
    Notebook,
}

/// Status of a paper implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImplementationStatus {
    Scaffolded,
    DepsInstalled,
    TestsGenerated,
    Implementing,
    TestsPassing,
    Complete,
    Failed(String),
}

impl std::fmt::Display for ImplementationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scaffolded => write!(f, "scaffolded"),
            Self::DepsInstalled => write!(f, "deps_installed"),
            Self::TestsGenerated => write!(f, "tests_generated"),
            Self::Implementing => write!(f, "implementing"),
            Self::TestsPassing => write!(f, "tests_passing"),
            Self::Complete => write!(f, "complete"),
            Self::Failed(msg) => write!(f, "failed: {}", msg),
        }
    }
}

/// Record of a paper implementation tracked in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationRecord {
    pub paper_id: String,
    pub project_path: String,
    pub language: String,
    pub mode: ImplementationMode,
    pub status: ImplementationStatus,
    pub created_at: DateTime<Utc>,
}

/// Get language-specific project configuration.
///
/// IMPORTANT: All language configs include environment isolation commands
/// (venv for Python, cargo for Rust, etc.) to prevent polluting the system.
pub fn language_config(lang: &str) -> Option<LanguageConfig> {
    match lang.to_lowercase().as_str() {
        "python" | "py" => Some(LanguageConfig {
            language: "python".to_string(),
            package_manager: "pip".to_string(),
            test_framework: "pytest".to_string(),
            file_extension: "py".to_string(),
            common_ml_libraries: vec![
                "numpy".into(),
                "torch".into(),
                "tensorflow".into(),
                "scikit-learn".into(),
                "matplotlib".into(),
                "pandas".into(),
            ],
            env_setup_commands: vec!["python3 -m venv .venv".to_string()],
            env_activate: "source .venv/bin/activate".to_string(),
        }),
        "rust" | "rs" => Some(LanguageConfig {
            language: "rust".to_string(),
            package_manager: "cargo".to_string(),
            test_framework: "cargo test".to_string(),
            file_extension: "rs".to_string(),
            common_ml_libraries: vec![
                "ndarray".into(),
                "burn".into(),
                "candle".into(),
                "linfa".into(),
                "plotters".into(),
            ],
            env_setup_commands: vec![], // Cargo handles isolation via Cargo.toml
            env_activate: String::new(),
        }),
        "typescript" | "ts" | "javascript" | "js" => Some(LanguageConfig {
            language: "typescript".to_string(),
            package_manager: "npm".to_string(),
            test_framework: "jest".to_string(),
            file_extension: "ts".to_string(),
            common_ml_libraries: vec![
                "@tensorflow/tfjs".into(),
                "onnxruntime-node".into(),
                "mathjs".into(),
                "chart.js".into(),
            ],
            env_setup_commands: vec!["npm init -y".to_string()],
            env_activate: String::new(), // node_modules is project-local by default
        }),
        "go" | "golang" => Some(LanguageConfig {
            language: "go".to_string(),
            package_manager: "go mod".to_string(),
            test_framework: "go test".to_string(),
            file_extension: "go".to_string(),
            common_ml_libraries: vec!["gonum.org/v1/gonum".into(), "gorgonia.org/gorgonia".into()],
            env_setup_commands: vec!["go mod init paper_impl".to_string()],
            env_activate: String::new(), // Go modules are project-local
        }),
        "cpp" | "c++" => Some(LanguageConfig {
            language: "cpp".to_string(),
            package_manager: "cmake".to_string(),
            test_framework: "ctest".to_string(),
            file_extension: "cpp".to_string(),
            common_ml_libraries: vec!["Eigen".into(), "libtorch".into(), "xtensor".into()],
            env_setup_commands: vec!["mkdir -p build".to_string()],
            env_activate: String::new(),
        }),
        "julia" | "jl" => Some(LanguageConfig {
            language: "julia".to_string(),
            package_manager: "Pkg".to_string(),
            test_framework: "Test".to_string(),
            file_extension: "jl".to_string(),
            common_ml_libraries: vec![
                "Flux".into(),
                "MLJ".into(),
                "Plots".into(),
                "DataFrames".into(),
            ],
            env_setup_commands: vec![], // Julia uses project-local Manifest.toml
            env_activate: String::new(),
        }),
        _ => None,
    }
}

// ── ArXiv API Client ──────────────────────────────────────────

const ARXIV_API_BASE: &str = "https://export.arxiv.org/api/query";
const USER_AGENT: &str = "Rustant/1.0 (https://github.com/rustant)";

/// HTTP client for the ArXiv API.
pub struct ArxivClient {
    client: reqwest::Client,
    last_request: std::sync::Mutex<Option<std::time::Instant>>,
}

impl ArxivClient {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
        Ok(Self {
            client,
            last_request: std::sync::Mutex::new(None),
        })
    }

    /// Enforce a minimum 3-second delay between ArXiv API requests.
    async fn rate_limit(&self) {
        let wait_duration = {
            let last = self.last_request.lock().unwrap();
            if let Some(instant) = *last {
                let elapsed = instant.elapsed();
                if elapsed < Duration::from_secs(3) {
                    Some(Duration::from_secs(3) - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        }; // MutexGuard is dropped here before any .await

        if let Some(wait) = wait_duration {
            tokio::time::sleep(wait).await;
        }

        let mut last = self.last_request.lock().unwrap();
        *last = Some(std::time::Instant::now());
    }

    /// Search ArXiv with the given parameters.
    pub async fn search(&self, params: &ArxivSearchParams) -> Result<ArxivSearchResult, String> {
        self.rate_limit().await;
        let url = build_search_url(params);
        tracing::debug!("ArXiv search URL: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("ArXiv API request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("ArXiv API returned status {}", status));
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read ArXiv response: {}", e))?;

        parse_atom_response(&body)
    }

    /// Fetch a single paper by its ArXiv ID.
    pub async fn fetch_paper(&self, arxiv_id: &str) -> Result<ArxivPaper, String> {
        self.rate_limit().await;
        let clean_id = arxiv_id.trim();
        validate_arxiv_id(clean_id)?;

        let url = format!(
            "{}?id_list={}",
            ARXIV_API_BASE,
            urlencoding::encode(clean_id)
        );
        tracing::debug!("ArXiv fetch URL: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("ArXiv API request failed: {}", e))?;

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read ArXiv response: {}", e))?;

        let result = parse_atom_response(&body)?;
        result
            .papers
            .into_iter()
            .next()
            .ok_or_else(|| format!("Paper '{}' not found on ArXiv", clean_id))
    }
}

// ── URL Building ──────────────────────────────────────────────

/// Build the ArXiv API search URL from parameters.
pub fn build_search_url(params: &ArxivSearchParams) -> String {
    let mut search_query = if params.query.is_empty() {
        "all:*".to_string()
    } else {
        format!("all:{}", params.query)
    };

    if let Some(cat) = &params.category {
        search_query = format!("{} AND cat:{}", search_query, cat);
    }

    format!(
        "{}?search_query={}&start={}&max_results={}&sortBy={}&sortOrder={}",
        ARXIV_API_BASE,
        urlencoding::encode(&search_query),
        params.start,
        params.max_results,
        params.sort_by.as_api_str(),
        params.sort_order.as_api_str(),
    )
}

// ── XML Parsing ───────────────────────────────────────────────

/// Parse the Atom XML response from the ArXiv API.
pub fn parse_atom_response(xml: &str) -> Result<ArxivSearchResult, String> {
    let total_results = extract_opensearch_value(xml, "totalResults").unwrap_or(0);
    let start_index = extract_opensearch_value(xml, "startIndex").unwrap_or(0);
    let items_per_page = extract_opensearch_value(xml, "itemsPerPage").unwrap_or(0);

    let entries = extract_entries(xml);
    let mut papers = Vec::new();

    for entry_xml in &entries {
        if let Some(paper) = parse_entry(entry_xml) {
            papers.push(paper);
        }
    }

    Ok(ArxivSearchResult {
        papers,
        total_results,
        start_index,
        items_per_page,
    })
}

/// Extract all <entry>...</entry> blocks from the XML.
fn extract_entries(xml: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut search_from = 0;

    loop {
        let start_tag = "<entry>";
        let end_tag = "</entry>";

        let start = match xml[search_from..].find(start_tag) {
            Some(pos) => search_from + pos,
            None => break,
        };

        let end = match xml[start..].find(end_tag) {
            Some(pos) => start + pos + end_tag.len(),
            None => break,
        };

        entries.push(xml[start..end].to_string());
        search_from = end;
    }

    entries
}

/// Parse a single <entry> XML block into an ArxivPaper.
fn parse_entry(entry: &str) -> Option<ArxivPaper> {
    let id_url = extract_tag_text(entry, "id")?;
    let arxiv_id = extract_arxiv_id_from_url(&id_url);
    let title = normalize_whitespace(&extract_tag_text(entry, "title")?);

    // Extract authors
    let mut authors = Vec::new();
    let mut author_search = 0;
    while let Some(pos) = entry[author_search..].find("<author>") {
        let author_start = author_search + pos;
        let Some(end_pos) = entry[author_start..].find("</author>") else {
            break;
        };
        let author_end = author_start + end_pos + "</author>".len();
        let author_block = &entry[author_start..author_end];
        if let Some(name) = extract_tag_text(author_block, "name") {
            authors.push(name);
        }
        author_search = author_end;
    }

    let summary = normalize_whitespace(&extract_tag_text(entry, "summary").unwrap_or_default());
    let published = extract_tag_text(entry, "published").unwrap_or_default();
    let updated = extract_tag_text(entry, "updated").unwrap_or_default();

    // Extract categories
    let mut categories = Vec::new();
    let mut primary_category = String::new();
    let mut cat_search = 0;
    while let Some(pos) = entry[cat_search..].find("<category") {
        let cat_start = cat_search + pos;
        let cat_end = if let Some(end_pos) = entry[cat_start..].find("/>") {
            cat_start + end_pos + 2
        } else if let Some(end_pos) = entry[cat_start..].find('>') {
            cat_start + end_pos + 1
        } else {
            break;
        };
        let cat_tag = &entry[cat_start..cat_end];
        if let Some(term) = extract_attribute(cat_tag, "term") {
            categories.push(term);
        }
        cat_search = cat_end;
    }

    // Primary category from arxiv:primary_category
    if let Some(pc_start) = entry.find("primary_category") {
        if let Some(pc_end) = entry[pc_start..]
            .find("/>")
            .or_else(|| entry[pc_start..].find(">"))
        {
            let pc_tag = &entry[pc_start..pc_start + pc_end + 2];
            if let Some(term) = extract_attribute(pc_tag, "term") {
                primary_category = term;
            }
        }
    }
    if primary_category.is_empty() {
        primary_category = categories.first().cloned().unwrap_or_default();
    }

    // Extract links
    let mut pdf_url = String::new();
    let mut abs_url = id_url.clone();
    let mut link_search = 0;
    while let Some(pos) = entry[link_search..].find("<link") {
        let link_start = link_search + pos;
        let Some(end_pos) = entry[link_start..]
            .find("/>")
            .or_else(|| entry[link_start..].find('>'))
        else {
            break;
        };
        let link_end = link_start + end_pos + 2;
        let link_tag = &entry[link_start..link_end];
        let href = extract_attribute(link_tag, "href").unwrap_or_default();
        let title_attr = extract_attribute(link_tag, "title").unwrap_or_default();
        let link_type = extract_attribute(link_tag, "type").unwrap_or_default();

        if title_attr == "pdf" || link_type == "application/pdf" {
            pdf_url = href;
        } else if link_type.is_empty() && href.contains("/abs/") {
            abs_url = href;
        }
        link_search = link_end;
    }

    if pdf_url.is_empty() {
        pdf_url = format!("https://arxiv.org/pdf/{}", arxiv_id);
    }

    let doi = extract_tag_text_ns(entry, "arxiv:doi");
    let comment = extract_tag_text_ns(entry, "arxiv:comment").map(|c| normalize_whitespace(&c));
    let journal_ref = extract_tag_text_ns(entry, "arxiv:journal_ref");

    Some(ArxivPaper {
        arxiv_id,
        title,
        authors,
        summary,
        categories,
        primary_category,
        published,
        updated,
        pdf_url,
        abs_url,
        doi,
        comment,
        journal_ref,
    })
}

/// Extract the text content of the first occurrence of <tag>text</tag>.
fn extract_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);

    let start_pos = xml.find(&open)?;
    // Find the end of the opening tag (could have attributes)
    let content_start = xml[start_pos..].find('>')? + start_pos + 1;
    let content_end = xml[content_start..].find(&close)? + content_start;

    Some(xml[content_start..content_end].trim().to_string())
}

/// Extract text from a namespaced tag like <arxiv:doi>.
fn extract_tag_text_ns(xml: &str, tag: &str) -> Option<String> {
    extract_tag_text(xml, tag)
}

/// Extract an attribute value from a tag string.
pub fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
    let search = format!("{}=\"", attr);
    let start = tag.find(&search)? + search.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

/// Extract the ArXiv ID from a URL like "http://arxiv.org/abs/1706.03762v7".
pub fn extract_arxiv_id_from_url(url: &str) -> String {
    if let Some(pos) = url.rfind("/abs/") {
        url[pos + 5..].to_string()
    } else if let Some(pos) = url.rfind("/pdf/") {
        url[pos + 5..].trim_end_matches(".pdf").to_string()
    } else {
        // Already just an ID
        url.to_string()
    }
}

/// Normalize whitespace: collapse runs of whitespace into single spaces.
pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract an OpenSearch value like <opensearch:totalResults>100</opensearch:totalResults>.
fn extract_opensearch_value(xml: &str, field: &str) -> Option<usize> {
    let tag = format!("opensearch:{}", field);
    extract_tag_text(xml, &tag).and_then(|s| s.trim().parse().ok())
}

// ── ID Validation ─────────────────────────────────────────────

/// Validate that a string looks like an ArXiv ID.
/// Accepts new format (YYMM.NNNNN) and old format (category/NNNNNNN).
pub fn validate_arxiv_id(id: &str) -> Result<(), String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("ArXiv ID cannot be empty".to_string());
    }

    // New format: YYMM.NNNNN (optionally with version vN)
    let new_format = regex_lite_match_arxiv_new(id);
    // Old format: category/NNNNNNN (e.g., hep-th/9901001)
    let old_format = regex_lite_match_arxiv_old(id);

    if new_format || old_format {
        Ok(())
    } else {
        Err(format!(
            "Invalid ArXiv ID '{}'. Expected format: YYMM.NNNNN (e.g., 2301.12345) or category/NNNNNNN (e.g., hep-th/9901001)",
            id
        ))
    }
}

/// Match new-format ArXiv IDs: YYMM.NNNNN[vN]
fn regex_lite_match_arxiv_new(id: &str) -> bool {
    let base = id.split('v').next().unwrap_or(id);
    let parts: Vec<&str> = base.split('.').collect();
    if parts.len() != 2 {
        return false;
    }
    let yymm = parts[0];
    let nnnnn = parts[1];

    if yymm.len() != 4 || !yymm.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if nnnnn.is_empty() || nnnnn.len() > 5 || !nnnnn.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    // Check version suffix if present
    if let Some(v_pos) = id.find('v') {
        let version = &id[v_pos + 1..];
        if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }

    true
}

/// Match old-format ArXiv IDs: category/NNNNNNN
fn regex_lite_match_arxiv_old(id: &str) -> bool {
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return false;
    }
    let category = parts[0];
    let number = parts[1].split('v').next().unwrap_or(parts[1]);

    // Category: letters and hyphens
    if category.is_empty()
        || !category
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return false;
    }
    // Number: digits
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    true
}

// ── BibTeX Generation ─────────────────────────────────────────

/// Generate a BibTeX entry for a paper.
pub fn generate_bibtex(paper: &ArxivPaper) -> String {
    let cite_key = generate_cite_key(paper);
    let authors_bibtex = paper.authors.join(" and ");
    let title_escaped = escape_bibtex(&paper.title);
    let year = extract_year(&paper.published);

    let mut entry = format!(
        "@article{{{},\n  title = {{{}}},\n  author = {{{}}},\n  year = {{{}}},\n  eprint = {{{}}},\n  archivePrefix = {{arXiv}},\n  primaryClass = {{{}}}",
        cite_key, title_escaped, authors_bibtex, year, paper.arxiv_id, paper.primary_category,
    );

    if let Some(doi) = &paper.doi {
        entry.push_str(&format!(",\n  doi = {{{}}}", doi));
    }
    if let Some(journal) = &paper.journal_ref {
        entry.push_str(&format!(",\n  journal = {{{}}}", escape_bibtex(journal)));
    }

    entry.push_str("\n}");
    entry
}

/// Generate a citation key like "vaswani2017attention".
fn generate_cite_key(paper: &ArxivPaper) -> String {
    let first_author = paper
        .authors
        .first()
        .map(|a| {
            a.split_whitespace()
                .last()
                .unwrap_or(a)
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .unwrap_or_else(|| "unknown".to_string());

    let year = extract_year(&paper.published);

    let title_word = paper
        .title
        .split_whitespace()
        .find(|w| w.len() > 3 && w.chars().next().is_some_and(|c| c.is_alphabetic()))
        .unwrap_or("paper")
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();

    format!("{}{}{}", first_author, year, title_word)
}

/// Escape special LaTeX characters in BibTeX fields.
fn escape_bibtex(s: &str) -> String {
    s.replace('&', "\\&")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('~', "\\textasciitilde{}")
        .replace('^', "\\textasciicircum{}")
}

/// Extract year from a date string like "2017-06-12T17:57:34Z".
fn extract_year(date_str: &str) -> String {
    date_str.split('-').next().unwrap_or("0000").to_string()
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_ENTRY: &str = r#"<entry>
    <id>http://arxiv.org/abs/1706.03762v7</id>
    <updated>2023-08-02T01:09:28Z</updated>
    <published>2017-06-12T17:57:34Z</published>
    <title>Attention Is All You Need</title>
    <summary>  The dominant sequence transduction models are based on complex recurrent or
convolutional neural networks that include an encoder and a decoder.  </summary>
    <author><name>Ashish Vaswani</name></author>
    <author><name>Noam Shazeer</name></author>
    <author><name>Niki Parmar</name></author>
    <arxiv:doi xmlns:arxiv="http://arxiv.org/schemas/atom">10.1234/nips.2017</arxiv:doi>
    <arxiv:comment xmlns:arxiv="http://arxiv.org/schemas/atom">15 pages, 5 figures</arxiv:comment>
    <arxiv:journal_ref xmlns:arxiv="http://arxiv.org/schemas/atom">NeurIPS 2017</arxiv:journal_ref>
    <link href="http://arxiv.org/abs/1706.03762v7" rel="alternate" type="text/html"/>
    <link href="http://arxiv.org/pdf/1706.03762v7" title="pdf" type="application/pdf"/>
    <arxiv:primary_category xmlns:arxiv="http://arxiv.org/schemas/atom" term="cs.CL" scheme="http://arxiv.org/schemas/atom"/>
    <category term="cs.CL" scheme="http://arxiv.org/schemas/atom"/>
    <category term="cs.AI" scheme="http://arxiv.org/schemas/atom"/>
</entry>"#;

    const SAMPLE_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/"
      xmlns:arxiv="http://arxiv.org/schemas/atom">
  <title>ArXiv Query</title>
  <opensearch:totalResults>100</opensearch:totalResults>
  <opensearch:startIndex>0</opensearch:startIndex>
  <opensearch:itemsPerPage>3</opensearch:itemsPerPage>
  <entry>
    <id>http://arxiv.org/abs/1706.03762v7</id>
    <updated>2023-08-02T01:09:28Z</updated>
    <published>2017-06-12T17:57:34Z</published>
    <title>Attention Is All You Need</title>
    <summary>The dominant sequence transduction models.</summary>
    <author><name>Ashish Vaswani</name></author>
    <link href="http://arxiv.org/abs/1706.03762v7" rel="alternate" type="text/html"/>
    <link href="http://arxiv.org/pdf/1706.03762v7" title="pdf" type="application/pdf"/>
    <arxiv:primary_category xmlns:arxiv="http://arxiv.org/schemas/atom" term="cs.CL"/>
    <category term="cs.CL"/>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/1810.04805v2</id>
    <updated>2019-05-24T12:00:00Z</updated>
    <published>2018-10-11T00:00:00Z</published>
    <title>BERT: Pre-training of Deep Bidirectional Transformers</title>
    <summary>We introduce a new language representation model.</summary>
    <author><name>Jacob Devlin</name></author>
    <link href="http://arxiv.org/pdf/1810.04805v2" title="pdf" type="application/pdf"/>
    <arxiv:primary_category xmlns:arxiv="http://arxiv.org/schemas/atom" term="cs.CL"/>
    <category term="cs.CL"/>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/2005.14165v4</id>
    <updated>2020-07-22T00:00:00Z</updated>
    <published>2020-05-28T00:00:00Z</published>
    <title>Language Models are Few-Shot Learners</title>
    <summary>Recent work demonstrates substantial gains.</summary>
    <author><name>Tom Brown</name></author>
    <link href="http://arxiv.org/pdf/2005.14165v4" title="pdf" type="application/pdf"/>
    <arxiv:primary_category xmlns:arxiv="http://arxiv.org/schemas/atom" term="cs.CL"/>
    <category term="cs.CL"/>
  </entry>
</feed>"#;

    #[test]
    fn test_parse_single_entry() {
        let feed = format!(
            r#"<feed><opensearch:totalResults>1</opensearch:totalResults>
            <opensearch:startIndex>0</opensearch:startIndex>
            <opensearch:itemsPerPage>1</opensearch:itemsPerPage>{}</feed>"#,
            SAMPLE_ENTRY
        );
        let result = parse_atom_response(&feed).unwrap();
        assert_eq!(result.papers.len(), 1);
        let paper = &result.papers[0];
        assert_eq!(paper.arxiv_id, "1706.03762v7");
        assert_eq!(paper.title, "Attention Is All You Need");
        assert_eq!(paper.authors.len(), 3);
        assert_eq!(paper.authors[0], "Ashish Vaswani");
    }

    #[test]
    fn test_parse_multiple_entries() {
        let result = parse_atom_response(SAMPLE_FEED).unwrap();
        assert_eq!(result.papers.len(), 3);
        assert_eq!(result.total_results, 100);
        assert_eq!(result.start_index, 0);
        assert_eq!(result.items_per_page, 3);
    }

    #[test]
    fn test_parse_empty_results() {
        let feed = r#"<feed>
            <opensearch:totalResults>0</opensearch:totalResults>
            <opensearch:startIndex>0</opensearch:startIndex>
            <opensearch:itemsPerPage>10</opensearch:itemsPerPage>
        </feed>"#;
        let result = parse_atom_response(feed).unwrap();
        assert_eq!(result.papers.len(), 0);
        assert_eq!(result.total_results, 0);
    }

    #[test]
    fn test_parse_entry_all_fields() {
        let feed = format!(
            r#"<feed><opensearch:totalResults>1</opensearch:totalResults>
            <opensearch:startIndex>0</opensearch:startIndex>
            <opensearch:itemsPerPage>1</opensearch:itemsPerPage>{}</feed>"#,
            SAMPLE_ENTRY
        );
        let result = parse_atom_response(&feed).unwrap();
        let paper = &result.papers[0];
        assert_eq!(paper.doi.as_deref(), Some("10.1234/nips.2017"));
        assert_eq!(paper.comment.as_deref(), Some("15 pages, 5 figures"));
        assert_eq!(paper.journal_ref.as_deref(), Some("NeurIPS 2017"));
        assert_eq!(paper.primary_category, "cs.CL");
        assert!(paper.categories.contains(&"cs.CL".to_string()));
        assert!(paper.categories.contains(&"cs.AI".to_string()));
        assert!(paper.pdf_url.contains("1706.03762"));
    }

    #[test]
    fn test_parse_entry_missing_optionals() {
        let entry = r#"<feed>
            <opensearch:totalResults>1</opensearch:totalResults>
            <opensearch:startIndex>0</opensearch:startIndex>
            <opensearch:itemsPerPage>1</opensearch:itemsPerPage>
            <entry>
                <id>http://arxiv.org/abs/2301.12345v1</id>
                <published>2023-01-15T00:00:00Z</published>
                <updated>2023-01-15T00:00:00Z</updated>
                <title>A Simple Paper</title>
                <summary>A summary.</summary>
                <author><name>John Doe</name></author>
                <category term="cs.AI"/>
            </entry>
        </feed>"#;
        let result = parse_atom_response(entry).unwrap();
        let paper = &result.papers[0];
        assert!(paper.doi.is_none());
        assert!(paper.comment.is_none());
        assert!(paper.journal_ref.is_none());
    }

    #[test]
    fn test_extract_arxiv_id_from_url() {
        assert_eq!(
            extract_arxiv_id_from_url("http://arxiv.org/abs/1706.03762v7"),
            "1706.03762v7"
        );
        assert_eq!(
            extract_arxiv_id_from_url("http://arxiv.org/pdf/2301.12345"),
            "2301.12345"
        );
        assert_eq!(extract_arxiv_id_from_url("2301.12345"), "2301.12345");
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(
            normalize_whitespace("  Hello   World\n  Test  "),
            "Hello World Test"
        );
        assert_eq!(normalize_whitespace("single"), "single");
    }

    #[test]
    fn test_build_search_url_basic() {
        let params = ArxivSearchParams {
            query: "transformer attention".to_string(),
            ..Default::default()
        };
        let url = build_search_url(&params);
        assert!(url.starts_with(ARXIV_API_BASE));
        assert!(url.contains("transformer"));
        assert!(url.contains("attention"));
        assert!(url.contains("max_results=10"));
    }

    #[test]
    fn test_build_search_url_with_category() {
        let params = ArxivSearchParams {
            query: "attention".to_string(),
            category: Some("cs.AI".to_string()),
            ..Default::default()
        };
        let url = build_search_url(&params);
        assert!(url.contains("cat%3Acs.AI") || url.contains("cat:cs.AI"));
    }

    #[test]
    fn test_build_search_url_with_sort() {
        let params = ArxivSearchParams {
            query: "test".to_string(),
            sort_by: ArxivSortBy::SubmittedDate,
            sort_order: ArxivSortOrder::Descending,
            ..Default::default()
        };
        let url = build_search_url(&params);
        assert!(url.contains("sortBy=submittedDate"));
        assert!(url.contains("sortOrder=descending"));
    }

    #[test]
    fn test_generate_bibtex() {
        let paper = ArxivPaper {
            arxiv_id: "1706.03762v7".to_string(),
            title: "Attention Is All You Need".to_string(),
            authors: vec!["Ashish Vaswani".to_string(), "Noam Shazeer".to_string()],
            summary: "A summary.".to_string(),
            categories: vec!["cs.CL".to_string()],
            primary_category: "cs.CL".to_string(),
            published: "2017-06-12T17:57:34Z".to_string(),
            updated: "2023-08-02T01:09:28Z".to_string(),
            pdf_url: "http://arxiv.org/pdf/1706.03762v7".to_string(),
            abs_url: "http://arxiv.org/abs/1706.03762v7".to_string(),
            doi: None,
            comment: None,
            journal_ref: None,
        };
        let bib = generate_bibtex(&paper);
        assert!(bib.starts_with("@article{"));
        assert!(bib.contains("Attention Is All You Need"));
        assert!(bib.contains("Ashish Vaswani and Noam Shazeer"));
        assert!(bib.contains("2017"));
        assert!(bib.contains("1706.03762v7"));
        assert!(bib.contains("cs.CL"));
        assert!(bib.ends_with('}'));
    }

    #[test]
    fn test_generate_bibtex_special_chars() {
        let paper = ArxivPaper {
            arxiv_id: "2301.00001".to_string(),
            title: "A & B: 50% Better $Models$ with #Tags".to_string(),
            authors: vec!["Jane Smith".to_string()],
            summary: String::new(),
            categories: vec!["cs.AI".to_string()],
            primary_category: "cs.AI".to_string(),
            published: "2023-01-01T00:00:00Z".to_string(),
            updated: "2023-01-01T00:00:00Z".to_string(),
            pdf_url: String::new(),
            abs_url: String::new(),
            doi: None,
            comment: None,
            journal_ref: None,
        };
        let bib = generate_bibtex(&paper);
        assert!(bib.contains("\\&"));
        assert!(bib.contains("\\%"));
        assert!(bib.contains("\\$"));
        assert!(bib.contains("\\#"));
    }

    #[test]
    fn test_library_state_roundtrip() {
        let state = ArxivLibraryState {
            entries: vec![LibraryEntry {
                paper: ArxivPaper {
                    arxiv_id: "2301.12345".to_string(),
                    title: "Test Paper".to_string(),
                    authors: vec!["Author One".to_string()],
                    summary: "A test.".to_string(),
                    categories: vec!["cs.AI".to_string()],
                    primary_category: "cs.AI".to_string(),
                    published: "2023-01-15T00:00:00Z".to_string(),
                    updated: "2023-01-15T00:00:00Z".to_string(),
                    pdf_url: "https://arxiv.org/pdf/2301.12345".to_string(),
                    abs_url: "https://arxiv.org/abs/2301.12345".to_string(),
                    doi: None,
                    comment: None,
                    journal_ref: None,
                },
                tags: vec!["ml".to_string(), "test".to_string()],
                collection: Some("Favorites".to_string()),
                notes: Some("Great paper".to_string()),
                saved_at: Utc::now(),
            }],
            collections: vec!["Favorites".to_string()],
            digest_config: Some(DigestConfig {
                keywords: vec!["transformer".to_string()],
                categories: vec!["cs.AI".to_string()],
                enabled: true,
            }),
            implementations: Vec::new(),
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        let restored: ArxivLibraryState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.entries[0].paper.arxiv_id, "2301.12345");
        assert_eq!(restored.collections, vec!["Favorites"]);
        assert!(restored.digest_config.unwrap().enabled);
    }

    #[test]
    fn test_validate_arxiv_id_new_format() {
        assert!(validate_arxiv_id("2301.12345").is_ok());
        assert!(validate_arxiv_id("2301.12345v2").is_ok());
        assert!(validate_arxiv_id("1706.03762").is_ok());
        assert!(validate_arxiv_id("1706.03762v7").is_ok());
    }

    #[test]
    fn test_validate_arxiv_id_old_format() {
        assert!(validate_arxiv_id("hep-th/9901001").is_ok());
        assert!(validate_arxiv_id("math/0211159").is_ok());
        assert!(validate_arxiv_id("cs/0112017").is_ok());
    }

    #[test]
    fn test_validate_arxiv_id_invalid() {
        assert!(validate_arxiv_id("not-an-id").is_err());
        assert!(validate_arxiv_id("").is_err());
        assert!(validate_arxiv_id("abc").is_err());
        assert!(validate_arxiv_id("12345").is_err());
    }

    // Integration tests — require network access
    #[tokio::test]
    #[ignore]
    async fn test_real_search() {
        let client = ArxivClient::new().unwrap();
        let params = ArxivSearchParams {
            query: "attention is all you need".to_string(),
            max_results: 3,
            ..Default::default()
        };
        let result = client.search(&params).await.unwrap();
        assert!(!result.papers.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_fetch_attention_paper() {
        let client = ArxivClient::new().unwrap();
        let paper = client.fetch_paper("1706.03762").await.unwrap();
        assert!(paper.title.contains("Attention"));
        assert!(!paper.authors.is_empty());
    }
}
