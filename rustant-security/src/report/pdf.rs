//! PDF report generator — compliance-ready PDF security reports.
//!
//! Generates structured text-based reports suitable for PDF rendering.
//! Uses a text layout approach that can be consumed by external PDF libraries.

use crate::finding::{Finding, FindingSeverity};
use chrono::Utc;

/// A PDF-ready report structure with sections.
#[derive(Debug)]
pub struct PdfReport {
    /// Report title.
    pub title: String,
    /// Report subtitle / date.
    pub subtitle: String,
    /// Report sections.
    pub sections: Vec<PdfSection>,
    /// Generated timestamp.
    pub generated_at: String,
}

impl PdfReport {
    /// Render this report to formatted text suitable for PDF rendering by external tools.
    ///
    /// Produces a well-structured plain-text document with headers, separators,
    /// indented content, and aligned tables.
    pub fn to_text(&self) -> String {
        render_text(self)
    }
}

/// A section in the PDF report.
#[derive(Debug)]
pub struct PdfSection {
    /// Section heading.
    pub heading: String,
    /// Section content paragraphs.
    pub paragraphs: Vec<String>,
    /// Tables in this section.
    pub tables: Vec<PdfTable>,
}

/// A table in the PDF report.
#[derive(Debug)]
pub struct PdfTable {
    /// Column headers.
    pub headers: Vec<String>,
    /// Table rows.
    pub rows: Vec<Vec<String>>,
}

/// Generate a PDF-ready report structure from findings.
///
/// Produces a structured report with:
/// - Executive Summary section with severity counts and critical highlights
/// - Detailed Findings sections grouped by severity (Critical down to Info)
/// - Each finding includes: title, location, description, remediation, references, confidence
/// - Scanner Summary table with counts per scanner
/// - Recommendations section with actionable guidance derived from top findings
pub fn findings_to_pdf_report(findings: &[Finding], title: &str) -> PdfReport {
    let now = Utc::now();
    let mut sections = Vec::new();

    // Summary section
    let critical = count_severity(findings, FindingSeverity::Critical);
    let high = count_severity(findings, FindingSeverity::High);
    let medium = count_severity(findings, FindingSeverity::Medium);
    let low = count_severity(findings, FindingSeverity::Low);
    let info = count_severity(findings, FindingSeverity::Info);

    let summary_table = PdfTable {
        headers: vec!["Severity".into(), "Count".into()],
        rows: vec![
            vec!["Critical".into(), critical.to_string()],
            vec!["High".into(), high.to_string()],
            vec!["Medium".into(), medium.to_string()],
            vec!["Low".into(), low.to_string()],
            vec!["Info".into(), info.to_string()],
            vec!["Total".into(), findings.len().to_string()],
        ],
    };

    let mut summary_paragraphs = vec![format!(
        "This report contains {} security findings across {} severity levels. \
         {} findings are rated Critical or High and require immediate attention.",
        findings.len(),
        count_active_severities(findings),
        critical + high
    )];

    // Add critical highlights in executive summary
    if critical > 0 {
        let critical_findings: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Critical)
            .collect();
        summary_paragraphs.push(String::new());
        summary_paragraphs.push(format!("Critical issues ({critical}):"));
        for (i, f) in critical_findings.iter().enumerate() {
            let location_str = f
                .location
                .as_ref()
                .map(|loc| format!(" [{}:{}]", loc.file.display(), loc.start_line))
                .unwrap_or_default();
            summary_paragraphs.push(format!("  {}. {}{}", i + 1, f.title, location_str));
        }
    }

    sections.push(PdfSection {
        heading: "Executive Summary".into(),
        paragraphs: summary_paragraphs,
        tables: vec![summary_table],
    });

    if findings.is_empty() {
        sections.push(PdfSection {
            heading: "Findings".into(),
            paragraphs: vec!["No security findings were detected.".into()],
            tables: Vec::new(),
        });
    } else {
        // Detailed findings section
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

            let label = severity_label(severity);
            let mut paragraphs = Vec::new();

            for (i, finding) in severity_findings.iter().enumerate() {
                paragraphs.push(format!("{}. {}", i + 1, finding.title));

                if let Some(ref loc) = finding.location {
                    paragraphs.push(format!(
                        "   Location: {}:{}",
                        loc.file.display(),
                        loc.start_line
                    ));
                }

                paragraphs.push(format!(
                    "   Scanner: {} | Confidence: {:.0}%",
                    finding.provenance.scanner,
                    finding.provenance.confidence * 100.0
                ));

                paragraphs.push(format!("   {}", finding.description));

                if let Some(ref rem) = finding.remediation {
                    paragraphs.push(format!("   Remediation: {}", rem.description));
                }

                if !finding.references.is_empty() {
                    let refs: Vec<&str> =
                        finding.references.iter().map(|r| r.id.as_str()).collect();
                    paragraphs.push(format!("   References: {}", refs.join(", ")));
                }

                paragraphs.push(String::new()); // blank line between findings
            }

            sections.push(PdfSection {
                heading: format!("{} Findings ({})", label, severity_findings.len()),
                paragraphs,
                tables: Vec::new(),
            });
        }

        // Scanner summary table
        let mut scanner_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for f in findings {
            *scanner_counts.entry(&f.provenance.scanner).or_insert(0) += 1;
        }
        let mut scanner_rows: Vec<Vec<String>> = scanner_counts
            .iter()
            .map(|(k, v)| vec![k.to_string(), v.to_string()])
            .collect();
        scanner_rows.sort_by(|a, b| b[1].cmp(&a[1]));

        sections.push(PdfSection {
            heading: "Scanner Summary".into(),
            paragraphs: Vec::new(),
            tables: vec![PdfTable {
                headers: vec!["Scanner".into(), "Findings".into()],
                rows: scanner_rows,
            }],
        });

        // Recommendations section based on top findings
        let recommendations = generate_recommendations(findings, critical, high, medium);
        if !recommendations.is_empty() {
            sections.push(PdfSection {
                heading: "Recommendations".into(),
                paragraphs: recommendations,
                tables: Vec::new(),
            });
        }
    }

    PdfReport {
        title: title.into(),
        subtitle: format!("Generated: {}", now.format("%Y-%m-%d %H:%M UTC")),
        sections,
        generated_at: now.to_rfc3339(),
    }
}

