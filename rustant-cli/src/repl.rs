//! REPL (Read-Eval-Print Loop) for interactive and single-task modes.

use rustant_core::safety::ActionRequest;
use rustant_core::types::{AgentStatus, RiskLevel, ToolOutput};
use rustant_core::{Agent, AgentCallback, AgentConfig, MockLlmProvider, RegisteredTool};
use rustant_tools::register_builtin_tools;
use rustant_tools::registry::ToolRegistry;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A CLI callback that prints to stdout and reads approval from stdin.
struct CliCallback;

#[async_trait::async_trait]
impl AgentCallback for CliCallback {
    async fn on_assistant_message(&self, message: &str) {
        println!("\n\x1b[32mRustant:\x1b[0m {}", message);
    }

    async fn on_token(&self, token: &str) {
        print!("{}", token);
        let _ = io::stdout().flush();
    }

    async fn request_approval(&self, action: &ActionRequest) -> bool {
        println!(
            "\n\x1b[33m[Approval Required]\x1b[0m {} (risk: {})",
            action.description, action.risk_level
        );
        print!("  [y]es / [n]o > ");
        let _ = io::stdout().flush();

        let stdin = io::stdin();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_ok() {
            let answer = line.trim().to_lowercase();
            matches!(answer.as_str(), "y" | "yes")
        } else {
            false
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

    let provider = match rustant_core::create_provider(&config.llm) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("LLM provider init failed: {}. Using mock.", e);
            Arc::new(MockLlmProvider::new())
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
            match input {
                "/quit" | "/exit" | "/q" => {
                    println!("Goodbye!");
                    break;
                }
                "/help" | "/?" => {
                    print_help();
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
                _ => {
                    println!(
                        "Unknown command: {}. Type /help for available commands.",
                        input
                    );
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
    let provider = match rustant_core::create_provider(&config.llm) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("LLM provider init failed: {}. Using mock.", e);
            Arc::new(MockLlmProvider::new())
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

fn print_help() {
    println!(
        r#"
Available commands:
  /help, /?     Show this help message
  /quit, /exit  Exit Rustant
  /clear        Clear the screen
  /cost         Show token usage and cost
  /tools        List available tools

Input:
  Type your task or question and press Enter.
  Use @path to reference files (future: autocomplete).
"#
    );
}
