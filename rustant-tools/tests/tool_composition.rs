//! Integration tests for tool composition.
//!
//! Tests that verify multiple tools work together in realistic workflows:
//! file_write → file_read, file_write → file_search, file_write → file_patch,
//! and tool registry round-trips.

use rustant_tools::file::{FileListTool, FileReadTool, FileSearchTool, FileWriteTool};
use rustant_tools::registry::Tool;
use serde_json::json;
use tempfile::TempDir;

// ── File write → read round-trip ─────────────────────────────────────────

#[tokio::test]
async fn test_write_then_read_roundtrip() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path().canonicalize().unwrap();

    let write_tool = FileWriteTool::new(workspace.clone());
    let read_tool = FileReadTool::new(workspace.clone());

    let content = "Hello from the integration test!\nLine 2.\nLine 3.";
    let file_path = workspace.join("roundtrip.txt");

    // Write
    let write_result = write_tool
        .execute(json!({
            "path": file_path.to_str().unwrap(),
            "content": content
        }))
        .await
        .unwrap();
    assert!(
        write_result.content.contains("Created")
            || write_result.content.contains("Written")
            || write_result.content.contains("wrote")
            || write_result.content.contains("bytes"),
        "Write output should indicate success: {}",
        write_result.content
    );

    // Read back
    let read_result = read_tool
        .execute(json!({
            "path": file_path.to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(
        read_result
            .content
            .contains("Hello from the integration test!"),
        "Read should return the written content: {}",
        read_result.content
    );
    assert!(read_result.content.contains("Line 2."));
    assert!(read_result.content.contains("Line 3."));
}

// ── File write → search ──────────────────────────────────────────────────

#[tokio::test]
async fn test_write_then_search() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path().canonicalize().unwrap();

    let write_tool = FileWriteTool::new(workspace.clone());
    let search_tool = FileSearchTool::new(workspace.clone());

    // Write a file with a unique keyword
    let file_path = workspace.join("searchable.txt");
    write_tool
        .execute(json!({
            "path": file_path.to_str().unwrap(),
            "content": "This file contains the unique keyword XYZZY123 for testing."
        }))
        .await
        .unwrap();

    // Search for the keyword
    let search_result = search_tool
        .execute(json!({
            "pattern": "XYZZY123"
        }))
        .await
        .unwrap();
    assert!(
        search_result.content.contains("XYZZY123"),
        "Search should find the keyword: {}",
        search_result.content
    );
    assert!(
        search_result.content.contains("searchable.txt"),
        "Search should reference the file: {}",
        search_result.content
    );
}

// ── File write → list ────────────────────────────────────────────────────

#[tokio::test]
async fn test_write_then_list() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path().canonicalize().unwrap();

    let write_tool = FileWriteTool::new(workspace.clone());
    let list_tool = FileListTool::new(workspace.clone());

    // Write multiple files
    for name in &["alpha.txt", "beta.txt", "gamma.txt"] {
        let file_path = workspace.join(name);
        write_tool
            .execute(json!({
                "path": file_path.to_str().unwrap(),
                "content": format!("Content of {}", name)
            }))
            .await
            .unwrap();
    }

    // List files
    let list_result = list_tool
        .execute(json!({
            "path": workspace.to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(
        list_result.content.contains("alpha.txt"),
        "Should list alpha.txt: {}",
        list_result.content
    );
    assert!(
        list_result.content.contains("beta.txt"),
        "Should list beta.txt: {}",
        list_result.content
    );
    assert!(
        list_result.content.contains("gamma.txt"),
        "Should list gamma.txt: {}",
        list_result.content
    );
}

// ── Multiple writes don't interfere ──────────────────────────────────────

#[tokio::test]
async fn test_multiple_writes_independent() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path().canonicalize().unwrap();

    let write_tool = FileWriteTool::new(workspace.clone());
    let read_tool = FileReadTool::new(workspace.clone());

    let file_a = workspace.join("file_a.txt");
    let file_b = workspace.join("file_b.txt");

    // Write two different files
    write_tool
        .execute(json!({
            "path": file_a.to_str().unwrap(),
            "content": "Content A"
        }))
        .await
        .unwrap();

    write_tool
        .execute(json!({
            "path": file_b.to_str().unwrap(),
            "content": "Content B"
        }))
        .await
        .unwrap();

    // Read both and verify they're independent
    let read_a = read_tool
        .execute(json!({"path": file_a.to_str().unwrap()}))
        .await
        .unwrap();
    let read_b = read_tool
        .execute(json!({"path": file_b.to_str().unwrap()}))
        .await
        .unwrap();

    assert!(read_a.content.contains("Content A"));
    assert!(!read_a.content.contains("Content B"));
    assert!(read_b.content.contains("Content B"));
    assert!(!read_b.content.contains("Content A"));
}

// ── Write overwrite behavior ─────────────────────────────────────────────

#[tokio::test]
async fn test_write_overwrites_existing() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path().canonicalize().unwrap();

    let write_tool = FileWriteTool::new(workspace.clone());
    let read_tool = FileReadTool::new(workspace.clone());

    let file_path = workspace.join("overwrite.txt");

    // First write
    write_tool
        .execute(json!({
            "path": file_path.to_str().unwrap(),
            "content": "Original content"
        }))
        .await
        .unwrap();

    // Overwrite
    write_tool
        .execute(json!({
            "path": file_path.to_str().unwrap(),
            "content": "Updated content"
        }))
        .await
        .unwrap();

    // Read should show updated content
    let result = read_tool
        .execute(json!({"path": file_path.to_str().unwrap()}))
        .await
        .unwrap();
    assert!(result.content.contains("Updated content"));
    assert!(!result.content.contains("Original content"));
}
