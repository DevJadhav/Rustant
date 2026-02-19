//! Model format conversion.

use crate::error::MlError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Target conversion format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionFormat {
    Onnx,
    CoreMl,
    Gguf { quant_type: Option<String> },
    TfLite,
    SafeTensors,
}

/// Result of a model conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub source_path: PathBuf,
    pub output_path: PathBuf,
    pub target_format: String,
    pub source_size_bytes: u64,
    pub output_size_bytes: u64,
    pub compression_ratio: f64,
}

/// Model converter (delegates to Python/CLI tools).
pub struct ModelConverter {
    #[allow(dead_code)]
    workspace: PathBuf,
}

impl ModelConverter {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    pub async fn convert(
        &self,
        source: &Path,
        format: &ConversionFormat,
    ) -> Result<ConversionResult, MlError> {
        let format_str = match format {
            ConversionFormat::Onnx => "onnx",
            ConversionFormat::CoreMl => "coreml",
            ConversionFormat::Gguf { .. } => "gguf",
            ConversionFormat::TfLite => "tflite",
            ConversionFormat::SafeTensors => "safetensors",
        };
        let output = source.with_extension(format_str);

        tracing::info!(source = %source.display(), format = format_str, "Model conversion (stub)");

        Ok(ConversionResult {
            source_path: source.to_path_buf(),
            output_path: output,
            target_format: format_str.to_string(),
            source_size_bytes: 0,
            output_size_bytes: 0,
            compression_ratio: 1.0,
        })
    }
}
