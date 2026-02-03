//! Multi-Agent Task Board widget.
//!
//! TUI overlay showing all spawned agents with their status, current task,
//! resource usage, and elapsed time. Toggleable with Ctrl+T.

use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::Frame;
use std::collections::HashMap;
use uuid::Uuid;

/// Status of an agent on the task board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BoardAgentStatus {
    Idle,
    Running,
    Waiting,
    Terminated,
}

impl BoardAgentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Running => "Running",
            Self::Waiting => "Waiting",
            Self::Terminated => "Done",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Idle => "○",
            Self::Running => "●",
            Self::Waiting => "◐",
            Self::Terminated => "✓",
        }
    }
}

/// Summary of a single agent for display.
#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub status: BoardAgentStatus,
    pub current_tool: Option<String>,
    pub elapsed_secs: f64,
    pub tool_calls: u32,
    pub token_usage: usize,
    pub pending_messages: usize,
}

/// The task board state.
#[derive(Debug, Clone, Default)]
pub struct TaskBoard {
    pub visible: bool,
    pub agents: Vec<AgentSummary>,
    pub selected: usize,
}

impl TaskBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn select_next(&mut self) {
        if !self.agents.is_empty() {
            self.selected = (self.selected + 1) % self.agents.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.agents.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.agents.len() - 1);
        }
    }

    /// Update the agent list from orchestrator data.
    pub fn update_agents(&mut self, agents: Vec<AgentSummary>) {
        self.agents = agents;
        if self.selected >= self.agents.len() && !self.agents.is_empty() {
            self.selected = self.agents.len() - 1;
        }
    }

    /// Get the currently selected agent.
    pub fn selected_agent(&self) -> Option<&AgentSummary> {
        self.agents.get(self.selected)
    }

    /// Summary counts by status.
    #[allow(dead_code)]
    pub fn status_counts(&self) -> HashMap<&'static str, usize> {
        let mut counts = HashMap::new();
        for agent in &self.agents {
            *counts.entry(agent.status.label()).or_insert(0) += 1;
        }
        counts
    }
}

/// Render the task board overlay.
pub fn render_task_board(
    frame: &mut Frame,
    area: Rect,
    board: &TaskBoard,
    theme: &Theme,
) {
    if !board.visible || area.width < 40 || area.height < 12 {
        return;
    }

    // Calculate centered popup (80% of screen)
    let width = (area.width as f32 * 0.85).min(100.0) as u16;
    let height = (area.height as f32 * 0.8).min(30.0) as u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    // Clear background
    frame.render_widget(Clear, popup_area);

    // Build title with counts
    let running = board.agents.iter().filter(|a| a.status == BoardAgentStatus::Running).count();
    let total = board.agents.len();
    let title = format!(" Task Board [{} agents, {} running] [Esc to close] ", total, running);

    let outer_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme.border_style())
        .style(theme.base_style());

    let inner = outer_block.inner(popup_area);
    frame.render_widget(outer_block, popup_area);

    if inner.height < 3 {
        return;
    }

    if board.agents.is_empty() {
        let empty_msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No agents spawned yet.",
                theme.sidebar_style(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Use multi-agent tasks to spawn child agents.",
                theme.sidebar_style(),
            )),
        ])
        .style(theme.base_style());
        frame.render_widget(empty_msg, inner);
        return;
    }

    // Split: table on top, detail on bottom
    let [table_area, detail_area] = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(inner.height.min(6)),
    ])
    .areas(inner);

    // Render agent table
    render_agent_table(frame, table_area, board, theme);

    // Render selected agent detail
    if let Some(agent) = board.selected_agent() {
        render_agent_detail(frame, detail_area, agent, theme);
    }
}

