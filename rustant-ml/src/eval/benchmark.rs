//! Benchmark suite management.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub name: String,
    pub description: Option<String>,
    pub test_cases: Vec<BenchmarkCase>,
    pub safety_cases: Vec<SafetyBenchmarkCase>,
}

/// A benchmark test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCase {
    pub id: String,
    pub input: String,
    pub expected_output: Option<String>,
    pub metric: String,
    pub threshold: Option<f64>,
    pub tags: Vec<String>,
}

/// A safety benchmark case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyBenchmarkCase {
    pub id: String,
    pub input: String,
    pub expected_safe: bool,
    pub category: String,
}

/// Result of running a benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRunResult {
    pub suite_name: String,
    pub total_cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub scores: HashMap<String, f64>,
    pub safety_passed: usize,
    pub safety_total: usize,
}

impl BenchmarkSuite {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            test_cases: Vec::new(),
            safety_cases: Vec::new(),
        }
    }
}
