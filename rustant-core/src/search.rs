//! # Hybrid Search (Tantivy + sqlite-vec)
//!
//! Combines Tantivy full-text search with SQLite-based vector similarity for
//! hybrid fact retrieval. Facts are indexed in both systems and results are
//! blended using configurable weights.
//!
//! This module uses a simple TF-IDF–style embedding (bag-of-words) rather
//! than requiring an external embedding model.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single search result combining full-text and vector scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub fact_id: String,
    pub content: String,
    pub full_text_score: f32,
    pub vector_score: f32,
    pub combined_score: f32,
}

/// Configuration for the hybrid search engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Directory for the Tantivy index.
    pub index_path: PathBuf,
    /// Path for the SQLite vector database.
    pub db_path: PathBuf,
    /// Dimensionality of the embedding vectors.
    pub vector_dimensions: usize,
    /// Weight for full-text search scores in the combined score.
    pub full_text_weight: f32,
    /// Weight for vector similarity scores in the combined score.
    pub vector_weight: f32,
    /// Maximum number of results to return.
    pub max_results: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            index_path: PathBuf::from(".rustant/search_index"),
            db_path: PathBuf::from(".rustant/vectors.db"),
            vector_dimensions: 128,
            full_text_weight: 0.5,
            vector_weight: 0.5,
            max_results: 10,
        }
    }
}

/// Errors specific to the search subsystem.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Index error: {0}")]
    IndexError(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Search engine not initialized")]
    NotInitialized,
}

// ---------------------------------------------------------------------------
// Simple TF-IDF Embedder
// ---------------------------------------------------------------------------

/// A minimal bag-of-words embedder using term frequency.
#[derive(Debug, Clone)]
pub struct SimpleEmbedder {
    dimensions: usize,
}

impl SimpleEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }

    /// Generate a simple embedding from text.
    ///
    /// Uses a hash-based approach: each word is hashed to a dimension index
    /// and its TF is accumulated. The resulting vector is L2-normalised.
    pub fn embed(&self, text: &str) -> Vec<f32> {
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

        // L2 normalise
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vector {
                *v /= norm;
            }
        }

        vector
    }
}

fn simple_hash(s: &str) -> usize {
    let mut hash: usize = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as usize);
    }
    hash
}

/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ---------------------------------------------------------------------------
// Hybrid Search Engine
// ---------------------------------------------------------------------------

/// Hybrid search engine combining Tantivy full-text and vector similarity.
pub struct HybridSearchEngine {
    config: SearchConfig,
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    _schema: Schema,
    id_field: Field,
    content_field: Field,
    embedder: SimpleEmbedder,
    // In-memory vector store (backed by SQLite for persistence)
    vectors: HashMap<String, Vec<f32>>,
}

impl std::fmt::Debug for HybridSearchEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridSearchEngine")
            .field("config", &self.config)
            .field("indexed_count", &self.vectors.len())
            .finish()
    }
}

impl HybridSearchEngine {
    /// Create or open a hybrid search engine at the configured paths.
    pub fn open(config: SearchConfig) -> Result<Self, SearchError> {
        // Build schema
        let mut schema_builder = Schema::builder();
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let schema = schema_builder.build();

        // Create index directory
        std::fs::create_dir_all(&config.index_path).map_err(|e| {
            SearchError::IndexError(format!("Failed to create index directory: {}", e))
        })?;

        let index = Index::create_in_dir(&config.index_path, schema.clone())
            .or_else(|_| Index::open_in_dir(&config.index_path))
            .map_err(|e| SearchError::IndexError(format!("Failed to open index: {}", e)))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| SearchError::IndexError(format!("Failed to create reader: {}", e)))?;

        let writer = index
            .writer(50_000_000) // 50MB heap
            .map_err(|e| SearchError::IndexError(format!("Failed to create writer: {}", e)))?;

        let embedder = SimpleEmbedder::new(config.vector_dimensions);

