//! Session management for persistent, resumable agent sessions.
//!
//! Maintains a session index in the sessions directory with metadata (name,
//! last task, timestamp, token usage, completion status). Supports auto-save,
//! listing, resume, rename, and delete operations.

use crate::error::MemoryError;
use crate::memory::MemorySystem;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Metadata for a session entry in the session index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    /// Unique session identifier.
    pub id: Uuid,
    /// Human-readable session name.
    pub name: String,
    /// When the session was first created.
    pub created_at: DateTime<Utc>,
    /// When the session was last saved.
    pub updated_at: DateTime<Utc>,
    /// The last goal/task the agent was working on.
    pub last_goal: Option<String>,
    /// Summary of what was accomplished.
    pub summary: Option<String>,
    /// Total messages in the session.
    pub message_count: usize,
    /// Total tokens used in the session.
    pub total_tokens: usize,
    /// Whether the session completed its task.
    pub completed: bool,
    /// File path to the session data (relative to sessions directory).
    pub file_name: String,
    /// User-defined tags for categorization.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Auto-detected project type at save time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type: Option<String>,
}

/// The session index stored as a JSON file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionIndex {
    pub entries: Vec<SessionEntry>,
}

impl SessionIndex {
    /// Load the session index from a directory.
    pub fn load(sessions_dir: &Path) -> Result<Self, MemoryError> {
        let index_path = sessions_dir.join("index.json");
        if !index_path.exists() {
            return Ok(Self::default());
        }
        let json =
            std::fs::read_to_string(&index_path).map_err(|e| MemoryError::PersistenceError {
                message: format!("Failed to read session index: {}", e),
            })?;
        serde_json::from_str(&json).map_err(|e| MemoryError::PersistenceError {
            message: format!("Failed to parse session index: {}", e),
        })
    }

    /// Save the session index to a directory.
    pub fn save(&self, sessions_dir: &Path) -> Result<(), MemoryError> {
        std::fs::create_dir_all(sessions_dir).map_err(|e| MemoryError::PersistenceError {
            message: format!("Failed to create sessions directory: {}", e),
        })?;
        let index_path = sessions_dir.join("index.json");
        let json =
            serde_json::to_string_pretty(self).map_err(|e| MemoryError::PersistenceError {
                message: format!("Failed to serialize session index: {}", e),
            })?;
        std::fs::write(&index_path, json).map_err(|e| MemoryError::PersistenceError {
            message: format!("Failed to write session index: {}", e),
        })
    }

    /// Find an entry by name (case-insensitive, fuzzy prefix match).
    pub fn find_by_name(&self, query: &str) -> Option<&SessionEntry> {
        let query_lower = query.to_lowercase();
        // Exact match first
        if let Some(entry) = self
            .entries
            .iter()
            .find(|e| e.name.to_lowercase() == query_lower)
        {
            return Some(entry);
        }
        // Prefix match
        self.entries
            .iter()
            .find(|e| e.name.to_lowercase().starts_with(&query_lower))
    }

    /// Find an entry by ID.
    pub fn find_by_id(&self, id: Uuid) -> Option<&SessionEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Get the most recent session (by updated_at).
    pub fn most_recent(&self) -> Option<&SessionEntry> {
        self.entries.iter().max_by_key(|e| e.updated_at)
    }

    /// List entries sorted by most recently updated.
    pub fn list_recent(&self, limit: usize) -> Vec<&SessionEntry> {
        let mut entries: Vec<&SessionEntry> = self.entries.iter().collect();
        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        entries.into_iter().take(limit).collect()
    }
}

/// Manages session persistence, indexing, and resume.
pub struct SessionManager {
    sessions_dir: PathBuf,
    index: SessionIndex,
    /// ID of the currently active session (if any).
    active_session_id: Option<Uuid>,
}

impl SessionManager {
    /// Create a new session manager for the given workspace.
    pub fn new(workspace: &Path) -> Result<Self, MemoryError> {
        let sessions_dir = workspace.join(".rustant").join("sessions");
        let index = SessionIndex::load(&sessions_dir)?;
        Ok(Self {
            sessions_dir,
            index,
            active_session_id: None,
        })
    }

    /// Create a new session manager with a custom sessions directory.
    pub fn with_dir(sessions_dir: PathBuf) -> Result<Self, MemoryError> {
        let index = SessionIndex::load(&sessions_dir)?;
        Ok(Self {
            sessions_dir,
            index,
            active_session_id: None,
        })
    }

