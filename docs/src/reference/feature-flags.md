# Feature Flags Reference

Rustant uses runtime feature flags to control which major subsystems are active. Flags are configured under `[features]` in `config.toml` and use `#[serde(default)]` for backward compatibility -- unknown flags are silently ignored.

## Configuration

```toml
[features]
prompt_caching = true
semantic_search = true
dynamic_personas = false
evaluation = true
security_scanning = false
compliance_engine = false
incident_response = false
fullstack_mode = false
ai_engineer = false
ai_eval = false
ai_inference = false
ai_rag = false
ai_training = false
ai_research = false
```

All flags can also be set via environment variables using the `RUSTANT_FEATURES__` prefix:

```bash
RUSTANT_FEATURES__SECURITY_SCANNING=true rustant
```

## Flags

### prompt_caching

**Default: `true`**

Enables provider-level prompt caching to reduce latency and cost by reusing cached prompt prefixes.

When enabled:
- Anthropic: Adds `cache_control: {"type": "ephemeral"}` markers on system blocks and last tool definition. Uses beta header `prompt-caching-2024-07-31`. 90% discount on cache reads, 25% premium on cache writes.
- OpenAI: Automatic caching. Parses `prompt_tokens_details.cached_tokens`. 50% discount on cache reads.
- Gemini: Parses `cachedContentTokenCount` from `usageMetadata`. 75% discount on cache reads.

Monitor via `/cache` in the REPL. Token usage includes `cache_read_tokens` and `cache_creation_tokens` fields.

### semantic_search

**Default: `true`**

Enables semantic search over the project index and knowledge base. Uses the configured embedding provider for vector similarity search.

When enabled:
- `LocalEmbedder` (128-dim hash TF-IDF) is always available as a fallback.
- `FastEmbedder` (384-dim) available with the `semantic-search` compile-time feature.
- `OpenAiEmbedder` (1536-dim) available with `OPENAI_API_KEY`.
- `OllamaEmbedder` available when Ollama is running locally.

Configure the embedding provider under `[embedding]`.

### dynamic_personas

**Default: `false`**

Enables dynamic persona evolution based on task performance. When enabled, the `PersonaEvolver` analyzes task history and proposes persona refinements (prompt revisions on low success rates, confidence adjustments on high iteration counts).

When disabled, personas still function but do not automatically evolve. Manual persona selection via `/persona set <name>` always works.

Personas available: `Architect`, `SecurityGuardian`, `MlopsEngineer`, `General`, `IncidentCommander`, `ObservabilityExpert`, `ReliabilityEngineer`, `DeploymentEngineer`.

### evaluation

**Default: `true`**

Enables the trace evaluation framework for analyzing agent performance. Built-in evaluators:

- `LoopDetectionEvaluator` -- Detects when the same tool is called more than N times consecutively.
- `SafetyFalsePositiveEvaluator` -- Flags when a tool call is denied then later approved (indicating an overly strict safety config).
- `CostEfficiencyEvaluator` -- Tracks tokens consumed per tool call to identify expensive patterns.

The evaluation pipeline generates an `EvaluationReport` with precision, recall, and F1 scores.

### security_scanning

**Default: `false`**

Enables the rustant-security crate's scanning tools: SAST (15+ rules across languages), SCA (dependency vulnerability scanning), secrets detection (60+ regex patterns + Shannon entropy), container scanning, IaC scanning (12 rules), and supply chain analysis.

When enabled:
- 33 security tool wrappers become active.
- Findings are stored in `.rustant/security/findings.json`.
- Audit trail includes security-specific event types.
- `/scan`, `/autofix`, `/findings` slash commands become functional.

### compliance_engine

**Default: `false`**

Enables compliance reporting, license checking, SBOM generation, and policy enforcement.

When enabled:
- License compliance checking against SPDX identifiers with gate/warn/inform rules.
- SBOM generation in CycloneDX, SPDX, and CSV formats with diff support.
- Compliance report generation for SOC 2, ISO 27001, NIST, PCI-DSS, OWASP frameworks.
- Risk scoring with multi-dimensional analysis.
- VEX (Vulnerability Exploitability eXchange) document support.
- `/license`, `/sbom`, `/compliance`, `/risk` slash commands become functional.

### incident_response

**Default: `false`**

Enables threat detection, alert management, playbook execution, and learning from past incidents.

When enabled:
- Rule-based threat detection with MITRE ATT&CK mapping.
- Alert correlation engine with priority scoring.
- Playbook registry with trigger matching and automated execution.
- Log parsers for 6+ formats including Kubernetes audit logs and CloudTrail.
- Action registry with 8 predefined incident response actions.
- Learning system with hotspot tracking and confidence updates.

### fullstack_mode

**Default: `false`**

Enables fullstack development features: context hydration pipeline, verification loop, and project templates.

When enabled:
- Context hydration combines repo map ranking with token-budgeted context assembly.
- Verification loop runs test/lint/typecheck with iterative fix-and-recheck (max 3 attempts).
- Project templates: react_vite, nextjs, fastapi, rust_axum, sveltekit, express.
- Development tools: scaffold, dev_server, database, test_runner, lint.
- `/init`, `/preview`, `/db`, `/test`, `/lint`, `/deps`, `/verify` slash commands become functional.

### ai_engineer

**Default: `false`**

Master switch for AI/ML engineering tools from the rustant-ml crate. Controls registration of 54 ML tool wrappers.

When enabled, also activates sub-flags: `ai_eval`, `ai_inference`, `ai_rag`, `ai_training`, `ai_research`.

### ai_eval

**Default: `false`**

Enables AI model evaluation tools: LLM-as-judge, error analysis, domain evaluations, benchmark suites.

### ai_inference

**Default: `false`**

Enables AI inference serving tools: model serving via Ollama/vLLM/llama.cpp/Candle backends, endpoint management, profiling.

### ai_rag

**Default: `false`**

Enables RAG (Retrieval-Augmented Generation) pipeline tools: document ingestion, chunking, retrieval, reranking, grounding checks, evaluation.

### ai_training

**Default: `false`**

Enables AI training tools: experiment tracking, hyperparameter sweeps, checkpointing, reproducibility enforcement.

### ai_research

**Default: `false`**

Enables AI research tools: methodology comparison, literature review, reproducibility checking, bibliography management.

## Checking Active Flags

In the REPL, use `/config features` to see which flags are currently active. The `/doctor` command also reports on feature flag status.
