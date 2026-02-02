# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
