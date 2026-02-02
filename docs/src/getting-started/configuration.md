# Configuration

Rustant uses a layered configuration system. Settings are applied in this order (later sources override earlier ones):

1. Built-in defaults
2. Config file (`~/.config/rustant/config.toml` or `.rustant/config.toml`)
3. Environment variables
4. CLI arguments

## Config File

Generate a default config file:

```bash
rustant config init
```

Show the current effective configuration:

```bash
rustant config show
```

## Key Sections

### `[llm]` — LLM Provider

```toml
[llm]
provider = "openai"        # openai, anthropic, gemini
model = "gpt-4o"           # Model name
auth_method = "env"        # env, keyring, oauth
temperature = 0.7
max_tokens = 4096
```

### `[safety]` — Security Settings

```toml
[safety]
approval_mode = "safe"     # safe, cautious, paranoid, yolo
max_iterations = 50
denied_paths = ["/etc/shadow", "/root"]
denied_commands = ["rm -rf /", "mkfs"]
```

### `[memory]` — Memory Configuration

```toml
[memory]
working_limit = 20
short_term_limit = 100
long_term_enabled = true
auto_summarize = true
```

### `[ui]` — Interface Settings

```toml
[ui]
use_tui = true
theme = "dark"
show_thinking = true
```

### `[tools]` — Tool Configuration

```toml
[tools]
timeout_secs = 30
max_file_size_bytes = 10485760  # 10 MB
shell = "bash"
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

### `[channels]` — Messaging Channels

See the [Channels](../user-guide/channels.md) guide for per-channel configuration.

### `[search]` — Search Engine

```toml
[search]
enabled = true
index_dir = ".rustant/search_index"
max_results = 20
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

## Environment Variables

Any config value can be overridden via environment variables using the prefix `RUSTANT_`:

```bash
export RUSTANT_LLM__PROVIDER=anthropic
export RUSTANT_LLM__MODEL=claude-sonnet-4-20250514
export RUSTANT_SAFETY__APPROVAL_MODE=cautious
```

Double underscores (`__`) represent nested config sections.

## CLI Overrides

```bash
rustant --model gpt-4o-mini --approval yolo "Quick task"
rustant --workspace /path/to/project --config custom-config.toml
rustant --verbose    # Debug logging
rustant --quiet      # Errors only
```
