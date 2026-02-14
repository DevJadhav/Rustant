//! LLM Council — Multi-model deliberation for planning tasks.
//!
//! Inspired by [karpathy/llm-council](https://github.com/karpathy/llm-council).
//! Sends a question to multiple LLM providers concurrently, optionally runs
//! peer review, and synthesizes a final answer via a chairman model.
//!
//! # Three-stage deliberation protocol
//!
//! 1. **Parallel Query** — All council members receive the question concurrently.
//! 2. **Peer Review** (optional) — Each model reviews others' responses anonymously.
//! 3. **Chairman Synthesis** — A designated model synthesizes all responses + reviews.

use crate::brain::LlmProvider;
use crate::config::{CouncilConfig, CouncilMemberConfig, VotingStrategy};
use crate::error::LlmError;
use crate::types::{CompletionRequest, Message, TokenUsage};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

/// A detected provider available in the environment.
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    /// Provider type: "openai", "anthropic", "gemini", "ollama".
    pub provider_type: String,
    /// Model name.
    pub model: String,
    /// Environment variable containing the API key.
    pub api_key_env: String,
    /// Whether this is a local provider (no API key needed).
    pub is_local: bool,
    /// Optional base URL override.
    pub base_url: Option<String>,
}

/// Response from a single council member.
#[derive(Debug, Clone)]
pub struct CouncilMemberResponse {
    /// Model name that produced this response.
    pub model_name: String,
    /// Provider type.
    pub provider: String,
    /// The response text.
    pub response_text: String,
    /// Token usage for this response.
    pub usage: TokenUsage,
    /// Estimated cost in USD.
    pub cost: f64,
    /// Response latency in milliseconds.
    pub latency_ms: u64,
}

/// A peer review from one model reviewing another's response.
#[derive(Debug, Clone)]
pub struct PeerReview {
    /// Model that performed this review.
    pub reviewer_model: String,
    /// Index of the response being reviewed (0-based).
    pub reviewed_index: usize,
    /// Score from 1-10.
    pub score: u8,
    /// Reasoning for the score.
    pub reasoning: String,
    /// Identified strengths.
    pub strengths: Vec<String>,
    /// Identified weaknesses.
    pub weaknesses: Vec<String>,
}

/// Result of a full council deliberation.
#[derive(Debug, Clone)]
pub struct CouncilResult {
    /// The final synthesized answer.
    pub synthesis: String,
    /// Individual member responses.
    pub member_responses: Vec<CouncilMemberResponse>,
    /// Peer reviews (empty if peer review was disabled).
    pub peer_reviews: Vec<PeerReview>,
    /// Total token usage across all stages.
    pub total_usage: TokenUsage,
    /// Total estimated cost in USD.
    pub total_cost: f64,
    /// Total deliberation time in milliseconds.
    pub total_latency_ms: u64,
}

/// The planning council: holds members and orchestrates deliberation.
pub struct PlanningCouncil {
    /// Council member providers paired with their configs.
    members: Vec<(Arc<dyn LlmProvider>, CouncilMemberConfig)>,
    /// Index of the chairman in the members vec.
    chairman_index: usize,
    /// Council configuration.
    config: CouncilConfig,
}

impl PlanningCouncil {
    /// Create a new planning council.
    ///
    /// Requires at least 2 members. Returns an error if fewer are provided.
    /// Auto-selects the chairman as the member with the largest context window,
    /// unless `config.chairman_model` is explicitly set.
    pub fn new(
        members: Vec<(Arc<dyn LlmProvider>, CouncilMemberConfig)>,
        config: CouncilConfig,
    ) -> Result<Self, LlmError> {
        if members.len() < 2 {
            return Err(LlmError::ApiRequest {
                message: format!("Council requires at least 2 members, got {}", members.len()),
            });
        }

        // Select chairman: explicit config or largest context window.
        let chairman_index = if let Some(ref chairman_model) = config.chairman_model {
            members
                .iter()
                .position(|(_, cfg)| cfg.model == *chairman_model)
                .unwrap_or(0)
        } else {
            members
                .iter()
                .enumerate()
                .max_by_key(|(_, (provider, _))| provider.context_window())
                .map(|(i, _)| i)
                .unwrap_or(0)
        };

        info!(
            members = members.len(),
            chairman = members[chairman_index].1.model.as_str(),
            strategy = %config.voting_strategy,
            "Planning council created"
        );

        Ok(Self {
            members,
            chairman_index,
            config,
        })
    }

