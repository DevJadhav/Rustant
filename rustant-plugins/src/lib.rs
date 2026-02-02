//! # Rustant Plugins
//!
//! Plugin system for the Rustant agent. Supports native dynamic loading (.so/.dll/.dylib)
//! and WASM sandboxed plugins. Plugins can register tools, hooks, and channels.

pub mod hooks;
pub mod loader;
pub mod security;
pub mod wasm_loader;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

pub use hooks::{Hook, HookContext, HookManager, HookPoint, HookResult};
pub use loader::NativePluginLoader;
pub use security::{PluginCapability, PluginSecurityValidator, SecurityValidationResult};
pub use wasm_loader::WasmPluginLoader;

/// Errors from plugin operations.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),
    #[error("Failed to load plugin: {0}")]
    LoadFailed(String),
    #[error("Plugin version incompatible: {0}")]
    VersionIncompatible(String),
    #[error("Security validation failed: {0}")]
    SecurityViolation(String),
    #[error("Plugin execution error: {0}")]
    ExecutionError(String),
}

/// Metadata about a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name.
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Author.
    pub author: Option<String>,
    /// Required Rustant core version (semver range).
    pub min_core_version: Option<String>,
    /// Plugin capabilities.
    pub capabilities: Vec<PluginCapability>,
}

/// The Plugin trait that all plugins must implement.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin metadata.
    fn metadata(&self) -> PluginMetadata;

    /// Called when the plugin is loaded.
    async fn on_load(&mut self) -> Result<(), PluginError>;

    /// Called when the plugin is unloaded.
    async fn on_unload(&mut self) -> Result<(), PluginError>;

    /// Return tool definitions provided by this plugin.
    fn tools(&self) -> Vec<PluginToolDef> {
        Vec::new()
    }

    /// Return hooks this plugin wants to register.
    fn hooks(&self) -> Vec<(HookPoint, Box<dyn Hook>)> {
        Vec::new()
    }
}

/// A tool definition from a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolDef {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
}

/// State of a loaded plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginState {
    pub metadata: PluginMetadata,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
    pub source_path: Option<String>,
    pub plugin_type: PluginType,
}

/// Type of plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginType {
    Native,
    Wasm,
    Managed,
}

/// Manages plugin lifecycle: load, unload, list.
pub struct PluginManager {
    plugins: HashMap<String, PluginEntry>,
    hook_manager: Arc<Mutex<HookManager>>,
    plugins_dir: PathBuf,
}

struct PluginEntry {
    plugin: Box<dyn Plugin>,
    state: PluginState,
}

impl PluginManager {
    /// Create a new plugin manager.
    pub fn new(plugins_dir: impl Into<PathBuf>) -> Self {
        Self {
            plugins: HashMap::new(),
            hook_manager: Arc::new(Mutex::new(HookManager::new())),
            plugins_dir: plugins_dir.into(),
        }
    }

    /// Get a reference to the hook manager.
    pub fn hook_manager(&self) -> Arc<Mutex<HookManager>> {
        self.hook_manager.clone()
    }

    /// Load a managed plugin (trait object).
    pub async fn load_managed(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        let metadata = plugin.metadata();
        let name = metadata.name.clone();

        if self.plugins.contains_key(&name) {
            return Err(PluginError::AlreadyLoaded(name));
        }

        // Security validation
        let validator = PluginSecurityValidator::new();
        let validation = validator.validate(&metadata);
        if !validation.is_valid {
            return Err(PluginError::SecurityViolation(
                validation
                    .errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Unknown security issue".into()),
            ));
        }

        let mut plugin = plugin;
        plugin.on_load().await?;

        // Register hooks
        let hooks = plugin.hooks();
        {
            let mut hm = self.hook_manager.lock().await;
            for (point, hook) in hooks {
                hm.register(point, hook);
            }
        }

        let state = PluginState {
            metadata: metadata.clone(),
            loaded_at: chrono::Utc::now(),
            source_path: None,
            plugin_type: PluginType::Managed,
        };

        self.plugins.insert(name, PluginEntry { plugin, state });
        Ok(())
    }

    /// Unload a plugin by name.
    pub async fn unload(&mut self, name: &str) -> Result<(), PluginError> {
        let mut entry = self
            .plugins
            .remove(name)
            .ok_or_else(|| PluginError::NotFound(name.into()))?;
        entry.plugin.on_unload().await?;
        Ok(())
    }

    /// List all loaded plugins.
    pub fn list(&self) -> Vec<&PluginState> {
        self.plugins.values().map(|e| &e.state).collect()
    }

    /// Get a plugin state by name.
    pub fn get(&self, name: &str) -> Option<&PluginState> {
        self.plugins.get(name).map(|e| &e.state)
    }

    /// Number of loaded plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether no plugins are loaded.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Get the plugins directory.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlugin {
        name: String,
        loaded: bool,
    }

    impl MockPlugin {
        fn new(name: &str) -> Self {
            Self {
                name: name.into(),
                loaded: false,
            }
        }
    }

    #[async_trait]
    impl Plugin for MockPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                name: self.name.clone(),
                version: "1.0.0".into(),
                description: "A mock plugin".into(),
                author: Some("Test".into()),
                min_core_version: None,
                capabilities: vec![],
            }
        }

        async fn on_load(&mut self) -> Result<(), PluginError> {
            self.loaded = true;
            Ok(())
        }

        async fn on_unload(&mut self) -> Result<(), PluginError> {
            self.loaded = false;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_plugin_manager_load_unload() {
        let mut mgr = PluginManager::new("/tmp/plugins");
        let plugin = Box::new(MockPlugin::new("test-plugin"));

        mgr.load_managed(plugin).await.unwrap();
        assert_eq!(mgr.len(), 1);
        assert!(!mgr.is_empty());

        let state = mgr.get("test-plugin").unwrap();
        assert_eq!(state.metadata.name, "test-plugin");
        assert_eq!(state.plugin_type, PluginType::Managed);

        mgr.unload("test-plugin").await.unwrap();
        assert_eq!(mgr.len(), 0);
        assert!(mgr.is_empty());
    }

    #[tokio::test]
    async fn test_plugin_manager_duplicate_load() {
        let mut mgr = PluginManager::new("/tmp/plugins");
        let plugin1 = Box::new(MockPlugin::new("dupe"));
        let plugin2 = Box::new(MockPlugin::new("dupe"));

        mgr.load_managed(plugin1).await.unwrap();
        let result = mgr.load_managed(plugin2).await;
        assert!(matches!(result, Err(PluginError::AlreadyLoaded(_))));
    }

    #[tokio::test]
    async fn test_plugin_manager_unload_not_found() {
        let mut mgr = PluginManager::new("/tmp/plugins");
        let result = mgr.unload("nonexistent").await;
        assert!(matches!(result, Err(PluginError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_plugin_manager_list() {
        let mut mgr = PluginManager::new("/tmp/plugins");
        mgr.load_managed(Box::new(MockPlugin::new("alpha")))
            .await
            .unwrap();
        mgr.load_managed(Box::new(MockPlugin::new("beta")))
            .await
            .unwrap();

        let list = mgr.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_plugin_metadata_serialization() {
        let meta = PluginMetadata {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "Test plugin".into(),
            author: None,
            min_core_version: Some("0.1.0".into()),
            capabilities: vec![PluginCapability::ToolRegistration],
        };
        let json = serde_json::to_string(&meta).unwrap();
        let restored: PluginMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "test");
    }
}
