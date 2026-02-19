//! Model zoo — registry, downloads, conversion, benchmarking, provenance.

pub mod benchmark;
pub mod card;
pub mod convert;
pub mod download;
pub mod provenance;
pub mod registry;

pub use benchmark::BenchmarkResult;
pub use card::ModelCard;
pub use download::ModelDownloader;
pub use provenance::ModelProvenance;
pub use registry::{ModelEntry, ModelRegistry, ModelSource};
