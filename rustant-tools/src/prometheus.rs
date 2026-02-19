//! Prometheus Tool — query Prometheus and Alertmanager HTTP APIs.
//!
//! Provides 8 actions: query, query_range, series, labels, alerts,
//! targets, rules, silence_create.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

pub struct PrometheusTool {
    workspace: PathBuf,
}

impl PrometheusTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn config_path(&self) -> PathBuf {
        self.workspace.join(".rustant").join("sre_config.json")
    }

    fn get_prometheus_url(&self) -> String {
        if let Ok(data) = std::fs::read_to_string(self.config_path())
            && let Ok(config) = serde_json::from_str::<Value>(&data)
            && let Some(url) = config.get("prometheus_url").and_then(|v| v.as_str())
        {
            return url.to_string();
        }
        // Check env var
        if let Ok(url) = std::env::var("PROMETHEUS_URL") {
            return url;
        }
        "http://localhost:9090".to_string()
    }

    fn get_alertmanager_url(&self) -> Option<String> {
        if let Ok(data) = std::fs::read_to_string(self.config_path())
            && let Ok(config) = serde_json::from_str::<Value>(&data)
            && let Some(url) = config.get("alertmanager_url").and_then(|v| v.as_str())
        {
            return Some(url.to_string());
        }
        std::env::var("ALERTMANAGER_URL").ok()
    }
}

#[async_trait]
impl Tool for PrometheusTool {
    fn name(&self) -> &str {
        "prometheus"
    }

