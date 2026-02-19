//! Resource limits enforcement for ML operations.

use serde::{Deserialize, Serialize};

/// Resource guard configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceGuard {
    pub max_gpu_memory_gb: f64,
    pub max_cpu_cores: usize,
    pub max_model_size_gb: f64,
    pub max_training_hours: f64,
    pub max_disk_gb: f64,
}

impl Default for ResourceGuard {
    fn default() -> Self {
        Self {
            max_gpu_memory_gb: 0.0, // 0 = unlimited
            max_cpu_cores: 0,
            max_model_size_gb: 50.0,
            max_training_hours: 24.0,
            max_disk_gb: 100.0,
        }
    }
}

impl ResourceGuard {
    /// Check if a model size is within limits.
    pub fn check_model_size(&self, size_gb: f64) -> Result<(), String> {
        if self.max_model_size_gb > 0.0 && size_gb > self.max_model_size_gb {
            Err(format!(
                "Model size {size_gb:.1}GB exceeds limit of {:.1}GB",
                self.max_model_size_gb
            ))
        } else {
            Ok(())
        }
    }

    /// Check if disk usage is within limits.
    pub fn check_disk_usage(&self, usage_gb: f64) -> Result<(), String> {
        if self.max_disk_gb > 0.0 && usage_gb > self.max_disk_gb {
            Err(format!(
                "Disk usage {usage_gb:.1}GB exceeds limit of {:.1}GB",
                self.max_disk_gb
            ))
        } else {
            Ok(())
        }
    }

    /// Check if requested GPU memory is within limits.
    pub fn check_gpu_memory(&self, requested_gb: f64) -> bool {
        self.max_gpu_memory_gb <= 0.0 || requested_gb <= self.max_gpu_memory_gb
    }

    /// Check if requested CPU cores are within limits.
    pub fn check_cpu_cores(&self, requested: usize) -> bool {
        self.max_cpu_cores == 0 || requested <= self.max_cpu_cores
    }

    /// Check if requested training hours are within limits.
    pub fn check_training_hours(&self, requested: f64) -> bool {
        self.max_training_hours <= 0.0 || requested <= self.max_training_hours
    }
}
