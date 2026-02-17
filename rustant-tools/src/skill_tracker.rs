//! Skill tracker tool — track skill progression, knowledge gaps, and learning paths.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PracticeEntry {
    date: DateTime<Utc>,
    duration_mins: u32,
    notes: String,
    proficiency_before: u32,
    proficiency_after: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Skill {
    id: usize,
    name: String,
    category: String,
    proficiency_level: u32, // 0-100
    target_level: u32,
    last_practiced: Option<DateTime<Utc>>,
    practice_log: Vec<PracticeEntry>,
    resources: Vec<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Milestone {
    name: String,
    description: String,
    target_proficiency: u32,
    completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LearningPath {
    id: usize,
    name: String,
    skill_ids: Vec<usize>,
    milestones: Vec<Milestone>,
    current_milestone: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SkillState {
    skills: Vec<Skill>,
    learning_paths: Vec<LearningPath>,
    next_skill_id: usize,
    next_path_id: usize,
}

pub struct SkillTrackerTool {
    workspace: PathBuf,
}

impl SkillTrackerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("skills")
            .join("tracker.json")
    }

    fn load_state(&self) -> SkillState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            SkillState {
                skills: Vec::new(),
                learning_paths: Vec::new(),
                next_skill_id: 1,
                next_path_id: 1,
            }
        }
    }

    fn save_state(&self, state: &SkillState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "skill_tracker".to_string(),
                message: format!("Failed to create state dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "skill_tracker".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "skill_tracker".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "skill_tracker".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }

    fn flashcards_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("flashcards")
            .join("cards.json")
    }
}

#[async_trait]
impl Tool for SkillTrackerTool {
    fn name(&self) -> &str {
        "skill_tracker"
    }

