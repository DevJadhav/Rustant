# Configuration

Rustant uses a layered configuration system. Settings are applied in this order (later sources override earlier ones):

1. Built-in defaults
2. Config file (`~/.config/rustant/config.toml` or `.rustant/config.toml`)
3. Environment variables
4. CLI arguments

## Quick Setup

```bash
rustant config init   # Create default config file
rustant config show   # Show current effective configuration
rustant setup         # Interactive wizard
```

## Key Sections

### `[llm]` — LLM Provider

```toml
[llm]
provider = "openai"              # openai, anthropic, gemini, azure, ollama, vllm
model = "gpt-4o"                 # Model name
credential_store_key = "openai"  # Reads API key from OS keychain (set via `rustant setup`)
auth_method = "keyring"          # keyring (recommended), env, oauth
temperature = 0.7
max_tokens = 4096
use_streaming = true             # Enable streaming responses

[llm.retry]
max_retries = 3
initial_backoff_ms = 1000
max_backoff_ms = 60000
backoff_multiplier = 2.0
jitter = true
```

### `[safety]` — Security Settings

```toml
[safety]
approval_mode = "safe"     # safe, cautious, paranoid, yolo
max_iterations = 50
denied_paths = ["/etc/shadow", "/root"]
denied_commands = ["rm -rf /", "mkfs"]
default_timeout_secs = 60
```

### `[memory]` — Memory Configuration

```toml
[memory]
window_size = 20           # Messages in working memory
enable_persistence = true  # Persistent long-term memory
```

### `[channels]` — Messaging Channels

See [Channels & Messaging](../user-guide/channels.md) for per-channel setup.

```toml
[channels.slack]
enabled = true
bot_token_ref = "keychain:channel:slack:bot_token"

[channels.email]
enabled = true
auth_method = "oauth"
poll_interval_secs = 60
```

### `[intelligence]` — Channel Intelligence

```toml
[intelligence]
enabled = true

[intelligence.defaults]
auto_reply = "full_auto"   # full_auto, auto_with_approval, draft_only, disabled
digest = "daily"           # off, hourly, daily, weekly
smart_scheduling = true
```

### `[cdc]` — Change Data Capture

```toml
[cdc]
enabled = true
default_interval_secs = 60
sent_record_ttl_days = 7
style_fact_threshold = 50

[cdc.channel_intervals]
slack = 30
email = 300
```

### `[search]` — Search Engine

```toml
[search]
enabled = true
index_dir = ".rustant/search_index"
max_results = 20
```

### `[embeddings]` — Embedding Providers

```toml
[embeddings]
provider = "local"         # local, fast, openai, ollama
model = ""                 # Model name (for openai/ollama)
```

### `[gateway]` — WebSocket Gateway

```toml
[gateway]
enabled = false
host = "127.0.0.1"
port = 18790
auth_tokens = []
max_connections = 50
```

### `[voice]` — Voice Settings

```toml
[voice]
enabled = true
stt_provider = "openai"
tts_voice = "alloy"       # alloy, echo, fable, onyx, nova, shimmer
```

### `[council]` — LLM Council

```toml
[council]
enabled = false
voting_strategy = "chairman_synthesis"  # chairman_synthesis, highest_score, majority_consensus
enable_peer_review = true
max_member_tokens = 4096
```

### `[personas]` — Adaptive Personas

```toml
[personas]
enabled = true
auto_detect = true
default = "general"
```

### `[scheduler]` — Cron Jobs

```toml
[scheduler]
enabled = true
max_background_jobs = 10

[[scheduler.cron_jobs]]
name = "daily-summary"
schedule = "0 0 9 * * * *"
task = "Summarize yesterday's git commits"
enabled = true
```

### `[feature_flags]` — Runtime Feature Flags

```toml
[feature_flags]
prompt_caching = true
semantic_search = true
dynamic_personas = false
evaluation = true
security_scanning = false
compliance_engine = false
incident_response = false
sre_mode = false
progressive_trust = false
global_circuit_breaker = true
```

### `[sre]` — SRE Mode Configuration

```toml
[sre]
enabled = false

[sre.prometheus]
url = "http://localhost:9090"

[sre.oncall]
provider = "local"         # local, pagerduty

[sre.daemon]
enabled = false
port = 18791
```

### `[[mcp_servers]]` — External MCP Servers

```toml
[[mcp_servers]]
name = "chrome-devtools"
command = "npx"
args = ["-y", "chrome-devtools-mcp@latest"]
auto_connect = true
```

## Environment Variables

Any config value can be overridden via `RUSTANT_` prefix with double underscores for nesting:

```bash
export RUSTANT_LLM__PROVIDER=anthropic
export RUSTANT_LLM__MODEL=claude-sonnet-4-20250514
export RUSTANT_SAFETY__APPROVAL_MODE=cautious
```

Common API key variables:
- `OPENAI_API_KEY` — OpenAI
- `ANTHROPIC_API_KEY` — Anthropic
- `GEMINI_API_KEY` — Google Gemini
- `AZURE_OPENAI_API_KEY` — Azure OpenAI

## CLI Overrides

```bash
rustant --model gpt-4o-mini --approval yolo "Quick task"
rustant --workspace /path/to/project --config custom-config.toml
rustant --verbose    # Debug logging
rustant --quiet      # Errors only
```

See the [Configuration Reference](../reference/configuration.md) for exhaustive documentation of all settings.
