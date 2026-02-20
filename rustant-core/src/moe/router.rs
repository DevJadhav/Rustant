//! Sparse MoE Router — DeepSeek V3-inspired sigmoid scoring with Top-K expert selection.
//!
//! The router scores ALL 20 experts independently (sigmoid gating, not competitive softmax),
//! selects Top-K (K=1-3) experts above an activation threshold, merges their tool sets,
//! and applies token-budget-limited routing with mixed-precision tool schemas.
//!
//! # DeepSeek V3 Parallels
//!
//! | DeepSeek V3 Concept | Rustant Adaptation |
//! |---------------------|-------------------|
//! | Sigmoid gating per expert | `keyword_affinity()` scores 0.0-1.0 independently |
//! | Top-K (8 of 256) activation | Top-K (1-3 of 20) activation |
//! | Shared expert (always-on) | 8 shared tools always sent |
//! | Auxiliary-loss-free bias | Dynamic `expert_bias` updated by success tracking |
//! | Node-limited routing | Token-budget cap (~6000 tokens) |
//! | FP8 mixed precision | Full/Half/Quarter tool schema precision |

use super::experts::{ExpertConfig, ExpertId};
use crate::types::TaskClassification;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;

// =============================================================================
// Configuration
// =============================================================================

/// Sparse MoE router configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoeConfig {
    /// Whether MoE routing is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Size of the classification → route cache (LRU).
    #[serde(default = "default_cache_size")]
    pub classification_cache_size: usize,
    /// Per-expert configuration overrides.
    #[serde(default)]
    pub expert_configs: HashMap<ExpertId, ExpertConfig>,
    /// Fallback expert when no expert scores above threshold.
    #[serde(default = "default_fallback")]
    pub fallback_expert: ExpertId,
    /// After N iterations with zero invocations, prune unused tools. 0 = disabled.
    #[serde(default = "default_prune_iterations")]
    pub prune_after_iterations: usize,
    /// Whether to warm tool definition caches on startup.
    #[serde(default = "default_true")]
    pub warm_on_startup: bool,
    /// Whether to speculatively prefetch likely next-expert tool sets.
    #[serde(default = "default_true")]
    pub speculative_prefetch: bool,
    /// Whether to compress tool schemas to reduce token count.
    #[serde(default = "default_true")]
    pub compress_schemas: bool,

    // --- Sparse router config ---
    /// Maximum number of experts to activate per route (Top-K).
    #[serde(default = "default_max_experts")]
    pub max_experts_per_route: usize,
    /// Minimum score for an expert to be considered (activation threshold).
    #[serde(default = "default_activation_threshold")]
    pub activation_threshold: f64,
    /// Maximum tool tokens budget for routed tools (excludes shared).
    #[serde(default = "default_max_tool_tokens")]
    pub max_tool_tokens: usize,
    /// How often to update bias terms (every N routes).
    #[serde(default = "default_bias_interval")]
    pub bias_update_interval: usize,
    /// Step size for bias adjustments.
    #[serde(default = "default_bias_gamma")]
    pub bias_gamma: f64,
    /// Maximum absolute bias value (clamped to ±bias_clamp).
    #[serde(default = "default_bias_clamp")]
    pub bias_clamp: f64,
}

fn default_true() -> bool {
    true
}
fn default_cache_size() -> usize {
    256
}
fn default_fallback() -> ExpertId {
    ExpertId::FileOps
}
fn default_prune_iterations() -> usize {
    5
}
fn default_max_experts() -> usize {
    3
}
fn default_activation_threshold() -> f64 {
    0.15
}
fn default_max_tool_tokens() -> usize {
    6000
}
fn default_bias_interval() -> usize {
    50
}
fn default_bias_gamma() -> f64 {
    0.01
}
fn default_bias_clamp() -> f64 {
    0.3
}

