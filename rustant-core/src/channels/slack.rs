//! Slack Web API channel implementation.
//!
//! Uses the Slack Web API via reqwest for messaging, channel listing,
//! user lookup, reactions, file metadata, and more.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use crate::oauth::AuthMethod;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Slack channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Bot token (xoxb-...) or OAuth access token.
    pub bot_token: String,
    pub app_token: Option<String>,
    pub default_channel: Option<String>,
    pub allowed_channels: Vec<String>,
    /// Authentication method. When `OAuth`, the `bot_token` field holds
    /// the OAuth 2.0 access token obtained via `slack_oauth_config()`.
    #[serde(default)]
    pub auth_method: AuthMethod,
}

// ── Slack API response types ───────────────────────────────────────────────

/// A Slack message from the API.
#[derive(Debug, Clone)]
pub struct SlackMessage {
    pub ts: String,
    pub channel: String,
    pub user: String,
    pub text: String,
    pub thread_ts: Option<String>,
}

/// Metadata about a Slack channel (public or private).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannelInfo {
    pub id: String,
    pub name: String,
    pub is_private: bool,
    pub is_member: bool,
    pub num_members: u64,
    pub topic: String,
    pub purpose: String,
}

/// Metadata about a Slack workspace user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUserInfo {
    pub id: String,
    pub name: String,
    pub real_name: String,
    pub display_name: String,
    pub is_bot: bool,
    pub is_admin: bool,
    pub email: Option<String>,
    pub status_text: String,
    pub status_emoji: String,
}

/// A reaction on a Slack message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackReaction {
    pub name: String,
    pub count: u64,
    pub users: Vec<String>,
}

/// A file shared in Slack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackFile {
    pub id: String,
    pub name: String,
    pub filetype: String,
    pub size: u64,
    pub url_private: String,
    pub user: String,
    pub timestamp: u64,
}

/// Workspace team information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackTeamInfo {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub icon_url: Option<String>,
}

/// User group information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUserGroup {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub description: String,
    pub user_count: u64,
}

// ── HTTP client trait ──────────────────────────────────────────────────────

/// Trait for Slack API interactions.
#[async_trait]
pub trait SlackHttpClient: Send + Sync {
    // Messaging
    async fn post_message(&self, channel: &str, text: &str) -> Result<String, String>;
    async fn post_thread_reply(
        &self,
        channel: &str,
        thread_ts: &str,
        text: &str,
    ) -> Result<String, String>;
    async fn conversations_history(
        &self,
        channel: &str,
        limit: usize,
    ) -> Result<Vec<SlackMessage>, String>;
    async fn auth_test(&self) -> Result<String, String>;

    // Channels
    async fn conversations_list(
        &self,
        types: &str,
        limit: usize,
    ) -> Result<Vec<SlackChannelInfo>, String>;
    async fn conversations_join(&self, channel_id: &str) -> Result<(), String>;
    async fn conversations_info(&self, channel_id: &str) -> Result<SlackChannelInfo, String>;

    // Users
    async fn users_list(&self, limit: usize) -> Result<Vec<SlackUserInfo>, String>;
    async fn users_info(&self, user_id: &str) -> Result<SlackUserInfo, String>;

    // Reactions
    async fn reactions_add(&self, channel: &str, timestamp: &str, name: &str)
        -> Result<(), String>;
    async fn reactions_get(
        &self,
        channel: &str,
        timestamp: &str,
    ) -> Result<Vec<SlackReaction>, String>;