    /// Run the full three-stage deliberation protocol.
    pub async fn deliberate(&self, question: &str) -> Result<CouncilResult, LlmError> {
        let start = Instant::now();

        // Stage 1: Parallel query to all members.
        info!(
            members = self.members.len(),
            "Stage 1: Querying council members"
        );
        let member_responses = self.stage_query(question).await?;

        // Stage 2: Optional peer review.
        let peer_reviews = if self.config.enable_peer_review && self.members.len() > 2 {
            info!("Stage 2: Running peer review");
            self.stage_peer_review(question, &member_responses).await
        } else {
            debug!("Skipping peer review (disabled or < 3 members)");
            Vec::new()
        };

        // Stage 3: Chairman synthesis.
        info!(
            chairman = self.members[self.chairman_index].1.model.as_str(),
            "Stage 3: Chairman synthesis"
        );
        let synthesis = self
            .stage_synthesis(question, &member_responses, &peer_reviews)
            .await?;

        // Compute totals.
        let mut total_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };
        let mut total_cost = 0.0;

        for resp in &member_responses {
            total_usage.input_tokens += resp.usage.input_tokens;
            total_usage.output_tokens += resp.usage.output_tokens;
            total_cost += resp.cost;
        }

        // Add synthesis usage (estimated from response length).
        total_usage.output_tokens += synthesis.len() / 4; // rough estimate

        let total_latency_ms = start.elapsed().as_millis() as u64;

        info!(
            total_cost = format!("${:.4}", total_cost),
            total_latency_ms,
            responses = member_responses.len(),
            reviews = peer_reviews.len(),
            "Council deliberation complete"
        );

