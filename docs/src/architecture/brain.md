# Brain & LLM Providers

The `Brain` abstracts all LLM interaction behind the `LlmProvider` trait.

## Providers

| Provider | Auth | Models | Notes |
|----------|------|--------|-------|
| **OpenAI** | API Key, OAuth | GPT-4o, GPT-4, GPT-3.5 | Automatic prompt caching (50% read discount) |
| **Anthropic** | API Key | Claude 3.5/4 Sonnet, Opus, Haiku | Explicit prompt caching (90% read, 25% write premium) |
| **Gemini** | API Key, OAuth | Gemini Pro, Flash | 75% cache read discount. Requires `fix_gemini_turns()` |
| **Azure OpenAI** | API Key | OpenAI models via Azure | Same as OpenAI with Azure endpoints |
| **Ollama** | None | Any local model | First-class support, auto-discovery, no API key |
| **vLLM** | None | Self-hosted | OpenAI-compatible API |

### FailoverProvider

Circuit-breaker failover across multiple providers. Configurable primary + fallbacks. Automatic switching on repeated failures.

### Ollama Integration

- `is_ollama_available()` checks localhost connectivity
- `list_ollama_models()` discovers installed models
- Setup wizard detects Ollama, lists models, lets user select
- Uses `"ollama"` as dummy bearer token

## LLM Council

Multi-model deliberation for planning tasks:

1. **Parallel Query** — All council members receive the same prompt simultaneously
2. **Peer Review** — Anonymous labels ("Response A/B/C") prevent model-name bias. Skipped with <3 members
3. **Chairman Synthesis** — Final synthesis incorporating all perspectives

`should_use_council()` heuristic: planning keywords → true, concrete actions → false.

### Voting Strategies

- `ChairmanSynthesis` (default) — Chairman produces unified response
- `HighestScore` — Best-scored individual response wins
- `MajorityConsensus` — Majority agreement required

### Remediation Council

Wraps `PlanningCouncil` for SRE incident remediation. Configurable consensus threshold (default 0.85). Unanimity required for destructive actions.

## Prompt Caching

Provider-level cache support via `supports_caching()` and `cache_cost_per_token()`:

| Provider | Read Discount | Write Premium | Mechanism |
|----------|--------------|---------------|-----------|
| Anthropic | 90% | 25% | `cache_control: {"type": "ephemeral"}` on system + last tool |
| OpenAI | 50% | None | Automatic, parses `prompt_tokens_details.cached_tokens` |
| Gemini | 75% | None | Parses `cachedContentTokenCount` from `usageMetadata` |

`TokenUsage` extended with `cache_read_tokens` and `cache_creation_tokens`.

## Gemini Quirks

`fix_gemini_turns()` runs 4 passes before API calls:

1. Merge consecutive same-role turns (multiple tool results create consecutive "user" turns)
2. Fix `functionResponse.name` to match preceding `functionCall.name` (was hardcoded `"tool"`)
3. Filter empty `parts` arrays
4. Ensure first message is "user" role

Additional: `function_response.response` must be a JSON object — non-object values wrapped in `{"result": value}`.

HTTP client: 120s timeout + 10s connect timeout. Streaming via `bytes_stream()` SSE with incremental parsing.

## Retry Logic

`RetryConfig` with exponential backoff + jitter:

| Setting | Default | Description |
|---------|---------|-------------|
| `max_retries` | 3 | Maximum retry attempts |
| `initial_backoff_ms` | 1000 | Initial backoff delay |
| `max_backoff_ms` | 60000 | Maximum backoff delay |
| `backoff_multiplier` | 2.0 | Exponential multiplier |
| `jitter` | true | Randomized jitter |

Retryable: 429 rate limited, timeouts, connection failures, streaming errors.
Non-retryable: auth failures, parse errors (fail immediately).

## Token Counting & Pricing

Token counting via `tiktoken-rs` for accurate context window management. Centralized `model_pricing()` in `models.rs` covers all supported models.

## Credential Resolution

`resolve_api_key_by_env(env_var)` centralizes API key lookup: checks OS keychain first, then environment variable. Used by voice, embeddings, and meeting tools.
