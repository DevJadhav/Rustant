//! Mixture-of-Experts (MoE) agentic architecture.
//!
//! Routes tasks to specialized expert agents, each with focused toolsets (5-12
//! domain tools + 8 shared tools), reducing per-request tool token overhead
//! from 25K-35K to 3K-7K while preserving full capability across all 195+ tools.
//!
//! # DeepSeek V3-Inspired Architecture
//!
//! ```text
//! User Task --> [SparseRouter] --> score 20 experts --> Top-K --> merge tools --> result
//!                   |                                    |
//!             Sigmoid scoring                    Mixed-precision schemas
//!             + bias adaptation                  (Full/Half/Quarter)
//!             + token budget cap                 8 shared always-on
//! ```
//!
//! The router uses sigmoid-style keyword affinity scoring (independent per expert,
//! not competitive softmax), selects Top-K (K=1-3) experts, merges their tool sets
//! with mixed-precision schemas, and applies token budget constraints.

pub mod context;
pub mod experts;
pub mod prompt_optimizer;
pub mod router;
pub mod warmup;

pub use experts::{ExpertConfig, ExpertId};
pub use prompt_optimizer::PromptOptimizer;
pub use router::{MoeConfig, MoeRouter, SparseRouteResult, ToolPrecision};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TaskClassification;

    #[test]
    fn test_expert_id_coverage() {
        // Every TaskClassification should map to an expert
        let classifications = vec![
            TaskClassification::General,
            TaskClassification::Calendar,
            TaskClassification::FileOperation,
            TaskClassification::GitOperation,
            TaskClassification::CodeAnalysis,
            TaskClassification::Browser,
            TaskClassification::ArxivResearch,
            TaskClassification::Messaging,
            TaskClassification::Music,
            TaskClassification::HomeKit,
            TaskClassification::Workflow("security_scan".to_string()),
            TaskClassification::DeepResearch,
        ];

        for cls in &classifications {
            let expert = ExpertId::from_classification(cls);
            assert!(
                !expert.tool_names().is_empty(),
                "Expert {expert:?} for classification {cls:?} should have tools",
            );
        }
    }

    #[test]
    fn test_all_experts_have_shared_tools() {
        let shared = ExpertId::shared_tools();
        for expert in ExpertId::all() {
            let tools = expert.tool_names();
            for shared_tool in &shared {
                assert!(
                    tools.contains(shared_tool),
                    "Expert {expert:?} missing shared tool '{shared_tool}'",
                );
            }
        }
    }

    #[test]
    fn test_moe_config_defaults() {
        let config = MoeConfig::default();
        assert!(config.enabled);
        assert_eq!(config.classification_cache_size, 256);
        assert_eq!(config.prune_after_iterations, 5);
        assert!(config.warm_on_startup);
        assert!(config.speculative_prefetch);
        assert!(config.compress_schemas);
        assert_eq!(config.max_experts_per_route, 3);
        assert!((config.activation_threshold - 0.15).abs() < f64::EPSILON);
        assert_eq!(config.max_tool_tokens, 6000);
    }

    #[test]
    fn test_expert_tool_counts() {
        // Each expert should have shared (8) + domain (1-12) tools
        for expert in ExpertId::all() {
            let domain_count = expert.domain_tools().len();
            let total_count = expert.tool_names().len();
            assert!(
                domain_count <= 12,
                "Expert {expert:?} has {domain_count} domain tools (max 12)",
            );
            assert!(
                total_count >= 8, // At least the 8 shared tools
                "Expert {expert:?} has only {total_count} total tools",
            );
            assert!(
                total_count <= 20, // 8 shared + 12 max domain
                "Expert {expert:?} has {total_count} tools (too many)",
            );
        }
    }

    #[test]
    fn test_20_experts() {
        assert_eq!(ExpertId::all().len(), 20);
    }
}
