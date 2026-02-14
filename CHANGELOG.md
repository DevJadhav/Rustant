# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **LLM Council** — Multi-model deliberation feature inspired by [karpathy/llm-council](https://github.com/karpathy/llm-council). Three-stage protocol: (1) parallel query to all council members, (2) optional anonymous peer review, (3) chairman synthesis. Supports 3+ cloud providers (OpenAI, Anthropic, Gemini) or 3+ Ollama models. Auto-detection of available providers via env vars and Ollama API. New `/council` slash command with `status`, `detect`, and direct question subcommands. Configured via `[council]` in config.toml with `VotingStrategy` (chairman_synthesis, highest_score, majority_consensus), per-member weights, and cost controls
- **TUI status bar metrics** — Bottom status bar now shows live token count and cost (`12.4k/128k | $0.0342`) alongside input mode hints
- **TUI sidebar iteration tracking** — Sidebar iteration counter (`Iteration: 1/50`) updates in real-time during agent execution via new `on_iteration_start` callback
- **Voice direct audio playback** — `/voice speak "text"` plays audio through speakers via macOS `afplay` or Linux `aplay` instead of saving to a temp file
- **Voice wake word mode** — `rustant --voice` activates "hey rustant" wake word listening: records mic, detects wake word via STT, transcribes commands, processes through agent, speaks response via TTS
- **Chrome DevTools MCP integration** — External MCP server support via `[[mcp_servers]]` config; `ProcessTransport` spawns and communicates with child processes over NDJSON stdin/stdout; Chrome DevTools MCP (`npx -y chrome-devtools-mcp@latest`) provides 26 browser automation tools (click, navigate, evaluate_script, performance traces, network inspection, screenshots)
- **External MCP server config** — `ExternalMcpServerConfig` struct in `AgentConfig` for configuring external MCP servers with command, args, env, working directory, and auto-connect toggle

### Fixed

- **Gemini provider hang** — Added 120s HTTP timeout and 10s connect timeout to the Gemini HTTP client (was zero timeout causing indefinite hangs). Replaced buffered SSE streaming (`response.text().await`) with true incremental streaming via `response.bytes_stream()` + line-by-line parsing, so tokens appear immediately instead of after the entire response completes. Added `warn!()` logging for malformed SSE JSON chunks (previously silently swallowed). Defensive `fix_gemini_turns()` now filters out turns with empty `parts` arrays to prevent Gemini API 400 errors

### Changed

