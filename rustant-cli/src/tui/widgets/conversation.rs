//! Conversation pane widget - scrollable message list.

use crate::tui::theme::Theme;
use crate::tui::widgets::markdown::{render_markdown, SyntaxHighlighter};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use rustant_core::types::{Content, Message, Role};

/// A displayable message in the conversation.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: Role,
    pub text: String,
    pub tool_name: Option<String>,
    pub is_error: bool,
    pub timestamp: String,
}

impl From<&Message> for DisplayMessage {
    fn from(msg: &Message) -> Self {
        let text = match &msg.content {
            Content::Text { text } => text.clone(),
            Content::ToolCall {
                name, arguments, ..
            } => {
                format!("[Calling {}] {}", name, arguments)
            }
            Content::ToolResult {
                output, is_error, ..
            } => {
                if *is_error {
                    format!("[Error] {}", output)
                } else {
                    output.clone()
                }
            }
            Content::MultiPart { parts } => parts
                .iter()
                .filter_map(|p| p.as_text())
                .collect::<Vec<_>>()
                .join("\n"),
        };

        let tool_name = match &msg.content {
            Content::ToolCall { name, .. } => Some(name.clone()),
            _ => None,
        };

        let is_error = matches!(&msg.content, Content::ToolResult { is_error: true, .. });

        Self {
            role: msg.role,
            text,
            tool_name,
            is_error,
            timestamp: msg.timestamp.format("%H:%M:%S").to_string(),
        }
    }
}

/// State for the conversation pane.
pub struct ConversationState {
    pub messages: Vec<DisplayMessage>,
    pub streaming_buffer: String,
    pub scroll_offset: u16,
    pub auto_scroll: bool,
}

