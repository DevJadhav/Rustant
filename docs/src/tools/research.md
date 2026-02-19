# Research Tools

Rustant provides a comprehensive ArXiv research tool (`arxiv_research`) with 22 actions for searching, managing, analyzing, and implementing academic papers. The tool integrates with multiple paper sources and supports code generation from research papers.

## Overview

| Feature | Description |
|---------|-------------|
| **Tool name** | `arxiv_research` |
| **Risk level** | Read-only (search/analyze) / Write (library/implement) |
| **State** | `.rustant/arxiv/library.json` |
| **Paper sources** | arXiv API, Semantic Scholar, OpenAlex |
| **Languages** | Python, Rust, TypeScript, Go, C++, Julia |

---

## Actions

The tool supports 22 actions organized into search, library management, analysis, implementation, and citation graph operations.

### Search & Fetch (5 actions)

| Action | Description |
|--------|-------------|
| `search` | Search arXiv for papers by query, category, and sort order |
| `fetch` | Fetch full metadata for a paper by arXiv ID |
| `analyze` | Deep analysis of a paper's methods and contributions |
| `compare` | Compare multiple papers side-by-side |
| `trending` | Show trending papers in a category |

**search parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `query` | string | Search query |
| `category` | string | ArXiv category (e.g., `cs.AI`, `cs.LG`, `cs.CL`) |
| `max_results` | integer | Maximum results (default: 10, max: 50) |
| `sort_by` | string | `relevance`, `date`, or `updated` |

**Example:**
```json
{
  "action": "search",
  "query": "transformer attention mechanisms",
  "category": "cs.LG",
  "max_results": 10,
  "sort_by": "relevance"
}
```

### Library Management (5 actions)

| Action | Description |
|--------|-------------|
| `save` | Save a paper to your local library |
| `library` | List papers in your library |
| `remove` | Remove a paper from the library |
| `export_bibtex` | Export library entries as BibTeX |
| `collections` | Manage paper collections (list, create, rename, delete) |

**collections sub-actions:**

| Sub-action | Description |
|------------|-------------|
| `list` | List all collections |
| `create` | Create a new collection |
| `rename` | Rename an existing collection |
| `delete` | Delete a collection |

### Analysis & Summarization (3 actions)

| Action | Description |
|--------|-------------|
| `semantic_search` | Keyword search over your local library |
| `summarize` | Multi-level summarization with audience awareness |
| `digest_config` | Configure digest settings for paper updates |

**summarize parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `arxiv_id` | string | Paper to summarize |
| `depth` | string | `quick`, `standard`, or `full` |
| `audience` | string | Target audience (e.g., "researcher", "engineer", "executive") |

**Example:**
```json
{
  "action": "summarize",
  "arxiv_id": "1706.03762",
  "depth": "full",
  "audience": "engineer"
}
```

### Code Generation & Implementation (5 actions)

| Action | Description |
|--------|-------------|
| `paper_to_code` | Generate code scaffolding from a paper |
| `paper_to_notebook` | Generate a 4-layer progressive notebook from a paper |
| `implement` | Full implementation of a paper's methods |
| `setup_env` | Set up an isolated environment for a specific language |
| `verify` | Verify an implementation against the paper's claims |

**Supported languages:** Python, Rust, TypeScript, Go, C++, Julia.

Each language gets an isolated environment for implementation. The `setup_env` action creates the appropriate project structure and dependency files.

**paper_to_notebook** generates 4 progressive layers:
1. Theory and background
2. Core algorithm walkthrough
3. Implementation with code cells
4. Experiments and results

**Example:**
```json
{
  "action": "paper_to_code",
  "arxiv_id": "2301.12345",
  "language": "python",
  "output_type": "project"
}
```

### Implementation Tracking (2 actions)

| Action | Description |
|--------|-------------|
| `implementation_status` | Check status of in-progress implementations |
| `reindex` | Rebuild the local search index |

### Citation Graph (2 actions)

| Action | Description |
|--------|-------------|
| `citation_graph` | Build and query citation graphs with advanced analysis |
| `blueprint` | Generate dependency-ordered implementation plans |

#### Citation Graph Analysis

The `citation_graph` action builds a graph of paper citations and supports:

| Analysis | Description |
|----------|-------------|
| **PageRank** | Rank papers by citation influence |
| **Co-citation analysis** | Find papers frequently cited together |
| **Bibliographic coupling** | Find papers that cite the same sources |
| **Path finding** | Find citation paths between two papers |

**Example:**
```json
{
  "action": "citation_graph",
  "arxiv_id": "1706.03762",
  "analysis": "pagerank",
  "depth": 2
}
```

#### Blueprint Generation

The `blueprint` action uses topological sorting on the citation/dependency graph to generate a dependency-ordered implementation plan. This ensures prerequisites are implemented before dependent components.

---

## Multi-Source Search

The research tool queries three paper sources simultaneously for comprehensive coverage:

| Source | Rate Limit | Features |
|--------|-----------|----------|
| **arXiv API** | 3s between requests | Full metadata, PDF links, categories |
| **Semantic Scholar** | 1s between requests | Citation counts, influence scores, abstracts |
| **OpenAlex** | 0.1s between requests | Open access status, institutional data, concepts |

Rate limiting is enforced per-source using `Mutex<Option<Instant>>` to prevent API abuse.

Response caching reduces redundant API calls when the same paper is accessed multiple times.

---

## Library State

The library state is persisted to `.rustant/arxiv/library.json` using the atomic write pattern (write to `.json.tmp`, then rename). The state includes:

- Saved papers with full metadata
- Collections and their membership
- Implementation records and status
- Search index data

---

## Related REPL Commands

| Command | Aliases | Description |
|---------|---------|-------------|
| `/arxiv` | `/research`, `/paper` | Access arXiv research tool |

The `/arxiv` slash command supports all 22 actions via subcommands:

```
/arxiv search transformer attention mechanisms
/arxiv fetch 1706.03762
/arxiv summarize 2301.12345 --depth full
/arxiv library
/arxiv citation_graph 1706.03762 --analysis pagerank
/arxiv blueprint 2301.12345 --language python
```

## Related Workflow Template

The `arxiv_research` workflow template provides a guided research workflow:
1. Search for papers on a topic
2. Save relevant papers to library
3. Summarize key papers
4. Build citation graph
5. Generate implementation blueprint
6. Implement and verify
