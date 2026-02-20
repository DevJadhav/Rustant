# Deep Research Mode

Rustant's deep research engine orchestrates a multi-phase pipeline for answering complex research questions. It decomposes questions, gathers information from multiple sources, detects contradictions, and synthesizes coherent reports.

## Quick Start

```
/research start "What are the latest advances in prompt caching?"
```

## Research Depths

| Depth | Sub-queries | Verification | Time |
|-------|-------------|--------------|------|
| `quick` | 1-2 | None | ~1-2 min |
| `detailed` | Full DAG | 1 iteration | ~5-10 min |
| `comprehensive` | Full DAG | 3 iterations | ~15-30 min |

## Pipeline Phases

1. **Decompose** — Breaks the question into a dependency graph of sub-queries
2. **Query** — Executes sub-queries in parallel using web search, ArXiv, and other tools
3. **Synthesize** — Merges results, weights evidence, detects contradictions
4. **Verify** — Iterative refinement: identifies gaps, runs additional queries
5. **Report** — Generates output in the requested format

## Output Formats

- `summary` — Concise 1-2 paragraph answer
- `detailed_report` — Full report with sections, citations, and analysis
- `annotated_bibliography` — Source list with annotations and claims
- `implementation_roadmap` — Step-by-step implementation plan

## Commands

| Command | Description |
|---------|-------------|
| `/research start <question>` | Start a new research session |
| `/research status` | Show current session status |
| `/research resume <id>` | Resume a paused session |
| `/research sessions` | List all saved sessions |
| `/research report [format]` | Generate report in specified format |
| `/research depth <level>` | Set research depth |

## Configuration

```toml
[research]
enabled = true
default_depth = "detailed"
max_parallel_queries = 5
use_council = false  # Use LLM Council for synthesis
max_refinement_iterations = 3
```

## Session Persistence

Research sessions are saved to `.rustant/research/sessions/<uuid>.json` and can be resumed later. Sessions track all sub-queries, source data, and synthesis results.

## Using with LLM Council

When `use_council = true`, the synthesis phase uses multi-model deliberation via the LLM Council for higher-quality analysis. This requires multiple LLM providers configured.
