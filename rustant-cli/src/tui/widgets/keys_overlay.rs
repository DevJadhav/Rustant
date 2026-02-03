//! Keyboard shortcuts overlay widget.
//!
//! Displays all keyboard shortcuts in a scrollable full-screen overlay.
//! Toggled with F1 or the /keys command.

use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// State for the keyboard shortcuts overlay.
#[derive(Debug, Clone, Default)]
pub struct KeysOverlay {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Scroll offset within the overlay.
    pub scroll_offset: u16,
}

impl KeysOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.scroll_offset = 0;
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }
}

/// Render the keyboard shortcuts overlay.
pub fn render_keys_overlay(frame: &mut Frame, area: Rect, overlay: &KeysOverlay, theme: &Theme) {
    if !overlay.visible || area.width < 20 || area.height < 10 {
        return;
    }

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Keyboard Shortcuts [F1 to close] ")
        .borders(Borders::ALL)
        .border_style(theme.border_style())
        .style(theme.base_style());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let section_style = theme.tool_call_style().add_modifier(Modifier::BOLD);
    let key_style = theme.assistant_message_style();
    let desc_style = theme.sidebar_style();

    let lines = vec![
        Line::from(Span::styled(" Global", section_style)),
        shortcut_line("  Ctrl+C / Ctrl+D", "Quit Rustant", key_style, desc_style),
        shortcut_line("  Ctrl+L", "Scroll to bottom", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled(" Input", section_style)),
        shortcut_line("  Enter", "Send message", key_style, desc_style),
        shortcut_line("  Shift+Enter", "New line", key_style, desc_style),
        shortcut_line(
            "  Up / Down",
            "Navigate input history",
            key_style,
            desc_style,
        ),
        shortcut_line("  @", "File autocomplete", key_style, desc_style),
        shortcut_line("  /", "Command palette", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled(" Navigation", section_style)),
        shortcut_line(
            "  Page Up / Down",
            "Scroll conversation",
            key_style,
            desc_style,
        ),
        shortcut_line(
            "  Home / End",
            "Start / end of input",
            key_style,
            desc_style,
        ),
        Line::from(""),
        Line::from(Span::styled(" Overlays", section_style)),
        shortcut_line(
            "  Ctrl+E",
            "Toggle explanation panel",
            key_style,
            desc_style,
        ),
        shortcut_line("  Ctrl+T", "Toggle task board", key_style, desc_style),
        shortcut_line("  Ctrl+S", "Show trust dashboard", key_style, desc_style),
        shortcut_line(
            "  F1",
            "Toggle this shortcuts overlay",
            key_style,
            desc_style,
        ),
        Line::from(""),
        Line::from(Span::styled(" Approval Mode", section_style)),
        shortcut_line("  y", "Approve action", key_style, desc_style),
        shortcut_line("  n", "Deny action", key_style, desc_style),
        shortcut_line("  a", "Approve all similar", key_style, desc_style),
        shortcut_line("  d", "Show diff preview", key_style, desc_style),
        shortcut_line("  ?", "Show approval help", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled(" Vim Mode (toggle with /vim)", section_style)),
        shortcut_line(
            "  i / a / I / A",
            "Enter insert mode",
            key_style,
            desc_style,
        ),
        shortcut_line("  Esc", "Return to normal mode", key_style, desc_style),
        shortcut_line("  j / k", "Scroll down / up", key_style, desc_style),
        shortcut_line("  q", "Quit", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled(
            " [Up/Down to scroll] [F1/Esc to close]",
            theme.status_bar_style(),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .scroll((overlay.scroll_offset, 0));

    frame.render_widget(paragraph, inner);
}

fn shortcut_line<'a>(
    key: &'a str,
    desc: &'a str,
    key_style: ratatui::style::Style,
    desc_style: ratatui::style::Style,
) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{:<24}", key), key_style),
        Span::styled(desc, desc_style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_default_hidden() {
        let overlay = KeysOverlay::new();
        assert!(!overlay.is_visible());
        assert_eq!(overlay.scroll_offset, 0);
    }

    #[test]
    fn test_overlay_toggle() {
        let mut overlay = KeysOverlay::new();
        overlay.toggle();
        assert!(overlay.is_visible());
        overlay.toggle();
        assert!(!overlay.is_visible());
    }

    #[test]
    fn test_overlay_toggle_resets_scroll() {
        let mut overlay = KeysOverlay::new();
        overlay.toggle(); // open
        overlay.scroll_down();
        overlay.scroll_down();
        assert_eq!(overlay.scroll_offset, 2);
        overlay.toggle(); // close
        overlay.toggle(); // re-open
        assert_eq!(overlay.scroll_offset, 0);
    }

    #[test]
    fn test_overlay_scroll() {
        let mut overlay = KeysOverlay::new();
        overlay.scroll_down();
        assert_eq!(overlay.scroll_offset, 1);
        overlay.scroll_down();
        assert_eq!(overlay.scroll_offset, 2);
        overlay.scroll_up();
        assert_eq!(overlay.scroll_offset, 1);
        overlay.scroll_up();
        assert_eq!(overlay.scroll_offset, 0);
        overlay.scroll_up(); // no underflow
        assert_eq!(overlay.scroll_offset, 0);
    }

    #[test]
    fn test_render_hidden_noop() {
        let backend = ratatui::backend::TestBackend::new(60, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let overlay = KeysOverlay::new();
        let theme = Theme::dark();
        terminal
            .draw(|frame| render_keys_overlay(frame, frame.area(), &overlay, &theme))
            .unwrap();
    }

    #[test]
    fn test_render_visible() {
        let backend = ratatui::backend::TestBackend::new(80, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut overlay = KeysOverlay::new();
        overlay.toggle();
        let theme = Theme::dark();
        terminal
            .draw(|frame| render_keys_overlay(frame, frame.area(), &overlay, &theme))
            .unwrap();
    }

    #[test]
    fn test_render_too_small_area_noop() {
        let backend = ratatui::backend::TestBackend::new(15, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut overlay = KeysOverlay::new();
        overlay.toggle();
        let theme = Theme::dark();
        // Should not panic on small area
        terminal
            .draw(|frame| render_keys_overlay(frame, frame.area(), &overlay, &theme))
            .unwrap();
    }
}
