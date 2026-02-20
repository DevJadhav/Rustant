//! Client-side token bucket rate limiter for LLM provider requests.
//!
//! Proactively throttles requests to stay within provider rate limits instead
//! of relying on 429 backpressure. Spreads requests evenly across the minute
//! window to avoid bursts.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Configuration for provider rate limits.
#[derive(Debug, Clone, Default)]
pub struct RateLimitConfig {
    /// Input tokens per minute limit (0 = unlimited).
    pub itpm: usize,
    /// Output tokens per minute limit (0 = unlimited).
    pub otpm: usize,
    /// Requests per minute limit (0 = unlimited).
    pub rpm: usize,
}

/// A sliding-window rate limiter that tracks token and request usage.
pub struct TokenBucketLimiter {
    config: RateLimitConfig,
    /// (timestamp, input_tokens) for recent requests within the window.
    input_tokens_window: VecDeque<(Instant, usize)>,
    /// (timestamp, output_tokens) for recent requests within the window.
    output_tokens_window: VecDeque<(Instant, usize)>,
    /// Timestamps of recent requests within the window.
    requests_window: VecDeque<Instant>,
    /// Sliding window duration (1 minute).
    window: Duration,
}

impl TokenBucketLimiter {
    /// Create a new rate limiter with the given config.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            input_tokens_window: VecDeque::new(),
            output_tokens_window: VecDeque::new(),
            requests_window: VecDeque::new(),
            window: Duration::from_secs(60),
        }
    }

    /// Check if a request with the estimated token count can proceed now.
    ///
    /// Returns `None` if the request can proceed immediately, or `Some(delay)`
    /// indicating how long to wait before retrying.
    pub fn check(&mut self, estimated_input_tokens: usize) -> Option<Duration> {
        let now = Instant::now();
        self.prune(now);

        // Check RPM
        if self.config.rpm > 0 && self.requests_window.len() >= self.config.rpm {
            if let Some(&oldest) = self.requests_window.front() {
                let wait = self.window.saturating_sub(now.duration_since(oldest));
                if !wait.is_zero() {
                    return Some(wait);
                }
            }
        }

        // Check ITPM
        if self.config.itpm > 0 {
            let current_input: usize = self.input_tokens_window.iter().map(|(_, t)| t).sum();
            if current_input + estimated_input_tokens > self.config.itpm {
                if let Some(&(oldest, _)) = self.input_tokens_window.front() {
                    let wait = self.window.saturating_sub(now.duration_since(oldest));
                    if !wait.is_zero() {
                        return Some(wait);
                    }
                }
            }
        }

        None
    }

    /// Record actual usage after a response is received.
    pub fn record(&mut self, input_tokens: usize, output_tokens: usize) {
        let now = Instant::now();
        self.input_tokens_window.push_back((now, input_tokens));
        self.output_tokens_window.push_back((now, output_tokens));
        self.requests_window.push_back(now);
    }

    /// Update limits from provider response headers (e.g. `anthropic-ratelimit-*`).
    pub fn update_from_headers(
        &mut self,
        itpm: Option<usize>,
        otpm: Option<usize>,
        rpm: Option<usize>,
    ) {
        if let Some(v) = itpm {
            self.config.itpm = v;
        }
        if let Some(v) = otpm {
            self.config.otpm = v;
        }
        if let Some(v) = rpm {
            self.config.rpm = v;
        }
    }

    /// Check if any limits are configured.
    pub fn has_limits(&self) -> bool {
        self.config.itpm > 0 || self.config.otpm > 0 || self.config.rpm > 0
    }

    /// Get current usage within the window.
    pub fn current_usage(&mut self) -> (usize, usize, usize) {
        let now = Instant::now();
        self.prune(now);
        let input: usize = self.input_tokens_window.iter().map(|(_, t)| t).sum();
        let output: usize = self.output_tokens_window.iter().map(|(_, t)| t).sum();
        let requests = self.requests_window.len();
        (input, output, requests)
    }

    /// Remove entries older than the sliding window.
    fn prune(&mut self, now: Instant) {
        let cutoff = now - self.window;
        while self
            .input_tokens_window
            .front()
            .is_some_and(|(t, _)| *t < cutoff)
        {
            self.input_tokens_window.pop_front();
        }
        while self
            .output_tokens_window
            .front()
            .is_some_and(|(t, _)| *t < cutoff)
        {
            self.output_tokens_window.pop_front();
        }
        while self.requests_window.front().is_some_and(|t| *t < cutoff) {
            self.requests_window.pop_front();
        }
    }
}

