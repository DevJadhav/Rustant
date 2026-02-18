//! Pluggable embedding providers for semantic search.
//!
//! Provides a trait-based abstraction over embedding models, with implementations
//! for local TF-IDF (always available), FastEmbed (optional, behind `semantic-search` feature),
//! OpenAI API, and Ollama API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait for embedding providers.
pub trait Embedder: Send + Sync {
    /// Generate an embedding for a single text.
    fn embed(&self, text: &str) -> Vec<f32>;

    /// Generate embeddings for a batch of texts.
    fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Return the dimensionality of embeddings.
    fn dimensions(&self) -> usize;

    /// Return the provider name.
    fn provider_name(&self) -> &str;
}

/// Configuration for embedding providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Provider name: "local" (default), "fastembed", "openai", "ollama"
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Provider-specific model name.
    #[serde(default)]
    pub model: Option<String>,
    /// Embedding dimensions (auto-detected from provider if 0).
    #[serde(default)]
    pub dimensions: usize,
    /// Batch size for bulk embedding operations.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Whether to cache embeddings to disk.
    #[serde(default = "default_cache_embeddings")]
    pub cache_embeddings: bool,
}

fn default_provider() -> String {
    "local".into()
}

fn default_batch_size() -> usize {
    32
}

fn default_cache_embeddings() -> bool {
    true
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "local".into(),
            model: None,
            dimensions: 0,
            batch_size: 32,
            cache_embeddings: true,
        }
    }
}

/// Local TF-IDF embedder (always available, no external dependencies).
/// This uses the same algorithm as `SimpleEmbedder` in `search.rs`.
#[derive(Debug, Clone)]
pub struct LocalEmbedder {
    dimensions: usize,
}

impl LocalEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

/// Hash function matching `simple_hash` in `search.rs`.
fn simple_hash(s: &str) -> usize {
    let mut hash: usize = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as usize);
    }
    hash
}

impl Embedder for LocalEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        // Same algorithm as SimpleEmbedder in search.rs
        let mut vector = vec![0.0f32; self.dimensions];

        let lowered = text.to_lowercase();
        let words: Vec<&str> = lowered
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();

        if words.is_empty() {
            return vector;
        }

        // Count term frequency
        let mut tf: HashMap<&str, usize> = HashMap::new();
        for word in &words {
            *tf.entry(word).or_insert(0) += 1;
        }

        // Hash each unique term into a dimension
        for (term, count) in &tf {
            let idx = simple_hash(term) % self.dimensions;
            vector[idx] += *count as f32;
        }

        // L2 normalize
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vector {
                *v /= norm;
            }
        }

        vector
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "local"
    }
}

/// FastEmbed embedder (behind `semantic-search` feature flag).
/// Uses the `fastembed` crate with local ONNX models.
#[cfg(feature = "semantic-search")]
pub struct FastEmbedder {
    model: fastembed::TextEmbedding,
    dims: usize,
}

#[cfg(feature = "semantic-search")]
impl FastEmbedder {
    pub fn new(model_name: Option<&str>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        let model_enum = match model_name {
            Some("all-MiniLM-L6-v2") | None => EmbeddingModel::AllMiniLML6V2,
            Some("bge-small-en-v1.5") => EmbeddingModel::BGESmallENV15,
            Some("bge-base-en-v1.5") => EmbeddingModel::BGEBaseENV15,
            Some(other) => {
                tracing::warn!(
                    "Unknown fastembed model '{}', falling back to AllMiniLML6V2",
                    other
                );
                EmbeddingModel::AllMiniLML6V2
            }
        };

        let model = TextEmbedding::try_new(InitOptions {
            model_name: model_enum,
            show_download_progress: true,
            ..Default::default()
        })?;

        // Detect dimensions from a test embedding
        let test = model.embed(vec!["test"], None)?;
        let dims = test.first().map(|v| v.len()).unwrap_or(384);

        Ok(Self { model, dims })
    }
}

