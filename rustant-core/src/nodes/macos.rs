//! macOS node — local execution via shell, AppleScript, notifications.
//!
//! Uses `tokio::process::Command` for executing local commands.
//! In tests, a trait abstraction allows mocking without real execution.

use super::{
    types::{Capability, NodeHealth, NodeId, NodeInfo, NodeResult, NodeTask, Platform},
    Node,
};
use crate::error::{NodeError, RustantError};
use async_trait::async_trait;
use chrono::Utc;

/// Trait for executing local commands, allowing test mocking.
#[async_trait]
pub trait LocalExecutor: Send + Sync {
    async fn execute_command(
        &self,
        command: &str,
        args: &[String],
        timeout_secs: u64,
    ) -> Result<(String, i32), String>;
}

/// macOS node using local command execution.
pub struct MacOsNode {
    node_id: NodeId,
    info: NodeInfo,
    capabilities: Vec<Capability>,
    executor: Box<dyn LocalExecutor>,
    health: NodeHealth,
}

impl MacOsNode {
    pub fn new(executor: Box<dyn LocalExecutor>) -> Self {
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "macos-local".to_string());
        let node_id = NodeId::new(format!("macos-{}", hostname));
        let info = NodeInfo {
            node_id: node_id.clone(),
            name: format!("macOS ({})", hostname),
            platform: Platform::MacOS,
            hostname,
            registered_at: Utc::now(),
            os_version: None,
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: 0,
        };
        Self {
            node_id,
            info,
            capabilities: vec![
                Capability::Shell,
                Capability::FileSystem,
                Capability::AppleScript,
                Capability::Screenshot,
                Capability::Clipboard,
                Capability::Notifications,
            ],
            executor,
            health: NodeHealth::Healthy,
        }
    }
}

#[async_trait]
impl Node for MacOsNode {
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    fn info(&self) -> &NodeInfo {
        &self.info
    }

    fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    async fn execute(&self, task: NodeTask) -> Result<NodeResult, RustantError> {
        if !self.capabilities.contains(&task.capability) {
            return Err(RustantError::Node(NodeError::ExecutionFailed {
                node_id: self.node_id.to_string(),
                message: format!("Capability {} not supported", task.capability),
            }));
        }

        let start = std::time::Instant::now();
        let (output, exit_code) = self
            .executor
            .execute_command(&task.command, &task.args, task.timeout_secs)
            .await
            .map_err(|e| {
                RustantError::Node(NodeError::ExecutionFailed {
                    node_id: self.node_id.to_string(),
                    message: e,
                })
            })?;
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(NodeResult {
            task_id: task.task_id,
            success: exit_code == 0,
            output,
            exit_code: Some(exit_code),
            duration_ms,
        })
    }

    fn health(&self) -> NodeHealth {
        self.health
    }

    async fn heartbeat(&self) -> Result<NodeHealth, RustantError> {
        // Simple check: try running 'echo ok'
        match self
            .executor
            .execute_command("echo", &["ok".into()], 5)
            .await
        {
            Ok((output, 0)) if output.contains("ok") => Ok(NodeHealth::Healthy),
            Ok(_) => Ok(NodeHealth::Degraded),
            Err(_) => Ok(NodeHealth::Unreachable),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor {
        output: String,
        exit_code: i32,
    }

    impl MockExecutor {
        fn ok(output: &str) -> Self {
            Self {
                output: output.to_string(),
                exit_code: 0,
            }
        }

        fn fail(message: &str) -> Self {
            Self {
                output: message.to_string(),
                exit_code: 1,
            }
        }
    }

    #[async_trait]
    impl LocalExecutor for MockExecutor {
        async fn execute_command(
            &self,
            _cmd: &str,
            _args: &[String],
            _timeout: u64,
        ) -> Result<(String, i32), String> {
            Ok((self.output.clone(), self.exit_code))
        }
    }

    #[test]
    fn test_macos_node_capabilities() {
        let node = MacOsNode::new(Box::new(MockExecutor::ok("ok")));
        let caps = node.capabilities();
        assert!(caps.contains(&Capability::Shell));
        assert!(caps.contains(&Capability::AppleScript));
        assert!(caps.contains(&Capability::Screenshot));
    }

    #[tokio::test]
    async fn test_macos_execute_success() {
        let node = MacOsNode::new(Box::new(MockExecutor::ok("file1\nfile2\n")));
        let task = NodeTask::new(Capability::Shell, "ls");
        let result = node.execute(task).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "file1\nfile2\n");
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_macos_execute_failure() {
        let node = MacOsNode::new(Box::new(MockExecutor::fail("error")));
        let task = NodeTask::new(Capability::Shell, "bad-cmd");
        let result = node.execute(task).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
    }

    #[tokio::test]
    async fn test_macos_heartbeat() {
        let node = MacOsNode::new(Box::new(MockExecutor::ok("ok")));
        let health = node.heartbeat().await.unwrap();
        assert_eq!(health, NodeHealth::Healthy);
    }
}
