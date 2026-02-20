# Contributing to Rustant

Thank you for your interest in contributing to Rustant! This guide will help you get started.

## Development Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/DevJadhav/Rustant.git
   cd Rustant
   ```

2. **Install Rust toolchain:**
   ```bash
   rustup toolchain install stable
   rustup component add rustfmt clippy
   ```
   Or simply rely on `rust-toolchain.toml` (pins Rust 1.88.0 with components).

3. **Configure git for tests:**
   ```bash
   git config --global user.email "test@test.com"
   git config --global user.name "Test User"
   git config --global commit.gpgsign false
   ```

4. **Build and test:**
   ```bash
   cargo build --workspace
   cargo test --workspace --exclude rustant-ui
   ```

## Pull Request Process

1. Fork the repository and create a feature branch from `main`.
2. Write tests for any new functionality (TDD is encouraged).
3. Ensure all checks pass:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --exclude rustant-ui -- -D warnings
   cargo test --workspace --exclude rustant-ui
   ```
4. Write a clear PR description explaining what changed and why.
5. Submit the PR against `main`.

## Coding Standards

- **Formatting:** All code must pass `cargo fmt`. The CI enforces this.
- **Linting:** Code must be clean under `cargo clippy` with `-D warnings`.
- **Testing:** New features must include tests. Bug fixes should include regression tests.
- **Error handling:** Use `thiserror` for library errors, `anyhow` for application-level errors.
- **Logging:** Use `tracing` macros (`tracing::info!`, `tracing::debug!`, etc.).
- **Async:** All async code uses the Tokio runtime. Tools use `async-trait`.

## Workspace Structure

| Crate | Purpose |
|-------|---------|
| `rustant-core` | Agent orchestrator, brain, memory, safety, channels, gateway, personas, policy, anomaly detection |
| `rustant-tools` | 73 built-in tools (45 base + 3 iMessage + 25 macOS native) + 5 fullstack tools |
| `rustant-cli` | CLI binary (published as `rustant` on crates.io), REPL, 117 slash commands |
| `rustant-mcp` | MCP protocol server and client (JSON-RPC 2.0) |
| `rustant-plugins` | Plugin system (native .so/.dll/.dylib + WASM via wasmi) |
| `rustant-security` | Security scanning (SAST/SCA/secrets), code review, compliance, incident response (33 tools) |
| `rustant-ml` | ML/AI engineering: data, training, zoo, LLM ops, RAG, eval, inference, research (54 tools) |
| `rustant-ui` | Tauri-based desktop dashboard |

Dependency flow: `rustant-cli` → core + tools + mcp + security + ml. `rustant-mcp` → core + tools + security. `rustant-security` → core + tools. `rustant-ml` → core + tools.

## Adding a New Tool

1. Create a new file in `rustant-tools/src/` implementing the `Tool` trait.
2. Register it in `rustant-tools/src/lib.rs` via `register_builtin_tools()`.
3. Add tests for the tool.
4. Update the tool count assertions in:
   - `rustant-tools/src/lib.rs`
   - `rustant-mcp/src/lib.rs`
   - `rustant-mcp/src/handlers.rs`
   - `rustant-mcp/src/client.rs`
5. Add `parse_action_details()` handling in `rustant-core/src/agent.rs`.
6. If the tool has a risk level above read-only, add `ActionDetails` handling in `rustant-core/src/safety.rs`.

### Dual Tool Registration Pattern

Rustant uses a dual tool registration pattern:
- `ToolRegistry` in `rustant-tools` holds all tool definitions
- `Agent` in `rustant-core` has its own `HashMap<String, RegisteredTool>`
- `register_agent_tools_from_registry()` bridges these using `Arc<ToolRegistry>` as a generic fallback executor

When adding tools, ensure they are registered in the `ToolRegistry` — the bridge function handles the rest.

### Testing Patterns

- `ToolOutput::text()` is a **constructor**, not a getter. Access content via `.content` field.
- `TempDir::new()` on macOS creates symlinked paths. Always use `dir.path().canonicalize().unwrap()` as the workspace.
- Tests using `AgentConfig::default()` must set `config.llm.use_streaming = false` for deterministic behavior.
- `Message.content` is a `Content` enum, not `Option<String>`. Use `.as_text()` to get `Option<&str>`.

