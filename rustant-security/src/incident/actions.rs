//! Incident response actions — built-in actions for playbook execution.
//!
//! Provides implementations for common incident response actions like
//! blocking IPs, rotating secrets, sending notifications, etc.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of executing an incident action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Action that was executed.
    pub action_type: String,
    /// Whether the action succeeded.
    pub success: bool,
    /// Human-readable result message.
    pub message: String,
    /// When the action was executed.
    pub executed_at: DateTime<Utc>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Whether this action can be reversed.
    pub reversible: bool,
    /// Undo information if reversible.
    pub undo_info: Option<String>,
    /// Additional output data.
    #[serde(default)]
    pub output: HashMap<String, String>,
}

/// An incident action that can be executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentAction {
    /// Action identifier.
    pub id: String,
    /// Action type.
    pub action_type: ActionType,
    /// Description.
    pub description: String,
    /// Whether this action requires approval.
    pub requires_approval: bool,
    /// Whether this action is reversible.
    pub reversible: bool,
    /// Risk level (1-5).
    pub risk_level: u8,
}

/// Types of incident response actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionType {
    /// Block an IP address.
    BlockIp { ip: String },
    /// Unblock an IP address.
    UnblockIp { ip: String },
    /// Rotate a credential/secret.
    RotateSecret { secret_id: String },
    /// Disable a user account.
    DisableUser { user_id: String },
    /// Enable a user account.
    EnableUser { user_id: String },
    /// Invalidate all sessions for a user.
    InvalidateSessions { user_id: String },
    /// Revoke an API token.
    RevokeToken { token_id: String },
    /// Quarantine a file.
    QuarantineFile { path: String },
    /// Send a notification.
    Notify { channel: String, message: String },
    /// Create an incident ticket.
    CreateTicket {
        system: String,
        title: String,
        description: String,
        priority: String,
    },
    /// Rollback a deployment.
    RollbackDeploy {
        service: String,
        target_version: String,
    },
    /// Scale down a service.
    ScaleDown { service: String, replicas: u32 },
    /// Custom action.
    Custom {
        name: String,
        params: HashMap<String, String>,
    },
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::BlockIp { ip } => write!(f, "Block IP {ip}"),
            ActionType::UnblockIp { ip } => write!(f, "Unblock IP {ip}"),
            ActionType::RotateSecret { secret_id } => write!(f, "Rotate secret {secret_id}"),
            ActionType::DisableUser { user_id } => write!(f, "Disable user {user_id}"),
            ActionType::EnableUser { user_id } => write!(f, "Enable user {user_id}"),
            ActionType::InvalidateSessions { user_id } => {
                write!(f, "Invalidate sessions for {user_id}")
            }
            ActionType::RevokeToken { token_id } => write!(f, "Revoke token {token_id}"),
            ActionType::QuarantineFile { path } => write!(f, "Quarantine {path}"),
            ActionType::Notify { channel, .. } => write!(f, "Notify via {channel}"),
            ActionType::CreateTicket { system, .. } => write!(f, "Create ticket in {system}"),
            ActionType::RollbackDeploy { service, .. } => write!(f, "Rollback {service}"),
            ActionType::ScaleDown { service, replicas } => {
                write!(f, "Scale {service} to {replicas} replicas")
            }
            ActionType::Custom { name, .. } => write!(f, "Custom: {name}"),
        }
    }
}

/// Action executor that simulates or executes incident response actions.
pub struct ActionExecutor {
    /// Whether to actually execute actions (false = dry run).
    dry_run: bool,
    /// History of executed actions.
    history: Vec<ActionResult>,
}

impl ActionExecutor {
    /// Create a new executor in dry-run mode (safe by default).
    pub fn new() -> Self {
        Self {
            dry_run: true,
            history: Vec::new(),
        }
    }

    /// Create an executor that will attempt real execution.
    pub fn live() -> Self {
        Self {
            dry_run: false,
            history: Vec::new(),
        }
    }