    // Files
    async fn files_list(
        &self,
        channel: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SlackFile>, String>;

    // Team / Workspace
    async fn team_info(&self) -> Result<SlackTeamInfo, String>;
    async fn usergroups_list(&self) -> Result<Vec<SlackUserGroup>, String>;

    // DMs
    async fn conversations_open(&self, user_ids: &[&str]) -> Result<String, String>;
}

// ── SlackChannel ───────────────────────────────────────────────────────────

/// Slack channel.
pub struct SlackChannel {
    config: SlackConfig,
    status: ChannelStatus,
    http_client: Box<dyn SlackHttpClient>,
    name: String,
}

impl SlackChannel {
    pub fn new(config: SlackConfig, http_client: Box<dyn SlackHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "slack".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    // ── Convenience methods for direct Slack API access ────────────────

    /// List public and private channels visible to the bot.
    pub async fn list_channels(&self) -> Result<Vec<SlackChannelInfo>, RustantError> {
        self.http_client
            .conversations_list("public_channel,private_channel", 200)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// Get info about a specific channel by ID.
    pub async fn channel_info(&self, channel_id: &str) -> Result<SlackChannelInfo, RustantError> {
        self.http_client
            .conversations_info(channel_id)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// Join a channel by ID.
    pub async fn join_channel(&self, channel_id: &str) -> Result<(), RustantError> {
        self.http_client
            .conversations_join(channel_id)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// List workspace users.
    pub async fn list_users(&self) -> Result<Vec<SlackUserInfo>, RustantError> {
        self.http_client.users_list(200).await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })
    }

    /// Look up a user by ID.
    pub async fn get_user_info(&self, user_id: &str) -> Result<SlackUserInfo, RustantError> {
        self.http_client.users_info(user_id).await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })
    }

    /// Add a reaction to a message.
    pub async fn add_reaction(
        &self,
        channel: &str,
        timestamp: &str,
        emoji: &str,
    ) -> Result<(), RustantError> {
        self.http_client
            .reactions_add(channel, timestamp, emoji)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::SendFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// Get reactions on a message.
    pub async fn get_reactions(
        &self,
        channel: &str,
        timestamp: &str,
    ) -> Result<Vec<SlackReaction>, RustantError> {
        self.http_client
            .reactions_get(channel, timestamp)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// List files, optionally filtered by channel.
    pub async fn list_files(&self, channel: Option<&str>) -> Result<Vec<SlackFile>, RustantError> {
        self.http_client
            .files_list(channel, 100)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// Get workspace/team info.
    pub async fn get_team_info(&self) -> Result<SlackTeamInfo, RustantError> {
        self.http_client.team_info().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })
    }

    /// List user groups.
    pub async fn list_usergroups(&self) -> Result<Vec<SlackUserGroup>, RustantError> {
        self.http_client.usergroups_list().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })
    }

    /// Open a DM conversation with one or more users. Returns the channel ID.
    pub async fn open_dm(&self, user_ids: &[&str]) -> Result<String, RustantError> {
        self.http_client
            .conversations_open(user_ids)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// Send a threaded reply to a message.
    pub async fn reply_in_thread(
        &self,
        channel: &str,
        thread_ts: &str,
        text: &str,
    ) -> Result<MessageId, RustantError> {
        self.http_client
            .post_thread_reply(channel, thread_ts, text)
            .await
            .map(MessageId::new)
            .map_err(|e| {
                RustantError::Channel(ChannelError::SendFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    /// Read recent messages from a channel. Wraps `conversations_history`.
    pub async fn read_history(
        &self,
        channel: &str,
        limit: usize,
    ) -> Result<Vec<SlackMessage>, RustantError> {
        self.http_client
            .conversations_history(channel, limit)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Slack
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.bot_token.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        self.http_client.auth_test().await.map_err(|_e| {
            RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            })
        })?;
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), RustantError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: ChannelMessage) -> Result<MessageId, RustantError> {
        let text = msg.content.as_text().unwrap_or("");
        let channel = if msg.channel_id.is_empty() {
            self.config.default_channel.as_deref().unwrap_or("general")
        } else {
            &msg.channel_id
        };

        self.http_client
            .post_message(channel, text)
            .await
            .map(MessageId::new)
            .map_err(|e| {
                RustantError::Channel(ChannelError::SendFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    async fn receive_messages(&self) -> Result<Vec<ChannelMessage>, RustantError> {
        let channels = if self.config.allowed_channels.is_empty() {
            vec![self
                .config
                .default_channel
                .clone()
                .unwrap_or_else(|| "general".to_string())]
        } else {
            self.config.allowed_channels.clone()
        };

        let mut all = Vec::new();
        for ch in &channels {
            let slack_msgs = self
                .http_client
                .conversations_history(ch, 25)
                .await
                .map_err(|e| {
                    RustantError::Channel(ChannelError::ConnectionFailed {
                        name: self.name.clone(),
                        message: e,
                    })
                })?;

            for sm in slack_msgs {
                let sender = ChannelUser::new(&sm.user, ChannelType::Slack);
                let msg = ChannelMessage::text(ChannelType::Slack, &sm.channel, sender, &sm.text);
                all.push(msg);
            }
        }
        Ok(all)
    }

    fn status(&self) -> ChannelStatus {
        self.status
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: true,
            supports_reactions: true,
            supports_files: true,
            supports_voice: false,
            supports_video: false,
            max_message_length: Some(40000),
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::WebSocket
    }
}

// ── Real HTTP client ───────────────────────────────────────────────────────

/// Real Slack HTTP client using the Slack Web API via reqwest.
pub struct RealSlackHttp {
    client: reqwest::Client,
    bot_token: String,
}

impl RealSlackHttp {
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.bot_token)
    }

    /// Make a GET request to a Slack API endpoint and parse the JSON response.
    async fn slack_get(&self, url: &str) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status, body));
        }

        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))?;

        if json.get("ok") != Some(&serde_json::Value::Bool(true)) {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown_error");
            return Err(format!("Slack API error: {}", error));
        }

        Ok(json)
    }

    /// Make a POST request to a Slack API endpoint with a JSON body.
    async fn slack_post(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .post(url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json; charset=utf-8")
            .json(body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status, body_text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&body_text).map_err(|e| format!("Invalid JSON: {}", e))?;

        if json.get("ok") != Some(&serde_json::Value::Bool(true)) {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown_error");
            return Err(format!("Slack API error: {}", error));
        }

        Ok(json)
    }
}

