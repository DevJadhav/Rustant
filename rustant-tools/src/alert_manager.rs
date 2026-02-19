//! Alert Manager Tool — alert lifecycle management for SRE operations.
//!
//! Provides 10 actions for managing alerts: create, list, acknowledge,
//! silence, escalate, correlate, group, resolve, history, rules.
//! State persisted to `.rustant/alerts/state.json`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

/// Alert severity levels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Critical => write!(f, "critical"),
            AlertSeverity::Warning => write!(f, "warning"),
            AlertSeverity::Info => write!(f, "info"),
        }
    }
}

/// Alert status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    Firing,
    Acknowledged,
    Silenced,
    Resolved,
}

impl std::fmt::Display for AlertStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertStatus::Firing => write!(f, "firing"),
            AlertStatus::Acknowledged => write!(f, "acknowledged"),
            AlertStatus::Silenced => write!(f, "silenced"),
            AlertStatus::Resolved => write!(f, "resolved"),
        }
    }
}

/// An alert instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: usize,
    pub name: String,
    pub severity: AlertSeverity,
    pub source: String,
    pub message: String,
    pub service_id: Option<usize>,
    pub status: AlertStatus,
    pub acknowledged_by: Option<String>,
    pub silenced_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub related_incident: Option<usize>,
    pub labels: std::collections::HashMap<String, String>,
}

/// An alerting rule definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: usize,
    pub name: String,
    pub condition: String,
    pub severity: AlertSeverity,
    pub for_duration_secs: u64,
    pub service_id: Option<usize>,
    pub enabled: bool,
}

/// Persistent state for the alert manager.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AlertState {
    alerts: Vec<Alert>,
    rules: Vec<AlertRule>,
    next_alert_id: usize,
    next_rule_id: usize,
}

// ---------------------------------------------------------------------------
// Tool struct
// ---------------------------------------------------------------------------

/// Alert manager tool.
pub struct AlertManagerTool {
    workspace: PathBuf,
}

