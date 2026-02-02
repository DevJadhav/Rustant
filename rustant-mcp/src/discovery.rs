//! MCP server discovery — reads config and manages external MCP server processes.
//!
//! Discovers configured MCP servers from the agent config, spawns them as subprocesses,
//! and manages their lifecycle.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for an external MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name (used as identifier).
    pub name: String,
    /// Command to start the server.
    pub command: String,
    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the server process.
    pub working_dir: Option<PathBuf>,
    /// Environment variables to set.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether to auto-connect on startup.
    #[serde(default = "default_true")]
    pub auto_connect: bool,
}

fn default_true() -> bool {
    true
}

/// State of a discovered MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerState {
    pub config: McpServerConfig,
    pub status: McpServerStatus,
    pub tools_count: usize,
}

/// Status of an MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpServerStatus {
    /// Server is configured but not started.
    Configured,
    /// Server process is starting.
    Starting,
    /// Server is connected and initialized.
    Connected,
    /// Server process has stopped or crashed.
    Disconnected,
    /// Server failed to start.
    Failed(String),
}

/// Manages discovery and lifecycle of external MCP servers.
pub struct McpServerManager {
    servers: HashMap<String, McpServerState>,
}

impl McpServerManager {
    /// Create a new server manager.
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    /// Add a server configuration.
    pub fn add_server(&mut self, config: McpServerConfig) {
        let name = config.name.clone();
        self.servers.insert(
            name,
            McpServerState {
                config,
                status: McpServerStatus::Configured,
                tools_count: 0,
            },
        );
    }

    /// List all configured servers.
    pub fn list_servers(&self) -> Vec<&McpServerState> {
        self.servers.values().collect()
    }

    /// Get a server by name.
    pub fn get_server(&self, name: &str) -> Option<&McpServerState> {
        self.servers.get(name)
    }

    /// Update server status.
    pub fn set_status(&mut self, name: &str, status: McpServerStatus) {
        if let Some(state) = self.servers.get_mut(name) {
            state.status = status;
        }
    }

    /// Update the tool count for a server.
    pub fn set_tools_count(&mut self, name: &str, count: usize) {
        if let Some(state) = self.servers.get_mut(name) {
            state.tools_count = count;
        }
    }

    /// Remove a server.
    pub fn remove_server(&mut self, name: &str) -> Option<McpServerState> {
        self.servers.remove(name)
    }

    /// Number of configured servers.
    pub fn len(&self) -> usize {
        self.servers.len()
    }

    /// Whether no servers are configured.
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    /// Get servers that should auto-connect.
    pub fn auto_connect_servers(&self) -> Vec<&McpServerConfig> {
        self.servers
            .values()
            .filter(|s| s.config.auto_connect)
            .map(|s| &s.config)
            .collect()
    }
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            command: "npx".into(),
            args: vec!["-y".into(), format!("@mcp/{}", name)],
            working_dir: None,
            env: HashMap::new(),
            auto_connect: true,
        }
    }

    #[test]
    fn test_manager_add_and_list() {
        let mut mgr = McpServerManager::new();
        mgr.add_server(make_config("server-a"));
        mgr.add_server(make_config("server-b"));

        assert_eq!(mgr.len(), 2);
        assert!(!mgr.is_empty());
        assert_eq!(mgr.list_servers().len(), 2);
    }

    #[test]
    fn test_manager_get_server() {
        let mut mgr = McpServerManager::new();
        mgr.add_server(make_config("test"));

        let server = mgr.get_server("test").unwrap();
        assert_eq!(server.config.name, "test");
        assert_eq!(server.status, McpServerStatus::Configured);
    }

    #[test]
    fn test_manager_set_status() {
        let mut mgr = McpServerManager::new();
        mgr.add_server(make_config("test"));

        mgr.set_status("test", McpServerStatus::Connected);
        assert_eq!(
            mgr.get_server("test").unwrap().status,
            McpServerStatus::Connected
        );
    }

    #[test]
    fn test_manager_set_tools_count() {
        let mut mgr = McpServerManager::new();
        mgr.add_server(make_config("test"));

        mgr.set_tools_count("test", 5);
        assert_eq!(mgr.get_server("test").unwrap().tools_count, 5);
    }

    #[test]
    fn test_manager_remove_server() {
        let mut mgr = McpServerManager::new();
        mgr.add_server(make_config("test"));
        assert_eq!(mgr.len(), 1);

        let removed = mgr.remove_server("test");
        assert!(removed.is_some());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_manager_auto_connect() {
        let mut mgr = McpServerManager::new();
        mgr.add_server(make_config("auto"));

        let mut manual_config = make_config("manual");
        manual_config.auto_connect = false;
        mgr.add_server(manual_config);

        let auto = mgr.auto_connect_servers();
        assert_eq!(auto.len(), 1);
        assert_eq!(auto[0].name, "auto");
    }

    #[test]
    fn test_server_config_serialization() {
        let config = make_config("test");
        let json = serde_json::to_string(&config).unwrap();
        let restored: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "test");
        assert!(restored.auto_connect);
    }

    #[test]
    fn test_server_status_serialization() {
        let status = McpServerStatus::Failed("timeout".into());
        let json = serde_json::to_string(&status).unwrap();
        let restored: McpServerStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, McpServerStatus::Failed("timeout".into()));
    }

    #[test]
    fn test_empty_manager() {
        let mgr = McpServerManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
        assert!(mgr.get_server("nonexistent").is_none());
    }
}
