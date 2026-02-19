# Security Tools

The `rustant-security` crate provides 33 security tools organized across 4 phases: code review and quality (Phase 2), security scanning (Phase 3), compliance and risk (Phase 4), and incident response (Phase 5). These tools integrate with the agent via the `Tool` trait and are registered separately from the base tool set.

## Feature Gates

Security scanning languages are feature-gated. Enable the languages you need:

| Feature | Description | Default |
|---------|-------------|---------|
| `sast-rust` | Rust SAST rules | Yes |
| `sast-python` | Python SAST rules | No |
| `sast-javascript` | JavaScript/TypeScript SAST rules | No |
| `sast-go` | Go SAST rules | No |
| `sast-java` | Java SAST rules | No |
| `sast-shell` | Shell/Bash SAST rules | No |
| `sast-iac` | Infrastructure-as-Code rules (HCL, YAML) | No |
| `sast-all` | Enable all SAST languages | No |

Enable in `Cargo.toml`:
```toml
[dependencies]
rustant-security = { version = "1.0", features = ["sast-all"] }
```

---

## Phase 2: Code Review & Quality (9 Tools)

Tools for automated code review, quality scoring, and technical debt management.

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `code_review` | Read-only | Automated code review with multi-model consensus |
| `quality_score` | Read-only | Compute code quality score (maintainability, reliability, complexity) |
| `analyze_diff` | Read-only | Analyze a git diff for issues, risks, and suggested improvements |
| `dead_code_detect` | Read-only | Detect unused functions, types, imports, and variables |
| `duplicate_detect` | Read-only | Find code duplication and near-duplicates |
| `tech_debt_report` | Read-only | Generate technical debt report with prioritized items |
| `complexity_check` | Read-only | Measure cyclomatic complexity via Tree-sitter AST |
| `suggest_fix` | Read-only | Generate fix suggestions for detected findings |
| `apply_fix` | Write | Apply auto-fix suggestions from code review |

### code_review

Runs a comprehensive code review using the security engine. Supports multi-model consensus via `SecurityConsensus` wrapping `PlanningCouncil`.

### quality_score

Computes a composite quality score based on:
- Maintainability (naming, structure, documentation)
- Reliability (error handling, edge cases)
- Complexity (cyclomatic complexity, nesting depth)

### analyze_diff

Analyzes a git diff and reports:
- Security issues introduced
- Style violations
- Potential bugs
- Suggested improvements

### dead_code_detect

Uses Tree-sitter AST analysis to identify:
- Unused functions and methods
- Unused imports and type definitions
- Unreachable code paths

### tech_debt_report

Generates a prioritized report of technical debt items with estimated effort and business impact.

---

## Phase 3: Security Scanning (12 Tools)

Tools for static analysis, secret detection, dependency scanning, and infrastructure security.

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `sast_scan` | Read-only | Static Application Security Testing (15+ rules) |
| `secrets_scan` | Read-only | Detect secrets and credentials (60+ patterns + entropy) |
| `secrets_validate` | Read-only | Validate detected secrets are real (not false positives) |
| `sca_scan` | Read-only | Software Composition Analysis for vulnerable dependencies |
| `container_scan` | Read-only | Scan container images for vulnerabilities (via Trivy) |
| `dockerfile_lint` | Read-only | Lint Dockerfiles for security and best practices |
| `iac_scan` | Read-only | Infrastructure-as-Code scanning (12 rules for HCL, YAML) |
| `k8s_lint` | Read-only | Lint Kubernetes manifests for security issues |
| `terraform_check` | Read-only | Terraform-specific security checks |
| `supply_chain_check` | Read-only | Check for typosquatting and supply chain attacks |
| `vulnerability_check` | Read-only | Look up known vulnerabilities by CVE or package |
| `security_scan` | Read-only | Orchestrated scan combining multiple scanners |

### sast_scan

Runs static analysis with 15+ built-in rules per language. Uses Tree-sitter for AST parsing with LRU cache.

**Example:**
```json
{
  "path": "src/",
  "language": "rust",
  "severity_threshold": "medium"
}
```

### secrets_scan

Detects secrets using 60+ regex patterns plus Shannon entropy analysis (threshold: 4.5). Applied to all LLM context, memory, audit logs, and MCP output.

Pattern categories include:
- API keys (AWS, GCP, Azure, GitHub, Slack, Stripe, etc.)
- Private keys (RSA, SSH, PGP)
- Connection strings (database URLs, Redis, AMQP)
- Tokens (JWT, OAuth, Bearer)

### sca_scan

Analyzes dependency lockfiles for known vulnerabilities:
- `Cargo.lock` (Rust)
- `package-lock.json` / `yarn.lock` (JavaScript)
- `requirements.txt` / `poetry.lock` (Python)
- And more (12 lockfile formats total)

