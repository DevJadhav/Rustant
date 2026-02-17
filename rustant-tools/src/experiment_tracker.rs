//! Experiment tracker tool — track scientific hypotheses, experiments, results, and evidence.

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum HypothesisStatus {
    Proposed,
    Testing,
    Supported,
    Refuted,
    Inconclusive,
}

impl std::fmt::Display for HypothesisStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Proposed => write!(f, "Proposed"),
            Self::Testing => write!(f, "Testing"),
            Self::Supported => write!(f, "Supported"),
            Self::Refuted => write!(f, "Refuted"),
            Self::Inconclusive => write!(f, "Inconclusive"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Evidence {
    experiment_id: String,
    finding: String,
    supports: bool,
    confidence: f64,
    recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Hypothesis {
    id: String,
    title: String,
    description: String,
    status: HypothesisStatus,
    evidence: Vec<Evidence>,
    tags: Vec<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ExperimentStatus {
    Planned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for ExperimentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planned => write!(f, "Planned"),
            Self::Running => write!(f, "Running"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
            Self::Cancelled => write!(f, "Cancelled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Experiment {
    id: String,
    hypothesis_id: Option<String>,
    name: String,
    description: String,
    config: Value,
    metrics: Value,
    status: ExperimentStatus,
    notes: String,
    tags: Vec<String>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ExperimentState {
    hypotheses: Vec<Hypothesis>,
    experiments: Vec<Experiment>,
    next_hypothesis_id: usize,
    next_experiment_id: usize,
}

// ---------------------------------------------------------------------------
// Tool struct
// ---------------------------------------------------------------------------

pub struct ExperimentTrackerTool {
    workspace: PathBuf,
}

impl ExperimentTrackerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("experiments")
            .join("tracker.json")
    }

    fn load_state(&self) -> ExperimentState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            ExperimentState {
                hypotheses: Vec::new(),
                experiments: Vec::new(),
                next_hypothesis_id: 1,
                next_experiment_id: 1,
            }
        }
    }

    fn save_state(&self, state: &ExperimentState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "experiment_tracker".to_string(),
                message: format!("Failed to create state dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "experiment_tracker".to_string(),
            message: format!("Failed to serialize state: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "experiment_tracker".to_string(),
            message: format!("Failed to write state: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "experiment_tracker".to_string(),
            message: format!("Failed to rename state file: {}", e),
        })?;
        Ok(())
    }

    // --- action helpers ---

    fn action_add_hypothesis(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if title.is_empty() {
            return Ok(ToolOutput::text(
                "Please provide a title for the hypothesis.",
            ));
        }
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tags = parse_tags(args);

        let mut state = self.load_state();
        let id = format!("h{}", state.next_hypothesis_id);
        state.next_hypothesis_id += 1;
        state.hypotheses.push(Hypothesis {
            id: id.clone(),
            title: title.to_string(),
            description,
            status: HypothesisStatus::Proposed,
            evidence: Vec::new(),
            tags,
            created_at: Utc::now(),
        });
        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Added hypothesis {} — '{}'.",
            id, title
        )))
    }

    fn action_update_hypothesis(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            return Ok(ToolOutput::text("Please provide a hypothesis id."));
        }
        let mut state = self.load_state();
        let hyp = state.hypotheses.iter_mut().find(|h| h.id == id);
        match hyp {
            Some(h) => {
                if let Some(title) = args.get("title").and_then(|v| v.as_str()) {
                    h.title = title.to_string();
                }
                if let Some(status_str) = args.get("status").and_then(|v| v.as_str())
                    && let Some(status) = parse_hypothesis_status(status_str) {
                        h.status = status;
                    }
                if let Some(tags_val) = args.get("tags")
                    && let Some(arr) = tags_val.as_array() {
                        h.tags = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                let title = h.title.clone();
                let status = h.status.clone();
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Updated hypothesis {} — '{}' [{}].",
                    id, title, status
                )))
            }
            None => Ok(ToolOutput::text(format!("Hypothesis {} not found.", id))),
        }
    }

    fn action_list_hypotheses(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let status_filter = args.get("status").and_then(|v| v.as_str());
        let tag_filter = args.get("tag").and_then(|v| v.as_str());

        let filtered: Vec<&Hypothesis> = state
            .hypotheses
            .iter()
            .filter(|h| {
                if let Some(sf) = status_filter
                    && let Some(parsed) = parse_hypothesis_status(sf)
                        && h.status != parsed {
                            return false;
                        }
                if let Some(tf) = tag_filter
                    && !h.tags.iter().any(|t| t == tf) {
                        return false;
                    }
                true
            })
            .collect();

        if filtered.is_empty() {
            return Ok(ToolOutput::text("No hypotheses found."));
        }
        let lines: Vec<String> = filtered
            .iter()
            .map(|h| {
                let tags = if h.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", h.tags.join(", "))
                };
                format!(
                    "  {} — {} [{}] ({} evidence){}",
                    h.id,
                    h.title,
                    h.status,
                    h.evidence.len(),
                    tags
                )
            })
            .collect();
        Ok(ToolOutput::text(format!(
            "Hypotheses ({}):\n{}",
            filtered.len(),
            lines.join("\n")
        )))
    }

    fn action_get_hypothesis(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            return Ok(ToolOutput::text("Please provide a hypothesis id."));
        }
        let state = self.load_state();
        let hyp = state.hypotheses.iter().find(|h| h.id == id);
        match hyp {
            Some(h) => {
                let linked_experiments: Vec<&Experiment> = state
                    .experiments
                    .iter()
                    .filter(|e| e.hypothesis_id.as_deref() == Some(&h.id))
                    .collect();

                let mut out = format!(
                    "Hypothesis: {} — {}\nStatus: {}\nDescription: {}\nTags: {}\nCreated: {}\n",
                    h.id,
                    h.title,
                    h.status,
                    if h.description.is_empty() {
                        "(none)"
                    } else {
                        &h.description
                    },
                    if h.tags.is_empty() {
                        "(none)".to_string()
                    } else {
                        h.tags.join(", ")
                    },
                    h.created_at.format("%Y-%m-%d %H:%M UTC"),
                );

                if !h.evidence.is_empty() {
                    out.push_str(&format!("\nEvidence ({}):\n", h.evidence.len()));
                    for ev in &h.evidence {
                        out.push_str(&format!(
                            "  [{}] {} (confidence: {:.2}, supports: {})\n",
                            ev.experiment_id, ev.finding, ev.confidence, ev.supports
                        ));
                    }
                }

                if !linked_experiments.is_empty() {
                    out.push_str(&format!(
                        "\nLinked experiments ({}):\n",
                        linked_experiments.len()
                    ));
                    for exp in &linked_experiments {
                        out.push_str(&format!("  {} — {} [{}]\n", exp.id, exp.name, exp.status));
                    }
                }

                Ok(ToolOutput::text(out))
            }
            None => Ok(ToolOutput::text(format!("Hypothesis {} not found.", id))),
        }
    }

    fn action_add_experiment(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return Ok(ToolOutput::text(
                "Please provide a name for the experiment.",
            ));
        }
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let hypothesis_id = args
            .get("hypothesis_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let config = args.get("config").cloned().unwrap_or(json!({}));
        let tags = parse_tags(args);

        let mut state = self.load_state();

        // Validate hypothesis_id if provided
        if let Some(ref hid) = hypothesis_id
            && !state.hypotheses.iter().any(|h| h.id == *hid) {
                return Ok(ToolOutput::text(format!("Hypothesis {} not found.", hid)));
            }

        let id = format!("e{}", state.next_experiment_id);
        state.next_experiment_id += 1;
        state.experiments.push(Experiment {
            id: id.clone(),
            hypothesis_id,
            name: name.to_string(),
            description,
            config,
            metrics: json!({}),
            status: ExperimentStatus::Planned,
            notes: String::new(),
            tags,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        });
        self.save_state(&state)?;
        Ok(ToolOutput::text(format!(
            "Added experiment {} — '{}'.",
            id, name
        )))
    }

    fn action_start_experiment(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            return Ok(ToolOutput::text("Please provide an experiment id."));
        }
        let mut state = self.load_state();
        let exp = state.experiments.iter_mut().find(|e| e.id == id);
        match exp {
            Some(e) => {
                if e.status != ExperimentStatus::Planned {
                    return Ok(ToolOutput::text(format!(
                        "Experiment {} cannot be started — current status is {}.",
                        id, e.status
                    )));
                }
                e.status = ExperimentStatus::Running;
                e.started_at = Some(Utc::now());
                let name = e.name.clone();
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Experiment {} '{}' is now running.",
                    id, name
                )))
            }
            None => Ok(ToolOutput::text(format!("Experiment {} not found.", id))),
        }
    }

    fn action_complete_experiment(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            return Ok(ToolOutput::text("Please provide an experiment id."));
        }
        let mut state = self.load_state();
        let exp = state.experiments.iter_mut().find(|e| e.id == id);
        match exp {
            Some(e) => {
                if e.status != ExperimentStatus::Running {
                    return Ok(ToolOutput::text(format!(
                        "Experiment {} cannot be completed — current status is {}.",
                        id, e.status
                    )));
                }
                e.status = ExperimentStatus::Completed;
                e.completed_at = Some(Utc::now());
                if let Some(metrics) = args.get("metrics") {
                    e.metrics = metrics.clone();
                }
                if let Some(notes) = args.get("notes").and_then(|v| v.as_str()) {
                    e.notes = notes.to_string();
                }
                let name = e.name.clone();
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Experiment {} '{}' completed.",
                    id, name
                )))
            }
            None => Ok(ToolOutput::text(format!("Experiment {} not found.", id))),
        }
    }

    fn action_fail_experiment(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            return Ok(ToolOutput::text("Please provide an experiment id."));
        }
        let mut state = self.load_state();
        let exp = state.experiments.iter_mut().find(|e| e.id == id);
        match exp {
            Some(e) => {
                if e.status != ExperimentStatus::Running {
                    return Ok(ToolOutput::text(format!(
                        "Experiment {} cannot be failed — current status is {}.",
                        id, e.status
                    )));
                }
                e.status = ExperimentStatus::Failed;
                e.completed_at = Some(Utc::now());
                if let Some(notes) = args.get("notes").and_then(|v| v.as_str()) {
                    e.notes = notes.to_string();
                }
                let name = e.name.clone();
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Experiment {} '{}' failed.",
                    id, name
                )))
            }
            None => Ok(ToolOutput::text(format!("Experiment {} not found.", id))),
        }
    }

    fn action_get_experiment(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            return Ok(ToolOutput::text("Please provide an experiment id."));
        }
        let state = self.load_state();
        let exp = state.experiments.iter().find(|e| e.id == id);
        match exp {
            Some(e) => {
                let mut out = format!(
                    "Experiment: {} — {}\nStatus: {}\nDescription: {}\nHypothesis: {}\nTags: {}\nConfig: {}\nMetrics: {}\nNotes: {}\nCreated: {}\nStarted: {}\nCompleted: {}\n",
                    e.id,
                    e.name,
                    e.status,
                    if e.description.is_empty() {
                        "(none)"
                    } else {
                        &e.description
                    },
                    e.hypothesis_id.as_deref().unwrap_or("(none)"),
                    if e.tags.is_empty() {
                        "(none)".to_string()
                    } else {
                        e.tags.join(", ")
                    },
                    e.config,
                    e.metrics,
                    if e.notes.is_empty() {
                        "(none)"
                    } else {
                        &e.notes
                    },
                    e.created_at.format("%Y-%m-%d %H:%M UTC"),
                    e.started_at
                        .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                        .unwrap_or_else(|| "(not started)".to_string()),
                    e.completed_at
                        .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                        .unwrap_or_else(|| "(not completed)".to_string()),
                );

                // Show related evidence if linked to a hypothesis
                if let Some(ref hid) = e.hypothesis_id
                    && let Some(hyp) = state.hypotheses.iter().find(|h| h.id == *hid) {
                        let related: Vec<&Evidence> = hyp
                            .evidence
                            .iter()
                            .filter(|ev| ev.experiment_id == e.id)
                            .collect();
                        if !related.is_empty() {
                            out.push_str(&format!(
                                "\nEvidence from this experiment ({}):\n",
                                related.len()
                            ));
                            for ev in &related {
                                out.push_str(&format!(
                                    "  {} (confidence: {:.2}, supports: {})\n",
                                    ev.finding, ev.confidence, ev.supports
                                ));
                            }
                        }
                    }

                Ok(ToolOutput::text(out))
            }
            None => Ok(ToolOutput::text(format!("Experiment {} not found.", id))),
        }
    }

    fn action_list_experiments(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let hypothesis_id_filter = args.get("hypothesis_id").and_then(|v| v.as_str());
        let status_filter = args.get("status").and_then(|v| v.as_str());
        let tag_filter = args.get("tag").and_then(|v| v.as_str());

        let filtered: Vec<&Experiment> = state
            .experiments
            .iter()
            .filter(|e| {
                if let Some(hid) = hypothesis_id_filter
                    && e.hypothesis_id.as_deref() != Some(hid) {
                        return false;
                    }
                if let Some(sf) = status_filter
                    && let Some(parsed) = parse_experiment_status(sf)
                        && e.status != parsed {
                            return false;
                        }
                if let Some(tf) = tag_filter
                    && !e.tags.iter().any(|t| t == tf) {
                        return false;
                    }
                true
            })
            .collect();

        if filtered.is_empty() {
            return Ok(ToolOutput::text("No experiments found."));
        }
        let lines: Vec<String> = filtered
            .iter()
            .map(|e| {
                let hyp = e
                    .hypothesis_id
                    .as_deref()
                    .map(|h| format!(" ({})", h))
                    .unwrap_or_default();
                let tags = if e.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", e.tags.join(", "))
                };
                format!("  {} — {} [{}]{}{}", e.id, e.name, e.status, hyp, tags)
            })
            .collect();
        Ok(ToolOutput::text(format!(
            "Experiments ({}):\n{}",
            filtered.len(),
            lines.join("\n")
        )))
    }

    fn action_record_evidence(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let hypothesis_id = args
            .get("hypothesis_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let experiment_id = args
            .get("experiment_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let finding = args.get("finding").and_then(|v| v.as_str()).unwrap_or("");
        let supports = args
            .get("supports")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let confidence = args
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        if hypothesis_id.is_empty() || experiment_id.is_empty() || finding.is_empty() {
            return Ok(ToolOutput::text(
                "Please provide hypothesis_id, experiment_id, and finding.",
            ));
        }

        let mut state = self.load_state();

        // Validate experiment exists
        if !state.experiments.iter().any(|e| e.id == experiment_id) {
            return Ok(ToolOutput::text(format!(
                "Experiment {} not found.",
                experiment_id
            )));
        }

        let hyp = state.hypotheses.iter_mut().find(|h| h.id == hypothesis_id);
        match hyp {
            Some(h) => {
                h.evidence.push(Evidence {
                    experiment_id: experiment_id.to_string(),
                    finding: finding.to_string(),
                    supports,
                    confidence,
                    recorded_at: Utc::now(),
                });
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Recorded evidence for {} from {} (supports: {}, confidence: {:.2}).",
                    hypothesis_id, experiment_id, supports, confidence
                )))
            }
            None => Ok(ToolOutput::text(format!(
                "Hypothesis {} not found.",
                hypothesis_id
            ))),
        }
    }

    fn action_compare_experiments(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let ids = args
            .get("ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if ids.len() < 2 {
            return Ok(ToolOutput::text(
                "Please provide at least 2 experiment ids to compare.",
            ));
        }

        let state = self.load_state();
        let experiments: Vec<&Experiment> = ids
            .iter()
            .filter_map(|id| state.experiments.iter().find(|e| e.id == *id))
            .collect();

        if experiments.is_empty() {
            return Ok(ToolOutput::text("No matching experiments found."));
        }

        let mut out = format!("Comparison of {} experiments:\n\n", experiments.len());
        for exp in &experiments {
            out.push_str(&format!("--- {} ---\n", exp.id));
            out.push_str(&format!("  Name: {}\n", exp.name));
            out.push_str(&format!("  Status: {}\n", exp.status));
            out.push_str(&format!(
                "  Hypothesis: {}\n",
                exp.hypothesis_id.as_deref().unwrap_or("(none)")
            ));
            out.push_str(&format!("  Config: {}\n", exp.config));
            out.push_str(&format!("  Metrics: {}\n", exp.metrics));
            if !exp.notes.is_empty() {
                out.push_str(&format!("  Notes: {}\n", exp.notes));
            }
            out.push('\n');
        }

        Ok(ToolOutput::text(out))
    }

    fn action_summary(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let hypothesis_id_filter = args.get("hypothesis_id").and_then(|v| v.as_str());

        let hypotheses: Vec<&Hypothesis> = if let Some(hid) = hypothesis_id_filter {
            state.hypotheses.iter().filter(|h| h.id == hid).collect()
        } else {
            state.hypotheses.iter().collect()
        };

        let experiments: Vec<&Experiment> = if let Some(hid) = hypothesis_id_filter {
            state
                .experiments
                .iter()
                .filter(|e| e.hypothesis_id.as_deref() == Some(hid))
                .collect()
        } else {
            state.experiments.iter().collect()
        };

        if hypotheses.is_empty() && experiments.is_empty() {
            return Ok(ToolOutput::text("No data to summarize."));
        }

        let mut out = String::from("Summary:\n\n");

        // Hypothesis stats
        out.push_str(&format!("Hypotheses: {}\n", hypotheses.len()));
        let proposed = hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Proposed)
            .count();
        let testing = hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Testing)
            .count();
        let supported = hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Supported)
            .count();
        let refuted = hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Refuted)
            .count();
        let inconclusive = hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Inconclusive)
            .count();
        out.push_str(&format!(
            "  Proposed: {}, Testing: {}, Supported: {}, Refuted: {}, Inconclusive: {}\n",
            proposed, testing, supported, refuted, inconclusive
        ));

        // Evidence balance
        let total_evidence: usize = hypotheses.iter().map(|h| h.evidence.len()).sum();
        let supporting: usize = hypotheses
            .iter()
            .flat_map(|h| h.evidence.iter())
            .filter(|e| e.supports)
            .count();
        let opposing = total_evidence - supporting;
        out.push_str(&format!(
            "\nEvidence: {} total ({} supporting, {} opposing)\n",
            total_evidence, supporting, opposing
        ));

        if total_evidence > 0 {
            let avg_confidence: f64 = hypotheses
                .iter()
                .flat_map(|h| h.evidence.iter())
                .map(|e| e.confidence)
                .sum::<f64>()
                / total_evidence as f64;
            out.push_str(&format!("  Average confidence: {:.2}\n", avg_confidence));
        }

        // Experiment stats
        out.push_str(&format!("\nExperiments: {}\n", experiments.len()));
        let completed = experiments
            .iter()
            .filter(|e| e.status == ExperimentStatus::Completed)
            .count();
        let failed = experiments
            .iter()
            .filter(|e| e.status == ExperimentStatus::Failed)
            .count();
        let running = experiments
            .iter()
            .filter(|e| e.status == ExperimentStatus::Running)
            .count();
        let planned = experiments
            .iter()
            .filter(|e| e.status == ExperimentStatus::Planned)
            .count();
        out.push_str(&format!(
            "  Planned: {}, Running: {}, Completed: {}, Failed: {}\n",
            planned, running, completed, failed
        ));

        if completed + failed > 0 {
            let success_rate = completed as f64 / (completed + failed) as f64 * 100.0;
            out.push_str(&format!("  Success rate: {:.0}%\n", success_rate));
        }

        Ok(ToolOutput::text(out))
    }

    fn action_export_markdown(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let hypothesis_id_filter = args.get("hypothesis_id").and_then(|v| v.as_str());

        let hypotheses: Vec<&Hypothesis> = if let Some(hid) = hypothesis_id_filter {
            state.hypotheses.iter().filter(|h| h.id == hid).collect()
        } else {
            state.hypotheses.iter().collect()
        };

        let mut md = String::from("# Experiment Tracker Report\n\n");

        if hypotheses.is_empty() && state.experiments.is_empty() {
            md.push_str("No data to export.\n");
            return Ok(ToolOutput::text(md));
        }

        for hyp in &hypotheses {
            md.push_str(&format!("## {} — {}\n\n", hyp.id, hyp.title));
            md.push_str(&format!("**Status:** {}\n\n", hyp.status));
            if !hyp.description.is_empty() {
                md.push_str(&format!("{}\n\n", hyp.description));
            }
            if !hyp.tags.is_empty() {
                md.push_str(&format!("**Tags:** {}\n\n", hyp.tags.join(", ")));
            }

            // Evidence
            if !hyp.evidence.is_empty() {
                md.push_str("### Evidence\n\n");
                md.push_str("| Experiment | Finding | Supports | Confidence |\n");
                md.push_str("|---|---|---|---|\n");
                for ev in &hyp.evidence {
                    md.push_str(&format!(
                        "| {} | {} | {} | {:.2} |\n",
                        ev.experiment_id, ev.finding, ev.supports, ev.confidence
                    ));
                }
                md.push('\n');
            }

            // Linked experiments
            let linked: Vec<&Experiment> = state
                .experiments
                .iter()
                .filter(|e| e.hypothesis_id.as_deref() == Some(&hyp.id))
                .collect();
            if !linked.is_empty() {
                md.push_str("### Experiments\n\n");
                for exp in &linked {
                    md.push_str(&format!(
                        "- **{}** — {} [{}]\n",
                        exp.id, exp.name, exp.status
                    ));
                }
                md.push('\n');
            }
        }

        // Unlinked experiments
        let unlinked: Vec<&Experiment> = state
            .experiments
            .iter()
            .filter(|e| {
                if hypothesis_id_filter.is_some() {
                    return false;
                }
                e.hypothesis_id.is_none()
            })
            .collect();
        if !unlinked.is_empty() {
            md.push_str("## Unlinked Experiments\n\n");
            for exp in &unlinked {
                md.push_str(&format!(
                    "- **{}** — {} [{}]\n",
                    exp.id, exp.name, exp.status
                ));
            }
            md.push('\n');
        }

        Ok(ToolOutput::text(md))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_tags(args: &Value) -> Vec<String> {
    args.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_hypothesis_status(s: &str) -> Option<HypothesisStatus> {
    match s.to_lowercase().as_str() {
        "proposed" => Some(HypothesisStatus::Proposed),
        "testing" => Some(HypothesisStatus::Testing),
        "supported" => Some(HypothesisStatus::Supported),
        "refuted" => Some(HypothesisStatus::Refuted),
        "inconclusive" => Some(HypothesisStatus::Inconclusive),
        _ => None,
    }
}

fn parse_experiment_status(s: &str) -> Option<ExperimentStatus> {
    match s.to_lowercase().as_str() {
        "planned" => Some(ExperimentStatus::Planned),
        "running" => Some(ExperimentStatus::Running),
        "completed" => Some(ExperimentStatus::Completed),
        "failed" => Some(ExperimentStatus::Failed),
        "cancelled" => Some(ExperimentStatus::Cancelled),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tool trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Tool for ExperimentTrackerTool {
    fn name(&self) -> &str {
        "experiment_tracker"
    }

    fn description(&self) -> &str {
        "Track scientific hypotheses, experiments, results, and evidence. Actions: add_hypothesis, update_hypothesis, list_hypotheses, get_hypothesis, add_experiment, start_experiment, complete_experiment, fail_experiment, get_experiment, list_experiments, record_evidence, compare_experiments, summary, export_markdown."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "add_hypothesis", "update_hypothesis", "list_hypotheses", "get_hypothesis",
                        "add_experiment", "start_experiment", "complete_experiment", "fail_experiment",
                        "get_experiment", "list_experiments",
                        "record_evidence", "compare_experiments", "summary", "export_markdown"
                    ],
                    "description": "Action to perform"
                },
                "id": { "type": "string", "description": "Hypothesis or experiment ID" },
                "title": { "type": "string", "description": "Hypothesis title" },
                "name": { "type": "string", "description": "Experiment name" },
                "description": { "type": "string", "description": "Description text" },
                "status": { "type": "string", "description": "Status to set (for update_hypothesis)" },
                "hypothesis_id": { "type": "string", "description": "Linked hypothesis ID" },
                "experiment_id": { "type": "string", "description": "Experiment ID (for record_evidence)" },
                "finding": { "type": "string", "description": "Evidence finding text" },
                "supports": { "type": "boolean", "description": "Whether evidence supports the hypothesis" },
                "confidence": { "type": "number", "description": "Confidence level 0.0-1.0 (default 0.5)" },
                "config": { "type": "object", "description": "Experiment configuration" },
                "metrics": { "type": "object", "description": "Experiment result metrics" },
                "notes": { "type": "string", "description": "Experiment notes" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for filtering"
                },
                "ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Experiment IDs (for compare_experiments)"
                },
                "tag": { "type": "string", "description": "Filter by tag" }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "add_hypothesis" => self.action_add_hypothesis(&args),
            "update_hypothesis" => self.action_update_hypothesis(&args),
            "list_hypotheses" => self.action_list_hypotheses(&args),
            "get_hypothesis" => self.action_get_hypothesis(&args),
            "add_experiment" => self.action_add_experiment(&args),
            "start_experiment" => self.action_start_experiment(&args),
            "complete_experiment" => self.action_complete_experiment(&args),
            "fail_experiment" => self.action_fail_experiment(&args),
            "get_experiment" => self.action_get_experiment(&args),
            "list_experiments" => self.action_list_experiments(&args),
            "record_evidence" => self.action_record_evidence(&args),
            "compare_experiments" => self.action_compare_experiments(&args),
            "summary" => self.action_summary(&args),
            "export_markdown" => self.action_export_markdown(&args),
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{}'. Use: add_hypothesis, update_hypothesis, list_hypotheses, get_hypothesis, add_experiment, start_experiment, complete_experiment, fail_experiment, get_experiment, list_experiments, record_evidence, compare_experiments, summary, export_markdown",
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

    fn make_tool() -> (TempDir, ExperimentTrackerTool) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = ExperimentTrackerTool::new(workspace);
        (dir, tool)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, tool) = make_tool();
        assert_eq!(tool.name(), "experiment_tracker");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert!(tool.description().contains("hypotheses"));
        assert!(tool.description().contains("experiments"));
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
        assert_eq!(actions.len(), 14);
    }

    #[tokio::test]
    async fn test_add_hypothesis() {
        let (_dir, tool) = make_tool();

        let result = tool
            .execute(json!({
                "action": "add_hypothesis",
                "title": "Caching improves latency",
                "tags": ["performance"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("h1"));
        assert!(result.content.contains("Caching improves latency"));

        let result = tool
            .execute(json!({"action": "list_hypotheses"}))
            .await
            .unwrap();
        assert!(result.content.contains("Caching improves latency"));
        assert!(result.content.contains("Proposed"));
    }

    #[tokio::test]
    async fn test_hypothesis_crud() {
        let (_dir, tool) = make_tool();

        // Add
        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Batch size matters",
            "description": "Larger batches reduce overhead"
        }))
        .await
        .unwrap();

        // Update status
        let result = tool
            .execute(json!({
                "action": "update_hypothesis",
                "id": "h1",
                "status": "testing"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Testing"));

        // Get full detail
        let result = tool
            .execute(json!({"action": "get_hypothesis", "id": "h1"}))
            .await
            .unwrap();
        assert!(result.content.contains("Batch size matters"));
        assert!(result.content.contains("Testing"));
        assert!(result.content.contains("Larger batches reduce overhead"));
    }

    #[tokio::test]
    async fn test_add_experiment() {
        let (_dir, tool) = make_tool();

        // Create hypothesis first
        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Test hyp"
        }))
        .await
        .unwrap();

        // Add experiment linked to hypothesis
        let result = tool
            .execute(json!({
                "action": "add_experiment",
                "name": "Run A",
                "hypothesis_id": "h1",
                "config": {"learning_rate": 0.01}
            }))
            .await
            .unwrap();
        assert!(result.content.contains("e1"));
        assert!(result.content.contains("Run A"));

        // Verify experiment appears in list
        let result = tool
            .execute(json!({"action": "list_experiments"}))
            .await
            .unwrap();
        assert!(result.content.contains("Run A"));
        assert!(result.content.contains("Planned"));
    }

    #[tokio::test]
    async fn test_experiment_lifecycle_complete() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Exp Alpha"
        }))
        .await
        .unwrap();

        // Start
        let result = tool
            .execute(json!({"action": "start_experiment", "id": "e1"}))
            .await
            .unwrap();
        assert!(result.content.contains("running"));

        // Complete
        let result = tool
            .execute(json!({
                "action": "complete_experiment",
                "id": "e1",
                "metrics": {"accuracy": 0.95},
                "notes": "Good results"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("completed"));

        // Verify via get
        let result = tool
            .execute(json!({"action": "get_experiment", "id": "e1"}))
            .await
            .unwrap();
        assert!(result.content.contains("Completed"));
        assert!(result.content.contains("Good results"));
    }

    #[tokio::test]
    async fn test_experiment_lifecycle_fail() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Exp Beta"
        }))
        .await
        .unwrap();

        tool.execute(json!({"action": "start_experiment", "id": "e1"}))
            .await
            .unwrap();

        let result = tool
            .execute(json!({
                "action": "fail_experiment",
                "id": "e1",
                "notes": "OOM error"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("failed"));

        let result = tool
            .execute(json!({"action": "get_experiment", "id": "e1"}))
            .await
            .unwrap();
        assert!(result.content.contains("Failed"));
        assert!(result.content.contains("OOM error"));
    }

    #[tokio::test]
    async fn test_record_evidence() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Evidence test"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Trial 1",
            "hypothesis_id": "h1"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "record_evidence",
                "hypothesis_id": "h1",
                "experiment_id": "e1",
                "finding": "Latency reduced by 40%",
                "supports": true,
                "confidence": 0.85
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Recorded evidence"));
        assert!(result.content.contains("h1"));
        assert!(result.content.contains("0.85"));

        // Verify on get_hypothesis
        let result = tool
            .execute(json!({"action": "get_hypothesis", "id": "h1"}))
            .await
            .unwrap();
        assert!(result.content.contains("Latency reduced by 40%"));
        assert!(result.content.contains("0.85"));
    }

    #[tokio::test]
    async fn test_compare_experiments() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Config A",
            "config": {"batch_size": 32}
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Config B",
            "config": {"batch_size": 64}
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({
                "action": "compare_experiments",
                "ids": ["e1", "e2"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Config A"));
        assert!(result.content.contains("Config B"));
        assert!(result.content.contains("Comparison of 2 experiments"));
    }

    #[tokio::test]
    async fn test_summary_empty() {
        let (_dir, tool) = make_tool();

        let result = tool.execute(json!({"action": "summary"})).await.unwrap();
        assert!(result.content.contains("No data to summarize"));
    }

    #[tokio::test]
    async fn test_summary_with_data() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "H1"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "E1",
            "hypothesis_id": "h1"
        }))
        .await
        .unwrap();

        tool.execute(json!({"action": "start_experiment", "id": "e1"}))
            .await
            .unwrap();

        tool.execute(json!({"action": "complete_experiment", "id": "e1"}))
            .await
            .unwrap();

        tool.execute(json!({
            "action": "record_evidence",
            "hypothesis_id": "h1",
            "experiment_id": "e1",
            "finding": "Positive result",
            "supports": true,
            "confidence": 0.9
        }))
        .await
        .unwrap();

        let result = tool.execute(json!({"action": "summary"})).await.unwrap();
        assert!(result.content.contains("Hypotheses: 1"));
        assert!(result.content.contains("Experiments: 1"));
        assert!(result.content.contains("1 supporting"));
        assert!(result.content.contains("Success rate: 100%"));
    }

    #[tokio::test]
    async fn test_export_markdown() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Cache hypothesis",
            "description": "Caching reduces latency"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Cache test",
            "hypothesis_id": "h1"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "export_markdown"}))
            .await
            .unwrap();
        assert!(result.content.contains("# Experiment Tracker Report"));
        assert!(result.content.contains("Cache hypothesis"));
        assert!(result.content.contains("**Status:** Proposed"));
        assert!(result.content.contains("Cache test"));
    }

    #[tokio::test]
    async fn test_list_hypotheses_filter_status() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Hyp A"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Hyp B"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "update_hypothesis",
            "id": "h2",
            "status": "testing"
        }))
        .await
        .unwrap();

        // Filter by Proposed — only h1
        let result = tool
            .execute(json!({"action": "list_hypotheses", "status": "proposed"}))
            .await
            .unwrap();
        assert!(result.content.contains("Hyp A"));
        assert!(!result.content.contains("Hyp B"));

        // Filter by Testing — only h2
        let result = tool
            .execute(json!({"action": "list_hypotheses", "status": "testing"}))
            .await
            .unwrap();
        assert!(result.content.contains("Hyp B"));
        assert!(!result.content.contains("Hyp A"));
    }

    #[tokio::test]
    async fn test_list_experiments_filter() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "H1"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "H2"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Exp for H1",
            "hypothesis_id": "h1"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Exp for H2",
            "hypothesis_id": "h2"
        }))
        .await
        .unwrap();

        // Filter by hypothesis_id
        let result = tool
            .execute(json!({"action": "list_experiments", "hypothesis_id": "h1"}))
            .await
            .unwrap();
        assert!(result.content.contains("Exp for H1"));
        assert!(!result.content.contains("Exp for H2"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Persist me",
            "tags": ["tag1"]
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Saved exp",
            "hypothesis_id": "h1"
        }))
        .await
        .unwrap();

        // Reload state manually and verify
        let state = tool.load_state();
        assert_eq!(state.hypotheses.len(), 1);
        assert_eq!(state.experiments.len(), 1);
        assert_eq!(state.hypotheses[0].title, "Persist me");
        assert_eq!(state.hypotheses[0].tags, vec!["tag1"]);
        assert_eq!(state.experiments[0].name, "Saved exp");
        assert_eq!(state.experiments[0].hypothesis_id, Some("h1".to_string()));
        assert_eq!(state.next_hypothesis_id, 2);
        assert_eq!(state.next_experiment_id, 2);
    }

    #[tokio::test]
    async fn test_evidence_confidence_clamping() {
        let (_dir, tool) = make_tool();

        tool.execute(json!({
            "action": "add_hypothesis",
            "title": "Clamp test"
        }))
        .await
        .unwrap();

        tool.execute(json!({
            "action": "add_experiment",
            "name": "Clamp exp"
        }))
        .await
        .unwrap();

        // Confidence above 1.0 should clamp to 1.0
        let result = tool
            .execute(json!({
                "action": "record_evidence",
                "hypothesis_id": "h1",
                "experiment_id": "e1",
                "finding": "Over confident",
                "supports": true,
                "confidence": 1.5
            }))
            .await
            .unwrap();
        assert!(result.content.contains("1.00"));

        // Confidence below 0.0 should clamp to 0.0
        let result = tool
            .execute(json!({
                "action": "record_evidence",
                "hypothesis_id": "h1",
                "experiment_id": "e1",
                "finding": "Under confident",
                "supports": false,
                "confidence": -0.5
            }))
            .await
            .unwrap();
        assert!(result.content.contains("0.00"));

        // Verify clamped values in state
        let state = tool.load_state();
        let hyp = &state.hypotheses[0];
        assert_eq!(hyp.evidence.len(), 2);
        assert!((hyp.evidence[0].confidence - 1.0).abs() < f64::EPSILON);
        assert!((hyp.evidence[1].confidence - 0.0).abs() < f64::EPSILON);
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
