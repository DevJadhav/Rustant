# Architecture Overview

## Crate Structure

```
rustant/
  rustant-core/    — Core library (agent, brain, memory, safety, channels, gateway)
  rustant-tools/   — Built-in tool implementations
  rustant-cli/     — Binary entry point (CLI, REPL, TUI)
  rustant-mcp/     — Model Context Protocol server and client
  rustant-plugins/ — Plugin loading and hook system
  rustant-ui/      — Tauri dashboard UI
```

Dependency flow: `rustant-cli` depends on all other crates. `rustant-mcp` depends on `rustant-tools`. `rustant-plugins` depends on `rustant-core`.

## Agent Loop

The core of Rustant is the Think-Act-Observe (ReAct) loop in `Agent`:

1. **Think** — Send conversation context to the LLM via `Brain`, receive tool calls or text response
2. **Act** — Execute tool calls through `ToolRegistry`, gated by `SafetyGuardian` approval
3. **Observe** — Feed tool results back into memory, repeat until task complete or max iterations

## Brain / LLM Providers

`Brain` abstracts LLM interaction behind the `LlmProvider` trait. Implementations:

- **OpenAI-compatible** — Also covers Azure OpenAI, Ollama, vLLM
- **Anthropic** — Claude models
- **Gemini** — Google's Gemini models
- **FailoverProvider** — Circuit-breaker failover across multiple providers

Token counting uses tiktoken-rs for accurate context window management.

## Tool System

Tools implement the `Tool` trait:

```text
trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError>;
    fn risk_level(&self) -> RiskLevel;
    fn timeout(&self) -> Duration;
}
```

12 built-in tools: `file_read`, `file_list`, `file_search`, `file_write`, `file_patch`, `git_status`, `git_diff`, `git_commit`, `shell_exec`, `echo`, `datetime`, `calculator`.

The `ToolRegistry` handles registration, lookup, and invocation with configurable timeouts.

## Gateway

The WebSocket gateway (built on axum) enables remote access:

- Task submission and status tracking
- Real-time event streaming
- Channel message bridging
- Node coordination for multi-agent setups
- REST API endpoints for dashboard integration

## Multi-Agent

The multi-agent system supports:

- Agent spawning with parent-child relationships
- `MessageBus` for inter-agent communication
- `AgentRouter` for message routing
- `AgentOrchestrator` for lifecycle management
- `ResourceLimits` for isolation between agents