impl AlertManagerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("alerts")
            .join("state.json")
    }

    fn load_state(&self) -> AlertState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            AlertState::default()
        }
    }

    fn save_state(&self, state: &AlertState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "alert_manager".to_string(),
                message: format!("Failed to create state dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "alert_manager".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "alert_manager".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "alert_manager".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }

    // --- action helpers ---

    fn action_create(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed alert");
        let severity = match args.get("severity").and_then(|v| v.as_str()) {
            Some("critical") => AlertSeverity::Critical,
            Some("warning") => AlertSeverity::Warning,
            _ => AlertSeverity::Info,
        };
        let source = args
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("manual")
            .to_string();
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let service_id = args
            .get("service_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let mut state = self.load_state();
        let id = state.next_alert_id;
        state.next_alert_id += 1;

        state.alerts.push(Alert {
            id,
            name: name.to_string(),
            severity: severity.clone(),
            source,
            message,
            service_id,
            status: AlertStatus::Firing,
            acknowledged_by: None,
            silenced_until: None,
            created_at: Utc::now(),
            resolved_at: None,
            related_incident: None,
            labels: std::collections::HashMap::new(),
        });

        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Alert #{} created: {} (severity: {})",
            id, name, severity
        )))
    }

    fn action_list(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let status_filter = args.get("status").and_then(|v| v.as_str());

        let filtered: Vec<&Alert> = state
            .alerts
            .iter()
            .filter(|a| {
                if let Some(sf) = status_filter {
                    format!("{}", a.status) == sf
                } else {
                    true
                }
            })
            .collect();

        if filtered.is_empty() {
            return Ok(ToolOutput::text("No alerts found."));
        }

        let mut out = format!("Alerts ({}):\n", filtered.len());
        for a in &filtered {
            out.push_str(&format!(
                "  #{} [{}] {} — {} (source: {}, created: {})\n",
                a.id,
                a.status,
                a.severity,
                a.name,
                a.source,
                a.created_at.format("%Y-%m-%d %H:%M")
            ));
        }
        Ok(ToolOutput::text(out))
    }

    fn action_acknowledge(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let alert_id = args
            .get("alert_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "alert_manager".to_string(),
                reason: "Missing 'alert_id' parameter".to_string(),
            })?;
        let acked_by = args
            .get("acknowledged_by")
            .and_then(|v| v.as_str())
            .unwrap_or("agent");

        let mut state = self.load_state();
        if let Some(alert) = state.alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.status = AlertStatus::Acknowledged;
            alert.acknowledged_by = Some(acked_by.to_string());
            self.save_state(&state)?;
            Ok(ToolOutput::text(format!(
                "Alert #{} acknowledged by {}",
                alert_id, acked_by
            )))
        } else {
            Ok(ToolOutput::text(format!("Alert #{} not found", alert_id)))
        }
    }

    fn action_silence(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let alert_id = args
            .get("alert_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "alert_manager".to_string(),
                reason: "Missing 'alert_id' parameter".to_string(),
            })?;
        let duration_mins = args
            .get("silence_duration_mins")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        let mut state = self.load_state();
        if let Some(alert) = state.alerts.iter_mut().find(|a| a.id == alert_id) {
            let until = Utc::now() + chrono::Duration::minutes(duration_mins as i64);
            alert.status = AlertStatus::Silenced;
            alert.silenced_until = Some(until);
            self.save_state(&state)?;
            Ok(ToolOutput::text(format!(
                "Alert #{} silenced until {}",
                alert_id,
                until.format("%Y-%m-%d %H:%M")
            )))
        } else {
            Ok(ToolOutput::text(format!("Alert #{} not found", alert_id)))
        }
    }

    fn action_escalate(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let alert_id = args
            .get("alert_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "alert_manager".to_string(),
                reason: "Missing 'alert_id' parameter".to_string(),
            })?;

        let mut state = self.load_state();
        let idx = state.alerts.iter().position(|a| a.id == alert_id);
        if let Some(idx) = idx {
            state.alerts[idx].severity = match state.alerts[idx].severity {
                AlertSeverity::Info => AlertSeverity::Warning,
                AlertSeverity::Warning => AlertSeverity::Critical,
                AlertSeverity::Critical => AlertSeverity::Critical,
            };
            state.alerts[idx].status = AlertStatus::Firing;
            let new_severity = state.alerts[idx].severity.clone();
            self.save_state(&state)?;
            Ok(ToolOutput::text(format!(
                "Alert #{} escalated to severity: {}",
                alert_id, new_severity
            )))
        } else {
            Ok(ToolOutput::text(format!("Alert #{} not found", alert_id)))
        }
    }

    fn action_correlate(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let window_mins = args
            .get("time_window_mins")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        let cutoff = Utc::now() - chrono::Duration::minutes(window_mins as i64);

        let recent: Vec<&Alert> = state
            .alerts
            .iter()
            .filter(|a| a.created_at >= cutoff && a.status == AlertStatus::Firing)
            .collect();

        if recent.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No firing alerts in the last {} minutes.",
                window_mins
            )));
        }

        // Group by service_id
        let mut by_service: std::collections::HashMap<Option<usize>, Vec<&Alert>> =
            std::collections::HashMap::new();
        for a in &recent {
            by_service.entry(a.service_id).or_default().push(a);
        }

        let mut out = format!(
            "Alert correlation (last {} mins, {} firing):\n",
            window_mins,
            recent.len()
        );
        for (svc, alerts) in &by_service {
            let svc_label = svc
                .map(|id| format!("Service #{}", id))
                .unwrap_or_else(|| "Unassigned".into());
            out.push_str(&format!("  {} ({} alerts):\n", svc_label, alerts.len()));
            for a in alerts {
                out.push_str(&format!("    #{} [{}] {}\n", a.id, a.severity, a.name));
            }
        }

        if by_service.values().any(|v| v.len() >= 3) {
            out.push_str(
                "\nPotential incident: multiple correlated alerts detected on same service\n",
            );
        }
        Ok(ToolOutput::text(out))
    }

    fn action_group(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let incident_id = args
            .get("incident_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "alert_manager".to_string(),
                reason: "Missing 'incident_id' parameter".to_string(),
            })?;
        let alert_id = args
            .get("alert_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "alert_manager".to_string(),
                reason: "Missing 'alert_id' parameter".to_string(),
            })?;

        let mut state = self.load_state();
        if let Some(alert) = state.alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.related_incident = Some(incident_id);
            self.save_state(&state)?;
            Ok(ToolOutput::text(format!(
                "Alert #{} grouped with incident #{}",
                alert_id, incident_id
            )))
        } else {
            Ok(ToolOutput::text(format!("Alert #{} not found", alert_id)))
        }
    }

    fn action_resolve(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let alert_id = args
            .get("alert_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "alert_manager".to_string(),
                reason: "Missing 'alert_id' parameter".to_string(),
            })?;

        let mut state = self.load_state();
        if let Some(alert) = state.alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.status = AlertStatus::Resolved;
            alert.resolved_at = Some(Utc::now());
            self.save_state(&state)?;
            Ok(ToolOutput::text(format!("Alert #{} resolved", alert_id)))
        } else {
            Ok(ToolOutput::text(format!("Alert #{} not found", alert_id)))
        }
    }

    fn action_history(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

        let resolved: Vec<&Alert> = state
            .alerts
            .iter()
            .filter(|a| a.status == AlertStatus::Resolved)
            .rev()
            .take(limit)
            .collect();

        if resolved.is_empty() {
            return Ok(ToolOutput::text("No resolved alerts."));
        }

        let mut out = format!("Alert history (last {}):\n", resolved.len());
        for a in &resolved {
            let resolved_at = a
                .resolved_at
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "?".into());
            out.push_str(&format!(
                "  #{} [{}] {} — resolved: {}\n",
                a.id, a.severity, a.name, resolved_at
            ));
        }
        Ok(ToolOutput::text(out))
    }

    fn action_rules(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let rule_name = args.get("rule_name").and_then(|v| v.as_str());

        if let Some(name) = rule_name {
            // Create a new rule
            let condition = args
                .get("condition")
                .and_then(|v| v.as_str())
                .unwrap_or("threshold_exceeded");
            let severity = match args.get("severity").and_then(|v| v.as_str()) {
                Some("critical") => AlertSeverity::Critical,
                Some("warning") => AlertSeverity::Warning,
                _ => AlertSeverity::Info,
            };
            let for_duration = args
                .get("for_duration_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(300);
            let service_id = args
                .get("service_id")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let mut state = self.load_state();
            let id = state.next_rule_id;
            state.next_rule_id += 1;

            state.rules.push(AlertRule {
                id,
                name: name.to_string(),
                condition: condition.to_string(),
                severity,
                for_duration_secs: for_duration,
                service_id,
                enabled: true,
            });

            self.save_state(&state)?;
            Ok(ToolOutput::text(format!(
                "Alert rule #{} '{}' created",
                id, name
            )))
        } else {
            // List rules
            let state = self.load_state();
            if state.rules.is_empty() {
                Ok(ToolOutput::text("No alerting rules defined."))
            } else {
                let mut out = format!("Alerting rules ({}):\n", state.rules.len());
                for r in &state.rules {
                    let status = if r.enabled { "enabled" } else { "disabled" };
                    out.push_str(&format!(
                        "  #{} [{}] {} — condition: '{}' (for {}s, status: {})\n",
                        r.id, r.severity, r.name, r.condition, r.for_duration_secs, status
                    ));
                }
                Ok(ToolOutput::text(out))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tool trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Tool for AlertManagerTool {
    fn name(&self) -> &str {
        "alert_manager"
    }

    fn description(&self) -> &str {
        "Alert lifecycle management: create, list, acknowledge, silence, escalate, correlate, group, resolve alerts, view history, manage alerting rules. Actions: create, list, acknowledge, silence, escalate, correlate, group, resolve, history, rules."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "acknowledge", "silence", "escalate", "correlate", "group", "resolve", "history", "rules"],
                    "description": "Action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Alert name (for create)"
                },
                "severity": {
                    "type": "string",
                    "enum": ["critical", "warning", "info"],
                    "description": "Alert severity"
                },
                "source": {
                    "type": "string",
                    "description": "Alert source (prometheus, manual, health_check)"
                },
                "message": {
                    "type": "string",
                    "description": "Alert message"
                },
                "alert_id": {
                    "type": "integer",
                    "description": "Alert ID (for ack, silence, escalate, resolve, group)"
                },
                "acknowledged_by": {
                    "type": "string",
                    "description": "Who acknowledged the alert"
                },
                "silence_duration_mins": {
                    "type": "integer",
                    "description": "Duration to silence in minutes (default: 60)"
                },
                "status": {
                    "type": "string",
                    "enum": ["firing", "acknowledged", "silenced", "resolved"],
                    "description": "Filter by status (for list)"
                },
                "time_window_mins": {
                    "type": "integer",
                    "description": "Time window for correlation in minutes (default: 30)"
                },
                "service_id": {
                    "type": "integer",
                    "description": "Associated service ID"
                },
                "incident_id": {
                    "type": "integer",
                    "description": "Related incident ID (for group)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max number of results (for history, default: 20)"
                },
                "rule_name": {
                    "type": "string",
                    "description": "Alert rule name (for rules action, creates a rule if provided)"
                },
                "condition": {
                    "type": "string",
                    "description": "Alert rule condition expression"
                },
                "for_duration_secs": {
                    "type": "integer",
                    "description": "Rule condition duration in seconds (default: 300)"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "create" => self.action_create(&args),
            "list" => self.action_list(&args),
            "acknowledge" => self.action_acknowledge(&args),
            "silence" => self.action_silence(&args),
            "escalate" => self.action_escalate(&args),
            "correlate" => self.action_correlate(&args),
            "group" => self.action_group(&args),
            "resolve" => self.action_resolve(&args),
            "history" => self.action_history(&args),
            "rules" => self.action_rules(&args),
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{}'. Use: create, list, acknowledge, silence, escalate, correlate, group, resolve, history, rules",
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

    fn make_tool() -> (TempDir, AlertManagerTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        (dir, AlertManagerTool::new(workspace))
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "alert_manager");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert!(tool.description().contains("Alert lifecycle"));
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, tool) = make_tool();
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        assert!(schema.get("properties").is_some());
        let action = &schema["properties"]["action"];
        assert!(action.get("enum").is_some());
        let actions = action["enum"].as_array().unwrap();
        assert_eq!(actions.len(), 10);
    }

    #[tokio::test]
    async fn test_create_and_list_alerts() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({
                "action": "create",
                "name": "High CPU",
                "severity": "warning",
                "source": "prometheus",
                "message": "CPU usage above 90%"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Alert #0 created"));

        let list = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(list.content.contains("High CPU"));
    }

    #[tokio::test]
    async fn test_acknowledge_alert() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Test", "severity": "info"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "acknowledge", "alert_id": 0, "acknowledged_by": "operator"}))
            .await
            .unwrap();
        assert!(result.content.contains("acknowledged"));
    }

    #[tokio::test]
    async fn test_silence_alert() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Noisy", "severity": "info"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "silence", "alert_id": 0, "silence_duration_mins": 120}))
            .await
            .unwrap();
        assert!(result.content.contains("silenced"));
    }

    #[tokio::test]
    async fn test_resolve_alert() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Fixed", "severity": "critical"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "resolve", "alert_id": 0}))
            .await
            .unwrap();
        assert!(result.content.contains("resolved"));
    }

    #[tokio::test]
    async fn test_escalate_alert() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Escalating", "severity": "info"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "escalate", "alert_id": 0}))
            .await
            .unwrap();
        assert!(result.content.contains("warning"));
    }

    #[tokio::test]
    async fn test_escalate_to_critical() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Double Escalate", "severity": "info"}))
            .await
            .unwrap();
        // First escalation: info -> warning
        tool.execute(json!({"action": "escalate", "alert_id": 0}))
            .await
            .unwrap();
        // Second escalation: warning -> critical
        let result = tool
            .execute(json!({"action": "escalate", "alert_id": 0}))
            .await
            .unwrap();
        assert!(result.content.contains("critical"));
    }

    #[tokio::test]
    async fn test_group_alert_with_incident() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Grouped", "severity": "warning"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "group", "alert_id": 0, "incident_id": 42}))
            .await
            .unwrap();
        assert!(result.content.contains("grouped with incident #42"));
    }

    #[tokio::test]
    async fn test_rules_create_and_list() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({
            "action": "rules",
            "rule_name": "high_error_rate",
            "condition": "error_rate > 0.05",
            "severity": "critical"
        }))
        .await
        .unwrap();
        let list = tool.execute(json!({"action": "rules"})).await.unwrap();
        assert!(list.content.contains("high_error_rate"));
    }

    #[tokio::test]
    async fn test_history_empty() {
        let (_dir, tool) = make_tool();
        let result = tool.execute(json!({"action": "history"})).await.unwrap();
        assert!(result.content.contains("No resolved alerts"));
    }

    #[tokio::test]
    async fn test_history_after_resolve() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Past alert", "severity": "info"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "resolve", "alert_id": 0}))
            .await
            .unwrap();
        let result = tool.execute(json!({"action": "history"})).await.unwrap();
        assert!(result.content.contains("Past alert"));
    }

    #[tokio::test]
    async fn test_list_filter_by_status() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "A", "severity": "info"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "create", "name": "B", "severity": "warning"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "resolve", "alert_id": 0}))
            .await
            .unwrap();

        // Only firing
        let result = tool
            .execute(json!({"action": "list", "status": "firing"}))
            .await
            .unwrap();
        assert!(result.content.contains("B"));
        assert!(!result.content.contains(" A "));

        // Only resolved
        let result = tool
            .execute(json!({"action": "list", "status": "resolved"}))
            .await
            .unwrap();
        assert!(result.content.contains("A"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();
        tool.execute(json!({"action": "create", "name": "Persist me", "severity": "critical"}))
            .await
            .unwrap();
        tool.execute(json!({
            "action": "rules",
            "rule_name": "test_rule",
            "condition": "cpu > 90"
        }))
        .await
        .unwrap();

        let state = tool.load_state();
        assert_eq!(state.alerts.len(), 1);
        assert_eq!(state.rules.len(), 1);
        assert_eq!(state.alerts[0].name, "Persist me");
        assert_eq!(state.rules[0].name, "test_rule");
        assert_eq!(state.next_alert_id, 1);
        assert_eq!(state.next_rule_id, 1);
    }

    #[tokio::test]
    async fn test_not_found_alert() {
        let (_dir, tool) = make_tool();
        let result = tool
            .execute(json!({"action": "acknowledge", "alert_id": 999}))
            .await
            .unwrap();
        assert!(result.content.contains("not found"));
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
