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

1. **Think** — Send conversation context to the LLM via `Brain`, receive tool calls or text response. A `DecisionExplanation` is built for each tool call decision, capturing reasoning and confidence.
2. **Act** — Execute tool calls through `ToolRegistry`, gated by `SafetyGuardian` approval. Tool arguments are parsed into typed `ActionDetails` (FileRead, FileWrite, ShellCommand, GitOperation) to produce rich `ApprovalContext` with reasoning, alternatives, consequences, and reversibility info. Budget checks emit user-facing warnings via `BudgetSeverity::Warning`/`Exceeded`. Safety denials and contract violations also produce `DecisionExplanation` entries.
3. **Observe** — Feed tool results back into memory. Successful tool results (10-5000 chars) are recorded as `Fact` entries in long-term memory for cross-session learning. User denials are recorded as `Correction` entries. Repeat until task complete or max iterations.

## Decision Transparency

Every significant action point in the agent loop emits a `DecisionExplanation` via the `AgentCallback` interface:

- **Tool calls** (single and multipart) — reasoning about which tool to use and why
- **Safety denials** — explanation of why a tool was blocked by the safety guardian
- **User denials** — records the user's decision to deny a proposed action
- **Contract violations** — explanation when a safety contract invariant is violated

Budget tracking surfaces real-time cost information to users through `BudgetSeverity` events (Warning and Exceeded), displayed in both CLI (colored terminal output) and TUI interfaces.

## Brain / LLM Providers

`Brain` abstracts LLM interaction behind the `LlmProvider` trait. Implementations:

- **OpenAI-compatible** — Also covers Azure OpenAI, Ollama, vLLM
- **Anthropic** — Claude models
- **Gemini** — Google's Gemini models (includes `fix_gemini_turns()` post-processing for API sequencing: merges consecutive same-role turns, fixes functionResponse names, filters empty parts, ensures user-first ordering). Uses 120s request timeout, 10s connect timeout, and true incremental SSE streaming.
- **FailoverProvider** — Circuit-breaker failover across multiple providers
- **PlanningCouncil** — Multi-model deliberation (inspired by [karpathy/llm-council](https://github.com/karpathy/llm-council)): three-stage protocol with parallel query, anonymous peer review, and chairman synthesis. Supports 2+ providers (cloud or Ollama).

Token counting uses tiktoken-rs for accurate context window management. Centralized model pricing in `models.rs` covers OpenAI, Anthropic, Gemini, and Ollama models.

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

39 built-in tools across 6 categories: core (file_read, file_list, file_search, file_write, file_patch, git_status, git_diff, git_commit, shell_exec, echo, datetime, calculator, web_search, web_fetch, document_read, smart_edit, codebase_search), productivity (organizer, compress, http_api, template, pdf, pomodoro, inbox, relationships, finance, flashcards, travel), research (arxiv_research), and cognitive extension (knowledge_graph, experiment_tracker, code_intelligence, content_engine, skill_tracker, career_intel, system_monitor, life_planner, privacy_manager, self_improvement).

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
