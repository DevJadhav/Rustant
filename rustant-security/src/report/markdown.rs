//! Markdown report generator — human-readable security scan reports.

use crate::finding::{Finding, FindingSeverity};

/// Generate a Markdown report from findings.
pub fn findings_to_markdown(findings: &[Finding], title: &str) -> String {
    let mut md = String::new();

    md.push_str(&format!("# {title}\n\n"));

    // Summary
    let critical = findings
        .iter()
        .filter(|f| f.severity == FindingSeverity::Critical)
        .count();
    let high = findings
        .iter()
        .filter(|f| f.severity == FindingSeverity::High)
        .count();
    let medium = findings
        .iter()
        .filter(|f| f.severity == FindingSeverity::Medium)
        .count();
    let low = findings
        .iter()
        .filter(|f| f.severity == FindingSeverity::Low)
        .count();
    let info = findings
        .iter()
        .filter(|f| f.severity == FindingSeverity::Info)
        .count();

    md.push_str("## Summary\n\n");
    md.push_str("| Severity | Count |\n");
    md.push_str("|----------|-------|\n");
    if critical > 0 {
        md.push_str(&format!("| Critical | {critical} |\n"));
    }
    if high > 0 {
        md.push_str(&format!("| High | {high} |\n"));
    }
    if medium > 0 {
        md.push_str(&format!("| Medium | {medium} |\n"));
    }
    if low > 0 {
        md.push_str(&format!("| Low | {low} |\n"));
    }
    if info > 0 {
        md.push_str(&format!("| Info | {info} |\n"));
    }
    md.push_str(&format!("| **Total** | **{}** |\n\n", findings.len()));

    if findings.is_empty() {
        md.push_str("No findings detected.\n");
        return md;
    }

    // Group findings by severity
    md.push_str("## Findings\n\n");

    let severity_order = [
        FindingSeverity::Critical,
        FindingSeverity::High,
        FindingSeverity::Medium,
        FindingSeverity::Low,
        FindingSeverity::Info,
    ];

    for severity in severity_order {
        let severity_findings: Vec<&Finding> =
            findings.iter().filter(|f| f.severity == severity).collect();

        if severity_findings.is_empty() {
            continue;
        }

        md.push_str(&format!(
            "### {} ({})\n\n",
            severity_label(severity),
            severity_findings.len()
        ));

        for finding in severity_findings {
            md.push_str(&format!("#### {}\n\n", finding.title));

            // Location
            if let Some(ref loc) = finding.location {
                md.push_str(&format!(
                    "**Location:** `{}:{}`\n\n",
                    loc.file.display(),
                    loc.start_line,
                ));
            }

            // Scanner info
            md.push_str(&format!(
                "**Scanner:** {} | **Confidence:** {:.0}%\n\n",
                finding.provenance.scanner,
                finding.provenance.confidence * 100.0,
            ));

            // Description
            md.push_str(&format!("{}\n\n", finding.description));

            // References
            if !finding.references.is_empty() {
                md.push_str("**References:** ");
                let refs: Vec<String> = finding
                    .references
                    .iter()
                    .map(|r| {
                        if let Some(ref url) = r.url {
                            format!("[{}]({})", r.id, url)
                        } else {
                            r.id.clone()
                        }
                    })
                    .collect();
                md.push_str(&refs.join(", "));
                md.push_str("\n\n");
            }

            // Remediation
            if let Some(ref rem) = finding.remediation {
                md.push_str(&format!("**Fix:** {}\n\n", rem.description));
            }

            md.push_str("---\n\n");
        }
    }

    md
}

/// Get the display label with emoji for a severity level.
fn severity_label(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Critical => "Critical",
        FindingSeverity::High => "High",
        FindingSeverity::Medium => "Medium",
        FindingSeverity::Low => "Low",
        FindingSeverity::Info => "Info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{CodeLocation, FindingCategory, FindingProvenance};

    fn sample_finding(severity: FindingSeverity) -> Finding {
        Finding::new(
            "Test Finding",
            "A test finding description",
            severity,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "test".into(),
                rule_id: Some("TEST-001".into()),
                confidence: 0.9,
                consensus: None,
            },
        )
        .with_location(CodeLocation::new("src/main.rs", 10))
    }

    #[test]
    fn test_markdown_report() {
        let findings = vec![
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::Medium),
        ];
        let report = findings_to_markdown(&findings, "Security Scan Report");

        assert!(report.contains("# Security Scan Report"));
        assert!(report.contains("| Critical | 1 |"));
        assert!(report.contains("| Medium | 1 |"));
        assert!(report.contains("| **Total** | **2** |"));
        assert!(report.contains("### Critical"));
        assert!(report.contains("src/main.rs:10"));
    }

    #[test]
    fn test_empty_findings_report() {
        let report = findings_to_markdown(&[], "Empty Report");
        assert!(report.contains("No findings detected."));
    }

    #[test]
    fn test_severity_ordering() {
        let findings = vec![
            sample_finding(FindingSeverity::Low),
            sample_finding(FindingSeverity::Critical),
        ];
        let report = findings_to_markdown(&findings, "Report");
        let critical_pos = report.find("### Critical").unwrap();
        let low_pos = report.find("### Low").unwrap();
        assert!(critical_pos < low_pos, "Critical should appear before Low");
    }
}
