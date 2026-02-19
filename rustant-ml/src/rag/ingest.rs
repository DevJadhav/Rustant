//! Document ingestion for RAG.

use crate::error::MlError;
use crate::rag::chunk::{Chunk, ChunkingStrategy, chunk_text};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Ingested document metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedDocument {
    pub id: String,
    pub path: PathBuf,
    pub title: Option<String>,
    pub chunk_count: usize,
    pub total_chars: usize,
    pub ingested_at: chrono::DateTime<chrono::Utc>,
}

/// Document ingestor.
pub struct DocumentIngestor {
    strategy: ChunkingStrategy,
}

impl DocumentIngestor {
    pub fn new(strategy: ChunkingStrategy) -> Self {
        Self { strategy }
    }

    /// Ingest a single file.
    pub async fn ingest_file(
        &self,
        path: &Path,
    ) -> Result<(IngestedDocument, Vec<Chunk>), MlError> {
        let content = tokio::fs::read_to_string(path).await?;
        let doc_id = uuid::Uuid::new_v4().to_string();
        let chunks = chunk_text(&content, &doc_id, &self.strategy);
        let doc = IngestedDocument {
            id: doc_id,
            path: path.to_path_buf(),
            title: path.file_name().map(|n| n.to_string_lossy().to_string()),
            chunk_count: chunks.len(),
            total_chars: content.len(),
            ingested_at: chrono::Utc::now(),
        };
        Ok((doc, chunks))
    }

    /// Ingest all supported files in a directory.
    pub async fn ingest_directory(
        &self,
        dir: &Path,
    ) -> Result<Vec<(IngestedDocument, Vec<Chunk>)>, MlError> {
        let mut results = Vec::new();
        let supported = [
            "txt", "md", "rs", "py", "js", "ts", "html", "json", "yaml", "toml",
        ];

        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    if supported.contains(&ext) {
                        match self.ingest_file(entry.path()).await {
                            Ok(result) => results.push(result),
                            Err(e) => {
                                tracing::warn!(path = %entry.path().display(), error = %e, "Skipping file")
                            }
                        }
                    }
                }
            }
        }
        Ok(results)
    }
}
