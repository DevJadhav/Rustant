//! Security analytics — MTTR, coverage, compliance rate, and trend tracking.
//!
//! Provides metrics for measuring security posture over time, including
//! a `SecurityAnalyticsCalculator` for computing comprehensive analytics
//! from finding data.

use crate::finding::{Finding, FindingStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Security analytics report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAnalytics {
    /// When this report was generated.
    pub generated_at: DateTime<Utc>,
    /// Mean time to remediation metrics.
    pub mttr: MttrMetrics,
    /// Scanner coverage metrics.
    pub coverage: CoverageMetrics,
    /// Finding distribution.
    pub distribution: FindingDistribution,
    /// Compliance rate (0.0-1.0).
    pub compliance_rate: f64,
    /// False positive rate (if tracked, 0.0-1.0).
    pub false_positive_rate: Option<f64>,
    /// Trend data points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trends: Vec<TrendPoint>,
}

/// Mean Time to Remediation metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MttrMetrics {
    /// Overall MTTR in hours.
    pub overall_hours: f64,
    /// MTTR by severity (in hours).
    pub by_severity: HashMap<String, f64>,
    /// Number of resolved findings used in calculation.
    pub sample_size: usize,
}

/// Scanner coverage metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoverageMetrics {
    /// Percentage of codebase scanned (0-100).
    pub codebase_pct: f64,
    /// Number of files scanned.
    pub files_scanned: usize,
    /// Total files in project.
    pub total_files: usize,
    /// Scanners that have been run.
    pub scanners_run: Vec<String>,
    /// Languages covered.
    pub languages_covered: Vec<String>,
}

/// Finding distribution by various dimensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindingDistribution {
    /// By severity.
    pub by_severity: HashMap<String, usize>,
    /// By category.
    pub by_category: HashMap<String, usize>,
    /// By scanner.
    pub by_scanner: HashMap<String, usize>,
    /// By status.
    pub by_status: HashMap<String, usize>,
    /// Total findings.
    pub total: usize,
}

/// A data point for trend tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Total finding count at this point.
    pub total_findings: usize,
    /// Open finding count.
    pub open_findings: usize,
    /// Risk score at this point.
    pub risk_score: f64,
    /// Compliance rate.
    pub compliance_rate: f64,
}

/// Trend analysis over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    /// Data points.
    pub points: Vec<TrendPoint>,
    /// Direction of total findings.
    pub findings_trend: TrendDirection,
    /// Direction of risk score.
    pub risk_trend: TrendDirection,
    /// Direction of compliance rate.
    pub compliance_trend: TrendDirection,
}

/// Trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendDirection {
    Improving,
    Stable,
    Worsening,
}

impl std::fmt::Display for TrendDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrendDirection::Improving => write!(f, "Improving"),
            TrendDirection::Stable => write!(f, "Stable"),
            TrendDirection::Worsening => write!(f, "Worsening"),
        }
    }
}

/// Resolved finding record for MTTR calculation.
#[derive(Debug, Clone)]
pub struct ResolvedFinding {
    /// Severity level.
    pub severity: String,
    /// When it was detected.
    pub detected_at: DateTime<Utc>,
    /// When it was resolved.
    pub resolved_at: DateTime<Utc>,
}

/// Policy evaluation result for compliance rate calculation.
#[derive(Debug, Clone)]
pub struct PolicyResult {
    /// Policy name.
    pub name: String,
    /// Whether the policy passed.
    pub passed: bool,
}

/// Convert a `chrono::Duration` to fractional hours.
pub fn duration_to_hours(duration: chrono::Duration) -> f64 {
    duration.num_minutes() as f64 / 60.0
}

