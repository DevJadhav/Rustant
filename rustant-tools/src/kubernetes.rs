//! Kubernetes Tool — kubectl wrapper for Kubernetes cluster operations.
//!
//! Provides 10 actions: pods, services, deployments, events, logs,
//! describe, top, rollout_status, rollout_restart, scale.
//! Uses kubectl CLI — requires kubectl to be installed and configured.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct KubernetesTool {
    #[allow(dead_code)]
    workspace: PathBuf,
}

impl KubernetesTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_kubectl(&self, args: &[&str]) -> Result<String, ToolError> {
        let output = tokio::process::Command::new("kubectl")
            .args(args)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "kubernetes".into(),
                message: format!("kubectl not found or failed to execute: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            if stderr.is_empty() {
                return Err(ToolError::ExecutionFailed {
                    name: "kubernetes".into(),
                    message: format!("kubectl exited with status {}: {}", output.status, stdout),
                });
            }
            return Err(ToolError::ExecutionFailed {
                name: "kubernetes".into(),
                message: format!("kubectl error: {}", stderr.trim()),
            });
        }

        Ok(if stdout.is_empty() { stderr } else { stdout })
    }
}

#[async_trait]
impl Tool for KubernetesTool {
    fn name(&self) -> &str {
        "kubernetes"
    }

