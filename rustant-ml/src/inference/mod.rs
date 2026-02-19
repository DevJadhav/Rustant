//! Inference & serving — unified model serving abstraction.

pub mod backends;
pub mod formats;
pub mod model_registry;
pub mod profiler;
pub mod serving;
pub mod streaming;

pub use backends::InferenceBackend;
pub use formats::ModelFormat;
pub use model_registry::LocalModelCatalog;
pub use profiler::InferenceProfile;
pub use serving::{ServingConfig, ServingInstance, ServingStatus};

use std::collections::HashMap;

/// Resource limits for inference workloads.
#[derive(Debug, Clone, Default)]
pub struct ResourceMonitor {
    pub max_memory_mb: Option<f64>,
    pub max_gpu_memory_mb: Option<f64>,
    pub max_concurrent: Option<usize>,
}

/// Manages active inference backends and serving instances.
pub struct InferenceManager {
    /// Registered inference backends.
    pub backends: Vec<Box<dyn InferenceBackend>>,
    /// Currently active serving instances (keyed by model name).
    pub active_instances: HashMap<String, ServingInstance>,
    /// Whether inference profiling is enabled.
    pub profiler_enabled: bool,
    /// Resource monitoring and limits.
    pub resource_monitor: ResourceMonitor,
}

impl InferenceManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
            active_instances: HashMap::new(),
            profiler_enabled: false,
            resource_monitor: ResourceMonitor::default(),
        }
    }

    /// Register an inference backend.
    pub fn add_backend(&mut self, backend: Box<dyn InferenceBackend>) {
        self.backends.push(backend);
    }

    /// List all registered backend names.
    pub fn backend_names(&self) -> Vec<&str> {
        self.backends.iter().map(|b| b.name()).collect()
    }

    /// Check which backends are available.
    pub async fn available_backends(&self) -> Vec<&str> {
        let mut available = Vec::new();
        for backend in &self.backends {
            if backend.is_available().await {
                available.push(backend.name());
            }
        }
        available
    }

    /// Get the number of active serving instances.
    pub fn active_count(&self) -> usize {
        self.active_instances.len()
    }
}

impl Default for InferenceManager {
    fn default() -> Self {
        Self::new()
    }
}
