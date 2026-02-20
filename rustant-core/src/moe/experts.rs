//! Expert agent definitions and tool-to-expert mapping.
//!
//! Each expert covers a domain of tools and is associated with one or more
//! `TaskClassification` variants. The router maps classifications to experts,
//! and experts expose only their subset of the full tool registry.
//!
//! # DeepSeek V3-Inspired Architecture
//!
//! 20 fine-grained experts (up from 10) enable C(20,3)=1,140 possible
//! Top-3 routings vs only 10 with single-expert routing. Each expert
//! has max 12 domain tools. 8 shared tools are always sent regardless
//! of routing, analogous to DeepSeek's "shared expert" concept.

use crate::types::TaskClassification;
use serde::{Deserialize, Serialize};

/// Unique identifier for each expert in the MoE system.
///
/// 20 fine-grained experts, each with max 12 domain tools.
/// The shared/always-on tools (8) are NOT included in any expert's
/// domain list — they are always sent separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExpertId {
    // --- System & Files (split from old System) ---
    /// File operations: read, write, search, organize (~8 tools)
    FileOps,
    /// Git and codebase operations (~5 tools)
    Git,

    // --- macOS (split from old MacOS) ---
    /// macOS apps: calendar, reminders, notes, mail, music, shortcuts (~10 tools)
    MacOSApps,
    /// macOS system: app_control, clipboard, screenshot, finder, spotlight (~8 tools)
    MacOSSystem,
    /// Screen automation: GUI scripting, accessibility, OCR, contacts, safari (~6 tools)
    ScreenUI,
    /// Communication: iMessage, Slack, Siri (~5 tools)
    Communication,

    // --- Web ---
    /// Web browsing and research (~8 tools)
    WebBrowse,

    // --- Development ---
    /// Dev tools: scaffold, dev_server, database, test_runner, lint (~6 tools)
    DevTools,

    // --- Productivity ---
    /// Personal productivity: knowledge_graph, career, life_planner, etc. (~10 tools)
    Productivity,

    // --- Security (split from old Security) ---
    /// Security scanning: SAST, SCA, secrets, container, IaC (~9 tools)
    SecScan,
    /// Security review: code_review, quality, dead_code, tech_debt (~9 tools)
    SecReview,
    /// Security compliance: license, SBOM, policy, risk, audit (~8 tools)
    SecCompliance,
    /// Security incidents: alerts, triage, response, log analysis (~7 tools)
    SecIncident,

    // --- ML/AI (split from old ML) ---
    /// ML training: experiment, finetune, checkpoint, quantize (~10 tools)
    MLTrain,
    /// ML data: source, schema, transform, features (~9 tools)
    MLData,
    /// ML inference & RAG: rag_*, inference_* (~8 tools)
    MLInference,
    /// ML safety: PII, bias, alignment, adversarial (~8 tools)
    MLSafety,
    /// ML research & eval: research_*, eval_*, explain (~9 tools)
    MLResearch,

    // --- SRE ---
    /// SRE/DevOps: alerts, deployments, prometheus, kubernetes, oncall (~6 tools)
    SRE,

    // --- Research ---
    /// Deep research: web, arxiv, document analysis, knowledge graph (~5 tools)
    Research,
}