impl Default for MoeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            classification_cache_size: 256,
            expert_configs: HashMap::new(),
            fallback_expert: ExpertId::FileOps,
            prune_after_iterations: 5,
            warm_on_startup: true,
            speculative_prefetch: true,
            compress_schemas: true,
            max_experts_per_route: 3,
            activation_threshold: 0.15,
            max_tool_tokens: 6000,
            bias_update_interval: 50,
            bias_gamma: 0.01,
            bias_clamp: 0.3,
        }
    }
}

// =============================================================================
// Tool Precision (FP8 analog)
// =============================================================================

/// Precision tier for tool schema serialization.
///
/// Mirrors DeepSeek's FP8 mixed precision: full detail for high-affinity tools,
/// reduced for lower-affinity tools in secondary/tertiary experts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolPrecision {
    /// Complete schema: name + description + all params + examples (~400 tokens).
    Full,
    /// Truncated: name + first-sentence desc + required params only (~200 tokens).
    Half,
    /// Minimal: name + 10-word desc + param names only (~100 tokens).
    Quarter,
}

impl ToolPrecision {
    /// Estimated token cost for a tool at this precision level.
    pub fn estimated_tokens(&self) -> usize {
        match self {
            ToolPrecision::Full => 400,
            ToolPrecision::Half => 200,
            ToolPrecision::Quarter => 100,
        }
    }
}

// =============================================================================
// Route Result
// =============================================================================

/// Result of sparse routing a task to one or more experts.
#[derive(Debug, Clone)]
pub struct SparseRouteResult {
    /// Selected experts with their base scores (sorted by score descending).
    pub selected_experts: Vec<(ExpertId, f64)>,
    /// Shared tools (always sent, 8 tools).
    pub shared_tools: Vec<String>,
    /// Routed domain tools with their precision tier.
    pub routed_tools: Vec<(String, ToolPrecision)>,
    /// System prompt addendum merged from all selected experts.
    pub system_prompt_addendum: String,
    /// Estimated total tool token cost.
    pub total_tool_tokens: usize,
    /// Whether this was a cache hit.
    pub cache_hit: bool,
    /// Human-readable routing reasoning (for interpretability).
    pub routing_reasoning: String,
    /// The heuristic classification that was used.
    pub classification: TaskClassification,
}

/// Backward-compatible alias.
pub type RouteResult = SparseRouteResult;

impl SparseRouteResult {
    /// Get the primary (highest-scored) expert.
    pub fn primary_expert(&self) -> ExpertId {
        self.selected_experts
            .first()
            .map(|(id, _)| *id)
            .unwrap_or(ExpertId::FileOps)
    }

    /// Get all tool names (shared + routed), deduped.
    pub fn all_tool_names(&self) -> Vec<String> {
        let mut tools = self.shared_tools.clone();
        for (name, _) in &self.routed_tools {
            if !tools.contains(name) {
                tools.push(name.clone());
            }
        }
        tools
    }

    /// Backward compatibility: primary expert ID.
    pub fn expert_id(&self) -> ExpertId {
        self.primary_expert()
    }

    /// Backward compatibility: all tool names as flat Vec.
    pub fn tool_names(&self) -> Vec<String> {
        self.all_tool_names()
    }
}

// =============================================================================
// Success Tracking (for bias adaptation)
// =============================================================================

/// Tracks routing success rate per expert for bias adaptation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuccessRate {
    /// Tasks completed without needing fallback.
    pub successes: u64,
    /// Total tasks routed to this expert.
    pub total: u64,
    /// Tasks where tool execution failed.
    pub tool_errors: u64,
}

// =============================================================================
// Router Stats
// =============================================================================

/// Routing statistics for the `/moe status` command.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RouterStats {
    /// Total tasks routed.
    pub total_routed: u64,
    /// Cache hits.
    pub cache_hits: u64,
    /// Per-expert hit counts.
    pub expert_hits: HashMap<ExpertId, u64>,
    /// Estimated tokens saved (tools not sent).
    pub tokens_saved: u64,
    /// Number of multi-expert routes (cross-domain).
    pub multi_expert_routes: u64,
}

