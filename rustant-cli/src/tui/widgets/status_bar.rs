//! Status bar widget showing keybinding hints and current mode.

use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// The current input mode of the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InputMode {
    Normal,
    VimNormal,
    VimInsert,
    CommandPalette,
    Autocomplete,
    Approval,
}

impl InputMode {
    /// Short display label for the status bar.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal => "INSERT",
            Self::VimNormal => "VIM-N",
            Self::VimInsert => "VIM-I",
            Self::CommandPalette => "CMD",
            Self::Autocomplete => "AUTO",
            Self::Approval => "APPROVE",
        }
    }

    /// Whether vim mode is active (either normal or insert).
    #[allow(dead_code)]
    pub fn is_vim(&self) -> bool {
        matches!(self, Self::VimNormal | Self::VimInsert)
    }
}

impl std::fmt::Display for InputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Render the status bar.
pub fn render_status_bar(frame: &mut Frame, area: Rect, mode: InputMode, theme: &Theme) {
    let hints = match mode {
        InputMode::Normal | InputMode::VimInsert => {
            "[Enter] Send │ [/] Commands │ [@] Files │ [Esc] Cancel │ [Ctrl+C] Quit"
        }
        InputMode::VimNormal => "[i] Insert │ [/] Search │ [:] Command │ [q] Quit",
        InputMode::CommandPalette => "[↑↓] Navigate │ [Enter] Select │ [Esc] Close",
        InputMode::Autocomplete => "[↑↓] Navigate │ [Tab] Accept │ [Esc] Close",
        InputMode::Approval => "[y] Approve │ [n] Deny │ [d] Diff │ [?] Help",
    };

    let spans = vec![
        Span::styled(
            format!(" {} ", mode.label()),
            theme
                .status_bar_style()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", theme.status_bar_style()),
        Span::styled(hints, theme.status_bar_style()),
    ];

    let bar = Paragraph::new(Line::from(spans)).style(theme.status_bar_style());
    frame.render_widget(bar, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_mode_labels() {
        assert_eq!(InputMode::Normal.label(), "INSERT");
        assert_eq!(InputMode::VimNormal.label(), "VIM-N");
        assert_eq!(InputMode::VimInsert.label(), "VIM-I");
        assert_eq!(InputMode::CommandPalette.label(), "CMD");
        assert_eq!(InputMode::Autocomplete.label(), "AUTO");
        assert_eq!(InputMode::Approval.label(), "APPROVE");
    }

    #[test]
    fn test_input_mode_display() {
        assert_eq!(format!("{}", InputMode::Normal), "INSERT");
        assert_eq!(format!("{}", InputMode::VimNormal), "VIM-N");
    }

    #[test]
    fn test_input_mode_is_vim() {
        assert!(!InputMode::Normal.is_vim());
        assert!(InputMode::VimNormal.is_vim());
        assert!(InputMode::VimInsert.is_vim());
        assert!(!InputMode::CommandPalette.is_vim());
    }

    #[test]
    fn test_render_status_bar_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                render_status_bar(frame, frame.area(), InputMode::Normal, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_status_bar_vim_mode() {
        let backend = ratatui::backend::TestBackend::new(80, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                render_status_bar(frame, frame.area(), InputMode::VimNormal, &theme);
            })
            .unwrap();
    }
}
