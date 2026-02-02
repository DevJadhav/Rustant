//! Gateway event types and message protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Events emitted by the gateway to connected clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GatewayEvent {
    /// A client has connected.
    Connected { connection_id: Uuid },
    /// A client has disconnected.
    Disconnected { connection_id: Uuid },
    /// A new task was submitted.
    TaskSubmitted { task_id: Uuid, description: String },
    /// Progress update on a running task.
    TaskProgress {
        task_id: Uuid,
        progress: f32,
        message: String,
    },
    /// A task has completed.
    TaskCompleted {
        task_id: Uuid,
        success: bool,
        summary: String,
    },
    /// An assistant message (full).
    AssistantMessage { content: String },
    /// A single token from a streaming response.
    StreamToken { token: String },
    /// A tool is being executed.
    ToolExecution {
        tool_name: String,
        status: ToolStatus,
    },
    /// An error occurred.
    Error { code: String, message: String },
    /// A channel message was received.
    ChannelMessageReceived {
        channel_type: String,
        message: String,
    },
    /// A task was dispatched to a node.
    NodeTaskDispatched { node_id: String, task_name: String },
    /// An agent was spawned.
    AgentSpawned { agent_id: String, name: String },
    /// An agent was terminated.
    AgentTerminated { agent_id: String },
    /// Metrics update for dashboard monitoring.
    MetricsUpdate {
        active_connections: usize,
        active_sessions: usize,
        total_tool_calls: u64,
        total_llm_requests: u64,
        uptime_secs: u64,
    },
    /// An approval request awaiting user decision.
    ApprovalRequest {
        approval_id: Uuid,
        tool_name: String,
        description: String,
        risk_level: String,
    },
    /// A config snapshot was requested or changed.
    ConfigSnapshot { config_json: String },
}

/// Status of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolStatus {
    Started,
    Running,
    Completed,
    Failed,
}

/// Messages sent from clients to the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Authenticate with a token.
    Authenticate { token: String },
    /// Submit a new task to the agent.
    SubmitTask { description: String },
    /// Cancel a running task.
    CancelTask { task_id: Uuid },
    /// Request the current status.
    GetStatus,
    /// Keep-alive ping.
    Ping { timestamp: DateTime<Utc> },
    /// List connected channels.
    ListChannels,
    /// List registered nodes.
    ListNodes,
    /// Request current metrics for the dashboard.
    GetMetrics,
    /// Request current configuration snapshot.
    GetConfig,
    /// Submit an approval decision.
    ApprovalDecision {
        approval_id: Uuid,
        approved: bool,
        reason: Option<String>,
    },
}

