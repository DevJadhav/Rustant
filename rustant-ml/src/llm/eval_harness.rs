//! LLM evaluation harness — perplexity, benchmarks.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LLM benchmark type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmBenchmark {
    Perplexity,
    Mmlu,
    HellaSwag,
    TruthfulQa,
    HumanEval,
    Gsm8k,
    Arc,
    WinoGrande,
    Custom { name: String, dataset_path: String },
}

/// Result of an LLM evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmEvalResult {
    pub model: String,
    pub benchmark: String,
    pub score: f64,
    pub details: HashMap<String, f64>,
    pub samples_evaluated: usize,
}

/// LLM evaluation harness.
pub struct LlmEvalHarness {
    #[allow(dead_code)]
    workspace: std::path::PathBuf,
}

impl LlmEvalHarness {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self { workspace }
    }

    pub async fn evaluate(
        &self,
        model: &str,
        benchmarks: &[LlmBenchmark],
    ) -> Result<Vec<LlmEvalResult>, crate::error::MlError> {
        let mut results = Vec::new();
        for bench in benchmarks {
            let bench_name = match bench {
                LlmBenchmark::Perplexity => "perplexity",
                LlmBenchmark::Mmlu => "mmlu",
                LlmBenchmark::HellaSwag => "hellaswag",
                LlmBenchmark::TruthfulQa => "truthfulqa",
                LlmBenchmark::HumanEval => "humaneval",
                LlmBenchmark::Gsm8k => "gsm8k",
                LlmBenchmark::Arc => "arc",
                LlmBenchmark::WinoGrande => "winogrande",
                LlmBenchmark::Custom { name, .. } => name.as_str(),
            };
            results.push(LlmEvalResult {
                model: model.to_string(),
                benchmark: bench_name.to_string(),
                score: 0.0,
                details: HashMap::new(),
                samples_evaluated: 0,
            });
        }
        Ok(results)
    }
}
