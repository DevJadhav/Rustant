# Introduction

Rustant is a privacy-first autonomous personal agent built in Rust. It uses a Think-Act-Observe (ReAct) loop to execute tasks via LLM reasoning, tool execution, and feedback observation.

The name: **Rust** + **Ass**istant = **Rustant**.

## Key Features

- **Multi-provider LLM support** — OpenAI, Anthropic, Gemini, Azure, Ollama, vLLM with circuit-breaker failover and prompt caching
- **159+ tools** — 72 base tools (45 cross-platform + 3 iMessage + 24 macOS native), 33 security tools, 54 ML tools across file I/O, Git, shell, search, productivity, research, cognitive extension, SRE/DevOps, fullstack development, security scanning, and ML engineering
- **13 messaging channels** — Slack, Discord, Telegram, Email, Teams, WhatsApp, Signal, Matrix, IRC, SMS, iMessage, WebChat, Webhook with CDC auto-reply and communication style learning
- **Three-tier memory** — Working, short-term, and long-term memory with auto-summarization, message pinning, and cross-session learning
- **Five-layer safety** — Input validation, authorization (4 modes + 5-level progressive trust), sandboxing, output validation, and Merkle audit trail
- **Security engine** — SAST, SCA, secrets scanning, container/IAC analysis, compliance frameworks, incident response
- **ML engine** — Data pipelines, training, model zoo, LLM fine-tuning, RAG, evaluation, inference, research
- **LLM Council** — Multi-model deliberation for planning tasks
- **Adaptive personas** — 8 task-specific personas with auto-detection and evolution
- **Voice interface** — Text-to-speech and speech-to-text via OpenAI
- **Browser automation** — Headless Chrome via CDP + MCP integration
- **Workflow engine** — 39+ templates with cron scheduling and automatic task routing
- **110+ slash commands** — Across 9+ categories with tab completion and typo suggestions
- **Plugin system** — Native and WASM plugin loading with hook points
- **MCP support** — Model Context Protocol server and client
- **Sessions** — Auto-save, resume, search, tagging, auto-recovery

## Architecture at a Glance

Rustant is organized as a Cargo workspace with eight crates:

| Crate | Purpose |
|-------|---------|
| `rustant-core` | Agent orchestrator, LLM brain, memory, safety, config, channels, gateway, personas, policy |
| `rustant-tools` | 72 built-in tool implementations (45 base + 3 iMessage + 24 macOS native) |
| `rustant-cli` | Binary entry point with CLI and REPL (110+ slash commands) |
| `rustant-mcp` | MCP server and client (JSON-RPC 2.0) |
| `rustant-plugins` | Plugin loading (native + WASM) and hook system |
| `rustant-security` | Security scanning, code review, compliance, incident response (33 tools) |
| `rustant-ml` | ML/AI engineering: data, training, zoo, LLM ops, RAG, eval, inference, research (54 tools) |
| `rustant-ui` | Tauri-based dashboard UI |

## Philosophy

Rustant is designed around these principles:

1. **Privacy first** — Your data stays local. No telemetry, no cloud dependencies unless you choose them.
2. **Safety by default** — Every tool execution goes through approval. Progressive trust lets the agent earn autonomy over time.
3. **Extensible** — Plugins, skills, MCP, and custom tools allow extending capabilities without modifying core code.
4. **Transparent** — Tamper-evident audit trails via Merkle chain verification. Decision explanations for every action.
