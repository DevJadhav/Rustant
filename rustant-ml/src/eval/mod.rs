//! Extended evaluation framework — LLM-as-Judge, error analysis, domain evaluators.

pub mod agreement;
pub mod benchmark;
pub mod ci_integration;
pub mod domain_evals;
pub mod error_analysis;
pub mod generators;
pub mod llm_judge;
pub mod traces;

pub use benchmark::BenchmarkSuite;
pub use error_analysis::ErrorTaxonomy;
pub use llm_judge::{JudgeCalibration, LlmJudgeEvaluator, LlmJudgement};
pub use traces::TraceStore;
