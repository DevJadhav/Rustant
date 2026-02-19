//! Deployment Intelligence Tool — deployment risk assessment and canary management.
//!
//! Provides 8 actions for deployment operations: assess_risk, canary_status,
//! rollback_check, deploy_timeline, change_window, pre_deploy_checklist,
//! post_deploy_verify, diff_analysis.
//! State persisted to `.rustant/deployments/state.json`.

use crate::registry::Tool;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Timelike, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Deployment status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployStatus {
    Planned,
    InProgress,
    Canary,
    Promoted,
    RolledBack,
    Failed,
}

impl std::fmt::Display for DeployStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployStatus::Planned => write!(f, "planned"),
            DeployStatus::InProgress => write!(f, "in_progress"),
            DeployStatus::Canary => write!(f, "canary"),
            DeployStatus::Promoted => write!(f, "promoted"),
            DeployStatus::RolledBack => write!(f, "rolled_back"),
            DeployStatus::Failed => write!(f, "failed"),
        }
    }
}

/// A risk factor contributing to deployment risk score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    pub name: String,
    pub score: f64,
    pub weight: f64,
    pub explanation: String,
}

/// Canary deployment metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryMetrics {
    pub traffic_percent: f64,
    pub error_rate: f64,
    pub latency_p99_ms: f64,
    pub success_criteria_met: bool,
}

/// A deployment record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub id: usize,
    pub service: String,
    pub version: String,
    pub previous_version: Option<String>,
    pub risk_score: f64,
    pub risk_factors: Vec<RiskFactor>,
    pub status: DeployStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub canary_metrics: Option<CanaryMetrics>,
    pub notes: Vec<String>,
}

/// Persistent state for deployment intelligence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DeployState {
    deployments: Vec<DeploymentRecord>,
    next_id: usize,
    change_windows: Vec<ChangeWindow>,
}

/// A defined change window (allowed deployment times).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChangeWindow {
    name: String,
    allowed_days: Vec<u8>, // 1=Mon..7=Sun
    start_hour: u8,
    end_hour: u8,
    timezone: String,
}

/// Deployment intelligence tool.
pub struct DeploymentIntelTool {
    workspace: PathBuf,
}

impl DeploymentIntelTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("deployments")
            .join("state.json")
    }

    fn load_state(&self) -> DeployState {
        if let Ok(data) = std::fs::read_to_string(self.state_path()) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            DeployState::default()
        }
    }

    fn save_state(&self, state: &DeployState) -> Result<(), ToolError> {
        let dir = self.workspace.join(".rustant").join("deployments");
        std::fs::create_dir_all(&dir).map_err(|e| ToolError::ExecutionFailed {
            name: "deployment_intel".to_string(),
            message: format!("Failed to create state dir: {}", e),
        })?;
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "deployment_intel".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = dir.join("state.json.tmp");
        let target = dir.join("state.json");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "deployment_intel".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &target).map_err(|e| ToolError::ExecutionFailed {
            name: "deployment_intel".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }

    fn compute_risk_score(factors: &[RiskFactor]) -> f64 {
        if factors.is_empty() {
            return 0.0;
        }
        let total_weight: f64 = factors.iter().map(|f| f.weight).sum();
        if total_weight <= 0.0 {
            return 0.0;
        }
        let weighted_sum: f64 = factors.iter().map(|f| f.score * f.weight).sum();
        (weighted_sum / total_weight).clamp(0.0, 1.0)
    }

    fn assess_risk_factors(service: &str, version: &str, state: &DeployState) -> Vec<RiskFactor> {
        let mut factors = Vec::new();

        // Check recent deployment history for this service
        let recent_deploys: Vec<&DeploymentRecord> = state
            .deployments
            .iter()
            .filter(|d| d.service == service)
            .rev()
            .take(5)
            .collect();

        // Factor: recent failures
        let recent_failures = recent_deploys
            .iter()
            .filter(|d| matches!(d.status, DeployStatus::Failed | DeployStatus::RolledBack))
            .count();
        if recent_failures > 0 {
            factors.push(RiskFactor {
                name: "recent_failures".to_string(),
                score: (recent_failures as f64 * 0.3).min(1.0),
                weight: 2.0,
                explanation: format!(
                    "{} recent failures/rollbacks for {}",
                    recent_failures, service
                ),
            });
        }

        // Factor: version jump magnitude (heuristic)
        if let Some(prev) = recent_deploys.first()
            && let Some(ref pv) = prev.previous_version
        {
            let is_major = version.split('.').next() != pv.split('.').next();
            if is_major {
                factors.push(RiskFactor {
                    name: "major_version_change".to_string(),
                    score: 0.6,
                    weight: 1.5,
                    explanation: format!("Major version change: {} -> {}", pv, version),
                });
            }
        }

        // Factor: time of day (deployments outside business hours are riskier)
        let hour = Utc::now().hour();
        if !(9..17).contains(&hour) {
            factors.push(RiskFactor {
                name: "off_hours".to_string(),
                score: 0.4,
                weight: 1.0,
                explanation: format!("Deployment at {}:00 UTC (outside 09:00-17:00 window)", hour),
            });
        }

        // Factor: day of week (Friday/weekend deployments)
        let weekday = Utc::now().weekday().num_days_from_monday();
        if weekday >= 4 {
            // Friday=4, Saturday=5, Sunday=6
            factors.push(RiskFactor {
                name: "weekend_deploy".to_string(),
                score: 0.5,
                weight: 1.5,
                explanation: "Deployment on Friday or weekend".to_string(),
            });
        }

        // Default baseline risk
        if factors.is_empty() {
            factors.push(RiskFactor {
                name: "baseline".to_string(),
                score: 0.1,
                weight: 1.0,
                explanation: format!("Baseline deployment risk for {} v{}", service, version),
            });
        }

        factors
    }
}

