//! Model downloader — HuggingFace, Ollama, URL.

use crate::error::MlError;
use std::path::PathBuf;

/// Model downloader.
pub struct ModelDownloader {
    cache_dir: PathBuf,
}

impl ModelDownloader {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Download a model from HuggingFace.
    pub async fn from_huggingface(
        &self,
        repo_id: &str,
        _revision: Option<&str>,
    ) -> Result<PathBuf, MlError> {
        let target = self
            .cache_dir
            .join("huggingface")
            .join(repo_id.replace('/', "_"));
        std::fs::create_dir_all(&target)?;
        // Actual download would use HF API
        tracing::info!(repo_id, "HuggingFace model download (stub)");
        Ok(target)
    }

    /// Pull a model via Ollama CLI.
    pub async fn from_ollama(&self, model_name: &str) -> Result<PathBuf, MlError> {
        let output = tokio::process::Command::new("ollama")
            .args(["pull", model_name])
            .output()
            .await
            .map_err(|e| MlError::model(format!("Ollama pull failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MlError::model(format!("Ollama pull failed: {stderr}")));
        }
        Ok(self.cache_dir.join("ollama").join(model_name))
    }

    /// Download from a URL.
    pub async fn from_url(&self, url: &str, filename: &str) -> Result<PathBuf, MlError> {
        let target = self.cache_dir.join(filename);
        let response = reqwest::get(url).await?;
        let bytes = response.bytes().await?;
        std::fs::write(&target, &bytes)?;
        Ok(target)
    }
}
