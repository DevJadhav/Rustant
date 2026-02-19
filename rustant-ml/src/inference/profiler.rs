//! Inference profiling — latency, throughput, memory.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Inference performance profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProfile {
    pub model: String,
    pub backend: String,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub tokens_per_second: f64,
    pub memory_usage_mb: f64,
    pub gpu_utilization: Option<f64>,
    pub requests_profiled: usize,
    pub cost_per_1k_tokens: f64,
    pub timestamp: DateTime<Utc>,
}

/// Calculate percentiles from a sorted list of values.
pub fn percentile(sorted_values: &[f64], p: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted_values.len() - 1) as f64).round() as usize;
    sorted_values[idx.min(sorted_values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert!((percentile(&values, 50.0) - 5.5).abs() < 1.5);
        assert!((percentile(&values, 95.0) - 9.5).abs() < 1.5);
    }
}
