//! @ file autocomplete widget using nucleo-matcher for fuzzy matching.

use crate::tui::theme::Theme;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};
use std::path::{Path, PathBuf};

/// Maximum number of files to scan.
const MAX_SCAN_FILES: usize = 5000;
/// Maximum suggestions to display.
const MAX_SUGGESTIONS: usize = 10;

/// A single autocomplete suggestion.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Suggestion {
    /// Display text (relative path).
    pub display: String,
    /// Full path for insertion.
    pub full_path: PathBuf,
    /// Fuzzy match score.
    pub score: u32,
}

/// Autocomplete state for @ file references.
pub struct AutocompleteState {
    /// Current query (text after @).
    query: String,
    /// Filtered suggestions.
    suggestions: Vec<Suggestion>,
    /// Selected index in the suggestions list.
    selected: usize,
    /// Whether autocomplete is active.
    active: bool,
    /// Cached file list for the workspace.
    file_cache: Vec<String>,
    /// Workspace root.
    workspace: PathBuf,
    /// nucleo matcher.
    matcher: Matcher,
}

#[allow(dead_code)]
impl AutocompleteState {
    /// Create a new autocomplete state for the given workspace.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            query: String::new(),
            suggestions: Vec::new(),
            selected: 0,
            active: false,
            file_cache: Vec::new(),
            workspace,
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Activate autocomplete and scan files if cache is empty.
    pub fn activate(&mut self, query: &str) {
        self.active = true;
        self.query = query.to_string();
        if self.file_cache.is_empty() {
            self.scan_files();
        }
        self.update_suggestions();
    }

    /// Deactivate autocomplete.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.suggestions.clear();
        self.selected = 0;
    }

    /// Whether autocomplete is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Update the query and refresh suggestions.
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.update_suggestions();
    }

    /// Get current suggestions.
    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }

    /// Get the currently selected suggestion.
    pub fn selected_suggestion(&self) -> Option<&Suggestion> {
        self.suggestions.get(self.selected)
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        if !self.suggestions.is_empty() && self.selected < self.suggestions.len() - 1 {
            self.selected += 1;
        }
    }

    /// Accept the currently selected suggestion. Returns the text to insert.
    pub fn accept(&mut self) -> Option<String> {
        let result = self.selected_suggestion().map(|s| s.display.clone());
        self.deactivate();
        result
    }

    /// Scan the workspace for files to cache.
    fn scan_files(&mut self) {
        self.file_cache.clear();
        let mut count = 0;
        scan_directory(
            &self.workspace,
            &self.workspace,
            &mut self.file_cache,
            &mut count,
        );
    }

    /// Invalidate the file cache (e.g., after file creation).
    pub fn invalidate_cache(&mut self) {
        self.file_cache.clear();
    }

    /// Update suggestions based on current query.
    fn update_suggestions(&mut self) {
        self.selected = 0;

        if self.query.is_empty() {
            // Show recent/all files when query is empty
            self.suggestions = self
                .file_cache
                .iter()
                .take(MAX_SUGGESTIONS)
                .map(|path| Suggestion {
                    display: path.clone(),
                    full_path: self.workspace.join(path),
                    score: 0,
                })
                .collect();
            return;
        }

        let pattern = Pattern::new(
            &self.query,
            CaseMatching::Smart,
            Normalization::Smart,
            nucleo_matcher::pattern::AtomKind::Fuzzy,
        );

        let mut scored: Vec<(u32, &String)> = self
            .file_cache
            .iter()
            .filter_map(|path| {
                let mut buf = Vec::new();
                let haystack = nucleo_matcher::Utf32Str::new(path, &mut buf);
                pattern
                    .score(haystack, &mut self.matcher)
                    .map(|score| (score, path))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));

        self.suggestions = scored
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .map(|(score, path)| Suggestion {
                display: path.clone(),
                full_path: self.workspace.join(path),
                score,
            })
            .collect();
    }

    /// Render the autocomplete popup.
    pub fn render(&self, frame: &mut Frame, anchor: Rect, theme: &Theme) {
        if !self.active || self.suggestions.is_empty() {
            return;
        }

        let height = (self.suggestions.len() as u16 + 2).min(MAX_SUGGESTIONS as u16 + 2);
        let width = 50.min(anchor.width);

        // Position popup above the input area
        let popup_y = anchor.y.saturating_sub(height);
        let popup_area = Rect::new(anchor.x + 1, popup_y, width, height);

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = self
            .suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let style = if i == self.selected {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(theme.fg)
                };
                ListItem::new(Line::from(vec![
                    Span::styled("  ", style),
                    Span::styled(&s.display, style),
                ]))
            })
            .collect();

        let block = Block::default()
            .title(" Files ")
            .borders(Borders::ALL)
            .border_style(theme.border_style())
            .style(Style::default().bg(theme.bg));

        let mut state = ListState::default();
        state.select(Some(self.selected));

        let list = List::new(items).block(block);
        frame.render_stateful_widget(list, popup_area, &mut state);
    }
}

