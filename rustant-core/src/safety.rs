//! Safety Guardian — enforces safety policies at every execution boundary.
//!
//! Implements a multi-layer defense model:
//! 1. Input validation
//! 2. Authorization (path/command restrictions)
//! 3. Sandbox execution decisions
//! 4. Output validation
//! 5. Audit logging

use crate::config::{ApprovalMode, SafetyConfig};
use crate::types::RiskLevel;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Result of a permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResult {
    Allowed,
    Denied { reason: String },
    RequiresApproval { context: String },
}

/// An action that the agent wants to perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub id: Uuid,
    pub tool_name: String,
    pub risk_level: RiskLevel,
    pub description: String,
    pub details: ActionDetails,
    pub timestamp: DateTime<Utc>,
}

/// Details specific to the type of action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionDetails {
    FileRead { path: PathBuf },
    FileWrite { path: PathBuf, size_bytes: usize },
    FileDelete { path: PathBuf },
    ShellCommand { command: String },
    NetworkRequest { host: String, method: String },
    GitOperation { operation: String },
    Other { info: String },
}

/// An entry in the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub session_id: Uuid,
    pub event: AuditEvent,
}

/// Types of events that can be audited.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEvent {
    ActionRequested {
        tool: String,
        risk_level: RiskLevel,
        description: String,
    },
    ActionApproved {
        tool: String,
    },
    ActionDenied {
        tool: String,
        reason: String,
    },
    ActionExecuted {
        tool: String,
        success: bool,
        duration_ms: u64,
    },
    ApprovalRequested {
        tool: String,
        context: String,
    },
    ApprovalDecision {
        tool: String,
        approved: bool,
    },
}

/// The Safety Guardian enforcing all safety policies.
pub struct SafetyGuardian {
    config: SafetyConfig,
    session_id: Uuid,
    audit_log: VecDeque<AuditEntry>,
    max_audit_entries: usize,
}

impl SafetyGuardian {
    pub fn new(config: SafetyConfig) -> Self {
        Self {
            config,
            session_id: Uuid::new_v4(),
            audit_log: VecDeque::new(),
            max_audit_entries: 10_000,
        }
    }

    /// Check whether an action is permitted under current safety policy.
    pub fn check_permission(&mut self, action: &ActionRequest) -> PermissionResult {
        // Layer 1: Check denied patterns first (always denied regardless of mode)
        if let Some(reason) = self.check_denied(action) {
            self.log_event(AuditEvent::ActionDenied {
                tool: action.tool_name.clone(),
                reason: reason.clone(),
            });
            return PermissionResult::Denied { reason };
        }

        // Layer 2: Check based on approval mode and risk level
        let result = match self.config.approval_mode {
            ApprovalMode::Yolo => PermissionResult::Allowed,
            ApprovalMode::Safe => self.check_safe_mode(action),
            ApprovalMode::Cautious => self.check_cautious_mode(action),
            ApprovalMode::Paranoid => PermissionResult::RequiresApproval {
                context: format!(
                    "{} (risk: {}) — paranoid mode requires approval for all actions",
                    action.description, action.risk_level
                ),
            },
        };

        // Log the result
        match &result {
            PermissionResult::Allowed => {
                self.log_event(AuditEvent::ActionApproved {
                    tool: action.tool_name.clone(),
                });
            }
            PermissionResult::Denied { reason } => {
                self.log_event(AuditEvent::ActionDenied {
                    tool: action.tool_name.clone(),
                    reason: reason.clone(),
                });
            }
            PermissionResult::RequiresApproval { context } => {
                self.log_event(AuditEvent::ApprovalRequested {
                    tool: action.tool_name.clone(),
                    context: context.clone(),
                });
            }
        }

        result
    }

