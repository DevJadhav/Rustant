//! Candle native Rust inference backend (optional).
//!
//! Uses the candle crate for native Rust inference without Python dependencies.
//! This is a stub implementation — full candle integration requires the `candle` feature.

use super::{InferenceBackend, LocalModel, ServingConfig, ServingInstance};
use crate::error::MlError;
use async_trait::async_trait;

/// Native Rust inference backend using candle.
pub struct CandleBackend;

#[async_trait]
impl InferenceBackend for CandleBackend {
    fn name(&self) -> &str {
        "candle"
    }

    #[allow(unexpected_cfgs)]
    async fn is_available(&self) -> bool {
        // Candle is available when compiled with the candle feature
        cfg!(feature = "candle")
    }

    async fn list_models(&self) -> Result<Vec<LocalModel>, MlError> {
        // Candle loads models from SafeTensors/GGUF files directly
        Ok(Vec::new())
    }

    async fn start_serving(
        &self,
        model: &LocalModel,
        config: &ServingConfig,
    ) -> Result<ServingInstance, MlError> {
        if !self.is_available().await {
            return Err(MlError::inference(
                "Candle backend not available. Compile with `candle` feature to enable."
                    .to_string(),
            ));
        }
        let endpoint = format!("http://{}:{}", config.host, config.port);
        Ok(ServingInstance {
            model_name: model.name.clone(),
            backend: "candle".to_string(),
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

    fn supported_formats(&self) -> Vec<String> {
        vec!["safetensors".to_string(), "gguf".to_string()]
    }
}