/// Render a PdfReport to a plain-text representation suitable for text-based PDF tools.
pub fn render_text(report: &PdfReport) -> String {
    let mut out = String::new();

    // Title block
    let separator = "=".repeat(70);
    out.push_str(&separator);
    out.push('\n');
    out.push_str(&format!("  {}\n", report.title));
    out.push_str(&format!("  {}\n", report.subtitle));
    out.push_str(&separator);
    out.push_str("\n\n");

    for section in &report.sections {
        // Section heading
        out.push_str(&section.heading);
        out.push('\n');
        out.push_str(&"-".repeat(section.heading.len()));
        out.push_str("\n\n");

        // Paragraphs
        for para in &section.paragraphs {
            out.push_str(para);
            out.push('\n');
        }
        if !section.paragraphs.is_empty() {
            out.push('\n');
        }

        // Tables
        for table in &section.tables {
            // Calculate column widths
            let num_cols = table.headers.len();
            let mut widths = vec![0usize; num_cols];
            for (i, h) in table.headers.iter().enumerate() {
                widths[i] = widths[i].max(h.len());
            }
            for row in &table.rows {
                for (i, cell) in row.iter().enumerate() {
                    if i < num_cols {
                        widths[i] = widths[i].max(cell.len());
                    }
                }
            }

            // Header row
            let header_line: Vec<String> = table
                .headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:width$}", h, width = widths[i]))
                .collect();
            out.push_str(&format!("  {}\n", header_line.join(" | ")));

            let divider: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
            out.push_str(&format!("  {}\n", divider.join("-+-")));

            // Data rows
            for row in &table.rows {
                let cells: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| {
                        if i < num_cols {
                            format!("{:width$}", cell, width = widths[i])
                        } else {
                            cell.clone()
                        }
                    })
                    .collect();
                out.push_str(&format!("  {}\n", cells.join(" | ")));
            }
            out.push('\n');
        }
    }

    out
}

