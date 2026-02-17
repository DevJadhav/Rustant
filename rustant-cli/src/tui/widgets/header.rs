//! Header bar widget showing model, approval mode, token usage, and cost.

use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Data needed to render the header bar.
#[derive(Debug, Clone)]
pub struct HeaderData {
    pub model: String,
    pub approval_mode: String,
    pub tokens_used: usize,
    pub context_window: usize,
    pub cost_usd: f64,
    pub is_streaming: bool,
    pub vim_enabled: bool,
}

impl Default for HeaderData {
    fn default() -> Self {
        Self {
            model: "unknown".to_string(),
            approval_mode: "safe".to_string(),
            tokens_used: 0,
            context_window: 128_000,
            cost_usd: 0.0,
            is_streaming: false,
            vim_enabled: false,
        }
    }
}

impl HeaderData {
    /// Context usage ratio (0.0 - 1.0).
    pub fn context_ratio(&self) -> f32 {
        if self.context_window == 0 {
            return 0.0;
        }
        self.tokens_used as f32 / self.context_window as f32
    }

    /// Format token usage as a human-readable string.
    pub fn token_display(&self) -> String {
        format_token_count(self.tokens_used, self.context_window)
    }

    /// Format cost as USD.
    pub fn cost_display(&self) -> String {
        format!("${:.4}", self.cost_usd)
    }
}

/// Format token counts with k/M suffixes.
pub fn format_token_count(used: usize, total: usize) -> String {
    fn fmt(n: usize) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}k", n as f64 / 1_000.0)
        } else {
            format!("{}", n)
        }
    }
    format!("{}/{}", fmt(used), fmt(total))
}

/// Render the header bar.
pub fn render_header(frame: &mut Frame, area: Rect, data: &HeaderData, theme: &Theme) {
    let ratio = data.context_ratio();
    let gauge_color = theme.context_gauge_color(ratio);
    let ratio_pct = format!("{:.0}%", ratio * 100.0);

    let status_indicator = if data.is_streaming { "⟳" } else { "●" };

    let mut spans = vec![
        Span::styled(
            format!(" {} Rustant", status_indicator),
            theme
                .header_style()
                .add_modifier(Modifier::BOLD)
                .fg(theme.success_fg),
        ),
        Span::styled(" │ ", theme.header_style().fg(theme.border_color)),
        Span::styled(
            data.model.clone(),
            theme.header_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", theme.header_style().fg(theme.border_color)),
        Span::styled(data.approval_mode.clone(), theme.header_style()),
        Span::styled(" │ ", theme.header_style().fg(theme.border_color)),
        Span::styled(data.token_display(), Style::default().fg(gauge_color)),
        Span::styled(
            format!(" ({})", ratio_pct),
            Style::default().fg(gauge_color),
        ),
        Span::styled(" │ ", theme.header_style().fg(theme.border_color)),
        Span::styled(data.cost_display(), theme.header_style()),
    ];
    if data.vim_enabled {
        spans.push(Span::styled(
            " │ ",
            theme.header_style().fg(theme.border_color),
        ));
        spans.push(Span::styled(
            "[VIM]",
            theme
                .header_style()
                .add_modifier(Modifier::BOLD)
                .fg(theme.accent),
        ));
    }

    let header = Paragraph::new(Line::from(spans)).style(theme.header_style());
    frame.render_widget(header, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_data_default() {
        let data = HeaderData::default();
        assert_eq!(data.model, "unknown");
        assert_eq!(data.context_window, 128_000);
        assert_eq!(data.cost_usd, 0.0);
    }

    #[test]
    fn test_context_ratio() {
        let data = HeaderData {
            tokens_used: 64_000,
            context_window: 128_000,
            ..Default::default()
        };
        assert!((data.context_ratio() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_context_ratio_zero_window() {
        let data = HeaderData {
            context_window: 0,
            ..Default::default()
        };
        assert_eq!(data.context_ratio(), 0.0);
    }

    #[test]
    fn test_token_display_small() {
        assert_eq!(format_token_count(500, 1000), "500/1.0k");
    }

    #[test]
    fn test_token_display_k() {
        assert_eq!(format_token_count(12_400, 128_000), "12.4k/128.0k");
    }

    #[test]
    fn test_token_display_m() {
        assert_eq!(format_token_count(1_500_000, 2_000_000), "1.5M/2.0M");
    }

    #[test]
    fn test_cost_display() {
        let data = HeaderData {
            cost_usd: 0.0342,
            ..Default::default()
        };
        assert_eq!(data.cost_display(), "$0.0342");
    }

    #[test]
    fn test_render_header_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let data = HeaderData {
            model: "claude-sonnet".to_string(),
            approval_mode: "safe".to_string(),
            tokens_used: 12_400,
            context_window: 128_000,
            cost_usd: 0.0342,
            is_streaming: false,
            vim_enabled: false,
        };
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                render_header(frame, frame.area(), &data, &theme);
            })
            .unwrap();
    }
}
