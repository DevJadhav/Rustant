//! Research engine — orchestrates the 5-phase research pipeline.

use super::contradiction::ContradictionDetector;
use super::decomposition::QuestionDecomposer;
use super::output::{OutputFormat, ReportGenerator, ResearchReport};
use super::session::{ResearchCallback, ResearchPhase, ResearchSession};
use super::sources::SourceTracker;
use super::synthesis::ResearchSynthesizer;
use crate::config::{ResearchConfig, ResearchDepth};
use std::path::PathBuf;
use std::sync::Arc;

/// The main research engine that orchestrates the 5-phase pipeline.
pub struct ResearchEngine {
    config: ResearchConfig,
    decomposer: QuestionDecomposer,
    synthesizer: ResearchSynthesizer,
    contradiction_detector: ContradictionDetector,
    base_dir: PathBuf,
}

impl ResearchEngine {
    /// Create a new research engine.
    pub fn new(config: ResearchConfig, base_dir: PathBuf) -> Self {
        Self {
            config,
            decomposer: QuestionDecomposer::new(),
            synthesizer: ResearchSynthesizer::new(),
            contradiction_detector: ContradictionDetector::new(),
            base_dir,
        }
    }

    /// Start a new research session.
    ///
    /// Returns the session which can be driven phase-by-phase.
    pub fn start_session(
        &self,
        question: impl Into<String>,
        depth: Option<ResearchDepth>,
    ) -> ResearchSession {
        let depth = depth.unwrap_or_else(|| self.config.default_depth.clone());
        ResearchSession::new(question, depth)
    }

    /// Execute Phase 1: Decompose the question into sub-queries.
    pub fn decompose(&self, session: &mut ResearchSession) {
        session.transition(ResearchPhase::Decomposing);
        session.sub_queries = self.decomposer.decompose(&session.question);
        session.transition(ResearchPhase::Querying);
    }

    /// Execute Phase 3: Synthesize results from completed sub-queries.
    pub fn synthesize(&self, session: &mut ResearchSession, tracker: &SourceTracker) {
        session.transition(ResearchPhase::Synthesizing);

        let contradictions = self.contradiction_detector.detect(tracker);
        let synthesis = self.synthesizer.synthesize(
            &session.question,
            &session.sub_queries,
            tracker,
            &contradictions,
        );

        session.synthesis = Some(synthesis);
        session.transition(ResearchPhase::Verifying);
    }

    /// Execute Phase 4: Verify and refine.
    ///
    /// Returns true if another iteration is needed, false if done.
    pub fn verify(&self, session: &mut ResearchSession, tracker: &SourceTracker) -> bool {
        if session.verification_iteration >= session.max_iterations {
            return false; // Done verifying
        }

        session.verification_iteration += 1;

        // Check for gaps that need filling
        if let Some(ref synthesis) = session.synthesis {
            if synthesis.gaps.is_empty() || synthesis.confidence > 0.85 {
                return false; // High confidence, no gaps
            }
        }

        // Re-synthesize with updated data
        let contradictions = self.contradiction_detector.detect(tracker);
        let synthesis = self.synthesizer.synthesize(
            &session.question,
            &session.sub_queries,
            tracker,
            &contradictions,
        );
        session.synthesis = Some(synthesis);

        session.verification_iteration < session.max_iterations
    }

    /// Execute Phase 5: Generate the final report.
    pub fn report(
        &self,
        session: &mut ResearchSession,
        tracker: &SourceTracker,
        format: Option<OutputFormat>,
    ) -> ResearchReport {
        let format = format.unwrap_or(OutputFormat::DetailedReport);

        let synthesis = session.synthesis.clone().unwrap_or_else(|| {
            self.synthesizer
                .synthesize(&session.question, &session.sub_queries, tracker, &[])
        });

        let report = ReportGenerator::generate(&session.question, &synthesis, tracker, &format);
        session.report = Some(report.clone());
        session.transition(ResearchPhase::Complete);

        // Persist the completed session
        let _ = session.save(&self.base_dir);

        report
    }

    /// Run the full pipeline end-to-end (synchronous phases only).
    ///
    /// Phase 2 (query execution) must be handled externally since it requires
    /// async tool execution. This method handles decompose → synthesize → verify → report.
    pub fn run_pipeline(
        &self,
        session: &mut ResearchSession,
        tracker: &SourceTracker,
        format: Option<OutputFormat>,
        callback: Option<Arc<dyn ResearchCallback>>,
    ) -> ResearchReport {
        // Phase 1: Decompose
        if let Some(ref cb) = callback {
            cb.on_phase_change("decomposing", 0.1);
        }
        self.decompose(session);

        // Phase 2: Querying is skipped here (handled by caller)

        // Phase 3: Synthesize
        if let Some(ref cb) = callback {
            cb.on_phase_change("synthesizing", 0.7);
        }
        self.synthesize(session, tracker);

        // Phase 4: Verify
        let mut iteration = 0;
        while self.verify(session, tracker) {
            iteration += 1;
            if let Some(ref cb) = callback {
                cb.on_synthesis_progress(iteration, session.max_iterations);
            }
        }

        // Phase 5: Report
        if let Some(ref cb) = callback {
            cb.on_phase_change("reporting", 0.95);
        }
        self.report(session, tracker, format)
    }

    /// Get the research config.
    pub fn config(&self) -> &ResearchConfig {
        &self.config
    }

    /// List saved research sessions.
    pub fn list_sessions(&self) -> Vec<super::session::SessionSummary> {
        ResearchSession::list_sessions(&self.base_dir)
    }

    /// Load a saved session.
    pub fn load_session(&self, session_id: &uuid::Uuid) -> Result<ResearchSession, std::io::Error> {
        ResearchSession::load(&self.base_dir, session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> ResearchEngine {
        ResearchEngine::new(
            ResearchConfig::default(),
            PathBuf::from("/tmp/rustant-test"),
        )
    }

    #[test]
    fn test_start_session() {
        let engine = make_engine();
        let session = engine.start_session("What is prompt caching?", None);
        assert_eq!(session.phase, ResearchPhase::Decomposing);
        assert_eq!(session.depth, ResearchDepth::Detailed);
    }

    #[test]
    fn test_decompose() {
        let engine = make_engine();
        let mut session = engine.start_session("Redis vs Memcached", None);
        engine.decompose(&mut session);
        assert!(!session.sub_queries.is_empty());
        assert_eq!(session.phase, ResearchPhase::Querying);
    }

    #[test]
    fn test_full_pipeline() {
        let engine = make_engine();
        let mut session = engine.start_session("What is X?", Some(ResearchDepth::Quick));
        let tracker = SourceTracker::new();

        let report = engine.run_pipeline(&mut session, &tracker, None, None);
        assert!(!report.content.is_empty());
        assert_eq!(session.phase, ResearchPhase::Complete);
    }
}
