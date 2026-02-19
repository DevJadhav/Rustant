//! System monitor tool — production service topology, health checks, and incident tracking.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ServiceType {
    Api,
    Database,
    Cache,
    Queue,
    Frontend,
    Worker,
    Gateway,
}

impl ServiceType {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "api" => Some(Self::Api),
            "database" => Some(Self::Database),
            "cache" => Some(Self::Cache),
            "queue" => Some(Self::Queue),
            "frontend" => Some(Self::Frontend),
            "worker" => Some(Self::Worker),
            "gateway" => Some(Self::Gateway),
            _ => None,
        }
    }
}

impl std::fmt::Display for ServiceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api => write!(f, "api"),
            Self::Database => write!(f, "database"),
            Self::Cache => write!(f, "cache"),
            Self::Queue => write!(f, "queue"),
            Self::Frontend => write!(f, "frontend"),
            Self::Worker => write!(f, "worker"),
            Self::Gateway => write!(f, "gateway"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ServiceStatus {
    Unknown,
    Healthy,
    Degraded,
    Down,
}

impl ServiceStatus {
    fn marker(&self) -> &'static str {
        match self {
            Self::Healthy => "\u{2713}",  // checkmark
            Self::Degraded => "\u{26a0}", // warning
            Self::Down => "\u{2717}",     // X mark
            Self::Unknown => "?",
        }
    }
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::Healthy => write!(f, "Healthy"),
            Self::Degraded => write!(f, "Degraded"),
            Self::Down => write!(f, "Down"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Service {
    id: usize,
    name: String,
    url: String,
    service_type: ServiceType,
    dependencies: Vec<usize>,
    health_endpoint: Option<String>,
    status: ServiceStatus,
    last_checked: Option<DateTime<Utc>>,
    response_time_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum IncidentSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl IncidentSeverity {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

impl std::fmt::Display for IncidentSeverity {
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
#[serde(rename_all = "lowercase")]
enum IncidentStatus {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
}

impl std::fmt::Display for IncidentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Investigating => write!(f, "Investigating"),
            Self::Identified => write!(f, "Identified"),
            Self::Monitoring => write!(f, "Monitoring"),
            Self::Resolved => write!(f, "Resolved"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimelineEntry {
    timestamp: DateTime<Utc>,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Incident {
    id: usize,
    title: String,
    severity: IncidentSeverity,
    affected_services: Vec<usize>,
    timeline: Vec<TimelineEntry>,
    status: IncidentStatus,
    root_cause: Option<String>,
    resolution: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MonitorState {
    services: Vec<Service>,
    incidents: Vec<Incident>,
    next_service_id: usize,
    next_incident_id: usize,
}

pub struct SystemMonitorTool {
    workspace: PathBuf,
}

impl SystemMonitorTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("monitoring")
            .join("topology.json")
    }

    fn load_state(&self) -> MonitorState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            MonitorState {
                services: Vec::new(),
                incidents: Vec::new(),
                next_service_id: 1,
                next_incident_id: 1,
            }
        }
    }

    fn save_state(&self, state: &MonitorState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "system_monitor".to_string(),
                message: format!("Failed to create state dir: {e}"),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "system_monitor".to_string(),
            message: format!("Failed to serialize state: {e}"),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "system_monitor".to_string(),
            message: format!("Failed to write state: {e}"),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "system_monitor".to_string(),
            message: format!("Failed to rename state file: {e}"),
        })?;
        Ok(())
    }

    fn action_add_service(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return Ok(ToolOutput::text("Please provide a service name."));
        }

        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if url.is_empty() {
            return Ok(ToolOutput::text("Please provide a service URL."));
        }

        let type_str = args
            .get("service_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let service_type = match ServiceType::from_str(type_str) {
            Some(t) => t,
            None => {
                return Ok(ToolOutput::text(format!(
                    "Invalid service_type: '{type_str}'. Use: api, database, cache, queue, frontend, worker, gateway"
                )));
            }
        };

        let dependencies: Vec<usize> = args
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();

        let health_endpoint = args
            .get("health_endpoint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut state = self.load_state();
        let id = state.next_service_id;
        state.next_service_id += 1;

        state.services.push(Service {
            id,
            name: name.to_string(),
            url: url.to_string(),
            service_type,
            dependencies,
            health_endpoint,
            status: ServiceStatus::Unknown,
            last_checked: None,
            response_time_ms: None,
        });

        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Added service '{name}' (#{id}) [{type_str}] at {url}"
        )))
    }

    fn action_topology(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        if state.services.is_empty() {
            return Ok(ToolOutput::text(
                "No services registered. Use add_service to register services.",
            ));
        }

        let mut output = String::from("Service Topology\n================\n\n");

        for service in &state.services {
            let marker = service.status.marker();
            output.push_str(&format!(
                "[{}] {} #{} ({}) - {}\n",
                marker, service.name, service.id, service.service_type, service.url
            ));
            if !service.dependencies.is_empty() {
                for dep_id in &service.dependencies {
                    if let Some(dep) = state.services.iter().find(|s| s.id == *dep_id) {
                        let dep_marker = dep.status.marker();
                        output
                            .push_str(&format!("  -> [{}] {} #{}\n", dep_marker, dep.name, dep.id));
                    } else {
                        output.push_str(&format!("  -> [?] unknown #{dep_id}\n"));
                    }
                }
            }
        }

        Ok(ToolOutput::text(output))
    }

    async fn action_health_check(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let mut state = self.load_state();
        if state.services.is_empty() {
            return Ok(ToolOutput::text("No services to check."));
        }

        let target_id = args
            .get("service_id")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "system_monitor".to_string(),
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        let indices: Vec<usize> = if let Some(sid) = target_id {
            match state.services.iter().position(|s| s.id == sid) {
                Some(idx) => vec![idx],
                None => {
                    return Ok(ToolOutput::text(format!("Service #{sid} not found.")));
                }
            }
        } else {
            (0..state.services.len()).collect()
        };

        let mut results = Vec::new();

        for idx in indices {
            let service = &state.services[idx];
            let endpoint = service.health_endpoint.as_deref().unwrap_or("/health");
            let check_url = format!("{}{}", service.url.trim_end_matches('/'), endpoint);

            let start = std::time::Instant::now();
            let result = client.get(&check_url).send().await;
            let elapsed_ms = start.elapsed().as_millis() as u64;

            let (new_status, detail) = match result {
                Ok(resp) => {
                    let status_code = resp.status().as_u16();
                    if (200..300).contains(&status_code) {
                        (
                            ServiceStatus::Healthy,
                            format!("HTTP {status_code} ({elapsed_ms}ms)"),
                        )
                    } else {
                        (
                            ServiceStatus::Degraded,
                            format!("HTTP {status_code} ({elapsed_ms}ms)"),
                        )
                    }
                }
                Err(e) => {
                    let msg = if e.is_timeout() {
                        "timeout".to_string()
                    } else if e.is_connect() {
                        "connection refused".to_string()
                    } else {
                        format!("{e}")
                    };
                    (ServiceStatus::Down, msg)
                }
            };

            let svc = &mut state.services[idx];
            svc.status = new_status.clone();
            svc.last_checked = Some(Utc::now());
            svc.response_time_ms = Some(elapsed_ms);

            results.push(format!(
                "  {} #{} ({}): {} - {}",
                svc.name,
                svc.id,
                new_status.marker(),
                new_status,
                detail
            ));
        }

        self.save_state(&state)?;

        let mut output = String::from("Health Check Results\n====================\n");
        for r in &results {
            output.push_str(r);
            output.push('\n');
        }
        Ok(ToolOutput::text(output))
    }

    fn action_log_incident(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if title.is_empty() {
            return Ok(ToolOutput::text("Please provide an incident title."));
        }

        let severity_str = args.get("severity").and_then(|v| v.as_str()).unwrap_or("");
        let severity = match IncidentSeverity::from_str(severity_str) {
            Some(s) => s,
            None => {
                return Ok(ToolOutput::text(format!(
                    "Invalid severity: '{severity_str}'. Use: low, medium, high, critical"
                )));
            }
        };

        let affected_services: Vec<usize> = args
            .get("affected_services")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();

        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Incident created")
            .to_string();

        let now = Utc::now();
        let mut state = self.load_state();
        let id = state.next_incident_id;
        state.next_incident_id += 1;

        state.incidents.push(Incident {
            id,
            title: title.to_string(),
            severity,
            affected_services,
            timeline: vec![TimelineEntry {
                timestamp: now,
                message,
            }],
            status: IncidentStatus::Investigating,
            root_cause: None,
            resolution: None,
            created_at: now,
        });

        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Incident #{id} logged: '{title}' [{severity_str}] - Status: Investigating"
        )))
    }

    fn action_correlate(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();

        let mut service_incident_count: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut severity_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for incident in &state.incidents {
            for sid in &incident.affected_services {
                *service_incident_count.entry(*sid).or_insert(0) += 1;
            }
            *severity_count
                .entry(format!("{}", incident.severity))
                .or_insert(0) += 1;
        }

        let mut output =
            String::from("Incident Correlation Analysis\n=============================\n\n");

        output.push_str(&format!("Total incidents: {}\n\n", state.incidents.len()));

        if !severity_count.is_empty() {
            output.push_str("Severity breakdown:\n");
            for (sev, count) in &severity_count {
                output.push_str(&format!("  {sev}: {count}\n"));
            }
            output.push('\n');
        }

        if !service_incident_count.is_empty() {
            output.push_str("Services by incident frequency:\n");
            let mut sorted: Vec<_> = service_incident_count.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));
            for (sid, count) in sorted {
                let name = state
                    .services
                    .iter()
                    .find(|s| s.id == *sid)
                    .map(|s| s.name.as_str())
                    .unwrap_or("unknown");
                output.push_str(&format!("  {name} (#{sid}) - {count} incidents\n"));
            }
            output.push('\n');
        }

        output.push_str("--- LLM Analysis Prompt ---\n");
        output.push_str("Based on the above incident data, identify:\n");
        output.push_str("1. Common failure patterns and root causes\n");
        output.push_str("2. Services that are single points of failure\n");
        output.push_str("3. Recommendations for improving reliability\n");

        Ok(ToolOutput::text(output))
    }

    fn action_generate_runbook(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let service_id = match args.get("service_id").and_then(|v| v.as_u64()) {
            Some(id) => id as usize,
            None => {
                return Ok(ToolOutput::text("Please provide a service_id."));
            }
        };

        let state = self.load_state();
        let service = match state.services.iter().find(|s| s.id == service_id) {
            Some(s) => s,
            None => {
                return Ok(ToolOutput::text(format!(
                    "Service #{service_id} not found."
                )));
            }
        };

        let dep_names: Vec<String> = service
            .dependencies
            .iter()
            .filter_map(|did| {
                state
                    .services
                    .iter()
                    .find(|s| s.id == *did)
                    .map(|s| format!("{} (#{})", s.name, s.id))
            })
            .collect();

        let related_incidents: Vec<&Incident> = state
            .incidents
            .iter()
            .filter(|i| i.affected_services.contains(&service_id))
            .collect();

        let mut output = String::from("--- Runbook Generation Prompt ---\n\n");
        output.push_str(&format!("Service: {} (#{}))\n", service.name, service.id));
        output.push_str(&format!("Type: {}\n", service.service_type));
        output.push_str(&format!("URL: {}\n", service.url));
        output.push_str(&format!("Status: {}\n", service.status));
        if let Some(ref ep) = service.health_endpoint {
            output.push_str(&format!("Health endpoint: {ep}\n"));
        }

        if !dep_names.is_empty() {
            output.push_str(&format!("Dependencies: {}\n", dep_names.join(", ")));
        }

        if !related_incidents.is_empty() {
            output.push_str(&format!(
                "\nRecent incidents ({}): \n",
                related_incidents.len()
            ));
            for inc in related_incidents.iter().rev().take(5) {
                output.push_str(&format!(
                    "  - #{} [{}] {} ({})\n",
                    inc.id, inc.severity, inc.title, inc.status
                ));
            }
        }

        output.push_str("\nPlease generate a runbook covering:\n");
        output.push_str("1. Service overview and architecture\n");
        output.push_str("2. Health check procedures\n");
        output.push_str("3. Common failure modes and troubleshooting steps\n");
        output.push_str("4. Escalation procedures\n");
        output.push_str("5. Recovery procedures\n");

        Ok(ToolOutput::text(output))
    }

    fn action_impact_analysis(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let service_id = match args.get("service_id").and_then(|v| v.as_u64()) {
            Some(id) => id as usize,
            None => {
                return Ok(ToolOutput::text("Please provide a service_id."));
            }
        };

        let state = self.load_state();

        if !state.services.iter().any(|s| s.id == service_id) {
            return Ok(ToolOutput::text(format!(
                "Service #{service_id} not found."
            )));
        }

        // Reverse BFS: find all services that transitively depend on the given service
        let mut impacted: Vec<usize> = Vec::new();
        let mut visited: HashSet<usize> = HashSet::new();
        let mut queue: VecDeque<usize> = VecDeque::new();

        queue.push_back(service_id);
        visited.insert(service_id);

        while let Some(current_id) = queue.pop_front() {
            // Find all services that list current_id in their dependencies
            for svc in &state.services {
                if svc.dependencies.contains(&current_id) && !visited.contains(&svc.id) {
                    visited.insert(svc.id);
                    impacted.push(svc.id);
                    queue.push_back(svc.id);
                }
            }
        }

        let source_name = state
            .services
            .iter()
            .find(|s| s.id == service_id)
            .map(|s| s.name.as_str())
            .unwrap_or("unknown");

        let mut output = String::from("Impact Analysis\n===============\n\n");
        output.push_str(&format!(
            "If '{source_name}' (#{service_id}) goes down:\n\n"
        ));

        if impacted.is_empty() {
            output.push_str("No other services depend on this service.\n");
        } else {
            output.push_str(&format!(
                "{} service(s) would be affected:\n",
                impacted.len()
            ));
            for sid in &impacted {
                if let Some(svc) = state.services.iter().find(|s| s.id == *sid) {
                    output.push_str(&format!(
                        "  - {} (#{}) [{}]\n",
                        svc.name, svc.id, svc.service_type
                    ));
                }
            }
        }

        Ok(ToolOutput::text(output))
    }

    fn action_list_services(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        if state.services.is_empty() {
            return Ok(ToolOutput::text(
                "No services registered. Use add_service to register services.",
            ));
        }

        let status_filter = args.get("status").and_then(|v| v.as_str());

        let filtered: Vec<&Service> = state
            .services
            .iter()
            .filter(|s| {
                if let Some(filter) = status_filter {
                    match filter.to_lowercase().as_str() {
                        "healthy" => s.status == ServiceStatus::Healthy,
                        "degraded" => s.status == ServiceStatus::Degraded,
                        "down" => s.status == ServiceStatus::Down,
                        "unknown" => s.status == ServiceStatus::Unknown,
                        _ => true,
                    }
                } else {
                    true
                }
            })
            .collect();

        if filtered.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No services match the filter '{}'.",
                status_filter.unwrap_or("all")
            )));
        }

        let mut output = String::from("Services\n========\n");
        for svc in &filtered {
            let checked = svc
                .last_checked
                .map(|t| format!(" (last checked: {})", t.format("%Y-%m-%d %H:%M:%S UTC")))
                .unwrap_or_default();
            let rt = svc
                .response_time_ms
                .map(|ms| format!(" [{ms}ms]"))
                .unwrap_or_default();
            output.push_str(&format!(
                "  #{} {} [{}] ({}) - {}{}{}\n",
                svc.id,
                svc.name,
                svc.status.marker(),
                svc.service_type,
                svc.status,
                rt,
                checked,
            ));
        }
        output.push_str(&format!("\nTotal: {} service(s)\n", filtered.len()));

        Ok(ToolOutput::text(output))
    }
}