impl ConversationState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            streaming_buffer: String::new(),
            scroll_offset: 0,
            auto_scroll: true,
        }
    }

    pub fn push_message(&mut self, msg: DisplayMessage) {
        self.messages.push(msg);
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn append_stream_token(&mut self, token: &str) {
        self.streaming_buffer.push_str(token);
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn finish_streaming(&mut self) {
        if !self.streaming_buffer.is_empty() {
            let msg = DisplayMessage {
                role: Role::Assistant,
                text: std::mem::take(&mut self.streaming_buffer),
                tool_name: None,
                is_error: false,
                timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            };
            self.messages.push(msg);
        }
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        if self.scroll_offset == 0 {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = true;
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.streaming_buffer.clear();
        self.scroll_offset = 0;
        self.auto_scroll = true;
    }
}

impl Default for ConversationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the conversation pane.
pub fn render_conversation(
    frame: &mut Frame,
    area: Rect,
    state: &ConversationState,
    theme: &Theme,
    highlighter: &SyntaxHighlighter,
) {
    let block = Block::default()
        .title(" Conversation ")
        .borders(Borders::NONE)
        .style(theme.base_style());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut all_lines: Vec<Line> = Vec::new();

    for msg in &state.messages {
        render_message_lines(&mut all_lines, msg, theme, highlighter);
        all_lines.push(Line::from("")); // spacing between messages
    }

    // Streaming buffer
    if !state.streaming_buffer.is_empty() {
        all_lines.push(Line::from(vec![
            Span::styled(
                "Rustant: ",
                theme.assistant_message_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw(""),
        ]));
        let md_lines = render_markdown(&state.streaming_buffer, theme, highlighter);
        all_lines.extend(md_lines);
    }

    // Calculate scroll: show the bottom of the conversation
    let total_lines = all_lines.len() as u16;
    let visible = inner.height;
    let scroll = if state.auto_scroll {
        total_lines.saturating_sub(visible)
    } else {
        let max_scroll = total_lines.saturating_sub(visible);
        max_scroll.saturating_sub(state.scroll_offset)
    };

    let text = Text::from(all_lines);
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, inner);
}

/// Render lines for a single message.
fn render_message_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    msg: &DisplayMessage,
    theme: &Theme,
    highlighter: &SyntaxHighlighter,
) {
    match msg.role {
        Role::User => {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", msg.timestamp),
                    Style::default().fg(theme.system_msg_fg),
                ),
                Span::styled("You: ", theme.user_message_style()),
            ]));
            lines.push(Line::from(Span::styled(
                msg.text.clone(),
                Style::default().fg(theme.user_msg_fg),
            )));
        }
        Role::Assistant => {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", msg.timestamp),
                    Style::default().fg(theme.system_msg_fg),
                ),
                Span::styled(
                    "Rustant: ",
                    theme.assistant_message_style().add_modifier(Modifier::BOLD),
                ),
            ]));
            let md_lines = render_markdown(&msg.text, theme, highlighter);
            lines.extend(md_lines);
        }
        Role::Tool => {
            let prefix = if let Some(ref name) = msg.tool_name {
                format!("[{}] ", name)
            } else {
                "[tool] ".to_string()
            };
            let style = if msg.is_error {
                theme.error_style()
            } else {
                theme.tool_result_style()
            };
            lines.push(Line::from(Span::styled(prefix, theme.tool_call_style())));

            // Truncate tool output for display
            let display_text = if msg.text.len() > 500 {
                format!("{}...", &msg.text[..500])
            } else {
                msg.text.clone()
            };
            lines.push(Line::from(Span::styled(display_text, style)));
        }
        Role::System => {
            lines.push(Line::from(Span::styled(
                format!("[system] {}", msg.text),
                Style::default().fg(theme.system_msg_fg),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_state_new() {
        let state = ConversationState::new();
        assert!(state.messages.is_empty());
        assert!(state.streaming_buffer.is_empty());
        assert_eq!(state.scroll_offset, 0);
        assert!(state.auto_scroll);
    }

    #[test]
    fn test_push_message() {
        let mut state = ConversationState::new();
        state.push_message(DisplayMessage {
            role: Role::User,
            text: "hello".to_string(),
            tool_name: None,
            is_error: false,
            timestamp: "12:00:00".to_string(),
        });
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "hello");
    }

    #[test]
    fn test_streaming() {
        let mut state = ConversationState::new();
        state.append_stream_token("Hello ");
        state.append_stream_token("world");
        assert_eq!(state.streaming_buffer, "Hello world");

        state.finish_streaming();
        assert!(state.streaming_buffer.is_empty());
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "Hello world");
        assert_eq!(state.messages[0].role, Role::Assistant);
    }

    #[test]
    fn test_finish_streaming_empty_noop() {
        let mut state = ConversationState::new();
        state.finish_streaming();
        assert!(state.messages.is_empty());
    }

    #[test]
    fn test_scroll_up_down() {
        let mut state = ConversationState::new();
        assert!(state.auto_scroll);

        state.scroll_up(5);
        assert!(!state.auto_scroll);
        assert_eq!(state.scroll_offset, 5);

        state.scroll_down(3);
        assert_eq!(state.scroll_offset, 2);
        assert!(!state.auto_scroll);

        state.scroll_down(2);
        assert_eq!(state.scroll_offset, 0);
        assert!(state.auto_scroll);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut state = ConversationState::new();
        state.scroll_up(10);
        state.scroll_to_bottom();
        assert_eq!(state.scroll_offset, 0);
        assert!(state.auto_scroll);
    }

    #[test]
    fn test_clear() {
        let mut state = ConversationState::new();
        state.push_message(DisplayMessage {
            role: Role::User,
            text: "test".to_string(),
            tool_name: None,
            is_error: false,
            timestamp: "12:00:00".to_string(),
        });
        state.append_stream_token("streaming");
        state.clear();
        assert!(state.messages.is_empty());
        assert!(state.streaming_buffer.is_empty());
    }

    #[test]
    fn test_display_message_from_text() {
        let msg = Message::user("hello");
        let display: DisplayMessage = (&msg).into();
        assert_eq!(display.role, Role::User);
        assert_eq!(display.text, "hello");
        assert!(!display.is_error);
    }

    #[test]
    fn test_display_message_from_tool_call() {
        let msg = Message::new(
            Role::Assistant,
            Content::tool_call("id1", "file_read", serde_json::json!({"path": "foo.rs"})),
        );
        let display: DisplayMessage = (&msg).into();
        assert_eq!(display.tool_name, Some("file_read".to_string()));
    }

    #[test]
    fn test_display_message_from_tool_error() {
        let msg = Message::tool_result("id1", "not found", true);
        let display: DisplayMessage = (&msg).into();
        assert!(display.is_error);
    }

    #[test]
    fn test_render_conversation_empty() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = ConversationState::new();
        let theme = Theme::dark();
        let hl = SyntaxHighlighter::new();
        terminal
            .draw(|frame| {
                render_conversation(frame, frame.area(), &state, &theme, &hl);
            })
            .unwrap();
    }

    #[test]
    fn test_render_conversation_with_messages() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut state = ConversationState::new();
        state.push_message(DisplayMessage {
            role: Role::User,
            text: "Refactor auth module".to_string(),
            tool_name: None,
            is_error: false,
            timestamp: "12:00:00".to_string(),
        });
        state.push_message(DisplayMessage {
            role: Role::Assistant,
            text: "I'll help with that. Let me **read** the file.".to_string(),
            tool_name: None,
            is_error: false,
            timestamp: "12:00:01".to_string(),
        });
        let theme = Theme::dark();
        let hl = SyntaxHighlighter::new();
        terminal
            .draw(|frame| {
                render_conversation(frame, frame.area(), &state, &theme, &hl);
            })
            .unwrap();
    }
}
