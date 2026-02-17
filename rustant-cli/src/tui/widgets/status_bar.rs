//! Status bar widget showing keybinding hints and current mode.

use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

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

/// Metrics data for the status bar.
#[derive(Debug, Clone, Default)]
pub struct StatusBarData {
    /// Total tokens consumed so far.
    pub tokens_used: usize,
    /// Context window size in tokens.
    pub context_window: usize,
    /// Cumulative cost in USD.
    pub cost_usd: f64,
    /// Whether voice command mode is active.
    pub voice_active: bool,
    /// Whether meeting recording is active.
    pub meeting_active: bool,
}

/// Format a token count as a compact string (e.g., "12.4k", "1.2M").
fn format_tokens_compact(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

/// Render the status bar.
pub fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    mode: InputMode,
    theme: &Theme,
    data: &StatusBarData,
) {
    let hints = match mode {
        InputMode::Normal | InputMode::VimInsert => {
            "[Enter] Send │ [/] Commands │ [@] Files │ [Esc] Cancel │ [Ctrl+C] Quit"
        }
        InputMode::VimNormal => "[i] Insert │ [/] Search │ [:] Command │ [q] Quit",
        InputMode::CommandPalette => "[↑↓] Navigate │ [Enter] Select │ [Esc] Close",
        InputMode::Autocomplete => "[↑↓] Navigate │ [Tab] Accept │ [Esc] Close",
        InputMode::Approval => "[y] Approve │ [n] Deny │ [d] Diff │ [?] Help",
    };

    // Build the metrics string for the right side
    let metrics = if data.context_window > 0 {
        format!(
            "{}/{} | ${:.4} ",
            format_tokens_compact(data.tokens_used),
            format_tokens_compact(data.context_window),
            data.cost_usd,
        )
    } else if data.tokens_used > 0 {
        format!(
            "{} | ${:.4} ",
            format_tokens_compact(data.tokens_used),
            data.cost_usd,
        )
    } else {
        String::new()
    };

    let mut left_spans = vec![
        Span::styled(
            format!(" {} ", mode.label()),
            theme
                .status_bar_style()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", theme.status_bar_style()),
    ];

    // Voice/meeting indicators
    if data.voice_active {
        left_spans.push(Span::styled(
            " MIC ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        left_spans.push(Span::styled(" ", theme.status_bar_style()));
    }
    if data.meeting_active {
        left_spans.push(Span::styled(
            " REC ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ));
        left_spans.push(Span::styled(" ", theme.status_bar_style()));
    }

    left_spans.push(Span::styled(hints, theme.status_bar_style()));

    let left_bar = Paragraph::new(Line::from(left_spans)).style(theme.status_bar_style());
    frame.render_widget(left_bar, area);

    // Render metrics right-aligned on top of the same area
    if !metrics.is_empty() {
        let right_bar = Paragraph::new(Line::from(Span::styled(
            metrics,
            theme.status_bar_style().add_modifier(Modifier::DIM),
        )))
        .alignment(Alignment::Right)
        .style(theme.status_bar_style());
        frame.render_widget(right_bar, area);
    }
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
    fn test_status_bar_data_default() {
        let data = StatusBarData::default();
        assert_eq!(data.tokens_used, 0);
        assert_eq!(data.context_window, 0);
        assert!((data.cost_usd - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_render_status_bar_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        let data = StatusBarData::default();
        terminal
            .draw(|frame| {
                render_status_bar(frame, frame.area(), InputMode::Normal, &theme, &data);
            })
            .unwrap();
    }

    #[test]
    fn test_render_status_bar_with_metrics() {
        let backend = ratatui::backend::TestBackend::new(120, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        let data = StatusBarData {
            tokens_used: 12400,
            context_window: 128000,
            cost_usd: 0.0342,
            ..Default::default()
        };
        terminal
            .draw(|frame| {
                render_status_bar(frame, frame.area(), InputMode::Normal, &theme, &data);
            })
            .unwrap();
    }

    #[test]
    fn test_render_status_bar_high_cost() {
        let backend = ratatui::backend::TestBackend::new(120, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        let data = StatusBarData {
            tokens_used: 1_500_000,
            context_window: 2_000_000,
            cost_usd: 12.5678,
            ..Default::default()
        };
        terminal
            .draw(|frame| {
                render_status_bar(frame, frame.area(), InputMode::Normal, &theme, &data);
            })
            .unwrap();
    }

    #[test]
    fn test_render_status_bar_vim_mode() {
        let backend = ratatui::backend::TestBackend::new(80, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        let data = StatusBarData::default();
        terminal
            .draw(|frame| {
                render_status_bar(frame, frame.area(), InputMode::VimNormal, &theme, &data);
            })
            .unwrap();
    }

    #[test]
    fn test_format_tokens_compact() {
        assert_eq!(format_tokens_compact(0), "0");
        assert_eq!(format_tokens_compact(500), "500");
        assert_eq!(format_tokens_compact(1000), "1.0k");
        assert_eq!(format_tokens_compact(12400), "12.4k");
        assert_eq!(format_tokens_compact(128000), "128.0k");
        assert_eq!(format_tokens_compact(1_500_000), "1.5M");
    }
}
