//! # rustant-ml — AI/ML Engineering, Training, Evaluation & Research
//!
//! This crate provides the ML-specific modules for Rustant, covering data engineering,
//! model training, fine-tuning, RAG, evaluation, inference serving, and research tools.
//!
//! ## Four Foundational Pillars
//!
//! Every module integrates these cross-cutting concerns:
//! 1. **Safety** — PII scanning, content filtering, resource limits, data quality gates
//! 2. **Security** — Input sanitization, adversarial detection, model provenance
//! 3. **Transparency** — Audit trails, data lineage, source attribution
//! 4. **Interpretability** — Feature importance, reasoning traces, explanations

// Foundation
pub mod config;
pub mod error;
pub mod pillars;
pub mod runtime;

// Phase 1: Data Engineering
pub mod data;
pub mod features;

// Phase 2: Training Infrastructure
pub mod training;

// Phase 3: Model Zoo & Algorithms
pub mod algorithms;
pub mod zoo;

// Phase 4: LLM Fine-tuning
pub mod llm;

// Phase 5: RAG System
pub mod rag;

// Phase 6: Extended Evaluation
pub mod eval;

// Phase 7: Inference & Serving
pub mod inference;

// Phase 8: Research Tools
pub mod research;

// Phase 9: Pillar Modules
pub mod interpretability;
pub mod safety;
pub mod security;
pub mod transparency;

// Phase 10: Tool Wrappers
pub mod tools;

// Re-exports
pub use config::MlConfig;
pub use error::MlError;
pub use pillars::{Pillar, PillarEnforcement, PillarResult};
pub use runtime::PythonRuntime;

use rustant_tools::registry::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;

/// Register all ML tools with the tool registry.
///
/// This follows the same pattern as `rustant_security::register_security_tools()`.
/// Called from `rustant-cli/src/main.rs` when the `ai_engineer` feature flag is enabled.
pub fn register_ml_tools(registry: &mut ToolRegistry, workspace: PathBuf) {
    let ws = Arc::new(workspace);

    // Phase 1: Data Engineering tools (9 tools)
    tools::data_tools::register(registry, &ws);

    // Phase 1B: Feature Store tools (3 tools)
    tools::feature_tools::register(registry, &ws);

    // Phase 2: Training tools (5 tools)
    tools::training_tools::register(registry, &ws);

    // Phase 3: Model Zoo tools (5 tools)
    tools::zoo_tools::register(registry, &ws);

    // Phase 4: LLM Fine-tuning tools (5 tools)
    tools::llm_tools::register(registry, &ws);

    // Phase 5: RAG tools (5 tools)
    tools::rag_tools::register(registry, &ws);

    // Phase 6: Evaluation tools (4 tools)
    tools::eval_tools::register(registry, &ws);

    // Phase 7: Inference tools (4 tools)
    tools::inference_tools::register(registry, &ws);

    // Phase 8: Research tools (4 tools)
    tools::research_tools::register(registry, &ws);

    // Phase 9: Safety tools (4 tools)
    tools::safety_tools::register(registry, &ws);

    // Phase 9: Security tools (3 tools)
    tools::security_tools::register(registry, &ws);

    // Phase 9: Transparency tools (3 tools)
    tools::transparency_tools::register(registry, &ws);

    // Phase 9: Interpretability tools (3 tools)
    tools::interpretability_tools::register(registry, &ws);

    tracing::info!("Registered 54 ML tools");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_ml_tools() {
        let mut registry = ToolRegistry::new();
        let workspace = PathBuf::from("/tmp/test-workspace");
        register_ml_tools(&mut registry, workspace);

        // Verify all 54 tools are registered
        let tools = registry.list_names();
        let ml_tools: Vec<_> = tools
            .iter()
            .filter(|t| {
                t.starts_with("ml_")
                    || t.starts_with("rag_")
                    || t.starts_with("eval_")
                    || t.starts_with("inference_")
                    || t.starts_with("research_")
                    || t.starts_with("ai_")
            })
            .collect();
        assert!(
            ml_tools.len() >= 54,
            "Expected >= 54 ML tools, got {}",
            ml_tools.len()
        );
    }
}
