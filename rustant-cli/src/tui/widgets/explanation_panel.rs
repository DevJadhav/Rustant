//! Explanation panel widget for the Safety Transparency Dashboard.
//!
//! Displays the agent's reasoning chain, safety decisions, considered
//! alternatives, and context factors in a navigable timeline view.
//! Toggled with Ctrl+E.

use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use rustant_core::explanation::{DecisionExplanation, DecisionType, FactorInfluence};

/// State for the explanation panel.
#[derive(Debug, Clone, Default)]
pub struct ExplanationPanel {
    /// History of all decision explanations in the session.
    pub explanations: Vec<DecisionExplanation>,
    /// Currently selected explanation index (for browsing).
    pub selected: usize,
    /// Whether the panel is visible.
    pub visible: bool,
    /// Scroll offset within the detail view.
    pub scroll_offset: u16,
}

impl ExplanationPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible && !self.explanations.is_empty() {
            self.selected = self.explanations.len() - 1; // Jump to most recent
            self.scroll_offset = 0;
        }
    }

    /// Add a new explanation.
    pub fn push(&mut self, explanation: DecisionExplanation) {
        self.explanations.push(explanation);
        // If visible, auto-select newest
        if self.visible {
            self.selected = self.explanations.len() - 1;
            self.scroll_offset = 0;
        }
    }

    /// Navigate to the previous explanation.
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.scroll_offset = 0;
        }
    }

    /// Navigate to the next explanation.
    pub fn select_next(&mut self) {
        if self.selected + 1 < self.explanations.len() {
            self.selected += 1;
            self.scroll_offset = 0;
        }
    }

    /// Scroll the detail view down.
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Scroll the detail view up.
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Whether the panel is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

/// Render the explanation panel as a full-screen overlay.
pub fn render_explanation_panel(
    frame: &mut Frame,
    area: Rect,
    panel: &ExplanationPanel,
    theme: &Theme,
) {
    if !panel.visible || area.width < 20 || area.height < 10 {
        return;
    }

    // Clear the overlay area
    frame.render_widget(Clear, area);

    let outer_block = Block::default()
        .title(format!(
            " Decision Transparency [{}/{}] ",
            if panel.explanations.is_empty() {
                0
            } else {
                panel.selected + 1
            },
            panel.explanations.len()
        ))
        .borders(Borders::ALL)
        .border_style(theme.border_style())
        .style(theme.base_style());

    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    if panel.explanations.is_empty() {
        let empty = Paragraph::new(
            "No decisions recorded yet.\n\nDecision explanations will appear here as the agent works.\nPress Ctrl+E to close.",
        )
        .style(theme.sidebar_style())
        .wrap(Wrap { trim: true });
        frame.render_widget(empty, inner);
        return;
    }

    // Split into timeline list (left) and detail view (right)
    let [list_area, detail_area] =
        Layout::horizontal([Constraint::Length(inner.width.min(30)), Constraint::Min(20)])
            .areas(inner);

    // --- Timeline list ---
    render_timeline(frame, list_area, panel, theme);

    // --- Detail view ---
    if let Some(explanation) = panel.explanations.get(panel.selected) {
        render_detail(frame, detail_area, explanation, panel.scroll_offset, theme);
    }
}

