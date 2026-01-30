//! Git-based checkpoint manager for undo/redo functionality.
//!
//! Uses git2 (libgit2 bindings) to create lightweight stash-like snapshots
//! of the workspace. Each checkpoint captures the full working tree state
//! so file modifications can be rolled back.

use git2::{Oid, Repository, Signature};
use std::path::{Path, PathBuf};

/// Errors specific to checkpoint operations.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("no checkpoints available")]
    NoCheckpoints,
    #[error("repository not found at {0}")]
    RepoNotFound(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// A single checkpoint (snapshot of the working tree).
#[derive(Debug, Clone)]
pub struct Checkpoint {
    /// The commit OID for this checkpoint.
    pub oid: String,
    /// Human-readable label.
    pub label: String,
    /// Timestamp when the checkpoint was created.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Files changed in this checkpoint.
    pub changed_files: Vec<String>,
}

/// Manages git-based checkpoints for the workspace.
pub struct CheckpointManager {
    workspace: PathBuf,
    checkpoints: Vec<Checkpoint>,
    /// Name of the checkpoint ref namespace.
    ref_prefix: String,
}

impl CheckpointManager {
    /// Create a new CheckpointManager for the given workspace.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            checkpoints: Vec::new(),
            ref_prefix: "refs/rustant/checkpoints".to_string(),
        }
    }

    /// Get the workspace path.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Open the repository at the workspace path.
    fn open_repo(&self) -> Result<Repository, CheckpointError> {
        Repository::discover(&self.workspace)
            .map_err(|_| CheckpointError::RepoNotFound(self.workspace.clone()))
    }

    /// Create a checkpoint of the current workspace state.
    ///
    /// This stages all changes and creates a commit on a detached ref
    /// so it doesn't affect the user's branch or history.
    pub fn create_checkpoint(&mut self, label: &str) -> Result<Checkpoint, CheckpointError> {
        let repo = self.open_repo()?;

        // Get the current HEAD as the parent
        let head = repo.head()?;
        let parent_commit = head.peel_to_commit()?;

        // Build a tree from the current working directory state
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        // Detect changed files by diffing against parent
        let parent_tree = parent_commit.tree()?;
        let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;
        let changed_files: Vec<String> = diff
            .deltas()
            .filter_map(|d| d.new_file().path().map(|p| p.to_string_lossy().to_string()))
            .collect();

        // Create the checkpoint commit
        let sig = Signature::now("rustant", "rustant@local")?;
        let message = format!("[checkpoint] {}", label);
        let oid = repo.commit(
            None, // don't update any ref yet
            &sig,
            &sig,
            &message,
            &tree,
            &[&parent_commit],
        )?;

        // Store as a named reference
        let ref_name = format!("{}/{}", self.ref_prefix, self.checkpoints.len());
        repo.reference(&ref_name, oid, true, &format!("checkpoint: {}", label))?;

        let checkpoint = Checkpoint {
            oid: oid.to_string(),
            label: label.to_string(),
            timestamp: chrono::Utc::now(),
            changed_files,
        };

        self.checkpoints.push(checkpoint.clone());
        Ok(checkpoint)
    }

    /// Restore the workspace to the state at the given checkpoint.
    pub fn restore_checkpoint(
        &mut self,
        checkpoint_index: usize,
    ) -> Result<&Checkpoint, CheckpointError> {
        if checkpoint_index >= self.checkpoints.len() {
            return Err(CheckpointError::NoCheckpoints);
        }

        let checkpoint = &self.checkpoints[checkpoint_index];
        let repo = self.open_repo()?;
        let oid = Oid::from_str(&checkpoint.oid)?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;

        // Reset the working directory to the checkpoint tree
        repo.checkout_tree(
            tree.as_object(),
            Some(git2::build::CheckoutBuilder::new().force()),
        )?;

        // Reset index to match
        let mut index = repo.index()?;
        index.read_tree(&tree)?;
        index.write()?;

        Ok(checkpoint)
    }

    /// Undo the last change by restoring the most recent checkpoint.
    pub fn undo(&mut self) -> Result<&Checkpoint, CheckpointError> {
        if self.checkpoints.is_empty() {
            return Err(CheckpointError::NoCheckpoints);
        }
        let last = self.checkpoints.len() - 1;
        self.restore_checkpoint(last)
    }

    /// Get all checkpoints.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Get the number of checkpoints.
    pub fn count(&self) -> usize {
        self.checkpoints.len()
    }

    /// Get the diff between the current working tree and the last checkpoint.
    pub fn diff_from_last(&self) -> Result<String, CheckpointError> {
        let repo = self.open_repo()?;

        if self.checkpoints.is_empty() {
            // Diff against HEAD
            let head = repo.head()?;
            let tree = head.peel_to_tree()?;
            let diff = repo.diff_tree_to_workdir(Some(&tree), None)?;
            let mut output = Vec::new();
            diff.print(git2::DiffFormat::Patch, |_, _, line| {
                output.extend_from_slice(line.content());
                true
            })?;
            return Ok(String::from_utf8_lossy(&output).to_string());
        }

        let last = &self.checkpoints[self.checkpoints.len() - 1];
        let oid = Oid::from_str(&last.oid)?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;

        let diff = repo.diff_tree_to_workdir(Some(&tree), None)?;
        let mut output = Vec::new();
        diff.print(git2::DiffFormat::Patch, |_, _, line| {
            output.extend_from_slice(line.content());
            true
        })?;

        Ok(String::from_utf8_lossy(&output).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        // Initialize repo
        let repo = Repository::init(&path).unwrap();

        // Create initial file and commit
        fs::write(path.join("initial.txt"), "initial content").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = Signature::now("test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        (dir, path)
    }

    #[test]
    fn test_checkpoint_manager_new() {
        let (_dir, path) = setup_test_repo();
        let mgr = CheckpointManager::new(path.clone());
        assert_eq!(mgr.workspace(), path);
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_create_checkpoint() {
        let (_dir, path) = setup_test_repo();
        let mut mgr = CheckpointManager::new(path.clone());

        // Modify a file
        fs::write(path.join("initial.txt"), "modified content").unwrap();

        let cp = mgr.create_checkpoint("before tool exec").unwrap();
        assert_eq!(cp.label, "before tool exec");
        assert!(!cp.oid.is_empty());
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn test_create_multiple_checkpoints() {
        let (_dir, path) = setup_test_repo();
        let mut mgr = CheckpointManager::new(path.clone());

        fs::write(path.join("initial.txt"), "v2").unwrap();
        mgr.create_checkpoint("cp1").unwrap();

        fs::write(path.join("initial.txt"), "v3").unwrap();
        mgr.create_checkpoint("cp2").unwrap();

        assert_eq!(mgr.count(), 2);
        assert_eq!(mgr.checkpoints()[0].label, "cp1");
        assert_eq!(mgr.checkpoints()[1].label, "cp2");
    }

    #[test]
    fn test_restore_checkpoint() {
        let (_dir, path) = setup_test_repo();
        let mut mgr = CheckpointManager::new(path.clone());

        // Create checkpoint with original state
        mgr.create_checkpoint("original").unwrap();

        // Modify file
        fs::write(path.join("initial.txt"), "CHANGED").unwrap();
        assert_eq!(
            fs::read_to_string(path.join("initial.txt")).unwrap(),
            "CHANGED"
        );

        // Restore
        mgr.restore_checkpoint(0).unwrap();
        // File should be restored to the checkpoint state
        let content = fs::read_to_string(path.join("initial.txt")).unwrap();
        assert_ne!(content, "CHANGED");
    }

    #[test]
    fn test_undo_no_checkpoints() {
        let (_dir, path) = setup_test_repo();
        let mut mgr = CheckpointManager::new(path);
        let result = mgr.undo();
        assert!(result.is_err());
        match result.unwrap_err() {
            CheckpointError::NoCheckpoints => {}
            other => panic!("Expected NoCheckpoints, got {:?}", other),
        }
    }

    #[test]
    fn test_undo_restores_last() {
        let (_dir, path) = setup_test_repo();
        let mut mgr = CheckpointManager::new(path.clone());

        fs::write(path.join("initial.txt"), "checkpoint state").unwrap();
        mgr.create_checkpoint("before change").unwrap();

        fs::write(path.join("initial.txt"), "after change").unwrap();
        mgr.undo().unwrap();

        let content = fs::read_to_string(path.join("initial.txt")).unwrap();
        assert_eq!(content, "checkpoint state");
    }

    #[test]
    fn test_diff_from_last_no_checkpoints() {
        let (_dir, path) = setup_test_repo();
        let mgr = CheckpointManager::new(path.clone());

        // Modify file
        fs::write(path.join("initial.txt"), "modified").unwrap();

        let diff = mgr.diff_from_last().unwrap();
        assert!(!diff.is_empty());
        assert!(diff.contains("modified"));
    }

    #[test]
    fn test_diff_from_last_with_checkpoint() {
        let (_dir, path) = setup_test_repo();
        let mut mgr = CheckpointManager::new(path.clone());

        fs::write(path.join("initial.txt"), "checkpoint state").unwrap();
        mgr.create_checkpoint("cp1").unwrap();

        fs::write(path.join("initial.txt"), "new state").unwrap();

        let diff = mgr.diff_from_last().unwrap();
        assert!(diff.contains("new state") || diff.contains("checkpoint state"));
    }

    #[test]
    fn test_checkpoint_changed_files() {
        let (_dir, path) = setup_test_repo();
        let mut mgr = CheckpointManager::new(path.clone());

        fs::write(path.join("new_file.txt"), "hello").unwrap();
        let cp = mgr.create_checkpoint("added file").unwrap();
        assert!(cp.changed_files.iter().any(|f| f.contains("new_file")));
    }

    #[test]
    fn test_repo_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = CheckpointManager::new(dir.path().to_path_buf());
        let result = mgr.diff_from_last();
        assert!(result.is_err());
    }
}