/// Recursively scan a directory for files, respecting common ignores.
fn scan_directory(root: &Path, dir: &Path, files: &mut Vec<String>, count: &mut usize) {
    if *count >= MAX_SCAN_FILES {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if *count >= MAX_SCAN_FILES {
            return;
        }

        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        // Skip hidden files and common ignore patterns
        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "__pycache__"
            || name == ".git"
        {
            continue;
        }

        if path.is_dir() {
            scan_directory(root, &path, files, count);
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push(relative.to_string_lossy().to_string());
            *count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().to_path_buf();

        // Create some test files
        fs::create_dir_all(ws.join("src")).unwrap();
        fs::write(ws.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(ws.join("src/lib.rs"), "").unwrap();
        fs::write(ws.join("Cargo.toml"), "[package]").unwrap();
        fs::create_dir_all(ws.join("src/utils")).unwrap();
        fs::write(ws.join("src/utils/helpers.rs"), "").unwrap();

        (dir, ws)
    }

    #[test]
    fn test_autocomplete_new() {
        let ac = AutocompleteState::new(PathBuf::from("/tmp"));
        assert!(!ac.is_active());
        assert!(ac.suggestions().is_empty());
    }

    #[test]
    fn test_activate_scans_files() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        assert!(ac.is_active());
        assert!(!ac.file_cache.is_empty());
    }

    #[test]
    fn test_deactivate() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        ac.deactivate();
        assert!(!ac.is_active());
        assert!(ac.suggestions().is_empty());
    }

    #[test]
    fn test_fuzzy_match() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("main");
        assert!(
            ac.suggestions()
                .iter()
                .any(|s| s.display.contains("main.rs"))
        );
    }

    #[test]
    fn test_fuzzy_match_partial() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("hlp");
        assert!(
            ac.suggestions()
                .iter()
                .any(|s| s.display.contains("helpers"))
        );
    }

    #[test]
    fn test_empty_query_shows_files() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        assert!(!ac.suggestions().is_empty());
    }

    #[test]
    fn test_move_up_down() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        assert_eq!(ac.selected, 0);
        ac.move_down();
        assert_eq!(ac.selected, 1);
        ac.move_up();
        assert_eq!(ac.selected, 0);
        // Should not go below 0
        ac.move_up();
        assert_eq!(ac.selected, 0);
    }

    #[test]
    fn test_accept_returns_selection() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        let result = ac.accept();
        assert!(result.is_some());
        assert!(!ac.is_active());
    }

    #[test]
    fn test_invalidate_cache() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        assert!(!ac.file_cache.is_empty());
        ac.invalidate_cache();
        assert!(ac.file_cache.is_empty());
    }

    #[test]
    fn test_scan_skips_hidden_and_target() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().to_path_buf();
        fs::create_dir_all(ws.join(".git")).unwrap();
        fs::write(ws.join(".git/config"), "").unwrap();
        fs::create_dir_all(ws.join("target")).unwrap();
        fs::write(ws.join("target/debug"), "").unwrap();
        fs::write(ws.join("visible.rs"), "").unwrap();

        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        // Should only find visible.rs
        assert_eq!(ac.file_cache.len(), 1);
        assert!(ac.file_cache[0].contains("visible"));
    }

    #[test]
    fn test_update_query() {
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        let count_all = ac.suggestions().len();
        ac.update_query("main");
        let count_filtered = ac.suggestions().len();
        assert!(count_filtered <= count_all);
    }

    #[test]
    fn test_render_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let (_dir, ws) = setup_test_workspace();
        let mut ac = AutocompleteState::new(ws);
        ac.activate("");
        let theme = Theme::dark();
        terminal
            .draw(|frame| {
                let area = Rect::new(0, 20, 80, 4);
                ac.render(frame, area, &theme);
            })
            .unwrap();
    }
}
