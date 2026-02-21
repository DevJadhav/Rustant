# Memory System

Rustant uses a three-tier memory system to manage context across conversations and sessions.

## Three Tiers

### Working Memory

The current context window sent to the LLM. Contains system prompt, recent conversation messages, active tool results, and current task context. Limited by the LLM's context window size.

### Short-Term Memory

A sliding window of recent messages beyond the working memory limit:
- Configurable size (default: 12 messages, compression at 24)
- **Message pinning** — `/pin [n]` pins messages that survive context compression. `/unpin <n>` removes the pin. Pinned messages are always included in the context window regardless of compression.
- Older messages are summarized and moved to long-term storage

### Long-Term Memory

Persistent storage across sessions with two specialized entry types:

- **Facts** — Tool execution results (10-5000 chars) automatically recorded after successful execution. Tags include tool name for searchable recall.
- **Corrections** — When a user denies a proposed tool action, the original attempt, denial reason, and goal context are recorded.

**Capacity limits**: max_facts=10,000, max_corrections=1,000 with FIFO eviction when limits are reached.

The `KnowledgeDistiller` processes accumulated facts and corrections into behavioral rules injected into the system prompt via `Brain.set_knowledge_addendum()`. This enables cross-session learning.

## Auto-Summarization

When working memory approaches the context limit (compression triggers at 2x window_size = 24 messages):

1. Identifies the oldest messages in the context window
2. Generates a summary via the LLM
3. Replaces detailed messages with the summary
4. Moves originals to short-term memory

**Fallback**: When LLM summarization fails, `smart_fallback_summary` preserves the first user message, tool names, result previews, and latest message — maintaining useful context without an LLM call.

## Embeddings

Pluggable `Embedder` trait with 4 providers for semantic search over long-term memory:

| Provider | Dimensions | Notes |
|----------|-----------|-------|
| `LocalEmbedder` | 128 | Hash-based TF-IDF, always available |
| `FastEmbedder` | 384 | Feature-gated (`semantic-search`) |
| `OpenAiEmbedder` | 1536 | Requires API key |
| `OllamaEmbedder` | Varies | Local, uses Ollama models |

Configure via `[embeddings]` in config.toml.

## Search

The search engine combines two backends:

- **Tantivy** — Full-text search with BM25 ranking for keyword queries
- **SQLite-backed vector search** — Semantic similarity search for conceptual queries

## Configuration

```toml
[memory]
window_size = 12            # Max messages in working memory
enable_persistence = true   # Enable persistent long-term memory

[search]
enabled = true
index_dir = ".rustant/search_index"
max_results = 20

[embeddings]
provider = "local"          # local, fast, openai, ollama
```
