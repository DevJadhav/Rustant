//! Configuration types for the rustant-ml crate.
//!
//! These are the ML-specific sub-configs referenced from the core `AIEngineerConfig`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level ML configuration within rustant-ml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MlConfig {
    /// Python runtime configuration.
    #[serde(default)]
    pub python: PythonConfig,
    /// Data engineering configuration.
    #[serde(default)]
    pub data: DataConfig,
    /// Training pipeline configuration.
    #[serde(default)]
    pub training: TrainingPipelineConfig,
    /// Model zoo configuration.
    #[serde(default)]
    pub zoo: ZooConfig,
    /// RAG pipeline configuration.
    #[serde(default)]
    pub rag: RagPipelineConfig,
    /// Evaluation framework configuration.
    #[serde(default)]
    pub evaluation: EvalConfig,
    /// Inference serving configuration.
    #[serde(default)]
    pub inference: InferenceServingConfig,
    /// Research tools configuration.
    #[serde(default)]
    pub research: ResearchConfig,
    /// AI safety pillar configuration.
    #[serde(default)]
    pub safety: AiSafetyConfig,
    /// Resource limits for ML operations.
    #[serde(default)]
    pub resource_limits: ResourceLimitsConfig,
}

/// Python runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonConfig {
    /// Path to Python executable (auto-detected if not set).
    #[serde(default)]
    pub python_path: Option<PathBuf>,
    /// Path to virtual environment (auto-detected if not set).
    #[serde(default)]
    pub venv_path: Option<PathBuf>,
    /// Default timeout for Python script execution (seconds).
    #[serde(default = "default_python_timeout")]
    pub timeout_secs: u64,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            python_path: None,
            venv_path: None,
            timeout_secs: default_python_timeout(),
        }
    }
}

fn default_python_timeout() -> u64 {
    300
}

/// Data engineering pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    /// Base directory for dataset storage.
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    /// Maximum dataset file size in MB.
    #[serde(default = "default_max_dataset_mb")]
    pub max_dataset_size_mb: u64,
    /// Enable automatic PII scanning on ingest.
    #[serde(default = "default_true")]
    pub auto_pii_scan: bool,
    /// Minimum data quality score to pass gate (0.0-1.0).
    #[serde(default = "default_quality_threshold")]
    pub quality_gate_threshold: f64,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            max_dataset_size_mb: default_max_dataset_mb(),
            auto_pii_scan: true,
            quality_gate_threshold: default_quality_threshold(),
        }
    }
}

fn default_data_dir() -> String {
    ".rustant/ml/datasets".to_string()
}

fn default_max_dataset_mb() -> u64 {
    500
}

fn default_quality_threshold() -> f64 {
    0.7
}

/// Training pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingPipelineConfig {
    /// Maximum training duration in hours.
    #[serde(default = "default_max_training_hours")]
    pub max_training_hours: f64,
    /// Enable training anomaly detection (loss spikes, NaN).
    #[serde(default = "default_true")]
    pub anomaly_detection: bool,
    /// Default early stopping patience (epochs).
    #[serde(default = "default_patience")]
    pub early_stopping_patience: usize,
    /// Checkpoint directory.
    #[serde(default = "default_checkpoint_dir")]
    pub checkpoint_dir: String,
}

impl Default for TrainingPipelineConfig {
    fn default() -> Self {
        Self {
            max_training_hours: default_max_training_hours(),
            anomaly_detection: true,
            early_stopping_patience: default_patience(),
            checkpoint_dir: default_checkpoint_dir(),
        }
    }
}

fn default_max_training_hours() -> f64 {
    24.0
}

fn default_patience() -> usize {
    5
}

fn default_checkpoint_dir() -> String {
    ".rustant/ml/checkpoints".to_string()
}

/// Model zoo configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZooConfig {
    /// Model storage directory.
    #[serde(default = "default_model_dir")]
    pub model_dir: String,
    /// Maximum model size in GB.
    #[serde(default = "default_max_model_gb")]
    pub max_model_size_gb: f64,
    /// Require provenance verification for downloaded models.
    #[serde(default = "default_true")]
    pub require_provenance: bool,
}

impl Default for ZooConfig {
    fn default() -> Self {
        Self {
            model_dir: default_model_dir(),
            max_model_size_gb: default_max_model_gb(),
            require_provenance: true,
        }
    }
}

fn default_model_dir() -> String {
    ".rustant/ml/models".to_string()
}

fn default_max_model_gb() -> f64 {
    50.0
}

/// RAG pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagPipelineConfig {
    /// Default chunk size in tokens.
    #[serde(default = "default_chunk_size")]
    pub default_chunk_size: usize,
    /// Default chunk overlap in tokens.
    #[serde(default = "default_chunk_overlap")]
    pub default_chunk_overlap: usize,
    /// Default number of results to retrieve.
    #[serde(default = "default_top_k")]
    pub default_top_k: usize,
    /// Enable groundedness checking on RAG responses.
    #[serde(default = "default_true")]
    pub groundedness_check: bool,
    /// Minimum groundedness score to return a response (0.0-1.0).
    #[serde(default = "default_groundedness_threshold")]
    pub groundedness_threshold: f64,
    /// Collections storage directory.
    #[serde(default = "default_collections_dir")]
    pub collections_dir: String,
}

