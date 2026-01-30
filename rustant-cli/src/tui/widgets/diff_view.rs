//! Diff preview widget for showing file changes.
//!
//! Renders unified diff output with color-coded additions, removals,
//! and context lines.

use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Frame;

/// State for the diff viewer.
#[allow(dead_code)]
pub struct DiffView {
    /// Raw diff text.
    diff_text: String,
    /// Scroll offset.
    scroll: usize,
    /// Whether the diff view is visible.
    visible: bool,
}

#[allow(dead_code)]
impl DiffView {
    /// Create a new diff view.
    pub fn new() -> Self {
        Self {
            diff_text: String::new(),
            scroll: 0,
            visible: false,
        }
    }

    /// Show the diff view with the given diff text.
    pub fn show(&mut self, diff_text: String) {
        self.diff_text = diff_text;
        self.scroll = 0;
        self.visible = true;
    }

    /// Hide the diff view.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Whether the diff view is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Scroll up.
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    /// Scroll down.
    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.diff_text.lines().count().saturating_sub(1);
        self.scroll = (self.scroll + amount).min(max);
    }

    /// Get the diff text.
    pub fn text(&self) -> &str {
        &self.diff_text
    }

    /// Render the diff view as a centered popup.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.visible || self.diff_text.is_empty() {
            return;
        }

        // Create a centered popup that takes 80% of the screen
        let width = (area.width as f32 * 0.8) as u16;
        let height = (area.height as f32 * 0.8) as u16;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let lines = render_diff_lines(&self.diff_text, theme);

        let block = Block::default()
            .title(" Diff Preview [Esc to close] ")
            .borders(Borders::ALL)
            .border_style(theme.border_style())
            .style(Style::default().bg(theme.bg));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((self.scroll as u16, 0));

        frame.render_widget(paragraph, popup_area);

        // Scrollbar
        let total_lines = self.diff_text.lines().count();
        let content_height = popup_area.height.saturating_sub(2) as usize;
        if total_lines > content_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::new(total_lines).position(self.scroll);
            let scrollbar_area = Rect::new(
                popup_area.x + popup_area.width - 1,
                popup_area.y + 1,
                1,
                popup_area.height - 2,
            );
            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }
}

/// Render diff text into colored Lines.
#[allow(dead_code)]
fn render_diff_lines<'a>(diff_text: &str, theme: &Theme) -> Vec<Line<'a>> {
    diff_text
        .lines()
        .map(|line| {
            if line.starts_with('+') && !line.starts_with("+++") {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.diff_add_fg),
                ))
            } else if line.starts_with('-') && !line.starts_with("---") {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.diff_remove_fg),
                ))
            } else if line.starts_with("@@") {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if line.starts_with("diff ") || line.starts_with("index ") {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default()
                        .fg(theme.info_fg)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.diff_context_fg),
                ))
            }
        })
        .collect()
}

/// An approval dialog widget that shows when tool execution requires approval.
#[allow(dead_code)]
pub struct ApprovalDialog {
    /// Description of the action.
    pub description: String,
    /// Risk level.
    pub risk_level: String,
    /// Diff text (if available).
    pub diff_text: Option<String>,
    /// Whether dialog is visible.
    visible: bool,
}

#[allow(dead_code)]
impl ApprovalDialog {
    /// Create a new approval dialog.
    pub fn new() -> Self {
        Self {
            description: String::new(),
            risk_level: String::new(),
            diff_text: None,
            visible: false,
        }
    }

    /// Show the approval dialog.
    pub fn show(&mut self, description: String, risk_level: String, diff_text: Option<String>) {
        self.description = description;
        self.risk_level = risk_level;
        self.diff_text = diff_text;
        self.visible = true;
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Whether the dialog is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Render the approval dialog.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.visible {
            return;
        }

        let width = 60.min(area.width.saturating_sub(4));
        let height = 8;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Action: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(&self.description, Style::default().fg(theme.fg)),
            ]),
            Line::from(vec![
                Span::styled("  Risk:   ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(&self.risk_level, Style::default().fg(theme.warning_fg)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  [y] Approve  [n] Deny  [d] Show Diff  [?] Help",
                Style::default().fg(theme.accent),
            )),
            Line::from(""),
        ];

        let block = Block::default()
            .title(" Approval Required ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.warning_fg))
            .style(Style::default().bg(theme.bg));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_view_new() {
        let dv = DiffView::new();
        assert!(!dv.is_visible());
        assert!(dv.text().is_empty());
    }

    #[test]
    fn test_diff_view_show_hide() {
        let mut dv = DiffView::new();
        dv.show("+ added\n- removed\n context".to_string());
        assert!(dv.is_visible());
        assert!(!dv.text().is_empty());
        dv.hide();
        assert!(!dv.is_visible());
    }

    #[test]
    fn test_diff_view_scrolling() {
        let mut dv = DiffView::new();
        let long_diff = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        dv.show(long_diff);
        assert_eq!(dv.scroll, 0);
        dv.scroll_down(5);
        assert_eq!(dv.scroll, 5);
        dv.scroll_up(3);
        assert_eq!(dv.scroll, 2);
        dv.scroll_up(10);
        assert_eq!(dv.scroll, 0);
    }

    #[test]
    fn test_diff_render_lines() {
        let theme = Theme::dark();
        let diff = "+ added\n- removed\n@@ -1,3 +1,3 @@\n context\ndiff --git a/f b/f";
        let lines = render_diff_lines(diff, &theme);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_render_diff_view_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut dv = DiffView::new();
        dv.show("+ added\n- removed".to_string());
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                dv.render(frame, frame.area(), &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_hidden_diff_view_is_noop() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let dv = DiffView::new();
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                dv.render(frame, frame.area(), &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_approval_dialog_new() {
        let ad = ApprovalDialog::new();
        assert!(!ad.is_visible());
    }

    #[test]
    fn test_approval_dialog_show_hide() {
        let mut ad = ApprovalDialog::new();
        ad.show("write to file.rs".to_string(), "Write".to_string(), None);
        assert!(ad.is_visible());
        assert_eq!(ad.description, "write to file.rs");
        ad.hide();
        assert!(!ad.is_visible());
    }

    #[test]
    fn test_render_approval_dialog_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut ad = ApprovalDialog::new();
        ad.show("test action".to_string(), "Execute".to_string(), None);
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                ad.render(frame, frame.area(), &theme);
            })
            .unwrap();
    }
}
