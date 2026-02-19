//! llama.cpp server backend.

use super::{InferenceBackend, LocalModel, ServingConfig, ServingInstance};
use crate::error::MlError;
use async_trait::async_trait;

pub struct LlamaCppBackend {
    pub server_path: Option<std::path::PathBuf>,
}

impl Default for LlamaCppBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl LlamaCppBackend {
    pub fn new() -> Self {
        Self { server_path: None }
    }
}

#[async_trait]
impl InferenceBackend for LlamaCppBackend {
    fn name(&self) -> &str {
        "llamacpp"
    }
    async fn is_available(&self) -> bool {
        tokio::process::Command::new("llama-server")
            .arg("--help")
            .output()
            .await
            .is_ok()
    }
    async fn list_models(&self) -> Result<Vec<LocalModel>, MlError> {
        Ok(Vec::new())
    }

    async fn start_serving(
        &self,
        model: &LocalModel,
        config: &ServingConfig,
    ) -> Result<ServingInstance, MlError> {
        let endpoint = format!("http://{}:{}", config.host, config.port);
        Ok(ServingInstance {
            model_name: model.name.clone(),
            backend: "llamacpp".to_string(),
            endpoint,
            pid: None,
            status: crate::inference::serving::ServingStatus::Running,
            started_at: Some(chrono::Utc::now()),
            requests_served: 0,
        })
    }

    async fn stop_serving(&self, _instance: &ServingInstance) -> Result<(), MlError> {
        Ok(())
    }
}
