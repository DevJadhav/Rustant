//! Dynamic risk scoring based on environmental context.
//!
//! Adjusts tool risk levels based on time of day, error history,
//! trust level, circuit breaker state, and active incidents.
//! Bridges ML safety checks into the core SafetyGuardian.

use crate::types::RiskLevel;
use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};

/// Environmental context for risk assessment.
#[derive(Debug, Clone)]
pub struct RiskContext {
    /// Current hour (0-23) in local time.
    pub hour: u32,
    /// Number of consecutive errors in the current session.
    pub consecutive_errors: usize,
    /// Current progressive trust level (0-4).
    pub trust_level: u8,
    /// Whether the circuit breaker is open.
    pub circuit_breaker_open: bool,
    /// Whether we're in a production environment.
    pub is_production_env: bool,
    /// Whether there's an active incident (SRE mode).
    pub active_incident: bool,
}

impl RiskContext {
    /// Create a context snapshot from current system state.
    pub fn current(
        consecutive_errors: usize,
        trust_level: u8,
        circuit_breaker_open: bool,
        is_production_env: bool,
        active_incident: bool,
    ) -> Self {
        Self {
            hour: Local::now().hour(),
            consecutive_errors,
            trust_level,
            circuit_breaker_open,
            is_production_env,
            active_incident,
        }
    }
}

impl Default for RiskContext {
    fn default() -> Self {
        Self {
            hour: Local::now().hour(),
            consecutive_errors: 0,
            trust_level: 2,
            circuit_breaker_open: false,
            is_production_env: false,
            active_incident: false,
        }
    }
}

/// How a risk level should be adjusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RiskAdjustment {
    /// Escalate the risk level by one step.
    Escalate,
    /// De-escalate the risk level by one step.
    DeEscalate,
    /// Override to a specific risk level.
    Override(RiskLevel),
    /// No adjustment.
    NoChange,
}

/// Scope of a risk modifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskModifierScope {
    /// Apply to all tools.
    AllTools,
    /// Apply to a specific tool.
    SpecificTool(String),
    /// Apply to tools at a specific risk level.
    RiskLevel(RiskLevel),
}

/// A risk modifier that adjusts risk based on conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskModifier {
    /// Human-readable name of this modifier.
    pub name: String,
    /// What this modifier applies to.
    pub scope: RiskModifierScope,
    /// The adjustment to make.
    pub adjustment: RiskAdjustment,
    /// Whether this modifier is currently active.
    pub active: bool,
}

/// Dynamic risk scorer that adjusts risk levels based on context.
pub struct DynamicRiskScorer {
    /// Active risk modifiers.
    modifiers: Vec<RiskModifier>,
}

impl DynamicRiskScorer {
    /// Create a new scorer with default modifiers.
    pub fn new() -> Self {
        Self {
            modifiers: Self::default_modifiers(),
        }
    }

    /// Evaluate the effective risk level for a tool given the current context.
    pub fn evaluate(
        &self,
        tool_name: &str,
        base_risk: RiskLevel,
        context: &RiskContext,
    ) -> RiskLevel {
        let mut risk = base_risk;

        // Apply context-aware rules
        risk = self.apply_time_rules(risk, context);
        risk = self.apply_error_rules(risk, context);
        risk = self.apply_incident_rules(risk, tool_name, context);
        risk = self.apply_circuit_breaker_rules(risk, context);
        risk = self.apply_production_rules(risk, context);

        // Apply custom modifiers
        for modifier in &self.modifiers {
            if !modifier.active {
                continue;
            }
            if self.modifier_applies(modifier, tool_name, &base_risk) {
                risk = self.apply_adjustment(risk, &modifier.adjustment);
            }
        }

        risk
    }

    /// Late night rule: destructive actions escalate one level between 23:00-06:00.
    fn apply_time_rules(&self, risk: RiskLevel, context: &RiskContext) -> RiskLevel {
        if context.hour >= 23 || context.hour < 6 {
            if risk == RiskLevel::Write {
                return RiskLevel::Execute;
            }
            if risk == RiskLevel::Execute {
                return RiskLevel::Destructive;
            }
        }
        risk
    }