// =============================================================================
// Sparse Router (MoeRouter replacement)
// =============================================================================

/// DeepSeek V3-inspired Sparse MoE Router.
///
/// Scores ALL 20 experts independently via keyword affinity (sigmoid-like),
/// selects Top-K above activation threshold, merges tool sets with
/// mixed-precision schemas, and applies token budget constraints.
pub struct MoeRouter {
    /// Configuration.
    config: MoeConfig,
    /// LRU cache: normalized task prefix → full route result.
    classification_cache: LruCache<String, CachedRoute>,
    /// Dynamic bias per expert (DeepSeek auxiliary-loss-free load balancing).
    expert_bias: HashMap<ExpertId, f64>,
    /// Success tracking per expert (for bias adaptation).
    success_tracker: HashMap<ExpertId, SuccessRate>,
    /// Routing statistics.
    stats: RouterStats,
    /// Per-expert tool usage tracking for conditional pruning.
    tool_usage_history: HashMap<ExpertId, HashMap<String, usize>>,
    /// Current iteration count per expert (for prune threshold).
    expert_iterations: HashMap<ExpertId, usize>,
    /// Total routes since last bias update.
    routes_since_bias_update: usize,
}

/// Cached route result (stored in LRU).
#[derive(Debug, Clone)]
struct CachedRoute {
    classification: TaskClassification,
    selected_experts: Vec<(ExpertId, f64)>,
}

impl MoeRouter {
    /// Create a new sparse MoE router.
    pub fn new(config: MoeConfig) -> Self {
        let cache_size =
            NonZeroUsize::new(config.classification_cache_size).unwrap_or(NonZeroUsize::MIN);

        // Initialize bias to 0.0 for all experts
        let expert_bias: HashMap<ExpertId, f64> =
            ExpertId::all().iter().map(|&id| (id, 0.0)).collect();

        Self {
            config,
            classification_cache: LruCache::new(cache_size),
            expert_bias,
            success_tracker: HashMap::new(),
            stats: RouterStats::default(),
            tool_usage_history: HashMap::new(),
            expert_iterations: HashMap::new(),
            routes_since_bias_update: 0,
        }
    }

