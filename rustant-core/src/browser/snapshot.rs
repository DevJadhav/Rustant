//! Page snapshot types for capturing browser page state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The mode used for capturing a page snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotMode {
    /// Raw HTML source.
    Html,
    /// Accessibility / ARIA tree representation.
    AriaTree,
    /// Extracted text content.
    Text,
    /// Base64-encoded screenshot PNG.
    Screenshot,
}

impl std::fmt::Display for SnapshotMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotMode::Html => write!(f, "html"),
            SnapshotMode::AriaTree => write!(f, "aria_tree"),
            SnapshotMode::Text => write!(f, "text"),
            SnapshotMode::Screenshot => write!(f, "screenshot"),
        }
    }
}

/// A captured snapshot of a browser page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSnapshot {
    /// Current URL of the page.
    pub url: String,
    /// Page title.
    pub title: String,
    /// The snapshot mode used.
    pub mode: SnapshotMode,
    /// The captured content (HTML, text, ARIA tree, or base64 screenshot).
    pub content: String,
    /// When the snapshot was taken.
    pub timestamp: DateTime<Utc>,
}

impl PageSnapshot {
    /// Create a new page snapshot.
    pub fn new(
        url: impl Into<String>,
        title: impl Into<String>,
        mode: SnapshotMode,
        content: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            mode,
            content: content.into(),
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_mode_serde_roundtrip() {
        let modes = vec![
            SnapshotMode::Html,
            SnapshotMode::AriaTree,
            SnapshotMode::Text,
            SnapshotMode::Screenshot,
        ];
        for mode in &modes {
            let json = serde_json::to_string(mode).unwrap();
            let deserialized: SnapshotMode = serde_json::from_str(&json).unwrap();
            assert_eq!(*mode, deserialized);
        }
    }

    #[test]
    fn test_snapshot_mode_display() {
        assert_eq!(SnapshotMode::Html.to_string(), "html");
        assert_eq!(SnapshotMode::AriaTree.to_string(), "aria_tree");
        assert_eq!(SnapshotMode::Text.to_string(), "text");
        assert_eq!(SnapshotMode::Screenshot.to_string(), "screenshot");
    }

    #[test]
    fn test_page_snapshot_creation() {
        let snapshot = PageSnapshot::new(
            "https://example.com",
            "Example",
            SnapshotMode::Html,
            "<html><body>Hello</body></html>",
        );
        assert_eq!(snapshot.url, "https://example.com");
        assert_eq!(snapshot.title, "Example");
        assert_eq!(snapshot.mode, SnapshotMode::Html);
        assert_eq!(snapshot.content, "<html><body>Hello</body></html>");
    }

    #[test]
    fn test_page_snapshot_serde() {
        let snapshot = PageSnapshot::new(
            "https://docs.rs",
            "Docs.rs",
            SnapshotMode::Text,
            "Welcome to docs.rs",
        );
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: PageSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.url, snapshot.url);
        assert_eq!(deserialized.title, snapshot.title);
        assert_eq!(deserialized.mode, snapshot.mode);
        assert_eq!(deserialized.content, snapshot.content);
    }

    #[test]
    fn test_snapshot_mode_json_values() {
        assert_eq!(
            serde_json::to_string(&SnapshotMode::Html).unwrap(),
            "\"html\""
        );
        assert_eq!(
            serde_json::to_string(&SnapshotMode::AriaTree).unwrap(),
            "\"aria_tree\""
        );
        assert_eq!(
            serde_json::to_string(&SnapshotMode::Screenshot).unwrap(),
            "\"screenshot\""
        );
    }
}
