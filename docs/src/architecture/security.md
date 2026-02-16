# Security Model

Rustant implements a five-layer defense system to prevent unintended actions.

## Five Layers

### 1. Input Validation

All inputs are validated before processing:

- Prompt injection detection scans for known attack patterns
- Path traversal prevention blocks `../` and absolute path escape attempts
- Command injection detection identifies dangerous shell metacharacters

### 2. Authorization

The `SafetyGuardian` controls what actions are permitted:

- **Approval modes** govern user interaction requirements
- **Deny lists** block specific paths and commands
- **Risk levels** categorize tools as read-only, write, or execute
- **Typed ActionDetails** — Tool arguments are parsed into specific variants (FileRead, FileWrite, ShellCommand, GitOperation) via `parse_action_details()`, producing `ApprovalContext` with reasoning, alternatives, consequences, and reversibility info instead of generic fallbacks

### 3. Sandboxing

Two sandboxing mechanisms:

- **Filesystem sandbox** via `cap-std` restricts file access to the workspace directory
- **WASM sandbox** via `wasmi` for plugin execution in an isolated environment

### 4. Output Validation

Tool outputs are checked before being returned to the agent:

- Sensitive data detection (API keys, passwords)
- Output size limits prevent memory exhaustion
- Content filtering for injection in tool results

### 5. Audit Trail

Every action is recorded in a tamper-evident audit log:

- Merkle chain verification ensures log integrity
- Each entry includes timestamp, tool name, arguments, result, and approval status
- Audit log is queryable for compliance and debugging

## Approval Modes

| Mode | Read | Write | Execute | Description |
|------|------|-------|---------|-------------|
| Safe | Auto | Prompt | Prompt | Default. Reads are automatic. |
| Cautious | Prompt | Prompt | Prompt | Prompt for most operations |
| Paranoid | Prompt | Prompt | Prompt | Prompt for everything |
| Yolo | Auto | Auto | Auto | Auto-approve all (development only) |

Budget warnings and exceeded notifications are surfaced to users in real-time via the `AgentCallback` interface. The CLI displays colored warnings (yellow for warnings, red for exceeded), and the TUI shows budget events in the conversation stream.

## Prompt Injection Detection

The `injection.rs` module scans for known prompt injection patterns:

- System prompt override attempts
- Role confusion attacks
- Encoded/obfuscated instructions
- Multi-turn manipulation patterns

## Credential Storage

Credentials are stored using the OS keyring via the `keyring` crate:

- macOS: Keychain
- Linux: Secret Service (GNOME Keyring, KWallet)
- Windows: Credential Manager

OAuth tokens include refresh token support with automatic expiration tracking.

### SecretRef

The `SecretRef` type provides a unified way to reference secrets:

- `"keychain:<account>"` — resolve from OS keychain
- `"env:<VAR_NAME>"` — resolve from environment variable
- Plain string — inline plaintext (deprecated, emits warnings)

Channel tokens can be migrated from plaintext to keychain:

```bash
rustant setup migrate-secrets
```

This scans all channel configs for plaintext tokens, stores them in the OS keychain under `channel:{type}:{field}` namespaces, and replaces the config values with `SecretRef::keychain(...)` references.

## API Rate Limiting

All LLM providers implement automatic retry with exponential backoff for transient errors (429 rate limits, timeouts, connection failures). The `RetryConfig` is configurable:

```toml
[llm.retry]
max_retries = 3
initial_backoff_ms = 1000
max_backoff_ms = 60000
backoff_multiplier = 2.0
jitter = true
```

Non-retryable errors (authentication failures, parse errors) fail immediately without retry.

## Configuration

```toml
[safety]
approval_mode = "safe"
max_iterations = 50
denied_paths = ["/etc/shadow", "/root/.ssh"]
denied_commands = ["rm -rf /", "mkfs", "dd if=/dev/zero"]
```