    /// Safe mode: only read-only operations are auto-approved.
    fn check_safe_mode(&self, action: &ActionRequest) -> PermissionResult {
        match action.risk_level {
            RiskLevel::ReadOnly => PermissionResult::Allowed,
            _ => PermissionResult::RequiresApproval {
                context: format!(
                    "{} (risk: {}) — safe mode requires approval for non-read operations",
                    action.description, action.risk_level
                ),
            },
        }
    }

    /// Cautious mode: read-only and reversible writes are auto-approved.
    fn check_cautious_mode(&self, action: &ActionRequest) -> PermissionResult {
        match action.risk_level {
            RiskLevel::ReadOnly | RiskLevel::Write => PermissionResult::Allowed,
            _ => PermissionResult::RequiresApproval {
                context: format!(
                    "{} (risk: {}) — cautious mode requires approval for execute/network/destructive operations",
                    action.description, action.risk_level
                ),
            },
        }
    }

    /// Check explicitly denied patterns.
    fn check_denied(&self, action: &ActionRequest) -> Option<String> {
        match &action.details {
            ActionDetails::FileRead { path }
            | ActionDetails::FileWrite { path, .. }
            | ActionDetails::FileDelete { path } => self.check_path_denied(path),
            ActionDetails::ShellCommand { command } => self.check_command_denied(command),
            ActionDetails::NetworkRequest { host, .. } => self.check_host_denied(host),
            _ => None,
        }
    }

    /// Check if a file path is denied.
    fn check_path_denied(&self, path: &Path) -> Option<String> {
        let path_str = path.to_string_lossy();
        for pattern in &self.config.denied_paths {
            if Self::glob_matches(pattern, &path_str) {
                return Some(format!(
                    "Path '{}' matches denied pattern '{}'",
                    path_str, pattern
                ));
            }
        }
        None
    }

    /// Check if a command is denied.
    fn check_command_denied(&self, command: &str) -> Option<String> {
        let cmd_lower = command.to_lowercase();
        for denied in &self.config.denied_commands {
            if cmd_lower.starts_with(&denied.to_lowercase())
                || cmd_lower.contains(&denied.to_lowercase())
            {
                return Some(format!(
                    "Command '{}' matches denied pattern '{}'",
                    command, denied
                ));
            }
        }
        None
    }

    /// Check if a host is denied (not in allowlist).
    fn check_host_denied(&self, host: &str) -> Option<String> {
        if self.config.allowed_hosts.is_empty() {
            return None; // No allowlist means all allowed
        }
        if !self.config.allowed_hosts.iter().any(|h| h == host) {
            return Some(format!("Host '{}' not in allowed hosts list", host));
        }
        None
    }

    /// Simple glob matching for path patterns.
    /// Supports: `**`, `**/suffix`, `prefix/**`, `**/*.ext`, `**/dir/**`, `*.ext`, `prefix*`
    fn glob_matches(pattern: &str, path: &str) -> bool {
        if pattern == "**" {
            return true;
        }

        // Pattern: **/dir/** — matches any path containing the dir segment
        if pattern.starts_with("**/") && pattern.ends_with("/**") {
            let middle = &pattern[3..pattern.len() - 3];
            let segment = format!("/{}/", middle);
            let starts_with = format!("{}/", middle);
            return path.contains(&segment) || path.starts_with(&starts_with) || path == middle;
        }

        // Pattern: **/*.ext — matches any file with that extension anywhere
        if let Some(suffix) = pattern.strip_prefix("**/") {
            if suffix.starts_with("*.") {
                // Extension match: **/*.key means any path ending with .key
                let ext = &suffix[1..]; // ".key"
                return path.ends_with(ext);
            }
            // Direct suffix match: **/foo matches any path ending in /foo or equal to foo
            return path.ends_with(suffix)
                || path.ends_with(&format!("/{}", suffix))
                || path == suffix;
        }

        // Pattern: prefix/** — matches anything under prefix/
        if let Some(prefix) = pattern.strip_suffix("/**") {
            return path.starts_with(prefix) && path.len() > prefix.len();
        }

        // Pattern: *.ext — matches files with that extension (in current dir)
        if pattern.starts_with("*.") {
            let ext = &pattern[1..]; // ".ext"
            return path.ends_with(ext);
        }

        // Pattern: prefix* — matches anything starting with prefix
        if let Some(prefix) = pattern.strip_suffix("*") {
            return path.starts_with(prefix);
        }

        // Direct match
        path == pattern || path.ends_with(pattern)
    }