/// Calculate MTTR from resolved finding records.
pub fn calculate_mttr(resolved: &[ResolvedFinding]) -> MttrMetrics {
    if resolved.is_empty() {
        return MttrMetrics::default();
    }

    let mut total_hours = 0.0;
    let mut severity_totals: HashMap<String, (f64, usize)> = HashMap::new();

    for finding in resolved {
        let duration = finding.resolved_at - finding.detected_at;
        let hours = duration_to_hours(duration);

        total_hours += hours;

        let entry = severity_totals
            .entry(finding.severity.clone())
            .or_insert((0.0, 0));
        entry.0 += hours;
        entry.1 += 1;
    }

    let by_severity = severity_totals
        .into_iter()
        .map(|(sev, (total, count))| (sev, total / count as f64))
        .collect();

    MttrMetrics {
        overall_hours: total_hours / resolved.len() as f64,
        by_severity,
        sample_size: resolved.len(),
    }
}

/// Analyze trends from historical data points.
pub fn analyze_trends(points: &[TrendPoint]) -> TrendAnalysis {
    let findings_trend = if points.len() < 2 {
        TrendDirection::Stable
    } else {
        let recent = &points[points.len() - 1];
        let previous = &points[0];
        let delta = recent.open_findings as f64 - previous.open_findings as f64;
        if delta < -2.0 {
            TrendDirection::Improving
        } else if delta > 2.0 {
            TrendDirection::Worsening
        } else {
            TrendDirection::Stable
        }
    };

    let risk_trend = if points.len() < 2 {
        TrendDirection::Stable
    } else {
        let recent = points[points.len() - 1].risk_score;
        let previous = points[0].risk_score;
        let delta = recent - previous;
        if delta < -5.0 {
            TrendDirection::Improving
        } else if delta > 5.0 {
            TrendDirection::Worsening
        } else {
            TrendDirection::Stable
        }
    };

    let compliance_trend = if points.len() < 2 {
        TrendDirection::Stable
    } else {
        let recent = points[points.len() - 1].compliance_rate;
        let previous = points[0].compliance_rate;
        let delta = recent - previous;
        if delta > 5.0 {
            TrendDirection::Improving
        } else if delta < -5.0 {
            TrendDirection::Worsening
        } else {
            TrendDirection::Stable
        }
    };

    TrendAnalysis {
        points: points.to_vec(),
        findings_trend,
        risk_trend,
        compliance_trend,
    }
}

/// Calculator for producing comprehensive `SecurityAnalytics` from finding data.
///
/// Aggregates findings, resolved records, coverage info, and policy results
/// into a single analytics report.
pub struct SecurityAnalyticsCalculator {
    /// All findings (open, resolved, suppressed, etc.).
    findings: Vec<Finding>,
    /// Resolved finding records for MTTR.
    resolved: Vec<ResolvedFinding>,
    /// Files scanned.
    files_scanned: usize,
    /// Total files in the project.
    total_files: usize,
    /// Scanners that were run.
    scanners_run: Vec<String>,
    /// Languages covered by scanning.
    languages_covered: Vec<String>,
    /// Policy evaluation results.
    policy_results: Vec<PolicyResult>,
}

impl SecurityAnalyticsCalculator {
    /// Create a new calculator with the given findings.
    pub fn new(findings: Vec<Finding>) -> Self {
        Self {
            findings,
            resolved: Vec::new(),
            files_scanned: 0,
            total_files: 0,
            scanners_run: Vec::new(),
            languages_covered: Vec::new(),
            policy_results: Vec::new(),
        }
    }

    /// Set resolved finding records for MTTR calculation.
    pub fn with_resolved(mut self, resolved: Vec<ResolvedFinding>) -> Self {
        self.resolved = resolved;
        self
    }

    /// Set coverage information.
    pub fn with_coverage(
        mut self,
        files_scanned: usize,
        total_files: usize,
        scanners_run: Vec<String>,
        languages_covered: Vec<String>,
    ) -> Self {
        self.files_scanned = files_scanned;
        self.total_files = total_files;
        self.scanners_run = scanners_run;
        self.languages_covered = languages_covered;
        self
    }

    /// Set policy evaluation results for compliance rate calculation.
    pub fn with_policies(mut self, results: Vec<PolicyResult>) -> Self {
        self.policy_results = results;
        self
    }

    /// Calculate mean time to remediation from resolved findings.
    pub fn calculate_mttr(&self) -> MttrMetrics {
        calculate_mttr(&self.resolved)
    }

