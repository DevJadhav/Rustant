//! LLM fine-tuning job management.

use crate::training::experiment::TrainingStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Fine-tuning method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FineTuneMethod {
    FullFineTune,
    LoRA {
        rank: u32,
        alpha: f32,
        target_modules: Vec<String>,
    },
    QLoRA {
        rank: u32,
        alpha: f32,
        bits: u8,
    },
    Prefix {
        prefix_length: usize,
    },
    Adapter {
        adapter_type: String,
    },
}

/// Fine-tuning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuneConfig {
    pub method: FineTuneMethod,
    pub learning_rate: f64,
    pub epochs: usize,
    pub batch_size: usize,
    pub warmup_steps: usize,
    pub max_seq_length: usize,
    pub gradient_accumulation_steps: usize,
}

impl Default for FineTuneConfig {
    fn default() -> Self {
        Self {
            method: FineTuneMethod::LoRA {
                rank: 16,
                alpha: 32.0,
                target_modules: vec!["q_proj".into(), "v_proj".into()],
            },
            learning_rate: 2e-4,
            epochs: 3,
            batch_size: 4,
            warmup_steps: 100,
            max_seq_length: 2048,
            gradient_accumulation_steps: 4,
        }
    }
}

/// A fine-tuning job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuneJob {
    pub id: String,
    pub base_model: String,
    pub method: FineTuneMethod,
    pub dataset_id: String,
    pub config: FineTuneConfig,
    pub status: TrainingStatus,
    pub output_path: Option<PathBuf>,
    pub alignment_report: Option<super::alignment::AlignmentReport>,
    pub red_team_report: Option<super::red_team::RedTeamReport>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FineTuneJob {
    pub fn new(base_model: &str, dataset_id: &str, config: FineTuneConfig) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            base_model: base_model.to_string(),
            method: config.method.clone(),
            dataset_id: dataset_id.to_string(),
            config,
            status: TrainingStatus::Pending,
            output_path: None,
            alignment_report: None,
            red_team_report: None,
            created_at: now,
            updated_at: now,
        }
    }
}