/// Messages sent from the gateway to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Authentication succeeded.
    Authenticated { connection_id: Uuid },
    /// Authentication failed.
    AuthFailed { reason: String },
    /// A gateway event.
    Event { event: GatewayEvent },
    /// Response to a GetStatus request.
    StatusResponse {
        connected_clients: usize,
        active_tasks: usize,
        uptime_secs: u64,
    },
    /// Pong response to a Ping.
    Pong { timestamp: DateTime<Utc> },
    /// Channel status listing.
    ChannelStatus { channels: Vec<(String, String)> },
    /// Node status listing.
    NodeStatus { nodes: Vec<(String, String)> },
    /// Metrics snapshot for dashboard.
    MetricsResponse {
        active_connections: usize,
        active_sessions: usize,
        total_tool_calls: u64,
        total_llm_requests: u64,
        uptime_secs: u64,
    },
    /// Configuration snapshot.
    ConfigResponse { config_json: String },
    /// Approval decision acknowledgment.
    ApprovalAck { approval_id: Uuid, accepted: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_event_serialization() {
        let event = GatewayEvent::Connected {
            connection_id: Uuid::new_v4(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Connected"));

        let restored: GatewayEvent = serde_json::from_str(&json).unwrap();
        match restored {
            GatewayEvent::Connected { connection_id } => {
                assert!(!connection_id.is_nil());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_client_message_serialization() {
        let msg = ClientMessage::Authenticate {
            token: "secret".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ClientMessage = serde_json::from_str(&json).unwrap();
        match restored {
            ClientMessage::Authenticate { token } => assert_eq!(token, "secret"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_server_message_serialization() {
        let msg = ServerMessage::StatusResponse {
            connected_clients: 3,
            active_tasks: 1,
            uptime_secs: 3600,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ServerMessage = serde_json::from_str(&json).unwrap();
        match restored {
            ServerMessage::StatusResponse {
                connected_clients,
                active_tasks,
                uptime_secs,
            } => {
                assert_eq!(connected_clients, 3);
                assert_eq!(active_tasks, 1);
                assert_eq!(uptime_secs, 3600);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_all_event_variants_serialize() {
        let events: Vec<GatewayEvent> = vec![
            GatewayEvent::Connected {
                connection_id: Uuid::new_v4(),
            },
            GatewayEvent::Disconnected {
                connection_id: Uuid::new_v4(),
            },
            GatewayEvent::TaskSubmitted {
                task_id: Uuid::new_v4(),
                description: "test".into(),
            },
            GatewayEvent::TaskProgress {
                task_id: Uuid::new_v4(),
                progress: 0.5,
                message: "halfway".into(),
            },
            GatewayEvent::TaskCompleted {
                task_id: Uuid::new_v4(),
                success: true,
                summary: "done".into(),
            },
            GatewayEvent::AssistantMessage {
                content: "hello".into(),
            },
            GatewayEvent::StreamToken {
                token: "tok".into(),
            },
            GatewayEvent::ToolExecution {
                tool_name: "read_file".into(),
                status: ToolStatus::Started,
            },
            GatewayEvent::Error {
                code: "E001".into(),
                message: "bad".into(),
            },
            GatewayEvent::ChannelMessageReceived {
                channel_type: "telegram".into(),
                message: "hello".into(),
            },
            GatewayEvent::NodeTaskDispatched {
                node_id: "n1".into(),
                task_name: "shell".into(),
            },
            GatewayEvent::AgentSpawned {
                agent_id: "a1".into(),
                name: "helper".into(),
            },
            GatewayEvent::AgentTerminated {
                agent_id: "a1".into(),
            },
            GatewayEvent::MetricsUpdate {
                active_connections: 5,
                active_sessions: 2,
                total_tool_calls: 100,
                total_llm_requests: 50,
                uptime_secs: 3600,
            },
            GatewayEvent::ApprovalRequest {
                approval_id: Uuid::new_v4(),
                tool_name: "shell_exec".into(),
                description: "Run rm -rf".into(),
                risk_level: "high".into(),
            },
            GatewayEvent::ConfigSnapshot {
                config_json: "{}".into(),
            },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let _: GatewayEvent = serde_json::from_str(&json).unwrap();
        }
        assert_eq!(events.len(), 16);
    }

    #[test]
    fn test_tool_status_serialization() {
        let statuses = vec![
            ToolStatus::Started,
            ToolStatus::Running,
            ToolStatus::Completed,
            ToolStatus::Failed,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let _: ToolStatus = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_ping_pong_messages() {
        let now = Utc::now();
        let ping = ClientMessage::Ping { timestamp: now };
        let json = serde_json::to_string(&ping).unwrap();
        let restored: ClientMessage = serde_json::from_str(&json).unwrap();
        match restored {
            ClientMessage::Ping { timestamp } => {
                assert_eq!(timestamp, now);
            }
            _ => panic!("Wrong variant"),
        }

        let pong = ServerMessage::Pong { timestamp: now };
        let json = serde_json::to_string(&pong).unwrap();
        let _: ServerMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_gateway_event_channel_message_received() {
        let event = GatewayEvent::ChannelMessageReceived {
            channel_type: "slack".into(),
            message: "hello world".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("slack"));
        let restored: GatewayEvent = serde_json::from_str(&json).unwrap();
        match restored {
            GatewayEvent::ChannelMessageReceived {
                channel_type,
                message,
            } => {
                assert_eq!(channel_type, "slack");
                assert_eq!(message, "hello world");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_gateway_event_node_task_dispatched() {
        let event = GatewayEvent::NodeTaskDispatched {
            node_id: "node-1".into(),
            task_name: "shell".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: GatewayEvent = serde_json::from_str(&json).unwrap();
        match restored {
            GatewayEvent::NodeTaskDispatched { node_id, task_name } => {
                assert_eq!(node_id, "node-1");
                assert_eq!(task_name, "shell");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_gateway_event_agent_spawned_terminated() {
        let spawned = GatewayEvent::AgentSpawned {
            agent_id: "a1".into(),
            name: "helper".into(),
        };
        let terminated = GatewayEvent::AgentTerminated {
            agent_id: "a1".into(),
        };
        let json1 = serde_json::to_string(&spawned).unwrap();
        let json2 = serde_json::to_string(&terminated).unwrap();
        let _: GatewayEvent = serde_json::from_str(&json1).unwrap();
        let _: GatewayEvent = serde_json::from_str(&json2).unwrap();
    }

    #[test]
    fn test_server_message_channel_status() {
        let msg = ServerMessage::ChannelStatus {
            channels: vec![
                ("telegram".into(), "connected".into()),
                ("slack".into(), "disconnected".into()),
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ServerMessage = serde_json::from_str(&json).unwrap();
        match restored {
            ServerMessage::ChannelStatus { channels } => {
                assert_eq!(channels.len(), 2);
                assert_eq!(channels[0].0, "telegram");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_server_message_node_status() {
        let msg = ServerMessage::NodeStatus {
            nodes: vec![("macos-local".into(), "healthy".into())],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ServerMessage = serde_json::from_str(&json).unwrap();
        match restored {
            ServerMessage::NodeStatus { nodes } => {
                assert_eq!(nodes.len(), 1);
                assert_eq!(nodes[0].1, "healthy");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_client_message_list_channels_nodes() {
        let lc = ClientMessage::ListChannels;
        let ln = ClientMessage::ListNodes;
        let json1 = serde_json::to_string(&lc).unwrap();
        let json2 = serde_json::to_string(&ln).unwrap();
        let _: ClientMessage = serde_json::from_str(&json1).unwrap();
        let _: ClientMessage = serde_json::from_str(&json2).unwrap();
    }
}
