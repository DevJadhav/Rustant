# Deep Research Architecture

The deep research engine orchestrates a multi-phase pipeline for answering complex research questions. It decomposes questions into sub-query DAGs, gathers information from multiple sources, detects contradictions, and synthesizes coherent reports.

## Module Structure

```
rustant-core/src/research/
  mod.rs            — Module declarations, re-exports
  engine.rs         — ResearchEngine (pipeline orchestrator)
  decomposition.rs  — QuestionDecomposer (sub-query DAG)
  sources.rs        — SourceTracker (evidence collection)
  contradiction.rs  — ContradictionDetector (conflict analysis)
  synthesis.rs      — ResearchSynthesizer (result merging)
  output.rs         — ReportGenerator (formatting)
  session.rs        — ResearchSession (state persistence)
```

## 5-Phase Pipeline

### Phase 1: Decompose

`QuestionDecomposer` breaks a complex question into a dependency graph of `SubQuery` nodes:

- Detects comparative questions ("X vs Y") and splits into parallel sub-queries
- Identifies "how to" questions and generates step-oriented sub-queries
- Assigns tool requirements per sub-query (arxiv, web, knowledge graph)
- Computes dependencies between sub-queries

### Phase 2: Query

Executes sub-queries in parallel (respecting dependency ordering) using existing tools:

- `arxiv_research` for academic papers
- `web_search` / `web_fetch` for web sources
- `knowledge_graph` for structured knowledge
- `http_api` for API endpoints

Each result is tracked as a `ResearchSource` with reliability scoring.

### Phase 3: Synthesize

`ResearchSynthesizer` merges sub-query results:

- Extracts key findings from each source
- Assigns confidence: `avg_reliability * 0.4 + completion_rate * 0.6 - contradiction_penalty`
- Identifies gaps: incomplete queries, low source diversity, unverified claims

### Phase 4: Verify

Iterative refinement loop (configurable, default max 3 iterations):

1. Check if confidence exceeds threshold (0.85)
2. Identify remaining gaps
3. Generate additional sub-queries to fill gaps
4. Re-synthesize with new data
5. Stop when confident or max iterations reached

### Phase 5: Report

`ReportGenerator` produces output in 4 formats:

| Format | Description |
|--------|-------------|
| `Summary` | 1-2 paragraph concise answer |
| `DetailedReport` | Full report with sections, citations, analysis |
| `AnnotatedBibliography` | Source list with annotations and claims |
| `ImplementationRoadmap` | Step-by-step implementation plan |

## Contradiction Detection

`ContradictionDetector` identifies conflicting claims across sources:

- **Direct Negation**: Detects opposing claims using negation words + keyword overlap
- **Numeric Disagreement**: Finds different numbers for the same metric
- Keyword extraction with stop-word removal
- Jaccard similarity for claim overlap measurement

## Research Depth Levels

| Depth | Sub-queries | Verification | Typical Duration |
|-------|-------------|--------------|------------------|
| Quick | 1-2 | None | 1-2 min |
| Detailed | Full DAG | 1 iteration | 5-10 min |
| Comprehensive | Full DAG | 3 iterations | 15-30 min |

## Session Persistence

`ResearchSession` implements a state machine:

```
Decomposing → Querying → Synthesizing → Verifying → Complete
                                          ↓
                                        Paused / Failed
```

Sessions are persisted to `.rustant/research/sessions/<uuid>.json` with full state including sub-queries, sources, and synthesis results. Sessions can be resumed via `/deepresearch resume <id>`.

## LLM Council Integration

When `use_council: true` in `ResearchConfig`, the synthesis phase uses multi-model deliberation via `PlanningCouncil` for higher-quality analysis. This requires multiple LLM providers to be configured.

## MoE Integration

The Research expert (10th expert in MoE) routes research-related tasks to a focused toolset: `arxiv_research`, `web_search`, `web_fetch`, `knowledge_graph`, `document_read`, `http_api`, `calculator`.
