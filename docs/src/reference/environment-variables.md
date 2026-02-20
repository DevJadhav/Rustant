# Environment Variables Reference

> **Security Note:** Rustant follows a **keychain-first** credential model aligned with its Security pillar. API keys should be stored in the OS keychain via `rustant setup`, not as environment variables. Environment variables are supported as a **fallback for CI/CD environments** where keychain access is unavailable.

Rustant reads configuration from environment variables using a `RUSTANT_` prefix with double-underscore (`__`) nesting for nested config keys. It also reads service-specific variables directly for API keys and integrations.

## Configuration Override Pattern

Any configuration key can be set via environment variables using the pattern:

```
RUSTANT_<SECTION>__<KEY>=value
```

The double underscore (`__`) separates nested config levels. Examples:

| Environment Variable | Config Equivalent |
|---------------------|-------------------|
| `RUSTANT_LLM__PROVIDER` | `[llm] provider` |
| `RUSTANT_LLM__MODEL` | `[llm] model` |
| `RUSTANT_LLM__USE_STREAMING` | `[llm] use_streaming` |
| `RUSTANT_LLM__MAX_TOKENS` | `[llm] max_tokens` |
| `RUSTANT_LLM__TEMPERATURE` | `[llm] temperature` |
| `RUSTANT_SAFETY__APPROVAL_MODE` | `[safety] approval_mode` |
| `RUSTANT_SAFETY__MAX_ITERATIONS` | `[safety] max_iterations` |
| `RUSTANT_MEMORY__WINDOW_SIZE` | `[memory] window_size` |
| `RUSTANT_MEMORY__ENABLE_PERSISTENCE` | `[memory] enable_persistence` |
| `RUSTANT_TOOLS__DEFAULT_TIMEOUT_SECS` | `[tools] default_timeout_secs` |
| `RUSTANT_UI__THEME` | `[ui] theme` |
| `RUSTANT_UI__VIM_MODE` | `[ui] vim_mode` |
| `RUSTANT_FEATURES__PROMPT_CACHING` | `[features] prompt_caching` |
| `RUSTANT_FEATURES__SECURITY_SCANNING` | `[features] security_scanning` |
| `RUSTANT_FEATURES__AI_ENGINEER` | `[features] ai_engineer` |

Environment variables take priority over config files but are overridden by CLI flags.

## LLM Provider API Keys (CI/CD Fallback)

These environment variables are checked as a **fallback** when no keychain credential is found. For interactive use, prefer `rustant setup` which stores keys in the OS keychain.

| Variable | Provider | Description |
|----------|----------|-------------|
| `OPENAI_API_KEY` | OpenAI | API key for GPT-4o, GPT-4, etc. Also used for TTS/STT (voice), embeddings |
| `ANTHROPIC_API_KEY` | Anthropic | API key for Claude models |
| `GEMINI_API_KEY` | Google Gemini | API key for Gemini models |
| `AZURE_OPENAI_API_KEY` | Azure OpenAI | API key for Azure-hosted OpenAI models |
| `AZURE_OPENAI_ENDPOINT` | Azure OpenAI | Base URL for Azure OpenAI deployment |

For Ollama (localhost), no API key is required. Rustant uses `"ollama"` as a dummy bearer token internally.

## Credential Resolution Order

Rustant resolves API keys in this order:

1. `api_key` field with `keychain:` prefix -- resolved from OS keychain
2. `credential_store_key` field -- resolved from OS keychain by provider name
3. Environment variable specified by `api_key_env`

The `resolve_api_key_by_env(env_var)` function centralizes this for all subsystems: checks OS keychain first, then the environment variable.

## Channel Integration Variables