    /// Calculate scanner coverage metrics.
    pub fn calculate_coverage(&self) -> CoverageMetrics {
        let codebase_pct = if self.total_files > 0 {
            (self.files_scanned as f64 / self.total_files as f64) * 100.0
        } else {
            0.0
        };

        CoverageMetrics {
            codebase_pct,
            files_scanned: self.files_scanned,
            total_files: self.total_files,
            scanners_run: self.scanners_run.clone(),
            languages_covered: self.languages_covered.clone(),
        }
    }

    /// Calculate finding distribution by severity, category, scanner, and status.
    pub fn calculate_distribution(&self) -> FindingDistribution {
        let mut by_severity: HashMap<String, usize> = HashMap::new();
        let mut by_category: HashMap<String, usize> = HashMap::new();
        let mut by_scanner: HashMap<String, usize> = HashMap::new();
        let mut by_status: HashMap<String, usize> = HashMap::new();

        for finding in &self.findings {
            *by_severity
                .entry(finding.severity.as_str().to_string())
                .or_insert(0) += 1;
            *by_category
                .entry(format!("{:?}", finding.category).to_lowercase())
                .or_insert(0) += 1;
            *by_scanner
                .entry(finding.provenance.scanner.clone())
                .or_insert(0) += 1;
            *by_status
                .entry(format!("{:?}", finding.status).to_lowercase())
                .or_insert(0) += 1;
        }

        FindingDistribution {
            by_severity,
            by_category,
            by_scanner,
            by_status,
            total: self.findings.len(),
        }
    }

    /// Calculate compliance rate: passed policies / total policies (0.0-1.0).
    /// Returns 1.0 if no policies are configured (fully compliant by default).
    pub fn calculate_compliance_rate(&self) -> f64 {
        if self.policy_results.is_empty() {
            return 1.0;
        }
        let passed = self.policy_results.iter().filter(|p| p.passed).count();
        passed as f64 / self.policy_results.len() as f64
    }

    /// Calculate false positive rate: false positives / total findings (0.0-1.0).
    /// Returns `None` if there are no findings.
    pub fn calculate_false_positive_rate(&self) -> Option<f64> {
        if self.findings.is_empty() {
            return None;
        }
        let false_positives = self
            .findings
            .iter()
            .filter(|f| f.status == FindingStatus::FalsePositive)
            .count();
        Some(false_positives as f64 / self.findings.len() as f64)
    }

    /// Calculate time-series trend points grouped by day.
    ///
    /// Groups findings by their creation date and produces a `TrendPoint` per day,
    /// tracking cumulative totals, open counts, and a simple risk score.
    pub fn calculate_trends(&self) -> Vec<TrendPoint> {
        if self.findings.is_empty() {
            return Vec::new();
        }

        // Group findings by date (day granularity)
        let mut by_date: HashMap<String, Vec<&Finding>> = HashMap::new();
        for f in &self.findings {
            let date_key = f.created_at.format("%Y-%m-%d").to_string();
            by_date.entry(date_key).or_default().push(f);
        }

        let mut dates: Vec<String> = by_date.keys().cloned().collect();
        dates.sort();

        let mut cumulative_total = 0usize;
        let mut cumulative_open = 0usize;
        let mut points = Vec::new();
        let compliance_rate = self.calculate_compliance_rate();

        for date_str in &dates {
            let day_findings = &by_date[date_str];
            cumulative_total += day_findings.len();

            let new_open = day_findings
                .iter()
                .filter(|f| f.status == FindingStatus::Open)
                .count();
            let new_resolved = day_findings
                .iter()
                .filter(|f| {
                    f.status == FindingStatus::Resolved || f.status == FindingStatus::FalsePositive
                })
                .count();

            // Open count grows with new open findings, shrinks with resolved
            cumulative_open = cumulative_open.saturating_add(new_open);
            cumulative_open = cumulative_open.saturating_sub(new_resolved);

            // Simple risk score: weighted sum of open findings by severity
            let risk_score = day_findings
                .iter()
                .filter(|f| f.status == FindingStatus::Open)
                .map(|f| match f.severity {
                    crate::finding::FindingSeverity::Critical => 10.0,
                    crate::finding::FindingSeverity::High => 7.0,
                    crate::finding::FindingSeverity::Medium => 4.0,
                    crate::finding::FindingSeverity::Low => 1.0,
                    crate::finding::FindingSeverity::Info => 0.5,
                })
                .sum::<f64>();

            // Parse the date back to a DateTime
            let timestamp = day_findings
                .first()
                .map(|f| f.created_at)
                .unwrap_or_else(Utc::now);

            points.push(TrendPoint {
                timestamp,
                total_findings: cumulative_total,
                open_findings: cumulative_open,
                risk_score,
                compliance_rate: compliance_rate * 100.0,
            });
        }

        points
    }

