//! Algorithm wrappers — classical ML, neural architectures, evaluation.

pub mod classical;
pub mod evaluation;
pub mod explainability;
pub mod neural;

pub use classical::ClassicalAlgorithm;
pub use neural::ArchitectureConfig;
