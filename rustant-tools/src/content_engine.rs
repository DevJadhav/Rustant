//! Content engine tool — multi-platform content pipeline with lifecycle tracking.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ContentPlatform {
    Blog,
    Twitter,
    LinkedIn,
    GitHub,
    Medium,
    Newsletter,
}

impl ContentPlatform {
    fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "blog" => Some(Self::Blog),
            "twitter" => Some(Self::Twitter),
            "linkedin" => Some(Self::LinkedIn),
            "github" => Some(Self::GitHub),
            "medium" => Some(Self::Medium),
            "newsletter" => Some(Self::Newsletter),
            _ => None,
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Blog => "Blog",
            Self::Twitter => "Twitter",
            Self::LinkedIn => "LinkedIn",
            Self::GitHub => "GitHub",
            Self::Medium => "Medium",
            Self::Newsletter => "Newsletter",
        }
    }
}

impl std::fmt::Display for ContentPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ContentStatus {
    Idea,
    Draft,
    Review,
    Scheduled,
    Published,
    Archived,
}

impl ContentStatus {
    fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "idea" => Some(Self::Idea),
            "draft" => Some(Self::Draft),
            "review" => Some(Self::Review),
            "scheduled" => Some(Self::Scheduled),
            "published" => Some(Self::Published),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Idea => "Idea",
            Self::Draft => "Draft",
            Self::Review => "Review",
            Self::Scheduled => "Scheduled",
            Self::Published => "Published",
            Self::Archived => "Archived",
        }
    }
}

