//! Interactive REPL input with history and slash command autocomplete.
//!
//! Replaces raw `stdin.read_line()` with a crossterm-based input handler
//! that provides:
//! - Up/Down arrow history navigation with draft preservation
//! - `/` prefix slash command autocomplete with visible dropdown list
//! - Tab to accept selected completion, Esc to dismiss
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

/// Maximum number of completion items to show in the dropdown.
const MAX_VISIBLE_COMPLETIONS: usize = 8;

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
        let content: String = self.entries.iter().map(|e| format!("{e}\n")).collect();
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

/// A single completion entry with name and description.
struct CompletionEntry {
    name: String,
    description: String,
}

/// Completion state for slash commands.
struct CompletionState {
    entries: Vec<CompletionEntry>,
    selected: usize,
    /// Scroll offset for long lists.
    scroll: usize,
}

impl CompletionState {
    fn selected_name(&self) -> &str {
        &self.entries[self.selected].name
    }

    /// Compute visible window range for the dropdown.
    fn visible_range(&self) -> (usize, usize) {
        let total = self.entries.len();
        let max_vis = MAX_VISIBLE_COMPLETIONS.min(total);
        let start = if self.selected < self.scroll {
            self.selected
        } else if self.selected >= self.scroll + max_vis {
            self.selected + 1 - max_vis
        } else {
            self.scroll
        };
        (start, (start + max_vis).min(total))
    }
}

/// Interactive REPL input handler.
pub struct ReplInput {
    history: InputHistory,
    /// Number of completion lines currently displayed below the input.
    rendered_lines: usize,
}