    /// High error rate: escalate after 3+ consecutive errors.
    fn apply_error_rules(&self, risk: RiskLevel, context: &RiskContext) -> RiskLevel {
        if context.consecutive_errors >= 3 && risk >= RiskLevel::Write {
            return self.escalate(risk);
        }
        risk
    }

    /// Active incident: deployment tools escalate to Destructive.
    fn apply_incident_rules(
        &self,
        risk: RiskLevel,
        tool_name: &str,
        context: &RiskContext,
    ) -> RiskLevel {
        if context.active_incident {
            let deployment_tools = ["deployment_intel", "kubernetes", "shell_exec"];
            if deployment_tools.contains(&tool_name) && risk >= RiskLevel::Write {
                return RiskLevel::Destructive;
            }
        }
        risk
    }

    /// Circuit breaker open: only allow read-only.
    fn apply_circuit_breaker_rules(&self, risk: RiskLevel, context: &RiskContext) -> RiskLevel {
        if context.circuit_breaker_open && risk > RiskLevel::ReadOnly {
            return RiskLevel::Destructive; // Will trigger approval/denial
        }
        risk
    }

    /// Production environment: escalate network and execute actions.
    fn apply_production_rules(&self, risk: RiskLevel, context: &RiskContext) -> RiskLevel {
        if context.is_production_env && risk == RiskLevel::Network {
            return RiskLevel::Destructive;
        }
        risk
    }

    /// Escalate a risk level by one step.
    fn escalate(&self, risk: RiskLevel) -> RiskLevel {
        match risk {
            RiskLevel::ReadOnly => RiskLevel::Write,
            RiskLevel::Write => RiskLevel::Execute,
            RiskLevel::Execute => RiskLevel::Network,
            RiskLevel::Network => RiskLevel::Destructive,
            RiskLevel::Destructive => RiskLevel::Destructive,
        }
    }

    /// De-escalate a risk level by one step.
    fn de_escalate(&self, risk: RiskLevel) -> RiskLevel {
        match risk {
            RiskLevel::Destructive => RiskLevel::Network,
            RiskLevel::Network => RiskLevel::Execute,
            RiskLevel::Execute => RiskLevel::Write,
            RiskLevel::Write => RiskLevel::ReadOnly,
            RiskLevel::ReadOnly => RiskLevel::ReadOnly,
        }
    }

    /// Apply a risk adjustment.
    fn apply_adjustment(&self, risk: RiskLevel, adjustment: &RiskAdjustment) -> RiskLevel {
        match adjustment {
            RiskAdjustment::Escalate => self.escalate(risk),
            RiskAdjustment::DeEscalate => self.de_escalate(risk),
            RiskAdjustment::Override(level) => *level,
            RiskAdjustment::NoChange => risk,
        }
    }

    /// Check if a modifier applies to the given tool and risk level.
    fn modifier_applies(&self, modifier: &RiskModifier, tool_name: &str, risk: &RiskLevel) -> bool {
        match &modifier.scope {
            RiskModifierScope::AllTools => true,
            RiskModifierScope::SpecificTool(name) => name == tool_name,
            RiskModifierScope::RiskLevel(level) => risk == level,
        }
    }

    /// Add a custom risk modifier.
    pub fn add_modifier(&mut self, modifier: RiskModifier) {
        self.modifiers.push(modifier);
    }

    /// Get active modifiers.
    pub fn active_modifiers(&self) -> Vec<&RiskModifier> {
        self.modifiers.iter().filter(|m| m.active).collect()
    }

    /// Default risk modifiers (always present).
    fn default_modifiers() -> Vec<RiskModifier> {
        vec![] // Built-in rules are in apply_* methods; custom modifiers go here
    }
}

impl Default for DynamicRiskScorer {
    fn default() -> Self {
        Self::new()
    }
}

/// ML safety bridge — checks ML-specific safety concerns.
pub struct MlSafetyBridge;

