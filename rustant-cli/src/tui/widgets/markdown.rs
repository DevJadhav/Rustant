//! Markdown renderer that converts text into styled ratatui Lines.
//!
//! Uses syntect for code block syntax highlighting and manual parsing
//! for inline formatting.

use crate::tui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// Shared syntax highlighting resources (loaded once, reused).
pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl SyntaxHighlighter {
    /// Load default syntax definitions and themes.
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Highlight a code block with the given language hint.
    pub fn highlight_code(&self, code: &str, lang: &str, theme_name: &str) -> Vec<Line<'static>> {
        let theme = self.theme_set.themes.get(theme_name).unwrap_or_else(|| {
            self.theme_set
                .themes
                .values()
                .next()
                .expect("at least one theme")
        });

        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut lines = Vec::new();

        for line in code.lines() {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            let spans: Vec<Span<'static>> = ranges
                .iter()
                .map(|(style, text)| {
                    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();

            lines.push(Line::from(spans));
        }

        lines
    }
}

/// Render markdown text into styled ratatui Lines.
///
/// Supports:
/// - `**bold**`
/// - `*italic*`
/// - `` `inline code` ``
/// - ``` ```lang ... ``` ``` code blocks (with syntax highlighting)
/// - `# headings`
/// - Plain text
pub fn render_markdown<'a>(
    text: &str,
    theme: &Theme,
    highlighter: &SyntaxHighlighter,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buffer = String::new();

    for raw_line in text.lines() {
        // Handle fenced code blocks
        if raw_line.starts_with("```") {
            if in_code_block {
                // End of code block: highlight and add
                let highlighted =
                    highlighter.highlight_code(&code_buffer, &code_lang, &theme.syntect_theme);
                lines.extend(highlighted);
                code_buffer.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                // Start of code block
                code_lang = raw_line.trim_start_matches('`').trim().to_string();
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            if !code_buffer.is_empty() {
                code_buffer.push('\n');
            }
            code_buffer.push_str(raw_line);
            continue;
        }

        // Headings
        if let Some(heading) = raw_line.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                heading.to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(heading) = raw_line.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                heading.to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }
        if let Some(heading) = raw_line.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                heading.to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }

        // Inline formatting
        lines.push(render_inline_markdown(raw_line, theme));
    }

    // Handle unclosed code block
    if in_code_block && !code_buffer.is_empty() {
        let highlighted =
            highlighter.highlight_code(&code_buffer, &code_lang, &theme.syntect_theme);
        lines.extend(highlighted);
    }

    lines
}

/// Render inline markdown formatting for a single line.
fn render_inline_markdown<'a>(text: &str, theme: &Theme) -> Line<'a> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current = String::new();

    let base_style = Style::default().fg(theme.fg);
    let bold_style = base_style.add_modifier(Modifier::BOLD);
    let italic_style = base_style.add_modifier(Modifier::ITALIC);
    let code_style = Style::default()
        .fg(theme.tool_call_fg)
        .add_modifier(Modifier::BOLD);

    while let Some(ch) = chars.next() {
        match ch {
            '`' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), base_style));
                    current.clear();
                }
                let mut code = String::new();
                for c in chars.by_ref() {
                    if c == '`' {
                        break;
                    }
                    code.push(c);
                }
                spans.push(Span::styled(code, code_style));
            }
            '*' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), base_style));
                    current.clear();
                }
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                    let mut bold_text = String::new();
                    while let Some(c) = chars.next() {
                        if c == '*' && chars.peek() == Some(&'*') {
                            chars.next();
                            break;
                        }
                        bold_text.push(c);
                    }
                    spans.push(Span::styled(bold_text, bold_style));
                } else {
                    let mut italic_text = String::new();
                    for c in chars.by_ref() {
                        if c == '*' {
                            break;
                        }
                        italic_text.push(c);
                    }
                    spans.push(Span::styled(italic_text, italic_style));
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, base_style));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        Theme::dark()
    }

    fn test_highlighter() -> SyntaxHighlighter {
        SyntaxHighlighter::new()
    }

    #[test]
    fn test_plain_text() {
        let theme = test_theme();
        let hl = test_highlighter();
        let lines = render_markdown("Hello world", &theme, &hl);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_heading() {
        let theme = test_theme();
        let hl = test_highlighter();
        let lines = render_markdown("# Title\n## Subtitle\n### Section", &theme, &hl);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_code_block() {
        let theme = test_theme();
        let hl = test_highlighter();
        let md = "```rust\nfn main() {}\n```";
        let lines = render_markdown(md, &theme, &hl);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_inline_code() {
        let theme = test_theme();
        let hl = test_highlighter();
        let lines = render_markdown("Use `foo()` here", &theme, &hl);
        assert_eq!(lines.len(), 1);
        // Should have multiple spans (text, code, text)
        assert!(lines[0].spans.len() >= 2);
    }

    #[test]
    fn test_bold_text() {
        let theme = test_theme();
        let hl = test_highlighter();
        let lines = render_markdown("This is **bold** text", &theme, &hl);
        assert_eq!(lines.len(), 1);
        // Check that at least one span has BOLD modifier
        let has_bold = lines[0]
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(has_bold);
    }

    #[test]
    fn test_italic_text() {
        let theme = test_theme();
        let hl = test_highlighter();
        let lines = render_markdown("This is *italic* text", &theme, &hl);
        assert_eq!(lines.len(), 1);
        let has_italic = lines[0]
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::ITALIC));
        assert!(has_italic);
    }

    #[test]
    fn test_multiline_text() {
        let theme = test_theme();
        let hl = test_highlighter();
        let lines = render_markdown("line1\nline2\nline3", &theme, &hl);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_highlight_code_unknown_lang() {
        let hl = test_highlighter();
        let lines = hl.highlight_code("some code", "unknownlang123", "base16-ocean.dark");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_highlight_code_rust() {
        let hl = test_highlighter();
        let lines = hl.highlight_code(
            "fn main() {\n    println!(\"hi\");\n}",
            "rs",
            "base16-ocean.dark",
        );
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_unclosed_code_block() {
        let theme = test_theme();
        let hl = test_highlighter();
        let md = "```rust\nfn main() {}";
        let lines = render_markdown(md, &theme, &hl);
        assert!(!lines.is_empty());
    }
}
