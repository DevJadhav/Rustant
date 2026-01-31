//! Sidebar widget showing active files, agent status, and context fill.

use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;
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

    let [status_area, files_area, gauge_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(3),
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

    // Context gauge
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_data_default() {
        let data = SidebarData::default();
        assert_eq!(data.agent_status, AgentStatus::Idle);
        assert_eq!(data.iteration, 0);
        assert!(data.active_files.is_empty());
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
}