#[async_trait]
impl Tool for DeploymentIntelTool {
    fn name(&self) -> &str {
        "deployment_intel"
    }

    fn description(&self) -> &str {
        "Deployment risk assessment, canary analysis, rollback decisions, change window management, and deployment timeline tracking"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["assess_risk", "canary_status", "rollback_check", "deploy_timeline", "change_window", "pre_deploy_checklist", "post_deploy_verify", "diff_analysis"],
                    "description": "Action to perform"
                },
                "service": {
                    "type": "string",
                    "description": "Service name"
                },
                "version": {
                    "type": "string",
                    "description": "Version string"
                },
                "deployment_id": {
                    "type": "integer",
                    "description": "Deployment record ID"
                },
                "traffic_percent": {
                    "type": "number",
                    "description": "Canary traffic percentage"
                },
                "error_rate": {
                    "type": "number",
                    "description": "Observed error rate"
                },
                "latency_p99_ms": {
                    "type": "number",
                    "description": "P99 latency in milliseconds"
                },
                "note": {
                    "type": "string",
                    "description": "Note to add to deployment"
                },
                "window_name": {
                    "type": "string",
                    "description": "Change window name"
                },
                "allowed_days": {
                    "type": "string",
                    "description": "Comma-separated allowed days (1=Mon..7=Sun)"
                },
                "start_hour": {
                    "type": "integer",
                    "description": "Start hour (0-23)"
                },
                "end_hour": {
                    "type": "integer",
                    "description": "End hour (0-23)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Number of records to return"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "deployment_intel".to_string(),
                reason: "Missing 'action' parameter".to_string(),
            }
        })?;

        let mut state = self.load_state();

        let result = match action {
            "assess_risk" => {
                let service = args
                    .get("service")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'service' parameter".to_string(),
                    })?;
                let version = args
                    .get("version")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'version' parameter".to_string(),
                    })?;

                let factors = Self::assess_risk_factors(service, version, &state);
                let risk_score = Self::compute_risk_score(&factors);

                let previous_version = state
                    .deployments
                    .iter()
                    .rev()
                    .find(|d| d.service == service)
                    .map(|d| d.version.clone());

                let id = state.next_id;
                state.next_id += 1;

                state.deployments.push(DeploymentRecord {
                    id,
                    service: service.to_string(),
                    version: version.to_string(),
                    previous_version,
                    risk_score,
                    risk_factors: factors.clone(),
                    status: DeployStatus::Planned,
                    started_at: Utc::now(),
                    completed_at: None,
                    canary_metrics: None,
                    notes: Vec::new(),
                });
                self.save_state(&state)?;

                let risk_label = if risk_score < 0.3 {
                    "LOW"
                } else if risk_score < 0.6 {
                    "MEDIUM"
                } else if risk_score < 0.8 {
                    "HIGH"
                } else {
                    "CRITICAL"
                };

                let mut out = format!(
                    "Deployment #{} risk assessment for {} v{}:\n  Overall risk: {:.0}% ({})\n\nRisk factors:\n",
                    id,
                    service,
                    version,
                    risk_score * 100.0,
                    risk_label
                );
                for f in &factors {
                    out.push_str(&format!(
                        "  - {} (score: {:.2}, weight: {:.1}): {}\n",
                        f.name, f.score, f.weight, f.explanation
                    ));
                }
                out
            }
            "canary_status" => {
                let deploy_id = args
                    .get("deployment_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'deployment_id' parameter".to_string(),
                    })?;

                // Update canary metrics if provided
                let traffic = args.get("traffic_percent").and_then(|v| v.as_f64());
                let error_rate = args.get("error_rate").and_then(|v| v.as_f64());
                let latency = args.get("latency_p99_ms").and_then(|v| v.as_f64());

                // Update canary metrics on the deployment
                if let Some(deploy) = state.deployments.iter_mut().find(|d| d.id == deploy_id)
                    && (traffic.is_some() || error_rate.is_some() || latency.is_some())
                {
                    let metrics = deploy.canary_metrics.get_or_insert(CanaryMetrics {
                        traffic_percent: 0.0,
                        error_rate: 0.0,
                        latency_p99_ms: 0.0,
                        success_criteria_met: false,
                    });
                    if let Some(t) = traffic {
                        metrics.traffic_percent = t;
                    }
                    if let Some(e) = error_rate {
                        metrics.error_rate = e;
                    }
                    if let Some(l) = latency {
                        metrics.latency_p99_ms = l;
                    }
                    metrics.success_criteria_met =
                        metrics.error_rate < 0.01 && metrics.latency_p99_ms < 500.0;
                    deploy.status = DeployStatus::Canary;
                }

                self.save_state(&state)?;

                // Now read the deployment for display
                if let Some(deploy) = state.deployments.iter().find(|d| d.id == deploy_id) {
                    if let Some(ref m) = deploy.canary_metrics {
                        let verdict = if m.success_criteria_met {
                            "PASS"
                        } else {
                            "FAIL"
                        };
                        format!(
                            "Canary status for deployment #{} ({} v{}):\n  Traffic: {:.1}%\n  Error rate: {:.4}\n  P99 latency: {:.0}ms\n  Criteria met: {} ({})",
                            deploy.id,
                            deploy.service,
                            deploy.version,
                            m.traffic_percent,
                            m.error_rate,
                            m.latency_p99_ms,
                            m.success_criteria_met,
                            verdict
                        )
                    } else {
                        format!("Deployment #{} has no canary metrics yet.", deploy_id)
                    }
                } else {
                    format!("Deployment #{} not found", deploy_id)
                }
            }
            "rollback_check" => {
                let deploy_id = args
                    .get("deployment_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'deployment_id' parameter".to_string(),
                    })?;

                if let Some(deploy) = state.deployments.iter().find(|d| d.id == deploy_id) {
                    let should_rollback = deploy
                        .canary_metrics
                        .as_ref()
                        .is_some_and(|m| !m.success_criteria_met);
                    let has_previous = deploy.previous_version.is_some();
                    let prev_label = deploy.previous_version.as_deref().unwrap_or("none");

                    let mut out = format!(
                        "Rollback analysis for deployment #{} ({} v{}):\n  Previous version: {}\n  Rollback recommended: {}\n",
                        deploy.id, deploy.service, deploy.version, prev_label, should_rollback
                    );

                    if should_rollback {
                        if has_previous {
                            out.push_str(&format!("  Action: Rollback to v{}\n", prev_label));
                        } else {
                            out.push_str("  Warning: No previous version available for rollback\n");
                        }
                    }
                    out
                } else {
                    format!("Deployment #{} not found", deploy_id)
                }
            }
            "deploy_timeline" => {
                let service = args.get("service").and_then(|v| v.as_str());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                let filtered: Vec<&DeploymentRecord> = state
                    .deployments
                    .iter()
                    .filter(|d| service.is_none_or(|s| d.service == s))
                    .rev()
                    .take(limit)
                    .collect();

                if filtered.is_empty() {
                    "No deployments found.".to_string()
                } else {
                    let mut out = format!("Deployment timeline ({}):\n", filtered.len());
                    for d in &filtered {
                        let risk_label = if d.risk_score < 0.3 {
                            "low"
                        } else if d.risk_score < 0.6 {
                            "med"
                        } else {
                            "high"
                        };
                        out.push_str(&format!(
                            "  #{} [{}] {} v{} — risk: {:.0}% ({}) — {}\n",
                            d.id,
                            d.status,
                            d.service,
                            d.version,
                            d.risk_score * 100.0,
                            risk_label,
                            d.started_at.format("%Y-%m-%d %H:%M")
                        ));
                    }
                    out
                }
            }
            "change_window" => {
                if let Some(name) = args.get("window_name").and_then(|v| v.as_str()) {
                    let days_str = args
                        .get("allowed_days")
                        .and_then(|v| v.as_str())
                        .unwrap_or("1,2,3,4,5");
                    let days: Vec<u8> = days_str
                        .split(',')
                        .filter_map(|s| s.trim().parse().ok())
                        .collect();
                    let start = args.get("start_hour").and_then(|v| v.as_u64()).unwrap_or(9) as u8;
                    let end = args.get("end_hour").and_then(|v| v.as_u64()).unwrap_or(17) as u8;

                    state.change_windows.push(ChangeWindow {
                        name: name.to_string(),
                        allowed_days: days.clone(),
                        start_hour: start,
                        end_hour: end,
                        timezone: "UTC".to_string(),
                    });
                    self.save_state(&state)?;
                    format!(
                        "Change window '{}' created: days {:?}, hours {:02}:00-{:02}:00 UTC",
                        name, days, start, end
                    )
                } else if state.change_windows.is_empty() {
                    "No change windows defined.".to_string()
                } else {
                    let mut out = format!("Change windows ({}):\n", state.change_windows.len());
                    for w in &state.change_windows {
                        out.push_str(&format!(
                            "  {} — days: {:?}, hours: {:02}:00-{:02}:00 {}\n",
                            w.name, w.allowed_days, w.start_hour, w.end_hour, w.timezone
                        ));
                    }
                    out
                }
            }
            "pre_deploy_checklist" => {
                let service = args
                    .get("service")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'service' parameter".to_string(),
                    })?;
                let version = args
                    .get("version")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'version' parameter".to_string(),
                    })?;

                let mut checklist = format!("Pre-deploy checklist for {} v{}:\n", service, version);
                checklist.push_str("  [?] Tests passing\n");
                checklist.push_str("  [?] Changelog updated\n");
                checklist.push_str("  [?] Dependencies reviewed\n");
                checklist.push_str("  [?] Rollback plan documented\n");
                checklist.push_str("  [?] Monitoring dashboards ready\n");
                checklist.push_str("  [?] On-call team notified\n");

                // Check change window
                let in_window = state.change_windows.iter().any(|w| {
                    let hour = Utc::now().hour() as u8;
                    let day = Utc::now().weekday().num_days_from_monday() as u8 + 1;
                    w.allowed_days.contains(&day) && hour >= w.start_hour && hour < w.end_hour
                });
                if !state.change_windows.is_empty() {
                    let status = if in_window { "[OK]" } else { "[!!]" };
                    checklist.push_str(&format!("  {} Within change window\n", status));
                }

                checklist
            }
            "post_deploy_verify" => {
                let deploy_id = args
                    .get("deployment_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'deployment_id' parameter".to_string(),
                    })?;

                let mut canary_ok = false;
                let mut found = false;

                if let Some(deploy) = state.deployments.iter_mut().find(|d| d.id == deploy_id) {
                    found = true;
                    canary_ok = deploy
                        .canary_metrics
                        .as_ref()
                        .is_some_and(|m| m.success_criteria_met);

                    if canary_ok {
                        deploy.status = DeployStatus::Promoted;
                        deploy.completed_at = Some(Utc::now());
                    }

                    if let Some(note) = args.get("note").and_then(|v| v.as_str()) {
                        deploy.notes.push(note.to_string());
                    }
                }

                if !found {
                    format!("Deployment #{} not found", deploy_id)
                } else {
                    self.save_state(&state)?;
                    let deploy = state
                        .deployments
                        .iter()
                        .find(|d| d.id == deploy_id)
                        .unwrap();
                    let status = if canary_ok {
                        "PROMOTED"
                    } else {
                        "NEEDS_REVIEW"
                    };
                    format!(
                        "Post-deploy verification for #{} ({} v{}): {}\n  Canary criteria met: {}",
                        deploy.id, deploy.service, deploy.version, status, canary_ok
                    )
                }
            }
            "diff_analysis" => {
                let service = args
                    .get("service")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "deployment_intel".to_string(),
                        reason: "Missing 'service' parameter".to_string(),
                    })?;

                let deploys: Vec<&DeploymentRecord> = state
                    .deployments
                    .iter()
                    .filter(|d| d.service == service)
                    .rev()
                    .take(2)
                    .collect();

                if deploys.len() < 2 {
                    format!("Not enough deployment history for {} to compare.", service)
                } else {
                    let current = deploys[0];
                    let previous = deploys[1];

                    let mut out = format!(
                        "Diff analysis for {}:\n  Current: v{} (risk: {:.0}%, status: {})\n  Previous: v{} (risk: {:.0}%, status: {})\n",
                        service,
                        current.version,
                        current.risk_score * 100.0,
                        current.status,
                        previous.version,
                        previous.risk_score * 100.0,
                        previous.status
                    );

                    let risk_delta = current.risk_score - previous.risk_score;
                    let direction = if risk_delta > 0.0 {
                        "INCREASED"
                    } else if risk_delta < 0.0 {
                        "DECREASED"
                    } else {
                        "UNCHANGED"
                    };
                    out.push_str(&format!(
                        "  Risk change: {:.0}% ({})\n",
                        risk_delta.abs() * 100.0,
                        direction
                    ));
                    out
                }
            }
            _ => {
                return Err(ToolError::InvalidArguments {
                    name: "deployment_intel".to_string(),
                    reason: format!("Unknown action: {}", action),
                });
            }
        };

        Ok(ToolOutput::text(result))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (DeploymentIntelTool, TempDir) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        (DeploymentIntelTool::new(workspace), dir)
    }

    #[tokio::test]
    async fn test_assess_risk() {
        let (tool, _dir) = make_tool();
        let result = tool
            .execute(json!({
                "action": "assess_risk",
                "service": "api-gateway",
                "version": "2.1.0"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("risk assessment"));
        assert!(result.content.contains("api-gateway"));
    }

    #[tokio::test]
    async fn test_deploy_timeline() {
        let (tool, _dir) = make_tool();
        tool.execute(json!({"action": "assess_risk", "service": "web", "version": "1.0"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "assess_risk", "service": "web", "version": "1.1"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "deploy_timeline", "service": "web"}))
            .await
            .unwrap();
        assert!(result.content.contains("v1.0"));
        assert!(result.content.contains("v1.1"));
    }

    #[tokio::test]
    async fn test_canary_status() {
        let (tool, _dir) = make_tool();
        tool.execute(json!({"action": "assess_risk", "service": "api", "version": "3.0"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({
                "action": "canary_status",
                "deployment_id": 0,
                "traffic_percent": 10.0,
                "error_rate": 0.005,
                "latency_p99_ms": 200.0
            }))
            .await
            .unwrap();
        assert!(result.content.contains("PASS"));
    }

    #[tokio::test]
    async fn test_change_window() {
        let (tool, _dir) = make_tool();
        let result = tool
            .execute(json!({
                "action": "change_window",
                "window_name": "business_hours",
                "allowed_days": "1,2,3,4,5",
                "start_hour": 9,
                "end_hour": 17
            }))
            .await
            .unwrap();
        assert!(result.content.contains("business_hours"));
    }

    #[tokio::test]
    async fn test_diff_analysis() {
        let (tool, _dir) = make_tool();
        tool.execute(json!({"action": "assess_risk", "service": "svc", "version": "1.0"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "assess_risk", "service": "svc", "version": "2.0"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "diff_analysis", "service": "svc"}))
            .await
            .unwrap();
        assert!(result.content.contains("v1.0"));
        assert!(result.content.contains("v2.0"));
    }

    #[tokio::test]
    async fn test_pre_deploy_checklist() {
        let (tool, _dir) = make_tool();
        let result = tool
            .execute(json!({
                "action": "pre_deploy_checklist",
                "service": "auth",
                "version": "4.0"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("checklist"));
        assert!(result.content.contains("Tests passing"));
    }
}
