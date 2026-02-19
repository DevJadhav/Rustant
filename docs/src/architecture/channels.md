# Channels & CDC

## Channel Architecture

13 integrations implement the unified `Channel` trait:

```text
trait Channel: Send + Sync {
    fn name(&self) -> &str;
    fn send(&self, message: &str) -> Result<()>;
    fn receive(&self) -> Result<Vec<Message>>;
    fn test_connection(&self) -> Result<()>;
}
```

### ChannelManager

`ChannelManager` coordinates all active channels:
- Registration and lifecycle management
- `ChannelAgentBridge` normalizes messages from all platforms into unified format
- Routes agent responses back through originating channel

### Intelligence Pipeline

Two-tier classification (`intelligence.rs`, `auto_reply.rs`):

1. **Heuristic Tier** — Fast pattern matching (<1ms) with `ClassificationCache` (wrapped in `RwLock`)
2. **LLM Tier** — Semantic classification when heuristic confidence < 0.7

`AutoReplyEngine` modes:
- `FullAuto` — Send all generated replies
- `AutoWithApproval` — Queue high-priority for review
- `DraftOnly` — Generate but don't send
- `Disabled` — No auto-reply

### Digest System

`DigestCollector` aggregates messages for periodic summaries:
- Frequencies: Off, Hourly, Daily, Weekly
- Includes highlights, action items, markdown export
- Stored in `.rustant/digests/`

### Email Intelligence

`EmailIntelligence` provides email-specific features:
- Auto-categorization: NeedsReply, ActionRequired, FYI, Newsletter, Automated
- Sender profile tracking over time
- Background IMAP polling with newsletter detection

### Scheduler Bridge

`SchedulerBridge` extracts scheduling information:
- Follow-up reminders for action-required messages
- ICS calendar export to `.rustant/reminders/`

## Change Data Capture (CDC)

### Architecture

`CdcProcessor` coordinates the polling pipeline:

```
Poll → Classify → Priority Boost → Auto-Reply → Style Track
```

### Cursor Tracking

Per-channel cursors persisted to `.rustant/cdc/state.json`:
- Slack: message timestamp
- Email: IMAP UID
- Telegram: update offset
- No duplicate processing

### Communication Style Learning

`CommunicationStyleTracker` builds per-sender profiles:
- Formality level
- Emoji usage patterns
- Greeting styles
- Common topics
- Generated as `Fact` entries for long-term memory after `style_fact_threshold` messages (default: 50)

### SecretRef

Channel credentials use `SecretRef` for secure resolution:
- `keychain:<account>` — OS keychain
- `env:<VAR>` — Environment variable
- Bare string — Deprecated plaintext

`migrate_channel_secrets()` moves plaintext to keychain.
