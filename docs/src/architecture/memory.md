# Memory System

Rustant uses a three-tier memory system to manage context across conversations and sessions.

## Three Tiers

### Working Memory

The current context window sent to the LLM. Contains:

- System prompt
- Recent conversation messages
- Active tool results
- Current task context

Limited by the LLM's context window size. When it grows too large, auto-summarization kicks in.

### Short-Term Memory

A sliding window of recent messages beyond the working memory limit:

- Configurable size (default: 100 messages)
- Older messages are summarized and moved to long-term storage
- Available for recall when the agent needs recent context

### Long-Term Memory

Persistent storage across sessions:

- Stored in a local database
- Searchable via hybrid search (full-text + vector)
- Auto-indexed when messages are archived from short-term memory
- Survives application restarts

Long-term memory also stores two specialized entry types for cross-session learning:

- **Facts** — Tool execution results (10-5000 chars) are automatically recorded with tool name tags after successful execution. Short or excessively large outputs are filtered to avoid noise and memory bloat.
- **Corrections** — When a user denies a proposed tool action, the original attempt, denial reason, and goal context are recorded as a correction entry.

The `KnowledgeDistiller` processes accumulated facts and corrections into behavioral rules that are injected into the system prompt via `Brain.set_knowledge_addendum()`. This enables the agent to learn from past sessions — avoiding previously denied actions and leveraging successful tool patterns.

## Auto-Summarization

When the working memory approaches the context limit, Rustant automatically:

1. Identifies the oldest messages in the context window
2. Generates a summary via the LLM
3. Replaces the detailed messages with the summary
4. Moves the original messages to short-term memory

When LLM-based summarization fails (e.g., provider error or timeout), a structure-preserving fallback (`smart_fallback_summary`) is used instead of naive truncation. This preserves the first user message, tool names, result previews, and the latest message — maintaining useful context even without an LLM call.

This allows for long-running sessions without losing important context.

## Search

The search engine combines two backends:

- **Tantivy** — Full-text search with BM25 ranking for keyword queries
- **SQLite-backed vector search** — Semantic similarity search for conceptual queries

```toml
[search]
enabled = true
index_dir = ".rustant/search_index"
max_results = 20
```

## Configuration

```toml
[memory]
working_limit = 20         # Max messages in working memory
short_term_limit = 100     # Max messages in short-term memory
long_term_enabled = true   # Enable persistent long-term memory
auto_summarize = true      # Enable auto-summarization
```
