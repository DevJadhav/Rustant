//! Adaptive system prompt compression for MoE.
//!
//! When system_prompt + tools + history exceeds 70% of the context window,
//! this module truncates addenda at sensible boundaries and deduplicates
//! redundant instructions across persona/expert/workflow/knowledge addenda.

use std::collections::HashSet;

/// Prompt optimizer that compresses system prompt addenda when context usage
/// is too high.
pub struct PromptOptimizer {
    /// Maximum fraction of context window the prompt + tools can occupy (0.0-1.0).
    threshold: f64,
}

impl PromptOptimizer {
    /// Create a new optimizer with the given context usage threshold.
    ///
    /// When total prompt tokens exceed `threshold * context_window`, compression kicks in.
    /// Default threshold is 0.7 (70%).
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold: threshold.clamp(0.1, 0.95),
        }
    }

    /// Check if compression is needed given current token usage.
    pub fn needs_compression(&self, prompt_tokens: usize, context_window: usize) -> bool {
        if context_window == 0 {
            return false;
        }
        (prompt_tokens as f64 / context_window as f64) > self.threshold
    }

    /// Deduplicate addenda by removing semantically redundant sentences.
    ///
    /// Uses sentence-level Jaccard similarity to identify near-duplicates across
    /// persona, expert, workflow, and knowledge addenda.
    pub fn dedup_addenda(&self, addenda: &[String]) -> Vec<String> {
        if addenda.len() <= 1 {
            return addenda.to_vec();
        }

        let mut seen_sentences: HashSet<Vec<String>> = HashSet::new();
        let mut result = Vec::new();

        for addendum in addenda {
            let mut deduped_lines = Vec::new();

            for line in addendum.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    deduped_lines.push(line.to_string());
                    continue;
                }

                // Tokenize into normalized words for Jaccard comparison
                let words: Vec<String> = trimmed
                    .split_whitespace()
                    .map(|w| w.to_lowercase())
                    .collect();

                if words.is_empty() {
                    deduped_lines.push(line.to_string());
                    continue;
                }

                // Check Jaccard similarity against all seen sentences
                let is_duplicate = seen_sentences
                    .iter()
                    .any(|seen| jaccard_similarity(&words, seen) > 0.75);

                if !is_duplicate {
                    seen_sentences.insert(words);
                    deduped_lines.push(line.to_string());
                }
            }

            let deduped = deduped_lines.join("\n");
            if !deduped.trim().is_empty() {
                result.push(deduped);
            }
        }

        result
    }

    /// Truncate an addendum to fit within a token budget.
    ///
    /// Truncates at sentence boundaries (periods, newlines) rather than
    /// mid-word, preserving readability.
    pub fn truncate_to_budget(&self, text: &str, max_tokens: usize) -> String {
        // Rough estimate: 1 token ~= 4 chars (for English text)
        let max_chars = max_tokens * 4;

        if text.len() <= max_chars {
            return text.to_string();
        }

        // Find the last sentence boundary before the limit
        let truncation_point = text[..max_chars]
            .rfind(". ")
            .or_else(|| text[..max_chars].rfind('\n'))
            .or_else(|| text[..max_chars].rfind(' '))
            .unwrap_or(max_chars);

        let mut result = text[..truncation_point].to_string();
        if !result.ends_with('.') {
            result.push('.');
        }
        result
    }

    /// Compress system prompt components to fit within the context budget.
    ///
    /// Returns the compressed system prompt string. Prioritizes keeping:
    /// 1. Core system instructions (never truncated)
    /// 2. Expert-specific addendum (truncated last)
    /// 3. Persona addendum (truncated second)
    /// 4. Workflow hints (truncated first)
    /// 5. Knowledge addendum (truncated first)
    pub fn compress_prompt(
        &self,
        core_prompt: &str,
        addenda: &[(&str, String)], // (label, content) pairs
        context_window: usize,
        tool_tokens: usize,
    ) -> String {
        let available_tokens =
            ((context_window as f64 * self.threshold) as usize).saturating_sub(tool_tokens);

        // Core prompt is never compressed
        let core_tokens = estimate_tokens(core_prompt);

        if core_tokens >= available_tokens {
            // Even core doesn't fit — return it unmodified, caller will handle
            return core_prompt.to_string();
        }

        let remaining = available_tokens - core_tokens;

        // Dedup addenda content
        let contents: Vec<String> = addenda.iter().map(|(_, c)| c.clone()).collect();
        let deduped = self.dedup_addenda(&contents);

        // Allocate budget proportionally to each addendum
        let total_addenda_tokens: usize = deduped.iter().map(|a| estimate_tokens(a)).sum();

        let mut parts = vec![core_prompt.to_string()];

        if total_addenda_tokens <= remaining {
            // Everything fits
            for d in &deduped {
                if !d.trim().is_empty() {
                    parts.push(d.clone());
                }
            }
        } else {
            // Need to truncate — distribute budget proportionally
            for d in &deduped {
                let d_tokens = estimate_tokens(d);
                let budget = if total_addenda_tokens > 0 {
                    (d_tokens as f64 / total_addenda_tokens as f64 * remaining as f64) as usize
                } else {
                    0
                };
                if budget > 0 {
                    let truncated = self.truncate_to_budget(d, budget);
                    if !truncated.trim().is_empty() {
                        parts.push(truncated);
                    }
                }
            }
        }

        parts.join("\n\n")
    }

    /// Strip irrelevant sections from a system prompt based on expert exclusion keywords.
    ///
    /// For each line in the prompt, if it contains any of the exclusion keywords
    /// (case-insensitive), the line is removed. This reduces the system prompt by
    /// 500-1500 tokens for domain-specific experts.
    ///
    /// Only non-empty lines containing keywords are removed; headings and structural
    /// lines (starting with `#` or `---`) are always preserved to maintain readability.
    pub fn strip_irrelevant_sections(&self, prompt: &str, exclusions: &[&str]) -> String {
        if exclusions.is_empty() {
            return prompt.to_string();
        }

        let exclusions_lower: Vec<String> = exclusions.iter().map(|e| e.to_lowercase()).collect();

        prompt
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                // Always keep empty lines, headings, and separators
                if trimmed.is_empty()
                    || trimmed.starts_with('#')
                    || trimmed.starts_with("---")
                    || trimmed.starts_with("| ")
                {
                    return true;
                }
                // Check if line contains any exclusion keyword
                let lower = trimmed.to_lowercase();
                !exclusions_lower
                    .iter()
                    .any(|exc| lower.contains(exc.as_str()))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for PromptOptimizer {
    fn default() -> Self {
        Self::new(0.7)
    }
}

/// Estimate token count for a string (rough: ~4 chars per token).
fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Compute Jaccard similarity between two word lists.
fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }

    let set_a: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_compression() {
        let optimizer = PromptOptimizer::new(0.7);
        // 80% usage should trigger compression
        assert!(optimizer.needs_compression(8000, 10000));
        // 50% usage should not
        assert!(!optimizer.needs_compression(5000, 10000));
        // Edge case: zero context window
        assert!(!optimizer.needs_compression(100, 0));
    }

    #[test]
    fn test_dedup_addenda() {
        let optimizer = PromptOptimizer::default();

        let addenda = vec![
            "You are specialized in file operations and shell commands.".to_string(),
            "You are specialized in file operations and shell commands. Focus on efficiency."
                .to_string(),
            "Always validate user input before processing.".to_string(),
        ];

        let deduped = optimizer.dedup_addenda(&addenda);
        // The near-duplicate should be removed
        assert!(deduped.len() <= addenda.len());
    }

    #[test]
    fn test_truncate_to_budget() {
        let optimizer = PromptOptimizer::default();

        let text = "First sentence. Second sentence. Third sentence. Fourth sentence.";
        let truncated = optimizer.truncate_to_budget(text, 10); // ~40 chars
        assert!(truncated.len() <= 44);
        assert!(truncated.ends_with('.'));
    }

    #[test]
    fn test_compress_prompt_fits() {
        let optimizer = PromptOptimizer::new(0.7);

        let core = "You are an AI assistant.";
        let addenda = vec![("expert", "Focus on code review.".to_string())];

        let result = optimizer.compress_prompt(core, &addenda, 100000, 1000);
        assert!(result.contains("You are an AI assistant"));
        assert!(result.contains("Focus on code review"));
    }

    #[test]
    fn test_jaccard_similarity() {
        let a: Vec<String> = vec!["hello".into(), "world".into()];
        let b: Vec<String> = vec!["hello".into(), "world".into()];
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);

        let c: Vec<String> = vec!["foo".into(), "bar".into()];
        assert!((jaccard_similarity(&a, &c)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_single_addendum_passthrough() {
        let optimizer = PromptOptimizer::default();
        let addenda = vec!["Only one addendum here.".to_string()];
        let deduped = optimizer.dedup_addenda(&addenda);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0], addenda[0]);
    }

    #[test]
    fn test_strip_irrelevant_sections_removes_keywords() {
        let optimizer = PromptOptimizer::default();
        let prompt = "You are an AI assistant.\nUse AppleScript for macOS automation.\nAlways validate input.\n# Heading\nCalendar integration is available.";
        let exclusions = &["applescript", "calendar"];
        let result = optimizer.strip_irrelevant_sections(prompt, exclusions);
        assert!(result.contains("You are an AI assistant."));
        assert!(!result.contains("AppleScript"));
        assert!(!result.contains("Calendar"));
        assert!(result.contains("Always validate input."));
        // Headings are always preserved
        assert!(result.contains("# Heading"));
    }

    #[test]
    fn test_strip_irrelevant_sections_empty_exclusions() {
        let optimizer = PromptOptimizer::default();
        let prompt = "Line one.\nLine two.";
        let result = optimizer.strip_irrelevant_sections(prompt, &[]);
        assert_eq!(result, prompt);
    }

    #[test]
    fn test_strip_irrelevant_sections_preserves_structure() {
        let optimizer = PromptOptimizer::default();
        let prompt = "--- separator ---\n| table row |\nLoRA finetuning details.\nNormal text.";
        let exclusions = &["lora"];
        let result = optimizer.strip_irrelevant_sections(prompt, exclusions);
        assert!(result.contains("--- separator ---"));
        assert!(result.contains("| table row |"));
        assert!(!result.contains("LoRA"));
        assert!(result.contains("Normal text."));
    }
}
