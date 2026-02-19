# SRE Mode

SRE mode activates operational tools and workflows for site reliability engineering.

## Activation

```toml
[feature_flags]
sre_mode = true
incident_response = true

[sre]
enabled = true

[sre.prometheus]
url = "http://localhost:9090"

[sre.oncall]
provider = "local"         # local or pagerduty
```

Or via environment: `RUSTANT_FEATURE_FLAGS__SRE_MODE=true`

## SRE Tools

| Tool | Purpose |
|------|---------|
| `alert_manager` | Alert lifecycle (create, ack, silence, escalate, correlate, resolve) |
| `deployment_intel` | Deployment risk, canary analysis, rollback checks |
| `prometheus` | Query Prometheus metrics, alerts, targets |
| `kubernetes` | kubectl operations (pods, deployments, logs, rollouts) |
| `oncall` | On-call schedule, incidents, escalation |

## SRE Slash Commands

```
/incident create <title>     # Create incident
/incident list               # List active incidents
/incident show <id>          # Show incident details
/incident escalate <id>      # Escalate incident
/incident resolve <id>       # Resolve incident
/incident postmortem <id>    # Generate postmortem

/deploy status               # Deployment status
/deploy risk                 # Risk assessment
/deploy rollback             # Rollback check
/deploy canary               # Canary analysis
/deploy history              # Deployment history

/alerts list                 # Active alerts
/alerts ack <id>             # Acknowledge alert
/alerts silence <id>         # Silence alert
/alerts correlate            # Correlate alerts
/alerts rules                # Show alert rules

/oncall who                  # Current on-call
/oncall schedule             # Show schedule
/oncall escalate             # Escalate to next tier
/oncall override             # Override schedule
```

## SRE Workflows

| Template | Description |
|----------|-------------|
| `incident_response` | Full incident response flow |
| `sre_deployment` | Deployment with risk assessment |
| `alert_triage` | Alert investigation and triage |
| `sre_health_review` | Infrastructure health review |

## Remediation Council

When `council.enabled = true` and SRE mode is active, the `RemediationCouncil` provides multi-model consensus for incident remediation:
- Configurable consensus threshold (default: 0.85)
- Unanimity required for destructive actions
- Anonymous peer review of proposed remediations

## Gateway Endpoints

SRE-specific REST API endpoints:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/sre/status` | GET | System status overview |
| `/api/sre/trust` | GET | Trust level information |
| `/api/sre/circuit` | GET/POST | Circuit breaker control |

## Personas

SRE mode activates specialized personas:
- **IncidentCommander** — Incident response and coordination
- **ObservabilityExpert** — Monitoring, metrics, tracing
- **ReliabilityEngineer** — SRE practices, capacity planning
- **DeploymentEngineer** — Deployment, CI/CD, rollbacks
