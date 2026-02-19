# Architecture Overview

## Crate Structure

```
rustant/
  rustant-core/      — Agent orchestrator, brain, memory, safety, channels, gateway, personas, policy
  rustant-tools/     — 72 built-in tools (45 base + 3 iMessage + 24 macOS native)
  rustant-cli/       — Binary entry point (CLI, REPL, 110+ slash commands)
  rustant-mcp/       — Model Context Protocol server and client
  rustant-plugins/   — Plugin loading and hook system
  rustant-security/  — Security scanning, code review, compliance, incident response (33 tools)
  rustant-ml/        — ML/AI engineering (54 tools)
  rustant-ui/        — Tauri dashboard UI
```

Dependency flow:

```
rustant-cli → rustant-core + rustant-tools + rustant-mcp + rustant-security + rustant-ml
rustant-mcp → rustant-core + rustant-tools + rustant-security
rustant-security → rustant-core + rustant-tools
rustant-ml → rustant-core + rustant-tools
rustant-plugins → rustant-core
```

## Agent Loop

The core of Rustant is the Think-Act-Observe (ReAct) loop in `Agent`:

1. **Think** — Send conversation context to the LLM via `Brain`, receive tool calls or text response. A `DecisionExplanation` is built for each tool call decision, capturing reasoning and confidence. Context health is checked (Warning at 70%, Critical at 90%).
2. **Act** — Execute tool calls through `ToolRegistry`, gated by `SafetyGuardian` approval. Tool arguments are parsed into typed `ActionDetails` to produce rich `ApprovalContext` with reasoning, alternatives, consequences, and reversibility info. Budget warnings with per-tool breakdown.
3. **Observe** — Feed tool results back into memory. Successful results (10-5000 chars) are recorded as `Fact` entries in long-term memory for cross-session learning. User denials are recorded as `Correction` entries. Auto-compression checked after each observation.

### Auto-Routing

`auto_correct_tool_call()` reroutes wrong tools based on task context (e.g., `document_read` → `macos_clipboard`). `tool_routing_hint()` and `workflow_routing_hint()` inject system messages guiding the LLM to correct tools and workflows.

## Brain / LLM Providers

`Brain` abstracts LLM interaction behind the `LlmProvider` trait:

- **OpenAI-compatible** — Also covers Azure OpenAI, Ollama, vLLM
- **Anthropic** — Claude models with prompt caching (90% read discount)
- **Gemini** — Google's models with `fix_gemini_turns()` post-processing, 120s timeout, incremental SSE streaming
- **FailoverProvider** — Circuit-breaker failover across multiple providers
- **PlanningCouncil** — Multi-model deliberation: parallel query, anonymous peer review, chairman synthesis

Token counting uses tiktoken-rs. Centralized model pricing in `models.rs`. Prompt caching support for Anthropic, OpenAI, and Gemini with provider-specific discount rates.

Retry: `RetryConfig` with exponential backoff + jitter. 3 retries, 1s initial, 2x multiplier, 60s max.

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

**159+ tools** across categories: core (17), productivity (11), research (1 with 22 actions), cognitive extension (10), macOS native (24), iMessage (3), SRE/DevOps (5), fullstack (5), security (33), ML (54).

**Dual registration**: `ToolRegistry` (rustant-tools) holds all tools; `Agent` (rustant-core) has its own `HashMap<String, RegisteredTool>`. Bridged via `register_agent_tools_from_registry()` using `Arc<ToolRegistry>` as fallback.

## Safety & Trust

Five-layer defense: input validation, authorization, sandboxing, output validation, audit trail.

- **Approval modes**: Safe (default), Cautious, Paranoid, Yolo
- **Progressive trust**: Shadow → DryRun → Assisted → Supervised → SelectiveAutonomy
- **Circuit breaker**: Sliding-window failure detection (Closed/Open/HalfOpen)
- **Policy engine**: `.rustant/policies.toml` with predicates and tool scoping
- **Rollback registry**: Tracks reversible actions with undo info

See [Safety & Trust](safety.md) for details.

## Memory System

Three tiers: working (context window), short-term (sliding window + pinning), long-term (persistent facts/corrections). Capacity: max_facts=10,000, max_corrections=1,000 with FIFO eviction.

See [Memory System](memory.md) for details.

## Personas

8 adaptive personas: Architect, SecurityGuardian, MlopsEngineer, General, IncidentCommander, ObservabilityExpert, ReliabilityEngineer, DeploymentEngineer. Auto-detect from task, manual override via `/persona set`. Evolution based on task history.

## Codebase Intelligence

- **AST Engine** — Tree-sitter parsing with feature-gated grammars (Rust, Python, JS/TS, Go, Java) + regex fallback
- **RepoMap** — `CodeGraph` with PageRank for context-aware file ranking
- **Hydration** — Token-budgeted context assembly combining RepoMap ranking
- **Verification** — Automated test/lint/build verification after code changes

## Other Subsystems

- **Gateway** — WebSocket server (axum) for remote access with TLS and REST API
- **Multi-Agent** — Agent spawning, `MessageBus`, `AgentRouter`, `AgentOrchestrator`, `ResourceLimits`
- **MCP** — JSON-RPC 2.0 with stdio/channel/process transports
- **Session Manager** — Persistent sessions with auto-save, resume, search, tagging
- **Project Detection** — Auto-detect language/framework/CI, generate safety whitelists
- **Project Indexer** — `.gitignore`-aware walking, multi-language signature extraction, incremental re-indexing
- **Anomaly Detection** — ZScore, IQR, MovingAverage methods for metric analysis
- **Evaluation** — `TraceEvaluator` with loop detection, safety false positive, cost efficiency evaluators
