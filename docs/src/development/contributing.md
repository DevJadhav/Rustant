# Contributing

See the main [CONTRIBUTING.md](https://github.com/DevJadhav/Rustant/blob/main/CONTRIBUTING.md) for complete development setup, coding standards, PR process, and workspace structure.

Key points:
- Rust 1.85+ required (pinned via `rust-toolchain.toml`)
- Git config needed for tests: `user.email`, `user.name`, `commit.gpgsign false`
- All code must pass `cargo fmt` and `cargo clippy -D warnings`
- TDD encouraged; new features must include tests
- 8 crate workspace: core, tools, cli, mcp, plugins, security, ml, ui
