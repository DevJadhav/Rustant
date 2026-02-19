# Adding Channels

This guide walks through adding a new messaging channel integration.

## Step 1: Create the Module

Create a new file in `rustant-core/src/channels/`:

```rust
use super::{Channel, ChannelMessage, ChannelError};
use async_trait::async_trait;

pub struct MyChannel {
    config: MyChannelConfig,
}

pub struct MyChannelConfig {
    pub api_token: String,
    pub enabled: bool,
}

#[async_trait]
impl Channel for MyChannel {
    fn name(&self) -> &str {
        "mychannel"
    }

    async fn send(&self, target: &str, message: &str) -> Result<String, ChannelError> {
        // Send message and return message ID
        Ok("msg-id".to_string())
    }

    async fn receive(&self, since: Option<&str>) -> Result<Vec<ChannelMessage>, ChannelError> {
        // Fetch new messages since cursor
        Ok(vec![])
    }

    async fn test_connection(&self) -> Result<(), ChannelError> {
        // Verify credentials and connectivity
        Ok(())
    }
}
```

## Step 2: Register the Channel

In `rustant-core/src/channels/mod.rs`, add to `build_channel_manager()`:

```rust
if let Some(config) = &agent_config.channels.mychannel {
    if config.enabled {
        manager.register(Box::new(MyChannel::new(config)));
    }
}
```

## Step 3: Add Configuration

In `rustant-core/src/config.rs`, add the channel config struct and field:

```rust
pub struct MyChannelConfig {
    pub enabled: bool,
    pub api_token_ref: Option<String>,  // SecretRef format
}
```

## Step 4: CDC Support (optional)

For channels that support polling, implement cursor-based tracking in the `receive()` method. Return the latest cursor position for `CdcState` persistence.

## Step 5: Add Tests

Write integration tests verifying send, receive, and test_connection work correctly with mock credentials.

## Configuration

Users configure the channel in `.rustant/config.toml`:

```toml
[channels.mychannel]
enabled = true
api_token_ref = "keychain:channel:mychannel:token"
```