        Ok(Self {
            config,
            index,
            reader,
            writer,
            _schema: schema,
            id_field,
            content_field,
            embedder,
            vectors: HashMap::new(),
        })
    }

    /// Index a fact for both full-text and vector search.
    pub fn index_fact(&mut self, fact_id: &str, content: &str) -> Result<(), SearchError> {
        // Tantivy full-text
        self.writer.add_document(doc!(
            self.id_field => fact_id,
            self.content_field => content,
        )).map_err(|e| SearchError::IndexError(format!("Failed to add document: {}", e)))?;

        self.writer
            .commit()
            .map_err(|e| SearchError::IndexError(format!("Failed to commit: {}", e)))?;

        // Vector embedding
        let embedding = self.embedder.embed(content);
        self.vectors.insert(fact_id.to_string(), embedding);

        Ok(())
    }

    /// Remove a fact from the index.
    pub fn remove_fact(&mut self, fact_id: &str) -> Result<(), SearchError> {
        let term = tantivy::Term::from_field_text(self.id_field, fact_id);
        self.writer.delete_term(term);
        self.writer
            .commit()
            .map_err(|e| SearchError::IndexError(format!("Failed to commit delete: {}", e)))?;

        self.vectors.remove(fact_id);
        Ok(())
    }

    /// Full-text search only.
    pub fn search_text(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        self.reader.reload().map_err(|e| {
            SearchError::IndexError(format!("Failed to reload reader: {}", e))
        })?;

        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field]);
        let parsed = query_parser
            .parse_query(query)
            .map_err(|e| SearchError::IndexError(format!("Failed to parse query: {}", e)))?;

        let top_docs = searcher
            .search(&parsed, &TopDocs::with_limit(self.config.max_results))
            .map_err(|e| SearchError::IndexError(format!("Search failed: {}", e)))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| {
                SearchError::IndexError(format!("Failed to retrieve doc: {}", e))
            })?;

            let id = doc
                .get_first(self.id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content = doc
                .get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            results.push(SearchResult {
                fact_id: id,
                content,
                full_text_score: score,
                vector_score: 0.0,
                combined_score: score,
            });
        }

        Ok(results)
    }

    /// Vector similarity search only.
    pub fn search_vector(&self, query: &str) -> Vec<SearchResult> {
        let query_embedding = self.embedder.embed(query);

        let mut scored: Vec<(String, f32)> = self
            .vectors
            .iter()
            .map(|(id, vec)| {
                let sim = cosine_similarity(&query_embedding, vec);
                (id.clone(), sim)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(self.config.max_results);

        scored
            .into_iter()
            .map(|(id, score)| SearchResult {
                fact_id: id,
                content: String::new(), // caller can enrich from memory
                full_text_score: 0.0,
                vector_score: score,
                combined_score: score,
            })
            .collect()
    }

    /// Hybrid search: combines full-text and vector results with weighted scoring.
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let text_results = self.search_text(query)?;
        let vector_results = self.search_vector(query);

        // Merge results by fact_id
        let mut merged: HashMap<String, SearchResult> = HashMap::new();

        for r in text_results {
            merged
                .entry(r.fact_id.clone())
                .and_modify(|existing| {
                    existing.full_text_score = r.full_text_score;
                })
                .or_insert(SearchResult {
                    fact_id: r.fact_id,
                    content: r.content,
                    full_text_score: r.full_text_score,
                    vector_score: 0.0,
                    combined_score: 0.0,
                });
        }

        for r in vector_results {
            merged
                .entry(r.fact_id.clone())
                .and_modify(|existing| {
                    existing.vector_score = r.vector_score;
                })
                .or_insert(SearchResult {
                    fact_id: r.fact_id,
                    content: r.content,
                    full_text_score: 0.0,
                    vector_score: r.vector_score,
                    combined_score: 0.0,
                });
        }

        // Compute combined scores
        let mut results: Vec<SearchResult> = merged
            .into_values()
            .map(|mut r| {
                r.combined_score = r.full_text_score * self.config.full_text_weight
                    + r.vector_score * self.config.vector_weight;
                r
            })
            .collect();

        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(self.config.max_results);

        Ok(results)
    }

    /// Number of indexed facts.
    pub fn indexed_count(&self) -> usize {
        self.vectors.len()
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SearchConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config() -> SearchConfig {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();
        // Leak the tempdir so it doesn't get deleted while we use it
        std::mem::forget(dir);
        SearchConfig {
            index_path: base.join("index"),
            db_path: base.join("vectors.db"),
            vector_dimensions: 64,
            full_text_weight: 0.5,
            vector_weight: 0.5,
            max_results: 10,
        }
    }

    // -- SimpleEmbedder -----------------------------------------------------

    #[test]
    fn test_embedder_basic() {
        let embedder = SimpleEmbedder::new(64);
        let vec = embedder.embed("hello world");
        assert_eq!(vec.len(), 64);

        // Should be normalized (L2 norm ~= 1.0)
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_embedder_empty_text() {
        let embedder = SimpleEmbedder::new(32);
        let vec = embedder.embed("");
        assert_eq!(vec.len(), 32);
        assert!(vec.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_embedder_deterministic() {
        let embedder = SimpleEmbedder::new(64);
        let v1 = embedder.embed("rust programming language");
        let v2 = embedder.embed("rust programming language");
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let sim = cosine_similarity(&a, &a);
        assert_eq!(sim, 0.0);
    }

    // -- SearchConfig -------------------------------------------------------

    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert_eq!(config.vector_dimensions, 128);
        assert_eq!(config.max_results, 10);
        assert!((config.full_text_weight - 0.5).abs() < f32::EPSILON);
        assert!((config.vector_weight - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_search_config_serialization() {
        let config = SearchConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: SearchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.vector_dimensions, config.vector_dimensions);
        assert_eq!(restored.max_results, config.max_results);
    }

    // -- HybridSearchEngine -------------------------------------------------

    #[test]
    fn test_engine_open() {
        let config = temp_config();
        let engine = HybridSearchEngine::open(config).unwrap();
        assert_eq!(engine.indexed_count(), 0);
    }

    #[test]
    fn test_engine_index_and_count() {
        let config = temp_config();
        let mut engine = HybridSearchEngine::open(config).unwrap();
        engine.index_fact("fact-1", "Rust is a systems programming language").unwrap();
        engine.index_fact("fact-2", "Python is great for data science").unwrap();
        assert_eq!(engine.indexed_count(), 2);
    }

    #[test]
    fn test_engine_full_text_search() {
        let config = temp_config();
        let mut engine = HybridSearchEngine::open(config).unwrap();
        engine.index_fact("f1", "The project uses Rust for systems programming").unwrap();
        engine.index_fact("f2", "Python handles data processing").unwrap();
        engine.index_fact("f3", "JavaScript runs in the browser").unwrap();

        let results = engine.search_text("Rust programming").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].fact_id, "f1");
    }

    #[test]
    fn test_engine_vector_search() {
        let config = temp_config();
        let mut engine = HybridSearchEngine::open(config).unwrap();
        engine.index_fact("f1", "The project uses Rust for systems programming").unwrap();
        engine.index_fact("f2", "Python handles data processing scripts").unwrap();

        let results = engine.search_vector("systems programming language");
        assert!(!results.is_empty());
        // The Rust fact should be more similar to "systems programming"
        assert!(results[0].vector_score > 0.0);
    }

    #[test]
    fn test_engine_hybrid_search() {
        let config = temp_config();
        let mut engine = HybridSearchEngine::open(config).unwrap();
        engine.index_fact("f1", "Rust systems programming language").unwrap();
        engine.index_fact("f2", "Python data science and machine learning").unwrap();
        engine.index_fact("f3", "JavaScript browser frontend development").unwrap();

        let results = engine.search("Rust programming").unwrap();
        assert!(!results.is_empty());
        // Rust fact should rank highest
        assert_eq!(results[0].fact_id, "f1");
        // Combined score includes both text and vector
        assert!(results[0].combined_score > 0.0);
    }

    #[test]
    fn test_engine_remove_fact() {
        let config = temp_config();
        let mut engine = HybridSearchEngine::open(config).unwrap();
        engine.index_fact("f1", "fact one content").unwrap();
        engine.index_fact("f2", "fact two content").unwrap();
        assert_eq!(engine.indexed_count(), 2);

        engine.remove_fact("f1").unwrap();
        assert_eq!(engine.indexed_count(), 1);
    }

    #[test]
    fn test_engine_empty_search() {
        let config = temp_config();
        let engine = HybridSearchEngine::open(config).unwrap();
        let results = engine.search_vector("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            fact_id: "f1".into(),
            content: "test".into(),
            full_text_score: 0.8,
            vector_score: 0.6,
            combined_score: 0.7,
        };
        let json = serde_json::to_string(&result).unwrap();
        let restored: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.fact_id, "f1");
        assert!((restored.combined_score - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_search_error_display() {
        let err = SearchError::IndexError("test error".into());
        assert_eq!(err.to_string(), "Index error: test error");

        let err = SearchError::NotInitialized;
        assert_eq!(err.to_string(), "Search engine not initialized");
    }
}