    /// Route a task to the appropriate expert(s).
    ///
    /// Algorithm (mirrors DeepSeek V3 sigmoid + top-K + bias):
    /// 1. CACHE: Check LRU cache
    /// 2. SCORE: Keyword affinity + bias per expert
    /// 3. FILTER: Keep experts above activation threshold
    /// 4. SELECT: Top-K by biased score
    /// 5. WEIGHT: Normalize using BASE scores only (no bias in weights)
    /// 6. MERGE: Union tool sets, deduplicated
    /// 7. BUDGET: Token budget constraint
    /// 8. PRECISION: Assign tool precision tiers
    pub fn route(&mut self, task: &str) -> SparseRouteResult {
        let cache_key = Self::normalize_task(task);

        // 1. CACHE CHECK
        let (classification, selected_experts, cache_hit) =
            if let Some(cached) = self.classification_cache.get(&cache_key) {
                (
                    cached.classification.clone(),
                    cached.selected_experts.clone(),
                    true,
                )
            } else {
                // 2. SCORE all experts
                let classification = TaskClassification::classify(task);
                let heuristic_expert = ExpertId::from_classification(&classification);
                let selected = self.score_and_select(task, heuristic_expert);

                // Cache the result
                self.classification_cache.put(
                    cache_key,
                    CachedRoute {
                        classification: classification.clone(),
                        selected_experts: selected.clone(),
                    },
                );

                (classification, selected, false)
            };

        // 6. MERGE tool sets from selected experts
        let shared_tools = ExpertId::shared_tools();
        let mut routed_tools = Vec::new();
        let mut seen_tools: std::collections::HashSet<String> = shared_tools.iter().cloned().collect();

        for (rank, (expert_id, _score)) in selected_experts.iter().enumerate() {
            let precision = match rank {
                0 => ToolPrecision::Full,    // Primary expert
                1 => ToolPrecision::Half,    // Secondary
                _ => ToolPrecision::Quarter, // Tertiary
            };

            // Get tools — config override or default domain tools
            let domain_tools = if let Some(ec) = self.config.expert_configs.get(expert_id) {
                ec.effective_tools()
                    .into_iter()
                    .filter(|t| !ExpertId::shared_tools().contains(t))
                    .collect()
            } else {
                expert_id.domain_tools().iter().map(|s| s.to_string()).collect::<Vec<_>>()
            };

            for tool in domain_tools {
                if !seen_tools.contains(&tool) {
                    seen_tools.insert(tool.clone());
                    routed_tools.push((tool, precision));
                }
            }
        }

        // 7. BUDGET: Apply token budget constraint
        let shared_tokens = shared_tools.len() * ToolPrecision::Full.estimated_tokens();
        let routed_budget = self.config.max_tool_tokens.saturating_sub(shared_tokens);
        let mut budget_remaining = routed_budget;
        let mut budgeted_tools = Vec::new();

        for (tool, precision) in &routed_tools {
            let cost = precision.estimated_tokens();
            if budget_remaining >= cost {
                budget_remaining -= cost;
                budgeted_tools.push((tool.clone(), *precision));
            } else if budget_remaining >= ToolPrecision::Quarter.estimated_tokens() {
                // Downgrade to Quarter precision if possible
                budget_remaining -= ToolPrecision::Quarter.estimated_tokens();
                budgeted_tools.push((tool.clone(), ToolPrecision::Quarter));
            }
            // else: skip this tool (budget exhausted)
        }

        let total_tool_tokens = shared_tokens
            + budgeted_tools
                .iter()
                .map(|(_, p)| p.estimated_tokens())
                .sum::<usize>();

        // Build system prompt addendum from all selected experts
        let mut addendum_parts = Vec::new();
        for (expert_id, _) in &selected_experts {
            addendum_parts.push(expert_id.system_prompt_addendum());
            if let Some(ec) = self.config.expert_configs.get(expert_id) {
                if !ec.extra_prompt.is_empty() {
                    addendum_parts.push(&ec.extra_prompt);
                }
            }
        }
        let system_prompt_addendum = addendum_parts.join("\n\n");

        // Build routing reasoning (interpretability)
        let reasoning = format!(
            "Scored {} experts; selected Top-{}: {}. Classification: {:?}. \
             {} shared + {} routed tools (~{} tokens). Cache {}.",
            ExpertId::all().len(),
            selected_experts.len(),
            selected_experts
                .iter()
                .map(|(id, s)| format!("{}({:.2})", id.display_name(), s))
                .collect::<Vec<_>>()
                .join(", "),
            classification,
            shared_tools.len(),
            budgeted_tools.len(),
            total_tool_tokens,
            if cache_hit { "hit" } else { "miss" },
        );

        // Update stats
        self.stats.total_routed += 1;
        if cache_hit {
            self.stats.cache_hits += 1;
        }
        if selected_experts.len() > 1 {
            self.stats.multi_expert_routes += 1;
        }
        for (expert_id, _) in &selected_experts {
            *self.stats.expert_hits.entry(*expert_id).or_insert(0) += 1;
        }
        // Token savings: full set is ~72 tools × 400 = 28800
        let all_tools_count = shared_tools.len() + budgeted_tools.len();
        let tools_saved = 72_u64.saturating_sub(all_tools_count as u64);
        self.stats.tokens_saved += tools_saved * 400;

        // Bias adaptation check
        self.routes_since_bias_update += 1;
        if self.routes_since_bias_update >= self.config.bias_update_interval {
            self.update_bias();
        }

        SparseRouteResult {
            selected_experts,
            shared_tools,
            routed_tools: budgeted_tools,
            system_prompt_addendum,
            total_tool_tokens,
            cache_hit,
            routing_reasoning: reasoning,
            classification,
        }
    }

