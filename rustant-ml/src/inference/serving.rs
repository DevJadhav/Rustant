//! Model serving configuration and management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Serving configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServingConfig {
    pub host: String,
    pub port: u16,
    pub max_batch_size: usize,
    pub max_concurrent_requests: usize,
    pub timeout_secs: u64,
}

impl Default for ServingConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_batch_size: 32,
            max_concurrent_requests: 64,
            timeout_secs: 30,
        }
    }
}

/// A running model serving instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServingInstance {
    pub model_name: String,
    pub backend: String,
    pub endpoint: String,
    pub pid: Option<u32>,
    pub status: ServingStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub requests_served: u64,
}

/// Serving status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServingStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}
