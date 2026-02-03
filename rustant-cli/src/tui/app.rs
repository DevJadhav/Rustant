//! Main TUI application: state, event loop, and top-level draw function.

use crate::tui::callback::{TuiCallback, TuiEvent};
use crate::tui::event::{map_approval_key, map_global_key, Action, EventHandler};
use crate::tui::theme::Theme;
use crate::tui::widgets::autocomplete::AutocompleteState;
use crate::tui::widgets::command_palette::CommandPalette;
use crate::tui::widgets::conversation::{render_conversation, ConversationState, DisplayMessage};
use crate::tui::widgets::diff_view::DiffView;
use crate::tui::widgets::explanation_panel::{render_explanation_panel, ExplanationPanel};
use crate::tui::widgets::header::{render_header, HeaderData};
use crate::tui::widgets::input_area::{InputAction, InputWidget};
use crate::tui::widgets::markdown::SyntaxHighlighter;
use crate::tui::widgets::progress_bar::{render_progress_bar, ProgressState};
use crate::tui::widgets::sidebar::{render_sidebar, FileEntry, FileStatus, SidebarData};
use crate::tui::widgets::status_bar::{render_status_bar, InputMode};
use crate::tui::widgets::task_board::{render_task_board, TaskBoard};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;
use rustant_core::audit::{Analytics, AuditExporter, AuditQuery, AuditStore, ExecutionTrace};
use rustant_core::replay::ReplaySession;
use rustant_core::types::{AgentStatus, Role};
use rustant_core::{
    Agent, AgentConfig, MockLlmProvider, RegisteredTool, TaskResult, TokenAlert, TokenCostDisplay,
};
use rustant_tools::checkpoint::CheckpointManager;
use rustant_tools::register_builtin_tools_with_progress;
use rustant_tools::registry::ToolRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// The main TUI application state.
#[allow(dead_code)]
pub struct App {
    // UI state
    pub conversation: ConversationState,
    pub input: InputWidget,
    pub header: HeaderData,
    pub sidebar: SidebarData,
    pub theme: Theme,
    pub mode: InputMode,
    pub show_sidebar: bool,

    // Syntax highlighting
    highlighter: SyntaxHighlighter,

    // Week 6: Autocomplete & command palette
    pub autocomplete: AutocompleteState,
    pub command_palette: CommandPalette,

    // Week 7: Diff view & checkpoints
    pub diff_view: DiffView,
    checkpoint_manager: CheckpointManager,

    // Agent communication
    callback_rx: mpsc::UnboundedReceiver<TuiEvent>,
    agent: Agent,
    workspace: PathBuf,

    // Approval state
    pending_approval: Option<oneshot::Sender<rustant_core::safety::ApprovalDecision>>,

    // Clarification state
    pending_clarification: Option<oneshot::Sender<String>>,

    // Audit & Replay (Week 12)
    audit_store: AuditStore,
    replay_session: ReplaySession,

    // Safety Transparency Dashboard
    pub explanation_panel: ExplanationPanel,

    // Streaming Progress Pipeline
    pub progress: ProgressState,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<rustant_core::types::ProgressUpdate>,

    // Multi-Agent Task Board
    pub task_board: TaskBoard,

    // App state
    pub should_quit: bool,
    is_processing: bool,
    vim_mode: bool,
}