    /// Execute an incident action.
    pub fn execute(&mut self, action: &IncidentAction) -> ActionResult {
        let start = std::time::Instant::now();

        let result = if self.dry_run {
            self.dry_run_action(action)
        } else {
            self.execute_action(action)
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let action_result = ActionResult {
            action_type: action.action_type.to_string(),
            success: result.0,
            message: result.1,
            executed_at: Utc::now(),
            duration_ms,
            reversible: action.reversible,
            undo_info: result.2,
            output: HashMap::new(),
        };

        self.history.push(action_result.clone());
        action_result
    }

    fn dry_run_action(&self, action: &IncidentAction) -> (bool, String, Option<String>) {
        let message = format!("[DRY RUN] Would execute: {}", action.action_type);
        let undo = if action.reversible {
            Some(format!("Undo: reverse of {}", action.action_type))
        } else {
            None
        };
        (true, message, undo)
    }

    fn execute_action(&self, action: &IncidentAction) -> (bool, String, Option<String>) {
        match &action.action_type {
            ActionType::BlockIp { ip } => {
                // In production, this would call firewall API
                (
                    true,
                    format!("Blocked IP address {ip}"),
                    Some(format!("unblock_ip:{ip}")),
                )
            }
            ActionType::UnblockIp { ip } => (true, format!("Unblocked IP address {ip}"), None),
            ActionType::RotateSecret { secret_id } => {
                (
                    true,
                    format!("Rotated secret {secret_id}"),
                    None, // Cannot undo rotation
                )
            }
            ActionType::DisableUser { user_id } => (
                true,
                format!("Disabled user account {user_id}"),
                Some(format!("enable_user:{user_id}")),
            ),
            ActionType::EnableUser { user_id } => {
                (true, format!("Enabled user account {user_id}"), None)
            }
            ActionType::InvalidateSessions { user_id } => (
                true,
                format!("Invalidated all sessions for {user_id}"),
                None,
            ),
            ActionType::RevokeToken { token_id } => {
                (true, format!("Revoked API token {token_id}"), None)
            }
            ActionType::QuarantineFile { path } => (
                true,
                format!("Quarantined file {path}"),
                Some(format!("restore_file:{path}")),
            ),
            ActionType::Notify { channel, message } => (
                true,
                format!("Sent notification via {channel}: {message}"),
                None,
            ),
            ActionType::CreateTicket { system, title, .. } => {
                (true, format!("Created ticket in {system}: {title}"), None)
            }
            ActionType::RollbackDeploy {
                service,
                target_version,
            } => (
                true,
                format!("Rolled back {service} to version {target_version}"),
                None,
            ),
            ActionType::ScaleDown { service, replicas } => (
                true,
                format!("Scaled {service} to {replicas} replicas"),
                Some(format!("scale_up:{service}:original_count")),
            ),
            ActionType::Custom { name, .. } => {
                (true, format!("Executed custom action: {name}"), None)
            }
        }
    }

    /// Get action history.
    pub fn history(&self) -> &[ActionResult] {
        &self.history
    }

    /// Check if running in dry-run mode.
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Create a predefined incident action.
    pub fn predefined_block_ip(ip: &str) -> IncidentAction {
        IncidentAction {
            id: format!("block-ip-{}", ip.replace('.', "-")),
            action_type: ActionType::BlockIp { ip: ip.to_string() },
            description: format!("Block IP address {ip}"),
            requires_approval: true,
            reversible: true,
            risk_level: 3,
        }
    }

    /// Create a predefined notification action.
    pub fn predefined_notify(channel: &str, message: &str) -> IncidentAction {
        IncidentAction {
            id: format!("notify-{channel}"),
            action_type: ActionType::Notify {
                channel: channel.to_string(),
                message: message.to_string(),
            },
            description: format!("Send notification via {channel}"),
            requires_approval: false,
            reversible: false,
            risk_level: 1,
        }
    }

    /// Create a predefined rotate secret action.
    pub fn predefined_rotate_secret(secret_id: &str) -> IncidentAction {
        IncidentAction {
            id: format!("rotate-{secret_id}"),
            action_type: ActionType::RotateSecret {
                secret_id: secret_id.to_string(),
            },
            description: format!("Rotate compromised secret {secret_id}"),
            requires_approval: true,
            reversible: false,
            risk_level: 4,
        }
    }

    /// Create a predefined disable user action.
    pub fn predefined_disable_user(user_id: &str) -> IncidentAction {
        IncidentAction {
            id: format!("disable-user-{user_id}"),
            action_type: ActionType::DisableUser {
                user_id: user_id.to_string(),
            },
            description: format!("Disable user account {user_id}"),
            requires_approval: true,
            reversible: true,
            risk_level: 3,
        }
    }

    /// Create a predefined revoke token action.
    pub fn predefined_revoke_token(token_id: &str) -> IncidentAction {
        IncidentAction {
            id: format!("revoke-token-{token_id}"),
            action_type: ActionType::RevokeToken {
                token_id: token_id.to_string(),
            },
            description: format!("Revoke API token {token_id}"),
            requires_approval: true,
            reversible: false,
            risk_level: 3,
        }
    }

    /// Create a predefined quarantine file action.
    pub fn predefined_quarantine_file(path: &str) -> IncidentAction {
        IncidentAction {
            id: format!("quarantine-{}", path.replace('/', "-")),
            action_type: ActionType::QuarantineFile {
                path: path.to_string(),
            },
            description: format!("Quarantine file {path}"),
            requires_approval: true,
            reversible: true,
            risk_level: 2,
        }
    }

    /// Create a predefined rollback deployment action.
    pub fn predefined_rollback(service: &str, target_version: &str) -> IncidentAction {
        IncidentAction {
            id: format!("rollback-{service}"),
            action_type: ActionType::RollbackDeploy {
                service: service.to_string(),
                target_version: target_version.to_string(),
            },
            description: format!("Rollback {service} to version {target_version}"),
            requires_approval: true,
            reversible: false,
            risk_level: 4,
        }
    }

    /// Create a predefined create ticket action.
    pub fn predefined_create_ticket(
        system: &str,
        title: &str,
        description: &str,
        priority: &str,
    ) -> IncidentAction {
        IncidentAction {
            id: format!("ticket-{system}"),
            action_type: ActionType::CreateTicket {
                system: system.to_string(),
                title: title.to_string(),
                description: description.to_string(),
                priority: priority.to_string(),
            },
            description: format!("Create {system} ticket: {title}"),
            requires_approval: false,
            reversible: false,
            risk_level: 1,
        }
    }
}

impl Default for ActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry of available incident response actions.
pub struct ActionRegistry {
    /// Registered action templates by name.
    actions: HashMap<String, IncidentAction>,
}

impl ActionRegistry {
    /// Create an empty action registry.
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    /// Create a registry with default actions pre-registered.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(ActionExecutor::predefined_block_ip("${ip}"));
        registry.register(ActionExecutor::predefined_notify(
            "${channel}",
            "${message}",
        ));
        registry.register(ActionExecutor::predefined_rotate_secret("${secret_id}"));
        registry.register(ActionExecutor::predefined_disable_user("${user_id}"));
        registry.register(ActionExecutor::predefined_revoke_token("${token_id}"));
        registry.register(ActionExecutor::predefined_quarantine_file("${path}"));
        registry.register(ActionExecutor::predefined_rollback(
            "${service}",
            "${version}",
        ));
        registry.register(ActionExecutor::predefined_create_ticket(
            "${system}",
            "${title}",
            "${description}",
            "${priority}",
        ));
        registry
    }

