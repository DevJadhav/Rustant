//! Sidebar widget showing active files, agent status, and context fill.

use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;
use rustant_core::memory::ContextBreakdown;
use rustant_core::types::AgentStatus;

/// Data for the sidebar.
#[derive(Debug, Clone)]
pub struct SidebarData {
    pub active_files: Vec<FileEntry>,
    pub agent_status: AgentStatus,
    pub iteration: usize,
    pub max_iterations: usize,
    pub context_ratio: f32,
    pub tools_available: usize,
    /// Detailed context breakdown for stacked display.
    pub context: Option<ContextBreakdown>,
}

/// An active file entry in the sidebar.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub status: FileStatus,
}

/// Status of an active file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FileStatus {
    Read,
    Modified,
    Created,
}

impl FileStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Read => "○",
            Self::Modified => "●",
            Self::Created => "+",
        }
    }
}

impl Default for SidebarData {
    fn default() -> Self {
        Self {
            active_files: Vec::new(),
            agent_status: AgentStatus::Idle,
            iteration: 0,
            max_iterations: 25,
            context_ratio: 0.0,
            tools_available: 0,
            context: None,
        }
    }
}

/// Render the sidebar.
pub fn render_sidebar(frame: &mut Frame, area: Rect, data: &SidebarData, theme: &Theme) {
    let block = Block::default()
        .title(" Context ")
        .borders(Borders::LEFT)
        .border_style(theme.border_style())
        .style(theme.sidebar_style());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // Determine context area height based on whether we have breakdown data
    let context_height = if data.context.is_some() { 7 } else { 3 };

    let [status_area, files_area, gauge_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(context_height),
    ])
    .areas(inner);

    // Status section
    let status_text = format!(
        " Status: {}\n Iteration: {}/{}\n Tools: {}",
        data.agent_status, data.iteration, data.max_iterations, data.tools_available,
    );
    let status = Paragraph::new(status_text).style(theme.sidebar_style());
    frame.render_widget(status, status_area);

    // Files section
    let file_items: Vec<ListItem> = data
        .active_files
        .iter()
        .map(|f| {
            let icon_style = match f.status {
                FileStatus::Read => theme.sidebar_style(),
                FileStatus::Modified => theme.warning_style(),
                FileStatus::Created => theme.success_style(),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", f.status.icon()), icon_style),
                Span::styled(f.path.clone(), theme.sidebar_style()),
            ]))
        })
        .collect();

    let files_block = Block::default()
        .title(" Files ")
        .borders(Borders::TOP)
        .border_style(theme.border_style());

    let files_list = List::new(file_items).block(files_block);
    frame.render_widget(files_list, files_area);

    // Context gauge with optional stacked breakdown
    if let Some(ref ctx) = data.context {
        render_context_breakdown(frame, gauge_area, ctx, theme);
    } else {
        // Simple gauge fallback
        let gauge_color = theme.context_gauge_color(data.context_ratio);
        let gauge_label = format!(" Context: {:.0}%", data.context_ratio * 100.0);
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(theme.border_style()),
            )
            .gauge_style(
                theme
                    .sidebar_style()
                    .fg(gauge_color)
                    .add_modifier(Modifier::BOLD),
            )
            .ratio(data.context_ratio.clamp(0.0, 1.0) as f64)
            .label(gauge_label);
        frame.render_widget(gauge, gauge_area);
    }
}

