# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **rustant-security crate** — Dedicated security scanning and compliance engine with 33 tool wrappers. Phase 1: unified `Finding` schema, `SecretRedactor` (60+ patterns + Shannon entropy), tree-sitter AST engine (30+ languages), `DependencyGraph` (petgraph) with lockfile parsers (Cargo, npm, yarn, pip, poetry), multi-model `SecurityConsensus`, `MemoryBridge`. Phase 2: code review engine with diff analysis, quality scoring, dead code detection, duplication detection, tech debt tracking, autofix suggestions. Phase 3: SAST (15+ rules), secrets scanner, SCA scanner, container security (Trivy + Dockerfile linter), IAC scanner (12 rules), supply chain typosquatting detection. Phase 4: license scanning (SPDX), policy engine (gate/warn/inform), SBOM generation (CycloneDX/CSV/diff), multi-dimensional risk scoring, VEX support, 6 built-in compliance frameworks, evidence collection. Phase 5: incident detection with MITRE ATT&CK mapping, alert correlation engine with priority scoring, playbook registry with trigger matching, log parsers (6+ formats incl. K8s audit + CloudTrail), action registry with 8 predefined actions, learning engine with hotspot tracking and confidence updates. Reports: SARIF 2.1.0, Markdown, OCSF, HTML, PDF, analytics with MTTR/trends
- **rustant-ml crate** — ML/AI engineering crate with 54 tool wrappers via `ml_tool!` macro. Four foundational pillars: Safety, Security, Transparency, Interpretability. Phase 0: runtime, config, error handling. Phase 1: data sources, schema, transforms, validation, storage, lineage + feature engineering. Phase 2: experiment tracking, training runner, metrics, checkpoints, hyperparameter sweeps, callbacks, reproducibility. Phase 3: model zoo with registry, cards, download, convert, benchmark, provenance + classical/neural algorithms with evaluation and explainability. Phase 4: LLM fine-tuning, dataset preparation, quantization, evaluation harness, adapter management, alignment, red teaming. Phase 5: RAG pipeline with ingestion, chunking, retrieval, reranking, context assembly, grounding, diagnostics, evaluation, collections. Phase 6: LLM judge, error analysis, domain evaluations, test generators, CI integration, traces, benchmarks, inter-annotator agreement. Phase 7: inference backends (Ollama, vLLM, llama.cpp, Candle), model registry, format conversion, serving, streaming, profiler. Phase 8: research methodology, comparison frameworks, literature review, datasets, reproducibility, bibliography, notebooks, synthesis. Phase 9: AI safety, security, transparency, interpretability pillar modules. Phase 10: 13 tool wrapper modules, 16 slash commands, 8 workflow templates, agent routing
- **SRE/DevOps tools** (5 new cross-platform tools) — `alert_manager` (create, list, acknowledge, silence, escalate, correlate, group, resolve, history, rules), `deployment_intel` (risk assessment, canary analysis, rollback checks, pre/post-deploy verification), `prometheus` (query, query_range, series, labels, alerts, targets, rules, silence), `kubernetes` (pods, services, deployments, events, logs, describe, top, rollout), `oncall` (schedule, incidents, escalation, PagerDuty integration)
- **Fullstack development tools** (5 new tools) — `scaffold` (project scaffolding with framework detection), `dev_server` (development server management), `database` (migration and query tools), `test_runner` (multi-framework test execution), `lint` (multi-language linting)
- **Project templates** (6 new templates) — `react_vite`, `nextjs`, `fastapi`, `rust_axum`, `sveltekit`, `express` in `TemplateLibrary` with alias support and variable substitution
- **Screen automation tools** (5 new macOS tools) — `macos_gui_scripting` (8 actions for native app UI interaction via System Events), `macos_accessibility` (4 actions for read-only accessibility tree inspection), `macos_screen_analyze` (2 actions for OCR via macOS Vision framework), `macos_contacts` (4 actions for Contacts.app management), `macos_safari` (6 actions for Safari browser automation)
- **Productivity macOS tools** (3 new tools) — `homekit` (3 actions for smart home control via Shortcuts CLI), `voice_tool` (TTS via macOS `say` command), `photos` (Photos.app automation)
- **Progressive Trust** — 5-level `TrustLevel` system: Shadow (suggest only), DryRun (dry-run execution), Assisted (execute with approval), Supervised (execute with monitoring), SelectiveAutonomy (autonomous for trusted operations). Escalation requires minimum successful actions and max error rate. Demotion on destructive failures or circuit breaker trips. `/trust` REPL command for level management
- **Global Circuit Breaker** — Sliding-window circuit breaker with Closed/Open/HalfOpen states. Opens on 3 consecutive failures or 50% failure rate (5-minute window). HalfOpen allows only read-only actions. `/circuit status|open|close|reset` REPL command
- **Policy Engine** — Custom policies via `.rustant/policies.toml` with predicates: TimeWindow, MaxBlastRadius, MinTrustLevel, RequiresConsensus, MaxConcurrentDeployments. Scoped to specific tools and actions
- **Anomaly Detection** — Statistical anomaly detection with `DetectionMethod` enum (ZScore, IQR, MovingAverage). `AnomalyDetector.detect()` returns score (0.0-1.0), expected/actual values, expected range. Used by system_monitor and alert correlation
- **AST engine** — Tree-sitter-based AST parsing with feature-gated grammars for Rust, Python, JavaScript, TypeScript, Go, Java. Regex fallback for unsupported languages. Symbol extraction and cyclomatic complexity calculation
- **RepoMap** — `CodeGraph` with PageRank ranking on `petgraph::DiGraph` for codebase navigation and context-aware file ranking
- **Hydration pipeline** — `HydrationPipeline` combining RepoMap ranking with token-budgeted context assembly. Auto-skips projects with fewer than 10 code files
- **Verification loop** — `VerificationConfig` with max_fix_attempts=3 for automated test/lint/build verification after code changes. `fullstack_verify` workflow template
- **Adaptive Personas** — 8 personas (Architect, SecurityGuardian, MlopsEngineer, General, IncidentCommander, ObservabilityExpert, ReliabilityEngineer, DeploymentEngineer). `PersonaResolver` maps task classification to persona. Auto-detect from task, manual override via `/persona set`. `PersonaEvolver` proposes refinements based on task history. Persisted metrics to `.rustant/personas/metrics.json`
- **Prompt Caching** — Provider-level cache support for Anthropic (90% read discount, 25% write premium), OpenAI (50% read discount), and Gemini (75% read discount). `TokenUsage` extended with `cache_read_tokens` and `cache_creation_tokens`. `/cache` shows state, `/cost` shows cache metrics
- **Embeddings** — Pluggable `Embedder` trait with 4 providers: `LocalEmbedder` (128-dim hash TF-IDF), `FastEmbedder` (384-dim, feature-gated), `OpenAiEmbedder` (1536-dim), `OllamaEmbedder`. Factory: `create_embedder(config)`
- **Evaluation Framework** — `TraceEvaluator` trait with built-in evaluators: `LoopDetectionEvaluator`, `SafetyFalsePositiveEvaluator`, `CostEfficiencyEvaluator`. `EvaluationPipeline` produces reports with precision/recall/F1
- **SRE slash commands** — `/incident` (create/list/show/escalate/resolve/postmortem), `/deploy` (status/risk/rollback/canary/history), `/alerts` (list/ack/silence/correlate/rules), `/oncall` (who/schedule/escalate/override), `/circuit` (status/open/close/reset), `/trust` (level/history/promote/demote)
- **Security slash commands** — `/scan`, `/autofix`, `/quality`, `/debt`, `/complexity`, `/findings`, `/license`, `/sbom`, `/risk`, `/compliance`, `/triage`
- **ML slash commands** — 16 new commands in AiEngineer category including `/data`, `/train`, `/model`, `/finetune`, `/rag`, `/evaluate`, `/infer`, `/mlresearch`, and more
- **Development slash commands** — `/init`, `/preview`, `/db`, `/test`, `/lint`, `/deps`, `/verify`, `/repomap`, `/symbols`, `/refs`
- **Webhook channel** — 13th messaging channel for generic webhook integration
- **Cron persistence** — Cron scheduler state now persists to `.rustant/cron/state.json` with atomic write pattern. `load_scheduler()` reads state file first, falls back to config
- **Rollback Registry** — `RollbackEntry` tracks reversible actions with undo info. `RollbackRegistry` (max 100 entries) supports find_by_tool, find_reversible, mark_rolled_back
- **ArXiv multi-source search** — Paper discovery from arXiv API, Semantic Scholar, and OpenAlex with response caching. Rate limiting per source. Citation graph with PageRank, co-citation analysis, bibliographic coupling
- **Gemini function_response fix** — `function_response.response` must be a JSON object; non-object values now wrapped in `{"result": value}`
- **API Rate Limiting & Retry** — Exponential backoff with jitter for all LLM providers. `RetryConfig` with configurable max retries (default 3), initial backoff (1s), max backoff (60s), and multiplier (2x). Retryable: 429, timeouts, connection failures, streaming errors
- **Secret Reference System** — `SecretRef` type for secure credential resolution via OS keychain, env vars, or inline plaintext (deprecated). `rustant setup migrate-secrets` CLI command
- **CDC — Change Data Capture** — Stateful channel polling with cursor-based tracking, reply-chain detection, and communication style learning. `/cdc` slash command suite
- **ArXiv Implementation Pipeline** — `implement`, `setup_env`, `verify`, `implementation_status` actions with 6-language environment isolation
- **LLM Council** — Multi-model deliberation with parallel query, anonymous peer review, chairman synthesis. `/council` slash command
- **Voice direct audio playback** — `/voice speak` plays through speakers via `afplay`/`aplay`
- **Voice wake word mode** — `rustant --voice` activates wake word listening
- **Chrome DevTools MCP integration** — External MCP server support via `[[mcp_servers]]` config with 26 browser tools
- **Feature flags** — 10 runtime feature flags: `prompt_caching`, `semantic_search`, `dynamic_personas`, `evaluation`, `security_scanning`, `compliance_engine`, `incident_response`, `sre_mode`, `progressive_trust`, `global_circuit_breaker`

