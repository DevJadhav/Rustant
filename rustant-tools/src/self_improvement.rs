//! Self-improvement tool — meta-capability for analyzing usage patterns, tracking
//! performance, storing preferences, and collecting feedback for continuous optimization.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::registry::Tool;

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsagePattern {
    tool_name: String,
    total_calls: usize,
    successful_calls: usize,
    total_tokens: usize,
    common_contexts: Vec<String>,
    last_used: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum AdaptationSource {
    AutoDiscovered,
    UserSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdaptationRule {
    id: usize,
    trigger: String,
    action: String,
    confidence: f64,
    source: AdaptationSource,
    created_at: DateTime<Utc>,
    times_applied: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerformanceMetric {
    task_type: String,
    samples: usize,
    avg_iterations: f64,
    avg_tokens: f64,
    success_rate: f64,
    improvement_trend: f64, // positive = improving
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserPreference {
    key: String,
    value: String,
    set_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FeedbackEntry {
    task_description: String,
    satisfaction: u32, // 1-5
    notes: String,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ImprovementState {
    patterns: Vec<UsagePattern>,
    rules: Vec<AdaptationRule>,
    metrics: Vec<PerformanceMetric>,
    preferences: Vec<UserPreference>,
    feedback: Vec<FeedbackEntry>,
    next_rule_id: usize,
}

// ---------------------------------------------------------------------------
// Tool implementation
// ---------------------------------------------------------------------------

pub struct SelfImprovementTool {
    workspace: PathBuf,
}

impl SelfImprovementTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("meta")
            .join("improvement.json")
    }

    fn load_state(&self) -> ImprovementState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            ImprovementState::default()
        }
    }

    fn save_state(&self, state: &ImprovementState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "self_improvement".to_string(),
                message: format!("Failed to create state dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "self_improvement".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "self_improvement".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "self_improvement".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }

    fn sessions_index_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("sessions")
            .join("index.json")
    }

    fn inbox_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("inbox")
            .join("items.json")
    }

    fn planner_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("life")
            .join("planner.json")
    }

    /// Try to load the session index JSON. Returns the sessions array value or None.
    fn load_sessions(&self) -> Option<Vec<Value>> {
        let path = self.sessions_index_path();
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        let parsed: Value = serde_json::from_str(&data).ok()?;
        // The SessionIndex struct serializes as { "entries": [...] }
        parsed
            .get("entries")
            .or_else(|| parsed.get("sessions"))
            .and_then(|v| v.as_array())
            .cloned()
    }

    fn analyze_patterns_impl(&self, state: &mut ImprovementState) -> String {
        let sessions = match self.load_sessions() {
            Some(s) if !s.is_empty() => s,
            _ => return "No session data available.".to_string(),
        };

        let total_sessions = sessions.len();
        let mut completed_count = 0usize;
        let mut total_tokens = 0usize;
        let mut total_messages = 0usize;
        let mut task_types: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut tool_mentions: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for session in &sessions {
            if session
                .get("completed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                completed_count += 1;
            }
            total_tokens += session
                .get("total_tokens")
                .or_else(|| session.get("token_usage"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            total_messages += session
                .get("message_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            // Extract task type from goal keywords
            if let Some(goal) = session
                .get("last_goal")
                .or_else(|| session.get("goal"))
                .and_then(|v| v.as_str())
            {
                let goal_lower = goal.to_lowercase();
                let keywords = [
                    "fix", "bug", "test", "review", "refactor", "document", "deploy", "search",
                    "write", "create", "build", "analyze",
                ];
                for kw in &keywords {
                    if goal_lower.contains(kw) {
                        *task_types.entry(kw.to_string()).or_default() += 1;
                    }
                }
                // Track tool mentions in goals
                let tool_names = [
                    "file_read",
                    "file_write",
                    "shell_exec",
                    "git",
                    "web_search",
                    "smart_edit",
                    "codebase_search",
                ];
                for tn in &tool_names {
                    if goal_lower.contains(tn) {
                        *tool_mentions.entry(tn.to_string()).or_default() += 1;
                    }
                }
            }
        }

        // Update patterns in state
        let now = Utc::now();
        for (tool_name, count) in &tool_mentions {
            if let Some(existing) = state
                .patterns
                .iter_mut()
                .find(|p| &p.tool_name == tool_name)
            {
                existing.total_calls += count;
                existing.last_used = Some(now);
            } else {
                state.patterns.push(UsagePattern {
                    tool_name: tool_name.clone(),
                    total_calls: *count,
                    successful_calls: 0,
                    total_tokens: 0,
                    common_contexts: Vec::new(),
                    last_used: Some(now),
                });
            }
        }

        // Update metrics
        let success_rate = if total_sessions > 0 {
            completed_count as f64 / total_sessions as f64
        } else {
            0.0
        };
        let avg_tokens = if total_sessions > 0 {
            total_tokens as f64 / total_sessions as f64
        } else {
            0.0
        };
        let avg_messages = if total_sessions > 0 {
            total_messages as f64 / total_sessions as f64
        } else {
            0.0
        };

        // Store overall metric
        if let Some(overall) = state.metrics.iter_mut().find(|m| m.task_type == "overall") {
            let old_rate = overall.success_rate;
            overall.samples = total_sessions;
            overall.avg_iterations = avg_messages;
            overall.avg_tokens = avg_tokens;
            overall.success_rate = success_rate;
            overall.improvement_trend = success_rate - old_rate;
        } else {
            state.metrics.push(PerformanceMetric {
                task_type: "overall".to_string(),
                samples: total_sessions,
                avg_iterations: avg_messages,
                avg_tokens,
                success_rate,
                improvement_trend: 0.0,
            });
        }

        // Store per-task-type metrics
        for (task_type, count) in &task_types {
            if let Some(metric) = state.metrics.iter_mut().find(|m| &m.task_type == task_type) {
                metric.samples = *count;
            } else {
                state.metrics.push(PerformanceMetric {
                    task_type: task_type.clone(),
                    samples: *count,
                    avg_iterations: 0.0,
                    avg_tokens: 0.0,
                    success_rate: 0.0,
                    improvement_trend: 0.0,
                });
            }
        }

        // Build output
        let mut output = format!(
            "=== Usage Pattern Analysis ===\n\
             Sessions analyzed: {}\n\
             Completed: {} ({:.0}%)\n\
             Total tokens: {}\n\
             Avg tokens/session: {:.0}\n\
             Avg messages/session: {:.1}\n",
            total_sessions,
            completed_count,
            success_rate * 100.0,
            total_tokens,
            avg_tokens,
            avg_messages,
        );

        if !task_types.is_empty() {
            output.push_str("\nTask type distribution:\n");
            let mut sorted_types: Vec<_> = task_types.iter().collect();
            sorted_types.sort_by(|a, b| b.1.cmp(a.1));
            for (task_type, count) in sorted_types {
                output.push_str(&format!("  {}: {}\n", task_type, count));
            }
        }

        if !tool_mentions.is_empty() {
            output.push_str("\nTool usage frequency (from goals):\n");
            let mut sorted_tools: Vec<_> = tool_mentions.iter().collect();
            sorted_tools.sort_by(|a, b| b.1.cmp(a.1));
            for (tool, count) in sorted_tools {
                output.push_str(&format!("  {}: {}\n", tool, count));
            }
        }

        output
    }

    fn performance_report_impl(&self, state: &ImprovementState, task_type: Option<&str>) -> String {
        let filtered: Vec<&PerformanceMetric> = if let Some(tt) = task_type {
            state
                .metrics
                .iter()
                .filter(|m| m.task_type.eq_ignore_ascii_case(tt))
                .collect()
        } else {
            state.metrics.iter().collect()
        };

        if filtered.is_empty() {
            return "No performance data recorded yet.".to_string();
        }

        let mut output = String::from("=== Performance Report ===\n");
        for metric in &filtered {
            let trend_arrow = if metric.improvement_trend > 0.01 {
                "^"
            } else if metric.improvement_trend < -0.01 {
                "v"
            } else {
                "="
            };
            output.push_str(&format!(
                "\n[{}] ({} samples)\n\
                 \x20 Avg iterations: {:.1}\n\
                 \x20 Avg tokens: {:.0}\n\
                 \x20 Success rate: {:.0}%\n\
                 \x20 Trend: {} ({:+.1}%)\n",
                metric.task_type,
                metric.samples,
                metric.avg_iterations,
                metric.avg_tokens,
                metric.success_rate * 100.0,
                trend_arrow,
                metric.improvement_trend * 100.0,
            ));
        }

        output
    }

    fn suggest_improvements_impl(&self, state: &ImprovementState) -> String {
        let mut prompt = String::from(
            "=== Self-Improvement Context ===\n\
             Analyze the following data and suggest concrete improvements.\n\n",
        );

        // Patterns
        if !state.patterns.is_empty() {
            prompt.push_str("Usage Patterns:\n");
            for p in &state.patterns {
                prompt.push_str(&format!(
                    "  - {} (calls: {}, successful: {}, tokens: {})\n",
                    p.tool_name, p.total_calls, p.successful_calls, p.total_tokens
                ));
            }
            prompt.push('\n');
        }

        // Metrics
        if !state.metrics.is_empty() {
            prompt.push_str("Performance Metrics:\n");
            for m in &state.metrics {
                prompt.push_str(&format!(
                    "  - {} ({} samples): {:.0}% success, {:.0} avg tokens, trend {:+.1}%\n",
                    m.task_type,
                    m.samples,
                    m.success_rate * 100.0,
                    m.avg_tokens,
                    m.improvement_trend * 100.0,
                ));
            }
            prompt.push('\n');
        }

        // Rules
        if !state.rules.is_empty() {
            prompt.push_str("Active Adaptation Rules:\n");
            for r in &state.rules {
                prompt.push_str(&format!(
                    "  - #{} trigger='{}' action='{}' confidence={:.2} applied={} times\n",
                    r.id, r.trigger, r.action, r.confidence, r.times_applied,
                ));
            }
            prompt.push('\n');
        }

        // Feedback
        if !state.feedback.is_empty() {
            prompt.push_str("Recent Feedback:\n");
            let recent: Vec<&FeedbackEntry> = state.feedback.iter().rev().take(10).collect();
            for f in &recent {
                prompt.push_str(&format!(
                    "  - [{}] satisfaction={}/5 task='{}' notes='{}'\n",
                    f.timestamp.format("%Y-%m-%d"),
                    f.satisfaction,
                    f.task_description,
                    f.notes,
                ));
            }
            prompt.push('\n');
        }

        // Preferences
        if !state.preferences.is_empty() {
            prompt.push_str("User Preferences:\n");
            for p in &state.preferences {
                prompt.push_str(&format!("  - {}={}\n", p.key, p.value));
            }
            prompt.push('\n');
        }

        prompt.push_str(
            "Suggestions requested:\n\
             1. What patterns indicate inefficiency?\n\
             2. Which tools could be used more effectively?\n\
             3. What adaptation rules should be added/modified?\n\
             4. How can task success rates be improved?\n\
             5. What user preferences should be adjusted?",
        );

        prompt
    }

    fn cognitive_load_impl(&self, task_description: Option<&str>) -> String {
        let mut score: u32 = 1; // base score
        let mut factors = Vec::new();

        // Count active sessions
        let active_sessions = self
            .load_sessions()
            .map(|sessions| {
                sessions
                    .iter()
                    .filter(|s| !s.get("completed").and_then(|v| v.as_bool()).unwrap_or(true))
                    .count()
            })
            .unwrap_or(0);
        if active_sessions > 0 {
            let add = active_sessions as u32;
            score += add;
            factors.push(format!("{} active session(s) (+{})", active_sessions, add));
        }

        // Count pending inbox items
        let inbox_items = self.count_inbox_items();
        if inbox_items > 0 {
            let add = (inbox_items / 5) as u32;
            if add > 0 {
                score += add;
                factors.push(format!(
                    "{} inbox items (+{}, 1 per 5 items)",
                    inbox_items, add
                ));
            }
        }

        // Count deadlines from planner
        let (critical, overdue) = self.count_deadlines();
        if critical > 0 {
            let add = critical as u32 * 2;
            score += add;
            factors.push(format!(
                "{} critical deadline(s) (+{}, 2 each)",
                critical, add
            ));
        }
        if overdue > 0 {
            let add = overdue as u32;
            score += add;
            factors.push(format!("{} overdue deadline(s) (+{})", overdue, add));
        }

        // Cap at 10
        score = score.min(10);

        let level = match score {
            1..=3 => "Low",
            4..=6 => "Moderate",
            7..=8 => "High",
            _ => "Very High",
        };

        let mut output = format!(
            "=== Cognitive Load Estimate ===\n\
             Score: {}/10 ({})\n",
            score, level,
        );

        if factors.is_empty() {
            output.push_str("Factors: none detected (minimal load)\n");
        } else {
            output.push_str("Factors:\n");
            for f in &factors {
                output.push_str(&format!("  - {}\n", f));
            }
        }

        if let Some(desc) = task_description {
            output.push_str(&format!("\nTask context: {}\n", desc));
            if score >= 7 {
                output.push_str(
                    "Recommendation: Consider deferring non-critical tasks or breaking this into smaller steps.\n",
                );
            } else if score >= 4 {
                output.push_str("Recommendation: Manageable load. Focus on one thing at a time.\n");
            } else {
                output
                    .push_str("Recommendation: Good capacity available. Proceed with the task.\n");
            }
        }

        output
    }

    fn count_inbox_items(&self) -> usize {
        let path = self.inbox_path();
        if !path.exists() {
            return 0;
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<Value>(&data).ok())
            .and_then(|v| {
                v.get("items")
                    .and_then(|items| items.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter(|i| !i.get("done").and_then(|d| d.as_bool()).unwrap_or(false))
                            .count()
                    })
            })
            .unwrap_or(0)
    }

    fn count_deadlines(&self) -> (usize, usize) {
        let path = self.planner_path();
        if !path.exists() {
            return (0, 0);
        }
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => return (0, 0),
        };
        let parsed: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => return (0, 0),
        };

        let mut critical = 0usize;
        let mut overdue = 0usize;
        let now = Utc::now();

        // Look for goals/deadlines arrays
        let items = parsed
            .get("goals")
            .or_else(|| parsed.get("deadlines"))
            .or_else(|| parsed.get("items"))
            .and_then(|v| v.as_array());

        if let Some(items) = items {
            for item in items {
                let priority = item.get("priority").and_then(|v| v.as_str()).unwrap_or("");
                let is_critical = priority.eq_ignore_ascii_case("critical")
                    || priority.eq_ignore_ascii_case("high");

                let is_overdue = item
                    .get("deadline")
                    .or_else(|| item.get("due_date"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .map(|d| d < now)
                    .unwrap_or(false);

                let is_done = item
                    .get("completed")
                    .or_else(|| item.get("done"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if is_done {
                    continue;
                }
                if is_critical {
                    critical += 1;
                }
                if is_overdue {
                    overdue += 1;
                }
            }
        }

        (critical, overdue)
    }
}

#[async_trait]
impl Tool for SelfImprovementTool {
    fn name(&self) -> &str {
        "self_improvement"
    }

    fn description(&self) -> &str {
        "Meta-capability: analyze usage patterns, track performance, store preferences. Actions: analyze_patterns, performance_report, suggest_improvements, set_preference, get_preferences, cognitive_load, feedback, reset_baseline."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "analyze_patterns",
                        "performance_report",
                        "suggest_improvements",
                        "set_preference",
                        "get_preferences",
                        "cognitive_load",
                        "feedback",
                        "reset_baseline"
                    ],
                    "description": "Action to perform"
                },
                "task_type": {
                    "type": "string",
                    "description": "Filter by task type (for performance_report)"
                },
                "key": {
                    "type": "string",
                    "description": "Preference key (for set_preference/get_preferences)"
                },
                "value": {
                    "type": "string",
                    "description": "Preference value (for set_preference)"
                },
                "task_description": {
                    "type": "string",
                    "description": "Task description (for cognitive_load/feedback)"
                },
                "satisfaction": {
                    "type": "integer",
                    "description": "Satisfaction score 1-5 (for feedback)",
                    "minimum": 1,
                    "maximum": 5
                },
                "notes": {
                    "type": "string",
                    "description": "Additional notes (for feedback)"
                }
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
            "analyze_patterns" => {
                let output = self.analyze_patterns_impl(&mut state);
                if output != "No session data available." {
                    self.save_state(&state)?;
                }
                Ok(ToolOutput::text(output))
            }

            "performance_report" => {
                let task_type = args.get("task_type").and_then(|v| v.as_str());
                let output = self.performance_report_impl(&state, task_type);
                Ok(ToolOutput::text(output))
            }

            "suggest_improvements" => {
                let output = self.suggest_improvements_impl(&state);
                Ok(ToolOutput::text(output))
            }

            "set_preference" => {
                let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("");
                if key.is_empty() || value.is_empty() {
                    return Ok(ToolOutput::text(
                        "Both 'key' and 'value' are required for set_preference.",
                    ));
                }
                let now = Utc::now();
                if let Some(existing) = state.preferences.iter_mut().find(|p| p.key == key) {
                    existing.value = value.to_string();
                    existing.set_at = now;
                } else {
                    state.preferences.push(UserPreference {
                        key: key.to_string(),
                        value: value.to_string(),
                        set_at: now,
                    });
                }
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Preference set: {} = {}",
                    key, value
                )))
            }

            "get_preferences" => {
                let key = args.get("key").and_then(|v| v.as_str());
                if let Some(key) = key {
                    if let Some(pref) = state.preferences.iter().find(|p| p.key == key) {
                        Ok(ToolOutput::text(format!(
                            "{} = {} (set {})",
                            pref.key,
                            pref.value,
                            pref.set_at.format("%Y-%m-%d %H:%M UTC")
                        )))
                    } else {
                        Ok(ToolOutput::text(format!(
                            "No preference found for key '{}'.",
                            key
                        )))
                    }
                } else if state.preferences.is_empty() {
                    Ok(ToolOutput::text("No preferences set."))
                } else {
                    let lines: Vec<String> = state
                        .preferences
                        .iter()
                        .map(|p| {
                            format!(
                                "  {} = {} (set {})",
                                p.key,
                                p.value,
                                p.set_at.format("%Y-%m-%d %H:%M UTC")
                            )
                        })
                        .collect();
                    Ok(ToolOutput::text(format!(
                        "Preferences ({}):\n{}",
                        state.preferences.len(),
                        lines.join("\n")
                    )))
                }
            }

            "cognitive_load" => {
                let task_description = args.get("task_description").and_then(|v| v.as_str());
                let output = self.cognitive_load_impl(task_description);
                Ok(ToolOutput::text(output))
            }

            "feedback" => {
                let task_description = args
                    .get("task_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if task_description.is_empty() {
                    return Ok(ToolOutput::text(
                        "Provide 'task_description' for feedback.",
                    ));
                }
                let satisfaction = args
                    .get("satisfaction")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                if satisfaction == 0 {
                    return Ok(ToolOutput::text(
                        "Provide 'satisfaction' score (1-5) for feedback.",
                    ));
                }
                let satisfaction = satisfaction.clamp(1, 5);
                let notes = args
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                state.feedback.push(FeedbackEntry {
                    task_description: task_description.to_string(),
                    satisfaction,
                    notes: notes.clone(),
                    timestamp: Utc::now(),
                });
                self.save_state(&state)?;

                Ok(ToolOutput::text(format!(
                    "Feedback recorded: satisfaction={}/5 for '{}'.",
                    satisfaction, task_description
                )))
            }

            "reset_baseline" => {
                state.patterns.clear();
                state.metrics.clear();
                state.rules.clear();
                // Preserve preferences and feedback
                state.next_rule_id = 0;
                self.save_state(&state)?;
                Ok(ToolOutput::text(
                    "Baseline reset: patterns, metrics, and rules cleared. Preferences and feedback preserved.",
                ))
            }

            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{}'. Use: analyze_patterns, performance_report, suggest_improvements, set_preference, get_preferences, cognitive_load, feedback, reset_baseline.",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (TempDir, SelfImprovementTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = SelfImprovementTool::new(workspace);
        (dir, tool)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "self_improvement");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert_eq!(tool.timeout(), std::time::Duration::from_secs(30));
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, tool) = make_tool();
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        assert!(schema.get("properties").is_some());
        let props = schema.get("properties").unwrap();
        assert!(props.get("action").is_some());
        assert!(props.get("key").is_some());
        assert!(props.get("value").is_some());
        assert!(props.get("task_type").is_some());
        assert!(props.get("task_description").is_some());
        assert!(props.get("satisfaction").is_some());
        assert!(props.get("notes").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].as_str().unwrap(), "action");
    }

    #[tokio::test]
    async fn test_set_preference() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "set_preference",
                "key": "theme",
                "value": "dark"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Preference set: theme = dark"));

        // Get it back
        let result = tool
            .execute(json!({
                "action": "get_preferences",
                "key": "theme"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("theme"));
        assert!(result.content.contains("dark"));
    }

    #[tokio::test]
    async fn test_set_preference_update() {
        let (_dir, tool) = make_tool();
        // Set initial
        tool.execute(json!({
            "action": "set_preference",
            "key": "editor",
            "value": "vim"
        }))
        .await
        .unwrap();

        // Update same key
        tool.execute(json!({
            "action": "set_preference",
            "key": "editor",
            "value": "emacs"
        }))
        .await
        .unwrap();

        // Verify updated
        let result = tool
            .execute(json!({
                "action": "get_preferences",
                "key": "editor"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("emacs"));
        assert!(!result.content.contains("vim"));

        // Verify only one preference exists (not two)
        let state = tool.load_state();
        assert_eq!(state.preferences.len(), 1);
    }

    #[tokio::test]
    async fn test_get_all_preferences() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "set_preference",
            "key": "theme",
            "value": "dark"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "set_preference",
            "key": "language",
            "value": "rust"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "get_preferences"}))
            .await
            .unwrap();
        assert!(result.content.contains("theme"));
        assert!(result.content.contains("dark"));
        assert!(result.content.contains("language"));
        assert!(result.content.contains("rust"));
        assert!(result.content.contains("Preferences (2)"));
    }

    #[tokio::test]
    async fn test_feedback_recording() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "feedback",
                "task_description": "Code review of PR #42",
                "satisfaction": 4,
                "notes": "Good suggestions but slow"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Feedback recorded"));
        assert!(result.content.contains("satisfaction=4/5"));
        assert!(result.content.contains("Code review of PR #42"));

        // Verify stored
        let state = tool.load_state();
        assert_eq!(state.feedback.len(), 1);
        assert_eq!(state.feedback[0].satisfaction, 4);
        assert_eq!(state.feedback[0].notes, "Good suggestions but slow");
    }

    #[tokio::test]
    async fn test_feedback_satisfaction_clamping() {
        let (_dir, tool) = make_tool();
        // Satisfaction > 5 should be clamped to 5
        let result = tool
            .execute(json!({
                "action": "feedback",
                "task_description": "Amazing task",
                "satisfaction": 10
            }))
            .await
            .unwrap();
        assert!(result.content.contains("satisfaction=5/5"));

        let state = tool.load_state();
        assert_eq!(state.feedback[0].satisfaction, 5);
    }

    #[tokio::test]
    async fn test_cognitive_load_empty() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({"action": "cognitive_load"}))
            .await
            .unwrap();
        assert!(result.content.contains("Cognitive Load Estimate"));
        assert!(result.content.contains("Score: 1/10"));
        assert!(result.content.contains("Low"));
    }

    #[tokio::test]
    async fn test_performance_report_empty() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({"action": "performance_report"}))
            .await
            .unwrap();
        assert!(result.content.contains("No performance data recorded yet."));
    }

    #[tokio::test]
    async fn test_analyze_patterns_no_sessions() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({"action": "analyze_patterns"}))
            .await
            .unwrap();
        assert!(result.content.contains("No session data available."));
    }

    #[tokio::test]
    async fn test_suggest_improvements_returns_prompt() {
        let (_dir, tool) = make_tool();
        // Add some data first
        tool.execute(json!({
            "action": "set_preference",
            "key": "verbosity",
            "value": "high"
        }))
        .await
        .unwrap();
        tool.execute(json!({
            "action": "feedback",
            "task_description": "Test task",
            "satisfaction": 3,
            "notes": "Okay"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "suggest_improvements"}))
            .await
            .unwrap();
        assert!(result.content.contains("Self-Improvement Context"));
        assert!(result.content.contains("Suggestions requested"));
        assert!(result.content.contains("verbosity"));
        assert!(result.content.contains("Test task"));
    }

    #[tokio::test]
    async fn test_reset_preserves_preferences() {
        let (_dir, tool) = make_tool();

        // Add preference
        tool.execute(json!({
            "action": "set_preference",
            "key": "theme",
            "value": "dark"
        }))
        .await
        .unwrap();

        // Add feedback
        tool.execute(json!({
            "action": "feedback",
            "task_description": "Some task",
            "satisfaction": 4
        }))
        .await
        .unwrap();

        // Manually add a pattern to state for the test
        {
            let mut state = tool.load_state();
            state.patterns.push(UsagePattern {
                tool_name: "shell_exec".to_string(),
                total_calls: 10,
                successful_calls: 8,
                total_tokens: 500,
                common_contexts: vec!["build".to_string()],
                last_used: Some(Utc::now()),
            });
            state.metrics.push(PerformanceMetric {
                task_type: "test".to_string(),
                samples: 5,
                avg_iterations: 3.0,
                avg_tokens: 200.0,
                success_rate: 0.8,
                improvement_trend: 0.1,
            });
            tool.save_state(&state).unwrap();
        }

        // Verify data is there
        let state = tool.load_state();
        assert_eq!(state.patterns.len(), 1);
        assert_eq!(state.metrics.len(), 1);
        assert_eq!(state.preferences.len(), 1);
        assert_eq!(state.feedback.len(), 1);

        // Reset
        let result = tool
            .execute(json!({"action": "reset_baseline"}))
            .await
            .unwrap();
        assert!(result.content.contains("Baseline reset"));
        assert!(result
            .content
            .contains("Preferences and feedback preserved"));

        // Verify patterns and metrics cleared, preferences and feedback kept
        let state = tool.load_state();
        assert!(state.patterns.is_empty());
        assert!(state.metrics.is_empty());
        assert!(state.rules.is_empty());
        assert_eq!(state.preferences.len(), 1);
        assert_eq!(state.preferences[0].key, "theme");
        assert_eq!(state.feedback.len(), 1);
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();

        // Set a preference
        tool.execute(json!({
            "action": "set_preference",
            "key": "lang",
            "value": "rust"
        }))
        .await
        .unwrap();

        // Record feedback
        tool.execute(json!({
            "action": "feedback",
            "task_description": "roundtrip test",
            "satisfaction": 5,
            "notes": "excellent"
        }))
        .await
        .unwrap();

        // Verify state persists by reloading
        let state = tool.load_state();
        assert_eq!(state.preferences.len(), 1);
        assert_eq!(state.preferences[0].key, "lang");
        assert_eq!(state.preferences[0].value, "rust");
        assert_eq!(state.feedback.len(), 1);
        assert_eq!(state.feedback[0].task_description, "roundtrip test");
        assert_eq!(state.feedback[0].satisfaction, 5);
        assert_eq!(state.feedback[0].notes, "excellent");

        // Verify serialization round-trip via raw JSON
        let path = tool.state_path();
        let raw = std::fs::read_to_string(&path).unwrap();
        let reparsed: ImprovementState = serde_json::from_str(&raw).unwrap();
        assert_eq!(reparsed.preferences.len(), 1);
        assert_eq!(reparsed.feedback.len(), 1);
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
