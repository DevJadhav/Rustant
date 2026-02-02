# Rustant

A high-performance, privacy-first autonomous personal agent built entirely in Rust.

**Rust** + **Assistant** = **Rustant** — like an industrious ant, small but capable of carrying workloads many times its size.

## Overview

Rustant is an LLM-powered coding agent that executes complex tasks through a Think-Act-Observe loop while maintaining strict safety guarantees. It operates locally with optional cloud features, giving developers complete control over their data and execution environment.

### Core Differentiators

- **Transparent Autonomy** — Every decision is logged and reviewable
- **Progressive Control** — From "suggest only" to "full autonomy with audit"
- **Adaptive Context Engineering** — Smart compression for long sessions
- **Git-Native Safety** — All file operations are reversible through automatic checkpointing
- **Zero-Cloud Option** — Complete functionality without any data leaving the machine
- **Rust Performance** — Sub-millisecond tool dispatch, minimal memory footprint

## Architecture

```
rustant/
├── rustant-core/     # Agent orchestrator, LLM brain, memory, safety guardian
├── rustant-tools/    # Built-in tools: file ops, git, shell, search
├── rustant-cli/      # CLI application with REPL interface
└── rustant-mcp/      # MCP server (Phase 2)
```

### Key Components

| Component | Description |
|-----------|-------------|
| **Brain** | LLM provider abstraction with streaming, cost tracking, and model switching |
| **Memory** | Three-tier system: working (task), short-term (session), long-term (persistent) |
| **Safety Guardian** | 5-layer defense-in-depth with configurable approval modes |
| **Tool Registry** | Dynamic tool registration with JSON schema validation and timeouts |
| **Agent Loop** | ReAct pattern (Think → Act → Observe) with async execution |

### Built-in Tools

| Tool | Risk Level | Description |
|------|------------|-------------|
| `file_read` | Read-only | Read file contents with optional line range |
| `file_list` | Read-only | List directory contents (respects .gitignore) |
| `file_search` | Read-only | Search for text patterns across files |
| `file_write` | Write | Create or overwrite files |
| `file_patch` | Write | Apply targeted text replacements |
| `git_status` | Read-only | Show repository status |
| `git_diff` | Read-only | Show working tree changes |
| `git_commit` | Write | Stage and commit changes |
| `shell_exec` | Execute | Run shell commands |

## Safety Model

Four approval modes control agent autonomy:

| Mode | Auto-approved | Requires Approval |
|------|--------------|-------------------|
| **Safe** (default) | Read-only | All writes, executes, network |
| **Cautious** | Read-only + writes | Execute, network, destructive |
| **Paranoid** | Nothing | Everything |
| **Yolo** | Everything | Nothing |

Explicit deny lists always override approval modes — paths like `.env*`, `**/*.key`, and commands like `sudo` are always blocked.

## Getting Started

### Build

```bash
cargo build --workspace
```

### Run Tests

```bash
cargo test --workspace
```

### Usage

```bash
# Interactive REPL
cargo run --bin rustant

# Single task
cargo run --bin rustant -- "refactor the auth module"

# With options
cargo run --bin rustant -- --model gpt-4o --approval cautious --workspace ./project
```

### Configuration

Create `.rustant/config.toml` in your workspace or run:

```bash
cargo run --bin rustant -- config init
```

## License

MIT
