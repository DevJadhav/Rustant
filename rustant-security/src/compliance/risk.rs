//! Risk scoring — multi-dimensional risk assessment for projects.
//!
//! Combines security findings, code quality, dependency health, and
//! compliance status into a composite risk score.

use serde::{Deserialize, Serialize};

/// Overall risk assessment for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// Composite risk score (0-100, higher = more risk).
    pub score: f64,
    /// Risk level classification.
    pub level: RiskLevel,
    /// Individual dimension scores.
    pub dimensions: RiskDimensions,
    /// Business context multiplier applied.
    pub context_multiplier: f64,
    /// Risk factors that contributed most.
    pub top_factors: Vec<RiskFactor>,
    /// Trend compared to previous assessment.
    pub trend: RiskTrend,
}

/// Risk level classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Score 0-20: minimal risk.
    Low,
    /// Score 21-40: acceptable risk.
    Medium,
    /// Score 41-60: elevated risk, attention needed.
    High,
    /// Score 61-80: significant risk, action required.
    Critical,
    /// Score 81-100: extreme risk, immediate action.
    Extreme,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "Low"),
            RiskLevel::Medium => write!(f, "Medium"),
            RiskLevel::High => write!(f, "High"),
            RiskLevel::Critical => write!(f, "Critical"),
            RiskLevel::Extreme => write!(f, "Extreme"),
        }
    }
}

impl RiskLevel {
    /// Classify a score into a risk level.
    pub fn from_score(score: f64) -> Self {
        match score as u32 {
            0..=20 => RiskLevel::Low,
            21..=40 => RiskLevel::Medium,
            41..=60 => RiskLevel::High,
            61..=80 => RiskLevel::Critical,
            _ => RiskLevel::Extreme,
        }
    }
}

/// Individual risk dimension scores.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskDimensions {
    /// Security risk (0-100): based on vulnerability count, severity, exploitability.
    pub security: f64,
    /// Code quality risk (0-100): based on complexity, duplication, test coverage.
    pub quality: f64,
    /// Dependency risk (0-100): based on outdated deps, known vulns, license issues.
    pub dependency: f64,
    /// Compliance risk (0-100): based on policy violations, missing controls.
    pub compliance: f64,
}

/// A specific risk factor contributing to the score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Factor description.
    pub description: String,
    /// Impact on score (positive = increases risk).
    pub impact: f64,
    /// Dimension this factor affects.
    pub dimension: String,
    /// Suggested mitigation.
    pub mitigation: Option<String>,
}

/// Risk trend compared to previous assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskTrend {
    /// Risk is decreasing.
    Improving,
    /// Risk is stable.
    Stable,
    /// Risk is increasing.
    Worsening,
    /// No previous data for comparison.
    Unknown,
}

impl std::fmt::Display for RiskTrend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskTrend::Improving => write!(f, "Improving"),
            RiskTrend::Stable => write!(f, "Stable"),
            RiskTrend::Worsening => write!(f, "Worsening"),
            RiskTrend::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Business context for risk adjustment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessContext {
    /// Is this internet-exposed?
    pub internet_exposed: bool,
    /// Data sensitivity level (1-5).
    pub data_sensitivity: u8,
    /// Deployment frequency (higher = more risk from changes).
    pub deployment_frequency: DeploymentFrequency,
    /// Approximate user base size.
    pub user_base: UserBaseSize,
}

impl Default for BusinessContext {
    fn default() -> Self {
        Self {
            internet_exposed: false,
            data_sensitivity: 1,
            deployment_frequency: DeploymentFrequency::Weekly,
            user_base: UserBaseSize::Small,
        }
    }
}

/// Deployment frequency classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentFrequency {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
}

/// User base size classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserBaseSize {
    /// < 100 users.
    Small,
    /// 100-10,000 users.
    Medium,
    /// 10,000-1M users.
    Large,
    /// > 1M users.
    Enterprise,
}

