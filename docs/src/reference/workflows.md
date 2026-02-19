# Workflow Templates Reference

Rustant ships with 39+ built-in workflow templates. Workflows are multi-step automation sequences defined in YAML, executed by the workflow engine. Each step invokes a tool, and steps can be gated with approval requirements.

Run workflows via:
- CLI: `rustant workflow run <name> -i key=val`
- REPL: `/workflow run <name> key=val`

List all: `rustant workflow list` or `/workflows`.

## Core Development

### code_review
Automated code review workflow. Reads files, analyzes for security/performance/correctness issues, and generates a report.

- **Inputs**: `path` (string) -- file or directory to review; `focus_areas` (string[], optional) -- defaults to security, performance, correctness.
- **Steps**: Read files, analyze code, generate review report.

### refactor
Guided code refactoring workflow. Reads source, plans refactoring strategy, and applies changes with approval gate.

- **Inputs**: `path` (string) -- file to refactor; `strategy` (string, optional) -- defaults to "simplify".
- **Steps**: Read source, plan refactor, apply changes (requires approval).

### test_generation
Generate tests for existing code. Reads the source file, generates test cases, and writes them to the appropriate test file.

- **Inputs**: `path` (string) -- file to generate tests for; `framework` (string, optional) -- test framework.
- **Steps**: Read source, generate tests, write test file.

### documentation
Generate or update documentation for a codebase path. Reads source, extracts public API surface, and generates markdown docs.

- **Inputs**: `path` (string) -- file or directory to document.
- **Steps**: Read source, extract API, generate docs.

### dependency_update
Check for outdated dependencies, evaluate update risk, and apply updates with approval.

- **Inputs**: `path` (string, optional) -- project root.
- **Steps**: List outdated deps, evaluate risk, apply updates (requires approval).

### security_scan
Run security scanners (SAST, SCA, secrets) across the workspace and generate a findings report.

- **Inputs**: `path` (string, optional) -- path to scan.
- **Steps**: Run scanners, collect findings, generate report.

### deployment
Multi-stage deployment workflow with pre-checks, approval gates, canary analysis, and rollback capability.

- **Inputs**: `target` (string) -- deployment target; `environment` (string) -- e.g., staging, production.
- **Steps**: Pre-deployment checks, build, deploy (requires approval), verify, post-deploy checks.

### incident_response
SRE incident response workflow (v2.0). Detects incidents, runs diagnostics, coordinates remediation, and generates postmortem.

- **Inputs**: `incident_id` (string) -- incident identifier.
- **Steps**: Detect, diagnose, remediate (requires approval), verify, postmortem.

## Daily Automation

### morning_briefing
Generate a morning briefing with calendar events, pending tasks, and weather.

- **Inputs**: None.
- **Steps**: Fetch calendar, check reminders, get weather, compile briefing.

### pr_review
Review open pull requests. Lists PRs, reads diffs, and provides review comments.

- **Inputs**: `repo` (string, optional) -- repository path.
- **Steps**: List open PRs, read diffs, generate review comments.

### dependency_audit
Audit dependencies for known vulnerabilities and license issues.

- **Inputs**: `path` (string, optional).
- **Steps**: Scan lockfiles, check CVE databases, generate audit report.

### changelog
Generate a changelog from recent git commits. Categorizes changes and formats as markdown.

- **Inputs**: `since` (string, optional) -- git ref (e.g., "v1.0.0").
- **Steps**: Read git log, categorize commits, generate changelog.

## macOS Daily Assistant

### meeting_recorder
Record, transcribe, and summarize meetings. Detects active meeting apps, records audio, transcribes via Whisper, and saves to Notes.app.

- **Inputs**: `title` (string, optional) -- meeting title.
- **Steps**: Detect meeting app, start recording, transcribe, summarize, save to Notes.

### daily_briefing_full
Full daily briefing combining calendar, reminders, weather, and recent notifications. Saves to Notes.app "Daily Briefings" folder.

- **Inputs**: `type` (string, optional) -- "morning" or "evening".
- **Steps**: Gather calendar/reminders/weather, compile, save to Notes.

### end_of_day_summary
End-of-day summary with completed tasks, open items, and tomorrow preview.

- **Inputs**: None.
- **Steps**: Review completed tasks, list open items, preview tomorrow, generate summary.

### app_automation
macOS application automation workflow using GUI scripting, accessibility, and screen analysis.

- **Inputs**: `app` (string) -- target application; `action` (string) -- automation action.
- **Steps**: Open app, perform action via GUI scripting, verify via screen analyze.

### email_triage
Email triage workflow. Reads inbox, classifies messages, drafts replies, and schedules follow-ups.

- **Inputs**: None.
- **Steps**: Fetch unread emails, classify by priority, draft replies, schedule follow-ups.

## Research

### arxiv_research
End-to-end research workflow: search papers, fetch details, analyze, and add to knowledge graph.

- **Inputs**: `query` (string) -- research topic.
- **Steps**: Search arXiv, fetch top results, analyze papers, save to library, update knowledge graph.

## Cognitive Extensions

### knowledge_graph
Build and query a knowledge graph of concepts, papers, methods, and people.

- **Inputs**: `action` (string) -- add/search/import/stats.
- **Steps**: Varies by action -- add nodes, create relationships, query graph.

### experiment_tracking
Track scientific hypotheses, design experiments, record results, and compare outcomes.