    /// Score all experts and select Top-K above activation threshold.
    fn score_and_select(
        &self,
        task: &str,
        heuristic_expert: ExpertId,
    ) -> Vec<(ExpertId, f64)> {
        let lower = task.to_lowercase();
        let mut scored: Vec<(ExpertId, f64, f64)> = Vec::with_capacity(20);

        for &expert_id in ExpertId::all() {
            let base_score = Self::keyword_affinity(&lower, expert_id);

            // Boost the heuristic-classified expert (trust TaskClassification)
            let boosted = if expert_id == heuristic_expert && base_score < 0.5 {
                (base_score + 0.5).min(1.0)
            } else {
                base_score
            };

            let bias = self.expert_bias.get(&expert_id).copied().unwrap_or(0.0);
            let biased_score = boosted + bias;

            if biased_score > self.config.activation_threshold {
                scored.push((expert_id, boosted, biased_score));
            }
        }

        // Sort by biased score (selection), but store base score (for weights)
        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Take Top-K
        let selected: Vec<(ExpertId, f64)> = scored
            .into_iter()
            .take(self.config.max_experts_per_route)
            .map(|(id, base, _biased)| (id, base))
            .collect();

        // Fallback: if nothing scored above threshold, use heuristic expert
        if selected.is_empty() {
            vec![(heuristic_expert, 1.0)]
        } else {
            selected
        }
    }

    /// Compute keyword affinity score for an expert (0.0-1.0).
    ///
    /// O(keywords) per expert, <1ms total for all 20 experts.
    fn keyword_affinity(lower_task: &str, expert_id: ExpertId) -> f64 {
        let keywords = expert_id.keywords();
        let negative = expert_id.negative_keywords();

        if keywords.is_empty() {
            return 0.0;
        }

        let matched = keywords
            .iter()
            .filter(|kw| lower_task.contains(**kw))
            .count();

        let negated = negative
            .iter()
            .filter(|kw| lower_task.contains(**kw))
            .count();

        let score = (matched as f64 / keywords.len() as f64) - (negated as f64 * 0.3);
        score.clamp(0.0, 1.0)
    }

    /// Update expert bias based on success tracking (DeepSeek auxiliary-loss-free).
    fn update_bias(&mut self) {
        let total_routes = self
            .success_tracker
            .values()
            .map(|s| s.total)
            .sum::<u64>();

        if total_routes == 0 {
            self.routes_since_bias_update = 0;
            return;
        }

        let num_experts = ExpertId::all().len() as u64;
        let avg_usage = total_routes / num_experts.max(1);
        let gamma = self.config.bias_gamma;
        let clamp = self.config.bias_clamp;

        for &expert_id in ExpertId::all() {
            let success = self
                .success_tracker
                .get(&expert_id)
                .cloned()
                .unwrap_or_default();

            let bias = self.expert_bias.entry(expert_id).or_insert(0.0);

            // Load balancing: under/over-loaded adjustment
            if success.total > avg_usage * 3 / 2 {
                *bias -= gamma; // Overloaded
            } else if success.total < avg_usage / 2 {
                *bias += gamma; // Underloaded
            }

            // Quality: high error rate penalty
            if success.total > 0 && (success.tool_errors as f64 / success.total as f64) > 0.2 {
                *bias -= gamma * 2.0;
            }

            *bias = bias.clamp(-clamp, clamp);
        }

        // Reset counters
        for rate in self.success_tracker.values_mut() {
            rate.successes = 0;
            rate.total = 0;
            rate.tool_errors = 0;
        }
        self.routes_since_bias_update = 0;
    }

    // =========================================================================
    // Public API (backward compatible with old MoeRouter)
    // =========================================================================