### Fixed

- **Gemini provider hang** — 120s HTTP timeout + 10s connect timeout. True incremental SSE streaming. `warn!()` for malformed chunks. Empty `parts` filtering
- **Gemini API sequencing** — `fix_gemini_turns()` merges consecutive same-role turns, fixes `functionResponse.name`, ensures user-first ordering
- **Tool registration gap** — All macOS native tools now executable (was: visible to LLM but returned `None`). Fixed via `register_agent_tools_from_registry()` with `Arc<ToolRegistry>` fallback
- **Cron ephemeral state** — Scheduler now persists to disk; previously each CLI invocation created ephemeral in-memory scheduler

### Changed

- **Slash command registry expanded** — 117 commands across 9 categories (General, Agent, Memory, Safety, Session, Debug, Workflow, Channel, Sre, Development, AiEngineer)
- **Workflow templates expanded** — 38 built-in templates (was 12), grouped by category
- **Tool count** — 73 base tools on macOS (45 base + 3 iMessage + 25 macOS native), plus 33 security tools and 54 ML tools registered separately
- **Workspace expanded** — 8 crates (added `rustant-security` and `rustant-ml`)

### Removed

- **TUI mode** — Removed ratatui-based TUI interface. REPL is now the sole interactive mode. All TUI-specific widgets (ExplanationPanel, TaskBoard, ProgressBar) and keybindings (Ctrl+E/T/S/D) removed. `--tui` flag removed

