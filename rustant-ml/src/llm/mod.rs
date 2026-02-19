//! LLM/VLM fine-tuning — LoRA, QLoRA, quantization, alignment, red teaming.

pub mod adapter;
pub mod alignment;
pub mod dataset_prep;
pub mod eval_harness;
pub mod finetune;
pub mod quantize;
pub mod red_team;

pub use adapter::AdapterManager;
pub use alignment::AlignmentReport;
pub use dataset_prep::ChatDatasetBuilder;
pub use finetune::{FineTuneConfig, FineTuneJob, FineTuneMethod};
pub use quantize::{QuantizationMethod, QuantizationRunner};
pub use red_team::RedTeamReport;