    fn description(&self) -> &str {
        "Track skill progression, knowledge gaps, and learning paths. Actions: add_skill, log_practice, assess, list_skills, knowledge_gaps, learning_path, progress_report, daily_practice."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add_skill", "log_practice", "assess", "list_skills", "knowledge_gaps", "learning_path", "progress_report", "daily_practice"],
                    "description": "Action to perform"
                },
                "name": { "type": "string", "description": "Skill name (for add_skill)" },
                "category": { "type": "string", "description": "Skill category (for add_skill, list_skills filter)" },
                "target_level": { "type": "integer", "description": "Target proficiency 0-100 (default: 80)" },
                "resources": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Learning resources (URLs, book names, etc.)"
                },
                "skill_id": { "type": "integer", "description": "Skill ID (for log_practice, assess)" },
                "duration_mins": { "type": "integer", "description": "Practice duration in minutes" },
                "notes": { "type": "string", "description": "Practice session notes" },
                "new_proficiency": { "type": "integer", "description": "Updated proficiency level 0-100 after practice" },
                "sub_action": {
                    "type": "string",
                    "enum": ["create", "update", "show"],
                    "description": "Learning path sub-action"
                },
                "path_id": { "type": "integer", "description": "Learning path ID" },
                "skill_ids": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Skill IDs for learning path"
                },
                "milestones": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "description": { "type": "string" },
                            "target_proficiency": { "type": "integer" }
                        }
                    },
                    "description": "Milestones for learning path"
                },
                "current_milestone": { "type": "integer", "description": "Current milestone index (for update)" },
                "period": {
                    "type": "string",
                    "enum": ["week", "month", "all"],
                    "description": "Report period (default: all)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let mut state = self.load_state();

        match action {
            "add_skill" => {
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() || category.is_empty() {
                    return Ok(ToolOutput::text(
                        "Provide both name and category for the skill.",
                    ));
                }
                let target_level = args
                    .get("target_level")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(80) as u32;
                let target_level = target_level.min(100);
                let resources: Vec<String> = args
                    .get("resources")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let id = state.next_skill_id;
                state.next_skill_id += 1;
                state.skills.push(Skill {
                    id,
                    name: name.to_string(),
                    category: category.to_string(),
                    proficiency_level: 0,
                    target_level,
                    last_practiced: None,
                    practice_log: Vec::new(),
                    resources,
                    created_at: Utc::now(),
                });
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Added skill #{}: '{}' [{}] with target level {}.",
                    id, name, category, target_level
                )))
            }

            "log_practice" => {
                let skill_id = args.get("skill_id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let duration_mins = args
                    .get("duration_mins")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                if skill_id == 0 || duration_mins == 0 {
                    return Ok(ToolOutput::text(
                        "Provide skill_id and duration_mins (both > 0).",
                    ));
                }
                let notes = args
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let new_proficiency = args
                    .get("new_proficiency")
                    .and_then(|v| v.as_u64())
                    .map(|v| (v as u32).min(100));

                if let Some(skill) = state.skills.iter_mut().find(|s| s.id == skill_id) {
                    let proficiency_before = skill.proficiency_level;
                    let proficiency_after = new_proficiency.unwrap_or(proficiency_before);
                    skill.practice_log.push(PracticeEntry {
                        date: Utc::now(),
                        duration_mins,
                        notes: notes.clone(),
                        proficiency_before,
                        proficiency_after,
                    });
                    skill.last_practiced = Some(Utc::now());
                    if let Some(np) = new_proficiency {
                        skill.proficiency_level = np;
                    }
                    let skill_name = skill.name.clone();
                    let current = skill.proficiency_level;
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!(
                        "Logged {} min practice for '{}'. Proficiency: {} -> {}.",
                        duration_mins, skill_name, proficiency_before, current
                    )))
                } else {
                    Ok(ToolOutput::text(format!("Skill #{} not found.", skill_id)))
                }
            }

            "assess" => {
                let skill_id = args.get("skill_id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Some(skill) = state.skills.iter().find(|s| s.id == skill_id) {
                    let mut prompt = format!(
                        "=== Self-Assessment: {} ===\n\
                         Category: {}\n\
                         Current Proficiency: {}/100\n\
                         Target Level: {}/100\n\
                         Gap: {} points\n",
                        skill.name,
                        skill.category,
                        skill.proficiency_level,
                        skill.target_level,
                        skill.target_level.saturating_sub(skill.proficiency_level)
                    );
                    if let Some(ref last) = skill.last_practiced {
                        prompt.push_str(&format!(
                            "Last Practiced: {}\n",
                            last.format("%Y-%m-%d %H:%M UTC")
                        ));
                    } else {
                        prompt.push_str("Last Practiced: Never\n");
                    }
                    if !skill.resources.is_empty() {
                        prompt.push_str(&format!("Resources: {}\n", skill.resources.join(", ")));
                    }
                    if !skill.practice_log.is_empty() {
                        let total_mins: u32 =
                            skill.practice_log.iter().map(|e| e.duration_mins).sum();
                        let sessions = skill.practice_log.len();
                        prompt.push_str(&format!(
                            "\nPractice History: {} sessions, {} total minutes\n",
                            sessions, total_mins
                        ));
                        prompt.push_str("Recent entries:\n");
                        for entry in skill.practice_log.iter().rev().take(5) {
                            prompt.push_str(&format!(
                                "  - {} ({} min): {} [{}->{}]\n",
                                entry.date.format("%Y-%m-%d"),
                                entry.duration_mins,
                                if entry.notes.is_empty() {
                                    "(no notes)"
                                } else {
                                    &entry.notes
                                },
                                entry.proficiency_before,
                                entry.proficiency_after,
                            ));
                        }
                    }
                    prompt.push_str(
                        "\nReflection prompts:\n\
                         1. What specific sub-skills do you feel weakest in?\n\
                         2. What has been your most effective learning method?\n\
                         3. What obstacles are preventing faster progress?\n\
                         4. What is your next concrete milestone?",
                    );
                    Ok(ToolOutput::text(prompt))
                } else {
                    Ok(ToolOutput::text(format!("Skill #{} not found.", skill_id)))
                }
            }

            "list_skills" => {
                let category_filter = args.get("category").and_then(|v| v.as_str());
                let filtered: Vec<&Skill> = state
                    .skills
                    .iter()
                    .filter(|s| {
                        category_filter
                            .map(|c| s.category.eq_ignore_ascii_case(c))
                            .unwrap_or(true)
                    })
                    .collect();
                if filtered.is_empty() {
                    return Ok(ToolOutput::text(if let Some(cat) = category_filter {
                        format!("No skills found in category '{}'.", cat)
                    } else {
                        "No skills tracked yet.".to_string()
                    }));
                }
                let mut lines = Vec::new();
                for skill in &filtered {
                    let last = skill
                        .last_practiced
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "never".to_string());
                    lines.push(format!(
                        "  #{} {} [{}]: {}/{} (last: {})",
                        skill.id,
                        skill.name,
                        skill.category,
                        skill.proficiency_level,
                        skill.target_level,
                        last
                    ));
                }
                Ok(ToolOutput::text(format!(
                    "Skills ({}):\n{}",
                    filtered.len(),
                    lines.join("\n")
                )))
            }

            "knowledge_gaps" => {
                let mut gaps: Vec<&Skill> = state
                    .skills
                    .iter()
                    .filter(|s| s.proficiency_level < s.target_level)
                    .collect();
                if gaps.is_empty() {
                    return Ok(ToolOutput::text(
                        "No knowledge gaps — all skills are at or above target!",
                    ));
                }
                // Sort by biggest gap first
                gaps.sort_by(|a, b| {
                    let gap_a = a.target_level - a.proficiency_level;
                    let gap_b = b.target_level - b.proficiency_level;
                    gap_b.cmp(&gap_a)
                });
                let now = Utc::now();
                let stale_threshold = chrono::Duration::days(14);
                let mut lines = Vec::new();
                for skill in &gaps {
                    let gap = skill.target_level - skill.proficiency_level;
                    let stale = skill
                        .last_practiced
                        .map(|lp| (now - lp) > stale_threshold)
                        .unwrap_or(true);
                    let stale_marker = if stale { " [STALE]" } else { "" };
                    lines.push(format!(
                        "  #{} {} [{}]: {}/{} (gap: {}){} ",
                        skill.id,
                        skill.name,
                        skill.category,
                        skill.proficiency_level,
                        skill.target_level,
                        gap,
                        stale_marker
                    ));
                }
                Ok(ToolOutput::text(format!(
                    "Knowledge gaps ({} skills below target):\n{}",
                    gaps.len(),
                    lines.join("\n")
                )))
            }

            "learning_path" => {
                let sub_action = args
                    .get("sub_action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match sub_action {
                    "create" => {
                        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        if name.is_empty() {
                            return Ok(ToolOutput::text("Provide a name for the learning path."));
                        }
                        let skill_ids: Vec<usize> = args
                            .get("skill_ids")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                                    .collect()
                            })
                            .unwrap_or_default();
                        if skill_ids.is_empty() {
                            return Ok(ToolOutput::text(
                                "Provide at least one skill_id for the learning path.",
                            ));
                        }
                        // Validate skill IDs exist
                        for &sid in &skill_ids {
                            if !state.skills.iter().any(|s| s.id == sid) {
                                return Ok(ToolOutput::text(format!("Skill #{} not found.", sid)));
                            }
                        }
                        let milestones: Vec<Milestone> = args
                            .get("milestones")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|m| Milestone {
                                        name: m
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        description: m
                                            .get("description")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        target_proficiency: m
                                            .get("target_proficiency")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(50)
                                            as u32,
                                        completed: false,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let id = state.next_path_id;
                        state.next_path_id += 1;
                        state.learning_paths.push(LearningPath {
                            id,
                            name: name.to_string(),
                            skill_ids,
                            milestones,
                            current_milestone: 0,
                        });
                        self.save_state(&state)?;
                        Ok(ToolOutput::text(format!(
                            "Created learning path #{}: '{}'.",
                            id, name
                        )))
                    }
                    "show" => {
                        let path_id =
                            args.get("path_id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        if let Some(path) = state.learning_paths.iter().find(|p| p.id == path_id) {
                            let mut output =
                                format!("Learning Path #{}: '{}'\nSkills:\n", path.id, path.name);
                            for &sid in &path.skill_ids {
                                if let Some(skill) = state.skills.iter().find(|s| s.id == sid) {
                                    output.push_str(&format!(
                                        "  #{} {} — {}/{}\n",
                                        skill.id,
                                        skill.name,
                                        skill.proficiency_level,
                                        skill.target_level
                                    ));
                                } else {
                                    output.push_str(&format!("  #{} (not found)\n", sid));
                                }
                            }
                            if !path.milestones.is_empty() {
                                output.push_str("Milestones:\n");
                                for (i, ms) in path.milestones.iter().enumerate() {
                                    let marker = if ms.completed {
                                        "[x]"
                                    } else if i == path.current_milestone {
                                        "[>]"
                                    } else {
                                        "[ ]"
                                    };
                                    output.push_str(&format!(
                                        "  {} {} — {} (target: {})\n",
                                        marker, ms.name, ms.description, ms.target_proficiency
                                    ));
                                }
                            }
                            Ok(ToolOutput::text(output))
                        } else {
                            Ok(ToolOutput::text(format!(
                                "Learning path #{} not found.",
                                path_id
                            )))
                        }
                    }
                    "update" => {
                        let path_id =
                            args.get("path_id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        if let Some(path) =
                            state.learning_paths.iter_mut().find(|p| p.id == path_id)
                        {
                            if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                                path.name = name.to_string();
                            }
                            if let Some(milestones) =
                                args.get("milestones").and_then(|v| v.as_array())
                            {
                                path.milestones = milestones
                                    .iter()
                                    .map(|m| Milestone {
                                        name: m
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        description: m
                                            .get("description")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        target_proficiency: m
                                            .get("target_proficiency")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(50)
                                            as u32,
                                        completed: m
                                            .get("completed")
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false),
                                    })
                                    .collect();
                            }
                            if let Some(cm) = args.get("current_milestone").and_then(|v| v.as_u64())
                            {
                                path.current_milestone = cm as usize;
                            }
                            let path_name = path.name.clone();
                            self.save_state(&state)?;
                            Ok(ToolOutput::text(format!(
                                "Updated learning path #{}: '{}'.",
                                path_id, path_name
                            )))
                        } else {
                            Ok(ToolOutput::text(format!(
                                "Learning path #{} not found.",
                                path_id
                            )))
                        }
                    }
                    _ => Ok(ToolOutput::text(format!(
                        "Unknown sub_action: '{}'. Use: create, show, update.",
                        sub_action
                    ))),
                }
            }

            "progress_report" => {
                let period = args.get("period").and_then(|v| v.as_str()).unwrap_or("all");
                let now = Utc::now();
                let cutoff = match period {
                    "week" => Some(now - chrono::Duration::days(7)),
                    "month" => Some(now - chrono::Duration::days(30)),
                    _ => None, // "all"
                };

                let mut total_practice_mins: u32 = 0;
                let mut skills_improved: usize = 0;
                let mut sessions_count: usize = 0;

                for skill in &state.skills {
                    let relevant_entries: Vec<&PracticeEntry> = skill
                        .practice_log
                        .iter()
                        .filter(|e| cutoff.map(|c| e.date >= c).unwrap_or(true))
                        .collect();
                    if !relevant_entries.is_empty() {
                        sessions_count += relevant_entries.len();
                        total_practice_mins += relevant_entries
                            .iter()
                            .map(|e| e.duration_mins)
                            .sum::<u32>();
                        // Check if proficiency improved during this period
                        if let (Some(first), Some(last)) =
                            (relevant_entries.first(), relevant_entries.last())
                            && last.proficiency_after > first.proficiency_before {
                                skills_improved += 1;
                            }
                    }
                }

                // Count completed milestones across all learning paths
                let milestones_completed: usize = state
                    .learning_paths
                    .iter()
                    .flat_map(|p| p.milestones.iter())
                    .filter(|m| m.completed)
                    .count();

                let hours = total_practice_mins / 60;
                let mins = total_practice_mins % 60;
                Ok(ToolOutput::text(format!(
                    "Progress Report ({}):\n\
                     \x20 Total practice time: {}h {}m ({} sessions)\n\
                     \x20 Skills improved: {}\n\
                     \x20 Milestones completed: {}\n\
                     \x20 Skills tracked: {}",
                    period,
                    hours,
                    mins,
                    sessions_count,
                    skills_improved,
                    milestones_completed,
                    state.skills.len()
                )))
            }

            "daily_practice" => {
                if state.skills.is_empty() {
                    return Ok(ToolOutput::text(
                        "No skills tracked yet. Add some skills first.",
                    ));
                }

                let now = Utc::now();
                let stale_threshold = chrono::Duration::days(14);

                // Score each skill: higher score = higher priority
                // Score = gap_size + stale_bonus
                let mut scored: Vec<(&Skill, u32)> = state
                    .skills
                    .iter()
                    .filter(|s| s.proficiency_level < s.target_level)
                    .map(|s| {
                        let gap = s.target_level - s.proficiency_level;
                        let stale = s
                            .last_practiced
                            .map(|lp| (now - lp) > stale_threshold)
                            .unwrap_or(true);
                        let stale_bonus: u32 = if stale { 25 } else { 0 };
                        (s, gap + stale_bonus)
                    })
                    .collect();

                scored.sort_by(|a, b| b.1.cmp(&a.1));
                let top3: Vec<&(&Skill, u32)> = scored.iter().take(3).collect();

                if top3.is_empty() {
                    return Ok(ToolOutput::text(
                        "All skills are at target! Consider raising your targets or adding new skills.",
                    ));
                }

                let mut output = String::from("Daily Practice Suggestions:\n");
                for (i, (skill, score)) in top3.iter().enumerate() {
                    let stale = skill
                        .last_practiced
                        .map(|lp| (now - lp) > stale_threshold)
                        .unwrap_or(true);
                    let stale_marker = if stale { " (STALE)" } else { "" };
                    output.push_str(&format!(
                        "  {}. #{} {} [{}]: {}/{} (priority: {}){}\n",
                        i + 1,
                        skill.id,
                        skill.name,
                        skill.category,
                        skill.proficiency_level,
                        skill.target_level,
                        score,
                        stale_marker
                    ));
                }

                // Cross-reference with flashcards if available
                let flashcards_path = self.flashcards_path();
                if flashcards_path.exists()
                    && let Ok(fc_data) = std::fs::read_to_string(&flashcards_path)
                        && let Ok(fc_state) = serde_json::from_str::<Value>(&fc_data)
                            && let Some(cards) = fc_state.get("cards").and_then(|v| v.as_array()) {
                                let decks: std::collections::HashSet<String> = cards
                                    .iter()
                                    .filter_map(|c| {
                                        c.get("deck")
                                            .and_then(|d| d.as_str())
                                            .map(|s| s.to_lowercase())
                                    })
                                    .collect();
                                if !decks.is_empty() {
                                    let mut relevant_decks = Vec::new();
                                    for (skill, _) in &top3 {
                                        let name_lower = skill.name.to_lowercase();
                                        let cat_lower = skill.category.to_lowercase();
                                        for deck in &decks {
                                            if deck.contains(&name_lower)
                                                || name_lower.contains(deck.as_str())
                                                || deck.contains(&cat_lower)
                                                || cat_lower.contains(deck.as_str())
                                            {
                                                relevant_decks.push(deck.clone());
                                            }
                                        }
                                    }
                                    relevant_decks.sort();
                                    relevant_decks.dedup();
                                    if !relevant_decks.is_empty() {
                                        output.push_str(&format!(
                                            "\nRelated flashcard decks: {}",
                                            relevant_decks.join(", ")
                                        ));
                                    }
                                }
                            }

                Ok(ToolOutput::text(output))
            }

            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{}'. Use: add_skill, log_practice, assess, list_skills, knowledge_gaps, learning_path, progress_report, daily_practice.",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (TempDir, SkillTrackerTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = SkillTrackerTool::new(workspace);
        (dir, tool)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "skill_tracker");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    #[tokio::test]
    async fn test_add_skill() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "add_skill",
                "name": "Rust",
                "category": "Programming",
                "target_level": 90,
                "resources": ["The Rust Book", "Rustlings"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Added skill #1"));
        assert!(result.content.contains("Rust"));
        assert!(result.content.contains("90"));

        // Verify it appears in list
        let list = tool
            .execute(json!({"action": "list_skills"}))
            .await
            .unwrap();
        assert!(list.content.contains("Rust"));
        assert!(list.content.contains("Programming"));
        assert!(list.content.contains("0/90"));
    }

    #[tokio::test]
    async fn test_log_practice_updates_proficiency() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_skill",
            "name": "Python",
            "category": "Programming"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "log_practice",
                "skill_id": 1,
                "duration_mins": 60,
                "notes": "Worked on async/await",
                "new_proficiency": 25
            }))
            .await
            .unwrap();
        assert!(result.content.contains("60 min"));
        assert!(result.content.contains("Python"));
        assert!(result.content.contains("0 -> 25"));

        // Verify proficiency updated in list
        let list = tool
            .execute(json!({"action": "list_skills"}))
            .await
            .unwrap();
        assert!(list.content.contains("25/80"));
    }

    #[tokio::test]
    async fn test_proficiency_clamping() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_skill",
            "name": "Go",
            "category": "Programming"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "log_practice",
                "skill_id": 1,
                "duration_mins": 30,
                "new_proficiency": 150
            }))
            .await
            .unwrap();
        // Should be clamped to 100
        assert!(result.content.contains("0 -> 100"));

        let list = tool
            .execute(json!({"action": "list_skills"}))
            .await
            .unwrap();
        assert!(list.content.contains("100/80"));
    }

    #[tokio::test]
    async fn test_knowledge_gaps_sorted_by_gap() {
        let (_dir, tool) = make_tool();
        // Skill 1: gap = 80 - 50 = 30
        tool.execute(json!({
            "action": "add_skill",
            "name": "Small Gap",
            "category": "A"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "log_practice",
            "skill_id": 1,
            "duration_mins": 10,
            "new_proficiency": 50
        }))
        .await
        .unwrap();

        // Skill 2: gap = 80 - 10 = 70
        tool.execute(json!({
            "action": "add_skill",
            "name": "Big Gap",
            "category": "B"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "log_practice",
            "skill_id": 2,
            "duration_mins": 10,
            "new_proficiency": 10
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "knowledge_gaps"}))
            .await
            .unwrap();
        // Big Gap (gap=70) should appear before Small Gap (gap=30)
        let big_pos = result.content.find("Big Gap").unwrap();
        let small_pos = result.content.find("Small Gap").unwrap();
        assert!(big_pos < small_pos, "Bigger gap should be listed first");
    }

    #[tokio::test]
    async fn test_daily_practice_suggestions() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_skill",
            "name": "TypeScript",
            "category": "Programming",
            "target_level": 80
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "daily_practice"}))
            .await
            .unwrap();
        assert!(result.content.contains("Daily Practice Suggestions"));
        assert!(result.content.contains("TypeScript"));
    }

    #[tokio::test]
    async fn test_learning_path_crud() {
        let (_dir, tool) = make_tool();
        // Add skills first
        tool.execute(json!({
            "action": "add_skill",
            "name": "HTML",
            "category": "Web"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "add_skill",
            "name": "CSS",
            "category": "Web"
        }))
        .await
        .unwrap();

        // Create path
        let result = tool
            .execute(json!({
                "action": "learning_path",
                "sub_action": "create",
                "name": "Web Fundamentals",
                "skill_ids": [1, 2],
                "milestones": [
                    {"name": "Basics", "description": "Learn HTML/CSS basics", "target_proficiency": 40},
                    {"name": "Advanced", "description": "Build responsive layouts", "target_proficiency": 80}
                ]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Created learning path #1"));
        assert!(result.content.contains("Web Fundamentals"));

        // Show path
        let result = tool
            .execute(json!({
                "action": "learning_path",
                "sub_action": "show",
                "path_id": 1
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Web Fundamentals"));
        assert!(result.content.contains("HTML"));
        assert!(result.content.contains("CSS"));
        assert!(result.content.contains("Basics"));
        assert!(result.content.contains("Advanced"));

        // Update path
        let result = tool
            .execute(json!({
                "action": "learning_path",
                "sub_action": "update",
                "path_id": 1,
                "name": "Web Dev Path",
                "current_milestone": 1
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Updated learning path #1"));
        assert!(result.content.contains("Web Dev Path"));
    }

    #[tokio::test]
    async fn test_assess_returns_prompt() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_skill",
            "name": "Machine Learning",
            "category": "Data Science",
            "resources": ["Coursera ML Course"]
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "assess",
                "skill_id": 1
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Self-Assessment: Machine Learning"));
        assert!(result.content.contains("Category: Data Science"));
        assert!(result.content.contains("Current Proficiency: 0/100"));
        assert!(result.content.contains("Target Level: 80/100"));
        assert!(result.content.contains("Reflection prompts"));
        assert!(result.content.contains("Coursera ML Course"));
    }

    #[tokio::test]
    async fn test_progress_report_empty() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({"action": "progress_report"}))
            .await
            .unwrap();
        assert!(result.content.contains("Progress Report"));
        assert!(result.content.contains("0h 0m"));
        assert!(result.content.contains("Skills improved: 0"));
        assert!(result.content.contains("Milestones completed: 0"));
        assert!(result.content.contains("Skills tracked: 0"));
    }

    #[tokio::test]
    async fn test_list_skills_filter_category() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "add_skill",
            "name": "Rust",
            "category": "Programming"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "add_skill",
            "name": "Piano",
            "category": "Music"
        }))
        .await
        .unwrap();

        // Filter by Programming
        let result = tool
            .execute(json!({"action": "list_skills", "category": "Programming"}))
            .await
            .unwrap();
        assert!(result.content.contains("Rust"));
        assert!(!result.content.contains("Piano"));

        // Filter by Music
        let result = tool
            .execute(json!({"action": "list_skills", "category": "Music"}))
            .await
            .unwrap();
        assert!(!result.content.contains("Rust"));
        assert!(result.content.contains("Piano"));

        // No filter — both present
        let result = tool
            .execute(json!({"action": "list_skills"}))
            .await
            .unwrap();
        assert!(result.content.contains("Rust"));
        assert!(result.content.contains("Piano"));
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, tool) = make_tool();
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        assert!(schema.get("properties").is_some());
        let props = schema.get("properties").unwrap();
        assert!(props.get("action").is_some());
        assert!(props.get("name").is_some());
        assert!(props.get("category").is_some());
        assert!(props.get("skill_id").is_some());
        assert!(props.get("duration_mins").is_some());
        assert!(props.get("sub_action").is_some());
        assert!(props.get("period").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].as_str().unwrap(), "action");
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();
        // Add skill, log practice, create path
        tool.execute(json!({
            "action": "add_skill",
            "name": "Rust",
            "category": "Programming",
            "target_level": 90
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "log_practice",
            "skill_id": 1,
            "duration_mins": 45,
            "new_proficiency": 30
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "learning_path",
            "sub_action": "create",
            "name": "Rust Mastery",
            "skill_ids": [1]
        }))
        .await
        .unwrap();

        // Verify state persists by loading fresh
        let state = tool.load_state();
        assert_eq!(state.skills.len(), 1);
        assert_eq!(state.skills[0].name, "Rust");
        assert_eq!(state.skills[0].proficiency_level, 30);
        assert_eq!(state.skills[0].practice_log.len(), 1);
        assert_eq!(state.learning_paths.len(), 1);
        assert_eq!(state.learning_paths[0].name, "Rust Mastery");
        assert_eq!(state.next_skill_id, 2);
        assert_eq!(state.next_path_id, 2);
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({"action": "nonexistent"}))
            .await
            .unwrap();
        assert!(result.content.contains("Unknown action"));
        assert!(result.content.contains("nonexistent"));
    }
}
