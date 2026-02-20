//! Shared context management for MoE expert agents.
//!
//! Handles fact forwarding between the router and experts, and between experts
//! when cross-domain collaboration is needed (cascading expert pattern).

use crate::memory::Fact;
use std::collections::HashMap;

/// Shared context that the router maintains across expert invocations.
///
/// When a task spans multiple domains, the router uses this context to
/// forward relevant facts from one expert's output to the next expert's input.
#[derive(Debug, Clone, Default)]
pub struct MoeContext {
    /// Facts accumulated from expert executions, keyed by expert name.
    pub expert_facts: HashMap<String, Vec<Fact>>,
    /// Summary of prior expert results (for cross-expert context injection).
    pub prior_results: Vec<ExpertResult>,
}

/// Summary of an expert's execution for context forwarding.
#[derive(Debug, Clone)]
pub struct ExpertResult {
    /// Which expert produced this result.
    pub expert_name: String,
    /// Brief summary of what the expert accomplished.
    pub summary: String,
    /// Key facts extracted from the expert's output.
    pub facts: Vec<Fact>,
}

impl MoeContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an expert's result for potential cross-expert forwarding.
    pub fn record_result(&mut self, result: ExpertResult) {
        let name = result.expert_name.clone();
        let facts = result.facts.clone();
        self.prior_results.push(result);
        self.expert_facts.entry(name).or_default().extend(facts);
    }

    /// Get a context summary suitable for injection into a subsequent expert's
    /// system prompt (e.g., "Previous expert (Security) found: ...").
    pub fn context_summary(&self) -> String {
        if self.prior_results.is_empty() {
            return String::new();
        }

        let mut summary = String::from("\n[Context from prior expert analysis]\n");
        for result in &self.prior_results {
            summary.push_str(&format!(
                "- {} expert: {}\n",
                result.expert_name, result.summary
            ));
        }
        summary
    }

    /// Check if any prior results exist.
    pub fn has_prior_results(&self) -> bool {
        !self.prior_results.is_empty()
    }

    /// Clear all accumulated context.
    pub fn clear(&mut self) {
        self.expert_facts.clear();
        self.prior_results.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context() {
        let ctx = MoeContext::new();
        assert!(!ctx.has_prior_results());
        assert!(ctx.context_summary().is_empty());
    }

    #[test]
    fn test_record_and_summarize() {
        let mut ctx = MoeContext::new();
        ctx.record_result(ExpertResult {
            expert_name: "Security".into(),
            summary: "Found 3 high-severity vulnerabilities".into(),
            facts: vec![],
        });

        assert!(ctx.has_prior_results());
        let summary = ctx.context_summary();
        assert!(summary.contains("Security expert"));
        assert!(summary.contains("3 high-severity"));
    }

    #[test]
    fn test_clear() {
        let mut ctx = MoeContext::new();
        ctx.record_result(ExpertResult {
            expert_name: "System".into(),
            summary: "Completed file operations".into(),
            facts: vec![],
        });
        assert!(ctx.has_prior_results());
        ctx.clear();
        assert!(!ctx.has_prior_results());
    }
}