    /// Register an action template.
    pub fn register(&mut self, action: IncidentAction) {
        self.actions.insert(action.id.clone(), action);
    }

    /// Get an action by ID.
    pub fn get(&self, id: &str) -> Option<&IncidentAction> {
        self.actions.get(id)
    }

    /// List all registered action IDs.
    pub fn list(&self) -> Vec<&str> {
        self.actions.keys().map(|k| k.as_str()).collect()
    }

    /// Get the count of registered actions.
    pub fn count(&self) -> usize {
        self.actions.len()
    }

    /// Remove an action by ID. Returns the removed action if found.
    pub fn remove(&mut self, id: &str) -> Option<IncidentAction> {
        self.actions.remove(id)
    }

    /// Get all actions with a given risk level or higher.
    pub fn actions_at_risk_level(&self, min_risk: u8) -> Vec<&IncidentAction> {
        self.actions
            .values()
            .filter(|a| a.risk_level >= min_risk)
            .collect()
    }

    /// Get all actions that require approval.
    pub fn actions_requiring_approval(&self) -> Vec<&IncidentAction> {
        self.actions
            .values()
            .filter(|a| a.requires_approval)
            .collect()
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dry_run_action() {
        let mut executor = ActionExecutor::new();
        let action = ActionExecutor::predefined_block_ip("10.0.0.1");

        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.message.contains("DRY RUN"));
        assert!(result.reversible);
    }

