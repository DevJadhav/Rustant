//! Prompt caching configuration and metrics for LLM providers.
//!
//! Supports Anthropic's `cache_control` breakpoints, OpenAI's automatic caching,
//! and Gemini's `CachedContent` API.

use serde::{Deserialize, Serialize};

/// Configuration for provider-level prompt caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable prompt caching (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Cache TTL in seconds for Gemini CachedContent (default: 3600).
    #[serde(default = "default_gemini_ttl")]
    pub gemini_ttl_secs: u64,
    /// Whether to cache tool definitions (default: true).
    #[serde(default = "default_true")]
    pub cache_tools: bool,
    /// Whether to cache the system prompt (default: true).
    #[serde(default = "default_true")]
    pub cache_system_prompt: bool,
    /// Minimum token count for cache eligibility (default: 1024).
    #[serde(default = "default_min_cacheable_tokens")]
    pub min_cacheable_tokens: usize,
}

fn default_true() -> bool {
    true
}
fn default_gemini_ttl() -> u64 {
    3600
}
fn default_min_cacheable_tokens() -> usize {
    1024
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            gemini_ttl_secs: 3600,
            cache_tools: true,
            cache_system_prompt: true,
            min_cacheable_tokens: 1024,
        }
    }
}

/// Current state of the prompt cache for a provider.
#[derive(Debug, Clone, PartialEq)]
pub enum CacheState {
    /// No cache established yet.
    Cold,
    /// Cache created but no hits yet.
    Warm { tokens: usize },
    /// Cache actively being hit.
    Hot { tokens: usize, hit_rate: f32 },
}

impl std::fmt::Display for CacheState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheState::Cold => write!(f, "Cold"),
            CacheState::Warm { tokens } => write!(f, "Warm ({tokens} tokens cached)"),
            CacheState::Hot { tokens, hit_rate } => {
                write!(
                    f,
                    "Hot ({} tokens, {:.0}% hit rate)",
                    tokens,
                    hit_rate * 100.0
                )
            }
        }
    }
}

/// Aggregate cache performance metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub cache_read_tokens: usize,
    pub cache_creation_tokens: usize,
    pub savings_usd: f64,
}

impl CacheMetrics {
    /// Compute the hit rate as a fraction (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn accumulate(&mut self, other: &CacheMetrics) {
        self.hits += other.hits;
        self.misses += other.misses;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.savings_usd += other.savings_usd;
    }
}

/// Information about a cached prefix (for tracking/invalidation).
#[derive(Debug, Clone)]
pub struct CachedPrefixInfo {
    /// Hash of the cached prefix content.
    pub prefix_hash: u64,
    /// Number of tokens in the cached prefix.
    pub token_count: usize,
    /// When the cache entry was created.
    pub created_at: std::time::Instant,
    /// When the cache entry expires (if applicable).
    pub expires_at: Option<std::time::Instant>,
}

/// Hints for provider-specific caching behavior, attached to CompletionRequest.
#[derive(Debug, Clone, Default)]
pub struct CacheHint {
    /// Enable prompt caching for this request.
    pub enable_prompt_cache: bool,
    /// Provider-specific cached content reference (e.g., Gemini cachedContent name).
    pub cached_content_ref: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_defaults() {
        let c = CacheConfig::default();
        assert!(c.enabled);
        assert_eq!(c.gemini_ttl_secs, 3600);
        assert!(c.cache_tools);
        assert!(c.cache_system_prompt);
        assert_eq!(c.min_cacheable_tokens, 1024);
    }

    #[test]
    fn test_cache_state_display() {
        assert_eq!(format!("{}", CacheState::Cold), "Cold");
        assert_eq!(
            format!("{}", CacheState::Warm { tokens: 5000 }),
            "Warm (5000 tokens cached)"
        );
        assert_eq!(
            format!(
                "{}",
                CacheState::Hot {
                    tokens: 8000,
                    hit_rate: 0.87
                }
            ),
            "Hot (8000 tokens, 87% hit rate)"
        );
    }

    #[test]
    fn test_cache_metrics_hit_rate() {
        let m = CacheMetrics {
            hits: 8,
            misses: 2,
            ..Default::default()
        };
        assert!((m.hit_rate() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_cache_metrics_hit_rate_zero() {
        let m = CacheMetrics::default();
        assert_eq!(m.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_metrics_accumulate() {
        let mut a = CacheMetrics {
            hits: 5,
            misses: 1,
            cache_read_tokens: 100,
            cache_creation_tokens: 50,
            savings_usd: 0.01,
        };
        let b = CacheMetrics {
            hits: 3,
            misses: 2,
            cache_read_tokens: 200,
            cache_creation_tokens: 0,
            savings_usd: 0.02,
        };
        a.accumulate(&b);
        assert_eq!(a.hits, 8);
        assert_eq!(a.misses, 3);
        assert_eq!(a.cache_read_tokens, 300);
        assert_eq!(a.cache_creation_tokens, 50);
        assert!((a.savings_usd - 0.03).abs() < 0.0001);
    }

    #[test]
    fn test_cache_config_serde_roundtrip() {
        let config = CacheConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CacheConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.enabled, config.enabled);
        assert_eq!(deserialized.gemini_ttl_secs, config.gemini_ttl_secs);
    }

    #[test]
    fn test_cache_config_deserialize_empty() {
        let config: CacheConfig = serde_json::from_str("{}").unwrap();
        assert!(config.enabled);
        assert_eq!(config.gemini_ttl_secs, 3600);
    }

    #[test]
    fn test_cache_hint_default() {
        let hint = CacheHint::default();
        assert!(!hint.enable_prompt_cache);
        assert!(hint.cached_content_ref.is_none());
    }
}
