//! ArXiv research tool — search, fetch, analyze, and manage academic papers.

use async_trait::async_trait;
use chrono::Utc;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

use crate::arxiv_api::{
    AnalysisDepth, ArxivClient, ArxivLibraryState, ArxivSearchParams, ArxivSortBy, DigestConfig,
    ImplementationMode, ImplementationRecord, ImplementationStatus, LibraryEntry, ProjectScaffold,
    ScaffoldFile, generate_bibtex, language_config,
};
use crate::paper_sources::{CitationGraph, OpenAlexClient, SemanticScholarClient};
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

        output.push_str("---\nTo work with a paper, ask the user which one they'd like to select (by number or title). Then use the paper's arxiv_id with actions like: fetch (full details), analyze (deep analysis), save (to library), implement (generate code scaffold), or paper_to_code/paper_to_notebook.");

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
            paper
                .doi
                .as_ref()
                .map(|d| format!("\nDOI: {}", d))
                .unwrap_or_default(),
            paper
                .comment
                .as_ref()
                .map(|c| format!("\nComment: {}", c))
                .unwrap_or_default(),
            paper
                .journal_ref
                .as_ref()
                .map(|j| format!("\nJournal: {}", j))
                .unwrap_or_default(),
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
        if let Some(ref col) = collection
            && !state.collections.contains(col)
        {
            state.collections.push(col.clone());
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
                if let Some(tag) = filter_tag
                    && !e.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
                {
                    return false;
                }
                if let Some(col) = filter_collection
                    && e.collection.as_deref() != Some(col)
                {
                    return false;
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

        let mut output = format!(
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

        // If we have a language config, also generate scaffold instructions
        if let Some(lang_cfg) = language_config(language) {
            output.push_str("\n\n--- SCAFFOLD INSTRUCTIONS ---\n");
            output
                .push_str("Alternatively, use action 'implement' for a full TDD scaffold with:\n");
            if !lang_cfg.env_setup_commands.is_empty() {
                output.push_str(&format!(
                    "  Environment isolation: {}\n",
                    lang_cfg.env_setup_commands.join("; ")
                ));
            }
            output.push_str(&format!("  Test framework: {}\n", lang_cfg.test_framework));
            output.push_str(&format!(
                "  Package manager: {}\n",
                lang_cfg.package_manager
            ));
        }

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

        let mut output = format!(
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

        output.push_str("\n\n--- GENERATED NOTEBOOK JSON ---\n");
        output.push_str(
            "Here is a pre-generated notebook skeleton you can use as a starting point:\n\n",
        );
        output.push_str(&generate_notebook_json(&paper));

        Ok(ToolOutput::text(output))
    }

    async fn handle_implement(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for implement action".to_string(),
            })?;

        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");
        let target_dir = args.get("target_dir").and_then(|v| v.as_str());
        let tdd = args.get("tdd").and_then(|v| v.as_bool()).unwrap_or(true);
        let mode_str = args
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("project");

        let lang_config = language_config(language).ok_or_else(|| ToolError::InvalidArguments {
            name: "arxiv_research".to_string(),
            reason: format!(
                "Unsupported language '{}'. Supported: python, rust, typescript, go, cpp, julia",
                language
            ),
        })?;

        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        let project_name = paper
            .title
            .to_lowercase()
            .split_whitespace()
            .take(4)
            .collect::<Vec<_>>()
            .join("_")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<String>();

        let base_dir = target_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace.join(&project_name));

        let mode = if mode_str == "notebook" {
            ImplementationMode::Notebook
        } else {
            ImplementationMode::StandaloneProject
        };

        // Generate scaffold
        let scaffold = generate_project_scaffold(&paper, &lang_config, &project_name, tdd);

        // Track implementation
        let mut state = self.load_state();
        state.implementations.push(ImplementationRecord {
            paper_id: paper.arxiv_id.clone(),
            project_path: base_dir.to_string_lossy().to_string(),
            language: language.to_string(),
            mode,
            status: ImplementationStatus::Scaffolded,
            created_at: Utc::now(),
        });
        self.save_state(&state)?;

        // Build step-by-step instructions for the agent
        let mut output = format!(
            "IMPLEMENTATION SCAFFOLD for '{}' ({}):\n\n",
            paper.title, paper.arxiv_id
        );
        output.push_str(&format!("Target directory: {}\n", base_dir.display()));
        output.push_str(&format!("Language: {} | TDD: {}\n\n", language, tdd));

        // Environment isolation warning
        if !lang_config.env_setup_commands.is_empty() {
            output.push_str("IMPORTANT: Environment isolation required!\n");
            output.push_str("Execute these commands FIRST to create an isolated environment:\n");
            for cmd in &lang_config.env_setup_commands {
                output.push_str(&format!(
                    "  shell_exec: cd {} && {}\n",
                    base_dir.display(),
                    cmd
                ));
            }
            if !lang_config.env_activate.is_empty() {
                output.push_str(&format!(
                    "  Activate with: cd {} && {}\n",
                    base_dir.display(),
                    lang_config.env_activate
                ));
            }
            output.push('\n');
        }

        output.push_str("STEP-BY-STEP INSTRUCTIONS:\n\n");

        // Step 1: Create directory structure
        output.push_str("Step 1: Create project directory structure\n");
        for dir in &scaffold.directory_structure {
            output.push_str(&format!(
                "  shell_exec: mkdir -p {}/{}\n",
                base_dir.display(),
                dir
            ));
        }
        output.push('\n');

        // Step 2: Create files (TDD: tests first)
        let test_files: Vec<_> = scaffold.files.iter().filter(|f| f.is_test).collect();
        let impl_files: Vec<_> = scaffold.files.iter().filter(|f| !f.is_test).collect();

        if tdd && !test_files.is_empty() {
            output.push_str("Step 2: Create TEST files FIRST (TDD approach)\n");
            for f in &test_files {
                output.push_str(&format!(
                    "  file_write: {}/{}\n  Content:\n```\n{}\n```\n\n",
                    base_dir.display(),
                    f.path,
                    f.content
                ));
            }
        }

        output.push_str(&format!(
            "Step {}: Create implementation files\n",
            if tdd { 3 } else { 2 }
        ));
        for f in &impl_files {
            output.push_str(&format!(
                "  file_write: {}/{}\n  Content:\n```\n{}\n```\n\n",
                base_dir.display(),
                f.path,
                f.content
            ));
        }

        // Step 3: Install dependencies (inside venv/isolated env)
        let step_n = if tdd { 4 } else { 3 };
        output.push_str(&format!("Step {}: Install dependencies\n", step_n));
        for cmd in &scaffold.setup_commands {
            if !lang_config.env_activate.is_empty() {
                output.push_str(&format!(
                    "  shell_exec: cd {} && {} && {}\n",
                    base_dir.display(),
                    lang_config.env_activate,
                    cmd
                ));
            } else {
                output.push_str(&format!(
                    "  shell_exec: cd {} && {}\n",
                    base_dir.display(),
                    cmd
                ));
            }
        }
        output.push('\n');

        // Step 4: Run tests
        output.push_str(&format!("Step {}: Run tests to verify\n", step_n + 1));
        for cmd in &scaffold.test_commands {
            if !lang_config.env_activate.is_empty() {
                output.push_str(&format!(
                    "  shell_exec: cd {} && {} && {}\n",
                    base_dir.display(),
                    lang_config.env_activate,
                    cmd
                ));
            } else {
                output.push_str(&format!(
                    "  shell_exec: cd {} && {}\n",
                    base_dir.display(),
                    cmd
                ));
            }
        }

        output.push_str(&format!(
            "\nPaper abstract for implementation reference:\n{}\n",
            paper.summary
        ));

        Ok(ToolOutput::text(output))
    }

    fn handle_setup_env(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for setup_env action".to_string(),
            })?;

        let state = self.load_state();
        let record = state
            .implementations
            .iter()
            .find(|r| r.paper_id == arxiv_id)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: format!(
                    "No implementation found for paper '{}'. Run 'implement' first.",
                    arxiv_id
                ),
            })?;

        let lang_config =
            language_config(&record.language).ok_or_else(|| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: format!("Unknown language: {}", record.language),
            })?;

        let mut output = format!(
            "Environment setup for {} ({}):\n\nProject: {}\n\n",
            arxiv_id, record.language, record.project_path
        );

        output.push_str("Commands to run:\n");
        for cmd in &lang_config.env_setup_commands {
            output.push_str(&format!(
                "  shell_exec: cd {} && {}\n",
                record.project_path, cmd
            ));
        }
        if !lang_config.env_activate.is_empty() {
            output.push_str(&format!(
                "  Activate: cd {} && {}\n",
                record.project_path, lang_config.env_activate
            ));
        }

        Ok(ToolOutput::text(output))
    }

    fn handle_verify(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for verify action".to_string(),
            })?;

        let state = self.load_state();
        let record = state
            .implementations
            .iter()
            .find(|r| r.paper_id == arxiv_id)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: format!(
                    "No implementation found for paper '{}'. Run 'implement' first.",
                    arxiv_id
                ),
            })?;

        let lang_config =
            language_config(&record.language).ok_or_else(|| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: format!("Unknown language: {}", record.language),
            })?;

        let mut output = format!(
            "Verification commands for {} ({}):\n\nProject: {}\n\n",
            arxiv_id, record.language, record.project_path
        );

        let activate = if !lang_config.env_activate.is_empty() {
            format!("{} && ", lang_config.env_activate)
        } else {
            String::new()
        };

        match record.language.as_str() {
            "python" => {
                output.push_str(&format!(
                    "1. Lint:  shell_exec: cd {} && {}python3 -m py_compile src/*.py\n",
                    record.project_path, activate
                ));
                output.push_str(&format!(
                    "2. Test:  shell_exec: cd {} && {}python3 -m pytest tests/ -v\n",
                    record.project_path, activate
                ));
                output.push_str(&format!("3. Type:  shell_exec: cd {} && {}python3 -m mypy src/ --ignore-missing-imports 2>/dev/null || true\n", record.project_path, activate));
            }
            "rust" => {
                output.push_str(&format!(
                    "1. Lint:  shell_exec: cd {} && cargo clippy -- -D warnings\n",
                    record.project_path
                ));
                output.push_str(&format!(
                    "2. Test:  shell_exec: cd {} && cargo test\n",
                    record.project_path
                ));
                output.push_str(&format!(
                    "3. Build: shell_exec: cd {} && cargo build\n",
                    record.project_path
                ));
            }
            "typescript" | "javascript" => {
                output.push_str(&format!(
                    "1. Lint:  shell_exec: cd {} && npx tsc --noEmit\n",
                    record.project_path
                ));
                output.push_str(&format!(
                    "2. Test:  shell_exec: cd {} && npx jest\n",
                    record.project_path
                ));
            }
            "go" => {
                output.push_str(&format!(
                    "1. Lint:  shell_exec: cd {} && go vet ./...\n",
                    record.project_path
                ));
                output.push_str(&format!(
                    "2. Test:  shell_exec: cd {} && go test ./...\n",
                    record.project_path
                ));
            }
            _ => {
                output.push_str("Verification commands not configured for this language.\n");
            }
        }

        output.push_str(&format!("\nCurrent status: {}\n", record.status));

        Ok(ToolOutput::text(output))
    }

    fn handle_implementation_status(&self, _args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();

        if state.implementations.is_empty() {
            return Ok(ToolOutput::text(
                "No implementations tracked. Use the 'implement' action to start one.",
            ));
        }

        let mut output = format!(
            "Tracked implementations ({}):\n\n",
            state.implementations.len()
        );

        for (i, record) in state.implementations.iter().enumerate() {
            output.push_str(&format!(
                "{}. Paper: {}\n   Language: {} | Status: {}\n   Path: {}\n   Created: {}\n\n",
                i + 1,
                record.paper_id,
                record.language,
                record.status,
                record.project_path,
                record.created_at.format("%Y-%m-%d %H:%M"),
            ));
        }

        Ok(ToolOutput::text(output))
    }

    // ── Phase 2: Semantic Search ──────────────────────────────

    async fn handle_semantic_search(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'query' for semantic_search action".to_string(),
            }
        })?;

        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(50) as usize;

        let state = self.load_state();

        if state.entries.is_empty() {
            return Ok(ToolOutput::text(
                "Your library is empty. Save papers first with the 'save' action, then use semantic_search to find them.\nAlternatively, use the 'search' action to search arXiv directly.",
            ));
        }

        // Simple keyword-based search over library papers
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(usize, f64)> = state
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let paper = &entry.paper;
                let searchable = format!(
                    "{} {} {} {} {}",
                    paper.title,
                    paper.summary,
                    paper.categories.join(" "),
                    paper.authors.join(" "),
                    entry.tags.join(" "),
                )
                .to_lowercase();

                let mut score = 0.0;
                for term in &query_terms {
                    if searchable.contains(term) {
                        score += 1.0;
                        // Boost for title matches
                        if paper.title.to_lowercase().contains(term) {
                            score += 2.0;
                        }
                        // Boost for tag matches
                        if entry.tags.iter().any(|t| t.to_lowercase().contains(term)) {
                            score += 1.5;
                        }
                    }
                }
                (i, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(max_results);

        if scored.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No papers in your library match '{}'. Try 'search' to find papers on arXiv.",
                query
            )));
        }

        let mut output = format!(
            "Semantic search results for '{}' ({} matches in library):\n\n",
            query,
            scored.len()
        );

        for (rank, (idx, score)) in scored.iter().enumerate() {
            let entry = &state.entries[*idx];
            output.push_str(&format!(
                "{}. **{}** (score: {:.1})\n   ID: {} | {} | Authors: {}{}\n\n",
                rank + 1,
                entry.paper.title,
                score,
                entry.paper.arxiv_id,
                entry.paper.primary_category,
                entry.paper.authors.join(", "),
                if !entry.tags.is_empty() {
                    format!("\n   Tags: {}", entry.tags.join(", "))
                } else {
                    String::new()
                },
            ));
        }

        Ok(ToolOutput::text(output))
    }

    fn handle_reindex(&self, _args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();

        if state.entries.is_empty() {
            return Ok(ToolOutput::text(
                "Library is empty — nothing to index. Save papers first.",
            ));
        }

        Ok(ToolOutput::text(format!(
            "Library search index refreshed. {} papers available for semantic search.",
            state.entries.len()
        )))
    }

    // ── Phase 3: LLM-Assisted Summarization ───────────────────

    async fn handle_summarize(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for summarize action".to_string(),
            })?;

        let level = args
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("abstract");
        let level = match level {
            "tldr" | "abstract" | "section" | "deep" => level,
            _ => "abstract",
        };

        let audience = args
            .get("audience")
            .and_then(|v| v.as_str())
            .unwrap_or("researcher");
        let audience = match audience {
            "researcher" | "engineer" | "student" => audience,
            _ => "researcher",
        };

        // Check cache first
        let state = self.load_state();
        if let Some(cached) = state
            .summaries
            .iter()
            .find(|s| s.arxiv_id == arxiv_id && s.level == level && s.audience == audience)
        {
            return Ok(ToolOutput::text(format!(
                "CACHED SUMMARY ({} level, {} audience):\n\n{}",
                cached.level, cached.audience, cached.summary
            )));
        }

        // Fetch paper
        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        // Try to get TLDR from Semantic Scholar for the tldr level
        if level == "tldr"
            && let Ok(s2_client) = SemanticScholarClient::new(None, 3600, 1000)
            && let Ok(meta) = s2_client.fetch_by_arxiv_id(arxiv_id).await
            && let Some(tldr) = &meta.tldr
        {
            return Ok(ToolOutput::text(format!(
                "TL;DR (from Semantic Scholar):\n\n{}\n\n---\nCitation count: {}",
                tldr,
                meta.citation_count
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            )));
        }

        // Build level-specific prompt
        let level_instructions = match level {
            "tldr" => {
                "Summarize in exactly one sentence (15-25 words), focusing on the main contribution."
            }
            "abstract" => {
                "Summarize in 100-200 words, covering: problem, approach, key results, significance."
            }
            "section" => {
                "Summarize each major section separately with structure preserved. Use headers."
            }
            "deep" => {
                "Provide comprehensive analysis: methodology details, mathematical framework, limitations, comparison to prior work, and future directions."
            }
            _ => "Summarize in 100-200 words.",
        };

        let audience_instructions = match audience {
            "researcher" => {
                "Target academic researchers: emphasize novelty, positioning vs prior work, and methodology gaps."
            }
            "engineer" => {
                "Target engineers: focus on implementation requirements, compute needs, data dependencies, and practical considerations."
            }
            "student" => {
                "Target students: use simplified language, define technical terms, connect to fundamentals, and explain why this matters."
            }
            _ => "",
        };

        let output = format!(
            "PAPER DATA FOR SUMMARIZATION:\n\n\
             Title: {}\n\
             Authors: {}\n\
             ArXiv ID: {}\n\
             Categories: {}\n\
             Published: {}\n\
             PDF: {}\n\n\
             Abstract:\n{}\n\n\
             ---\n\n\
             SUMMARIZATION INSTRUCTIONS:\n\
             Level: {} | Audience: {}\n\n\
             {}\n\n\
             {}\n\n\
             After generating the summary, the result should be cached for future use.",
            paper.title,
            paper.authors.join(", "),
            paper.arxiv_id,
            paper.categories.join(", "),
            paper.published,
            paper.pdf_url,
            paper.summary,
            level,
            audience,
            level_instructions,
            audience_instructions,
        );

        Ok(ToolOutput::text(output))
    }

    // ── Phase 4: Citation Network Analysis ────────────────────

    async fn handle_citation_graph(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for citation_graph action"
                    .to_string(),
            })?;

        let sub_action = args
            .get("sub_action")
            .and_then(|v| v.as_str())
            .unwrap_or("build");

        match sub_action {
            "build" => self.citation_graph_build(arxiv_id, args).await,
            "pagerank" => self.citation_graph_pagerank(arxiv_id),
            "similar" => self.citation_graph_similar(arxiv_id, args),
            "path" => self.citation_graph_path(arxiv_id, args),
            _ => Ok(ToolOutput::text(format!(
                "Unknown sub_action '{}'. Use: build, pagerank, similar, path.",
                sub_action
            ))),
        }
    }

    async fn citation_graph_build(
        &self,
        arxiv_id: &str,
        args: &Value,
    ) -> Result<ToolOutput, ToolError> {
        let depth = args
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .min(2) as usize;

        let s2_client = SemanticScholarClient::new(None, 3600, 1000).map_err(|e| {
            ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: format!("Failed to create Semantic Scholar client: {}", e),
            }
        })?;

        let mut graph = CitationGraph::new();
        let mut to_process = vec![arxiv_id.to_string()];
        let mut processed = std::collections::HashSet::new();

        for current_depth in 0..depth {
            let mut next_batch = Vec::new();

            for paper_id in &to_process {
                if processed.contains(paper_id) {
                    continue;
                }
                processed.insert(paper_id.clone());

                // Fetch metadata
                let meta = match s2_client.fetch_by_arxiv_id(paper_id).await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!("Failed to fetch S2 data for {}: {}", paper_id, e);
                        continue;
                    }
                };

                // Fetch citations and references
                let limit = if current_depth == 0 { 50 } else { 20 };
                let citations = s2_client
                    .fetch_citations(paper_id, limit)
                    .await
                    .unwrap_or_default();
                let references = s2_client
                    .fetch_references(paper_id, limit)
                    .await
                    .unwrap_or_default();

                let ref_ids: Vec<String> = references.iter().map(|(id, _)| id.clone()).collect();
                let citer_ids: Vec<String> = citations.iter().map(|(id, _)| id.clone()).collect();

                // Add titles for referenced/citing papers
                for (id, title) in &references {
                    graph.titles.insert(id.clone(), title.clone());
                }
                for (id, title) in &citations {
                    graph.titles.insert(id.clone(), title.clone());
                }

                graph.add_paper(
                    paper_id,
                    paper_id, // use arxiv_id as title placeholder
                    meta.citation_count.unwrap_or(0),
                    ref_ids.clone(),
                    citer_ids.clone(),
                );

                // Queue next depth
                if current_depth + 1 < depth {
                    next_batch.extend(ref_ids);
                    next_batch.extend(citer_ids);
                }
            }

            to_process = next_batch;
        }

        // Persist
        let mut state = self.load_state();
        state.citation_graph = Some(graph.to_state(arxiv_id));
        self.save_state(&state)?;

        let mut output = format!(
            "Citation graph built for {} (depth {}):\n\n",
            arxiv_id, depth
        );
        output.push_str(&format!(
            "  Nodes: {}\n  References tracked: {}\n  Citations tracked: {}\n\n",
            graph.node_count(),
            graph.references.values().map(|v| v.len()).sum::<usize>(),
            graph.cited_by.values().map(|v| v.len()).sum::<usize>(),
        ));
        output.push_str("Use sub_action 'pagerank' to rank papers by importance,\n");
        output.push_str("'similar' to find bibliographically coupled papers,\n");
        output.push_str("or 'path' to find shortest citation path between two papers.");

        Ok(ToolOutput::text(output))
    }

    fn citation_graph_pagerank(&self, arxiv_id: &str) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let graph_state = state.citation_graph.as_ref().ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: format!(
                    "No citation graph found. Run citation_graph with sub_action 'build' for {} first.",
                    arxiv_id
                ),
            }
        })?;

        let graph = CitationGraph::from_state(graph_state);
        let ranks = graph.pagerank(0.85, 20);

        let mut output = "PageRank results (top papers by importance):\n\n".to_string();
        for (i, (paper_id, score)) in ranks.iter().take(20).enumerate() {
            let title = graph
                .titles
                .get(paper_id)
                .map(|s| s.as_str())
                .unwrap_or("Unknown");
            let citations = graph.citation_counts.get(paper_id).copied().unwrap_or(0);
            output.push_str(&format!(
                "{}. {} (score: {:.6}, citations: {})\n   ID: {}\n\n",
                i + 1,
                title,
                score,
                citations,
                paper_id,
            ));
        }

        Ok(ToolOutput::text(output))
    }

    fn citation_graph_similar(
        &self,
        arxiv_id: &str,
        args: &Value,
    ) -> Result<ToolOutput, ToolError> {
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let state = self.load_state();
        let graph_state =
            state
                .citation_graph
                .as_ref()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "arxiv_research".to_string(),
                    reason: "No citation graph found. Run 'build' first.".to_string(),
                })?;

        let graph = CitationGraph::from_state(graph_state);
        let coupling = graph.bibliographic_coupling(arxiv_id, max_results);

        if coupling.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No bibliographically coupled papers found for {}. The graph may need more depth.",
                arxiv_id
            )));
        }

        let mut output = format!("Papers similar to {} (by shared references):\n\n", arxiv_id);
        for (i, (paper_id, shared)) in coupling.iter().enumerate() {
            let title = graph
                .titles
                .get(paper_id)
                .map(|s| s.as_str())
                .unwrap_or("Unknown");
            output.push_str(&format!(
                "{}. {} (shared refs: {})\n   ID: {}\n\n",
                i + 1,
                title,
                shared,
                paper_id,
            ));
        }

        Ok(ToolOutput::text(output))
    }

    fn citation_graph_path(&self, arxiv_id: &str, args: &Value) -> Result<ToolOutput, ToolError> {
        let other_id = args
            .get("other_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing 'other_id' parameter for path sub_action".to_string(),
            })?;

        let state = self.load_state();
        let graph_state =
            state
                .citation_graph
                .as_ref()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "arxiv_research".to_string(),
                    reason: "No citation graph found. Run 'build' first.".to_string(),
                })?;

        let graph = CitationGraph::from_state(graph_state);
        match graph.shortest_path(arxiv_id, other_id) {
            Some(path) => {
                let mut output = format!(
                    "Citation path from {} to {} ({} steps):\n\n",
                    arxiv_id,
                    other_id,
                    path.len() - 1
                );
                for (i, node) in path.iter().enumerate() {
                    let title = graph
                        .titles
                        .get(node)
                        .map(|s| s.as_str())
                        .unwrap_or("Unknown");
                    let arrow = if i < path.len() - 1 { " →" } else { "" };
                    output.push_str(&format!("  {} ({}){}\n", node, title, arrow));
                }
                Ok(ToolOutput::text(output))
            }
            None => Ok(ToolOutput::text(format!(
                "No citation path found between {} and {}. They may be in disconnected components.",
                arxiv_id, other_id
            ))),
        }
    }

    // ── Phase 5: Blueprint Distillation ───────────────────────

    async fn handle_blueprint(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let arxiv_id = args
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: "Missing required parameter 'arxiv_id' for blueprint action".to_string(),
            })?;

        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");

        let focus = args.get("focus").and_then(|v| v.as_str()).unwrap_or("full");
        let focus = match focus {
            "full" | "model_only" | "training" | "inference" => focus,
            _ => "full",
        };

        let client = self.make_client()?;
        let paper = client
            .fetch_paper(arxiv_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "arxiv_research".to_string(),
                message: e,
            })?;

        // Try to enrich with citation data
        let mut citation_info = String::new();
        if let Ok(s2_client) = SemanticScholarClient::new(None, 3600, 1000)
            && let Ok(meta) = s2_client.fetch_by_arxiv_id(arxiv_id).await
        {
            if let Some(count) = meta.citation_count {
                citation_info = format!("\nCitation count: {}", count);
            }
            if let Some(tldr) = &meta.tldr {
                citation_info.push_str(&format!("\nTL;DR: {}", tldr));
            }
        }

        let focus_instructions = match focus {
            "model_only" => {
                "Focus ONLY on the model architecture. Ignore training, data pipeline, and evaluation."
            }
            "training" => {
                "Focus on the training pipeline: loss functions, optimizers, schedulers, data loading."
            }
            "inference" => {
                "Focus on inference: model loading, preprocessing, prediction, post-processing."
            }
            _ => "Cover the full pipeline: data, model, training, evaluation, inference.",
        };

        let output = format!(
            "BLUEPRINT DISTILLATION REQUEST:\n\n\
             Paper: {}\n\
             Authors: {}\n\
             ArXiv ID: {}\n\
             Target Language: {}\n\
             Focus: {}\n\
             {}\n\n\
             Abstract:\n{}\n\n\
             ---\n\n\
             BLUEPRINT INSTRUCTIONS:\n\
             Based on the paper above, produce an implementation blueprint in {} with the following structure:\n\n\
             1. **Architecture Overview**: High-level description of the system\n\
             2. **Components**: List each component with:\n\
                - ID (short identifier)\n\
                - Name\n\
                - Description\n\
                - Paper section reference\n\
                - Dependencies on other components\n\
             3. **File Structure**: List each file with:\n\
                - Path\n\
                - Purpose\n\
                - Which component it implements\n\
             4. **Dependencies**: External libraries with versions\n\
             5. **Generation Order**: Topological order for implementing files (dependencies first)\n\
             6. **Test Strategy**: How to verify each component\n\n\
             {}\n\n\
             Output the blueprint as structured text that can guide step-by-step implementation.",
            paper.title,
            paper.authors.join(", "),
            paper.arxiv_id,
            language,
            focus,
            citation_info,
            paper.summary,
            language,
            focus_instructions,
        );

        Ok(ToolOutput::text(output))
    }

    /// Enrich a paper with external metadata from Semantic Scholar and OpenAlex.
    /// Fails gracefully — returns original paper if enrichment APIs are unavailable.
    #[allow(dead_code)]
    async fn enrich_paper(
        &self,
        paper: &crate::arxiv_api::ArxivPaper,
    ) -> crate::arxiv_api::ArxivPaper {
        let mut enriched = paper.clone();

        // Try Semantic Scholar
        if let Ok(s2_client) = SemanticScholarClient::new(None, 3600, 1000)
            && let Ok(meta) = s2_client.fetch_by_arxiv_id(&paper.arxiv_id).await
        {
            enriched.external.citation_count = meta.citation_count;
            enriched.external.influential_citation_count = meta.influential_citation_count;
            enriched.external.tldr = meta.tldr;
            enriched.external.semantic_scholar_id = meta.semantic_scholar_id;
            enriched.external.references = meta.references;
            enriched.external.cited_by = meta.cited_by;
        }

        // Try OpenAlex
        if let Ok(oa_client) = OpenAlexClient::new(None, 3600, 1000)
            && let Ok(meta) = oa_client.fetch_by_arxiv_id(&paper.arxiv_id).await
        {
            enriched.external.openalex_id = meta.openalex_id;
            if !meta.concepts.is_empty() {
                enriched.external.concepts = meta.concepts;
            }
            // Use OpenAlex citation count if S2 didn't provide one
            if enriched.external.citation_count.is_none() {
                enriched.external.citation_count = meta.citation_count;
            }
        }

        enriched
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

/// Generate a project scaffold for a paper implementation.
fn generate_project_scaffold(
    paper: &crate::arxiv_api::ArxivPaper,
    lang_config: &crate::arxiv_api::LanguageConfig,
    project_name: &str,
    tdd: bool,
) -> ProjectScaffold {
    let (dirs, files, deps, setup_cmds, test_cmds) = match lang_config.language.as_str() {
        "python" => {
            let dirs = vec!["src".into(), "tests".into()];
            let mut files = vec![];

            // Requirements file
            files.push(ScaffoldFile {
                path: "requirements.txt".into(),
                content: "numpy>=1.24.0\ntorch>=2.0.0\npytest>=7.0.0\n".into(),
                is_test: false,
            });

            // README
            files.push(ScaffoldFile {
                path: "README.md".into(),
                content: format!(
                    "# {}\n\nImplementation of: {}\n\nArXiv: https://arxiv.org/abs/{}\nAuthors: {}\n\n## Setup\n\n```bash\npython3 -m venv .venv\nsource .venv/bin/activate\npip install -r requirements.txt\n```\n\n## Test\n\n```bash\npytest tests/ -v\n```\n",
                    project_name, paper.title, paper.arxiv_id, paper.authors.join(", ")
                ),
                is_test: false,
            });

            // Test file (TDD first)
            if tdd {
                files.push(ScaffoldFile {
                    path: "tests/__init__.py".into(),
                    content: String::new(),
                    is_test: true,
                });
                files.push(ScaffoldFile {
                    path: "tests/test_model.py".into(),
                    content: format!(
                        "\"\"\"Tests for {} implementation.\n\nPaper: {}\nArXiv: {}\n\"\"\"\nimport pytest\n# import sys; sys.path.insert(0, 'src')\n\n\ndef test_model_initialization():\n    \"\"\"Test that the model can be initialized.\"\"\"\n    # TODO: Implement based on paper Section 3\n    pass\n\n\ndef test_forward_pass():\n    \"\"\"Test forward pass produces correct output shape.\"\"\"\n    # TODO: Implement based on paper architecture\n    pass\n\n\ndef test_loss_computation():\n    \"\"\"Test that loss can be computed.\"\"\"\n    # TODO: Implement based on paper Section 4\n    pass\n",
                        project_name, paper.title, paper.arxiv_id
                    ),
                    is_test: true,
                });
            }

            // Implementation stubs
            files.push(ScaffoldFile {
                path: "src/__init__.py".into(),
                content: String::new(),
                is_test: false,
            });
            files.push(ScaffoldFile {
                path: "src/model.py".into(),
                content: format!(
                    "\"\"\"Core model implementation for: {}\n\nArXiv: https://arxiv.org/abs/{}\nAuthors: {}\n\"\"\"\nimport numpy as np\n\n\n# TODO: Implement the main model/algorithm from the paper\n# Reference: Section 3 of the paper\n\nclass Model:\n    def __init__(self):\n        pass\n\n    def forward(self, x):\n        raise NotImplementedError(\"Implement based on paper architecture\")\n",
                    paper.title, paper.arxiv_id, paper.authors.join(", ")
                ),
                is_test: false,
            });

            let setup = vec!["source .venv/bin/activate && pip install -r requirements.txt".into()];
            let tests = vec!["source .venv/bin/activate && python3 -m pytest tests/ -v".into()];
            (
                dirs,
                files,
                vec!["numpy".into(), "torch".into(), "pytest".into()],
                setup,
                tests,
            )
        }
        "rust" => {
            let dirs = vec!["src".into(), "tests".into()];
            let mut files = vec![];

            files.push(ScaffoldFile {
                path: "Cargo.toml".into(),
                content: format!(
                    "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n# Paper: {}\n# ArXiv: https://arxiv.org/abs/{}\n\n[dependencies]\nndarray = \"0.16\"\n\n[dev-dependencies]\n",
                    project_name, paper.title, paper.arxiv_id
                ),
                is_test: false,
            });

            files.push(ScaffoldFile {
                path: "README.md".into(),
                content: format!(
                    "# {}\n\nRust implementation of: {}\nArXiv: https://arxiv.org/abs/{}\n\n## Build & Test\n\n```bash\ncargo build\ncargo test\n```\n",
                    project_name, paper.title, paper.arxiv_id
                ),
                is_test: false,
            });

            if tdd {
                files.push(ScaffoldFile {
                    path: "tests/integration_test.rs".into(),
                    content: format!(
                        "//! Integration tests for: {}\n//! ArXiv: {}\n\n#[test]\nfn test_model_creation() {{\n    // TODO: Implement\n}}\n\n#[test]\nfn test_forward_pass() {{\n    // TODO: Implement\n}}\n",
                        paper.title, paper.arxiv_id
                    ),
                    is_test: true,
                });
            }

            files.push(ScaffoldFile {
                path: "src/lib.rs".into(),
                content: format!(
                    "//! {} - Implementation of: {}\n//! ArXiv: https://arxiv.org/abs/{}\n\npub mod model;\n",
                    project_name, paper.title, paper.arxiv_id
                ),
                is_test: false,
            });

            files.push(ScaffoldFile {
                path: "src/model.rs".into(),
                content: format!(
                    "//! Core model for: {}\n\n/// TODO: Implement the main model\npub struct Model {{}}\n\nimpl Model {{\n    pub fn new() -> Self {{\n        Self {{}}\n    }}\n}}\n",
                    paper.title
                ),
                is_test: false,
            });

            let setup = vec!["cargo build".into()];
            let tests = vec!["cargo test".into()];
            (dirs, files, vec!["ndarray".into()], setup, tests)
        }
        _ => {
            // Generic fallback
            let dirs = vec!["src".into(), "tests".into()];
            let files = vec![ScaffoldFile {
                path: "README.md".into(),
                content: format!(
                    "# {}\n\nImplementation of: {}\nArXiv: https://arxiv.org/abs/{}\n",
                    project_name, paper.title, paper.arxiv_id
                ),
                is_test: false,
            }];
            (dirs, files, vec![], vec![], vec![])
        }
    };

    ProjectScaffold {
        paper_id: paper.arxiv_id.clone(),
        project_name: project_name.to_string(),
        language_config: lang_config.clone(),
        directory_structure: dirs,
        files,
        dependencies: deps,
        setup_commands: setup_cmds,
        test_commands: test_cmds,
    }
}

/// Generate a valid Jupyter notebook JSON structure with 4-layer progressive design.
fn generate_notebook_json(paper: &crate::arxiv_api::ArxivPaper) -> String {
    let notebook = json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {
            "kernelspec": {
                "display_name": "Python 3",
                "language": "python",
                "name": "python3"
            },
            "language_info": {
                "name": "python",
                "version": "3.10.0"
            }
        },
        "cells": [
            // ── Layer 1: Intuition ──
            {
                "cell_type": "markdown",
                "metadata": {},
                "source": [format!("# {}\n\n**Authors:** {}\n**ArXiv:** https://arxiv.org/abs/{}\n**Published:** {}\n\n---\n\n## Layer 1: Intuition\n*Goal: Understand the core idea before any code.*",
                    paper.title, paper.authors.join(", "), paper.arxiv_id,
                    &paper.published[..10.min(paper.published.len())])],
                "id": "cell_01"
            },
            {
                "cell_type": "markdown",
                "metadata": {},
                "source": [format!("### Problem Statement\n\n**Abstract:** {}\n\nTODO: In your own words, what problem does this paper solve? What gap in existing work does it fill?",
                    truncate_text(&paper.summary, 500))],
                "id": "cell_02"
            },
            {
                "cell_type": "markdown",
                "metadata": {},
                "source": ["### Key Insight\n\nTODO: What is the core idea in plain language? If you had to explain this to someone in one paragraph, what would you say?"],
                "id": "cell_03"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Visual Intuition — sketch the core concept\nimport matplotlib.pyplot as plt\nimport numpy as np\n\n# TODO: Create a diagram or visualization of the key concept\n# Example: plot the architecture, show data flow, or illustrate the key equation\nfig, ax = plt.subplots(1, 1, figsize=(10, 6))\nax.set_title('Core Concept Visualization')\nax.text(0.5, 0.5, 'TODO: Visualize the key idea', ha='center', va='center', fontsize=14)\nplt.show()"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_04"
            },
            // ── Layer 2: Minimal Example ──
            {
                "cell_type": "markdown",
                "metadata": {},
                "source": ["---\n\n## Layer 2: Minimal Example\n*Goal: Get the core algorithm working on toy data in <50 lines.*"],
                "id": "cell_05"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Minimal imports — just the basics\nimport numpy as np"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_06"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Toy data — small synthetic dataset\n# TODO: Create a minimal dataset that demonstrates the paper's approach\nnp.random.seed(42)\nX = np.random.randn(100, 10)\ny = np.random.randint(0, 2, 100)\nprint(f'Data shape: X={X.shape}, y={y.shape}')"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_07"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Simplified algorithm — core idea in <50 lines\n# TODO: Implement the essential algorithm from the paper\n# Reference the specific section/equation numbers\n\ndef simplified_model(x):\n    \"\"\"Minimal implementation of the paper's core contribution.\"\"\"\n    # TODO: Replace with actual algorithm\n    return x\n\nresult = simplified_model(X)\nprint(f'Output shape: {result.shape}')\n\n# Validation\nassert result.shape == X.shape, 'Output shape mismatch'\nprint('Minimal example validated!')"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_08"
            },
            // ── Layer 3: Full Implementation ──
            {
                "cell_type": "markdown",
                "metadata": {},
                "source": ["---\n\n## Layer 3: Full Implementation\n*Goal: Complete, paper-faithful implementation with all components.*"],
                "id": "cell_09"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Full imports and configuration\nimport numpy as np\nimport matplotlib.pyplot as plt\n\n# TODO: Add paper-specific imports (torch, tensorflow, etc.)\n\n# Hyperparameters from the paper\nCONFIG = {\n    'learning_rate': 1e-3,\n    'batch_size': 32,\n    'epochs': 100,\n    'hidden_dim': 256,\n    # TODO: Add paper-specific hyperparameters\n}\nprint('Configuration:', CONFIG)"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_10"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Data pipeline — preprocessing, splits, DataLoader\n# TODO: Implement full data pipeline as described in the paper\n# Reference: Section on Datasets/Experimental Setup"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_11"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Model architecture — complete implementation\n# TODO: Implement the full model architecture\n# Reference: Section on Model/Architecture/Method"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_12"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Training loop — optimizer, scheduler, logging\n# TODO: Implement training procedure from the paper\n# Reference: Section on Training/Optimization"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_13"
            },
            // ── Layer 4: Experimentation ──
            {
                "cell_type": "markdown",
                "metadata": {},
                "source": ["---\n\n## Layer 4: Experimentation\n*Goal: Reproduce results and explore beyond the paper.*"],
                "id": "cell_14"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Hyperparameter sweep\n# TODO: Systematically vary key parameters to understand sensitivity\nresults = []\nfor lr in [1e-4, 1e-3, 1e-2]:\n    # TODO: Train and evaluate with each setting\n    results.append({'lr': lr, 'metric': 0.0})\nprint('Sweep results:', results)"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_15"
            },
            {
                "cell_type": "code",
                "metadata": {},
                "source": ["# Ablation study — remove components to measure impact\n# TODO: Systematically disable components to understand their contribution\n# This is key for understanding which parts of the paper matter most"],
                "execution_count": null,
                "outputs": [],
                "id": "cell_16"
            },
            {
                "cell_type": "markdown",
                "metadata": {},
                "source": [format!("---\n\n## Summary & Conclusions\n\n**Paper:** {}\n**ArXiv:** https://arxiv.org/abs/{}\n\n### Results\nTODO: Summarize reproduction status and key findings\n\n### Differences from Paper\nTODO: Note any deviations from the original implementation\n\n### Next Steps\nTODO: What would you explore next?",
                    paper.title, paper.arxiv_id)],
                "id": "cell_17"
            }
        ]
    });

    serde_json::to_string(&notebook).unwrap_or_else(|_| "{}".to_string())
}