    /// Record an event in the audit log.
    fn log_event(&mut self, event: AuditEvent) {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            session_id: self.session_id,
            event,
        };
        self.audit_log.push_back(entry);
        if self.audit_log.len() > self.max_audit_entries {
            self.audit_log.pop_front();
        }
    }

    /// Record the result of an action execution.
    pub fn log_execution(&mut self, tool: &str, success: bool, duration_ms: u64) {
        self.log_event(AuditEvent::ActionExecuted {
            tool: tool.to_string(),
            success,
            duration_ms,
        });
    }

    /// Record a user approval decision.
    pub fn log_approval_decision(&mut self, tool: &str, approved: bool) {
        self.log_event(AuditEvent::ApprovalDecision {
            tool: tool.to_string(),
            approved,
        });
    }

    /// Get the audit log entries.
    pub fn audit_log(&self) -> &VecDeque<AuditEntry> {
        &self.audit_log
    }

    /// Get the session ID.
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Get the current approval mode.
    pub fn approval_mode(&self) -> ApprovalMode {
        self.config.approval_mode
    }

    /// Get the maximum iterations allowed.
    pub fn max_iterations(&self) -> usize {
        self.config.max_iterations
    }

    /// Create an action request helper.
    pub fn create_action_request(
        tool_name: impl Into<String>,
        risk_level: RiskLevel,
        description: impl Into<String>,
        details: ActionDetails,
    ) -> ActionRequest {
        ActionRequest {
            id: Uuid::new_v4(),
            tool_name: tool_name.into(),
            risk_level,
            description: description.into(),
            details,
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SafetyConfig;

    fn default_guardian() -> SafetyGuardian {
        SafetyGuardian::new(SafetyConfig::default())
    }

    fn make_action(tool: &str, risk: RiskLevel, details: ActionDetails) -> ActionRequest {
        SafetyGuardian::create_action_request(tool, risk, format!("{} action", tool), details)
    }

    #[test]
    fn test_safe_mode_allows_read_only() {
        let mut guardian = default_guardian();
        let action = make_action(
            "file_read",
            RiskLevel::ReadOnly,
            ActionDetails::FileRead {
                path: "src/main.rs".into(),
            },
        );
        assert_eq!(
            guardian.check_permission(&action),
            PermissionResult::Allowed
        );
    }

    #[test]
    fn test_safe_mode_requires_approval_for_writes() {
        let mut guardian = default_guardian();
        let action = make_action(
            "file_write",
            RiskLevel::Write,
            ActionDetails::FileWrite {
                path: "src/main.rs".into(),
                size_bytes: 100,
            },
        );
        assert!(matches!(
            guardian.check_permission(&action),
            PermissionResult::RequiresApproval { .. }
        ));
    }

    #[test]
    fn test_cautious_mode_allows_writes() {
        let config = SafetyConfig {
            approval_mode: ApprovalMode::Cautious,
            ..SafetyConfig::default()
        };
        let mut guardian = SafetyGuardian::new(config);

        let action = make_action(
            "file_write",
            RiskLevel::Write,
            ActionDetails::FileWrite {
                path: "src/main.rs".into(),
                size_bytes: 100,
            },
        );
        assert_eq!(
            guardian.check_permission(&action),
            PermissionResult::Allowed
        );
    }

    #[test]
    fn test_cautious_mode_requires_approval_for_execute() {
        let config = SafetyConfig {
            approval_mode: ApprovalMode::Cautious,
            ..SafetyConfig::default()
        };
        let mut guardian = SafetyGuardian::new(config);

        let action = make_action(
            "shell_exec",
            RiskLevel::Execute,
            ActionDetails::ShellCommand {
                command: "cargo test".into(),
            },
        );
        assert!(matches!(
            guardian.check_permission(&action),
            PermissionResult::RequiresApproval { .. }
        ));
    }

    #[test]
    fn test_paranoid_mode_requires_approval_for_everything() {
        let config = SafetyConfig {
            approval_mode: ApprovalMode::Paranoid,
            ..SafetyConfig::default()
        };
        let mut guardian = SafetyGuardian::new(config);

        let action = make_action(
            "file_read",
            RiskLevel::ReadOnly,
            ActionDetails::FileRead {
                path: "src/main.rs".into(),
            },
        );
        assert!(matches!(
            guardian.check_permission(&action),
            PermissionResult::RequiresApproval { .. }
        ));
    }

    #[test]
    fn test_yolo_mode_allows_everything() {
        let config = SafetyConfig {
            approval_mode: ApprovalMode::Yolo,
            ..SafetyConfig::default()
        };
        let mut guardian = SafetyGuardian::new(config);

        let action = make_action(
            "file_delete",
            RiskLevel::Destructive,
            ActionDetails::FileDelete {
                path: "important.rs".into(),
            },
        );
        assert_eq!(
            guardian.check_permission(&action),
            PermissionResult::Allowed
        );
    }

    #[test]
    fn test_denied_path_always_denied() {
        let mut guardian = default_guardian();
        // .env* is in the default denied_paths
        let action = make_action(
            "file_read",
            RiskLevel::ReadOnly,
            ActionDetails::FileRead {
                path: ".env.local".into(),
            },
        );
        assert!(matches!(
            guardian.check_permission(&action),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn test_denied_path_secrets() {
        let mut guardian = default_guardian();
        let action = make_action(
            "file_read",
            RiskLevel::ReadOnly,
            ActionDetails::FileRead {
                path: "config/secrets/api.key".into(),
            },
        );
        assert!(matches!(
            guardian.check_permission(&action),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn test_denied_command() {
        let mut guardian = default_guardian();
        let action = make_action(
            "shell_exec",
            RiskLevel::Execute,
            ActionDetails::ShellCommand {
                command: "sudo rm -rf /".into(),
            },
        );
        assert!(matches!(
            guardian.check_permission(&action),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn test_denied_host() {
        let mut guardian = default_guardian();
        let action = make_action(
            "http_fetch",
            RiskLevel::Network,
            ActionDetails::NetworkRequest {
                host: "evil.example.com".into(),
                method: "GET".into(),
            },
        );
        assert!(matches!(
            guardian.check_permission(&action),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn test_allowed_host() {
        let config = SafetyConfig {
            approval_mode: ApprovalMode::Yolo,
            ..SafetyConfig::default()
        };
        let mut guardian = SafetyGuardian::new(config);

        let action = make_action(
            "http_fetch",
            RiskLevel::Network,
            ActionDetails::NetworkRequest {
                host: "api.github.com".into(),
                method: "GET".into(),
            },
        );
        assert_eq!(
            guardian.check_permission(&action),
            PermissionResult::Allowed
        );
    }

    #[test]
    fn test_audit_log_records_events() {
        let mut guardian = default_guardian();

        let action = make_action(
            "file_read",
            RiskLevel::ReadOnly,
            ActionDetails::FileRead {
                path: "src/main.rs".into(),
            },
        );
        guardian.check_permission(&action);

        assert!(!guardian.audit_log().is_empty());
        let entry = &guardian.audit_log()[0];
        assert!(matches!(&entry.event, AuditEvent::ActionApproved { tool } if tool == "file_read"));
    }

    #[test]
    fn test_audit_log_denied_event() {
        let mut guardian = default_guardian();

        let action = make_action(
            "file_read",
            RiskLevel::ReadOnly,
            ActionDetails::FileRead {
                path: ".env".into(),
            },
        );
        guardian.check_permission(&action);

        let entry = &guardian.audit_log()[0];
        assert!(matches!(&entry.event, AuditEvent::ActionDenied { .. }));
    }

    #[test]
    fn test_log_execution() {
        let mut guardian = default_guardian();
        guardian.log_execution("file_read", true, 42);

        let entry = guardian.audit_log().back().unwrap();
        match &entry.event {
            AuditEvent::ActionExecuted {
                tool,
                success,
                duration_ms,
            } => {
                assert_eq!(tool, "file_read");
                assert!(success);
                assert_eq!(*duration_ms, 42);
            }
            _ => panic!("Expected ActionExecuted event"),
        }
    }

    #[test]
    fn test_log_approval_decision() {
        let mut guardian = default_guardian();
        guardian.log_approval_decision("shell_exec", true);

        let entry = guardian.audit_log().back().unwrap();
        match &entry.event {
            AuditEvent::ApprovalDecision { tool, approved } => {
                assert_eq!(tool, "shell_exec");
                assert!(approved);
            }
            _ => panic!("Expected ApprovalDecision event"),
        }
    }

    #[test]
    fn test_audit_log_capacity() {
        let config = SafetyConfig {
            approval_mode: ApprovalMode::Yolo,
            ..SafetyConfig::default()
        };
        let mut guardian = SafetyGuardian::new(config);
        guardian.max_audit_entries = 5;

        for i in 0..10 {
            guardian.log_execution(&format!("tool_{}", i), true, 1);
        }

        assert_eq!(guardian.audit_log().len(), 5);
    }

    #[test]
    fn test_glob_matches() {
        assert!(SafetyGuardian::glob_matches(".env*", ".env"));
        assert!(SafetyGuardian::glob_matches(".env*", ".env.local"));
        assert!(SafetyGuardian::glob_matches(
            "**/*.key",
            "path/to/secret.key"
        ));
        assert!(SafetyGuardian::glob_matches(
            "**/secrets/**",
            "config/secrets/api.key"
        ));
        assert!(SafetyGuardian::glob_matches("src/**", "src/main.rs"));
        assert!(SafetyGuardian::glob_matches("*.rs", "main.rs"));
        assert!(!SafetyGuardian::glob_matches(".env*", "config.toml"));
    }

    #[test]
    fn test_create_action_request() {
        let action = SafetyGuardian::create_action_request(
            "file_read",
            RiskLevel::ReadOnly,
            "Reading source file",
            ActionDetails::FileRead {
                path: "src/lib.rs".into(),
            },
        );
        assert_eq!(action.tool_name, "file_read");
        assert_eq!(action.risk_level, RiskLevel::ReadOnly);
        assert_eq!(action.description, "Reading source file");
    }

    #[test]
    fn test_session_id_is_set() {
        let guardian = default_guardian();
        let id = guardian.session_id();
        // UUID v4 should be non-nil
        assert!(!id.is_nil());
    }

    #[test]
    fn test_max_iterations() {
        let guardian = default_guardian();
        assert_eq!(guardian.max_iterations(), 25);
    }

    #[test]
    fn test_empty_host_allowlist_allows_all() {
        let config = SafetyConfig {
            allowed_hosts: vec![], // empty = no restriction
            approval_mode: ApprovalMode::Yolo,
            ..SafetyConfig::default()
        };
        let mut guardian = SafetyGuardian::new(config);

        let action = make_action(
            "http_fetch",
            RiskLevel::Network,
            ActionDetails::NetworkRequest {
                host: "any.host.com".into(),
                method: "GET".into(),
            },
        );
        assert_eq!(
            guardian.check_permission(&action),
            PermissionResult::Allowed
        );
    }
}
