//! Interpretability pillar — attention, feature importance, explanations.

pub mod attention;
pub mod concept_probe;
pub mod explanations;
pub mod features;
pub mod token_importance;

pub use attention::AttentionAnalyzer;
pub use explanations::ExplanationMethod;
pub use features::FeatureImportanceAnalyzer;
