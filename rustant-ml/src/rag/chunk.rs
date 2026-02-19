//! Document chunking strategies.

use serde::{Deserialize, Serialize};

/// A document chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub text: String,
    pub chunk_index: usize,
    pub start_offset: usize,
    pub end_offset: usize,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Chunking strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChunkingStrategy {
    FixedSize {
        chunk_size: usize,
        overlap: usize,
    },
    Sentence {
        max_sentences: usize,
        overlap_sentences: usize,
    },
    Semantic {
        similarity_threshold: f32,
        max_chunk_size: usize,
    },
    Recursive {
        separators: Vec<String>,
        chunk_size: usize,
        overlap: usize,
    },
    Code {
        language: String,
    },
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self::Recursive {
            separators: vec!["\n\n".into(), "\n".into(), ". ".into(), " ".into()],
            chunk_size: 512,
            overlap: 64,
        }
    }
}

/// Chunk text using the specified strategy.
pub fn chunk_text(text: &str, doc_id: &str, strategy: &ChunkingStrategy) -> Vec<Chunk> {
    match strategy {
        ChunkingStrategy::FixedSize {
            chunk_size,
            overlap,
        } => chunk_fixed(text, doc_id, *chunk_size, *overlap),
        ChunkingStrategy::Recursive {
            separators,
            chunk_size,
            overlap,
        } => chunk_recursive(text, doc_id, separators, *chunk_size, *overlap),
        ChunkingStrategy::Sentence {
            max_sentences,
            overlap_sentences,
        } => chunk_sentence(text, doc_id, *max_sentences, *overlap_sentences),
        ChunkingStrategy::Semantic {
            similarity_threshold: _,
            max_chunk_size,
        } => chunk_semantic(text, doc_id, *max_chunk_size),
        ChunkingStrategy::Code { language } => chunk_code(text, doc_id, language),
    }
}

fn chunk_fixed(text: &str, doc_id: &str, size: usize, overlap: usize) -> Vec<Chunk> {
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut idx = 0;

    while start < chars.len() {
        let end = (start + size).min(chars.len());
        let chunk_text: String = chars[start..end].iter().collect();
        chunks.push(Chunk {
            id: format!("{doc_id}-chunk-{idx}"),
            document_id: doc_id.to_string(),
            text: chunk_text,
            chunk_index: idx,
            start_offset: start,
            end_offset: end,
            metadata: std::collections::HashMap::new(),
        });
        idx += 1;
        if end >= chars.len() {
            break;
        }
        start = end.saturating_sub(overlap);
    }
    chunks
}

fn chunk_recursive(
    text: &str,
    doc_id: &str,
    separators: &[String],
    chunk_size: usize,
    overlap: usize,
) -> Vec<Chunk> {
    // Simple implementation: split by first separator that produces chunks under size
    for sep in separators {
        let parts: Vec<&str> = text.split(sep.as_str()).collect();
        if parts.len() > 1 {
            let mut chunks = Vec::new();
            let mut current = String::new();
            let mut idx = 0;
            let mut start_offset = 0;

            for part in parts {
                if current.len() + part.len() + sep.len() > chunk_size && !current.is_empty() {
                    chunks.push(Chunk {
                        id: format!("{doc_id}-chunk-{idx}"),
                        document_id: doc_id.to_string(),
                        text: current.trim().to_string(),
                        chunk_index: idx,
                        start_offset,
                        end_offset: start_offset + current.len(),
                        metadata: std::collections::HashMap::new(),
                    });
                    idx += 1;
                    // Keep overlap
                    let overlap_text = if current.len() > overlap {
                        current[current.len() - overlap..].to_string()
                    } else {
                        String::new()
                    };
                    start_offset += current.len() - overlap_text.len();
                    current = overlap_text;
                }
                if !current.is_empty() {
                    current.push_str(sep);
                }
                current.push_str(part);
            }
            if !current.trim().is_empty() {
                chunks.push(Chunk {
                    id: format!("{doc_id}-chunk-{idx}"),
                    document_id: doc_id.to_string(),
                    text: current.trim().to_string(),
                    chunk_index: idx,
                    start_offset,
                    end_offset: start_offset + current.len(),
                    metadata: std::collections::HashMap::new(),
                });
            }
            return chunks;
        }
    }
    // Fallback to fixed size
    chunk_fixed(text, doc_id, chunk_size, overlap)
}