/// Render the detailed context breakdown with stacked components.
fn render_context_breakdown(frame: &mut Frame, area: Rect, ctx: &ContextBreakdown, theme: &Theme) {
    let block = Block::default()
        .title(" Memory ")
        .borders(Borders::TOP)
        .border_style(theme.border_style());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Summary line
    if ctx.has_summary {
        lines.push(Line::from(vec![
            Span::styled(" Summary: ", theme.sidebar_style()),
            Span::styled(
                format!("~{}tok", format_tokens(ctx.summary_tokens)),
                theme.tool_call_style(),
            ),
        ]));
    }

    // Messages line
    lines.push(Line::from(vec![
        Span::styled(" Messages: ", theme.sidebar_style()),
        Span::styled(
            format!(
                "{} (~{}tok)",
                ctx.message_count,
                format_tokens(ctx.message_tokens)
            ),
            theme.sidebar_style(),
        ),
    ]));

    // Pinned messages
    if ctx.pinned_count > 0 {
        lines.push(Line::from(vec![
            Span::styled(" Pinned: ", theme.sidebar_style()),
            Span::styled(format!("{}", ctx.pinned_count), theme.warning_style()),
        ]));
    }

    // Facts and rules
    if ctx.facts_count > 0 || ctx.rules_count > 0 {
        lines.push(Line::from(vec![
            Span::styled(" Knowledge: ", theme.sidebar_style()),
            Span::styled(
                format!("{} facts, {} rules", ctx.facts_count, ctx.rules_count),
                theme.sidebar_style(),
            ),
        ]));
    }

    // Usage gauge
    let ratio = ctx.usage_ratio();
    let gauge_style = if ratio >= 0.8 {
        theme.error_style().add_modifier(Modifier::BOLD)
    } else if ratio >= 0.5 {
        theme.warning_style().add_modifier(Modifier::BOLD)
    } else {
        theme.success_style().add_modifier(Modifier::BOLD)
    };

    lines.push(Line::from(vec![
        Span::styled(format!(" [{:.0}%] ", ratio * 100.0), gauge_style),
        Span::styled(
            format!(
                "{}tok / {}tok",
                format_tokens(ctx.total_tokens),
                format_tokens(ctx.context_window)
            ),
            theme.sidebar_style(),
        ),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Format token counts for display (e.g., 1500 -> "1.5k", 150000 -> "150k").
fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_data_default() {
        let data = SidebarData::default();
        assert_eq!(data.agent_status, AgentStatus::Idle);
        assert_eq!(data.iteration, 0);
        assert!(data.active_files.is_empty());
        assert!(data.context.is_none());
    }

    #[test]
    fn test_file_status_icons() {
        assert_eq!(FileStatus::Read.icon(), "○");
        assert_eq!(FileStatus::Modified.icon(), "●");
        assert_eq!(FileStatus::Created.icon(), "+");
    }

    #[test]
    fn test_render_sidebar_empty() {
        let backend = ratatui::backend::TestBackend::new(30, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let data = SidebarData::default();
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                render_sidebar(frame, frame.area(), &data, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_sidebar_with_files() {
        let backend = ratatui::backend::TestBackend::new(30, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let data = SidebarData {
            active_files: vec![
                FileEntry {
                    path: "src/main.rs".to_string(),
                    status: FileStatus::Modified,
                },
                FileEntry {
                    path: "src/lib.rs".to_string(),
                    status: FileStatus::Read,
                },
                FileEntry {
                    path: "src/new.rs".to_string(),
                    status: FileStatus::Created,
                },
            ],
            agent_status: AgentStatus::Thinking,
            iteration: 3,
            context_ratio: 0.45,
            tools_available: 12,
            ..Default::default()
        };
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                render_sidebar(frame, frame.area(), &data, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_sidebar_small_area() {
        let backend = ratatui::backend::TestBackend::new(20, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let data = SidebarData::default();
        let theme = Theme::dark();
        // Should not panic on very small area
        terminal
            .draw(|frame| {
                render_sidebar(frame, frame.area(), &data, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_sidebar_with_context_breakdown() {
        let backend = ratatui::backend::TestBackend::new(40, 25);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let data = SidebarData {
            context: Some(ContextBreakdown {
                summary_tokens: 500,
                message_tokens: 2000,
                total_tokens: 2500,
                context_window: 8000,
                remaining_tokens: 5500,
                message_count: 15,
                total_messages_seen: 30,
                pinned_count: 2,
                has_summary: true,
                facts_count: 5,
                rules_count: 3,
            }),
            ..Default::default()
        };
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                render_sidebar(frame, frame.area(), &data, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(150000), "150.0k");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_context_breakdown_usage_ratio() {
        let ctx = ContextBreakdown {
            total_tokens: 4000,
            context_window: 8000,
            ..Default::default()
        };
        assert!((ctx.usage_ratio() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_context_breakdown_warning() {
        let ctx = ContextBreakdown {
            total_tokens: 7000,
            context_window: 8000,
            ..Default::default()
        };
        assert!(ctx.is_warning());

        let ctx2 = ContextBreakdown {
            total_tokens: 3000,
            context_window: 8000,
            ..Default::default()
        };
        assert!(!ctx2.is_warning());
    }
}
