//! / command palette widget for slash commands.

use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};
use ratatui::Frame;

/// A command palette entry.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    /// The slash command (e.g., "/help").
    pub command: String,
    /// Short description.
    pub description: String,
}

/// State for the command palette.
pub struct CommandPalette {
    /// All available commands.
    commands: Vec<CommandEntry>,
    /// Filtered commands based on current query.
    filtered: Vec<usize>,
    /// Current query (text after /).
    query: String,
    /// Selected index.
    selected: usize,
    /// Whether the palette is active.
    active: bool,
}

#[allow(dead_code)]
impl CommandPalette {
    /// Create a new command palette with default commands.
    pub fn new() -> Self {
        let commands = vec![
            CommandEntry {
                command: "/help".into(),
                description: "Show available commands".into(),
            },
            CommandEntry {
                command: "/quit".into(),
                description: "Exit Rustant".into(),
            },
            CommandEntry {
                command: "/clear".into(),
                description: "Clear conversation".into(),
            },
            CommandEntry {
                command: "/cost".into(),
                description: "Show token usage and cost".into(),
            },
            CommandEntry {
                command: "/tools".into(),
                description: "List available tools".into(),
            },
            CommandEntry {
                command: "/sidebar".into(),
                description: "Toggle sidebar".into(),
            },
            CommandEntry {
                command: "/vim".into(),
                description: "Toggle vim mode".into(),
            },
            CommandEntry {
                command: "/theme".into(),
                description: "Switch theme (dark/light)".into(),
            },
            CommandEntry {
                command: "/save".into(),
                description: "Save current session".into(),
            },
            CommandEntry {
                command: "/load".into(),
                description: "Load a saved session".into(),
            },
            CommandEntry {
                command: "/undo".into(),
                description: "Undo last file change".into(),
            },
            CommandEntry {
                command: "/audit".into(),
                description: "Show audit trail".into(),
            },
            CommandEntry {
                command: "/audit export".into(),
                description: "Export traces (json/csv/text)".into(),
            },
            CommandEntry {
                command: "/audit query".into(),
                description: "Query traces by tool name".into(),
            },
            CommandEntry {
                command: "/analytics".into(),
                description: "Show usage analytics and patterns".into(),
            },
            CommandEntry {
                command: "/replay".into(),
                description: "Start or show replay".into(),
            },
            CommandEntry {
                command: "/replay next".into(),
                description: "Step forward in replay".into(),
            },
            CommandEntry {
                command: "/replay prev".into(),
                description: "Step backward in replay".into(),
            },
            CommandEntry {
                command: "/replay timeline".into(),
                description: "Show full replay timeline".into(),
            },
            CommandEntry {
                command: "/replay reset".into(),
                description: "Clear replay session".into(),
            },
            CommandEntry {
                command: "/channel-setup".into(),
                description: "Set up a messaging channel (run from CLI)".into(),
            },
        ];

        let filtered: Vec<usize> = (0..commands.len()).collect();

        Self {
            commands,
            filtered,
            query: String::new(),
            selected: 0,
            active: false,
        }
    }

    /// Activate the command palette.
    pub fn activate(&mut self, query: &str) {
        self.active = true;
        self.query = query.to_string();
        self.filter();
    }

