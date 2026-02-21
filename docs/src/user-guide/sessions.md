# Sessions & Memory

## Session Management

Rustant provides persistent session management with auto-save, resume, search, and tagging.

### Saving and Loading

Sessions are auto-saved periodically and on exit. You can also manage them manually:

```
/session save my-project     # Save current session with name
/session load my-project     # Load a saved session
/session list                # List all sessions
```

### Resuming Sessions

```bash
# CLI
rustant resume               # Resume most recent session
rustant resume my-project    # Resume by name (fuzzy prefix match)

# REPL
/resume                      # Resume most recent
/resume my-project           # Resume by name
```

### Searching and Tagging

```
/sessions                        # List saved sessions with details
/sessions search "auth module"   # Full-text search across names, goals, summaries
/sessions tag my-project auth    # Tag a session
/sessions filter auth            # List sessions with tag
```

Sessions show relative timestamps ("2 days ago") for quick scanning.

### Auto-Recovery

On startup, Rustant checks for the last session and offers to resume it. On exit, unsaved work triggers a save prompt.

## Memory System

### Working Memory
The current context window sent to the LLM. Auto-compression triggers when messages exceed 2x the window size (default: 24 messages).

### Message Pinning

Pin important messages to survive context compression:

```
/pin          # Pin the last message
/pin 5        # Pin message #5
/unpin 5      # Unpin message #5
```

Pinned messages are always included in the context window.

### Context Management

```
/context      # Show context window usage breakdown
/memory       # Show memory system stats
/compact      # Manually compress conversation context
/cost         # Show token usage, cost, and cache metrics
```

### Long-Term Memory

Successful tool results (10-5000 chars) are recorded as facts for cross-session learning. User denials are recorded as corrections. The `KnowledgeDistiller` converts these into behavioral rules.

Capacity: max_facts=10,000, max_corrections=1,000 with FIFO eviction.