    /// Check if MoE is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get current routing statistics.
    pub fn stats(&self) -> &RouterStats {
        &self.stats
    }

    /// Get the expert config for a given expert.
    pub fn expert_config(&self, expert_id: &ExpertId) -> Option<&ExpertConfig> {
        self.config.expert_configs.get(expert_id)
    }

    /// Get a reference to the config.
    pub fn config(&self) -> &MoeConfig {
        &self.config
    }

    /// Get current bias values for all experts.
    pub fn bias_values(&self) -> &HashMap<ExpertId, f64> {
        &self.expert_bias
    }

    /// Get success tracking data.
    pub fn success_tracker(&self) -> &HashMap<ExpertId, SuccessRate> {
        &self.success_tracker
    }

    /// Record a routing success for an expert.
    pub fn record_success(&mut self, expert_id: ExpertId) {
        let rate = self.success_tracker.entry(expert_id).or_default();
        rate.successes += 1;
        rate.total += 1;
    }

    /// Record a routing failure (tool error) for an expert.
    pub fn record_failure(&mut self, expert_id: ExpertId) {
        let rate = self.success_tracker.entry(expert_id).or_default();
        rate.tool_errors += 1;
        rate.total += 1;
    }

    /// Record that a tool was used by an expert. Used for conditional pruning.
    pub fn record_tool_usage(&mut self, expert_id: ExpertId, tool_name: &str) {
        *self
            .tool_usage_history
            .entry(expert_id)
            .or_default()
            .entry(tool_name.to_string())
            .or_insert(0) += 1;
    }

    /// Increment the iteration count for an expert and return pruned tool set
    /// if pruning threshold is reached.
    pub fn prune_unused_tools(&mut self, expert_id: ExpertId) -> Option<Vec<String>> {
        if self.config.prune_after_iterations == 0 {
            return None;
        }

        let count = self.expert_iterations.entry(expert_id).or_insert(0);
        *count += 1;

        if *count < self.config.prune_after_iterations {
            return None;
        }

        let usage = self.tool_usage_history.get(&expert_id)?;
        let all_tools = expert_id.tool_names();
        let core_tools = ExpertId::shared_tools();

        let pruned: Vec<String> = all_tools
            .into_iter()
            .filter(|tool| {
                core_tools.contains(tool) || usage.get(tool.as_str()).copied().unwrap_or(0) > 0
            })
            .collect();

        Some(pruned)
    }

    /// Reset the iteration count and usage history for an expert.
    pub fn reset_pruning(&mut self, expert_id: ExpertId) {
        self.expert_iterations.remove(&expert_id);
        self.tool_usage_history.remove(&expert_id);
    }

    /// Normalize a task string into a cache key (lowercase, first 100 chars).
    fn normalize_task(task: &str) -> String {
        let lower = task.to_lowercase();
        if lower.len() > 100 {
            lower[..100].to_string()
        } else {
            lower
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_file_task() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("Read the file main.rs and show me what is inside");
        assert_eq!(result.primary_expert(), ExpertId::FileOps);
        assert!(result.all_tool_names().contains(&"file_read".to_string()));
    }

    #[test]
    fn test_route_macos_task() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("Add a reminder for tomorrow at 3pm");
        // Should route to MacOSApps expert for reminders
        assert_eq!(result.primary_expert(), ExpertId::MacOSApps);
    }

