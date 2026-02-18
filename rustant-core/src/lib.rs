//! # Rustant Core
//!
//! Core library for the Rustant autonomous agent.
//! Provides the agent orchestrator, LLM interface (brain), memory system,
//! safety guardian, configuration, and fundamental types.

pub mod agent;
pub mod audit;
pub mod brain;
pub mod browser;
pub mod cache;
pub mod canvas;
pub mod channels;
pub mod config;
pub mod council;
pub mod credentials;
pub mod embeddings;
pub mod encryption;
pub mod error;
pub mod evaluation;
pub mod explanation;
pub mod gateway;
pub mod hooks;
pub mod indexer;
pub mod injection;
pub mod memory;
pub mod merkle;
pub mod metrics;
pub mod multi;
pub mod nodes;
pub mod oauth;
pub mod pairing;
pub mod personas;
pub mod plan;
pub mod project_detect;
pub mod providers;
pub mod replay;
pub mod safety;
pub mod sandbox;
pub mod sanitize;
pub mod scheduler;
pub mod search;
pub mod secret_ref;
pub mod session_manager;
pub mod skills;
pub mod summarizer;
pub mod types;
pub mod updater;
pub mod voice;
pub mod workflow;

// Re-export commonly used types at the crate root.
pub use agent::{
    Agent, AgentCallback, AgentMessage, BudgetSeverity, ContextHealthEvent, NoOpCallback,
    RegisteredTool, TaskResult,
};
pub use brain::{Brain, LlmProvider, MockLlmProvider, TokenCounter};
#[cfg(feature = "browser")]
pub use browser::ChromiumCdpClient;
pub use browser::{
    BrowserSecurityGuard, BrowserSession, CdpClient, MockCdpClient, PageSnapshot, SnapshotMode,
};
pub use channels::cdc::{CdcAction, CdcConfig, CdcProcessor, CdcState};
pub use channels::style_tracker::{CommunicationStyleTracker, SenderStyleProfile};
pub use channels::{
    AutoReplyEngine, Channel, ChannelAgentBridge, ChannelCapabilities, ChannelDigest,
    ChannelManager, ChannelMessage, ChannelStatus, ChannelType, ChannelUser, ClassificationCache,
    ClassifiedMessage, DigestActionItem, DigestCollector, DigestHighlight, EmailCategory,
    EmailClassification, EmailIntelligence, FollowUpReminder, IMessageChannel, IMessageConfig,
    IntelligenceResult, IrcChannel, IrcConfig, LlmClassificationResponse, MessageClassifier,
    MessageContent, MessageId, MessageType, PendingReply, ReminderStatus, ReplyStatus,
    ResolvedContact, SchedulerBridge, SenderProfile, SmsChannel, SmsConfig, StreamingMode,
    SuggestedAction, TeamsChannel, TeamsConfig, ThreadId, WebhookChannel, WebhookConfig,
};
pub use config::MultiAgentConfig;
pub use config::{
    AgentConfig, ApprovalMode, CouncilConfig, CouncilMemberConfig, ExternalMcpServerConfig,
    KnowledgeConfig, VotingStrategy, config_exists,
};
pub use config::{
    AutoReplyMode, ChannelIntelligenceConfig, DigestFrequency, IntelligenceConfig,
    MessagePriority as ChannelMessagePriority,
};
pub use council::{
    CouncilMemberResponse, CouncilResult, DetectedProvider, PeerReview, PlanningCouncil,
    detect_available_providers, should_use_council,
};
pub use hooks::{HookDefinition, HookEvent, HookRegistry, HookResult};
pub use plan::{
    ExecutionPlan, PlanAlternative, PlanConfig, PlanDecision, PlanStatus, PlanStep, StepStatus,
};
pub use tokio_util::sync::CancellationToken;

