# Foundational Pillars

Rustant is built on four foundational pillars that inform every design decision: Safety, Security, Transparency, and Interpretability.

## Safety

The safety system provides five layers of protection:

1. **Input Validation** -- Prompt injection detection (`injection.rs`) with 12-category regex scanning
2. **Authorization** -- `SafetyGuardian` with modes: Safe (default), Cautious, Paranoid, Yolo
3. **Sandbox** -- WASM (wasmi) + filesystem (cap-std) containment
4. **Output Validation** -- Secret redaction before display
5. **Audit** -- Merkle chain tamper-evident logging

### Progressive Trust

`TrustLevel` tracks agent reliability over time:

| Level | Name | Permissions |
|-------|------|-------------|
| 0 | Shadow | Observe only |
| 1 | DryRun | Preview actions |
| 2 | Assisted | Execute with approval |
| 3 | Supervised | Auto-approve read-only |
| 4 | SelectiveAutonomy | Auto-approve most actions |

Promotion requires meeting criteria: minimum successful actions, maximum error rate, minimum time at level. Demotion triggers on destructive failures or circuit breaker trips.

### Dynamic Risk Scoring

`DynamicRiskScorer` adjusts risk levels based on context:

- **Late night** (23:00-06:00): Write escalates to Execute, Execute to Destructive
- **High errors** (3+): Write and above escalate one level
- **Active incident**: Deployment tools escalate to Destructive
- **Circuit breaker open**: Only ReadOnly actions permitted
- **Production environment**: Network actions escalate to Destructive

Custom `RiskModifier` rules can be scoped to AllTools, SpecificTool, or RiskLevel.

### Global Circuit Breaker

Closed/Open/HalfOpen states with sliding window (5 min default). Opens on 3 consecutive failures or 50% failure rate. HalfOpen allows only ReadOnly actions.

## Security

### Secret Redaction

`SecretRedactor` applies 60+ regex patterns plus Shannon entropy analysis (threshold 4.5) to all:
- LLM context (before sending to providers)
- Memory entries (before persistence)
- Audit trail entries
- MCP output
- Log messages

### Merkle Chain Audit

Enabled by default. Every agent action is recorded as a `TraceEvent` in a tamper-evident Merkle chain. Each event includes a hash linking to the previous event, making any tampering detectable.

### ML Safety Bridge

`MlSafetyBridge` provides additional safety checks for ML operations:
- Detects PII in training data file paths
- Flags large model downloads for review
- Requires alignment review for fine-tuning operations

## Transparency

### Data Flow Tracking

`DataFlowTracker` records every data movement through the system:

| Source | Destination | Tracked |
|--------|-------------|---------|
| User Input | LLM Provider | Provider name, model, token count |
| Tool Output | LLM Provider | Tool name, data type |
| File Content | Tool Execution | File path, redaction status |
| Memory Fact | LLM Provider | Fact type |
| Siri Voice | Daemon | Command text |

Ring buffer (max 10,000 entries) with persistence to `.rustant/data_flows.json`. Query via `/dataflow` command.

### User Consent Framework

`ConsentManager` manages consent records per scope:

| Scope | Example |
|-------|---------|
| `Provider{provider}` | Consent to send data to Anthropic |
| `LocalStorage` | Consent to persist data locally |
| `MemoryRetention` | Consent to retain long-term memory |
| `ToolAccess{tool}` | Consent for specific tool usage |
| `ChannelAccess{channel}` | Consent for messaging channel |
| `Global` | Blanket consent (fallback) |

Consent records include TTL, can be granted/revoked via `/consent` command, and persist across sessions.

## Interpretability

### Agent Decision Log

`DecisionLog` records every agent decision with full context:

```
DecisionEntry {
    id, timestamp, iteration,
    action: "shell_exec rm -rf /tmp/cache",
    reasoning: "User requested cleanup of cache directory",
    alternatives: ["file_delete individual files", "compress then delete"],
    risk_level: "Destructive",
    confidence: 0.85,
    outcome: UserApproved,
    expert: Some(DevOps),
    persona: Some(General),
    source: Some("repl"),
}
```

Query recent decisions via `/decisions` command. Supports filtering by iteration, expert, and outcome.

### Decision Outcomes

| Outcome | Meaning |
|---------|---------|
| AutoApproved | Safety system approved automatically |
| UserApproved | User confirmed the action |
| UserDenied | User rejected the action |
| SafetyDenied | Safety system blocked the action |
| Succeeded | Action completed successfully |
| Failed | Action failed during execution |

### MoE Expert Routing Transparency

When MoE is enabled, each task classification and expert routing decision is logged, showing why a particular expert was chosen and what tools were made available.