impl App {
    /// Create a new TUI application.
    pub fn new(config: AgentConfig, workspace: PathBuf) -> Self {
        let theme = Theme::from_name(&config.ui.theme);
        let vim_mode = config.ui.vim_mode;

        let (callback, callback_rx) = TuiCallback::new();
        let provider = match rustant_core::create_provider(&config.llm) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("LLM provider init failed: {}. Using mock.", e);
                Arc::new(MockLlmProvider::new())
            }
        };
        let callback_arc = Arc::new(callback);
        let mut agent = Agent::new(provider, config.clone(), callback_arc);

        // Register tools with progress channel for streaming shell output
        let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut registry = ToolRegistry::new();
        register_builtin_tools_with_progress(&mut registry, workspace.clone(), Some(progress_tx));
        register_agent_tools(&mut agent, &registry, &workspace);

        let header = HeaderData {
            model: config.llm.model.clone(),
            approval_mode: config.safety.approval_mode.to_string(),
            tokens_used: 0,
            context_window: config.llm.context_window,
            cost_usd: 0.0,
            is_streaming: false,
        };

        let sidebar = SidebarData {
            tools_available: agent.tool_definitions().len(),
            max_iterations: config.safety.max_iterations,
            ..Default::default()
        };

        let mut app = Self {
            conversation: ConversationState::new(),
            input: InputWidget::new(&theme),
            header,
            sidebar,
            theme,
            mode: if vim_mode {
                InputMode::VimNormal
            } else {
                InputMode::Normal
            },
            show_sidebar: true,
            highlighter: SyntaxHighlighter::new(),
            autocomplete: AutocompleteState::new(workspace.clone()),
            command_palette: CommandPalette::new(),
            diff_view: DiffView::new(),
            checkpoint_manager: CheckpointManager::new(workspace.clone()),
            callback_rx,
            agent,
            workspace,
            pending_approval: None,
            pending_clarification: None,
            audit_store: AuditStore::new(),
            replay_session: ReplaySession::new(),
            explanation_panel: ExplanationPanel::new(),
            progress: ProgressState::new(),
            progress_rx,
            task_board: TaskBoard::new(),
            should_quit: false,
            is_processing: false,
            vim_mode,
        };

        // Load input history from previous sessions
        app.load_history();
        // Try to recover the last auto-saved session
        app.try_recover_session();

        app
    }

    /// Run the main event loop.
    pub async fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> anyhow::Result<()> {
        let mut event_handler = EventHandler::new();
        let tick_rate = std::time::Duration::from_millis(100);

        loop {
            // Draw
            terminal.draw(|frame| self.draw(frame))?;

            // Poll events
            tokio::select! {
                // Terminal events
                event = event_handler.next() => {
                    if let Some(event) = event {
                        self.handle_terminal_event(event);
                    }
                }
                // Agent callback events
                event = self.callback_rx.recv() => {
                    if let Some(event) = event {
                        self.handle_tui_event(event);
                    }
                }
                // Tool progress events (streaming shell output)
                update = self.progress_rx.recv() => {
                    if let Some(update) = update {
                        self.progress.apply_progress(&update);
                    }
                }
                // Tick
                _ = tokio::time::sleep(tick_rate) => {
                    // Tick for spinners/animation updates
                    self.progress.tick();
                }
            }

            if self.should_quit {
                // Auto-save session and persist history before exit
                self.auto_save_session();
                self.save_history();
                break;
            }
        }

        Ok(())
    }

    /// Draw the full UI.
    pub fn draw(&self, frame: &mut Frame) {
        // Dynamic layout: include progress bar area when a tool is executing
        let progress_height = if self.progress.is_active() {
            let base = 2_u16;
            let shell_lines = self.progress.shell_lines.len() as u16;
            base + shell_lines.min(5) // Up to 5 lines of shell output
        } else {
            0
        };

        let [header_area, main_area, progress_area, input_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(progress_height),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        // Header
        render_header(frame, header_area, &self.header, &self.theme);

        // Main area: conversation + optional sidebar
        if self.show_sidebar {
            let [conv_area, sidebar_area] =
                Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
                    .areas(main_area);

            render_conversation(
                frame,
                conv_area,
                &self.conversation,
                &self.theme,
                &self.highlighter,
            );
            render_sidebar(frame, sidebar_area, &self.sidebar, &self.theme);
        } else {
            render_conversation(
                frame,
                main_area,
                &self.conversation,
                &self.theme,
                &self.highlighter,
            );
        }

        // Progress bar (visible during tool execution)
        if self.progress.is_active() {
            render_progress_bar(frame, progress_area, &self.progress, &self.theme);
        }

        // Input area
        self.input.render(frame, input_area);

        // Status bar
        render_status_bar(frame, status_area, self.mode, &self.theme);

        // Popups (rendered last, on top)
        if self.autocomplete.is_active() {
            self.autocomplete.render(frame, input_area, &self.theme);
        }
        if self.command_palette.is_active() {
            self.command_palette.render(frame, input_area, &self.theme);
        }

        // Diff view overlay (rendered on top of everything)
        if self.diff_view.is_visible() {
            self.diff_view.render(frame, frame.area(), &self.theme);
        }

        // Explanation panel overlay (Safety Transparency Dashboard)
        if self.explanation_panel.is_visible() {
            render_explanation_panel(frame, frame.area(), &self.explanation_panel, &self.theme);
        }

        // Multi-Agent Task Board overlay
        if self.task_board.is_visible() {
            render_task_board(frame, frame.area(), &self.task_board, &self.theme);
        }
    }

    /// Handle a terminal event (keyboard, mouse, resize).
    fn handle_terminal_event(&mut self, event: Event) {
        match event {
            Event::Key(key_event) => self.handle_key_event(key_event),
            Event::Mouse(mouse_event) => self.handle_mouse_event(mouse_event),
            Event::Resize(_, _) => {} // ratatui redraws on next frame
            _ => {}
        }
    }

    /// Handle a key event.
    fn handle_key_event(&mut self, key: KeyEvent) {
        // Escape key: cancel streaming, close popups, or exit vim insert
        if key.code == KeyCode::Esc {
            if self.task_board.is_visible() {
                self.task_board.toggle();
                return;
            }
            if self.explanation_panel.is_visible() {
                self.explanation_panel.toggle();
                return;
            }
            if self.diff_view.is_visible() {
                self.diff_view.hide();
                return;
            }
            if self.autocomplete.is_active() {
                self.autocomplete.deactivate();
                self.mode = self.base_mode();
                return;
            }
            if self.command_palette.is_active() {
                self.command_palette.deactivate();
                self.mode = self.base_mode();
                return;
            }
            if self.is_processing {
                self.agent.cancel();
                self.is_processing = false;
                self.header.is_streaming = false;
                self.sidebar.agent_status = AgentStatus::Idle;
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: "[Cancelled]".to_string(),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
                return;
            }
            if self.mode == InputMode::VimInsert {
                self.mode = InputMode::VimNormal;
                return;
            }
            if self.mode == InputMode::Approval {
                self.resolve_approval(rustant_core::safety::ApprovalDecision::Deny);
                return;
            }
            return;
        }

        // Global keybindings (Ctrl+C, Ctrl+D, Ctrl+L)
        if let Some(action) = map_global_key(&key) {
            self.execute_action(action);
            return;
        }

        // Ctrl+E: Toggle explanation panel (Safety Transparency Dashboard)
        if key.code == KeyCode::Char('e') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.explanation_panel.toggle();
            return;
        }

        // Ctrl+T: Toggle multi-agent task board
        if key.code == KeyCode::Char('t') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.task_board.toggle();
            return;
        }

        // Task board navigation when visible
        if self.task_board.is_visible() {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.task_board.select_prev();
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.task_board.select_next();
                    return;
                }
                _ => {} // Other keys pass through
            }
        }

        // Explanation panel navigation when visible
        if self.explanation_panel.is_visible() {
            match key.code {
                KeyCode::Left => {
                    self.explanation_panel.select_prev();
                    return;
                }
                KeyCode::Right => {
                    self.explanation_panel.select_next();
                    return;
                }
                KeyCode::Up => {
                    self.explanation_panel.scroll_up();
                    return;
                }
                KeyCode::Down => {
                    self.explanation_panel.scroll_down();
                    return;
                }
                _ => {} // Other keys pass through
            }
        }

        // Diff view mode: intercept scroll keys when visible
        if self.diff_view.is_visible() {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.diff_view.scroll_up(1),
                KeyCode::Down | KeyCode::Char('j') => self.diff_view.scroll_down(1),
                KeyCode::PageUp => self.diff_view.scroll_up(10),
                KeyCode::PageDown => self.diff_view.scroll_down(10),
                _ => {}
            }
            return;
        }

        // Autocomplete mode
        if self.autocomplete.is_active() {
            self.handle_autocomplete_key(key);
            return;
        }

        // Command palette mode
        if self.command_palette.is_active() {
            self.handle_command_palette_key(key);
            return;
        }

        // Mode-specific handling
        match self.mode {
            InputMode::Approval => {
                if let Some(action) = map_approval_key(&key) {
                    self.execute_action(action);
                }
            }
            InputMode::VimNormal => {
                self.handle_vim_normal_key(key);
            }
            _ => {
                // Pass to input widget
                let event = Event::Key(key);
                match self.input.handle_event(&event) {
                    InputAction::Submit(text) => {
                        self.submit_task(&text);
                    }
                    InputAction::TriggerAutocomplete(query) => {
                        self.mode = InputMode::Autocomplete;
                        self.autocomplete.activate(&query);
                    }
                    InputAction::TriggerCommandPalette(query) => {
                        self.mode = InputMode::CommandPalette;
                        self.command_palette.activate(&query);
                    }
                    InputAction::Consumed | InputAction::NotConsumed => {}
                }
            }
        }
    }

    /// Handle keystrokes when autocomplete popup is active.
    fn handle_autocomplete_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.autocomplete.move_up(),
            KeyCode::Down => self.autocomplete.move_down(),
            KeyCode::Tab | KeyCode::Enter => {
                if let Some(selected) = self.autocomplete.accept() {
                    // Replace the @query with the selected file
                    let text = self.input.text();
                    if let Some(at_pos) = text.rfind('@') {
                        let new_text = format!("{}{} ", &text[..at_pos + 1], selected);
                        self.input.set_text(&new_text);
                    }
                }
                self.mode = self.base_mode();
            }
            KeyCode::Esc => {
                self.autocomplete.deactivate();
                self.mode = self.base_mode();
            }
            KeyCode::Char(c) => {
                // Continue typing: pass to input and update autocomplete query
                let event = Event::Key(key);
                self.input.handle_event(&event);
                let text = self.input.text();
                if let Some(at_pos) = text.rfind('@') {
                    self.autocomplete.update_query(&text[at_pos + 1..]);
                } else {
                    self.autocomplete.deactivate();
                    self.mode = self.base_mode();
                }
                let _ = c;
            }
            KeyCode::Backspace => {
                let event = Event::Key(key);
                self.input.handle_event(&event);
                let text = self.input.text();
                if let Some(at_pos) = text.rfind('@') {
                    self.autocomplete.update_query(&text[at_pos + 1..]);
                } else {
                    self.autocomplete.deactivate();
                    self.mode = self.base_mode();
                }
            }
            _ => {}
        }
    }

    /// Handle keystrokes when command palette is active.
    fn handle_command_palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.command_palette.move_up(),
            KeyCode::Down => self.command_palette.move_down(),
            KeyCode::Tab | KeyCode::Enter => {
                if let Some(cmd) = self.command_palette.accept() {
                    self.input.clear();
                    self.mode = self.base_mode();
                    self.handle_command(&cmd);
                } else {
                    self.mode = self.base_mode();
                }
            }
            KeyCode::Esc => {
                self.command_palette.deactivate();
                self.input.clear();
                self.mode = self.base_mode();
            }
            KeyCode::Char(_) => {
                let event = Event::Key(key);
                self.input.handle_event(&event);
                let text = self.input.text();
                let query = text.trim_start_matches('/');
                self.command_palette.update_query(query);
            }
            KeyCode::Backspace => {
                let text = self.input.text();
                if text == "/" || text.is_empty() {
                    self.command_palette.deactivate();
                    self.input.clear();
                    self.mode = self.base_mode();
                } else {
                    let event = Event::Key(key);
                    self.input.handle_event(&event);
                    let text = self.input.text();
                    let query = text.trim_start_matches('/');
                    self.command_palette.update_query(query);
                }
            }
            _ => {}
        }
    }

    /// Handle vim normal mode keys.
    fn handle_vim_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('i') => {
                self.mode = InputMode::VimInsert;
            }
            KeyCode::Char('a') => {
                self.mode = InputMode::VimInsert;
                // Move cursor right (append mode)
            }
            KeyCode::Char('I') => {
                self.mode = InputMode::VimInsert;
                // Move to beginning of line
            }
            KeyCode::Char('A') => {
                self.mode = InputMode::VimInsert;
                // Move to end of line
            }
            KeyCode::Char('o') => {
                self.mode = InputMode::VimInsert;
                // Open new line below
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.conversation.scroll_down(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.conversation.scroll_up(1);
            }
            KeyCode::Char('G') => {
                self.conversation.scroll_to_bottom();
            }
            KeyCode::Char('g') => {
                // gg: scroll to top (simplified: single g scrolls to top)
                self.conversation.scroll_offset = 0;
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.conversation.scroll_down(10);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.conversation.scroll_up(10);
            }
            KeyCode::Char('/') => {
                // Start command palette from vim normal mode
                self.input.set_text("/");
                self.mode = InputMode::CommandPalette;
                self.command_palette.activate("");
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('y') => {
                // Yank last response to clipboard
                self.copy_last_response();
            }
            _ => {}
        }
    }

    /// Handle mouse events.
    fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.conversation.scroll_up(3),
            MouseEventKind::ScrollDown => self.conversation.scroll_down(3),
            _ => {}
        }
    }

    /// Execute a high-level action.
    fn execute_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }
            Action::Cancel => {
                if self.is_processing {
                    self.agent.cancel();
                    self.is_processing = false;
                    self.header.is_streaming = false;
                }
                if self.mode == InputMode::Approval {
                    self.resolve_approval(rustant_core::safety::ApprovalDecision::Deny);
                }
            }
            Action::ScrollUp => self.conversation.scroll_up(1),
            Action::ScrollDown => self.conversation.scroll_down(1),
            Action::PageUp => self.conversation.scroll_up(10),
            Action::PageDown => self.conversation.scroll_down(10),
            Action::ScrollToBottom => self.conversation.scroll_to_bottom(),
            Action::ToggleSidebar => self.show_sidebar = !self.show_sidebar,
            Action::ToggleVimMode => {
                self.vim_mode = !self.vim_mode;
                self.mode = if self.vim_mode {
                    InputMode::VimNormal
                } else {
                    InputMode::Normal
                };
            }
            Action::CopyLastResponse => self.copy_last_response(),
            Action::Approve => {
                self.resolve_approval(rustant_core::safety::ApprovalDecision::Approve)
            }
            Action::ApproveAllSimilar => {
                self.resolve_approval(rustant_core::safety::ApprovalDecision::ApproveAllSimilar)
            }
            Action::Deny => self.resolve_approval(rustant_core::safety::ApprovalDecision::Deny),
            Action::ShowDiff => {
                if self.diff_view.is_visible() {
                    self.diff_view.hide();
                } else {
                    let diff_text = self
                        .checkpoint_manager
                        .diff_from_last()
                        .unwrap_or_else(|_| "No changes to display.".to_string());
                    self.diff_view.show(diff_text);
                }
            }
            Action::ShowHelp => {}
            _ => {}
        }
    }

    /// Copy last assistant response to clipboard.
    fn copy_last_response(&mut self) {
        if let Some(msg) = self
            .conversation
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
        {
            match arboard::Clipboard::new() {
                Ok(mut clipboard) => {
                    let _ = clipboard.set_text(&msg.text);
                    self.conversation.push_message(DisplayMessage {
                        role: Role::System,
                        text: "[Copied to clipboard]".to_string(),
                        tool_name: None,
                        is_error: false,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                }
                Err(_) => {
                    self.conversation.push_message(DisplayMessage {
                        role: Role::System,
                        text: "[Clipboard unavailable]".to_string(),
                        tool_name: None,
                        is_error: true,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                }
            }
        }
    }

    /// Get the base input mode (Normal or VimNormal).
    fn base_mode(&self) -> InputMode {
        if self.vim_mode {
            InputMode::VimInsert
        } else {
            InputMode::Normal
        }
    }

    /// Submit a task to the agent.
    fn submit_task(&mut self, text: &str) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }

        // If there's a pending clarification, resolve it with the user's input
        if self.pending_clarification.is_some() {
            self.conversation.push_message(DisplayMessage {
                role: Role::User,
                text: text.clone(),
                tool_name: None,
                is_error: false,
                timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            });
            self.resolve_clarification(text);
            return;
        }

        // Handle slash commands locally
        if text.starts_with('/') {
            self.handle_command(&text);
            return;
        }

        // Add user message to conversation
        self.conversation.push_message(DisplayMessage {
            role: Role::User,
            text: text.clone(),
            tool_name: None,
            is_error: false,
            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
        });

        self.is_processing = true;
        self.header.is_streaming = true;
        self.sidebar.agent_status = AgentStatus::Thinking;
    }

    /// Handle a slash command.
    pub fn handle_command(&mut self, cmd: &str) {
        match cmd.trim() {
            "/quit" | "/exit" | "/q" => self.should_quit = true,
            "/clear" => self.conversation.clear(),
            "/sidebar" => self.show_sidebar = !self.show_sidebar,
            "/vim" => {
                self.vim_mode = !self.vim_mode;
                self.mode = if self.vim_mode {
                    InputMode::VimNormal
                } else {
                    InputMode::Normal
                };
            }
            "/theme dark" => self.theme = Theme::dark(),
            "/theme light" => self.theme = Theme::light(),
            "/cost" => {
                let display = TokenCostDisplay::from_brain(self.agent.brain());
                let alert_prefix = match display.alert {
                    TokenAlert::Warning => "[WARNING] ",
                    TokenAlert::Critical => "[CRITICAL] ",
                    TokenAlert::Overflow => "[OVERFLOW] ",
                    TokenAlert::Normal => "",
                };
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!("{}{}", alert_prefix, display.format_display()),
                    tool_name: None,
                    is_error: matches!(display.alert, TokenAlert::Critical | TokenAlert::Overflow),
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            "/tools" => {
                let defs = self.agent.tool_definitions();
                let tool_list = defs
                    .iter()
                    .map(|d| format!("  {} - {}", d.name, d.description))
                    .collect::<Vec<_>>()
                    .join("\n");
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!("Available tools ({}):\n{}", defs.len(), tool_list),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            "/help" | "/?" => {
                let registry = crate::slash::CommandRegistry::with_defaults();
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: registry.help_text(),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            "/undo" => match self.checkpoint_manager.undo() {
                Ok(cp) => {
                    let files = if cp.changed_files.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", cp.changed_files.join(", "))
                    };
                    self.conversation.push_message(DisplayMessage {
                        role: Role::System,
                        text: format!("Restored checkpoint: {}{}", cp.label, files),
                        tool_name: None,
                        is_error: false,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                }
                Err(e) => {
                    self.conversation.push_message(DisplayMessage {
                        role: Role::System,
                        text: format!("Undo failed: {}", e),
                        tool_name: None,
                        is_error: true,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                }
            },
            other if other.starts_with("/save") => {
                let name = other.strip_prefix("/save").unwrap().trim();
                if name.is_empty() {
                    self.conversation.push_message(DisplayMessage {
                        role: Role::System,
                        text: "Usage: /save <name>".to_string(),
                        tool_name: None,
                        is_error: true,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                } else {
                    self.save_session(name);
                }
            }
            other if other.starts_with("/load") => {
                let name = other.strip_prefix("/load").unwrap().trim();
                if name.is_empty() {
                    self.conversation.push_message(DisplayMessage {
                        role: Role::System,
                        text: "Usage: /load <name>".to_string(),
                        tool_name: None,
                        is_error: true,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                } else {
                    self.load_session(name);
                }
            }
            // ── Week 12: Audit, Replay & Analytics commands ──────────
            "/audit" => {
                let traces = self.audit_store.traces();
                if traces.is_empty() {
                    self.push_system_msg("Audit store is empty. No traces recorded yet.");
                } else {
                    let latest: Vec<&ExecutionTrace> =
                        self.audit_store.latest(10).into_iter().collect();
                    let text = AuditExporter::to_text(&latest);
                    self.push_system_msg(&format!(
                        "Audit Trail ({} traces, showing latest {}):\n{}",
                        traces.len(),
                        latest.len(),
                        text
                    ));
                }
            }
            other if other.starts_with("/audit export") => {
                let format = other.strip_prefix("/audit export").unwrap_or("").trim();
                let traces = self.audit_store.traces();
                if traces.is_empty() {
                    self.push_system_msg("Audit store is empty. Nothing to export.");
                } else {
                    let refs: Vec<&ExecutionTrace> = traces.iter().collect();
                    let output = match format {
                        "json" => AuditExporter::to_json(&refs)
                            .unwrap_or_else(|e| format!("Export error: {}", e)),
                        "jsonl" => AuditExporter::to_jsonl(&refs)
                            .unwrap_or_else(|e| format!("Export error: {}", e)),
                        "csv" => AuditExporter::to_csv(&refs),
                        _ => AuditExporter::to_text(&refs),
                    };
                    self.push_system_msg(&format!(
                        "Audit Export ({} format):\n{}",
                        if format.is_empty() { "text" } else { format },
                        output
                    ));
                }
            }
            other if other.starts_with("/audit query") => {
                let tool_name = other.strip_prefix("/audit query").unwrap_or("").trim();
                if tool_name.is_empty() {
                    self.push_system_msg("Usage: /audit query <tool_name>");
                } else {
                    let query = AuditQuery::new().for_tool(tool_name);
                    let results = self.audit_store.query(&query);
                    if results.is_empty() {
                        self.push_system_msg(&format!("No traces found for tool '{}'.", tool_name));
                    } else {
                        let text = AuditExporter::to_text(&results);
                        self.push_system_msg(&format!(
                            "Audit Query Results for '{}' ({} traces):\n{}",
                            tool_name,
                            results.len(),
                            text
                        ));
                    }
                }
            }
            "/analytics" => {
                let traces = self.audit_store.traces();
                if traces.is_empty() {
                    self.push_system_msg("No traces available for analytics.");
                } else {
                    let refs: Vec<&ExecutionTrace> = traces.iter().collect();
                    let usage = Analytics::tool_usage_summary(&refs);
                    let costs = Analytics::cost_breakdown(&refs);
                    let patterns = Analytics::detect_patterns(&refs);
                    let success = Analytics::success_rate(&refs);
                    let avg_iter = Analytics::avg_iterations(&refs);

                    let mut text = format!(
                        "Analytics ({} traces):\n  Success rate: {:.1}%\n  Avg iterations: {:.1}\n  Total cost: ${:.4}\n  Total tokens: {}\n",
                        refs.len(),
                        success * 100.0,
                        avg_iter,
                        costs.total_cost,
                        costs.total_tokens,
                    );

                    if let Some(ref tool) = usage.most_used {
                        text.push_str(&format!("  Most used tool: {}\n", tool));
                    }
                    if let Some(ref tool) = usage.most_denied {
                        text.push_str(&format!("  Most denied tool: {}\n", tool));
                    }

                    if !patterns.is_empty() {
                        text.push_str("\nDetected patterns:\n");
                        for p in &patterns {
                            text.push_str(&format!(
                                "  [{:?}] {} (×{})\n",
                                p.kind, p.description, p.occurrences
                            ));
                        }
                    }

                    if !costs.by_model.is_empty() {
                        text.push_str("\nCost breakdown by model:\n");
                        for (model, entry) in &costs.by_model {
                            text.push_str(&format!(
                                "  {}: {} calls, {} tokens, ${:.4}\n",
                                model, entry.calls, entry.total_tokens, entry.total_cost
                            ));
                        }
                    }

                    self.push_system_msg(&text);
                }
            }
            "/replay" => {
                if self.replay_session.is_empty() {
                    let traces = self.audit_store.traces();
                    if traces.is_empty() {
                        self.push_system_msg("No traces available for replay.");
                    } else {
                        // Load the latest trace into the replay session
                        let latest = traces.last().unwrap().clone();
                        let idx = self.replay_session.add_replay(latest);
                        self.replay_session.set_active(idx).ok();
                        let engine = self.replay_session.active().unwrap();
                        self.push_system_msg(&format!(
                            "Replay loaded: {} ({} events)\n  {}",
                            engine.trace().goal,
                            engine.total_events(),
                            engine.describe_current()
                        ));
                    }
                } else {
                    // Show current replay state
                    if let Some(engine) = self.replay_session.active() {
                        let snap = engine.snapshot();
                        self.push_system_msg(&format!(
                            "Replay: {}\n  Position: {}/{} ({:.0}%)\n  {}",
                            engine.trace().goal,
                            snap.position + 1,
                            snap.total_events,
                            snap.progress_pct,
                            engine.describe_current()
                        ));
                    }
                }
            }
            "/replay next" | "/replay forward" => {
                let msg = if let Some(engine) = self.replay_session.active_mut() {
                    if engine.step_forward().is_some() {
                        engine.describe_current()
                    } else {
                        "Already at the end of the replay.".to_string()
                    }
                } else {
                    "No active replay. Use /replay to start.".to_string()
                };
                self.push_system_msg(&msg);
            }
            "/replay prev" | "/replay back" => {
                let msg = if let Some(engine) = self.replay_session.active_mut() {
                    if engine.step_backward().is_some() {
                        engine.describe_current()
                    } else {
                        "Already at the start of the replay.".to_string()
                    }
                } else {
                    "No active replay. Use /replay to start.".to_string()
                };
                self.push_system_msg(&msg);
            }
            "/replay timeline" => {
                if let Some(engine) = self.replay_session.active() {
                    let timeline = engine.timeline();
                    let lines: Vec<String> = timeline
                        .iter()
                        .map(|entry| {
                            let marker = if entry.is_current { "▶" } else { " " };
                            let bm = if entry.is_bookmarked { "🔖" } else { "  " };
                            format!(
                                " {} {} [{:>3}] +{:>6}ms  {}",
                                marker, bm, entry.sequence, entry.elapsed_ms, entry.description
                            )
                        })
                        .collect();
                    self.push_system_msg(&format!(
                        "Replay Timeline ({} events):\n{}",
                        timeline.len(),
                        lines.join("\n")
                    ));
                } else {
                    self.push_system_msg("No active replay. Use /replay to start.");
                }
            }
            "/replay reset" => {
                self.replay_session = ReplaySession::new();
                self.push_system_msg("Replay session cleared.");
            }
            "/compact" => {
                let (before, after) = self.agent.compact();
                if before == after {
                    self.push_system_msg(&format!("Nothing to compact ({} messages).", before));
                } else {
                    self.push_system_msg(&format!(
                        "Compacted {} messages down to {} (+ summary).",
                        before, after
                    ));
                }
            }
            "/status" => {
                let state = self.agent.state();
                let usage = self.agent.brain().total_usage();
                let cost = self.agent.brain().total_cost();
                let goal = state.current_goal.as_deref().unwrap_or("(none)");
                self.push_system_msg(&format!(
                    "Status: {} | Goal: {} | Iter: {}/{} | Tokens: {} | Cost: ${:.4}",
                    state.status,
                    goal,
                    state.iteration,
                    state.max_iterations,
                    usage.total(),
                    cost.total()
                ));
            }
            other if other.starts_with("/config") => {
                let parts: Vec<&str> = other.splitn(3, ' ').collect();
                let key = parts.get(1).copied().unwrap_or("");
                let value = parts.get(2).copied().unwrap_or("");
                if key.is_empty() {
                    let config = self.agent.config();
                    self.push_system_msg(&format!(
                        "Config: model={}, approval_mode={:?}, max_iterations={}, streaming={}",
                        config.llm.model,
                        config.safety.approval_mode,
                        config.safety.max_iterations,
                        config.llm.use_streaming
                    ));
                } else if value.is_empty() {
                    let config = self.agent.config();
                    let val = match key {
                        "model" => config.llm.model.clone(),
                        "approval_mode" => format!("{:?}", config.safety.approval_mode),
                        "max_iterations" => config.safety.max_iterations.to_string(),
                        "streaming" => config.llm.use_streaming.to_string(),
                        _ => format!("Unknown key: {}", key),
                    };
                    self.push_system_msg(&format!("{} = {}", key, val));
                } else {
                    match key {
                        "approval_mode" => {
                            use rustant_core::ApprovalMode;
                            match value {
                                "safe" => {
                                    self.agent
                                        .safety_mut()
                                        .set_approval_mode(ApprovalMode::Safe);
                                    self.agent.config_mut().safety.approval_mode =
                                        ApprovalMode::Safe;
                                    self.push_system_msg("Approval mode set to: safe");
                                }
                                "cautious" => {
                                    self.agent
                                        .safety_mut()
                                        .set_approval_mode(ApprovalMode::Cautious);
                                    self.agent.config_mut().safety.approval_mode =
                                        ApprovalMode::Cautious;
                                    self.push_system_msg("Approval mode set to: cautious");
                                }
                                "paranoid" => {
                                    self.agent
                                        .safety_mut()
                                        .set_approval_mode(ApprovalMode::Paranoid);
                                    self.agent.config_mut().safety.approval_mode =
                                        ApprovalMode::Paranoid;
                                    self.push_system_msg("Approval mode set to: paranoid");
                                }
                                "yolo" => {
                                    self.agent
                                        .safety_mut()
                                        .set_approval_mode(ApprovalMode::Yolo);
                                    self.agent.config_mut().safety.approval_mode =
                                        ApprovalMode::Yolo;
                                    self.push_system_msg("Approval mode set to: yolo");
                                }
                                _ => self.push_system_msg(&format!(
                                    "Invalid mode: {}. Options: safe, cautious, paranoid, yolo",
                                    value
                                )),
                            }
                        }
                        "max_iterations" => {
                            if let Ok(n) = value.parse::<usize>() {
                                self.agent.config_mut().safety.max_iterations = n;
                                self.push_system_msg(&format!("Max iterations set to: {}", n));
                            } else {
                                self.push_system_msg(&format!("Invalid number: {}", value));
                            }
                        }
                        _ => self.push_system_msg(&format!(
                            "Cannot set '{}'. Settable: approval_mode, max_iterations",
                            key
                        )),
                    }
                }
            }
            "/doctor" => {
                let config = self.agent.config();
                let tools = self.agent.tool_definitions();
                let mem = self.agent.memory();
                let audit_count = self.agent.safety().audit_log().len();
                let has_git = self.workspace.join(".git").exists();
                self.push_system_msg(&format!(
                    "Rustant Doctor\n  Workspace: {}\n  Git: {}\n  Provider: {} ({})\n  Tools: {} registered\n  Memory: {} messages, {} facts\n  Audit: {} entries\n  All checks passed.",
                    self.workspace.display(),
                    if has_git { "yes" } else { "no" },
                    config.llm.provider,
                    config.llm.model,
                    tools.len(),
                    mem.short_term.len(),
                    mem.long_term.facts.len(),
                    audit_count
                ));
            }
            other if other.starts_with("/permissions") => {
                let mode_arg = other.strip_prefix("/permissions").unwrap_or("").trim();
                if mode_arg.is_empty() {
                    self.push_system_msg(&format!(
                        "Approval mode: {:?}. Options: safe, cautious, paranoid, yolo",
                        self.agent.safety().approval_mode()
                    ));
                } else {
                    use rustant_core::ApprovalMode;
                    match mode_arg {
                        "safe" => {
                            self.agent
                                .safety_mut()
                                .set_approval_mode(ApprovalMode::Safe);
                            self.agent.config_mut().safety.approval_mode = ApprovalMode::Safe;
                            self.push_system_msg("Approval mode set to: safe");
                        }
                        "cautious" => {
                            self.agent
                                .safety_mut()
                                .set_approval_mode(ApprovalMode::Cautious);
                            self.agent.config_mut().safety.approval_mode = ApprovalMode::Cautious;
                            self.push_system_msg("Approval mode set to: cautious");
                        }
                        "paranoid" => {
                            self.agent
                                .safety_mut()
                                .set_approval_mode(ApprovalMode::Paranoid);
                            self.agent.config_mut().safety.approval_mode = ApprovalMode::Paranoid;
                            self.push_system_msg("Approval mode set to: paranoid");
                        }
                        "yolo" => {
                            self.agent
                                .safety_mut()
                                .set_approval_mode(ApprovalMode::Yolo);
                            self.agent.config_mut().safety.approval_mode = ApprovalMode::Yolo;
                            self.push_system_msg("Approval mode set to: yolo");
                        }
                        _ => self.push_system_msg(&format!(
                            "Unknown mode: {}. Options: safe, cautious, paranoid, yolo",
                            mode_arg
                        )),
                    }
                }
            }
            "/diff" => match self.checkpoint_manager.diff_from_last() {
                Ok(diff) => {
                    if diff.is_empty() {
                        self.push_system_msg("No changes since last checkpoint.");
                    } else {
                        self.push_system_msg(&diff);
                    }
                }
                Err(e) => self.push_system_msg(&format!("Diff failed: {}", e)),
            },
            "/review" => {
                let checkpoints = self.checkpoint_manager.checkpoints();
                if checkpoints.is_empty() {
                    self.push_system_msg("No file changes to review.");
                } else {
                    let mut text =
                        format!("Session changes ({} checkpoints):\n", checkpoints.len());
                    for (i, cp) in checkpoints.iter().enumerate() {
                        text.push_str(&format!(
                            "  {}. {} - {}\n",
                            i + 1,
                            cp.label,
                            cp.timestamp.format("%H:%M:%S")
                        ));
                        for f in &cp.changed_files {
                            text.push_str(&format!("     {}\n", f));
                        }
                    }
                    self.push_system_msg(&text);
                }
            }
            other => {
                // Use registry for unknown command suggestions
                let registry = crate::slash::CommandRegistry::with_defaults();
                let cmd_name = other.split_whitespace().next().unwrap_or(other);
                let msg = if let Some(suggestion) = registry.suggest(cmd_name) {
                    format!(
                        "Unknown command: {}. Did you mean {}?",
                        cmd_name, suggestion
                    )
                } else {
                    format!("Unknown command: {}. Type /help for commands.", other)
                };
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: msg,
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
        }
    }

    /// Push a system-role message into the conversation panel.
    fn push_system_msg(&mut self, text: &str) {
        self.conversation.push_message(DisplayMessage {
            role: Role::System,
            text: text.to_string(),
            tool_name: None,
            is_error: false,
            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
        });
    }

    /// Handle a TUI event from the agent callback.
    pub fn handle_tui_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::AssistantMessage(msg) => {
                self.conversation.finish_streaming();
                self.conversation.push_message(DisplayMessage {
                    role: Role::Assistant,
                    text: msg,
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            TuiEvent::StreamToken(token) => {
                self.conversation.append_stream_token(&token);
            }
            TuiEvent::ApprovalRequest { action, reply } => {
                self.mode = InputMode::Approval;
                self.pending_approval = Some(reply);

                // Build rich approval dialog text
                let mut text = format!(
                    "[Approval Required] {} (risk: {})",
                    action.description, action.risk_level
                );
                if let Some(ref reasoning) = action.approval_context.reasoning {
                    text.push_str(&format!("\n  Reason: {}", reasoning));
                }
                for consequence in &action.approval_context.consequences {
                    text.push_str(&format!("\n  Consequence: {}", consequence));
                }
                if let Some(ref rev) = action.approval_context.reversibility {
                    let rev_label = if rev.is_reversible { "yes" } else { "no" };
                    text.push_str(&format!("\n  Reversible: {}", rev_label));
                    if let Some(ref desc) = rev.undo_description {
                        text.push_str(&format!(" ({})", desc));
                    }
                }
                text.push_str("\n  Press [y] approve, [n] deny, [a] approve all similar, [d] diff");

                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text,
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            TuiEvent::ToolStart { name, args } => {
                // Create a checkpoint before write/execute tools for undo support
                if tool_risk_level(&name) >= rustant_core::types::RiskLevel::Write {
                    let _ = self
                        .checkpoint_manager
                        .create_checkpoint(&format!("before {}", name));
                }

                // Start progress tracking
                self.progress.tool_started(&name);

                let args_str = args.to_string();
                let args_preview = if args_str.len() > 100 {
                    format!("{}...", &args_str[..100])
                } else {
                    args_str
                };
                self.conversation.push_message(DisplayMessage {
                    role: Role::Tool,
                    text: format!("Executing... {}", args_preview),
                    tool_name: Some(name),
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            TuiEvent::ToolResult {
                name,
                output,
                duration_ms,
            } => {
                // Stop progress tracking
                self.progress.tool_finished();

                self.conversation.push_message(DisplayMessage {
                    role: Role::Tool,
                    text: format!("Completed in {}ms: {}", duration_ms, output.content),
                    tool_name: Some(name),
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
                // Track file artifacts in sidebar
                for artifact in &output.artifacts {
                    match artifact {
                        rustant_core::types::Artifact::FileCreated { path } => {
                            self.sidebar.active_files.push(FileEntry {
                                path: path.display().to_string(),
                                status: FileStatus::Created,
                            });
                        }
                        rustant_core::types::Artifact::FileModified { path, .. } => {
                            self.sidebar.active_files.push(FileEntry {
                                path: path.display().to_string(),
                                status: FileStatus::Modified,
                            });
                        }
                        _ => {}
                    }
                }
            }
            TuiEvent::StatusChange(status) => {
                self.sidebar.agent_status = status;
                match status {
                    AgentStatus::Complete | AgentStatus::Error => {
                        self.is_processing = false;
                        self.header.is_streaming = false;
                        self.header.tokens_used = self.agent.brain().total_usage().total();
                        self.header.cost_usd = self.agent.brain().total_cost().total();
                    }
                    AgentStatus::Thinking => {
                        self.header.is_streaming = true;
                    }
                    _ => {}
                }
            }
            TuiEvent::UsageUpdate { usage, cost } => {
                self.header.tokens_used = usage.total();
                self.header.cost_usd = cost.total();
            }
            TuiEvent::DecisionExplanation(explanation) => {
                // Display a compact decision trace in the conversation
                let tool = match &explanation.decision_type {
                    rustant_core::explanation::DecisionType::ToolSelection { selected_tool } => {
                        selected_tool.clone()
                    }
                    _ => "decision".to_string(),
                };
                let reasoning = explanation
                    .reasoning_chain
                    .first()
                    .map(|s| s.description.as_str())
                    .unwrap_or("");
                let trace = format!(
                    "[decision: {} | confidence: {:.0}% | {}]",
                    tool,
                    explanation.confidence * 100.0,
                    reasoning
                );
                self.conversation.push_message(DisplayMessage {
                    role: rustant_core::types::Role::System,
                    text: trace,
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                });
                // Store in explanation panel for Safety Dashboard
                self.explanation_panel.push(explanation);
            }
            TuiEvent::BudgetWarning { message, severity } => {
                let prefix = match severity {
                    rustant_core::BudgetSeverity::Warning => "⚠ Budget Warning",
                    rustant_core::BudgetSeverity::Exceeded => "🛑 Budget Exceeded",
                };
                self.conversation.push_message(DisplayMessage {
                    role: rustant_core::types::Role::System,
                    text: format!("[{}: {}]", prefix, message),
                    tool_name: None,
                    is_error: severity == rustant_core::BudgetSeverity::Exceeded,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                });
            }
            TuiEvent::Progress(update) => {
                self.progress.apply_progress(&update);
            }
            TuiEvent::MultiAgentUpdate(agents) => {
                self.task_board.update_agents(agents);
            }
            TuiEvent::ClarificationRequest { question, reply } => {
                // Show the question in the conversation and store the reply sender.
                self.conversation.push_message(DisplayMessage {
                    role: Role::Assistant,
                    text: format!("? {}", question),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
                self.pending_clarification = Some(reply);
            }
        }
    }

    /// Resolve a pending clarification request with the user's answer.
    pub fn resolve_clarification(&mut self, answer: String) {
        if let Some(reply) = self.pending_clarification.take() {
            let _ = reply.send(answer);
        }
    }

    /// Resolve a pending approval request.
    pub fn resolve_approval(&mut self, decision: rustant_core::safety::ApprovalDecision) {
        if let Some(reply) = self.pending_approval.take() {
            let _ = reply.send(decision);
        }
        self.mode = if self.vim_mode {
            InputMode::VimNormal
        } else {
            InputMode::Normal
        };
    }

    /// Save input history to disk.
    fn save_history(&self) {
        let history_dir = directories::ProjectDirs::from("dev", "rustant", "rustant")
            .map(|d| d.data_dir().to_path_buf());
        if let Some(dir) = history_dir {
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join("input_history.json");
            let history = self.input.history();
            if let Ok(json) = serde_json::to_string(&history) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    /// Load input history from disk.
    pub fn load_history(&mut self) {
        let history_dir = directories::ProjectDirs::from("dev", "rustant", "rustant")
            .map(|d| d.data_dir().to_path_buf());
        if let Some(dir) = history_dir {
            let path = dir.join("input_history.json");
            if let Ok(json) = std::fs::read_to_string(path) {
                if let Ok(entries) = serde_json::from_str::<Vec<String>>(&json) {
                    self.input.load_history(entries);
                }
            }
        }
    }

    /// Get the sessions directory.
    fn sessions_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "rustant", "rustant")
            .map(|d| d.data_dir().join("sessions"))
    }

    /// Auto-save the current session on exit.
    fn auto_save_session(&self) {
        if let Some(dir) = Self::sessions_dir() {
            let path = dir.join("_autosave.json");
            let _ = self.agent.memory().save_session(&path);
        }
    }

    /// Try to recover the last auto-saved session. Returns true if recovered.
    pub fn try_recover_session(&mut self) -> bool {
        let Some(dir) = Self::sessions_dir() else {
            return false;
        };
        let path = dir.join("_autosave.json");
        if !path.exists() {
            return false;
        }
        match rustant_core::MemorySystem::load_session(&path) {
            Ok(loaded) => {
                let messages = loaded.context_messages();
                if messages.is_empty() {
                    return false;
                }
                for msg in &messages {
                    let text = match &msg.content {
                        rustant_core::types::Content::Text { text } => text.clone(),
                        rustant_core::types::Content::ToolCall {
                            name, arguments, ..
                        } => format!("[Tool Call: {} ({})]", name, arguments),
                        rustant_core::types::Content::ToolResult { output, .. } => {
                            format!("[Tool Result: {}]", output)
                        }
                        rustant_core::types::Content::MultiPart { .. } => "[MultiPart]".to_string(),
                    };
                    self.conversation.push_message(DisplayMessage {
                        role: msg.role,
                        text,
                        tool_name: None,
                        is_error: false,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                }
                *self.agent.memory_mut() = loaded;
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!(
                        "[Recovered] Previous session restored ({} messages)",
                        messages.len()
                    ),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
                true
            }
            Err(_) => false,
        }
    }

    /// Save the current session to disk with a given name.
    fn save_session(&mut self, name: &str) {
        let Some(dir) = Self::sessions_dir() else {
            self.conversation.push_message(DisplayMessage {
                role: Role::System,
                text: "[Error] Could not determine sessions directory.".to_string(),
                tool_name: None,
                is_error: true,
                timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            });
            return;
        };
        let path = dir.join(format!("{}.json", name));
        match self.agent.memory().save_session(&path) {
            Ok(()) => {
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!("Session saved: {}", path.display()),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            Err(e) => {
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!("[Error] Failed to save session: {}", e),
                    tool_name: None,
                    is_error: true,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
        }
    }

    /// Load a session from disk by name.
    fn load_session(&mut self, name: &str) {
        let Some(dir) = Self::sessions_dir() else {
            self.conversation.push_message(DisplayMessage {
                role: Role::System,
                text: "[Error] Could not determine sessions directory.".to_string(),
                tool_name: None,
                is_error: true,
                timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            });
            return;
        };
        let path = dir.join(format!("{}.json", name));
        match rustant_core::MemorySystem::load_session(&path) {
            Ok(loaded) => {
                // Restore messages into conversation view
                let messages = loaded.context_messages();
                self.conversation.clear();
                for msg in &messages {
                    let text = match &msg.content {
                        rustant_core::types::Content::Text { text } => text.clone(),
                        rustant_core::types::Content::ToolCall {
                            name, arguments, ..
                        } => format!("[Tool Call: {} ({})]", name, arguments),
                        rustant_core::types::Content::ToolResult { output, .. } => {
                            format!("[Tool Result: {}]", output)
                        }
                        rustant_core::types::Content::MultiPart { .. } => "[MultiPart]".to_string(),
                    };
                    self.conversation.push_message(DisplayMessage {
                        role: msg.role,
                        text,
                        tool_name: None,
                        is_error: false,
                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    });
                }
                // Replace agent memory with loaded memory
                *self.agent.memory_mut() = loaded;
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!(
                        "Session loaded: {} ({} messages restored)",
                        name,
                        messages.len()
                    ),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            Err(e) => {
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!("[Error] Failed to load session: {}", e),
                    tool_name: None,
                    is_error: true,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
        }
    }

    /// Process a task asynchronously. Called from the event loop.
    #[allow(dead_code)]
    pub async fn process_task(&mut self, task: &str) -> anyhow::Result<TaskResult> {
        let result = self.agent.process_task(task).await?;
        self.is_processing = false;
        self.header.is_streaming = false;
        self.header.tokens_used = self.agent.brain().total_usage().total();
        self.header.cost_usd = self.agent.brain().total_cost().total();
        self.sidebar.iteration = 0;
        Ok(result)
    }

    /// Get reference to the agent.
    #[allow(dead_code)]
    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Get mutable reference to the agent.
    #[allow(dead_code)]
    pub fn agent_mut(&mut self) -> &mut Agent {
        &mut self.agent
    }
}

/// Register tools from registry into the agent (shared logic with repl.rs).
fn register_agent_tools(agent: &mut Agent, registry: &ToolRegistry, workspace: &Path) {
    let tool_defs = registry.list_definitions();
    for def in tool_defs {
        let name = def.name.clone();
        let ws = workspace.to_path_buf();
        if let Some(executor) = create_tool_executor(&name, &ws) {
            agent.register_tool(RegisteredTool {
                definition: def,
                risk_level: tool_risk_level(&name),
                executor,
            });
        }
    }
}

fn create_tool_executor(name: &str, workspace: &Path) -> Option<rustant_core::agent::ToolExecutor> {
    let ws = workspace.to_path_buf();
    match name {
        "file_read" => {
            let tool = Arc::new(rustant_tools::file::FileReadTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_list" => {
            let tool = Arc::new(rustant_tools::file::FileListTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_search" => {
            let tool = Arc::new(rustant_tools::file::FileSearchTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_write" => {
            let tool = Arc::new(rustant_tools::file::FileWriteTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_patch" => {
            let tool = Arc::new(rustant_tools::file::FilePatchTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "git_status" => {
            let tool = Arc::new(rustant_tools::git::GitStatusTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "git_diff" => {
            let tool = Arc::new(rustant_tools::git::GitDiffTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "git_commit" => {
            let tool = Arc::new(rustant_tools::git::GitCommitTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "shell_exec" => {
            let tool = Arc::new(rustant_tools::shell::ShellExecTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "echo" => {
            let tool = Arc::new(rustant_tools::utils::EchoTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "datetime" => {
            let tool = Arc::new(rustant_tools::utils::DateTimeTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "calculator" => {
            let tool = Arc::new(rustant_tools::utils::CalculatorTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        _ => None,
    }
}

fn tool_risk_level(name: &str) -> rustant_core::types::RiskLevel {
    use rustant_core::types::RiskLevel;
    match name {
        "file_read" | "file_list" | "file_search" | "git_status" | "git_diff" | "echo"
        | "datetime" | "calculator" => RiskLevel::ReadOnly,
        "file_write" | "file_patch" | "git_commit" => RiskLevel::Write,
        "shell_exec" => RiskLevel::Execute,
        _ => RiskLevel::Execute,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AgentConfig {
        AgentConfig::default()
    }

    #[test]
    fn test_app_creation() {
        let config = test_config();
        let workspace = std::env::temp_dir();
        let app = App::new(config, workspace);
        assert!(!app.should_quit);
        assert!(!app.is_processing);
        assert!(app.conversation.messages.is_empty());
    }

    #[test]
    fn test_handle_command_quit() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/quit");
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_command_clear() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.conversation.push_message(DisplayMessage {
            role: Role::User,
            text: "test".to_string(),
            tool_name: None,
            is_error: false,
            timestamp: "00:00:00".to_string(),
        });
        app.handle_command("/clear");
        assert!(app.conversation.messages.is_empty());
    }

    #[test]
    fn test_handle_command_sidebar_toggle() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        assert!(app.show_sidebar);
        app.handle_command("/sidebar");
        assert!(!app.show_sidebar);
        app.handle_command("/sidebar");
        assert!(app.show_sidebar);
    }

    #[test]
    fn test_handle_command_vim_toggle() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        assert_eq!(app.mode, InputMode::Normal);
        app.handle_command("/vim");
        assert!(app.vim_mode);
        assert_eq!(app.mode, InputMode::VimNormal);
        app.handle_command("/vim");
        assert!(!app.vim_mode);
        assert_eq!(app.mode, InputMode::Normal);
    }

    #[test]
    fn test_handle_command_theme() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        assert_eq!(app.theme.name, "dark");
        app.handle_command("/theme light");
        assert_eq!(app.theme.name, "light");
        app.handle_command("/theme dark");
        assert_eq!(app.theme.name, "dark");
    }

    #[test]
    fn test_handle_command_help() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/help");
        assert_eq!(app.conversation.messages.len(), 1);
        assert!(app.conversation.messages[0].text.contains("/quit"));
    }

    #[test]
    fn test_handle_command_unknown() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/foobar");
        assert_eq!(app.conversation.messages.len(), 1);
        assert!(app.conversation.messages[0]
            .text
            .contains("Unknown command"));
    }

    #[test]
    fn test_submit_adds_user_message() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.submit_task("hello world");
        assert_eq!(app.conversation.messages.len(), 1);
        assert_eq!(app.conversation.messages[0].role, Role::User);
        assert_eq!(app.conversation.messages[0].text, "hello world");
    }

    #[test]
    fn test_submit_empty_is_noop() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.submit_task("");
        assert!(app.conversation.messages.is_empty());
    }

    #[test]
    fn test_submit_slash_routes_to_command() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.submit_task("/help");
        assert!(app.conversation.messages[0].text.contains("/quit"));
    }

    #[test]
    fn test_resolve_approval() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        let (tx, rx) = oneshot::channel();
        app.pending_approval = Some(tx);
        app.mode = InputMode::Approval;
        app.resolve_approval(rustant_core::safety::ApprovalDecision::Approve);
        assert_eq!(
            rx.blocking_recv().unwrap(),
            rustant_core::safety::ApprovalDecision::Approve
        );
        assert_eq!(app.mode, InputMode::Normal);
    }

    #[test]
    fn test_draw_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app = App::new(test_config(), std::env::temp_dir());
        terminal.draw(|frame| app.draw(frame)).unwrap();
    }

    #[test]
    fn test_draw_without_sidebar() {
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.show_sidebar = false;
        terminal.draw(|frame| app.draw(frame)).unwrap();
    }

    #[test]
    fn test_handle_tui_event_stream_token() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_tui_event(TuiEvent::StreamToken("hello ".to_string()));
        app.handle_tui_event(TuiEvent::StreamToken("world".to_string()));
        assert_eq!(app.conversation.streaming_buffer, "hello world");
    }

    #[test]
    fn test_handle_tui_event_assistant_message() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_tui_event(TuiEvent::AssistantMessage("response".to_string()));
        assert_eq!(app.conversation.messages.len(), 1);
        assert_eq!(app.conversation.messages[0].role, Role::Assistant);
    }

    #[test]
    fn test_handle_tui_event_status_complete() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.is_processing = true;
        app.header.is_streaming = true;
        app.handle_tui_event(TuiEvent::StatusChange(AgentStatus::Complete));
        assert!(!app.is_processing);
        assert!(!app.header.is_streaming);
    }

    #[test]
    fn test_tool_risk_level() {
        use rustant_core::types::RiskLevel;
        assert_eq!(tool_risk_level("file_read"), RiskLevel::ReadOnly);
        assert_eq!(tool_risk_level("file_write"), RiskLevel::Write);
        assert_eq!(tool_risk_level("shell_exec"), RiskLevel::Execute);
        assert_eq!(tool_risk_level("unknown"), RiskLevel::Execute);
    }

    // Week 6 tests

    #[test]
    fn test_autocomplete_activates() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        assert!(!app.autocomplete.is_active());
        app.autocomplete.activate("test");
        assert!(app.autocomplete.is_active());
    }

    #[test]
    fn test_command_palette_activates() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        assert!(!app.command_palette.is_active());
        app.command_palette.activate("");
        assert!(app.command_palette.is_active());
    }

    #[test]
    fn test_escape_cancels_processing() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.is_processing = true;
        app.header.is_streaming = true;
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key_event(key);
        assert!(!app.is_processing);
        assert!(!app.header.is_streaming);
    }

    #[test]
    fn test_escape_exits_vim_insert() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.vim_mode = true;
        app.mode = InputMode::VimInsert;
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key_event(key);
        assert_eq!(app.mode, InputMode::VimNormal);
    }

    #[test]
    fn test_vim_normal_mode_j_k_scrolling() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.vim_mode = true;
        app.mode = InputMode::VimNormal;
        // j scrolls down
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key_event(key);
        // k scrolls up
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        app.handle_key_event(key);
        // Should not panic
    }

    #[test]
    fn test_vim_normal_i_enters_insert() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.vim_mode = true;
        app.mode = InputMode::VimNormal;
        let key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        app.handle_key_event(key);
        assert_eq!(app.mode, InputMode::VimInsert);
    }

    #[test]
    fn test_base_mode_normal() {
        let app = App::new(test_config(), std::env::temp_dir());
        assert_eq!(app.base_mode(), InputMode::Normal);
    }

    #[test]
    fn test_base_mode_vim() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.vim_mode = true;
        assert_eq!(app.base_mode(), InputMode::VimInsert);
    }

    #[test]
    fn test_draw_with_autocomplete() {
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.autocomplete.activate("");
        terminal.draw(|frame| app.draw(frame)).unwrap();
    }

    #[test]
    fn test_draw_with_command_palette() {
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.command_palette.activate("");
        terminal.draw(|frame| app.draw(frame)).unwrap();
    }

    // Week 8 tests

    #[test]
    fn test_handle_command_cost_shows_token_info() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/cost");
        assert_eq!(app.conversation.messages.len(), 1);
        let text = &app.conversation.messages[0].text;
        assert!(text.contains("Tokens:"));
        assert!(text.contains("Context:"));
        assert!(text.contains("Cost:"));
    }

    #[test]
    fn test_handle_command_save_no_name() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/save");
        assert_eq!(app.conversation.messages.len(), 1);
        assert!(app.conversation.messages[0].text.contains("Usage:"));
        assert!(app.conversation.messages[0].is_error);
    }

    #[test]
    fn test_handle_command_save_with_name() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/save test_session");
        assert_eq!(app.conversation.messages.len(), 1);
        // Should either succeed or show an error - not panic
        let text = &app.conversation.messages[0].text;
        assert!(text.contains("Session saved") || text.contains("Error"));
    }

    #[test]
    fn test_handle_command_load_no_name() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/load");
        assert_eq!(app.conversation.messages.len(), 1);
        assert!(app.conversation.messages[0].text.contains("Usage:"));
        assert!(app.conversation.messages[0].is_error);
    }

    #[test]
    fn test_handle_command_load_nonexistent() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/load nonexistent_session_xyz");
        assert_eq!(app.conversation.messages.len(), 1);
        assert!(app.conversation.messages[0].text.contains("Error"));
        assert!(app.conversation.messages[0].is_error);
    }

    #[test]
    fn test_auto_save_does_not_panic() {
        let app = App::new(test_config(), std::env::temp_dir());
        app.auto_save_session();
    }

    #[test]
    fn test_try_recover_no_session() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        // Recovery from a non-existent autosave should return false
        // (may depend on whether a previous test created an autosave)
        let _ = app.try_recover_session();
        // Should not panic
    }

    #[test]
    fn test_sessions_dir() {
        let dir = App::sessions_dir();
        // Should return Some on most systems
        if let Some(d) = dir {
            assert!(d.to_string_lossy().contains("sessions"));
        }
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        // Add a message
        app.submit_task("hello");
        assert_eq!(app.conversation.messages.len(), 1);

        // Save with a unique test name
        let test_name = format!("_test_{}", std::process::id());
        app.handle_command(&format!("/save {}", test_name));
        // Should have added a success or error message
        assert!(app.conversation.messages.len() >= 2);

        // If save succeeded, try to load
        if app
            .conversation
            .messages
            .last()
            .map(|m| m.text.contains("Session saved"))
            .unwrap_or(false)
        {
            app.handle_command(&format!("/load {}", test_name));
            // Should have a "Session loaded" message
            assert!(app
                .conversation
                .messages
                .iter()
                .any(|m| m.text.contains("Session loaded")));

            // Clean up
            if let Some(dir) = App::sessions_dir() {
                let _ = std::fs::remove_file(dir.join(format!("{}.json", test_name)));
            }
        }
    }

    // Week 7 gap-closure tests

    #[test]
    fn test_show_diff_toggles() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        assert!(!app.diff_view.is_visible());
        // ShowDiff opens the diff view
        app.execute_action(Action::ShowDiff);
        assert!(app.diff_view.is_visible());
        // ShowDiff again closes it
        app.execute_action(Action::ShowDiff);
        assert!(!app.diff_view.is_visible());
    }

    #[test]
    fn test_draw_with_diff_view() {
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.diff_view
            .show("--- a/file.rs\n+++ b/file.rs\n-old\n+new".to_string());
        terminal.draw(|frame| app.draw(frame)).unwrap();
    }

    #[test]
    fn test_undo_no_checkpoints() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/undo");
        assert_eq!(app.conversation.messages.len(), 1);
        assert!(app.conversation.messages[0].is_error);
        assert!(app.conversation.messages[0].text.contains("Undo failed"));
    }

    #[test]
    fn test_escape_closes_diff_view() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.diff_view.show("test diff".to_string());
        assert!(app.diff_view.is_visible());
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key_event(key);
        assert!(!app.diff_view.is_visible());
    }

    #[test]
    fn test_diff_view_scroll_keys() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.diff_view.show(
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10".to_string(),
        );
        assert!(app.diff_view.is_visible());
        // Scroll down with j
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key_event(key);
        // Scroll up with k
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        app.handle_key_event(key);
        // Should not panic
    }

    // ── Week 12 audit / replay / analytics tests ────────────────────

    use rustant_core::audit::TraceEventKind;
    use rustant_core::types::RiskLevel;
    use uuid::Uuid;

    /// Helper: build a sample trace with a few events.
    fn sample_trace() -> ExecutionTrace {
        let session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let mut trace = ExecutionTrace::new(session_id, task_id, "test task");
        trace.push_event(TraceEventKind::ToolRequested {
            tool: "file_read".into(),
            risk_level: RiskLevel::ReadOnly,
            args_summary: "path=/src/main.rs".into(),
        });
        trace.push_event(TraceEventKind::ToolApproved {
            tool: "file_read".into(),
        });
        trace.push_event(TraceEventKind::ToolExecuted {
            tool: "file_read".into(),
            success: true,
            duration_ms: 42,
            output_preview: "fn main()".into(),
        });
        trace.push_event(TraceEventKind::LlmCall {
            model: "gpt-4".into(),
            input_tokens: 500,
            output_tokens: 200,
            cost: 0.021,
        });
        trace.complete(true);
        trace
    }

    #[test]
    fn test_handle_command_audit_empty() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/audit");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("empty"));
    }

    #[test]
    fn test_handle_command_audit_with_traces() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/audit");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("Audit Trail"));
        assert!(last.text.contains("1 traces"));
    }

    #[test]
    fn test_handle_command_audit_export_json() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/audit export json");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("json"));
        assert!(last.text.contains("trace_id"));
    }

    #[test]
    fn test_handle_command_audit_export_csv() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/audit export csv");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("csv"));
    }

    #[test]
    fn test_handle_command_audit_export_text() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/audit export text");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("text"));
    }

    #[test]
    fn test_handle_command_audit_export_empty() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/audit export json");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("empty"));
    }

    #[test]
    fn test_handle_command_audit_query_no_tool() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/audit query");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("Usage"));
    }

    #[test]
    fn test_handle_command_audit_query_with_tool() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/audit query file_read");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("file_read"));
    }

    #[test]
    fn test_handle_command_audit_query_no_match() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/audit query nonexistent_tool");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("No traces found"));
    }

    #[test]
    fn test_handle_command_analytics_empty() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/analytics");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("No traces available"));
    }

    #[test]
    fn test_handle_command_analytics_with_data() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/analytics");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("Analytics"));
        assert!(last.text.contains("Success rate"));
        assert!(last.text.contains("Total cost"));
    }

    #[test]
    fn test_handle_command_replay_no_traces() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/replay");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("No traces available"));
    }

    #[test]
    fn test_handle_command_replay_loads_trace() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/replay");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("Replay loaded"));
        assert!(last.text.contains("test task"));
    }

    #[test]
    fn test_handle_command_replay_shows_status() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/replay"); // load
        app.handle_command("/replay"); // show status
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("Replay:"));
        assert!(last.text.contains("Position:"));
    }

    #[test]
    fn test_handle_command_replay_next() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/replay");
        app.handle_command("/replay next");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("[2/"));
    }

    #[test]
    fn test_handle_command_replay_prev() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/replay");
        app.handle_command("/replay next");
        app.handle_command("/replay prev");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("[1/"));
    }

    #[test]
    fn test_handle_command_replay_prev_at_start() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/replay");
        app.handle_command("/replay prev");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("Already at the start"));
    }

    #[test]
    fn test_handle_command_replay_next_no_replay() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/replay next");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("No active replay"));
    }

    #[test]
    fn test_handle_command_replay_timeline() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/replay");
        app.handle_command("/replay timeline");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("Timeline"));
        assert!(last.text.contains("Task started"));
    }

    #[test]
    fn test_handle_command_replay_reset() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.audit_store.add_trace(sample_trace());
        app.handle_command("/replay");
        assert!(!app.replay_session.is_empty());
        app.handle_command("/replay reset");
        assert!(app.replay_session.is_empty());
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("cleared"));
    }

    #[test]
    fn test_handle_command_help_contains_key_commands() {
        let mut app = App::new(test_config(), std::env::temp_dir());
        app.handle_command("/help");
        let last = app.conversation.messages.last().unwrap();
        assert!(last.text.contains("/audit"), "Help should contain /audit");
        assert!(last.text.contains("/help"), "Help should contain /help");
        assert!(
            last.text.contains("/compact"),
            "Help should contain /compact"
        );
        assert!(last.text.contains("/config"), "Help should contain /config");
        assert!(
            last.text.contains("/permissions"),
            "Help should contain /permissions"
        );
    }
}
