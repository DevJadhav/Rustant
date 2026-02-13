//! Browser session persistence for reconnecting across Rustant sessions.
//!
//! Saves browser connection info (debug port, WebSocket URL, tabs) to
//! `.rustant/browser-session.json` so subsequent Rustant invocations can
//! reconnect to the same Chrome instance instead of launching a new one.

use crate::browser::cdp::TabInfo;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Persisted browser connection metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConnectionInfo {
    pub debug_port: u16,
    pub ws_url: Option<String>,
    pub user_data_dir: Option<PathBuf>,
    pub tabs: Vec<TabInfo>,
    pub active_tab_id: Option<String>,
    pub saved_at: DateTime<Utc>,
}

/// Handles saving/loading browser session files.
pub struct BrowserSessionStore;

const SESSION_FILE: &str = ".rustant/browser-session.json";

impl BrowserSessionStore {
    /// Save browser connection info to the workspace.
    ///
    /// Uses atomic write (write to temp file, then rename) to prevent
    /// corruption if the process crashes mid-write.
    pub fn save(workspace: &Path, info: &BrowserConnectionInfo) -> io::Result<()> {
        let path = workspace.join(SESSION_FILE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(info).map_err(io::Error::other)?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)?;
        std::fs::rename(&tmp_path, &path)
    }

    /// Load saved browser connection info, if it exists and is still recent.
    ///
    /// Returns `None` if the file doesn't exist or is older than `max_age`.
    pub fn load(workspace: &Path) -> io::Result<Option<BrowserConnectionInfo>> {
        let path = workspace.join(SESSION_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(&path)?;
        let info: BrowserConnectionInfo = serde_json::from_str(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Discard sessions older than 24 hours — Chrome was probably restarted.
        let age = Utc::now() - info.saved_at;
        if age.num_hours() > 24 {
            // Stale session file — remove it.
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }

        Ok(Some(info))
    }

    /// Remove the saved session file.
    pub fn clear(workspace: &Path) -> io::Result<()> {
        let path = workspace.join(SESSION_FILE);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_info() -> BrowserConnectionInfo {
        BrowserConnectionInfo {
            debug_port: 9222,
            ws_url: Some("ws://127.0.0.1:9222/devtools/browser/abc".to_string()),
            user_data_dir: None,
            tabs: vec![
                TabInfo {
                    id: "tab-0".to_string(),
                    url: "https://example.com".to_string(),
                    title: "Example".to_string(),
                    active: true,
                },
                TabInfo {
                    id: "tab-1".to_string(),
                    url: "https://rust-lang.org".to_string(),
                    title: "Rust".to_string(),
                    active: false,
                },
            ],
            active_tab_id: Some("tab-0".to_string()),
            saved_at: Utc::now(),
        }
    }

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let info = make_info();
        BrowserSessionStore::save(dir.path(), &info).unwrap();

        let loaded = BrowserSessionStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.debug_port, 9222);
        assert_eq!(loaded.tabs.len(), 2);
        assert_eq!(loaded.active_tab_id, Some("tab-0".to_string()));
        assert!(loaded.ws_url.is_some());
    }

    #[test]
    fn test_load_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let loaded = BrowserSessionStore::load(dir.path()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_clear_removes_file() {
        let dir = TempDir::new().unwrap();
        let info = make_info();
        BrowserSessionStore::save(dir.path(), &info).unwrap();

        let path = dir.path().join(SESSION_FILE);
        assert!(path.exists());

        BrowserSessionStore::clear(dir.path()).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_clear_missing_is_ok() {
        let dir = TempDir::new().unwrap();
        BrowserSessionStore::clear(dir.path()).unwrap();
    }

    #[test]
    fn test_stale_session_discarded() {
        let dir = TempDir::new().unwrap();
        let mut info = make_info();
        // Set saved_at to 25 hours ago.
        info.saved_at = Utc::now() - chrono::Duration::hours(25);
        BrowserSessionStore::save(dir.path(), &info).unwrap();

        let loaded = BrowserSessionStore::load(dir.path()).unwrap();
        assert!(loaded.is_none());

        // File should be cleaned up.
        let path = dir.path().join(SESSION_FILE);
        assert!(!path.exists());
    }
}
