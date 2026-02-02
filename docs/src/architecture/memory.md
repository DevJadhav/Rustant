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

## Auto-Summarization

When the working memory approaches the context limit, Rustant automatically:

1. Identifies the oldest messages in the context window
2. Generates a summary via the LLM
3. Replaces the detailed messages with the summary
4. Moves the original messages to short-term memory

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