#[async_trait]
impl SlackHttpClient for RealSlackHttp {
    async fn post_message(&self, channel: &str, text: &str) -> Result<String, String> {
        let body = serde_json::json!({ "channel": channel, "text": text });
        let json = self
            .slack_post("https://slack.com/api/chat.postMessage", &body)
            .await?;
        json.get("ts")
            .and_then(|ts| ts.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing 'ts' in response".to_string())
    }

    async fn post_thread_reply(
        &self,
        channel: &str,
        thread_ts: &str,
        text: &str,
    ) -> Result<String, String> {
        let body = serde_json::json!({ "channel": channel, "thread_ts": thread_ts, "text": text });
        let json = self
            .slack_post("https://slack.com/api/chat.postMessage", &body)
            .await?;
        json.get("ts")
            .and_then(|ts| ts.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing 'ts' in response".to_string())
    }

    async fn conversations_history(
        &self,
        channel: &str,
        limit: usize,
    ) -> Result<Vec<SlackMessage>, String> {
        let url = format!(
            "https://slack.com/api/conversations.history?channel={}&limit={}",
            channel, limit
        );
        let json = self.slack_get(&url).await?;

        let messages = json
            .get("messages")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|msg| {
                        let ts = msg.get("ts")?.as_str()?.to_string();
                        let user = msg
                            .get("user")
                            .and_then(|u| u.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let text = msg
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        let thread_ts = msg
                            .get("thread_ts")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string());
                        Some(SlackMessage {
                            ts,
                            channel: channel.to_string(),
                            user,
                            text,
                            thread_ts,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(messages)
    }

    async fn auth_test(&self) -> Result<String, String> {
        let json = self
            .slack_post("https://slack.com/api/auth.test", &serde_json::json!({}))
            .await?;
        json.get("user_id")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing 'user_id' in auth.test response".to_string())
    }

    async fn conversations_list(
        &self,
        types: &str,
        limit: usize,
    ) -> Result<Vec<SlackChannelInfo>, String> {
        let url = format!(
            "https://slack.com/api/conversations.list?types={}&limit={}&exclude_archived=true",
            types, limit
        );
        let json = self.slack_get(&url).await?;

        let channels = json
            .get("channels")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|ch| {
                        Some(SlackChannelInfo {
                            id: ch.get("id")?.as_str()?.to_string(),
                            name: ch.get("name")?.as_str()?.to_string(),
                            is_private: ch
                                .get("is_private")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            is_member: ch
                                .get("is_member")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            num_members: ch
                                .get("num_members")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0),
                            topic: ch
                                .get("topic")
                                .and_then(|t| t.get("value"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            purpose: ch
                                .get("purpose")
                                .and_then(|p| p.get("value"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(channels)
    }

    async fn conversations_join(&self, channel_id: &str) -> Result<(), String> {
        let body = serde_json::json!({ "channel": channel_id });
        self.slack_post("https://slack.com/api/conversations.join", &body)
            .await?;
        Ok(())
    }

    async fn conversations_info(&self, channel_id: &str) -> Result<SlackChannelInfo, String> {
        let url = format!(
            "https://slack.com/api/conversations.info?channel={}",
            channel_id
        );
        let json = self.slack_get(&url).await?;

        let ch = json.get("channel").ok_or("Missing 'channel' in response")?;
        Ok(SlackChannelInfo {
            id: ch
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: ch
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            is_private: ch
                .get("is_private")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            is_member: ch
                .get("is_member")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            num_members: ch.get("num_members").and_then(|v| v.as_u64()).unwrap_or(0),
            topic: ch
                .get("topic")
                .and_then(|t| t.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            purpose: ch
                .get("purpose")
                .and_then(|p| p.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    async fn users_list(&self, limit: usize) -> Result<Vec<SlackUserInfo>, String> {
        let url = format!("https://slack.com/api/users.list?limit={}", limit);
        let json = self.slack_get(&url).await?;

        let users = json
            .get("members")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|u| {
                        let profile = u.get("profile")?;
                        Some(SlackUserInfo {
                            id: u.get("id")?.as_str()?.to_string(),
                            name: u
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            real_name: profile
                                .get("real_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            display_name: profile
                                .get("display_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            is_bot: u.get("is_bot").and_then(|v| v.as_bool()).unwrap_or(false),
                            is_admin: u.get("is_admin").and_then(|v| v.as_bool()).unwrap_or(false),
                            email: profile
                                .get("email")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            status_text: profile
                                .get("status_text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            status_emoji: profile
                                .get("status_emoji")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(users)
    }

    async fn users_info(&self, user_id: &str) -> Result<SlackUserInfo, String> {
        let url = format!("https://slack.com/api/users.info?user={}", user_id);
        let json = self.slack_get(&url).await?;

        let u = json.get("user").ok_or("Missing 'user' in response")?;
        let profile = u.get("profile").ok_or("Missing 'profile'")?;

        Ok(SlackUserInfo {
            id: u
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: u
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            real_name: profile
                .get("real_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            display_name: profile
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            is_bot: u.get("is_bot").and_then(|v| v.as_bool()).unwrap_or(false),
            is_admin: u.get("is_admin").and_then(|v| v.as_bool()).unwrap_or(false),
            email: profile
                .get("email")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            status_text: profile
                .get("status_text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status_emoji: profile
                .get("status_emoji")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    async fn reactions_add(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> Result<(), String> {
        let body = serde_json::json!({ "channel": channel, "timestamp": timestamp, "name": name });
        self.slack_post("https://slack.com/api/reactions.add", &body)
            .await?;
        Ok(())
    }

    async fn reactions_get(
        &self,
        channel: &str,
        timestamp: &str,
    ) -> Result<Vec<SlackReaction>, String> {
        let url = format!(
            "https://slack.com/api/reactions.get?channel={}&timestamp={}&full=true",
            channel, timestamp
        );
        let json = self.slack_get(&url).await?;

        let reactions = json
            .get("message")
            .and_then(|m| m.get("reactions"))
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        Some(SlackReaction {
                            name: r.get("name")?.as_str()?.to_string(),
                            count: r.get("count").and_then(|c| c.as_u64()).unwrap_or(0),
                            users: r
                                .get("users")
                                .and_then(|u| u.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(reactions)
    }

    async fn files_list(
        &self,
        channel: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SlackFile>, String> {
        let mut url = format!("https://slack.com/api/files.list?count={}", limit);
        if let Some(ch) = channel {
            url.push_str(&format!("&channel={}", ch));
        }

        let json = self.slack_get(&url).await?;

        let files = json
            .get("files")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| {
                        Some(SlackFile {
                            id: f.get("id")?.as_str()?.to_string(),
                            name: f
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unnamed")
                                .to_string(),
                            filetype: f
                                .get("filetype")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            size: f.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
                            url_private: f
                                .get("url_private")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            user: f
                                .get("user")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            timestamp: f.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(files)
    }

    async fn team_info(&self) -> Result<SlackTeamInfo, String> {
        let json = self.slack_get("https://slack.com/api/team.info").await?;

        let team = json.get("team").ok_or("Missing 'team' in response")?;
        Ok(SlackTeamInfo {
            id: team
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: team
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            domain: team
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            icon_url: team
                .get("icon")
                .and_then(|i| i.get("image_132"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    async fn usergroups_list(&self) -> Result<Vec<SlackUserGroup>, String> {
        let json = self
            .slack_get("https://slack.com/api/usergroups.list?include_count=true")
            .await?;

        let groups = json
            .get("usergroups")
            .and_then(|g| g.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|g| {
                        Some(SlackUserGroup {
                            id: g.get("id")?.as_str()?.to_string(),
                            name: g
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            handle: g
                                .get("handle")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            description: g
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            user_count: g.get("user_count").and_then(|v| v.as_u64()).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(groups)
    }

    async fn conversations_open(&self, user_ids: &[&str]) -> Result<String, String> {
        let body = serde_json::json!({ "users": user_ids.join(",") });
        let json = self
            .slack_post("https://slack.com/api/conversations.open", &body)
            .await?;

        json.get("channel")
            .and_then(|c| c.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing 'channel.id' in response".to_string())
    }
}

/// Create a SlackChannel with a real HTTP client.
pub fn create_slack_channel(config: SlackConfig) -> SlackChannel {
    let http = RealSlackHttp::new(config.bot_token.clone());
    SlackChannel::new(config, Box::new(http))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelUser;
    use std::sync::{Arc, Mutex};

    struct MockSlackHttp {
        sent: Arc<Mutex<Vec<(String, String)>>>,
        messages: Vec<SlackMessage>,
        auth_ok: bool,
    }

    impl MockSlackHttp {
        fn new() -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                messages: Vec::new(),
                auth_ok: true,
            }
        }

        fn with_messages(mut self, messages: Vec<SlackMessage>) -> Self {
            self.messages = messages;
            self
        }
    }

    #[async_trait]
    impl SlackHttpClient for MockSlackHttp {
        async fn post_message(&self, channel: &str, text: &str) -> Result<String, String> {
            self.sent
                .lock()
                .unwrap()
                .push((channel.to_string(), text.to_string()));
            Ok("1234567890.123456".to_string())
        }

        async fn post_thread_reply(
            &self,
            channel: &str,
            _thread_ts: &str,
            text: &str,
        ) -> Result<String, String> {
            self.sent
                .lock()
                .unwrap()
                .push((channel.to_string(), text.to_string()));
            Ok("1234567890.654321".to_string())
        }

        async fn conversations_history(
            &self,
            _channel: &str,
            _limit: usize,
        ) -> Result<Vec<SlackMessage>, String> {
            Ok(self.messages.clone())
        }

        async fn auth_test(&self) -> Result<String, String> {
            if self.auth_ok {
                Ok("bot-user-id".to_string())
            } else {
                Err("invalid_auth".to_string())
            }
        }

        async fn conversations_list(
            &self,
            _types: &str,
            _limit: usize,
        ) -> Result<Vec<SlackChannelInfo>, String> {
            Ok(vec![
                SlackChannelInfo {
                    id: "C001".into(),
                    name: "general".into(),
                    is_private: false,
                    is_member: true,
                    num_members: 42,
                    topic: "General chat".into(),
                    purpose: "Company-wide".into(),
                },
                SlackChannelInfo {
                    id: "C002".into(),
                    name: "random".into(),
                    is_private: false,
                    is_member: true,
                    num_members: 38,
                    topic: "".into(),
                    purpose: "Random stuff".into(),
                },
            ])
        }

        async fn conversations_join(&self, _channel_id: &str) -> Result<(), String> {
            Ok(())
        }

        async fn conversations_info(&self, channel_id: &str) -> Result<SlackChannelInfo, String> {
            Ok(SlackChannelInfo {
                id: channel_id.to_string(),
                name: "general".into(),
                is_private: false,
                is_member: true,
                num_members: 42,
                topic: "General chat".into(),
                purpose: "Company-wide".into(),
            })
        }

        async fn users_list(&self, _limit: usize) -> Result<Vec<SlackUserInfo>, String> {
            Ok(vec![SlackUserInfo {
                id: "U001".into(),
                name: "alice".into(),
                real_name: "Alice Smith".into(),
                display_name: "alice".into(),
                is_bot: false,
                is_admin: true,
                email: Some("alice@example.com".into()),
                status_text: "Working".into(),
                status_emoji: ":computer:".into(),
            }])
        }

        async fn users_info(&self, user_id: &str) -> Result<SlackUserInfo, String> {
            Ok(SlackUserInfo {
                id: user_id.to_string(),
                name: "alice".into(),
                real_name: "Alice Smith".into(),
                display_name: "alice".into(),
                is_bot: false,
                is_admin: true,
                email: Some("alice@example.com".into()),
                status_text: "Working".into(),
                status_emoji: ":computer:".into(),
            })
        }

        async fn reactions_add(
            &self,
            _channel: &str,
            _timestamp: &str,
            _name: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn reactions_get(
            &self,
            _channel: &str,
            _timestamp: &str,
        ) -> Result<Vec<SlackReaction>, String> {
            Ok(vec![SlackReaction {
                name: "thumbsup".into(),
                count: 3,
                users: vec!["U001".into(), "U002".into(), "U003".into()],
            }])
        }

        async fn files_list(
            &self,
            _channel: Option<&str>,
            _limit: usize,
        ) -> Result<Vec<SlackFile>, String> {
            Ok(vec![SlackFile {
                id: "F001".into(),
                name: "report.pdf".into(),
                filetype: "pdf".into(),
                size: 1024,
                url_private: "https://files.slack.com/report.pdf".into(),
                user: "U001".into(),
                timestamp: 1700000000,
            }])
        }

        async fn team_info(&self) -> Result<SlackTeamInfo, String> {
            Ok(SlackTeamInfo {
                id: "T001".into(),
                name: "Test Workspace".into(),
                domain: "test-workspace".into(),
                icon_url: None,
            })
        }

        async fn usergroups_list(&self) -> Result<Vec<SlackUserGroup>, String> {
            Ok(vec![SlackUserGroup {
                id: "S001".into(),
                name: "Engineering".into(),
                handle: "engineering".into(),
                description: "Engineering team".into(),
                user_count: 15,
            }])
        }

        async fn conversations_open(&self, _user_ids: &[&str]) -> Result<String, String> {
            Ok("D001".to_string())
        }
    }

    // ── Existing tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_slack_connect() {
        let config = SlackConfig {
            bot_token: "xoxb-123".into(),
            ..Default::default()
        };
        let mut ch = SlackChannel::new(config, Box::new(MockSlackHttp::new()));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[tokio::test]
    async fn test_slack_send_message() {
        let config = SlackConfig {
            bot_token: "xoxb-123".into(),
            default_channel: Some("general".into()),
            ..Default::default()
        };
        let http = MockSlackHttp::new();
        let sent = http.sent.clone();
        let mut ch = SlackChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let sender = ChannelUser::new("bot", ChannelType::Slack);
        let msg = ChannelMessage::text(ChannelType::Slack, "random", sender, "Hello Slack!");
        ch.send_message(msg).await.unwrap();

        let sent = sent.lock().unwrap();
        assert_eq!(sent[0].0, "random");
        assert_eq!(sent[0].1, "Hello Slack!");
    }

    #[tokio::test]
    async fn test_slack_receive_messages() {
        let config = SlackConfig {
            bot_token: "xoxb-123".into(),
            allowed_channels: vec!["general".into()],
            ..Default::default()
        };
        let http = MockSlackHttp::new().with_messages(vec![SlackMessage {
            ts: "123.456".into(),
            channel: "general".into(),
            user: "U123".into(),
            text: "hey".into(),
            thread_ts: None,
        }]);
        let mut ch = SlackChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hey"));
    }

    #[test]
    fn test_slack_capabilities() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let caps = ch.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_reactions);
        assert!(caps.supports_files);
        assert!(!caps.supports_voice);
        assert_eq!(caps.max_message_length, Some(40000));
    }

    #[test]
    fn test_slack_streaming_mode() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        assert_eq!(ch.streaming_mode(), StreamingMode::WebSocket);
    }

    #[tokio::test]
    async fn test_slack_oauth_config_connect() {
        let config = SlackConfig {
            bot_token: "xoxb-oauth-token-from-oauth-flow".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let mut ch = SlackChannel::new(config, Box::new(MockSlackHttp::new()));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[test]
    fn test_slack_config_auth_method_default() {
        let config = SlackConfig::default();
        assert_eq!(config.auth_method, AuthMethod::ApiKey);
    }

    #[test]
    fn test_slack_config_auth_method_serde() {
        let config = SlackConfig {
            bot_token: "xoxb-test".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"oauth\""));
        let parsed: SlackConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.auth_method, AuthMethod::OAuth);
    }

    // ── New feature tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_slack_list_channels() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let channels = ch.list_channels().await.unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].name, "general");
        assert_eq!(channels[1].name, "random");
        assert_eq!(channels[0].num_members, 42);
    }

    #[tokio::test]
    async fn test_slack_channel_info() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let info = ch.channel_info("C001").await.unwrap();
        assert_eq!(info.id, "C001");
        assert_eq!(info.name, "general");
    }

    #[tokio::test]
    async fn test_slack_join_channel() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        ch.join_channel("C002").await.unwrap();
    }

    #[tokio::test]
    async fn test_slack_list_users() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let users = ch.list_users().await.unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "alice");
        assert_eq!(users[0].real_name, "Alice Smith");
        assert!(users[0].is_admin);
        assert!(!users[0].is_bot);
    }

    #[tokio::test]
    async fn test_slack_get_user_info() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let user = ch.get_user_info("U001").await.unwrap();
        assert_eq!(user.id, "U001");
        assert_eq!(user.email, Some("alice@example.com".into()));
    }

    #[tokio::test]
    async fn test_slack_add_reaction() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        ch.add_reaction("C001", "123.456", "thumbsup")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_slack_get_reactions() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let reactions = ch.get_reactions("C001", "123.456").await.unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].name, "thumbsup");
        assert_eq!(reactions[0].count, 3);
        assert_eq!(reactions[0].users.len(), 3);
    }

    #[tokio::test]
    async fn test_slack_list_files() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let files = ch.list_files(None).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "report.pdf");
        assert_eq!(files[0].size, 1024);
    }

    #[tokio::test]
    async fn test_slack_team_info() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let team = ch.get_team_info().await.unwrap();
        assert_eq!(team.name, "Test Workspace");
        assert_eq!(team.domain, "test-workspace");
    }

    #[tokio::test]
    async fn test_slack_list_usergroups() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let groups = ch.list_usergroups().await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].handle, "engineering");
        assert_eq!(groups[0].user_count, 15);
    }

    #[tokio::test]
    async fn test_slack_open_dm() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let dm_channel = ch.open_dm(&["U001"]).await.unwrap();
        assert_eq!(dm_channel, "D001");
    }

    #[tokio::test]
    async fn test_slack_thread_reply() {
        let http = MockSlackHttp::new();
        let sent = http.sent.clone();
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(http));
        let id = ch
            .reply_in_thread("C001", "123.456", "reply text")
            .await
            .unwrap();
        assert!(!id.0.is_empty());
        let sent = sent.lock().unwrap();
        assert_eq!(sent[0].1, "reply text");
    }

    #[tokio::test]
    async fn test_slack_read_history() {
        let http = MockSlackHttp::new().with_messages(vec![SlackMessage {
            ts: "999.888".into(),
            channel: "test".into(),
            user: "U001".into(),
            text: "history msg".into(),
            thread_ts: None,
        }]);
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(http));
        let msgs = ch.read_history("test", 10).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text, "history msg");
    }
}