pub use credentials::{
    CredentialError, CredentialStore, InMemoryCredentialStore, KeyringCredentialStore,
};
pub use encryption::{EncryptionError, SessionEncryptor};
pub use error::BrowserError;
pub use error::SchedulerError;
pub use error::VoiceError;
pub use error::{ChannelError, NodeError};
pub use error::{Result, RustantError};
pub use explanation::{DecisionExplanation, DecisionType, ExplanationBuilder, FactorInfluence};
pub use gateway::{
    ChannelBridge, ClientMessage, GatewayConfig, GatewayEvent, NodeBridge, ServerMessage,
};
pub use indexer::{IndexStats, IndexerConfig, ProjectIndexer};
pub use injection::{
    InjectionDetector, InjectionScanResult, InjectionType, Severity as InjectionSeverity,
};
pub use memory::{
    BehavioralRule, ContextBreakdown, KnowledgeDistiller, KnowledgeStore, MemorySystem, Session,
    SessionMetadata,
};
pub use merkle::{AuditNode, MerkleChain, VerificationResult};
pub use multi::AgentStatus as MultiAgentStatus;
pub use multi::{
    AgentContext, AgentEnvelope, AgentOrchestrator, AgentPayload, AgentRoute, AgentRouter,
    AgentSpawner, AgentTeam, CoordinationStrategy, MessageBus, MessagePriority, ResourceLimits,
    SharedContext, TaskHandler, TeamDecision, TeamMember, TeamRegistry, TeamTask, TeamTaskStatus,
};
pub use nodes::{
    Capability, ConsentEntry, ConsentStore, DiscoveredNode, Node, NodeCapability, NodeDiscovery,
    NodeHealth, NodeId, NodeManager, NodeMessage, NodeResult, NodeTask, Platform, RateLimit,
};
pub use oauth::AuthMethod;
pub use pairing::{DeviceIdentity, PairingChallenge, PairingManager, PairingResult};
pub use project_detect::{
    ProjectInfo, ProjectType, detect_project, example_tasks, recommended_allowed_commands,
};
pub use providers::{
    CircuitBreaker, CircuitState, FailoverProvider, GeminiProvider, ModelInfo,
    create_council_members, create_provider, create_provider_with_auth, is_ollama_available,
    list_ollama_models, resolve_api_key_by_env,
};
pub use safety::{
    AdaptiveTrust, ApprovalContext, ApprovalDecision, BehavioralFingerprint, ContractEnforcer,
    Invariant, PermissionPolicy, Predicate, ResourceBounds, ReversibilityInfo, SafetyContract,
    SafetyGuardian, ToolRateLimiter,
};
pub use sandbox::SandboxedFs;
pub use scheduler::{
    BackgroundJob, CronJob, CronJobConfig, CronScheduler, HeartbeatConfig, HeartbeatManager,
    JobManager, JobStatus, WebhookEndpoint, WebhookHandler,
};
pub use search::{HybridSearchEngine, SearchConfig, SearchResult};
pub use secret_ref::{MigrationResult, SecretRef, SecretResolveError, SecretResolver};
pub use session_manager::{SessionEntry, SessionIndex, SessionManager};
pub use skills::{
    ParseError as SkillParseError, SkillConfig, SkillDefinition, SkillLoader, SkillRegistry,
    SkillRequirement, SkillRiskLevel, SkillToolDef, ValidationError, ValidationResult,
    parse_skill_md, validate_skill,
};
pub use summarizer::{ContextSummarizer, ContextSummary, TokenAlert, TokenCostDisplay};
pub use types::{
    AgentState, AgentStatus, Artifact, CitationSource, CompletionRequest, CompletionResponse,
    Content, CostEstimate, GroundingResult, GroundingTool, ImageSource, Message, ProgressUpdate,
    ResponseFormat, RiskLevel, Role, StreamEvent, TaskClassification, ThinkingConfig, TokenUsage,
    ToolChoice, ToolDefinition, ToolOutput,
};
pub use voice::{
    AudioChunk, AudioFormat, MeetingRecordingSession, MeetingResult, MeetingStatus,
    MockSttProvider, MockTtsProvider, MockWakeDetector, OpenAiSttProvider, OpenAiTtsProvider,
    SttProvider, SttWakeDetector, SynthesisRequest, SynthesisResult, ToggleState,
    TranscriptionResult, TranscriptionSegment, TtsProvider, VadEvent, VoiceActivityDetector,
    VoiceCommandSession, WakeWordDetector, audio_convert,
};
#[cfg(feature = "voice")]
pub use voice::{
    AudioInput, AudioOutput, PorcupineWakeDetector, VoicePipeline, VoicePipelineEvent,
    WhisperLocalProvider,
};
pub use workflow::{
    WorkflowDefinition, WorkflowExecutor, WorkflowState, WorkflowStatus, get_builtin,
    list_builtin_names, parse_workflow, validate_workflow,
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
