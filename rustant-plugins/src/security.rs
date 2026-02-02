//! Plugin security validation.
//!
//! Validates plugin metadata and capabilities for security risks.

use crate::PluginMetadata;
use serde::{Deserialize, Serialize};

/// Capabilities a plugin can request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginCapability {
    /// Register new tools.
    ToolRegistration,
    /// Register hooks.
    HookRegistration,
    /// Access filesystem.
    FileSystemAccess,
    /// Access network.
    NetworkAccess,
    /// Execute shell commands.
    ShellExecution,
    /// Access credentials/secrets.
    SecretAccess,
}

/// Result of security validation.
#[derive(Debug)]
pub struct SecurityValidationResult {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Validates plugin metadata and capabilities.
pub struct PluginSecurityValidator {
    /// Blocked plugin names.
    blocked_names: Vec<String>,
    /// Maximum allowed capabilities (if set).
    max_capabilities: Option<usize>,
}

impl PluginSecurityValidator {
    /// Create a new validator with default settings.
    pub fn new() -> Self {
        Self {
            blocked_names: Vec::new(),
            max_capabilities: None,
        }
    }

    /// Block a specific plugin name.
    pub fn block_name(&mut self, name: impl Into<String>) {
        self.blocked_names.push(name.into());
    }

    /// Set maximum number of capabilities allowed.
    pub fn set_max_capabilities(&mut self, max: usize) {
        self.max_capabilities = Some(max);
    }

    /// Validate plugin metadata.
    pub fn validate(&self, metadata: &PluginMetadata) -> SecurityValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check blocked names
        if self.blocked_names.contains(&metadata.name) {
            errors.push(format!("Plugin '{}' is blocked", metadata.name));
        }

        // Check name is not empty
        if metadata.name.is_empty() {
            errors.push("Plugin name cannot be empty".into());
        }

        // Check version is not empty
        if metadata.version.is_empty() {
            errors.push("Plugin version cannot be empty".into());
        }

        // Check capability count
        if let Some(max) = self.max_capabilities {
            if metadata.capabilities.len() > max {
                errors.push(format!(
                    "Plugin requests {} capabilities (max: {})",
                    metadata.capabilities.len(),
                    max
                ));
            }
        }

        // Warn about dangerous capabilities
        for cap in &metadata.capabilities {
            match cap {
                PluginCapability::ShellExecution => {
                    warnings.push("Plugin requests shell execution capability".into());
                }
                PluginCapability::SecretAccess => {
                    warnings.push("Plugin requests secret/credential access".into());
                }
                PluginCapability::FileSystemAccess => {
                    warnings.push("Plugin requests filesystem access".into());
                }
                PluginCapability::NetworkAccess => {
                    warnings.push("Plugin requests network access".into());
                }
                _ => {}
            }
        }

        // Version compatibility check
        if let Some(ref min_version) = metadata.min_core_version {
            if !is_version_compatible(min_version, env!("CARGO_PKG_VERSION")) {
                errors.push(format!(
                    "Plugin requires core version >= {} (current: {})",
                    min_version,
                    env!("CARGO_PKG_VERSION")
                ));
            }
        }

        let is_valid = errors.is_empty();
        SecurityValidationResult {
            is_valid,
            warnings,
            errors,
        }
    }
}

impl Default for PluginSecurityValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple semver-compatible version comparison.
/// Returns true if current >= required.
fn is_version_compatible(required: &str, current: &str) -> bool {
    let req_parts: Vec<u32> = required.split('.').filter_map(|p| p.parse().ok()).collect();
    let cur_parts: Vec<u32> = current.split('.').filter_map(|p| p.parse().ok()).collect();

    for i in 0..3 {
        let req = req_parts.get(i).copied().unwrap_or(0);
        let cur = cur_parts.get(i).copied().unwrap_or(0);
        if cur > req {
            return true;
        }
        if cur < req {
            return false;
        }
    }
    true // Equal versions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metadata(name: &str, caps: Vec<PluginCapability>) -> PluginMetadata {
        PluginMetadata {
            name: name.into(),
            version: "1.0.0".into(),
            description: "Test".into(),
            author: None,
            min_core_version: None,
            capabilities: caps,
        }
    }

    #[test]
    fn test_validate_clean_plugin() {
        let validator = PluginSecurityValidator::new();
        let meta = make_metadata("safe-plugin", vec![PluginCapability::ToolRegistration]);
        let result = validator.validate(&meta);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_blocked_name() {
        let mut validator = PluginSecurityValidator::new();
        validator.block_name("evil-plugin");
        let meta = make_metadata("evil-plugin", vec![]);
        let result = validator.validate(&meta);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validate_empty_name() {
        let validator = PluginSecurityValidator::new();
        let meta = make_metadata("", vec![]);
        let result = validator.validate(&meta);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validate_dangerous_capabilities_warn() {
        let validator = PluginSecurityValidator::new();
        let meta = make_metadata(
            "risky",
            vec![
                PluginCapability::ShellExecution,
                PluginCapability::SecretAccess,
            ],
        );
        let result = validator.validate(&meta);
        assert!(result.is_valid); // Warnings don't fail validation
        assert_eq!(result.warnings.len(), 2);
    }

    #[test]
    fn test_validate_max_capabilities() {
        let mut validator = PluginSecurityValidator::new();
        validator.set_max_capabilities(1);
        let meta = make_metadata(
            "greedy",
            vec![
                PluginCapability::ToolRegistration,
                PluginCapability::HookRegistration,
                PluginCapability::NetworkAccess,
            ],
        );
        let result = validator.validate(&meta);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_version_compatible() {
        assert!(is_version_compatible("0.1.0", "0.1.0"));
        assert!(is_version_compatible("0.1.0", "0.2.0"));
        assert!(is_version_compatible("0.1.0", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "0.9.0"));
        assert!(!is_version_compatible("0.2.0", "0.1.9"));
    }

    #[test]
    fn test_version_incompatible_core() {
        let validator = PluginSecurityValidator::new();
        let mut meta = make_metadata("new-plugin", vec![]);
        meta.min_core_version = Some("999.0.0".into());
        let result = validator.validate(&meta);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_capability_serialization() {
        let cap = PluginCapability::ShellExecution;
        let json = serde_json::to_string(&cap).unwrap();
        let restored: PluginCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, PluginCapability::ShellExecution);
    }
}
