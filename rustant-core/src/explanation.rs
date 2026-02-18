//! # Decision / Tool Selection Explanations
//!
//! This module provides structured explanations for every decision the agent
//! makes -- tool selections, parameter choices, task decompositions, and error
//! recovery strategies.  Each [`DecisionExplanation`] captures the full
//! reasoning chain, considered alternatives, contextual factors, and a
//! confidence score so that users (and auditors) can understand *why* the
//! agent acted the way it did.
//!
//! Use [`ExplanationBuilder`] for ergonomic, fluent construction of
//! explanations.

use crate::types::RiskLevel;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Core data model
// ---------------------------------------------------------------------------

/// A complete explanation for a single agent decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExplanation {
    /// Unique identifier for this decision.
    pub decision_id: Uuid,
    /// When the decision was made.
    pub timestamp: DateTime<Utc>,
    /// The kind of decision that was made.
    pub decision_type: DecisionType,
    /// Ordered chain of reasoning steps that led to the decision.
    pub reasoning_chain: Vec<ReasoningStep>,
    /// Alternatives that were evaluated but ultimately not chosen.
    pub considered_alternatives: Vec<AlternativeAction>,
    /// Agent confidence in this decision, clamped to `[0.0, 1.0]`.
    pub confidence: f32,
    /// Contextual factors that influenced the decision.
    pub context_factors: Vec<ContextFactor>,
    /// Active persona at the time of this decision (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_persona: Option<String>,
    /// Rationale for persona selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_selection_rationale: Option<String>,
    /// Current cache state (Cold/Warm/Hot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_state: Option<String>,
    /// Condensed summary of extended thinking, if thinking was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_summary: Option<String>,
    /// Citations/sources referenced in the decision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations_used: Vec<String>,
    /// Web/grounding sources used for factual claims.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounding_sources: Vec<String>,
    /// Which provider capabilities were leveraged for this decision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities_used: Vec<String>,
}

/// The category of decision the agent made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionType {
    /// The agent selected a specific tool to invoke.
    ToolSelection {
        /// Name of the tool that was selected.
        selected_tool: String,
    },
    /// The agent chose a particular value for a tool parameter.
    ParameterChoice {
        /// The tool whose parameter is being set.
        tool: String,
        /// The parameter name.
        parameter: String,
    },
    /// The agent decomposed a high-level task into sub-tasks.
    TaskDecomposition {
        /// Descriptions of the resulting sub-tasks.
        sub_tasks: Vec<String>,
    },
    /// The agent is recovering from an error.
    ErrorRecovery {
        /// Description of the error that occurred.
        error: String,
        /// The recovery strategy chosen.
        strategy: String,
    },
}

/// A single step in the agent's reasoning chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// 1-based index of this step within the chain.
    pub step_number: usize,
    /// Human-readable description of the reasoning.
    pub description: String,
    /// Optional supporting evidence (e.g. a memory excerpt or metric).
    pub evidence: Option<String>,
}

/// An alternative action that the agent considered but did not select.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeAction {
    /// The tool that was considered.
    pub tool_name: String,
    /// Why this alternative was not chosen.
    pub reason_not_selected: String,
    /// Estimated risk level of this alternative.
    pub estimated_risk: RiskLevel,
}

/// A contextual factor that influenced the decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFactor {
    /// Description of the factor (e.g. "user expressed urgency").
    pub factor: String,
    /// Whether this factor nudged the decision positively, negatively, or had
    /// no net effect.
    pub influence: FactorInfluence,
}

/// The direction of influence a context factor has on a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactorInfluence {
    /// The factor pushed the decision in a favourable direction.
    Positive,
    /// The factor pushed the decision in an unfavourable direction.
    Negative,
    /// The factor was noted but had no net directional effect.
    Neutral,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Fluent builder for constructing [`DecisionExplanation`] instances.
