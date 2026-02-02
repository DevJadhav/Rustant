# Rustant

[![CI](https://github.com/DevJadhav/Rustant/actions/workflows/ci.yml/badge.svg)](https://github.com/DevJadhav/Rustant/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-1.0.0-green.svg)](CHANGELOG.md)

A high-performance, privacy-first autonomous personal agent built entirely in Rust.

**Rust** + **Assistant** = **Rustant** — like an industrious ant, small but capable of carrying workloads many times its size.

## Overview

Rustant is an LLM-powered agent that executes complex tasks through a Think-Act-Observe loop while maintaining strict safety guarantees. It supports voice commands, browser automation, 13 messaging channels, a rich canvas system, multi-agent orchestration, and extensible plugins — all running locally with optional cloud features.

### Core Differentiators

- **Transparent Autonomy** — Every decision is logged and reviewable via Merkle-chain audit trail
- **Progressive Control** — From "suggest only" to "full autonomy with audit" across four approval modes
- **Adaptive Context Engineering** — Three-tier memory with smart compression for long sessions
- **Git-Native Safety** — All file operations are reversible through automatic checkpointing
- **Zero-Cloud Option** — Complete functionality with local LLMs (Ollama, vLLM) — no data leaves your machine
- **Rust Performance** — Sub-millisecond tool dispatch, minimal memory footprint
- **Multi-Modal Interaction** — Voice, text, canvas, and browser automation in a single agent
- **Platform Agnostic** — 13 messaging channels, MCP protocol for tool interoperability
- **Enterprise Ready** — OAuth 2.0, tamper-evident audit trails, WASM-sandboxed plugins

## Quick Start

### Install

```bash
# Shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/DevJadhav/Rustant/main/scripts/install.sh | bash

# Homebrew (macOS)
brew install DevJadhav/tap/rustant

# Cargo
cargo install rustant

# Build from source
git clone https://github.com/DevJadhav/Rustant.git
cd Rustant && cargo build --release --workspace
```

### First Run

```bash
# Interactive setup wizard — configure LLM provider, API key, preferences
rustant setup

# Interactive REPL with TUI
rustant

# Single task
rustant "refactor the auth module"

# With options
rustant --model gpt-4o --approval cautious --workspace ./project "add tests for the API"
```

### CLI Flags

| Flag | Description |
|------|-------------|
| `-m, --model` | Override LLM model |
| `-w, --workspace` | Set workspace directory |
| `--approval` | Approval mode: `safe`, `cautious`, `paranoid`, `yolo` |
| `--no-tui` | Use simple REPL instead of TUI |
| `-c, --config` | Custom config file path |
| `-v, -vv, -vvv` | Increase verbosity |
| `-q, --quiet` | Suppress non-essential output |

## Architecture

```
rustant/
├── rustant-core/      # Agent orchestrator, brain, memory, safety, channels, gateway
├── rustant-tools/     # 12 built-in tools + 7 LSP tools
├── rustant-cli/       # CLI with REPL, TUI, and subcommands
├── rustant-mcp/       # MCP server + client (JSON-RPC 2.0)
├── rustant-ui/        # Tauri desktop dashboard
└── rustant-plugins/   # Plugin system (native + WASM)
```

### Key Components

| Component | Description |
|-----------|-------------|
| **Brain** | LLM provider abstraction with 6 providers, streaming, cost tracking, failover |
| **Memory** | Three-tier system: working (task), short-term (session), long-term (persistent) |
| **Safety Guardian** | 5-layer defense with 4 approval modes and prompt injection detection |
| **Tool Registry** | Dynamic registration with JSON schema validation, timeouts, and risk levels |
| **Agent Loop** | ReAct pattern (Think → Act → Observe) with async execution |
| **Channels** | 13 platform integrations with unified `Channel` trait |
| **Skills** | SKILL.md-based declarative tool definitions with security validation |
| **Plugins** | Native (.so/.dll/.dylib) and WASM (wasmi) sandboxed extensions |
| **Workflow Engine** | YAML-based multi-step automation with approval gates |
| **Search Engine** | Hybrid Tantivy full-text + SQLite vector search |
| **Audit Trail** | Merkle chain with SHA-256 for tamper-evident execution history |

## Features

### LLM Providers

| Provider | Auth Methods | Notes |
|----------|-------------|-------|
| **OpenAI** | API Key, OAuth | GPT-4o, GPT-4, GPT-3.5 |
| **Anthropic** | API Key | Claude 3.5 Sonnet, Opus, Haiku |
| **Google Gemini** | API Key, OAuth | Gemini Pro, Gemini Flash |
| **Azure OpenAI** | API Key | OpenAI models via Azure endpoints |
| **Ollama** | None (local) | Any locally hosted model |
| **vLLM** | None (local) | Self-hosted model serving |

All providers support a **failover circuit breaker** for automatic fallback across multiple backends.

### Messaging Channels

Connect Rustant to 13 platforms with a unified interface:

- **Slack** — Full API with OAuth, 13 CLI subcommands (send, history, channels, users, reactions, DMs, threads, files, teams, groups)
- **Discord** — Bot and webhook integration
- **Telegram** — Bot API support
- **Email** — SMTP/IMAP with Gmail OAuth
- **Matrix** — Decentralized chat protocol
- **Signal** — End-to-end encrypted messaging
- **WhatsApp** — Business API integration
- **SMS** — Twilio integration
- **IRC** — Traditional IRC protocol
- **Microsoft Teams** — Teams bot integration
- **iMessage** — macOS native integration
- **WebChat** — Custom web chat widget
- **Webhook** — Generic webhook receiver

### Voice & Audio

- **Speech-to-Text** — OpenAI Whisper (cloud and local models)
- **Text-to-Speech** — OpenAI TTS with configurable voices (alloy, echo, fable, onyx, nova, shimmer)
- **Voice Activity Detection** — Energy-threshold VAD with configurable sensitivity
- **Wake Word Detection** — Porcupine integration and STT-based fallback

### Browser Automation

- Chrome DevTools Protocol (CDP) via chromiumoxide
- Headless and headful modes
- Page navigation, interaction, screenshots, JS execution
- Domain allowlist/blocklist security guard
- Isolated browser profiles

### More Capabilities

- **Canvas** — Rich content rendering: charts (Chart.js), tables, forms, Mermaid diagrams, code, HTML, markdown
- **Workflow Engine** — Declarative YAML DSL with step dependencies, approval gates, conditional execution, retry/fallback
- **Cron Scheduler** — Background job management, heartbeat monitoring, webhook endpoints
- **Multi-Agent** — Agent spawning with parent-child relationships, message bus, resource limits, sandboxed workspaces
- **WebSocket Gateway** — axum-based remote access with TLS, REST API, session management
- **MCP Protocol** — JSON-RPC 2.0 server and client for tool interoperability with external systems
- **Hybrid Search** — Tantivy full-text + SQLite vector search for long-term memory
- **Dashboard UI** — Tauri-based desktop application for real-time monitoring

## Built-in Tools

### Core Tools (12)

| Tool | Risk Level | Description |
|------|------------|-------------|
| `file_read` | Read-only | Read file contents with optional line range |
| `file_list` | Read-only | List directory contents (respects .gitignore) |
| `file_search` | Read-only | Search text patterns across files |
| `file_write` | Write | Create or overwrite files |
| `file_patch` | Write | Apply targeted text replacements |
| `git_status` | Read-only | Show repository status |
| `git_diff` | Read-only | Show working tree changes |
| `git_commit` | Write | Stage and commit changes |
| `shell_exec` | Execute | Run shell commands (sandboxed) |
| `echo` | Read-only | Echo messages for debugging |
| `datetime` | Read-only | Get current date and time |
| `calculator` | Read-only | Evaluate mathematical expressions |

### LSP Tools (7)

Code intelligence powered by language servers (Rust, Python, TypeScript, Go, Java, C/C++):

| Tool | Description |
|------|-------------|
| `lsp_hover` | Show symbol information |
| `lsp_definition` | Go to definition |
| `lsp_references` | Find all references |
| `lsp_diagnostics` | Get errors and warnings |
| `lsp_completions` | Code completion suggestions |
| `lsp_rename` | Rename symbol across project |
| `lsp_format` | Format document |

## Safety Model

Four approval modes control agent autonomy:

| Mode | Auto-approved | Requires Approval |
|------|--------------|-------------------|
| **Safe** (default) | Read-only | All writes, executes, network |
| **Cautious** | Read-only + writes | Execute, network, destructive |
| **Paranoid** | Nothing | Everything |
| **Yolo** | Everything | Nothing |

Explicit deny lists always override approval modes — paths like `.env*`, `**/*.key`, and commands like `sudo` are always blocked.

Additional safety layers:
- **Prompt injection detection** — Multi-layer pattern scanning with risk scoring
- **Git checkpointing** — Automatic reversibility for all file operations
- **Merkle audit trail** — Tamper-evident, cryptographically verified execution history
- **WASM sandboxing** — Plugin isolation via wasmi
- **Filesystem sandboxing** — Path restrictions via cap-std

## CLI Reference

```bash
# Core
rustant                                    # Interactive REPL with TUI
rustant "task"                             # Single task execution
rustant setup                              # Interactive provider setup wizard
rustant config init                        # Create default config
rustant config show                        # Display current config

# Channels
rustant channel list                       # List configured channels
rustant channel test <name>                # Test channel connection
rustant channel slack send <ch> <msg>      # Send Slack message
rustant channel slack history <ch>         # Get channel history
rustant channel slack channels             # List Slack channels
rustant channel slack users                # List Slack users
rustant channel slack dm <user> <msg>      # Send direct message
rustant channel slack thread <ch> <ts> <msg>  # Reply in thread

# Authentication
rustant auth status                        # Show auth status
rustant auth login <provider>              # OAuth login
rustant auth logout <provider>             # Remove credentials
rustant auth refresh <provider>            # Refresh OAuth token

# Workflows
rustant workflow list                      # List available workflows
rustant workflow run <name> [-i key=val]   # Execute workflow
rustant workflow status <run_id>           # Check run status
rustant workflow resume <run_id>           # Resume paused workflow
rustant workflow cancel <run_id>           # Cancel running workflow

# Scheduled Jobs
rustant cron list                          # List all cron jobs
rustant cron add <name> <schedule> <task>  # Add new cron job
rustant cron run <name>                    # Manually trigger job
rustant cron enable|disable <name>         # Toggle cron job
rustant cron remove <name>                 # Delete cron job
rustant cron jobs                          # List background jobs

# Voice
rustant voice speak <text>                 # Text-to-speech
rustant voice roundtrip <text>             # TTS → STT roundtrip test

# Browser
rustant browser test [url]                 # Test browser automation

# Canvas
rustant canvas push <type> <content>       # Push content to canvas
rustant canvas clear                       # Clear canvas
rustant canvas snapshot                    # Get canvas state

# Skills & Plugins
rustant skill list [--dir <path>]          # List loaded skills
rustant skill info <path>                  # Show skill details
rustant skill validate <path>              # Security validation
rustant skill load <path>                  # Load and parse skill
rustant plugin list [--dir <path>]         # List loaded plugins
rustant plugin info <name>                 # Show plugin details

# System
rustant update check                       # Check for updates
rustant update install                     # Install latest version
rustant ui [--port 18790]                  # Launch Tauri dashboard
```

## Configuration

Configuration is layered: defaults → config file → environment variables → CLI arguments.

```bash
# Create default config
rustant config init

# Or use the interactive wizard
rustant setup
```

Config file location: `~/.config/rustant/config.toml` or `.rustant/config.toml`

```toml
[llm]
provider = "openai"
model = "gpt-4o"
api_key_env = "OPENAI_API_KEY"
max_tokens = 4096

[safety]
approval_mode = "safe"
denied_paths = [".env*", "**/*.key"]
denied_commands = ["sudo", "rm -rf"]
max_iterations = 50

[memory]
window_size = 20
enable_persistence = true

[channels.slack]
bot_token_env = "SLACK_BOT_TOKEN"

[gateway]
enabled = true
port = 18790

[voice]
enabled = true
stt_provider = "openai"
tts_voice = "alloy"
```

See the [Configuration Guide](docs/src/getting-started/configuration.md) for full reference.

## Security & Privacy

- **Credential Storage** — OS-native keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- **OAuth 2.0 + PKCE** — Browser-based authentication for LLM providers and channels
- **WASM Sandboxing** — Plugin isolation via wasmi with capability-based permissions
- **Filesystem Sandboxing** — Path restrictions via cap-std
- **Merkle Audit Trail** — Tamper-evident execution history with SHA-256 chain verification
- **Prompt Injection Detection** — Multi-layer pattern scanning with configurable thresholds
- **Git Checkpointing** — Automatic reversibility for all file operations
- **Zero-Cloud Mode** — Full functionality with local LLMs — no data leaves your machine

See [SECURITY.md](SECURITY.md) for vulnerability reporting and security policy.

## Development

```bash
# Build
cargo build --workspace
cargo build --workspace --release

# Test (1,733 tests)
cargo test --workspace

# Lint & Format
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Benchmarks
cargo bench -p rustant-core
cargo bench -p rustant-tools

# API Documentation
cargo doc --workspace --no-deps --open
```

**Requirements:** Rust 1.70+, Git configured (`git config --global user.email` / `user.name`)

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and PR guidelines.

## Documentation

- **User Guide** — [docs/](docs/) (mdBook)
- **API Reference** — `cargo doc --workspace --no-deps --open`
- **Changelog** — [CHANGELOG.md](CHANGELOG.md)

## Community

- [Contributing](CONTRIBUTING.md) — Development setup, coding standards, PR process
- [Code of Conduct](CODE_OF_CONDUCT.md) — Contributor Covenant v2.0
- [Security Policy](SECURITY.md) — Vulnerability reporting
- [GitHub Issues](https://github.com/DevJadhav/Rustant/issues) — Bug reports and feature requests

## License

[MIT](LICENSE)
