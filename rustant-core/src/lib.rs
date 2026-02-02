//! # Rustant Core
//!
//! Core library for the Rustant autonomous agent.
//! Provides the agent orchestrator, LLM interface (brain), memory system,
//! safety guardian, configuration, and fundamental types.

pub mod agent;
pub mod audit;
pub mod brain;
pub mod config;
pub mod credentials;
pub mod error;
pub mod explanation;
pub mod injection;
pub mod memory;
pub mod merkle;
pub mod pairing;
pub mod providers;
pub mod replay;
pub mod safety;
pub mod sandbox;
pub mod search;
pub mod summarizer;
pub mod types;
pub mod gateway;
pub mod channels;
pub mod nodes;
pub mod multi;
pub mod oauth;
pub mod workflow;
pub mod browser;
pub mod scheduler;
pub mod voice;

// Re-export commonly used types at the crate root.
pub use agent::{Agent, AgentCallback, AgentMessage, NoOpCallback, RegisteredTool, TaskResult};
pub use brain::{Brain, LlmProvider, MockLlmProvider, TokenCounter};
pub use config::{AgentConfig, ApprovalMode};
pub use oauth::AuthMethod;
pub use credentials::{CredentialError, CredentialStore, InMemoryCredentialStore, KeyringCredentialStore};
pub use error::{Result, RustantError};
pub use memory::{MemorySystem, Session, SessionMetadata};
pub use providers::{create_provider, create_provider_with_auth, CircuitBreaker, CircuitState, FailoverProvider, GeminiProvider, ModelInfo};
pub use explanation::{DecisionExplanation, DecisionType, ExplanationBuilder, FactorInfluence};
pub use injection::{InjectionDetector, InjectionScanResult, InjectionType, Severity as InjectionSeverity};
pub use merkle::{AuditNode, MerkleChain, VerificationResult};
pub use pairing::{DeviceIdentity, PairingChallenge, PairingManager, PairingResult};
pub use safety::{ApprovalContext, ReversibilityInfo, SafetyGuardian};
pub use search::{HybridSearchEngine, SearchConfig, SearchResult};
pub use gateway::{GatewayConfig, GatewayEvent, ClientMessage, ServerMessage, ChannelBridge, NodeBridge};
pub use channels::{
    Channel, ChannelAgentBridge, ChannelCapabilities, ChannelManager, ChannelMessage,
    ChannelStatus, ChannelType, ChannelUser, MessageContent, MessageId, StreamingMode, ThreadId,
    IMessageChannel, IMessageConfig, TeamsChannel, TeamsConfig, SmsChannel, SmsConfig,
    IrcChannel, IrcConfig, WebhookChannel, WebhookConfig,
};
pub use error::{ChannelError, NodeError};
pub use nodes::{
    Capability, ConsentEntry, ConsentStore, DiscoveredNode, Node, NodeCapability, NodeDiscovery,
    NodeHealth, NodeId, NodeManager, NodeMessage, NodeResult, NodeTask, Platform, RateLimit,
};
pub use multi::{
    AgentContext, AgentEnvelope, AgentOrchestrator, AgentPayload, AgentRoute, AgentRouter,
    AgentSpawner, MessageBus, MessagePriority, ResourceLimits, TaskHandler,
};
pub use multi::AgentStatus as MultiAgentStatus;
pub use config::MultiAgentConfig;
pub use sandbox::SandboxedFs;
pub use workflow::{
    WorkflowDefinition, WorkflowExecutor, WorkflowState, WorkflowStatus,
    parse_workflow, validate_workflow, list_builtin_names, get_builtin,
};
pub use browser::{
    BrowserSecurityGuard, BrowserSession, CdpClient, MockCdpClient, PageSnapshot, SnapshotMode,
};
pub use error::BrowserError;
pub use error::SchedulerError;
pub use error::VoiceError;
pub use voice::{
    AudioChunk, AudioFormat, MockSttProvider, MockTtsProvider, MockWakeDetector,
    OpenAiSttProvider, OpenAiTtsProvider, SttProvider, SttWakeDetector,
    SynthesisRequest, SynthesisResult, TranscriptionResult, TranscriptionSegment,
    TtsProvider, VadEvent, VoiceActivityDetector, WakeWordDetector, audio_convert,
};
#[cfg(feature = "voice")]
pub use voice::{
    AudioInput, AudioOutput, PorcupineWakeDetector, VoicePipeline, VoicePipelineEvent,
    WhisperLocalProvider,
};
pub use scheduler::{
    BackgroundJob, CronJob, CronJobConfig, CronScheduler, HeartbeatConfig, HeartbeatManager,
    JobManager, JobStatus, WebhookEndpoint, WebhookHandler,
};
pub use summarizer::{ContextSummarizer, ContextSummary, TokenAlert, TokenCostDisplay};
pub use types::{
    AgentState, AgentStatus, Artifact, CompletionRequest, CompletionResponse, Content,
    CostEstimate, Message, RiskLevel, Role, StreamEvent, TokenUsage, ToolDefinition, ToolOutput,
};

#[cfg(test)]
mod reexport_tests {
    use super::*;

    #[test]
    fn test_lib_new_reexports_channels() {
        // Verify new channel-related types are accessible from crate root.
        let _caps = ChannelCapabilities::default();
        let _mode = StreamingMode::Polling { interval_ms: 1000 };
        let _tid = ThreadId("thread-1".into());
        let _bridge = ChannelBridge;

        // Verify new channel implementations are accessible.
        let _imessage_cfg = IMessageConfig::default();
        let _teams_cfg = TeamsConfig::default();
        let _sms_cfg = SmsConfig::default();
        let _irc_cfg = IrcConfig::default();
        let _webhook_cfg = WebhookConfig::default();
    }

    #[test]
    fn test_lib_new_reexports_browser() {
        // Verify browser types are accessible from crate root.
        let _guard = BrowserSecurityGuard::default();
        let _mode = SnapshotMode::Html;
        let _mock = MockCdpClient::new();
    }

    #[test]
    fn test_lib_new_reexports_scheduler() {
        // Verify scheduler types are accessible from crate root.
        let _scheduler = CronScheduler::new();
        let _config = HeartbeatConfig::default();
        let _manager = JobManager::new(10);
        let _status = JobStatus::Pending;
        let _endpoint = WebhookEndpoint::new("/hooks");
    }

    #[test]
    fn test_lib_new_reexports_voice() {
        // Verify voice types are accessible from crate root.
        let _vad = VoiceActivityDetector::new(0.01);
        let _chunk = AudioChunk::silence(16000, 1, 480);
        let _mock_stt = MockSttProvider::new();
        let _mock_tts = MockTtsProvider::new();
        let _format = AudioFormat::Wav;
        let _req = SynthesisRequest::new("test");
        let _event = VadEvent::NoChange;
    }

    #[test]
    fn test_lib_new_reexports_nodes_multi() {
        // Verify new node types are accessible from crate root.
        let _cap = NodeCapability::basic(Capability::Shell);
        let _rl = RateLimit {
            max_calls: 10,
            window_secs: 60,
        };
        let _msg = NodeMessage::Ping;

        // Verify multi-agent types are accessible.
        let _limits = ResourceLimits::default();
        let _prio = MessagePriority::Critical;
        let _status = MultiAgentStatus::Idle;

        // Verify bridge types are accessible.
        let _nb = NodeBridge;
    }
}
