//! Model benchmarking — latency, throughput, accuracy, resource usage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Bias metrics for fairness evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BiasMetrics {
    pub gender_bias_score: f64,
    pub racial_bias_score: f64,
    pub overall_bias_score: f64,
}

/// Benchmark result for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub model_id: String,
    pub model_name: String,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub throughput_rps: f64,
    pub memory_mb: f64,
    pub accuracy: Option<f64>,
    pub safety_score: Option<f64>,
    pub bias_metrics: Option<BiasMetrics>,
    pub custom_metrics: HashMap<String, f64>,
    pub timestamp: DateTime<Utc>,
}

impl BenchmarkResult {
    pub fn new(model_id: &str, model_name: &str) -> Self {
        Self {
            model_id: model_id.to_string(),
            model_name: model_name.to_string(),
            latency_p50_ms: 0.0,
            latency_p95_ms: 0.0,
            latency_p99_ms: 0.0,
            throughput_rps: 0.0,
            memory_mb: 0.0,
            accuracy: None,
            safety_score: None,
            bias_metrics: None,
            custom_metrics: HashMap::new(),
            timestamp: Utc::now(),
        }
    }
}

/// Compare multiple benchmark results.
pub fn compare_benchmarks(results: &[BenchmarkResult]) -> BenchmarkComparison {
    let best_latency = results
        .iter()
        .min_by(|a, b| {
            a.latency_p50_ms
                .partial_cmp(&b.latency_p50_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| r.model_name.clone());
    let best_throughput = results
        .iter()
        .max_by(|a, b| {
            a.throughput_rps
                .partial_cmp(&b.throughput_rps)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| r.model_name.clone());

    BenchmarkComparison {
        models_compared: results.len(),
        best_latency,
        best_throughput,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    pub models_compared: usize,
    pub best_latency: Option<String>,
    pub best_throughput: Option<String>,
}
