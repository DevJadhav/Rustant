//! Terminal markdown renderer — converts markdown to ANSI escape codes.
//!
//! Designed for streaming LLM output: buffers partial lines and renders
//! complete lines with ANSI formatting for bold, italic, inline code,
//! code blocks, headings, and bullet points.

/// ANSI escape codes for terminal formatting.
mod ansi {
    pub const BOLD_ON: &str = "\x1b[1m";
    pub const BOLD_OFF: &str = "\x1b[22m";
    pub const ITALIC_ON: &str = "\x1b[3m";
    pub const ITALIC_OFF: &str = "\x1b[23m";
    pub const DIM_ON: &str = "\x1b[2m";
    pub const DIM_OFF: &str = "\x1b[22m";
    pub const CYAN: &str = "\x1b[36m";
    pub const RESET: &str = "\x1b[0m";
    pub const UNDERLINE_ON: &str = "\x1b[4m";
}

/// A streaming-aware markdown renderer that buffers partial lines
/// and emits ANSI-formatted text when a line is complete.
pub struct TerminalMarkdownRenderer {
    /// Buffer for the current incomplete line.
    line_buffer: String,
    /// Whether we're currently inside a fenced code block.
    in_code_block: bool,
}

impl TerminalMarkdownRenderer {
    pub fn new() -> Self {
        Self {
            line_buffer: String::new(),
            in_code_block: false,
        }
    }

    /// Feed a streaming token into the renderer.
    ///
    /// Returns ANSI-formatted text that is ready to be printed.
    /// Buffers partial lines until a newline is encountered.
    pub fn feed(&mut self, token: &str) -> String {
        let mut output = String::new();

        for ch in token.chars() {
            if ch == '\n' {
                // Line complete — render it
                let rendered = self.render_line(&self.line_buffer.clone());
                output.push_str(&rendered);
                output.push('\n');
                self.line_buffer.clear();
            } else {
                self.line_buffer.push(ch);
            }
        }

        output
    }

    /// Flush any remaining buffered content.
    ///
    /// Call this when streaming is complete to emit the final partial line.
    pub fn flush(&mut self) -> String {
        if self.line_buffer.is_empty() {
            return String::new();
        }
        let rendered = self.render_line(&self.line_buffer.clone());
        self.line_buffer.clear();
        rendered
    }

    /// Reset the renderer state (e.g., between messages).
    pub fn reset(&mut self) {
        self.line_buffer.clear();
        self.in_code_block = false;
    }

    /// Render a single complete line with ANSI formatting.
    fn render_line(&mut self, line: &str) -> String {
        let trimmed = line.trim_start();

        // Toggle code fences
        if trimmed.starts_with("```") {
            self.in_code_block = !self.in_code_block;
            // Show the fence line dimmed
            return format!("{}{}{}", ansi::DIM_ON, line, ansi::DIM_OFF);
        }

        // Inside code block — render dimmed, no inline formatting
        if self.in_code_block {
            return format!("{}{}{}", ansi::DIM_ON, line, ansi::DIM_OFF);
        }

        // Headings (ATX style)
        if let Some(heading) = parse_heading(trimmed) {
            let indent = &line[..line.len() - trimmed.len()];
            return format!(
                "{}{}{}{}{}",
                indent,
                ansi::BOLD_ON,
                ansi::UNDERLINE_ON,
                heading,
                ansi::RESET
            );
        }

        // Blockquotes
        if let Some(rest) = trimmed.strip_prefix("> ") {
            let indent = &line[..line.len() - trimmed.len()];
            let rendered_rest = render_inline(rest);
            return format!(
                "{}{}│ {}{}",
                indent,
                ansi::DIM_ON,
                rendered_rest,
                ansi::RESET
            );
        }

        // Unordered list items: - or * at start
        if (trimmed.starts_with("- ") || trimmed.starts_with("* ")) && !trimmed.starts_with("**") {
            let indent = &line[..line.len() - trimmed.len()];
            let rest = &trimmed[2..];
            let rendered_rest = render_inline(rest);
            return format!("{indent}  \u{2022} {rendered_rest}");
        }

        // Horizontal rules
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            return format!("{}{}{}", ansi::DIM_ON, "\u{2500}".repeat(40), ansi::DIM_OFF);
        }

