# Configuration Reference

Rustant uses layered configuration via [figment](https://docs.rs/figment). Configuration is loaded from multiple sources in priority order:

1. CLI flags (highest priority)
2. Environment variables (`RUSTANT_` prefix with `__` nesting)
3. Workspace config (`.rustant/config.toml`)
4. User config (`~/.config/rustant/config.toml`)
5. Built-in defaults (lowest priority)

## `[llm]` -- LLM Provider

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `provider` | string | `"openai"` | Provider: `openai`, `anthropic`, `gemini`, `ollama` |
| `model` | string | `"gpt-4o"` | Model identifier |
| `api_key_env` | string | `"OPENAI_API_KEY"` | Env var fallback for API key (CI/CD only; prefer `credential_store_key` with OS keychain) |
| `api_key` | string? | `null` | Direct API key (supports `keychain:` prefix) |
| `base_url` | string? | `null` | Base URL override (e.g., for Azure or Ollama) |
| `max_tokens` | int | `4096` | Maximum tokens per response |
| `temperature` | float | `0.7` | Generation temperature (0.0--2.0) |
| `context_window` | int | `128000` | Model context window size |
| `input_cost_per_million` | float | `2.50` | Cost per 1M input tokens (USD) |
| `output_cost_per_million` | float | `10.00` | Cost per 1M output tokens (USD) |
| `use_streaming` | bool | `true` | Enable streaming responses |
| `credential_store_key` | string? | `null` | OS keychain service name for API key |
| `auth_method` | string | `""` | `"api_key"` (default) or `"oauth"` |

### `[llm.retry]` -- Retry Configuration

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `max_retries` | int | `3` | Maximum retry attempts |
| `initial_backoff_ms` | int | `1000` | Initial backoff delay (ms) |
| `max_backoff_ms` | int | `60000` | Maximum backoff delay (ms) |
| `backoff_multiplier` | float | `2.0` | Exponential backoff multiplier |
| `jitter` | bool | `true` | Add random jitter to backoff |

### `[[llm.fallback_providers]]` -- Fallback Providers

```toml
[[llm.fallback_providers]]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
credential_store_key = "anthropic"  # OS keychain (preferred)
# api_key_env = "ANTHROPIC_API_KEY"  # CI/CD fallback only
# base_url = "https://custom-endpoint.example.com"
```

## `[safety]` -- Safety and Permissions

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `approval_mode` | string | `"safe"` | `safe`, `cautious`, `paranoid`, `yolo` |
| `allowed_paths` | string[] | `["src/**", "tests/**", "docs/**"]` | Glob patterns for allowed file paths |
| `denied_paths` | string[] | `[".env*", "**/*.key", ...]` | Glob patterns for denied file paths |
| `allowed_commands` | string[] | `["cargo", "git", "npm", ...]` | Allowed shell command prefixes |
| `ask_commands` | string[] | `["rm", "mv", "cp", "chmod"]` | Commands requiring approval |
| `denied_commands` | string[] | `["sudo", "curl \| sh", ...]` | Blocked commands |
| `allowed_hosts` | string[] | `["api.github.com", ...]` | Allowed network hosts |
| `max_iterations` | int | `50` | Max agent iterations before pause |
| `max_tool_calls_per_minute` | int | `0` | Rate limit (0 = unlimited) |

### `[safety.injection_detection]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable prompt injection detection |
| `threshold` | float | `0.5` | Risk score threshold (0.0--1.0) |
| `scan_tool_outputs` | bool | `true` | Scan tool outputs for indirect injection |

### `[safety.adaptive_trust]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable adaptive trust gradient |
| `trust_escalation_threshold` | int | `5` | Consecutive approvals before auto-promotion |
| `anomaly_threshold` | float | `0.7` | Anomaly score for de-escalation |

## `[memory]` -- Memory System

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `window_size` | int | `12` | Short-term memory window (messages) |
| `compression_threshold` | float | `0.7` | Context fraction triggering compression |
| `persist_path` | string? | `null` | Path for persistent long-term memory |
| `enable_persistence` | bool | `true` | Enable long-term memory |

## `[tools]` -- Tool Configuration

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enable_builtins` | bool | `true` | Enable built-in tools |
| `default_timeout_secs` | int | `60` | Tool execution timeout (seconds) |
| `max_output_bytes` | int | `1048576` | Maximum tool output size (1 MB) |

## `[ui]` -- UI Configuration

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `theme` | string | `"dark"` | Color theme |
| `vim_mode` | bool | `false` | Enable vim keybindings |
| `show_cost` | bool | `true` | Show cost information |
| `verbose` | bool | `false` | Enable verbose output |

## `[gateway]` -- WebSocket Gateway

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable WebSocket gateway |
| `host` | string | `"127.0.0.1"` | Listen address |
| `port` | int | `18790` | Listen port |

## `[search]` -- Hybrid Search

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable tantivy full-text + SQLite vector search |
| `index_path` | string? | `null` | Path for search index |

## `[embedding]` -- Embedding Provider

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `provider` | string | `"local"` | `local` (128-dim hash), `fastembed` (384-dim), `openai` (1536-dim), `ollama` |
| `model` | string? | `null` | Model name for remote providers |
| `api_key_env` | string? | `null` | Env var for API key |
| `base_url` | string? | `null` | Base URL override |

## `[voice]` -- Voice and Audio

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable voice features |
| `stt_provider` | string | `"openai"` | STT provider: `openai`, `whisper-local`, `mock` |
| `stt_model` | string | `"base"` | Whisper model size (for local) |
| `stt_language` | string | `"en"` | Language code for STT |
| `tts_provider` | string | `"openai"` | TTS provider: `openai`, `mock` |
| `tts_voice` | string | `"alloy"` | TTS voice name |
| `tts_speed` | float | `1.0` | Speech speed multiplier |
| `vad_enabled` | bool | `true` | Voice activity detection |
| `vad_threshold` | float | `0.01` | VAD energy threshold |
| `wake_words` | string[] | `["hey rustant"]` | Wake word phrases |
| `wake_sensitivity` | float | `0.5` | Wake word sensitivity |
| `auto_speak` | bool | `false` | Auto-speak responses |
| `max_listen_secs` | int | `30` | Maximum listening duration |
| `input_device` | string? | `null` | Audio input device |
| `output_device` | string? | `null` | Audio output device |

## `[meeting]` -- Meeting Recording

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable meeting features |
| `notes_folder` | string | `"Meeting Transcripts"` | Notes.app folder for transcripts |
| `audio_format` | string | `"wav"` | Recording format |
| `sample_rate` | int | `16000` | Audio sample rate (Hz) |
| `max_duration_mins` | int | `180` | Maximum recording duration |
| `auto_detect_virtual_audio` | bool | `true` | Detect BlackHole/Loopback devices |
| `auto_transcribe` | bool | `true` | Auto-transcribe on stop |
| `auto_summarize` | bool | `true` | Auto-summarize after transcription |
| `silence_timeout_secs` | int | `60` | Silence auto-stop (0 = disabled) |

## `[channels]` -- Messaging Channels

Each channel type has its own sub-table:

```toml
[channels.slack]
bot_token = "keychain:channel:slack:bot_token"
app_token = "env:SLACK_APP_TOKEN"

[channels.telegram]
bot_token = "env:TELEGRAM_BOT_TOKEN"

[channels.discord]
bot_token = "env:DISCORD_BOT_TOKEN"

[channels.email]
imap_host = "imap.gmail.com"
smtp_host = "smtp.gmail.com"
username = "user@gmail.com"
password_env = "EMAIL_PASSWORD"

[channels.imessage]
enabled = true

[channels.teams]
tenant_id = "..."
client_id = "..."

[channels.sms]
provider = "twilio"

[channels.matrix]
homeserver = "https://matrix.org"

[channels.signal]
phone_number = "+1234567890"

[channels.whatsapp]
api_url = "..."

[channels.irc]
server = "irc.libera.chat"

[channels.webchat]
enabled = true

[channels.webhook]
url = "https://..."
```

Secret values support three formats:
- `keychain:<account>` -- Resolve from OS keychain
- `env:<VAR>` -- Resolve from environment variable
- Bare string -- Inline (deprecated, auto-migrated to keychain)

## `[intelligence]` -- Channel Intelligence

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable channel intelligence globally |
| `digest_dir` | string | `".rustant/digests"` | Digest export directory |
| `reminders_dir` | string | `".rustant/reminders"` | ICS calendar export directory |
| `max_reply_tokens` | int | `500` | Max tokens per auto-reply |

### `[intelligence.defaults]` -- Default Channel Intelligence

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `auto_reply` | string | `"full_auto"` | `disabled`, `draft_only`, `auto_with_approval`, `full_auto` |
| `digest` | string | `"off"` | `off`, `hourly`, `daily`, `weekly` |
| `smart_scheduling` | bool | `true` | Auto-schedule follow-ups |
| `escalation_threshold` | string | `"high"` | `low`, `normal`, `high`, `urgent` |
| `default_followup_minutes` | int | `60` | Follow-up reminder delay |

### `[intelligence.channels.<name>]`

Per-channel overrides with the same keys as `[intelligence.defaults]`.

### `[intelligence.quiet_hours]`

| Key | Type | Description |
|-----|------|-------------|
| `start` | string | Start time in HH:MM format |
| `end` | string | End time in HH:MM format |

## `[cdc]` -- Change Data Capture

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable background channel polling |
| `default_interval_secs` | int | `60` | Default polling interval |
| `channels` | map | `{}` | Per-channel interval overrides |

## `[scheduler]` -- Scheduler / Cron

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable scheduler |
| `cron_jobs` | array | `[]` | Cron job definitions |
| `webhook_port` | int? | `null` | Webhook listener port |
| `max_background_jobs` | int | `10` | Max concurrent background jobs |
| `state_path` | string? | `null` | State persistence path |

## `[budget]` -- Token Budget

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `session_limit_usd` | float | `0.0` | Max cost per session (0 = unlimited) |
| `task_limit_usd` | float | `0.0` | Max cost per task (0 = unlimited) |
| `session_token_limit` | int | `0` | Max total tokens per session (0 = unlimited) |
| `halt_on_exceed` | bool | `false` | Halt (true) or warn (false) on budget exceeded |

## `[knowledge]` -- Cross-Session Knowledge

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable knowledge distillation |
| `max_rules` | int | `20` | Max distilled rules in system prompt |
| `min_entries_for_distillation` | int | `3` | Min entries before distillation triggers |
| `knowledge_path` | string? | `null` | Local knowledge store file path |

## `[council]` -- LLM Council

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable multi-model deliberation |
| `voting_strategy` | string | `"chairman_synthesis"` | `chairman_synthesis`, `highest_score`, `majority_consensus` |
| `enable_peer_review` | bool | `true` | Enable peer review stage |
| `chairman_model` | string? | `null` | Explicit chairman (auto-selects if null) |
| `max_member_tokens` | int | `2048` | Max tokens per member response |
| `auto_detect` | bool | `true` | Auto-detect providers from env vars + Ollama |

### `[[council.members]]`

```toml
[[council.members]]
provider = "openai"
model = "gpt-4o"
credential_store_key = "openai"  # OS keychain (preferred)
weight = 1.0

[[council.members]]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
credential_store_key = "anthropic"  # OS keychain (preferred)
weight = 1.0

[[council.members]]
provider = "ollama"
model = "llama3"
base_url = "http://127.0.0.1:11434/v1"
weight = 0.8
```

## `[persona]` -- Adaptive Personas

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable dynamic persona evolution |
| `auto_detect` | bool | `true` | Auto-detect persona from task classification |
| `metrics_path` | string? | `null` | Path for persona metrics persistence |

Personas: `Architect`, `SecurityGuardian`, `MlopsEngineer`, `General`, `IncidentCommander`, `ObservabilityExpert`, `ReliabilityEngineer`, `DeploymentEngineer`.

## `[cache]` -- Prompt Caching

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable prompt caching |
| `ttl_secs` | int | `300` | Cache time-to-live |

## `[plan]` -- Plan Mode

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable plan mode by default |
| `max_steps` | int | `20` | Maximum steps in a plan |
| `require_approval` | bool | `true` | Require plan approval before execution |

## `[workflow]` -- Workflow Engine

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable workflow engine |
| `workflow_dir` | string? | `null` | Custom workflow definitions directory |
| `max_concurrent_runs` | int | `4` | Max concurrent workflow runs |
| `default_step_timeout_secs` | int | `300` | Default timeout per step |
| `state_persistence_path` | string? | `null` | Workflow state persistence path |

## `[browser]` -- Browser Automation

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable browser automation |
| `connection_mode` | string | `"auto"` | `auto`, `connect`, `launch` |
| `debug_port` | int | `9222` | Chrome remote debugging port |
| `ws_url` | string? | `null` | Direct WebSocket URL |
| `chrome_path` | string? | `null` | Chrome/Chromium binary path |
| `headless` | bool | `true` | Run headless |
| `default_viewport_width` | int | `1280` | Viewport width (px) |
| `default_viewport_height` | int | `720` | Viewport height (px) |
| `default_timeout_secs` | int | `30` | Operation timeout |
| `allowed_domains` | string[] | `[]` | Domain allowlist (empty = all) |
| `blocked_domains` | string[] | `[]` | Domain blocklist |
| `isolate_profile` | bool | `true` | Use isolated browser profile |
| `user_data_dir` | string? | `null` | Persistent browser profile dir |
| `max_pages` | int | `5` | Maximum open pages/tabs |

## `[multi_agent]` -- Multi-Agent System

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable multi-agent mode |
| `max_agents` | int | `8` | Maximum concurrent agents |
| `max_mailbox_size` | int | `1000` | Max messages per agent mailbox |
| `default_workspace_base` | string? | `null` | Base directory for agent workspaces |

## `[[mcp_servers]]` -- External MCP Servers

```toml
[[mcp_servers]]
name = "chrome-devtools"
command = "npx"
args = ["chrome-devtools-mcp@latest"]
auto_connect = true
# working_dir = "/path/to/dir"
# env = { KEY = "value" }
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | required | Server identifier |
| `command` | string | required | Command to start the server |
| `args` | string[] | `[]` | Command arguments |
| `working_dir` | string? | `null` | Working directory |
| `env` | map | `{}` | Environment variables |
| `auto_connect` | bool | `true` | Auto-connect on startup |

## `[mcp_safety]` -- MCP Safety Policy

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable MCP safety checks |
| `max_risk_level` | string | `"write"` | Max risk level: `read_only`, `write`, `execute`, `network`, `destructive` |
| `allowed_tools` | string[] | `[]` | Always-allowed tools |
| `denied_tools` | string[] | `["shell_exec", "macos_gui_scripting"]` | Always-denied tools |
| `scan_inputs` | bool | `true` | Scan tool args for injection |
| `scan_outputs` | bool | `true` | Scan tool outputs for injection |
| `audit_enabled` | bool | `true` | Log MCP calls to audit trail |
| `max_calls_per_minute` | int | `60` | Rate limit (0 = unlimited) |

## `[hooks]` -- Lifecycle Hooks

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable hooks system |
| `hooks` | array | `[]` | Hook definitions |

Events: `session_start`, `session_end`, `task_start`, `task_complete`, `pre_tool_use`, `post_tool_use`, `safety_denial`, `error_occurred`.

## `[arxiv]` -- ArXiv Research

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `semantic_search_enabled` | bool | `true` | Enable semantic search over library |
| `openalex_email` | string? | `null` | Email for OpenAlex polite pool |
| `cache_ttl_secs` | int | `3600` | Response cache TTL |
| `cache_max_entries` | int | `1000` | Maximum cache entries |

## `[hydration]` -- Context Hydration

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable context hydration pipeline |
| `max_token_budget` | int | `4096` | Token budget for context assembly |

## `[verification]` -- Verification Loop

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable auto-verification |
| `max_fix_attempts` | int | `3` | Maximum fix-and-recheck iterations |

## `[ai_engineer]` -- AI/ML Engineering

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Master enable switch |
| `python_path` | string? | `null` | Python interpreter path |
| `venv_path` | string? | `null` | Virtual environment path |

Sub-configs: `[ai_engineer.evaluation]`, `[ai_engineer.inference]`, `[ai_engineer.research]`, `[ai_engineer.safety]`, `[ai_engineer.rag]`, `[ai_engineer.training]`.

## `[features]` -- Feature Flags

See the [Feature Flags](feature-flags.md) reference for the complete list.

## `[security]` -- Security Engine

Stored as raw JSON to avoid circular dependency with the rustant-security crate. See the security crate documentation for schema details.

## Example Configuration

```toml
[llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
credential_store_key = "anthropic"  # OS keychain (set via `rustant setup`)
use_streaming = true
max_tokens = 8192
temperature = 0.5

[llm.retry]
max_retries = 3
initial_backoff_ms = 1000

[safety]
approval_mode = "safe"
max_iterations = 50

[memory]
window_size = 12
enable_persistence = true

[budget]
session_limit_usd = 5.00
halt_on_exceed = false

[features]
prompt_caching = true
semantic_search = true

[[mcp_servers]]
name = "chrome-devtools"
command = "npx"
args = ["chrome-devtools-mcp@latest"]
auto_connect = true
```
