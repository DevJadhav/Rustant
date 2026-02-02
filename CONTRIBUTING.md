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

3. **Configure git for tests:**
   ```bash
   git config --global user.email "test@test.com"
   git config --global user.name "Test User"
   git config --global commit.gpgsign false
   ```

4. **Build and test:**
   ```bash
   cargo build --workspace
   cargo test --workspace
   ```

## Pull Request Process

1. Fork the repository and create a feature branch from `main`.
2. Write tests for any new functionality (TDD is encouraged).
3. Ensure all checks pass:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
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
| `rustant-core` | Core library |
| `rustant-tools` | Built-in tools |
| `rustant-cli` | CLI binary |
| `rustant-mcp` | MCP protocol |
| `rustant-plugins` | Plugin system |
| `rustant-ui` | Dashboard UI |

## Adding a New Tool

1. Create a new file in `rustant-tools/src/` implementing the `Tool` trait.
2. Register it in `rustant-tools/src/lib.rs` via `register_builtin_tools()`.
3. Add tests for the tool.
4. Update documentation if the tool is user-facing.

## Adding a New Channel

1. Create a new module in `rustant-core/src/channels/`.
2. Implement the `Channel` trait.
3. Register it in `rustant-core/src/channels/mod.rs` via `build_channel_manager()`.
4. Add integration tests.

## Reporting Issues

Use the [GitHub Issues](https://github.com/DevJadhav/Rustant/issues) page. Include:

- Rustant version (`rustant --version`)
- Operating system and version
- Steps to reproduce
- Expected vs. actual behavior
- Relevant logs (with `--verbose` flag)

## Code of Conduct

All contributors are expected to follow our [Code of Conduct](CODE_OF_CONDUCT.md).