    /// Generate a full `SecurityAnalytics` report from all configured data.
    pub fn generate_report(&self) -> SecurityAnalytics {
        SecurityAnalytics {
            generated_at: Utc::now(),
            mttr: self.calculate_mttr(),
            coverage: self.calculate_coverage(),
            distribution: self.calculate_distribution(),
            compliance_rate: self.calculate_compliance_rate(),
            false_positive_rate: self.calculate_false_positive_rate(),
            trends: self.calculate_trends(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{FindingCategory, FindingProvenance, FindingSeverity};
    use chrono::Duration;

    fn make_finding(severity: FindingSeverity, status: FindingStatus) -> Finding {
        let mut f = Finding::new(
            format!("Finding-{severity:?}"),
            "Test finding",
            severity,
            FindingCategory::Security,
            FindingProvenance::new("sast", 0.9),
        );
        match status {
            FindingStatus::Resolved => f.resolve(),
            FindingStatus::FalsePositive => f.mark_false_positive(),
            FindingStatus::Suppressed => f.suppress("admin", "test"),
            FindingStatus::Open => {}
        }
        f
    }

    #[test]
    fn test_calculate_mttr() {
        let now = Utc::now();
        let resolved = vec![
            ResolvedFinding {
                severity: "critical".into(),
                detected_at: now - Duration::hours(24),
                resolved_at: now - Duration::hours(12),
            },
            ResolvedFinding {
                severity: "critical".into(),
                detected_at: now - Duration::hours(48),
                resolved_at: now - Duration::hours(24),
            },
            ResolvedFinding {
                severity: "high".into(),
                detected_at: now - Duration::hours(72),
                resolved_at: now,
            },
        ];

        let mttr = calculate_mttr(&resolved);
        assert_eq!(mttr.sample_size, 3);
        assert!(mttr.overall_hours > 0.0);
        assert!(mttr.by_severity.contains_key("critical"));
        assert!(mttr.by_severity.contains_key("high"));
    }

    #[test]
    fn test_empty_mttr() {
        let mttr = calculate_mttr(&[]);
        assert_eq!(mttr.sample_size, 0);
        assert_eq!(mttr.overall_hours, 0.0);
    }

    #[test]
    fn test_trend_analysis_improving() {
        let now = Utc::now();
        let points = vec![
            TrendPoint {
                timestamp: now - Duration::days(7),
                total_findings: 20,
                open_findings: 15,
                risk_score: 60.0,
                compliance_rate: 70.0,
            },
            TrendPoint {
                timestamp: now,
                total_findings: 22,
                open_findings: 5,
                risk_score: 30.0,
                compliance_rate: 90.0,
            },
        ];

        let analysis = analyze_trends(&points);
        assert_eq!(analysis.findings_trend, TrendDirection::Improving);
        assert_eq!(analysis.risk_trend, TrendDirection::Improving);
        assert_eq!(analysis.compliance_trend, TrendDirection::Improving);
    }

    #[test]
    fn test_trend_analysis_worsening() {
        let now = Utc::now();
        let points = vec![
            TrendPoint {
                timestamp: now - Duration::days(7),
                total_findings: 5,
                open_findings: 3,
                risk_score: 20.0,
                compliance_rate: 95.0,
            },
            TrendPoint {
                timestamp: now,
                total_findings: 15,
                open_findings: 12,
                risk_score: 55.0,
                compliance_rate: 60.0,
            },
        ];

        let analysis = analyze_trends(&points);
        assert_eq!(analysis.findings_trend, TrendDirection::Worsening);
        assert_eq!(analysis.risk_trend, TrendDirection::Worsening);
        assert_eq!(analysis.compliance_trend, TrendDirection::Worsening);
    }

    #[test]
    fn test_single_point_stable() {
        let now = Utc::now();
        let points = vec![TrendPoint {
            timestamp: now,
            total_findings: 10,
            open_findings: 5,
            risk_score: 40.0,
            compliance_rate: 80.0,
        }];

        let analysis = analyze_trends(&points);
        assert_eq!(analysis.findings_trend, TrendDirection::Stable);
    }

    #[test]
    fn test_trend_direction_display() {
        assert_eq!(TrendDirection::Improving.to_string(), "Improving");
        assert_eq!(TrendDirection::Stable.to_string(), "Stable");
        assert_eq!(TrendDirection::Worsening.to_string(), "Worsening");
    }

    #[test]
    fn test_mttr_by_severity() {
        let now = Utc::now();
        let resolved = vec![
            ResolvedFinding {
                severity: "critical".into(),
                detected_at: now - Duration::hours(2),
                resolved_at: now,
            },
            ResolvedFinding {
                severity: "low".into(),
                detected_at: now - Duration::hours(168), // 1 week
                resolved_at: now,
            },
        ];

        let mttr = calculate_mttr(&resolved);
        let critical_mttr = mttr.by_severity.get("critical").unwrap();
        let low_mttr = mttr.by_severity.get("low").unwrap();
        assert!(
            critical_mttr < low_mttr,
            "Critical should have shorter MTTR: {critical_mttr} vs {low_mttr}"
        );
    }

    #[test]
    fn test_duration_to_hours() {
        assert!((duration_to_hours(Duration::hours(1)) - 1.0).abs() < 0.01);
        assert!((duration_to_hours(Duration::minutes(90)) - 1.5).abs() < 0.01);
        assert!((duration_to_hours(Duration::hours(0)) - 0.0).abs() < 0.01);
        assert!((duration_to_hours(Duration::days(1)) - 24.0).abs() < 0.01);
    }

    #[test]
    fn test_calculator_distribution() {
        let findings = vec![
            make_finding(FindingSeverity::Critical, FindingStatus::Open),
            make_finding(FindingSeverity::Critical, FindingStatus::Open),
            make_finding(FindingSeverity::High, FindingStatus::Open),
            make_finding(FindingSeverity::Low, FindingStatus::Resolved),
        ];

        let calc = SecurityAnalyticsCalculator::new(findings);
        let dist = calc.calculate_distribution();

        assert_eq!(dist.total, 4);
        assert_eq!(dist.by_severity.get("critical"), Some(&2));
        assert_eq!(dist.by_severity.get("high"), Some(&1));
        assert_eq!(dist.by_severity.get("low"), Some(&1));
        assert_eq!(dist.by_scanner.get("sast"), Some(&4));
        assert_eq!(dist.by_category.get("security"), Some(&4));
        assert_eq!(dist.by_status.get("open"), Some(&3));
        assert_eq!(dist.by_status.get("resolved"), Some(&1));
    }

    #[test]
    fn test_calculator_coverage() {
        let calc = SecurityAnalyticsCalculator::new(Vec::new()).with_coverage(
            80,
            100,
            vec!["sast".into(), "sca".into()],
            vec!["rust".into(), "python".into()],
        );

        let cov = calc.calculate_coverage();
        assert!((cov.codebase_pct - 80.0).abs() < 0.01);
        assert_eq!(cov.files_scanned, 80);
        assert_eq!(cov.total_files, 100);
        assert_eq!(cov.scanners_run.len(), 2);
        assert_eq!(cov.languages_covered.len(), 2);
    }

    #[test]
    fn test_calculator_coverage_zero_files() {
        let calc = SecurityAnalyticsCalculator::new(Vec::new()).with_coverage(
            0,
            0,
            Vec::new(),
            Vec::new(),
        );
        let cov = calc.calculate_coverage();
        assert!((cov.codebase_pct - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_calculator_compliance_rate() {
        let policies = vec![
            PolicyResult {
                name: "no-critical".into(),
                passed: true,
            },
            PolicyResult {
                name: "no-secrets".into(),
                passed: true,
            },
            PolicyResult {
                name: "license-check".into(),
                passed: false,
            },
            PolicyResult {
                name: "dep-audit".into(),
                passed: true,
            },
        ];

        let calc = SecurityAnalyticsCalculator::new(Vec::new()).with_policies(policies);
        let rate = calc.calculate_compliance_rate();
        assert!((rate - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_calculator_compliance_rate_no_policies() {
        let calc = SecurityAnalyticsCalculator::new(Vec::new());
        assert!((calc.calculate_compliance_rate() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_calculator_false_positive_rate() {
        let findings = vec![
            make_finding(FindingSeverity::High, FindingStatus::Open),
            make_finding(FindingSeverity::High, FindingStatus::FalsePositive),
            make_finding(FindingSeverity::Low, FindingStatus::Open),
            make_finding(FindingSeverity::Low, FindingStatus::FalsePositive),
        ];

        let calc = SecurityAnalyticsCalculator::new(findings);
        let fp_rate = calc.calculate_false_positive_rate().unwrap();
        assert!((fp_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_calculator_false_positive_rate_empty() {
        let calc = SecurityAnalyticsCalculator::new(Vec::new());
        assert!(calc.calculate_false_positive_rate().is_none());
    }

    #[test]
    fn test_calculator_trends() {
        let findings = vec![
            make_finding(FindingSeverity::Critical, FindingStatus::Open),
            make_finding(FindingSeverity::High, FindingStatus::Open),
            make_finding(FindingSeverity::Low, FindingStatus::Resolved),
        ];

        let calc = SecurityAnalyticsCalculator::new(findings);
        let trends = calc.calculate_trends();

        // All findings created "now" so they group into one day
        assert!(!trends.is_empty());
        let point = &trends[0];
        assert_eq!(point.total_findings, 3);
        // 2 new open - 1 resolved = net 1 open
        assert_eq!(point.open_findings, 1);
        // Risk score: Critical(10) + High(7) = 17 for open findings
        assert!((point.risk_score - 17.0).abs() < 0.01);
    }

    #[test]
    fn test_calculator_trends_empty() {
        let calc = SecurityAnalyticsCalculator::new(Vec::new());
        assert!(calc.calculate_trends().is_empty());
    }

    #[test]
    fn test_calculator_generate_report() {
        let now = Utc::now();
        let findings = vec![
            make_finding(FindingSeverity::Critical, FindingStatus::Open),
            make_finding(FindingSeverity::High, FindingStatus::Resolved),
            make_finding(FindingSeverity::Medium, FindingStatus::FalsePositive),
        ];
        let resolved = vec![ResolvedFinding {
            severity: "high".into(),
            detected_at: now - Duration::hours(48),
            resolved_at: now,
        }];
        let policies = vec![
            PolicyResult {
                name: "p1".into(),
                passed: true,
            },
            PolicyResult {
                name: "p2".into(),
                passed: false,
            },
        ];

        let report = SecurityAnalyticsCalculator::new(findings)
            .with_resolved(resolved)
            .with_coverage(50, 100, vec!["sast".into()], vec!["rust".into()])
            .with_policies(policies)
            .generate_report();

        // MTTR
        assert_eq!(report.mttr.sample_size, 1);
        assert!(report.mttr.overall_hours > 0.0);

        // Coverage
        assert!((report.coverage.codebase_pct - 50.0).abs() < 0.01);
        assert_eq!(report.coverage.scanners_run, vec!["sast"]);

        // Distribution
        assert_eq!(report.distribution.total, 3);

        // Compliance
        assert!((report.compliance_rate - 0.5).abs() < 0.01);

        // False positive rate: 1 FP out of 3
        let fp = report.false_positive_rate.unwrap();
        assert!((fp - 1.0 / 3.0).abs() < 0.01);

        // Trends
        assert!(!report.trends.is_empty());
    }
}