        // Regular line — apply inline formatting
        render_inline(line)
    }
}

/// Render a complete text (non-streaming) with markdown formatting.
pub fn render_markdown(text: &str) -> String {
    let mut renderer = TerminalMarkdownRenderer::new();
    let mut output = renderer.feed(text);
    output.push_str(&renderer.flush());
    output
}

/// Parse an ATX heading (# through ###), returning the heading text.
fn parse_heading(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("### ") {
        Some(rest)
    } else if let Some(rest) = line.strip_prefix("## ") {
        Some(rest)
    } else if let Some(rest) = line.strip_prefix("# ") {
        Some(rest)
    } else {
        None
    }
}

/// Apply inline markdown formatting to a single line.
///
/// Handles **bold**, *italic*, and `inline code`.
fn render_inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut output = String::with_capacity(len + 64);
    let mut i = 0;

    while i < len {
        // Bold: **...**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_closing_delimiter(&chars, i + 2, &['*', '*']) {
                output.push_str(ansi::BOLD_ON);
                // Recursively render inline content within bold
                let inner: String = chars[i + 2..end].iter().collect();
                output.push_str(&render_inline_simple(&inner));
                output.push_str(ansi::BOLD_OFF);
                i = end + 2;
                continue;
            }
        }

        // Italic: *...* (but not **)
        if chars[i] == '*' && i + 1 < len && chars[i + 1] != '*' && chars[i + 1] != ' ' {
            if let Some(end) = find_closing_single_star(&chars, i + 1) {
                output.push_str(ansi::ITALIC_ON);
                let inner: String = chars[i + 1..end].iter().collect();
                output.push_str(&inner);
                output.push_str(ansi::ITALIC_OFF);
                i = end + 1;
                continue;
            }
        }

        // Inline code: `...`
        if chars[i] == '`' {
            if let Some(end) = find_closing_backtick(&chars, i + 1) {
                output.push_str(ansi::CYAN);
                for ch in &chars[(i + 1)..end] {
                    output.push(*ch);
                }
                output.push_str(ansi::RESET);
                i = end + 1;
                continue;
            }
        }

        output.push(chars[i]);
        i += 1;
    }

    output
}

/// Simplified inline rendering (no recursion) for content already inside bold.
fn render_inline_simple(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut output = String::with_capacity(len + 32);
    let mut i = 0;

    while i < len {
        // Inline code inside bold
        if chars[i] == '`' {
            if let Some(end) = find_closing_backtick(&chars, i + 1) {
                output.push_str(ansi::CYAN);
                for ch in &chars[(i + 1)..end] {
                    output.push(*ch);
                }
                output.push_str(ansi::RESET);
                output.push_str(ansi::BOLD_ON); // re-enable bold after code
                i = end + 1;
                continue;
            }
        }

        output.push(chars[i]);
        i += 1;
    }

    output
}

