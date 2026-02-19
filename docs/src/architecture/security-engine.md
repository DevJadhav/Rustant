# Security Engine

The `rustant-security` crate provides comprehensive security scanning, code review, compliance, and incident response capabilities.

## Overview

90 source files, 31,000+ LOC, 565 tests, 33 tool wrappers. Organized in 5 phases.

## Phase 1: Foundation

### Finding Schema (`finding.rs`)
Unified `Finding` type with CVSS 3.1 severity, content-hash deduplication, provenance tracking, and explanation chains.

### Secret Redaction (`redaction.rs`)
`SecretRedactor` with 60+ regex patterns + Shannon entropy analysis (threshold 4.5). Applied to all LLM context, memory, audit logs, and MCP output.

### AST Engine (`ast/`)
Tree-sitter-based parsing with LRU cache. Feature-gated grammars for 30+ languages:
- Default: `sast-rust`
- Optional: `sast-python`, `sast-javascript`, `sast-go`, `sast-java`, `sast-shell`, `sast-iac`
- Meta: `sast-all` enables everything

Provides symbol extraction and cyclomatic complexity calculation.

### Dependency Graph (`dep_graph/`)
`DependencyGraph` built on `petgraph` with lockfile parsers:
- Cargo.lock, package-lock.json, yarn.lock
- requirements.txt, poetry.lock, Pipfile.lock
- And 6 more formats

Features: transitive dependency resolution, reverse dependency lookup, blast radius analysis.

### Multi-Model Consensus (`consensus.rs`)
`SecurityConsensus` wrapping `PlanningCouncil` for multi-model finding validation with weighted voting.

### Memory Bridge (`memory_bridge.rs`)
Converts findings to redacted `Fact` entries for long-term memory integration.

## Phase 2: Code Review

9 tool wrappers covering:
- **Diff Analysis** — Parse and analyze code diffs
- **Quality Scoring** — Multi-dimensional code quality metrics
- **Dead Code Detection** — Identify unused functions and types
- **Duplication Detection** — Find copy-paste patterns
- **Tech Debt Tracking** — Quantify and track technical debt
- **Autofix Suggestions** — Automated fix proposals
- **Comment Analysis** — Review code comments and documentation

## Phase 3: Scanners

12 tool wrappers covering:
- **SAST** — Static application security testing with 15+ rules
- **Secrets Scanner** — Detect hardcoded secrets and credentials
- **SCA** — Software composition analysis for known vulnerabilities
- **Container Security** — Trivy integration for container image scanning
- **Dockerfile Linter** — Best practice and security checks for Dockerfiles
- **IAC Scanner** — Infrastructure-as-code analysis with 12 rules
- **Supply Chain** — Typosquatting detection for package names
- **Scan Orchestrator** — Parallel execution with semaphore-based concurrency

## Phase 4: Compliance

7 tool wrappers covering:
- **License Scanning** — SPDX-based license identification and compatibility
- **Policy Engine** — Gate/warn/inform actions based on policy rules
- **SBOM Generation** — CycloneDX and CSV formats with diff support
- **Risk Assessment** — Multi-dimensional risk scoring
- **VEX** — Vulnerability Exploitability Exchange management
- **Compliance Frameworks** — 6 built-in frameworks with evidence collection

## Phase 5: Incident Response

5 tool wrappers covering:
- **Incident Detection** — Rule-based detection with MITRE ATT&CK mapping
- **Alert Correlation** — Priority scoring and event correlation engine
- **Playbook Registry** — Trigger-matching automated response playbooks
- **Log Parsers** — 6+ formats including K8s audit logs and CloudTrail
- **Action Registry** — 8 predefined response actions with learning

## Reports

Multiple output formats:
- **SARIF 2.1.0** — Standard static analysis format
- **Markdown** — Human-readable reports
- **OCSF** — Open Cybersecurity Schema Framework
- **HTML** — Standalone with embedded CSS
- **PDF** — With recommendations
- **Analytics** — MTTR trends, security metrics

## Integration

- Audit trail: 6 security-specific `TraceEventKind` variants
- CLI: 10 security subcommands (`rustant scan`, `rustant review`, etc.)
- REPL: 11 slash commands (`/scan`, `/autofix`, `/quality`, `/compliance`, etc.)
- Workflows: `security_scan`, `compliance_audit`, `code_review_ai` templates
- Config: `security: Option<serde_json::Value>` on `AgentConfig` (avoids circular dependency)
