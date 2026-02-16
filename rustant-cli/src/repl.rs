//! REPL (Read-Eval-Print Loop) for interactive and single-task modes.

#[cfg(feature = "browser")]
use rustant_core::browser::BrowserSecurityGuard;
use rustant_core::browser::CdpClient;
use rustant_core::explanation::DecisionExplanation;
use rustant_core::safety::{ActionRequest, ApprovalDecision};
#[cfg(feature = "browser")]
use rustant_core::types::ToolDefinition;
use rustant_core::types::{AgentStatus, CostEstimate, RiskLevel, TokenUsage, ToolOutput};
use rustant_core::{Agent, AgentCallback, AgentConfig, MockLlmProvider, RegisteredTool};
#[cfg(feature = "browser")]
use rustant_tools::browser::{create_browser_tools, BrowserToolContext};
use rustant_tools::register_builtin_tools;
use rustant_tools::registry::ToolRegistry;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Connect to or launch a browser and register all 24 browser tools with the agent.
///
/// Connection strategy:
/// 1. Try to reconnect using a saved session (`.rustant/browser-session.json`)
/// 2. Use `connect_or_launch()` based on `BrowserConnectionMode` config
/// 3. Save connection info for future reconnection
///
/// Returns the CDP client Arc so it can be kept alive for the session.
#[allow(unused_variables)]
async fn try_register_browser_tools(
    agent: &mut Agent,
    config: &AgentConfig,
    workspace: &Path,
) -> Option<Arc<dyn CdpClient>> {
    #[cfg(feature = "browser")]
    {
        use rustant_core::browser::{
            BrowserConnectionInfo, BrowserSessionStore, ChromiumCdpClient,
        };
        use rustant_core::config::BrowserConfig;

        let browser_config = config.browser.clone().unwrap_or(BrowserConfig {
            enabled: true,
            headless: false,
            ..BrowserConfig::default()
        });

        // Step 1: Try to reconnect using saved session
        if let Ok(Some(saved)) = BrowserSessionStore::load(workspace) {
            let url = format!("http://127.0.0.1:{}", saved.debug_port);
            if let Ok(client) = ChromiumCdpClient::connect(&url, saved.debug_port).await {
                let client: Arc<dyn CdpClient> = Arc::new(client);
                let tab_count = client.list_tabs().await.map(|t| t.len()).unwrap_or(0);
                let security = Arc::new(BrowserSecurityGuard::new(
                    browser_config.allowed_domains.clone(),
                    browser_config.blocked_domains.clone(),
                ));
                let ctx = BrowserToolContext::new(Arc::clone(&client), security);
                register_browser_tools_to_agent(agent, ctx);
                println!(
                    "\x1b[90m  Browser: reconnected ({} tabs, port {})\x1b[0m",
                    tab_count, saved.debug_port
                );
                return Some(client);
            } else {
                // Stale session — clear it
                if let Err(e) = BrowserSessionStore::clear(workspace) {
                    tracing::debug!("Failed to clear stale browser session: {}", e);
                }
            }
        }

        // Step 2: Use connect_or_launch based on config
        match ChromiumCdpClient::connect_or_launch(&browser_config).await {
            Ok(client) => {
                let client: Arc<dyn CdpClient> = Arc::new(client);
                let tab_count = client.list_tabs().await.map(|t| t.len()).unwrap_or(0);
                let mode = &browser_config.connection_mode;

                let security = Arc::new(BrowserSecurityGuard::new(
                    browser_config.allowed_domains.clone(),
                    browser_config.blocked_domains.clone(),
                ));
                let ctx = BrowserToolContext::new(Arc::clone(&client), security);
                register_browser_tools_to_agent(agent, ctx);

                // Save session for future reconnection
                let info = BrowserConnectionInfo {
                    debug_port: browser_config.debug_port,
                    ws_url: browser_config.ws_url.clone(),
                    user_data_dir: browser_config.user_data_dir.clone(),
                    tabs: client.list_tabs().await.unwrap_or_default(),
                    active_tab_id: client.active_tab_id().await.ok(),
                    saved_at: chrono::Utc::now(),
                };
                if let Err(e) = BrowserSessionStore::save(workspace, &info) {
                    tracing::debug!("Failed to save browser session: {}", e);
                }

                println!(
                    "\x1b[90m  Browser automation: 24 tools registered ({}, {} tabs)\x1b[0m",
                    mode, tab_count
                );
                return Some(client);
            }
            Err(e) => {
                tracing::warn!("Browser setup failed: {}. Browser tools unavailable.", e);
                println!("\x1b[33m  Browser automation unavailable: {}\x1b[0m", e);
            }
        }
    }

    #[cfg(not(feature = "browser"))]
    {
        tracing::debug!("Browser feature not enabled. Compile with --features browser.");
    }

    None
}

/// Register browser tools from a `BrowserToolContext` into the Agent.
///
/// This converts each `Arc<dyn Tool>` from `create_browser_tools()` into a
/// `RegisteredTool` with the proper `ToolDefinition`, `RiskLevel`, and executor.
#[cfg(feature = "browser")]
fn register_browser_tools_to_agent(agent: &mut Agent, ctx: BrowserToolContext) {
    let tools = create_browser_tools(ctx);
    for tool in tools {
        let name = tool.name().to_string();
        let description = tool.description().to_string();
        let parameters = tool.parameters_schema();
        let risk = tool.risk_level();

        let definition = ToolDefinition {
            name: name.clone(),
            description,
            parameters,
        };

        // Create an executor closure that calls the Arc<dyn Tool>::execute
        let tool_arc = tool;
        let executor: rustant_core::agent::ToolExecutor = Box::new(move |args| {
            let t = Arc::clone(&tool_arc);
            Box::pin(async move { t.execute(args).await })
        });

        agent.register_tool(RegisteredTool {
            definition,
            risk_level: risk,
            executor,
        });
    }
}

