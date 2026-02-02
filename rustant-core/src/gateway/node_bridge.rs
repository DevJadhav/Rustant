//! Gateway ↔ Nodes bridge — translates between gateway task events and node tasks.

use crate::gateway::events::ServerMessage;
use crate::nodes::types::{Capability, NodeResult, NodeTask};
use std::collections::HashMap;

/// Bridge connecting Gateway task events to the Node system.
pub struct NodeBridge;

impl NodeBridge {
    pub fn new() -> Self {
        Self
    }

    /// Route a task name and args from the gateway into a NodeTask, if applicable.
    pub fn route_task_to_node(
        &self,
        task_name: &str,
        args: &HashMap<String, String>,
    ) -> Option<NodeTask> {
        let capability = match task_name {
            "shell" | "execute" | "run" => Some(Capability::Shell),
            "applescript" | "osascript" => Some(Capability::AppleScript),
            "screenshot" | "capture" => Some(Capability::Screenshot),
            "clipboard" | "paste" => Some(Capability::Clipboard),
            "filesystem" | "file" | "read_file" | "write_file" => Some(Capability::FileSystem),
            "notify" | "notification" => Some(Capability::Notifications),
            "browser" | "open_url" => Some(Capability::Browser),
            _ => None,
        };

        capability.map(|cap| {
            let command = args
                .get("command")
                .cloned()
                .unwrap_or_else(|| task_name.to_string());
            let mut task = NodeTask::new(cap, command);
            if let Some(timeout) = args.get("timeout") {
                if let Ok(secs) = timeout.parse() {
                    task = task.with_timeout(secs);
                }
            }
            if let Some(task_args) = args.get("args") {
                task = task.with_args(task_args.split_whitespace().map(String::from).collect());
            }
            task
        })
    }

    /// Convert a NodeResult into a ServerMessage.
    pub fn node_result_to_server_message(result: &NodeResult) -> ServerMessage {
        if result.success {
            ServerMessage::Event {
                event: crate::gateway::events::GatewayEvent::TaskCompleted {
                    task_id: result.task_id,
                    success: true,
                    summary: result.output.clone(),
                },
            }
        } else {
            ServerMessage::Event {
                event: crate::gateway::events::GatewayEvent::TaskCompleted {
                    task_id: result.task_id,
                    success: false,
                    summary: result.output.clone(),
                },
            }
        }
    }
}

impl Default for NodeBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_node_bridge_route_shell_task() {
        let bridge = NodeBridge::new();
        let args = HashMap::from([("command".into(), "ls -la".into())]);
        let task = bridge.route_task_to_node("shell", &args).unwrap();
        assert_eq!(task.capability, Capability::Shell);
        assert_eq!(task.command, "ls -la");
    }

    #[test]
    fn test_node_bridge_route_applescript_task() {
        let bridge = NodeBridge::new();
        let args = HashMap::from([("command".into(), "tell app \"Finder\" to activate".into())]);
        let task = bridge.route_task_to_node("applescript", &args).unwrap();
        assert_eq!(task.capability, Capability::AppleScript);
    }

    #[test]
    fn test_node_bridge_unknown_task_returns_none() {
        let bridge = NodeBridge::new();
        let args = HashMap::new();
        assert!(bridge.route_task_to_node("unknown_task", &args).is_none());
    }

    #[test]
    fn test_node_bridge_result_to_server_message() {
        let result = NodeResult {
            task_id: Uuid::new_v4(),
            success: true,
            output: "file1\nfile2".into(),
            exit_code: Some(0),
            duration_ms: 5,
        };
        let msg = NodeBridge::node_result_to_server_message(&result);
        match msg {
            ServerMessage::Event {
                event:
                    crate::gateway::events::GatewayEvent::TaskCompleted {
                        success, summary, ..
                    },
            } => {
                assert!(success);
                assert_eq!(summary, "file1\nfile2");
            }
            _ => panic!("Expected TaskCompleted event"),
        }
    }

    #[test]
    fn test_node_bridge_error_result() {
        let result = NodeResult {
            task_id: Uuid::new_v4(),
            success: false,
            output: "command not found".into(),
            exit_code: Some(127),
            duration_ms: 1,
        };
        let msg = NodeBridge::node_result_to_server_message(&result);
        match msg {
            ServerMessage::Event {
                event:
                    crate::gateway::events::GatewayEvent::TaskCompleted {
                        success, summary, ..
                    },
            } => {
                assert!(!success);
                assert_eq!(summary, "command not found");
            }
            _ => panic!("Expected TaskCompleted event"),
        }
    }
}
