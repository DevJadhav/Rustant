//! Incident response playbooks — YAML-defined automated response workflows.
//!
//! Defines and executes structured response procedures for security incidents.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An incident response playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playbook {
    /// Playbook identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// When this playbook should trigger.
    pub trigger: PlaybookTrigger,
    /// Steps to execute.
    pub steps: Vec<PlaybookStep>,
    /// Playbook description.
    pub description: String,
    /// Whether this playbook is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Trigger condition for a playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookTrigger {
    /// Scanner type that triggers this playbook.
    pub scanner: Option<String>,
    /// Minimum severity to trigger.
    pub min_severity: Option<String>,
    /// Specific event type to trigger on.
    pub event_type: Option<String>,
    /// Whether live validation confirmed the finding.
    pub live_validation: Option<bool>,
}

/// A step in a playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStep {
    /// Step name.
    pub name: String,
    /// Action to perform.
    pub action: PlaybookAction,
    /// Whether this step requires approval.
    #[serde(default)]
    pub requires_approval: bool,
    /// Timeout in seconds.
    pub timeout_secs: Option<u64>,
    /// Condition for executing this step.
    pub condition: Option<String>,
}

/// Actions that can be taken in a playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaybookAction {
    /// Block an IP address.
    BlockIp { ip: String },
    /// Rotate a secret/credential.
    RotateSecret { secret_id: String },
    /// Invalidate all sessions for a user.
    InvalidateSessions { user_id: String },
    /// Send a notification.
    Notify { channel: String, message: String },
    /// Create an incident ticket.
    CreateTicket { system: String, summary: String },
    /// Quarantine a file.
    QuarantineFile { path: String },
    /// Revoke an API token.
    RevokeToken { token_id: String },
    /// Run a custom command.
    CustomAction {
        name: String,
        params: HashMap<String, String>,
    },
}

/// Result of executing a playbook step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name.
    pub step_name: String,
    /// Whether the step succeeded.
    pub success: bool,
    /// Output message.
    pub message: String,
    /// Time taken in milliseconds.
    pub duration_ms: u64,
    /// Whether approval was obtained (if required).
    pub approval_obtained: Option<bool>,
}

/// Result of executing an entire playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookResult {
    /// Playbook ID.
    pub playbook_id: String,
    /// Individual step results.
    pub step_results: Vec<StepResult>,
    /// Whether all steps succeeded.
    pub all_succeeded: bool,
    /// Steps that failed.
    pub failures: Vec<String>,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
}

/// Playbook registry for storing and matching playbooks.
pub struct PlaybookRegistry {
    playbooks: Vec<Playbook>,
}

impl PlaybookRegistry {
    pub fn new() -> Self {
        Self {
            playbooks: Vec::new(),
        }
    }