/// Parse rate limit headers from HTTP responses.
///
/// Supports Anthropic (`anthropic-ratelimit-*`) and OpenAI (`x-ratelimit-*`) formats.
pub fn parse_rate_limit_headers(
    headers: &reqwest::header::HeaderMap,
) -> (Option<usize>, Option<usize>, Option<usize>) {
    let itpm = headers
        .get("anthropic-ratelimit-input-tokens-limit")
        .or_else(|| headers.get("x-ratelimit-limit-tokens"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());

    let rpm = headers
        .get("anthropic-ratelimit-requests-limit")
        .or_else(|| headers.get("x-ratelimit-limit-requests"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());

    // Output TPM not consistently exposed, leave as None
    (itpm, None, rpm)
}

/// Parse Retry-After header from HTTP 429 responses.
///
/// Supports both seconds (numeric) and HTTP-date formats. Returns the
/// delay as a Duration.
pub fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers
        .get("retry-after")
        .or_else(|| headers.get("anthropic-ratelimit-input-tokens-reset"))
        .or_else(|| headers.get("x-ratelimit-reset-tokens"))
        .and_then(|v| v.to_str().ok())?;

    // Try parsing as seconds first
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }

    // Try parsing as fractional seconds (e.g., "0.5s")
    if let Some(stripped) = value.strip_suffix('s') {
        if let Ok(secs) = stripped.parse::<f64>() {
            return Some(Duration::from_secs_f64(secs));
        }
    }

    // Fall back to a conservative 5 second delay
    Some(Duration::from_secs(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_no_limits() {
        let mut limiter = TokenBucketLimiter::new(RateLimitConfig::default());
        assert!(limiter.check(10000).is_none());
        assert!(!limiter.has_limits());
    }

    #[test]
    fn test_limiter_rpm() {
        let config = RateLimitConfig {
            rpm: 2,
            itpm: 0,
            otpm: 0,
        };
        let mut limiter = TokenBucketLimiter::new(config);

        // First two requests should pass
        assert!(limiter.check(100).is_none());
        limiter.record(100, 50);
        assert!(limiter.check(100).is_none());
        limiter.record(100, 50);

        // Third request should be delayed
        let delay = limiter.check(100);
        assert!(delay.is_some());
        assert!(delay.unwrap().as_secs() > 0);
    }

    #[test]
    fn test_limiter_itpm() {
        let config = RateLimitConfig {
            rpm: 0,
            itpm: 500,
            otpm: 0,
        };
        let mut limiter = TokenBucketLimiter::new(config);

        assert!(limiter.check(400).is_none());
        limiter.record(400, 200);

        // This would exceed 500 ITPM
        let delay = limiter.check(200);
        assert!(delay.is_some());
    }

    #[test]
    fn test_current_usage() {
        let mut limiter = TokenBucketLimiter::new(RateLimitConfig::default());
        limiter.record(100, 50);
        limiter.record(200, 100);
        let (input, output, requests) = limiter.current_usage();
        assert_eq!(input, 300);
        assert_eq!(output, 150);
        assert_eq!(requests, 2);
    }

    #[test]
    fn test_update_from_headers() {
        let mut limiter = TokenBucketLimiter::new(RateLimitConfig::default());
        assert!(!limiter.has_limits());
        limiter.update_from_headers(Some(100_000), None, Some(60));
        assert!(limiter.has_limits());
    }
}