    /// Start a new session with an optional name.
    pub fn start_session(&mut self, name: Option<&str>) -> SessionEntry {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let name = name
            .map(|n| n.to_string())
            .unwrap_or_else(|| now.format("%Y-%m-%d_%H%M%S").to_string());
        let file_name = format!("{}.json", id);

        let entry = SessionEntry {
            id,
            name,
            created_at: now,
            updated_at: now,
            last_goal: None,
            summary: None,
            message_count: 0,
            total_tokens: 0,
            completed: false,
            file_name,
            tags: Vec::new(),
            project_type: None,
        };

        self.index.entries.push(entry.clone());
        self.active_session_id = Some(id);
        let _ = self.index.save(&self.sessions_dir);
        entry
    }

    /// Save the current state of a memory system to the active session.
    pub fn save_checkpoint(
        &mut self,
        memory: &MemorySystem,
        total_tokens: usize,
    ) -> Result<(), MemoryError> {
        let session_id = self
            .active_session_id
            .ok_or_else(|| MemoryError::PersistenceError {
                message: "No active session to save".to_string(),
            })?;

        // Find the entry
        let entry = self
            .index
            .entries
            .iter_mut()
            .find(|e| e.id == session_id)
            .ok_or_else(|| MemoryError::PersistenceError {
                message: "Active session not found in index".to_string(),
            })?;

        // Update metadata
        entry.updated_at = Utc::now();
        entry.last_goal = memory.working.current_goal.clone();
        entry.message_count = memory.short_term.len();
        entry.total_tokens = total_tokens;

        // Save session data
        let session_path = self.sessions_dir.join(&entry.file_name);
        memory.save_session(&session_path)?;

        // Save updated index
        self.index.save(&self.sessions_dir)
    }

    /// Mark the active session as completed.
    pub fn complete_session(&mut self, summary: Option<String>) -> Result<(), MemoryError> {
        if let Some(session_id) = self.active_session_id {
            if let Some(entry) = self.index.entries.iter_mut().find(|e| e.id == session_id) {
                entry.completed = true;
                entry.updated_at = Utc::now();
                entry.summary = summary;
            }
            self.index.save(&self.sessions_dir)?;
        }
        Ok(())
    }

    /// Resume a session by name or ID. Returns the loaded MemorySystem and
    /// a continuation prompt to inject into the agent.
    pub fn resume_session(&mut self, query: &str) -> Result<(MemorySystem, String), MemoryError> {
        let entry = if let Ok(id) = Uuid::parse_str(query) {
            self.index
                .find_by_id(id)
                .cloned()
                .ok_or_else(|| MemoryError::SessionLoadFailed {
                    message: format!("No session found with ID: {}", id),
                })?
        } else {
            self.index.find_by_name(query).cloned().ok_or_else(|| {
                MemoryError::SessionLoadFailed {
                    message: format!("No session found matching: '{}'", query),
                }
            })?
        };

        let session_path = self.sessions_dir.join(&entry.file_name);
        let memory = MemorySystem::load_session(&session_path)?;

        // Build continuation prompt
        let mut continuation =
            String::from("You are resuming a previous session. Here is what was accomplished:\n");
        if let Some(ref goal) = entry.last_goal {
            continuation.push_str(&format!("- Last goal: {}\n", goal));
        }
        if let Some(ref summary) = entry.summary {
            continuation.push_str(&format!("- Summary: {}\n", summary));
        }
        continuation.push_str(&format!("- Messages exchanged: {}\n", entry.message_count));
        continuation.push_str(&format!(
            "- Session started: {}\n",
            entry.created_at.format("%Y-%m-%d %H:%M UTC")
        ));
        if entry.completed {
            continuation.push_str("- Status: Completed\n");
        } else {
            continuation.push_str("- Status: In progress (was interrupted)\n");
        }
        continuation.push_str("\nContinue from where the session left off.");

        // Set this as the active session
        self.active_session_id = Some(entry.id);

        Ok((memory, continuation))
    }

    /// Resume the most recent session.
    pub fn resume_latest(&mut self) -> Result<(MemorySystem, String), MemoryError> {
        let entry =
            self.index
                .most_recent()
                .cloned()
                .ok_or_else(|| MemoryError::SessionLoadFailed {
                    message: "No sessions found to resume".to_string(),
                })?;
        self.resume_session(&entry.id.to_string())
    }

    /// List recent sessions.
    pub fn list_sessions(&self, limit: usize) -> Vec<&SessionEntry> {
        self.index.list_recent(limit)
    }

