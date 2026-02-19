//! Inference backend trait and implementations.

pub mod candle;
pub mod llamacpp;
pub mod ollama;
pub mod vllm;

use crate::error::MlError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use super::serving::{ServingConfig, ServingInstance};

/// A local model available for serving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModel {
    pub name: String,
    pub path: Option<std::path::PathBuf>,
    pub format: super::formats::ModelFormat,
    pub size_bytes: Option<u64>,
    pub parameters: Option<String>,
}

/// Inference backend trait.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn is_available(&self) -> bool;
    async fn list_models(&self) -> Result<Vec<LocalModel>, MlError>;
    async fn start_serving(
        &self,
        model: &LocalModel,
        config: &ServingConfig,
    ) -> Result<ServingInstance, MlError>;
    async fn stop_serving(&self, instance: &ServingInstance) -> Result<(), MlError>;

    /// Return the model formats supported by this backend (e.g. "gguf", "safetensors").
    fn supported_formats(&self) -> Vec<String> {
        Vec::new()
    }
}
