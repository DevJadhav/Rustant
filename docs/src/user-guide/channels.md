# Channels & Messaging

Rustant supports 13 messaging channels for receiving and responding to messages. Each channel implements the unified `Channel` trait.

## Supported Channels

| Channel | Auth Method | Config Key |
|---------|-------------|------------|
| Slack | OAuth | `[channels.slack]` |
| Discord | OAuth | `[channels.discord]` |
| Telegram | Bot token | `[channels.telegram]` |
| Email (Gmail) | OAuth | `[channels.email]` |
| Matrix | Access token | `[channels.matrix]` |
| Signal | Signal CLI | `[channels.signal]` |
| WhatsApp | OAuth | `[channels.whatsapp]` |
| SMS (Twilio) | Account SID + Auth token | `[channels.sms]` |
| IRC | Server/nick | `[channels.irc]` |
| Teams | OAuth | `[channels.teams]` |
| iMessage | AppleScript (macOS) | `[channels.imessage]` |
| WebChat | Gateway WebSocket | `[channels.webchat]` |
| Webhook | HTTP endpoint | `[channels.webhook]` |

## Channel Management

```bash
rustant channel list          # List configured channels
rustant channel test slack    # Test a channel connection
```

## OAuth Channels

For Slack, Discord, Teams, WhatsApp, and Gmail:

```bash
rustant auth login slack
rustant auth status
rustant auth refresh slack
rustant auth logout slack
```

## Slack Operations

Rustant has deep Slack integration:

```bash
rustant channel slack send general "Hello from Rustant!"
rustant channel slack history general -n 20
rustant channel slack channels
rustant channel slack users
rustant channel slack dm U0AC521V7UK "Direct message"
rustant channel slack thread C04M40V9B61 1770007692.977549 "Thread reply"
rustant channel slack react C04M40V9B61 1770007692.977549 thumbsup
rustant channel slack files
rustant channel slack team
rustant channel slack groups
```

## Channel Intelligence

Rustant automatically processes incoming messages with an intelligent classification and response pipeline:

- **Two-Tier Classification** — Fast heuristic pattern matching (<1ms) + LLM-based semantic classification for ambiguous messages, with caching
- **Auto-Reply** — Modes: `full_auto`, `auto_with_approval`, `draft_only`, `disabled`. Safety-gated through SafetyGuardian
- **Channel Digests** — Periodic summaries (hourly/daily/weekly) with highlights and action items
- **Smart Scheduling** — Automatic follow-up reminders with ICS calendar export
- **Email Intelligence** — Auto-categorization (NeedsReply, ActionRequired, FYI, Newsletter, Automated), sender profile tracking
- **Quiet Hours** — Suppress all auto-actions during configured time windows

```toml
[intelligence]
enabled = true

[intelligence.defaults]
auto_reply = "full_auto"
digest = "daily"
smart_scheduling = true

[intelligence.channels.email]
auto_reply = "draft_only"
digest = "daily"

[intelligence.channels.slack]
auto_reply = "full_auto"
digest = "hourly"
```

REPL commands: `/digest`, `/digest history`, `/replies`, `/replies approve <id>`, `/reminders`, `/intelligence on/off`.

## Change Data Capture (CDC)

CDC provides stateful, cursor-based polling with automatic reply-chain detection and communication style learning.

1. **Cursor Tracking** — Per-channel cursors (Slack timestamp, IMAP UID, etc.) persisted to `.rustant/cdc/state.json`
2. **Reply-Chain Detection** — Agent tracks sent message IDs; replies receive priority boost
3. **Style Learning** — Per-sender profiles (formality, emoji, greetings, topics) fed into long-term memory

### CDC Commands

```
/cdc status              # Show polling state
/cdc on / off            # Enable/disable globally
/cdc interval slack 30   # Set per-channel interval
/cdc enable email        # Enable CDC for channel
/cdc disable imessage    # Disable CDC for channel
/cdc cursors             # Show cursor positions
/cdc style               # Show learned styles
```

### CDC Configuration

```toml
[cdc]
enabled = true
default_interval_secs = 60
sent_record_ttl_days = 7
style_fact_threshold = 50

[cdc.channel_intervals]
slack = 30
email = 300
```

## Credential Security

Channel tokens support `SecretRef` format:

- **Keychain**: `bot_token_ref = "keychain:channel:slack:bot_token"`
- **Environment**: `bot_token_ref = "env:SLACK_BOT_TOKEN"`
- **Inline** (deprecated): plain string values still work but emit warnings

Migrate plaintext tokens: `rustant setup migrate-secrets`

## Configuration Example

```toml
[channels.slack]
enabled = true
auth_method = "oauth"

[channels.telegram]
enabled = true
bot_token = "env:TELEGRAM_BOT_TOKEN"
allowed_chat_ids = [123456789]

[channels.email]
enabled = true
auth_method = "oauth"
poll_interval_secs = 60

[channels.webhook]
enabled = true
port = 8080
secret = "keychain:channel:webhook:secret"
```