    /// Deactivate the command palette.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.selected = 0;
        self.filtered = (0..self.commands.len()).collect();
    }

    /// Whether the palette is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Update the query and refresh filtered list.
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.filter();
    }

    /// Get filtered commands.
    pub fn filtered_commands(&self) -> Vec<&CommandEntry> {
        self.filtered.iter().map(|&i| &self.commands[i]).collect()
    }

    /// Get the currently selected command.
    pub fn selected_command(&self) -> Option<&CommandEntry> {
        self.filtered.get(self.selected).map(|&i| &self.commands[i])
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() && self.selected < self.filtered.len() - 1 {
            self.selected += 1;
        }
    }

    /// Accept the currently selected command. Returns the command string.
    pub fn accept(&mut self) -> Option<String> {
        let result = self.selected_command().map(|c| c.command.clone());
        self.deactivate();
        result
    }

    /// Filter commands based on the current query.
    fn filter(&mut self) {
        self.selected = 0;
        let query_lower = self.query.to_lowercase();

        self.filtered = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| {
                if query_lower.is_empty() {
                    return true;
                }
                let cmd_lower = cmd.command.to_lowercase();
                let desc_lower = cmd.description.to_lowercase();
                // Match if query is substring of command (without /) or description
                let cmd_body = cmd_lower.trim_start_matches('/');
                cmd_body.contains(&query_lower) || desc_lower.contains(&query_lower)
            })
            .map(|(i, _)| i)
            .collect();
    }

    /// Render the command palette popup.
    pub fn render(&self, frame: &mut Frame, anchor: Rect, theme: &Theme) {
        if !self.active || self.filtered.is_empty() {
            return;
        }

        let height = (self.filtered.len() as u16 + 2).min(14);
        let width = 50.min(anchor.width);

        // Position popup above the input area
        let popup_y = anchor.y.saturating_sub(height);
        let popup_area = Rect::new(anchor.x + 1, popup_y, width, height);

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(i, &cmd_idx)| {
                let cmd = &self.commands[cmd_idx];
                let style = if i == self.selected {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(theme.fg)
                };
                let desc_style = if i == self.selected {
                    style
                } else {
                    Style::default().fg(theme.system_msg_fg)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", cmd.command), style),
                    Span::styled(format!("— {}", cmd.description), desc_style),
                ]))
            })
            .collect();

        let block = Block::default()
            .title(" Commands ")
            .borders(Borders::ALL)
            .border_style(theme.border_style())
            .style(Style::default().bg(theme.bg));

        let mut state = ListState::default();
        state.select(Some(self.selected));

        let list = List::new(items).block(block);
        frame.render_stateful_widget(list, popup_area, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_palette_new() {
        let cp = CommandPalette::new();
        assert!(!cp.is_active());
        assert!(!cp.commands.is_empty());
    }

    #[test]
    fn test_activate_deactivate() {
        let mut cp = CommandPalette::new();
        cp.activate("");
        assert!(cp.is_active());
        cp.deactivate();
        assert!(!cp.is_active());
    }

    #[test]
    fn test_filter_all() {
        let mut cp = CommandPalette::new();
        cp.activate("");
        assert_eq!(cp.filtered_commands().len(), cp.commands.len());
    }

    #[test]
    fn test_filter_by_query() {
        let mut cp = CommandPalette::new();
        cp.activate("help");
        let filtered = cp.filtered_commands();
        assert!(!filtered.is_empty());
        assert!(filtered.iter().any(|c| c.command == "/help"));
    }

    #[test]
    fn test_filter_no_match() {
        let mut cp = CommandPalette::new();
        cp.activate("zzzznonexistent");
        assert!(cp.filtered_commands().is_empty());
    }

    #[test]
    fn test_move_up_down() {
        let mut cp = CommandPalette::new();
        cp.activate("");
        assert_eq!(cp.selected, 0);
        cp.move_down();
        assert_eq!(cp.selected, 1);
        cp.move_up();
        assert_eq!(cp.selected, 0);
        cp.move_up();
        assert_eq!(cp.selected, 0);
    }

    #[test]
    fn test_accept_returns_command() {
        let mut cp = CommandPalette::new();
        cp.activate("");
        let result = cp.accept();
        assert!(result.is_some());
        assert!(result.unwrap().starts_with('/'));
        assert!(!cp.is_active());
    }

    #[test]
    fn test_update_query() {
        let mut cp = CommandPalette::new();
        cp.activate("");
        let all = cp.filtered_commands().len();
        cp.update_query("quit");
        let filtered = cp.filtered_commands().len();
        assert!(filtered <= all);
        assert!(filtered > 0);
    }

    #[test]
    fn test_selected_command() {
        let mut cp = CommandPalette::new();
        cp.activate("");
        assert!(cp.selected_command().is_some());
    }

    #[test]
    fn test_render_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut cp = CommandPalette::new();
        cp.activate("");
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                let area = Rect::new(0, 20, 80, 4);
                cp.render(frame, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_filter_by_description() {
        let mut cp = CommandPalette::new();
        cp.activate("token");
        let filtered = cp.filtered_commands();
        // Should match /cost which has "token" in description
        assert!(filtered.iter().any(|c| c.command == "/cost"));
    }
}
