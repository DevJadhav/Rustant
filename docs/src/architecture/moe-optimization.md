# MoE & TTFT Optimization

The Mixture-of-Experts (MoE) architecture and associated optimizations reduce time-to-first-token (TTFT) and per-request token overhead.

## MoE Architecture

The MoE router dispatches tasks to 20 specialized expert agents (DeepSeek V3-inspired), each with a focused toolset (5-12 tools) instead of the full 73+. This cuts per-request tool tokens from ~25K to ~3K. 8 shared tools are always sent regardless of routing.

### Experts (20 Fine-Grained)

| Expert | Domain | Max Tools |
|--------|--------|-----------|
| **FileOps** | File read/write/search/organize | ~8 |
| **Git** | Git operations, codebase ops | ~5 |
| **MacOSApps** | Calendar, reminders, notes, mail, music, shortcuts | ~10 |
| **MacOSSystem** | App control, clipboard, screenshot, finder, spotlight | ~8 |
| **ScreenUI** | GUI scripting, accessibility, OCR, contacts, safari | ~6 |
| **Communication** | iMessage, Slack, Siri | ~5 |
| **WebBrowse** | Web search/fetch, browser, arXiv | ~8 |
| **DevTools** | Scaffold, dev server, database, test runner, lint | ~6 |
| **Productivity** | Knowledge graph, career, life planning, skills, etc. | ~10 |
| **SecScan** | SAST, SCA, secrets, container, IaC scanning | ~9 |
| **SecReview** | Code review, quality, dead code, tech debt | ~9 |
| **SecCompliance** | License, SBOM, policy, risk, audit | ~8 |
| **SecIncident** | Threat detection, alerts, incidents, log analysis | ~7 |
| **MLTrain** | Experiment, finetune, checkpoint, quantize | ~10 |
| **MLData** | Data source, schema, transform, features | ~9 |
| **MLInference** | RAG, inference backends (Ollama, vLLM, llama.cpp) | ~8 |
| **MLSafety** | PII, bias, alignment, adversarial testing | ~8 |
| **MLResearch** | Research, evaluation, explainability | ~9 |
| **SRE** | Alerts, deployments, Prometheus, Kubernetes, oncall | ~6 |
| **Research** | Deep research, arXiv, document analysis, knowledge | ~5 |

**Shared always-on tools (8):** file_read, codebase_search, shell_exec, git_status, git_diff, git_commit, smart_edit, echo.

C(20,3) = 1,140 possible Top-3 routings vs only 10 with single-expert routing.

### Routing

`TaskClassification::classify()` uses keyword heuristics to classify tasks, then `ExpertId::from_classification()` maps to the appropriate expert. An LRU classification cache (default 256 entries) avoids re-classification of similar tasks.

## TTFT Optimizations

### 1. Tool Definition Warmup

`ToolDefinitionCache` pre-computes `Vec<ToolDefinition>` for all 20 experts during `Agent::new()`:

- Pre-serialized JSON bytes per expert
- Token count estimation
- Eliminates per-request tool definition rebuilding (~10-20ms savings)

### 2. Speculative Prefetching

`SpeculativePrefetcher` tracks expert-to-expert transition patterns and pre-warms the top-2 likely next experts while the current LLM request is in-flight.

### 3. Tool Schema Compression

`compress_tool_schema()` strips non-essential fields from tool JSON schemas:

- Removes descriptions from nested object properties
- Strips default values and examples
- Preserves type, enum, and required constraints
- 15-25% schema token reduction per tool

### 4. Adaptive Prompt Compression

`PromptOptimizer` activates when system_prompt + tools + history exceeds 70% of context window:

- `dedup_addenda()` removes redundant instructions using sentence-level Jaccard similarity (>0.75 threshold)
- `truncate_to_budget()` cuts at sentence boundaries
- Distributes token budget proportionally across addenda

### 5. Conditional Tool Pruning

After N iterations (default 5, configurable via `prune_after_iterations`):

- Tools with zero invocations are dropped from the active set
- Core meta-tools (echo, datetime, calculator, file_read, file_write) are never pruned
- Reduces expert tool set from 15-25 to 8-12 in long-running sessions

### 6. Provider Connection Warmup

`warm_provider_connections()` pre-establishes HTTP connection pools during startup:

- HEAD requests to Anthropic, OpenAI, Gemini API endpoints
- Eliminates first-request TCP/TLS handshake latency (~50-150ms)
- Skips Ollama (localhost, no TLS needed)

## Configuration

```toml
[moe]
enabled = true
classification_cache_size = 256
fallback_expert = "system"
prune_after_iterations = 5     # 0 = disabled
warm_on_startup = true
speculative_prefetch = true
compress_schemas = true
```

## Monitoring

- `RouterStats` tracks classification hit rates and token savings
- `/status` shows current expert and tool count
- Agent debug logs show per-request routing decisions