impl ReplInput {
    /// Create a new interactive input handler.
    pub fn new(workspace: &Path) -> Self {
        Self {
            history: InputHistory::new(workspace),
            rendered_lines: 0,
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
        print!("{prompt}");
        io::stdout().flush()?;

        terminal::enable_raw_mode()?;
        let result = self.read_line_raw(cmd_registry);
        // Clean up completion lines before disabling raw mode
        self.clear_completion_lines()?;
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
                        self.clear_completion_lines()?;
                        self.redraw_input("", 0, None)?;
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
                            buffer = comp.selected_name().to_string();
                            buffer.push(' ');
                            cursor_pos = buffer.len();
                            completion = None;
                            self.clear_completion_lines()?;
                            self.redraw_input(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Enter: submit (or accept completion if active)
                    (KeyCode::Enter, _) => {
                        if let Some(ref comp) = completion {
                            buffer = comp.selected_name().to_string();
                            buffer.push(' ');
                            cursor_pos = buffer.len();
                            completion = None;
                            self.clear_completion_lines()?;
                            self.redraw_input(&buffer, cursor_pos, None)?;
                        } else {
                            let line = buffer.trim().to_string();
                            if !line.is_empty() {
                                self.history.push(&line);
                            }
                            return Ok(Some(line));
                        }
                    }
                    // Escape: dismiss completion
                    (KeyCode::Esc, _) => {
                        if completion.is_some() {
                            completion = None;
                            self.clear_completion_lines()?;
                            self.redraw_input(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Up arrow: cycle completion or history
                    (KeyCode::Up, _) => {
                        if let Some(ref mut comp) = completion {
                            if comp.selected > 0 {
                                comp.selected -= 1;
                                // Update scroll
                                let (start, _) = comp.visible_range();
                                comp.scroll = start;
                            }
                            self.render_with_completion(&buffer, cursor_pos, &completion)?;
                        } else if let Some(entry) = self.history.navigate_up(&buffer) {
                            buffer = entry.to_string();
                            cursor_pos = buffer.len();
                            self.redraw_input(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Down arrow: cycle completion or history
                    (KeyCode::Down, _) => {
                        if let Some(ref mut comp) = completion {
                            if comp.selected < comp.entries.len() - 1 {
                                comp.selected += 1;
                                let (start, _) = comp.visible_range();
                                comp.scroll = start;
                            }
                            self.render_with_completion(&buffer, cursor_pos, &completion)?;
                        } else if let Some(entry) = self.history.navigate_down() {
                            buffer = entry;
                            cursor_pos = buffer.len();
                            self.redraw_input(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Right arrow: accept completion inline, or move cursor
                    (KeyCode::Right, _) => {
                        if let Some(ref comp) = completion {
                            buffer = comp.selected_name().to_string();
                            buffer.push(' ');
                            cursor_pos = buffer.len();
                            completion = None;
                            self.clear_completion_lines()?;
                            self.redraw_input(&buffer, cursor_pos, None)?;
                        } else if cursor_pos < buffer.len() {
                            cursor_pos += 1;
                            self.redraw_input(&buffer, cursor_pos, None)?;
                        }
                    }
                    // Left arrow: dismiss completion, move cursor
                    (KeyCode::Left, _) => {
                        if completion.is_some() {
                            completion = None;
                            self.clear_completion_lines()?;
                        }
                        cursor_pos = cursor_pos.saturating_sub(1);
                        self.redraw_input(&buffer, cursor_pos, None)?;
                    }
                    // Backspace
                    (KeyCode::Backspace, _) => {
                        if cursor_pos > 0 {
                            buffer.remove(cursor_pos - 1);
                            cursor_pos -= 1;
                        }
                        completion = self.update_completion(&buffer, cmd_registry);
                        self.render_with_completion(&buffer, cursor_pos, &completion)?;
                    }
                    // Home
                    (KeyCode::Home, _) => {
                        cursor_pos = 0;
                        if completion.is_some() {
                            completion = None;
                            self.clear_completion_lines()?;
                        }
                        self.redraw_input(&buffer, cursor_pos, None)?;
                    }
                    // End
                    (KeyCode::End, _) => {
                        cursor_pos = buffer.len();
                        self.redraw_input(&buffer, cursor_pos, None)?;
                    }
                    // Regular character input
                    (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                        buffer.insert(cursor_pos, c);
                        cursor_pos += 1;
                        self.history.reset_navigation();

                        completion = self.update_completion(&buffer, cmd_registry);
                        self.render_with_completion(&buffer, cursor_pos, &completion)?;
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
        // Only complete if buffer starts with / and has no spaces yet
        if !buffer.starts_with('/') || buffer.contains(' ') {
            return None;
        }
        let matches = cmd_registry.completions_with_desc(buffer);
        if matches.is_empty() || (matches.len() == 1 && matches[0].0 == buffer) {
            return None;
        }
        let entries: Vec<CompletionEntry> = matches
            .into_iter()
            .map(|(name, desc)| CompletionEntry {
                name: name.to_string(),
                description: desc.to_string(),
            })
            .collect();
        Some(CompletionState {
            entries,
            selected: 0,
            scroll: 0,
        })
    }

    /// Render the input line plus the completion dropdown (if any).
    fn render_with_completion(
        &mut self,
        buffer: &str,
        cursor_pos: usize,
        completion: &Option<CompletionState>,
    ) -> io::Result<()> {
        // First clear any previously rendered completion lines
        self.clear_completion_lines()?;

        // Redraw the input line with ghost text from the selected completion
        let ghost = completion.as_ref().map(|c| c.selected_name());
        self.redraw_input(buffer, cursor_pos, ghost)?;

        // Render the dropdown list below
        if let Some(comp) = completion {
            self.render_dropdown(comp)?;
            // Move cursor back up to the input line
            if self.rendered_lines > 0 {
                let mut stdout = io::stdout();
                let lines_up = self.rendered_lines;
                let col = 2 + cursor_pos; // 2 = prompt "> " width
                write!(stdout, "\x1b[{lines_up}A")?;
                write!(stdout, "\r\x1b[{col}C")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    /// Render the completion dropdown list below the input line.
    fn render_dropdown(&mut self, comp: &CompletionState) -> io::Result<()> {
        let mut stdout = io::stdout();
        let (start, end) = comp.visible_range();
        let visible_count = end - start;

        // Get terminal width for truncation
        let term_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);

        for i in start..end {
            let entry = &comp.entries[i];
            let is_selected = i == comp.selected;

            // Format: "  /command  description"
            let name = &entry.name;
            let desc = &entry.description;

            // Truncate description to fit terminal width
            let prefix_len = 2 + name.len() + 2; // "  /cmd  "
            let max_desc = term_width.saturating_sub(prefix_len + 2);
            let desc_truncated = if desc.len() > max_desc {
                &desc[..max_desc]
            } else {
                desc
            };

            let padded_name = format!("{name:<20}");
            write!(stdout, "\r\n")?;
            if is_selected {
                // Highlighted: white on dark bg
                write!(
                    stdout,
                    "\x1b[2K\x1b[7m  {padded_name} {desc_truncated}\x1b[0m",
                )?;
            } else {
                write!(
                    stdout,
                    "\x1b[2K  \x1b[36m{padded_name}\x1b[0m \x1b[90m{desc_truncated}\x1b[0m",
                )?;
            }
        }

        // Show scroll indicator if there are more items
        let total = comp.entries.len();
        if total > visible_count {
            write!(stdout, "\r\n")?;
            write!(
                stdout,
                "\x1b[2K  \x1b[90m({visible_count}/{total} commands)\x1b[0m",
            )?;
            self.rendered_lines = visible_count + 1;
        } else {
            self.rendered_lines = visible_count;
        }

        stdout.flush()?;
        Ok(())
    }

    /// Clear previously rendered completion lines below the input.
    fn clear_completion_lines(&mut self) -> io::Result<()> {
        if self.rendered_lines > 0 {
            let mut stdout = io::stdout();
            // Save cursor position, go down and clear each line, restore
            for _ in 0..self.rendered_lines {
                write!(stdout, "\r\n\x1b[2K")?;
            }
            // Move back up
            write!(stdout, "\x1b[{}A", self.rendered_lines)?;
            stdout.flush()?;
            self.rendered_lines = 0;
        }
        Ok(())
    }

    /// Redraw just the input line (row 0) with optional ghost text.
    fn redraw_input(
        &self,
        buffer: &str,
        cursor_pos: usize,
        ghost_hint: Option<&str>,
    ) -> io::Result<()> {
        let mut stdout = io::stdout();
        // Move to start of line and clear
        write!(stdout, "\r\x1b[2K")?;
        // Reprint prompt
        write!(stdout, "\x1b[1;34m> \x1b[0m")?;
        // Print buffer
        write!(stdout, "{buffer}")?;
        // Print ghost completion text if available
        if let Some(hint) = ghost_hint
            && hint.len() > buffer.len()
            && hint.starts_with(buffer)
        {
            let suffix = &hint[buffer.len()..];
            write!(stdout, "\x1b[90m{suffix}\x1b[0m")?;
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