/// Render the left-side timeline of decisions.
fn render_timeline(frame: &mut Frame, area: Rect, panel: &ExplanationPanel, theme: &Theme) {
    let items: Vec<ListItem> = panel
        .explanations
        .iter()
        .enumerate()
        .rev() // Most recent first
        .map(|(i, exp)| {
            let tool = match &exp.decision_type {
                DecisionType::ToolSelection { selected_tool } => selected_tool.clone(),
                DecisionType::ParameterChoice { tool, .. } => format!("{}:param", tool),
                DecisionType::TaskDecomposition { .. } => "decompose".to_string(),
                DecisionType::ErrorRecovery { .. } => "recovery".to_string(),
            };
            let time = exp.timestamp.format("%H:%M:%S");
            let confidence = format!("{:.0}%", exp.confidence * 100.0);

            let style = if i == panel.selected {
                theme
                    .assistant_message_style()
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                theme.sidebar_style()
            };

            let line = Line::from(vec![
                Span::styled(format!("{} ", time), style),
                Span::styled(truncate_str(&tool, 12), style),
                Span::styled(format!(" {}", confidence), style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let timeline_block = Block::default()
        .title(" Timeline ")
        .borders(Borders::RIGHT)
        .border_style(theme.border_style());

    let list = List::new(items).block(timeline_block);
    frame.render_widget(list, area);
}

/// Render the right-side detail view for a single explanation.
fn render_detail(
    frame: &mut Frame,
    area: Rect,
    explanation: &DecisionExplanation,
    scroll: u16,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();

    // Decision type header
    let (dtype, dtype_detail) = match &explanation.decision_type {
        DecisionType::ToolSelection { selected_tool } => {
            ("Tool Selection", format!("Selected: {}", selected_tool))
        }
        DecisionType::ParameterChoice { tool, parameter } => {
            ("Parameter Choice", format!("{}.{}", tool, parameter))
        }
        DecisionType::TaskDecomposition { sub_tasks } => (
            "Task Decomposition",
            format!("{} sub-tasks", sub_tasks.len()),
        ),
        DecisionType::ErrorRecovery { error, strategy } => {
            ("Error Recovery", format!("{}: {}", strategy, error))
        }
    };

    lines.push(Line::from(vec![
        Span::styled(
            format!(" {} ", dtype),
            theme.assistant_message_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  Confidence: {:.0}%", explanation.confidence * 100.0),
            confidence_style(explanation.confidence, theme),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        format!(" {}", dtype_detail),
        theme.sidebar_style(),
    )));
    lines.push(Line::from(""));

    // Reasoning chain
    if !explanation.reasoning_chain.is_empty() {
        lines.push(Line::from(Span::styled(
            " Reasoning Chain:",
            theme.tool_call_style().add_modifier(Modifier::BOLD),
        )));
        for step in &explanation.reasoning_chain {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}. ", step.step_number), theme.tool_call_style()),
                Span::styled(&step.description, theme.sidebar_style()),
            ]));
            if let Some(ref evidence) = step.evidence {
                lines.push(Line::from(Span::styled(
                    format!("     [evidence: {}]", evidence),
                    theme.sidebar_style(),
                )));
            }
        }
        lines.push(Line::from(""));
    }

    // Considered alternatives
    if !explanation.considered_alternatives.is_empty() {
        lines.push(Line::from(Span::styled(
            " Alternatives Considered:",
            theme.warning_style().add_modifier(Modifier::BOLD),
        )));
        for alt in &explanation.considered_alternatives {
            lines.push(Line::from(vec![
                Span::styled(format!("  x {} ", alt.tool_name), theme.error_style()),
                Span::styled(format!("({})", alt.estimated_risk), theme.sidebar_style()),
            ]));
            lines.push(Line::from(Span::styled(
                format!("    Rejected: {}", alt.reason_not_selected),
                theme.sidebar_style(),
            )));
        }
        lines.push(Line::from(""));
    }

    // Context factors
    if !explanation.context_factors.is_empty() {
        lines.push(Line::from(Span::styled(
            " Context Factors:",
            theme.sidebar_style().add_modifier(Modifier::BOLD),
        )));
        for factor in &explanation.context_factors {
            let (indicator, style) = match factor.influence {
                FactorInfluence::Positive => ("+", theme.success_style()),
                FactorInfluence::Negative => ("-", theme.error_style()),
                FactorInfluence::Neutral => ("=", theme.sidebar_style()),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  [{}] ", indicator), style),
                Span::styled(&factor.factor, theme.sidebar_style()),
            ]));
        }
        lines.push(Line::from(""));
    }

    // Footer with navigation hints
    lines.push(Line::from(Span::styled(
        " [Left/Right] navigate | [Up/Down] scroll | [Ctrl+E] close",
        theme.status_bar_style(),
    )));

    let detail_block = Block::default().title(" Detail ").borders(Borders::NONE);

    let paragraph = Paragraph::new(lines)
        .block(detail_block)
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

/// Get a style based on confidence level.
fn confidence_style(confidence: f32, theme: &Theme) -> Style {
    if confidence >= 0.8 {
        theme.success_style()
    } else if confidence >= 0.5 {
        theme.warning_style()
    } else {
        theme.error_style()
    }
}

/// Truncate a string to a max length.
fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}..", &s[..max.saturating_sub(2)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustant_core::explanation::ExplanationBuilder;
    use rustant_core::types::RiskLevel;

    fn make_explanation() -> DecisionExplanation {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "file_read".into(),
        });
        builder.add_reasoning_step("User wants to view a config file", None);
        builder.add_reasoning_step("file_read has ReadOnly risk", Some("risk matrix"));
        builder.add_alternative(
            "shell_exec",
            "Unnecessary privileges for read operation",
            RiskLevel::Execute,
        );
        builder.add_context_factor("User is in safe mode", FactorInfluence::Positive);
        builder.add_context_factor("File is outside workspace", FactorInfluence::Negative);
        builder.set_confidence(0.92);
        builder.build()
    }

    #[test]
    fn test_panel_default() {
        let panel = ExplanationPanel::new();
        assert!(!panel.is_visible());
        assert_eq!(panel.explanations.len(), 0);
    }

    #[test]
    fn test_panel_toggle() {
        let mut panel = ExplanationPanel::new();
        panel.push(make_explanation());

        panel.toggle();
        assert!(panel.is_visible());
        assert_eq!(panel.selected, 0);

        panel.toggle();
        assert!(!panel.is_visible());
    }

    #[test]
    fn test_panel_navigation() {
        let mut panel = ExplanationPanel::new();
        panel.push(make_explanation());
        panel.push(make_explanation());
        panel.push(make_explanation());
        panel.visible = true;

        panel.selected = 0;
        panel.select_next();
        assert_eq!(panel.selected, 1);

        panel.select_next();
        assert_eq!(panel.selected, 2);

        panel.select_next(); // At end
        assert_eq!(panel.selected, 2);

        panel.select_prev();
        assert_eq!(panel.selected, 1);

        panel.select_prev();
        assert_eq!(panel.selected, 0);

        panel.select_prev(); // At start
        assert_eq!(panel.selected, 0);
    }

    #[test]
    fn test_panel_scroll() {
        let mut panel = ExplanationPanel::new();
        panel.scroll_down();
        assert_eq!(panel.scroll_offset, 1);
        panel.scroll_down();
        assert_eq!(panel.scroll_offset, 2);
        panel.scroll_up();
        assert_eq!(panel.scroll_offset, 1);
        panel.scroll_up();
        assert_eq!(panel.scroll_offset, 0);
        panel.scroll_up(); // Can't go below 0
        assert_eq!(panel.scroll_offset, 0);
    }

    #[test]
    fn test_panel_push_updates_selected_when_visible() {
        let mut panel = ExplanationPanel::new();
        panel.visible = true;

        panel.push(make_explanation());
        assert_eq!(panel.selected, 0);

        panel.push(make_explanation());
        assert_eq!(panel.selected, 1);
    }

    #[test]
    fn test_render_empty_panel() {
        let backend = ratatui::backend::TestBackend::new(60, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut panel = ExplanationPanel::new();
        panel.visible = true;
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                render_explanation_panel(frame, frame.area(), &panel, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_panel_with_explanations() {
        let backend = ratatui::backend::TestBackend::new(80, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut panel = ExplanationPanel::new();
        panel.visible = true;
        panel.push(make_explanation());
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                render_explanation_panel(frame, frame.area(), &panel, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_hidden_panel_noop() {
        let backend = ratatui::backend::TestBackend::new(60, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let panel = ExplanationPanel::new(); // visible = false
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                render_explanation_panel(frame, frame.area(), &panel, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_small_area() {
        let backend = ratatui::backend::TestBackend::new(15, 5);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut panel = ExplanationPanel::new();
        panel.visible = true;
        panel.push(make_explanation());
        let theme = Theme::dark();

        // Should not panic on small area
        terminal
            .draw(|frame| {
                render_explanation_panel(frame, frame.area(), &panel, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 7), "hello..");
        assert_eq!(truncate_str("hi", 2), "hi");
    }

    #[test]
    fn test_confidence_style_tiers() {
        let theme = Theme::dark();
        // High confidence = success style
        let _s1 = confidence_style(0.9, &theme);
        // Medium confidence = warning style
        let _s2 = confidence_style(0.6, &theme);
        // Low confidence = error style
        let _s3 = confidence_style(0.3, &theme);
    }
}