/// Input data for risk calculation.
#[derive(Debug, Clone, Default)]
pub struct RiskInput {
    /// Number of critical findings.
    pub critical_findings: usize,
    /// Number of high findings.
    pub high_findings: usize,
    /// Number of medium findings.
    pub medium_findings: usize,
    /// Number of low findings.
    pub low_findings: usize,
    /// Average cyclomatic complexity.
    pub avg_complexity: f64,
    /// Code duplication percentage.
    pub duplication_pct: f64,
    /// Test coverage percentage (0-100).
    pub test_coverage: f64,
    /// Number of outdated dependencies.
    pub outdated_deps: usize,
    /// Number of dependencies with known vulnerabilities.
    pub vulnerable_deps: usize,
    /// Number of license policy violations.
    pub license_violations: usize,
    /// Number of policy gate failures.
    pub policy_failures: usize,
    /// Total dependency count.
    pub total_deps: usize,
}

/// Risk calculator.
pub struct RiskCalculator {
    /// Weight for security dimension (default 0.40).
    pub security_weight: f64,
    /// Weight for quality dimension (default 0.20).
    pub quality_weight: f64,
    /// Weight for dependency dimension (default 0.25).
    pub dependency_weight: f64,
    /// Weight for compliance dimension (default 0.15).
    pub compliance_weight: f64,
}

impl Default for RiskCalculator {
    fn default() -> Self {
        Self {
            security_weight: 0.40,
            quality_weight: 0.20,
            dependency_weight: 0.25,
            compliance_weight: 0.15,
        }
    }
}

