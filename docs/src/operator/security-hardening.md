# Security Hardening

## Approval Modes

Start with `paranoid` mode and gradually relax as you build confidence:

```toml
[safety]
approval_mode = "paranoid"   # Prompt for everything
denied_paths = [".env*", "**/*.key", "**/*.pem", "/etc/**", "/root/**"]
denied_commands = ["sudo", "rm -rf", "mkfs", "dd", "chmod 777"]
```

## Progressive Trust

Enable graduated autonomy:

```toml
[feature_flags]
progressive_trust = true
global_circuit_breaker = true
```

The agent starts at `Shadow` level and must earn trust through successful actions.

## Policy Engine

Define custom policies in `.rustant/policies.toml`:

```toml
[[policies]]
name = "no-production-writes"
tools = ["shell_exec", "file_write"]
predicates = [
  { type = "TimeWindow", start = "20:00", end = "08:00", action = "deny" },
  { type = "MinTrustLevel", level = "Supervised" }
]

[[policies]]
name = "deployment-consensus"
tools = ["kubernetes"]
predicates = [
  { type = "RequiresConsensus", min_approvers = 2 },
  { type = "MaxConcurrentDeployments", limit = 1 }
]
```

## Credential Security

- Use `keychain:` references instead of plaintext tokens
- Migrate existing secrets: `rustant setup migrate-secrets`
- Never store API keys in config files
- Use `env:` references for CI/CD environments

## Sandboxing

- **Filesystem**: `cap-std` restricts tools to the workspace directory
- **WASM**: Plugins run in `wasmi` sandbox with declared capabilities
- **Shell**: `denied_commands` list blocks dangerous shell operations

## Audit Verification

Regularly verify the Merkle chain audit trail:

```
/audit verify    # Verify chain integrity
/audit show 50   # Show last 50 audit entries
```

## Secret Redaction

The security engine's `SecretRedactor` (60+ patterns + entropy analysis) is applied to:
- All LLM context (system prompts, tool results)
- Memory entries (facts, corrections)
- Audit log entries
- MCP output
- Error messages (home directory paths sanitized)

## Network Isolation

For zero-cloud operation:
- Use Ollama or vLLM for local LLM inference
- Disable gateway if not needed
- Disable channel polling
- Block network in denied_commands if desired
