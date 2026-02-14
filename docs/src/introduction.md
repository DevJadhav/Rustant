# Introduction

Rustant is a privacy-first autonomous personal agent built in Rust. It uses a Think-Act-Observe (ReAct) loop to execute tasks via LLM reasoning, tool execution, and feedback observation.

The name: **Rust** + **Ass**istant = **Rustant**.

## Key Features

- **Multi-provider LLM support** — OpenAI, Anthropic, Gemini, Azure, Ollama, vLLM with circuit-breaker failover
- **39 built-in tools** — File I/O, Git, shell execution, search, productivity, research, cognitive extension (knowledge graph, experiments, code intelligence, content, skills, career, monitoring, life planning, privacy, self-improvement), and more
- **13 messaging channels** — Slack, Discord, Telegram, Email, Teams, WhatsApp, Signal, Matrix, IRC, SMS, iMessage, WebChat, Webhook
- **Three-tier memory** — Working, short-term, and long-term memory with auto-summarization
- **Five-layer safety** — Input validation, authorization, sandboxing, output validation, and audit trail
- **Voice interface** — Text-to-speech and speech-to-text via OpenAI
- **Browser automation** — Headless Chrome via CDP
- **Workflow engine** — Declarative multi-step workflows with cron scheduling
- **Canvas system** — Rich content rendering (charts, tables, forms, diagrams)
- **Plugin system** — Native and WASM plugin loading with hook points
- **Skills** — SKILL.md files for declarative tool definitions
- **MCP support** — Model Context Protocol server and client

## Architecture at a Glance

Rustant is organized as a Cargo workspace with six crates:

| Crate | Purpose |
|-------|---------|
| `rustant-core` | Agent orchestrator, LLM brain, memory, safety, config, channels, gateway |
| `rustant-tools` | Built-in tool implementations |
| `rustant-cli` | Binary entry point with CLI, REPL, and TUI |
| `rustant-mcp` | MCP server and client |
| `rustant-plugins` | Plugin loading (native + WASM) and hook system |
| `rustant-ui` | Tauri-based dashboard UI |

## Philosophy

Rustant is designed around these principles:

1. **Privacy first** — Your data stays local. No telemetry, no cloud dependencies unless you choose them.
2. **Safety by default** — Every tool execution goes through approval. Sandboxing prevents unintended system access.
3. **Extensible** — Plugins, skills, and MCP allow extending capabilities without modifying core code.
4. **Transparent** — Tamper-evident audit trails via Merkle chain verification.
