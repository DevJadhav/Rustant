# Rustant

[![CI](https://github.com/DevJadhav/Rustant/actions/workflows/ci.yml/badge.svg)](https://github.com/DevJadhav/Rustant/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rustant.svg)](https://crates.io/crates/rustant)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-1.0.0-green.svg)](CHANGELOG.md)

A high-performance, privacy-first autonomous personal agent built entirely in Rust.

**Rust** + **Assistant** = **Rustant** — like an industrious ant, small but capable of carrying workloads many times its size.

## Overview

Rustant is an LLM-powered agent that executes complex tasks through a Think-Act-Observe loop while maintaining strict safety guarantees. It supports voice commands, browser automation, 13 messaging channels, a rich canvas system, multi-agent orchestration, and extensible plugins — all running locally with optional cloud features.

### Core Differentiators

- **Transparent Autonomy** — Every tool call, safety denial, and contract violation produces a reviewable `DecisionExplanation`, logged via Merkle-chain audit trail
- **Progressive Control** — From "suggest only" to "full autonomy with audit" across four approval modes
- **Adaptive Context Engineering** — Three-tier memory with smart compression, structure-preserving fallback summarization, and cross-session learning via facts and corrections
- **Git-Native Safety** — All file operations are reversible through automatic checkpointing
- **Zero-Cloud Option** — Complete functionality with local LLMs (Ollama, vLLM) — no data leaves your machine
- **Rust Performance** — Sub-millisecond tool dispatch, minimal memory footprint
- **Multi-Modal Interaction** — Voice, text, canvas, and browser automation in a single agent
- **Platform Agnostic** — 13 messaging channels, MCP protocol for tool interoperability
- **Enterprise Ready** — OAuth 2.0, tamper-evident audit trails, WASM-sandboxed plugins

## Quick Start

### Install

```bash
# Cargo (from crates.io)
cargo install rustant

# Pre-built binary (faster, no compilation)
cargo binstall rustant

# Homebrew (macOS/Linux)
brew install DevJadhav/rustant/rustant

# Shell installer (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/DevJadhav/Rustant/main/scripts/install.sh | bash

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
├── rustant-tools/     # 39 built-in tools + 7 LSP tools
├── rustant-cli/       # CLI with REPL, TUI, and subcommands
├── rustant-mcp/       # MCP server + client (JSON-RPC 2.0)
├── rustant-ui/        # Tauri desktop dashboard
└── rustant-plugins/   # Plugin system (native + WASM)
```

### Key Components

| Component | Description |
|-----------|-------------|
| **Brain** | LLM provider abstraction with 6 providers, streaming, cost tracking, failover |
| **Memory** | Three-tier system: working (task), short-term (session), long-term (persistent) with cross-session learning via facts and corrections |
| **Safety Guardian** | 5-layer defense with 4 approval modes, prompt injection detection, typed ActionDetails, rich approval context with tool execution previews, and `/trust` dashboard |
| **Tool Registry** | Dynamic registration with JSON schema validation, timeouts, and risk levels |
| **Agent Loop** | ReAct pattern (Think → Act → Observe) with async execution and mid-task clarification via `ask_user` |
| **Command Registry** | Categorized slash commands with aliases, tab completion, typo suggestions, per-command detailed help, and graceful TUI-only command handling |
| **Channels** | 13 platform integrations with unified `Channel` trait |
| **Skills** | SKILL.md-based declarative tool definitions with security validation |
| **Plugins** | Native (.so/.dll/.dylib) and WASM (wasmi) sandboxed extensions |
| **Workflow Engine** | YAML-based multi-step automation with 28 built-in templates and approval gates |
| **Search Engine** | Hybrid Tantivy full-text + SQLite vector search |
| **Project Indexer** | Background workspace indexer with .gitignore-aware walking and multi-language signature extraction |
| **Session Manager** | Persistent sessions with auto-save, resume by name/ID, task continuations, search, tagging, auto-recovery on startup, and exit save prompts |
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

### Channel Intelligence

Rustant automatically processes incoming messages across all 13 channels with an intelligent classification and response pipeline:

- **Two-Tier Classification** — Fast heuristic pattern matching (<1ms) + LLM-based semantic classification for ambiguous messages, with caching to avoid re-classifying identical messages
- **Auto-Reply** — Configurable modes: `full_auto` (send all), `auto_with_approval` (queue high-priority for review), `draft_only` (generate but don't send), `disabled`. Safety-gated through SafetyGuardian
- **Channel Digests** — Periodic summaries (hourly/daily/weekly) with highlights, action items, and markdown export to `.rustant/digests/`
- **Smart Scheduling** — Automatic follow-up reminders for action-required messages. ICS calendar export to `.rustant/reminders/` for integration with calendar apps
- **Email Intelligence** — Auto-categorization (NeedsReply, ActionRequired, FYI, Newsletter, Automated), sender profile tracking, background IMAP polling
- **Per-Channel Config** — Each channel can have different auto-reply modes, digest frequencies, and escalation thresholds
- **Quiet Hours** — Suppress all auto-actions during configured time windows

```toml
# .rustant/config.toml
[intelligence]
enabled = true

[intelligence.defaults]
auto_reply = "full_auto"
digest = "daily"
smart_scheduling = true
escalation_threshold = "high"

[intelligence.channels.email]
auto_reply = "draft_only"
digest = "daily"

[intelligence.channels.slack]
auto_reply = "full_auto"
digest = "hourly"
```

REPL/TUI commands: `/digest`, `/digest history`, `/replies`, `/replies approve <id>`, `/reminders`, `/reminders dismiss <id>`, `/intelligence`, `/intelligence on/off`.

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
- **Workflow Engine** — Declarative YAML DSL with 28 built-in templates (code_review, morning_briefing, pr_review, dependency_audit, changelog, knowledge_graph, experiment_tracking, code_analysis, content_pipeline, skill_development, career_planning, system_monitoring, life_planning, privacy_audit, self_improvement_loop, and more), step dependencies, approval gates, and conditional execution
- **Cron Scheduler** — Background job management, heartbeat monitoring, webhook endpoints
- **Multi-Agent** — Agent spawning with parent-child relationships, message bus, resource limits, sandboxed workspaces
- **WebSocket Gateway** — axum-based remote access with TLS, REST API, session management
- **MCP Protocol** — JSON-RPC 2.0 server and client for tool interoperability with external systems
- **Hybrid Search** — Tantivy full-text + SQLite vector search for long-term memory
- **Dashboard UI** — Tauri-based desktop application for real-time monitoring
- **Project Indexer** — Background codebase indexing with function signature extraction for Rust, Python, JS/TS, Go, Java, Ruby, and C/C++
- **Session Resume** — Persistent session management with auto-save, resume by name or ID, task continuations, search, tagging, and auto-recovery on startup
- **Smart Editing** — Semantic code edit tool with fuzzy location matching (exact, line numbers, function patterns, similarity), diff preview, and auto-checkpoint
- **Zero-Config Init** — Project type auto-detection for 8 languages with framework detection, safety whitelist generation, and example tasks
- **First-Run Onboarding** — Interactive tour on first launch with project-aware examples and progressive capability introduction
- **Context Health Monitor** — Proactive warnings at 70%/90% context usage with compression notifications and pinning suggestions
- **Actionable Error Recovery** — Every error maps to specific recovery guidance with next steps and command suggestions
- **Vim Mode** — Full vim keybindings (normal/insert) with VIM-N/VIM-I status labels and [VIM] header badge

### UX & Usability

- **First-Run Onboarding Tour** — On first launch, a project-aware interactive tour introduces capabilities, safety model, and example tasks. Personalized using auto-detected project type and framework.
- **Context Window Health Monitor** — Proactive warnings at 70% (yellow) and 90% (red) context usage. Notifications when context is compressed, including whether LLM summarization or fallback truncation was used and how many pinned messages were preserved.
- **Actionable Error Recovery** — Every error variant maps to specific recovery guidance. Rate limits show retry timers, auth failures suggest `/doctor` and `/setup`, file-not-found errors suggest closest matches.
- **Tool Execution Previews** — Before approving destructive operations, see exactly what will change: file write sizes and paths, shell commands (truncated for safety), git operations, and smart-edit targets.
- **Safety Trust Dashboard** — `/trust` command (Ctrl+S in TUI) shows current approval mode with plain-English explanation, per-tool approval history, and adaptive suggestions to relax or tighten trust.
- **Progressive Help** — `/help [topic]` provides per-command detailed help with examples. State-aware suggestions (e.g., "consider `/compact`" when context is high). TUI-only commands in REPL show how to switch modes.
- **Session Search & Tagging** — Tag sessions for organization, search across names/goals/summaries, filter by tag. Relative timestamps ("2 days ago") for quick scanning.
- **Keyboard Shortcut Overlay** — F1 or `/keys` shows all shortcuts grouped by context.
- **Enriched Tool Execution Display** — Tool execution shows file paths and key arguments: `[file_read: src/main.rs]` instead of generic `[file_read] executing...`.
- **Real Diagnostics** — `/doctor` performs actual health checks: LLM connectivity, tool registration, config validation, workspace writability, session index integrity.
- **Session Auto-Recovery** — Automatic recovery of the last session on startup with user notification. Exit prompts to save unsaved work.
- **Vim Mode Indicators** — Distinct VIM-N/VIM-I status labels and a persistent [VIM] badge in the header bar.

### Natural Language Interaction

Rustant works like a conversational coding assistant. Type your request in plain English, and the agent works on it autonomously:

```
> refactor the auth module to use async/await
> add unit tests for the payment service
> find all usages of the deprecated API and suggest replacements
```

When the agent needs more information, it asks a clarifying question directly in the session and waits for your response before continuing. This is powered by the `ask_user` pseudo-tool — the LLM calls it like any tool, and the answer is routed back through the agent callback.

All `/command` slash commands are registered in a categorized command registry with alias support, tab completion, per-command detailed help (`/help <topic>`), and "did you mean?" suggestions for typos (Levenshtein distance). TUI-only commands gracefully inform REPL users how to access TUI mode.

### TUI Panels & Overlays

| Keybinding | Panel | Description |
|------------|-------|-------------|
| `Ctrl+E` | Explanation Panel | Safety transparency dashboard showing decision reasoning chains, alternatives, confidence, and context factors |
| `Ctrl+T` | Task Board | Multi-agent status board showing agent names, roles, current tool, elapsed time, and tool call counts |
| `Ctrl+S` | Trust Dashboard | Safety trust calibration: approval mode explanation, per-tool approval stats, and adaptive trust suggestions |
| `Ctrl+D` | Doctor | Run diagnostic checks (LLM connectivity, tool registration, config validation, workspace health) |
| `F1` / `/keys` | Keyboard Shortcuts | Floating overlay with all shortcuts grouped by context (Global, Input, Navigation, Overlays, Approval) |

## Built-in Tools

### Core Tools (17)

> **Tool count summary:** 39 base tools + 3 iMessage + 24 macOS native = **66 on macOS**, 39 on non-macOS. Plus 20 browser automation, 5 canvas, and 7 LSP tools.

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
| `web_search` | Read-only | Search the web via DuckDuckGo (privacy-first, no API key) |
| `web_fetch` | Read-only | Fetch a URL and extract readable text content |
| `document_read` | Read-only | Read local documents (txt, md, csv, json, yaml, xml, html, and more) |
| `smart_edit` | Write | Semantic code editor with fuzzy location matching and diff preview |
| `codebase_search` | Read-only | Natural language search over indexed project files and signatures |

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

### Productivity Tools (11)

| Tool | Description |
|------|-------------|
| `organizer` | Task and project organization |
| `compress` | File compression and archiving |
| `http_api` | HTTP API client for REST endpoints |
| `template` | Template rendering engine |
| `pdf` | PDF generation and manipulation |
| `pomodoro` | Focus timer with Pomodoro technique |
| `inbox` | Capture and triage incoming items |
| `relationships` | Contact and relationship management |
| `finance` | Personal finance tracking (transactions, budgets) |
| `flashcards` | Spaced repetition flashcard system |
| `travel` | Trip planning and itinerary management |

### Research (1)

| Tool | Description |
|------|-------------|
| `arxiv_research` | ArXiv paper search, analysis, library management, BibTeX export, paper-to-code |

### Cognitive Extension Tools (10)

Deep research intelligence, codebase analysis, experiment tracking, content strategy, production monitoring, skill development, career strategy, life planning, privacy management, and self-improvement — all through plain English commands.

| Tool | Actions | Description |
|------|---------|-------------|
| `knowledge_graph` | 13 | Local knowledge graph of concepts, papers, methods, people, and their relationships |
| `experiment_tracker` | 14 | Hypothesis lifecycle, experiment management, evidence recording, comparison |
| `code_intelligence` | 7 | Cross-language architecture analysis, pattern detection, tech debt scanning, API surface |
| `content_engine` | 14 | Multi-platform content pipeline with lifecycle, calendar, audience-aware drafting |
| `skill_tracker` | 8 | Skill progression tracking, knowledge gaps, learning paths, daily practice |
| `career_intel` | 8 | Career goals, achievements, portfolio management, networking notes |
| `system_monitor` | 8 | Service topology, health monitoring, incident tracking, cascade impact analysis |
| `life_planner` | 8 | Energy-aware scheduling, deadline tracking, habit management, context switching |
| `privacy_manager` | 8 | Data boundary management, access auditing, data export/deletion |
| `self_improvement` | 8 | Usage pattern analysis, performance tracking, cognitive load estimation, feedback |

### macOS Native Tools (24)

Deep integration with macOS applications (Calendar, Reminders, Notes, Mail, Music, Contacts, Safari, and more) via AppleScript and system APIs. Includes GUI scripting, accessibility inspection, screen OCR, HomeKit, and meeting recording.

### iMessage Tools (3)

Send, read, and search iMessage conversations directly from the agent.

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
- **Budget tracking** — Real-time budget warnings with per-tool token breakdown showing top consumers
- **Decision explanations** — Every tool call, safety denial, and contract violation produces a reviewable `DecisionExplanation` with reasoning, confidence, and alternatives
- **Tool execution previews** — Auto-generated previews for destructive tools (diffs for file writes, command preview for shell, staged diff for commits) shown before approval
- **Trust calibration** — `/trust` command and Ctrl+S overlay showing per-tool approval stats and adaptive trust suggestions

## CLI Reference

```bash
# Core
rustant                                    # Interactive REPL with TUI
rustant "task"                             # Single task execution
rustant setup                              # Interactive provider setup wizard
rustant init                               # Smart project init (auto-detect type, generate config)
rustant config init                        # Create default config
rustant config show                        # Display current config

# Sessions
rustant resume [name]                      # Resume a session (most recent if no name)

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

# REPL Commands (inside interactive session)
# Session
/quit (/exit, /q)                         # Exit Rustant (prompts to save unsaved work)
/clear                                    # Clear the screen
/session save|load|list [name]            # Session management
/resume [name]                            # Resume a saved session (latest if no name)
/sessions                                 # List saved sessions with details
/sessions search <query>                  # Full-text search across session names, goals, summaries
/sessions tag <name> <tag>                # Tag a session for organization
/sessions filter <tag>                    # List sessions matching a tag

# Agent
/cost                                     # Show token usage and cost
/tools                                    # List available tools
/status                                   # Show agent status, task, and iteration count
/compact                                  # Compress conversation context to free memory
/context                                  # Show context window usage breakdown
/memory                                   # Show memory system stats
/pin [n]                                  # Pin message to survive compression
/unpin <n>                                # Unpin a message

# Safety
/safety                                   # Show current safety mode and stats
/permissions [mode]                       # View or set approval mode (safe/cautious/paranoid/yolo)
/trust                                    # Safety trust calibration dashboard
/audit show [n] | verify                  # Show audit log or verify Merkle chain integrity

# Development
/undo                                     # Undo last file operation via git checkpoint
/diff                                     # Show recent file changes
/review                                   # Review all session file changes

# System
/help [topic]                             # Categorized help, or detailed help for a topic
/keys                                     # Show keyboard shortcut overlay (TUI: F1)
/config [key] [value]                     # View or modify runtime configuration
/doctor                                   # Run diagnostic checks (LLM, tools, config, sessions)
/setup                                    # Re-run provider setup wizard
/workflows                                # List available workflow templates
/verbose (/v)                             # Toggle verbose output (tool details, usage, decisions)

# CLI-Parity Commands (equivalent to `rustant <subcommand>`)
/channel (/ch) list|setup|test <name>     # Manage messaging channels
/workflow (/wf) list|show|run|status|cancel  # Manage and run workflows
/voice speak <text> [-v voice]            # Text-to-speech synthesis
/browser test|launch|connect|status       # Browser automation control
/auth status|login|logout <provider>      # OAuth authentication management
/canvas push <type> <content>|clear|snapshot  # Canvas operations
/skill list|info|validate <path>          # Skill management (SKILL.md files)
/plugin list|info <name>                  # Plugin management
/update check|install                     # Check for and install updates
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

# Test (2,900+ tests)
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
