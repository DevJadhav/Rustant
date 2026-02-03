//! Progress bar widget for the Streaming Progress Pipeline.
//!
//! Displays real-time progress during tool execution: spinner,
//! tool name, elapsed time, optional completion percentage,
//! and a scrollable mini-terminal for shell output lines.

use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};
use ratatui::Frame;
use rustant_core::types::ProgressUpdate;
use std::time::Instant;

/// Maximum number of shell output lines to retain.
const MAX_SHELL_LINES: usize = 100;

/// State for the progress bar area.
#[derive(Debug, Clone, Default)]
pub struct ProgressState {
    /// Currently executing tool name (None if idle).
    pub active_tool: Option<String>,
    /// The current stage description.
    pub stage: String,
    /// Optional completion percentage (0.0 to 1.0).
    pub percent: Option<f32>,
    /// When the current tool started executing.
    pub started_at: Option<Instant>,
    /// Recent shell output lines (scrollable mini-terminal).
    pub shell_lines: Vec<ShellLine>,
    /// Scroll offset for shell output.
    pub shell_scroll: u16,
    /// Animation frame counter (for spinner).
    pub tick: usize,
}

/// A single line of shell output.
#[derive(Debug, Clone)]
pub struct ShellLine {
    pub text: String,
    pub is_stderr: bool,
}

impl ProgressState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a tool as started.
    pub fn tool_started(&mut self, tool_name: &str) {
        self.active_tool = Some(tool_name.to_string());
        self.stage = "starting".to_string();
        self.percent = None;
        self.started_at = Some(Instant::now());
        self.shell_lines.clear();
        self.shell_scroll = 0;
    }

    /// Mark the current tool as finished.
    pub fn tool_finished(&mut self) {
        self.active_tool = None;
        self.stage.clear();
        self.percent = None;
        self.started_at = None;
        // Keep shell_lines for a brief moment (cleared on next tool_started)
    }

    /// Apply a progress update.
    pub fn apply_progress(&mut self, update: &ProgressUpdate) {
        match update {
            ProgressUpdate::ToolProgress {
                tool,
                stage,
                percent,
            } => {
                self.active_tool = Some(tool.clone());
                self.stage = stage.clone();
                self.percent = *percent;
            }
            ProgressUpdate::FileOperation {
                path, operation, ..
            } => {
                self.stage = format!("{} {}", operation, path.display());
            }
            ProgressUpdate::ShellOutput { line, is_stderr } => {
                self.shell_lines.push(ShellLine {
                    text: line.clone(),
                    is_stderr: *is_stderr,
                });
                if self.shell_lines.len() > MAX_SHELL_LINES {
                    self.shell_lines.remove(0);
                }
                // Auto-scroll to bottom
                let visible_lines = 3_u16; // approximate
                let total = self.shell_lines.len() as u16;
                self.shell_scroll = total.saturating_sub(visible_lines);
            }
        }
    }

    /// Advance the animation tick.
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Whether the progress bar should be displayed.
    pub fn is_active(&self) -> bool {
        self.active_tool.is_some()
    }

    /// Get elapsed time since tool started.
    pub fn elapsed_secs(&self) -> f64 {
        self.started_at
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }
}

/// Spinner frames for animation.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Render the progress bar area.
pub fn render_progress_bar(frame: &mut Frame, area: Rect, state: &ProgressState, theme: &Theme) {
    if !state.is_active() || area.height < 2 {
        return;
    }

    let tool_name = state.active_tool.as_deref().unwrap_or("unknown");
    let spinner = SPINNER_FRAMES[state.tick % SPINNER_FRAMES.len()];
    let elapsed = state.elapsed_secs();

    // If we have shell output, split the area: top for status, bottom for output
    if !state.shell_lines.is_empty() && area.height >= 4 {
        let [status_area, output_area] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);

        render_status_line(
            frame,
            status_area,
            spinner,
            tool_name,
            &state.stage,
            elapsed,
            state.percent,
            theme,
        );
        render_shell_output(frame, output_area, state, theme);
    } else {
        render_status_line(
            frame,
            area,
            spinner,
            tool_name,
            &state.stage,
            elapsed,
            state.percent,
            theme,
        );
    }
}

/// Render the top status line with spinner, tool name, stage, and elapsed time.
#[allow(clippy::too_many_arguments)]
fn render_status_line(
    frame: &mut Frame,
    area: Rect,
    spinner: &str,
    tool_name: &str,
    stage: &str,
    elapsed: f64,
    percent: Option<f32>,
    theme: &Theme,
) {
    if area.height < 1 {
        return;
    }

    // First line: spinner + tool + stage + elapsed
    let elapsed_str = if elapsed < 60.0 {
        format!("{:.1}s", elapsed)
    } else {
        format!("{}m{:.0}s", (elapsed / 60.0) as u64, elapsed % 60.0)
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", spinner),
            theme.tool_call_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", tool_name),
            theme.tool_call_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(stage, theme.sidebar_style()),
        Span::styled(format!("  [{}]", elapsed_str), theme.sidebar_style()),
    ]);

    if area.height >= 2 {
        if let Some(pct) = percent {
            // Show a gauge on the second line
            let [line_area, gauge_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

            let paragraph = Paragraph::new(line).style(theme.base_style());
            frame.render_widget(paragraph, line_area);

            let pct_clamped = pct.clamp(0.0, 1.0);
            let gauge = Gauge::default()
                .gauge_style(theme.tool_call_style())
                .ratio(pct_clamped as f64)
                .label(format!("{:.0}%", pct_clamped * 100.0));
            frame.render_widget(gauge, gauge_area);
        } else {
            let paragraph = Paragraph::new(line).style(theme.base_style());
            frame.render_widget(paragraph, area);
        }
    } else {
        let paragraph = Paragraph::new(line).style(theme.base_style());
        frame.render_widget(paragraph, area);
    }
}

