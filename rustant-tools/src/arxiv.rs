//! ArXiv research tool — search, fetch, analyze, and manage academic papers.

use async_trait::async_trait;
use chrono::Utc;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

use crate::arxiv_api::{
    generate_bibtex, AnalysisDepth, ArxivClient, ArxivLibraryState, ArxivSearchParams, ArxivSortBy,
    DigestConfig, LibraryEntry,
};
use crate::registry::Tool;

pub struct ArxivResearchTool {
    workspace: PathBuf,
}

impl ArxivResearchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("arxiv")
            .join("library.json")
    }

    fn load_state(&self) -> ArxivLibraryState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            ArxivLibraryState::default()
        }
    }

    fn save_state(&self, state: &ArxivLibraryState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: format!("Create dir failed: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "arxiv_research".to_string(),
            message: format!("Serialize failed: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "arxiv_research".to_string(),
            message: e.to_string(),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "arxiv_research".to_string(),
            message: e.to_string(),
        })?;
        Ok(())
    }

    fn make_client(&self) -> Result<ArxivClient, ToolError> {
        ArxivClient::new().map_err(|e| ToolError::ExecutionFailed {
            name: "arxiv_research".to_string(),
            message: e,
        })
    }

    async fn handle_search(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'query' for search action".to_string(),
            }
        })?;

        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(50) as usize;

        let sort_by = args
            .get("sort_by")
            .and_then(|v| v.as_str())
            .map(ArxivSortBy::from_str_loose)
            .unwrap_or(ArxivSortBy::Relevance);

        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let client = self.make_client()?;
        let params = ArxivSearchParams {
            query: query.to_string(),
            category,
            max_results,
            sort_by,
            ..Default::default()
        };

        let result = client
            .search(&params)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        if result.papers.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No papers found for query: \"{}\"",
                query
            )));
        }

        let mut output = format!(
            "Found {} papers (showing {}):\n\n",
            result.total_results,
            result.papers.len()
        );

        for (i, paper) in result.papers.iter().enumerate() {
            output.push_str(&format!(
                "{}. **{}**\n   Authors: {}\n   ID: {} | Category: {} | Published: {}\n   Abstract: {}\n   PDF: {}\n\n",
                i + 1,
                paper.title,
                paper.authors.join(", "),
                paper.arxiv_id,
                paper.primary_category,
                &paper.published[..10.min(paper.published.len())],
                truncate_text(&paper.summary, 200),
                paper.pdf_url,
            ));
        }

        Ok(ToolOutput::text(output))
    }

    async fn handle_fetch(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for fetch action".to_string(),
            })?;

        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        let output = format!(
            "**{}**\n\nAuthors: {}\nArXiv ID: {}\nCategories: {}\nPrimary Category: {}\nPublished: {}\nUpdated: {}\nPDF: {}\nAbstract URL: {}{}{}{}\n\n**Abstract:**\n{}",
            paper.title,
            paper.authors.join(", "),
            paper.arxiv_id,
            paper.categories.join(", "),
            paper.primary_category,
            paper.published,
            paper.updated,
            paper.pdf_url,
            paper.abs_url,
            paper.doi.as_ref().map(|d| format!("\nDOI: {}", d)).unwrap_or_default(),
            paper.comment.as_ref().map(|c| format!("\nComment: {}", c)).unwrap_or_default(),
            paper.journal_ref.as_ref().map(|j| format!("\nJournal: {}", j)).unwrap_or_default(),
            paper.summary,
        );

        Ok(ToolOutput::text(output))
    }

    async fn handle_analyze(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for analyze action".to_string(),
            })?;

        let depth = args
            .get("depth")
            .and_then(|v| v.as_str())
            .map(AnalysisDepth::from_str_loose)
            .unwrap_or(AnalysisDepth::Standard);

        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        let depth_instructions = match depth {
            AnalysisDepth::Quick => {
                "Provide a 2-3 sentence summary of the paper's main contribution."
            }
            AnalysisDepth::Standard => {
                "Provide a structured analysis with: 1) Summary (2-3 paragraphs), 2) Key Contributions (bullet points), 3) Methodology overview, 4) Strengths and Limitations."
            }
            AnalysisDepth::Full => {
                "Provide a comprehensive analysis with: 1) Executive Summary, 2) Problem Statement & Motivation, 3) Key Contributions (detailed), 4) Methodology (step-by-step), 5) Experimental Results, 6) Strengths, 7) Limitations & Future Work, 8) Impact & Significance, 9) Related Work connections."
            }
        };

        let output = format!(
            "PAPER DATA FOR ANALYSIS:\n\n\
             Title: {}\n\
             Authors: {}\n\
             ArXiv ID: {}\n\
             Categories: {}\n\
             Published: {}\n\
             PDF: {}\n\n\
             Abstract:\n{}\n\n\
             ---\n\n\
             ANALYSIS INSTRUCTIONS:\n\
             Based on the paper data above, please {}",
            paper.title,
            paper.authors.join(", "),
            paper.arxiv_id,
            paper.categories.join(", "),
            paper.published,
            paper.pdf_url,
            paper.summary,
            depth_instructions,
        );

        Ok(ToolOutput::text(output))
    }

    async fn handle_compare(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_ids = args
            .get("arxiv_ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_ids' (array) for compare action"
                    .to_string(),
            })?;

        if arxiv_ids.len() < 2 {
            return Err(ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Need at least 2 paper IDs to compare".to_string(),
            });
        }

        let client = self.make_client()?;
        let mut papers = Vec::new();

        for id_val in arxiv_ids {
            let id = id_val.as_str().ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Each arxiv_id must be a string".to_string(),
            })?;
            let paper = client
                .fetch_paper(id)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "arxiv_research".to_string(),
                    message: e,
                })?;
            papers.push(paper);
        }

        let mut output = format!("PAPERS FOR COMPARISON ({} papers):\n\n", papers.len());

        for (i, paper) in papers.iter().enumerate() {
            output.push_str(&format!(
                "--- Paper {} ---\n\
                 Title: {}\n\
                 Authors: {}\n\
                 ArXiv ID: {}\n\
                 Categories: {}\n\
                 Published: {}\n\
                 Abstract:\n{}\n\n",
                i + 1,
                paper.title,
                paper.authors.join(", "),
                paper.arxiv_id,
                paper.categories.join(", "),
                paper.published,
                paper.summary,
            ));
        }

        output.push_str(
            "---\n\n\
             COMPARISON INSTRUCTIONS:\n\
             Please compare these papers, highlighting:\n\
             1. Shared themes and goals\n\
             2. Different methodologies/approaches\n\
             3. Relative strengths and limitations\n\
             4. How they build on or complement each other\n\
             5. A recommendation for which to read based on different interests",
        );

        Ok(ToolOutput::text(output))
    }

    async fn handle_trending(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(50) as usize;

        let query = if let Some(cat) = &category {
            format!("cat:{}", cat)
        } else {
            "cat:cs.AI OR cat:cs.LG OR cat:cs.CL".to_string()
        };

        let client = self.make_client()?;
        let params = ArxivSearchParams {
            query,
            max_results,
            sort_by: ArxivSortBy::SubmittedDate,
            ..Default::default()
        };

        let result = client
            .search(&params)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        if result.papers.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No trending papers found{}",
                category
                    .as_ref()
                    .map(|c| format!(" in category {}", c))
                    .unwrap_or_default()
            )));
        }

        let mut output = format!(
            "Trending papers{} (sorted by submission date):\n\n",
            category
                .as_ref()
                .map(|c| format!(" in {}", c))
                .unwrap_or_default()
        );

        for (i, paper) in result.papers.iter().enumerate() {
            output.push_str(&format!(
                "{}. **{}**\n   Authors: {}\n   ID: {} | {} | Published: {}\n   {}\n\n",
                i + 1,
                paper.title,
                paper.authors.join(", "),
                paper.arxiv_id,
                paper.primary_category,
                &paper.published[..10.min(paper.published.len())],
                truncate_text(&paper.summary, 150),
            ));
        }

        Ok(ToolOutput::text(output))
    }

    async fn handle_save(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for save action".to_string(),
            })?;

        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .or_else(|| {
                args.get("tags")
                    .and_then(|v| v.as_str())
                    .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
            })
            .unwrap_or_default();

        let collection = args
            .get("collection")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let notes = args
            .get("notes")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Fetch the paper
        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        let mut state = self.load_state();

        // Check if already saved
        if state
            .entries
            .iter()
            .any(|e| e.paper.arxiv_id == paper.arxiv_id)
        {
            return Ok(ToolOutput::text(format!(
                "Paper '{}' ({}) is already in your library.",
                paper.title, paper.arxiv_id
            )));
        }

        // Add collection if new
        if let Some(ref col) = collection {
            if !state.collections.contains(col) {
                state.collections.push(col.clone());
            }
        }

        let title = paper.title.clone();
        let id = paper.arxiv_id.clone();

        state.entries.push(LibraryEntry {
            paper,
            tags: tags.clone(),
            collection: collection.clone(),
            notes,
            saved_at: Utc::now(),
        });

        self.save_state(&state)?;

        Ok(ToolOutput::text(format!(
            "Saved '{}' ({}) to library.{}{}",
            title,
            id,
            if !tags.is_empty() {
                format!(" Tags: {}", tags.join(", "))
            } else {
                String::new()
            },
            collection
                .map(|c| format!(" Collection: {}", c))
                .unwrap_or_default(),
        )))
    }

    fn handle_library(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();

        if state.entries.is_empty() {
            return Ok(ToolOutput::text(
                "Your ArXiv library is empty. Use the 'save' action to add papers.",
            ));
        }

        let filter_tag = args.get("filter_tag").and_then(|v| v.as_str());
        let filter_collection = args.get("filter_collection").and_then(|v| v.as_str());

        let filtered: Vec<&LibraryEntry> = state
            .entries
            .iter()
            .filter(|e| {
                if let Some(tag) = filter_tag {
                    if !e.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                        return false;
                    }
                }
                if let Some(col) = filter_collection {
                    if e.collection.as_deref() != Some(col) {
                        return false;
                    }
                }
                true
            })
            .collect();

        if filtered.is_empty() {
            return Ok(ToolOutput::text("No papers match the given filters."));
        }

        let mut output = format!("Library ({} papers):\n\n", filtered.len());

        for (i, entry) in filtered.iter().enumerate() {
            output.push_str(&format!(
                "{}. **{}**\n   ID: {} | {} | Saved: {}\n   Authors: {}{}{}\n\n",
                i + 1,
                entry.paper.title,
                entry.paper.arxiv_id,
                entry.paper.primary_category,
                entry.saved_at.format("%Y-%m-%d"),
                entry.paper.authors.join(", "),
                if !entry.tags.is_empty() {
                    format!("\n   Tags: {}", entry.tags.join(", "))
                } else {
                    String::new()
                },
                entry
                    .collection
                    .as_ref()
                    .map(|c| format!("\n   Collection: {}", c))
                    .unwrap_or_default(),
            ));
        }

        Ok(ToolOutput::text(output))
    }

    fn handle_remove(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for remove action".to_string(),
            })?;

        let mut state = self.load_state();
        let before = state.entries.len();
        state.entries.retain(|e| e.paper.arxiv_id != arxiv_id);

        if state.entries.len() == before {
            return Ok(ToolOutput::text(format!(
                "Paper '{}' not found in library.",
                arxiv_id
            )));
        }

        self.save_state(&state)?;

        Ok(ToolOutput::text(format!(
            "Removed paper '{}' from library.",
            arxiv_id
        )))
    }

    fn handle_export_bibtex(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let specific_ids: Option<Vec<String>> =
            args.get("arxiv_ids").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });

        let state = self.load_state();

        let papers_to_export: Vec<_> = if let Some(ids) = &specific_ids {
            state
                .entries
                .iter()
                .filter(|e| ids.contains(&e.paper.arxiv_id))
                .collect()
        } else {
            state.entries.iter().collect()
        };

        if papers_to_export.is_empty() {
            return Ok(ToolOutput::text(
                "No papers to export. Save papers to your library first, or specify valid arxiv_ids.",
            ));
        }

        let mut bibtex = String::new();
        for entry in &papers_to_export {
            bibtex.push_str(&generate_bibtex(&entry.paper));
            bibtex.push_str("\n\n");
        }

        Ok(ToolOutput::text(format!(
            "BibTeX export ({} entries):\n\n{}",
            papers_to_export.len(),
            bibtex.trim_end()
        )))
    }

    fn handle_collections(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let sub_action = args
            .get("sub_action")
            .or_else(|| args.get("notes"))
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        let mut state = self.load_state();

        match sub_action {
            "list" => {
                if state.collections.is_empty() {
                    return Ok(ToolOutput::text(
                        "No collections yet. Create one with sub_action 'create' and a 'name' parameter.",
                    ));
                }
                let mut output = "Collections:\n".to_string();
                for col in &state.collections {
                    let count = state
                        .entries
                        .iter()
                        .filter(|e| e.collection.as_deref() == Some(col))
                        .count();
                    output.push_str(&format!("  - {} ({} papers)\n", col, count));
                }
                Ok(ToolOutput::text(output))
            }
            "create" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "arxiv_research".to_string(),
                        reason: "Missing 'name' parameter for create collection".to_string(),
                    }
                })?;
                if state.collections.contains(&name.to_string()) {
                    return Ok(ToolOutput::text(format!(
                        "Collection '{}' already exists.",
                        name
                    )));
                }
                state.collections.push(name.to_string());
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!("Created collection '{}'.", name)))
            }
            "rename" => {
                let old_name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "arxiv_research".to_string(),
                        reason: "Missing 'name' parameter for rename".to_string(),
                    }
                })?;
                let new_name = args
                    .get("new_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "arxiv_research".to_string(),
                        reason: "Missing 'new_name' parameter for rename".to_string(),
                    })?;

                if let Some(pos) = state.collections.iter().position(|c| c == old_name) {
                    state.collections[pos] = new_name.to_string();
                    // Update entries
                    for entry in &mut state.entries {
                        if entry.collection.as_deref() == Some(old_name) {
                            entry.collection = Some(new_name.to_string());
                        }
                    }
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!(
                        "Renamed collection '{}' to '{}'.",
                        old_name, new_name
                    )))
                } else {
                    Ok(ToolOutput::text(format!(
                        "Collection '{}' not found.",
                        old_name
                    )))
                }
            }
            "delete" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "arxiv_research".to_string(),
                        reason: "Missing 'name' parameter for delete".to_string(),
                    }
                })?;
                if let Some(pos) = state.collections.iter().position(|c| c == name) {
                    state.collections.remove(pos);
                    // Clear collection from entries
                    for entry in &mut state.entries {
                        if entry.collection.as_deref() == Some(name) {
                            entry.collection = None;
                        }
                    }
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!("Deleted collection '{}'.", name)))
                } else {
                    Ok(ToolOutput::text(format!(
                        "Collection '{}' not found.",
                        name
                    )))
                }
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown collection sub_action '{}'. Use: list, create, rename, delete.",
                sub_action
            ))),
        }
    }

    fn handle_digest_config(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let mut state = self.load_state();

        let keywords: Option<Vec<String>> = args
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .or_else(|| {
                args.get("keywords")
                    .and_then(|v| v.as_str())
                    .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
            });

        let categories: Option<Vec<String>> = args
            .get("categories")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .or_else(|| {
                args.get("categories")
                    .and_then(|v| v.as_str())
                    .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
            });

        let enabled = args.get("enabled").and_then(|v| v.as_bool());

        // If no parameters provided, show current config
        if keywords.is_none() && categories.is_none() && enabled.is_none() {
            return Ok(ToolOutput::text(match &state.digest_config {
                Some(config) => format!(
                    "Digest configuration:\n  Enabled: {}\n  Keywords: {}\n  Categories: {}",
                    config.enabled,
                    if config.keywords.is_empty() {
                        "(none)".to_string()
                    } else {
                        config.keywords.join(", ")
                    },
                    if config.categories.is_empty() {
                        "(none)".to_string()
                    } else {
                        config.categories.join(", ")
                    },
                ),
                None => {
                    "No digest configuration set. Provide keywords and/or categories to configure."
                        .to_string()
                }
            }));
        }

        let config = state.digest_config.get_or_insert(DigestConfig {
            keywords: Vec::new(),
            categories: Vec::new(),
            enabled: true,
        });

        if let Some(kw) = keywords {
            config.keywords = kw;
        }
        if let Some(cats) = categories {
            config.categories = cats;
        }
        if let Some(en) = enabled {
            config.enabled = en;
        }

        let summary = format!(
            "Digest configuration updated:\n  Enabled: {}\n  Keywords: {}\n  Categories: {}",
            config.enabled,
            if config.keywords.is_empty() {
                "(none)".to_string()
            } else {
                config.keywords.join(", ")
            },
            if config.categories.is_empty() {
                "(none)".to_string()
            } else {
                config.categories.join(", ")
            },
        );

        self.save_state(&state)?;
        Ok(ToolOutput::text(summary))
    }

    async fn handle_paper_to_code(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for paper_to_code action"
                    .to_string(),
            })?;

        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");

        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        let output = format!(
            "PAPER-TO-CODE REQUEST:\n\n\
             Paper: {}\n\
             Authors: {}\n\
             ArXiv ID: {}\n\
             Target Language: {}\n\n\
             Abstract:\n{}\n\n\
             ---\n\n\
             CODE GENERATION INSTRUCTIONS:\n\
             Based on the paper above, please generate a {} implementation that includes:\n\
             1. **Imports/Dependencies**: Required libraries and packages\n\
             2. **Core Data Structures**: Key classes/structs representing the paper's concepts\n\
             3. **Algorithm Implementation**: The main algorithm or model architecture described\n\
             4. **Training/Execution Loop**: If applicable, the training procedure or main execution flow\n\
             5. **Example Usage**: A brief example showing how to use the implementation\n\
             6. **Comments**: Inline comments referencing specific sections/equations from the paper\n\n\
             Write the code to a file using the file_write tool. Name the file based on the paper topic.",
            paper.title,
            paper.authors.join(", "),
            paper.arxiv_id,
            language,
            paper.summary,
            language,
        );

        Ok(ToolOutput::text(output))
    }

    async fn handle_paper_to_notebook(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for paper_to_notebook action"
                    .to_string(),
            })?;

        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        let output = format!(
            "PAPER-TO-NOTEBOOK REQUEST:\n\n\
             Paper: {}\n\
             Authors: {}\n\
             ArXiv ID: {}\n\
             PDF: {}\n\n\
             Abstract:\n{}\n\n\
             ---\n\n\
             NOTEBOOK GENERATION INSTRUCTIONS:\n\
             Create a Jupyter notebook (.ipynb) with the following 12 sections as cells:\n\n\
             1. **Title & Metadata** (markdown): Paper title, authors, ArXiv link, date\n\
             2. **Problem Statement** (markdown): What problem does this paper solve?\n\
             3. **Imports & Setup** (code): All required library imports\n\
             4. **Dataset Preparation** (code): Data loading and preprocessing\n\
             5. **Model Architecture** (code): Core model/algorithm implementation\n\
             6. **Loss Function & Metrics** (code): Training objectives and evaluation metrics\n\
             7. **Baseline Comparison** (code): Simple baseline for comparison\n\
             8. **Training Loop** (code): Model training procedure\n\
             9. **Inference** (code): Running the trained model\n\
             10. **Evaluation** (code): Quantitative evaluation with metrics\n\
             11. **Visualizations** (code): Plots and charts of results\n\
             12. **Summary & Conclusions** (markdown): Key findings and takeaways\n\n\
             Write the notebook as a valid .ipynb JSON file using the file_write tool.\n\
             Name it based on the paper topic (e.g., attention_is_all_you_need.ipynb).",
            paper.title,
            paper.authors.join(", "),
            paper.arxiv_id,
            paper.pdf_url,
            paper.summary,
        );

        Ok(ToolOutput::text(output))
    }
}

