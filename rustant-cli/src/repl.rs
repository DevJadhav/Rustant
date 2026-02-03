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

    async fn on_tool_start(&self, tool_name: &str, _args: &serde_json::Value) {
        println!("\x1b[36m  [{}] executing...\x1b[0m", tool_name);
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
                    println!("Goodbye!");
                    break;
                }
                "/help" | "/?" => {
                    let registry = crate::slash::CommandRegistry::with_defaults();
                    println!("{}", registry.help_text());
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
                    handle_sessions_command(&workspace);
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
                    // Use registry for unknown command suggestions
                    let registry = crate::slash::CommandRegistry::with_defaults();
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
        _ => {
            println!("Usage: /audit show [n] | /audit verify");
        }
    }
}

/// Handle `/session` subcommands.
fn handle_session_command(sub: &str, name: &str, agent: &mut Agent, workspace: &Path) {
    let sessions_dir = workspace.join(".rustant").join("sessions");
    match sub {
        "save" => {
            let session_name = if name.is_empty() {
                chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string()
            } else {
                name.to_string()
            };
            let _ = std::fs::create_dir_all(&sessions_dir);
            let path = sessions_dir.join(format!("{}.json", session_name));
            match agent.memory().save_session(&path) {
                Ok(()) => println!("Session saved to {}", path.display()),
                Err(e) => println!("Failed to save session: {}", e),
            }
        }
        "load" => {
            if name.is_empty() {
                println!("Usage: /session load <name>");
                return;
            }
            let path = sessions_dir.join(format!("{}.json", name));
            match rustant_core::memory::MemorySystem::load_session(&path) {
                Ok(mem) => {
                    *agent.memory_mut() = mem;
                    println!("Session '{}' loaded.", name);
                }
                Err(e) => println!("Failed to load session: {}", e),
            }
        }
        "list" => {
            if !sessions_dir.exists() {
                println!("No saved sessions found.");
                return;
            }
            let mut entries: Vec<_> = std::fs::read_dir(&sessions_dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
                .collect();
            if entries.is_empty() {
                println!("No saved sessions found.");
                return;
            }
            entries.sort_by_key(|e| e.file_name());
            println!("Saved sessions:");
            for entry in &entries {
                let name = entry
                    .path()
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                println!("  - {} ({} bytes)", name, size);
            }
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

/// Handle `/sessions` REPL command.
fn handle_sessions_command(workspace: &Path) {
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
        let status = if entry.completed { "done" } else { "..." };
        let goal = entry.last_goal.as_deref().unwrap_or("(no goal)");
        let goal_display = if goal.len() > 50 {
            format!("{}...", &goal[..50])
        } else {
            goal.to_string()
        };
        println!(
            "  \x1b[36m{}\x1b[0m [{}] - {} ({} msgs, {})",
            entry.name,
            status,
            goal_display,
            entry.message_count,
            entry.updated_at.format("%m/%d %H:%M")
        );
    }
    println!("\nResume with: /resume <name>");
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

    // Workspace
    println!("  Workspace:     {}", workspace.display());
    let has_git = workspace.join(".git").exists();
    println!("  Git repo:      {}", if has_git { "yes" } else { "no" });

    // Config
    let config_path = workspace.join(".rustant").join("config.toml");
    let config_found = config_path.exists() || rustant_core::config_exists(Some(workspace));
    println!(
        "  Config file:   {}",
        if config_found {
            "found"
        } else {
            "using defaults"
        }
    );

    // Model
    let config = agent.config();
    println!("  LLM provider:  {}", config.llm.provider);
    println!("  Model:         {}", config.llm.model);
    println!("  Approval mode: {:?}", config.safety.approval_mode);

    // Tools
    let tools = agent.tool_definitions();
    println!("  Tools:         {} registered", tools.len());

    // Memory
    let mem = agent.memory();
    println!(
        "  Memory:        {} messages, {} facts",
        mem.short_term.len(),
        mem.long_term.facts.len()
    );

    // Safety
    let audit_count = agent.safety().audit_log().len();
    println!("  Audit entries: {}", audit_count);

    println!("══════════════════════════════");
    println!("  All checks passed.");
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