/// Render the agent table.
fn render_agent_table(
    frame: &mut Frame,
    area: Rect,
    board: &TaskBoard,
    theme: &Theme,
) {
    let header = Row::new(vec!["", "Agent", "Role", "Status", "Tool", "Time", "Calls"])
        .style(theme.tool_call_style().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = board
        .agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let selected_marker = if i == board.selected { ">" } else { " " };
            let status_style = match agent.status {
                BoardAgentStatus::Running => theme.success_style(),
                BoardAgentStatus::Waiting => theme.warning_style(),
                BoardAgentStatus::Terminated => theme.sidebar_style(),
                BoardAgentStatus::Idle => theme.base_style(),
            };

            let tool = agent
                .current_tool
                .as_deref()
                .unwrap_or("-");

            let elapsed = if agent.elapsed_secs < 60.0 {
                format!("{:.0}s", agent.elapsed_secs)
            } else {
                format!("{:.0}m", agent.elapsed_secs / 60.0)
            };

            Row::new(vec![
                selected_marker.to_string(),
                truncate_str(&agent.name, 16),
                truncate_str(&agent.role, 12),
                format!("{} {}", agent.status.icon(), agent.status.label()),
                truncate_str(tool, 14),
                elapsed,
                agent.tool_calls.to_string(),
            ])
            .style(status_style)
        })
        .collect();

    let widths = [
        Constraint::Length(1),
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(14),
        Constraint::Length(6),
        Constraint::Length(5),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(theme.border_style()),
        );

    frame.render_widget(table, area);
}

