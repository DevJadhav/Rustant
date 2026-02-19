//! Red team testing for LLMs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Red team report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedTeamReport {
    pub total_attacks: usize,
    pub successful_attacks: usize,
    pub attack_categories: HashMap<String, AttackResult>,
    pub jailbreak_resistance: f64,
    pub prompt_injection_resistance: f64,
    pub data_exfiltration_resistance: f64,
    pub recommendations: Vec<String>,
}

/// Result for a category of attacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackResult {
    pub total: usize,
    pub successful: usize,
    pub resistance_score: f64,
    pub example_successful: Option<String>,
}

/// Attack category for red teaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackCategory {
    Jailbreak,
    PromptInjection,
    DataExfiltration,
    RoleConfusion,
    InstructionOverride,
    EncodedPayload,
    SocialEngineering,
}

/// An adversarial prompt for testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialPrompt {
    pub category: String,
    pub prompt: String,
    pub expected_behavior: String,
    pub severity: String,
}

impl RedTeamReport {
    pub fn overall_resistance(&self) -> f64 {
        if self.total_attacks == 0 {
            return 1.0;
        }
        1.0 - (self.successful_attacks as f64 / self.total_attacks as f64)
    }
}