impl std::fmt::Display for ContentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContentPiece {
    id: usize,
    title: String,
    body: String,
    platform: ContentPlatform,
    status: ContentStatus,
    audience: String,
    tone: String,
    tags: Vec<String>,
    word_count: usize,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scheduled_for: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalendarEntry {
    date: String, // YYYY-MM-DD format
    platform: ContentPlatform,
    topic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_id: Option<usize>,
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ContentState {
    pieces: Vec<ContentPiece>,
    calendar: Vec<CalendarEntry>,
    next_id: usize,
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

pub struct ContentEngineTool {
    workspace: PathBuf,
}

impl ContentEngineTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("content")
            .join("library.json")
    }

    fn load_state(&self) -> ContentState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            ContentState {
                pieces: Vec::new(),
                calendar: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn save_state(&self, state: &ContentState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "content_engine".to_string(),
                message: format!("Failed to create state dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "content_engine".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "content_engine".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "content_engine".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }

    fn find_piece(pieces: &[ContentPiece], id: usize) -> Option<usize> {
        pieces.iter().position(|p| p.id == id)
    }

    fn format_piece_summary(piece: &ContentPiece) -> String {
        let scheduled = piece
            .scheduled_for
            .map(|d| format!(" | Scheduled: {}", d.format("%Y-%m-%d %H:%M")))
            .unwrap_or_default();
        let published = piece
            .published_at
            .map(|d| format!(" | Published: {}", d.format("%Y-%m-%d %H:%M")))
            .unwrap_or_default();
        let tags = if piece.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", piece.tags.join(", "))
        };
        format!(
            "  #{} — {} ({}, {}) {} words{}{}{}",
            piece.id,
            piece.title,
            piece.platform,
            piece.status,
            piece.word_count,
            tags,
            scheduled,
            published,
        )
    }

    fn format_piece_detail(piece: &ContentPiece) -> String {
        let mut out = String::new();
        out.push_str(&format!("Content #{}\n", piece.id));
        out.push_str(&format!("  Title:    {}\n", piece.title));
        out.push_str(&format!("  Platform: {}\n", piece.platform));
        out.push_str(&format!("  Status:   {}\n", piece.status));
        out.push_str(&format!("  Audience: {}\n", piece.audience));
        out.push_str(&format!("  Tone:     {}\n", piece.tone));
        out.push_str(&format!("  Words:    {}\n", piece.word_count));
        if !piece.tags.is_empty() {
            out.push_str(&format!("  Tags:     {}\n", piece.tags.join(", ")));
        }
        out.push_str(&format!(
            "  Created:  {}\n",
            piece.created_at.format("%Y-%m-%d %H:%M")
        ));
        out.push_str(&format!(
            "  Updated:  {}\n",
            piece.updated_at.format("%Y-%m-%d %H:%M")
        ));
        if let Some(s) = piece.scheduled_for {
            out.push_str(&format!("  Scheduled: {}\n", s.format("%Y-%m-%d %H:%M")));
        }
        if let Some(p) = piece.published_at {
            out.push_str(&format!("  Published: {}\n", p.format("%Y-%m-%d %H:%M")));
        }
        if !piece.body.is_empty() {
            out.push_str(&format!("\n--- Body ---\n{}\n", piece.body));
        }
        out
    }

    fn platform_constraints(platform: &ContentPlatform) -> &'static str {
        match platform {
            ContentPlatform::Twitter => {
                "Twitter: Max 280 characters. Use concise, punchy language. Include relevant hashtags. Encourage engagement (questions, polls)."
            }
            ContentPlatform::LinkedIn => {
                "LinkedIn: Professional tone. Use clear structure with line breaks. Open with a hook. End with a call-to-action. Keep under 3000 characters for best engagement."
            }
            ContentPlatform::Blog => {
                "Blog: Long-form with headers, subheaders, and paragraphs. Include an introduction and conclusion. SEO-friendly with keywords. Target 800-2000 words."
            }
            ContentPlatform::GitHub => {
                "GitHub: Technical and precise. Use Markdown formatting. Include code examples where relevant. Be concise and actionable."
            }
            ContentPlatform::Medium => {
                "Medium: Storytelling format. Use a compelling title and subtitle. Break into sections with subheadings. Include images/quotes. Target 5-7 minute read (1000-1500 words)."
            }
            ContentPlatform::Newsletter => {
                "Newsletter: Engaging and personable. Use a strong subject line hook. Keep sections scannable. Include clear CTAs. Balance value with brevity."
            }
        }
    }
}

#[async_trait]
impl Tool for ContentEngineTool {
    fn name(&self) -> &str {
        "content_engine"
    }

    fn description(&self) -> &str {
        "Multi-platform content pipeline with lifecycle tracking. Actions: create, update, set_status, get, list, search, delete, schedule, calendar_add, calendar_list, calendar_remove, stats, adapt, export_markdown."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "create", "update", "set_status", "get", "list", "search",
                        "delete", "schedule", "calendar_add", "calendar_list",
                        "calendar_remove", "stats", "adapt", "export_markdown"
                    ],
                    "description": "Action to perform"
                },
                "id": { "type": "integer", "description": "Content piece ID" },
                "title": { "type": "string", "description": "Content title" },
                "body": { "type": "string", "description": "Content body text" },
                "platform": { "type": "string", "description": "Platform: blog, twitter, linkedin, github, medium, newsletter" },
                "status": { "type": "string", "description": "Status: idea, draft, review, scheduled, published, archived" },
                "audience": { "type": "string", "description": "Target audience" },
                "tone": { "type": "string", "description": "Writing tone (e.g., casual, formal, technical)" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Content tags" },
                "tag": { "type": "string", "description": "Single tag filter for list" },
                "query": { "type": "string", "description": "Search query" },
                "date": { "type": "string", "description": "Date in YYYY-MM-DD format" },
                "time": { "type": "string", "description": "Time in HH:MM format (for schedule)" },
                "month": { "type": "string", "description": "Month filter in YYYY-MM format" },
                "topic": { "type": "string", "description": "Calendar entry topic" },
                "content_id": { "type": "integer", "description": "Linked content piece ID for calendar" },
                "notes": { "type": "string", "description": "Calendar entry notes" },
                "target_platform": { "type": "string", "description": "Target platform for adapt action" },
                "target_tone": { "type": "string", "description": "Target tone for adapt action" }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(30)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let mut state = self.load_state();

        match action {
            "create" => {
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if title.is_empty() {
                    return Ok(ToolOutput::text("Please provide a title for the content."));
                }
                let platform_str = args
                    .get("platform")
                    .and_then(|v| v.as_str())
                    .unwrap_or("blog");
                let platform =
                    ContentPlatform::from_str_loose(platform_str).unwrap_or(ContentPlatform::Blog);
                let body = args
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let audience = args
                    .get("audience")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tone = args
                    .get("tone")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tags: Vec<String> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let word_count = count_words(&body);
                let status = if body.is_empty() {
                    ContentStatus::Idea
                } else {
                    ContentStatus::Draft
                };

                let id = state.next_id;
                state.next_id += 1;
                let now = Utc::now();
                state.pieces.push(ContentPiece {
                    id,
                    title: title.to_string(),
                    body,
                    platform: platform.clone(),
                    status: status.clone(),
                    audience,
                    tone,
                    tags,
                    word_count,
                    created_at: now,
                    updated_at: now,
                    scheduled_for: None,
                    published_at: None,
                });
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Created content #{} '{}' ({}, {}).",
                    id, title, platform, status
                )))
            }

            "update" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let idx = match Self::find_piece(&state.pieces, id) {
                    Some(i) => i,
                    None => return Ok(ToolOutput::text(format!("Content #{} not found.", id))),
                };