    /// Rename a session.
    pub fn rename_session(&mut self, query: &str, new_name: &str) -> Result<(), MemoryError> {
        let entry = if let Ok(id) = Uuid::parse_str(query) {
            self.index.entries.iter_mut().find(|e| e.id == id)
        } else {
            let query_lower = query.to_lowercase();
            self.index.entries.iter_mut().find(|e| {
                e.name.to_lowercase() == query_lower
                    || e.name.to_lowercase().starts_with(&query_lower)
            })
        };

        match entry {
            Some(e) => {
                e.name = new_name.to_string();
                self.index.save(&self.sessions_dir)
            }
            None => Err(MemoryError::SessionLoadFailed {
                message: format!("No session found matching: '{}'", query),
            }),
        }
    }

    /// Delete a session (removes data file and index entry).
    pub fn delete_session(&mut self, query: &str) -> Result<String, MemoryError> {
        let (idx, file_name, name) = {
            let query_lower = query.to_lowercase();
            let found = if let Ok(id) = Uuid::parse_str(query) {
                self.index
                    .entries
                    .iter()
                    .enumerate()
                    .find(|(_, e)| e.id == id)
            } else {
                self.index.entries.iter().enumerate().find(|(_, e)| {
                    e.name.to_lowercase() == query_lower
                        || e.name.to_lowercase().starts_with(&query_lower)
                })
            };
            match found {
                Some((i, e)) => (i, e.file_name.clone(), e.name.clone()),
                None => {
                    return Err(MemoryError::SessionLoadFailed {
                        message: format!("No session found matching: '{}'", query),
                    })
                }
            }
        };

        // Remove session data file
        let session_path = self.sessions_dir.join(&file_name);
        if session_path.exists() {
            let _ = std::fs::remove_file(&session_path);
        }

        // Remove from index
        self.index.entries.remove(idx);
        self.index.save(&self.sessions_dir)?;

        Ok(name)
    }

    /// Get the active session ID.
    pub fn active_session_id(&self) -> Option<Uuid> {
        self.active_session_id
    }

    /// Get the sessions directory path.
    pub fn sessions_dir(&self) -> &Path {
        &self.sessions_dir
    }

    /// Get a reference to the session index.
    pub fn index(&self) -> &SessionIndex {
        &self.index
    }

    /// Create a SessionManager from an in-memory index (for testing).
    #[cfg(test)]
    pub(crate) fn from_index(index: SessionIndex) -> Self {
        Self {
            sessions_dir: PathBuf::from("/tmp/rustant-test-sessions"),
            index,
            active_session_id: None,
        }
    }

