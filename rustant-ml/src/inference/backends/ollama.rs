//! Ollama inference backend.

use super::{InferenceBackend, LocalModel, ServingConfig, ServingInstance};
use crate::error::MlError;
use crate::inference::formats::ModelFormat;
use async_trait::async_trait;

pub struct OllamaBackend;

#[async_trait]
impl InferenceBackend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new("ollama")
            .arg("list")
            .output()
            .await
            .is_ok_and(|o| o.status.success())
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>, MlError> {
        let output = tokio::process::Command::new("ollama")
            .arg("list")
            .output()
            .await
            .map_err(|e| MlError::inference(format!("Ollama list failed: {e}")))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let models: Vec<LocalModel> = stdout
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|name| LocalModel {
                    name: name.to_string(),
                    path: None,
                    format: ModelFormat::Gguf,
                    size_bytes: None,
                    parameters: parts.get(2).map(|s| s.to_string()),
                })
            })
            .collect();
        Ok(models)
    }

    async fn start_serving(
        &self,
        model: &LocalModel,
        config: &ServingConfig,
    ) -> Result<ServingInstance, MlError> {
        // Ollama serves models via `ollama run <model>` or its built-in API
        let endpoint = format!("http://{}:{}", config.host, 11434); // Ollama default port
        Ok(ServingInstance {
            model_name: model.name.clone(),
            backend: "ollama".to_string(),
            endpoint,
            pid: None, // Ollama manages its own process
            status: crate::inference::serving::ServingStatus::Running,
            started_at: Some(chrono::Utc::now()),
            requests_served: 0,
        })
    }

    async fn stop_serving(&self, _instance: &ServingInstance) -> Result<(), MlError> {
        // Ollama manages its own lifecycle; stop is a no-op
        Ok(())
    }
}