pub struct ExplanationBuilder {
    decision_type: DecisionType,
    reasoning_chain: Vec<ReasoningStep>,
    considered_alternatives: Vec<AlternativeAction>,
    confidence: f32,
    context_factors: Vec<ContextFactor>,
    active_persona: Option<String>,
    persona_selection_rationale: Option<String>,
    cache_state: Option<String>,
    thinking_summary: Option<String>,
    citations_used: Vec<String>,
    grounding_sources: Vec<String>,
    capabilities_used: Vec<String>,
}

impl ExplanationBuilder {
    /// Create a new builder for the given [`DecisionType`].
    ///
    /// The default confidence is `0.5`.
    pub fn new(decision_type: DecisionType) -> Self {
        Self {
            decision_type,
            reasoning_chain: Vec::new(),
            considered_alternatives: Vec::new(),
            confidence: 0.5,
            context_factors: Vec::new(),
            active_persona: None,
            persona_selection_rationale: None,
            cache_state: None,
            thinking_summary: None,
            citations_used: Vec::new(),
            grounding_sources: Vec::new(),
            capabilities_used: Vec::new(),
        }
    }

    /// Append a reasoning step.
    ///
    /// Steps are automatically numbered in the order they are added (starting
    /// from 1).
    pub fn add_reasoning_step(
        &mut self,
        description: impl Into<String>,
        evidence: Option<&str>,
    ) -> &mut Self {
        let step_number = self.reasoning_chain.len() + 1;
        self.reasoning_chain.push(ReasoningStep {
            step_number,
            description: description.into(),
            evidence: evidence.map(String::from),
        });
        self
    }

    /// Record an alternative that was considered but not selected.
    pub fn add_alternative(&mut self, tool: &str, reason: &str, risk: RiskLevel) -> &mut Self {
        self.considered_alternatives.push(AlternativeAction {
            tool_name: tool.to_owned(),
            reason_not_selected: reason.to_owned(),
            estimated_risk: risk,
        });
        self
    }

    /// Record a contextual factor that influenced the decision.
    pub fn add_context_factor(&mut self, factor: &str, influence: FactorInfluence) -> &mut Self {
        self.context_factors.push(ContextFactor {
            factor: factor.to_owned(),
            influence,
        });
        self
    }

    /// Set the confidence score.  Values outside `[0.0, 1.0]` are clamped.
    pub fn set_confidence(&mut self, confidence: f32) -> &mut Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the active persona and selection rationale.
    pub fn set_persona(&mut self, persona: &str, rationale: &str) -> &mut Self {
        self.active_persona = Some(persona.to_string());
        self.persona_selection_rationale = Some(rationale.to_string());
        self
    }

    /// Set the current cache state.
    pub fn set_cache_state(&mut self, state: &str) -> &mut Self {
        self.cache_state = Some(state.to_string());
        self
    }

    /// Set thinking summary.
    pub fn set_thinking_summary(&mut self, summary: &str) -> &mut Self {
        self.thinking_summary = Some(summary.to_string());
        self
    }

    /// Add a citation.
    pub fn add_citation(&mut self, citation: &str) -> &mut Self {
        self.citations_used.push(citation.to_string());
        self
    }

    /// Add a grounding source.
    pub fn add_grounding_source(&mut self, source: &str) -> &mut Self {
        self.grounding_sources.push(source.to_string());
        self
    }

    /// Add a capability that was used.
    pub fn add_capability_used(&mut self, capability: &str) -> &mut Self {
        self.capabilities_used.push(capability.to_string());
        self
    }