/// Generate recommendations based on the findings.
fn generate_recommendations(
    findings: &[Finding],
    critical: usize,
    high: usize,
    medium: usize,
) -> Vec<String> {
    let mut recommendations = Vec::new();
    let mut idx = 1;

    if critical > 0 {
        recommendations.push(format!(
            "{idx}. IMMEDIATE: Address {critical} critical finding(s) as the highest priority. \
             Critical vulnerabilities represent exploitable risks that could lead \
             to full system compromise."
        ));
        idx += 1;

        // Add specific remediation from critical findings
        for f in findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Critical)
        {
            if let Some(ref rem) = f.remediation {
                recommendations.push(format!("   - {}: {}", f.title, rem.description));
            }
        }
    }

    if high > 0 {
        recommendations.push(format!(
            "{idx}. HIGH PRIORITY: Remediate {high} high-severity finding(s) within the next \
             sprint cycle. These issues pose significant risk and should be tracked \
             in the backlog."
        ));
        idx += 1;
    }

    if medium > 0 {
        recommendations.push(format!(
            "{idx}. PLANNED: Schedule remediation of {medium} medium-severity finding(s). \
             Consider grouping related issues for efficient resolution."
        ));
        idx += 1;
    }

    // Collect unique scanners to suggest additional scanning coverage
    let scanners: std::collections::HashSet<&str> = findings
        .iter()
        .map(|f| f.provenance.scanner.as_str())
        .collect();

    let known_scanners = ["sast", "sca", "secrets", "container", "iac"];
    let missing: Vec<&&str> = known_scanners
        .iter()
        .filter(|s| !scanners.contains(**s))
        .collect();

    if !missing.is_empty() {
        let missing_names: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
        recommendations.push(format!(
            "{}. COVERAGE: Consider enabling additional scanners for broader coverage: {}.",
            idx,
            missing_names.join(", ")
        ));
        idx += 1;
    }

    // Check for findings without remediation
    let no_remediation = findings.iter().filter(|f| f.remediation.is_none()).count();
    if no_remediation > 0 {
        recommendations.push(format!(
            "{idx}. PROCESS: {no_remediation} finding(s) lack remediation guidance. Consider enriching \
             scanner rules or adding manual triage notes."
        ));
    }

    recommendations
}

fn count_severity(findings: &[Finding], severity: FindingSeverity) -> usize {
    findings.iter().filter(|f| f.severity == severity).count()
}

