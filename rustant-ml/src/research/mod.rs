//! Research tools — methodology extraction, literature review, reproducibility.

pub mod bibliography;
pub mod comparison;
pub mod datasets;
pub mod literature_review;
pub mod methodology;
pub mod notebooks;
pub mod reproducibility;
pub mod synthesis;

pub use bibliography::BibEntry;
pub use comparison::PaperComparison;
pub use literature_review::LiteratureReview;
pub use methodology::MethodologyExtraction;
pub use reproducibility::ReproducibilityRecord;
pub use synthesis::ResearchSynthesis;
