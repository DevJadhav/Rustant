//! Research report generation in multiple output formats.

use super::sources::SourceTracker;
use super::synthesis::SynthesisResult;
use serde::{Deserialize, Serialize};

/// Output format for research reports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Concise summary (1-2 paragraphs).
    Summary,
    /// Full report with sections and citations.
    DetailedReport,
    /// Bibliography with annotations.
    AnnotatedBibliography,
    /// Step-by-step implementation plan.
    ImplementationRoadmap,
}

/// A generated research report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchReport {
    /// The original research question.
    pub question: String,
    /// The output format used.
    pub format: OutputFormat,
    /// The formatted report content.
    pub content: String,
    /// Overall confidence (0.0-1.0).
    pub confidence: f64,
    /// Number of sources cited.
    pub sources_cited: usize,
    /// Number of contradictions found.
    pub contradictions_found: usize,
}

/// Generates research reports from synthesis results.
pub struct ReportGenerator;

impl ReportGenerator {
    /// Generate a report in the specified format.
    pub fn generate(
        question: &str,
        synthesis: &SynthesisResult,
        tracker: &SourceTracker,
        format: &OutputFormat,
    ) -> ResearchReport {
        let content = match format {
            OutputFormat::Summary => Self::generate_summary(question, synthesis),
            OutputFormat::DetailedReport => Self::generate_detailed(question, synthesis, tracker),
            OutputFormat::AnnotatedBibliography => Self::generate_bibliography(tracker),
            OutputFormat::ImplementationRoadmap => Self::generate_roadmap(question, synthesis),
        };

        ResearchReport {
            question: question.to_string(),
            format: format.clone(),
            content,
            confidence: synthesis.confidence,
            sources_cited: synthesis.sources_used,
            contradictions_found: synthesis.contradictions.len(),
        }
    }

    fn generate_summary(question: &str, synthesis: &SynthesisResult) -> String {
        let mut out = format!("# Research Summary: {question}\n\n");
        out.push_str(&synthesis.answer);
        out.push_str(&format!(
            "\n\n**Confidence:** {:.0}% | **Sources:** {} | **Contradictions:** {}\n",
            synthesis.confidence * 100.0,
            synthesis.sources_used,
            synthesis.contradictions.len(),
        ));
        out
    }

    fn generate_detailed(
        question: &str,
        synthesis: &SynthesisResult,
        tracker: &SourceTracker,
    ) -> String {
        let mut out = format!("# Research Report: {question}\n\n");

        // Findings section
        out.push_str("## Key Findings\n\n");
        for (i, finding) in synthesis.findings.iter().enumerate() {
            out.push_str(&format!(
                "{}. {} (evidence: {:.0}%)\n",
                i + 1,
                finding.text,
                finding.evidence_strength * 100.0
            ));
        }

        // Contradictions section
        if !synthesis.contradictions.is_empty() {
            out.push_str("\n## Contradictions\n\n");
            for c in &synthesis.contradictions {
                out.push_str(&format!("- **Claim A:** {}\n", c.claim_a_text));
                out.push_str(&format!("  **Claim B:** {}\n", c.claim_b_text));
                out.push_str(&format!("  **Type:** {:?}\n\n", c.contradiction_type));
            }
        }

        // Gaps section
        if !synthesis.gaps.is_empty() {
            out.push_str("## Research Gaps\n\n");
            for gap in &synthesis.gaps {
                out.push_str(&format!("- {}\n", gap.description));
            }
        }

        // Sources section
        out.push_str("\n## Sources\n\n");
        for source in tracker.sources() {
            let url = source.url.as_deref().unwrap_or("N/A");
            out.push_str(&format!(
                "- **{}** ({:?}, reliability: {:.0}%)\n  {}\n",
                source.title,
                source.source_type,
                source.reliability * 100.0,
                url,
            ));
        }

        // Confidence summary
        out.push_str(&format!(
            "\n---\n**Overall Confidence:** {:.0}%\n",
            synthesis.confidence * 100.0
        ));

        out
    }

    fn generate_bibliography(tracker: &SourceTracker) -> String {
        let mut out = "# Annotated Bibliography\n\n".to_string();

        for source in tracker.sources() {
            out.push_str(&format!("## {}\n", source.title));
            if let Some(ref url) = source.url {
                out.push_str(&format!("**URL:** {url}\n"));
            }
            out.push_str(&format!(
                "**Type:** {:?} | **Reliability:** {:.0}%\n",
                source.source_type,
                source.reliability * 100.0,
            ));
            if let Some(ref summary) = source.summary {
                out.push_str(&format!("**Summary:** {summary}\n"));
            }

            let claims = tracker.claims_from_source(&source.id);
            if !claims.is_empty() {
                out.push_str("**Key claims:**\n");
                for claim in claims {
                    let verified = if claim.verified { " [verified]" } else { "" };
                    out.push_str(&format!("  - {}{}\n", claim.text, verified));
                }
            }
            out.push('\n');
        }

        out
    }

    fn generate_roadmap(question: &str, synthesis: &SynthesisResult) -> String {
        let mut out = format!("# Implementation Roadmap: {question}\n\n");

        out.push_str("## Overview\n\n");
        out.push_str(&synthesis.answer);

        out.push_str("\n\n## Implementation Steps\n\n");
        for (i, finding) in synthesis.findings.iter().enumerate() {
            out.push_str(&format!("### Step {}: {}\n\n", i + 1, finding.text));
        }

        if !synthesis.gaps.is_empty() {
            out.push_str("## Open Questions\n\n");
            for gap in &synthesis.gaps {
                out.push_str(&format!("- {}\n", gap.description));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::synthesis::SynthesisResult;

    fn make_synthesis() -> SynthesisResult {
        SynthesisResult {
            answer: "Test answer".into(),
            confidence: 0.85,
            findings: vec![super::super::synthesis::Finding {
                text: "Key finding".into(),
                evidence_strength: 0.9,
                supporting_sources: 2,
                tags: vec![],
            }],
            contradictions: vec![],
            gaps: vec![],
            sources_used: 3,
        }
    }

    #[test]
    fn test_generate_summary() {
        let synthesis = make_synthesis();
        let tracker = SourceTracker::new();
        let report =
            ReportGenerator::generate("Test?", &synthesis, &tracker, &OutputFormat::Summary);
        assert!(report.content.contains("Test answer"));
        assert!(report.content.contains("85%"));
    }

    #[test]
    fn test_generate_detailed() {
        let synthesis = make_synthesis();
        let tracker = SourceTracker::new();
        let report =
            ReportGenerator::generate("Test?", &synthesis, &tracker, &OutputFormat::DetailedReport);
        assert!(report.content.contains("Key Findings"));
        assert!(report.content.contains("Key finding"));
    }

    #[test]
    fn test_generate_roadmap() {
        let synthesis = make_synthesis();
        let tracker = SourceTracker::new();
        let report = ReportGenerator::generate(
            "Test?",
            &synthesis,
            &tracker,
            &OutputFormat::ImplementationRoadmap,
        );
        assert!(report.content.contains("Implementation Steps"));
    }
}