fn count_active_severities(findings: &[Finding]) -> usize {
    let severities = [
        FindingSeverity::Critical,
        FindingSeverity::High,
        FindingSeverity::Medium,
        FindingSeverity::Low,
        FindingSeverity::Info,
    ];
    severities
        .iter()
        .filter(|s| findings.iter().any(|f| f.severity == **s))
        .count()
}

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
    use crate::finding::{
        CodeLocation, FindingCategory, FindingProvenance, FindingReference, ReferenceType,
        Remediation,
    };

    fn sample_finding(severity: FindingSeverity) -> Finding {
        Finding::new(
            "Test Finding",
            "A test finding description",
            severity,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "sast".into(),
                rule_id: Some("TEST-001".into()),
                confidence: 0.85,
                consensus: None,
            },
        )
        .with_location(CodeLocation::new("src/main.rs", 10))
    }

    #[test]
    fn test_pdf_report_structure() {
        let findings = vec![
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::High),
        ];
        let report = findings_to_pdf_report(&findings, "Security Report");

        assert_eq!(report.title, "Security Report");
        assert!(!report.sections.is_empty());
        assert!(!report.generated_at.is_empty());
    }

    #[test]
    fn test_pdf_report_empty() {
        let report = findings_to_pdf_report(&[], "Empty Report");

        assert_eq!(report.title, "Empty Report");
        // Should have summary + empty findings
        assert!(report.sections.len() >= 2);
        assert!(
            report.sections[1]
                .paragraphs
                .iter()
                .any(|p| p.contains("No security findings"))
        );
    }

    #[test]
    fn test_render_text() {
        let findings = vec![
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::Low),
        ];
        let report = findings_to_pdf_report(&findings, "Test Report");
        let text = render_text(&report);

        assert!(text.contains("Test Report"));
        assert!(text.contains("Executive Summary"));
        assert!(text.contains("Critical"));
        assert!(text.contains("Low"));
        assert!(text.contains("src/main.rs:10"));
    }

    #[test]
    fn test_render_text_table() {
        let findings = vec![sample_finding(FindingSeverity::Medium)];
        let report = findings_to_pdf_report(&findings, "Report");
        let text = render_text(&report);

        // Should contain table separators
        assert!(text.contains("Severity"));
        assert!(text.contains("Count"));
        assert!(text.contains("-+-"));
    }

    #[test]
    fn test_scanner_summary() {
        let findings = vec![
            sample_finding(FindingSeverity::High),
            sample_finding(FindingSeverity::Low),
        ];
        let report = findings_to_pdf_report(&findings, "Report");

        // Should have a Scanner Summary section
        let has_scanner_section = report
            .sections
            .iter()
            .any(|s| s.heading == "Scanner Summary");
        assert!(has_scanner_section);
    }

    #[test]
    fn test_executive_summary_content() {
        let findings = vec![
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::Critical),
            sample_finding(FindingSeverity::High),
        ];
        let report = findings_to_pdf_report(&findings, "Report");
        let summary = &report.sections[0];

        assert_eq!(summary.heading, "Executive Summary");
        assert!(summary.paragraphs[0].contains("3 security findings"));
        assert!(summary.paragraphs[0].contains("3 findings are rated Critical or High"));
    }

    #[test]
    fn test_to_text_method() {
        let findings = vec![
            sample_finding(FindingSeverity::High),
            sample_finding(FindingSeverity::Low),
        ];
        let report = findings_to_pdf_report(&findings, "Method Test");
        let text = report.to_text();

        // to_text() should produce the same output as render_text()
        let expected = render_text(&report);
        assert_eq!(text, expected);
        assert!(text.contains("Method Test"));
        assert!(text.contains("Executive Summary"));
    }

    #[test]
    fn test_recommendations_section() {
        let findings = vec![
            sample_finding(FindingSeverity::Critical).with_remediation(Remediation {
                description: "Fix the critical bug".into(),
                patch: None,
                effort: None,
                confidence: 0.9,
            }),
            sample_finding(FindingSeverity::High),
            sample_finding(FindingSeverity::Medium),
        ];
        let report = findings_to_pdf_report(&findings, "Report");

        let rec_section = report
            .sections
            .iter()
            .find(|s| s.heading == "Recommendations");
        assert!(
            rec_section.is_some(),
            "Report should have a Recommendations section"
        );

        let rec = rec_section.unwrap();
        // Should have IMMEDIATE recommendation for critical
        assert!(rec.paragraphs.iter().any(|p| p.contains("IMMEDIATE")));
        // Should have HIGH PRIORITY recommendation
        assert!(rec.paragraphs.iter().any(|p| p.contains("HIGH PRIORITY")));
        // Should have PLANNED recommendation for medium
        assert!(rec.paragraphs.iter().any(|p| p.contains("PLANNED")));
        // Should reference the specific critical remediation
        assert!(
            rec.paragraphs
                .iter()
                .any(|p| p.contains("Fix the critical bug"))
        );
    }

    #[test]
    fn test_recommendations_coverage_suggestion() {
        // Only use "sast" scanner, so others should be suggested
        let findings = vec![sample_finding(FindingSeverity::Low)];
        let report = findings_to_pdf_report(&findings, "Report");

        let rec_section = report
            .sections
            .iter()
            .find(|s| s.heading == "Recommendations");
        assert!(rec_section.is_some());

        let rec = rec_section.unwrap();
        // Should suggest missing scanners
        assert!(rec.paragraphs.iter().any(|p| p.contains("COVERAGE")));
        assert!(rec.paragraphs.iter().any(|p| p.contains("sca")));
    }

    #[test]
    fn test_critical_highlights_in_summary() {
        let findings = vec![
            Finding::new(
                "SQL Injection",
                "SQL injection in login",
                FindingSeverity::Critical,
                FindingCategory::Security,
                FindingProvenance::new("sast", 0.95),
            )
            .with_location(CodeLocation::new("src/auth.rs", 42)),
            Finding::new(
                "RCE via deserialization",
                "Remote code execution",
                FindingSeverity::Critical,
                FindingCategory::Security,
                FindingProvenance::new("sast", 0.9),
            ),
        ];

        let report = findings_to_pdf_report(&findings, "Report");
        let summary = &report.sections[0];

        // Executive summary should list critical issues
        assert!(
            summary
                .paragraphs
                .iter()
                .any(|p| p.contains("Critical issues"))
        );
        assert!(
            summary
                .paragraphs
                .iter()
                .any(|p| p.contains("SQL Injection"))
        );
        assert!(
            summary
                .paragraphs
                .iter()
                .any(|p| p.contains("RCE via deserialization"))
        );
    }

    #[test]
    fn test_finding_references_in_text() {
        let finding = sample_finding(FindingSeverity::High).with_reference(FindingReference {
            ref_type: ReferenceType::Cwe,
            id: "CWE-89".into(),
            url: Some("https://cwe.mitre.org/data/definitions/89.html".into()),
        });

        let report = findings_to_pdf_report(&[finding], "Report");
        let text = report.to_text();
        assert!(text.contains("CWE-89"));
    }

    #[test]
    fn test_no_remediation_process_recommendation() {
        // Findings without remediation should trigger a PROCESS recommendation
        let findings = vec![
            sample_finding(FindingSeverity::High), // no remediation set
        ];
        let report = findings_to_pdf_report(&findings, "Report");

        let rec_section = report
            .sections
            .iter()
            .find(|s| s.heading == "Recommendations");
        assert!(rec_section.is_some());

        let rec = rec_section.unwrap();
        assert!(rec.paragraphs.iter().any(|p| p.contains("PROCESS")));
        assert!(
            rec.paragraphs
                .iter()
                .any(|p| p.contains("lack remediation"))
        );
    }
}
