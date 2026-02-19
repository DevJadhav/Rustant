//! RAG (Retrieval-Augmented Generation) pipeline.

pub mod chunk;
pub mod collections;
pub mod context;
pub mod diagnostics;
pub mod evaluation;
pub mod grounding;
pub mod ingest;
pub mod pipeline;
pub mod reranker;
pub mod retriever;

pub use chunk::ChunkingStrategy;
pub use collections::CollectionManager;
pub use grounding::{GroundednessChecker, GroundednessReport};
pub use ingest::DocumentIngestor;
pub use pipeline::{RagPipeline, RagResponse};
pub use retriever::{RagRetriever, RetrieverConfig};
