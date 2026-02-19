//! Career intelligence tool — goal tracking, achievements, portfolio, networking, and strategy.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

use crate::registry::Tool;

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum GoalStatus {
    Active,
    Completed,
    Paused,
    Abandoned,
}

impl std::fmt::Display for GoalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GoalStatus::Active => write!(f, "Active"),
            GoalStatus::Completed => write!(f, "Completed"),
            GoalStatus::Paused => write!(f, "Paused"),
            GoalStatus::Abandoned => write!(f, "Abandoned"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoalMilestone {
    name: String,
    completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CareerGoal {
    id: usize,
    title: String,
    description: String,
    target_date: Option<String>,
    status: GoalStatus,
    milestones: Vec<GoalMilestone>,
    progress_pct: u32,
    created_at: DateTime<Utc>,
}

impl CareerGoal {
    fn compute_progress(milestones: &[GoalMilestone]) -> u32 {
        if milestones.is_empty() {
            return 0;
        }
        let completed = milestones.iter().filter(|m| m.completed).count();
        ((completed as f64 / milestones.len() as f64) * 100.0).round() as u32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum AchievementCategory {
    Technical,
    Leadership,
    Publication,
    Talk,
    Certification,
}

impl std::fmt::Display for AchievementCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AchievementCategory::Technical => write!(f, "Technical"),
            AchievementCategory::Leadership => write!(f, "Leadership"),
            AchievementCategory::Publication => write!(f, "Publication"),
            AchievementCategory::Talk => write!(f, "Talk"),
            AchievementCategory::Certification => write!(f, "Certification"),
        }
    }
}

fn parse_achievement_category(s: &str) -> Option<AchievementCategory> {
    match s.to_lowercase().as_str() {
        "technical" => Some(AchievementCategory::Technical),
        "leadership" => Some(AchievementCategory::Leadership),
        "publication" => Some(AchievementCategory::Publication),
        "talk" => Some(AchievementCategory::Talk),
        "certification" => Some(AchievementCategory::Certification),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Achievement {
    id: usize,
    title: String,
    description: String,
    date: String,
    category: AchievementCategory,
    impact: String,
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum PortfolioType {
    Project,
    Paper,
    Talk,
    Blog,
    Certification,
    OpenSource,
}

impl std::fmt::Display for PortfolioType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortfolioType::Project => write!(f, "Project"),
            PortfolioType::Paper => write!(f, "Paper"),
            PortfolioType::Talk => write!(f, "Talk"),
            PortfolioType::Blog => write!(f, "Blog"),
            PortfolioType::Certification => write!(f, "Certification"),
            PortfolioType::OpenSource => write!(f, "OpenSource"),
        }
    }
}

fn parse_portfolio_type(s: &str) -> Option<PortfolioType> {
    match s.to_lowercase().as_str() {
        "project" => Some(PortfolioType::Project),
        "paper" => Some(PortfolioType::Paper),
        "talk" => Some(PortfolioType::Talk),
        "blog" => Some(PortfolioType::Blog),
        "certification" => Some(PortfolioType::Certification),
        "open_source" | "opensource" => Some(PortfolioType::OpenSource),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortfolioItem {
    id: usize,
    title: String,
    item_type: PortfolioType,
    url: String,
    description: String,
    skills_demonstrated: Vec<String>,
    date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkNote {
    id: usize,
    person_name: String,
    context: String,
    notes: String,
    date: String,
    follow_up: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CareerState {
    goals: Vec<CareerGoal>,
    achievements: Vec<Achievement>,
    portfolio: Vec<PortfolioItem>,
    network_notes: Vec<NetworkNote>,
    next_id: usize,
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

pub struct CareerIntelTool {
    workspace: PathBuf,
}

impl CareerIntelTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("career")
            .join("intel.json")
    }

    fn load_state(&self) -> CareerState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            CareerState {
                goals: Vec::new(),
                achievements: Vec::new(),
                portfolio: Vec::new(),
                network_notes: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn save_state(&self, state: &CareerState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "career_intel".to_string(),
                message: format!("Failed to create state dir: {e}"),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "career_intel".to_string(),
            message: format!("Failed to serialize state: {e}"),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "career_intel".to_string(),
            message: format!("Failed to write state: {e}"),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "career_intel".to_string(),
            message: format!("Failed to rename state file: {e}"),
        })?;
        Ok(())
    }

    fn today_str() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    /// Try to load skill tracker data for gap analysis enrichment.
    fn load_skills_data(&self) -> Option<String> {
        let path = self
            .workspace
            .join(".rustant")
            .join("skills")
            .join("tracker.json");
        if path.exists() {
            std::fs::read_to_string(&path).ok()
        } else {
            None
        }
    }

    // ------------------------------------------------------------------
    // Actions
    // ------------------------------------------------------------------

    fn set_goal(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if title.is_empty() {
            return Ok(ToolOutput::text("Provide a title for the goal."));
        }
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let target_date = args
            .get("target_date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let milestones: Vec<GoalMilestone> = args
            .get("milestones")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let name = m.get("name").and_then(|n| n.as_str())?;
                        let completed = m
                            .get("completed")
                            .and_then(|c| c.as_bool())
                            .unwrap_or(false);
                        Some(GoalMilestone {
                            name: name.to_string(),
                            completed,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let progress_pct = CareerGoal::compute_progress(&milestones);

        let mut state = self.load_state();
        let id = state.next_id;
        state.next_id += 1;

        state.goals.push(CareerGoal {
            id,
            title: title.to_string(),
            description,
            target_date,
            status: GoalStatus::Active,
            milestones,
            progress_pct,
            created_at: Utc::now(),
        });

        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Goal #{} created: '{}' ({}% complete, {} milestones)",
            id,
            title,
            progress_pct,
            state.goals.last().map(|g| g.milestones.len()).unwrap_or(0)
        )))
    }

    fn log_achievement(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if title.is_empty() {
            return Ok(ToolOutput::text("Provide a title for the achievement."));
        }
        let category_str = args.get("category").and_then(|v| v.as_str()).unwrap_or("");
        let category = match parse_achievement_category(category_str) {
            Some(c) => c,
            None => {
                return Ok(ToolOutput::text(format!(
                    "Invalid category: '{category_str}'. Use: technical, leadership, publication, talk, certification."
                )));
            }
        };
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let impact = args
            .get("impact")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let date = args
            .get("date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(Self::today_str);
        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let mut state = self.load_state();
        let id = state.next_id;
        state.next_id += 1;

        state.achievements.push(Achievement {
            id,
            title: title.to_string(),
            description,
            date,
            category,
            impact,
            tags,
        });

        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Achievement #{id} logged: '{title}'."
        )))
    }

    fn add_portfolio(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if title.is_empty() {
            return Ok(ToolOutput::text("Provide a title for the portfolio item."));
        }
        let type_str = args.get("item_type").and_then(|v| v.as_str()).unwrap_or("");
        let item_type = match parse_portfolio_type(type_str) {
            Some(t) => t,
            None => {
                return Ok(ToolOutput::text(format!(
                    "Invalid item_type: '{type_str}'. Use: project, paper, talk, blog, certification, open_source."
                )));
            }
        };
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let date = args
            .get("date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(Self::today_str);
        let skills_demonstrated: Vec<String> = args
            .get("skills_demonstrated")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(|x| x.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let mut state = self.load_state();
        let id = state.next_id;
        state.next_id += 1;

        state.portfolio.push(PortfolioItem {
            id,
            title: title.to_string(),
            item_type,
            url,
            description,
            skills_demonstrated,
            date,
        });

        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Portfolio item #{id} added: '{title}'."
        )))
    }

    fn network_note(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let person_name = args
            .get("person_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if person_name.is_empty() {
            return Ok(ToolOutput::text(
                "Provide person_name for the network note.",
            ));
        }
        let context = args.get("context").and_then(|v| v.as_str()).unwrap_or("");
        if context.is_empty() {
            return Ok(ToolOutput::text("Provide context for the network note."));
        }
        let notes = args
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let date = args
            .get("date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(Self::today_str);
        let follow_up = args
            .get("follow_up")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut state = self.load_state();
        let id = state.next_id;
        state.next_id += 1;

        state.network_notes.push(NetworkNote {
            id,
            person_name: person_name.to_string(),
            context: context.to_string(),
            notes,
            date,
            follow_up,
        });

        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Network note #{id} recorded for '{person_name}'."
        )))
    }

    fn gap_analysis(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();

        let mut prompt = String::from("=== Career Gap Analysis Data ===\n\n");

        // Goals
        prompt.push_str("## Active Goals\n");
        let active: Vec<&CareerGoal> = state
            .goals
            .iter()
            .filter(|g| g.status == GoalStatus::Active)
            .collect();
        if active.is_empty() {
            prompt.push_str("  (none)\n");
        } else {
            for g in &active {
                prompt.push_str(&format!(
                    "- [{}%] {} — {}\n",
                    g.progress_pct, g.title, g.description
                ));
                if let Some(ref td) = g.target_date {
                    prompt.push_str(&format!("  Target: {td}\n"));
                }
                for m in &g.milestones {
                    let check = if m.completed { "x" } else { " " };
                    prompt.push_str(&format!("  [{}] {}\n", check, m.name));
                }
            }
        }

        // Achievements
        prompt.push_str("\n## Achievements\n");
        if state.achievements.is_empty() {
            prompt.push_str("  (none)\n");
        } else {
            for a in &state.achievements {
                prompt.push_str(&format!(
                    "- [{}] {} ({}) — {}\n",
                    a.date, a.title, a.category, a.description
                ));
                if !a.impact.is_empty() {
                    prompt.push_str(&format!("  Impact: {}\n", a.impact));
                }
            }
        }

        // Portfolio
        prompt.push_str("\n## Portfolio\n");
        if state.portfolio.is_empty() {
            prompt.push_str("  (none)\n");
        } else {
            for p in &state.portfolio {
                prompt.push_str(&format!(
                    "- [{}] {} ({}) — {}\n",
                    p.date, p.title, p.item_type, p.description
                ));
                if !p.skills_demonstrated.is_empty() {
                    prompt.push_str(&format!("  Skills: {}\n", p.skills_demonstrated.join(", ")));
                }
            }
        }

        // Skills data (optional)
        if let Some(skills_json) = self.load_skills_data() {
            prompt.push_str("\n## Skill Tracker Data\n");
            prompt.push_str(&skills_json);
            prompt.push('\n');
        }

        prompt.push_str("\n## Instructions\n");
        prompt.push_str(
            "Analyze the above career data and identify:\n\
             1. Skill gaps relative to stated goals\n\
             2. Missing portfolio evidence for claimed skills\n\
             3. Achievements that could be better leveraged\n\
             4. Milestones at risk of falling behind\n\
             5. Recommended next actions to close gaps\n",
        );

        Ok(ToolOutput::text(prompt))
    }

    fn market_scan(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let role = args
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("software engineer");
        let industry = args
            .get("industry")
            .and_then(|v| v.as_str())
            .unwrap_or("technology");

        let state = self.load_state();

        let mut prompt = String::from("=== Market Scan Context ===\n\n");
        prompt.push_str(&format!("Target Role: {role}\n"));
        prompt.push_str(&format!("Industry: {industry}\n\n"));

        // Include current skills from portfolio and achievements for context
        let mut skills: Vec<String> = Vec::new();
        for p in &state.portfolio {
            for s in &p.skills_demonstrated {
                if !skills.contains(s) {
                    skills.push(s.clone());
                }
            }
        }
        for a in &state.achievements {
            for t in &a.tags {
                if !skills.contains(t) {
                    skills.push(t.clone());
                }
            }
        }
        if !skills.is_empty() {
            prompt.push_str(&format!("Current Skills: {}\n\n", skills.join(", ")));
        }

        prompt.push_str("## Instructions\n");
        prompt.push_str(&format!(
            "Using web_search, research the current market for '{role}' roles in the '{industry}' industry:\n\
             1. In-demand skills and technologies\n\
             2. Salary ranges and compensation trends\n\
             3. Emerging roles and specializations\n\
             4. Key companies hiring\n\
             5. How the user's current skills align with market demand\n\
             6. Recommended skills to develop\n"
        ));

        Ok(ToolOutput::text(prompt))
    }

    fn progress_report(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let goal_id = args
            .get("goal_id")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        if let Some(gid) = goal_id {
            // Single goal report
            if let Some(goal) = state.goals.iter().find(|g| g.id == gid) {
                let mut report = format!(
                    "Goal #{}: {}\n  Status: {}\n  Progress: {}%\n  Description: {}\n",
                    goal.id, goal.title, goal.status, goal.progress_pct, goal.description
                );
                if let Some(ref td) = goal.target_date {
                    report.push_str(&format!("  Target date: {td}\n"));
                }
                if !goal.milestones.is_empty() {
                    report.push_str("  Milestones:\n");
                    for m in &goal.milestones {
                        let check = if m.completed { "x" } else { " " };
                        report.push_str(&format!("    [{}] {}\n", check, m.name));
                    }
                }
                Ok(ToolOutput::text(report))
            } else {
                Ok(ToolOutput::text(format!("Goal #{gid} not found.")))
            }
        } else {
            // Full progress report
            let mut report = String::from("=== Career Progress Report ===\n\n");

            // Goals overview
            report.push_str("## Goals\n");
            if state.goals.is_empty() {
                report.push_str("  No goals set.\n");
            } else {
                for g in &state.goals {
                    report.push_str(&format!(
                        "  #{} [{}] {} — {}%\n",
                        g.id, g.status, g.title, g.progress_pct
                    ));
                }
            }

            // Achievement stats
            let achievement_count = state.achievements.len();
            report.push_str(&format!("\n## Achievements: {achievement_count} total\n"));
            if !state.achievements.is_empty() {
                // Count by category
                let mut by_cat: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for a in &state.achievements {
                    *by_cat.entry(a.category.to_string()).or_insert(0) += 1;
                }
                for (cat, count) in &by_cat {
                    report.push_str(&format!("  {cat}: {count}\n"));
                }
            }

            // Portfolio stats
            let portfolio_count = state.portfolio.len();
            report.push_str(&format!("\n## Portfolio: {portfolio_count} items\n"));
            if !state.portfolio.is_empty() {
                let mut by_type: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for p in &state.portfolio {
                    *by_type.entry(p.item_type.to_string()).or_insert(0) += 1;
                }
                for (ptype, count) in &by_type {
                    report.push_str(&format!("  {ptype}: {count}\n"));
                }
            }

            // Network notes
            report.push_str(&format!(
                "\n## Network: {} contacts noted\n",
                state.network_notes.len()
            ));

            Ok(ToolOutput::text(report))
        }
    }

    fn strategy_review(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();

        let mut prompt = String::from("=== Comprehensive Career Strategy Review ===\n\n");

        // Goals
        prompt.push_str("## Goals\n");
        if state.goals.is_empty() {
            prompt.push_str("  (none)\n");
        } else {
            for g in &state.goals {
                prompt.push_str(&format!(
                    "- #{} [{}] {} — {}% complete\n  {}\n",
                    g.id, g.status, g.title, g.progress_pct, g.description
                ));
                if let Some(ref td) = g.target_date {
                    prompt.push_str(&format!("  Target: {td}\n"));
                }
                for m in &g.milestones {
                    let check = if m.completed { "x" } else { " " };
                    prompt.push_str(&format!("  [{}] {}\n", check, m.name));
                }
            }
        }

        // Achievements
        prompt.push_str("\n## Achievements\n");
        if state.achievements.is_empty() {
            prompt.push_str("  (none)\n");
        } else {
            for a in &state.achievements {
                prompt.push_str(&format!(
                    "- [{}] {} ({}) — {}\n",
                    a.date, a.title, a.category, a.description
                ));
                if !a.impact.is_empty() {
                    prompt.push_str(&format!("  Impact: {}\n", a.impact));
                }
                if !a.tags.is_empty() {
                    prompt.push_str(&format!("  Tags: {}\n", a.tags.join(", ")));
                }
            }
        }

        // Portfolio
        prompt.push_str("\n## Portfolio\n");
        if state.portfolio.is_empty() {
            prompt.push_str("  (none)\n");
        } else {
            for p in &state.portfolio {
                prompt.push_str(&format!(
                    "- [{}] {} ({}) — {}\n",
                    p.date, p.title, p.item_type, p.description
                ));
                if !p.url.is_empty() {
                    prompt.push_str(&format!("  URL: {}\n", p.url));
                }
                if !p.skills_demonstrated.is_empty() {
                    prompt.push_str(&format!("  Skills: {}\n", p.skills_demonstrated.join(", ")));
                }
            }
        }

        // Network
        prompt.push_str("\n## Network Notes\n");
        if state.network_notes.is_empty() {
            prompt.push_str("  (none)\n");
        } else {
            for n in &state.network_notes {
                prompt.push_str(&format!(
                    "- [{}] {} — {} | {}\n",
                    n.date, n.person_name, n.context, n.notes
                ));
                if let Some(ref fu) = n.follow_up {
                    prompt.push_str(&format!("  Follow-up: {fu}\n"));
                }
            }
        }

        // Skills data (optional)
        if let Some(skills_json) = self.load_skills_data() {
            prompt.push_str("\n## Skill Tracker Data\n");
            prompt.push_str(&skills_json);
            prompt.push('\n');
        }

        prompt.push_str("\n## Instructions\n");
        prompt.push_str(
            "Perform a comprehensive career strategy assessment:\n\
             1. Overall trajectory analysis — are goals, achievements, and portfolio aligned?\n\
             2. Strengths to double down on\n\
             3. Weaknesses and blind spots\n\
             4. Network leverage opportunities\n\
             5. Timeline feasibility for goals with target dates\n\
             6. Prioritized action plan for the next 30/60/90 days\n\
             7. Long-term strategic recommendations\n",
        );

        Ok(ToolOutput::text(prompt))
    }
}

