//! Interactive REPL input with history and slash command autocomplete.
//!
//! Replaces raw `stdin.read_line()` with a crossterm-based input handler
//! that provides:
//! - Up/Down arrow history navigation with draft preservation
//! - `/` prefix slash command autocomplete (Tab to accept, Esc to dismiss)
//! - Ctrl-C to clear line, Ctrl-D on empty line for EOF
//! - Persistent history file at `.rustant/repl_history`

use crate::slash::CommandRegistry;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal,
};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Persistent command history.
pub struct InputHistory {
    entries: Vec<String>,
    index: Option<usize>,
    draft: Option<String>,
    file_path: PathBuf,
    max_entries: usize,
}

impl InputHistory {
    /// Create a new history, loading from the given workspace directory.
    pub fn new(workspace: &Path) -> Self {
        let file_path = workspace.join(".rustant").join("repl_history");
        let entries = Self::load_from_file(&file_path);
        Self {
            entries,
            index: None,
            draft: None,
            file_path,
            max_entries: 500,
        }
    }

    fn load_from_file(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect()
    }

    fn save_to_file(&self) {
        if let Some(parent) = self.file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content: String = self.entries.iter().map(|e| format!("{}\n", e)).collect();
        let _ = std::fs::write(&self.file_path, content);
    }

    /// Push a new entry to history, deduplicating consecutive entries.
    pub fn push(&mut self, entry: &str) {
        let trimmed = entry.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        // Skip duplicate consecutive entries
        if self.entries.last().map(|s| s.as_str()) == Some(&trimmed) {
            return;
        }
        self.entries.push(trimmed);
        // Trim to max size
        while self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.save_to_file();
        self.reset_navigation();
    }

    /// Navigate up (older entries). Returns the entry to display, or None.
    fn navigate_up(&mut self, current_buffer: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.index {
            None => {
                // Save current draft
                self.draft = Some(current_buffer.to_string());
                self.index = Some(self.entries.len() - 1);
            }
            Some(0) => return Some(&self.entries[0]),
            Some(ref mut idx) => {
                *idx -= 1;
            }
        }
        self.index.map(|i| self.entries[i].as_str())
    }

    /// Navigate down (newer entries). Returns the entry or restores draft.
    fn navigate_down(&mut self) -> Option<String> {
        match self.index {
            None => None,
            Some(idx) => {
                if idx >= self.entries.len() - 1 {
                    // Restore draft
                    self.index = None;
                    self.draft.take()
                } else {
                    self.index = Some(idx + 1);
                    Some(self.entries[idx + 1].clone())
                }
            }
        }
    }

    /// Reset navigation state.
    fn reset_navigation(&mut self) {
        self.index = None;
        self.draft = None;
    }
}

/// Completion state for slash commands.
struct CompletionState {
    matches: Vec<String>,
    selected: usize,
}

/// Interactive REPL input handler.
pub struct ReplInput {
    history: InputHistory,
}

impl ReplInput {
    /// Create a new interactive input handler.
    pub fn new(workspace: &Path) -> Self {
        Self {
            history: InputHistory::new(workspace),
        }
    }

    /// Read a line of input with interactive features.
    ///
    /// Returns `Some(line)` on Enter, `None` on Ctrl-D (EOF).
    /// Enables raw mode during input, restores on return.
    pub fn read_line(
        &mut self,
        prompt: &str,
        cmd_registry: &CommandRegistry,
    ) -> io::Result<Option<String>> {
        // Print prompt
        print!("{}", prompt);
        io::stdout().flush()?;

        terminal::enable_raw_mode()?;
        let result = self.read_line_raw(cmd_registry);
        terminal::disable_raw_mode()?;

        // Move to next line after input
        print!("\r\n");
        io::stdout().flush()?;

        result
    }