    fn description(&self) -> &str {
        "Kubernetes cluster operations via kubectl: list pods/services/deployments, view events/logs, describe resources, check resource usage, manage rollouts and scaling"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["pods", "services", "deployments", "events", "logs", "describe", "top", "rollout_status", "rollout_restart", "scale"],
                    "description": "Action to perform"
                },
                "namespace": {
                    "type": "string",
                    "description": "Kubernetes namespace (default: current context namespace)"
                },
                "name": {
                    "type": "string",
                    "description": "Resource name (for logs, describe, rollout, scale)"
                },
                "resource_type": {
                    "type": "string",
                    "description": "Resource type (for describe: pod, service, deployment, etc.)"
                },
                "labels": {
                    "type": "string",
                    "description": "Label selector (e.g., 'app=nginx,env=prod')"
                },
                "container": {
                    "type": "string",
                    "description": "Container name (for logs, when pod has multiple containers)"
                },
                "lines": {
                    "type": "integer",
                    "description": "Number of log lines to tail (default: 100)"
                },
                "since": {
                    "type": "string",
                    "description": "Show logs since duration (e.g., '1h', '30m')"
                },
                "replicas": {
                    "type": "integer",
                    "description": "Target replica count (for scale action)"
                },
                "status_filter": {
                    "type": "string",
                    "description": "Filter by status (Running, Pending, Failed, etc.)"
                },
                "event_type": {
                    "type": "string",
                    "description": "Event type filter (Normal, Warning)"
                },
                "top_type": {
                    "type": "string",
                    "enum": ["pods", "nodes"],
                    "description": "Resource type for top command"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "kubernetes".into(),
                reason: "Missing 'action'".into(),
            }
        })?;

        let namespace = args.get("namespace").and_then(|v| v.as_str());

        let result = match action {
            "pods" => {
                let mut cmd_args = vec!["get", "pods", "-o", "wide"];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                } else {
                    cmd_args.push("--all-namespaces");
                }
                if let Some(labels) = args.get("labels").and_then(|v| v.as_str()) {
                    cmd_args.extend_from_slice(&["-l", labels]);
                }

                let output = self.run_kubectl(&cmd_args).await?;

                // Optionally filter by status
                if let Some(status) = args.get("status_filter").and_then(|v| v.as_str()) {
                    let filtered: Vec<&str> = output
                        .lines()
                        .filter(|line| {
                            line.contains(status)
                                || line.starts_with("NAMESPACE")
                                || line.starts_with("NAME")
                        })
                        .collect();
                    filtered.join("\n")
                } else {
                    output
                }
            }
            "services" => {
                let mut cmd_args = vec!["get", "services", "-o", "wide"];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                } else {
                    cmd_args.push("--all-namespaces");
                }
                self.run_kubectl(&cmd_args).await?
            }
            "deployments" => {
                let mut cmd_args = vec!["get", "deployments", "-o", "wide"];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                } else {
                    cmd_args.push("--all-namespaces");
                }
                if let Some(labels) = args.get("labels").and_then(|v| v.as_str()) {
                    cmd_args.extend_from_slice(&["-l", labels]);
                }
                self.run_kubectl(&cmd_args).await?
            }
            "events" => {
                let mut cmd_args = vec!["get", "events", "--sort-by=.lastTimestamp"];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                } else {
                    cmd_args.push("--all-namespaces");
                }

                let output = self.run_kubectl(&cmd_args).await?;

                if let Some(event_type) = args.get("event_type").and_then(|v| v.as_str()) {
                    let filtered: Vec<&str> = output
                        .lines()
                        .filter(|line| {
                            line.contains(event_type)
                                || line.starts_with("NAMESPACE")
                                || line.starts_with("LAST")
                        })
                        .collect();
                    filtered.join("\n")
                } else {
                    output
                }
            }
            "logs" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "kubernetes".into(),
                        reason: "Missing 'name' (pod name)".into(),
                    }
                })?;
                let lines = args.get("lines").and_then(|v| v.as_u64()).unwrap_or(100);
                let lines_str = lines.to_string();

                let mut cmd_args = vec!["logs", name, "--tail", &lines_str];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                }
                if let Some(container) = args.get("container").and_then(|v| v.as_str()) {
                    cmd_args.extend_from_slice(&["-c", container]);
                }
                if let Some(since) = args.get("since").and_then(|v| v.as_str()) {
                    cmd_args.extend_from_slice(&["--since", since]);
                }
                self.run_kubectl(&cmd_args).await?
            }
            "describe" => {
                let resource_type = args
                    .get("resource_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pod");
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "kubernetes".into(),
                        reason: "Missing 'name'".into(),
                    }
                })?;

                let mut cmd_args = vec!["describe", resource_type, name];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                }
                self.run_kubectl(&cmd_args).await?
            }
            "top" => {
                let top_type = args
                    .get("top_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pods");
                let mut cmd_args = vec!["top", top_type];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                }
                self.run_kubectl(&cmd_args).await?
            }
            "rollout_status" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "kubernetes".into(),
                        reason: "Missing 'name' (deployment name)".into(),
                    }
                })?;

                let deployment = format!("deployment/{}", name);
                let mut cmd_args = vec!["rollout", "status", &deployment];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                }
                self.run_kubectl(&cmd_args).await?
            }
            "rollout_restart" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "kubernetes".into(),
                        reason: "Missing 'name' (deployment name)".into(),
                    }
                })?;

                let deployment = format!("deployment/{}", name);
                let mut cmd_args = vec!["rollout", "restart", &deployment];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                }
                self.run_kubectl(&cmd_args).await?
            }
            "scale" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "kubernetes".into(),
                        reason: "Missing 'name' (deployment name)".into(),
                    }
                })?;
                let replicas = args
                    .get("replicas")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "kubernetes".into(),
                        reason: "Missing 'replicas'".into(),
                    })?;
                let replicas_str = format!("--replicas={}", replicas);

                let deployment = format!("deployment/{}", name);
                let mut cmd_args = vec!["scale", &deployment, &replicas_str];
                if let Some(ns) = namespace {
                    cmd_args.extend_from_slice(&["-n", ns]);
                }
                self.run_kubectl(&cmd_args).await?
            }
            _ => {
                return Err(ToolError::InvalidArguments {
                    name: "kubernetes".into(),
                    reason: format!("Unknown action: {}", action),
                });
            }
        };

        Ok(ToolOutput::text(result))
    }

    fn risk_level(&self) -> RiskLevel {
        // Most actions are read-only but rollout_restart and scale are Execute
        RiskLevel::ReadOnly
    }
}
