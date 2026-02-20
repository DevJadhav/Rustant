//! Data flow tracking for transparency.
//!
//! Records every data movement: user input → LLM, tool output → LLM,
//! file content → tool, etc. Provides a queryable audit of where user
//! data has been sent, supporting the Transparency pillar.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;

/// Maximum number of data flow entries to retain in memory.
const MAX_ENTRIES: usize = 10_000;

/// Where data originated from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataSource {
    /// User typed input.
    UserInput,
    /// Output from a tool execution.
    ToolOutput { tool: String },
    /// Content read from a file.
    FileContent { path: String },
    /// A fact from long-term memory.
    MemoryFact,
    /// Previous session history.
    SessionHistory,
    /// Voice input via Siri bridge.
    SiriVoiceInput,
    /// System/internal data.
    System,
}

impl std::fmt::Display for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSource::UserInput => write!(f, "user_input"),
            DataSource::ToolOutput { tool } => write!(f, "tool:{tool}"),
            DataSource::FileContent { path } => write!(f, "file:{path}"),
            DataSource::MemoryFact => write!(f, "memory"),
            DataSource::SessionHistory => write!(f, "session_history"),
            DataSource::SiriVoiceInput => write!(f, "siri_voice"),
            DataSource::System => write!(f, "system"),
        }
    }
}

/// Where data was sent to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataDestination {
    /// Sent to an LLM provider.
    LlmProvider { provider: String, model: String },
    /// Written to local storage (file, database).
    LocalStorage { path: String },
    /// Passed to a tool for execution.
    ToolExecution { tool: String },
    /// Stored in memory (short-term or long-term).
    Memory,
}

impl std::fmt::Display for DataDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataDestination::LlmProvider { provider, model } => {
                write!(f, "llm:{provider}/{model}")
            }
            DataDestination::LocalStorage { path } => write!(f, "local:{path}"),
            DataDestination::ToolExecution { tool } => write!(f, "tool:{tool}"),
            DataDestination::Memory => write!(f, "memory"),
        }
    }
}

/// A single recorded data movement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlow {
    /// Unique flow ID.
    pub id: usize,
    /// When the flow occurred.
    pub timestamp: DateTime<Utc>,
    /// Where the data came from.
    pub source: DataSource,
    /// Where the data was sent.
    pub destination: DataDestination,
    /// Type of data (e.g., "text", "code", "json").
    pub data_type: String,
    /// Estimated token count of the data.
    pub token_count: usize,
    /// Whether the data was redacted before sending.
    pub was_redacted: bool,
    /// Current consent status for this flow.
    pub consent_status: ConsentStatus,
}

/// Consent status for a data flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConsentStatus {
    /// Consent was explicitly granted.
    Granted,
    /// Consent was implied (default behavior).
    Implied,
    /// Consent was not checked.
    NotChecked,
    /// Consent was denied but flow proceeded (with redaction).
    DeniedRedacted,
}

/// Tracks all data flows for transparency.
pub struct DataFlowTracker {
    entries: VecDeque<DataFlow>,
    next_id: usize,
    persist_path: Option<PathBuf>,
}