    /// Search sessions by matching query against name, goal, summary, and tags.
    /// Returns empty vec for empty/whitespace-only queries.
    pub fn search(&self, query: &str) -> Vec<&SessionEntry> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        self.index
            .entries
            .iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&query_lower)
                    || e.last_goal
                        .as_ref()
                        .is_some_and(|g| g.to_lowercase().contains(&query_lower))
                    || e.summary
                        .as_ref()
                        .is_some_and(|s| s.to_lowercase().contains(&query_lower))
                    || e.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Filter sessions by tag.
    pub fn filter_by_tag(&self, tag: &str) -> Vec<&SessionEntry> {
        let tag_lower = tag.to_lowercase();
        self.index
            .entries
            .iter()
            .filter(|e| e.tags.iter().any(|t| t.to_lowercase() == tag_lower))
            .collect()
    }

    /// Add a tag to a session.
    pub fn tag_session(&mut self, query: &str, tag: &str) -> Result<(), MemoryError> {
        let query_lower = query.to_lowercase();
        let entry = if let Ok(id) = Uuid::parse_str(query) {
            self.index.entries.iter_mut().find(|e| e.id == id)
        } else {
            self.index.entries.iter_mut().find(|e| {
                e.name.to_lowercase() == query_lower
                    || e.name.to_lowercase().starts_with(&query_lower)
            })
        };
        match entry {
            Some(e) => {
                let tag_str = tag.to_string();
                if !e.tags.contains(&tag_str) {
                    e.tags.push(tag_str);
                }
                self.index.save(&self.sessions_dir)
            }
            None => Err(MemoryError::SessionLoadFailed {
                message: format!("No session found matching: '{}'", query),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    fn create_test_manager(dir: &Path) -> SessionManager {
        SessionManager::with_dir(dir.to_path_buf()).unwrap()
    }

    #[test]
    fn test_start_session_default_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let entry = mgr.start_session(None);
        assert!(!entry.name.is_empty());
        assert!(!entry.completed);
        assert_eq!(mgr.active_session_id(), Some(entry.id));
    }

    #[test]
    fn test_start_session_with_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let entry = mgr.start_session(Some("refactor-auth"));
        assert_eq!(entry.name, "refactor-auth");
    }

    #[test]
    fn test_save_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let entry = mgr.start_session(Some("test-save"));

        let mut memory = MemorySystem::new(10);
        memory.start_new_task("fix the bug");
        memory.add_message(Message::user("fix bug #42"));
        memory.add_message(Message::assistant("Looking into it."));

        mgr.save_checkpoint(&memory, 500).unwrap();

        // Verify file was created
        let session_path = dir.path().join(&entry.file_name);
        assert!(session_path.exists());

        // Verify index was updated
        let reloaded = SessionIndex::load(dir.path()).unwrap();
        let saved = reloaded.find_by_id(entry.id).unwrap();
        assert_eq!(saved.last_goal.as_deref(), Some("fix the bug"));
        assert_eq!(saved.message_count, 2);
        assert_eq!(saved.total_tokens, 500);
    }

    #[test]
    fn test_resume_session_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        // Create and save a session
        mgr.start_session(Some("my-project"));
        let mut memory = MemorySystem::new(10);
        memory.start_new_task("implement feature X");
        memory.add_message(Message::user("implement feature X"));
        mgr.save_checkpoint(&memory, 200).unwrap();

        // Create a new manager (simulating restart)
        let mut mgr2 = create_test_manager(dir.path());
        let (loaded_mem, continuation) = mgr2.resume_session("my-project").unwrap();

        assert_eq!(
            loaded_mem.working.current_goal.as_deref(),
            Some("implement feature X")
        );
        assert!(continuation.contains("implement feature X"));
        assert!(continuation.contains("resuming a previous session"));
    }

    #[test]
    fn test_resume_session_by_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        mgr.start_session(Some("long-project-name"));
        let mut memory = MemorySystem::new(10);
        memory.add_message(Message::user("hello"));
        mgr.save_checkpoint(&memory, 100).unwrap();

        let mut mgr2 = create_test_manager(dir.path());
        let result = mgr2.resume_session("long");
        assert!(result.is_ok());
    }

    #[test]
    fn test_resume_latest() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        // First session
        mgr.start_session(Some("old-session"));
        let mut mem1 = MemorySystem::new(10);
        mem1.add_message(Message::user("old task"));
        mgr.save_checkpoint(&mem1, 100).unwrap();

        // Second session (more recent)
        mgr.start_session(Some("new-session"));
        let mut mem2 = MemorySystem::new(10);
        mem2.start_new_task("new task");
        mem2.add_message(Message::user("new task"));
        mgr.save_checkpoint(&mem2, 200).unwrap();

        // Resume latest
        let mut mgr2 = create_test_manager(dir.path());
        let (loaded, _) = mgr2.resume_latest().unwrap();
        assert_eq!(loaded.working.current_goal.as_deref(), Some("new task"));
    }

    #[test]
    fn test_list_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        for i in 0..5 {
            mgr.start_session(Some(&format!("session-{}", i)));
            let mut mem = MemorySystem::new(10);
            mem.add_message(Message::user("test"));
            mgr.save_checkpoint(&mem, 100).unwrap();
        }

        let sessions = mgr.list_sessions(3);
        assert_eq!(sessions.len(), 3);
    }

    #[test]
    fn test_rename_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let entry = mgr.start_session(Some("old-name"));
        mgr.rename_session("old-name", "new-name").unwrap();

        let reloaded = SessionIndex::load(dir.path()).unwrap();
        let found = reloaded.find_by_id(entry.id).unwrap();
        assert_eq!(found.name, "new-name");
    }

    #[test]
    fn test_delete_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let entry = mgr.start_session(Some("to-delete"));
        let mut mem = MemorySystem::new(10);
        mem.add_message(Message::user("test"));
        mgr.save_checkpoint(&mem, 100).unwrap();

        let session_path = dir.path().join(&entry.file_name);
        assert!(session_path.exists());

        let name = mgr.delete_session("to-delete").unwrap();
        assert_eq!(name, "to-delete");
        assert!(!session_path.exists());

        let reloaded = SessionIndex::load(dir.path()).unwrap();
        assert!(reloaded.find_by_id(entry.id).is_none());
    }

    #[test]
    fn test_complete_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let entry = mgr.start_session(Some("completing"));
        mgr.complete_session(Some("Finished all tasks".to_string()))
            .unwrap();

        let reloaded = SessionIndex::load(dir.path()).unwrap();
        let found = reloaded.find_by_id(entry.id).unwrap();
        assert!(found.completed);
        assert_eq!(found.summary.as_deref(), Some("Finished all tasks"));
    }

    #[test]
    fn test_session_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let result = mgr.resume_session("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resume_latest_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let result = mgr.resume_latest();
        assert!(result.is_err());
    }

    #[test]
    fn test_session_index_persistence() {
        let dir = tempfile::tempdir().unwrap();

        {
            let mut mgr = create_test_manager(dir.path());
            mgr.start_session(Some("persistent-session"));
            let mut mem = MemorySystem::new(10);
            mem.add_message(Message::user("test"));
            mgr.save_checkpoint(&mem, 100).unwrap();
        }

        // New manager should find the persisted session
        let mgr2 = create_test_manager(dir.path());
        let sessions = mgr2.list_sessions(10);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "persistent-session");
    }

    #[test]
    fn test_save_no_active_session_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = create_test_manager(dir.path());

        let mem = MemorySystem::new(10);
        let result = mgr.save_checkpoint(&mem, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_index_load_empty() {
        let dir = tempfile::tempdir().unwrap();
        let index = SessionIndex::load(dir.path()).unwrap();
        assert!(index.entries.is_empty());
    }

    #[test]
    fn test_session_entry_serialization() {
        let entry = SessionEntry {
            id: Uuid::new_v4(),
            name: "test-session".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_goal: Some("fix bug".to_string()),
            summary: None,
            message_count: 5,
            total_tokens: 1000,
            completed: false,
            file_name: "test.json".to_string(),
            tags: vec!["bugfix".to_string()],
            project_type: Some("Rust".to_string()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let restored: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "test-session");
        assert_eq!(restored.message_count, 5);
        assert_eq!(restored.tags, vec!["bugfix"]);
        assert_eq!(restored.project_type, Some("Rust".to_string()));
    }

    #[test]
    fn test_session_entry_deserialize_without_tags() {
        // Ensure backward compatibility: old entries without tags/project_type still deserialize
        let json = r#"{"id":"00000000-0000-0000-0000-000000000001","name":"old-session","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","last_goal":null,"summary":null,"message_count":3,"total_tokens":500,"completed":false,"file_name":"old.json"}"#;
        let entry: SessionEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.name, "old-session");
        assert!(entry.tags.is_empty());
        assert!(entry.project_type.is_none());
    }

    #[test]
    fn test_session_search() {
        let mut index = SessionIndex::default();
        let make_entry = |name: &str, goal: Option<&str>, tags: Vec<&str>| SessionEntry {
            id: Uuid::new_v4(),
            name: name.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_goal: goal.map(|g| g.to_string()),
            summary: None,
            message_count: 1,
            total_tokens: 100,
            completed: false,
            file_name: format!("{}.json", name),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            project_type: None,
        };
        index.entries.push(make_entry(
            "debug-auth",
            Some("fix authentication bug"),
            vec!["bugfix"],
        ));
        index.entries.push(make_entry(
            "refactor-api",
            Some("clean up API endpoints"),
            vec!["refactor"],
        ));
        index.entries.push(make_entry(
            "add-tests",
            Some("write unit tests"),
            vec!["testing"],
        ));

        let mgr = SessionManager::from_index(index);

        // Search by name
        let results = mgr.search("auth");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "debug-auth");

        // Search by goal
        let results = mgr.search("unit tests");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "add-tests");

        // Search by tag
        let results = mgr.search("bugfix");
        assert_eq!(results.len(), 1);

        // No matches
        let results = mgr.search("nonexistent");
        assert_eq!(results.len(), 0);

        // Empty query returns empty (not all sessions)
        let results = mgr.search("");
        assert!(results.is_empty(), "Empty query should return no results");

        // Whitespace-only query returns empty
        let results = mgr.search("   ");
        assert!(
            results.is_empty(),
            "Whitespace-only query should return no results"
        );
    }

    #[test]
    fn test_session_filter_by_tag() {
        let mut index = SessionIndex::default();
        let make_entry = |name: &str, tags: Vec<&str>| SessionEntry {
            id: Uuid::new_v4(),
            name: name.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_goal: None,
            summary: None,
            message_count: 1,
            total_tokens: 100,
            completed: false,
            file_name: format!("{}.json", name),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            project_type: None,
        };
        index
            .entries
            .push(make_entry("s1", vec!["bugfix", "urgent"]));
        index.entries.push(make_entry("s2", vec!["refactor"]));
        index.entries.push(make_entry("s3", vec!["bugfix"]));

        let mgr = SessionManager::from_index(index);

        let results = mgr.filter_by_tag("bugfix");
        assert_eq!(results.len(), 2);

        let results = mgr.filter_by_tag("urgent");
        assert_eq!(results.len(), 1);

        let results = mgr.filter_by_tag("nonexistent");
        assert_eq!(results.len(), 0);
    }
}