Uses `DependencyGraph` (petgraph) for transitive dependency analysis, reverse dependency lookups, and blast radius estimation.

### security_scan

The `ScanOrchestrator` runs multiple scanners in parallel using semaphore-based concurrency. Combines results with content-hash deduplication and CVSS 3.1 severity scoring.

---

## Phase 4: Compliance & Risk (7 Tools)

Tools for license compliance, SBOM generation, risk assessment, and policy enforcement.

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `license_check` | Read-only | Scan dependencies for license compliance (SPDX) |
| `sbom_generate` | Read-only | Generate Software Bill of Materials (CycloneDX/CSV) |
| `sbom_diff` | Read-only | Diff two SBOMs to detect changes |
| `compliance_report` | Read-only | Generate compliance report against frameworks |
| `risk_score` | Read-only | Multi-dimensional risk assessment |
| `policy_check` | Read-only | Evaluate code against policy definitions |
| `audit_export` | Read-only | Export audit data for evidence collection |

### license_check

Scans all dependencies and classifies licenses using SPDX identifiers. Supports policy levels:
- **Gate** -- block builds with non-compliant licenses
- **Warn** -- warn about potentially problematic licenses
- **Inform** -- informational reporting only

### sbom_generate

Generates a Software Bill of Materials in CycloneDX JSON or CSV format. Includes:
- All direct and transitive dependencies
- License information
- Version details
- Source repository URLs

### compliance_report

Generates compliance reports against 6 built-in frameworks. Includes evidence collection and gap analysis.

### risk_score

Computes multi-dimensional risk scores considering:
- Vulnerability severity and count
- Dependency health (age, maintenance status)
- Code complexity and test coverage
- License risk

---

## Phase 5: Incident Response (5 Tools)

Tools for security incident detection, alert correlation, playbook execution, and log analysis.

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `threat_detect` | Read-only | Detect threats using rules and MITRE ATT&CK mapping |
| `alert_triage` | Read-only | Triage and correlate security alerts with priority scoring |
| `incident_respond` | Execute | Execute incident response playbooks |
| `log_analyze` | Read-only | Parse and analyze logs (6+ formats including K8s audit, CloudTrail) |
| `alert_status` | Read-only | Check alert and incident status |

### threat_detect

Rule-based threat detection with MITRE ATT&CK framework mapping. Features:
- Rule management (add, remove, enable, disable)
- Confidence scoring with learning updates
- Risky change detection
- Hotspot tracking

### alert_triage

`AlertCorrelationEngine` with priority scoring that:
- Groups related alerts by common patterns
- Scores alert priority based on severity, frequency, and context
- Correlates security events across multiple sources

### incident_respond

Execute predefined incident response playbooks from the `ActionRegistry` (8 predefined actions). Features:
- Trigger matching based on alert patterns
- Playbook registry with step-by-step execution
- Destructive actions require unanimous council consensus

### log_analyze

Parse and analyze logs from multiple formats:
- Syslog
- JSON structured logs
- Apache/Nginx access logs
- Kubernetes audit logs
- AWS CloudTrail
- Application-specific formats

---

## Findings Schema

All security tools produce findings using the unified `Finding` schema:

| Field | Description |
|-------|-------------|
| `id` | Content-hash for deduplication |
| `title` | Human-readable finding title |
| `severity` | CVSS 3.1 severity (Critical, High, Medium, Low, Info) |
| `category` | Finding category (vulnerability, secret, license, etc.) |
| `location` | File path and line number |
| `description` | Detailed description |
| `remediation` | Suggested fix |
| `provenance` | Source scanner and confidence level |
| `explanation_chain` | Chain of reasoning for the finding |

## Report Formats

Security findings can be exported in multiple formats:

| Format | Description |
|--------|-------------|
| SARIF 2.1.0 | Static Analysis Results Interchange Format |
| Markdown | Human-readable Markdown report |
| OCSF | Open Cybersecurity Schema Framework |
| HTML | Standalone HTML with embedded CSS |
| PDF | PDF report with recommendations |

---

## Related REPL Commands

| Command | Description |
|---------|-------------|
| `/scan` | Run a security scan |
| `/autofix` | Apply auto-fix suggestions |
| `/quality` | Check code quality |
| `/debt` | Technical debt report |
| `/complexity` | Cyclomatic complexity analysis |
| `/findings` | List current findings |
| `/license` | License compliance check |
| `/sbom` | Generate SBOM |
| `/risk` | Risk assessment |
| `/compliance` | Compliance report |
| `/triage` | Alert triage |

## Related CLI Subcommands

`rustant scan`, `rustant review`, `rustant quality`, `rustant license`, `rustant sbom`, `rustant compliance`, `rustant audit`, `rustant risk`, `rustant policy`, `rustant alerts`.
