//! Neural network architecture configurations.

use serde::{Deserialize, Serialize};

/// Architecture type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureType {
    Mlp,
    Cnn,
    Rnn,
    Lstm,
    Transformer,
    Autoencoder,
}

/// Layer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerConfig {
    Linear {
        in_features: usize,
        out_features: usize,
    },
    Conv2d {
        in_channels: usize,
        out_channels: usize,
        kernel_size: usize,
    },
    Lstm {
        input_size: usize,
        hidden_size: usize,
        num_layers: usize,
    },
    TransformerEncoder {
        d_model: usize,
        nhead: usize,
        num_layers: usize,
    },
    Dropout {
        p: f64,
    },
    BatchNorm {
        num_features: usize,
    },
    Activation {
        function: String,
    },
}

/// Optimizer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OptimizerConfig {
    Adam { lr: f64, weight_decay: f64 },
    Sgd { lr: f64, momentum: f64 },
    AdamW { lr: f64, weight_decay: f64 },
    RmsProp { lr: f64 },
}

/// Full architecture configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureConfig {
    pub arch_type: ArchitectureType,
    pub layers: Vec<LayerConfig>,
    pub optimizer: OptimizerConfig,
    pub loss_function: String,
    pub batch_size: usize,
    pub epochs: usize,
}
