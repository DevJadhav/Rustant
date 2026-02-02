# Channels

Rustant supports 12 messaging channels for receiving and responding to messages. Each channel implements the unified `Channel` trait.

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
rustant channel slack info C04M40V9B61
rustant channel slack react C04M40V9B61 1770007692.977549 thumbsup
rustant channel slack dm U0AC521V7UK "Direct message"
rustant channel slack thread C04M40V9B61 1770007692.977549 "Thread reply"
rustant channel slack join C04M40V9B61
rustant channel slack files
rustant channel slack team
rustant channel slack groups
```

## Configuration Example

```toml
[channels.slack]
enabled = true
auth_method = "oauth"

[channels.telegram]
enabled = true
bot_token = "your-bot-token"
allowed_chat_ids = [123456789]

[channels.email]
enabled = true
auth_method = "oauth"
poll_interval_secs = 60
```

## Channel Agent Bridge

When channels are enabled, incoming messages are routed to the agent via the `ChannelAgentBridge`. The bridge normalizes messages from all platforms into a unified format, routes them to the agent, and sends responses back through the originating channel.