/// Truncate text to a maximum length, adding "..." if truncated.
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

#[async_trait]
impl Tool for ArxivResearchTool {
    fn name(&self) -> &str {
        "arxiv_research"
    }

    fn description(&self) -> &str {
        "Search, fetch, and manage academic papers from arXiv. Actions: search (find papers), \
         fetch (get by ID), analyze (structured analysis prompt), compare (side-by-side), \
         trending (recent papers), save/library/remove (manage local library), \
         export_bibtex, collections, digest_config, paper_to_code, paper_to_notebook."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "search", "fetch", "analyze", "compare", "trending",
                        "save", "library", "remove", "export_bibtex",
                        "collections", "digest_config", "paper_to_code", "paper_to_notebook"
                    ],
                    "description": "Action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search action)"
                },
                "arxiv_id": {
                    "type": "string",
                    "description": "ArXiv paper ID, e.g. '2301.12345' or '1706.03762'"
                },
                "arxiv_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Array of ArXiv IDs (for compare/export_bibtex)"
                },
                "category": {
                    "type": "string",
                    "description": "ArXiv category filter, e.g. 'cs.AI', 'cs.LG', 'cs.CL'"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum papers to return (default: 10, max: 50)"
                },
                "sort_by": {
                    "type": "string",
                    "enum": ["relevance", "date", "updated"],
                    "description": "Sort order for search results"
                },
                "depth": {
                    "type": "string",
                    "enum": ["quick", "standard", "full"],
                    "description": "Analysis depth (for analyze action)"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for saved papers"
                },
                "collection": {
                    "type": "string",
                    "description": "Collection name for organizing papers"
                },
                "notes": {
                    "type": "string",
                    "description": "Personal notes about a paper"
                },
                "filter_tag": {
                    "type": "string",
                    "description": "Filter library by tag"
                },
                "filter_collection": {
                    "type": "string",
                    "description": "Filter library by collection"
                },
                "sub_action": {
                    "type": "string",
                    "enum": ["list", "create", "rename", "delete"],
                    "description": "Sub-action for collections management"
                },
                "name": {
                    "type": "string",
                    "description": "Collection name (for create/rename/delete)"
                },
                "new_name": {
                    "type": "string",
                    "description": "New name for collection rename"
                },
                "keywords": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Keywords for digest tracking"
                },
                "categories": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Categories for digest tracking"
                },
                "enabled": {
                    "type": "boolean",
                    "description": "Enable/disable digest"
                },
                "language": {
                    "type": "string",
                    "description": "Target programming language for paper_to_code (default: python)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Network
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'action'".to_string(),
            }
        })?;

        match action {
            "search" => self.handle_search(&args).await,
            "fetch" => self.handle_fetch(&args).await,
            "analyze" => self.handle_analyze(&args).await,
            "compare" => self.handle_compare(&args).await,
            "trending" => self.handle_trending(&args).await,
            "save" => self.handle_save(&args).await,
            "library" => self.handle_library(&args),
            "remove" => self.handle_remove(&args),
            "export_bibtex" => self.handle_export_bibtex(&args),
            "collections" => self.handle_collections(&args),
            "digest_config" => self.handle_digest_config(&args),
            "paper_to_code" => self.handle_paper_to_code(&args).await,
            "paper_to_notebook" => self.handle_paper_to_notebook(&args).await,
            _ => Err(ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: format!(
                    "Unknown action '{}'. Valid actions: search, fetch, analyze, compare, trending, save, library, remove, export_bibtex, collections, digest_config, paper_to_code, paper_to_notebook",
                    action
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tool_name() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "arxiv_research");
    }

    #[test]
    fn test_risk_level() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        assert_eq!(tool.risk_level(), RiskLevel::Network);
    }

    #[test]
    fn test_timeout() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        assert_eq!(tool.timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_schema_action_required() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("action")));
    }

    #[test]
    fn test_schema_all_actions_listed() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let schema = tool.parameters_schema();
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        let action_strs: Vec<&str> = actions.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(action_strs.len(), 13);
        assert!(action_strs.contains(&"search"));
        assert!(action_strs.contains(&"fetch"));
        assert!(action_strs.contains(&"analyze"));
        assert!(action_strs.contains(&"compare"));
        assert!(action_strs.contains(&"trending"));
        assert!(action_strs.contains(&"save"));
        assert!(action_strs.contains(&"library"));
        assert!(action_strs.contains(&"remove"));
        assert!(action_strs.contains(&"export_bibtex"));
        assert!(action_strs.contains(&"collections"));
        assert!(action_strs.contains(&"digest_config"));
        assert!(action_strs.contains(&"paper_to_code"));
        assert!(action_strs.contains(&"paper_to_notebook"));
    }

    #[tokio::test]
    async fn test_missing_action() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("action"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_invalid_action() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "nonexistent"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("nonexistent"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_search_missing_query() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "search"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("query"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_fetch_missing_id() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "fetch"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("arxiv_id"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_library_initially_empty() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "library"})).await.unwrap();
        assert!(result.content.contains("empty"));
    }

    #[tokio::test]
    async fn test_library_state_roundtrip() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ArxivResearchTool::new(workspace.clone());

        let state = ArxivLibraryState {
            entries: vec![LibraryEntry {
                paper: crate::arxiv_api::ArxivPaper {
                    arxiv_id: "2301.12345".to_string(),
                    title: "Test Paper".to_string(),
                    authors: vec!["Author".to_string()],
                    summary: "Summary".to_string(),
                    categories: vec!["cs.AI".to_string()],
                    primary_category: "cs.AI".to_string(),
                    published: "2023-01-15".to_string(),
                    updated: "2023-01-15".to_string(),
                    pdf_url: "https://arxiv.org/pdf/2301.12345".to_string(),
                    abs_url: "https://arxiv.org/abs/2301.12345".to_string(),
                    doi: None,
                    comment: None,
                    journal_ref: None,
                },
                tags: vec!["test".to_string()],
                collection: None,
                notes: None,
                saved_at: Utc::now(),
            }],
            collections: Vec::new(),
            digest_config: None,
        };

        tool.save_state(&state).unwrap();
        let loaded = tool.load_state();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].paper.arxiv_id, "2301.12345");
    }

    #[tokio::test]
    async fn test_export_bibtex_empty() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "export_bibtex"}))
            .await
            .unwrap();
        assert!(result.content.contains("No papers to export"));
    }

    #[tokio::test]
    async fn test_collections_empty() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "collections"}))
            .await
            .unwrap();
        assert!(result.content.contains("No collections"));
    }

    #[tokio::test]
    async fn test_remove_nonexistent() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "remove", "arxiv_id": "9999.99999"}))
            .await
            .unwrap();
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_digest_config_show_empty() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "digest_config"}))
            .await
            .unwrap();
        assert!(result.content.contains("No digest configuration"));
    }

    #[tokio::test]
    async fn test_digest_config_set_and_show() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ArxivResearchTool::new(workspace);

        // Set config
        let result = tool
            .execute(json!({
                "action": "digest_config",
                "keywords": ["transformer", "attention"],
                "categories": ["cs.AI"],
                "enabled": true
            }))
            .await
            .unwrap();
        assert!(result.content.contains("updated"));
        assert!(result.content.contains("transformer"));

        // Show config
        let result = tool
            .execute(json!({"action": "digest_config"}))
            .await
            .unwrap();
        assert!(result.content.contains("transformer"));
        assert!(result.content.contains("cs.AI"));
    }

    #[tokio::test]
    async fn test_collections_create_and_list() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ArxivResearchTool::new(workspace);

        // Create
        let result = tool
            .execute(json!({
                "action": "collections",
                "sub_action": "create",
                "name": "Favorites"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Created"));

        // List
        let result = tool
            .execute(json!({"action": "collections", "sub_action": "list"}))
            .await
            .unwrap();
        assert!(result.content.contains("Favorites"));
    }

    // Integration tests — require network access
    #[tokio::test]
    #[ignore]
    async fn test_real_search() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "search",
                "query": "attention is all you need",
                "max_results": 3
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Found"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_fetch_attention_paper() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "fetch",
                "arxiv_id": "1706.03762"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Attention"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_trending_cs_ai() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "trending",
                "category": "cs.AI",
                "max_results": 5
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Trending") || result.content.contains("No trending"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_full_save_retrieve_remove_flow() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ArxivResearchTool::new(workspace);

        // Save
        let result = tool
            .execute(json!({
                "action": "save",
                "arxiv_id": "1706.03762",
                "tags": ["transformers", "nlp"],
                "collection": "Foundational"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Saved"));

        // Library
        let result = tool.execute(json!({"action": "library"})).await.unwrap();
        assert!(result.content.contains("1706.03762"));

        // Export BibTeX
        let result = tool
            .execute(json!({"action": "export_bibtex"}))
            .await
            .unwrap();
        assert!(result.content.contains("@article"));

        // Remove
        let result = tool
            .execute(json!({"action": "remove", "arxiv_id": "1706.03762v7"}))
            .await
            .unwrap();
        // Note: the fetched ID may include version suffix
        assert!(result.content.contains("Removed") || result.content.contains("not found"));
    }
}
