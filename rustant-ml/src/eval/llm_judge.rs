//! LLM-as-Judge evaluator.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A judging rubric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgingRubric {
    pub name: String,
    pub criteria: Vec<JudgingCriterion>,
    pub scale_min: f32,
    pub scale_max: f32,
}

/// A single judging criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgingCriterion {
    pub name: String,
    pub description: String,
    pub weight: f32,
}

/// LLM judgement result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmJudgement {
    pub scores: HashMap<String, f32>,
    pub overall_score: f32,
    pub reasoning: String,
    pub confidence_interval: (f32, f32),
    pub bias_indicators: Vec<String>,
}

/// Calibration settings for LLM judge to reduce bias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeCalibration {
    pub position_bias_correction: bool,
    pub length_bias_correction: bool,
    pub calibration_samples: usize,
    pub temperature: f64,
}

impl Default for JudgeCalibration {
    fn default() -> Self {
        Self {
            position_bias_correction: true,
            length_bias_correction: true,
            calibration_samples: 10,
            temperature: 0.3,
        }
    }
}

/// LLM-as-Judge evaluator.
pub struct LlmJudgeEvaluator {
    rubric: JudgingRubric,
    /// Calibration settings for bias correction.
    calibration: JudgeCalibration,
    /// Provider model identifier (can't use Arc<dyn LlmProvider> due to Serialize constraint).
    provider_model: Option<String>,
}

impl LlmJudgeEvaluator {
    pub fn new(rubric: JudgingRubric) -> Self {
        Self {
            rubric,
            calibration: JudgeCalibration::default(),
            provider_model: None,
        }
    }

    /// Create with explicit calibration and provider model.
    pub fn with_calibration(mut self, calibration: JudgeCalibration) -> Self {
        self.calibration = calibration;
        self
    }

    /// Set the provider model identifier.
    pub fn with_provider_model(mut self, model: String) -> Self {
        self.provider_model = Some(model);
        self
    }

    /// Get calibration settings.
    pub fn calibration(&self) -> &JudgeCalibration {
        &self.calibration
    }

    /// Get provider model identifier.
    pub fn provider_model(&self) -> Option<&str> {
        self.provider_model.as_deref()
    }

    pub fn rubric(&self) -> &JudgingRubric {
        &self.rubric
    }

    /// Evaluate a response against the rubric (stub — real impl uses LlmProvider).
    pub async fn evaluate(&self, _query: &str, response: &str) -> LlmJudgement {
        let scores: HashMap<String, f32> = self
            .rubric
            .criteria
            .iter()
            .map(|c| (c.name.clone(), 0.5))
            .collect();
        let overall = scores.values().sum::<f32>() / scores.len().max(1) as f32;

        LlmJudgement {
            scores,
            overall_score: overall,
            reasoning: format!("Evaluated response of {} chars", response.len()),
            confidence_interval: (overall - 0.1, overall + 0.1),
            bias_indicators: Vec::new(),
        }
    }
}
