# SRE & DevOps Tools

Rustant includes 5 cross-platform SRE/DevOps tools for production operations, incident management, and infrastructure monitoring. These tools are designed for Site Reliability Engineers and operations teams.

## Prerequisites

| Tool | Requirement |
|------|-------------|
| `alert_manager` | None (local state management) |
| `deployment_intel` | None (local state management) |
| `prometheus` | Prometheus server accessible via HTTP |
| `kubernetes` | `kubectl` installed and configured |
| `oncall` | None (local mode) or PagerDuty API key |

## Configuration

SRE tools are configured via `.rustant/sre_config.json` or environment variables:

```json
{
  "prometheus_url": "http://localhost:9090",
  "pagerduty_api_key": "keychain:pagerduty-api-key",
  "oncall_mode": "local"
}
```

The `PROMETHEUS_URL` environment variable can also be used to set the Prometheus endpoint.

Enable SRE mode via the `sre_mode` feature flag in your configuration.

---

## alert_manager

Manage the full alert lifecycle. State is persisted to `.rustant/alerts/state.json`.

**Risk Level:** Write

| Action | Description |
|--------|-------------|
| `create` | Create a new alert with name, severity, and description |
| `list` | List alerts, optionally filtered by status |
| `acknowledge` | Acknowledge a firing alert |
| `silence` | Silence an alert for a specified duration |
| `escalate` | Escalate an alert to a higher severity or team |
| `correlate` | Correlate related alerts to identify common causes |
| `group` | Group related alerts together |
| `resolve` | Resolve an alert |
| `history` | View alert history and timeline |
| `rules` | Manage alerting rules |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | One of the 10 actions above |
| `name` | string | Alert name |
| `severity` | string | `critical`, `warning`, or `info` |
| `description` | string | Alert description |
| `alert_id` | string | Alert identifier (for ack/silence/escalate/resolve) |
| `status` | string | Filter: `firing`, `acknowledged`, `silenced`, `resolved` |
| `duration` | string | Silence duration (e.g., `2h`, `30m`) |

**Example:**
```json
{
  "action": "create",
  "name": "HighCPUUsage",
  "severity": "warning",
  "description": "CPU usage above 90% on web-server-03"
}
```

---

## deployment_intel

Deployment risk assessment, canary analysis, and pre/post-deploy verification. State is persisted to `.rustant/deployments/state.json`.

**Risk Level:** Read-only (assessment) / Write (deploy actions)

| Action | Description |
|--------|-------------|
| `assess_risk` | Assess deployment risk based on changes, timing, and blast radius |
| `canary_status` | Check canary deployment health and metrics |
| `rollback_check` | Verify rollback readiness and prerequisites |
| `deploy_timeline` | Show deployment timeline and history |
| `change_window` | Check if current time is within a change window |
| `pre_deploy_checklist` | Run pre-deployment verification checklist |
| `post_deploy_verify` | Verify deployment health post-release |
| `diff_analysis` | Analyze diff between current and target deployment |

**Example:**
```json
{
  "action": "assess_risk",
  "service": "api-gateway",
  "changes": "database migration + new endpoints",
  "blast_radius": "high"
}
```

---

## prometheus

Query and manage Prometheus monitoring. Wraps the Prometheus HTTP API.

**Risk Level:** Read-only (queries) / Write (silence_create)

**Configuration:** Set `PROMETHEUS_URL` environment variable or configure in `.rustant/sre_config.json`.

| Action | Description |
|--------|-------------|
| `query` | Execute an instant PromQL query |
| `query_range` | Execute a range query with start/end/step |
| `series` | Find time series matching label matchers |
| `labels` | Get label names or values |
| `alerts` | List active alerts from Prometheus |
| `targets` | List scrape targets and their health |
| `rules` | List alerting and recording rules |
| `silence_create` | Create a new silence for alerts |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | One of the 8 actions above |
| `query` | string | PromQL expression |
| `start` | string | Range query start time (RFC3339 or relative) |
| `end` | string | Range query end time |
| `step` | string | Query resolution step (e.g., `15s`, `1m`) |
| `match` | string | Series matcher for series/labels actions |

**Example:**
```json
{
  "action": "query",
  "query": "rate(http_requests_total{status=~\"5..\"}[5m])"
}
```

---

## kubernetes

Interact with Kubernetes clusters via `kubectl`. Requires `kubectl` to be installed and configured with cluster access.

**Risk Level:** Read-only (queries) / Execute (rollout_restart, scale)

| Action | Description |
|--------|-------------|
| `pods` | List pods in a namespace |
| `services` | List services |
| `deployments` | List deployments |
| `events` | List cluster events |
| `logs` | Get pod logs |
| `describe` | Describe a resource in detail |
| `top` | Show resource usage (CPU/memory) |
| `rollout_status` | Check rollout status of a deployment |
| `rollout_restart` | Restart a deployment rollout |
| `scale` | Scale a deployment to N replicas |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | One of the 10 actions above |
| `namespace` | string | Kubernetes namespace (default: `default`) |
| `name` | string | Resource name |
| `resource` | string | Resource type for describe (e.g., `pod`, `service`) |
| `replicas` | integer | Target replica count (for scale) |
| `top_type` | string | Resource type for top: `pods` or `nodes` |

**Example:**
```json
{
  "action": "logs",
  "namespace": "production",
  "name": "api-server-7b9f4d5c8-x2k9p",
}
```

---

## oncall

On-call management with local mode and PagerDuty API integration. State is persisted to `.rustant/oncall/state.json`.

**Risk Level:** Write

| Action | Description |
|--------|-------------|
| `who_is_oncall` | Check who is currently on call |
| `create_incident` | Create a new incident |
| `acknowledge` | Acknowledge an incident |
| `escalate` | Escalate an incident to the next tier |
| `schedule` | View on-call schedule |
| `override_oncall` | Override the on-call rotation temporarily |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | One of the 6 actions above |
| `title` | string | Incident title (for create_incident) |
| `urgency` | string | `high` or `low` (default: `high`) |
| `description` | string | Incident description |
| `incident_id` | string | Incident identifier |

**Example:**
```json
{
  "action": "create_incident",
  "title": "Database connection pool exhausted",
  "urgency": "high",
  "description": "Primary DB connection pool at 100%, secondary failover active"
}
```

---

## Related REPL Commands

| Command | Description |
|---------|-------------|
| `/incident` | Create, list, show, escalate, resolve, postmortem |
| `/deploy` | Status, risk assessment, rollback, canary, history |
| `/alerts` | List, acknowledge, silence, correlate, rules |
| `/oncall` | Who is on call, schedule, escalate, override |
| `/circuit` | Circuit breaker: status, open, close, reset |
| `/trust` | Progressive trust: level, history, promote, demote |

## Related Workflow Templates

- `incident_response` -- Full SRE incident workflow with real steps
- `sre_deployment` -- Deployment with risk assessment and canary
- `alert_triage` -- Alert correlation and triage
- `sre_health_review` -- Infrastructure health review
