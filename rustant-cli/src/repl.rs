//! REPL (Read-Eval-Print Loop) for interactive and single-task modes.

use rustant_core::explanation::DecisionExplanation;
use rustant_core::safety::{ActionRequest, ApprovalDecision};
use rustant_core::types::{AgentStatus, CostEstimate, RiskLevel, TokenUsage, ToolOutput};
use rustant_core::{Agent, AgentCallback, AgentConfig, MockLlmProvider, RegisteredTool};
use rustant_tools::register_builtin_tools;
use rustant_tools::registry::ToolRegistry;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
            if s.len() > 60 {
                format!("{}...", &s[..60])
            } else {
                s.to_string()
            }
        }),
        "git_status" | "git_diff" => Some("workspace".to_string()),
        "git_commit" => args.get("message").and_then(|v| v.as_str()).map(|s| {
            if s.len() > 50 {
                format!("\"{}...\"", &s[..50])
            } else {
                format!("\"{}\"", s)
            }
        }),
        "web_search" => args.get("query").and_then(|v| v.as_str()).map(|s| {
            if s.len() > 50 {
                format!("\"{}...\"", &s[..50])
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
            if s.len() > 50 {
                format!("\"{}...\"", &s[..50])
            } else {
                format!("\"{}\"", s)
            }
        }),
        _ => None,
    }
}

/// A CLI callback that prints to stdout and reads approval from stdin.
pub(crate) struct CliCallback;

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
        let detail = extract_tool_detail(tool_name, args);
        if let Some(ref detail) = detail {
            println!("\x1b[36m  [{}: {}] executing...\x1b[0m", tool_name, detail);
        } else {
            println!("\x1b[36m  [{}] executing...\x1b[0m", tool_name);
        }
    }

    async fn on_tool_result(&self, tool_name: &str, output: &ToolOutput, duration_ms: u64) {
        let preview = if output.content.len() > 200 {
            format!("{}...", &output.content[..200])
        } else {
            output.content.clone()
        };
        println!(
            "\x1b[36m  [{}] completed in {}ms\x1b[0m\n  {}",
            tool_name, duration_ms, preview
        );
    }

    async fn on_status_change(&self, status: AgentStatus) {
        match status {
            AgentStatus::Thinking => print!("\x1b[90m  thinking...\x1b[0m"),
            AgentStatus::Executing => {}
            AgentStatus::Complete => println!("\x1b[90m  done.\x1b[0m"),
            _ => {}
        }
        let _ = io::stdout().flush();
    }

    async fn on_usage_update(&self, usage: &TokenUsage, cost: &CostEstimate) {
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
            } => {
                println!(
                    "\x1b[33m  [Context: {}% used ({}/{})] Consider using /pin for important messages\x1b[0m",
                    usage_percent, total_tokens, context_window
                );
            }
            rustant_core::ContextHealthEvent::Critical {
                usage_percent,
                total_tokens,
                context_window,
            } => {
                println!(
                    "\x1b[31m  [Context: {}% used ({}/{})] Use /pin for critical messages or /compact to compress now\x1b[0m",
                    usage_percent, total_tokens, context_window
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
    let callback = Arc::new(CliCallback);
    let mut agent = Agent::new(provider, config, callback);

    // Register built-in tools as agent tools
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry, workspace.clone());
    register_agent_tools_from_registry(&mut agent, &registry, &workspace);

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

    let stdin = io::stdin();
    loop {
        print!("\x1b[1;34m> \x1b[0m");
        io::stdout().flush()?;

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() || input.is_empty() {
            break;
        }

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
                        if stdin.lock().read_line(&mut save_input).is_ok() {
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
                    println!("Goodbye!");
                    break;
                }
                "/help" | "/?" => {
                    let registry = crate::slash::CommandRegistry::with_defaults();
                    if !arg1.is_empty() {
                        // Topic-specific help
                        match registry.help_for(arg1) {
                            Some(help) => println!("\n{}", help),
                            None => {
                                println!(
                                    "No help found for '{}'. Try /help for all commands.",
                                    arg1
                                );
                                if let Some(suggestion) = registry.suggest(&format!("/{}", arg1)) {
                                    println!("Did you mean: {} ?", suggestion);
                                }
                            }
                        }
                    } else {
                        println!("{}", registry.help_text());
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
                _ => {
                    let registry = crate::slash::CommandRegistry::with_defaults();
                    // Check if this is a TUI-only command
                    if let Some(info) = registry.lookup(cmd) {
                        if info.tui_only {
                            println!(
                                "The {} command is only available in TUI mode. Launch with: rustant (without --no-tui)",
                                cmd
                            );
                            continue;
                        }
                    }
                    // Use registry for unknown command suggestions
                    if let Some(suggestion) = registry.suggest(cmd) {
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
    let callback = Arc::new(CliCallback);
    let mut agent = Agent::new(provider, config, callback);

    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry, workspace.clone());
    register_agent_tools_from_registry(&mut agent, &registry, &workspace);

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
    // We re-create the tools since the agent uses a different tool registration model.
    // In Phase 1+, this will be unified.
    let tool_defs = registry.list_definitions();
    for def in tool_defs {
        let name = def.name.clone();
        let ws = workspace.to_path_buf();
        let registry_clone = create_tool_executor(&name, &ws);
        if let Some(executor) = registry_clone {
            agent.register_tool(RegisteredTool {
                definition: def,
                risk_level: tool_risk_level(&name),
                executor,
            });
        }
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
        _ => None,
    }
}

/// Get the risk level for a tool by name.
fn tool_risk_level(name: &str) -> RiskLevel {
    match name {
        "file_read" | "file_list" | "file_search" | "git_status" | "git_diff" | "echo"
        | "datetime" | "calculator" => RiskLevel::ReadOnly,
        "file_write" | "file_patch" | "git_commit" => RiskLevel::Write,
        "shell_exec" => RiskLevel::Execute,
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
                                if text.len() > 60 {
                                    format!("{}...", &text[..60])
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
    let goal_display = if goal.len() > 50 {
        format!("{}...", &goal[..50])
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
}