    /// Create with default incident response playbooks.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        registry.add(Playbook {
            id: "credential-leak".into(),
            name: "Credential Leak Response".into(),
            description: "Responds to detected credential leaks".into(),
            trigger: PlaybookTrigger {
                scanner: Some("secrets".into()),
                min_severity: Some("critical".into()),
                event_type: None,
                live_validation: Some(true),
            },
            steps: vec![
                PlaybookStep {
                    name: "Rotate compromised credential".into(),
                    action: PlaybookAction::RotateSecret {
                        secret_id: "${detected_secret_id}".into(),
                    },
                    requires_approval: true,
                    timeout_secs: Some(300),
                    condition: None,
                },
                PlaybookStep {
                    name: "Invalidate active sessions".into(),
                    action: PlaybookAction::InvalidateSessions {
                        user_id: "${affected_user}".into(),
                    },
                    requires_approval: false,
                    timeout_secs: Some(60),
                    condition: None,
                },
                PlaybookStep {
                    name: "Notify security team".into(),
                    action: PlaybookAction::Notify {
                        channel: "slack".into(),
                        message: "Credential leak detected and rotated".into(),
                    },
                    requires_approval: false,
                    timeout_secs: Some(30),
                    condition: None,
                },
            ],
            enabled: true,
        });

        registry.add(Playbook {
            id: "brute-force-response".into(),
            name: "Brute Force Response".into(),
            description: "Responds to detected brute force attacks".into(),
            trigger: PlaybookTrigger {
                scanner: None,
                min_severity: Some("high".into()),
                event_type: Some("brute-force-login".into()),
                live_validation: None,
            },
            steps: vec![
                PlaybookStep {
                    name: "Block attacker IP".into(),
                    action: PlaybookAction::BlockIp {
                        ip: "${source_ip}".into(),
                    },
                    requires_approval: true,
                    timeout_secs: Some(60),
                    condition: None,
                },
                PlaybookStep {
                    name: "Create incident ticket".into(),
                    action: PlaybookAction::CreateTicket {
                        system: "jira".into(),
                        summary: "Brute force attack detected".into(),
                    },
                    requires_approval: false,
                    timeout_secs: Some(30),
                    condition: None,
                },
            ],
            enabled: true,
        });

        registry
    }

    /// Add a playbook.
    pub fn add(&mut self, playbook: Playbook) {
        self.playbooks.push(playbook);
    }

    /// Find playbooks matching a trigger condition.
    pub fn find_matching(
        &self,
        scanner: Option<&str>,
        severity: Option<&str>,
        event_type: Option<&str>,
    ) -> Vec<&Playbook> {
        self.playbooks
            .iter()
            .filter(|p| {
                if !p.enabled {
                    return false;
                }

                // Check scanner match
                if let Some(ref required) = p.trigger.scanner {
                    if let Some(actual) = scanner {
                        if required != actual {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }

                // Check severity match
                if let Some(ref min_sev) = p.trigger.min_severity
                    && let Some(actual_sev) = severity
                    && severity_rank(actual_sev) < severity_rank(min_sev)
                {
                    return false;
                }

                // Check event type match
                if let Some(ref required) = p.trigger.event_type {
                    if let Some(actual) = event_type {
                        if required != actual {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    /// Get all playbooks.
    pub fn all(&self) -> &[Playbook] {
        &self.playbooks
    }

    /// Get playbook by ID.
    pub fn get(&self, id: &str) -> Option<&Playbook> {
        self.playbooks.iter().find(|p| p.id == id)
    }
}

impl Default for PlaybookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Map severity string to numeric rank for comparison.
fn severity_rank(severity: &str) -> u8 {
    match severity.to_lowercase().as_str() {
        "critical" => 5,
        "high" => 4,
        "medium" => 3,
        "low" => 2,
        "info" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_playbooks() {
        let registry = PlaybookRegistry::with_defaults();
        assert_eq!(registry.all().len(), 2);
        assert!(registry.get("credential-leak").is_some());
        assert!(registry.get("brute-force-response").is_some());
    }

    #[test]
    fn test_find_matching_scanner() {
        let registry = PlaybookRegistry::with_defaults();
        let matches = registry.find_matching(Some("secrets"), Some("critical"), None);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "credential-leak");
    }

    #[test]
    fn test_find_matching_event_type() {
        let registry = PlaybookRegistry::with_defaults();
        let matches = registry.find_matching(None, Some("high"), Some("brute-force-login"));

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "brute-force-response");
    }

    #[test]
    fn test_no_match_low_severity() {
        let registry = PlaybookRegistry::with_defaults();
        let matches = registry.find_matching(Some("secrets"), Some("low"), None);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_disabled_playbook_not_matched() {
        let mut registry = PlaybookRegistry::new();
        registry.add(Playbook {
            id: "disabled".into(),
            name: "Disabled Playbook".into(),
            description: "Should not match".into(),
            trigger: PlaybookTrigger {
                scanner: Some("sast".into()),
                min_severity: None,
                event_type: None,
                live_validation: None,
            },
            steps: Vec::new(),
            enabled: false,
        });

        let matches = registry.find_matching(Some("sast"), None, None);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_playbook_step_approval() {
        let registry = PlaybookRegistry::with_defaults();
        let playbook = registry.get("credential-leak").unwrap();

        // First step (rotate) requires approval
        assert!(playbook.steps[0].requires_approval);
        // Second step (invalidate sessions) does not
        assert!(!playbook.steps[1].requires_approval);
    }

    #[test]
    fn test_severity_rank() {
        assert!(severity_rank("critical") > severity_rank("high"));
        assert!(severity_rank("high") > severity_rank("medium"));
        assert!(severity_rank("medium") > severity_rank("low"));
        assert!(severity_rank("low") > severity_rank("info"));
    }

    #[test]
    fn test_playbook_result() {
        let result = PlaybookResult {
            playbook_id: "test".into(),
            step_results: vec![
                StepResult {
                    step_name: "step1".into(),
                    success: true,
                    message: "OK".into(),
                    duration_ms: 100,
                    approval_obtained: None,
                },
                StepResult {
                    step_name: "step2".into(),
                    success: false,
                    message: "Failed".into(),
                    duration_ms: 200,
                    approval_obtained: Some(true),
                },
            ],
            all_succeeded: false,
            failures: vec!["step2".into()],
            total_duration_ms: 300,
        };

        assert!(!result.all_succeeded);
        assert_eq!(result.failures.len(), 1);
    }
}
