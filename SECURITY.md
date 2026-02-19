# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability in Rustant, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, please email **security@rustant.dev** with:

1. A description of the vulnerability
2. Steps to reproduce
3. Potential impact
4. Suggested fix (if any)

We will acknowledge receipt within 48 hours and aim to provide an initial
assessment within 5 business days.

## Security Model

Rustant implements a five-layer security defense:

1. **Input validation** — Prompt injection detection, path traversal prevention
2. **Authorization** — Configurable approval modes (safe, cautious, paranoid, yolo) with progressive trust (5 levels from Shadow to SelectiveAutonomy)
3. **Sandboxing** — Filesystem sandbox via cap-std, WASM sandbox via wasmi
4. **Output validation** — Sensitive data detection, output size limits
5. **Audit trail** — Tamper-evident Merkle chain verification

### Progressive Trust

The `TrustLevel` system provides graduated autonomy:
- **Shadow** — Agent suggests, human executes
- **DryRun** — Agent executes in dry-run mode
- **Assisted** — Agent executes with approval
- **Supervised** — Agent executes, human monitors
- **SelectiveAutonomy** — Agent executes autonomously for trusted operations

Trust level escalation requires minimum successful actions and maximum error rate thresholds. Demotion occurs on destructive failures or circuit breaker trips.

### Global Circuit Breaker

A sliding-window circuit breaker tracks recent action results:
- **Closed** — Normal operation
- **Open** — All non-read actions blocked (3 consecutive failures or 50% failure rate)
- **HalfOpen** — Only read-only actions permitted during recovery

### Security Engine (rustant-security crate)

The dedicated security crate provides:
- **SAST** — Static analysis with 15+ rules across Rust, Python, JavaScript, Go, Java, Shell, IaC
- **SCA** — Dependency vulnerability scanning for Cargo, npm, pip, poetry
- **Secrets Detection** — 60+ regex patterns + Shannon entropy analysis
- **Container Security** — Trivy integration + Dockerfile linting
- **Compliance** — License scanning (SPDX), SBOM generation (CycloneDX), 6 built-in frameworks
- **Incident Response** — MITRE ATT&CK mapping, alert correlation, playbook automation

### Policy Engine

Custom policies via `.rustant/policies.toml` with predicates: TimeWindow, MaxBlastRadius, MinTrustLevel, RequiresConsensus, MaxConcurrentDeployments. Policies can be scoped to specific tools or actions.

## Responsible Disclosure

We follow a 90-day disclosure timeline:

- We will work to fix confirmed vulnerabilities within 90 days
- We will coordinate public disclosure timing with the reporter
- We will credit the reporter in release notes (unless they prefer anonymity)

## Security Updates

Security patches are released as point releases and announced via:

- GitHub Releases
- The `rustant update check` command

We recommend enabling automatic update checks in your configuration:

```toml
[update]
auto_check = true
check_interval_hours = 24
```