| Variable | Channel | Description |
|----------|---------|-------------|
| `SLACK_BOT_TOKEN` | Slack | Bot user OAuth token (`xoxb-...`) |
| `SLACK_APP_TOKEN` | Slack | App-level token for Socket Mode (`xapp-...`) |
| `DISCORD_BOT_TOKEN` | Discord | Discord bot token |
| `TELEGRAM_BOT_TOKEN` | Telegram | Telegram Bot API token |
| `EMAIL_PASSWORD` | Email | Email account password (for IMAP/SMTP) |
| `MATRIX_ACCESS_TOKEN` | Matrix | Matrix homeserver access token |
| `SIGNAL_CLI_PATH` | Signal | Path to signal-cli binary |
| `WHATSAPP_API_TOKEN` | WhatsApp | WhatsApp Business API token |
| `TWILIO_ACCOUNT_SID` | SMS (Twilio) | Twilio account SID |
| `TWILIO_AUTH_TOKEN` | SMS (Twilio) | Twilio auth token |
| `TEAMS_CLIENT_SECRET` | Microsoft Teams | OAuth client secret |

Channel secrets can also be stored in config using the `SecretRef` format:
- `keychain:<account>` -- OS keychain lookup
- `env:<VAR>` -- Environment variable lookup
- Bare string -- Inline (deprecated, auto-migrated to keychain on first run)

## SRE / DevOps Variables

| Variable | Service | Description |
|----------|---------|-------------|
| `PROMETHEUS_URL` | Prometheus | Prometheus server URL for metrics queries |
| `PAGERDUTY_API_KEY` | PagerDuty | API key for on-call and incident management |
| `KUBECONFIG` | Kubernetes | Path to kubeconfig file (standard kubectl variable) |

SRE tools can also be configured via `.rustant/sre_config.json` or the `[sre]` config section.

## Voice and Audio Variables

| Variable | Description |
|----------|-------------|
| `OPENAI_API_KEY` | Used for Whisper STT and OpenAI TTS (shared with LLM provider) |
| `PICOVOICE_ACCESS_KEY` | Access key for Porcupine wake word detection |

## Research Variables

| Variable | Description |
|----------|-------------|
| `SEMANTIC_SCHOLAR_API_KEY` | API key for Semantic Scholar (optional, for higher rate limits) |
| `OPENALEX_EMAIL` | Email for OpenAlex polite pool (not a secret, enables faster rate limits) |

## CI / Build Variables

| Variable | Description |
|----------|-------------|
| `RUSTFLAGS` | Rust compiler flags (CI sets `"-Dwarnings"`) |
| `CI` | Standard CI indicator (Rustant adjusts behavior in CI) |

## .env File Support

Rustant loads `.env` files from the working directory via `dotenvy`. Place a `.env` file in your project root for **non-secret** configuration overrides:

```bash
# .env — for configuration overrides only
RUSTANT_SAFETY__APPROVAL_MODE=cautious
RUSTANT_LLM__MODEL=claude-sonnet-4-20250514
```

> **Prefer OS keychain for API keys.** Use `rustant setup` to store secrets in the OS keychain rather than `.env` files. If you must use `.env` for CI/CD, ensure `.env` is in your `.gitignore`.

The `.env` file is loaded before any other configuration, so its values can be overridden by workspace config, user config, and CLI flags.

## Priority Order

From highest to lowest priority:

1. CLI flags (`--model`, `--approval`)
2. `RUSTANT_*` environment variables
3. Workspace config (`.rustant/config.toml`)
4. User config (`~/.config/rustant/config.toml`)
5. `.env` file values (loaded into environment)
6. Built-in defaults

## Security Notes

- **Store API keys in the OS keychain** via `rustant setup` (preferred). Never commit API keys to version control.
- Rustant auto-migrates plaintext channel secrets in config files to the OS keychain on first run.
- The secret redaction system (60+ patterns + Shannon entropy) scrubs API keys from all LLM context, memory, audit logs, and MCP output.
- Denied paths in the default safety config include `.env*`, `**/*.key`, `**/secrets/**`, and other sensitive patterns.
