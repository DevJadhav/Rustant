//! Model quantization.

use crate::error::MlError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Quantization method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuantizationMethod {
    Gptq { bits: u8, group_size: u32 },
    Awq { bits: u8 },
    Gguf { quant_type: String },
    BitsAndBytes { bits: u8, double_quant: bool },
}

/// Quantization result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationResult {
    pub method: String,
    pub original_size_mb: f64,
    pub quantized_size_mb: f64,
    pub compression_ratio: f64,
    pub perplexity_original: Option<f64>,
    pub perplexity_quantized: Option<f64>,
    pub output_path: PathBuf,
}

/// Quantization runner.
pub struct QuantizationRunner {
    #[allow(dead_code)]
    workspace: PathBuf,
}

impl QuantizationRunner {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    pub async fn quantize(
        &self,
        model_path: &Path,
        method: &QuantizationMethod,
    ) -> Result<QuantizationResult, MlError> {
        let method_str = match method {
            QuantizationMethod::Gptq { bits, .. } => format!("gptq-{bits}bit"),
            QuantizationMethod::Awq { bits } => format!("awq-{bits}bit"),
            QuantizationMethod::Gguf { quant_type } => format!("gguf-{quant_type}"),
            QuantizationMethod::BitsAndBytes { bits, .. } => format!("bnb-{bits}bit"),
        };
        let output_path = model_path.with_extension(&method_str);
        tracing::info!(method = %method_str, "Quantization (stub)");

        Ok(QuantizationResult {
            method: method_str,
            original_size_mb: 0.0,
            quantized_size_mb: 0.0,
            compression_ratio: 1.0,
            perplexity_original: None,
            perplexity_quantized: None,
            output_path,
        })
    }
}
