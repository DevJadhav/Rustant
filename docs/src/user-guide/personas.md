# Adaptive Personas

Rustant uses adaptive personas to specialize agent behavior based on task type.

## Available Personas

| Persona | Focus Area |
|---------|------------|
| **General** | Default persona for general-purpose tasks |
| **Architect** | System design, architecture decisions, refactoring |
| **SecurityGuardian** | Security scanning, vulnerability analysis, compliance |
| **MlopsEngineer** | ML training, model management, data pipelines |
| **IncidentCommander** | Incident response, alert triage, escalation |
| **ObservabilityExpert** | Monitoring, metrics, logging, tracing |
| **ReliabilityEngineer** | SRE practices, reliability, capacity planning |
| **DeploymentEngineer** | Deployments, CI/CD, rollbacks, canary analysis |

## How It Works

### Auto-Detection

The `PersonaResolver` maps task classification to the most appropriate persona. For example:
- "Review the architecture of this service" → Architect
- "Scan for vulnerabilities" → SecurityGuardian
- "Train a model on this dataset" → MlopsEngineer
- "There's an outage in production" → IncidentCommander

### Effects

Each persona injects a `system_prompt_addendum` that guides the LLM's behavior and modifies confidence scoring via `confidence_modifier`.

### Evolution

The `PersonaEvolver` tracks task outcomes per persona and proposes refinements:
- Low success rate → RevisePrompt suggestion
- High iteration count → AdjustConfidence suggestion

Metrics are persisted to `.rustant/personas/metrics.json`.

## Commands

```
/persona status    # Show active persona and metrics
/persona list      # List all available personas
/persona set arch  # Manually set persona (fuzzy match)
/persona auto      # Re-enable auto-detection
/persona stats     # Show per-persona statistics
```

## Configuration

```toml
[personas]
enabled = true
auto_detect = true
default = "general"

[feature_flags]
dynamic_personas = false    # Enable persona evolution
```
