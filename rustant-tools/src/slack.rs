//! Slack tool — send messages, read channels, list users via Slack Web API.
//!
//! Uses the Slack Bot Token API (same as `RealSlackHttp` in rustant-core channels)
//! to provide direct Slack interaction from the agent. Cross-platform (not macOS only).
//! Bot token resolved from `SLACK_BOT_TOKEN` env var or `channels.slack.bot_token` in config.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::debug;

/// Slack tool providing 6 actions for Slack workspace interaction.
pub struct SlackTool {
    workspace: PathBuf,
}

impl SlackTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for SlackTool {
    fn name(&self) -> &str {
        "slack"
    }

    fn description(&self) -> &str {
        "Interact with Slack workspaces via Bot Token API. Actions: \
         send_message (post to a channel or DM), read_messages (fetch recent messages), \
         list_channels (list workspace channels), reply_thread (reply in a thread), \
         list_users (list workspace members), add_reaction (react to a message). \
         Requires SLACK_BOT_TOKEN env var or channels.slack.bot_token in config."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The action to perform",
                    "enum": ["send_message", "read_messages", "list_channels", "reply_thread", "list_users", "add_reaction"]
                },
                "channel": {
                    "type": "string",
                    "description": "Channel name (e.g. '#general') or ID (e.g. 'C01234'). Required for send_message, read_messages, reply_thread, add_reaction."
                },
                "message": {
                    "type": "string",
                    "description": "Message text to send. Required for send_message and reply_thread."
                },
                "thread_ts": {
                    "type": "string",
                    "description": "Thread timestamp to reply to. Required for reply_thread and add_reaction."
                },
                "emoji": {
                    "type": "string",
                    "description": "Emoji name without colons (e.g. 'thumbsup'). Required for add_reaction."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max messages to return for read_messages (default: 10, max: 100).",
                    "default": 10
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "slack".to_string(),
                reason: "missing required 'action' parameter".to_string(),
            })?;

        let token = get_bot_token(&self.workspace)?;
        let client = reqwest::Client::new();

        match action {
            "send_message" => {
                let channel = args["channel"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "send_message requires 'channel' parameter".to_string(),
                    })?;
                let message = args["message"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "send_message requires 'message' parameter".to_string(),
                    })?;

                debug!(channel = channel, "Sending Slack message");

                let body = json!({
                    "channel": channel,
                    "text": message,
                });
                let resp = slack_api_post(
                    &client,
                    &token,
                    "https://slack.com/api/chat.postMessage",
                    &body,
                )
                .await?;

                let ts = resp["ts"].as_str().unwrap_or("unknown");
                let ch = resp["channel"].as_str().unwrap_or(channel);
                Ok(ToolOutput::text(format!(
                    "Message sent to {} (ts: {})",
                    ch, ts
                )))
            }

            "read_messages" => {
                let channel = args["channel"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "read_messages requires 'channel' parameter".to_string(),
                    })?;
                let limit = args["limit"].as_u64().unwrap_or(10).min(100);

                debug!(channel = channel, limit = limit, "Reading Slack messages");

                let url = format!(
                    "https://slack.com/api/conversations.history?channel={}&limit={}",
                    urlencoding::encode(channel),
                    limit
                );
                let resp = slack_api_get(&client, &token, &url).await?;

                let messages = resp["messages"].as_array();
                match messages {
                    Some(msgs) if !msgs.is_empty() => {
                        let mut output = format!(
                            "Recent messages in {} ({} message(s)):\n\n",
                            channel,
                            msgs.len()
                        );
                        for msg in msgs {
                            let user = msg["user"].as_str().unwrap_or("unknown");
                            let text = msg["text"].as_str().unwrap_or("");
                            let ts = msg["ts"].as_str().unwrap_or("");
                            output.push_str(&format!("[{}] {}: {}\n", ts, user, text));
                        }
                        Ok(ToolOutput::text(output))
                    }
                    _ => Ok(ToolOutput::text(format!(
                        "No recent messages in {}.",
                        channel
                    ))),
                }
            }

            "list_channels" => {
                debug!("Listing Slack channels");

                let url = "https://slack.com/api/conversations.list?types=public_channel,private_channel&limit=200";
                let resp = slack_api_get(&client, &token, url).await?;

                let channels = resp["channels"].as_array();
                match channels {
                    Some(chs) if !chs.is_empty() => {
                        let mut output =
                            format!("Slack channels ({} found):\n\n", chs.len());
                        for ch in chs {
                            let name = ch["name"].as_str().unwrap_or("?");
                            let id = ch["id"].as_str().unwrap_or("?");
                            let members = ch["num_members"].as_u64().unwrap_or(0);
                            let purpose = ch["purpose"]["value"]
                                .as_str()
                                .unwrap_or("");
                            let private = if ch["is_private"].as_bool().unwrap_or(false) {
                                " (private)"
                            } else {
                                ""
                            };
                            output.push_str(&format!(
                                "#{}{} [{}] — {} members",
                                name, private, id, members
                            ));
                            if !purpose.is_empty() {
                                output.push_str(&format!(" — {}", purpose));
                            }
                            output.push('\n');
                        }
                        Ok(ToolOutput::text(output))
                    }
                    _ => Ok(ToolOutput::text("No channels found.".to_string())),
                }
            }

            "reply_thread" => {
                let channel = args["channel"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "reply_thread requires 'channel' parameter".to_string(),
                    })?;
                let thread_ts = args["thread_ts"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "reply_thread requires 'thread_ts' parameter".to_string(),
                    })?;
                let message = args["message"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "reply_thread requires 'message' parameter".to_string(),
                    })?;

                debug!(
                    channel = channel,
                    thread_ts = thread_ts,
                    "Replying to Slack thread"
                );

                let body = json!({
                    "channel": channel,
                    "text": message,
                    "thread_ts": thread_ts,
                });
                let resp = slack_api_post(
                    &client,
                    &token,
                    "https://slack.com/api/chat.postMessage",
                    &body,
                )
                .await?;

                let ts = resp["ts"].as_str().unwrap_or("unknown");
                Ok(ToolOutput::text(format!(
                    "Thread reply sent to {} (ts: {})",
                    channel, ts
                )))
            }

            "list_users" => {
                debug!("Listing Slack users");

                let url = "https://slack.com/api/users.list?limit=200";
                let resp = slack_api_get(&client, &token, url).await?;

                let members = resp["members"].as_array();
                match members {
                    Some(users) if !users.is_empty() => {
                        let mut output =
                            format!("Slack workspace members ({} found):\n\n", users.len());
                        for user in users {
                            let name = user["name"].as_str().unwrap_or("?");
                            let real_name = user["real_name"].as_str().unwrap_or("");
                            let id = user["id"].as_str().unwrap_or("?");
                            let is_bot = user["is_bot"].as_bool().unwrap_or(false);
                            let bot_tag = if is_bot { " [bot]" } else { "" };
                            output.push_str(&format!(
                                "@{}{} ({}) [{}]\n",
                                name, bot_tag, real_name, id
                            ));
                        }
                        Ok(ToolOutput::text(output))
                    }
                    _ => Ok(ToolOutput::text("No users found.".to_string())),
                }
            }

            "add_reaction" => {
                let channel = args["channel"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "add_reaction requires 'channel' parameter".to_string(),
                    })?;
                let timestamp = args["thread_ts"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "add_reaction requires 'thread_ts' parameter (message timestamp)"
                            .to_string(),
                    })?;
                let emoji = args["emoji"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "slack".to_string(),
                        reason: "add_reaction requires 'emoji' parameter".to_string(),
                    })?;

                debug!(
                    channel = channel,
                    timestamp = timestamp,
                    emoji = emoji,
                    "Adding Slack reaction"
                );

                let body = json!({
                    "channel": channel,
                    "timestamp": timestamp,
                    "name": emoji,
                });
                slack_api_post(
                    &client,
                    &token,
                    "https://slack.com/api/reactions.add",
                    &body,
                )
                .await?;

                Ok(ToolOutput::text(format!(
                    "Reaction :{}:  added to message in {}.",
                    emoji, channel
                )))
            }

            other => Err(ToolError::InvalidArguments {
                name: "slack".to_string(),
                reason: format!(
                    "unknown action '{}'. Valid: send_message, read_messages, list_channels, reply_thread, list_users, add_reaction",
                    other
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        // The tool contains both read and write actions; the safety guardian
        // will gate writes via parse_action_details in agent.rs.
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Resolve the Slack bot token.
/// 1. `SLACK_BOT_TOKEN` env var (fast path)
/// 2. Config file `channels.slack.bot_token`
fn get_bot_token(workspace: &Path) -> Result<String, ToolError> {
    // Fast path: env var
    if let Ok(token) = std::env::var("SLACK_BOT_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Fallback: load from config
    if let Ok(config) = rustant_core::config::load_config(Some(workspace), None) {
        if let Some(channels) = &config.channels {
            if let Some(slack) = &channels.slack {
                if !slack.bot_token.is_empty() {
                    match slack.resolve_bot_token() {
                        Ok(token) if !token.is_empty() => return Ok(token),
                        Ok(_) => {}
                        Err(e) => tracing::warn!("Failed to resolve Slack bot token: {}", e),
                    }
                }
            }
        }
    }

    Err(ToolError::ExecutionFailed {
        name: "slack".to_string(),
        message: "No Slack bot token found. Set SLACK_BOT_TOKEN env var or configure \
                  channels.slack.bot_token in .rustant/config.toml"
            .to_string(),
    })
}

/// Make a GET request to the Slack API with Bearer auth.
async fn slack_api_get(
    client: &reqwest::Client,
    token: &str,
    url: &str,
) -> Result<serde_json::Value, ToolError> {
    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: "slack".to_string(),
            message: format!("HTTP request failed: {}", e),
        })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|e| ToolError::ExecutionFailed {
        name: "slack".to_string(),
        message: format!("Failed to parse response: {}", e),
    })?;

    if !status.is_success() {
        return Err(ToolError::ExecutionFailed {
            name: "slack".to_string(),
            message: format!("Slack API returned HTTP {}: {:?}", status, body),
        });
    }

    if body["ok"].as_bool() != Some(true) {
        let error = body["error"].as_str().unwrap_or("unknown_error");
        return Err(ToolError::ExecutionFailed {
            name: "slack".to_string(),
            message: format!("Slack API error: {}", error),
        });
    }

    Ok(body)
}

/// Make a POST request to the Slack API with Bearer auth and JSON body.
async fn slack_api_post(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ToolError> {
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json; charset=utf-8")
        .timeout(Duration::from_secs(15))
        .json(body)
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: "slack".to_string(),
            message: format!("HTTP request failed: {}", e),
        })?;

    let status = resp.status();
    let resp_body: serde_json::Value =
        resp.json().await.map_err(|e| ToolError::ExecutionFailed {
            name: "slack".to_string(),
            message: format!("Failed to parse response: {}", e),
        })?;

    if !status.is_success() {
        return Err(ToolError::ExecutionFailed {
            name: "slack".to_string(),
            message: format!("Slack API returned HTTP {}: {:?}", status, resp_body),
        });
    }

    if resp_body["ok"].as_bool() != Some(true) {
        let error = resp_body["error"].as_str().unwrap_or("unknown_error");
        return Err(ToolError::ExecutionFailed {
            name: "slack".to_string(),
            message: format!("Slack API error: {}", error),
        });
    }

    Ok(resp_body)
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mutex to serialize tests that modify the SLACK_BOT_TOKEN env var.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_slack_tool_definition() {
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.name(), "slack");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["channel"].is_object());
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["thread_ts"].is_object());
        assert!(schema["properties"]["emoji"].is_object());
        assert!(schema["properties"]["limit"].is_object());
    }

    #[test]
    fn test_slack_tool_schema_required_fields() {
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
        assert_eq!(required.len(), 1);
    }

    #[test]
    fn test_slack_tool_timeout() {
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.timeout(), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_slack_missing_action() {
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "slack");
                assert!(reason.contains("action"));
            }
            _ => panic!("Expected InvalidArguments error"),
        }
    }

    #[tokio::test]
    async fn test_slack_send_message_missing_channel() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SLACK_BOT_TOKEN", "xoxb-test-token");
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"action": "send_message", "message": "hello"}))
            .await;
        assert!(result.is_err());
        std::env::remove_var("SLACK_BOT_TOKEN");
    }

    #[tokio::test]
    async fn test_slack_send_message_missing_message() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SLACK_BOT_TOKEN", "xoxb-test-token");
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"action": "send_message", "channel": "#general"}))
            .await;
        assert!(result.is_err());
        std::env::remove_var("SLACK_BOT_TOKEN");
    }

    #[tokio::test]
    async fn test_slack_unknown_action() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SLACK_BOT_TOKEN", "xoxb-test-token");
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        let result = tool.execute(json!({"action": "invalid_action"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "slack");
                assert!(reason.contains("unknown action"));
            }
            _ => panic!("Expected InvalidArguments error"),
        }
        std::env::remove_var("SLACK_BOT_TOKEN");
    }

    #[test]
    fn test_bot_token_env_resolution() {
        let _guard = ENV_MUTEX.lock().unwrap();
        // Test 1: valid env var is used
        std::env::set_var("SLACK_BOT_TOKEN", "xoxb-from-env");
        let result = get_bot_token(Path::new("/tmp"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "xoxb-from-env");

        // Test 2: empty env var is rejected (falls through to config)
        std::env::set_var("SLACK_BOT_TOKEN", "");
        let result = get_bot_token(Path::new("/tmp"));
        // Result depends on config; just verify empty string is not returned.
        if let Ok(ref token) = result {
            assert!(!token.is_empty());
        }

        std::env::remove_var("SLACK_BOT_TOKEN");
    }

    #[tokio::test]
    async fn test_slack_reply_thread_missing_thread_ts() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SLACK_BOT_TOKEN", "xoxb-test-token");
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"action": "reply_thread", "channel": "#general", "message": "hi"}))
            .await;
        assert!(result.is_err());
        std::env::remove_var("SLACK_BOT_TOKEN");
    }

    #[tokio::test]
    async fn test_slack_add_reaction_missing_emoji() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SLACK_BOT_TOKEN", "xoxb-test-token");
        let tool = SlackTool::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(
                json!({"action": "add_reaction", "channel": "#general", "thread_ts": "123.456"}),
            )
            .await;
        assert!(result.is_err());
        std::env::remove_var("SLACK_BOT_TOKEN");
    }
}