## Adding a Security Tool

1. Create a tool wrapper in `rustant-security/src/tools/` using the existing patterns.
2. Register it in `rustant-security/src/lib.rs` via `register_security_tools()`.
3. Add `ActionDetails` variant handling in `rustant-core/src/safety.rs` if needed.
4. Add `TraceEventKind` variant in `rustant-core/src/audit.rs` if it produces auditable events.
5. Update security tool count assertions.

## Adding an ML Tool

1. Create a tool wrapper in `rustant-ml/src/tools/` using the `ml_tool!` macro.
2. Register it in `rustant-ml/src/lib.rs` via `register_ml_tools()`.
3. Add `ActionDetails` variant handling if the tool modifies external state.
4. Update ML tool count assertions.

## Adding a New Channel

1. Create a new module in `rustant-core/src/channels/`.
2. Implement the `Channel` trait.
3. Register it in `rustant-core/src/channels/mod.rs` via `build_channel_manager()`.
4. Add integration tests.

## Performance Guidelines

When contributing to Rustant, follow these patterns to maintain the optimized performance characteristics:

### Token Efficiency
- **MoE Expert Assignment**: When adding a new tool, assign it to the appropriate expert in `rustant-core/src/moe/experts.rs` via `ExpertId::tool_names()`. Every tool must belong to exactly one expert.
- **Tool Definition Caching**: The Agent caches tool definitions via `Arc<Vec<ToolDefinition>>` keyed by `TaskClassification`. Never bypass `tool_definitions()` to build definitions ad-hoc.
- **Observation Masking**: Large tool outputs are automatically masked after consumption. Keep tool output under 5000 chars when possible.

### Avoid Expensive Operations in Hot Paths
- **Batch Tantivy commits**: When indexing multiple facts, call `flush()` once at the end rather than committing per-fact. The `HybridSearchEngine` batches at 100 documents automatically.
- **Lazy initialization**: Expensive components (`ContextSummarizer`, tiktoken BPE) are lazily initialized. Don't eagerly create resources that may not be used in every session.
- **Safety fast-path**: ReadOnly tools in Safe/Yolo mode skip injection scanning. Don't mark tools as higher risk than necessary.

### State Persistence
- **Use `atomic_write_json()`** from `rustant_core::persistence` for all state files. Never use raw `std::fs::write()` for JSON state — it risks corruption on crash.
- **Atomic write pattern**: write to `.tmp` file, then `rename()` into place. The shared utility handles this.

### Tool Boilerplate
- **Use `define_tool!` macro** from `rustant-tools/src/macros.rs` for new simple tools. Use `ml_tool!` from `rustant-ml/src/tools/` for ML tools.
- **Unit struct tools** (no workspace dependency): use the no-fields `define_tool!` variant.
- **Workspace tools** (need PathBuf): use the fields variant.

### Rate Limiting
- Provider rate limits are tracked via `TokenBucketLimiter` in Brain. Response headers from Anthropic/OpenAI update limits automatically.
- `ProviderLimits` config (`itpm`, `otpm`, `rpm`) can be set per-provider in config. Values of 0 mean unlimited.

## Common Pitfalls

- When changing config defaults, update ALL test assertions (unit, integration, AND MCP tests).
- macOS-only code must use `#[cfg(target_os = "macos")]`.
- AppleScript strings must go through `sanitize_applescript_string()` for injection prevention.
- Workflow gate types: use `approval_required` not `approval` (parser rejects the short form).
- `ToolError::InvalidArguments` has fields `{ name, reason }` not `{ message }`.
- OpenAI API: never inject system messages between tool_call and tool_result.

## Reporting Issues

Use the [GitHub Issues](https://github.com/DevJadhav/Rustant/issues) page. Include:

- Rustant version (`rustant --version`)
- Operating system and version
- Steps to reproduce
- Expected vs. actual behavior
- Relevant logs (with `--verbose` flag)

## Code of Conduct

All contributors are expected to follow our [Code of Conduct](CODE_OF_CONDUCT.md).