- **Safety transparency dashboard** — `ExplanationPanel` TUI widget (Ctrl+E) showing decision reasoning chains, considered alternatives, confidence scores, and context factors with a navigable timeline
- **Streaming progress pipeline** — `ProgressBar` TUI widget with spinner animation, tool name, elapsed time, completion gauge, and scrollable shell output
- **Multi-agent task board** — `TaskBoard` TUI widget (Ctrl+T) showing spawned agent status, roles, current tool, elapsed time, tool call counts, and token usage with detail panel
- **Project context auto-indexer** — `ProjectIndexer` walks workspace respecting .gitignore, indexes file paths, content summaries, and function signatures (Rust, Python, JS/TS, Go, Java, Ruby, C/C++) into hybrid search engine
- **Codebase search tool** — `codebase_search` tool for natural language search over indexed project files, functions, and content
- **Zero-config quick start** — `rustant init` command with auto-detection for 8 project types (Rust, Node, Python, Go, Java, Ruby, C#, C++), framework detection, safety whitelist generation, and example tasks
- **Web search tool** — `web_search` tool using DuckDuckGo instant answers (privacy-first, no API key required)
- **Web fetch tool** — `web_fetch` tool to fetch URLs and extract readable text with HTML tag stripping and entity decoding
- **Document read tool** — `document_read` tool for reading local documents (txt, md, csv, json, yaml, toml, xml, html, and 20+ formats)
- **Smart edit tool** — `smart_edit` tool with 4 location strategies (exact match, line numbers, function patterns, fuzzy similarity), edit operations (replace, insert, delete), unified diff preview, and auto-checkpoint
- **Session management** — `SessionManager` with auto-save, resume by name or ID (fuzzy prefix match), list/delete/rename sessions, and task continuation prompts
- **Session REPL commands** — `/resume`, `/sessions`, `/session save|load|list` commands for session management in interactive mode
- **Message pinning** — `/pin` and `/unpin` in short-term memory; pinned messages survive context compression and are always included in the context window
- **Daily workflow templates** — 4 new built-in workflows: `morning_briefing`, `pr_review`, `dependency_audit`, `changelog` (12 total)
- **Workflows REPL command** — `/workflows` command listing all available workflow templates with descriptions, inputs, and usage examples
- **Slash command registry** — `CommandRegistry` with 24 commands across 5 categories (Session, Agent, Safety, Development, System), alias resolution, tab completion, and Levenshtein-based "did you mean?" suggestions for typos
- **Agent clarification mechanism** — `ask_user` pseudo-tool lets the LLM ask clarifying questions mid-task; routed through `AgentCallback::on_clarification_request` to REPL (stdin) and TUI (input panel) with oneshot reply channels
- **`/compact`** — Manually compress conversation context via `smart_fallback_summary`, freeing memory while preserving structure
- **`/status`** — Show agent status, current goal, iteration count, token usage, and cost
- **`/config [key] [value]`** — View or modify runtime configuration (model, approval_mode, max_iterations)
- **`/doctor`** — Run diagnostic checks on workspace, git, config, LLM provider, tools, memory, and audit chain
- **`/permissions [mode]`** — View or set approval mode at runtime (safe/cautious/paranoid/yolo)
- **`/undo`** — Undo last file operation via git checkpoint (ported from TUI to REPL)
- **`/diff`** — Show file changes since last checkpoint
- **`/review`** — Review all session file changes across checkpoints
- **Categorized `/help`** — Replaced hardcoded help text with registry-generated categorized output including aliases
- **Gemini API sequencing fix** — `fix_gemini_turns()` post-processing merges consecutive same-role turns, fixes `functionResponse.name` to match `functionCall.name`, and ensures user-first message ordering (fixes HTTP 400 errors with multi-tool calls)
- **CLI-REPL command parity** — 9 CLI subcommands now available as REPL slash commands: `/channel` (`/ch`), `/workflow` (`/wf`), `/voice`, `/browser`, `/auth`, `/canvas`, `/skill`, `/plugin`, `/update`
- **Workflow routing** — `workflow_routing_hint()` in agent automatically detects task patterns matching built-in workflows (security_scan, code_review, test_generation, etc.) and guides the LLM to run or accomplish them. Platform-independent, works on all OSes
- **Workflow catalog in system prompt** — Agent now knows about all 17 built-in workflow templates and can route tasks to them
- **`/verbose` toggle** — `/verbose` (alias `/v`) toggles verbose output in REPL, controlling visibility of tool execution details, status changes, and decision explanations
- **TUI as default** — `use_tui: true` is now the default; use `--no-tui` for simple REPL
- **Interactive REPL input** — `repl_input.rs` provides line editing, history, and tab completion
- **Centralized model pricing** — `model_pricing()` in `models.rs` covers OpenAI, Anthropic, Gemini, and Ollama models

## [1.0.0] - 2026-02-02

### Added

- **Agent core** — Think-Act-Observe (ReAct) loop with configurable max iterations
- **Multi-provider LLM** — OpenAI, Anthropic, Gemini, Azure, Ollama, vLLM support
- **Failover provider** — Circuit-breaker failover across multiple LLM backends
- **12 built-in tools** — file_read, file_list, file_search, file_write, file_patch, git_status, git_diff, git_commit, shell_exec, echo, datetime, calculator
- **LSP tools** — Language Server Protocol integration for code intelligence
- **Three-tier memory** — Working, short-term, and long-term memory with auto-summarization
- **Five-layer safety** — Input validation, authorization, sandboxing, output validation, audit trail
- **Prompt injection detection** — Pattern-based scanning for known attack vectors
- **Merkle chain audit** — Tamper-evident execution history
- **12 messaging channels** — Slack, Discord, Telegram, Email, Matrix, Signal, WhatsApp, SMS, IRC, Teams, iMessage, WebChat
- **Slack deep integration** — Send, history, channels, users, reactions, DMs, threads, files, teams, groups
- **OAuth authentication** — Browser-based OAuth flows with token refresh
- **Credential storage** — OS keyring integration (macOS Keychain, Linux Secret Service, Windows Credential Manager)
- **Workflow engine** — Declarative multi-step workflows with inputs, outputs, and gates
- **Cron scheduler** — Cron-based task scheduling with background job management
- **Voice interface** — Text-to-speech and speech-to-text via OpenAI
- **Browser automation** — Headless Chrome via CDP
- **Canvas system** — Rich content rendering (charts, tables, forms, diagrams via Mermaid)
- **Skills system** — SKILL.md-based declarative tool definitions with security validation
- **Plugin system** — Native (libloading) and WASM (wasmi) plugin loading
- **Hook system** — 7 hook points for plugin interception of agent behavior
- **MCP server** — Model Context Protocol server via JSON-RPC 2.0
- **MCP client** — Connect to external MCP servers for tool discovery
- **WebSocket gateway** — Remote access with session management and REST API
- **Multi-agent** — Agent spawning, message bus, routing, orchestration
- **Hybrid search** — Tantivy full-text + SQLite vector search
- **TUI interface** — ratatui-based terminal UI
- **Dashboard UI** — Tauri-based desktop dashboard (rustant-ui)
- **Self-update** — GitHub Releases-based update checking and binary replacement
- **Cross-platform CI** — GitHub Actions with Linux + macOS testing
- **Security audit** — cargo-audit in CI pipeline
- **Shell installer** — curl-based installer for Linux/macOS
- **Homebrew formula** — macOS package installation
- **cargo-binstall** — Pre-built binary installation support
- **mdbook documentation** — User guide, architecture docs, plugin development guide
