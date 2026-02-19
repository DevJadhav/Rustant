//! Token-by-token streaming support.

use serde::{Deserialize, Serialize};

/// A streaming token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamToken {
    pub token: String,
    pub index: usize,
    pub logprob: Option<f64>,
    pub finish_reason: Option<String>,
}

/// Streaming configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub buffer_size: usize,
    pub include_logprobs: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024,
            include_logprobs: false,
        }
    }
}