#[cfg(feature = "semantic-search")]
impl Embedder for FastEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        match self.model.embed(vec![text], None) {
            Ok(embeddings) => embeddings
                .into_iter()
                .next()
                .unwrap_or_else(|| vec![0.0; self.dims]),
            Err(e) => {
                tracing::warn!("FastEmbed error: {}, returning zero vector", e);
                vec![0.0; self.dims]
            }
        }
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let texts_ref: Vec<&str> = texts_owned.iter().map(|s| s.as_str()).collect();
        match self.model.embed(texts_ref, None) {
            Ok(embeddings) => embeddings,
            Err(e) => {
                tracing::warn!("FastEmbed batch error: {}", e);
                texts.iter().map(|_| vec![0.0; self.dims]).collect()
            }
        }
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn provider_name(&self) -> &str {
        "fastembed"
    }
}

/// OpenAI API embedder (uses text-embedding-3-small by default).
pub struct OpenAiEmbedder {
    client: reqwest::Client,
    api_key: String,
    model: String,
    dims: usize,
    base_url: String,
}

impl OpenAiEmbedder {
    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "text-embedding-3-small".into());
        let dims = match model.as_str() {
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            "text-embedding-ada-002" => 1536,
            _ => 1536,
        };
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            dims,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com".into()),
        }
    }

    fn embed_sync(&self, text: &str) -> Vec<f32> {
        // Use a blocking approach since the Embedder trait is sync
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                let api_key = self.api_key.clone();
                let model = self.model.clone();
                let base_url = self.base_url.clone();
                let text = text.to_string();
                let client = self.client.clone();
                let dims = self.dims;

                // Spawn a blocking task to avoid blocking the async runtime
                std::thread::scope(|s| {
                    s.spawn(|| {
                        handle.block_on(async {
                            Self::embed_api_call(&client, &api_key, &model, &base_url, &text, dims)
                                .await
                        })
                    })
                    .join()
                    .unwrap_or_else(|_| vec![0.0; dims])
                })
            }
            Err(_) => {
                // No runtime available, return zero vector
                tracing::warn!("No tokio runtime available for OpenAI embedding");
                vec![0.0; self.dims]
            }
        }
    }

    async fn embed_api_call(
        client: &reqwest::Client,
        api_key: &str,
        model: &str,
        base_url: &str,
        text: &str,
        dims: usize,
    ) -> Vec<f32> {
        let url = format!("{}/v1/embeddings", base_url);
        let body = serde_json::json!({
            "model": model,
            "input": text,
        });

        match client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>().await
                    && let Some(embedding) = json["data"][0]["embedding"].as_array()
                {
                    return embedding
                        .iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect();
                }
                vec![0.0; dims]
            }
            Err(e) => {
                tracing::warn!("OpenAI embedding error: {}", e);
                vec![0.0; dims]
            }
        }
    }
}

impl Embedder for OpenAiEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        self.embed_sync(text)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn provider_name(&self) -> &str {
        "openai"
    }
}

/// Ollama embedder (uses local Ollama API).
pub struct OllamaEmbedder {
    client: reqwest::Client,
    model: String,
    dims: usize,
    base_url: String,
}