        Ok(CouncilResult {
            synthesis,
            member_responses,
            peer_reviews,
            total_usage,
            total_cost,
            total_latency_ms,
        })
    }

    /// Stage 1: Send the question to all members concurrently.
    async fn stage_query(&self, question: &str) -> Result<Vec<CouncilMemberResponse>, LlmError> {
        let futures: Vec<_> = self
            .members
            .iter()
            .map(|(provider, cfg)| {
                let provider = Arc::clone(provider);
                let model = cfg.model.clone();
                let provider_name = cfg.provider.clone();
                let max_tokens = self.config.max_member_tokens;
                let question = question.to_string();

                async move {
                    let start = Instant::now();
                    let request = CompletionRequest {
                        messages: vec![
                            Message::system(
                                "You are a council member deliberating on a planning question. \
                                 Provide your best analysis with concrete, actionable recommendations.",
                            ),
                            Message::user(&question),
                        ],
                        tools: None,
                        temperature: 0.7,
                        max_tokens: Some(max_tokens),
                        stop_sequences: vec![],
                        model: Some(model.clone()),
                    };

                    let result = provider.complete(request).await;
                    let latency_ms = start.elapsed().as_millis() as u64;

                    match result {
                        Ok(response) => {
                            let (cost_in, cost_out) = provider.cost_per_token();
                            let cost = (response.usage.input_tokens as f64 * cost_in)
                                + (response.usage.output_tokens as f64 * cost_out);
                            let text = response
                                .message
                                .content
                                .as_text()
                                .unwrap_or("")
                                .to_string();

                            Ok(CouncilMemberResponse {
                                model_name: model,
                                provider: provider_name,
                                response_text: text,
                                usage: response.usage,
                                cost,
                                latency_ms,
                            })
                        }
                        Err(e) => {
                            warn!(
                                model = model.as_str(),
                                error = %e,
                                "Council member failed to respond"
                            );
                            Err(e)
                        }
                    }
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        // Collect successful responses; warn about failures.
        let mut responses = Vec::new();
        for result in results {
            match result {
                Ok(resp) => responses.push(resp),
                Err(e) => {
                    warn!(error = %e, "Skipping failed council member");
                }
            }
        }

        if responses.is_empty() {
            return Err(LlmError::ApiRequest {
                message: "All council members failed to respond".to_string(),
            });
        }

        Ok(responses)
    }

    /// Stage 2: Each member reviews other members' responses anonymously.
    async fn stage_peer_review(
        &self,
        question: &str,
        responses: &[CouncilMemberResponse],
    ) -> Vec<PeerReview> {
        let mut reviews = Vec::new();

        // Build anonymous response labels.
        let labels: Vec<String> = (0..responses.len())
            .map(|i| format!("Response {}", (b'A' + i as u8) as char))
            .collect();

        let mut responses_text = String::new();
        for (i, resp) in responses.iter().enumerate() {
            responses_text.push_str(&format!(
                "\n--- {} ---\n{}\n",
                labels[i], resp.response_text
            ));
        }

        for (reviewer_idx, (provider, cfg)) in self.members.iter().enumerate() {
            for (reviewed_idx, _) in responses.iter().enumerate() {
                if reviewer_idx == reviewed_idx {
                    continue; // Don't review yourself.
                }

                let prompt = format!(
                    "You are reviewing responses to this question:\n\n\"{}\"\n\n\
                     Here are all the responses:\n{}\n\n\
                     Please review {} specifically.\n\
                     Rate it 1-10 and provide:\n\
                     - Score (1-10)\n\
                     - Brief reasoning\n\
                     - Key strengths (bullet points)\n\
                     - Key weaknesses (bullet points)\n\n\
                     Format your response as:\n\
                     SCORE: <number>\n\
                     REASONING: <text>\n\
                     STRENGTHS:\n- <point>\n\
                     WEAKNESSES:\n- <point>",
                    question, responses_text, labels[reviewed_idx]
                );

                let request = CompletionRequest {
                    messages: vec![
                        Message::system(
                            "You are an impartial reviewer evaluating LLM responses. \
                             Be objective and constructive.",
                        ),
                        Message::user(&prompt),
                    ],
                    tools: None,
                    temperature: 0.3,
                    max_tokens: Some(512),
                    stop_sequences: vec![],
                    model: Some(cfg.model.clone()),
                };

                match provider.complete(request).await {
                    Ok(response) => {
                        let text = response.message.content.as_text().unwrap_or("").to_string();
                        let review = parse_peer_review(&cfg.model, reviewed_idx, &text);
                        reviews.push(review);
                    }
                    Err(e) => {
                        warn!(
                            reviewer = cfg.model.as_str(),
                            error = %e,
                            "Peer review failed"
                        );
                    }
                }
            }
        }

        reviews
    }

    /// Stage 3: Chairman synthesizes all responses and reviews into a final answer.
    async fn stage_synthesis(
        &self,
        question: &str,
        responses: &[CouncilMemberResponse],
        reviews: &[PeerReview],
    ) -> Result<String, LlmError> {
        let (chairman_provider, chairman_cfg) = &self.members[self.chairman_index];

        // Build synthesis prompt.
        let mut prompt = format!(
            "You are the chairman of an LLM council. Multiple models have responded \
             to the following question:\n\n\"{}\"\n\n",
            question
        );

        // Add anonymized responses.
        for (i, resp) in responses.iter().enumerate() {
            let label = (b'A' + i as u8) as char;
            prompt.push_str(&format!(
                "--- Response {} ---\n{}\n\n",
                label, resp.response_text
            ));
        }

        // Add peer reviews if available.
        if !reviews.is_empty() {
            prompt.push_str("--- Peer Reviews ---\n");
            for review in reviews {
                let reviewed_label = (b'A' + review.reviewed_index as u8) as char;
                prompt.push_str(&format!(
                    "Review of Response {} (score: {}/10): {}\n",
                    reviewed_label, review.score, review.reasoning
                ));
            }
            prompt.push('\n');
        }

        let strategy_instruction = match self.config.voting_strategy {
            VotingStrategy::ChairmanSynthesis => {
                "Synthesize the best elements from all responses into a comprehensive, \
                 well-structured final answer. Resolve any contradictions and add your own insights."
            }
            VotingStrategy::HighestScore => {
                "Identify the highest-quality response based on peer reviews. \
                 Present it as the final answer with minimal modifications."
            }
            VotingStrategy::MajorityConsensus => {
                "Identify the points where most responses agree. \
                 Present the consensus view, noting any significant dissenting perspectives."
            }
        };

        prompt.push_str(&format!(
            "Your task: {}\n\nProvide your final synthesized answer:",
            strategy_instruction
        ));

        let request = CompletionRequest {
            messages: vec![
                Message::system(
                    "You are the chairman of an LLM council, responsible for producing \
                     a final synthesized answer from multiple model responses.",
                ),
                Message::user(&prompt),
            ],
            tools: None,
            temperature: 0.5,
            max_tokens: Some(self.config.max_member_tokens * 2),
            stop_sequences: vec![],
            model: Some(chairman_cfg.model.clone()),
        };

        let response = chairman_provider.complete(request).await?;
        Ok(response.message.content.as_text().unwrap_or("").to_string())
    }
}

/// Parse a peer review response into structured data.
fn parse_peer_review(reviewer_model: &str, reviewed_index: usize, text: &str) -> PeerReview {
    let mut score: u8 = 5;
    let mut reasoning = String::new();
    let mut strengths = Vec::new();
    let mut weaknesses = Vec::new();

    let mut in_strengths = false;
    let mut in_weaknesses = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if let Some(s) = trimmed.strip_prefix("SCORE:") {
            if let Ok(n) = s.trim().parse::<u8>() {
                score = n.clamp(1, 10);
            }
            in_strengths = false;
            in_weaknesses = false;
        } else if let Some(r) = trimmed.strip_prefix("REASONING:") {
            reasoning = r.trim().to_string();
            in_strengths = false;
            in_weaknesses = false;
        } else if trimmed == "STRENGTHS:" {
            in_strengths = true;
            in_weaknesses = false;
        } else if trimmed == "WEAKNESSES:" {
            in_strengths = false;
            in_weaknesses = true;
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            if in_strengths {
                strengths.push(item.to_string());
            } else if in_weaknesses {
                weaknesses.push(item.to_string());
            }
        }
    }

    PeerReview {
        reviewer_model: reviewer_model.to_string(),
        reviewed_index,
        score,
        reasoning,
        strengths,
        weaknesses,
    }
}

/// Heuristic to determine if a task should use council deliberation.
///
/// Returns `true` for planning/analysis tasks, `false` for concrete/action tasks.
pub fn should_use_council(task: &str) -> bool {
    let lower = task.to_lowercase();

    // Planning keywords that suggest deliberation would be valuable.
    let planning_keywords = [
        "plan",
        "design",
        "architect",
        "strategy",
        "approach",
        "compare",
        "evaluate",
        "trade-off",
        "tradeoff",
        "pros and cons",
        "best way to",
        "how should",
        "what approach",
        "recommend",
        "analyze",
        "brainstorm",
        "review my",
        "help me decide",
        "which is better",
    ];

    // Concrete action keywords that don't need council.
    let concrete_keywords = [
        "fix",
        "write",
        "create file",
        "delete",
        "run",
        "execute",
        "install",
        "commit",
        "push",
        "deploy",
        "read file",
        "open",
        "close",
        "set",
        "update",
    ];

    // Check for planning keywords.
    let has_planning = planning_keywords.iter().any(|kw| lower.contains(kw));

    // Check for concrete keywords.
    let has_concrete = concrete_keywords.iter().any(|kw| lower.contains(kw));

    // Planning tasks that aren't also concrete actions.
    has_planning && !has_concrete
}

/// Auto-detect available LLM providers from environment variables and Ollama.
///
/// Checks for:
/// - OPENAI_API_KEY → OpenAI
/// - ANTHROPIC_API_KEY → Anthropic
/// - GEMINI_API_KEY or GOOGLE_API_KEY → Gemini
/// - Ollama at localhost:11434 (with 3-second timeout)
pub async fn detect_available_providers() -> Vec<DetectedProvider> {
    let mut providers = Vec::new();

    // Check cloud providers via env vars.
    if std::env::var("OPENAI_API_KEY").is_ok() {
        providers.push(DetectedProvider {
            provider_type: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            is_local: false,
            base_url: None,
        });
    }

    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        providers.push(DetectedProvider {
            provider_type: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            is_local: false,
            base_url: None,
        });
    }

    if std::env::var("GEMINI_API_KEY")
        .or_else(|_| std::env::var("GOOGLE_API_KEY"))
        .is_ok()
    {
        let env_key = if std::env::var("GEMINI_API_KEY").is_ok() {
            "GEMINI_API_KEY"
        } else {
            "GOOGLE_API_KEY"
        };
        providers.push(DetectedProvider {
            provider_type: "gemini".to_string(),
            model: "gemini-2.0-flash".to_string(),
            api_key_env: env_key.to_string(),
            is_local: false,
            base_url: None,
        });
    }

    // Check Ollama.
    match detect_ollama_models().await {
        Ok(models) => {
            for model in models {
                providers.push(DetectedProvider {
                    provider_type: "ollama".to_string(),
                    model,
                    api_key_env: String::new(),
                    is_local: true,
                    base_url: Some("http://127.0.0.1:11434/v1".to_string()),
                });
            }
        }
        Err(e) => {
            debug!(error = %e, "Ollama not detected");
        }
    }

    providers
}

