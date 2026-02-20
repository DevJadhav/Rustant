//! Research session state machine with persistence.
//!
//! Manages the lifecycle of a research session, enabling pause/resume
//! and persistence to disk.

use super::decomposition::SubQuery;
use super::output::ResearchReport;
use super::synthesis::SynthesisResult;
use crate::config::ResearchDepth;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current phase of a research session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchPhase {
    /// Breaking down the question.
    Decomposing,
    /// Executing sub-queries.
    Querying,
    /// Merging results.
    Synthesizing,
    /// Running verification/refinement loop.
    Verifying,
    /// Research complete.
    Complete,
    /// Session paused by user.
    Paused,
    /// Session failed.
    Failed,
}

/// A persistent research session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSession {
    /// Unique session ID.
    pub id: Uuid,
    /// The original research question.
    pub question: String,
    /// Research depth level.
    pub depth: ResearchDepth,
    /// Current phase.
    pub phase: ResearchPhase,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Sub-queries derived from decomposition.
    pub sub_queries: Vec<SubQuery>,
    /// Current verification iteration (0-based).
    pub verification_iteration: usize,
    /// Maximum verification iterations.
    pub max_iterations: usize,
    /// Overall progress (0.0-1.0).
    pub progress: f64,
    /// Synthesis result (populated after synthesis phase).
    pub synthesis: Option<SynthesisResult>,
    /// Generated report (populated after complete phase).
    pub report: Option<ResearchReport>,
    /// Error message if failed.
    pub error: Option<String>,
}

impl ResearchSession {
    /// Create a new research session.
    pub fn new(question: impl Into<String>, depth: ResearchDepth) -> Self {
        let max_iterations = match depth {
            ResearchDepth::Quick => 0,
            ResearchDepth::Detailed => 1,
            ResearchDepth::Comprehensive => 3,
        };

        Self {
            id: Uuid::new_v4(),
            question: question.into(),
            depth,
            phase: ResearchPhase::Decomposing,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            sub_queries: Vec::new(),
            verification_iteration: 0,
            max_iterations,
            progress: 0.0,
            synthesis: None,
            report: None,
            error: None,
        }
    }

    /// Transition to a new phase.
    pub fn transition(&mut self, new_phase: ResearchPhase) {
        self.phase = new_phase;
        self.updated_at = Utc::now();
        self.update_progress();
    }

    /// Mark the session as failed.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.phase = ResearchPhase::Failed;
        self.updated_at = Utc::now();
    }

    /// Pause the session.
    pub fn pause(&mut self) {
        self.phase = ResearchPhase::Paused;
        self.updated_at = Utc::now();
    }

    /// Resume from paused state to the appropriate phase.
    pub fn resume(&mut self) {
        if self.phase == ResearchPhase::Paused {
            // Determine the right phase to resume to
            if self.synthesis.is_some() {
                self.phase = ResearchPhase::Verifying;
            } else if !self.sub_queries.is_empty() {
                self.phase = ResearchPhase::Querying;
            } else {
                self.phase = ResearchPhase::Decomposing;
            }
            self.updated_at = Utc::now();
        }
    }

    /// Check if the session is active (not complete, failed, or paused).
    pub fn is_active(&self) -> bool {
        matches!(
            self.phase,
            ResearchPhase::Decomposing
                | ResearchPhase::Querying
                | ResearchPhase::Synthesizing
                | ResearchPhase::Verifying
        )
    }

    /// Update progress based on current phase and sub-query completion.
    fn update_progress(&mut self) {
        self.progress = match self.phase {
            ResearchPhase::Decomposing => 0.1,
            ResearchPhase::Querying => {
                let total = self.sub_queries.len().max(1);
                let completed = self.sub_queries.iter().filter(|q| q.completed).count();
                0.1 + 0.5 * (completed as f64 / total as f64)
            }
            ResearchPhase::Synthesizing => 0.7,
            ResearchPhase::Verifying => {
                let max = self.max_iterations.max(1);
                0.7 + 0.2 * (self.verification_iteration as f64 / max as f64)
            }
            ResearchPhase::Complete => 1.0,
            ResearchPhase::Paused | ResearchPhase::Failed => self.progress, // Keep current
        };
    }

    /// Persist session to disk.
    pub fn save(&self, base_dir: &std::path::Path) -> Result<(), std::io::Error> {
        let dir = base_dir.join("research").join("sessions");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", self.id));
        let data = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::persistence::atomic_write(&path, data.as_bytes())?;
        Ok(())
    }

    /// Load a session from disk.
    pub fn load(base_dir: &std::path::Path, session_id: &Uuid) -> Result<Self, std::io::Error> {
        let path = base_dir
            .join("research")
            .join("sessions")
            .join(format!("{session_id}.json"));
        let data = std::fs::read_to_string(&path)?;
        serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// List all saved sessions.
    pub fn list_sessions(base_dir: &std::path::Path) -> Vec<SessionSummary> {
        let dir = base_dir.join("research").join("sessions");
        if !dir.exists() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "json")
                    .unwrap_or(false)
                {
                    if let Ok(data) = std::fs::read_to_string(entry.path()) {
                        if let Ok(session) = serde_json::from_str::<ResearchSession>(&data) {
                            summaries.push(SessionSummary {
                                id: session.id,
                                question: session.question.clone(),
                                phase: session.phase.clone(),
                                depth: session.depth.clone(),
                                progress: session.progress,
                                created_at: session.created_at,
                                updated_at: session.updated_at,
                            });
                        }
                    }
                }
            }
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        summaries
    }
}