    fn read_line_raw(&mut self, cmd_registry: &CommandRegistry) -> io::Result<Option<String>> {
        let mut buffer = String::new();
        let mut cursor_pos: usize = 0;
        let mut completion: Option<CompletionState> = None;

        loop {
            if !event::poll(std::time::Duration::from_millis(100))? {
                continue;
            }

            let evt = event::read()?;
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = evt
            {
                match (code, modifiers) {
                    // Ctrl-C: clear line
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        buffer.clear();
                        // Redraw empty line
                        self.redraw_line("", 0, None)?;
                        return Ok(Some(String::new()));
                    }
                    // Ctrl-D on empty line: EOF
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        if buffer.is_empty() {
                            return Ok(None);
                        }
                    }
                    // Tab: accept completion
                    (KeyCode::Tab, _) => {
                        if let Some(ref comp) = completion {
                            buffer = comp.matches[comp.selected].clone();
                            // Add a space after the command
                            buffer.push(' ');
                            cursor_pos = buffer.len();
                            completion = None;
                            self.redraw_line(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Enter: submit
                    (KeyCode::Enter, _) => {
                        let line = buffer.trim().to_string();
                        if !line.is_empty() {
                            self.history.push(&line);
                        }
                        return Ok(Some(line));
                    }
                    // Escape: dismiss completion
                    (KeyCode::Esc, _) => {
                        if completion.is_some() {
                            completion = None;
                            self.redraw_line(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Up arrow: history or cycle completion
                    (KeyCode::Up, _) => {
                        if let Some(ref mut comp) = completion {
                            if comp.selected > 0 {
                                comp.selected -= 1;
                            }
                            let hint = Some(comp.matches[comp.selected].as_str());
                            self.redraw_line(&buffer, cursor_pos, hint)?;
                        } else if let Some(entry) = self.history.navigate_up(&buffer) {
                            buffer = entry.to_string();
                            cursor_pos = buffer.len();
                            self.redraw_line(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Down arrow: history or cycle completion
                    (KeyCode::Down, _) => {
                        if let Some(ref mut comp) = completion {
                            if comp.selected < comp.matches.len() - 1 {
                                comp.selected += 1;
                            }
                            let hint = Some(comp.matches[comp.selected].as_str());
                            self.redraw_line(&buffer, cursor_pos, hint)?;
                        } else if let Some(entry) = self.history.navigate_down() {
                            buffer = entry;
                            cursor_pos = buffer.len();
                            self.redraw_line(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Right arrow: accept completion inline, or move cursor
                    (KeyCode::Right, _) => {
                        if let Some(ref comp) = completion {
                            buffer = comp.matches[comp.selected].clone();
                            buffer.push(' ');
                            cursor_pos = buffer.len();
                            completion = None;
                            self.redraw_line(&buffer, cursor_pos, None)?;
                        } else if cursor_pos < buffer.len() {
                            cursor_pos += 1;
                            self.redraw_line(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Left arrow: dismiss completion, move cursor
                    (KeyCode::Left, _) => {
                        completion = None;
                        cursor_pos = cursor_pos.saturating_sub(1);
                        self.redraw_line(&buffer, cursor_pos, None)?;
                    }
                    // Backspace
                    (KeyCode::Backspace, _) => {
                        if cursor_pos > 0 {
                            buffer.remove(cursor_pos - 1);
                            cursor_pos -= 1;
                        }
                        // Update completion
                        completion = self.update_completion(&buffer, cmd_registry);
                        let hint = completion.as_ref().map(|c| c.matches[c.selected].as_str());
                        self.redraw_line(&buffer, cursor_pos, hint)?;
                    }
                    // Home
                    (KeyCode::Home, _) => {
                        cursor_pos = 0;
                        completion = None;
                        self.redraw_line(&buffer, cursor_pos, None)?;
                    }
                    // End
                    (KeyCode::End, _) => {
                        cursor_pos = buffer.len();
                        self.redraw_line(&buffer, cursor_pos, None)?;
                    }
                    // Regular character input
                    (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                        buffer.insert(cursor_pos, c);
                        cursor_pos += 1;
                        self.history.reset_navigation();

                        // Update completions for slash commands
                        completion = self.update_completion(&buffer, cmd_registry);
                        let hint = completion.as_ref().map(|c| c.matches[c.selected].as_str());
                        self.redraw_line(&buffer, cursor_pos, hint)?;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Update completion state based on current buffer content.
    fn update_completion(
        &self,
        buffer: &str,
        cmd_registry: &CommandRegistry,
    ) -> Option<CompletionState> {
        // Only complete if buffer starts with / and has no spaces yet (typing a command name)
        if !buffer.starts_with('/') || buffer.contains(' ') {
            return None;
        }
        let matches: Vec<String> = cmd_registry
            .completions(buffer)
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        if matches.is_empty() || (matches.len() == 1 && matches[0] == buffer) {
            return None;
        }
        Some(CompletionState {
            matches,
            selected: 0,
        })
    }

    /// Redraw the current input line (overwrites the current line).
    fn redraw_line(
        &self,
        buffer: &str,
        cursor_pos: usize,
        completion_hint: Option<&str>,
    ) -> io::Result<()> {
        let mut stdout = io::stdout();
        // Move to start of line and clear
        write!(stdout, "\r\x1b[2K")?;
        // Reprint prompt
        write!(stdout, "\x1b[1;34m> \x1b[0m")?;
        // Print buffer
        write!(stdout, "{}", buffer)?;
        // Print ghost completion text if available
        if let Some(hint) = completion_hint
            && hint.len() > buffer.len()
            && hint.starts_with(buffer)
        {
            let suffix = &hint[buffer.len()..];
            write!(stdout, "\x1b[90m{}\x1b[0m", suffix)?;
            // Move cursor back to actual position
            let hint_len = suffix.len();
            if hint_len > 0 {
                write!(stdout, "{}", cursor::MoveLeft(hint_len as u16))?;
            }
        }
        // Position cursor correctly within the buffer
        let chars_after_cursor = buffer.len() - cursor_pos;
        if chars_after_cursor > 0 {
            write!(stdout, "{}", cursor::MoveLeft(chars_after_cursor as u16))?;
        }
        stdout.flush()?;
        Ok(())
    }
}
