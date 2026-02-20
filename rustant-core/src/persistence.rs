//! Shared persistence utilities — atomic file writes, JSON load/save.
//!
//! Consolidates the atomic write pattern (write to .tmp then rename) used
//! across 24+ files in the codebase into a single reusable implementation.

use std::io;
use std::path::Path;

/// Atomically write JSON data to a file.
///
/// Serializes `data` to pretty-printed JSON, writes to a `.tmp` sibling file,
/// then atomically renames to the target path. This prevents corruption from
/// partial writes or process crashes.
///
/// Creates parent directories if they don't exist.
pub fn atomic_write_json<T: serde::Serialize>(path: &Path, data: &T) -> io::Result<()> {
    let json = serde_json::to_string_pretty(data).map_err(io::Error::other)?;
    atomic_write(path, json.as_bytes())
}

/// Atomically write raw bytes to a file.
///
/// Writes to a `.tmp` sibling file, then atomically renames to the target path.
/// Creates parent directories if they don't exist.
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Load and deserialize JSON from a file.
///
/// Returns `Ok(None)` if the file doesn't exist.
/// Returns `Err` on I/O errors or deserialization failures.
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(path)?;
    let value =
        serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestData {
        name: String,
        count: u32,
    }

    #[test]
    fn test_atomic_write_json_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");

        let data = TestData {
            name: "hello".into(),
            count: 42,
        };

        atomic_write_json(&path, &data).unwrap();
        let loaded: Option<TestData> = load_json(&path).unwrap();
        assert_eq!(loaded, Some(data));
    }

    #[test]
    fn test_atomic_write_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("dir").join("test.json");

        let data = TestData {
            name: "nested".into(),
            count: 1,
        };

        atomic_write_json(&path, &data).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_load_json_nonexistent() {
        let result: io::Result<Option<TestData>> = load_json(Path::new("/nonexistent/file.json"));
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_atomic_write_raw_bytes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("raw.bin");

        atomic_write(&path, b"hello world").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_atomic_write_no_tmp_leftover() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("clean.json");

        atomic_write_json(&path, &"test").unwrap();

        // The .tmp file should not remain
        let tmp = path.with_extension("tmp");
        assert!(!tmp.exists());
    }
}
