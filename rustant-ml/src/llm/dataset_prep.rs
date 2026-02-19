//! Chat dataset preparation for fine-tuning.

use crate::error::MlError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Chat dataset format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatFormat {
    Alpaca,
    ShareGpt,
    ChatMl,
    Llama3,
    OpenAi,
}

/// A chat message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// A conversation for fine-tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Builder for chat datasets.
pub struct ChatDatasetBuilder {
    conversations: Vec<Conversation>,
    #[allow(dead_code)]
    format: ChatFormat,
}

impl ChatDatasetBuilder {
    pub fn new(format: ChatFormat) -> Self {
        Self {
            conversations: Vec::new(),
            format,
        }
    }

    pub fn add_conversation(&mut self, conv: Conversation) {
        self.conversations.push(conv);
    }

    pub fn len(&self) -> usize {
        self.conversations.len()
    }
    pub fn is_empty(&self) -> bool {
        self.conversations.is_empty()
    }

    /// Export to JSONL file.
    pub fn export(&self, path: &PathBuf) -> Result<usize, MlError> {
        let mut output = String::new();
        for conv in &self.conversations {
            let line = serde_json::to_string(conv)?;
            output.push_str(&line);
            output.push('\n');
        }
        std::fs::write(path, &output)?;
        Ok(self.conversations.len())
    }

    /// Validate conversations (check for empty messages, role consistency).
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        for (i, conv) in self.conversations.iter().enumerate() {
            if conv.messages.is_empty() {
                issues.push(format!("Conversation {i}: empty"));
            }
            for (j, msg) in conv.messages.iter().enumerate() {
                if msg.content.trim().is_empty() {
                    issues.push(format!("Conversation {i}, message {j}: empty content"));
                }
                if !["system", "user", "assistant"].contains(&msg.role.as_str()) {
                    issues.push(format!(
                        "Conversation {i}, message {j}: invalid role '{}'",
                        msg.role
                    ));
                }
            }
        }
        issues
    }
}