/// Render the scrollable shell output mini-terminal.
fn render_shell_output(frame: &mut Frame, area: Rect, state: &ProgressState, theme: &Theme) {
    let lines: Vec<Line> = state
        .shell_lines
        .iter()
        .map(|sl| {
            let style = if sl.is_stderr {
                theme.warning_style()
            } else {
                theme.sidebar_style()
            };
            Line::from(Span::styled(&sl.text, style))
        })
        .collect();

    let block = Block::default()
        .title(" Output ")
        .borders(Borders::TOP)
        .border_style(theme.border_style());

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((state.shell_scroll, 0));

    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_state_default() {
        let state = ProgressState::new();
        assert!(!state.is_active());
        assert_eq!(state.elapsed_secs(), 0.0);
        assert!(state.shell_lines.is_empty());
    }

    #[test]
    fn test_tool_started() {
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        assert!(state.is_active());
        assert_eq!(state.active_tool.as_deref(), Some("shell_exec"));
        assert_eq!(state.stage, "starting");
        assert!(state.started_at.is_some());
    }

    #[test]
    fn test_tool_finished() {
        let mut state = ProgressState::new();
        state.tool_started("file_read");
        state.tool_finished();
        assert!(!state.is_active());
        assert!(state.started_at.is_none());
    }

    #[test]
    fn test_apply_tool_progress() {
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        state.apply_progress(&ProgressUpdate::ToolProgress {
            tool: "shell_exec".into(),
            stage: "running tests".into(),
            percent: Some(0.5),
        });
        assert_eq!(state.stage, "running tests");
        assert_eq!(state.percent, Some(0.5));
    }

    #[test]
    fn test_apply_shell_output() {
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        state.apply_progress(&ProgressUpdate::ShellOutput {
            line: "compiling crate...".into(),
            is_stderr: false,
        });
        assert_eq!(state.shell_lines.len(), 1);
        assert_eq!(state.shell_lines[0].text, "compiling crate...");
        assert!(!state.shell_lines[0].is_stderr);

        state.apply_progress(&ProgressUpdate::ShellOutput {
            line: "warning: unused variable".into(),
            is_stderr: true,
        });
        assert_eq!(state.shell_lines.len(), 2);
        assert!(state.shell_lines[1].is_stderr);
    }

    #[test]
    fn test_apply_file_operation() {
        let mut state = ProgressState::new();
        state.tool_started("file_write");
        state.apply_progress(&ProgressUpdate::FileOperation {
            path: "/src/main.rs".into(),
            operation: "writing".into(),
            bytes_processed: Some(1024),
        });
        assert!(state.stage.contains("writing"));
        assert!(state.stage.contains("main.rs"));
    }

    #[test]
    fn test_shell_output_max_lines() {
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        for i in 0..150 {
            state.apply_progress(&ProgressUpdate::ShellOutput {
                line: format!("line {}", i),
                is_stderr: false,
            });
        }
        assert_eq!(state.shell_lines.len(), MAX_SHELL_LINES);
        // The oldest lines should have been evicted
        assert!(state.shell_lines[0].text.contains("50"));
    }

    #[test]
    fn test_tick_advances() {
        let mut state = ProgressState::new();
        assert_eq!(state.tick, 0);
        state.tick();
        assert_eq!(state.tick, 1);
        state.tick();
        assert_eq!(state.tick, 2);
    }

    #[test]
    fn test_elapsed_when_idle() {
        let state = ProgressState::new();
        assert_eq!(state.elapsed_secs(), 0.0);
    }

    #[test]
    fn test_elapsed_when_active() {
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        // Elapsed should be > 0 (even if tiny)
        // We can't assert exact value but it shouldn't be zero after starting
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(state.elapsed_secs() > 0.0);
    }

    #[test]
    fn test_render_inactive_noop() {
        let backend = ratatui::backend::TestBackend::new(80, 3);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = ProgressState::new();
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                render_progress_bar(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_active_no_output() {
        let backend = ratatui::backend::TestBackend::new(80, 3);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                render_progress_bar(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_active_with_percent() {
        let backend = ratatui::backend::TestBackend::new(80, 3);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        state.percent = Some(0.75);
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                render_progress_bar(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_active_with_shell_output() {
        let backend = ratatui::backend::TestBackend::new(80, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        state.apply_progress(&ProgressUpdate::ShellOutput {
            line: "test 1 passed".into(),
            is_stderr: false,
        });
        state.apply_progress(&ProgressUpdate::ShellOutput {
            line: "warning: deprecation".into(),
            is_stderr: true,
        });
        let theme = Theme::dark();

        terminal
            .draw(|frame| {
                render_progress_bar(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_small_area() {
        let backend = ratatui::backend::TestBackend::new(20, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut state = ProgressState::new();
        state.tool_started("shell_exec");
        let theme = Theme::dark();

        // Should not panic on very small area
        terminal
            .draw(|frame| {
                render_progress_bar(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }
}
