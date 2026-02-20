//! Agent decision log for interpretability.
//!
//! Records every agent-level decision (tool selection, approval outcomes,
//! expert routing) with human-readable reasoning, alternatives considered,
//! and outcomes. Addresses the Interpretability pillar by making the agent's
//! reasoning transparent and queryable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Maximum number of decision entries to retain.
const MAX_ENTRIES: usize = 500;

/// A single recorded decision made by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    /// Unique decision ID (monotonically increasing within session).
    pub id: usize,
    /// When the decision was made.
    pub timestamp: DateTime<Utc>,
    /// Agent iteration number when the decision was made.
    pub iteration: usize,
    /// The action that was decided on (tool name, approval, routing, etc.).
    pub action: String,
    /// Human-readable reasoning explaining why this action was chosen.
    pub reasoning: String,
    /// Other options that were considered (if known).
    pub alternatives: Vec<String>,
    /// Risk level assessment (low, medium, high, critical).
    pub risk_level: String,
    /// Confidence in the decision (0.0-1.0), if available.
    pub confidence: Option<f64>,
    /// What happened after the decision.
    pub outcome: DecisionOutcome,
    /// The MoE expert that was active, if any.
    pub expert: Option<String>,
    /// The persona that was active, if any.
    pub persona: Option<String>,
    /// Source of the decision trigger (e.g., "user", "siri", "cron").
    pub source: Option<String>,
}

/// What happened after an agent decision was made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionOutcome {
    /// The action was auto-approved (safe, read-only, etc.).
    AutoApproved,
    /// The user approved the action.
    UserApproved,
    /// The user denied the action.
    UserDenied,
    /// The safety guardian denied the action.
    SafetyDenied { reason: String },
    /// The action is pending (not yet resolved).
    Pending,
    /// The action completed successfully.
    Succeeded,
    /// The action failed with an error.
    Failed { error: String },
}

impl std::fmt::Display for DecisionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionOutcome::AutoApproved => write!(f, "auto-approved"),
            DecisionOutcome::UserApproved => write!(f, "user-approved"),
            DecisionOutcome::UserDenied => write!(f, "user-denied"),
            DecisionOutcome::SafetyDenied { reason } => write!(f, "safety-denied: {reason}"),
            DecisionOutcome::Pending => write!(f, "pending"),
            DecisionOutcome::Succeeded => write!(f, "succeeded"),
            DecisionOutcome::Failed { error } => write!(f, "failed: {error}"),
        }
    }
}

/// A bounded log of agent decisions for interpretability.
///
/// Provides queryable history of what the agent decided, why, and what happened.
/// Used by the `/explain` command and data flow tracker.
pub struct DecisionLog {
    entries: VecDeque<DecisionEntry>,
    next_id: usize,
}

impl DecisionLog {
    /// Create a new empty decision log.
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 0,
        }
    }

    /// Record a new decision.
    pub fn record(
        &mut self,
        iteration: usize,
        action: impl Into<String>,
        reasoning: impl Into<String>,
        risk_level: impl Into<String>,
        outcome: DecisionOutcome,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        let entry = DecisionEntry {
            id,
            timestamp: Utc::now(),
            iteration,
            action: action.into(),
            reasoning: reasoning.into(),
            alternatives: Vec::new(),
            risk_level: risk_level.into(),
            confidence: None,
            outcome,
            expert: None,
            persona: None,
            source: None,
        };

        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);

        id
    }

    /// Record a decision with full metadata.
    pub fn record_full(&mut self, mut entry: DecisionEntry) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        entry.id = id;

        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);

        id
    }

    /// Update the outcome of an existing decision by ID.
    pub fn update_outcome(&mut self, id: usize, outcome: DecisionOutcome) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.outcome = outcome;
        }
    }

    /// Get the most recent N decisions.
    pub fn recent(&self, n: usize) -> Vec<&DecisionEntry> {
        self.entries.iter().rev().take(n).collect()
    }

    /// Get all decisions for a specific iteration.
    pub fn for_iteration(&self, iteration: usize) -> Vec<&DecisionEntry> {
        self.entries
            .iter()
            .filter(|e| e.iteration == iteration)
            .collect()
    }

    /// Get a decision by ID.
    pub fn get(&self, id: usize) -> Option<&DecisionEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Get a mutable reference to a decision by ID.
    pub fn get_mut(&mut self, id: usize) -> Option<&mut DecisionEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Total number of recorded decisions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Format recent decisions as a human-readable string for `/explain`.
    pub fn format_recent(&self, n: usize) -> String {
        let recent = self.recent(n);
        if recent.is_empty() {
            return "No decisions recorded yet.".to_string();
        }

        let mut output = String::new();
        for entry in recent.iter().rev() {
            output.push_str(&format!(
                "[#{} iter={} {}] {} (risk: {})\n",
                entry.id,
                entry.iteration,
                entry.timestamp.format("%H:%M:%S"),
                entry.action,
                entry.risk_level,
            ));
            output.push_str(&format!("  Reasoning: {}\n", entry.reasoning));
            if !entry.alternatives.is_empty() {
                output.push_str(&format!(
                    "  Alternatives: {}\n",
                    entry.alternatives.join(", ")
                ));
            }
            output.push_str(&format!("  Outcome: {}\n", entry.outcome));
            if let Some(ref expert) = entry.expert {
                output.push_str(&format!("  Expert: {expert}\n"));
            }
            output.push('\n');
        }
        output
    }
}