#[async_trait]
impl Tool for SystemMonitorTool {
    fn name(&self) -> &str {
        "system_monitor"
    }

    fn description(&self) -> &str {
        "Production system monitoring: service topology, health checks, incident tracking. Actions: add_service, topology, health_check, log_incident, correlate, generate_runbook, impact_analysis, list_services."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add_service", "topology", "health_check", "log_incident", "correlate", "generate_runbook", "impact_analysis", "list_services"],
                    "description": "Action to perform"
                },
                "name": { "type": "string", "description": "Service name (for add_service)" },
                "url": { "type": "string", "description": "Service URL (for add_service)" },
                "service_type": {
                    "type": "string",
                    "enum": ["api", "database", "cache", "queue", "frontend", "worker", "gateway"],
                    "description": "Type of service (for add_service)"
                },
                "dependencies": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Array of service IDs this service depends on (for add_service)"
                },
                "health_endpoint": { "type": "string", "description": "Health check endpoint path (default: /health)" },
                "service_id": { "type": "integer", "description": "Target service ID (for health_check, generate_runbook, impact_analysis)" },
                "title": { "type": "string", "description": "Incident title (for log_incident)" },
                "severity": {
                    "type": "string",
                    "enum": ["low", "medium", "high", "critical"],
                    "description": "Incident severity (for log_incident)"
                },
                "affected_services": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Array of affected service IDs (for log_incident)"
                },
                "message": { "type": "string", "description": "Incident timeline message (for log_incident)" },
                "status": {
                    "type": "string",
                    "enum": ["healthy", "degraded", "down", "unknown"],
                    "description": "Filter by status (for list_services)"
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
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "add_service" => self.action_add_service(&args),
            "topology" => self.action_topology(),
            "health_check" => self.action_health_check(&args).await,
            "log_incident" => self.action_log_incident(&args),
            "correlate" => self.action_correlate(),
            "generate_runbook" => self.action_generate_runbook(&args),
            "impact_analysis" => self.action_impact_analysis(&args),
            "list_services" => self.action_list_services(&args),
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{action}'. Use: add_service, topology, health_check, log_incident, correlate, generate_runbook, impact_analysis, list_services"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (SystemMonitorTool, PathBuf) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = SystemMonitorTool::new(workspace.clone());
        // Leak the TempDir so it is not dropped while the test runs
        std::mem::forget(dir);
        (tool, workspace)
    }

    #[test]
    fn test_tool_properties() {
        let (tool, _ws) = make_tool();
        assert_eq!(tool.name(), "system_monitor");
        assert!(tool.description().contains("service topology"));
        assert_eq!(tool.risk_level(), RiskLevel::Network);
        assert_eq!(tool.timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_schema_validation() {
        let (tool, _ws) = make_tool();
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["action"].get("enum").is_some());
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert_eq!(actions.len(), 8);
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("action"))
        );
    }

    #[tokio::test]
    async fn test_add_service() {
        let (tool, ws) = make_tool();

        let result = tool
            .execute(json!({
                "action": "add_service",
                "name": "user-api",
                "url": "http://localhost:8080",
                "service_type": "api",
                "health_endpoint": "/healthz"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("user-api"));
        assert!(result.content.contains("#1"));

        // Verify state was persisted
        let state_path = ws.join(".rustant").join("monitoring").join("topology.json");
        assert!(state_path.exists());

        let state: MonitorState =
            serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
        assert_eq!(state.services.len(), 1);
        assert_eq!(state.services[0].name, "user-api");
        assert_eq!(state.services[0].status, ServiceStatus::Unknown);
        assert_eq!(
            state.services[0].health_endpoint,
            Some("/healthz".to_string())
        );
    }

    #[tokio::test]
    async fn test_topology_output() {
        let (tool, _ws) = make_tool();

        // Add a database
        tool.execute(json!({
            "action": "add_service",
            "name": "postgres",
            "url": "http://db:5432",
            "service_type": "database"
        }))
        .await
        .unwrap();

        // Add a cache
        tool.execute(json!({
            "action": "add_service",
            "name": "redis",
            "url": "http://cache:6379",
            "service_type": "cache"
        }))
        .await
        .unwrap();

        // Add an API that depends on both
        tool.execute(json!({
            "action": "add_service",
            "name": "user-api",
            "url": "http://api:3000",
            "service_type": "api",
            "dependencies": [1, 2]
        }))
        .await
        .unwrap();

        let result = tool.execute(json!({"action": "topology"})).await.unwrap();

        assert!(result.content.contains("Service Topology"));
        assert!(result.content.contains("postgres"));
        assert!(result.content.contains("redis"));
        assert!(result.content.contains("user-api"));
        // user-api should show its dependencies
        assert!(result.content.contains("-> [?] postgres #1"));
        assert!(result.content.contains("-> [?] redis #2"));
    }

    #[tokio::test]
    async fn test_health_check_invalid_url() {
        let (tool, _ws) = make_tool();

        // Add a service with a URL that will fail to connect
        tool.execute(json!({
            "action": "add_service",
            "name": "bad-service",
            "url": "http://127.0.0.1:1",
            "service_type": "api"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "health_check",
                "service_id": 1
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Health Check Results"));
        assert!(result.content.contains("bad-service"));
        assert!(result.content.contains("Down"));

        // Verify the state was updated
        let state = tool.load_state();
        assert_eq!(state.services[0].status, ServiceStatus::Down);
        assert!(state.services[0].last_checked.is_some());
    }

    #[tokio::test]
    async fn test_log_incident() {
        let (tool, _ws) = make_tool();

        // Add a service first
        tool.execute(json!({
            "action": "add_service",
            "name": "api",
            "url": "http://localhost:8080",
            "service_type": "api"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "log_incident",
                "title": "API latency spike",
                "severity": "high",
                "affected_services": [1],
                "message": "Response times exceeded 5s"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Incident #1"));
        assert!(result.content.contains("API latency spike"));
        assert!(result.content.contains("Investigating"));

        // Verify incident was stored
        let state = tool.load_state();
        assert_eq!(state.incidents.len(), 1);
        assert_eq!(state.incidents[0].title, "API latency spike");
        assert_eq!(state.incidents[0].severity, IncidentSeverity::High);
        assert_eq!(state.incidents[0].status, IncidentStatus::Investigating);
        assert_eq!(state.incidents[0].timeline.len(), 1);
        assert_eq!(
            state.incidents[0].timeline[0].message,
            "Response times exceeded 5s"
        );
        assert_eq!(state.incidents[0].affected_services, vec![1]);
    }

    #[tokio::test]
    async fn test_impact_analysis() {
        let (tool, _ws) = make_tool();

        // C (#1) - database, no deps
        tool.execute(json!({
            "action": "add_service",
            "name": "database-C",
            "url": "http://db:5432",
            "service_type": "database"
        }))
        .await
        .unwrap();

        // B (#2) depends on C
        tool.execute(json!({
            "action": "add_service",
            "name": "cache-B",
            "url": "http://cache:6379",
            "service_type": "cache",
            "dependencies": [1]
        }))
        .await
        .unwrap();

        // A (#3) depends on B
        tool.execute(json!({
            "action": "add_service",
            "name": "api-A",
            "url": "http://api:3000",
            "service_type": "api",
            "dependencies": [2]
        }))
        .await
        .unwrap();

        // Impact analysis on C: should find B and A
        let result = tool
            .execute(json!({
                "action": "impact_analysis",
                "service_id": 1
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Impact Analysis"));
        assert!(result.content.contains("database-C"));
        assert!(result.content.contains("cache-B"));
        assert!(result.content.contains("api-A"));
        assert!(result.content.contains("2 service(s) would be affected"));
    }

    #[tokio::test]
    async fn test_list_services_filter() {
        let (tool, _ws) = make_tool();

        // Add two services
        tool.execute(json!({
            "action": "add_service",
            "name": "svc-a",
            "url": "http://a:80",
            "service_type": "api"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_service",
            "name": "svc-b",
            "url": "http://b:80",
            "service_type": "worker"
        }))
        .await
        .unwrap();

        // Both should be Unknown status
        let result = tool
            .execute(json!({"action": "list_services"}))
            .await
            .unwrap();
        assert!(result.content.contains("svc-a"));
        assert!(result.content.contains("svc-b"));
        assert!(result.content.contains("Total: 2"));

        // Filter by unknown should show both
        let result = tool
            .execute(json!({"action": "list_services", "status": "unknown"}))
            .await
            .unwrap();
        assert!(result.content.contains("svc-a"));
        assert!(result.content.contains("svc-b"));

        // Filter by healthy should show none
        let result = tool
            .execute(json!({"action": "list_services", "status": "healthy"}))
            .await
            .unwrap();
        assert!(result.content.contains("No services match"));
    }

    #[tokio::test]
    async fn test_correlate_empty() {
        let (tool, _ws) = make_tool();

        let result = tool.execute(json!({"action": "correlate"})).await.unwrap();

        assert!(result.content.contains("Incident Correlation Analysis"));
        assert!(result.content.contains("Total incidents: 0"));
        assert!(result.content.contains("LLM Analysis Prompt"));
    }

    #[tokio::test]
    async fn test_generate_runbook_returns_prompt() {
        let (tool, _ws) = make_tool();

        tool.execute(json!({
            "action": "add_service",
            "name": "payment-api",
            "url": "http://payments:8080",
            "service_type": "api",
            "health_endpoint": "/status"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "generate_runbook",
                "service_id": 1
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Runbook Generation Prompt"));
        assert!(result.content.contains("payment-api"));
        assert!(result.content.contains("http://payments:8080"));
        assert!(result.content.contains("/status"));
        assert!(result.content.contains("Health check procedures"));
        assert!(result.content.contains("Recovery procedures"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (tool, _ws) = make_tool();

        // Add services and an incident
        tool.execute(json!({
            "action": "add_service",
            "name": "db",
            "url": "http://db:5432",
            "service_type": "database"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_service",
            "name": "api",
            "url": "http://api:3000",
            "service_type": "api",
            "dependencies": [1]
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "log_incident",
            "title": "DB down",
            "severity": "critical",
            "affected_services": [1]
        }))
        .await
        .unwrap();

        // Load and verify full state
        let state = tool.load_state();
        assert_eq!(state.services.len(), 2);
        assert_eq!(state.incidents.len(), 1);
        assert_eq!(state.next_service_id, 3);
        assert_eq!(state.next_incident_id, 2);
        assert_eq!(state.services[1].dependencies, vec![1]);
        assert_eq!(state.incidents[0].severity, IncidentSeverity::Critical);

        // Serialize and deserialize to verify roundtrip
        let json = serde_json::to_string_pretty(&state).unwrap();
        let restored: MonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.services.len(), 2);
        assert_eq!(restored.incidents.len(), 1);
        assert_eq!(restored.next_service_id, 3);
        assert_eq!(restored.next_incident_id, 2);
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let (tool, _ws) = make_tool();

        let result = tool
            .execute(json!({"action": "nonexistent"}))
            .await
            .unwrap();

        assert!(result.content.contains("Unknown action"));
        assert!(result.content.contains("nonexistent"));
    }
}
