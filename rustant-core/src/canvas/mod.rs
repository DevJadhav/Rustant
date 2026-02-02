//! # Canvas System
//!
//! A2UI-inspired protocol for agent-to-UI rich content display.
//! Supports pushing HTML, Markdown, charts, tables, forms, and diagrams
//! to connected UI clients (Tauri dashboard, web clients).

pub mod components;
pub mod protocol;
pub mod renderer;

pub use components::{ChartDataset, ChartSpec, DiagramSpec, FormField, FormSpec, TableSpec};
pub use protocol::{CanvasItem, CanvasMessage, CanvasTarget, ContentType};
pub use renderer::{
    render_chart_config, render_diagram_mermaid, render_form_html, render_table_html,
};

use std::collections::HashMap;

/// Maximum content size per canvas item (default 1MB).
const MAX_CONTENT_SIZE: usize = 1_048_576;

/// Manages canvas state — stores pushed items and provides snapshots.
#[derive(Debug, Default)]
pub struct CanvasManager {
    /// Items organized by target name. "broadcast" is the default target.
    targets: HashMap<String, Vec<CanvasItem>>,
    /// Maximum content size in bytes.
    max_content_size: usize,
}

impl CanvasManager {
    pub fn new() -> Self {
        Self {
            targets: HashMap::new(),
            max_content_size: MAX_CONTENT_SIZE,
        }
    }

    /// Set the maximum content size per item.
    pub fn set_max_content_size(&mut self, size: usize) {
        self.max_content_size = size;
    }

    /// Push content to a target. Returns the item ID.
    pub fn push(
        &mut self,
        target: &CanvasTarget,
        content_type: ContentType,
        content: String,
    ) -> Result<uuid::Uuid, CanvasError> {
        if content.len() > self.max_content_size {
            return Err(CanvasError::ContentTooLarge {
                size: content.len(),
                max: self.max_content_size,
            });
        }
        let item = CanvasItem::new(content_type, content);
        let id = item.id;
        let key = target_key(target);
        self.targets.entry(key).or_default().push(item);
        Ok(id)
    }

    /// Clear all items from a target.
    pub fn clear(&mut self, target: &CanvasTarget) {
        let key = target_key(target);
        self.targets.remove(&key);
    }

    /// Get a snapshot of all items for a target.
    pub fn snapshot(&self, target: &CanvasTarget) -> Vec<&CanvasItem> {
        let key = target_key(target);
        self.targets
            .get(&key)
            .map(|items| items.iter().collect())
            .unwrap_or_default()
    }

    /// Total number of items across all targets.
    pub fn total_items(&self) -> usize {
        self.targets.values().map(|v| v.len()).sum()
    }

    /// Check if a target has any items.
    pub fn is_empty(&self, target: &CanvasTarget) -> bool {
        let key = target_key(target);
        self.targets.get(&key).map(|v| v.is_empty()).unwrap_or(true)
    }
}

fn target_key(target: &CanvasTarget) -> String {
    match target {
        CanvasTarget::Broadcast => "broadcast".into(),
        CanvasTarget::Named(name) => name.clone(),
    }
}

/// Errors from canvas operations.
#[derive(Debug, thiserror::Error)]
pub enum CanvasError {
    #[error("Content too large: {size} bytes exceeds maximum {max} bytes")]
    ContentTooLarge { size: usize, max: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_manager_push_and_snapshot() {
        let mut mgr = CanvasManager::new();
        let target = CanvasTarget::Broadcast;

        let id = mgr
            .push(&target, ContentType::Html, "<h1>Hello</h1>".into())
            .unwrap();
        assert!(!id.is_nil());

        let items = mgr.snapshot(&target);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "<h1>Hello</h1>");
        assert_eq!(items[0].content_type, ContentType::Html);
    }

    #[test]
    fn test_canvas_manager_push_multiple() {
        let mut mgr = CanvasManager::new();
        let target = CanvasTarget::Broadcast;

        mgr.push(&target, ContentType::Html, "<p>1</p>".into())
            .unwrap();
        mgr.push(&target, ContentType::Markdown, "# Title".into())
            .unwrap();
        mgr.push(&target, ContentType::Code, "fn main() {}".into())
            .unwrap();

        assert_eq!(mgr.snapshot(&target).len(), 3);
        assert_eq!(mgr.total_items(), 3);
    }

    #[test]
    fn test_canvas_manager_clear() {
        let mut mgr = CanvasManager::new();
        let target = CanvasTarget::Broadcast;

        mgr.push(&target, ContentType::Html, "<p>data</p>".into())
            .unwrap();
        assert!(!mgr.is_empty(&target));

        mgr.clear(&target);
        assert!(mgr.is_empty(&target));
        assert_eq!(mgr.snapshot(&target).len(), 0);
    }

    #[test]
    fn test_canvas_manager_named_targets() {
        let mut mgr = CanvasManager::new();
        let main = CanvasTarget::Named("main".into());
        let sidebar = CanvasTarget::Named("sidebar".into());

        mgr.push(&main, ContentType::Html, "<p>main</p>".into())
            .unwrap();
        mgr.push(&sidebar, ContentType::Html, "<p>side</p>".into())
            .unwrap();

        assert_eq!(mgr.snapshot(&main).len(), 1);
        assert_eq!(mgr.snapshot(&sidebar).len(), 1);
        assert_eq!(mgr.total_items(), 2);

        // Clearing main doesn't affect sidebar
        mgr.clear(&main);
        assert!(mgr.is_empty(&main));
        assert!(!mgr.is_empty(&sidebar));
    }

    #[test]
    fn test_canvas_manager_max_content_size() {
        let mut mgr = CanvasManager::new();
        mgr.set_max_content_size(10);

        let target = CanvasTarget::Broadcast;
        let result = mgr.push(&target, ContentType::Html, "short".into());
        assert!(result.is_ok());

        let result = mgr.push(&target, ContentType::Html, "this is way too long".into());
        assert!(result.is_err());
        match result {
            Err(CanvasError::ContentTooLarge { size, max }) => {
                assert_eq!(max, 10);
                assert!(size > 10);
            }
            _ => panic!("Expected ContentTooLarge"),
        }
    }

    #[test]
    fn test_canvas_snapshot_captures_state() {
        let mut mgr = CanvasManager::new();
        let target = CanvasTarget::Broadcast;

        mgr.push(&target, ContentType::Chart, "{\"type\":\"bar\"}".into())
            .unwrap();
        mgr.push(&target, ContentType::Table, "{\"headers\":[\"A\"]}".into())
            .unwrap();

        let snap = mgr.snapshot(&target);
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].content_type, ContentType::Chart);
        assert_eq!(snap[1].content_type, ContentType::Table);
    }

    #[test]
    fn test_canvas_empty_target() {
        let mgr = CanvasManager::new();
        let target = CanvasTarget::Named("nonexistent".into());
        assert!(mgr.is_empty(&target));
        assert_eq!(mgr.snapshot(&target).len(), 0);
    }
}