impl OllamaEmbedder {
    pub fn new(model: Option<String>, base_url: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "nomic-embed-text".into());
        let dims = match model.as_str() {
            "nomic-embed-text" => 768,
            "mxbai-embed-large" => 1024,
            "all-minilm" => 384,
            _ => 768,
        };
        Self {
            client: reqwest::Client::new(),
            model,
            dims,
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".into()),
        }
    }

    fn embed_sync(&self, text: &str) -> Vec<f32> {
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                let client = self.client.clone();
                let model = self.model.clone();
                let base_url = self.base_url.clone();
                let text = text.to_string();
                let dims = self.dims;

                std::thread::scope(|s| {
                    s.spawn(|| {
                        handle.block_on(async {
                            Self::embed_api_call(&client, &model, &base_url, &text, dims).await
                        })
                    })
                    .join()
                    .unwrap_or_else(|_| vec![0.0; dims])
                })
            }
            Err(_) => {
                tracing::warn!("No tokio runtime available for Ollama embedding");
                vec![0.0; self.dims]
            }
        }
    }

    async fn embed_api_call(
        client: &reqwest::Client,
        model: &str,
        base_url: &str,
        text: &str,
        dims: usize,
    ) -> Vec<f32> {
        let url = format!("{}/api/embed", base_url);
        let body = serde_json::json!({
            "model": model,
            "input": text,
        });

        match client.post(&url).json(&body).send().await {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>().await
                    && let Some(embedding) = json["embeddings"][0].as_array()
                {
                    return embedding
                        .iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect();
                }
                vec![0.0; dims]
            }
            Err(e) => {
                tracing::warn!("Ollama embedding error: {}", e);
                vec![0.0; dims]
            }
        }
    }
}

impl Embedder for OllamaEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        self.embed_sync(text)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }
}

