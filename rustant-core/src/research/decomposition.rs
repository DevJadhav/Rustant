//! Question decomposition into sub-query DAG.
//!
//! Breaks complex research questions into smaller, independently answerable
//! sub-queries with dependency relationships.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single sub-query derived from the main research question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubQuery {
    /// Unique identifier.
    pub id: Uuid,
    /// The sub-question to answer.
    pub question: String,
    /// IDs of sub-queries that must complete before this one.
    pub depends_on: Vec<Uuid>,
    /// Suggested tools to use for answering this query.
    pub suggested_tools: Vec<String>,
    /// Expected output type (e.g., "factual", "comparative", "statistical").
    pub output_type: String,
    /// Priority (1 = highest).
    pub priority: u32,
    /// Whether this query has been completed.
    pub completed: bool,
    /// The result of this query, if completed.
    pub result: Option<String>,
}

/// Decomposes a complex question into a DAG of sub-queries.
pub struct QuestionDecomposer;

impl QuestionDecomposer {
    /// Create a new decomposer.
    pub fn new() -> Self {
        Self
    }

    /// Decompose a question into sub-queries using heuristic analysis.
    ///
    /// This performs a basic structural decomposition. For LLM-powered
    /// decomposition, use `decompose_with_llm()`.
    pub fn decompose(&self, question: &str) -> Vec<SubQuery> {
        let mut queries = Vec::new();

        // Always create a main query
        let main_id = Uuid::new_v4();
        queries.push(SubQuery {
            id: main_id,
            question: question.to_string(),
            depends_on: vec![],
            suggested_tools: self.suggest_tools(question),
            output_type: "comprehensive".to_string(),
            priority: 1,
            completed: false,
            result: None,
        });

        // Look for decomposition patterns
        if question.contains(" and ") || question.contains(" vs ") || question.contains(" versus ")
        {
            // Comparative question — split into parts
            let parts = self.split_comparative(question);
            for (i, part) in parts.iter().enumerate() {
                let sub_id = Uuid::new_v4();
                queries.push(SubQuery {
                    id: sub_id,
                    question: part.clone(),
                    depends_on: vec![],
                    suggested_tools: self.suggest_tools(part),
                    output_type: "factual".to_string(),
                    priority: (i + 2) as u32,
                    completed: false,
                    result: None,
                });
            }

            // Add a synthesis query that depends on all parts
            let dep_ids: Vec<Uuid> = queries.iter().skip(1).map(|q| q.id).collect();
            queries.push(SubQuery {
                id: Uuid::new_v4(),
                question: format!("Synthesize findings about: {question}"),
                depends_on: dep_ids,
                suggested_tools: vec!["echo".to_string()],
                output_type: "synthesis".to_string(),
                priority: 10,
                completed: false,
                result: None,
            });
        }

        // For "how to" questions, add implementation sub-query
        if question.to_lowercase().starts_with("how") {
            queries.push(SubQuery {
                id: Uuid::new_v4(),
                question: format!("What are the practical steps for: {question}"),
                depends_on: vec![main_id],
                suggested_tools: vec!["web_search".to_string(), "web_fetch".to_string()],
                output_type: "procedural".to_string(),
                priority: 5,
                completed: false,
                result: None,
            });
        }

        queries
    }

    /// Suggest tools for a given sub-query based on content analysis.
    fn suggest_tools(&self, question: &str) -> Vec<String> {
        let lower = question.to_lowercase();
        let mut tools = vec!["web_search".to_string()];

        if lower.contains("paper") || lower.contains("research") || lower.contains("arxiv") {
            tools.push("arxiv_research".to_string());
        }
        if lower.contains("code") || lower.contains("implement") || lower.contains("library") {
            tools.push("web_fetch".to_string());
        }
        if lower.contains("knowledge") || lower.contains("graph") {
            tools.push("knowledge_graph".to_string());
        }

        tools
    }

    /// Split a comparative question into its constituent parts.
    fn split_comparative(&self, question: &str) -> Vec<String> {
        let separators = [" vs ", " versus ", " compared to ", " or "];
        for sep in &separators {
            if question.contains(sep) {
                return question
                    .split(sep)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        // Fallback: split on " and " only if it looks like enumeration
        if question.contains(" and ") {
            let parts: Vec<String> = question
                .split(" and ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() <= 3 {
                return parts;
            }
        }
        vec![question.to_string()]
    }

    /// Get sub-queries that are ready to execute (all dependencies met).
    pub fn ready_queries(queries: &[SubQuery]) -> Vec<&SubQuery> {
        let completed_ids: std::collections::HashSet<Uuid> = queries
            .iter()
            .filter(|q| q.completed)
            .map(|q| q.id)
            .collect();

        queries
            .iter()
            .filter(|q| !q.completed && q.depends_on.iter().all(|dep| completed_ids.contains(dep)))
            .collect()
    }
}

impl Default for QuestionDecomposer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_decomposition() {
        let decomposer = QuestionDecomposer::new();
        let queries = decomposer.decompose("What is prompt caching?");
        assert!(!queries.is_empty());
        assert_eq!(queries[0].question, "What is prompt caching?");
    }

    #[test]
    fn test_comparative_decomposition() {
        let decomposer = QuestionDecomposer::new();
        let queries = decomposer.decompose("Redis vs Memcached for caching");
        // Should have original + 2 parts + synthesis = 4
        assert!(queries.len() >= 3);
    }

    #[test]
    fn test_how_to_decomposition() {
        let decomposer = QuestionDecomposer::new();
        let queries = decomposer.decompose("How to implement RAG pipeline?");
        // Should have main + implementation sub-query
        assert!(queries.len() >= 2);
    }

    #[test]
    fn test_tool_suggestions() {
        let decomposer = QuestionDecomposer::new();
        let queries = decomposer.decompose("Find arxiv papers on attention mechanisms");
        assert!(
            queries[0]
                .suggested_tools
                .contains(&"arxiv_research".to_string())
        );
    }

    #[test]
    fn test_ready_queries() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let queries = vec![
            SubQuery {
                id: id1,
                question: "Q1".into(),
                depends_on: vec![],
                suggested_tools: vec![],
                output_type: "factual".into(),
                priority: 1,
                completed: true,
                result: Some("Done".into()),
            },
            SubQuery {
                id: id2,
                question: "Q2".into(),
                depends_on: vec![id1],
                suggested_tools: vec![],
                output_type: "synthesis".into(),
                priority: 2,
                completed: false,
                result: None,
            },
        ];

        let ready = QuestionDecomposer::ready_queries(&queries);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, id2);
    }
}
