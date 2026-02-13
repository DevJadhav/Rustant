//! Replay panel — step-by-step playback of agent execution traces.
//!
//! Toggle with Ctrl+R. Shows a timeline of events on the left and
//! event detail + cumulative metrics on the right.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::theme::Theme;
use rustant_core::replay::ReplaySession;

/// TUI overlay panel for replaying agent execution traces.
#[derive(Default)]
pub struct ReplayPanel {
    /// The replay session containing one or more loaded traces.
    pub session: ReplaySession,
    /// Whether the panel overlay is visible.
    pub visible: bool,
    /// Scroll offset for the detail pane.
    pub scroll_offset: u16,
}

impl ReplayPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle panel visibility.  When showing, rewind to start.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.scroll_offset = 0;
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Step forward in the active replay.
    pub fn step_forward(&mut self) {
        if let Some(engine) = self.session.active_mut() {
            engine.step_forward();
        }
    }

    /// Step backward in the active replay.
    pub fn step_backward(&mut self) {
        if let Some(engine) = self.session.active_mut() {
            engine.step_backward();
        }
    }

    /// Rewind to start of the active replay.
    pub fn rewind(&mut self) {
        if let Some(engine) = self.session.active_mut() {
            engine.rewind();
        }
    }

    /// Fast-forward to end of the active replay.
    pub fn fast_forward(&mut self) {
        if let Some(engine) = self.session.active_mut() {
            engine.fast_forward();
        }
    }

    /// Scroll the detail view up.
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll the detail view down.
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }
}

/// Render the replay panel as a full-screen overlay.
pub fn render_replay_panel(frame: &mut Frame, area: Rect, panel: &ReplayPanel, theme: &Theme) {
    // Minimum viable area
    if area.width < 20 || area.height < 10 {
        return;
    }

    // Clear background
    frame.render_widget(Clear, area);

    let outer_block = Block::default()
        .title(" Replay  [Ctrl+R to close] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    if panel.session.is_empty() {
        let msg = Paragraph::new("No replays loaded. Run a task to generate trace data.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(msg, inner);
        return;
    }

    let engine = match panel.session.active() {
        Some(e) => e,
        None => return,
    };

    // Split into timeline (left) and detail (right)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(inner);

    render_timeline(frame, chunks[0], engine, theme);
    render_detail(frame, chunks[1], engine, panel.scroll_offset, theme);
}

fn render_timeline(
    frame: &mut Frame,
    area: Rect,
    engine: &rustant_core::replay::ReplayEngine,
    theme: &Theme,
) {
    let timeline = engine.timeline();
    let items: Vec<ListItem> = timeline
        .iter()
        .map(|entry| {
            let marker = if entry.is_current { ">" } else { " " };
            let bookmark = if entry.is_bookmarked { "*" } else { "" };
            let text = format!(
                "{} {:>3}. {} {}",
                marker, entry.sequence, entry.description, bookmark
            );
            let style = if entry.is_current {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(text, style)))
        })
        .collect();

    let block = Block::default().title(" Timeline ").borders(Borders::ALL);
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    engine: &rustant_core::replay::ReplayEngine,
    scroll_offset: u16,
    theme: &Theme,
) {
    let snapshot = engine.snapshot();
    let mut lines: Vec<Line> = Vec::new();

    // Progress bar text
    lines.push(Line::from(Span::styled(
        format!(
            "Progress: {}/{} ({:.0}%)",
            snapshot.position, snapshot.total_events, snapshot.progress_pct
        ),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Current event description
    lines.push(Line::from(Span::styled(
        engine.describe_current(),
        Style::default().fg(Color::Yellow),
    )));
    lines.push(Line::from(""));

    // Cumulative metrics
    let usage = &snapshot.cumulative_usage;
    let cost = &snapshot.cumulative_cost;
    lines.push(Line::from(Span::styled(
        "Cumulative Metrics",
        Style::default().add_modifier(Modifier::UNDERLINED),
    )));
    lines.push(Line::from(format!(
        "  Tokens: {} in / {} out",
        usage.input_tokens, usage.output_tokens
    )));
    lines.push(Line::from(format!("  Cost: ${:.4}", cost.total())));
    if let Some(elapsed) = snapshot.elapsed_from_start {
        lines.push(Line::from(format!("  Elapsed: {}ms", elapsed)));
    }
    lines.push(Line::from(format!(
        "  Errors so far: {}",
        snapshot.errors_so_far
    )));
    lines.push(Line::from(""));

    // Tools executed
    if !snapshot.tools_executed_so_far.is_empty() {
        lines.push(Line::from(Span::styled(
            "Tools used:",
            Style::default().add_modifier(Modifier::UNDERLINED),
        )));
        for tool in &snapshot.tools_executed_so_far {
            lines.push(Line::from(format!("  - {}", tool)));
        }
    }

    // Controls hint
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Left/Right: step | Home: rewind | End: fast-forward | Up/Down: scroll",
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default().title(" Detail ").borders(Borders::ALL);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_panel_default() {
        let panel = ReplayPanel::new();
        assert!(!panel.is_visible());
        assert!(panel.session.is_empty());
    }

    #[test]
    fn test_toggle_visibility() {
        let mut panel = ReplayPanel::new();
        assert!(!panel.is_visible());
        panel.toggle();
        assert!(panel.is_visible());
        panel.toggle();
        assert!(!panel.is_visible());
    }

    #[test]
    fn test_toggle_resets_scroll() {
        let mut panel = ReplayPanel::new();
        panel.scroll_offset = 10;
        panel.toggle(); // show
        assert_eq!(panel.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_up_saturates_at_zero() {
        let mut panel = ReplayPanel::new();
        panel.scroll_up();
        assert_eq!(panel.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_down() {
        let mut panel = ReplayPanel::new();
        panel.scroll_down();
        assert_eq!(panel.scroll_offset, 1);
        panel.scroll_down();
        assert_eq!(panel.scroll_offset, 2);
    }

    #[test]
    fn test_step_forward_no_active_noop() {
        let mut panel = ReplayPanel::new();
        // No active replay, should not panic
        panel.step_forward();
    }

    #[test]
    fn test_step_backward_no_active_noop() {
        let mut panel = ReplayPanel::new();
        panel.step_backward();
    }

    #[test]
    fn test_rewind_no_active_noop() {
        let mut panel = ReplayPanel::new();
        panel.rewind();
    }

    #[test]
    fn test_fast_forward_no_active_noop() {
        let mut panel = ReplayPanel::new();
        panel.fast_forward();
    }
}