    fn description(&self) -> &str {
        "Query Prometheus metrics and alerts: instant queries, range queries, series discovery, alerting rules, targets, and Alertmanager silence management"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["query", "query_range", "series", "labels", "alerts", "targets", "rules", "silence_create"],
                    "description": "Action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "PromQL query expression"
                },
                "start": {
                    "type": "string",
                    "description": "Start time (RFC3339 or relative like '1h')"
                },
                "end": {
                    "type": "string",
                    "description": "End time (RFC3339 or 'now')"
                },
                "step": {
                    "type": "string",
                    "description": "Query resolution step (e.g., '15s', '1m')"
                },
                "match": {
                    "type": "string",
                    "description": "Series selector for series/labels actions"
                },
                "label": {
                    "type": "string",
                    "description": "Label name for label values query"
                },
                "matchers": {
                    "type": "string",
                    "description": "Alertmanager matchers for silence (JSON array)"
                },
                "duration": {
                    "type": "string",
                    "description": "Silence duration (e.g., '2h', '30m')"
                },
                "comment": {
                    "type": "string",
                    "description": "Comment for silence creation"
                },
                "created_by": {
                    "type": "string",
                    "description": "Creator of the silence"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "prometheus".into(),
                reason: "Missing 'action'".into(),
            }
        })?;

        let base_url = self.get_prometheus_url();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "prometheus".into(),
                message: format!("HTTP client error: {}", e),
            })?;

        let result =
            match action {
                "query" => {
                    let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                        ToolError::InvalidArguments {
                            name: "prometheus".into(),
                            reason: "Missing 'query'".into(),
                        }
                    })?;

                    let url = format!("{}/api/v1/query", base_url);
                    let resp = client
                        .get(&url)
                        .query(&[("query", query)])
                        .send()
                        .await
                        .map_err(|e: reqwest::Error| ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Prometheus query failed: {}", e),
                        })?;

                    let body: Value = resp.json().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Failed to parse response: {}", e),
                        }
                    })?;

                    format_prometheus_response("query", &body)
                }
                "query_range" => {
                    let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                        ToolError::InvalidArguments {
                            name: "prometheus".into(),
                            reason: "Missing 'query'".into(),
                        }
                    })?;
                    let start = args.get("start").and_then(|v| v.as_str()).unwrap_or("1h");
                    let end = args.get("end").and_then(|v| v.as_str()).unwrap_or("now");
                    let step = args.get("step").and_then(|v| v.as_str()).unwrap_or("60s");

                    let url = format!("{}/api/v1/query_range", base_url);
                    let resp = client
                        .get(&url)
                        .query(&[
                            ("query", query),
                            ("start", start),
                            ("end", end),
                            ("step", step),
                        ])
                        .send()
                        .await
                        .map_err(|e: reqwest::Error| ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Prometheus range query failed: {}", e),
                        })?;

                    let body: Value = resp.json().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Failed to parse response: {}", e),
                        }
                    })?;

                    format_prometheus_response("query_range", &body)
                }
                "series" => {
                    let match_expr = args.get("match").and_then(|v| v.as_str()).unwrap_or("up");

                    let url = format!("{}/api/v1/series", base_url);
                    let resp = client
                        .get(&url)
                        .query(&[("match[]", match_expr)])
                        .send()
                        .await
                        .map_err(|e: reqwest::Error| ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Series query failed: {}", e),
                        })?;

                    let body: Value = resp.json().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Failed to parse response: {}", e),
                        }
                    })?;

                    let data = body.get("data").and_then(|d| d.as_array());
                    match data {
                        Some(series) => {
                            let mut out =
                                format!("Series matching '{}' ({}):\n", match_expr, series.len());
                            for (i, s) in series.iter().take(50).enumerate() {
                                out.push_str(&format!("  {}. {}\n", i + 1, s));
                            }
                            if series.len() > 50 {
                                out.push_str(&format!("  ... and {} more\n", series.len() - 50));
                            }
                            out
                        }
                        None => "No series found.".to_string(),
                    }
                }
                "labels" => {
                    let label = args.get("label").and_then(|v| v.as_str());

                    let (url, label_name) = if let Some(l) = label {
                        (
                            format!("{}/api/v1/label/{}/values", base_url, l),
                            l.to_string(),
                        )
                    } else {
                        (format!("{}/api/v1/labels", base_url), "all".to_string())
                    };

                    let resp = client.get(&url).send().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Labels query failed: {}", e),
                        }
                    })?;

                    let body: Value = resp.json().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Failed to parse response: {}", e),
                        }
                    })?;

                    let data = body.get("data").and_then(|d| d.as_array());
                    match data {
                        Some(labels) => {
                            let mut out =
                                format!("Labels ({}, {} values):\n", label_name, labels.len());
                            for l in labels.iter().take(100) {
                                out.push_str(&format!("  - {}\n", l.as_str().unwrap_or("?")));
                            }
                            out
                        }
                        None => "No labels found.".to_string(),
                    }
                }
                "alerts" => {
                    let url = format!("{}/api/v1/alerts", base_url);
                    let resp = client.get(&url).send().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Alerts query failed: {}", e),
                        }
                    })?;

                    let body: Value = resp.json().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Failed to parse response: {}", e),
                        }
                    })?;

                    let alerts = body
                        .get("data")
                        .and_then(|d| d.get("alerts"))
                        .and_then(|a| a.as_array());
                    match alerts {
                        Some(list) => {
                            let firing: Vec<&Value> = list
                                .iter()
                                .filter(|a| {
                                    a.get("state").and_then(|s| s.as_str()) == Some("firing")
                                })
                                .collect();
                            let mut out = format!(
                                "Prometheus alerts ({} total, {} firing):\n",
                                list.len(),
                                firing.len()
                            );
                            for a in list.iter().take(30) {
                                let name = a
                                    .get("labels")
                                    .and_then(|l| l.get("alertname"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("?");
                                let state = a.get("state").and_then(|s| s.as_str()).unwrap_or("?");
                                let severity = a
                                    .get("labels")
                                    .and_then(|l| l.get("severity"))
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("?");
                                out.push_str(&format!(
                                    "  [{}] {} (severity: {})\n",
                                    state, name, severity
                                ));
                            }
                            out
                        }
                        None => "No alerts found.".to_string(),
                    }
                }
                "targets" => {
                    let url = format!("{}/api/v1/targets", base_url);
                    let resp = client.get(&url).send().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Targets query failed: {}", e),
                        }
                    })?;

                    let body: Value = resp.json().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Failed to parse response: {}", e),
                        }
                    })?;

                    let active = body
                        .get("data")
                        .and_then(|d| d.get("activeTargets"))
                        .and_then(|t| t.as_array());
                    match active {
                        Some(targets) => {
                            let up = targets
                                .iter()
                                .filter(|t| t.get("health").and_then(|h| h.as_str()) == Some("up"))
                                .count();
                            let mut out =
                                format!("Scrape targets ({} total, {} up):\n", targets.len(), up);
                            for t in targets.iter().take(30) {
                                let scrape_url =
                                    t.get("scrapeUrl").and_then(|u| u.as_str()).unwrap_or("?");
                                let health =
                                    t.get("health").and_then(|h| h.as_str()).unwrap_or("?");
                                let job = t
                                    .get("labels")
                                    .and_then(|l| l.get("job"))
                                    .and_then(|j| j.as_str())
                                    .unwrap_or("?");
                                out.push_str(&format!(
                                    "  [{}] {} (job: {})\n",
                                    health, scrape_url, job
                                ));
                            }
                            out
                        }
                        None => "No targets found.".to_string(),
                    }
                }
                "rules" => {
                    let url = format!("{}/api/v1/rules", base_url);
                    let resp = client.get(&url).send().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Rules query failed: {}", e),
                        }
                    })?;

                    let body: Value = resp.json().await.map_err(|e: reqwest::Error| {
                        ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Failed to parse response: {}", e),
                        }
                    })?;

                    let groups = body
                        .get("data")
                        .and_then(|d| d.get("groups"))
                        .and_then(|g| g.as_array());
                    match groups {
                        Some(grps) => {
                            let total_rules: usize = grps
                                .iter()
                                .filter_map(|g| {
                                    g.get("rules").and_then(|r| r.as_array()).map(|r| r.len())
                                })
                                .sum();
                            let mut out = format!(
                                "Rule groups ({} groups, {} rules):\n",
                                grps.len(),
                                total_rules
                            );
                            for g in grps.iter().take(20) {
                                let name = g.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                                let rules = g
                                    .get("rules")
                                    .and_then(|r| r.as_array())
                                    .map(|r| r.len())
                                    .unwrap_or(0);
                                out.push_str(&format!("  {} ({} rules)\n", name, rules));
                            }
                            out
                        }
                        None => "No rules found.".to_string(),
                    }
                }
                "silence_create" => {
                    let am_url =
                        self.get_alertmanager_url()
                            .ok_or_else(|| ToolError::ExecutionFailed {
                                name: "prometheus".into(),
                                message:
                                    "Alertmanager URL not configured. Set ALERTMANAGER_URL env var."
                                        .into(),
                            })?;
                    let matchers = args
                        .get("matchers")
                        .and_then(|v| v.as_str())
                        .unwrap_or("[]");
                    let duration = args
                        .get("duration")
                        .and_then(|v| v.as_str())
                        .unwrap_or("2h");
                    let comment = args
                        .get("comment")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Silenced by Rustant agent");
                    let created_by = args
                        .get("created_by")
                        .and_then(|v| v.as_str())
                        .unwrap_or("rustant-agent");

                    // Parse duration to compute end time
                    let duration_secs = parse_duration_str(duration);
                    let now = chrono::Utc::now();
                    let ends_at = now + chrono::Duration::seconds(duration_secs as i64);

                    let silence_body = json!({
                        "matchers": serde_json::from_str::<Value>(matchers).unwrap_or(json!([])),
                        "startsAt": now.to_rfc3339(),
                        "endsAt": ends_at.to_rfc3339(),
                        "createdBy": created_by,
                        "comment": comment,
                    });

                    let resp = client
                        .post(format!("{}/api/v2/silences", am_url))
                        .json(&silence_body)
                        .send()
                        .await
                        .map_err(|e: reqwest::Error| ToolError::ExecutionFailed {
                            name: "prometheus".into(),
                            message: format!("Silence creation failed: {}", e),
                        })?;

                    let status = resp.status();
                    let body: Value = resp.json().await.unwrap_or(json!({}));
                    let silence_id = body
                        .get("silenceID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    format!(
                        "Silence created (status: {}, id: {}, until: {})",
                        status,
                        silence_id,
                        ends_at.format("%Y-%m-%d %H:%M UTC")
                    )
                }
                _ => {
                    return Err(ToolError::InvalidArguments {
                        name: "prometheus".into(),
                        reason: format!("Unknown action: {}", action),
                    });
                }
            };

        Ok(ToolOutput::text(result))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Network
    }
}