                if let Some(title) = args.get("title").and_then(|v| v.as_str()) {
                    state.pieces[idx].title = title.to_string();
                }
                if let Some(body) = args.get("body").and_then(|v| v.as_str()) {
                    state.pieces[idx].body = body.to_string();
                    state.pieces[idx].word_count = count_words(body);
                }
                if let Some(platform_str) = args.get("platform").and_then(|v| v.as_str())
                    && let Some(p) = ContentPlatform::from_str_loose(platform_str)
                {
                    state.pieces[idx].platform = p;
                }
                if let Some(audience) = args.get("audience").and_then(|v| v.as_str()) {
                    state.pieces[idx].audience = audience.to_string();
                }
                if let Some(tone) = args.get("tone").and_then(|v| v.as_str()) {
                    state.pieces[idx].tone = tone.to_string();
                }
                state.pieces[idx].updated_at = Utc::now();
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Updated content #{} '{}'.",
                    id, state.pieces[idx].title
                )))
            }

            "set_status" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let status_str = args.get("status").and_then(|v| v.as_str()).unwrap_or("");
                let new_status = match ContentStatus::from_str_loose(status_str) {
                    Some(s) => s,
                    None => {
                        return Ok(ToolOutput::text(format!(
                            "Unknown status '{}'. Use: idea, draft, review, scheduled, published, archived.",
                            status_str
                        )));
                    }
                };
                let idx = match Self::find_piece(&state.pieces, id) {
                    Some(i) => i,
                    None => return Ok(ToolOutput::text(format!("Content #{} not found.", id))),
                };

                state.pieces[idx].status = new_status.clone();
                state.pieces[idx].updated_at = Utc::now();
                if new_status == ContentStatus::Published {
                    state.pieces[idx].published_at = Some(Utc::now());
                }
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Content #{} status set to {}.",
                    id, new_status
                )))
            }

            "get" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                match Self::find_piece(&state.pieces, id) {
                    Some(idx) => Ok(ToolOutput::text(Self::format_piece_detail(
                        &state.pieces[idx],
                    ))),
                    None => Ok(ToolOutput::text(format!("Content #{} not found.", id))),
                }
            }

            "list" => {
                let platform_filter = args
                    .get("platform")
                    .and_then(|v| v.as_str())
                    .and_then(ContentPlatform::from_str_loose);
                let status_filter = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .and_then(ContentStatus::from_str_loose);
                let tag_filter = args.get("tag").and_then(|v| v.as_str());

                let filtered: Vec<&ContentPiece> = state
                    .pieces
                    .iter()
                    .filter(|p| {
                        platform_filter
                            .as_ref()
                            .map(|pf| p.platform == *pf)
                            .unwrap_or(true)
                    })
                    .filter(|p| {
                        status_filter
                            .as_ref()
                            .map(|sf| p.status == *sf)
                            .unwrap_or(true)
                    })
                    .filter(|p| {
                        tag_filter
                            .map(|t| p.tags.iter().any(|tag| tag.eq_ignore_ascii_case(t)))
                            .unwrap_or(true)
                    })
                    .collect();

                if filtered.is_empty() {
                    return Ok(ToolOutput::text("No content pieces found."));
                }

                let lines: Vec<String> = filtered
                    .into_iter()
                    .map(Self::format_piece_summary)
                    .collect();
                Ok(ToolOutput::text(format!(
                    "Content ({} pieces):\n{}",
                    lines.len(),
                    lines.join("\n")
                )))
            }

            "search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                if query.is_empty() {
                    return Ok(ToolOutput::text("Please provide a search query."));
                }

                let matches: Vec<String> = state
                    .pieces
                    .iter()
                    .filter(|p| {
                        p.title.to_lowercase().contains(&query)
                            || p.body.to_lowercase().contains(&query)
                            || p.tags.iter().any(|t| t.to_lowercase().contains(&query))
                    })
                    .map(Self::format_piece_summary)
                    .collect();

                if matches.is_empty() {
                    Ok(ToolOutput::text(format!(
                        "No content matching '{}'.",
                        query
                    )))
                } else {
                    Ok(ToolOutput::text(format!(
                        "Found {} pieces:\n{}",
                        matches.len(),
                        matches.join("\n")
                    )))
                }
            }

            "delete" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let idx = match Self::find_piece(&state.pieces, id) {
                    Some(i) => i,
                    None => return Ok(ToolOutput::text(format!("Content #{} not found.", id))),
                };
                let title = state.pieces[idx].title.clone();
                state.pieces.remove(idx);
                // Remove linked calendar entries
                state.calendar.retain(|c| c.content_id != Some(id));
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Deleted content #{} '{}' and linked calendar entries.",
                    id, title
                )))
            }

            "schedule" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let date_str = args.get("date").and_then(|v| v.as_str()).unwrap_or("");
                if date_str.is_empty() {
                    return Ok(ToolOutput::text(
                        "Please provide a date in YYYY-MM-DD format.",
                    ));
                }
                let time_str = args.get("time").and_then(|v| v.as_str()).unwrap_or("09:00");

                let datetime_str = format!("{}T{}:00Z", date_str, time_str);
                let scheduled_dt = datetime_str.parse::<DateTime<Utc>>().map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "content_engine".to_string(),
                        message: format!("Invalid date/time '{}': {}", datetime_str, e),
                    }
                })?;

                let idx = match Self::find_piece(&state.pieces, id) {
                    Some(i) => i,
                    None => return Ok(ToolOutput::text(format!("Content #{} not found.", id))),
                };

                state.pieces[idx].status = ContentStatus::Scheduled;
                state.pieces[idx].scheduled_for = Some(scheduled_dt);
                state.pieces[idx].updated_at = Utc::now();
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Content #{} '{}' scheduled for {} {}.",
                    id, state.pieces[idx].title, date_str, time_str
                )))
            }

            "calendar_add" => {
                let date = args
                    .get("date")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if date.is_empty() {
                    return Ok(ToolOutput::text(
                        "Please provide a date in YYYY-MM-DD format.",
                    ));
                }
                let platform_str = args
                    .get("platform")
                    .and_then(|v| v.as_str())
                    .unwrap_or("blog");
                let platform =
                    ContentPlatform::from_str_loose(platform_str).unwrap_or(ContentPlatform::Blog);
                let topic = args
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if topic.is_empty() {
                    return Ok(ToolOutput::text("Please provide a topic."));
                }
                let content_id = args
                    .get("content_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let notes = args
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                state.calendar.push(CalendarEntry {
                    date: date.clone(),
                    platform: platform.clone(),
                    topic: topic.clone(),
                    content_id,
                    notes,
                });
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Added calendar entry: {} on {} ({}).",
                    topic, date, platform
                )))
            }

            "calendar_list" => {
                let month_filter = args.get("month").and_then(|v| v.as_str());
                let platform_filter = args
                    .get("platform")
                    .and_then(|v| v.as_str())
                    .and_then(ContentPlatform::from_str_loose);

                let filtered: Vec<&CalendarEntry> = state
                    .calendar
                    .iter()
                    .filter(|c| month_filter.map(|m| c.date.starts_with(m)).unwrap_or(true))
                    .filter(|c| {
                        platform_filter
                            .as_ref()
                            .map(|pf| c.platform == *pf)
                            .unwrap_or(true)
                    })
                    .collect();

                if filtered.is_empty() {
                    return Ok(ToolOutput::text("No calendar entries found."));
                }

                let lines: Vec<String> = filtered
                    .iter()
                    .map(|c| {
                        let linked = c
                            .content_id
                            .map(|id| format!(" (content #{})", id))
                            .unwrap_or_default();
                        let notes = if c.notes.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", c.notes)
                        };
                        format!(
                            "  {} | {} | {}{}{}",
                            c.date, c.platform, c.topic, linked, notes
                        )
                    })
                    .collect();

                Ok(ToolOutput::text(format!(
                    "Content calendar ({} entries):\n{}",
                    filtered.len(),
                    lines.join("\n")
                )))
            }

            "calendar_remove" => {
                let date = args.get("date").and_then(|v| v.as_str()).unwrap_or("");
                let platform_str = args.get("platform").and_then(|v| v.as_str()).unwrap_or("");
                let platform = match ContentPlatform::from_str_loose(platform_str) {
                    Some(p) => p,
                    None => {
                        return Ok(ToolOutput::text(format!(
                            "Unknown platform '{}'. Use: blog, twitter, linkedin, github, medium, newsletter.",
                            platform_str
                        )));
                    }
                };

                let before = state.calendar.len();
                state
                    .calendar
                    .retain(|c| !(c.date == date && c.platform == platform));
                let removed = before - state.calendar.len();

                if removed == 0 {
                    return Ok(ToolOutput::text(format!(
                        "No calendar entry found for {} on {}.",
                        platform, date
                    )));
                }

                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Removed {} calendar entry/entries for {} on {}.",
                    removed, platform, date
                )))
            }

            "stats" => {
                if state.pieces.is_empty() {
                    return Ok(ToolOutput::text("No content pieces yet."));
                }

                // Counts by status
                let mut by_status: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for p in &state.pieces {
                    *by_status.entry(p.status.as_str().to_string()).or_insert(0) += 1;
                }

                // Counts by platform
                let mut by_platform: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for p in &state.pieces {
                    *by_platform
                        .entry(p.platform.as_str().to_string())
                        .or_insert(0) += 1;
                }

                // Upcoming scheduled
                let now = Utc::now();
                let upcoming: Vec<&ContentPiece> = state
                    .pieces
                    .iter()
                    .filter(|p| {
                        p.status == ContentStatus::Scheduled
                            && p.scheduled_for.map(|s| s > now).unwrap_or(false)
                    })
                    .collect();

                // Total word count
                let total_words: usize = state.pieces.iter().map(|p| p.word_count).sum();

                let mut out = String::from("Content stats:\n");
                out.push_str(&format!("  Total pieces: {}\n", state.pieces.len()));
                out.push_str(&format!("  Total words:  {}\n\n", total_words));

                out.push_str("  By status:\n");
                let mut status_entries: Vec<_> = by_status.iter().collect();
                status_entries.sort_by_key(|(k, _)| (*k).clone());
                for (status, count) in &status_entries {
                    out.push_str(&format!("    {}: {}\n", status, count));
                }

                out.push_str("\n  By platform:\n");
                let mut platform_entries: Vec<_> = by_platform.iter().collect();
                platform_entries.sort_by_key(|(k, _)| (*k).clone());
                for (platform, count) in &platform_entries {
                    out.push_str(&format!("    {}: {}\n", platform, count));
                }

                if !upcoming.is_empty() {
                    out.push_str(&format!("\n  Upcoming scheduled: {}\n", upcoming.len()));
                    for p in &upcoming {
                        let date = p
                            .scheduled_for
                            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default();
                        out.push_str(&format!("    #{} '{}' — {}\n", p.id, p.title, date));
                    }
                }

                Ok(ToolOutput::text(out))
            }

            "adapt" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let target_str = args
                    .get("target_platform")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let target_platform = match ContentPlatform::from_str_loose(target_str) {
                    Some(p) => p,
                    None => {
                        return Ok(ToolOutput::text(format!(
                            "Unknown target platform '{}'. Use: blog, twitter, linkedin, github, medium, newsletter.",
                            target_str
                        )));
                    }
                };
                let target_tone = args
                    .get("target_tone")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let idx = match Self::find_piece(&state.pieces, id) {
                    Some(i) => i,
                    None => return Ok(ToolOutput::text(format!("Content #{} not found.", id))),
                };

                let piece = &state.pieces[idx];
                let constraints = Self::platform_constraints(&target_platform);

                let mut prompt = String::new();
                prompt.push_str(&format!(
                    "Adapt the following content for {}.\n\n",
                    target_platform
                ));
                prompt.push_str(&format!("Platform constraints:\n{}\n\n", constraints));
                if !target_tone.is_empty() {
                    prompt.push_str(&format!("Target tone: {}\n\n", target_tone));
                } else if !piece.tone.is_empty() {
                    prompt.push_str(&format!("Original tone: {}\n\n", piece.tone));
                }
                if !piece.audience.is_empty() {
                    prompt.push_str(&format!("Target audience: {}\n\n", piece.audience));
                }
                prompt.push_str(&format!("Original title: {}\n", piece.title));
                prompt.push_str(&format!("Original platform: {}\n\n", piece.platform));
                prompt.push_str(&format!("Original content:\n{}\n", piece.body));

                Ok(ToolOutput::text(format!(
                    "Adaptation prompt for #{} → {}:\n\n{}",
                    id, target_platform, prompt
                )))
            }

            "export_markdown" => {
                let id_filter = args.get("id").and_then(|v| v.as_u64()).map(|v| v as usize);
                let status_filter = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .and_then(ContentStatus::from_str_loose);

                let filtered: Vec<&ContentPiece> = state
                    .pieces
                    .iter()
                    .filter(|p| id_filter.map(|id| p.id == id).unwrap_or(true))
                    .filter(|p| {
                        status_filter
                            .as_ref()
                            .map(|sf| p.status == *sf)
                            .unwrap_or(true)
                    })
                    .collect();

                if filtered.is_empty() {
                    return Ok(ToolOutput::text("No content to export."));
                }

                let mut md = String::new();
                for piece in &filtered {
                    md.push_str(&format!("# {}\n\n", piece.title));
                    md.push_str(&format!(
                        "**Platform:** {} | **Status:** {} | **Words:** {}\n\n",
                        piece.platform, piece.status, piece.word_count
                    ));
                    if !piece.audience.is_empty() {
                        md.push_str(&format!("**Audience:** {}\n\n", piece.audience));
                    }
                    if !piece.tone.is_empty() {
                        md.push_str(&format!("**Tone:** {}\n\n", piece.tone));
                    }
                    if !piece.tags.is_empty() {
                        md.push_str(&format!("**Tags:** {}\n\n", piece.tags.join(", ")));
                    }
                    if !piece.body.is_empty() {
                        md.push_str(&format!("{}\n\n", piece.body));
                    }
                    md.push_str("---\n\n");
                }

                Ok(ToolOutput::text(format!(
                    "Exported {} piece(s) as Markdown:\n\n{}",
                    filtered.len(),
                    md
                )))
            }

            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{}'. Use: create, update, set_status, get, list, search, delete, schedule, calendar_add, calendar_list, calendar_remove, stats, adapt, export_markdown.",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (TempDir, ContentEngineTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ContentEngineTool::new(workspace);
        (dir, tool)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "content_engine");
        assert!(tool.description().contains("content pipeline"));
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert_eq!(tool.timeout(), std::time::Duration::from_secs(30));
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, tool) = make_tool();
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        let action_enum = &schema["properties"]["action"]["enum"];
        assert!(action_enum.is_array());
        let actions: Vec<&str> = action_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(actions.contains(&"create"));
        assert!(actions.contains(&"update"));
        assert!(actions.contains(&"set_status"));
        assert!(actions.contains(&"get"));
        assert!(actions.contains(&"list"));
        assert!(actions.contains(&"search"));
        assert!(actions.contains(&"delete"));
        assert!(actions.contains(&"schedule"));
        assert!(actions.contains(&"calendar_add"));
        assert!(actions.contains(&"calendar_list"));
        assert!(actions.contains(&"calendar_remove"));
        assert!(actions.contains(&"stats"));
        assert!(actions.contains(&"adapt"));
        assert!(actions.contains(&"export_markdown"));
        assert_eq!(actions.len(), 14);
    }

    #[tokio::test]
    async fn test_create_idea() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({"action": "create", "title": "AI trends"}))
            .await
            .unwrap();
        assert!(result.content.contains("Created content #1"));
        assert!(result.content.contains("AI trends"));
        assert!(result.content.contains("Idea"));
    }

    #[tokio::test]
    async fn test_create_draft() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "create",
                "title": "Rust ownership guide",
                "body": "Ownership is a set of rules that govern memory management.",
                "platform": "blog",
                "tags": ["rust", "programming"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Created content #1"));
        assert!(result.content.contains("Draft"));

        // Verify word count was computed
        let get_result = tool
            .execute(json!({"action": "get", "id": 1}))
            .await
            .unwrap();
        assert!(get_result.content.contains("Words:    10"));
        assert!(get_result.content.contains("rust, programming"));
    }

    #[tokio::test]
    async fn test_update_body_recomputes_word_count() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "create",
            "title": "Article",
            "body": "one two three"
        }))
        .await
        .unwrap();

        // Check initial word count
        let get1 = tool
            .execute(json!({"action": "get", "id": 1}))
            .await
            .unwrap();
        assert!(get1.content.contains("Words:    3"));

        // Update body
        tool.execute(json!({
            "action": "update",
            "id": 1,
            "body": "one two three four five six"
        }))
        .await
        .unwrap();

        let get2 = tool
            .execute(json!({"action": "get", "id": 1}))
            .await
            .unwrap();
        assert!(get2.content.contains("Words:    6"));
    }

    #[tokio::test]
    async fn test_status_lifecycle() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "create",
            "title": "Post",
            "body": "Some content here."
        }))
        .await
        .unwrap();

        // Draft -> Review
        let r = tool
            .execute(json!({"action": "set_status", "id": 1, "status": "review"}))
            .await
            .unwrap();
        assert!(r.content.contains("Review"));

        // Review -> Scheduled
        let r = tool
            .execute(json!({"action": "set_status", "id": 1, "status": "scheduled"}))
            .await
            .unwrap();
        assert!(r.content.contains("Scheduled"));

        // Scheduled -> Published (should set published_at)
        let r = tool
            .execute(json!({"action": "set_status", "id": 1, "status": "published"}))
            .await
            .unwrap();
        assert!(r.content.contains("Published"));

        let detail = tool
            .execute(json!({"action": "get", "id": 1}))
            .await
            .unwrap();
        assert!(detail.content.contains("Published:"));
    }

    #[tokio::test]
    async fn test_search_across_fields() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "create",
            "title": "Kubernetes basics",
            "body": "Learn about pods and deployments.",
            "tags": ["devops", "containers"]
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "create",
            "title": "Cooking pasta",
            "body": "Boil water and add salt."
        }))
        .await
        .unwrap();

        // Search by title
        let r = tool
            .execute(json!({"action": "search", "query": "kubernetes"}))
            .await
            .unwrap();
        assert!(r.content.contains("Kubernetes basics"));
        assert!(!r.content.contains("Cooking"));

        // Search by body
        let r = tool
            .execute(json!({"action": "search", "query": "pods"}))
            .await
            .unwrap();
        assert!(r.content.contains("Kubernetes"));

        // Search by tag
        let r = tool
            .execute(json!({"action": "search", "query": "devops"}))
            .await
            .unwrap();
        assert!(r.content.contains("Kubernetes"));

        // No match
        let r = tool
            .execute(json!({"action": "search", "query": "zzznomatch"}))
            .await
            .unwrap();
        assert!(r.content.contains("No content matching"));
    }

    #[tokio::test]
    async fn test_delete_cascades_calendar() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "create",
            "title": "Blog post",
            "body": "Content body."
        }))
        .await
        .unwrap();

        // Add a calendar entry linked to content #1
        tool.execute(json!({
            "action": "calendar_add",
            "date": "2026-03-15",
            "platform": "blog",
            "topic": "Publish blog post",
            "content_id": 1
        }))
        .await
        .unwrap();

        // Verify calendar entry exists
        let cal = tool
            .execute(json!({"action": "calendar_list"}))
            .await
            .unwrap();
        assert!(cal.content.contains("Publish blog post"));

        // Delete content #1 — should cascade to calendar
        let del = tool
            .execute(json!({"action": "delete", "id": 1}))
            .await
            .unwrap();
        assert!(del.content.contains("Deleted content #1"));

        // Calendar should be empty
        let cal2 = tool
            .execute(json!({"action": "calendar_list"}))
            .await
            .unwrap();
        assert!(cal2.content.contains("No calendar entries"));
    }

    #[tokio::test]
    async fn test_schedule_sets_status() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "create",
            "title": "Scheduled post",
            "body": "Will go live soon."
        }))
        .await
        .unwrap();

        let r = tool
            .execute(json!({
                "action": "schedule",
                "id": 1,
                "date": "2026-04-01",
                "time": "14:30"
            }))
            .await
            .unwrap();
        assert!(r.content.contains("scheduled for 2026-04-01 14:30"));

        let detail = tool
            .execute(json!({"action": "get", "id": 1}))
            .await
            .unwrap();
        assert!(detail.content.contains("Status:   Scheduled"));
        assert!(detail.content.contains("Scheduled: 2026-04-01 14:30"));
    }

    #[tokio::test]
    async fn test_calendar_crud() {
        let (_dir, tool) = make_tool();

        // Add
        let r = tool
            .execute(json!({
                "action": "calendar_add",
                "date": "2026-03-01",
                "platform": "twitter",
                "topic": "Thread on Rust async"
            }))
            .await
            .unwrap();
        assert!(r.content.contains("Added calendar entry"));

        // List
        let r = tool
            .execute(json!({"action": "calendar_list"}))
            .await
            .unwrap();
        assert!(r.content.contains("Thread on Rust async"));
        assert!(r.content.contains("Twitter"));
        assert!(r.content.contains("2026-03-01"));

        // Remove
        let r = tool
            .execute(json!({
                "action": "calendar_remove",
                "date": "2026-03-01",
                "platform": "twitter"
            }))
            .await
            .unwrap();
        assert!(r.content.contains("Removed"));

        // Verify empty
        let r = tool
            .execute(json!({"action": "calendar_list"}))
            .await
            .unwrap();
        assert!(r.content.contains("No calendar entries"));
    }

    #[tokio::test]
    async fn test_calendar_list_filter_month() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "calendar_add",
            "date": "2026-03-01",
            "platform": "blog",
            "topic": "March post"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "calendar_add",
            "date": "2026-04-15",
            "platform": "blog",
            "topic": "April post"
        }))
        .await
        .unwrap();

        // Filter by month
        let r = tool
            .execute(json!({"action": "calendar_list", "month": "2026-03"}))
            .await
            .unwrap();
        assert!(r.content.contains("March post"));
        assert!(!r.content.contains("April post"));

        // Different month
        let r = tool
            .execute(json!({"action": "calendar_list", "month": "2026-04"}))
            .await
            .unwrap();
        assert!(r.content.contains("April post"));
        assert!(!r.content.contains("March post"));
    }

    #[tokio::test]
    async fn test_stats_counts() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "create",
            "title": "Post A",
            "body": "word1 word2 word3",
            "platform": "blog"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "create",
            "title": "Tweet B",
            "body": "short tweet",
            "platform": "twitter"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "create",
            "title": "Idea C"
        }))
        .await
        .unwrap();

        let r = tool.execute(json!({"action": "stats"})).await.unwrap();
        assert!(r.content.contains("Total pieces: 3"));
        assert!(r.content.contains("Total words:  5"));
        // Post A (blog) + Idea C (defaults to blog) = 2 Blog
        assert!(r.content.contains("Blog: 2"));
        assert!(r.content.contains("Twitter: 1"));
        assert!(r.content.contains("Draft: 2"));
        assert!(r.content.contains("Idea: 1"));
    }

    #[tokio::test]
    async fn test_adapt_twitter_constraints() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "create",
            "title": "Big blog post",
            "body": "This is a long form blog post about Rust programming language.",
            "platform": "blog"
        }))
        .await
        .unwrap();

        let r = tool
            .execute(json!({
                "action": "adapt",
                "id": 1,
                "target_platform": "twitter"
            }))
            .await
            .unwrap();
        assert!(r.content.contains("280 char"));
        assert!(r.content.contains("Twitter"));
        assert!(r.content.contains("Big blog post"));
    }

    #[tokio::test]
    async fn test_adapt_linkedin_constraints() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "create",
            "title": "Tech article",
            "body": "Technical content about distributed systems.",
            "platform": "blog"
        }))
        .await
        .unwrap();

        let r = tool
            .execute(json!({
                "action": "adapt",
                "id": 1,
                "target_platform": "linkedin",
                "target_tone": "thought-leadership"
            }))
            .await
            .unwrap();
        assert!(r.content.contains("rofessional")); // "Professional" case-insensitive partial
        assert!(r.content.contains("LinkedIn"));
        assert!(r.content.contains("thought-leadership"));
    }

    #[tokio::test]
    async fn test_export_markdown() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "create",
            "title": "Markdown test",
            "body": "Export this content.",
            "platform": "medium",
            "audience": "developers",
            "tone": "casual",
            "tags": ["test", "export"]
        }))
        .await
        .unwrap();

        let r = tool
            .execute(json!({"action": "export_markdown", "id": 1}))
            .await
            .unwrap();
        assert!(r.content.contains("# Markdown test"));
        assert!(r.content.contains("**Platform:** Medium"));
        assert!(r.content.contains("**Status:** Draft"));
        assert!(r.content.contains("**Audience:** developers"));
        assert!(r.content.contains("**Tone:** casual"));
        assert!(r.content.contains("**Tags:** test, export"));
        assert!(r.content.contains("Export this content."));
    }

    #[tokio::test]
    async fn test_list_filter_platform() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "create",
            "title": "Blog A",
            "body": "body",
            "platform": "blog"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "create",
            "title": "Tweet B",
            "body": "body",
            "platform": "twitter"
        }))
        .await
        .unwrap();

        let r = tool
            .execute(json!({"action": "list", "platform": "blog"}))
            .await
            .unwrap();
        assert!(r.content.contains("Blog A"));
        assert!(!r.content.contains("Tweet B"));
    }

    #[tokio::test]
    async fn test_list_filter_status() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "create",
            "title": "Draft piece",
            "body": "has body"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "create",
            "title": "Idea piece"
        }))
        .await
        .unwrap();

        let r = tool
            .execute(json!({"action": "list", "status": "idea"}))
            .await
            .unwrap();
        assert!(r.content.contains("Idea piece"));
        assert!(!r.content.contains("Draft piece"));

        let r = tool
            .execute(json!({"action": "list", "status": "draft"}))
            .await
            .unwrap();
        assert!(r.content.contains("Draft piece"));
        assert!(!r.content.contains("Idea piece"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();

        // Create some state
        tool.execute(json!({
            "action": "create",
            "title": "Persisted",
            "body": "Body text here.",
            "platform": "github",
            "tags": ["persist"]
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "calendar_add",
            "date": "2026-06-01",
            "platform": "github",
            "topic": "Release notes"
        }))
        .await
        .unwrap();

        // Load raw state and verify roundtrip
        let state = tool.load_state();
        assert_eq!(state.pieces.len(), 1);
        assert_eq!(state.pieces[0].title, "Persisted");
        assert_eq!(state.pieces[0].platform, ContentPlatform::GitHub);
        assert_eq!(state.pieces[0].word_count, 3);
        assert_eq!(state.calendar.len(), 1);
        assert_eq!(state.calendar[0].topic, "Release notes");
        assert_eq!(state.next_id, 2);

        // Save and reload
        tool.save_state(&state).unwrap();
        let reloaded = tool.load_state();
        assert_eq!(reloaded.pieces.len(), 1);
        assert_eq!(reloaded.calendar.len(), 1);
        assert_eq!(reloaded.next_id, 2);
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let (_dir, tool) = make_tool();
        let r = tool.execute(json!({"action": "foobar"})).await.unwrap();
        assert!(r.content.contains("Unknown action"));
        assert!(r.content.contains("foobar"));
    }
}
