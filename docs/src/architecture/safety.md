# Safety & Trust

Rustant implements a comprehensive safety system with five defense layers, progressive trust, circuit breakers, and a policy engine.

## Five Layers

### 1. Input Validation

- Prompt injection detection (`injection.rs`) scans for known attack patterns, role confusion, encoded instructions
- Path traversal prevention blocks `../` and absolute path escape attempts
- Command injection detection identifies dangerous shell metacharacters

### 2. Authorization

The `SafetyGuardian` controls what actions are permitted:

- **Approval modes** govern user interaction requirements (see below)
- **Deny lists** block specific paths and commands
- **Risk levels** categorize tools as read-only, write, or execute
- **Typed ActionDetails** — Tool arguments are parsed into specific variants (FileRead, FileWrite, ShellCommand, GitOperation, BrowserAction, NetworkRequest, SecurityScan, ModelInference, etc.) via `parse_action_details()`, producing `ApprovalContext` with reasoning, alternatives, consequences, and reversibility info

### 3. Sandboxing

- **Filesystem sandbox** via `cap-std` restricts file access to the workspace directory
- **WASM sandbox** via `wasmi` for plugin execution in an isolated environment

### 4. Output Validation

- Sensitive data detection (API keys, passwords)
- Output size limits (10MB max) prevent memory exhaustion
- Content filtering for injection in tool results
- Secret redaction (60+ regex patterns + Shannon entropy) applied to all LLM context

### 5. Audit Trail

- Merkle chain verification ensures log integrity
- Each entry includes timestamp, tool name, arguments, result, and approval status
- Security-specific variants: SecurityScanCompleted, FindingDetected, FindingSuppressed, PolicyEvaluated, ComplianceReportGenerated, IncidentActionTaken

## Approval Modes

| Mode | Read | Write | Execute | Description |
|------|------|-------|---------|-------------|
| Safe | Auto | Prompt | Prompt | Default. Reads are automatic. |
| Cautious | Auto | Auto | Prompt | Writes allowed, shell/network prompted |
| Paranoid | Prompt | Prompt | Prompt | Prompt for everything |
| Yolo | Auto | Auto | Auto | Auto-approve all (development only) |

## Progressive Trust

The `TrustLevel` system provides graduated autonomy beyond the 4 approval modes:

| Level | Value | Behavior |
|-------|-------|----------|
| Shadow | 0 | Agent suggests actions, human executes |
| DryRun | 1 | Agent executes in dry-run mode |
| Assisted | 2 | Agent executes with per-action approval |
| Supervised | 3 | Agent executes, human monitors |
| SelectiveAutonomy | 4 | Agent executes autonomously for trusted operations |

Escalation criteria: minimum successful actions, maximum error rate, minimum hours at level. Demotion triggers: destructive failures, circuit breaker trips.

```
/trust              # Show current trust level and history
/trust promote      # Request trust level escalation
/trust demote       # Lower trust level
```

## Global Circuit Breaker

A sliding-window circuit breaker tracks recent action results:

| State | Behavior |
|-------|----------|
| Closed | Normal operation |
| Open | All non-read actions blocked |
| HalfOpen | Only read-only actions permitted during recovery |

Opens on 3 consecutive failures or 50% failure rate within a 5-minute window.

```
/circuit status     # Show circuit breaker state
/circuit open       # Force open
/circuit close      # Force close
/circuit reset      # Reset to closed
```

## Rollback Registry

The `RollbackRegistry` tracks reversible actions with undo information:
- Maximum 100 entries (VecDeque)
- Methods: `find_by_tool()`, `find_reversible()`, `mark_rolled_back()`
- Integrated with `/undo` command for git checkpoint rollback

## Policy Engine

Custom policies via `.rustant/policies.toml`:

```toml
[[policies]]
name = "no-deploys-at-night"
tools = ["shell_exec", "kubernetes"]
predicates = [
  { type = "TimeWindow", start = "22:00", end = "06:00", action = "deny" }
]

[[policies]]
name = "require-consensus-for-destructive"
tools = ["shell_exec"]
predicates = [
  { type = "MaxBlastRadius", threshold = 5 },
  { type = "RequiresConsensus", min_approvers = 2 }
]
```

Available predicates:
- `TimeWindow` — Allow/deny based on time of day
- `MaxBlastRadius` — Limit scope of impact
- `MinTrustLevel` — Require minimum trust level
- `RequiresConsensus` — Multi-approver requirement
- `MaxConcurrentDeployments` — Limit parallel deployments

## Credential Storage

Credentials use the OS keyring via the `keyring` crate (macOS Keychain, Linux Secret Service, Windows Credential Manager).

The `SecretRef` type provides unified secret resolution:
- `"keychain:<account>"` — resolve from OS keychain
- `"env:<VAR_NAME>"` — resolve from environment variable
- Plain string — inline plaintext (deprecated, emits warnings)

Migrate plaintext to keychain: `rustant setup migrate-secrets`

## Configuration

```toml
[safety]
approval_mode = "safe"
max_iterations = 50
denied_paths = ["/etc/shadow", "/root/.ssh"]
denied_commands = ["rm -rf /", "mkfs", "dd if=/dev/zero"]

[feature_flags]
progressive_trust = false
global_circuit_breaker = true
```
