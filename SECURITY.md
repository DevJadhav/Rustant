# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

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
2. **Authorization** — Configurable approval modes (safe, cautious, paranoid, yolo)
3. **Sandboxing** — Filesystem sandbox via cap-std, WASM sandbox via wasmi
4. **Output validation** — Sensitive data detection, output size limits
5. **Audit trail** — Tamper-evident Merkle chain verification

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