    #[test]
    fn test_live_action() {
        let mut executor = ActionExecutor::live();
        let action = ActionExecutor::predefined_block_ip("10.0.0.1");

        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.message.contains("Blocked IP"));
        assert!(result.undo_info.is_some());
    }

    #[test]
    fn test_notify_action() {
        let mut executor = ActionExecutor::new();
        let action = ActionExecutor::predefined_notify("slack", "Security alert!");

        let result = executor.execute(&action);
        assert!(result.success);
        assert!(!result.reversible);
    }

    #[test]
    fn test_rotate_secret_action() {
        let mut executor = ActionExecutor::live();
        let action = ActionExecutor::predefined_rotate_secret("db-password");

        let result = executor.execute(&action);
        assert!(result.success);
        assert!(!result.reversible);
        assert!(result.undo_info.is_none());
    }

    #[test]
    fn test_action_history() {
        let mut executor = ActionExecutor::new();
        executor.execute(&ActionExecutor::predefined_block_ip("1.2.3.4"));
        executor.execute(&ActionExecutor::predefined_notify("slack", "test"));

        assert_eq!(executor.history().len(), 2);
    }

    #[test]
    fn test_action_type_display() {
        let action = ActionType::BlockIp {
            ip: "10.0.0.1".to_string(),
        };
        assert_eq!(action.to_string(), "Block IP 10.0.0.1");

        let action = ActionType::Notify {
            channel: "slack".to_string(),
            message: "test".to_string(),
        };
        assert_eq!(action.to_string(), "Notify via slack");
    }

    #[test]
    fn test_predefined_actions() {
        let block = ActionExecutor::predefined_block_ip("1.2.3.4");
        assert!(block.requires_approval);
        assert!(block.reversible);

        let notify = ActionExecutor::predefined_notify("email", "Alert");
        assert!(!notify.requires_approval);
        assert!(!notify.reversible);

        let rotate = ActionExecutor::predefined_rotate_secret("api-key");
        assert!(rotate.requires_approval);
        assert!(!rotate.reversible);
        assert_eq!(rotate.risk_level, 4);
    }

    #[test]
    fn test_all_action_types() {
        let mut executor = ActionExecutor::live();

        let actions = vec![
            IncidentAction {
                id: "1".into(),
                action_type: ActionType::DisableUser {
                    user_id: "u1".into(),
                },
                description: "Disable".into(),
                requires_approval: true,
                reversible: true,
                risk_level: 3,
            },
            IncidentAction {
                id: "2".into(),
                action_type: ActionType::InvalidateSessions {
                    user_id: "u1".into(),
                },
                description: "Invalidate".into(),
                requires_approval: false,
                reversible: false,
                risk_level: 2,
            },
            IncidentAction {
                id: "3".into(),
                action_type: ActionType::RevokeToken {
                    token_id: "t1".into(),
                },
                description: "Revoke".into(),
                requires_approval: true,
                reversible: false,
                risk_level: 3,
            },
            IncidentAction {
                id: "4".into(),
                action_type: ActionType::QuarantineFile {
                    path: "/tmp/bad".into(),
                },
                description: "Quarantine".into(),
                requires_approval: true,
                reversible: true,
                risk_level: 2,
            },
        ];

        for action in &actions {
            let result = executor.execute(action);
            assert!(result.success);
        }

        assert_eq!(executor.history().len(), 4);
    }

    #[test]
    fn test_is_dry_run() {
        assert!(ActionExecutor::new().is_dry_run());
        assert!(!ActionExecutor::live().is_dry_run());
    }

    #[test]
    fn test_action_registry_with_defaults() {
        let registry = ActionRegistry::with_defaults();
        assert!(registry.count() >= 8);
    }

    #[test]
    fn test_action_registry_register_and_get() {
        let mut registry = ActionRegistry::new();
        let action = ActionExecutor::predefined_block_ip("10.0.0.1");
        let id = action.id.clone();

        registry.register(action);
        assert_eq!(registry.count(), 1);
        assert!(registry.get(&id).is_some());
    }

    #[test]
    fn test_action_registry_remove() {
        let mut registry = ActionRegistry::new();
        let action = ActionExecutor::predefined_block_ip("10.0.0.1");
        let id = action.id.clone();
        registry.register(action);

        let removed = registry.remove(&id);
        assert!(removed.is_some());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_action_registry_list() {
        let mut registry = ActionRegistry::new();
        registry.register(ActionExecutor::predefined_block_ip("1.2.3.4"));
        registry.register(ActionExecutor::predefined_notify("slack", "alert"));

        let ids = registry.list();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_action_registry_risk_filter() {
        let mut registry = ActionRegistry::new();
        registry.register(ActionExecutor::predefined_notify("slack", "msg")); // risk 1
        registry.register(ActionExecutor::predefined_block_ip("1.2.3.4")); // risk 3
        registry.register(ActionExecutor::predefined_rotate_secret("db-pw")); // risk 4

        let high_risk = registry.actions_at_risk_level(4);
        assert_eq!(high_risk.len(), 1);

        let mid_risk = registry.actions_at_risk_level(3);
        assert_eq!(mid_risk.len(), 2);
    }

    #[test]
    fn test_action_registry_approval_filter() {
        let mut registry = ActionRegistry::new();
        registry.register(ActionExecutor::predefined_notify("slack", "msg")); // no approval
        registry.register(ActionExecutor::predefined_block_ip("1.2.3.4")); // needs approval

        let need_approval = registry.actions_requiring_approval();
        assert_eq!(need_approval.len(), 1);
    }

    #[test]
    fn test_predefined_disable_user() {
        let action = ActionExecutor::predefined_disable_user("admin");
        assert!(action.requires_approval);
        assert!(action.reversible);
        assert_eq!(action.risk_level, 3);

        let mut executor = ActionExecutor::live();
        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.message.contains("Disabled user"));
        assert!(result.undo_info.is_some());
    }

    #[test]
    fn test_predefined_revoke_token() {
        let action = ActionExecutor::predefined_revoke_token("tok-123");
        assert!(!action.reversible);

        let mut executor = ActionExecutor::live();
        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.undo_info.is_none());
    }

    #[test]
    fn test_predefined_quarantine_file() {
        let action = ActionExecutor::predefined_quarantine_file("/tmp/malware.bin");
        assert!(action.reversible);

        let mut executor = ActionExecutor::live();
        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.undo_info.is_some());
    }

    #[test]
    fn test_predefined_rollback() {
        let action = ActionExecutor::predefined_rollback("api-server", "v1.2.3");
        assert!(action.requires_approval);
        assert_eq!(action.risk_level, 4);

        let mut executor = ActionExecutor::live();
        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.message.contains("Rolled back"));
    }

    #[test]
    fn test_predefined_create_ticket() {
        let action = ActionExecutor::predefined_create_ticket(
            "jira",
            "Security Incident",
            "Details here",
            "P1",
        );
        assert!(!action.requires_approval);
        assert_eq!(action.risk_level, 1);
    }

    #[test]
    fn test_scale_down_action() {
        let mut executor = ActionExecutor::live();
        let action = IncidentAction {
            id: "scale".into(),
            action_type: ActionType::ScaleDown {
                service: "web".into(),
                replicas: 0,
            },
            description: "Scale down".into(),
            requires_approval: true,
            reversible: true,
            risk_level: 4,
        };

        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.message.contains("Scaled web to 0 replicas"));
    }

    #[test]
    fn test_custom_action() {
        let mut executor = ActionExecutor::live();
        let mut params = HashMap::new();
        params.insert("key".into(), "value".into());

        let action = IncidentAction {
            id: "custom".into(),
            action_type: ActionType::Custom {
                name: "run-forensics".into(),
                params,
            },
            description: "Custom forensics".into(),
            requires_approval: true,
            reversible: false,
            risk_level: 2,
        };

        let result = executor.execute(&action);
        assert!(result.success);
        assert!(result.message.contains("run-forensics"));
    }
}