// ---------------------------------------------------------------------------
// Tool trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Tool for CareerIntelTool {
    fn name(&self) -> &str {
        "career_intel"
    }

    fn description(&self) -> &str {
        "Career strategy intelligence: goal tracking, achievements, portfolio, networking. Actions: set_goal, log_achievement, add_portfolio, gap_analysis, market_scan, network_note, progress_report, strategy_review."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["set_goal", "log_achievement", "add_portfolio", "gap_analysis", "market_scan", "network_note", "progress_report", "strategy_review"],
                    "description": "Action to perform"
                },
                "title": { "type": "string", "description": "Title (for goal, achievement, or portfolio item)" },
                "description": { "type": "string", "description": "Description text" },
                "target_date": { "type": "string", "description": "Target date YYYY-MM-DD (for goals)" },
                "milestones": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "completed": { "type": "boolean" }
                        },
                        "required": ["name"]
                    },
                    "description": "Goal milestones"
                },
                "category": {
                    "type": "string",
                    "enum": ["technical", "leadership", "publication", "talk", "certification"],
                    "description": "Achievement category"
                },
                "impact": { "type": "string", "description": "Impact description (for achievements)" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags (for achievements)"
                },
                "item_type": {
                    "type": "string",
                    "enum": ["project", "paper", "talk", "blog", "certification", "open_source"],
                    "description": "Portfolio item type"
                },
                "url": { "type": "string", "description": "URL (for portfolio items)" },
                "skills_demonstrated": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Skills demonstrated (for portfolio items)"
                },
                "date": { "type": "string", "description": "Date YYYY-MM-DD (defaults to today)" },
                "person_name": { "type": "string", "description": "Person name (for network notes)" },
                "context": { "type": "string", "description": "Context of interaction (for network notes)" },
                "notes": { "type": "string", "description": "Additional notes" },
                "follow_up": { "type": "string", "description": "Follow-up action (for network notes)" },
                "role": { "type": "string", "description": "Target role (for market scan)" },
                "industry": { "type": "string", "description": "Target industry (for market scan)" },
                "goal_id": { "type": "integer", "description": "Goal ID (for progress report)" }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "set_goal" => self.set_goal(&args),
            "log_achievement" => self.log_achievement(&args),
            "add_portfolio" => self.add_portfolio(&args),
            "gap_analysis" => self.gap_analysis(),
            "market_scan" => self.market_scan(&args),
            "network_note" => self.network_note(&args),
            "progress_report" => self.progress_report(&args),
            "strategy_review" => self.strategy_review(),
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{action}'. Use: set_goal, log_achievement, add_portfolio, gap_analysis, market_scan, network_note, progress_report, strategy_review."
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (TempDir, CareerIntelTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = CareerIntelTool::new(workspace);
        (dir, tool)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "career_intel");
        assert!(tool.description().contains("goal tracking"));
        assert!(tool.description().contains("achievements"));
        assert!(tool.description().contains("portfolio"));
        assert!(tool.description().contains("networking"));
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert_eq!(tool.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, tool) = make_tool();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert_eq!(actions.len(), 8);
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "action");
    }

    #[tokio::test]
    async fn test_set_goal_with_milestones() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "set_goal",
                "title": "Become Staff Engineer",
                "description": "Get promoted to staff level",
                "target_date": "2026-12-31",
                "milestones": [
                    { "name": "Lead a cross-team project", "completed": true },
                    { "name": "Publish tech blog", "completed": false },
                    { "name": "Mentor two juniors", "completed": true },
                    { "name": "Design system architecture", "completed": false }
                ]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Goal #1"));
        assert!(result.content.contains("Become Staff Engineer"));
        assert!(result.content.contains("50%"));
        assert!(result.content.contains("4 milestones"));

        // Verify state was persisted
        let state = tool.load_state();
        assert_eq!(state.goals.len(), 1);
        assert_eq!(state.goals[0].progress_pct, 50);
        assert_eq!(state.goals[0].status, GoalStatus::Active);
        assert_eq!(state.goals[0].milestones.len(), 4);
        assert_eq!(state.goals[0].target_date.as_deref(), Some("2026-12-31"));
    }

    #[tokio::test]
    async fn test_log_achievement() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "log_achievement",
                "title": "Shipped v2.0 platform",
                "category": "technical",
                "description": "Led the complete rewrite of the platform",
                "impact": "Reduced latency by 40%",
                "tags": ["rust", "performance"],
                "date": "2026-01-15"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Achievement #1"));
        assert!(result.content.contains("Shipped v2.0 platform"));

        // Verify in progress report
        let report = tool
            .execute(json!({ "action": "progress_report" }))
            .await
            .unwrap();
        assert!(report.content.contains("1 total"));
        assert!(report.content.contains("Technical"));
    }

    #[tokio::test]
    async fn test_add_portfolio() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "add_portfolio",
                "title": "rustant",
                "item_type": "open_source",
                "url": "https://github.com/example/rustant",
                "description": "Privacy-first autonomous agent",
                "skills_demonstrated": ["rust", "async", "llm"],
                "date": "2026-02-01"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Portfolio item #1"));
        assert!(result.content.contains("rustant"));

        // Verify in progress report
        let report = tool
            .execute(json!({ "action": "progress_report" }))
            .await
            .unwrap();
        assert!(report.content.contains("1 items"));
        assert!(report.content.contains("OpenSource"));
    }

    #[tokio::test]
    async fn test_network_note() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "network_note",
                "person_name": "Alice Chen",
                "context": "RustConf 2026",
                "notes": "Works on compiler team at Mozilla",
                "follow_up": "Send slides from my talk"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Network note #1"));
        assert!(result.content.contains("Alice Chen"));

        // Verify persisted
        let state = tool.load_state();
        assert_eq!(state.network_notes.len(), 1);
        assert_eq!(state.network_notes[0].person_name, "Alice Chen");
        assert_eq!(state.network_notes[0].context, "RustConf 2026");
        assert_eq!(
            state.network_notes[0].follow_up.as_deref(),
            Some("Send slides from my talk")
        );
    }

    #[tokio::test]
    async fn test_gap_analysis_returns_prompt() {
        let (_dir, tool) = make_tool();

        // Add some data first
        tool.execute(json!({
            "action": "set_goal",
            "title": "Learn Kubernetes",
            "milestones": [{ "name": "Complete CKA course" }]
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "log_achievement",
            "title": "AWS Certified",
            "category": "certification"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({ "action": "gap_analysis" }))
            .await
            .unwrap();
        assert!(result.content.contains("Career Gap Analysis Data"));
        assert!(result.content.contains("Active Goals"));
        assert!(result.content.contains("Learn Kubernetes"));
        assert!(result.content.contains("AWS Certified"));
        assert!(result.content.contains("Instructions"));
        assert!(result.content.contains("Skill gaps"));
    }

    #[tokio::test]
    async fn test_strategy_review_returns_prompt() {
        let (_dir, tool) = make_tool();

        // Add some data
        tool.execute(json!({
            "action": "set_goal",
            "title": "Senior role",
            "description": "Reach senior level"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "network_note",
            "person_name": "Bob",
            "context": "Meetup"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({ "action": "strategy_review" }))
            .await
            .unwrap();
        assert!(
            result
                .content
                .contains("Comprehensive Career Strategy Review")
        );
        assert!(result.content.contains("Senior role"));
        assert!(result.content.contains("Bob"));
        assert!(result.content.contains("Instructions"));
        assert!(result.content.contains("trajectory analysis"));
        assert!(result.content.contains("30/60/90 days"));
    }

    #[tokio::test]
    async fn test_progress_report_specific_goal() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "set_goal",
            "title": "Goal A",
            "description": "First goal",
            "milestones": [
                { "name": "Step 1", "completed": true },
                { "name": "Step 2", "completed": false }
            ]
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "set_goal",
            "title": "Goal B",
            "description": "Second goal"
        }))
        .await
        .unwrap();

        // Request specific goal
        let result = tool
            .execute(json!({ "action": "progress_report", "goal_id": 1 }))
            .await
            .unwrap();
        assert!(result.content.contains("Goal #1"));
        assert!(result.content.contains("Goal A"));
        assert!(result.content.contains("50%"));
        assert!(result.content.contains("[x] Step 1"));
        assert!(result.content.contains("[ ] Step 2"));
        // Should NOT contain Goal B
        assert!(!result.content.contains("Goal B"));

        // Non-existent goal
        let result = tool
            .execute(json!({ "action": "progress_report", "goal_id": 99 }))
            .await
            .unwrap();
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();

        // Add one of each type
        tool.execute(json!({
            "action": "set_goal",
            "title": "Goal 1",
            "milestones": [{ "name": "M1", "completed": true }]
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "log_achievement",
            "title": "Ach 1",
            "category": "technical"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "add_portfolio",
            "title": "Port 1",
            "item_type": "project"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "network_note",
            "person_name": "Eve",
            "context": "Conference"
        }))
        .await
        .unwrap();

        // Reload from disk
        let state = tool.load_state();
        assert_eq!(state.goals.len(), 1);
        assert_eq!(state.achievements.len(), 1);
        assert_eq!(state.portfolio.len(), 1);
        assert_eq!(state.network_notes.len(), 1);
        assert_eq!(state.next_id, 5);

        // Verify data integrity
        assert_eq!(state.goals[0].title, "Goal 1");
        assert_eq!(state.goals[0].progress_pct, 100);
        assert_eq!(state.achievements[0].title, "Ach 1");
        assert_eq!(
            state.achievements[0].category,
            AchievementCategory::Technical
        );
        assert_eq!(state.portfolio[0].title, "Port 1");
        assert_eq!(state.portfolio[0].item_type, PortfolioType::Project);
        assert_eq!(state.network_notes[0].person_name, "Eve");
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let (_dir, tool) = make_tool();
        let result = tool.execute(json!({ "action": "bogus" })).await.unwrap();
        assert!(result.content.contains("Unknown action"));
        assert!(result.content.contains("bogus"));
    }
}
