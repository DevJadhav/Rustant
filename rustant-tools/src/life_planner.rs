//! Life planner tool — energy-aware scheduling, deadlines, habits, context switching.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::registry::Tool;

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnergyProfile {
    peak_hours: Vec<u8>,
    low_energy_hours: Vec<u8>,
    preferred_break_interval_mins: u32,
}

impl Default for EnergyProfile {
    fn default() -> Self {
        Self {
            peak_hours: vec![9, 10, 11],
            low_energy_hours: vec![14, 15],
            preferred_break_interval_mins: 90,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum DeadlinePriority {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for DeadlinePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum DeadlineStatus {
    Pending,
    InProgress,
    Completed,
    Overdue,
}

impl std::fmt::Display for DeadlineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::InProgress => write!(f, "InProgress"),
            Self::Completed => write!(f, "Completed"),
            Self::Overdue => write!(f, "Overdue"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Deadline {
    id: usize,
    title: String,
    due_date: String,
    priority: DeadlinePriority,
    estimated_hours: f64,
    category: String,
    status: DeadlineStatus,
    notes: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum HabitFrequency {
    Daily,
    Weekly,
}

impl std::fmt::Display for HabitFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daily => write!(f, "Daily"),
            Self::Weekly => write!(f, "Weekly"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HabitEntry {
    id: usize,
    name: String,
    frequency: HabitFrequency,
    streak: u32,
    best_streak: u32,
    last_completed: Option<String>,
    target_streak: u32,
    history: Vec<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContextSwitchEntry {
    from_task: String,
    to_task: String,
    timestamp: DateTime<Utc>,
    reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlannerState {
    energy_profile: EnergyProfile,
    deadlines: Vec<Deadline>,
    habits: Vec<HabitEntry>,
    context_switches: Vec<ContextSwitchEntry>,
    next_deadline_id: usize,
    next_habit_id: usize,
}

impl Default for PlannerState {
    fn default() -> Self {
        Self {
            energy_profile: EnergyProfile::default(),
            deadlines: Vec::new(),
            habits: Vec::new(),
            context_switches: Vec::new(),
            next_deadline_id: 1,
            next_habit_id: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Tool implementation
// ---------------------------------------------------------------------------

pub struct LifePlannerTool {
    workspace: PathBuf,
}

impl LifePlannerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("life")
            .join("planner.json")
    }

    fn load_state(&self) -> PlannerState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            PlannerState::default()
        }
    }

    fn save_state(&self, state: &PlannerState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "life_planner".to_string(),
                message: format!("Failed to create state dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "life_planner".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "life_planner".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "life_planner".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }

    fn today_str() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    fn parse_priority(s: &str) -> DeadlinePriority {
        match s.to_lowercase().as_str() {
            "low" => DeadlinePriority::Low,
            "high" => DeadlinePriority::High,
            "critical" => DeadlinePriority::Critical,
            _ => DeadlinePriority::Medium,
        }
    }

    fn priority_sort_key(p: &DeadlinePriority) -> u8 {
        match p {
            DeadlinePriority::Critical => 0,
            DeadlinePriority::High => 1,
            DeadlinePriority::Medium => 2,
            DeadlinePriority::Low => 3,
        }
    }

    // -----------------------------------------------------------------------
    // Actions
    // -----------------------------------------------------------------------

    fn action_set_energy_profile(
        &self,
        args: &Value,
        state: &mut PlannerState,
    ) -> Result<ToolOutput, ToolError> {
        let peak_hours: Vec<u8> = args
            .get("peak_hours")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect()
            })
            .unwrap_or_default();

        let low_energy_hours: Vec<u8> = args
            .get("low_energy_hours")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect()
            })
            .unwrap_or_default();

        if peak_hours.is_empty() || low_energy_hours.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "life_planner".to_string(),
                reason: "peak_hours and low_energy_hours are required and must be non-empty"
                    .to_string(),
            });
        }

        // Validate all hours 0-23
        for &h in peak_hours.iter().chain(low_energy_hours.iter()) {
            if h > 23 {
                return Err(ToolError::InvalidArguments {
                    name: "life_planner".to_string(),
                    reason: format!("Invalid hour {}: must be 0-23", h),
                });
            }
        }

        // Check no overlap
        for &h in &peak_hours {
            if low_energy_hours.contains(&h) {
                return Err(ToolError::InvalidArguments {
                    name: "life_planner".to_string(),
                    reason: format!("Hour {} appears in both peak_hours and low_energy_hours", h),
                });
            }
        }

        let break_interval = args
            .get("preferred_break_interval_mins")
            .and_then(|v| v.as_u64())
            .unwrap_or(90) as u32;

        state.energy_profile = EnergyProfile {
            peak_hours: peak_hours.clone(),
            low_energy_hours: low_energy_hours.clone(),
            preferred_break_interval_mins: break_interval,
        };
        self.save_state(state)?;

        Ok(ToolOutput::text(format!(
            "Energy profile updated.\n  Peak hours: {:?}\n  Low energy hours: {:?}\n  Break interval: {} mins",
            peak_hours, low_energy_hours, break_interval
        )))
    }

    fn action_add_deadline(
        &self,
        args: &Value,
        state: &mut PlannerState,
    ) -> Result<ToolOutput, ToolError> {
        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "life_planner".to_string(),
                reason: "title is required".to_string(),
            });
        }

        let due_date = args
            .get("due_date")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if due_date.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "life_planner".to_string(),
                reason: "due_date is required (YYYY-MM-DD)".to_string(),
            });
        }

        // Validate date format
        let parsed_date = NaiveDate::parse_from_str(&due_date, "%Y-%m-%d").map_err(|_| {
            ToolError::InvalidArguments {
                name: "life_planner".to_string(),
                reason: format!("Invalid due_date '{}': expected YYYY-MM-DD", due_date),
            }
        })?;

        let priority = Self::parse_priority(
            args.get("priority")
                .and_then(|v| v.as_str())
                .unwrap_or("medium"),
        );
        let estimated_hours = args
            .get("estimated_hours")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("general")
            .to_string();
        let notes = args
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Determine status: if due_date is in the past, set to Overdue
        let today = Utc::now().date_naive();
        let status = if parsed_date < today {
            DeadlineStatus::Overdue
        } else {
            DeadlineStatus::Pending
        };

        let id = state.next_deadline_id;
        state.next_deadline_id += 1;

        state.deadlines.push(Deadline {
            id,
            title: title.clone(),
            due_date: due_date.clone(),
            priority: priority.clone(),
            estimated_hours,
            category,
            status: status.clone(),
            notes,
            created_at: Utc::now(),
        });
        self.save_state(state)?;

        Ok(ToolOutput::text(format!(
            "Deadline #{} added: '{}' due {} [{}] ({})",
            id, title, due_date, priority, status
        )))
    }

    fn action_log_habit(
        &self,
        args: &Value,
        state: &mut PlannerState,
    ) -> Result<ToolOutput, ToolError> {
        let habit_id = args
            .get("habit_id")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let name = args.get("name").and_then(|v| v.as_str());
        let today = Self::today_str();

        if let Some(hid) = habit_id {
            // Mark existing habit as completed today
            let habit = state
                .habits
                .iter_mut()
                .find(|h| h.id == hid)
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "life_planner".to_string(),
                    reason: format!("Habit #{} not found", hid),
                })?;

            // Check if already completed today
            if habit.history.last().map(|d| d.as_str()) == Some(today.as_str()) {
                return Ok(ToolOutput::text(format!(
                    "Habit '{}' already completed today. Streak: {}",
                    habit.name, habit.streak
                )));
            }

            // Determine if streak continues or resets
            let streak_continues = if let Some(ref last) = habit.last_completed {
                match habit.frequency {
                    HabitFrequency::Daily => {
                        // Last completed must be yesterday
                        if let Ok(last_date) = NaiveDate::parse_from_str(last, "%Y-%m-%d") {
                            if let Ok(today_date) = NaiveDate::parse_from_str(&today, "%Y-%m-%d") {
                                let diff = today_date.signed_duration_since(last_date).num_days();
                                diff == 1
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                    HabitFrequency::Weekly => {
                        // Last completed must be within 7 days
                        if let Ok(last_date) = NaiveDate::parse_from_str(last, "%Y-%m-%d") {
                            if let Ok(today_date) = NaiveDate::parse_from_str(&today, "%Y-%m-%d") {
                                let diff = today_date.signed_duration_since(last_date).num_days();
                                (1..=7).contains(&diff)
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                }
            } else {
                false
            };

            if streak_continues {
                habit.streak += 1;
            } else {
                habit.streak = 1;
            }

            if habit.streak > habit.best_streak {
                habit.best_streak = habit.streak;
            }

            habit.last_completed = Some(today.clone());
            habit.history.push(today);

            let msg = format!(
                "Habit '{}' completed! Streak: {} (best: {}, target: {})",
                habit.name, habit.streak, habit.best_streak, habit.target_streak
            );
            self.save_state(state)?;
            Ok(ToolOutput::text(msg))
        } else if let Some(habit_name) = name {
            // Create new habit
            let frequency_str = args
                .get("frequency")
                .and_then(|v| v.as_str())
                .unwrap_or("daily");
            let frequency = match frequency_str.to_lowercase().as_str() {
                "weekly" => HabitFrequency::Weekly,
                _ => HabitFrequency::Daily,
            };
            let target_streak = args
                .get("target_streak")
                .and_then(|v| v.as_u64())
                .unwrap_or(30) as u32;

            let id = state.next_habit_id;
            state.next_habit_id += 1;

            state.habits.push(HabitEntry {
                id,
                name: habit_name.to_string(),
                frequency: frequency.clone(),
                streak: 1,
                best_streak: 1,
                last_completed: Some(today.clone()),
                target_streak,
                history: vec![today],
                created_at: Utc::now(),
            });
            self.save_state(state)?;

            Ok(ToolOutput::text(format!(
                "Habit #{} created: '{}' ({}, target streak: {}). Marked complete for today.",
                id, habit_name, frequency, target_streak
            )))
        } else {
            Err(ToolError::InvalidArguments {
                name: "life_planner".to_string(),
                reason: "Provide either habit_id (to log existing) or name (to create new)"
                    .to_string(),
            })
        }
    }

    fn action_daily_plan(&self, state: &PlannerState) -> Result<ToolOutput, ToolError> {
        let today = Self::today_str();
        let profile = &state.energy_profile;

        let mut output = String::from("=== Daily Plan ===\n\n");

        // Energy profile
        output.push_str(&format!(
            "Energy Profile:\n  Peak hours: {:?}\n  Low energy hours: {:?}\n  Break every {} mins\n\n",
            profile.peak_hours, profile.low_energy_hours, profile.preferred_break_interval_mins
        ));

        // Today's deadlines sorted by priority
        let mut todays_deadlines: Vec<&Deadline> = state
            .deadlines
            .iter()
            .filter(|d| d.due_date == today && d.status != DeadlineStatus::Completed)
            .collect();
        todays_deadlines.sort_by_key(|d| Self::priority_sort_key(&d.priority));

        if todays_deadlines.is_empty() {
            output.push_str("Deadlines due today: None\n\n");
        } else {
            output.push_str("Deadlines due today:\n");
            for d in &todays_deadlines {
                output.push_str(&format!(
                    "  [{}] #{} '{}' — est. {:.1}h ({})\n",
                    d.priority, d.id, d.title, d.estimated_hours, d.status
                ));
            }
            output.push('\n');
        }

        // Upcoming deadlines (next 7 days, excluding today)
        let mut upcoming: Vec<&Deadline> = state
            .deadlines
            .iter()
            .filter(|d| {
                d.status != DeadlineStatus::Completed
                    && d.due_date > today
                    && d.due_date <= upcoming_date_str(7)
            })
            .collect();
        upcoming.sort_by_key(|d| (d.due_date.clone(), Self::priority_sort_key(&d.priority)));

        if !upcoming.is_empty() {
            output.push_str("Upcoming (next 7 days):\n");
            for d in &upcoming {
                output.push_str(&format!(
                    "  {} [{}] #{} '{}'\n",
                    d.due_date, d.priority, d.id, d.title
                ));
            }
            output.push('\n');
        }

        // Overdue deadlines
        let overdue: Vec<&Deadline> = state
            .deadlines
            .iter()
            .filter(|d| d.status == DeadlineStatus::Overdue)
            .collect();
        if !overdue.is_empty() {
            output.push_str("OVERDUE:\n");
            for d in &overdue {
                output.push_str(&format!(
                    "  [{}] #{} '{}' was due {}\n",
                    d.priority, d.id, d.title, d.due_date
                ));
            }
            output.push('\n');
        }

        // Habits due today
        let daily_habits: Vec<&HabitEntry> = state
            .habits
            .iter()
            .filter(|h| {
                h.frequency == HabitFrequency::Daily
                    && h.last_completed.as_deref() != Some(today.as_str())
            })
            .collect();
        let weekly_habits: Vec<&HabitEntry> = state
            .habits
            .iter()
            .filter(|h| {
                if h.frequency != HabitFrequency::Weekly {
                    return false;
                }
                // Due if not completed within last 7 days
                match &h.last_completed {
                    Some(last) => {
                        if let (Ok(last_date), Ok(today_date)) = (
                            NaiveDate::parse_from_str(last, "%Y-%m-%d"),
                            NaiveDate::parse_from_str(&today, "%Y-%m-%d"),
                        ) {
                            today_date.signed_duration_since(last_date).num_days() >= 7
                        } else {
                            true
                        }
                    }
                    None => true,
                }
            })
            .collect();

        if !daily_habits.is_empty() || !weekly_habits.is_empty() {
            output.push_str("Habits due:\n");
            for h in &daily_habits {
                output.push_str(&format!(
                    "  [Daily] '{}' — streak: {}/{}\n",
                    h.name, h.streak, h.target_streak
                ));
            }
            for h in &weekly_habits {
                output.push_str(&format!(
                    "  [Weekly] '{}' — streak: {}/{}\n",
                    h.name, h.streak, h.target_streak
                ));
            }
            output.push('\n');
        }

        // Suggested break times based on preferred_break_interval
        output.push_str(&format!(
            "Suggested breaks: every {} minutes during work blocks.\n",
            profile.preferred_break_interval_mins
        ));
        output.push_str(
            "Tip: Use macos_calendar and macos_reminders for additional context on today's events.\n",
        );

        Ok(ToolOutput::text(output))
    }

    fn action_weekly_review(&self, state: &PlannerState) -> Result<ToolOutput, ToolError> {
        let today = Self::today_str();
        let today_date =
            NaiveDate::parse_from_str(&today, "%Y-%m-%d").unwrap_or(Utc::now().date_naive());
        let week_start = today_date - chrono::Duration::days(7);
        let week_start_str = week_start.format("%Y-%m-%d").to_string();

        let mut output = String::from("=== Weekly Review ===\n\n");

        // Deadlines this week
        let completed: Vec<&Deadline> = state
            .deadlines
            .iter()
            .filter(|d| {
                d.status == DeadlineStatus::Completed
                    && d.due_date >= week_start_str
                    && d.due_date <= today
            })
            .collect();
        let pending: Vec<&Deadline> = state
            .deadlines
            .iter()
            .filter(|d| {
                d.status == DeadlineStatus::Pending
                    && d.due_date >= week_start_str
                    && d.due_date <= today
            })
            .collect();
        let overdue: Vec<&Deadline> = state
            .deadlines
            .iter()
            .filter(|d| d.status == DeadlineStatus::Overdue)
            .collect();

        output.push_str(&format!(
            "Deadlines this week:\n  Completed: {}\n  Pending: {}\n  Overdue: {}\n\n",
            completed.len(),
            pending.len(),
            overdue.len()
        ));

        if !completed.is_empty() {
            output.push_str("  Completed:\n");
            for d in &completed {
                output.push_str(&format!("    #{} '{}'\n", d.id, d.title));
            }
        }
        if !overdue.is_empty() {
            output.push_str("  Overdue:\n");
            for d in &overdue {
                output.push_str(&format!(
                    "    #{} '{}' (due {})\n",
                    d.id, d.title, d.due_date
                ));
            }
        }
        output.push('\n');

        // Habit streaks
        output.push_str("Habit streaks:\n");
        if state.habits.is_empty() {
            output.push_str("  No habits tracked.\n");
        } else {
            for h in &state.habits {
                let pct = if h.target_streak > 0 {
                    (h.streak as f64 / h.target_streak as f64 * 100.0) as u32
                } else {
                    0
                };
                output.push_str(&format!(
                    "  '{}' ({}) — streak: {}/{} ({}%) best: {}\n",
                    h.name, h.frequency, h.streak, h.target_streak, pct, h.best_streak
                ));
            }
        }
        output.push('\n');

        // Context switches this week
        let week_switches: Vec<&ContextSwitchEntry> = state
            .context_switches
            .iter()
            .filter(|cs| {
                let cs_date = cs.timestamp.format("%Y-%m-%d").to_string();
                cs_date >= week_start_str && cs_date <= today
            })
            .collect();

        output.push_str(&format!(
            "Context switches this week: {}\n",
            week_switches.len()
        ));
        if !week_switches.is_empty() {
            // Show patterns: group by from_task
            let mut switch_counts: HashMap<&str, usize> = HashMap::new();
            for cs in &week_switches {
                *switch_counts.entry(&cs.from_task).or_insert(0) += 1;
            }
            let mut sorted: Vec<_> = switch_counts.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));
            output.push_str("  Most interrupted tasks:\n");
            for (task, count) in sorted.iter().take(5) {
                output.push_str(&format!("    '{}' — {} switches\n", task, count));
            }
        }

        Ok(ToolOutput::text(output))
    }

    fn action_context_switch_log(
        &self,
        args: &Value,
        state: &mut PlannerState,
    ) -> Result<ToolOutput, ToolError> {
        let from_task = args
            .get("from_task")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if from_task.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "life_planner".to_string(),
                reason: "from_task is required".to_string(),
            });
        }

        let to_task = args
            .get("to_task")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if to_task.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "life_planner".to_string(),
                reason: "to_task is required".to_string(),
            });
        }

        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let entry = ContextSwitchEntry {
            from_task: from_task.clone(),
            to_task: to_task.clone(),
            timestamp: Utc::now(),
            reason: reason.clone(),
        };
        state.context_switches.push(entry);

        // Keep last 500 entries to prevent unbounded growth
        if state.context_switches.len() > 500 {
            state
                .context_switches
                .drain(0..state.context_switches.len() - 500);
        }

        self.save_state(state)?;

        let reason_str = if reason.is_empty() {
            String::new()
        } else {
            format!(" (reason: {})", reason)
        };

        Ok(ToolOutput::text(format!(
            "Context switch logged: '{}' -> '{}'{}\nTotal switches today: {}",
            from_task,
            to_task,
            reason_str,
            state
                .context_switches
                .iter()
                .filter(|cs| cs.timestamp.format("%Y-%m-%d").to_string() == Self::today_str())
                .count()
        )))
    }

    fn action_balance_report(&self, state: &PlannerState) -> Result<ToolOutput, ToolError> {
        let mut output = String::from("=== Balance Report ===\n\n");

        // Group deadlines by category
        let mut by_category: HashMap<&str, (usize, f64, usize, usize)> = HashMap::new();
        for d in &state.deadlines {
            let entry = by_category.entry(&d.category).or_insert((0, 0.0, 0, 0));
            entry.0 += 1; // total
            entry.1 += d.estimated_hours; // hours
            if d.status == DeadlineStatus::Completed {
                entry.2 += 1; // completed
            }
            if d.status == DeadlineStatus::Overdue {
                entry.3 += 1; // overdue
            }
        }

        output.push_str("Time allocation by category:\n");
        if by_category.is_empty() {
            output.push_str("  No deadlines tracked.\n");
        } else {
            let total_hours: f64 = by_category.values().map(|v| v.1).sum();
            let mut sorted: Vec<_> = by_category.iter().collect();
            sorted.sort_by(|a, b| {
                b.1 .1
                    .partial_cmp(&a.1 .1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for (cat, (total, hours, completed, overdue)) in &sorted {
                let pct = if total_hours > 0.0 {
                    (*hours / total_hours * 100.0) as u32
                } else {
                    0
                };
                output.push_str(&format!(
                    "  {}: {:.1}h ({}%) — {} total, {} done, {} overdue\n",
                    cat, hours, pct, total, completed, overdue
                ));
            }
        }
        output.push('\n');

        // Habit health
        output.push_str("Habit health:\n");
        if state.habits.is_empty() {
            output.push_str("  No habits tracked.\n");
        } else {
            for h in &state.habits {
                let pct = if h.target_streak > 0 {
                    (h.streak as f64 / h.target_streak as f64 * 100.0).min(100.0) as u32
                } else {
                    0
                };
                let health = if pct >= 80 {
                    "GREAT"
                } else if pct >= 50 {
                    "GOOD"
                } else if pct >= 20 {
                    "NEEDS WORK"
                } else {
                    "AT RISK"
                };
                output.push_str(&format!(
                    "  '{}' [{}]: {}/{} streak ({}%) — {}\n",
                    h.name, h.frequency, h.streak, h.target_streak, pct, health
                ));
            }
        }
        output.push('\n');

        // Context switch frequency
        let today = Self::today_str();
        let today_switches = state
            .context_switches
            .iter()
            .filter(|cs| cs.timestamp.format("%Y-%m-%d").to_string() == today)
            .count();
        let total_switches = state.context_switches.len();
        output.push_str(&format!(
            "Context switches:\n  Today: {}\n  Total recorded: {}\n",
            today_switches, total_switches
        ));

        Ok(ToolOutput::text(output))
    }

    fn action_optimize_schedule(&self, state: &PlannerState) -> Result<ToolOutput, ToolError> {
        let today = Self::today_str();
        let profile = &state.energy_profile;

        let mut output = String::from("=== Schedule Optimization Prompt ===\n\n");

        output.push_str("Based on the following data, suggest an optimized daily schedule:\n\n");

        // Energy profile
        output.push_str(&format!(
            "Energy profile:\n  Peak productivity hours: {:?}\n  Low energy hours: {:?}\n  Preferred break interval: {} mins\n\n",
            profile.peak_hours, profile.low_energy_hours, profile.preferred_break_interval_mins
        ));

        // Active deadlines sorted by urgency
        let mut active: Vec<&Deadline> = state
            .deadlines
            .iter()
            .filter(|d| d.status != DeadlineStatus::Completed)
            .collect();
        active.sort_by(|a, b| {
            let urgency_a = (Self::priority_sort_key(&a.priority), a.due_date.clone());
            let urgency_b = (Self::priority_sort_key(&b.priority), b.due_date.clone());
            urgency_a.cmp(&urgency_b)
        });

        if !active.is_empty() {
            output.push_str("Active deadlines (by urgency):\n");
            for d in &active {
                output.push_str(&format!(
                    "  [{}] '{}' due {} — est. {:.1}h, category: {}\n",
                    d.priority, d.title, d.due_date, d.estimated_hours, d.category
                ));
            }
            output.push('\n');
        }

        // Habits needing attention
        let needs_attention: Vec<&HabitEntry> = state
            .habits
            .iter()
            .filter(|h| h.last_completed.as_deref() != Some(today.as_str()))
            .collect();
        if !needs_attention.is_empty() {
            output.push_str("Habits pending today:\n");
            for h in &needs_attention {
                output.push_str(&format!(
                    "  '{}' ({}) — current streak: {}\n",
                    h.name, h.frequency, h.streak
                ));
            }
            output.push('\n');
        }

        // Context switch patterns
        let recent_switches: Vec<&ContextSwitchEntry> =
            state.context_switches.iter().rev().take(10).collect();
        if !recent_switches.is_empty() {
            output.push_str("Recent context switches (for pattern awareness):\n");
            for cs in &recent_switches {
                output.push_str(&format!(
                    "  {} -> {} at {}\n",
                    cs.from_task,
                    cs.to_task,
                    cs.timestamp.format("%H:%M")
                ));
            }
            output.push('\n');
        }

        output.push_str("Optimization guidelines:\n");
        output.push_str("  - Schedule high-priority/complex tasks during peak hours\n");
        output.push_str("  - Schedule routine/low-energy tasks during low energy hours\n");
        output.push_str("  - Minimize context switches by batching similar work\n");
        output.push_str("  - Include breaks at the preferred interval\n");
        output.push_str("  - Protect habit completion times\n");

        Ok(ToolOutput::text(output))
    }
}

/// Returns a date string N days from today in YYYY-MM-DD format.
fn upcoming_date_str(days: i64) -> String {
    (Utc::now().date_naive() + chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

#[async_trait]
impl Tool for LifePlannerTool {
    fn name(&self) -> &str {
        "life_planner"
    }

    fn description(&self) -> &str {
        "Personal productivity: energy-aware scheduling, deadlines, habits, context switching. Actions: set_energy_profile, add_deadline, log_habit, daily_plan, weekly_review, context_switch_log, balance_report, optimize_schedule."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "set_energy_profile",
                        "add_deadline",
                        "log_habit",
                        "daily_plan",
                        "weekly_review",
                        "context_switch_log",
                        "balance_report",
                        "optimize_schedule"
                    ],
                    "description": "Action to perform"
                },
                "peak_hours": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Peak productivity hours 0-23 (for set_energy_profile)"
                },
                "low_energy_hours": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Low energy hours 0-23 (for set_energy_profile)"
                },
                "preferred_break_interval_mins": {
                    "type": "integer",
                    "description": "Preferred break interval in minutes (default: 90)"
                },
                "title": {
                    "type": "string",
                    "description": "Deadline title (for add_deadline)"
                },
                "due_date": {
                    "type": "string",
                    "description": "Deadline due date YYYY-MM-DD (for add_deadline)"
                },
                "priority": {
                    "type": "string",
                    "enum": ["low", "medium", "high", "critical"],
                    "description": "Deadline priority (default: medium)"
                },
                "estimated_hours": {
                    "type": "number",
                    "description": "Estimated hours for the deadline (default: 0)"
                },
                "category": {
                    "type": "string",
                    "description": "Deadline category (default: general)"
                },
                "notes": {
                    "type": "string",
                    "description": "Additional notes for the deadline"
                },
                "habit_id": {
                    "type": "integer",
                    "description": "Habit ID to mark as completed (for log_habit)"
                },
                "name": {
                    "type": "string",
                    "description": "Name for new habit (for log_habit)"
                },
                "frequency": {
                    "type": "string",
                    "enum": ["daily", "weekly"],
                    "description": "Habit frequency (default: daily)"
                },
                "target_streak": {
                    "type": "integer",
                    "description": "Target streak for the habit (default: 30)"
                },
                "from_task": {
                    "type": "string",
                    "description": "Task switching from (for context_switch_log)"
                },
                "to_task": {
                    "type": "string",
                    "description": "Task switching to (for context_switch_log)"
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for context switch"
                }
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
        let mut state = self.load_state();

        match action {
            "set_energy_profile" => self.action_set_energy_profile(&args, &mut state),
            "add_deadline" => self.action_add_deadline(&args, &mut state),
            "log_habit" => self.action_log_habit(&args, &mut state),
            "daily_plan" => self.action_daily_plan(&state),
            "weekly_review" => self.action_weekly_review(&state),
            "context_switch_log" => self.action_context_switch_log(&args, &mut state),
            "balance_report" => self.action_balance_report(&state),
            "optimize_schedule" => self.action_optimize_schedule(&state),
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{}'. Use: set_energy_profile, add_deadline, log_habit, daily_plan, weekly_review, context_switch_log, balance_report, optimize_schedule",
                action
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

    fn make_tool() -> (TempDir, LifePlannerTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = LifePlannerTool::new(workspace);
        (dir, tool)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "life_planner");
        assert!(tool.description().contains("energy-aware"));
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert_eq!(tool.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, tool) = make_tool();
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        let action = &schema["properties"]["action"];
        assert!(action.get("enum").is_some());
        let actions: Vec<&str> = action["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(actions.contains(&"set_energy_profile"));
        assert!(actions.contains(&"add_deadline"));
        assert!(actions.contains(&"log_habit"));
        assert!(actions.contains(&"daily_plan"));
        assert!(actions.contains(&"weekly_review"));
        assert!(actions.contains(&"context_switch_log"));
        assert!(actions.contains(&"balance_report"));
        assert!(actions.contains(&"optimize_schedule"));
        assert_eq!(actions.len(), 8);
    }

    #[tokio::test]
    async fn test_set_energy_profile_valid() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "set_energy_profile",
                "peak_hours": [8, 9, 10, 11],
                "low_energy_hours": [13, 14],
                "preferred_break_interval_mins": 60
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Energy profile updated"));
        assert!(result.content.contains("[8, 9, 10, 11]"));
        assert!(result.content.contains("[13, 14]"));
        assert!(result.content.contains("60 mins"));

        // Verify persisted
        let state = tool.load_state();
        assert_eq!(state.energy_profile.peak_hours, vec![8, 9, 10, 11]);
        assert_eq!(state.energy_profile.low_energy_hours, vec![13, 14]);
        assert_eq!(state.energy_profile.preferred_break_interval_mins, 60);
    }

    #[tokio::test]
    async fn test_energy_profile_invalid_hours() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "set_energy_profile",
                "peak_hours": [9, 25],
                "low_energy_hours": [14]
            }))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("25"));
                assert!(reason.contains("0-23"));
            }
            e => panic!("Expected InvalidArguments, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_energy_profile_overlap() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "set_energy_profile",
                "peak_hours": [9, 10, 14],
                "low_energy_hours": [14, 15]
            }))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("14"));
                assert!(reason.contains("both"));
            }
            e => panic!("Expected InvalidArguments, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_add_deadline() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "add_deadline",
                "title": "Ship v2.0",
                "due_date": "2099-12-31",
                "priority": "high",
                "estimated_hours": 8.0,
                "category": "engineering",
                "notes": "Big release"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Deadline #1"));
        assert!(result.content.contains("Ship v2.0"));
        assert!(result.content.contains("2099-12-31"));
        assert!(result.content.contains("High"));
        assert!(result.content.contains("Pending"));

        // Verify persisted
        let state = tool.load_state();
        assert_eq!(state.deadlines.len(), 1);
        assert_eq!(state.deadlines[0].title, "Ship v2.0");
        assert_eq!(state.deadlines[0].category, "engineering");
        assert_eq!(state.deadlines[0].estimated_hours, 8.0);
        assert_eq!(state.next_deadline_id, 2);
    }

    #[tokio::test]
    async fn test_add_deadline_overdue() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "add_deadline",
                "title": "Missed task",
                "due_date": "2020-01-01"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Overdue"));

        let state = tool.load_state();
        assert_eq!(state.deadlines[0].status, DeadlineStatus::Overdue);
    }

    #[tokio::test]
    async fn test_log_habit_create_new() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "log_habit",
                "name": "Exercise",
                "frequency": "daily",
                "target_streak": 60
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Habit #1 created"));
        assert!(result.content.contains("Exercise"));
        assert!(result.content.contains("target streak: 60"));

        let state = tool.load_state();
        assert_eq!(state.habits.len(), 1);
        assert_eq!(state.habits[0].name, "Exercise");
        assert_eq!(state.habits[0].streak, 1);
        assert_eq!(state.habits[0].frequency, HabitFrequency::Daily);
        assert_eq!(state.habits[0].target_streak, 60);
        assert_eq!(state.habits[0].history.len(), 1);
    }

    #[tokio::test]
    async fn test_log_habit_increment_streak() {
        let (_dir, tool) = make_tool();

        // Create a habit
        tool.execute(json!({
            "action": "log_habit",
            "name": "Meditate"
        }))
        .await
        .unwrap();

        // Manually set last_completed to yesterday to test streak increment
        let mut state = tool.load_state();
        let yesterday = (Utc::now().date_naive() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        state.habits[0].last_completed = Some(yesterday.clone());
        state.habits[0].history = vec![yesterday];
        tool.save_state(&state).unwrap();

        // Log again today
        let result = tool
            .execute(json!({
                "action": "log_habit",
                "habit_id": 1
            }))
            .await
            .unwrap();
        assert!(result.content.contains("completed"));
        assert!(result.content.contains("Streak: 2"));

        let state = tool.load_state();
        assert_eq!(state.habits[0].streak, 2);
        assert_eq!(state.habits[0].best_streak, 2);
        assert_eq!(state.habits[0].history.len(), 2);
    }

    #[tokio::test]
    async fn test_context_switch_log() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "context_switch_log",
                "from_task": "coding",
                "to_task": "meeting",
                "reason": "standup time"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Context switch logged"));
        assert!(result.content.contains("coding"));
        assert!(result.content.contains("meeting"));
        assert!(result.content.contains("standup time"));

        let state = tool.load_state();
        assert_eq!(state.context_switches.len(), 1);
        assert_eq!(state.context_switches[0].from_task, "coding");
        assert_eq!(state.context_switches[0].to_task, "meeting");
        assert_eq!(state.context_switches[0].reason, "standup time");
    }

    #[tokio::test]
    async fn test_daily_plan_returns_prompt() {
        let (_dir, tool) = make_tool();

        // Add some data first
        tool.execute(json!({
            "action": "add_deadline",
            "title": "Future task",
            "due_date": "2099-06-15",
            "priority": "high"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "log_habit",
            "name": "Read"
        }))
        .await
        .unwrap();

        let result = tool.execute(json!({"action": "daily_plan"})).await.unwrap();
        assert!(result.content.contains("Daily Plan"));
        assert!(result.content.contains("Energy Profile"));
        assert!(result.content.contains("Peak hours"));
        assert!(result.content.contains("macos_calendar"));
        assert!(result.content.contains("break"));
    }

    #[tokio::test]
    async fn test_balance_report() {
        let (_dir, tool) = make_tool();

        // Add deadlines in different categories
        tool.execute(json!({
            "action": "add_deadline",
            "title": "Code feature",
            "due_date": "2099-12-01",
            "category": "engineering",
            "estimated_hours": 10.0
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_deadline",
            "title": "Write docs",
            "due_date": "2099-12-01",
            "category": "documentation",
            "estimated_hours": 3.0
        }))
        .await
        .unwrap();

        // Add a habit
        tool.execute(json!({
            "action": "log_habit",
            "name": "Walk",
            "target_streak": 30
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "balance_report"}))
            .await
            .unwrap();
        assert!(result.content.contains("Balance Report"));
        assert!(result.content.contains("engineering"));
        assert!(result.content.contains("documentation"));
        assert!(result.content.contains("10.0h"));
        assert!(result.content.contains("3.0h"));
        assert!(result.content.contains("Walk"));
        assert!(result.content.contains("Context switches"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();

        // Set energy profile
        tool.execute(json!({
            "action": "set_energy_profile",
            "peak_hours": [7, 8],
            "low_energy_hours": [13]
        }))
        .await
        .unwrap();

        // Add deadline
        tool.execute(json!({
            "action": "add_deadline",
            "title": "Test roundtrip",
            "due_date": "2099-01-01"
        }))
        .await
        .unwrap();

        // Add habit
        tool.execute(json!({
            "action": "log_habit",
            "name": "Stretch"
        }))
        .await
        .unwrap();

        // Log context switch
        tool.execute(json!({
            "action": "context_switch_log",
            "from_task": "A",
            "to_task": "B"
        }))
        .await
        .unwrap();

        // Load and verify full roundtrip
        let state = tool.load_state();
        assert_eq!(state.energy_profile.peak_hours, vec![7, 8]);
        assert_eq!(state.energy_profile.low_energy_hours, vec![13]);
        assert_eq!(state.energy_profile.preferred_break_interval_mins, 90);
        assert_eq!(state.deadlines.len(), 1);
        assert_eq!(state.deadlines[0].title, "Test roundtrip");
        assert_eq!(state.habits.len(), 1);
        assert_eq!(state.habits[0].name, "Stretch");
        assert_eq!(state.context_switches.len(), 1);
        assert_eq!(state.context_switches[0].from_task, "A");
        assert_eq!(state.next_deadline_id, 2);
        assert_eq!(state.next_habit_id, 2);

        // Verify the file actually exists at the expected path
        assert!(tool.state_path().exists());
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let (_dir, tool) = make_tool();
        let result = tool.execute(json!({"action": "bogus"})).await.unwrap();
        assert!(result.content.contains("Unknown action"));
        assert!(result.content.contains("bogus"));
    }
}