/// Render detail for the selected agent.
fn render_agent_detail(
    frame: &mut Frame,
    area: Rect,
    agent: &AgentSummary,
    theme: &Theme,
) {
    let block = Block::default()
        .title(format!(" {} ", agent.name))
        .borders(Borders::TOP)
        .border_style(theme.border_style());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" ID: ", theme.sidebar_style()),
            Span::styled(
                format!("{}", agent.id),
                theme.tool_call_style(),
            ),
            Span::styled("  Role: ", theme.sidebar_style()),
            Span::styled(&agent.role, theme.tool_call_style()),
        ]),
        Line::from(vec![
            Span::styled(" Status: ", theme.sidebar_style()),
            Span::styled(
                format!("{} {}", agent.status.icon(), agent.status.label()),
                match agent.status {
                    BoardAgentStatus::Running => theme.success_style(),
                    BoardAgentStatus::Waiting => theme.warning_style(),
                    _ => theme.sidebar_style(),
                },
            ),
            Span::styled("  Tool calls: ", theme.sidebar_style()),
            Span::styled(agent.tool_calls.to_string(), theme.sidebar_style()),
            Span::styled("  Tokens: ", theme.sidebar_style()),
            Span::styled(
                format_tokens(agent.token_usage),
                theme.sidebar_style(),
            ),
        ]),
    ];

    if let Some(ref tool) = agent.current_tool {
        lines.push(Line::from(vec![
            Span::styled(" Current tool: ", theme.sidebar_style()),
            Span::styled(tool, theme.tool_call_style().add_modifier(Modifier::BOLD)),
        ]));
    }

    if agent.pending_messages > 0 {
        lines.push(Line::from(vec![
            Span::styled(" Pending messages: ", theme.sidebar_style()),
            Span::styled(
                agent.pending_messages.to_string(),
                theme.warning_style(),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Format token count for display.
fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

/// Truncate a string for table display.
fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_board_default() {
        let board = TaskBoard::new();
        assert!(!board.visible);
        assert!(board.agents.is_empty());
        assert_eq!(board.selected, 0);
    }

    #[test]
    fn test_task_board_toggle() {
        let mut board = TaskBoard::new();
        assert!(!board.is_visible());
        board.toggle();
        assert!(board.is_visible());
        board.toggle();
        assert!(!board.is_visible());
    }

    #[test]
    fn test_task_board_navigation() {
        let mut board = TaskBoard::new();
        board.update_agents(vec![
            AgentSummary {
                id: Uuid::new_v4(),
                name: "agent-1".into(),
                role: "worker".into(),
                status: BoardAgentStatus::Running,
                current_tool: Some("shell_exec".into()),
                elapsed_secs: 5.0,
                tool_calls: 3,
                token_usage: 1500,
                pending_messages: 0,
            },
            AgentSummary {
                id: Uuid::new_v4(),
                name: "agent-2".into(),
                role: "reviewer".into(),
                status: BoardAgentStatus::Idle,
                current_tool: None,
                elapsed_secs: 0.0,
                tool_calls: 0,
                token_usage: 0,
                pending_messages: 1,
            },
        ]);

        assert_eq!(board.selected, 0);
        board.select_next();
        assert_eq!(board.selected, 1);
        board.select_next();
        assert_eq!(board.selected, 0); // wraps

        board.select_prev();
        assert_eq!(board.selected, 1); // wraps backward
    }

    #[test]
    fn test_status_counts() {
        let mut board = TaskBoard::new();
        board.update_agents(vec![
            AgentSummary {
                id: Uuid::new_v4(),
                name: "a".into(),
                role: "w".into(),
                status: BoardAgentStatus::Running,
                current_tool: None,
                elapsed_secs: 0.0,
                tool_calls: 0,
                token_usage: 0,
                pending_messages: 0,
            },
            AgentSummary {
                id: Uuid::new_v4(),
                name: "b".into(),
                role: "w".into(),
                status: BoardAgentStatus::Running,
                current_tool: None,
                elapsed_secs: 0.0,
                tool_calls: 0,
                token_usage: 0,
                pending_messages: 0,
            },
            AgentSummary {
                id: Uuid::new_v4(),
                name: "c".into(),
                role: "w".into(),
                status: BoardAgentStatus::Idle,
                current_tool: None,
                elapsed_secs: 0.0,
                tool_calls: 0,
                token_usage: 0,
                pending_messages: 0,
            },
        ]);

        let counts = board.status_counts();
        assert_eq!(counts.get("Running"), Some(&2));
        assert_eq!(counts.get("Idle"), Some(&1));
    }

    #[test]
    fn test_board_agent_status_labels() {
        assert_eq!(BoardAgentStatus::Idle.label(), "Idle");
        assert_eq!(BoardAgentStatus::Running.label(), "Running");
        assert_eq!(BoardAgentStatus::Waiting.label(), "Waiting");
        assert_eq!(BoardAgentStatus::Terminated.label(), "Done");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("short", 10), "short");
        assert_eq!(truncate_str("a long name here", 10), "a long na…");
    }

    #[test]
    fn test_render_empty_board() {
        let backend = ratatui::backend::TestBackend::new(80, 25);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut board = TaskBoard::new();
        board.visible = true;
        let theme = crate::tui::theme::Theme::dark();

        terminal
            .draw(|frame| {
                render_task_board(frame, frame.area(), &board, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_board_with_agents() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut board = TaskBoard::new();
        board.visible = true;
        board.update_agents(vec![
            AgentSummary {
                id: Uuid::new_v4(),
                name: "worker-1".into(),
                role: "code-gen".into(),
                status: BoardAgentStatus::Running,
                current_tool: Some("file_write".into()),
                elapsed_secs: 12.5,
                tool_calls: 5,
                token_usage: 3200,
                pending_messages: 0,
            },
            AgentSummary {
                id: Uuid::new_v4(),
                name: "reviewer".into(),
                role: "review".into(),
                status: BoardAgentStatus::Waiting,
                current_tool: None,
                elapsed_secs: 30.0,
                tool_calls: 2,
                token_usage: 800,
                pending_messages: 3,
            },
        ]);

        let theme = crate::tui::theme::Theme::dark();

        terminal
            .draw(|frame| {
                render_task_board(frame, frame.area(), &board, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_board_hidden() {
        let backend = ratatui::backend::TestBackend::new(80, 25);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let board = TaskBoard::new(); // not visible
        let theme = crate::tui::theme::Theme::dark();

        // Should be a no-op
        terminal
            .draw(|frame| {
                render_task_board(frame, frame.area(), &board, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_board_small_area() {
        let backend = ratatui::backend::TestBackend::new(30, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut board = TaskBoard::new();
        board.visible = true;
        let theme = crate::tui::theme::Theme::dark();

        // Should not panic on very small area
        terminal
            .draw(|frame| {
                render_task_board(frame, frame.area(), &board, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_selected_agent() {
        let mut board = TaskBoard::new();
        assert!(board.selected_agent().is_none());

        board.update_agents(vec![AgentSummary {
            id: Uuid::new_v4(),
            name: "test".into(),
            role: "worker".into(),
            status: BoardAgentStatus::Idle,
            current_tool: None,
            elapsed_secs: 0.0,
            tool_calls: 0,
            token_usage: 0,
            pending_messages: 0,
        }]);

        assert!(board.selected_agent().is_some());
        assert_eq!(board.selected_agent().unwrap().name, "test");
    }
}