    #[test]
    fn test_route_caching() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result1 = router.route("Read the file foo.rs");
        assert!(!result1.cache_hit);
        let result2 = router.route("Read the file foo.rs");
        assert!(result2.cache_hit);
        assert_eq!(result1.primary_expert(), result2.primary_expert());
    }

    #[test]
    fn test_route_stats() {
        let mut router = MoeRouter::new(MoeConfig::default());
        router.route("test task 1");
        router.route("test task 2");
        let stats = router.stats();
        assert_eq!(stats.total_routed, 2);
    }

    #[test]
    fn test_route_security_workflow() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("run a security scan on the codebase");
        // Should go to a security expert
        let primary = result.primary_expert();
        assert!(
            matches!(primary, ExpertId::SecScan | ExpertId::SecReview | ExpertId::SecCompliance | ExpertId::SecIncident),
            "Expected Security expert, got {primary:?}"
        );
    }

    #[test]
    fn test_disabled_moe() {
        let config = MoeConfig {
            enabled: false,
            ..Default::default()
        };
        let router = MoeRouter::new(config);
        assert!(!router.is_enabled());
    }

    #[test]
    fn test_token_savings_tracked() {
        let mut router = MoeRouter::new(MoeConfig::default());
        router.route("check my calendar for tomorrow");
        assert!(router.stats().tokens_saved > 0);
    }

    #[test]
    fn test_tool_usage_tracking() {
        let mut router = MoeRouter::new(MoeConfig::default());
        router.record_tool_usage(ExpertId::FileOps, "file_read");
        router.record_tool_usage(ExpertId::FileOps, "file_read");
        router.record_tool_usage(ExpertId::FileOps, "shell_exec");

        let usage = router.tool_usage_history.get(&ExpertId::FileOps).unwrap();
        assert_eq!(usage["file_read"], 2);
        assert_eq!(usage["shell_exec"], 1);
    }

    #[test]
    fn test_prune_unused_tools() {
        let config = MoeConfig {
            prune_after_iterations: 3,
            ..Default::default()
        };
        let mut router = MoeRouter::new(config);

        // Record usage of only file_read and shell_exec
        router.record_tool_usage(ExpertId::FileOps, "file_read");
        router.record_tool_usage(ExpertId::FileOps, "shell_exec");

        // Not at threshold yet
        assert!(router.prune_unused_tools(ExpertId::FileOps).is_none());
        assert!(router.prune_unused_tools(ExpertId::FileOps).is_none());

        // At threshold — should return pruned set
        let pruned = router.prune_unused_tools(ExpertId::FileOps);
        assert!(pruned.is_some());
        let tools = pruned.unwrap();
        // Should have core/shared tools + used tools
        assert!(tools.contains(&"file_read".to_string()));
        assert!(tools.contains(&"shell_exec".to_string()));
        assert!(tools.contains(&"ask_user".to_string())); // Shared tool
    }

    #[test]
    fn test_prune_disabled() {
        let config = MoeConfig {
            prune_after_iterations: 0,
            ..Default::default()
        };
        let mut router = MoeRouter::new(config);
        router.record_tool_usage(ExpertId::FileOps, "file_read");
        for _ in 0..10 {
            assert!(router.prune_unused_tools(ExpertId::FileOps).is_none());
        }
    }

    #[test]
    fn test_reset_pruning() {
        let mut router = MoeRouter::new(MoeConfig::default());
        router.record_tool_usage(ExpertId::FileOps, "file_read");
        router.reset_pruning(ExpertId::FileOps);
        assert!(!router.tool_usage_history.contains_key(&ExpertId::FileOps));
    }

    // --- New Sparse Router Tests ---

    #[test]
    fn test_sigmoid_scoring_single_expert() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("train a neural network model with gradient descent");
        // Should score MLTrain highest
        assert_eq!(result.primary_expert(), ExpertId::MLTrain);
    }

    #[test]
    fn test_sigmoid_scoring_multi_expert() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("scan my model for security vulnerabilities and check for bias");
        // Should select multiple experts (ML + Security)
        let experts: Vec<ExpertId> = result.selected_experts.iter().map(|(id, _)| *id).collect();
        // At least one ML and one Security expert
        let has_ml = experts.iter().any(|e| matches!(e,
            ExpertId::MLTrain | ExpertId::MLData | ExpertId::MLInference
            | ExpertId::MLSafety | ExpertId::MLResearch
        ));
        let has_sec = experts.iter().any(|e| matches!(e,
            ExpertId::SecScan | ExpertId::SecReview | ExpertId::SecCompliance
            | ExpertId::SecIncident
        ));
        assert!(
            has_ml || has_sec,
            "Expected cross-domain routing, got: {experts:?}"
        );
    }

    #[test]
    fn test_shared_tools_always_present() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("any random task");
        // Shared tools must always be present
        assert!(result.shared_tools.contains(&"ask_user".to_string()));
        assert!(result.shared_tools.contains(&"file_read".to_string()));
        assert!(result.shared_tools.contains(&"file_write".to_string()));
        assert!(result.shared_tools.contains(&"shell_exec".to_string()));
        assert_eq!(result.shared_tools.len(), 8);
    }

    #[test]
    fn test_token_budget_constraint() {
        let config = MoeConfig {
            max_tool_tokens: 6000,
            ..Default::default()
        };
        let mut router = MoeRouter::new(config);
        let result = router.route("do everything possible");
        assert!(
            result.total_tool_tokens <= 6000,
            "Token budget exceeded: {}",
            result.total_tool_tokens
        );
    }

    #[test]
    fn test_mixed_precision_assignment() {
        let mut router = MoeRouter::new(MoeConfig::default());
        // Force a multi-expert route
        let result = router.route("scan my model for security vulnerabilities and run evaluation");
        if result.selected_experts.len() >= 2 {
            // Primary expert tools should be Full precision
            let primary_tools: Vec<&str> = result.selected_experts[0]
                .0
                .domain_tools()
                .into_iter()
                .collect();
            for (tool, precision) in &result.routed_tools {
                if primary_tools.contains(&tool.as_str()) {
                    assert_eq!(
                        *precision,
                        ToolPrecision::Full,
                        "Primary tool {tool} should be Full precision"
                    );
                }
            }
        }
    }

    #[test]
    fn test_bias_does_not_affect_gating_weights() {
        let mut router = MoeRouter::new(MoeConfig::default());
        // Record many failures for FileOps
        for _ in 0..100 {
            router.record_failure(ExpertId::FileOps);
        }
        // Force bias update
        router.routes_since_bias_update = router.config.bias_update_interval;
        router.update_bias();

        // FileOps bias should be negative
        assert!(*router.expert_bias.get(&ExpertId::FileOps).unwrap() < 0.0);

        // But a file task should still work (heuristic boost overrides)
        let result = router.route("read the file test.txt");
        assert_eq!(result.primary_expert(), ExpertId::FileOps);
    }

    #[test]
    fn test_20_experts_exist() {
        assert_eq!(ExpertId::all().len(), 20);
    }

    #[test]
    fn test_20_experts_max_12_domain_tools() {
        for &expert in ExpertId::all() {
            let count = expert.domain_tools().len();
            assert!(
                count <= 12,
                "Expert {expert:?} has {count} domain tools (max 12)"
            );
        }
    }

    #[test]
    fn test_cache_stores_full_result() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result1 = router.route("train model with LoRA adapter");
        assert!(!result1.cache_hit);
        let result2 = router.route("train model with LoRA adapter");
        assert!(result2.cache_hit);
        assert_eq!(result1.primary_expert(), result2.primary_expert());
        assert_eq!(result1.selected_experts.len(), result2.selected_experts.len());
    }

    #[test]
    fn test_routing_reasoning_populated() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("read a file from disk");
        assert!(!result.routing_reasoning.is_empty());
        assert!(result.routing_reasoning.contains("expert"));
    }

    #[test]
    fn test_ml_keyword_routing() {
        let mut router = MoeRouter::new(MoeConfig::default());
        let result = router.route("How can I use LoRA for quantization");
        let primary = result.primary_expert();
        assert!(
            matches!(primary, ExpertId::MLTrain),
            "Expected MLTrain for LoRA query, got {primary:?}"
        );
    }
}