impl Default for RagPipelineConfig {
    fn default() -> Self {
        Self {
            default_chunk_size: default_chunk_size(),
            default_chunk_overlap: default_chunk_overlap(),
            default_top_k: default_top_k(),
            groundedness_check: true,
            groundedness_threshold: default_groundedness_threshold(),
            collections_dir: default_collections_dir(),
        }
    }
}

fn default_chunk_size() -> usize {
    512
}

fn default_chunk_overlap() -> usize {
    64
}

fn default_top_k() -> usize {
    5
}

fn default_groundedness_threshold() -> f64 {
    0.6
}

fn default_collections_dir() -> String {
    ".rustant/ml/collections".to_string()
}

/// Evaluation framework configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfig {
    /// Trace storage database path.
    #[serde(default = "default_trace_db")]
    pub trace_db_path: String,
    /// Maximum stored traces before eviction.
    #[serde(default = "default_max_traces")]
    pub max_traces: usize,
    /// Enable LLM-as-Judge evaluator.
    #[serde(default)]
    pub llm_judge_enabled: bool,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            trace_db_path: default_trace_db(),
            max_traces: default_max_traces(),
            llm_judge_enabled: false,
        }
    }
}

fn default_trace_db() -> String {
    ".rustant/ml/traces.db".to_string()
}

fn default_max_traces() -> usize {
    10_000
}

/// Inference serving configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceServingConfig {
    /// Default serving port.
    #[serde(default = "default_serve_port")]
    pub default_port: u16,
    /// Maximum concurrent inference requests.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,
    /// Default request timeout in seconds.
    #[serde(default = "default_inference_timeout")]
    pub request_timeout_secs: u64,
}

impl Default for InferenceServingConfig {
    fn default() -> Self {
        Self {
            default_port: default_serve_port(),
            max_concurrent_requests: default_max_concurrent(),
            request_timeout_secs: default_inference_timeout(),
        }
    }
}

fn default_serve_port() -> u16 {
    8080
}

fn default_max_concurrent() -> usize {
    16
}

fn default_inference_timeout() -> u64 {
    30
}

/// Research tools configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchConfig {
    /// Papers With Code API integration.
    #[serde(default)]
    pub papers_with_code_enabled: bool,
    /// Research state directory.
    #[serde(default = "default_research_dir")]
    pub research_dir: String,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            papers_with_code_enabled: false,
            research_dir: default_research_dir(),
        }
    }
}

fn default_research_dir() -> String {
    ".rustant/ml/research".to_string()
}

/// AI safety pillar configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSafetyConfig {
    /// Toxicity detection threshold (0.0-1.0).
    #[serde(default = "default_toxicity_threshold")]
    pub toxicity_threshold: f32,
    /// Enable bias detection in model outputs.
    #[serde(default = "default_true")]
    pub bias_detection_enabled: bool,
    /// Enable red team testing.
    #[serde(default)]
    pub red_team_enabled: bool,
    /// Maximum number of red team attacks per campaign.
    #[serde(default = "default_max_attacks")]
    pub max_red_team_attacks: usize,
}

impl Default for AiSafetyConfig {
    fn default() -> Self {
        Self {
            toxicity_threshold: default_toxicity_threshold(),
            bias_detection_enabled: true,
            red_team_enabled: false,
            max_red_team_attacks: default_max_attacks(),
        }
    }
}

fn default_toxicity_threshold() -> f32 {
    0.7
}

fn default_max_attacks() -> usize {
    100
}

/// Resource limits for ML operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimitsConfig {
    /// Maximum GPU memory in GB (0 = unlimited).
    #[serde(default)]
    pub max_gpu_memory_gb: f64,
    /// Maximum CPU cores to use (0 = all available).
    #[serde(default)]
    pub max_cpu_cores: usize,
    /// Maximum disk space for ML artifacts in GB.
    #[serde(default = "default_max_disk_gb")]
    pub max_disk_gb: f64,
}

impl Default for ResourceLimitsConfig {
    fn default() -> Self {
        Self {
            max_gpu_memory_gb: 0.0,
            max_cpu_cores: 0,
            max_disk_gb: default_max_disk_gb(),
        }
    }
}

fn default_max_disk_gb() -> f64 {
    100.0
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ml_config() {
        let config = MlConfig::default();
        assert_eq!(config.python.timeout_secs, 300);
        assert!(config.data.auto_pii_scan);
        assert_eq!(config.data.quality_gate_threshold, 0.7);
        assert!(config.training.anomaly_detection);
        assert_eq!(config.rag.default_top_k, 5);
        assert!(config.rag.groundedness_check);
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let config = MlConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: MlConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.python.timeout_secs, config.python.timeout_secs);
        assert_eq!(parsed.rag.default_top_k, config.rag.default_top_k);
    }
}
