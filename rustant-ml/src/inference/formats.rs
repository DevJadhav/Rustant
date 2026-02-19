//! Model format detection.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported model formats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    Gguf,
    Gptq,
    Awq,
    SafeTensors,
    PyTorch,
    Onnx,
    CoreMl,
    TfLite,
    Unknown,
}

/// Detect model format from file extension or content.
pub fn detect_format(path: &Path) -> ModelFormat {
    match path.extension().and_then(|e| e.to_str()) {
        Some("gguf") => ModelFormat::Gguf,
        Some("onnx") => ModelFormat::Onnx,
        Some("mlmodel" | "mlpackage") => ModelFormat::CoreMl,
        Some("tflite") => ModelFormat::TfLite,
        Some("safetensors") => ModelFormat::SafeTensors,
        Some("pt" | "pth" | "bin") => ModelFormat::PyTorch,
        _ => ModelFormat::Unknown,
    }
}
