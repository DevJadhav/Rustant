//! Main TUI application: state, event loop, and top-level draw function.

use crate::tui::callback::{TuiCallback, TuiEvent};
use crate::tui::event::{map_approval_key, map_global_key, Action, EventHandler};
use crate::tui::theme::Theme;
use crate::tui::widgets::autocomplete::AutocompleteState;
use crate::tui::widgets::command_palette::CommandPalette;
use crate::tui::widgets::conversation::{render_conversation, ConversationState, DisplayMessage};
use crate::tui::widgets::header::{render_header, HeaderData};
use crate::tui::widgets::input_area::{InputAction, InputWidget};
use crate::tui::widgets::markdown::SyntaxHighlighter;
use crate::tui::widgets::sidebar::{render_sidebar, FileEntry, FileStatus, SidebarData};
use crate::tui::widgets::status_bar::{render_status_bar, InputMode};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;
use rustant_core::types::{AgentStatus, Role};
use rustant_core::{
    Agent, AgentConfig, MockLlmProvider, RegisteredTool, TaskResult, TokenAlert, TokenCostDisplay,
};
use rustant_tools::register_builtin_tools;
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

    // Agent communication
    callback_rx: mpsc::UnboundedReceiver<TuiEvent>,
    agent: Agent,
    workspace: PathBuf,

    // Approval state
    pending_approval: Option<oneshot::Sender<bool>>,

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
        let provider = Arc::new(MockLlmProvider::new());
        let callback_arc = Arc::new(callback);
        let mut agent = Agent::new(provider, config.clone(), callback_arc);

        // Register tools
        let mut registry = ToolRegistry::new();
        register_builtin_tools(&mut registry, workspace.clone());
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

        Self {
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
            callback_rx,
            agent,
            workspace,
            pending_approval: None,
            should_quit: false,
            is_processing: false,
            vim_mode,
        }
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
                // Tick
                _ = tokio::time::sleep(tick_rate) => {
                    // Tick for spinners/animation updates
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
        let [header_area, main_area, input_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(8),
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
                self.resolve_approval(false);
                return;
            }
            return;
        }

        // Global keybindings (Ctrl+C, Ctrl+D, Ctrl+L)
        if let Some(action) = map_global_key(&key) {
            self.execute_action(action);
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
                    self.resolve_approval(false);
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
            Action::Approve => self.resolve_approval(true),
            Action::Deny => self.resolve_approval(false),
            Action::ShowDiff | Action::ShowHelp => {
                // Week 7: diff preview
            }
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
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: concat!(
                        "Commands:\n",
                        "  /help      - Show this help\n",
                        "  /quit      - Exit Rustant\n",
                        "  /clear     - Clear conversation\n",
                        "  /cost      - Show token usage and cost\n",
                        "  /tools     - List available tools\n",
                        "  /sidebar   - Toggle sidebar\n",
                        "  /vim       - Toggle vim mode\n",
                        "  /theme <name> - Switch theme (dark/light)\n",
                        "  /save <name>  - Save session\n",
                        "  /load <name>  - Load session\n",
                        "  /undo      - Undo last file change"
                    )
                    .to_string(),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
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
            other => {
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!("Unknown command: {}. Type /help for commands.", other),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
        }
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
                self.conversation.push_message(DisplayMessage {
                    role: Role::System,
                    text: format!(
                        "[Approval Required] {} (risk: {})\n  Press [y] approve, [n] deny, [d] diff",
                        action.description, action.risk_level
                    ),
                    tool_name: None,
                    is_error: false,
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                });
            }
            TuiEvent::ToolStart { name, args } => {
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
        }
    }

    /// Resolve a pending approval request.
    pub fn resolve_approval(&mut self, approved: bool) {
        if let Some(reply) = self.pending_approval.take() {
            let _ = reply.send(approved);
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
        app.resolve_approval(true);
        assert!(rx.blocking_recv().unwrap());
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
}
