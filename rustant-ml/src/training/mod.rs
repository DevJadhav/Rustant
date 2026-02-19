//! Training infrastructure — experiments, runners, metrics, checkpoints, sweeps.

pub mod callbacks;
pub mod checkpoint;
pub mod experiment;
pub mod metrics;
pub mod reproducibility;
pub mod runner;
pub mod sweep;

pub use checkpoint::CheckpointManager;
pub use experiment::{TrainingExperiment, TrainingStatus};
pub use metrics::TrainingMetrics;
pub use runner::TrainingRunner;
pub use sweep::{HyperparamSweep, SweepStrategy};