impl Default for DecisionLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_query() {
        let mut log = DecisionLog::new();

        let id = log.record(
            1,
            "file_read",
            "Need to inspect config",
            "low",
            DecisionOutcome::AutoApproved,
        );
        assert_eq!(id, 0);
        assert_eq!(log.len(), 1);

        let entry = log.get(0).unwrap();
        assert_eq!(entry.action, "file_read");
        assert_eq!(entry.iteration, 1);
    }

    #[test]
    fn test_update_outcome() {
        let mut log = DecisionLog::new();
        let id = log.record(
            1,
            "shell_exec",
            "Run tests",
            "medium",
            DecisionOutcome::Pending,
        );

        log.update_outcome(id, DecisionOutcome::Succeeded);

        let entry = log.get(id).unwrap();
        assert!(matches!(entry.outcome, DecisionOutcome::Succeeded));
    }

    #[test]
    fn test_recent() {
        let mut log = DecisionLog::new();
        for i in 0..10 {
            log.record(
                i,
                format!("action_{i}"),
                "reason",
                "low",
                DecisionOutcome::AutoApproved,
            );
        }

        let recent = log.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].action, "action_9");
        assert_eq!(recent[2].action, "action_7");
    }

    #[test]
    fn test_for_iteration() {
        let mut log = DecisionLog::new();
        log.record(
            1,
            "action_a",
            "reason",
            "low",
            DecisionOutcome::AutoApproved,
        );
        log.record(
            1,
            "action_b",
            "reason",
            "low",
            DecisionOutcome::AutoApproved,
        );
        log.record(
            2,
            "action_c",
            "reason",
            "low",
            DecisionOutcome::AutoApproved,
        );

        let iter1 = log.for_iteration(1);
        assert_eq!(iter1.len(), 2);
    }

    #[test]
    fn test_capacity_limit() {
        let mut log = DecisionLog::new();
        for i in 0..600 {
            log.record(
                i,
                format!("action_{i}"),
                "reason",
                "low",
                DecisionOutcome::AutoApproved,
            );
        }
        // Should be capped at MAX_ENTRIES
        assert_eq!(log.len(), MAX_ENTRIES);
    }

    #[test]
    fn test_format_recent() {
        let mut log = DecisionLog::new();
        log.record(
            1,
            "file_read",
            "Inspecting config.toml",
            "low",
            DecisionOutcome::AutoApproved,
        );

        let formatted = log.format_recent(5);
        assert!(formatted.contains("file_read"));
        assert!(formatted.contains("Inspecting config.toml"));
        assert!(formatted.contains("auto-approved"));
    }

    #[test]
    fn test_record_full() {
        let mut log = DecisionLog::new();
        let entry = DecisionEntry {
            id: 0,
            timestamp: Utc::now(),
            iteration: 5,
            action: "deployment".to_string(),
            reasoning: "User requested deploy".to_string(),
            alternatives: vec!["rollback".to_string(), "canary".to_string()],
            risk_level: "high".to_string(),
            confidence: Some(0.85),
            outcome: DecisionOutcome::UserApproved,
            expert: Some("SRE".to_string()),
            persona: Some("IncidentCommander".to_string()),
            source: Some("siri".to_string()),
        };

        let id = log.record_full(entry);
        let retrieved = log.get(id).unwrap();
        assert_eq!(retrieved.alternatives.len(), 2);
        assert_eq!(retrieved.source.as_deref(), Some("siri"));
    }

    #[test]
    fn test_decision_outcome_display() {
        assert_eq!(DecisionOutcome::AutoApproved.to_string(), "auto-approved");
        assert_eq!(
            DecisionOutcome::SafetyDenied {
                reason: "too risky".into()
            }
            .to_string(),
            "safety-denied: too risky"
        );
    }
}
