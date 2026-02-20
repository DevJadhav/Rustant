# MoE & TTFT Optimization

The Mixture-of-Experts (MoE) architecture and associated optimizations reduce time-to-first-token (TTFT) and per-request token overhead.

## MoE Architecture

The MoE router dispatches tasks to 10 specialized expert agents, each with a focused toolset (10-25 tools) instead of the full 72+. This cuts per-request tool tokens from ~25K to ~3K.

### Experts

| Expert | Domain | Key Tools |
|--------|--------|-----------|
| System | General operations | file_*, shell_exec, git_*, echo, datetime |
| MacOS | macOS native | macos_*, imessage_*, homekit, siri |
| ScreenAutomation | UI automation | gui_scripting, accessibility, screen_analyze, safari |
| WebBrowser | Web operations | web_*, browser_*, http_api |
| DevOps | CI/CD, deployment | shell_exec, git_*, deployment_intel, kubernetes |
| Productivity | Personal tools | pomodoro, inbox, life_planner, finance, travel |
| Security | Security scanning | All 33 security tools |
| ML | AI/ML engineering | All 54 ML tools |
| SRE | Site reliability | alert_manager, prometheus, kubernetes, oncall |
| Research | Deep research | arxiv, web_search, web_fetch, knowledge_graph |

### Routing

`TaskClassification::classify()` uses keyword heuristics to classify tasks, then `ExpertId::from_classification()` maps to the appropriate expert. An LRU classification cache (default 256 entries) avoids re-classification of similar tasks.

## TTFT Optimizations

### 1. Tool Definition Warmup

`ToolDefinitionCache` pre-computes `Vec<ToolDefinition>` for all 10 experts during `Agent::new()`:

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
