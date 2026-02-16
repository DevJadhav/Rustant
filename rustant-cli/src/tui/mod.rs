//! TUI (Terminal User Interface) module for Rustant.
//!
//! Provides a full-featured terminal interface with conversation pane,
//! context sidebar, input area, syntax highlighting, and approval dialogs.

pub mod app;
pub mod callback;
pub mod event;
pub mod theme;
pub mod widgets;

use app::App;
use rustant_core::AgentConfig;
use std::path::PathBuf;

/// Run the TUI application.
pub async fn run(config: AgentConfig, workspace: PathBuf) -> anyhow::Result<()> {
    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    terminal.clear()?;

    // Run app
    let mut app = App::new(config, workspace);
    let result = app.run(&mut terminal).await;

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}