impl ExpertId {
    /// All expert variants for enumeration.
    pub fn all() -> &'static [ExpertId] {
        &[
            ExpertId::FileOps,
            ExpertId::Git,
            ExpertId::MacOSApps,
            ExpertId::MacOSSystem,
            ExpertId::ScreenUI,
            ExpertId::Communication,
            ExpertId::WebBrowse,
            ExpertId::DevTools,
            ExpertId::Productivity,
            ExpertId::SecScan,
            ExpertId::SecReview,
            ExpertId::SecCompliance,
            ExpertId::SecIncident,
            ExpertId::MLTrain,
            ExpertId::MLData,
            ExpertId::MLInference,
            ExpertId::MLSafety,
            ExpertId::MLResearch,
            ExpertId::SRE,
            ExpertId::Research,
        ]
    }

    /// Map a task classification to the most appropriate expert.
    pub fn from_classification(classification: &TaskClassification) -> Self {
        match classification {
            // General fallback to FileOps (most versatile basic tools)
            TaskClassification::General => ExpertId::FileOps,
            TaskClassification::Workflow(name) => Self::from_workflow_name(name),

            // File and search operations
            TaskClassification::FileOperation | TaskClassification::Search => ExpertId::FileOps,

            // Git operations
            TaskClassification::GitOperation => ExpertId::Git,

            // Code analysis and intelligence
            TaskClassification::CodeAnalysis | TaskClassification::CodeIntelligence => {
                ExpertId::DevTools
            }

            // macOS native apps
            TaskClassification::Calendar
            | TaskClassification::Reminders
            | TaskClassification::Notes
            | TaskClassification::Email
            | TaskClassification::Music
            | TaskClassification::Voice
            | TaskClassification::Photos
            | TaskClassification::HomeKit
            | TaskClassification::Meeting
            | TaskClassification::DailyBriefing => ExpertId::MacOSApps,

            // macOS system tools
            TaskClassification::AppControl
            | TaskClassification::Clipboard
            | TaskClassification::Screenshot
            | TaskClassification::SystemInfo
            | TaskClassification::Notification
            | TaskClassification::Spotlight
            | TaskClassification::FocusMode
            | TaskClassification::Finder => ExpertId::MacOSSystem,

            // Screen automation
            TaskClassification::GuiScripting
            | TaskClassification::Accessibility
            | TaskClassification::Contacts
            | TaskClassification::Safari => ExpertId::ScreenUI,

            // Communication
            TaskClassification::Messaging | TaskClassification::Slack => ExpertId::Communication,

            // Web/Browser
            TaskClassification::Browser
            | TaskClassification::WebSearch
            | TaskClassification::WebFetch
            | TaskClassification::ArxivResearch => ExpertId::WebBrowse,

            // Productivity/Cognitive tools
            TaskClassification::KnowledgeGraph
            | TaskClassification::ExperimentTracking
            | TaskClassification::ContentEngine
            | TaskClassification::SkillTracker
            | TaskClassification::CareerIntel
            | TaskClassification::LifePlanner
            | TaskClassification::PrivacyManager
            | TaskClassification::SelfImprovement => ExpertId::Productivity,

            // System monitoring goes to SRE
            TaskClassification::SystemMonitor => ExpertId::SRE,

            // Deep research
            TaskClassification::DeepResearch => ExpertId::Research,
        }
    }

    /// Map a workflow name to the appropriate expert.
    fn from_workflow_name(name: &str) -> Self {
        match name {
            // Security workflows → specific security sub-experts
            "security_scan" => ExpertId::SecScan,
            "compliance_audit" => ExpertId::SecCompliance,
            "code_review_ai" | "code_review" => ExpertId::SecReview,

            // SRE workflows
            "sre_deployment" | "alert_triage" | "sre_health_review" | "incident_response" => {
                ExpertId::SRE
            }

            // DevOps workflows
            "refactor" | "test_generation" | "documentation" | "dependency_update"
            | "dependency_audit" | "fullstack_verify" => ExpertId::DevTools,

            // macOS briefing workflows
            "morning_briefing"
            | "daily_briefing_full"
            | "end_of_day_summary"
            | "email_triage"
            | "meeting_recorder" => ExpertId::MacOSApps,

            // Screen automation
            "app_automation" => ExpertId::ScreenUI,

            // Research
            "arxiv_research" | "deep_research" => ExpertId::Research,

            // ML workflows
            "ml_training" | "ml_finetune" | "ml_quantize" => ExpertId::MLTrain,
            "ml_data" | "ml_pipeline" => ExpertId::MLData,
            "ml_rag" | "ml_inference" => ExpertId::MLInference,
            "ml_safety" => ExpertId::MLSafety,
            "ml_eval" | "ml_research" | "ml_engineering" => ExpertId::MLResearch,

            // Productivity workflows
            "knowledge_graph"
            | "experiment_tracking"
            | "code_analysis"
            | "content_pipeline"
            | "skill_development"
            | "career_planning"
            | "life_planning"
            | "privacy_audit"
            | "self_improvement_loop"
            | "system_monitoring" => ExpertId::Productivity,

            _ => ExpertId::FileOps,
        }
    }

    /// Shared tools always sent regardless of routing (DeepSeek's "shared expert").
    ///
    /// These 8 tools appear in 7+ of the old experts. Moving them to shared:
    /// - Eliminates ~1200 tokens of duplication per expert
    /// - Guarantees basic file/shell capability regardless of routing
    /// - Lets routed experts focus purely on domain-specific tools
    pub fn shared_tools() -> Vec<String> {
        vec![
            "ask_user".into(),
            "echo".into(),
            "datetime".into(),
            "calculator".into(),
            "web_search".into(),
            "file_read".into(),
            "file_write".into(),
            "shell_exec".into(),
        ]
    }

    /// Legacy core tools (for backward compatibility with pruning).
    pub fn core_tools() -> Vec<String> {
        Self::shared_tools()
    }

    /// Get the tool names assigned to this expert (including shared tools).
    pub fn tool_names(&self) -> Vec<String> {
        let mut tools = Self::shared_tools();
        tools.extend(self.domain_tools().into_iter().map(String::from));
        tools
    }

    /// Domain-specific tools for this expert (excluding shared tools).
    /// Max 12 tools per expert to keep token budget manageable.
    pub fn domain_tools(&self) -> Vec<&'static str> {
        match self {
            ExpertId::FileOps => vec![
                "file_list",
                "file_search",
                "file_patch",
                "smart_edit",
                "file_organizer",
                "compress",
                "document_read",
                "pdf_generate",
            ],
            ExpertId::Git => vec![
                "git_status",
                "git_diff",
                "git_commit",
                "codebase_search",
                "code_intelligence",
            ],
            ExpertId::MacOSApps => vec![
                "macos_calendar",
                "macos_reminders",
                "macos_notes",
                "macos_mail",
                "macos_music",
                "macos_shortcuts",
                "photos",
                "homekit",
                "macos_say",
                "macos_notification",
                "macos_meeting_recorder",
                "macos_daily_briefing",
            ],
            ExpertId::MacOSSystem => vec![
                "macos_app_control",
                "macos_clipboard",
                "macos_screenshot",
                "macos_system_info",
                "macos_spotlight",
                "macos_finder",
                "macos_focus_mode",
                "macos_notification",
            ],
            ExpertId::ScreenUI => vec![
                "macos_gui_scripting",
                "macos_accessibility",
                "macos_screen_analyze",
                "macos_contacts",
                "macos_safari",
                "macos_app_control",
            ],
            ExpertId::Communication => vec![
                "imessage_send",
                "imessage_read",
                "imessage_contacts",
                "slack",
                "siri",
            ],
            ExpertId::WebBrowse => vec![
                "web_fetch",
                "http_api",
                "arxiv_research",
                "browser_navigate",
                "browser_click",
                "browser_type",
                "browser_screenshot",
                "knowledge_graph",
            ],
            ExpertId::DevTools => vec![
                "scaffold",
                "dev_server",
                "database",
                "test_runner",
                "lint",
                "template",
                "git_status",
                "git_diff",
                "git_commit",
                "codebase_search",
                "code_intelligence",
                "smart_edit",
            ],
            ExpertId::Productivity => vec![
                "knowledge_graph",
                "experiment_tracker",
                "content_engine",
                "skill_tracker",
                "career_intel",
                "life_planner",
                "privacy_manager",
                "self_improvement",
                "pomodoro",
                "inbox",
            ],
            ExpertId::SecScan => vec![
                "sast_scan",
                "sca_scan",
                "secrets_scan",
                "security_scan",
                "supply_chain_check",
                "container_scan",
                "dockerfile_lint",
                "iac_scan",
                "vulnerability_check",
            ],
            ExpertId::SecReview => vec![
                "code_review",
                "analyze_diff",
                "quality_score",
                "complexity_check",
                "dead_code_detect",
                "duplicate_detect",
                "tech_debt_report",
                "suggest_fix",
                "apply_fix",
            ],
            ExpertId::SecCompliance => vec![
                "license_check",
                "sbom_generate",
                "sbom_diff",
                "compliance_report",
                "policy_check",
                "risk_score",
                "audit_export",
                "secrets_validate",
            ],
            ExpertId::SecIncident => vec![
                "alert_status",
                "alert_triage",
                "incident_respond",
                "log_analyze",
                "threat_detect",
                "k8s_lint",
                "terraform_check",
            ],
            ExpertId::MLTrain => vec![
                "ml_train",
                "ml_experiment",
                "ml_hyperparams",
                "ml_checkpoint",
                "ml_metrics",
                "ml_finetune",
                "ml_dataset_prep",
                "ml_quantize",
                "ml_adapter",
                "ml_eval_harness",
            ],
            ExpertId::MLData => vec![
                "ml_source",
                "ml_schema",
                "ml_transform",
                "ml_validate",
                "ml_storage",
                "ml_lineage",
                "ml_feature_define",
                "ml_feature_transform",
                "ml_feature_store",
            ],
            ExpertId::MLInference => vec![
                "rag_ingest",
                "rag_chunk",
                "rag_retriever",
                "rag_reranker",
                "rag_pipeline",
                "inference_serve",
                "inference_predict",
                "inference_benchmark",
            ],
            ExpertId::MLSafety => vec![
                "ai_safety_check",
                "ai_pii_scan",
                "ai_bias_check",
                "ai_alignment_eval",
                "ai_threat_detect",
                "ai_adversarial_check",
                "ai_provenance",
                "ai_audit_trail",
            ],
            ExpertId::MLResearch => vec![
                "research_search",
                "research_summarize",
                "research_compare",
                "research_implement",
                "ai_explain",
                "ai_reasoning_trace",
                "ai_feature_importance",
                "eval_run",
                "eval_compare",
            ],
            ExpertId::SRE => vec![
                "alert_manager",
                "deployment_intel",
                "prometheus",
                "kubernetes",
                "oncall",
                "system_monitor",
            ],
            ExpertId::Research => vec![
                "arxiv_research",
                "web_fetch",
                "http_api",
                "document_read",
                "knowledge_graph",
            ],
        }
    }

    /// A short human-readable name for this expert.
    pub fn display_name(&self) -> &'static str {
        match self {
            ExpertId::FileOps => "File Operations",
            ExpertId::Git => "Git & Code",
            ExpertId::MacOSApps => "macOS Apps",
            ExpertId::MacOSSystem => "macOS System",
            ExpertId::ScreenUI => "Screen UI",
            ExpertId::Communication => "Communication",
            ExpertId::WebBrowse => "Web & Browser",
            ExpertId::DevTools => "Development",
            ExpertId::Productivity => "Productivity",
            ExpertId::SecScan => "Security Scan",
            ExpertId::SecReview => "Security Review",
            ExpertId::SecCompliance => "Compliance",
            ExpertId::SecIncident => "Incident Response",
            ExpertId::MLTrain => "ML Training",
            ExpertId::MLData => "ML Data",
            ExpertId::MLInference => "ML Inference",
            ExpertId::MLSafety => "ML Safety",
            ExpertId::MLResearch => "ML Research",
            ExpertId::SRE => "SRE/DevOps",
            ExpertId::Research => "Deep Research",
        }
    }

    /// System prompt addendum describing this expert's specialization.
    pub fn system_prompt_addendum(&self) -> &'static str {
        match self {
            ExpertId::FileOps => {
                "You are specialized in file operations: reading, writing, searching, patching, organizing files, and document processing. Focus on precise file manipulation."
            }
            ExpertId::Git => {
                "You are specialized in git version control: status, diff, commit, codebase search, and code intelligence. Focus on efficient VCS operations."
            }
            ExpertId::MacOSApps => {
                "You are specialized in macOS native apps via AppleScript: calendars, reminders, notes, mail, music, shortcuts, photos, HomeKit, notifications, meeting recording, and daily briefings."
            }
            ExpertId::MacOSSystem => {
                "You are specialized in macOS system operations: app control, clipboard, screenshots, system info, Spotlight search, Finder, Focus Mode, and notifications."
            }
            ExpertId::ScreenUI => {
                "You are specialized in macOS screen automation. Use GUI scripting for UI interaction, accessibility APIs for element inspection, and OCR for screen text extraction. Follow the workflow: app_control -> accessibility -> gui_scripting -> screen_analyze."
            }
            ExpertId::Communication => {
                "You are specialized in messaging and communication: iMessage (send, read, contacts), Slack integration, and Siri voice commands."
            }
            ExpertId::WebBrowse => {
                "You are specialized in web interaction: browser automation (navigate, click, type, screenshot), HTTP APIs, web content fetching, and academic research via arXiv."
            }
            ExpertId::DevTools => {
                "You are specialized in full-stack development: scaffolding projects, running dev servers, database operations, testing, linting, and code intelligence. Use framework-aware tools that detect project type automatically."
            }
            ExpertId::Productivity => {
                "You are specialized in personal productivity: knowledge graphs, experiment tracking, content creation, skill development, career planning, life planning, and privacy management."
            }
            ExpertId::SecScan => {
                "You are specialized in security scanning: SAST, SCA, secrets detection, supply chain analysis, container scanning, Dockerfile linting, IaC scanning, and vulnerability checking."
            }
            ExpertId::SecReview => {
                "You are specialized in code quality and security review: code review, diff analysis, quality scoring, complexity checking, dead code detection, duplication analysis, tech debt reporting, and automated fix suggestions."
            }
            ExpertId::SecCompliance => {
                "You are specialized in compliance: license checking, SBOM generation and diffing, compliance reporting, policy enforcement, risk scoring, audit export, and secrets validation."
            }
            ExpertId::SecIncident => {
                "You are specialized in incident response: alert management, alert triage, incident response execution, log analysis, threat detection, Kubernetes linting, and Terraform validation."
            }
            ExpertId::MLTrain => {
                "You are specialized in ML model training: experiment management, fine-tuning (LoRA, QLoRA), hyperparameter tuning, checkpointing, metrics logging, dataset preparation, quantization, and adapter management."
            }
            ExpertId::MLData => {
                "You are specialized in ML data engineering: data sourcing, schema management, transformations, validation, storage, lineage tracking, and feature engineering (definition, transforms, feature store)."
            }
            ExpertId::MLInference => {
                "You are specialized in ML inference and RAG: document ingestion, chunking, retrieval, reranking, RAG pipelines, model serving, prediction, and inference benchmarking."
            }
            ExpertId::MLSafety => {
                "You are specialized in AI safety and security: PII scanning, bias detection, alignment evaluation, threat detection, adversarial testing, provenance tracking, and audit trails."
            }
            ExpertId::MLResearch => {
                "You are specialized in ML research and evaluation: literature search, summarization, comparison, implementation, explainability, reasoning traces, feature importance, and evaluation runs."
            }
            ExpertId::SRE => {
                "You are specialized in Site Reliability Engineering: alert management, deployment risk assessment, Prometheus monitoring, Kubernetes operations, on-call management, and system monitoring."
            }
            ExpertId::Research => {
                "You are specialized in deep research: decomposing complex questions, gathering information from multiple sources (web, academic papers, documentation), and synthesizing findings. Always cite sources."
            }
        }
    }

    /// Keywords associated with this expert for sigmoid scoring.
    pub fn keywords(&self) -> &'static [&'static str] {
        match self {
            ExpertId::FileOps => &[
                "file",
                "read",
                "write",
                "create",
                "delete",
                "list",
                "search",
                "directory",
                "folder",
                "path",
                "move",
                "rename",
                "organize",
                "compress",
                "document",
                "pdf",
            ],
            ExpertId::Git => &[
                "git",
                "commit",
                "diff",
                "branch",
                "merge",
                "push",
                "pull",
                "status",
                "log",
                "stash",
                "rebase",
                "cherry-pick",
                "codebase",
            ],
            ExpertId::MacOSApps => &[
                "calendar",
                "reminder",
                "notes",
                "mail",
                "email",
                "music",
                "shortcut",
                "photo",
                "homekit",
                "say",
                "speak",
                "notification",
                "meeting",
                "briefing",
                "todo",
            ],
            ExpertId::MacOSSystem => &[
                "app",
                "launch",
                "quit",
                "clipboard",
                "screenshot",
                "system info",
                "battery",
                "cpu",
                "disk",
                "spotlight",
                "finder",
                "focus mode",
                "dnd",
            ],
            ExpertId::ScreenUI => &[
                "gui",
                "scripting",
                "accessibility",
                "ocr",
                "screen",
                "click",
                "button",
                "window",
                "contacts",
                "safari",
                "ui element",
                "automation",
            ],
            ExpertId::Communication => &[
                "imessage",
                "message",
                "sms",
                "text",
                "slack",
                "siri",
                "voice command",
                "chat",
                "send message",
            ],
            ExpertId::WebBrowse => &[
                "browser", "web", "fetch", "http", "url", "navigate", "webpage", "download", "api",
                "arxiv",
            ],
            ExpertId::DevTools => &[
                "scaffold",
                "dev server",
                "database",
                "test",
                "lint",
                "template",
                "build",
                "compile",
                "framework",
                "project",
                "code review",
                "refactor",
            ],
            ExpertId::Productivity => &[
                "knowledge graph",
                "experiment",
                "content",
                "skill",
                "career",
                "life plan",
                "privacy",
                "self improvement",
                "pomodoro",
                "inbox",
                "productivity",
            ],
            ExpertId::SecScan => &[
                "sast",
                "sca",
                "secret",
                "vulnerability",
                "scan",
                "supply chain",
                "container",
                "dockerfile",
                "iac",
                "security scan",
            ],
            ExpertId::SecReview => &[
                "code review",
                "quality",
                "complexity",
                "dead code",
                "duplication",
                "tech debt",
                "autofix",
                "suggest fix",
            ],
            ExpertId::SecCompliance => &[
                "license",
                "sbom",
                "compliance",
                "policy",
                "risk",
                "audit",
                "regulation",
                "standard",
            ],
            ExpertId::SecIncident => &[
                "alert",
                "triage",
                "incident",
                "respond",
                "log",
                "threat",
                "k8s",
                "terraform",
                "mitre",
            ],
            ExpertId::MLTrain => &[
                "train",
                "fine-tune",
                "finetune",
                "lora",
                "qlora",
                "adapter",
                "quantize",
                "quantization",
                "checkpoint",
                "hyperparameter",
                "epoch",
                "gradient",
                "backprop",
                "neural",
                "model",
            ],
            ExpertId::MLData => &[
                "dataset",
                "data pipeline",
                "schema",
                "transform",
                "validate",
                "feature",
                "feature store",
                "lineage",
                "data source",
                "training data",
            ],
            ExpertId::MLInference => &[
                "rag",
                "retrieval",
                "inference",
                "serve",
                "predict",
                "embed",
                "vector",
                "chunk",
                "rerank",
                "pipeline",
            ],
            ExpertId::MLSafety => &[
                "ai safety",
                "pii",
                "bias",
                "fairness",
                "alignment",
                "adversarial",
                "provenance",
                "audit trail",
                "red team",
            ],
            ExpertId::MLResearch => &[
                "research",
                "evaluate",
                "benchmark",
                "explain",
                "reasoning",
                "interpretab",
                "explainab",
                "compare model",
                "eval harness",
            ],
            ExpertId::SRE => &[
                "alert",
                "deployment",
                "prometheus",
                "kubernetes",
                "k8s",
                "oncall",
                "monitor",
                "uptime",
                "sre",
                "incident",
            ],
            ExpertId::Research => &[
                "research",
                "investigate",
                "analyze",
                "synthesize",
                "literature",
                "source",
                "deep research",
            ],
        }
    }

    /// Negative keywords — if found, reduce this expert's score.
    pub fn negative_keywords(&self) -> &'static [&'static str] {
        match self {
            ExpertId::FileOps => &["train", "deploy", "scan vulnerability"],
            ExpertId::Git => &["calendar", "music", "train model"],
            ExpertId::MacOSApps => &["scan", "vulnerability", "kubernetes"],
            ExpertId::MacOSSystem => &["train", "deploy", "vulnerability"],
            ExpertId::ScreenUI => &["train", "scan", "kubernetes"],
            ExpertId::Communication => &["file", "git", "train", "scan"],
            ExpertId::WebBrowse => &["calendar", "reminder", "train model"],
            ExpertId::DevTools => &["calendar", "music", "train model"],
            ExpertId::Productivity => &["scan vulnerability", "kubernetes", "train model"],
            ExpertId::SecScan => &["calendar", "music", "train model", "rag"],
            ExpertId::SecReview => &["calendar", "music", "train model", "rag"],
            ExpertId::SecCompliance => &["calendar", "music", "train model", "rag"],
            ExpertId::SecIncident => &["calendar", "music", "train model"],
            ExpertId::MLTrain => &["calendar", "music", "kubernetes", "compliance"],
            ExpertId::MLData => &["calendar", "music", "kubernetes", "compliance"],
            ExpertId::MLInference => &["calendar", "music", "kubernetes"],
            ExpertId::MLSafety => &["calendar", "music", "kubernetes"],
            ExpertId::MLResearch => &["calendar", "music", "kubernetes"],
            ExpertId::SRE => &["calendar", "music", "train model", "rag"],
            ExpertId::Research => &["calendar", "music", "kubernetes", "deploy"],
        }
    }
}