/// Find the closing `**` starting from `start`.
/// Returns the index of the first `*` in the closing `**`, or None.
fn find_closing_delimiter(chars: &[char], start: usize, _delim: &[char]) -> Option<usize> {
    let len = chars.len();
    let mut i = start;
    while i + 1 < len {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find the closing single `*` for italic text.
/// Requires the closing `*` to not be preceded by a space.
fn find_closing_single_star(chars: &[char], start: usize) -> Option<usize> {
    let len = chars.len();
    let mut i = start;
    while i < len {
        if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
            // Closing star should not be preceded by space
            if i > start && chars[i - 1] != ' ' {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Find the closing backtick for inline code.
fn find_closing_backtick(chars: &[char], start: usize) -> Option<usize> {
    (start..chars.len()).find(|&i| chars[i] == '`')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bold_rendering() {
        let input = "The **Fibonacci sequence** is a series";
        let output = render_markdown(input);
        assert!(output.contains(ansi::BOLD_ON));
        assert!(output.contains("Fibonacci sequence"));
        assert!(output.contains(ansi::BOLD_OFF));
        assert!(!output.contains("**"));
    }

    #[test]
    fn test_italic_rendering() {
        let input = "This is *important* text";
        let output = render_markdown(input);
        assert!(output.contains(ansi::ITALIC_ON));
        assert!(output.contains("important"));
        assert!(output.contains(ansi::ITALIC_OFF));
    }

    #[test]
    fn test_inline_code_rendering() {
        let input = "Use the `println!` macro";
        let output = render_markdown(input);
        assert!(output.contains(ansi::CYAN));
        assert!(output.contains("println!"));
        assert!(output.contains(ansi::RESET));
    }

    #[test]
    fn test_heading_rendering() {
        let input = "# Main Title\n## Subtitle\n### Section\n";
        let output = render_markdown(input);
        assert!(output.contains(ansi::BOLD_ON));
        assert!(output.contains(ansi::UNDERLINE_ON));
        assert!(output.contains("Main Title"));
    }

    #[test]
    fn test_code_block_rendering() {
        let input = "```rust\nfn main() {}\n```\n";
        let output = render_markdown(input);
        assert!(output.contains(ansi::DIM_ON));
        assert!(output.contains("fn main()"));
    }

    #[test]
    fn test_bullet_points() {
        let input = "- First item\n- Second item\n";
        let output = render_markdown(input);
        assert!(output.contains("\u{2022}"));
        assert!(output.contains("First item"));
    }

    #[test]
    fn test_blockquote() {
        let input = "> This is a quote\n";
        let output = render_markdown(input);
        assert!(output.contains("\u{2502}"));
        assert!(output.contains("This is a quote"));
    }

    #[test]
    fn test_unmatched_bold_stays_literal() {
        let input = "This has ** unmatched stars";
        let output = render_markdown(input);
        assert!(output.contains("**"));
    }

    #[test]
    fn test_streaming_line_buffering() {
        let mut renderer = TerminalMarkdownRenderer::new();

        // Feed partial tokens
        let out1 = renderer.feed("The **Fib");
        assert!(out1.is_empty()); // no newline yet

        let out2 = renderer.feed("onacci** seq\n");
        assert!(out2.contains(ansi::BOLD_ON));
        assert!(out2.contains("Fibonacci"));
        assert!(!out2.contains("**"));
    }

    #[test]
    fn test_flush_partial_line() {
        let mut renderer = TerminalMarkdownRenderer::new();
        renderer.feed("Hello **world**");
        let flushed = renderer.flush();
        assert!(flushed.contains(ansi::BOLD_ON));
        assert!(flushed.contains("world"));
    }

    #[test]
    fn test_horizontal_rule() {
        let input = "---\n";
        let output = render_markdown(input);
        assert!(output.contains("\u{2500}"));
    }

    #[test]
    fn test_mixed_bold_and_code() {
        let input = "Use **`cargo build`** to compile\n";
        let output = render_markdown(input);
        assert!(output.contains(ansi::BOLD_ON));
        assert!(output.contains(ansi::CYAN));
        assert!(output.contains("cargo build"));
    }

    #[test]
    fn test_no_bold_in_code_block() {
        let input = "```\n**not bold**\n```\n";
        let output = render_markdown(input);
        // Inside code block, ** should not be rendered as bold
        assert!(output.contains("**not bold**"));
    }

    #[test]
    fn test_reset_clears_state() {
        let mut renderer = TerminalMarkdownRenderer::new();
        renderer.feed("```\n");
        assert!(renderer.in_code_block);
        renderer.reset();
        assert!(!renderer.in_code_block);
        assert!(renderer.line_buffer.is_empty());
    }
}
