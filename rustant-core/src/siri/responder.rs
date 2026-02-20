//! Siri response formatting for voice output.
//!
//! Formats agent output for natural voice delivery: strips markdown,
//! truncates to speech duration limit, adds pauses, and generates
//! confirmation prompts for destructive actions.

/// Formats agent responses for Siri voice output.
pub struct SiriResponder {
    /// Maximum speech duration in seconds.
    max_duration_secs: u32,
    /// macOS voice name (optional).
    voice: Option<String>,
}

impl SiriResponder {
    /// Create a new responder with the given settings.
    pub fn new(max_duration_secs: u32, voice: Option<String>) -> Self {
        Self {
            max_duration_secs,
            voice,
        }
    }

    /// Format agent output for voice delivery.
    ///
    /// Strips markdown, truncates to fit within max speech duration,
    /// and adds natural pauses.
    pub fn format_for_speech(&self, text: &str) -> String {
        let mut cleaned = Self::strip_markdown(text);

        // Estimate words per minute for speech (average ~150 WPM)
        let max_words = (self.max_duration_secs as usize * 150) / 60;
        let words: Vec<&str> = cleaned.split_whitespace().collect();

        if words.len() > max_words {
            cleaned = words[..max_words].join(" ");
            cleaned.push_str(". That's the summary. Ask me for more details.");
        }

        cleaned
    }

    /// Generate a confirmation prompt for a destructive action.
    pub fn confirmation_prompt(&self, action: &str, details: &str) -> String {
        format!("I need your approval. {action}. {details}. Should I proceed? Say yes or no.")
    }

    /// Strip markdown formatting from text.
    fn strip_markdown(text: &str) -> String {
        let mut result = String::new();

        for line in text.lines() {
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                if !result.is_empty() && !result.ends_with(". ") {
                    result.push_str(". ");
                }
                continue;
            }

            // Strip headers
            let content = if trimmed.starts_with('#') {
                trimmed.trim_start_matches('#').trim()
            } else {
                trimmed
            };

            // Strip bold/italic markers
            let content = content
                .replace("**", "")
                .replace("__", "")
                .replace(['*', '_'], "");

            // Strip inline code
            let content = content.replace('`', "");

            // Strip bullet points
            let content = if content.starts_with("- ") || content.starts_with("• ") {
                &content[2..]
            } else {
                &content
            };

            // Strip numbered lists
            let content = if content.len() > 2
                && content
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                && content.chars().nth(1) == Some('.')
            {
                content[2..].trim()
            } else {
                content
            };

            if !content.is_empty() {
                if !result.is_empty() && !result.ends_with(". ") && !result.ends_with(' ') {
                    result.push(' ');
                }
                result.push_str(content);
            }
        }

        result
    }

    /// Get the configured voice name.
    pub fn voice(&self) -> Option<&str> {
        self.voice.as_deref()
    }
}

impl Default for SiriResponder {
    fn default() -> Self {
        Self::new(30, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markdown() {
        let text = "# Hello World\n\n**Bold text** and *italic*.\n\n- Bullet one\n- Bullet two";
        let cleaned = SiriResponder::strip_markdown(text);
        assert!(!cleaned.contains('#'));
        assert!(!cleaned.contains('*'));
        assert!(cleaned.contains("Hello World"));
        assert!(cleaned.contains("Bold text"));
    }

    #[test]
    fn test_format_for_speech_truncation() {
        let responder = SiriResponder::new(5, None); // 5 seconds = ~12 words
        let long_text = "This is a very long text with many words that should be truncated because it exceeds the maximum speech duration that we have configured for our test scenario here today and it just keeps going and going.";
        let formatted = responder.format_for_speech(long_text);
        assert!(formatted.len() < long_text.len());
        assert!(formatted.contains("summary"));
    }

    #[test]
    fn test_confirmation_prompt() {
        let responder = SiriResponder::default();
        let prompt =
            responder.confirmation_prompt("This will delete 3 files", "Including config.toml");
        assert!(prompt.contains("delete 3 files"));
        assert!(prompt.contains("yes or no"));
    }

    #[test]
    fn test_short_text_no_truncation() {
        let responder = SiriResponder::new(30, None);
        let text = "Your next meeting is at 3 PM.";
        let formatted = responder.format_for_speech(text);
        assert_eq!(formatted, text);
    }
}