impl ExpertId {
    /// System prompt section keywords that are irrelevant for this expert.
    ///
    /// When MoE is active, the prompt optimizer can strip lines containing these
    /// keywords from the system prompt, saving 500-1500 tokens per request.
    /// Only affects non-essential sections — core instructions are never stripped.
    pub fn system_prompt_exclusions(&self) -> &'static [&'static str] {
        match self {
            // ML experts don't need macOS/GUI/calendar instructions
            ExpertId::MLTrain
            | ExpertId::MLData
            | ExpertId::MLInference
            | ExpertId::MLSafety
            | ExpertId::MLResearch => &[
                "AppleScript",
                "macOS",
                "Calendar",
                "Reminders",
                "Notes.app",
                "GUI scripting",
                "accessibility",
                "HomeKit",
                "Photos.app",
                "Safari",
                "iMessage",
                "Siri",
                "meeting_recorder",
            ],
            // Security experts don't need ML/macOS instructions
            ExpertId::SecScan
            | ExpertId::SecReview
            | ExpertId::SecCompliance
            | ExpertId::SecIncident => &[
                "AppleScript",
                "macOS",
                "Calendar",
                "Reminders",
                "Notes.app",
                "HomeKit",
                "Photos.app",
                "Safari",
                "iMessage",
                "Siri",
                "LoRA",
                "quantiz",
                "finetun",
                "RAG",
                "embedding",
            ],
            // SRE doesn't need macOS/ML-specific instructions
            ExpertId::SRE => &[
                "AppleScript",
                "Calendar",
                "Reminders",
                "Notes.app",
                "HomeKit",
                "Photos.app",
                "Safari",
                "iMessage",
                "Siri",
                "LoRA",
                "quantiz",
                "finetun",
            ],
            // macOS experts don't need ML/SRE instructions
            ExpertId::MacOSApps
            | ExpertId::MacOSSystem
            | ExpertId::ScreenUI
            | ExpertId::Communication => &[
                "kubernetes",
                "prometheus",
                "terraform",
                "LoRA",
                "quantiz",
                "finetun",
                "training data",
                "SAST",
                "SBOM",
                "CycloneDX",
            ],
            // File/Git/Dev experts are generic — minimal exclusions
            ExpertId::FileOps | ExpertId::Git | ExpertId::DevTools => {
                &["HomeKit", "Photos.app", "Siri", "LoRA", "quantiz"]
            }
            // Web/Productivity/Research are broad — minimal exclusions
            ExpertId::WebBrowse | ExpertId::Productivity | ExpertId::Research => {
                &["HomeKit", "Photos.app"]
            }
        }
    }
}

/// Configuration for an expert agent instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertConfig {
    /// Which expert this configuration is for.
    pub id: ExpertId,
    /// Override tool names (if empty, uses default from ExpertId::tool_names()).
    #[serde(default)]
    pub tool_overrides: Vec<String>,
    /// Additional system prompt text appended after the expert's default addendum.
    #[serde(default)]
    pub extra_prompt: String,
}

impl ExpertConfig {
    /// Create a default config for the given expert.
    pub fn new(id: ExpertId) -> Self {
        Self {
            id,
            tool_overrides: Vec::new(),
            extra_prompt: String::new(),
        }
    }

    /// Get the effective tool names for this expert.
    pub fn effective_tools(&self) -> Vec<String> {
        if self.tool_overrides.is_empty() {
            self.id.tool_names()
        } else {
            let mut tools = ExpertId::shared_tools();
            tools.extend(self.tool_overrides.clone());
            tools
        }
    }
}
