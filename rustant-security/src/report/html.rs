//! HTML report generator — interactive security scan reports.
//!
//! Generates standalone HTML reports with embedded CSS for viewing in any browser.

use crate::finding::{Finding, FindingSeverity};

/// Generate an HTML report from findings.
pub fn findings_to_html(findings: &[Finding], title: &str) -> String {
    let mut html = String::new();

    // Header
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    html.push_str(&format!("<title>{}</title>\n", escape_html(title)));
    html.push_str("<style>\n");
    html.push_str(CSS_STYLES);
    html.push_str("</style>\n</head>\n<body>\n");

    // Title
    html.push_str(&format!("<h1>{}</h1>\n", escape_html(title)));

    // Summary table
    let counts = severity_counts(findings);
    html.push_str("<div class=\"summary\">\n");
    html.push_str("<h2>Summary</h2>\n");
    html.push_str("<table>\n<thead><tr><th>Severity</th><th>Count</th></tr></thead>\n<tbody>\n");
    for (label, class, count) in &counts {
        if *count > 0 {
            html.push_str(&format!(
                "<tr><td><span class=\"badge {class}\">{label}</span></td><td>{count}</td></tr>\n"
            ));
        }
    }
    html.push_str(&format!(
        "<tr class=\"total\"><td><strong>Total</strong></td><td><strong>{}</strong></td></tr>\n",
        findings.len()
    ));
    html.push_str("</tbody>\n</table>\n</div>\n");

    if findings.is_empty() {
        html.push_str("<p class=\"no-findings\">No findings detected.</p>\n");
    } else {
        // Findings by severity
        html.push_str("<div class=\"findings\">\n");
        html.push_str("<h2>Findings</h2>\n");

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

            let (label, class) = severity_info(severity);
            html.push_str(&format!(
                "<h3><span class=\"badge {}\">{}</span> ({} findings)</h3>\n",
                class,
                label,
                severity_findings.len()
            ));

            for finding in severity_findings {
                html.push_str(&format!("<div class=\"finding {class}\">\n"));
                html.push_str(&format!("<h4>{}</h4>\n", escape_html(&finding.title)));

                // Location
                if let Some(ref loc) = finding.location {
                    html.push_str(&format!(
                        "<p class=\"location\">Location: <code>{}:{}</code></p>\n",
                        loc.file.display(),
                        loc.start_line
                    ));
                }

                // Scanner and confidence
                html.push_str(&format!(
                    "<p class=\"meta\">Scanner: {} | Confidence: {:.0}%</p>\n",
                    escape_html(&finding.provenance.scanner),
                    finding.provenance.confidence * 100.0
                ));

                // Description
                html.push_str(&format!(
                    "<p class=\"description\">{}</p>\n",
                    escape_html(&finding.description)
                ));

                // References
                if !finding.references.is_empty() {
                    html.push_str("<p class=\"references\">References: ");
                    let refs: Vec<String> = finding
                        .references
                        .iter()
                        .map(|r| {
                            if let Some(ref url) = r.url {
                                format!(
                                    "<a href=\"{}\">{}</a>",
                                    escape_html(url),
                                    escape_html(&r.id)
                                )
                            } else {
                                escape_html(&r.id)
                            }
                        })
                        .collect();
                    html.push_str(&refs.join(", "));
                    html.push_str("</p>\n");
                }

                // Remediation
                if let Some(ref rem) = finding.remediation {
                    html.push_str(&format!(
                        "<p class=\"remediation\"><strong>Fix:</strong> {}</p>\n",
                        escape_html(&rem.description)
                    ));
                }

                html.push_str("</div>\n");
            }
        }
        html.push_str("</div>\n");
    }

    // Footer
    html.push_str("<footer><p>Generated by Rustant Security</p></footer>\n");
    html.push_str("</body>\n</html>\n");

    html
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Get severity counts for summary table.
fn severity_counts(findings: &[Finding]) -> Vec<(&'static str, &'static str, usize)> {
    vec![
        (
            "Critical",
            "critical",
            findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Critical)
                .count(),
        ),
        (
            "High",
            "high",
            findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::High)
                .count(),
        ),
        (
            "Medium",
            "medium",
            findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Medium)
                .count(),
        ),
        (
            "Low",
            "low",
            findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Low)
                .count(),
        ),
        (
            "Info",
            "info",
            findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Info)
                .count(),
        ),
    ]
}