/// Summary of a research session for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: Uuid,
    pub question: String,
    pub phase: ResearchPhase,
    pub depth: ResearchDepth,
    pub progress: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Callback trait for progressive research UI updates.
pub trait ResearchCallback: Send + Sync {
    /// Called when the research phase changes.
    fn on_phase_change(&self, phase: &str, progress: f32);
    /// Called when a sub-query completes.
    fn on_sub_query_complete(&self, query: &str, sources_found: usize);
    /// Called when a contradiction is found.
    fn on_contradiction_found(&self, claim_a: &str, claim_b: &str);
    /// Called during synthesis progress.
    fn on_synthesis_progress(&self, iteration: usize, max: usize);
}

/// No-op callback for testing.
pub struct NoOpResearchCallback;

impl ResearchCallback for NoOpResearchCallback {
    fn on_phase_change(&self, _phase: &str, _progress: f32) {}
    fn on_sub_query_complete(&self, _query: &str, _sources_found: usize) {}
    fn on_contradiction_found(&self, _claim_a: &str, _claim_b: &str) {}
    fn on_synthesis_progress(&self, _iteration: usize, _max: usize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_lifecycle() {
        let mut session = ResearchSession::new("What is prompt caching?", ResearchDepth::Detailed);
        assert_eq!(session.phase, ResearchPhase::Decomposing);
        assert!(session.is_active());

        session.transition(ResearchPhase::Querying);
        assert_eq!(session.phase, ResearchPhase::Querying);

        session.transition(ResearchPhase::Synthesizing);
        assert_eq!(session.phase, ResearchPhase::Synthesizing);

        session.transition(ResearchPhase::Complete);
        assert!(!session.is_active());
        assert!((session.progress - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pause_resume() {
        let mut session = ResearchSession::new("Test?", ResearchDepth::Quick);
        session.transition(ResearchPhase::Querying);
        session.pause();
        assert_eq!(session.phase, ResearchPhase::Paused);

        // resume() determines phase from state: no synthesis and no sub_queries
        // means it resumes to Decomposing (the initial phase)
        session.resume();
        assert_eq!(session.phase, ResearchPhase::Decomposing);
    }

    #[test]
    fn test_fail() {
        let mut session = ResearchSession::new("Test?", ResearchDepth::Quick);
        session.fail("Network error");
        assert_eq!(session.phase, ResearchPhase::Failed);
        assert_eq!(session.error.as_deref(), Some("Network error"));
    }

    #[test]
    fn test_max_iterations_by_depth() {
        let quick = ResearchSession::new("Q?", ResearchDepth::Quick);
        assert_eq!(quick.max_iterations, 0);

        let detailed = ResearchSession::new("Q?", ResearchDepth::Detailed);
        assert_eq!(detailed.max_iterations, 1);

        let comprehensive = ResearchSession::new("Q?", ResearchDepth::Comprehensive);
        assert_eq!(comprehensive.max_iterations, 3);
    }
}
