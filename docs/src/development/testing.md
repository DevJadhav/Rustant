# Testing Guide

## Overview

Rustant has ~2,800 tests across the workspace, including unit tests, integration tests, and property tests.

## Running Tests

```bash
# All tests
cargo test --workspace --exclude rustant-ui

# Specific crate
cargo test -p rustant-core
cargo test -p rustant-tools
cargo test -p rustant-security
cargo test -p rustant-ml
cargo test -p rustant-mcp
cargo test -p rustant

# Specific test
cargo test -p rustant-core agent::tests::test_name

# With feature flags
cargo test -p rustant-security --features sast-all
cargo test -p rustant-core --no-default-features
```

## Property Tests

28 property tests in `rustant-core/tests/proptest_core.rs` covering:
- Version parsing roundtrips
- Merkle chain integrity
- Memory compression invariants
- Injection detection patterns
- Skill parsing
- Cron scheduling
- Credential resolution
- Cache behavior
- Persona selection
- Embedding consistency

Run with more cases:
```bash
PROPTEST_CASES=500 cargo test -p rustant-core --test proptest_core
```

## MockLlmProvider

For testing agent behavior without LLM calls:

```rust
let mock = MockLlmProvider::with_response("test response");
// Pre-queues 20 copies for multi-call tests
```

Key behaviors:
- `complete_streaming()` adds trailing space to each word token
- Tests using `AgentConfig::default()` must set `config.llm.use_streaming = false`

## Common Pitfalls

### Content Type
`Message.content` is a `Content` enum, not `Option<String>`:
```rust
// Correct
message.as_text()  // → Option<&str>

// Wrong
message.content.as_deref()  // Won't compile
```

### TempDir on macOS
macOS creates symlinked temp paths. Always canonicalize:
```rust
let dir = TempDir::new().unwrap();
let workspace = dir.path().canonicalize().unwrap();
// Use workspace.join(...) NOT dir.path().join(...)
```

### ToolOutput
`ToolOutput::text()` is a **constructor**, not a getter:
```rust
let output = ToolOutput::text("result");  // Creates new ToolOutput
let content = output.content;              // Access the string
```

### File Write Assertions
```rust
// Correct
assert!(result.content.contains("Created"));  // or "bytes"

// Wrong
assert!(result.content.contains("Written"));
```

### Tool Count Assertions
When adding tools, update counts in 4 files:
- `rustant-tools/src/lib.rs`
- `rustant-mcp/src/lib.rs`
- `rustant-mcp/src/handlers.rs`
- `rustant-mcp/src/client.rs`

### Config Test Race Condition
`test_load_config_from_workspace` and `test_env_var_override_approval_mode` share `ENV_MUTEX`. Don't add parallel config tests without the mutex.

## Test Organization

| Crate | Test Location | Focus |
|-------|-------------|-------|
| rustant-core | `src/` (inline), `tests/` | Agent loop, memory, safety, config |
| rustant-tools | `src/` (inline) | Individual tool execution |
| rustant-security | `src/` (inline) | Scanners, compliance, findings |
| rustant-ml | `src/` (inline) | ML pipelines, tools |
| rustant-mcp | `src/` (inline) | Protocol, handlers, client |
| rustant-cli | `src/` (inline) | Slash commands, REPL |
