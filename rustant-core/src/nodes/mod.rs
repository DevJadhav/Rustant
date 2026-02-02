//! # Node System
//!
//! Multi-device capability nodes for the Rustant agent.
//! Each node implements the `Node` trait, providing capabilities
//! like shell execution, AppleScript, screenshots, etc.

pub mod consent;
pub mod discovery;
pub mod linux;
pub mod macos;
pub mod manager;
pub mod types;

pub use consent::{ConsentEntry, ConsentStore};
pub use discovery::{
    DiscoveredNode, MdnsConfig, MdnsDiscovery, MdnsServiceRecord, MdnsTransport, NodeDiscovery,
    UdpMdnsTransport, MDNS_MULTICAST_ADDR, MDNS_PORT, RUSTANT_SERVICE_NAME,
};
pub use manager::NodeManager;
pub use types::{
    Capability, NodeCapability, NodeHealth, NodeId, NodeInfo, NodeMessage, NodeResult, NodeTask,
    Platform, RateLimit,
};

use crate::error::RustantError;
use async_trait::async_trait;

/// Core trait for node implementations.
#[async_trait]
pub trait Node: Send + Sync {
    /// Unique identifier for this node.
    fn node_id(&self) -> &NodeId;

    /// Descriptive info about this node.
    fn info(&self) -> &NodeInfo;

    /// Capabilities this node can provide.
    fn capabilities(&self) -> &[Capability];

    /// Execute a task on this node.
    async fn execute(&self, task: NodeTask) -> Result<NodeResult, RustantError>;

    /// Current health of this node.
    fn health(&self) -> NodeHealth;

    /// Perform a heartbeat check.
    async fn heartbeat(&self) -> Result<NodeHealth, RustantError>;

    /// Rich capability descriptions. Default wraps each Capability into a basic NodeCapability.
    fn rich_capabilities(&self) -> Vec<NodeCapability> {
        self.capabilities()
            .iter()
            .map(|c| NodeCapability::basic(c.clone()))
            .collect()
    }

    /// Handle an inter-node protocol message. Default: responds to Ping and CapabilityQuery.
    async fn handle_message(&self, msg: NodeMessage) -> Result<Option<NodeMessage>, RustantError> {
        match msg {
            NodeMessage::Ping => Ok(Some(NodeMessage::Pong {
                uptime_secs: self.info().uptime_secs,
            })),
            NodeMessage::CapabilityQuery => Ok(Some(NodeMessage::CapabilityResponse {
                capabilities: self.rich_capabilities(),
            })),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    struct DefaultTestNode {
        id: NodeId,
        info: NodeInfo,
        capabilities: Vec<Capability>,
    }

    impl DefaultTestNode {
        fn new() -> Self {
            let id = NodeId::new("test-node");
            Self {
                id: id.clone(),
                info: NodeInfo {
                    node_id: id,
                    name: "test".into(),
                    platform: Platform::MacOS,
                    hostname: "test-host".into(),
                    registered_at: Utc::now(),
                    os_version: None,
                    agent_version: "0.1.0".into(),
                    uptime_secs: 42,
                },
                capabilities: vec![Capability::Shell, Capability::FileSystem],
            }
        }
    }

    #[async_trait]
    impl Node for DefaultTestNode {
        fn node_id(&self) -> &NodeId {
            &self.id
        }
        fn info(&self) -> &NodeInfo {
            &self.info
        }
        fn capabilities(&self) -> &[Capability] {
            &self.capabilities
        }
        async fn execute(&self, task: NodeTask) -> Result<NodeResult, RustantError> {
            Ok(NodeResult {
                task_id: task.task_id,
                success: true,
                output: "ok".into(),
                exit_code: Some(0),
                duration_ms: 1,
            })
        }
        fn health(&self) -> NodeHealth {
            NodeHealth::Healthy
        }
        async fn heartbeat(&self) -> Result<NodeHealth, RustantError> {
            Ok(NodeHealth::Healthy)
        }
    }

    #[test]
    fn test_node_types_reexported() {
        let _ = NodeHealth::Healthy;
        let _ = Capability::Shell;
        let _ = Platform::MacOS;
    }

    #[test]
    fn test_node_default_rich_capabilities() {
        let node = DefaultTestNode::new();
        let rich = node.rich_capabilities();
        assert_eq!(rich.len(), 2);
        assert_eq!(rich[0].capability, Capability::Shell);
        assert!(rich[0].requires_consent);
        assert_eq!(rich[1].capability, Capability::FileSystem);
    }

    #[tokio::test]
    async fn test_node_default_handle_ping() {
        let node = DefaultTestNode::new();
        let response = node.handle_message(NodeMessage::Ping).await.unwrap();
        match response {
            Some(NodeMessage::Pong { uptime_secs }) => assert_eq!(uptime_secs, 42),
            _ => panic!("Expected Pong"),
        }
    }

    #[tokio::test]
    async fn test_node_default_handle_capability_query() {
        let node = DefaultTestNode::new();
        let response = node
            .handle_message(NodeMessage::CapabilityQuery)
            .await
            .unwrap();
        match response {
            Some(NodeMessage::CapabilityResponse { capabilities }) => {
                assert_eq!(capabilities.len(), 2);
            }
            _ => panic!("Expected CapabilityResponse"),
        }
    }

    #[tokio::test]
    async fn test_node_default_handle_unknown() {
        let node = DefaultTestNode::new();
        let response = node.handle_message(NodeMessage::Shutdown).await.unwrap();
        assert!(response.is_none());
    }
}