/// Split by sentences (". "), grouping up to `max_sentences` per chunk with overlap.
fn chunk_sentence(
    text: &str,
    doc_id: &str,
    max_sentences: usize,
    overlap_sentences: usize,
) -> Vec<Chunk> {
    let sentences: Vec<&str> = text
        .split(". ")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if sentences.is_empty() {
        return vec![Chunk {
            id: format!("{doc_id}-chunk-0"),
            document_id: doc_id.to_string(),
            text: text.to_string(),
            chunk_index: 0,
            start_offset: 0,
            end_offset: text.len(),
            metadata: std::collections::HashMap::new(),
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let mut idx = 0;

    while start < sentences.len() {
        let end = (start + max_sentences).min(sentences.len());
        let chunk_text = sentences[start..end].join(". ");
        let byte_start = text.find(sentences[start]).unwrap_or(0);
        let byte_end = (byte_start + chunk_text.len()).min(text.len());

        chunks.push(Chunk {
            id: format!("{doc_id}-chunk-{idx}"),
            document_id: doc_id.to_string(),
            text: chunk_text,
            chunk_index: idx,
            start_offset: byte_start,
            end_offset: byte_end,
            metadata: std::collections::HashMap::new(),
        });
        idx += 1;
        if end >= sentences.len() {
            break;
        }
        start = end.saturating_sub(overlap_sentences);
    }
    chunks
}

/// Semantic chunking: split by sentences, then merge adjacent sentences until
/// `max_chunk_size` is reached. This is a simple sentence-grouping approach
/// (not embedding-based).
fn chunk_semantic(text: &str, doc_id: &str, max_chunk_size: usize) -> Vec<Chunk> {
    let sentences: Vec<&str> = text
        .split(". ")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if sentences.is_empty() {
        return chunk_fixed(text, doc_id, max_chunk_size, 0);
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut idx = 0;
    let mut start_offset = 0;

    for sentence in &sentences {
        let candidate_len = if current.is_empty() {
            sentence.len()
        } else {
            current.len() + 2 + sentence.len() // ". " separator
        };

        if candidate_len > max_chunk_size && !current.is_empty() {
            // Emit current chunk.
            chunks.push(Chunk {
                id: format!("{doc_id}-chunk-{idx}"),
                document_id: doc_id.to_string(),
                text: current.clone(),
                chunk_index: idx,
                start_offset,
                end_offset: start_offset + current.len(),
                metadata: std::collections::HashMap::new(),
            });
            idx += 1;
            start_offset += current.len() + 2; // skip ". "
            current.clear();
        }

        if current.is_empty() {
            current.push_str(sentence);
        } else {
            current.push_str(". ");
            current.push_str(sentence);
        }
    }

    if !current.trim().is_empty() {
        chunks.push(Chunk {
            id: format!("{doc_id}-chunk-{idx}"),
            document_id: doc_id.to_string(),
            text: current.clone(),
            chunk_index: idx,
            start_offset,
            end_offset: start_offset + current.len(),
            metadata: std::collections::HashMap::new(),
        });
    }

    // If nothing was produced (e.g. single huge sentence), fall back to fixed-size.
    if chunks.is_empty() {
        return chunk_fixed(text, doc_id, max_chunk_size, 0);
    }
    chunks
}

/// Code chunking: split on function/class boundaries ("\n\n"), then apply
/// fixed-size fallback for blocks that exceed 1024 chars.
fn chunk_code(text: &str, doc_id: &str, _language: &str) -> Vec<Chunk> {
    let blocks: Vec<&str> = text
        .split("\n\n")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let max_block_size = 1024;
    let mut chunks = Vec::new();
    let mut idx = 0;
    let mut offset = 0;

    for block in &blocks {
        if block.len() > max_block_size {
            // Large block: fall back to fixed-size chunking within the block.
            let sub_chunks = chunk_fixed(block, doc_id, max_block_size, 64);
            for mut sc in sub_chunks {
                sc.id = format!("{doc_id}-chunk-{idx}");
                sc.chunk_index = idx;
                sc.start_offset += offset;
                sc.end_offset += offset;
                let mut meta = std::collections::HashMap::new();
                meta.insert("strategy".to_string(), "code_fixed_fallback".to_string());
                sc.metadata = meta;
                chunks.push(sc);
                idx += 1;
            }
        } else {
            let mut meta = std::collections::HashMap::new();
            meta.insert("strategy".to_string(), "code_block".to_string());
            chunks.push(Chunk {
                id: format!("{doc_id}-chunk-{idx}"),
                document_id: doc_id.to_string(),
                text: block.to_string(),
                chunk_index: idx,
                start_offset: offset,
                end_offset: offset + block.len(),
                metadata: meta,
            });
            idx += 1;
        }
        offset += block.len() + 2; // account for "\n\n"
    }

    if chunks.is_empty() {
        return chunk_fixed(text, doc_id, max_block_size, 64);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_fixed() {
        let text = "Hello world this is a test of chunking";
        let chunks = chunk_fixed(text, "doc1", 15, 3);
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].document_id, "doc1");
    }

    #[test]
    fn test_chunk_recursive() {
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let strategy = ChunkingStrategy::default();
        let chunks = chunk_text(text, "doc1", &strategy);
        assert!(!chunks.is_empty());
    }
}