    /// Consume the builder and produce the final [`DecisionExplanation`].
    pub fn build(self) -> DecisionExplanation {
        DecisionExplanation {
            decision_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            decision_type: self.decision_type,
            reasoning_chain: self.reasoning_chain,
            considered_alternatives: self.considered_alternatives,
            confidence: self.confidence,
            context_factors: self.context_factors,
            active_persona: self.active_persona,
            persona_selection_rationale: self.persona_selection_rationale,
            cache_state: self.cache_state,
            thinking_summary: self.thinking_summary,
            citations_used: self.citations_used,
            grounding_sources: self.grounding_sources,
            capabilities_used: self.capabilities_used,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Builder basics -----------------------------------------------------

    #[test]
    fn test_builder_basic() {
        let explanation = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "read_file".into(),
        })
        .build();

        assert!(!explanation.decision_id.is_nil());
        assert!(explanation.reasoning_chain.is_empty());
        assert!(explanation.considered_alternatives.is_empty());
        assert!(explanation.context_factors.is_empty());
    }

    #[test]
    fn test_builder_with_reasoning_steps() {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "write_file".into(),
        });
        builder.add_reasoning_step("Step one", None);
        builder.add_reasoning_step("Step two", Some("evidence"));
        let explanation = builder.build();

        assert_eq!(explanation.reasoning_chain.len(), 2);
        assert_eq!(explanation.reasoning_chain[0].step_number, 1);
        assert_eq!(explanation.reasoning_chain[1].step_number, 2);
        assert_eq!(explanation.reasoning_chain[0].description, "Step one");
        assert_eq!(
            explanation.reasoning_chain[1].evidence.as_deref(),
            Some("evidence")
        );
    }

    #[test]
    fn test_builder_with_alternatives() {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "read_file".into(),
        });
        builder.add_alternative("shell_exec", "Too risky", RiskLevel::Execute);
        builder.add_alternative("network_fetch", "Not needed", RiskLevel::Network);
        let explanation = builder.build();

        assert_eq!(explanation.considered_alternatives.len(), 2);
        assert_eq!(
            explanation.considered_alternatives[0].tool_name,
            "shell_exec"
        );
        assert_eq!(
            explanation.considered_alternatives[0].estimated_risk,
            RiskLevel::Execute
        );
        assert_eq!(
            explanation.considered_alternatives[1].reason_not_selected,
            "Not needed"
        );
    }

    #[test]
    fn test_builder_with_context_factors() {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "read_file".into(),
        });
        builder.add_context_factor("User is admin", FactorInfluence::Positive);
        builder.add_context_factor("Network is slow", FactorInfluence::Negative);
        builder.add_context_factor("Disk usage normal", FactorInfluence::Neutral);
        let explanation = builder.build();

        assert_eq!(explanation.context_factors.len(), 3);
        assert_eq!(
            explanation.context_factors[0].influence,
            FactorInfluence::Positive
        );
        assert_eq!(
            explanation.context_factors[1].influence,
            FactorInfluence::Negative
        );
        assert_eq!(
            explanation.context_factors[2].influence,
            FactorInfluence::Neutral
        );
    }

    #[test]
    fn test_builder_default_confidence() {
        let explanation = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "noop".into(),
        })
        .build();

        assert!((explanation.confidence - 0.5).abs() < f32::EPSILON);
    }

    // -- Serialization ------------------------------------------------------

    #[test]
    fn test_serialization_roundtrip() {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "read_file".into(),
        });
        builder.add_reasoning_step("Check permissions", Some("policy"));
        builder.add_alternative("write_file", "Not applicable", RiskLevel::Write);
        builder.add_context_factor("Sandbox active", FactorInfluence::Positive);
        builder.set_confidence(0.85);
        let original = builder.build();

        let json = serde_json::to_string(&original).expect("serialize");
        let restored: DecisionExplanation = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(original.decision_id, restored.decision_id);
        assert!((original.confidence - restored.confidence).abs() < f32::EPSILON);
        assert_eq!(
            original.reasoning_chain.len(),
            restored.reasoning_chain.len()
        );
        assert_eq!(
            original.considered_alternatives.len(),
            restored.considered_alternatives.len()
        );
    }

    #[test]
    fn test_deserialization_missing_fields() {
        let bad_json = r#"{
            "decision_id": "00000000-0000-0000-0000-000000000000",
            "timestamp": "2025-01-01T00:00:00Z",
            "decision_type": { "ToolSelection": { "selected_tool": "x" } },
            "reasoning_chain": [],
            "considered_alternatives": [],
            "confidence": 0.5
        }"#;

        let result = serde_json::from_str::<DecisionExplanation>(bad_json);
        assert!(result.is_err(), "Missing field should cause an error");
    }

    // -- DecisionType variants -----------------------------------------------

    #[test]
    fn test_decision_type_variants() {
        let tool = DecisionType::ToolSelection {
            selected_tool: "grep".into(),
        };
        let param = DecisionType::ParameterChoice {
            tool: "grep".into(),
            parameter: "pattern".into(),
        };
        let decomp = DecisionType::TaskDecomposition {
            sub_tasks: vec!["a".into(), "b".into()],
        };
        let recovery = DecisionType::ErrorRecovery {
            error: "timeout".into(),
            strategy: "retry".into(),
        };

        let jsons: Vec<String> = [&tool, &param, &decomp, &recovery]
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect();

        let unique: std::collections::HashSet<&String> = jsons.iter().collect();
        assert_eq!(unique.len(), 4);
    }

    // -- Scenario-oriented tests --------------------------------------------

    #[test]
    fn test_explanation_builder_tool_selection() {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "read_file".into(),
        });
        builder.add_reasoning_step("User wants to view a config", None);
        builder.add_reasoning_step("read_file has ReadOnly risk", Some("risk matrix"));
        builder.add_alternative("shell_exec", "Unnecessary privileges", RiskLevel::Execute);
        builder.set_confidence(0.92);
        let explanation = builder.build();

        match &explanation.decision_type {
            DecisionType::ToolSelection { selected_tool } => {
                assert_eq!(selected_tool, "read_file");
            }
            other => panic!("Expected ToolSelection, got {:?}", other),
        }
        assert_eq!(explanation.reasoning_chain.len(), 2);
        assert_eq!(explanation.considered_alternatives.len(), 1);
        assert!((explanation.confidence - 0.92).abs() < f32::EPSILON);
    }

    #[test]
    fn test_explanation_builder_error_recovery() {
        let mut builder = ExplanationBuilder::new(DecisionType::ErrorRecovery {
            error: "connection reset".into(),
            strategy: "exponential backoff".into(),
        });
        builder.add_reasoning_step("Network call failed", Some("HTTP 503"));
        builder.add_reasoning_step("Retrying with backoff", None);
        builder.set_confidence(0.7);
        let explanation = builder.build();

        match &explanation.decision_type {
            DecisionType::ErrorRecovery { error, strategy } => {
                assert_eq!(error, "connection reset");
                assert_eq!(strategy, "exponential backoff");
            }
            other => panic!("Expected ErrorRecovery, got {:?}", other),
        }
        assert!((explanation.confidence - 0.7).abs() < f32::EPSILON);
    }

    // -- Confidence clamping ------------------------------------------------

    #[test]
    fn test_confidence_clamping() {
        let mut builder_high = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "x".into(),
        });
        builder_high.set_confidence(1.5);
        let too_high = builder_high.build();
        assert!((too_high.confidence - 1.0).abs() < f32::EPSILON);

        let mut builder_low = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "x".into(),
        });
        builder_low.set_confidence(-0.3);
        let too_low = builder_low.build();
        assert!(too_low.confidence.abs() < f32::EPSILON);
    }

    // -- Edge cases ---------------------------------------------------------

    #[test]
    fn test_empty_explanation() {
        let explanation = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: String::new(),
        })
        .build();

        assert!(explanation.reasoning_chain.is_empty());
        assert!(explanation.considered_alternatives.is_empty());
        assert!(explanation.context_factors.is_empty());
        assert!((explanation.confidence - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_full_explanation() {
        let mut builder = ExplanationBuilder::new(DecisionType::TaskDecomposition {
            sub_tasks: vec!["lint".into(), "test".into(), "deploy".into()],
        });
        builder.add_reasoning_step("Decompose CI pipeline", Some("pipeline.yml"));
        builder.add_reasoning_step("Lint first for fast feedback", None);
        builder.add_reasoning_step("Deploy only after tests pass", Some("policy #5"));
        builder.add_alternative("single_step", "Too monolithic", RiskLevel::Destructive);
        builder.add_alternative("manual_deploy", "Slow", RiskLevel::Network);
        builder.add_context_factor("CI environment available", FactorInfluence::Positive);
        builder.add_context_factor("Production freeze in effect", FactorInfluence::Negative);
        builder.add_context_factor("Team size is medium", FactorInfluence::Neutral);
        builder.set_confidence(0.88);
        let explanation = builder.build();

        assert_eq!(explanation.reasoning_chain.len(), 3);
        assert_eq!(explanation.considered_alternatives.len(), 2);
        assert_eq!(explanation.context_factors.len(), 3);
        assert!((explanation.confidence - 0.88).abs() < f32::EPSILON);

        match &explanation.decision_type {
            DecisionType::TaskDecomposition { sub_tasks } => {
                assert_eq!(sub_tasks.len(), 3);
                assert_eq!(sub_tasks[0], "lint");
            }
            other => panic!("Expected TaskDecomposition, got {:?}", other),
        }

        for (i, step) in explanation.reasoning_chain.iter().enumerate() {
            assert_eq!(step.step_number, i + 1);
        }
    }

    #[test]
    fn test_reasoning_step_with_evidence() {
        let step = ReasoningStep {
            step_number: 1,
            description: "Evaluated risk".into(),
            evidence: Some("audit log entry #42".into()),
        };
        assert_eq!(step.evidence.as_deref(), Some("audit log entry #42"));
    }

    #[test]
    fn test_reasoning_step_without_evidence() {
        let step = ReasoningStep {
            step_number: 1,
            description: "Intuitive judgement".into(),
            evidence: None,
        };
        assert!(step.evidence.is_none());
    }

    #[test]
    fn test_decision_explanation_with_thinking() {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "shell_exec".into(),
        });
        builder.set_thinking_summary(
            "Analyzed the user's request for file deletion and verified the path is safe",
        );
        builder.add_capability_used("extended_thinking");
        let explanation = builder.build();

        assert!(explanation.thinking_summary.is_some());
        assert_eq!(explanation.capabilities_used.len(), 1);
    }

    #[test]
    fn test_decision_explanation_with_citations() {
        let mut builder = ExplanationBuilder::new(DecisionType::ToolSelection {
            selected_tool: "file_read".into(),
        });
        builder
            .add_citation("docs/api.md:42")
            .add_citation("README.md:10")
            .add_grounding_source("https://docs.rs/tokio/latest");
        let explanation = builder.build();

        assert_eq!(explanation.citations_used.len(), 2);
        assert_eq!(explanation.grounding_sources.len(), 1);
    }

    #[test]
    fn test_decision_explanation_backward_compat() {
        // Old JSON without new fields should still deserialize
        let json = r#"{
            "decision_id": "00000000-0000-0000-0000-000000000000",
            "timestamp": "2024-01-01T00:00:00Z",
            "decision_type": {"ToolSelection": {"selected_tool": "file_read"}},
            "reasoning_chain": [],
            "considered_alternatives": [],
            "confidence": 0.8,
            "context_factors": []
        }"#;
        let explanation: DecisionExplanation = serde_json::from_str(json).unwrap();
        assert!(explanation.thinking_summary.is_none());
        assert!(explanation.citations_used.is_empty());
        assert!(explanation.grounding_sources.is_empty());
        assert!(explanation.capabilities_used.is_empty());
    }
}
