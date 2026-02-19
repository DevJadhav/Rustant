//! Integration tests for the plugin lifecycle.
//!
//! Tests the full load → use → unload cycle from the public API,
//! security validation, and error handling.

use async_trait::async_trait;
use rustant_plugins::{
    Plugin, PluginCapability, PluginError, PluginManager, PluginMetadata, PluginSecurityValidator,
    PluginToolDef,
};

// ── Test plugin implementations ──────────────────────────────────────────

struct TestPlugin {
    name: String,
    load_count: u32,
    unload_count: u32,
}

impl TestPlugin {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            load_count: 0,
            unload_count: 0,
        }
    }
}

#[async_trait]
impl Plugin for TestPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: self.name.clone(),
            version: "0.1.0".into(),
            description: "Test plugin for integration testing".into(),
            author: Some("Test Author".into()),
            min_core_version: None,
            capabilities: vec![PluginCapability::ToolRegistration],
        }
    }

    async fn on_load(&mut self) -> Result<(), PluginError> {
        self.load_count += 1;
        Ok(())
    }

    async fn on_unload(&mut self) -> Result<(), PluginError> {
        self.unload_count += 1;
        Ok(())
    }

    fn tools(&self) -> Vec<PluginToolDef> {
        vec![PluginToolDef {
            name: format!("{}_tool", self.name),
            description: "A test tool".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }]
    }
}

/// Plugin that fails on load.
struct FailingLoadPlugin;

#[async_trait]
impl Plugin for FailingLoadPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "failing-plugin".into(),
            version: "1.0.0".into(),
            description: "Always fails to load".into(),
            author: None,
            min_core_version: None,
            capabilities: vec![],
        }
    }

    async fn on_load(&mut self) -> Result<(), PluginError> {
        Err(PluginError::ExecutionError("Load failed".into()))
    }

    async fn on_unload(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}

/// Plugin with an empty name (should fail security validation).
struct EmptyNamePlugin;

#[async_trait]
impl Plugin for EmptyNamePlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "".into(),
            version: "1.0.0".into(),
            description: "Has empty name".into(),
            author: None,
            min_core_version: None,
            capabilities: vec![],
        }
    }

    async fn on_load(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    async fn on_unload(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}

// ── Lifecycle tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_full_lifecycle_load_list_unload() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut mgr = PluginManager::new(dir.path());

    // Initially empty
    assert!(mgr.is_empty());
    assert_eq!(mgr.len(), 0);
    assert!(mgr.list().is_empty());

    // Load
    let plugin = Box::new(TestPlugin::new("lifecycle-test"));
    mgr.load_managed(plugin).await.unwrap();

    // Verify loaded
    assert_eq!(mgr.len(), 1);
    assert!(!mgr.is_empty());

    let state = mgr.get("lifecycle-test").unwrap();
    assert_eq!(state.metadata.name, "lifecycle-test");
    assert_eq!(state.metadata.version, "0.1.0");

    // Unload
    mgr.unload("lifecycle-test").await.unwrap();
    assert!(mgr.is_empty());
    assert!(mgr.get("lifecycle-test").is_none());
}

#[tokio::test]
async fn test_load_multiple_plugins() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut mgr = PluginManager::new(dir.path());

    mgr.load_managed(Box::new(TestPlugin::new("plugin-a")))
        .await
        .unwrap();
    mgr.load_managed(Box::new(TestPlugin::new("plugin-b")))
        .await
        .unwrap();
    mgr.load_managed(Box::new(TestPlugin::new("plugin-c")))
        .await
        .unwrap();

    assert_eq!(mgr.len(), 3);
    let names: Vec<&str> = mgr
        .list()
        .iter()
        .map(|s| s.metadata.name.as_str())
        .collect();
    assert!(names.contains(&"plugin-a"));
    assert!(names.contains(&"plugin-b"));
    assert!(names.contains(&"plugin-c"));
}

