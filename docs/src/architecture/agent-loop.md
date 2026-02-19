# Agent Loop (ReAct)

The agent loop implements the Think-Act-Observe (ReAct) pattern for autonomous task execution.

## Overview

```
User Task → [Think → Act → Observe] × N → Result
```

The loop runs up to `max_iterations` (default: 50) or until the LLM produces a text response (no tool calls).

## Think Phase

1. **Context Assembly** — Working memory (system prompt + conversation + tool results) is sent to the `Brain`
2. **LLM Inference** — The provider returns either tool calls or a text response
3. **Decision Explanation** — Each tool call generates a `DecisionExplanation` with reasoning, confidence, and alternatives
4. **Context Health** — Checked at Warning (70%) and Critical (90%) thresholds, emitting hints to compress or pin messages

### Auto-Routing

Before the LLM sees the context, routing hints are injected:

- **`tool_routing_hint()`** — Platform-specific hints guiding the LLM to correct macOS tools
- **`workflow_routing_hint()`** — Matches task patterns to 39+ workflow templates
- **`auto_correct_tool_call()`** — Post-LLM correction, reroutes mismatched tools (e.g., `document_read` → `macos_clipboard`)

Auto-routing is duplicated in both single `ToolCall` and `MultiPart` code paths. System messages are never injected between `tool_call` and `tool_result` (breaks OpenAI sequencing).

## Act Phase

1. **Safety Check** — `SafetyGuardian` evaluates the `ActionDetails` and produces an `ApprovalContext`
2. **`ask_user` Interception** — The `ask_user` pseudo-tool is intercepted before safety, triggering `on_clarification_request`
3. **Tool Execution** — `ToolRegistry` dispatches with configurable timeout (default: 60s)
4. **Budget Tracking** — Per-tool token usage tracked in `tool_token_usage: HashMap`, warnings emitted at thresholds

### ActionDetails

Tool arguments are parsed into typed variants via `parse_action_details()`:

- `FileRead`, `FileWrite`, `FileDelete` — File operations
- `ShellCommand` — Shell execution
- `GitOperation` — Git commands
- `BrowserAction` — Browser automation
- `NetworkRequest` — HTTP requests
- `SecurityScan`, `VulnerabilityLookup`, `ComplianceCheck` — Security operations
- `ModelInference`, `ModelTraining`, `DataPipeline` — ML operations
- `GuiAction` — macOS GUI scripting
- And more...

Each variant produces a rich `ApprovalContext` with `with_preview_from_tool()` for showing diffs, command previews, etc.

## Observe Phase

1. **Memory Recording** — Tool results fed into working memory
2. **Fact Extraction** — Successful results (10-5000 chars) → `Fact` in long-term memory
3. **Correction Recording** — User denials → `Correction` for behavioral learning
4. **Knowledge Distillation** — `KnowledgeDistiller` processes facts/corrections → behavioral rules via `Brain.set_knowledge_addendum()`
5. **Auto-Compression** — Checked after each observation; triggers when messages exceed 2x window_size

## Iteration Budget

- Default: 50 iterations
- Budget warnings emitted with per-tool breakdown showing top consumers
- `BudgetSeverity::Warning` at configurable threshold
- `BudgetSeverity::Exceeded` when max_iterations reached

## Multi-Tool Calls

When the LLM returns multiple tool calls in a single response (MultiPart), each is processed sequentially with the same safety checks and auto-routing logic.
