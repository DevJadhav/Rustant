//! Plan panel overlay widget for reviewing and editing execution plans.
//!
//! Toggle with Ctrl+P. When a plan is under review, the panel auto-shows
//! and captures keyboard focus for plan-specific actions.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use rustant_core::plan::{ExecutionPlan, PlanDecision, StepStatus};
use tokio::sync::oneshot;

/// State for the plan panel overlay.
pub struct PlanPanel {
    pub visible: bool,
    pub plan: Option<ExecutionPlan>,
    pub selected_step: usize,
    pub scroll_offset: u16,
    pub pending_reply: Option<oneshot::Sender<PlanDecision>>,
}

impl PlanPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            plan: None,
            selected_step: 0,
            scroll_offset: 0,
            pending_reply: None,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Set a plan for review and auto-show the panel.
    pub fn set_plan_for_review(
        &mut self,
        plan: ExecutionPlan,
        reply: oneshot::Sender<PlanDecision>,
    ) {
        self.plan = Some(plan);
        self.pending_reply = Some(reply);
        self.selected_step = 0;
        self.scroll_offset = 0;
        self.visible = true;
    }

    /// Update the plan display (e.g., after step progress).
    pub fn update_step(&mut self, index: usize, success: bool) {
        if let Some(ref mut plan) = self.plan
            && let Some(step) = plan.steps.get_mut(index)
        {
            step.status = if success {
                StepStatus::Completed
            } else {
                StepStatus::Failed
            };
        }
    }

    /// Mark a step as in-progress.
    pub fn mark_step_in_progress(&mut self, index: usize) {
        if let Some(ref mut plan) = self.plan
            && let Some(step) = plan.steps.get_mut(index)
        {
            step.status = StepStatus::InProgress;
        }
    }

    /// Whether the panel is in review mode (waiting for user decision).
    pub fn is_reviewing(&self) -> bool {
        self.pending_reply.is_some()
    }

    /// Send a decision and close review mode.
    pub fn send_decision(&mut self, decision: PlanDecision) {
        if let Some(reply) = self.pending_reply.take() {
            let _ = reply.send(decision);
        }
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.selected_step > 0 {
            self.selected_step -= 1;
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if let Some(ref plan) = self.plan
            && self.selected_step + 1 < plan.steps.len()
        {
            self.selected_step += 1;
        }
    }
}

/// Render the plan panel as a centered overlay.
pub fn render_plan_panel(f: &mut Frame, panel: &PlanPanel) {
    if !panel.visible {
        return;
    }

    let area = f.area();
    // Center the panel, taking 80% width and 80% height
    let panel_width = (area.width as f32 * 0.8) as u16;
    let panel_height = (area.height as f32 * 0.8) as u16;
    let x = (area.width.saturating_sub(panel_width)) / 2;
    let y = (area.height.saturating_sub(panel_height)) / 2;
    let panel_area = Rect::new(x, y, panel_width, panel_height);

    f.render_widget(Clear, panel_area);

    let Some(ref plan) = panel.plan else {
        let block = Block::default()
            .title(" Plan ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));
        let msg = Paragraph::new("No active plan.")
            .block(block)
            .wrap(Wrap { trim: true });
        f.render_widget(msg, panel_area);
        return;
    };

    let block = Block::default()
        .title(format!(" Plan: {} ", plan.goal))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let inner = block.inner(panel_area);
    f.render_widget(block, panel_area);

    // Split inner into header, steps, and footer
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(4),    // steps
        Constraint::Length(2), // footer
    ])
    .split(inner);

    // Header: summary + status
    let header_lines = vec![
        Line::from(vec![
            Span::styled("Summary: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&plan.summary),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::raw(plan.status.to_string()),
            Span::raw("  "),
            Span::styled(
                plan.progress_summary(),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    let header = Paragraph::new(header_lines);
    f.render_widget(header, chunks[0]);

    // Steps list
    let mut step_lines = Vec::new();
    for (i, step) in plan.steps.iter().enumerate() {
        let (icon, icon_color) = match step.status {
            StepStatus::Pending => ("○", Color::DarkGray),
            StepStatus::InProgress => ("●", Color::Blue),
            StepStatus::Completed => ("✓", Color::Green),
            StepStatus::Failed => ("✗", Color::Red),
            StepStatus::Skipped => ("⊘", Color::DarkGray),
        };

        let is_selected = i == panel.selected_step;
        let base_style = if is_selected {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let mut spans = vec![
            Span::styled(format!("  {} ", icon), Style::default().fg(icon_color)),
            Span::styled(format!("{}. ", i + 1), base_style.fg(Color::DarkGray)),
            Span::styled(&step.description, base_style),
        ];

        if let Some(ref tool) = step.tool {
            spans.push(Span::styled(
                format!(" [{}]", tool),
                Style::default().fg(Color::Cyan),
            ));
        }

        if let Some(ref risk) = step.risk_level {
            spans.push(Span::styled(
                format!(" ({})", risk),
                Style::default().fg(Color::Yellow),
            ));
        }

        if step.requires_approval {
            spans.push(Span::styled(" ⚠", Style::default().fg(Color::Yellow)));
        }

        if is_selected {
            spans.insert(0, Span::styled("▶ ", Style::default().fg(Color::Blue)));
        } else {
            spans.insert(0, Span::raw("  "));
        }

        step_lines.push(Line::from(spans));
    }
    let steps = Paragraph::new(step_lines)
        .scroll((panel.scroll_offset, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(steps, chunks[1]);

    // Footer: key hints
    let footer = if panel.is_reviewing() {
        Line::from(vec![
            Span::styled(
                " Enter",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" approve  "),
            Span::styled(
                "d",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" remove  "),
            Span::styled(
                "x",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" cancel  "),
            Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" navigate  "),
            Span::styled("Ctrl+P", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" close"),
        ])
    } else {
        Line::from(vec![
            Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" navigate  "),
            Span::styled("Ctrl+P", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" close"),
        ])
    };
    f.render_widget(Paragraph::new(footer), chunks[2]);
}
