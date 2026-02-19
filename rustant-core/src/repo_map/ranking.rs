//! Symbol ranking — combines PageRank with text relevance scoring.

use super::ContextChunk;
use super::graph::CodeGraph;
use crate::ast::Symbol;

/// Select ranked context chunks within a token budget.
pub fn select_ranked_context(
    symbols: &[Symbol],
    graph: &CodeGraph,
    query: &str,
    token_budget: usize,
) -> Vec<ContextChunk> {
    // Compute PageRank scores
    let pagerank_scores = graph.pagerank(0.85, 20);
    let mut name_to_rank: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for (idx, score) in &pagerank_scores {
        if let Some(node) = graph.get_node_by_index(*idx) {
            name_to_rank.insert(node.name.clone(), *score);
        }
    }

    // Compute text relevance scores
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored_symbols: Vec<(usize, f64)> = symbols
        .iter()
        .enumerate()
        .map(|(i, sym)| {
            let name_lower = sym.name.to_lowercase();
            let sig_lower = sym.signature.to_lowercase();

            // Text relevance: exact match > prefix > contains > none
            let text_score = if name_lower == query_lower {
                1.0
            } else if name_lower.starts_with(&query_lower) {
                0.8
            } else if name_lower.contains(&query_lower) {
                0.6
            } else if query_words.iter().any(|w| name_lower.contains(w)) {
                0.4
            } else if query_words.iter().any(|w| sig_lower.contains(w)) {
                0.2
            } else {
                0.0
            };

            // PageRank relevance
            let pr_score = name_to_rank.get(&sym.name).copied().unwrap_or(0.0);

            // Combined score: weighted average
            let combined = text_score * 0.7 + pr_score * 100.0 * 0.3;
            (i, combined)
        })
        .collect();

    // Sort by combined score descending
    scored_symbols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Select chunks within token budget (rough estimate: 4 chars per token)
    let chars_per_token = 4;
    let char_budget = token_budget * chars_per_token;
    let mut used_chars = 0;
    let mut chunks = Vec::new();

    for (idx, score) in scored_symbols {
        if score <= 0.0 {
            break;
        }
        let sym = &symbols[idx];
        let content = sym.signature.clone();
        let chunk_chars = content.len();

        if used_chars + chunk_chars > char_budget {
            break;
        }

        chunks.push(ContextChunk {
            file: sym.file.clone(),
            start_line: sym.start_line,
            end_line: sym.end_line,
            content,
            relevance_score: score,
        });

        used_chars += chunk_chars;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::SymbolKind;

    #[test]
    fn test_select_ranked_context_empty() {
        let graph = CodeGraph::new();
        let chunks = select_ranked_context(&[], &graph, "test", 1000);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_select_ranked_context_basic() {
        let symbols = vec![
            Symbol {
                name: "hello".into(),
                kind: SymbolKind::Function,
                file: "main.rs".into(),
                start_line: 1,
                end_line: 3,
                signature: "pub fn hello()".into(),
            },
            Symbol {
                name: "goodbye".into(),
                kind: SymbolKind::Function,
                file: "main.rs".into(),
                start_line: 5,
                end_line: 7,
                signature: "pub fn goodbye()".into(),
            },
        ];

        let graph = CodeGraph::new();
        let chunks = select_ranked_context(&symbols, &graph, "hello", 1000);
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].file, "main.rs");
    }

    #[test]
    fn test_token_budget_respected() {
        let symbols: Vec<Symbol> = (0..100)
            .map(|i| Symbol {
                name: format!("func_{i}"),
                kind: SymbolKind::Function,
                file: "big.rs".into(),
                start_line: i,
                end_line: i + 1,
                signature: format!("pub fn func_{i}() -> Result<(), Error>"),
            })
            .collect();

        let graph = CodeGraph::new();
        // Very small budget
        let chunks = select_ranked_context(&symbols, &graph, "func", 10);
        let total_chars: usize = chunks.iter().map(|c| c.content.len()).sum();
        assert!(total_chars <= 40); // 10 tokens * 4 chars
    }
}