## [1.0.0] - 2026-02-02

### Added

- **Agent core** — Think-Act-Observe (ReAct) loop with configurable max iterations
- **Multi-provider LLM** — OpenAI, Anthropic, Gemini, Azure, Ollama, vLLM support
- **Failover provider** — Circuit-breaker failover across multiple LLM backends
- **12 built-in tools** — file_read, file_list, file_search, file_write, file_patch, git_status, git_diff, git_commit, shell_exec, echo, datetime, calculator
- **LSP tools** — Language Server Protocol integration for code intelligence
- **Three-tier memory** — Working, short-term, and long-term memory with auto-summarization
- **Five-layer safety** — Input validation, authorization, sandboxing, output validation, audit trail
- **Prompt injection detection** — Pattern-based scanning for known attack vectors
- **Merkle chain audit** — Tamper-evident execution history
- **12 messaging channels** — Slack, Discord, Telegram, Email, Matrix, Signal, WhatsApp, SMS, IRC, Teams, iMessage, WebChat
- **Slack deep integration** — Send, history, channels, users, reactions, DMs, threads, files, teams, groups
- **OAuth authentication** — Browser-based OAuth flows with token refresh
- **Credential storage** — OS keyring integration (macOS Keychain, Linux Secret Service, Windows Credential Manager)
- **Workflow engine** — Declarative multi-step workflows with inputs, outputs, and gates
- **Cron scheduler** — Cron-based task scheduling with background job management
- **Voice interface** — Text-to-speech and speech-to-text via OpenAI
- **Browser automation** — Headless Chrome via CDP
- **Canvas system** — Rich content rendering (charts, tables, forms, diagrams via Mermaid)
- **Skills system** — SKILL.md-based declarative tool definitions with security validation
- **Plugin system** — Native (libloading) and WASM (wasmi) plugin loading
- **Hook system** — 7 hook points for plugin interception of agent behavior
- **MCP server** — Model Context Protocol server via JSON-RPC 2.0
- **MCP client** — Connect to external MCP servers for tool discovery
- **WebSocket gateway** — Remote access with session management and REST API
- **Multi-agent** — Agent spawning, message bus, routing, orchestration
- **Hybrid search** — Tantivy full-text + SQLite vector search
- **TUI interface** — ratatui-based terminal UI
- **Dashboard UI** — Tauri-based desktop dashboard (rustant-ui)
- **Self-update** — GitHub Releases-based update checking and binary replacement
- **Cross-platform CI** — GitHub Actions with Linux + macOS testing
- **Security audit** — cargo-audit in CI pipeline
- **Shell installer** — curl-based installer for Linux/macOS
- **Homebrew formula** — macOS package installation
- **cargo-binstall** — Pre-built binary installation support
- **mdbook documentation** — User guide, architecture docs, plugin development guide
