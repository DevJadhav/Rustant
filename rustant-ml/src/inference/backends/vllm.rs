//! vLLM inference backend.

use super::{InferenceBackend, LocalModel, ServingConfig, ServingInstance};
use crate::error::MlError;
use async_trait::async_trait;

pub struct VllmBackend {
    pub api_url: String,
}

impl VllmBackend {
    pub fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.to_string(),
        }
    }
}

#[async_trait]
impl InferenceBackend for VllmBackend {
    fn name(&self) -> &str {
        "vllm"
    }
    async fn is_available(&self) -> bool {
        reqwest::get(format!("{}/health", self.api_url))
            .await
            .is_ok_and(|r| r.status().is_success())
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
            backend: "vllm".to_string(),
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
