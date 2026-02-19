//! Code review engine — diff analysis, AI review comments, auto-fix, quality scoring.
//!
//! Phase 2 implementation: structural diff classification, quality scoring,
//! review comment model, dead code detection, duplication analysis,
//! technical debt tracking, and auto-fix suggestion engine.

pub mod autofix;
pub mod comments;
pub mod dead_code;
pub mod diff;
pub mod duplication;
pub mod quality;
pub mod tech_debt;