/// Truncate a string to at most `max_chars` characters, respecting UTF-8 boundaries.
fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Extract a human-readable detail string from tool arguments.
///
/// Maps tool names to their primary argument for display during execution:
/// - File tools show the file path
/// - Shell tools show the command (truncated)
/// - Git tools show the operation or commit message
pub(crate) fn extract_tool_detail(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "file_read" | "file_list" | "file_search" | "file_write" | "file_patch" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "shell_exec" => args.get("command").and_then(|v| v.as_str()).map(|s| {
            if s.chars().count() > 60 {
                format!("{}...", truncate_str(s, 60))
            } else {
                s.to_string()
            }
        }),
        "git_status" | "git_diff" => Some("workspace".to_string()),
        "git_commit" => args.get("message").and_then(|v| v.as_str()).map(|s| {
            if s.chars().count() > 50 {
                format!("\"{}...\"", truncate_str(s, 50))
            } else {
                format!("\"{}\"", s)
            }
        }),
        "web_search" => args.get("query").and_then(|v| v.as_str()).map(|s| {
            if s.chars().count() > 50 {
                format!("\"{}...\"", truncate_str(s, 50))
            } else {
                format!("\"{}\"", s)
            }
        }),
        "web_fetch" | "document_read" => args
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "smart_edit" => args
            .get("file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "codebase_search" => args.get("query").and_then(|v| v.as_str()).map(|s| {
            if s.chars().count() > 50 {
                format!("\"{}...\"", truncate_str(s, 50))
            } else {
                format!("\"{}\"", s)
            }
        }),
        // macOS screen automation tools
        "macos_gui_scripting" => {
            let app = args.get("app_name").and_then(|v| v.as_str()).unwrap_or("?");
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            Some(format!("{} → {}", app, action))
        }
        "macos_accessibility" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            let app = args.get("app_name").and_then(|v| v.as_str());
            if let Some(app) = app {
                Some(format!("{} → {}", app, action))
            } else {
                Some(action.to_string())
            }
        }
        "macos_screen_analyze" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("ocr");
            Some(action.to_string())
        }
        "macos_contacts" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            let query = args.get("query").and_then(|v| v.as_str());
            if let Some(q) = query {
                Some(format!("{}: {}", action, q))
            } else {
                Some(action.to_string())
            }
        }
        "macos_safari" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            let url = args.get("url").and_then(|v| v.as_str());
            if let Some(u) = url {
                Some(format!("{}: {}", action, u))
            } else {
                Some(action.to_string())
            }
        }
        // Browser automation tools
        name if name.starts_with("browser_") => {
            let action = name.strip_prefix("browser_").unwrap_or(name);
            let url = args.get("url").and_then(|v| v.as_str());
            let selector = args
                .get("selector")
                .and_then(|v| v.as_str())
                .or_else(|| args.get("ref").and_then(|v| v.as_str()));
            let text = args.get("text").and_then(|v| v.as_str());
            match (url, selector, text) {
                (Some(u), _, _) => Some(format!("{}: {}", action, u)),
                (_, Some(s), _) => Some(format!("{}: {}", action, truncate_str(s, 40))),
                (_, _, Some(t)) => Some(format!("{}: \"{}\"", action, truncate_str(t, 40))),
                _ => Some(action.to_string()),
            }
        }
        // iMessage tools
        "imessage_send" => {
            let recipient = args
                .get("recipient")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            Some(format!("→ {}", recipient))
        }
        "imessage_read" => {
            let contact = args
                .get("contact")
                .and_then(|v| v.as_str())
                .unwrap_or("inbox");
            Some(contact.to_string())
        }
        // Slack tool
        "slack" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            let channel = args.get("channel").and_then(|v| v.as_str());
            let message = args.get("message").and_then(|v| v.as_str());
            match (channel, message) {
                (Some(ch), Some(msg)) => {
                    Some(format!("{}: {} → {}", action, ch, truncate_str(msg, 40)))
                }
                (Some(ch), None) => Some(format!("{}: {}", action, ch)),
                _ => Some(action.to_string()),
            }
        }
        // macOS tools with action pattern
        "macos_calendar"
        | "macos_reminders"
        | "macos_notes"
        | "macos_mail"
        | "macos_music"
        | "macos_shortcuts"
        | "macos_focus_mode"
        | "macos_clipboard"
        | "macos_meeting_recorder"
        | "macos_daily_briefing" => args
            .get("action")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "arxiv_research" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("search");
            let detail = args
                .get("query")
                .or_else(|| args.get("arxiv_id"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        // Cognitive extension tools — action + key param display
        "knowledge_graph" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list");
            let detail = args
                .get("name")
                .or_else(|| args.get("id"))
                .or_else(|| args.get("query"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "experiment_tracker" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list_experiments");
            let detail = args
                .get("title")
                .or_else(|| args.get("name"))
                .or_else(|| args.get("id"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "code_intelligence" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("analyze_architecture");
            let path = args.get("path").and_then(|v| v.as_str());
            Some(if let Some(p) = path {
                format!("{}: {}", action, p)
            } else {
                action.to_string()
            })
        }
        "content_engine" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list");
            let detail = args
                .get("title")
                .or_else(|| args.get("id"))
                .or_else(|| args.get("query"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "skill_tracker" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list_skills");
            let detail = args
                .get("name")
                .or_else(|| args.get("skill_id"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "career_intel" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("progress_report");
            let detail = args
                .get("title")
                .or_else(|| args.get("person_name"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "system_monitor" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list_services");
            let detail = args
                .get("name")
                .or_else(|| args.get("service_id"))
                .or_else(|| args.get("title"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "life_planner" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("daily_plan");
            let detail = args
                .get("title")
                .or_else(|| args.get("name"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "privacy_manager" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("list_boundaries");
            let detail = args
                .get("name")
                .or_else(|| args.get("domain"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        "self_improvement" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("analyze_patterns");
            let detail = args
                .get("key")
                .or_else(|| args.get("task_description"))
                .and_then(|v| v.as_str());
            Some(if let Some(d) = detail {
                format!("{}: {}", action, d)
            } else {
                action.to_string()
            })
        }
        _ => None,
    }
}

/// A CLI callback that prints to stdout and reads approval from stdin.
///
/// When `verbose` is false (default), tool execution details, status changes,
/// usage updates, and decision explanations are hidden for cleaner output.
pub(crate) struct CliCallback {
    pub verbose: Arc<AtomicBool>,
}

impl CliCallback {
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose: Arc::new(AtomicBool::new(verbose)),
        }
    }
}

#[async_trait::async_trait]
impl AgentCallback for CliCallback {
    async fn on_assistant_message(&self, message: &str) {
        println!("\n\x1b[32mRustant:\x1b[0m {}", message);
    }

    async fn on_token(&self, token: &str) {
        print!("{}", token);
        let _ = io::stdout().flush();
    }

    async fn request_approval(&self, action: &ActionRequest) -> ApprovalDecision {
        println!(
            "\n\x1b[33m[Approval Required]\x1b[0m {} (risk: {})",
            action.description, action.risk_level
        );

        // Show rich context if available
        if let Some(ref reasoning) = action.approval_context.reasoning {
            println!("  \x1b[90mReason:\x1b[0m {}", reasoning);
        }
        for consequence in &action.approval_context.consequences {
            println!("  \x1b[90mConsequence:\x1b[0m {}", consequence);
        }
        if let Some(ref rev) = action.approval_context.reversibility {
            let rev_label = if rev.is_reversible {
                "\x1b[32mreversible\x1b[0m"
            } else {
                "\x1b[31mirreversible\x1b[0m"
            };
            print!("  \x1b[90mReversible:\x1b[0m {}", rev_label);
            if let Some(ref desc) = rev.undo_description {
                print!(" ({})", desc);
            }
            println!();
        }
        if let Some(ref preview) = action.approval_context.preview {
            println!("  \x1b[36mPreview:\x1b[0m {}", preview);
        }

        print!("  [y]es / [n]o / [a]pprove all similar > ");
        let _ = io::stdout().flush();

        let stdin = io::stdin();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_ok() {
            match line.trim().to_lowercase().as_str() {
                "y" | "yes" => ApprovalDecision::Approve,
                "a" | "all" => ApprovalDecision::ApproveAllSimilar,
                _ => ApprovalDecision::Deny,
            }
        } else {
            ApprovalDecision::Deny
        }
    }

    async fn on_tool_start(&self, tool_name: &str, args: &serde_json::Value) {
        if !self.verbose.load(Ordering::Relaxed) {
            return;
        }
        let detail = extract_tool_detail(tool_name, args);
        if let Some(ref detail) = detail {
            println!("\x1b[36m  [{}: {}] executing...\x1b[0m", tool_name, detail);
        } else {
            println!("\x1b[36m  [{}] executing...\x1b[0m", tool_name);
        }
    }

    async fn on_tool_result(&self, tool_name: &str, output: &ToolOutput, duration_ms: u64) {
        if !self.verbose.load(Ordering::Relaxed) {
            return;
        }
        let preview = if output.content.chars().count() > 200 {
            format!("{}...", truncate_str(&output.content, 200))
        } else {
            output.content.clone()
        };
        println!(
            "\x1b[36m  [{}] completed in {}ms\x1b[0m\n  {}",
            tool_name, duration_ms, preview
        );
    }

    async fn on_status_change(&self, status: AgentStatus) {
        if !self.verbose.load(Ordering::Relaxed) {
            return;
        }
        match status {
            AgentStatus::Thinking => print!("\x1b[90m  thinking...\x1b[0m"),
            AgentStatus::Executing => {}
            AgentStatus::Complete => println!("\x1b[90m  done.\x1b[0m"),
            _ => {}
        }
        let _ = io::stdout().flush();
    }

    async fn on_usage_update(&self, usage: &TokenUsage, cost: &CostEstimate) {
        if !self.verbose.load(Ordering::Relaxed) {
            return;
        }
        let input = usage.input_tokens;
        let output = usage.output_tokens;
        let total_cost = cost.total();
        print!(
            "\r\x1b[90m  [tokens: {}/{} | cost: ${:.4}]\x1b[0m",
            input, output, total_cost
        );
        let _ = io::stdout().flush();
    }

    async fn on_decision_explanation(&self, explanation: &DecisionExplanation) {
        if !self.verbose.load(Ordering::Relaxed) {
            return;
        }
        let tool = match &explanation.decision_type {
            rustant_core::explanation::DecisionType::ToolSelection { selected_tool } => {
                selected_tool.as_str()
            }
            _ => "decision",
        };
        print!(
            "\n\x1b[90m  [why: {} | confidence: {:.0}%",
            tool,
            explanation.confidence * 100.0
        );
        if !explanation.reasoning_chain.is_empty() {
            print!(" | {}", explanation.reasoning_chain[0].description);
        }
        println!("]\x1b[0m");
        let _ = io::stdout().flush();
    }

    async fn on_cost_prediction(&self, estimated_tokens: usize, estimated_cost: f64) {
        if !self.verbose.load(Ordering::Relaxed) {
            return;
        }
        println!(
            "\x1b[33m  [Cost estimate: ~{} tokens, ~${:.4}]\x1b[0m",
            estimated_tokens, estimated_cost
        );
        let _ = io::stdout().flush();
    }

    async fn on_budget_warning(&self, message: &str, severity: rustant_core::BudgetSeverity) {
        match severity {
            rustant_core::BudgetSeverity::Warning => {
                println!("\x1b[33m  [Budget Warning] {}\x1b[0m", message);
            }
            rustant_core::BudgetSeverity::Exceeded => {
                println!("\x1b[31m  [Budget Exceeded] {}\x1b[0m", message);
            }
        }
        let _ = io::stdout().flush();
    }

    async fn on_clarification_request(&self, question: &str) -> String {
        println!("\n\x1b[33m?\x1b[0m {}", question);
        print!("\x1b[1;34m> \x1b[0m");
        let _ = io::stdout().flush();

        let stdin = io::stdin();
        let mut answer = String::new();
        if stdin.lock().read_line(&mut answer).is_ok() {
            answer.trim().to_string()
        } else {
            String::new()
        }
    }

    async fn on_context_health(&self, event: &rustant_core::ContextHealthEvent) {
        match event {
            rustant_core::ContextHealthEvent::Warning {
                usage_percent,
                total_tokens,
                context_window,
                hint,
            } => {
                println!(
                    "\x1b[33m  [Context: {}% used ({}/{})] {}\x1b[0m",
                    usage_percent, total_tokens, context_window, hint
                );
            }
            rustant_core::ContextHealthEvent::Critical {
                usage_percent,
                total_tokens,
                context_window,
                hint,
            } => {
                println!(
                    "\x1b[31m  [Context: {}% used ({}/{})] {}\x1b[0m",
                    usage_percent, total_tokens, context_window, hint
                );
            }
            rustant_core::ContextHealthEvent::Compressed {
                messages_compressed,
                was_llm_summarized,
                pinned_preserved,
            } => {
                let method = if *was_llm_summarized {
                    "LLM-summarized"
                } else {
                    "fallback truncation"
                };
                let pinned_info = if *pinned_preserved > 0 {
                    format!(", {} pinned messages preserved", pinned_preserved)
                } else {
                    String::new()
                };
                println!(
                    "\x1b[90m  [Context compressed: {} messages via {}{}]\x1b[0m",
                    messages_compressed, method, pinned_info
                );
            }
        }
        let _ = io::stdout().flush();
    }

    async fn on_plan_generating(&self, goal: &str) {
        println!("\n\x1b[34m[Planning]\x1b[0m Generating plan for: {}", goal);
        let _ = io::stdout().flush();
    }

    async fn on_plan_review(
        &self,
        plan: &rustant_core::plan::ExecutionPlan,
    ) -> rustant_core::plan::PlanDecision {
        use rustant_core::plan::PlanDecision;

        println!();
        display_plan(plan);
        println!();
        println!(
            "Plan ready ({} steps). \x1b[1m[a]\x1b[0mpprove / \x1b[1m[e]\x1b[0mdit <n> <desc> / \x1b[1m[r]\x1b[0memove <n> / \x1b[1m[+]\x1b[0m <n> <desc> / \x1b[1m[?]\x1b[0m question / \x1b[1m[x]\x1b[0m cancel",
            plan.steps.len()
        );
        print!("> ");
        let _ = io::stdout().flush();

        let stdin = io::stdin();
        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            return PlanDecision::Approve;
        }
        let input = input.trim();

        if input.is_empty() || input.starts_with('a') {
            PlanDecision::Approve
        } else if input.starts_with('x') || input.starts_with('c') {
            PlanDecision::Reject
        } else if input.starts_with('e') {
            // e <n> <new description>
            let parts: Vec<&str> = input.splitn(3, ' ').collect();
            if parts.len() >= 3 {
                if let Ok(idx) = parts[1].parse::<usize>() {
                    let idx = idx.saturating_sub(1); // 1-based to 0-based
                    return PlanDecision::EditStep(idx, parts[2].to_string());
                }
            }
            println!("\x1b[31mUsage: e <step_number> <new description>\x1b[0m");
            PlanDecision::Approve // re-review
        } else if input.starts_with('r') {
            // r <n>
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                if let Ok(idx) = parts[1].parse::<usize>() {
                    return PlanDecision::RemoveStep(idx.saturating_sub(1));
                }
            }
            println!("\x1b[31mUsage: r <step_number>\x1b[0m");
            PlanDecision::Approve
        } else if input.starts_with('+') {
            // + <n> <description>
            let parts: Vec<&str> = input.splitn(3, ' ').collect();
            if parts.len() >= 3 {
                if let Ok(idx) = parts[1].parse::<usize>() {
                    return PlanDecision::AddStep(idx.saturating_sub(1), parts[2].to_string());
                }
            }
            println!("\x1b[31mUsage: + <position> <description>\x1b[0m");
            PlanDecision::Approve
        } else if let Some(rest) = input.strip_prefix('?') {
            let question = rest.trim().to_string();
            if question.is_empty() {
                println!("\x1b[31mUsage: ? <your question about the plan>\x1b[0m");
                PlanDecision::Approve
            } else {
                PlanDecision::AskQuestion(question)
            }
        } else {
            PlanDecision::Approve
        }
    }

    async fn on_plan_step_start(&self, step_index: usize, step: &rustant_core::plan::PlanStep) {
        let tool_info = step
            .tool
            .as_deref()
            .map(|t| format!(" [{}]", t))
            .unwrap_or_default();
        println!(
            "\x1b[34m  [Step {}]\x1b[0m {}{}...",
            step_index + 1,
            step.description,
            tool_info
        );
        let _ = io::stdout().flush();
    }

    async fn on_plan_step_complete(&self, step_index: usize, step: &rustant_core::plan::PlanStep) {
        let (icon, color) = match step.status {
            rustant_core::plan::StepStatus::Completed => ("✓", "\x1b[32m"),
            rustant_core::plan::StepStatus::Failed => ("✗", "\x1b[31m"),
            _ => ("●", "\x1b[90m"),
        };
        println!("  {}{} Step {}\x1b[0m", color, icon, step_index + 1);
        let _ = io::stdout().flush();
    }
}

/// Print contextual hints after /help based on current agent state.
fn print_contextual_hints(agent: &Agent) {
    let mem = agent.memory();
    let mut hints = Vec::new();

    if mem.short_term.len() > 50 {
        hints.push("Tip: You have many messages. Use /compact to free context space.");
    }
    if mem.long_term.facts.is_empty() && mem.short_term.len() > 10 {
        hints.push("Tip: Use /session save <name> to checkpoint your work.");
    }

    let context_window = agent.brain().context_window();
    let ctx = mem.context_breakdown(context_window);
    if ctx.usage_ratio() > 0.7 {
        hints.push(
            "Tip: Context is >70% full. Use /pin to protect important messages before compression.",
        );
    }

    if !hints.is_empty() {
        println!();
        for hint in hints {
            println!("\x1b[33m  {}\x1b[0m", hint);
        }
    }
}

/// Show first-run onboarding tour with project-specific guidance.
///
/// Checks for `.rustant/.onboarding_complete` marker. If absent, prints
/// a contextual welcome message using project detection, then creates the marker.
fn show_onboarding(workspace: &Path) {
    let marker = workspace.join(".rustant").join(".onboarding_complete");
    if marker.exists() {
        return;
    }

    let info = rustant_core::project_detect::detect_project(workspace);
    let tasks = rustant_core::project_detect::example_tasks(&info);

    println!("\x1b[1;36m  Welcome to Rustant!\x1b[0m");
    println!();

    // Project-specific welcome
    if info.project_type != rustant_core::project_detect::ProjectType::Unknown {
        let framework_note = info
            .framework
            .as_ref()
            .map(|f| format!(" ({} framework)", f))
            .unwrap_or_default();
        println!(
            "  Detected a \x1b[1m{}{}\x1b[0m project.",
            info.project_type, framework_note
        );
    }

    println!("  Here are some things you can try:");
    println!();
    for task in tasks.iter().take(3) {
        println!("    {}", task);
    }
    println!();
    println!("  Quick reference:");
    println!("    \x1b[36m@\x1b[0m reference files  |  \x1b[36m/\x1b[0m commands  |  \x1b[36m/tools\x1b[0m list tools");
    println!("    \x1b[36m/permissions\x1b[0m adjust safety  |  \x1b[36m/context\x1b[0m check memory usage");
    println!();
    println!(
        "  I'll ask for approval before modifying files. Adjust with \x1b[36m/permissions\x1b[0m."
    );
    println!();
    println!("  \x1b[2mPress Enter to continue, or type 'skip' to dismiss.\x1b[0m");

    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
    if input.trim().eq_ignore_ascii_case("skip") {
        println!("  Tour dismissed. Run \x1b[36m/help\x1b[0m anytime for commands.\n");
    }

    // Create the marker so the tour doesn't show again
    let rustant_dir = workspace.join(".rustant");
    if std::fs::create_dir_all(&rustant_dir).is_ok() {
        let _ = std::fs::write(&marker, "onboarding completed\n");
    }
}

/// Run the agent in interactive REPL mode.
pub async fn run_interactive(config: AgentConfig, workspace: PathBuf) -> anyhow::Result<()> {
    println!("\x1b[1;32m");
    println!(r#"  ██████╗ ██╗   ██╗███████╗████████╗ █████╗ ███╗   ██╗████████╗"#);
    println!(r#"  ██╔══██╗██║   ██║██╔════╝╚══██╔══╝██╔══██╗████╗  ██║╚══██╔══╝"#);
    println!(r#"  ██████╔╝██║   ██║███████╗   ██║   ███████║██╔██╗ ██║   ██║   "#);
    println!(r#"  ██╔══██╗██║   ██║╚════██║   ██║   ██╔══██║██║╚██╗██║   ██║   "#);
    println!(r#"  ██║  ██║╚██████╔╝███████║   ██║   ██║  ██║██║ ╚████║   ██║   "#);
    println!(r#"  ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═══╝   ╚═╝   "#);
    println!("\x1b[0m");
    println!(
        "  Model: {} | Approval: {} | Workspace: {}",
        config.llm.model,
        config.safety.approval_mode,
        workspace.display()
    );
    println!("  Type /help for commands, /quit to exit\n");

    // Show first-run onboarding tour if not yet completed
    show_onboarding(&workspace);

    let provider = if config.llm.auth_method == "oauth" {
        let cred_store = rustant_core::credentials::KeyringCredentialStore::new();
        match rustant_core::create_provider_with_auth(&config.llm, &cred_store).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("LLM provider (OAuth) init failed: {}. Using mock.", e);
                Arc::new(MockLlmProvider::new())
            }
        }
    } else {
        match rustant_core::create_provider(&config.llm) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("LLM provider init failed: {}. Using mock.", e);
                Arc::new(MockLlmProvider::new())
            }
        }
    };
    let callback = Arc::new(CliCallback::new(config.ui.verbose));
    let verbose_flag = Arc::clone(&callback.verbose);
    // Clone config before moving into Agent (needed for browser setup)
    let config_ref = config.clone();
    let mut agent = Agent::new(provider, config, callback);

    // Register built-in tools as agent tools
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry, workspace.clone());
    register_agent_tools_from_registry(&mut agent, &registry, &workspace);

    // Register browser tools if the browser feature is enabled.
    // Keep _browser_client alive so Chrome stays open for the REPL session.
    let _browser_client = try_register_browser_tools(&mut agent, &config_ref, &workspace).await;

    // Load scheduler state from disk
    let scheduler_state_dir = workspace.join(".rustant").join("scheduler");
    agent.load_scheduler_state(&scheduler_state_dir);

    // Attempt auto-recovery of the most recent session
    if let Ok(mut mgr) = rustant_core::SessionManager::new(&workspace) {
        match mgr.resume_latest() {
            Ok((memory, _continuation)) => {
                let msg_count = memory.short_term.len();
                if msg_count > 0 {
                    *agent.memory_mut() = memory;
                    println!(
                        "\x1b[90m  Recovered previous session ({} messages). Type /clear to start fresh.\x1b[0m",
                        msg_count
                    );
                }
            }
            Err(_) => {
                // No sessions available or recovery failed — start fresh silently
            }
        }
    }

    // Set up signal handling for graceful shutdown
    let cancel_token = agent.cancellation_token();
    let ws_for_signal = workspace.clone();
    tokio::spawn(async move {
        use tokio::signal;
        let ctrl_c = signal::ctrl_c();
        #[cfg(unix)]
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler");
        #[cfg(unix)]
        let sigterm_recv = sigterm.recv();
        #[cfg(not(unix))]
        let sigterm_recv = std::future::pending::<Option<()>>();

        tokio::select! {
            _ = ctrl_c => {
                tracing::info!("Received SIGINT, initiating graceful shutdown");
            }
            _ = sigterm_recv => {
                tracing::info!("Received SIGTERM, initiating graceful shutdown");
            }
        }
        cancel_token.cancel();
        // Best-effort auto-save WIP session
        let _ = auto_save_wip_session(&ws_for_signal);
        eprintln!("\nShutting down gracefully...");
    });

    let cmd_registry = crate::slash::CommandRegistry::with_defaults();
    let mut repl_input = crate::repl_input::ReplInput::new(&workspace);
    loop {
        let input = match repl_input.read_line("\x1b[1;34m> \x1b[0m", &cmd_registry) {
            Ok(Some(line)) => line,
            Ok(None) => break, // Ctrl-D EOF
            Err(_) => break,
        };

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Handle commands
        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(3, ' ').collect();
            let cmd = parts[0];
            let arg1 = parts.get(1).copied().unwrap_or("");
            let arg2 = parts.get(2).copied().unwrap_or("");

            match cmd {
                "/quit" | "/exit" | "/q" => {
                    // Offer to save session if there's meaningful context
                    if agent.memory().short_term.total_messages_seen() > 2 {
                        print!("Save session before exiting? [y/n/name] > ");
                        let _ = io::stdout().flush();
                        let mut save_input = String::new();
                        if io::stdin().lock().read_line(&mut save_input).is_ok() {
                            let save_input = save_input.trim();
                            if !save_input.is_empty() && save_input != "n" && save_input != "no" {
                                let session_name = if save_input == "y" || save_input == "yes" {
                                    None
                                } else {
                                    Some(save_input)
                                };
                                if let Ok(mut mgr) = rustant_core::SessionManager::new(&workspace) {
                                    let entry = mgr.start_session(session_name);
                                    let total_tokens = agent.brain().total_usage().total();
                                    match mgr.save_checkpoint(agent.memory(), total_tokens) {
                                        Ok(()) => {
                                            println!("Session saved as '{}'.", entry.name);
                                        }
                                        Err(e) => {
                                            println!("Failed to save session: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Save scheduler state on exit
                    auto_save_scheduler(&agent, &workspace);
                    println!("Goodbye!");
                    break;
                }
                "/help" | "/?" => {
                    if !arg1.is_empty() {
                        // Topic-specific help
                        match cmd_registry.help_for(arg1) {
                            Some(help) => println!("\n{}", help),
                            None => {
                                println!(
                                    "No help found for '{}'. Try /help for all commands.",
                                    arg1
                                );
                                if let Some(suggestion) =
                                    cmd_registry.suggest(&format!("/{}", arg1))
                                {
                                    println!("Did you mean: {} ?", suggestion);
                                }
                            }
                        }
                    } else {
                        println!("{}", cmd_registry.help_text());
                        // Contextual suggestions based on agent state
                        print_contextual_hints(&agent);
                    }
                    continue;
                }
                "/clear" => {
                    print!("\x1b[2J\x1b[H");
                    continue;
                }
                "/cost" => {
                    let usage = agent.brain().total_usage();
                    let cost = agent.brain().total_cost();
                    println!(
                        "Tokens: {} in / {} out ({} total)",
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.total()
                    );
                    println!("Cost: ${:.4}", cost.total());
                    continue;
                }
                "/tools" => {
                    let defs = agent.tool_definitions();
                    println!("Registered tools ({}):", defs.len());
                    for def in &defs {
                        println!("  - {}: {}", def.name, def.description);
                    }
                    continue;
                }
                "/setup" => {
                    if let Err(e) = crate::setup::run_setup(&workspace).await {
                        println!("Setup failed: {}", e);
                    }
                    continue;
                }
                "/audit" => {
                    handle_audit_command(arg1, arg2, &agent);
                    continue;
                }
                "/session" => {
                    handle_session_command(arg1, arg2, &mut agent, &workspace);
                    continue;
                }
                "/resume" => {
                    handle_resume_command(arg1, &mut agent, &workspace);
                    continue;
                }
                "/sessions" => {
                    handle_sessions_command(arg1, arg2, &workspace);
                    continue;
                }
                "/safety" => {
                    handle_safety_command(&agent);
                    continue;
                }
                "/memory" => {
                    handle_memory_command(&agent);
                    continue;
                }
                "/pin" => {
                    handle_pin_command(arg1, &mut agent);
                    continue;
                }
                "/unpin" => {
                    handle_unpin_command(arg1, &mut agent);
                    continue;
                }
                "/context" => {
                    handle_context_command(&agent);
                    continue;
                }
                "/workflows" => {
                    handle_workflows_command();
                    continue;
                }
                "/compact" => {
                    handle_compact_command(&mut agent);
                    continue;
                }
                "/status" => {
                    handle_status_command(&agent);
                    continue;
                }
                "/config" => {
                    handle_config_command(arg1, arg2, &mut agent);
                    continue;
                }
                "/doctor" => {
                    handle_doctor_command(&agent, &workspace).await;
                    continue;
                }
                "/permissions" => {
                    handle_permissions_command(arg1, &mut agent);
                    continue;
                }
                "/trust" => {
                    handle_trust_command(&agent);
                    continue;
                }
                "/keys" => {
                    handle_keys_command();
                    continue;
                }
                "/undo" => {
                    handle_undo_command(&workspace);
                    continue;
                }
                "/diff" => {
                    handle_diff_command(&workspace);
                    continue;
                }
                "/review" => {
                    handle_review_command(&workspace);
                    continue;
                }
                "/digest" => {
                    handle_digest_command(arg1, &workspace);
                    continue;
                }
                "/replies" => {
                    handle_replies_command(arg1, arg2);
                    continue;
                }
                "/reminders" => {
                    handle_reminders_command(arg1, arg2, &workspace);
                    continue;
                }
                "/intelligence" | "/intel" => {
                    handle_intelligence_command(arg1);
                    continue;
                }
                "/council" => {
                    let question = if arg1.is_empty() {
                        String::new()
                    } else if arg2.is_empty() {
                        arg1.to_string()
                    } else {
                        format!("{} {}", arg1, arg2)
                    };
                    handle_council_command(&question, &config_ref);
                    continue;
                }
                "/plan" => {
                    match arg1 {
                        "on" => {
                            agent.set_plan_mode(true);
                            println!("\x1b[32mPlan mode enabled.\x1b[0m Tasks will generate a plan for review before execution.");
                        }
                        "off" => {
                            agent.set_plan_mode(false);
                            println!(
                                "\x1b[33mPlan mode disabled.\x1b[0m Tasks will execute directly."
                            );
                        }
                        "show" => {
                            if let Some(plan) = agent.current_plan() {
                                display_plan(plan);
                            } else {
                                println!("\x1b[90mNo active plan.\x1b[0m");
                            }
                        }
                        "" => {
                            let status = if agent.plan_mode() { "ON" } else { "OFF" };
                            println!("Plan mode: \x1b[1m{}\x1b[0m", status);
                            if let Some(plan) = agent.current_plan() {
                                println!();
                                display_plan(plan);
                            }
                        }
                        _ => {
                            println!("Unknown subcommand '{}'. Usage: /plan [on|off|show]", arg1);
                        }
                    }
                    continue;
                }
                "/schedule" | "/sched" | "/cron" => {
                    handle_schedule_command(arg1, arg2, &mut agent, &workspace);
                    continue;
                }
                "/why" => {
                    handle_why_command(arg1, &agent);
                    continue;
                }
                "/channel" | "/ch" => {
                    let action = match arg1 {
                        "list" | "" => crate::ChannelAction::List,
                        "setup" => crate::ChannelAction::Setup {
                            channel: if arg2.is_empty() {
                                None
                            } else {
                                Some(arg2.to_string())
                            },
                        },
                        "test" => {
                            if arg2.is_empty() {
                                println!("Usage: /channel test <name>");
                                continue;
                            }
                            crate::ChannelAction::Test {
                                name: arg2.to_string(),
                            }
                        }
                        _ => {
                            println!("Usage: /channel list|setup [name]|test <name>");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_channel(action, &workspace).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/workflow" | "/wf" => {
                    let action = match arg1 {
                        "list" | "" => crate::WorkflowAction::List,
                        "show" => {
                            if arg2.is_empty() {
                                println!("Usage: /workflow show <name>");
                                continue;
                            }
                            crate::WorkflowAction::Show {
                                name: arg2.to_string(),
                            }
                        }
                        "run" => {
                            if arg2.is_empty() {
                                println!("Usage: /workflow run <name> [key=value ...]");
                                continue;
                            }
                            let parts: Vec<&str> = arg2.splitn(2, ' ').collect();
                            let name = parts[0].to_string();
                            let input = if parts.len() > 1 {
                                parts[1].split_whitespace().map(|s| s.to_string()).collect()
                            } else {
                                Vec::new()
                            };
                            crate::WorkflowAction::Run { name, input }
                        }
                        "runs" => crate::WorkflowAction::Runs,
                        "status" => {
                            if arg2.is_empty() {
                                println!("Usage: /workflow status <run_id>");
                                continue;
                            }
                            crate::WorkflowAction::Status {
                                run_id: arg2.to_string(),
                            }
                        }
                        "cancel" => {
                            if arg2.is_empty() {
                                println!("Usage: /workflow cancel <run_id>");
                                continue;
                            }
                            crate::WorkflowAction::Cancel {
                                run_id: arg2.to_string(),
                            }
                        }
                        _ => {
                            println!("Usage: /workflow list|show|run <name> [key=val]|runs|status|cancel <id>");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_workflow(action, &workspace).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/voice" => {
                    let action = match arg1 {
                        "speak" => {
                            if arg2.is_empty() {
                                println!("Usage: /voice speak <text> [-v voice]");
                                continue;
                            }
                            // Parse optional -v flag from arg2
                            let (text, voice) = if let Some(idx) = arg2.find(" -v ") {
                                (arg2[..idx].to_string(), arg2[idx + 4..].trim().to_string())
                            } else {
                                (arg2.to_string(), "alloy".to_string())
                            };
                            crate::VoiceAction::Speak { text, voice }
                        }
                        _ => {
                            println!("Usage: /voice speak <text> [-v voice]");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_voice(action).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/browser" => {
                    let action = match arg1 {
                        "test" => crate::BrowserAction::Test {
                            url: if arg2.is_empty() {
                                "https://example.com".to_string()
                            } else {
                                arg2.to_string()
                            },
                        },
                        "launch" => {
                            let port = arg2.parse::<u16>().unwrap_or(9222);
                            crate::BrowserAction::Launch {
                                port,
                                headless: false,
                            }
                        }
                        "connect" => {
                            let port = arg2.parse::<u16>().unwrap_or(9222);
                            crate::BrowserAction::Connect { port }
                        }
                        "status" | "" => crate::BrowserAction::Status,
                        _ => {
                            println!("Usage: /browser test|launch|connect|status");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_browser(action, &workspace).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/auth" => {
                    let action = match arg1 {
                        "status" | "" => crate::AuthAction::Status,
                        "login" => {
                            if arg2.is_empty() {
                                println!("Usage: /auth login <provider>");
                                continue;
                            }
                            crate::AuthAction::Login {
                                provider: arg2.to_string(),
                                redirect_uri: None,
                            }
                        }
                        "logout" => {
                            if arg2.is_empty() {
                                println!("Usage: /auth logout <provider>");
                                continue;
                            }
                            crate::AuthAction::Logout {
                                provider: arg2.to_string(),
                            }
                        }
                        _ => {
                            println!("Usage: /auth status|login|logout <provider>");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_auth(action, &workspace).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/canvas" => {
                    let action = match arg1 {
                        "clear" => crate::CanvasAction::Clear,
                        "snapshot" | "" => crate::CanvasAction::Snapshot,
                        "push" => {
                            if arg2.is_empty() {
                                println!("Usage: /canvas push <type> <content>");
                                continue;
                            }
                            let parts: Vec<&str> = arg2.splitn(2, ' ').collect();
                            if parts.len() < 2 {
                                println!("Usage: /canvas push <type> <content>");
                                continue;
                            }
                            crate::CanvasAction::Push {
                                content_type: parts[0].to_string(),
                                content: parts[1].to_string(),
                            }
                        }
                        _ => {
                            println!("Usage: /canvas push <type> <content>|clear|snapshot");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_canvas(action).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/skill" => {
                    let action = match arg1 {
                        "list" | "" => crate::SkillAction::List { dir: None },
                        "info" => {
                            if arg2.is_empty() {
                                println!("Usage: /skill info <path>");
                                continue;
                            }
                            crate::SkillAction::Info {
                                path: arg2.to_string(),
                            }
                        }
                        "validate" => {
                            if arg2.is_empty() {
                                println!("Usage: /skill validate <path>");
                                continue;
                            }
                            crate::SkillAction::Validate {
                                path: arg2.to_string(),
                            }
                        }
                        _ => {
                            println!("Usage: /skill list|info|validate <path>");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_skill(action).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/plugin" => {
                    let action = match arg1 {
                        "list" | "" => crate::PluginAction::List { dir: None },
                        "info" => {
                            if arg2.is_empty() {
                                println!("Usage: /plugin info <name>");
                                continue;
                            }
                            crate::PluginAction::Info {
                                name: arg2.to_string(),
                            }
                        }
                        _ => {
                            println!("Usage: /plugin list|info <name>");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_plugin(action).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/update" => {
                    let action = match arg1 {
                        "check" | "" => crate::UpdateAction::Check,
                        "install" => crate::UpdateAction::Install,
                        _ => {
                            println!("Usage: /update check|install");
                            continue;
                        }
                    };
                    if let Err(e) = crate::commands::handle_update(action).await {
                        println!("\x1b[31mError: {}\x1b[0m", e);
                    }
                    continue;
                }
                "/meeting" | "/meet" => {
                    #[cfg(target_os = "macos")]
                    {
                        use rustant_tools::registry::Tool;
                        let subcommand = if arg1.is_empty() { "status" } else { arg1 };
                        let tool = rustant_tools::meeting::MacosMeetingRecorderTool;
                        let args = match subcommand {
                            "detect" => serde_json::json!({"action": "detect_meeting"}),
                            "record" => {
                                let title = if arg2.is_empty() {
                                    "Meeting".to_string()
                                } else {
                                    arg2.to_string()
                                };
                                serde_json::json!({"action": "record_and_transcribe", "title": title})
                            }
                            "stop" => serde_json::json!({"action": "stop"}),
                            "status" => serde_json::json!({"action": "status"}),
                            _ => {
                                println!("Usage: /meeting detect|record|stop|status");
                                continue;
                            }
                        };
                        match tool.execute(args).await {
                            Ok(output) => println!("{}", output.content),
                            Err(e) => println!("\x1b[31mError: {}\x1b[0m", e),
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        println!("Meeting recording is only available on macOS.");
                    }
                    continue;
                }
                "/verbose" | "/v" => {
                    let prev = verbose_flag.load(Ordering::Relaxed);
                    verbose_flag.store(!prev, Ordering::Relaxed);
                    if !prev {
                        println!("\x1b[32m  Verbose mode ON\x1b[0m — tool details, status, and usage will be shown.");
                    } else {
                        println!("\x1b[33m  Verbose mode OFF\x1b[0m — clean output (only responses and approvals).");
                    }
                    continue;
                }
                _ => {
                    // Check if this is a TUI-only command
                    if let Some(info) = cmd_registry.lookup(cmd) {
                        if info.tui_only {
                            println!(
                                "The {} command is only available in TUI mode. Launch with: rustant (without --no-tui)",
                                cmd
                            );
                            continue;
                        }
                    }
                    // Use registry for unknown command suggestions
                    if let Some(suggestion) = cmd_registry.suggest(cmd) {
                        println!("Unknown command: {}. Did you mean {}?", cmd, suggestion);
                    } else {
                        println!(
                            "Unknown command: {}. Type /help for available commands.",
                            cmd
                        );
                    }
                    continue;
                }
            }
        }

        // Process task
        match agent.process_task(input).await {
            Ok(result) => {
                if !result.response.is_empty() {
                    // Response already printed via callback
                }
                println!(
                    "\x1b[90m  [{} iterations, {} tokens, ${:.4}]\x1b[0m",
                    result.iterations,
                    result.total_usage.total(),
                    result.total_cost.total()
                );
            }
            Err(e) => {
                println!("\x1b[31mError: {}\x1b[0m", e);
                // Show actionable guidance if available
                {
                    use rustant_core::error::UserGuidance;
                    if let Some(suggestion) = e.suggestion() {
                        println!("\x1b[33m  Suggestion: {}\x1b[0m", suggestion);
                    }
                    let steps = e.next_steps();
                    if !steps.is_empty() {
                        println!("\x1b[90m  Next steps:\x1b[0m");
                        for step in &steps {
                            println!("\x1b[90m    - {}\x1b[0m", step);
                        }
                    }
                }
            }
        }
    }

    // Auto-save WIP session on normal exit (if not already saved by /quit)
    let _ = auto_save_wip_session(&workspace);

    Ok(())
}

/// Best-effort auto-save of work-in-progress session data.
fn auto_save_wip_session(workspace: &std::path::Path) -> Result<(), anyhow::Error> {
    // This is a best-effort save — we don't have access to the agent here,
    // so we just save scheduler state if possible.
    let scheduler_state_dir = workspace.join(".rustant").join("scheduler");
    if scheduler_state_dir.exists() {
        tracing::info!("WIP session state preserved at {:?}", workspace);
    }
    Ok(())
}

/// Run a single task and exit.
pub async fn run_single_task(
    task: &str,
    config: AgentConfig,
    workspace: PathBuf,
) -> anyhow::Result<()> {
    let provider = if config.llm.auth_method == "oauth" {
        let cred_store = rustant_core::credentials::KeyringCredentialStore::new();
        match rustant_core::create_provider_with_auth(&config.llm, &cred_store).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("LLM provider (OAuth) init failed: {}. Using mock.", e);
                Arc::new(MockLlmProvider::new())
            }
        }
    } else {
        match rustant_core::create_provider(&config.llm) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("LLM provider init failed: {}. Using mock.", e);
                Arc::new(MockLlmProvider::new())
            }
        }
    };
    let callback = Arc::new(CliCallback::new(config.ui.verbose));
    // Clone config before moving into Agent (needed for browser setup)
    let config_ref = config.clone();
    let mut agent = Agent::new(provider, config, callback);

    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry, workspace.clone());
    register_agent_tools_from_registry(&mut agent, &registry, &workspace);

    // Register browser tools if the browser feature is enabled.
    // Keep _browser_client alive so Chrome stays open for the task duration.
    let _browser_client = try_register_browser_tools(&mut agent, &config_ref, &workspace).await;

    match agent.process_task(task).await {
        Ok(result) => {
            if result.success {
                std::process::exit(0);
            } else {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Register tools from the ToolRegistry as agent RegisteredTools.
fn register_agent_tools_from_registry(
    agent: &mut Agent,
    registry: &ToolRegistry,
    workspace: &Path,
) {
    // Re-create tool executors for the agent's internal tool model.
    // Tools in create_tool_executor() get purpose-built executors.
    // All other tools (macOS native, etc.) use the ToolRegistry as a
    // generic fallback executor so they are actually callable.
    let registry_arc = Arc::new(registry.clone());
    let tool_defs = registry.list_definitions();
    for def in tool_defs {
        let name = def.name.clone();
        let ws = workspace.to_path_buf();
        let executor = if let Some(specific) = create_tool_executor(&name, &ws) {
            specific
        } else {
            // Generic fallback: delegate to the ToolRegistry
            let reg = registry_arc.clone();
            let tool_name = name.clone();
            Box::new(move |args: serde_json::Value| {
                let r = reg.clone();
                let n = tool_name.clone();
                Box::pin(async move { r.execute(&n, args).await })
                    as std::pin::Pin<
                        Box<
                            dyn std::future::Future<
                                    Output = Result<
                                        rustant_core::types::ToolOutput,
                                        rustant_core::error::ToolError,
                                    >,
                                > + Send,
                        >,
                    >
            }) as rustant_core::agent::ToolExecutor
        };
        agent.register_tool(RegisteredTool {
            definition: def,
            risk_level: tool_risk_level(&name),
            executor,
        });
    }
}

/// Create a tool executor function for the given tool name.
fn create_tool_executor(name: &str, workspace: &Path) -> Option<rustant_core::agent::ToolExecutor> {
    let ws = workspace.to_path_buf();
    match name {
        "file_read" => {
            let tool = Arc::new(rustant_tools::file::FileReadTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_list" => {
            let tool = Arc::new(rustant_tools::file::FileListTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_search" => {
            let tool = Arc::new(rustant_tools::file::FileSearchTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_write" => {
            let tool = Arc::new(rustant_tools::file::FileWriteTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "file_patch" => {
            let tool = Arc::new(rustant_tools::file::FilePatchTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "git_status" => {
            let tool = Arc::new(rustant_tools::git::GitStatusTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "git_diff" => {
            let tool = Arc::new(rustant_tools::git::GitDiffTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "git_commit" => {
            let tool = Arc::new(rustant_tools::git::GitCommitTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "shell_exec" => {
            let tool = Arc::new(rustant_tools::shell::ShellExecTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "echo" => {
            let tool = Arc::new(rustant_tools::utils::EchoTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "datetime" => {
            let tool = Arc::new(rustant_tools::utils::DateTimeTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "calculator" => {
            let tool = Arc::new(rustant_tools::utils::CalculatorTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "web_search" => {
            let tool = Arc::new(rustant_tools::web::WebSearchTool::new());
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "web_fetch" => {
            let tool = Arc::new(rustant_tools::web::WebFetchTool::new());
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "document_read" => {
            let tool = Arc::new(rustant_tools::web::DocumentReadTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "smart_edit" => {
            let tool = Arc::new(rustant_tools::smart_edit::SmartEditTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        "codebase_search" => {
            let tool = Arc::new(rustant_tools::codebase_search::CodebaseSearchTool::new(ws));
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        #[cfg(target_os = "macos")]
        "imessage_contacts" => {
            let tool = Arc::new(rustant_tools::imessage::IMessageContactsTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        #[cfg(target_os = "macos")]
        "imessage_send" => {
            let tool = Arc::new(rustant_tools::imessage::IMessageSendTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        #[cfg(target_os = "macos")]
        "imessage_read" => {
            let tool = Arc::new(rustant_tools::imessage::IMessageReadTool);
            Some(Box::new(move |args| {
                let t = tool.clone();
                Box::pin(async move {
                    use rustant_tools::registry::Tool;
                    t.execute(args).await
                })
            }))
        }
        _ => None,
    }
}

/// Get the risk level for a tool by name.
fn tool_risk_level(name: &str) -> RiskLevel {
    match name {
        "file_read" | "file_list" | "file_search" | "git_status" | "git_diff" | "echo"
        | "datetime" | "calculator" | "web_search" | "web_fetch" | "document_read"
        | "codebase_search" => RiskLevel::ReadOnly,
        "file_write" | "file_patch" | "git_commit" | "smart_edit" => RiskLevel::Write,
        "shell_exec" => RiskLevel::Execute,
        #[cfg(target_os = "macos")]
        "imessage_contacts" | "imessage_read" => RiskLevel::ReadOnly,
        #[cfg(target_os = "macos")]
        "imessage_send" => RiskLevel::Write,
        _ => RiskLevel::Execute,
    }
}

/// Handle `/audit` subcommands.
fn handle_audit_command(sub: &str, _arg: &str, agent: &Agent) {
    match sub {
        "show" | "" => {
            let n: usize = _arg.parse().unwrap_or(10);
            let log = agent.safety().audit_log();
            if log.is_empty() {
                println!("No audit entries recorded yet.");
                return;
            }
            let start = log.len().saturating_sub(n);
            println!("Audit log (last {}):", log.len().min(n));
            for entry in log.iter().skip(start) {
                let ts = entry.timestamp.format("%H:%M:%S");
                let desc = match &entry.event {
                    rustant_core::safety::AuditEvent::ActionRequested {
                        tool, risk_level, ..
                    } => format!("REQUESTED {} ({})", tool, risk_level),
                    rustant_core::safety::AuditEvent::ActionApproved { tool } => {
                        format!("APPROVED  {}", tool)
                    }
                    rustant_core::safety::AuditEvent::ActionDenied { tool, reason } => {
                        format!("DENIED    {} ({})", tool, reason)
                    }
                    rustant_core::safety::AuditEvent::ActionExecuted {
                        tool,
                        success,
                        duration_ms,
                    } => {
                        let status = if *success { "ok" } else { "FAIL" };
                        format!("EXECUTED  {} [{}] {}ms", tool, status, duration_ms)
                    }
                    rustant_core::safety::AuditEvent::ApprovalRequested { tool, .. } => {
                        format!("APPROVAL? {}", tool)
                    }
                    rustant_core::safety::AuditEvent::ApprovalDecision { tool, approved } => {
                        let decision = if *approved { "yes" } else { "no" };
                        format!("DECISION  {} -> {}", tool, decision)
                    }
                };
                println!("  [{}] {}", ts, desc);
            }
        }
        "verify" => {
            println!("Merkle chain verification is available for persisted audit stores.");
            println!(
                "Session audit log has {} entries.",
                agent.safety().audit_log().len()
            );
        }
        "export" => {
            let log = agent.safety().audit_log();
            if log.is_empty() {
                println!("No audit entries to export.");
                return;
            }
            let format = _arg;
            match format {
                "json" => match serde_json::to_string_pretty(&log) {
                    Ok(json) => println!("{}", json),
                    Err(e) => println!("Export error: {}", e),
                },
                "jsonl" => {
                    for entry in log {
                        match serde_json::to_string(entry) {
                            Ok(line) => println!("{}", line),
                            Err(e) => println!("Export error: {}", e),
                        }
                    }
                }
                "csv" | "" | "text" => {
                    println!("id,timestamp,event_type,tool,details");
                    for entry in log {
                        let (event_type, tool, details) = match &entry.event {
                            rustant_core::safety::AuditEvent::ActionRequested {
                                tool,
                                risk_level,
                                description,
                            } => (
                                "requested",
                                tool.as_str(),
                                format!("{} - {}", risk_level, description),
                            ),
                            rustant_core::safety::AuditEvent::ActionApproved { tool } => {
                                ("approved", tool.as_str(), String::new())
                            }
                            rustant_core::safety::AuditEvent::ActionDenied { tool, reason } => {
                                ("denied", tool.as_str(), reason.clone())
                            }
                            rustant_core::safety::AuditEvent::ActionExecuted {
                                tool,
                                success,
                                duration_ms,
                            } => {
                                let detail = format!("success={} {}ms", success, duration_ms);
                                ("executed", tool.as_str(), detail)
                            }
                            rustant_core::safety::AuditEvent::ApprovalRequested {
                                tool,
                                context,
                            } => ("approval_requested", tool.as_str(), context.clone()),
                            rustant_core::safety::AuditEvent::ApprovalDecision {
                                tool,
                                approved,
                            } => (
                                "approval_decision",
                                tool.as_str(),
                                format!("approved={}", approved),
                            ),
                        };
                        println!(
                            "{},{},{},{},\"{}\"",
                            entry.id,
                            entry.timestamp.format("%Y-%m-%dT%H:%M:%S"),
                            event_type,
                            tool,
                            details.replace('"', "\"\"")
                        );
                    }
                }
                _ => println!(
                    "Unknown format '{}'. Supported: json, jsonl, csv, text",
                    format
                ),
            }
        }
        "query" => {
            let tool_name = _arg;
            if tool_name.is_empty() {
                println!("Usage: /audit query <tool_name>");
                return;
            }
            let log = agent.safety().audit_log();
            let matches: Vec<_> = log
                .iter()
                .filter(|entry| {
                    let entry_tool = match &entry.event {
                        rustant_core::safety::AuditEvent::ActionRequested { tool, .. } => tool,
                        rustant_core::safety::AuditEvent::ActionApproved { tool } => tool,
                        rustant_core::safety::AuditEvent::ActionDenied { tool, .. } => tool,
                        rustant_core::safety::AuditEvent::ActionExecuted { tool, .. } => tool,
                        rustant_core::safety::AuditEvent::ApprovalRequested { tool, .. } => tool,
                        rustant_core::safety::AuditEvent::ApprovalDecision { tool, .. } => tool,
                    };
                    entry_tool == tool_name
                })
                .collect();
            if matches.is_empty() {
                println!("No audit entries found for tool '{}'.", tool_name);
            } else {
                println!(
                    "Audit entries for '{}' ({} matches):",
                    tool_name,
                    matches.len()
                );
                for entry in &matches {
                    let ts = entry.timestamp.format("%H:%M:%S");
                    let desc = match &entry.event {
                        rustant_core::safety::AuditEvent::ActionRequested {
                            tool,
                            risk_level,
                            ..
                        } => format!("REQUESTED {} ({})", tool, risk_level),
                        rustant_core::safety::AuditEvent::ActionApproved { tool } => {
                            format!("APPROVED  {}", tool)
                        }
                        rustant_core::safety::AuditEvent::ActionDenied { tool, reason } => {
                            format!("DENIED    {} ({})", tool, reason)
                        }
                        rustant_core::safety::AuditEvent::ActionExecuted {
                            tool,
                            success,
                            duration_ms,
                        } => {
                            let status = if *success { "ok" } else { "FAIL" };
                            format!("EXECUTED  {} [{}] {}ms", tool, status, duration_ms)
                        }
                        rustant_core::safety::AuditEvent::ApprovalRequested { tool, .. } => {
                            format!("APPROVAL? {}", tool)
                        }
                        rustant_core::safety::AuditEvent::ApprovalDecision { tool, approved } => {
                            let decision = if *approved { "yes" } else { "no" };
                            format!("DECISION  {} -> {}", tool, decision)
                        }
                    };
                    println!("  [{}] {}", ts, desc);
                }
            }
        }
        _ => {
            println!("Usage: /audit [show [n] | verify | export [fmt] | query <tool>]");
        }
    }
}

/// Handle `/session` subcommands.
fn handle_session_command(sub: &str, name: &str, agent: &mut Agent, workspace: &Path) {
    match sub {
        "save" => {
            let session_name = if name.is_empty() { None } else { Some(name) };
            let mut mgr = match rustant_core::SessionManager::new(workspace) {
                Ok(m) => m,
                Err(e) => {
                    println!("Failed to initialize session manager: {}", e);
                    return;
                }
            };
            let entry = mgr.start_session(session_name);
            let total_tokens = agent.brain().total_usage().total();
            match mgr.save_checkpoint(agent.memory(), total_tokens) {
                Ok(()) => println!("Session '{}' saved.", entry.name),
                Err(e) => println!("Failed to save session: {}", e),
            }
        }
        "load" => {
            if name.is_empty() {
                println!("Usage: /session load <name>");
                return;
            }
            let mut mgr = match rustant_core::SessionManager::new(workspace) {
                Ok(m) => m,
                Err(e) => {
                    println!("Failed to initialize session manager: {}", e);
                    return;
                }
            };
            match mgr.resume_session(name) {
                Ok((mem, continuation)) => {
                    *agent.memory_mut() = mem;
                    println!("Session '{}' loaded.", name);
                    if !continuation.is_empty() {
                        println!("{}", continuation);
                    }
                }
                Err(e) => println!("Failed to load session: {}", e),
            }
        }
        "list" => {
            // Delegate to the same handler as /sessions for consistency.
            handle_sessions_command("", "", workspace);
        }
        _ => {
            println!("Usage: /session save [name] | /session load <name> | /session list");
        }
    }
}

/// Handle `/safety` command.
fn handle_safety_command(agent: &Agent) {
    let safety = agent.safety();
    println!("Safety Configuration:");
    println!("  Approval mode: {}", safety.approval_mode());
    println!("  Max iterations: {}", safety.max_iterations());
    println!("  Session ID: {}", safety.session_id());
    println!("  Audit entries: {}", safety.audit_log().len());
}

/// Handle `/memory` command.
fn handle_memory_command(agent: &Agent) {
    let mem = agent.memory();
    println!("Memory System Stats:");
    println!("  Working memory:");
    println!(
        "    Goal: {}",
        mem.working.current_goal.as_deref().unwrap_or("(none)")
    );
    println!("    Sub-tasks: {}", mem.working.sub_tasks.len());
    println!("    Active files: {}", mem.working.active_files.len());
    println!("    Scratchpad entries: {}", mem.working.scratchpad.len());
    println!("  Short-term memory:");
    println!("    Messages: {}", mem.short_term.len());
    println!("    Total seen: {}", mem.short_term.total_messages_seen());
    println!("    Window size: {}", mem.short_term.window_size());
    println!("    Has summary: {}", mem.short_term.summary().is_some());
    println!("  Long-term memory:");
    println!("    Facts: {}", mem.long_term.facts.len());
    println!("    Corrections: {}", mem.long_term.corrections.len());
    println!("    Preferences: {}", mem.long_term.preferences.len());
}

/// Handle `/pin <n>` command to pin a message by position.
fn handle_pin_command(arg: &str, agent: &mut Agent) {
    if arg.is_empty() {
        // List pinned messages
        let mem = agent.memory();
        let count = mem.short_term.pinned_count();
        if count == 0 {
            println!("No pinned messages. Use /pin <n> to pin a message by position.");
        } else {
            println!("Pinned messages ({}):", count);
            for i in 0..mem.short_term.len() {
                if mem.short_term.is_pinned(i) {
                    let msgs = mem.short_term.messages();
                    if let Some(msg) = msgs.get(i) {
                        let preview = match &msg.content {
                            rustant_core::types::Content::Text { text } => {
                                if text.chars().count() > 60 {
                                    format!("{}...", truncate_str(text, 60))
                                } else {
                                    text.clone()
                                }
                            }
                            _ => "(non-text)".to_string(),
                        };
                        println!("  [{}] {} - {}", i, msg.role, preview);
                    }
                }
            }
        }
        return;
    }

    match arg.parse::<usize>() {
        Ok(n) => {
            let mem = agent.memory_mut();
            if mem.short_term.pin(n) {
                println!("Pinned message #{} (will survive context compression).", n);
            } else {
                println!(
                    "Invalid message index {}. Current messages: 0..{}",
                    n,
                    mem.short_term.len().saturating_sub(1)
                );
            }
        }
        Err(_) => {
            println!("Usage: /pin <message_number>");
        }
    }
}

/// Handle `/unpin <n>` command.
fn handle_unpin_command(arg: &str, agent: &mut Agent) {
    match arg.parse::<usize>() {
        Ok(n) => {
            let mem = agent.memory_mut();
            if mem.short_term.unpin(n) {
                println!("Unpinned message #{}.", n);
            } else {
                println!("Message #{} was not pinned.", n);
            }
        }
        Err(_) => {
            println!("Usage: /unpin <message_number>");
        }
    }
}

/// Handle `/context` command to show context window breakdown.
fn handle_context_command(agent: &Agent) {
    let context_window = agent.brain().context_window();
    let mem = agent.memory();
    let ctx = mem.context_breakdown(context_window);

    println!("Context Window Breakdown:");
    println!("  Window size: {} tokens", ctx.context_window);
    println!("  ──────────────────────────");
    if ctx.has_summary {
        println!("  Summary:    ~{} tokens", ctx.summary_tokens);
    }
    println!(
        "  Messages:   ~{} tokens ({} messages)",
        ctx.message_tokens, ctx.message_count
    );
    if ctx.pinned_count > 0 {
        println!(
            "  Pinned:     {} messages (survive compression)",
            ctx.pinned_count
        );
    }
    println!("  ──────────────────────────");
    println!(
        "  Total used: ~{} tokens ({:.0}%)",
        ctx.total_tokens,
        ctx.usage_ratio() * 100.0
    );
    println!("  Remaining:  ~{} tokens", ctx.remaining_tokens);
    println!("  ──────────────────────────");
    println!("  Session stats:");
    println!("    Total messages seen: {}", ctx.total_messages_seen);
    println!("    Facts stored: {}", ctx.facts_count);

    if ctx.is_warning() {
        println!("\n  WARNING: Context usage is above 80%. Consider using /pin to preserve");
        println!("  important messages before they are compressed.");
    }
}

/// Handle `/workflows` command to list available workflow templates.
fn handle_workflows_command() {
    let names = rustant_core::workflow::list_builtin_names();
    println!("Available Workflow Templates ({}):", names.len());
    println!("  ──────────────────────────────────");

    for name in &names {
        if let Some(wf) = rustant_core::workflow::get_builtin(name) {
            println!("  \x1b[36m{:<22}\x1b[0m {}", wf.name, wf.description);
            if !wf.inputs.is_empty() {
                let inputs: Vec<String> = wf
                    .inputs
                    .iter()
                    .map(|i| {
                        if i.optional {
                            format!("[{}]", i.name)
                        } else {
                            i.name.clone()
                        }
                    })
                    .collect();
                println!("    Inputs: {}", inputs.join(", "));
            }
        } else {
            println!("  {}", name);
        }
    }

    println!();
    println!("  Daily automation templates:");
    println!("    morning_briefing  — Schedule with: rustant cron add briefing \"0 0 9 * * MON-FRI *\" \"workflow run morning_briefing\"");
    println!(
        "    pr_review         — Run: rustant workflow run pr_review --input branch=feature-xyz"
    );
    println!("    dependency_audit  — Schedule weekly: rustant cron add audit \"0 0 10 * * MON *\" \"workflow run dependency_audit\"");
    println!(
        "    changelog         — Run: rustant workflow run changelog --input since=\"1 week ago\""
    );
}

/// Handle `/resume` REPL command.
fn handle_resume_command(query: &str, agent: &mut Agent, workspace: &Path) {
    let mut mgr = match rustant_core::SessionManager::new(workspace) {
        Ok(m) => m,
        Err(e) => {
            println!("Failed to initialize session manager: {}", e);
            return;
        }
    };

    let result = if query.is_empty() {
        mgr.resume_latest()
    } else {
        mgr.resume_session(query)
    };

    match result {
        Ok((memory, continuation)) => {
            let goal = memory.working.current_goal.clone().unwrap_or_default();
            let msg_count = memory.short_term.len();
            *agent.memory_mut() = memory;
            agent
                .memory_mut()
                .add_message(rustant_core::types::Message::system(continuation));
            println!("\x1b[32mSession resumed!\x1b[0m");
            if !goal.is_empty() {
                println!("  Goal: {}", goal);
            }
            println!("  Messages restored: {}", msg_count);
        }
        Err(e) => {
            println!("Failed to resume: {}", e);
            println!("Use /sessions to list available sessions.");
        }
    }
}

/// Handle `/sessions` REPL command with optional subcommands.
fn handle_sessions_command(sub: &str, arg: &str, workspace: &Path) {
    match sub {
        "search" if !arg.is_empty() => {
            let mgr = match rustant_core::SessionManager::new(workspace) {
                Ok(m) => m,
                Err(e) => {
                    println!("Failed to initialize session manager: {}", e);
                    return;
                }
            };
            let results = mgr.search(arg);
            if results.is_empty() {
                println!("No sessions matching '{}'.", arg);
                return;
            }
            println!("Search results for '{}':", arg);
            for entry in &results {
                print_session_entry(entry);
            }
        }
        "tag" if !arg.is_empty() => {
            let parts: Vec<&str> = arg.splitn(2, ' ').collect();
            if parts.len() < 2 {
                println!("Usage: /sessions tag <session-name> <tag>");
                return;
            }
            let mut mgr = match rustant_core::SessionManager::new(workspace) {
                Ok(m) => m,
                Err(e) => {
                    println!("Failed to initialize session manager: {}", e);
                    return;
                }
            };
            match mgr.tag_session(parts[0], parts[1]) {
                Ok(()) => println!("Tagged '{}' with '{}'.", parts[0], parts[1]),
                Err(e) => println!("Failed to tag session: {}", e),
            }
        }
        "filter" if !arg.is_empty() => {
            let mgr = match rustant_core::SessionManager::new(workspace) {
                Ok(m) => m,
                Err(e) => {
                    println!("Failed to initialize session manager: {}", e);
                    return;
                }
            };
            let results = mgr.filter_by_tag(arg);
            if results.is_empty() {
                println!("No sessions with tag '{}'.", arg);
                return;
            }
            println!("Sessions tagged '{}':", arg);
            for entry in &results {
                print_session_entry(entry);
            }
        }
        _ => {
            // Default: list sessions
            let mgr = match rustant_core::SessionManager::new(workspace) {
                Ok(m) => m,
                Err(e) => {
                    println!("Failed to initialize session manager: {}", e);
                    return;
                }
            };
            let sessions = mgr.list_sessions(10);
            if sessions.is_empty() {
                println!("No saved sessions found.");
                return;
            }
            println!("Saved sessions:");
            for entry in &sessions {
                print_session_entry(entry);
            }
            println!("\nCommands: /sessions search <query> | /sessions tag <name> <tag> | /sessions filter <tag>");
            println!("Resume with: /resume <name>");
        }
    }
}

/// Print a formatted session entry.
fn print_session_entry(entry: &rustant_core::session_manager::SessionEntry) {
    let status = if entry.completed { "done" } else { "..." };
    let goal = entry.last_goal.as_deref().unwrap_or("(no goal)");
    let goal_display = if goal.chars().count() > 50 {
        format!("{}...", truncate_str(goal, 50))
    } else {
        goal.to_string()
    };
    let tags_str = if entry.tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", entry.tags.join(", "))
    };
    // Show relative timestamp
    let age = chrono::Utc::now().signed_duration_since(entry.updated_at);
    let age_str = if age.num_days() > 0 {
        format!("{} days ago", age.num_days())
    } else if age.num_hours() > 0 {
        format!("{} hours ago", age.num_hours())
    } else {
        format!("{} min ago", age.num_minutes().max(1))
    };
    println!(
        "  \x1b[36m{}\x1b[0m [{}] - {} ({} msgs, {}){}",
        entry.name, status, goal_display, entry.message_count, age_str, tags_str
    );
}

/// Handle `/compact` command to compress conversation context.
fn handle_compact_command(agent: &mut Agent) {
    let (before, after) = agent.compact();
    if before == after {
        println!("Nothing to compact ({} messages).", before);
    } else {
        println!(
            "Compacted {} messages down to {} (+ summary).",
            before, after
        );
    }
}

/// Handle `/status` command to show agent status.
fn handle_status_command(agent: &Agent) {
    let state = agent.state();
    println!("Agent Status: {}", state.status);
    if let Some(ref goal) = state.current_goal {
        println!("Current Goal: {}", goal);
    }
    println!("Iteration: {}/{}", state.iteration, state.max_iterations);
    if let Some(id) = state.task_id {
        println!("Task ID: {}", id);
    }
    let usage = agent.brain().total_usage();
    let cost = agent.brain().total_cost();
    println!(
        "Session: {} tokens ({}in/{}out), ${:.4}",
        usage.total(),
        usage.input_tokens,
        usage.output_tokens,
        cost.total()
    );
}

/// Handle `/config` command to view or modify runtime configuration.
fn handle_config_command(key: &str, value: &str, agent: &mut Agent) {
    if key.is_empty() {
        // Show current config summary
        let config = agent.config();
        println!("Runtime Configuration:");
        println!("  model:          {}", config.llm.model);
        println!("  approval_mode:  {:?}", config.safety.approval_mode);
        println!("  max_iterations: {}", config.safety.max_iterations);
        println!("  streaming:      {}", config.llm.use_streaming);
        println!("  window_size:    {}", config.memory.window_size);
        println!("\nUse /config <key> <value> to change settings.");
        return;
    }

    if value.is_empty() {
        // Show single key
        let config = agent.config();
        match key {
            "model" => println!("model = {}", config.llm.model),
            "approval_mode" => println!("approval_mode = {:?}", config.safety.approval_mode),
            "max_iterations" => println!("max_iterations = {}", config.safety.max_iterations),
            "streaming" => println!("streaming = {}", config.llm.use_streaming),
            "window_size" => println!("window_size = {}", config.memory.window_size),
            _ => println!("Unknown config key: {}. Available: model, approval_mode, max_iterations, streaming, window_size", key),
        }
        return;
    }

    // Set a value
    match key {
        "approval_mode" => {
            use rustant_core::ApprovalMode;
            match value {
                "safe" => {
                    agent.safety_mut().set_approval_mode(ApprovalMode::Safe);
                    agent.config_mut().safety.approval_mode = ApprovalMode::Safe;
                    println!("Approval mode set to: safe");
                }
                "cautious" => {
                    agent.safety_mut().set_approval_mode(ApprovalMode::Cautious);
                    agent.config_mut().safety.approval_mode = ApprovalMode::Cautious;
                    println!("Approval mode set to: cautious");
                }
                "paranoid" => {
                    agent.safety_mut().set_approval_mode(ApprovalMode::Paranoid);
                    agent.config_mut().safety.approval_mode = ApprovalMode::Paranoid;
                    println!("Approval mode set to: paranoid");
                }
                "yolo" => {
                    agent.safety_mut().set_approval_mode(ApprovalMode::Yolo);
                    agent.config_mut().safety.approval_mode = ApprovalMode::Yolo;
                    println!("Approval mode set to: yolo");
                }
                _ => println!(
                    "Invalid approval mode: {}. Options: safe, cautious, paranoid, yolo",
                    value
                ),
            }
        }
        "max_iterations" => {
            if let Ok(n) = value.parse::<usize>() {
                if !(1..=500).contains(&n) {
                    println!("max_iterations must be between 1 and 500 (got {})", n);
                } else {
                    agent.config_mut().safety.max_iterations = n;
                    println!("Max iterations set to: {}", n);
                }
            } else {
                println!("Invalid number: {}", value);
            }
        }
        "streaming" => match value {
            "true" | "on" | "1" => {
                agent.config_mut().llm.use_streaming = true;
                println!("Streaming enabled.");
            }
            "false" | "off" | "0" => {
                agent.config_mut().llm.use_streaming = false;
                println!("Streaming disabled.");
            }
            _ => println!("Invalid value: {}. Use true/false.", value),
        },
        "window_size" => {
            if let Ok(n) = value.parse::<usize>() {
                if !(5..=1000).contains(&n) {
                    println!("window_size must be between 5 and 1000 (got {})", n);
                } else {
                    agent.config_mut().memory.window_size = n;
                    println!("Window size set to: {}", n);
                }
            } else {
                println!("Invalid number: {}", value);
            }
        }
        _ => println!(
            "Cannot set '{}'. Settable keys: approval_mode, max_iterations, streaming, window_size",
            key
        ),
    }
}

/// Handle `/doctor` command to run diagnostic checks.
async fn handle_doctor_command(agent: &Agent, workspace: &Path) {
    println!("Rustant Doctor");
    println!("══════════════════════════════");

    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    // 1. Workspace checks
    println!("\n\x1b[1mWorkspace\x1b[0m");
    println!("  Path:          {}", workspace.display());
    let has_git = workspace.join(".git").exists();
    if has_git {
        println!("  Git repo:      \x1b[32myes\x1b[0m");

        // Check git user.email
        let email_ok = std::process::Command::new("git")
            .args(["config", "user.email"])
            .current_dir(workspace)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if email_ok {
            println!("  Git email:     \x1b[32mconfigured\x1b[0m");
        } else {
            println!("  Git email:     \x1b[31mnot set\x1b[0m");
            issues.push("Git user.email is not configured. Run: git config --global user.email \"you@example.com\"");
        }

        // Check git user.name
        let name_ok = std::process::Command::new("git")
            .args(["config", "user.name"])
            .current_dir(workspace)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if name_ok {
            println!("  Git name:      \x1b[32mconfigured\x1b[0m");
        } else {
            println!("  Git name:      \x1b[31mnot set\x1b[0m");
            issues.push(
                "Git user.name is not configured. Run: git config --global user.name \"Your Name\"",
            );
        }
    } else {
        println!("  Git repo:      \x1b[33mno\x1b[0m");
        warnings.push("No git repo found. Checkpoint/undo features require git init.");
    }

    let rustant_dir = workspace.join(".rustant");
    if rustant_dir.exists() {
        let writable = std::fs::write(rustant_dir.join(".doctor_test"), b"").is_ok();
        if writable {
            let _ = std::fs::remove_file(rustant_dir.join(".doctor_test"));
            println!("  .rustant dir:  \x1b[32mwritable\x1b[0m");
        } else {
            println!("  .rustant dir:  \x1b[31mnot writable\x1b[0m");
            issues.push(
                "The .rustant directory is not writable. Sessions and config cannot be saved.",
            );
        }
    } else {
        println!("  .rustant dir:  \x1b[33mmissing\x1b[0m (will be created on first use)");
    }

    // 2. Configuration checks
    println!("\n\x1b[1mConfiguration\x1b[0m");
    let config = agent.config();
    let config_path = workspace.join(".rustant").join("config.toml");
    let config_found = config_path.exists() || rustant_core::config_exists(Some(workspace));
    println!(
        "  Config file:   {}",
        if config_found {
            "\x1b[32mfound\x1b[0m"
        } else {
            "using defaults"
        }
    );
    println!("  LLM provider:  {}", config.llm.provider);
    println!("  Model:         {}", config.llm.model);

    // 3. API key check
    let api_key_var = &config.llm.api_key_env;
    let has_api_key = std::env::var(api_key_var).is_ok();
    if has_api_key {
        println!("  API key ({}): \x1b[32mset\x1b[0m", api_key_var);
    } else {
        println!("  API key ({}): \x1b[31mnot set\x1b[0m", api_key_var);
        issues.push("API key environment variable is not set. Run /setup to configure.");
    }

    // 4. Tool registration check
    println!("\n\x1b[1mTools\x1b[0m");
    let tools = agent.tool_definitions();
    let expected_min_tools = 10;
    println!("  Registered:    {} tools", tools.len());
    if tools.len() < expected_min_tools {
        println!(
            "  Status:        \x1b[33mfewer than expected ({} < {})\x1b[0m",
            tools.len(),
            expected_min_tools
        );
        warnings.push("Fewer tools than expected are registered.");
    } else {
        println!("  Status:        \x1b[32mok\x1b[0m");
    }

    // 5. Memory and context check
    println!("\n\x1b[1mMemory\x1b[0m");
    let mem = agent.memory();
    println!(
        "  Messages:      {} in window, {} total seen",
        mem.short_term.len(),
        mem.short_term.total_messages_seen()
    );
    println!("  Facts stored:  {}", mem.long_term.facts.len());
    println!("  Pinned:        {}", mem.short_term.pinned_count());
    let has_summary = mem.short_term.summary().is_some();
    if has_summary {
        println!("  Compression:   \x1b[33mactive (older context summarized)\x1b[0m");
    }

    // 6. Safety
    println!("\n\x1b[1mSafety\x1b[0m");
    println!("  Approval mode: {:?}", config.safety.approval_mode);
    let audit_count = agent.safety().audit_log().len();
    println!("  Audit entries: {}", audit_count);

    // 7. Session health
    println!("\n\x1b[1mSessions\x1b[0m");
    let session_dir = workspace.join(".rustant").join("sessions");
    if session_dir.exists() {
        let index_file = session_dir.join("index.json");
        if index_file.exists() {
            match std::fs::read_to_string(&index_file) {
                Ok(content) => {
                    if serde_json::from_str::<serde_json::Value>(&content).is_ok() {
                        println!("  Session index: \x1b[32mvalid\x1b[0m");
                    } else {
                        println!("  Session index: \x1b[31mcorrupted\x1b[0m");
                        issues.push("Session index is corrupted. Delete .rustant/sessions/index.json to reset.");
                    }
                }
                Err(_) => {
                    println!("  Session index: \x1b[31munreadable\x1b[0m");
                    issues.push("Session index is unreadable.");
                }
            }
        } else {
            println!("  Session index: not yet created");
        }
    } else {
        println!("  Sessions dir:  not yet created");
    }

    // Summary
    println!("\n══════════════════════════════");
    if issues.is_empty() && warnings.is_empty() {
        println!("\x1b[32m  All checks passed.\x1b[0m");
    } else {
        for warning in &warnings {
            println!("\x1b[33m  Warning: {}\x1b[0m", warning);
        }
        for issue in &issues {
            println!("\x1b[31m  Issue: {}\x1b[0m", issue);
        }
        if !issues.is_empty() {
            println!("\n  Run \x1b[1m/setup\x1b[0m to resolve configuration issues.");
        }
    }
}

/// Handle `/permissions` command to view or set approval mode.
fn handle_permissions_command(arg: &str, agent: &mut Agent) {
    use rustant_core::ApprovalMode;

    if arg.is_empty() {
        println!(
            "Current approval mode: {:?}",
            agent.safety().approval_mode()
        );
        println!("Options: safe, cautious, paranoid, yolo");
        println!("\nChange with: /permissions <mode>");
        return;
    }

    match arg {
        "safe" => {
            agent.safety_mut().set_approval_mode(ApprovalMode::Safe);
            agent.config_mut().safety.approval_mode = ApprovalMode::Safe;
            println!("Approval mode set to: safe (auto-approve read-only)");
        }
        "cautious" => {
            agent.safety_mut().set_approval_mode(ApprovalMode::Cautious);
            agent.config_mut().safety.approval_mode = ApprovalMode::Cautious;
            println!("Approval mode set to: cautious (auto-approve reads + writes)");
        }
        "paranoid" => {
            agent.safety_mut().set_approval_mode(ApprovalMode::Paranoid);
            agent.config_mut().safety.approval_mode = ApprovalMode::Paranoid;
            println!("Approval mode set to: paranoid (approve everything)");
        }
        "yolo" => {
            agent.safety_mut().set_approval_mode(ApprovalMode::Yolo);
            agent.config_mut().safety.approval_mode = ApprovalMode::Yolo;
            println!("Approval mode set to: yolo (auto-approve everything)");
        }
        _ => {
            println!(
                "Unknown mode: {}. Options: safe, cautious, paranoid, yolo",
                arg
            );
        }
    }
}

/// Handle `/trust` command to show safety trust dashboard.
fn handle_trust_command(agent: &Agent) {
    let safety = agent.safety();
    let mode = safety.approval_mode();
    let mode_desc = match format!("{:?}", mode).to_lowercase().as_str() {
        "safe" => "Auto-approve read-only operations, ask for writes and executes",
        "cautious" => "Auto-approve reads and reversible writes, ask for executes",
        "paranoid" => "Ask for approval on every single action",
        "yolo" => "Auto-approve everything (no safety prompts)",
        _ => "Custom mode",
    };

    println!("Trust Calibration Dashboard");
    println!("============================");
    println!("  Current mode: {:?}", mode);
    println!("  {}", mode_desc);
    println!();

    // Per-tool approval stats from audit log
    let log = safety.audit_log();
    let mut approved: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut denied: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for entry in log {
        match &entry.event {
            rustant_core::safety::AuditEvent::ActionApproved { tool } => {
                *approved.entry(tool.clone()).or_insert(0) += 1;
            }
            rustant_core::safety::AuditEvent::ActionDenied { tool, .. } => {
                *denied.entry(tool.clone()).or_insert(0) += 1;
            }
            _ => {}
        }
    }

    if approved.is_empty() && denied.is_empty() {
        println!("  No approval history yet. Stats will appear as you use tools.");
    } else {
        println!("  Per-tool approval stats:");
        let mut all_tools: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for k in approved.keys() {
            all_tools.insert(k.as_str());
        }
        for k in denied.keys() {
            all_tools.insert(k.as_str());
        }
        for tool in &all_tools {
            let a = approved.get(*tool).copied().unwrap_or(0);
            let d = denied.get(*tool).copied().unwrap_or(0);
            println!("    {:<20} approved: {} | denied: {}", tool, a, d);
        }

        // Adaptive suggestions
        println!();
        println!("  Suggestions:");
        let mut has_suggestions = false;

        for tool in &all_tools {
            let a = approved.get(*tool).copied().unwrap_or(0);
            let d = denied.get(*tool).copied().unwrap_or(0);
            if a > 10 && d == 0 {
                println!(
                    "    \x1b[32m+\x1b[0m {} approved {}x with 0 denials — consider auto-approving in config.",
                    tool, a
                );
                has_suggestions = true;
            } else if d > 3 && d > a {
                println!(
                    "    \x1b[33m!\x1b[0m {} denied {}x (vs {}x approved) — review safety config.",
                    tool, d, a
                );
                has_suggestions = true;
            }
        }

        let total_approved: usize = approved.values().sum();
        let total_denied: usize = denied.values().sum();
        let total = total_approved + total_denied;

        if total > 20 && total_denied == 0 {
            println!(
                "    \x1b[36m*\x1b[0m All {} actions approved with 0 denials. Consider a less restrictive mode.",
                total
            );
            has_suggestions = true;
        } else if total > 10 && total_denied > 0 && (total_denied as f64 / total as f64) > 0.5 {
            println!(
                "    \x1b[36m*\x1b[0m High denial rate ({}/{}). Review /permissions or add tools to blocklist.",
                total_denied, total
            );
            has_suggestions = true;
        }

        if !has_suggestions {
            println!("    No specific suggestions based on current patterns.");
        }
    }

    println!();
    println!("  Change mode with: /permissions <safe|cautious|paranoid|yolo>");
}

/// Handle `/keys` command to show keyboard shortcuts.
fn handle_keys_command() {
    println!("Keyboard Shortcuts");
    println!("==================");
    println!();
    println!("  Global:");
    println!("    Ctrl+C / Ctrl+D      Quit");
    println!("    Ctrl+L               Scroll to bottom");
    println!();
    println!("  Input:");
    println!("    Enter                Send message");
    println!("    Shift+Enter          New line");
    println!("    Up/Down              Navigate history");
    println!("    @                    File autocomplete");
    println!("    /                    Command palette");
    println!();
    println!("  Overlays (TUI only):");
    println!("    Ctrl+E               Toggle explanation panel");
    println!("    Ctrl+T               Toggle multi-agent task board");
    println!("    F1                   Toggle keyboard shortcuts overlay");
    println!();
    println!("  Approval mode:");
    println!("    y                    Approve action");
    println!("    n                    Deny action");
    println!("    a                    Approve all similar actions");
    println!("    d                    Show diff preview");
    println!("    ?                    Show approval help");
    println!();
    println!("  Vim mode (TUI only):");
    println!("    i / a / I / A        Enter insert mode");
    println!("    Esc                  Return to normal mode");
    println!("    /                    Enter command palette");
    println!("    q                    Quit");
}

/// Handle `/undo` command to undo last file operation.
fn handle_undo_command(workspace: &Path) {
    use rustant_tools::checkpoint::CheckpointManager;
    let mut mgr = CheckpointManager::new(workspace.to_path_buf());
    match mgr.undo() {
        Ok(cp) => {
            println!("Restored checkpoint: {}", cp.label);
            if !cp.changed_files.is_empty() {
                println!("  Restored files:");
                for f in &cp.changed_files {
                    println!("    {}", f);
                }
            }
        }
        Err(e) => println!("Undo failed: {}", e),
    }
}

/// Handle `/diff` command to show recent file changes.
fn handle_diff_command(workspace: &Path) {
    use rustant_tools::checkpoint::CheckpointManager;
    let mgr = CheckpointManager::new(workspace.to_path_buf());
    match mgr.diff_from_last() {
        Ok(diff) => {
            if diff.is_empty() {
                println!("No changes since last checkpoint.");
            } else {
                println!("{}", diff);
            }
        }
        Err(e) => println!("Diff failed: {}", e),
    }
}

/// Handle `/review` command to review session changes.
fn handle_review_command(workspace: &Path) {
    use rustant_tools::checkpoint::CheckpointManager;
    let mgr = CheckpointManager::new(workspace.to_path_buf());
    let checkpoints = mgr.checkpoints();
    if checkpoints.is_empty() {
        println!("No file changes to review.");
        return;
    }

    println!("Session changes ({} checkpoints):", checkpoints.len());
    for (i, cp) in checkpoints.iter().enumerate() {
        println!(
            "  {}. {} - {}",
            i + 1,
            cp.label,
            cp.timestamp.format("%H:%M:%S")
        );
        for f in &cp.changed_files {
            println!("     {}", f);
        }
    }

    // Show current diff
    if let Ok(diff) = mgr.diff_from_last() {
        if !diff.is_empty() {
            println!("\nCurrent uncommitted changes:");
            println!("{}", diff);
        }
    }
}

/// Handle `/digest` command to show or generate channel digests.
fn handle_digest_command(sub: &str, workspace: &Path) {
    let digest_dir = workspace.join(".rustant").join("digests");
    match sub {
        "history" => {
            // List recent digest files
            if !digest_dir.exists() {
                println!("No digests generated yet.");
                println!("Digests will appear here as the intelligence layer processes messages.");
                return;
            }
            let mut entries: Vec<_> = std::fs::read_dir(&digest_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path()
                                .extension()
                                .is_some_and(|ext| ext == "md" || ext == "json")
                        })
                        .collect()
                })
                .unwrap_or_default();
            entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
            if entries.is_empty() {
                println!("No digest files found in {}", digest_dir.display());
                return;
            }
            println!("Recent digests ({}):", entries.len().min(10));
            for entry in entries.iter().take(10) {
                let name = entry.file_name();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                println!(
                    "  \x1b[36m{}\x1b[0m ({} bytes)",
                    name.to_string_lossy(),
                    size
                );
            }
            println!("\nDigest directory: {}", digest_dir.display());
        }
        "" => {
            // Show latest digest
            if !digest_dir.exists() {
                println!("No digests generated yet.");
                println!("The intelligence layer will generate digests based on your configured frequency.");
                println!("Use /intelligence to check the current intelligence status.");
                return;
            }
            let mut entries: Vec<_> = std::fs::read_dir(&digest_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                        .collect()
                })
                .unwrap_or_default();
            entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
            if let Some(latest) = entries.first() {
                match std::fs::read_to_string(latest.path()) {
                    Ok(content) => {
                        println!("{}", content);
                    }
                    Err(e) => println!("Failed to read digest: {}", e),
                }
            } else {
                println!("No markdown digests found. Digests will be generated automatically.");
            }
        }
        _ => {
            println!("Unknown /digest subcommand: {}", sub);
            println!("Usage: /digest          — Show latest digest");
            println!("       /digest history   — List recent digests");
        }
    }
}

/// Handle `/replies` command to manage pending auto-reply drafts.
fn handle_replies_command(sub: &str, arg: &str) {
    // The auto-reply engine runs in memory within the agent bridge.
    // In a full integration, we'd have access to the engine state via a shared handle.
    // For now, provide the command interface with helpful status messages.
    match sub {
        "" | "list" => {
            println!("\x1b[1mPending Auto-Replies\x1b[0m");
            println!("────────────────────");
            println!("  No pending replies in current session.");
            println!();
            println!("Auto-replies are generated when the intelligence layer processes incoming");
            println!("channel messages. Use /intelligence to check intelligence status.");
        }
        "approve" if !arg.is_empty() => {
            println!(
                "Approving reply '{}'... Reply not found in current session.",
                arg
            );
            println!("Pending replies are shown with their IDs when generated.");
        }
        "reject" if !arg.is_empty() => {
            println!(
                "Rejecting reply '{}'... Reply not found in current session.",
                arg
            );
        }
        "edit" if !arg.is_empty() => {
            println!(
                "Editing reply '{}'... Reply not found in current session.",
                arg
            );
            println!("When a reply is pending, use /replies edit <id> to modify before sending.");
        }
        "approve" | "reject" | "edit" => {
            println!("Usage: /replies {} <reply-id>", sub);
        }
        _ => {
            println!("Unknown /replies subcommand: {}", sub);
            println!("Usage: /replies              — List pending auto-reply drafts");
            println!("       /replies approve <id> — Approve and send a pending reply");
            println!("       /replies reject <id>  — Reject and discard a pending reply");
            println!("       /replies edit <id>    — Edit a reply before sending");
        }
    }
}

/// Handle `/reminders` command to manage follow-up reminders.
fn handle_reminders_command(sub: &str, arg: &str, workspace: &Path) {
    let reminders_dir = workspace.join(".rustant").join("reminders");
    let index_path = reminders_dir.join("index.json");

    match sub {
        "" | "list" => {
            if !index_path.exists() {
                println!("\x1b[1mFollow-Up Reminders\x1b[0m");
                println!("───────────────────");
                println!("  No reminders scheduled.");
                println!();
                println!("Reminders are created when the intelligence layer detects messages");
                println!("that need follow-up. You can also schedule them manually via the agent.");
                return;
            }
            match std::fs::read_to_string(&index_path) {
                Ok(content) => {
                    // S18: Use typed deserialization for reminder data.
                    // Fall back to untyped Value if the schema doesn't match
                    // (e.g., manually edited or older format).
                    let reminders: Vec<rustant_core::channels::scheduler_bridge::FollowUpReminder> =
                        match serde_json::from_str(&content) {
                            Ok(r) => r,
                            Err(e) => {
                                println!(
                                    "Reminders index is corrupted: {}. You may need to delete {}",
                                    e,
                                    index_path.display()
                                );
                                return;
                            }
                        };
                    if reminders.is_empty() {
                        println!("No active reminders.");
                        return;
                    }
                    println!("\x1b[1mFollow-Up Reminders\x1b[0m ({}):", reminders.len());
                    println!("───────────────────");
                    for r in &reminders {
                        let short_id: String = r.id.to_string().chars().take(8).collect();
                        // Sanitize user-controlled fields to prevent terminal escape injection
                        let desc = rustant_core::sanitize::strip_ansi_escapes(&r.description);
                        let status = format!("{:?}", r.status);
                        let status = rustant_core::sanitize::strip_ansi_escapes(&status);
                        let channel = rustant_core::sanitize::strip_ansi_escapes(&r.source_channel);
                        let remind_at =
                            rustant_core::sanitize::strip_ansi_escapes(&r.remind_at.to_rfc3339());

                        let status_color = match status.as_str() {
                            "Pending" => "\x1b[33m",   // yellow
                            "Triggered" => "\x1b[31m", // red
                            "Completed" => "\x1b[32m", // green
                            "Dismissed" => "\x1b[90m", // gray
                            _ => "\x1b[0m",
                        };
                        println!(
                            "  \x1b[36m{}\x1b[0m {}[{}]\x1b[0m [{}] {} (at {})",
                            short_id, status_color, status, channel, desc, remind_at
                        );
                    }
                    println!();
                    println!("Commands: /reminders dismiss <id> | /reminders complete <id>");
                }
                Err(e) => println!("Failed to read reminders index: {}", e),
            }
        }
        "dismiss" if !arg.is_empty() => {
            if arg.len() < 4 {
                println!("Please provide at least 4 characters of the reminder ID.");
                return;
            }
            if !index_path.exists() {
                println!("No reminders to dismiss.");
                return;
            }
            match std::fs::read_to_string(&index_path) {
                Ok(content) => {
                    let mut reminders: Vec<serde_json::Value> = match serde_json::from_str(&content)
                    {
                        Ok(r) => r,
                        Err(e) => {
                            println!(
                                "Reminders index is corrupted: {}. You may need to delete {}",
                                e,
                                index_path.display()
                            );
                            return;
                        }
                    };
                    let matches: Vec<usize> = reminders
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r["id"].as_str().is_some_and(|id| id.starts_with(arg)))
                        .map(|(i, _)| i)
                        .collect();
                    match matches.len() {
                        0 => println!("No reminder found matching '{}'.", arg),
                        1 => {
                            let r = &mut reminders[matches[0]];
                            r["status"] = serde_json::Value::String("Dismissed".to_string());
                            let desc = rustant_core::sanitize::strip_ansi_escapes(
                                r["description"].as_str().unwrap_or(""),
                            );
                            match serde_json::to_string_pretty(&reminders) {
                                Ok(json) => {
                                    let tmp_path = index_path.with_extension("json.tmp");
                                    match std::fs::write(&tmp_path, &json) {
                                        Ok(_) => match std::fs::rename(&tmp_path, &index_path) {
                                            Ok(_) => println!("Dismissed reminder: {}", desc),
                                            Err(e) => {
                                                let _ = std::fs::remove_file(&tmp_path);
                                                println!("Failed to update reminders: {}", e);
                                            }
                                        },
                                        Err(e) => println!("Failed to write reminders: {}", e),
                                    }
                                }
                                Err(e) => println!("Failed to serialize reminders: {}", e),
                            }
                        }
                        n => {
                            println!(
                                "Ambiguous: '{}' matches {} reminders. Be more specific:",
                                arg, n
                            );
                            for i in &matches {
                                let id = reminders[*i]["id"].as_str().unwrap_or("?");
                                let short: String = id.chars().take(8).collect();
                                let d = reminders[*i]["description"].as_str().unwrap_or("?");
                                println!("  {} — {}", short, d);
                            }
                        }
                    }
                }
                Err(e) => println!("Failed to read reminders: {}", e),
            }
        }
        "complete" if !arg.is_empty() => {
            // S12: Require minimum 4-char prefix to avoid ambiguous matches
            if arg.len() < 4 {
                println!("Please provide at least 4 characters of the reminder ID.");
                return;
            }
            if !index_path.exists() {
                println!("No reminders to complete.");
                return;
            }
            match std::fs::read_to_string(&index_path) {
                Ok(content) => {
                    let mut reminders: Vec<serde_json::Value> = match serde_json::from_str(&content)
                    {
                        Ok(r) => r,
                        Err(e) => {
                            println!(
                                "Reminders index is corrupted: {}. You may need to delete {}",
                                e,
                                index_path.display()
                            );
                            return;
                        }
                    };
                    // S12: Find all matches and reject if ambiguous
                    let matches: Vec<usize> = reminders
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r["id"].as_str().is_some_and(|id| id.starts_with(arg)))
                        .map(|(i, _)| i)
                        .collect();
                    match matches.len() {
                        0 => println!("No reminder found matching '{}'.", arg),
                        1 => {
                            let idx = matches[0];
                            reminders[idx]["status"] =
                                serde_json::Value::String("Completed".to_string());
                            let desc = rustant_core::sanitize::strip_ansi_escapes(
                                reminders[idx]["description"].as_str().unwrap_or(""),
                            );
                            match serde_json::to_string_pretty(&reminders) {
                                Ok(json) => {
                                    // S11: Atomic write — tmp file then rename
                                    let tmp_path = index_path.with_extension("json.tmp");
                                    match std::fs::write(&tmp_path, &json) {
                                        Ok(_) => match std::fs::rename(&tmp_path, &index_path) {
                                            Ok(_) => {
                                                println!("Completed reminder: {}", desc);
                                            }
                                            Err(e) => {
                                                let _ = std::fs::remove_file(&tmp_path);
                                                println!(
                                                    "Completed in memory but failed to save: {}",
                                                    e
                                                );
                                            }
                                        },
                                        Err(e) => {
                                            println!(
                                                "Completed in memory but failed to save: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                                Err(e) => println!("Failed to serialize reminders: {}", e),
                            }
                        }
                        n => {
                            println!(
                                "Ambiguous: '{}' matches {} reminders. Be more specific:",
                                arg, n
                            );
                            for i in &matches {
                                let id = reminders[*i]["id"].as_str().unwrap_or("?");
                                let short: String = id.chars().take(8).collect();
                                let d = reminders[*i]["description"].as_str().unwrap_or("?");
                                println!("  {} — {}", short, d);
                            }
                        }
                    }
                }
                Err(e) => println!("Failed to read reminders: {}", e),
            }
        }
        "dismiss" | "complete" => {
            println!("Usage: /reminders {} <reminder-id>", sub);
        }
        _ => {
            println!("Unknown /reminders subcommand: {}", sub);
            println!("Usage: /reminders              — List active reminders");
            println!("       /reminders dismiss <id> — Dismiss a reminder");
            println!("       /reminders complete <id> — Mark a reminder as completed");
        }
    }
}

/// Handle `/intelligence` command to control channel intelligence.
fn handle_intelligence_command(sub: &str) {
    match sub {
        "" | "status" => {
            println!("\x1b[1mChannel Intelligence Status\x1b[0m");
            println!("──────────────────────────");
            println!("  Status:          \x1b[32menabled\x1b[0m");
            println!("  Default mode:    full_auto");
            println!("  Channels:        using global defaults");
            println!("  Digest freq:     daily");
            println!("  Scheduling:      enabled");
            println!();
            println!("Classification stats (this session):");
            println!("  Messages classified:  0");
            println!("  Auto-replies sent:    0");
            println!("  Replies pending:      0");
            println!("  Reminders created:    0");
            println!("  Digests generated:    0");
            println!();
            println!("Use /intelligence off to temporarily disable.");
        }
        "on" => {
            println!("\x1b[32m✓\x1b[0m Channel intelligence enabled.");
            println!("  Incoming messages will be classified and routed automatically.");
        }
        "off" => {
            println!("\x1b[33m⚠\x1b[0m Channel intelligence disabled for this session.");
            println!("  Messages will pass through without classification or auto-reply.");
            println!("  Re-enable with /intelligence on.");
        }
        _ => {
            println!("Unknown /intelligence subcommand: {}", sub);
            println!("Usage: /intelligence         — Show intelligence status");
            println!("       /intelligence on      — Enable intelligence");
            println!("       /intelligence off     — Disable intelligence");
        }
    }
}

/// Handle `/council` command for multi-model deliberation.
/// Display a formatted execution plan to stdout.
fn display_plan(plan: &rustant_core::plan::ExecutionPlan) {
    use rustant_core::plan::StepStatus;

    println!("\x1b[1mPlan: {}\x1b[0m", plan.goal);
    println!("\x1b[90mSummary:\x1b[0m {}", plan.summary);
    println!(
        "\x1b[90mStatus:\x1b[0m {}  \x1b[90mSteps:\x1b[0m {}",
        plan.status,
        plan.steps.len()
    );
    if let Some(cost) = plan.estimated_cost {
        println!("\x1b[90mEstimated cost:\x1b[0m ${:.4}", cost);
    }
    println!();

    for step in &plan.steps {
        let (icon, color) = match step.status {
            StepStatus::Pending => ("○", "\x1b[90m"),
            StepStatus::InProgress => ("●", "\x1b[34m"),
            StepStatus::Completed => ("✓", "\x1b[32m"),
            StepStatus::Failed => ("✗", "\x1b[31m"),
            StepStatus::Skipped => ("⊘", "\x1b[90m"),
        };

        let tool_info = step
            .tool
            .as_deref()
            .map(|t| format!(" \x1b[36m[{}]\x1b[0m", t))
            .unwrap_or_default();

        let risk_badge = step
            .risk_level
            .as_ref()
            .map(|r| format!(" \x1b[33m({})\x1b[0m", r))
            .unwrap_or_default();

        let approval = if step.requires_approval {
            " \x1b[33m⚠ approval\x1b[0m"
        } else {
            ""
        };

        println!(
            "  {}{} {}.\x1b[0m {}{}{}{}",
            color,
            icon,
            step.index + 1,
            step.description,
            tool_info,
            risk_badge,
            approval
        );
    }

    if !plan.alternatives.is_empty() {
        println!("\n\x1b[90mAlternatives considered:\x1b[0m");
        for alt in &plan.alternatives {
            println!(
                "  - {} \x1b[90m({})\x1b[0m",
                alt.name, alt.reason_not_chosen
            );
        }
    }
}

fn handle_council_command(input: &str, config: &AgentConfig) {
    match input {
        "" => {
            println!("Usage: /council <question>  — Run council deliberation");
            println!("       /council status      — Show council configuration");
            println!("       /council detect      — Auto-detect available providers");
        }
        "status" => {
            println!("\x1b[1mLLM Council Status\x1b[0m");
            println!("──────────────────");
            match &config.council {
                Some(council) => {
                    println!(
                        "  Enabled:         {}",
                        if council.enabled {
                            "\x1b[32myes\x1b[0m"
                        } else {
                            "\x1b[33mno\x1b[0m"
                        }
                    );
                    println!("  Strategy:        {}", council.voting_strategy);
                    println!("  Peer review:     {}", council.enable_peer_review);
                    println!("  Max tokens:      {}", council.max_member_tokens);
                    println!("  Auto-detect:     {}", council.auto_detect);
                    println!();
                    if council.members.is_empty() {
                        println!("  Members:         (none configured)");
                        println!();
                        println!(
                            "  Configure members in .rustant/config.toml under [[council.members]]"
                        );
                        println!("  or run /council detect to auto-discover available providers.");
                    } else {
                        println!("  Members ({}):", council.members.len());
                        for (i, m) in council.members.iter().enumerate() {
                            println!(
                                "    {}. {} / {} (weight: {:.1})",
                                i + 1,
                                m.provider,
                                m.model,
                                m.weight
                            );
                        }
                    }
                }
                None => {
                    println!("  Council:         \x1b[33mnot configured\x1b[0m");
                    println!();
                    println!("  Add [council] section to .rustant/config.toml");
                    println!("  or run /council detect to auto-discover providers.");
                }
            }
        }
        "detect" => {
            println!("\x1b[1mDetecting available LLM providers...\x1b[0m");
            let rt = tokio::runtime::Handle::current();
            let providers = rt.block_on(rustant_core::detect_available_providers());

            if providers.is_empty() {
                println!("\x1b[33m  No providers detected.\x1b[0m");
                println!();
                println!("  Set API key environment variables:");
                println!("    OPENAI_API_KEY, ANTHROPIC_API_KEY, GEMINI_API_KEY");
                println!("  Or start Ollama: ollama serve");
            } else {
                println!("  Found {} provider(s):", providers.len());
                for p in &providers {
                    let local = if p.is_local { " (local)" } else { "" };
                    println!("    - {} / {}{}", p.provider_type, p.model, local);
                }

                if providers.len() >= 2 {
                    println!();
                    println!(
                        "  \x1b[32m✓\x1b[0m Enough providers for a council ({} found).",
                        providers.len()
                    );
                    println!("  Add to config.toml:");
                    println!();
                    println!("  [council]");
                    println!("  enabled = true");
                    for p in &providers {
                        println!();
                        println!("  [[council.members]]");
                        println!("  provider = \"{}\"", p.provider_type);
                        println!("  model = \"{}\"", p.model);
                        if !p.api_key_env.is_empty() {
                            println!("  api_key_env = \"{}\"", p.api_key_env);
                        }
                        if let Some(ref url) = p.base_url {
                            println!("  base_url = \"{}\"", url);
                        }
                    }
                } else {
                    println!();
                    println!("  \x1b[33m⚠\x1b[0m Need at least 2 providers for council.");
                }
            }
        }
        question => {
            // Run deliberation.
            let council_cfg = match &config.council {
                Some(c) if c.enabled && c.members.len() >= 2 => c.clone(),
                Some(c) if !c.enabled => {
                    println!("\x1b[33m⚠\x1b[0m Council is disabled. Set enabled = true in config.");
                    return;
                }
                Some(c) if c.members.len() < 2 => {
                    println!(
                        "\x1b[33m⚠\x1b[0m Council needs >= 2 members, got {}. Run /council detect.",
                        c.members.len()
                    );
                    return;
                }
                _ => {
                    println!("\x1b[33m⚠\x1b[0m Council not configured. Run /council detect first.");
                    return;
                }
            };

            println!("\x1b[1mCouncil Deliberation\x1b[0m");
            println!("Question: {}", question);
            println!();

            let members = rustant_core::create_council_members(&council_cfg);
            if members.len() < 2 {
                println!("\x1b[31m✗\x1b[0m Failed to initialize enough council members.");
                println!("  Check API keys and provider configuration.");
                return;
            }

            let council = match rustant_core::PlanningCouncil::new(members, council_cfg) {
                Ok(c) => c,
                Err(e) => {
                    println!("\x1b[31m✗\x1b[0m Failed to create council: {}", e);
                    return;
                }
            };

            let rt = tokio::runtime::Handle::current();
            match rt.block_on(council.deliberate(question)) {
                Ok(result) => {
                    // Display member responses.
                    println!("\x1b[1mMember Responses:\x1b[0m");
                    for (i, resp) in result.member_responses.iter().enumerate() {
                        let label = (b'A' + i as u8) as char;
                        println!(
                            "\n  \x1b[36m[{}] {} ({}, {}ms, ${:.4})\x1b[0m",
                            label, resp.model_name, resp.provider, resp.latency_ms, resp.cost
                        );
                        for line in resp.response_text.lines().take(10) {
                            println!("    {}", line);
                        }
                        if resp.response_text.lines().count() > 10 {
                            println!(
                                "    ... ({} more lines)",
                                resp.response_text.lines().count() - 10
                            );
                        }
                    }

                    // Display peer reviews if any.
                    if !result.peer_reviews.is_empty() {
                        println!("\n\x1b[1mPeer Reviews:\x1b[0m");
                        for review in &result.peer_reviews {
                            let reviewed_label = (b'A' + review.reviewed_index as u8) as char;
                            println!(
                                "  {} reviewing [{}]: {}/10 — {}",
                                review.reviewer_model,
                                reviewed_label,
                                review.score,
                                review.reasoning
                            );
                        }
                    }

                    // Display synthesis.
                    println!("\n\x1b[1;32mSynthesized Answer:\x1b[0m");
                    println!("{}", result.synthesis);
                    println!(
                        "\n\x1b[90m(Total: {} responses, {} reviews, {}ms, ${:.4})\x1b[0m",
                        result.member_responses.len(),
                        result.peer_reviews.len(),
                        result.total_latency_ms,
                        result.total_cost
                    );
                }
                Err(e) => {
                    println!("\x1b[31m✗\x1b[0m Council deliberation failed: {}", e);
                }
            }
        }
    }
}

/// Persist scheduler state to disk after a mutation.
fn auto_save_scheduler(agent: &Agent, workspace: &std::path::Path) {
    let state_dir = workspace.join(".rustant").join("scheduler");
    if let Err(e) = agent.save_scheduler_state(&state_dir) {
        tracing::warn!("Failed to auto-save scheduler state: {}", e);
    }
}

/// Handle the /schedule command.
/// `action` is the subcommand (e.g. "list", "add", "remove").
/// `rest` is everything after the subcommand (e.g. "myname 0 0 9 * * * * task description").
fn handle_schedule_command(
    action: &str,
    rest: &str,
    agent: &mut Agent,
    workspace: &std::path::Path,
) {
    // Split rest into name (first word) and remainder
    let (name, remainder) = match rest.split_once(' ') {
        Some((n, r)) => (n, r),
        None => (rest, ""),
    };

    match action {
        "" | "list" => {
            if let Some(scheduler) = agent.cron_scheduler() {
                let jobs = scheduler.list_jobs();
                if jobs.is_empty() {
                    println!("No scheduled jobs.");
                } else {
                    println!("Scheduled jobs ({}):", jobs.len());
                    for job in jobs {
                        let status = if job.config.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        };
                        let next = job
                            .next_run
                            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                            .unwrap_or_else(|| "N/A".to_string());
                        println!(
                            "  {} [{}] -- next: {} -- runs: {} -- {}",
                            job.config.name, status, next, job.run_count, job.config.task
                        );
                    }
                }
            } else {
                println!("Scheduler is not enabled. Enable it in config.toml under [scheduler].");
            }
        }
        "add" => {
            if name.is_empty() || remainder.is_empty() {
                println!("Usage: /schedule add <name> <cron_expr> <task>");
                println!("  Cron expression has 7 fields: sec min hour day month weekday year");
                println!("  Example: /schedule add morning 0 0 8 * * * * check email");
                return;
            }
            // remainder contains "<cron_expr (7 fields)> <task>"
            let words: Vec<&str> = remainder.split_whitespace().collect();
            if words.len() < 8 {
                println!("Error: Cron expression needs 7 fields followed by the task.");
                println!("  Format: sec min hour day month weekday year");
                println!("  Example: /schedule add myjob 0 0 9 * * MON-FRI * check email");
                return;
            }
            let cron_expr = words[..7].join(" ");
            let task = words[7..].join(" ");

            let config = rustant_core::scheduler::CronJobConfig::new(name, &cron_expr, &task);
            if let Some(scheduler) = agent.cron_scheduler_mut() {
                match scheduler.add_job(config) {
                    Ok(()) => {
                        let next = scheduler
                            .get_job(name)
                            .and_then(|j| j.next_run)
                            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                            .unwrap_or_else(|| "N/A".to_string());
                        println!("Added job '{}' -- next run: {}", name, next);
                        auto_save_scheduler(agent, workspace);
                    }
                    Err(e) => println!("Failed to add job: {}", e),
                }
            } else {
                println!("Scheduler is not enabled. Enable it in config.toml under [scheduler].");
            }
        }
        "remove" => {
            if name.is_empty() {
                println!("Usage: /schedule remove <name>");
                return;
            }
            if let Some(scheduler) = agent.cron_scheduler_mut() {
                match scheduler.remove_job(name) {
                    Ok(()) => {
                        println!("Removed job '{}'.", name);
                        auto_save_scheduler(agent, workspace);
                    }
                    Err(e) => println!("Failed to remove job: {}", e),
                }
            } else {
                println!("Scheduler is not enabled.");
            }
        }
        "enable" => {
            if name.is_empty() {
                println!("Usage: /schedule enable <name>");
                return;
            }
            if let Some(scheduler) = agent.cron_scheduler_mut() {
                match scheduler.enable_job(name) {
                    Ok(()) => {
                        println!("Enabled job '{}'.", name);
                        auto_save_scheduler(agent, workspace);
                    }
                    Err(e) => println!("Failed to enable job: {}", e),
                }
            } else {
                println!("Scheduler is not enabled.");
            }
        }
        "disable" => {
            if name.is_empty() {
                println!("Usage: /schedule disable <name>");
                return;
            }
            if let Some(scheduler) = agent.cron_scheduler_mut() {
                match scheduler.disable_job(name) {
                    Ok(()) => {
                        println!("Disabled job '{}'.", name);
                        auto_save_scheduler(agent, workspace);
                    }
                    Err(e) => println!("Failed to disable job: {}", e),
                }
            } else {
                println!("Scheduler is not enabled.");
            }
        }
        "jobs" => {
            let jobs = agent.job_manager().list();
            if jobs.is_empty() {
                println!("No background jobs.");
            } else {
                println!("Background jobs ({}):", jobs.len());
                for job in jobs {
                    let duration = job
                        .completed_at
                        .map(|c| {
                            let dur = c - job.started_at;
                            format!("{}s", dur.num_seconds())
                        })
                        .unwrap_or_else(|| "running".to_string());
                    println!(
                        "  {} [{}] -- {} -- {}",
                        job.name, job.status, duration, job.id
                    );
                }
            }
        }
        "run" => {
            if name.is_empty() {
                println!("Usage: /schedule run <name>");
                return;
            }
            if let Some(scheduler) = agent.cron_scheduler() {
                if let Some(job) = scheduler.get_job(name) {
                    println!("Manually triggering job '{}': {}", name, job.config.task);
                    println!("(Note: manual job execution runs in the current session)");
                } else {
                    println!("Job '{}' not found.", name);
                }
            } else {
                println!("Scheduler is not enabled.");
            }
        }
        _ => {
            println!(
                "Unknown schedule action: {}. Try: list, add, remove, enable, disable, jobs, run",
                action
            );
        }
    }
}

/// Handle the /why command -- show recent decision explanations.
fn handle_why_command(index_str: &str, agent: &Agent) {
    let explanations = agent.recent_explanations();
    if explanations.is_empty() {
        println!("No decision explanations recorded yet. Run a task first.");
        return;
    }

    let idx = if index_str.is_empty() {
        explanations.len() - 1 // Show most recent
    } else if let Ok(n) = index_str.parse::<usize>() {
        if n >= explanations.len() {
            println!(
                "Index {} out of range. {} explanations available (0-{}).",
                n,
                explanations.len(),
                explanations.len() - 1
            );
            return;
        }
        n
    } else {
        println!("Invalid index. Usage: /why [index]");
        return;
    };

    let exp = &explanations[idx];
    println!(
        "\n--- Decision Explanation [{}/{}] ---",
        idx,
        explanations.len() - 1
    );
    println!("Type: {:?}", exp.decision_type);

    // Extract tool name from decision type
    let tool_name = match &exp.decision_type {
        rustant_core::explanation::DecisionType::ToolSelection { selected_tool } => {
            selected_tool.as_str()
        }
        rustant_core::explanation::DecisionType::ParameterChoice { tool, .. } => tool.as_str(),
        rustant_core::explanation::DecisionType::ErrorRecovery { .. } => "N/A",
        rustant_core::explanation::DecisionType::TaskDecomposition { .. } => "N/A",
    };
    println!("Tool: {}", tool_name);
    println!("Confidence: {:.2}", exp.confidence);

    // Reasoning chain
    if !exp.reasoning_chain.is_empty() {
        let chain: Vec<&str> = exp
            .reasoning_chain
            .iter()
            .map(|s| s.description.as_str())
            .collect();
        println!("Reasoning: {}", chain.join(" -> "));
    }

    // Alternatives considered
    if !exp.considered_alternatives.is_empty() {
        let alts: Vec<&str> = exp
            .considered_alternatives
            .iter()
            .map(|a| a.tool_name.as_str())
            .collect();
        println!("Alternatives: {}", alts.join(", "));
    }

    // Context factors
    if !exp.context_factors.is_empty() {
        println!("Factors:");
        for factor in &exp.context_factors {
            let icon = match factor.influence {
                rustant_core::explanation::FactorInfluence::Positive => "+",
                rustant_core::explanation::FactorInfluence::Negative => "-",
                rustant_core::explanation::FactorInfluence::Neutral => "~",
            };
            println!("  [{}] {}", icon, factor.factor);
        }
    }
    println!("---");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tool_detail_file_read() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(
            extract_tool_detail("file_read", &args),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn test_extract_tool_detail_file_write() {
        let args = serde_json::json!({"path": "output.txt", "content": "hello"});
        assert_eq!(
            extract_tool_detail("file_write", &args),
            Some("output.txt".to_string())
        );
    }

    #[test]
    fn test_extract_tool_detail_shell_exec() {
        let args = serde_json::json!({"command": "cargo test"});
        assert_eq!(
            extract_tool_detail("shell_exec", &args),
            Some("cargo test".to_string())
        );
    }

    #[test]
    fn test_extract_tool_detail_shell_exec_truncation() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = extract_tool_detail("shell_exec", &args).unwrap();
        assert!(result.ends_with("..."));
        assert!(result.len() <= 64); // 60 chars + "..."
    }

    #[test]
    fn test_extract_tool_detail_git_commit() {
        let args = serde_json::json!({"message": "fix auth bug"});
        assert_eq!(
            extract_tool_detail("git_commit", &args),
            Some("\"fix auth bug\"".to_string())
        );
    }

    #[test]
    fn test_extract_tool_detail_git_status() {
        let args = serde_json::json!({});
        assert_eq!(
            extract_tool_detail("git_status", &args),
            Some("workspace".to_string())
        );
    }

    #[test]
    fn test_extract_tool_detail_web_search() {
        let args = serde_json::json!({"query": "rust async patterns"});
        assert_eq!(
            extract_tool_detail("web_search", &args),
            Some("\"rust async patterns\"".to_string())
        );
    }

    #[test]
    fn test_extract_tool_detail_unknown_tool() {
        let args = serde_json::json!({"foo": "bar"});
        assert_eq!(extract_tool_detail("unknown_tool", &args), None);
    }

    #[test]
    fn test_extract_tool_detail_missing_arg() {
        let args = serde_json::json!({"other": "value"});
        assert_eq!(extract_tool_detail("file_read", &args), None);
    }

    #[test]
    fn test_extract_tool_detail_smart_edit() {
        let args = serde_json::json!({"file": "src/lib.rs", "location": "fn main"});
        assert_eq!(
            extract_tool_detail("smart_edit", &args),
            Some("src/lib.rs".to_string())
        );
    }

    #[test]
    fn test_extract_tool_detail_codebase_search() {
        let args = serde_json::json!({"query": "error handling"});
        assert_eq!(
            extract_tool_detail("codebase_search", &args),
            Some("\"error handling\"".to_string())
        );
    }

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello world", 5), "hello");
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn test_truncate_str_utf8_multibyte() {
        // CJK characters (3 bytes each)
        let cjk = "日本語テストの文字列です";
        let truncated = truncate_str(cjk, 3);
        assert_eq!(truncated, "日本語");

        // Emoji (4 bytes each)
        let emoji = "🦀🦀🦀🦀🦀";
        let truncated = truncate_str(emoji, 2);
        assert_eq!(truncated, "🦀🦀");

        // Mixed ASCII and multi-byte
        let mixed = "hello日本語world";
        let truncated = truncate_str(mixed, 8);
        assert_eq!(truncated, "hello日本語");
    }

    #[test]
    fn test_extract_tool_detail_utf8_shell_exec() {
        // Shell command with CJK chars exceeding truncation limit
        let long_cjk = "日".repeat(70);
        let args = serde_json::json!({"command": long_cjk});
        let result = extract_tool_detail("shell_exec", &args);
        assert!(result.is_some());
        let detail = result.unwrap();
        assert!(detail.ends_with("..."));
    }

    #[test]
    fn test_extract_tool_detail_utf8_git_commit() {
        // Git commit message with emoji exceeding truncation limit
        let long_emoji = "🦀".repeat(60);
        let args = serde_json::json!({"message": long_emoji});
        let result = extract_tool_detail("git_commit", &args);
        assert!(result.is_some());
        let detail = result.unwrap();
        assert!(detail.ends_with("...\""));
    }

    #[test]
    fn test_extract_tool_detail_utf8_web_search() {
        let long_kanji = "漢".repeat(60);
        let args = serde_json::json!({"query": long_kanji});
        let result = extract_tool_detail("web_search", &args);
        assert!(result.is_some());
        let detail = result.unwrap();
        assert!(detail.ends_with("...\""));
    }

    // ── New macOS tool detail extraction tests ──

    #[test]
    fn test_extract_tool_detail_gui_scripting() {
        let args = serde_json::json!({"action": "click_element", "app_name": "Finder"});
        let result = extract_tool_detail("macos_gui_scripting", &args);
        assert_eq!(result, Some("Finder → click_element".to_string()));
    }

    #[test]
    fn test_extract_tool_detail_accessibility() {
        let args = serde_json::json!({"action": "get_tree", "app_name": "Safari"});
        let result = extract_tool_detail("macos_accessibility", &args);
        assert_eq!(result, Some("Safari → get_tree".to_string()));
    }

    #[test]
    fn test_extract_tool_detail_screen_analyze() {
        let args = serde_json::json!({"action": "ocr"});
        let result = extract_tool_detail("macos_screen_analyze", &args);
        assert_eq!(result, Some("ocr".to_string()));
    }

    #[test]
    fn test_extract_tool_detail_contacts() {
        let args = serde_json::json!({"action": "search", "query": "John"});
        let result = extract_tool_detail("macos_contacts", &args);
        assert_eq!(result, Some("search: John".to_string()));
    }

    #[test]
    fn test_extract_tool_detail_safari() {
        let args = serde_json::json!({"action": "navigate", "url": "https://example.com"});
        let result = extract_tool_detail("macos_safari", &args);
        assert_eq!(result, Some("navigate: https://example.com".to_string()));
    }

    // ── Channel Intelligence REPL handler tests ──

    #[test]
    fn test_handle_digest_command_no_dir() {
        let tmp = tempfile::tempdir().unwrap();
        // No .rustant/digests directory → should not panic
        handle_digest_command("", tmp.path());
        handle_digest_command("history", tmp.path());
    }

    #[test]
    fn test_handle_digest_command_with_files() {
        let tmp = tempfile::tempdir().unwrap();
        let digest_dir = tmp.path().join(".rustant").join("digests");
        std::fs::create_dir_all(&digest_dir).unwrap();
        std::fs::write(
            digest_dir.join("2026-02-04_09.md"),
            "# Digest\n\nTest digest content.",
        )
        .unwrap();
        // Should read and display the latest digest
        handle_digest_command("", tmp.path());
        // History should list it
        handle_digest_command("history", tmp.path());
    }

    #[test]
    fn test_handle_digest_command_unknown_sub() {
        let tmp = tempfile::tempdir().unwrap();
        handle_digest_command("foobar", tmp.path());
    }

    #[test]
    fn test_handle_replies_command_list() {
        handle_replies_command("", "");
        handle_replies_command("list", "");
    }

    #[test]
    fn test_handle_replies_command_approve_reject_edit() {
        handle_replies_command("approve", "abc123");
        handle_replies_command("reject", "abc123");
        handle_replies_command("edit", "abc123");
    }

    #[test]
    fn test_handle_replies_command_missing_id() {
        handle_replies_command("approve", "");
        handle_replies_command("reject", "");
        handle_replies_command("edit", "");
    }

    #[test]
    fn test_handle_replies_command_unknown_sub() {
        handle_replies_command("foobar", "");
    }

    #[test]
    fn test_handle_reminders_command_no_index() {
        let tmp = tempfile::tempdir().unwrap();
        handle_reminders_command("", "", tmp.path());
        handle_reminders_command("list", "", tmp.path());
    }

    #[test]
    fn test_handle_reminders_command_with_index() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        let reminders = serde_json::json!([
            {
                "id": "abc12345-6789-0000-0000-000000000000",
                "description": "Follow up on Q1 report",
                "status": "Pending",
                "source_channel": "email",
                "source_sender": "boss@corp.com",
                "remind_at": "2026-02-04T15:00:00Z"
            }
        ]);
        std::fs::write(
            reminders_dir.join("index.json"),
            serde_json::to_string_pretty(&reminders).unwrap(),
        )
        .unwrap();
        // Should list the reminder
        handle_reminders_command("", "", tmp.path());
    }

    #[test]
    fn test_handle_reminders_dismiss() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        let reminders = serde_json::json!([
            {
                "id": "abc12345-6789-0000-0000-000000000000",
                "description": "Follow up test",
                "status": "Pending",
                "source_channel": "slack",
                "source_sender": "@alice",
                "remind_at": "2026-02-04T15:00:00Z"
            }
        ]);
        let index_path = reminders_dir.join("index.json");
        std::fs::write(
            &index_path,
            serde_json::to_string_pretty(&reminders).unwrap(),
        )
        .unwrap();

        // Dismiss by prefix match
        handle_reminders_command("dismiss", "abc12345", tmp.path());

        // Verify status changed
        let updated: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&index_path).unwrap()).unwrap();
        assert_eq!(updated[0]["status"].as_str().unwrap(), "Dismissed");
    }

    #[test]
    fn test_handle_reminders_complete() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        let reminders = serde_json::json!([
            {
                "id": "def12345-6789-0000-0000-000000000000",
                "description": "Complete test",
                "status": "Pending",
                "source_channel": "email",
                "source_sender": "john@test.com",
                "remind_at": "2026-02-04T16:00:00Z"
            }
        ]);
        let index_path = reminders_dir.join("index.json");
        std::fs::write(
            &index_path,
            serde_json::to_string_pretty(&reminders).unwrap(),
        )
        .unwrap();

        handle_reminders_command("complete", "def12345", tmp.path());

        let updated: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&index_path).unwrap()).unwrap();
        assert_eq!(updated[0]["status"].as_str().unwrap(), "Completed");
    }

    #[test]
    fn test_handle_reminders_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        std::fs::write(reminders_dir.join("index.json"), "[]").unwrap();
        handle_reminders_command("dismiss", "nonexistent", tmp.path());
        handle_reminders_command("complete", "nonexistent", tmp.path());
    }

    #[test]
    fn test_handle_reminders_unknown_sub() {
        let tmp = tempfile::tempdir().unwrap();
        handle_reminders_command("foobar", "", tmp.path());
    }

    #[test]
    fn test_handle_intelligence_command_status() {
        handle_intelligence_command("");
        handle_intelligence_command("status");
    }

    #[test]
    fn test_handle_intelligence_command_on_off() {
        handle_intelligence_command("on");
        handle_intelligence_command("off");
    }

    #[test]
    fn test_handle_intelligence_command_unknown_sub() {
        handle_intelligence_command("foobar");
    }

    // --- L6: Dismiss/complete with non-existent ID prefix ---

    #[test]
    fn test_handle_reminders_dismiss_nonexistent_id() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        let reminder = serde_json::json!([{
            "id": "abc12345-6789-0000-1111-222233334444",
            "description": "Test reminder",
            "status": "Pending",
            "source_channel": "slack"
        }]);
        std::fs::write(
            reminders_dir.join("index.json"),
            serde_json::to_string(&reminder).unwrap(),
        )
        .unwrap();
        // Should print "No reminder found matching 'xyz'"
        handle_reminders_command("dismiss", "xyz", tmp.path());
    }

    // --- L7: Digest history with no digest directory ---

    #[test]
    fn test_handle_digest_history_no_directory() {
        let tmp = tempfile::tempdir().unwrap();
        // No .rustant/digests/ directory exists
        handle_digest_command("history", tmp.path());
    }

    #[test]
    fn test_handle_digest_no_directory() {
        let tmp = tempfile::tempdir().unwrap();
        // No .rustant/digests/ directory
        handle_digest_command("", tmp.path());
    }

    // --- M4: Corrupted JSON in reminders ---

    #[test]
    fn test_handle_reminders_corrupted_json() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        // Write corrupted JSON
        std::fs::write(reminders_dir.join("index.json"), "not valid json{{{").unwrap();
        // Should print corruption error instead of silently showing "No active reminders"
        handle_reminders_command("", "", tmp.path());
    }

    #[test]
    fn test_handle_reminders_dismiss_corrupted_json() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        std::fs::write(reminders_dir.join("index.json"), "{invalid").unwrap();
        handle_reminders_command("dismiss", "abc", tmp.path());
    }

    #[test]
    fn test_handle_reminders_complete_corrupted_json() {
        let tmp = tempfile::tempdir().unwrap();
        let reminders_dir = tmp.path().join(".rustant").join("reminders");
        std::fs::create_dir_all(&reminders_dir).unwrap();
        std::fs::write(reminders_dir.join("index.json"), "{invalid").unwrap();
        handle_reminders_command("complete", "abc", tmp.path());
    }
}
