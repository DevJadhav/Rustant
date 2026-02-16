//! Terminal event handling using crossterm EventStream.

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;

/// High-level actions the TUI can perform.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Action {
    Quit,
    Cancel,
    Interrupt,
    Submit(String),
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ScrollToBottom,
    ToggleVimMode,
    ToggleSidebar,
    CopyLastResponse,
    OpenCommandPalette,
    Approve,
    ApproveAllSimilar,
    Deny,
    ShowDiff,
    ShowHelp,
    // Raw event passthrough
    RawEvent(RawEventData),
}

/// Minimal representation of a raw event for cases that need passthrough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEventData {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

/// Reads terminal events asynchronously using crossterm's EventStream.
pub struct EventHandler {
    stream: EventStream,
}

impl EventHandler {
    pub fn new() -> Self {
        Self {
            stream: EventStream::new(),
        }
    }

    /// Read the next terminal event. Returns None if the stream ends.
    pub async fn next(&mut self) -> Option<Event> {
        self.stream.next().await.and_then(|r| r.ok())
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a key event to an Action, given the current app mode.
/// Returns None if the event should be passed to the input widget.
pub fn map_global_key(event: &KeyEvent) -> Option<Action> {
    match (event.modifiers, event.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(Action::Interrupt),
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => Some(Action::Quit),
        (KeyModifiers::CONTROL, KeyCode::Char('l')) => Some(Action::ScrollToBottom),
        _ => None,
    }
}

/// Map key events when in approval mode.
pub fn map_approval_key(event: &KeyEvent) -> Option<Action> {
    match event.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::Approve),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::ApproveAllSimilar),
        KeyCode::Char('n') | KeyCode::Char('N') => Some(Action::Deny),
        KeyCode::Char('d') | KeyCode::Char('D') => Some(Action::ShowDiff),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        KeyCode::Esc => Some(Action::Deny),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn test_ctrl_c_interrupts() {
        assert_eq!(
            map_global_key(&ctrl(KeyCode::Char('c'))),
            Some(Action::Interrupt)
        );
    }

    #[test]
    fn test_ctrl_d_quits() {
        assert_eq!(
            map_global_key(&ctrl(KeyCode::Char('d'))),
            Some(Action::Quit)
        );
    }

    #[test]
    fn test_ctrl_l_scrolls_to_bottom() {
        assert_eq!(
            map_global_key(&ctrl(KeyCode::Char('l'))),
            Some(Action::ScrollToBottom)
        );
    }

    #[test]
    fn test_regular_key_not_global() {
        assert_eq!(map_global_key(&key(KeyCode::Char('a'))), None);
    }

    #[test]
    fn test_approval_y_approves() {
        assert_eq!(
            map_approval_key(&key(KeyCode::Char('y'))),
            Some(Action::Approve)
        );
    }

    #[test]
    fn test_approval_n_denies() {
        assert_eq!(
            map_approval_key(&key(KeyCode::Char('n'))),
            Some(Action::Deny)
        );
    }

    #[test]
    fn test_approval_d_shows_diff() {
        assert_eq!(
            map_approval_key(&key(KeyCode::Char('d'))),
            Some(Action::ShowDiff)
        );
    }

    #[test]
    fn test_approval_esc_denies() {
        assert_eq!(map_approval_key(&key(KeyCode::Esc)), Some(Action::Deny));
    }

    #[test]
    fn test_approval_unknown_key() {
        assert_eq!(map_approval_key(&key(KeyCode::Char('x'))), None);
    }
}