fn format_prometheus_response(action: &str, body: &Value) -> String {
    let status = body
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");
    let result_type = body
        .get("data")
        .and_then(|d| d.get("resultType"))
        .and_then(|t| t.as_str())
        .unwrap_or("unknown");

    if status != "success" {
        let error = body
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("unknown error");
        return format!("Prometheus {} error: {}", action, error);
    }

    let results = body
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(|r| r.as_array());
    match results {
        Some(data) => {
            let mut out = format!(
                "Prometheus {} (type: {}, {} results):\n",
                action,
                result_type,
                data.len()
            );
            for (i, item) in data.iter().take(20).enumerate() {
                let metric = item.get("metric").cloned().unwrap_or(json!({}));
                let metric_str = metric
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or("?")))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "{}".to_string());

                if let Some(value) = item.get("value").and_then(|v| v.as_array()) {
                    let val = value.get(1).and_then(|v| v.as_str()).unwrap_or("?");
                    out.push_str(&format!("  {}. {{{}}} = {}\n", i + 1, metric_str, val));
                } else if let Some(values) = item.get("values").and_then(|v| v.as_array()) {
                    out.push_str(&format!(
                        "  {}. {{{}}} ({} points)\n",
                        i + 1,
                        metric_str,
                        values.len()
                    ));
                }
            }
            if data.len() > 20 {
                out.push_str(&format!("  ... and {} more results\n", data.len() - 20));
            }
            out
        }
        None => format!("Prometheus {}: no results", action),
    }
}

fn parse_duration_str(s: &str) -> u64 {
    let s = s.trim();
    if let Some(hours) = s.strip_suffix('h') {
        hours.parse::<u64>().unwrap_or(2) * 3600
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<u64>().unwrap_or(120) * 60
    } else if let Some(secs) = s.strip_suffix('s') {
        secs.parse::<u64>().unwrap_or(7200)
    } else {
        s.parse::<u64>().unwrap_or(7200)
    }
}