#[tokio::test]
async fn test_duplicate_load_rejected() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut mgr = PluginManager::new(dir.path());

    mgr.load_managed(Box::new(TestPlugin::new("dup")))
        .await
        .unwrap();
    let result = mgr.load_managed(Box::new(TestPlugin::new("dup"))).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        PluginError::AlreadyLoaded(name) => assert_eq!(name, "dup"),
        e => panic!("Expected AlreadyLoaded, got: {e:?}"),
    }
    // Original plugin still loaded
    assert_eq!(mgr.len(), 1);
}

#[tokio::test]
async fn test_unload_nonexistent_returns_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut mgr = PluginManager::new(dir.path());

    let result = mgr.unload("ghost").await;
    assert!(matches!(result, Err(PluginError::NotFound(_))));
}

#[tokio::test]
async fn test_failing_load_plugin() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut mgr = PluginManager::new(dir.path());

    let result = mgr.load_managed(Box::new(FailingLoadPlugin)).await;
    assert!(result.is_err());
    // Plugin should NOT be in the manager after failed load
    assert!(mgr.is_empty());
}

#[tokio::test]
async fn test_empty_name_plugin_rejected_by_security() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut mgr = PluginManager::new(dir.path());

    let result = mgr.load_managed(Box::new(EmptyNamePlugin)).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        PluginError::SecurityViolation(msg) => {
            assert!(
                msg.contains("empty") || msg.contains("name"),
                "Error should mention empty name: {msg}"
            );
        }
        e => panic!("Expected SecurityViolation, got: {e:?}"),
    }
}

// ── Security validator tests ─────────────────────────────────────────────

#[test]
fn test_validator_rejects_blocked_names() {
    let mut validator = PluginSecurityValidator::new();
    validator.block_name("banned-plugin");
    validator.block_name("another-bad");

    let meta = PluginMetadata {
        name: "banned-plugin".into(),
        version: "1.0.0".into(),
        description: "Blocked".into(),
        author: None,
        min_core_version: None,
        capabilities: vec![],
    };

    let result = validator.validate(&meta);
    assert!(!result.is_valid);
    assert!(!result.errors.is_empty());
}

#[test]
fn test_validator_warns_on_dangerous_capabilities() {
    let validator = PluginSecurityValidator::new();
    let meta = PluginMetadata {
        name: "dangerous".into(),
        version: "1.0.0".into(),
        description: "Has dangerous caps".into(),
        author: None,
        min_core_version: None,
        capabilities: vec![
            PluginCapability::ShellExecution,
            PluginCapability::SecretAccess,
            PluginCapability::FileSystemAccess,
            PluginCapability::NetworkAccess,
        ],
    };

    let result = validator.validate(&meta);
    assert!(result.is_valid); // Warnings don't prevent loading
    assert_eq!(result.warnings.len(), 4);
}

#[test]
fn test_validator_capability_limit_enforcement() {
    let mut validator = PluginSecurityValidator::new();
    validator.set_max_capabilities(2);

    let meta = PluginMetadata {
        name: "greedy".into(),
        version: "1.0.0".into(),
        description: "Too many caps".into(),
        author: None,
        min_core_version: None,
        capabilities: vec![
            PluginCapability::ToolRegistration,
            PluginCapability::HookRegistration,
            PluginCapability::NetworkAccess,
        ],
    };

    let result = validator.validate(&meta);
    assert!(!result.is_valid);
}

#[test]
fn test_plugin_metadata_roundtrip_serialization() {
    let meta = PluginMetadata {
        name: "serialization-test".into(),
        version: "2.3.4".into(),
        description: "Tests serialization round-trip".into(),
        author: Some("Author Name".into()),
        min_core_version: Some("0.5.0".into()),
        capabilities: vec![
            PluginCapability::ToolRegistration,
            PluginCapability::NetworkAccess,
        ],
    };

    let json = serde_json::to_string_pretty(&meta).unwrap();
    let restored: PluginMetadata = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.name, "serialization-test");
    assert_eq!(restored.version, "2.3.4");
    assert_eq!(restored.author, Some("Author Name".into()));
    assert_eq!(restored.min_core_version, Some("0.5.0".into()));
    assert_eq!(restored.capabilities.len(), 2);
}