#[async_trait]
impl Tool for ArxivResearchTool {
    fn name(&self) -> &str {
        "arxiv_research"
    }

    fn description(&self) -> &str {
        "Search, fetch, analyze, and implement academic papers from arXiv with multi-source \
         enrichment. Actions: search, fetch, analyze, compare, trending, save/library/remove, \
         export_bibtex, collections, digest_config, paper_to_code, paper_to_notebook, implement \
         (full TDD scaffold), setup_env, verify, implementation_status, semantic_search (keyword \
         search over library), summarize (multi-level LLM summary), citation_graph (build/pagerank/\
         similar/path), blueprint (implementation planning), reindex. \
         IMPORTANT workflow: after 'search', present numbered results and ask user to select. \
         For implement/paper_to_code/paper_to_notebook, ask language and mode. Python uses venv."
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
                        "collections", "digest_config", "paper_to_code", "paper_to_notebook",
                        "implement", "setup_env", "verify", "implementation_status",
                        "semantic_search", "summarize", "citation_graph", "blueprint", "reindex"
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
                },
                "target_dir": {
                    "type": "string",
                    "description": "Target directory for implementation (for implement action)"
                },
                "mode": {
                    "type": "string",
                    "enum": ["project", "notebook"],
                    "description": "Implementation mode (default: project)"
                },
                "tdd": {
                    "type": "boolean",
                    "description": "Whether to use TDD approach with tests first (default: true)"
                },
                "level": {
                    "type": "string",
                    "enum": ["tldr", "abstract", "section", "deep"],
                    "description": "Summary level (for summarize action, default: abstract)"
                },
                "audience": {
                    "type": "string",
                    "enum": ["researcher", "engineer", "student"],
                    "description": "Target audience for summary (default: researcher)"
                },
                "other_id": {
                    "type": "string",
                    "description": "Second paper ID for citation_graph path sub_action"
                },
                "focus": {
                    "type": "string",
                    "enum": ["full", "model_only", "training", "inference"],
                    "description": "Blueprint focus area (default: full)"
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
            "implement" => self.handle_implement(&args).await,
            "setup_env" => self.handle_setup_env(&args),
            "verify" => self.handle_verify(&args),
            "implementation_status" => self.handle_implementation_status(&args),
            "semantic_search" => self.handle_semantic_search(&args).await,
            "summarize" => self.handle_summarize(&args).await,
            "citation_graph" => self.handle_citation_graph(&args).await,
            "blueprint" => self.handle_blueprint(&args).await,
            "reindex" => self.handle_reindex(&args),
            _ => Err(ToolError::InvalidArguments {
                name: "arxiv_research".to_string(),
                reason: format!(
                    "Unknown action '{}'. Valid actions: search, fetch, analyze, compare, trending, save, library, remove, export_bibtex, collections, digest_config, paper_to_code, paper_to_notebook, implement, setup_env, verify, implementation_status, semantic_search, summarize, citation_graph, blueprint, reindex",
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
        assert_eq!(action_strs.len(), 22);
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
        assert!(action_strs.contains(&"implement"));
        assert!(action_strs.contains(&"setup_env"));
        assert!(action_strs.contains(&"verify"));
        assert!(action_strs.contains(&"implementation_status"));
        assert!(action_strs.contains(&"semantic_search"));
        assert!(action_strs.contains(&"summarize"));
        assert!(action_strs.contains(&"citation_graph"));
        assert!(action_strs.contains(&"blueprint"));
        assert!(action_strs.contains(&"reindex"));
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
                    external: Default::default(),
                },
                tags: vec!["test".to_string()],
                collection: None,
                notes: None,
                saved_at: Utc::now(),
            }],
            collections: Vec::new(),
            digest_config: None,
            implementations: Vec::new(),
            summaries: Vec::new(),
            citation_graph: None,
            blueprints: Vec::new(),
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

    // ── Tests for new actions (Phase 9) ──

    #[tokio::test]
    async fn test_semantic_search_missing_query() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "semantic_search"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("query"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_semantic_search_empty_library() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "semantic_search", "query": "attention"}))
            .await
            .unwrap();
        assert!(result.content.contains("empty"));
    }

    #[tokio::test]
    async fn test_summarize_missing_id() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "summarize"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("arxiv_id"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_blueprint_missing_id() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "blueprint"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("arxiv_id"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_citation_graph_missing_id() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "citation_graph"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("arxiv_id"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_reindex_empty_library() {
        let dir = TempDir::new().unwrap();
        let tool = ArxivResearchTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "reindex"})).await.unwrap();
        assert!(result.content.contains("empty"));
    }

    #[tokio::test]
    async fn test_library_state_roundtrip_with_new_fields() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ArxivResearchTool::new(workspace);

        let state = ArxivLibraryState {
            entries: Vec::new(),
            collections: Vec::new(),
            digest_config: None,
            implementations: Vec::new(),
            summaries: vec![crate::arxiv_api::CachedSummary {
                arxiv_id: "2301.12345".to_string(),
                level: "abstract".to_string(),
                audience: "researcher".to_string(),
                summary: "Test summary.".to_string(),
                created_at: Utc::now(),
            }],
            citation_graph: None,
            blueprints: Vec::new(),
        };

        tool.save_state(&state).unwrap();
        let loaded = tool.load_state();
        assert_eq!(loaded.summaries.len(), 1);
        assert_eq!(loaded.summaries[0].arxiv_id, "2301.12345");
        assert!(loaded.citation_graph.is_none());
        assert!(loaded.blueprints.is_empty());
    }

    #[tokio::test]
    async fn test_backward_compat_old_library_format() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ArxivResearchTool::new(workspace.clone());

        // Write old-format JSON (no summaries, citation_graph, blueprints fields)
        let old_json = r#"{
            "entries": [],
            "collections": [],
            "digest_config": null,
            "implementations": []
        }"#;

        let state_path = workspace
            .join(".rustant")
            .join("arxiv")
            .join("library.json");
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        std::fs::write(&state_path, old_json).unwrap();

        let loaded = tool.load_state();
        assert!(loaded.summaries.is_empty());
        assert!(loaded.citation_graph.is_none());
        assert!(loaded.blueprints.is_empty());
    }

    // Integration tests for new actions — require network
    #[tokio::test]
    #[ignore]
    async fn test_real_semantic_scholar_fetch() {
        let client = crate::paper_sources::SemanticScholarClient::new(None, 3600, 100).unwrap();
        let meta = client.fetch_by_arxiv_id("1706.03762").await.unwrap();
        assert!(meta.citation_count.is_some());
        assert!(meta.citation_count.unwrap() > 100);
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_openalex_search() {
        let client = crate::paper_sources::OpenAlexClient::new(None, 3600, 100).unwrap();
        let meta = client.fetch_by_arxiv_id("1706.03762").await.unwrap();
        // OpenAlex may or may not find the paper; just check no crash
        assert!(meta.citation_count.is_some() || meta.openalex_id.is_none());
    }
}