/// Get label and CSS class for a severity level.
fn severity_info(severity: FindingSeverity) -> (&'static str, &'static str) {
    match severity {
        FindingSeverity::Critical => ("Critical", "critical"),
        FindingSeverity::High => ("High", "high"),
        FindingSeverity::Medium => ("Medium", "medium"),
        FindingSeverity::Low => ("Low", "low"),
        FindingSeverity::Info => ("Info", "info"),
    }
}

const CSS_STYLES: &str = r#"
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 960px; margin: 0 auto; padding: 20px; color: #333; background: #fafafa; }
h1 { border-bottom: 2px solid #333; padding-bottom: 10px; }
h2 { color: #555; }
table { border-collapse: collapse; width: 100%; margin: 10px 0; }
th, td { border: 1px solid #ddd; padding: 8px 12px; text-align: left; }
th { background: #f5f5f5; }
tr.total { font-weight: bold; background: #f0f0f0; }
.badge { padding: 2px 8px; border-radius: 4px; color: white; font-size: 0.85em; }
.badge.critical { background: #d32f2f; }
.badge.high { background: #f57c00; }
.badge.medium { background: #fbc02d; color: #333; }
.badge.low { background: #388e3c; }
.badge.info { background: #1976d2; }
.finding { border: 1px solid #ddd; border-radius: 6px; padding: 15px; margin: 10px 0; background: white; }
.finding.critical { border-left: 4px solid #d32f2f; }
.finding.high { border-left: 4px solid #f57c00; }
.finding.medium { border-left: 4px solid #fbc02d; }
.finding.low { border-left: 4px solid #388e3c; }
.finding.info { border-left: 4px solid #1976d2; }
.finding h4 { margin-top: 0; }
.location code { background: #f5f5f5; padding: 2px 6px; border-radius: 3px; }
.meta { color: #666; font-size: 0.9em; }
.remediation { background: #e8f5e9; padding: 8px 12px; border-radius: 4px; }
.no-findings { color: #388e3c; font-size: 1.2em; text-align: center; padding: 40px; }
footer { margin-top: 40px; padding-top: 10px; border-top: 1px solid #ddd; color: #999; font-size: 0.85em; text-align: center; }
a { color: #1976d2; }
"#;

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
    fn test_html_report_structure() {
        let findings = vec![
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::Low),
        ];
        let html = findings_to_html(&findings, "Security Report");

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Security Report</title>"));
        assert!(html.contains("Critical"));
        assert!(html.contains("Low"));
        assert!(html.contains("src/main.rs:10"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_html_empty_findings() {
        let html = findings_to_html(&[], "Empty Report");
        assert!(html.contains("No findings detected."));
    }

    #[test]
    fn test_html_escaping() {
        let finding = Finding::new(
            "XSS <script>alert(1)</script>",
            "Description with <b>html</b>",
            FindingSeverity::High,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "test".into(),
                rule_id: None,
                confidence: 0.8,
                consensus: None,
            },
        );
        let html = findings_to_html(&[finding], "Test & Report");

        assert!(html.contains("Test &amp; Report"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>alert"));
    }

    #[test]
    fn test_html_has_css() {
        let html = findings_to_html(&[], "Report");
        assert!(html.contains("<style>"));
        assert!(html.contains(".badge.critical"));
    }

    #[test]
    fn test_severity_counts_fn() {
        let findings = vec![
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::Low),
        ];
        let counts = severity_counts(&findings);
        assert_eq!(counts[0].2, 2); // Critical
        assert_eq!(counts[3].2, 1); // Low
    }

    #[test]
    fn test_html_remediation() {
        let finding =
            sample_finding(FindingSeverity::High).with_remediation(crate::finding::Remediation {
                description: "Use parameterized queries".to_string(),
                patch: None,
                effort: Some(crate::finding::RemediationEffort::Low),
                confidence: 0.9,
            });
        let html = findings_to_html(&[finding], "Report");
        assert!(html.contains("Use parameterized queries"));
        assert!(html.contains("remediation"));
    }
}