impl RiskCalculator {
    /// Calculate risk assessment from input data.
    pub fn calculate(&self, input: &RiskInput, context: &BusinessContext) -> RiskAssessment {
        let dimensions = self.calculate_dimensions(input);
        let mut factors = Vec::new();

        // Calculate raw composite score
        let raw_score = (dimensions.security * self.security_weight)
            + (dimensions.quality * self.quality_weight)
            + (dimensions.dependency * self.dependency_weight)
            + (dimensions.compliance * self.compliance_weight);

        // Apply business context multiplier
        let context_multiplier = self.context_multiplier(context);
        let score = (raw_score * context_multiplier).min(100.0);
        let level = RiskLevel::from_score(score);

        // Identify top risk factors
        if input.critical_findings > 0 {
            factors.push(RiskFactor {
                description: format!("{} critical security findings", input.critical_findings),
                impact: input.critical_findings as f64 * 15.0,
                dimension: "security".into(),
                mitigation: Some("Remediate critical findings immediately".into()),
            });
        }

        if input.vulnerable_deps > 0 {
            factors.push(RiskFactor {
                description: format!(
                    "{} dependencies with known vulnerabilities",
                    input.vulnerable_deps
                ),
                impact: input.vulnerable_deps as f64 * 10.0,
                dimension: "dependency".into(),
                mitigation: Some("Update vulnerable dependencies to patched versions".into()),
            });
        }

        if input.avg_complexity > 15.0 {
            factors.push(RiskFactor {
                description: format!("High average complexity: {:.1}", input.avg_complexity),
                impact: (input.avg_complexity - 10.0) * 2.0,
                dimension: "quality".into(),
                mitigation: Some(
                    "Refactor complex functions to reduce cyclomatic complexity".into(),
                ),
            });
        }

        if input.policy_failures > 0 {
            factors.push(RiskFactor {
                description: format!("{} policy gate failures", input.policy_failures),
                impact: input.policy_failures as f64 * 12.0,
                dimension: "compliance".into(),
                mitigation: Some("Address policy violations before deployment".into()),
            });
        }

        if input.test_coverage < 50.0 {
            factors.push(RiskFactor {
                description: format!("Low test coverage: {:.0}%", input.test_coverage),
                impact: (50.0 - input.test_coverage) * 0.5,
                dimension: "quality".into(),
                mitigation: Some("Increase test coverage to at least 60%".into()),
            });
        }

        // Sort by impact descending, take top 5
        factors.sort_by(|a, b| {
            b.impact
                .partial_cmp(&a.impact)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        factors.truncate(5);

        RiskAssessment {
            score,
            level,
            dimensions,
            context_multiplier,
            top_factors: factors,
            trend: RiskTrend::Unknown,
        }
    }

    /// Calculate individual dimension scores.
    fn calculate_dimensions(&self, input: &RiskInput) -> RiskDimensions {
        RiskDimensions {
            security: self.security_score(input),
            quality: self.quality_score(input),
            dependency: self.dependency_score(input),
            compliance: self.compliance_score(input),
        }
    }

    /// Security score: based on finding count and severity.
    fn security_score(&self, input: &RiskInput) -> f64 {
        let weighted_findings = (input.critical_findings as f64 * 25.0)
            + (input.high_findings as f64 * 10.0)
            + (input.medium_findings as f64 * 3.0)
            + (input.low_findings as f64 * 0.5);
        // Normalize to 0-100 range (100 = very risky)
        weighted_findings.min(100.0)
    }

    /// Quality score: based on complexity, duplication, coverage.
    fn quality_score(&self, input: &RiskInput) -> f64 {
        let complexity_risk = if input.avg_complexity > 25.0 {
            40.0
        } else if input.avg_complexity > 15.0 {
            25.0
        } else if input.avg_complexity > 10.0 {
            15.0
        } else {
            5.0
        };

        let duplication_risk = (input.duplication_pct * 2.0).min(30.0);

        let coverage_risk = if input.test_coverage >= 80.0 {
            0.0
        } else if input.test_coverage >= 60.0 {
            10.0
        } else if input.test_coverage >= 40.0 {
            20.0
        } else {
            30.0
        };

        (complexity_risk + duplication_risk + coverage_risk).min(100.0)
    }

    /// Dependency score: based on outdated/vulnerable deps.
    fn dependency_score(&self, input: &RiskInput) -> f64 {
        let vuln_risk = (input.vulnerable_deps as f64 * 15.0).min(60.0);
        let outdated_risk = if input.total_deps > 0 {
            (input.outdated_deps as f64 / input.total_deps as f64 * 40.0).min(40.0)
        } else {
            0.0
        };
        (vuln_risk + outdated_risk).min(100.0)
    }

    /// Compliance score: based on policy failures and license issues.
    fn compliance_score(&self, input: &RiskInput) -> f64 {
        let policy_risk = (input.policy_failures as f64 * 20.0).min(60.0);
        let license_risk = (input.license_violations as f64 * 15.0).min(40.0);
        (policy_risk + license_risk).min(100.0)
    }

    /// Calculate business context multiplier.
    fn context_multiplier(&self, context: &BusinessContext) -> f64 {
        let mut mult = 1.0;

        if context.internet_exposed {
            mult *= 1.3;
        }

        mult *= match context.data_sensitivity {
            5 => 1.5,
            4 => 1.3,
            3 => 1.1,
            _ => 1.0,
        };

        mult *= match context.deployment_frequency {
            DeploymentFrequency::Daily => 1.1,
            DeploymentFrequency::Weekly => 1.0,
            DeploymentFrequency::Monthly => 0.95,
            DeploymentFrequency::Quarterly => 0.9,
        };

        mult *= match context.user_base {
            UserBaseSize::Enterprise => 1.4,
            UserBaseSize::Large => 1.2,
            UserBaseSize::Medium => 1.0,
            UserBaseSize::Small => 0.9,
        };

        mult
    }

    /// Determine trend by comparing current and previous scores.
    pub fn determine_trend(current: f64, previous: f64) -> RiskTrend {
        let delta = current - previous;
        if delta < -5.0 {
            RiskTrend::Improving
        } else if delta > 5.0 {
            RiskTrend::Worsening
        } else {
            RiskTrend::Stable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_from_score() {
        assert_eq!(RiskLevel::from_score(10.0), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(30.0), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(50.0), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(70.0), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(90.0), RiskLevel::Extreme);
    }

    #[test]
    fn test_clean_project_low_risk() {
        let calc = RiskCalculator::default();
        let input = RiskInput {
            test_coverage: 85.0,
            avg_complexity: 5.0,
            ..Default::default()
        };
        let context = BusinessContext::default();
        let assessment = calc.calculate(&input, &context);

        assert!(
            assessment.score < 20.0,
            "Clean project should be low risk, got {}",
            assessment.score
        );
        assert_eq!(assessment.level, RiskLevel::Low);
    }

    #[test]
    fn test_critical_findings_high_risk() {
        let calc = RiskCalculator::default();
        let input = RiskInput {
            critical_findings: 3,
            high_findings: 5,
            test_coverage: 30.0,
            avg_complexity: 20.0,
            ..Default::default()
        };
        let context = BusinessContext::default();
        let assessment = calc.calculate(&input, &context);

        assert!(
            assessment.score > 40.0,
            "Project with critical findings should be high risk, got {}",
            assessment.score
        );
        assert!(!assessment.top_factors.is_empty());
    }

    #[test]
    fn test_context_multiplier_internet_exposed() {
        let calc = RiskCalculator::default();
        let input = RiskInput {
            high_findings: 2,
            test_coverage: 50.0,
            ..Default::default()
        };

        let internal = BusinessContext::default();
        let exposed = BusinessContext {
            internet_exposed: true,
            data_sensitivity: 4,
            user_base: UserBaseSize::Large,
            ..Default::default()
        };

        let internal_score = calc.calculate(&input, &internal).score;
        let exposed_score = calc.calculate(&input, &exposed).score;

        assert!(
            exposed_score > internal_score,
            "Internet-exposed should have higher risk: {exposed_score} vs {internal_score}"
        );
    }

    #[test]
    fn test_dependency_risk() {
        let calc = RiskCalculator::default();
        let input = RiskInput {
            vulnerable_deps: 3,
            outdated_deps: 10,
            total_deps: 50,
            test_coverage: 80.0,
            ..Default::default()
        };
        let context = BusinessContext::default();
        let assessment = calc.calculate(&input, &context);

        assert!(
            assessment.dimensions.dependency > 30.0,
            "Should have high dependency risk: {}",
            assessment.dimensions.dependency
        );
    }

    #[test]
    fn test_compliance_risk() {
        let calc = RiskCalculator::default();
        let input = RiskInput {
            policy_failures: 2,
            license_violations: 3,
            test_coverage: 80.0,
            ..Default::default()
        };
        let context = BusinessContext::default();
        let assessment = calc.calculate(&input, &context);

        assert!(
            assessment.dimensions.compliance > 50.0,
            "Should have high compliance risk: {}",
            assessment.dimensions.compliance
        );
    }

    #[test]
    fn test_trend_detection() {
        assert_eq!(
            RiskCalculator::determine_trend(30.0, 50.0),
            RiskTrend::Improving
        );
        assert_eq!(
            RiskCalculator::determine_trend(50.0, 30.0),
            RiskTrend::Worsening
        );
        assert_eq!(
            RiskCalculator::determine_trend(50.0, 48.0),
            RiskTrend::Stable
        );
    }

    #[test]
    fn test_risk_capped_at_100() {
        let calc = RiskCalculator::default();
        let input = RiskInput {
            critical_findings: 20,
            high_findings: 50,
            medium_findings: 100,
            vulnerable_deps: 30,
            policy_failures: 10,
            test_coverage: 0.0,
            avg_complexity: 50.0,
            duplication_pct: 50.0,
            ..Default::default()
        };
        let context = BusinessContext {
            internet_exposed: true,
            data_sensitivity: 5,
            user_base: UserBaseSize::Enterprise,
            deployment_frequency: DeploymentFrequency::Daily,
        };
        let assessment = calc.calculate(&input, &context);

        assert!(
            assessment.score <= 100.0,
            "Score should be capped at 100, got {}",
            assessment.score
        );
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "Low");
        assert_eq!(RiskLevel::Extreme.to_string(), "Extreme");
    }

    #[test]
    fn test_risk_trend_display() {
        assert_eq!(RiskTrend::Improving.to_string(), "Improving");
        assert_eq!(RiskTrend::Worsening.to_string(), "Worsening");
    }
}