impl DataFlowTracker {
    /// Create a new tracker.
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 0,
            persist_path: None,
        }
    }

    /// Create a tracker with persistence to a file.
    pub fn with_persistence(path: PathBuf) -> Self {
        let mut tracker = Self::new();
        tracker.persist_path = Some(path);
        tracker
    }

    /// Record a new data flow.
    pub fn record(
        &mut self,
        source: DataSource,
        destination: DataDestination,
        data_type: impl Into<String>,
        token_count: usize,
        was_redacted: bool,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        let flow = DataFlow {
            id,
            timestamp: Utc::now(),
            source,
            destination,
            data_type: data_type.into(),
            token_count,
            was_redacted,
            consent_status: ConsentStatus::Implied,
        };

        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(flow);

        id
    }

    /// Get recent N flows.
    pub fn recent(&self, n: usize) -> Vec<&DataFlow> {
        self.entries.iter().rev().take(n).collect()
    }

    /// Get flows to a specific destination type.
    pub fn flows_to_provider(&self, provider: &str) -> Vec<&DataFlow> {
        self.entries
            .iter()
            .filter(|f| match &f.destination {
                DataDestination::LlmProvider { provider: p, .. } => p == provider,
                _ => false,
            })
            .collect()
    }

    /// Get flows from a specific source type.
    pub fn flows_from_source(&self, source_type: &str) -> Vec<&DataFlow> {
        self.entries
            .iter()
            .filter(|f| f.source.to_string().starts_with(source_type))
            .collect()
    }

    /// Total tokens sent to LLM providers.
    pub fn total_tokens_to_providers(&self) -> usize {
        self.entries
            .iter()
            .filter(|f| matches!(f.destination, DataDestination::LlmProvider { .. }))
            .map(|f| f.token_count)
            .sum()
    }

    /// Number of flows that were redacted.
    pub fn redacted_count(&self) -> usize {
        self.entries.iter().filter(|f| f.was_redacted).count()
    }

    /// Total number of recorded flows.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the tracker is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Format recent flows as human-readable text for `/dataflow`.
    pub fn format_recent(&self, n: usize) -> String {
        let flows = self.recent(n);
        if flows.is_empty() {
            return "No data flows recorded yet.".to_string();
        }

        let mut output = format!("Recent data flows ({} total):\n\n", self.len());
        for flow in flows.iter().rev() {
            output.push_str(&format!(
                "[{}] {} -> {} ({} tokens, {}{})\n",
                flow.timestamp.format("%H:%M:%S"),
                flow.source,
                flow.destination,
                flow.token_count,
                flow.data_type,
                if flow.was_redacted { ", redacted" } else { "" },
            ));
        }

        let total_to_llm = self.total_tokens_to_providers();
        let redacted = self.redacted_count();
        output.push_str(&format!(
            "\nSummary: {total_to_llm} total tokens to LLM providers, {redacted} flows redacted\n",
        ));

        output
    }

    /// Persist current flows to disk (if persistence is configured).
    pub fn persist(&self) -> Result<(), std::io::Error> {
        if let Some(ref path) = self.persist_path {
            let data = serde_json::to_string_pretty(&self.entries.iter().collect::<Vec<_>>())
                .map_err(std::io::Error::other)?;
            crate::persistence::atomic_write(path, data.as_bytes())?;
        }
        Ok(())
    }
}

impl Default for DataFlowTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_query() {
        let mut tracker = DataFlowTracker::new();

        tracker.record(
            DataSource::UserInput,
            DataDestination::LlmProvider {
                provider: "anthropic".into(),
                model: "claude-3-opus".into(),
            },
            "text",
            500,
            false,
        );

        tracker.record(
            DataSource::ToolOutput {
                tool: "file_read".into(),
            },
            DataDestination::LlmProvider {
                provider: "anthropic".into(),
                model: "claude-3-opus".into(),
            },
            "code",
            1200,
            true,
        );

        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.total_tokens_to_providers(), 1700);
        assert_eq!(tracker.redacted_count(), 1);
    }

    #[test]
    fn test_flows_to_provider() {
        let mut tracker = DataFlowTracker::new();

        tracker.record(
            DataSource::UserInput,
            DataDestination::LlmProvider {
                provider: "anthropic".into(),
                model: "claude".into(),
            },
            "text",
            100,
            false,
        );
        tracker.record(
            DataSource::UserInput,
            DataDestination::LlmProvider {
                provider: "openai".into(),
                model: "gpt-4".into(),
            },
            "text",
            200,
            false,
        );

        let anthropic_flows = tracker.flows_to_provider("anthropic");
        assert_eq!(anthropic_flows.len(), 1);
        assert_eq!(anthropic_flows[0].token_count, 100);
    }

    #[test]
    fn test_capacity_limit() {
        let mut tracker = DataFlowTracker::new();
        for i in 0..MAX_ENTRIES + 100 {
            tracker.record(
                DataSource::UserInput,
                DataDestination::Memory,
                "text",
                i,
                false,
            );
        }
        assert_eq!(tracker.len(), MAX_ENTRIES);
    }

    #[test]
    fn test_format_recent() {
        let mut tracker = DataFlowTracker::new();
        tracker.record(
            DataSource::UserInput,
            DataDestination::LlmProvider {
                provider: "anthropic".into(),
                model: "claude".into(),
            },
            "text",
            500,
            false,
        );

        let formatted = tracker.format_recent(10);
        assert!(formatted.contains("user_input"));
        assert!(formatted.contains("anthropic"));
        assert!(formatted.contains("500 tokens"));
    }

    #[test]
    fn test_data_source_display() {
        assert_eq!(DataSource::UserInput.to_string(), "user_input");
        assert_eq!(
            DataSource::ToolOutput {
                tool: "file_read".into()
            }
            .to_string(),
            "tool:file_read"
        );
        assert_eq!(DataSource::SiriVoiceInput.to_string(), "siri_voice");
    }
}