/// Factory function to create an embedder based on configuration.
pub fn create_embedder(config: &EmbeddingConfig) -> Box<dyn Embedder> {
    match config.provider.as_str() {
        #[cfg(feature = "semantic-search")]
        "fastembed" => match FastEmbedder::new(config.model.as_deref()) {
            Ok(embedder) => Box::new(embedder),
            Err(e) => {
                tracing::warn!(
                    "Failed to create FastEmbedder: {}, falling back to local",
                    e
                );
                let dims = if config.dimensions > 0 {
                    config.dimensions
                } else {
                    128
                };
                Box::new(LocalEmbedder::new(dims))
            }
        },
        #[cfg(not(feature = "semantic-search"))]
        "fastembed" => {
            tracing::warn!(
                "FastEmbed requested but 'semantic-search' feature is not enabled, falling back to local"
            );
            let dims = if config.dimensions > 0 {
                config.dimensions
            } else {
                128
            };
            Box::new(LocalEmbedder::new(dims))
        }
        "openai" => {
            let api_key =
                crate::providers::resolve_api_key_by_env("OPENAI_API_KEY").unwrap_or_default();
            if api_key.is_empty() {
                tracing::warn!("OPENAI_API_KEY not set, falling back to local embedder");
                let dims = if config.dimensions > 0 {
                    config.dimensions
                } else {
                    128
                };
                Box::new(LocalEmbedder::new(dims))
            } else {
                Box::new(OpenAiEmbedder::new(api_key, config.model.clone(), None))
            }
        }
        "ollama" => Box::new(OllamaEmbedder::new(config.model.clone(), None)),
        _ => {
            // Default: local TF-IDF
            let dims = if config.dimensions > 0 {
                config.dimensions
            } else {
                128
            };
            Box::new(LocalEmbedder::new(dims))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_embedder_dimensions() {
        let embedder = LocalEmbedder::new(128);
        assert_eq!(embedder.dimensions(), 128);
        let v = embedder.embed("hello world");
        assert_eq!(v.len(), 128);
    }

    #[test]
    fn test_local_embedder_normalized() {
        let embedder = LocalEmbedder::new(128);
        let v = embedder.embed("test input text for normalization");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "Expected normalized vector, got norm={}",
            norm
        );
    }

    #[test]
    fn test_local_embedder_empty_text() {
        let embedder = LocalEmbedder::new(128);
        let v = embedder.embed("");
        assert_eq!(v.len(), 128);
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_local_embedder_deterministic() {
        let embedder = LocalEmbedder::new(128);
        let v1 = embedder.embed("same text");
        let v2 = embedder.embed("same text");
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_local_embedder_different_texts_differ() {
        let embedder = LocalEmbedder::new(128);
        let v1 = embedder.embed("hello world");
        let v2 = embedder.embed("goodbye universe");
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_embed_batch_default() {
        let embedder = LocalEmbedder::new(64);
        let texts = &["hello", "world", "test"];
        let embeddings = embedder.embed_batch(texts);
        assert_eq!(embeddings.len(), 3);
        for emb in &embeddings {
            assert_eq!(emb.len(), 64);
        }
    }

    #[test]
    fn test_embedder_trait_object() {
        let embedder: Box<dyn Embedder> = Box::new(LocalEmbedder::new(128));
        assert_eq!(embedder.dimensions(), 128);
        assert_eq!(embedder.provider_name(), "local");
        let v = embedder.embed("test");
        assert_eq!(v.len(), 128);
    }

    #[test]
    fn test_embedding_config_defaults() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider, "local");
        assert!(config.model.is_none());
        assert_eq!(config.dimensions, 0);
        assert_eq!(config.batch_size, 32);
        assert!(config.cache_embeddings);
    }

    #[test]
    fn test_embedding_config_serde_roundtrip() {
        let config = EmbeddingConfig {
            provider: "openai".into(),
            model: Some("text-embedding-3-small".into()),
            dimensions: 1536,
            batch_size: 64,
            cache_embeddings: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EmbeddingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.provider, "openai");
        assert_eq!(deserialized.dimensions, 1536);
    }

    #[test]
    fn test_embedding_config_deserialize_empty() {
        let config: EmbeddingConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.provider, "local");
        assert_eq!(config.batch_size, 32);
    }

    #[test]
    fn test_create_embedder_default() {
        let config = EmbeddingConfig::default();
        let embedder = create_embedder(&config);
        assert_eq!(embedder.provider_name(), "local");
        assert_eq!(embedder.dimensions(), 128);
    }

    #[test]
    fn test_create_embedder_explicit_local() {
        let config = EmbeddingConfig {
            provider: "local".into(),
            dimensions: 256,
            ..Default::default()
        };
        let embedder = create_embedder(&config);
        assert_eq!(embedder.dimensions(), 256);
    }

    #[test]
    fn test_create_embedder_openai_no_key() {
        // Without OPENAI_API_KEY, should fall back to local
        unsafe { std::env::remove_var("OPENAI_API_KEY") };
        let config = EmbeddingConfig {
            provider: "openai".into(),
            ..Default::default()
        };
        let embedder = create_embedder(&config);
        // Falls back to local when no API key
        assert_eq!(embedder.provider_name(), "local");
    }

    #[test]
    fn test_create_embedder_fastembed_without_feature() {
        // Without the semantic-search feature, should fall back to local
        let config = EmbeddingConfig {
            provider: "fastembed".into(),
            ..Default::default()
        };
        let embedder = create_embedder(&config);
        // Falls back to local when feature is not enabled
        assert_eq!(embedder.provider_name(), "local");
    }

    #[test]
    fn test_ollama_embedder_dimensions() {
        let embedder = OllamaEmbedder::new(None, None);
        assert_eq!(embedder.dimensions(), 768); // nomic-embed-text default
    }

    #[test]
    fn test_openai_embedder_dimensions() {
        let embedder = OpenAiEmbedder::new("test-key".into(), None, None);
        assert_eq!(embedder.dimensions(), 1536); // text-embedding-3-small default
    }

    #[test]
    fn test_local_embedder_matches_simple_embedder() {
        // Verify that LocalEmbedder produces the same output as SimpleEmbedder in search.rs
        use crate::search::SimpleEmbedder;

        let local = LocalEmbedder::new(128);
        let simple = SimpleEmbedder::new(128);

        let texts = &[
            "hello world",
            "rust programming language",
            "the quick brown fox",
            "",
            "single",
        ];

        for text in texts {
            let local_vec = local.embed(text);
            let simple_vec = simple.embed(text);
            assert_eq!(
                local_vec, simple_vec,
                "LocalEmbedder and SimpleEmbedder differ for text: '{}'",
                text
            );
        }
    }
}