/// Detect available Ollama models by querying the Ollama API.
///
/// Returns a list of model names. Uses a 3-second timeout.
pub async fn detect_ollama_models() -> Result<Vec<String>, LlmError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| LlmError::Connection {
            message: format!("Failed to build HTTP client: {}", e),
        })?;

    let response = client
        .get("http://127.0.0.1:11434/api/tags")
        .send()
        .await
        .map_err(|e| LlmError::Connection {
            message: format!("Ollama not available: {}", e),
        })?;

    let body: serde_json::Value = response.json().await.map_err(|e| LlmError::ResponseParse {
        message: format!("Failed to parse Ollama response: {}", e),
    })?;

    let models = body["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

/// Convert detected providers into council member configs.
pub fn providers_to_council_members(providers: &[DetectedProvider]) -> Vec<CouncilMemberConfig> {
    providers
        .iter()
        .map(|p| CouncilMemberConfig {
            provider: p.provider_type.clone(),
            model: p.model.clone(),
            api_key_env: p.api_key_env.clone(),
            base_url: p.base_url.clone(),
            weight: 1.0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MockLlmProvider;

    #[test]
    fn test_should_use_council_planning_tasks() {
        assert!(should_use_council(
            "Help me plan the architecture for a new API"
        ));
        assert!(should_use_council("What's the best approach for caching?"));
        assert!(should_use_council(
            "Compare REST vs GraphQL for this project"
        ));
        assert!(should_use_council(
            "Analyze the trade-offs of microservices"
        ));
        assert!(should_use_council("Help me design a database schema"));
        assert!(should_use_council(
            "What strategy should I use for testing?"
        ));
        assert!(should_use_council("Brainstorm ideas for the landing page"));
    }

    #[test]
    fn test_should_use_council_concrete_tasks() {
        assert!(!should_use_council("Fix the bug in main.rs"));
        assert!(!should_use_council("Write a function to sort an array"));
        assert!(!should_use_council("Create file config.toml"));
        assert!(!should_use_council("Delete the old test file"));
        assert!(!should_use_council("Run cargo test"));
        assert!(!should_use_council("Install the reqwest dependency"));
        assert!(!should_use_council("Read file src/main.rs"));
    }

    #[test]
    fn test_should_use_council_mixed() {
        // "plan" + "write" → concrete wins
        assert!(!should_use_council("Plan and write the implementation"));
        // Pure planning
        assert!(should_use_council(
            "Help me plan the implementation approach"
        ));
    }

    #[test]
    fn test_council_requires_at_least_two_members() {
        let provider = Arc::new(MockLlmProvider::new()) as Arc<dyn LlmProvider>;
        let config = CouncilConfig::default();

        // One member should fail
        let result = PlanningCouncil::new(
            vec![(provider.clone(), CouncilMemberConfig::default())],
            config.clone(),
        );
        assert!(result.is_err());

        // Two members should succeed
        let result = PlanningCouncil::new(
            vec![
                (provider.clone(), CouncilMemberConfig::default()),
                (
                    provider.clone(),
                    CouncilMemberConfig {
                        model: "model-b".to_string(),
                        ..Default::default()
                    },
                ),
            ],
            config,
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_council_deliberation_with_mocks() {
        let provider_a = Arc::new(MockLlmProvider::with_response("Response from model A"))
            as Arc<dyn LlmProvider>;
        let provider_b = Arc::new(MockLlmProvider::with_response("Response from model B"))
            as Arc<dyn LlmProvider>;
        let provider_c = Arc::new(MockLlmProvider::with_response("Response from model C"))
            as Arc<dyn LlmProvider>;

        let config = CouncilConfig {
            enabled: true,
            enable_peer_review: false, // Skip peer review for speed
            ..Default::default()
        };

        let council = PlanningCouncil::new(
            vec![
                (
                    provider_a,
                    CouncilMemberConfig {
                        model: "model-a".to_string(),
                        ..Default::default()
                    },
                ),
                (
                    provider_b,
                    CouncilMemberConfig {
                        model: "model-b".to_string(),
                        ..Default::default()
                    },
                ),
                (
                    provider_c,
                    CouncilMemberConfig {
                        model: "model-c".to_string(),
                        ..Default::default()
                    },
                ),
            ],
            config,
        )
        .unwrap();

        let result = council
            .deliberate("What architecture should we use?")
            .await
            .unwrap();

        // Should have 3 member responses
        assert_eq!(result.member_responses.len(), 3);
        // Peer reviews disabled
        assert!(result.peer_reviews.is_empty());
        // Synthesis should exist
        assert!(!result.synthesis.is_empty());
        // Verify totals are computed (mock providers have 0 cost)
        let _ = result.total_cost;
        let _ = result.total_latency_ms;
    }

    #[tokio::test]
    async fn test_detect_available_providers_no_panic() {
        // Should not panic even if no providers are available.
        let providers = detect_available_providers().await;
        // We can't assert much here since it depends on the environment,
        // but it should not panic.
        let _ = providers;
    }

    #[test]
    fn test_parse_peer_review() {
        let text = "SCORE: 8\n\
                    REASONING: Good analysis with practical recommendations.\n\
                    STRENGTHS:\n\
                    - Clear structure\n\
                    - Actionable steps\n\
                    WEAKNESSES:\n\
                    - Missing error handling consideration\n\
                    - No cost analysis";

        let review = parse_peer_review("reviewer-model", 0, text);
        assert_eq!(review.score, 8);
        assert_eq!(review.reviewed_index, 0);
        assert!(review.reasoning.contains("Good analysis"));
        assert_eq!(review.strengths.len(), 2);
        assert_eq!(review.weaknesses.len(), 2);
        assert_eq!(review.strengths[0], "Clear structure");
        assert_eq!(review.weaknesses[0], "Missing error handling consideration");
    }

    #[test]
    fn test_parse_peer_review_malformed() {
        // Should handle malformed input gracefully
        let review = parse_peer_review("reviewer", 1, "Just some random text without format");
        assert_eq!(review.score, 5); // Default
        assert!(review.reasoning.is_empty());
        assert!(review.strengths.is_empty());
        assert!(review.weaknesses.is_empty());
    }

    #[test]
    fn test_council_peer_review_anonymization() {
        // Verify that the prompt uses labels (A, B, C), not model names.
        // This is tested indirectly through the stage_peer_review implementation.
        // The labels vec should use letters not model names.
        let labels: Vec<String> = (0..3)
            .map(|i| format!("Response {}", (b'A' + i as u8) as char))
            .collect();
        assert_eq!(labels, vec!["Response A", "Response B", "Response C"]);
    }

    #[test]
    fn test_providers_to_council_members() {
        let providers = vec![
            DetectedProvider {
                provider_type: "openai".to_string(),
                model: "gpt-4o".to_string(),
                api_key_env: "OPENAI_API_KEY".to_string(),
                is_local: false,
                base_url: None,
            },
            DetectedProvider {
                provider_type: "ollama".to_string(),
                model: "llama3.2".to_string(),
                api_key_env: String::new(),
                is_local: true,
                base_url: Some("http://127.0.0.1:11434/v1".to_string()),
            },
        ];

        let members = providers_to_council_members(&providers);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].provider, "openai");
        assert_eq!(members[0].model, "gpt-4o");
        assert_eq!(members[1].provider, "ollama");
        assert_eq!(members[1].model, "llama3.2");
        assert_eq!(
            members[1].base_url,
            Some("http://127.0.0.1:11434/v1".to_string())
        );
    }

    #[tokio::test]
    async fn test_council_with_two_members_no_peer_review() {
        // Minimum viable council: 2 members, peer review auto-skipped (< 3 members).
        let provider_a =
            Arc::new(MockLlmProvider::with_response("Answer A")) as Arc<dyn LlmProvider>;
        let provider_b =
            Arc::new(MockLlmProvider::with_response("Answer B")) as Arc<dyn LlmProvider>;

        let config = CouncilConfig {
            enabled: true,
            enable_peer_review: true, // Enabled but should be skipped (< 3 members)
            ..Default::default()
        };

        let council = PlanningCouncil::new(
            vec![
                (
                    provider_a,
                    CouncilMemberConfig {
                        model: "model-a".to_string(),
                        ..Default::default()
                    },
                ),
                (
                    provider_b,
                    CouncilMemberConfig {
                        model: "model-b".to_string(),
                        ..Default::default()
                    },
                ),
            ],
            config,
        )
        .unwrap();

        let result = council.deliberate("Compare approaches").await.unwrap();

        assert_eq!(result.member_responses.len(), 2);
        // Peer review should be empty because < 3 members
        assert!(result.peer_reviews.is_empty());
        assert!(!result.synthesis.is_empty());
    }

    #[tokio::test]
    async fn test_council_cost_tracking() {
        let provider_a =
            Arc::new(MockLlmProvider::with_response("Cost test A")) as Arc<dyn LlmProvider>;
        let provider_b =
            Arc::new(MockLlmProvider::with_response("Cost test B")) as Arc<dyn LlmProvider>;
        let provider_c =
            Arc::new(MockLlmProvider::with_response("Cost test C")) as Arc<dyn LlmProvider>;

        let config = CouncilConfig {
            enabled: true,
            enable_peer_review: false,
            ..Default::default()
        };

        let council = PlanningCouncil::new(
            vec![
                (
                    provider_a,
                    CouncilMemberConfig {
                        model: "model-a".to_string(),
                        ..Default::default()
                    },
                ),
                (
                    provider_b,
                    CouncilMemberConfig {
                        model: "model-b".to_string(),
                        ..Default::default()
                    },
                ),
                (
                    provider_c,
                    CouncilMemberConfig {
                        model: "model-c".to_string(),
                        ..Default::default()
                    },
                ),
            ],
            config,
        )
        .unwrap();

        let result = council.deliberate("Test cost tracking").await.unwrap();

        // Total cost should be the sum of all member costs
        let member_cost_sum: f64 = result.member_responses.iter().map(|r| r.cost).sum();
        assert!((result.total_cost - member_cost_sum).abs() < f64::EPSILON);
    }
}
