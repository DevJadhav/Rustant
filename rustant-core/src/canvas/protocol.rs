//! Canvas protocol — A2UI-inspired message types for agent-to-UI communication.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages exchanged between the agent and UI canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CanvasMessage {
    /// Push new content to the canvas.
    Push {
        target: CanvasTarget,
        content_type: ContentType,
        content: String,
    },
    /// Clear all content from a canvas target.
    Clear { target: CanvasTarget },
    /// Update existing content (partial replacement).
    Update {
        target: CanvasTarget,
        content_type: ContentType,
        content: String,
    },
    /// User interaction event from the UI.
    Interact {
        action: String,
        selector: String,
        data: serde_json::Value,
    },
    /// Event notification from UI to agent.
    Event {
        event_type: String,
        target: CanvasTarget,
        data: serde_json::Value,
    },
    /// Request the current canvas state.
    State { target: CanvasTarget },
    /// Full snapshot of canvas state.
    Snapshot {
        target: CanvasTarget,
        items: Vec<CanvasItem>,
    },
}

/// A content type for canvas items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Html,
    Markdown,
    Code,
    Chart,
    Table,
    Form,
    Image,
    Diagram,
}

impl ContentType {
    /// Parse a content type from a string.
    pub fn from_str_loose(s: &str) -> Option<ContentType> {
        match s.to_lowercase().as_str() {
            "html" => Some(ContentType::Html),
            "markdown" | "md" => Some(ContentType::Markdown),
            "code" => Some(ContentType::Code),
            "chart" => Some(ContentType::Chart),
            "table" => Some(ContentType::Table),
            "form" => Some(ContentType::Form),
            "image" | "img" => Some(ContentType::Image),
            "diagram" | "mermaid" => Some(ContentType::Diagram),
            _ => None,
        }
    }
}

/// Target for canvas operations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CanvasTarget {
    /// Broadcast to all connected canvases.
    #[default]
    Broadcast,
    /// Target a specific canvas by name.
    Named(String),
}

/// An item stored in the canvas state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasItem {
    pub id: Uuid,
    pub content_type: ContentType,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl CanvasItem {
    pub fn new(content_type: ContentType, content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            content_type,
            content,
            created_at: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_message_push_serialization() {
        let msg = CanvasMessage::Push {
            target: CanvasTarget::Broadcast,
            content_type: ContentType::Html,
            content: "<h1>Hello</h1>".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Push"));
        let restored: CanvasMessage = serde_json::from_str(&json).unwrap();
        match restored {
            CanvasMessage::Push {
                content_type,
                content,
                ..
            } => {
                assert_eq!(content_type, ContentType::Html);
                assert_eq!(content, "<h1>Hello</h1>");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_canvas_message_clear_serialization() {
        let msg = CanvasMessage::Clear {
            target: CanvasTarget::Named("main".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: CanvasMessage = serde_json::from_str(&json).unwrap();
        match restored {
            CanvasMessage::Clear { target } => {
                assert_eq!(target, CanvasTarget::Named("main".into()));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_canvas_message_update_serialization() {
        let msg = CanvasMessage::Update {
            target: CanvasTarget::Broadcast,
            content_type: ContentType::Markdown,
            content: "# Updated".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let _: CanvasMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_canvas_message_interact_serialization() {
        let msg = CanvasMessage::Interact {
            action: "click".into(),
            selector: "#submit".into(),
            data: serde_json::json!({"value": "ok"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let _: CanvasMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_canvas_message_event_serialization() {
        let msg = CanvasMessage::Event {
            event_type: "form_submit".into(),
            target: CanvasTarget::Broadcast,
            data: serde_json::json!({"field": "value"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let _: CanvasMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_canvas_message_state_serialization() {
        let msg = CanvasMessage::State {
            target: CanvasTarget::Broadcast,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let _: CanvasMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_canvas_message_snapshot_serialization() {
        let msg = CanvasMessage::Snapshot {
            target: CanvasTarget::Broadcast,
            items: vec![CanvasItem::new(ContentType::Html, "<p>Test</p>".into())],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: CanvasMessage = serde_json::from_str(&json).unwrap();
        match restored {
            CanvasMessage::Snapshot { items, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].content_type, ContentType::Html);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_content_type_parsing() {
        assert_eq!(ContentType::from_str_loose("html"), Some(ContentType::Html));
        assert_eq!(
            ContentType::from_str_loose("markdown"),
            Some(ContentType::Markdown)
        );
        assert_eq!(
            ContentType::from_str_loose("md"),
            Some(ContentType::Markdown)
        );
        assert_eq!(ContentType::from_str_loose("code"), Some(ContentType::Code));
        assert_eq!(
            ContentType::from_str_loose("chart"),
            Some(ContentType::Chart)
        );
        assert_eq!(
            ContentType::from_str_loose("table"),
            Some(ContentType::Table)
        );
        assert_eq!(ContentType::from_str_loose("form"), Some(ContentType::Form));
        assert_eq!(
            ContentType::from_str_loose("image"),
            Some(ContentType::Image)
        );
        assert_eq!(ContentType::from_str_loose("img"), Some(ContentType::Image));
        assert_eq!(
            ContentType::from_str_loose("diagram"),
            Some(ContentType::Diagram)
        );
        assert_eq!(
            ContentType::from_str_loose("mermaid"),
            Some(ContentType::Diagram)
        );
        assert_eq!(ContentType::from_str_loose("invalid"), None);
    }

    #[test]
    fn test_content_type_case_insensitive() {
        assert_eq!(ContentType::from_str_loose("HTML"), Some(ContentType::Html));
        assert_eq!(
            ContentType::from_str_loose("Chart"),
            Some(ContentType::Chart)
        );
    }

    #[test]
    fn test_canvas_target_default() {
        let target = CanvasTarget::default();
        assert_eq!(target, CanvasTarget::Broadcast);
    }
}
