# Monitoring & Observability

## Built-in Diagnostics

### /doctor

Run comprehensive health checks:

```
/doctor
```

Checks: LLM connectivity, tool registration, config validation, workspace writability, session index integrity, audit chain verification.

### /status

```
/status    # Agent status, iteration count, token usage, cost
/cost      # Detailed token usage with cache metrics
/context   # Context window usage breakdown
/memory    # Memory system statistics
```

## Evaluation Framework

Enable the evaluation pipeline:

```toml
[feature_flags]
evaluation = true
```

### Built-in Evaluators

| Evaluator | What it Detects |
|-----------|----------------|
| `LoopDetectionEvaluator` | Same tool called >N times consecutively |
| `SafetyFalsePositiveEvaluator` | Tools denied then approved (over-restrictive safety) |
| `CostEfficiencyEvaluator` | Tokens per tool call efficiency |

The `EvaluationPipeline` produces reports with precision, recall, and F1 scores.

## Anomaly Detection

```toml
[feature_flags]
# Anomaly detection is always available
```

Three detection methods:

| Method | Best For |
|--------|----------|
| `ZScore` | Normally distributed metrics |
| `IQR` | Skewed distributions, outlier detection |
| `MovingAverage` | Trend detection in time series |

`AnomalyDetector.detect()` returns a score (0.0-1.0), expected/actual values, and expected range. Used by `system_monitor` health checks and alert correlation.

## Audit Trail

The Merkle chain audit trail provides tamper-evident execution history:

```
/audit show 20     # Show last 20 events
/audit verify      # Verify chain integrity
```

Security-specific audit events:
- `SecurityScanCompleted` — Scan finished
- `FindingDetected` — Vulnerability found
- `FindingSuppressed` — Finding dismissed
- `PolicyEvaluated` — Policy check result
- `ComplianceReportGenerated` — Report created
- `IncidentActionTaken` — Response action executed

## Logging

Rustant uses `tracing` for structured logging:

```bash
# Verbose output
rustant --verbose "task"
rustant -vvv "task"       # Maximum verbosity

# Environment variable
RUST_LOG=debug rustant "task"
RUST_LOG=rustant_core=trace,rustant_tools=debug rustant "task"
```

## Prompt Cache Monitoring

```
/cache     # Show cache state (Cold/Warm/Hot), hit rate, savings
/cost      # Show cache-aware cost calculations
```

## Circuit Breaker Status

```
/circuit status    # Current state, failure count, window
```

Monitor for unexpected Open states indicating systemic failures.
