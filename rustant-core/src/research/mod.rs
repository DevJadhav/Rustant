//! Deep Research engine — multi-phase research pipeline.
//!
//! Orchestrates a 5-phase research process:
//! 1. **Decompose** — Break complex questions into sub-query DAG
//! 2. **Query** — Execute sub-queries in parallel using existing tools
//! 3. **Synthesize** — Merge results, detect contradictions
//! 4. **Verify** — Iterative refinement loop
//! 5. **Report** — Generate output in requested format
//!
//! Works with a single provider or LLM Council for synthesis.

pub mod contradiction;
pub mod decomposition;
pub mod engine;
pub mod output;
pub mod session;
pub mod sources;
pub mod synthesis;

pub use engine::ResearchEngine;
pub use session::{ResearchPhase, ResearchSession};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_research_module_exports() {
        // Verify module structure is intact
        let _phase = ResearchPhase::Decomposing;
    }
}