impl MlSafetyBridge {
    /// Check an ML action for safety concerns.
    ///
    /// Returns a warning message if the action has safety implications.
    pub fn check_ml_action(tool_name: &str, args: &serde_json::Value) -> Option<String> {
        // Check for PII in training data paths
        let pii_warning = if matches!(tool_name, "ml_train" | "ml_finetune" | "ml_dataset_prep") {
            args.get("data_path")
                .and_then(|v| v.as_str())
                .filter(|path| {
                    path.contains("personal")
                        || path.contains("pii")
                        || path.contains("user_data")
                })
                .map(|path| {
                    format!(
                        "Warning: Training data path '{path}' may contain PII. Consider running PII scan first."
                    )
                })
        } else {
            None
        };

        if pii_warning.is_some() {
            return pii_warning;
        }

        // Check for alignment review requirement (only tools not already covered above)
        if matches!(tool_name, "ml_finetune" | "ml_adapter") {
            Some(
                "Note: Fine-tuning models should include alignment evaluation afterward."
                    .to_string(),
            )
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_late_night_escalation() {
        let scorer = DynamicRiskScorer::new();
        let context = RiskContext {
            hour: 2, // 2 AM
            ..Default::default()
        };

        // Write should escalate to Execute at night
        let risk = scorer.evaluate("file_write", RiskLevel::Write, &context);
        assert_eq!(risk, RiskLevel::Execute);
    }

    #[test]
    fn test_daytime_no_change() {
        let scorer = DynamicRiskScorer::new();
        let context = RiskContext {
            hour: 14, // 2 PM
            ..Default::default()
        };

        let risk = scorer.evaluate("file_write", RiskLevel::Write, &context);
        assert_eq!(risk, RiskLevel::Write);
    }

    #[test]
    fn test_error_escalation() {
        let scorer = DynamicRiskScorer::new();
        let context = RiskContext {
            hour: 12,
            consecutive_errors: 5,
            ..Default::default()
        };

        let risk = scorer.evaluate("shell_exec", RiskLevel::Write, &context);
        assert!(risk > RiskLevel::Write);
    }

    #[test]
    fn test_active_incident_escalation() {
        let scorer = DynamicRiskScorer::new();
        let context = RiskContext {
            hour: 12,
            active_incident: true,
            ..Default::default()
        };

        let risk = scorer.evaluate("deployment_intel", RiskLevel::Write, &context);
        assert_eq!(risk, RiskLevel::Destructive);
    }

    #[test]
    fn test_circuit_breaker_blocks() {
        let scorer = DynamicRiskScorer::new();
        let context = RiskContext {
            hour: 12,
            circuit_breaker_open: true,
            ..Default::default()
        };

        let risk = scorer.evaluate("file_write", RiskLevel::Write, &context);
        assert_eq!(risk, RiskLevel::Destructive);

        // ReadOnly should pass through
        let risk = scorer.evaluate("file_read", RiskLevel::ReadOnly, &context);
        assert_eq!(risk, RiskLevel::ReadOnly);
    }

    #[test]
    fn test_custom_modifier() {
        let mut scorer = DynamicRiskScorer::new();
        scorer.add_modifier(RiskModifier {
            name: "Always escalate git_commit".into(),
            scope: RiskModifierScope::SpecificTool("git_commit".into()),
            adjustment: RiskAdjustment::Escalate,
            active: true,
        });

        let context = RiskContext {
            hour: 12,
            ..Default::default()
        };

        let risk = scorer.evaluate("git_commit", RiskLevel::Write, &context);
        assert_eq!(risk, RiskLevel::Execute);

        // Other tools unaffected
        let risk = scorer.evaluate("file_write", RiskLevel::Write, &context);
        assert_eq!(risk, RiskLevel::Write);
    }

    #[test]
    fn test_ml_safety_bridge() {
        let warning = MlSafetyBridge::check_ml_action(
            "ml_train",
            &serde_json::json!({"data_path": "/data/personal_records"}),
        );
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("PII"));

        let warning = MlSafetyBridge::check_ml_action(
            "ml_train",
            &serde_json::json!({"data_path": "/data/clean_dataset"}),
        );
        assert!(warning.is_none());
    }

    #[test]
    fn test_finetune_alignment_warning() {
        let warning = MlSafetyBridge::check_ml_action("ml_finetune", &serde_json::json!({}));
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("alignment"));
    }
}
