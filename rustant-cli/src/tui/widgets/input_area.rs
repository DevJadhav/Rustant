//! Input area widget wrapping tui-textarea for multiline editing.

use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use tui_textarea::TextArea;

/// Input widget wrapping tui-textarea.
pub struct InputWidget {
    textarea: TextArea<'static>,
    history: Vec<String>,
    history_index: Option<usize>,
    draft: Option<String>,
}

/// Result of processing an input event.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum InputAction {
    /// User pressed Enter to submit the input.
    Submit(String),
    /// @ was typed — trigger autocomplete with the query after @.
    TriggerAutocomplete(String),
    /// / was typed at the beginning — trigger command palette.
    TriggerCommandPalette(String),
    /// Input was consumed by the textarea (no special action).
    Consumed,
    /// Input was not consumed (pass to the app).
    NotConsumed,
}

#[allow(dead_code)]
impl InputWidget {
    pub fn new(theme: &Theme) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(Style::default());
        textarea.set_style(Style::default().fg(theme.fg).bg(theme.bg));
        textarea.set_block(
            Block::default()
                .title(" > ")
                .borders(Borders::TOP)
                .border_style(theme.border_style()),
        );

        Self {
            textarea,
            history: Vec::new(),
            history_index: None,
            draft: None,
        }
    }

    /// Get the current input text.
    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Check if input is empty.
    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    /// Clear the input and reset history navigation state.
    pub fn clear(&mut self) {
        self.clear_textarea();
        self.history_index = None;
        self.draft = None;
    }

    /// Clear just the textarea content without resetting history state.
    fn clear_textarea(&mut self) {
        self.textarea.select_all();
        self.textarea.cut();
    }

    /// Set the input text.
    pub fn set_text(&mut self, text: &str) {
        self.clear_textarea();
        self.textarea.insert_str(text);
    }

    /// Insert text at cursor position.
    pub fn insert(&mut self, text: &str) {
        self.textarea.insert_str(text);
    }

    /// Process a crossterm event. Returns the resulting action.
    pub fn handle_event(&mut self, event: &Event) -> InputAction {
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                ..
            }) => {
                if modifiers.contains(KeyModifiers::SHIFT) || modifiers.contains(KeyModifiers::ALT)
                {
                    // Shift+Enter or Alt+Enter: newline
                    self.textarea.insert_newline();
                    InputAction::Consumed
                } else {
                    // Enter: submit
                    let text = self.text();
                    if text.trim().is_empty() {
                        return InputAction::Consumed;
                    }
                    self.add_to_history(&text);
                    self.clear();
                    InputAction::Submit(text)
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            }) => {
                // Only navigate history if on first line
                let cursor = self.textarea.cursor();
                if cursor.0 == 0 {
                    self.history_prev();
                    InputAction::Consumed
                } else {
                    self.textarea.input(event.clone());
                    InputAction::Consumed
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            }) => {
                let cursor = self.textarea.cursor();
                let last_line = self.textarea.lines().len().saturating_sub(1);
                if cursor.0 == last_line {
                    self.history_next();
                    InputAction::Consumed
                } else {
                    self.textarea.input(event.clone());
                    InputAction::Consumed
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('@'),
                ..
            }) => {
                self.textarea.input(event.clone());
                // Extract query after @ for autocomplete
                let text = self.text();
                if let Some(pos) = text.rfind('@') {
                    let query = text[pos + 1..].to_string();
                    InputAction::TriggerAutocomplete(query)
                } else {
                    InputAction::Consumed
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('/'),
                ..
            }) => {
                let text = self.text();
                if text.is_empty() {
                    self.textarea.input(event.clone());
                    let full = self.text();
                    InputAction::TriggerCommandPalette(full[1..].to_string())
                } else {
                    self.textarea.input(event.clone());
                    InputAction::Consumed
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                ..
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Delete,
                ..
            }) => {
                self.textarea.input(event.clone());
                self.sanitize_after_delete();
                InputAction::Consumed
            }
            _ => {
                self.textarea.input(event.clone());
                InputAction::Consumed
            }
        }
    }

    /// Remove trailing phantom empty lines that may appear after delete/backspace.
    ///
    /// When tui-textarea processes backspace at certain positions, it can leave
    /// trailing empty lines in its internal line array. This method detects and
    /// removes them, unless the cursor is actually on those lines.
    fn sanitize_after_delete(&mut self) {
        let lines = self.textarea.lines().to_vec();
        let cursor = self.textarea.cursor();
        // Find how many trailing empty lines exist beyond the cursor
        let mut end = lines.len();
        while end > 1 && lines[end - 1].is_empty() && cursor.0 < end - 1 {
            end -= 1;
        }
        if end < lines.len() {
            let text: String = lines[..end].join("\n");
            let saved_cursor = cursor;
            self.clear_textarea();
            if !text.is_empty() {
                self.textarea.insert_str(&text);
            }
            // Restore cursor position
            self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                saved_cursor.0 as u16,
                saved_cursor.1 as u16,
            ));
        }
    }

    /// Navigate to the previous history entry.
    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.draft = Some(self.text());
                self.history_index = Some(self.history.len() - 1);
            }
            Some(0) => return,
            Some(ref mut idx) => {
                *idx -= 1;
            }
        }
        if let Some(idx) = self.history_index {
            self.set_text(&self.history[idx].clone());
        }
    }

    /// Navigate to the next history entry.
    fn history_next(&mut self) {
        match self.history_index {
            None => {}
            Some(idx) if idx >= self.history.len() - 1 => {
                self.history_index = None;
                if let Some(ref draft) = self.draft.clone() {
                    self.set_text(draft);
                } else {
                    self.clear();
                }
                self.draft = None;
            }
            Some(ref mut idx) => {
                *idx += 1;
                let text = self.history[*idx].clone();
                self.set_text(&text);
            }
        }
    }

    /// Add an entry to history.
    pub fn add_to_history(&mut self, text: &str) {
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        // Deduplicate against last entry
        if self.history.last() != Some(&trimmed) {
            self.history.push(trimmed);
        }
        self.history_index = None;
        self.draft = None;
    }

    /// Get the full history.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Load history from a list of strings.
    pub fn load_history(&mut self, entries: Vec<String>) {
        self.history = entries;
    }

    /// Render the input widget.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(&self.textarea, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input() -> InputWidget {
        InputWidget::new(&Theme::dark())
    }

    fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn key_event_with_mod(code: KeyCode, mods: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(code, mods))
    }

    #[test]
    fn test_new_input_is_empty() {
        let input = make_input();
        assert!(input.is_empty());
        assert_eq!(input.text(), "");
    }

    #[test]
    fn test_set_and_get_text() {
        let mut input = make_input();
        input.set_text("hello world");
        assert_eq!(input.text(), "hello world");
        assert!(!input.is_empty());
    }

    #[test]
    fn test_clear() {
        let mut input = make_input();
        input.set_text("hello");
        input.clear();
        assert!(input.is_empty());
    }

    #[test]
    fn test_submit_on_enter() {
        let mut input = make_input();
        // Type some text
        for ch in "hello".chars() {
            input.handle_event(&key_event(KeyCode::Char(ch)));
        }
        let result = input.handle_event(&key_event(KeyCode::Enter));
        assert_eq!(result, InputAction::Submit("hello".to_string()));
        assert!(input.is_empty());
    }

    #[test]
    fn test_empty_enter_does_not_submit() {
        let mut input = make_input();
        let result = input.handle_event(&key_event(KeyCode::Enter));
        assert_eq!(result, InputAction::Consumed);
    }

    #[test]
    fn test_shift_enter_inserts_newline() {
        let mut input = make_input();
        input.set_text("line1");
        let result = input.handle_event(&key_event_with_mod(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(result, InputAction::Consumed);
        assert!(input.text().contains('\n'));
    }

    #[test]
    fn test_at_triggers_autocomplete() {
        let mut input = make_input();
        let result = input.handle_event(&key_event(KeyCode::Char('@')));
        match result {
            InputAction::TriggerAutocomplete(q) => assert_eq!(q, ""),
            other => panic!("Expected TriggerAutocomplete, got {:?}", other),
        }
    }

    #[test]
    fn test_slash_at_beginning_triggers_command_palette() {
        let mut input = make_input();
        let result = input.handle_event(&key_event(KeyCode::Char('/')));
        match result {
            InputAction::TriggerCommandPalette(q) => assert_eq!(q, ""),
            other => panic!("Expected TriggerCommandPalette, got {:?}", other),
        }
    }

    #[test]
    fn test_slash_mid_text_does_not_trigger() {
        let mut input = make_input();
        input.set_text("hello");
        let result = input.handle_event(&key_event(KeyCode::Char('/')));
        assert_eq!(result, InputAction::Consumed);
    }

    #[test]
    fn test_history_navigation() {
        let mut input = make_input();
        input.add_to_history("first");
        input.add_to_history("second");
        input.add_to_history("third");

        // Navigate up
        input.history_prev();
        assert_eq!(input.text(), "third");

        input.history_prev();
        assert_eq!(input.text(), "second");

        input.history_prev();
        assert_eq!(input.text(), "first");

        // At beginning, should stay
        input.history_prev();
        assert_eq!(input.text(), "first");

        // Navigate back down
        input.history_next();
        assert_eq!(input.text(), "second");

        input.history_next();
        assert_eq!(input.text(), "third");

        // Back to draft
        input.history_next();
        assert_eq!(input.text(), "");
    }

    #[test]
    fn test_history_dedup() {
        let mut input = make_input();
        input.add_to_history("same");
        input.add_to_history("same");
        assert_eq!(input.history().len(), 1);
    }

    #[test]
    fn test_history_empty_not_added() {
        let mut input = make_input();
        input.add_to_history("");
        input.add_to_history("   ");
        assert!(input.history().is_empty());
    }

    #[test]
    fn test_load_history() {
        let mut input = make_input();
        input.load_history(vec!["a".into(), "b".into()]);
        assert_eq!(input.history().len(), 2);
    }

    #[test]
    fn test_history_preserves_draft() {
        let mut input = make_input();
        input.add_to_history("old");
        input.set_text("current draft");

        input.history_prev();
        assert_eq!(input.text(), "old");

        input.history_next();
        assert_eq!(input.text(), "current draft");
    }

    #[test]
    fn test_backspace_removes_character() {
        let mut input = make_input();
        for ch in "hello".chars() {
            input.handle_event(&key_event(KeyCode::Char(ch)));
        }
        assert_eq!(input.text(), "hello");
        input.handle_event(&key_event(KeyCode::Backspace));
        assert_eq!(input.text(), "hell");
        // No extra lines should appear
        assert_eq!(input.textarea.lines().len(), 1);
    }

    #[test]
    fn test_delete_no_phantom_lines() {
        let mut input = make_input();
        for ch in "hello world".chars() {
            input.handle_event(&key_event(KeyCode::Char(ch)));
        }
        // Backspace 5 times to remove "world"
        for _ in 0..5 {
            input.handle_event(&key_event(KeyCode::Backspace));
        }
        let text = input.text();
        assert_eq!(text, "hello ");
        // Should be a single line, no phantom empty lines
        assert_eq!(input.textarea.lines().len(), 1);
    }

    #[test]
    fn test_backspace_multiline_no_artifacts() {
        let mut input = make_input();
        input.set_text("line1");
        // Add a newline
        input.handle_event(&key_event_with_mod(KeyCode::Enter, KeyModifiers::SHIFT));
        for ch in "line2".chars() {
            input.handle_event(&key_event(KeyCode::Char(ch)));
        }
        assert!(input.text().contains('\n'));
        // Delete characters from line2
        input.handle_event(&key_event(KeyCode::Backspace));
        input.handle_event(&key_event(KeyCode::Backspace));
        // Should have 2 lines, no phantom third line
        assert!(input.textarea.lines().len() <= 2);
    }

    #[test]
    fn test_render_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 5);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let input = make_input();
        terminal
            .draw(|frame| {
                input.render(frame, frame.area());
            })
            .unwrap();
    }
}