- **Inputs**: `action` (string) -- add/start/complete/compare.
- **Steps**: Create hypothesis, design experiment, record results, statistical analysis.

### code_analysis
Cross-language codebase analysis: architecture mapping, tech debt scanning, pattern detection.

- **Inputs**: `path` (string) -- codebase path; `action` (string) -- architecture/debt/patterns/deps.
- **Steps**: Scan codebase, analyze structure, generate report.

### content_pipeline
Multi-platform content creation and publishing pipeline with lifecycle tracking.

- **Inputs**: `title` (string) -- content title; `platform` (string) -- target platform.
- **Steps**: Create draft, review, adapt for platform, schedule, publish.

### skill_development
Track skill progression, identify knowledge gaps, and generate practice plans.

- **Inputs**: `skill` (string) -- skill to track.
- **Steps**: Assess current level, identify gaps, create learning path, track progress.

### career_planning
Career goal tracking, achievement logging, and gap analysis.

- **Inputs**: `action` (string) -- goals/achieve/gaps/strategy.
- **Steps**: Define goals, track achievements, analyze gaps, generate strategy.

### system_monitoring
Service topology management, health monitoring, and incident tracking.

- **Inputs**: `action` (string) -- add/check/topology/incident.
- **Steps**: Register services, run health checks, detect anomalies, log incidents.

### life_planning
Energy-aware scheduling, deadline tracking, and habit management.

- **Inputs**: `action` (string) -- deadline/habits/daily/review.
- **Steps**: Track deadlines, manage habits, generate daily plan, weekly review.

### privacy_audit
Data boundary management, access auditing, and privacy compliance checks.

- **Inputs**: None.
- **Steps**: Scan data boundaries, audit access logs, check compliance, generate report.

### self_improvement_loop
Analyze agent usage patterns, identify improvements, and optimize performance.

- **Inputs**: None.
- **Steps**: Analyze tool usage, review performance metrics, generate suggestions, apply optimizations.

## ML/AI Engineering

### ml_data_pipeline
End-to-end data pipeline: ingest, validate, transform, split, and version datasets.

- **Inputs**: `source` (string) -- data source path or URL.
- **Steps**: Ingest data, validate schema, transform features, create splits, version snapshot.

### ml_training_experiment
Model training experiment: configure, train, evaluate, and compare results.

- **Inputs**: `config` (string) -- training configuration; `dataset` (string) -- training data.
- **Steps**: Load data, configure training, train model, evaluate, record metrics.

### ml_model_deployment
Deploy trained models to inference endpoints with health checks and monitoring.

- **Inputs**: `model` (string) -- model to deploy; `target` (string) -- deployment target.
- **Steps**: Load model, configure serving, deploy (requires approval), health check, monitor.

### ml_rag_setup
Set up a RAG pipeline: ingest documents, configure retrieval, evaluate quality.

- **Inputs**: `source` (string) -- document source; `collection` (string) -- collection name.
- **Steps**: Ingest documents, chunk and embed, configure retriever, evaluate retrieval quality.

### ml_llm_finetune
Fine-tune an LLM with dataset preparation, training, evaluation, and adapter merging.

- **Inputs**: `model` (string) -- base model; `dataset` (string) -- training data.
- **Steps**: Prepare dataset, configure fine-tuning, train, evaluate, merge adapter.

### ai_research
ML research workflow: literature review, methodology comparison, and reproducibility checks.

- **Inputs**: `topic` (string) -- research topic.
- **Steps**: Search papers, review methodology, compare approaches, reproduce results.

### ai_safety_audit
AI safety audit: PII detection, bias analysis, alignment evaluation, and red teaming.

- **Inputs**: `model` (string) -- model to audit.
- **Steps**: Scan for PII, detect bias, evaluate alignment, run red team tests, generate report.

### ai_evaluation_suite
Comprehensive model evaluation: benchmarks, LLM-as-judge, error analysis, and reporting.

- **Inputs**: `model` (string) -- model to evaluate; `suite` (string) -- evaluation suite.
- **Steps**: Run benchmarks, LLM judge comparison, error analysis, generate evaluation report.

## Fullstack Development

### fullstack_verify
Full-stack verification pipeline: test, lint, typecheck with iterative fix-and-recheck (max 3 iterations).

- **Inputs**: `path` (string, optional) -- project path.
- **Steps**: Run tests, run linter, run type checker, auto-fix failures, re-verify (up to 3 iterations).

## Security & Compliance

### compliance_audit
Security compliance audit against frameworks (SOC 2, ISO 27001, NIST, PCI-DSS, OWASP).

- **Inputs**: `framework` (string) -- compliance framework name.
- **Steps**: Scan codebase, check controls, collect evidence, generate compliance report.

## Custom Workflows

Custom workflows can be defined in YAML files placed in the directory specified by `[workflow].workflow_dir` in your config:

```yaml
name: my_workflow
description: Custom workflow example
version: "1.0"
inputs:
  - name: target
    type: string
    description: Target path
steps:
  - id: step_1
    tool: file_read
    params:
      path: "{{ inputs.target }}"
  - id: step_2
    tool: shell_exec
    params:
      command: "echo 'Processing {{ inputs.target }}'"
    gate:
      type: approval_required
      message: "Execute this shell command?"
outputs:
  - name: result
    value: "{{ steps.step_2.output }}"
```

Gate types: `approval_required` (prompts user), `condition` (evaluates expression).
