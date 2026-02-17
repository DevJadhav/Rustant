//! Node manager — registers, finds capable nodes, and executes tasks.

use super::{
    Node,
    consent::ConsentStore,
    types::{Capability, NodeCapability, NodeHealth, NodeId, NodeMessage, NodeResult, NodeTask},
};
use crate::error::{NodeError, RustantError};
use std::collections::HashMap;

/// Manages registered nodes and dispatches tasks.
pub struct NodeManager {
    nodes: HashMap<NodeId, Box<dyn Node>>,
    consent: ConsentStore,
}

impl NodeManager {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            consent: ConsentStore::new(),
        }
    }

    /// Register a node.
    pub fn register_node(&mut self, node: Box<dyn Node>) {
        let id = node.node_id().clone();
        self.nodes.insert(id, node);
    }

    /// Remove a node by ID.
    pub fn remove_node(&mut self, node_id: &NodeId) -> bool {
        self.consent.revoke_all(node_id);
        self.nodes.remove(node_id).is_some()
    }

    /// Number of registered nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get a reference to the consent store.
    pub fn consent(&self) -> &ConsentStore {
        &self.consent
    }

    /// Get a mutable reference to the consent store.
    pub fn consent_mut(&mut self) -> &mut ConsentStore {
        &mut self.consent
    }

    /// Find all nodes that have a given capability and are healthy.
    pub fn find_capable(&self, capability: &Capability) -> Vec<&NodeId> {
        self.nodes
            .iter()
            .filter(|(_, node)| {
                node.capabilities().contains(capability) && node.health() == NodeHealth::Healthy
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// Execute a task on the best available node (healthy + consented).
    pub async fn execute_on_best(&self, task: NodeTask) -> Result<NodeResult, RustantError> {
        let capable = self.find_capable(&task.capability);
        if capable.is_empty() {
            return Err(RustantError::Node(NodeError::NoCapableNode {
                capability: task.capability.to_string(),
            }));
        }

        // Find first consented healthy node
        for node_id in &capable {
            if self.consent.is_granted(node_id, &task.capability)
                && let Some(node) = self.nodes.get(node_id)
            {
                return node.execute(task).await;
            }
        }

        Err(RustantError::Node(NodeError::ConsentDenied {
            capability: task.capability.to_string(),
        }))
    }

    /// Get node IDs.
    pub fn node_ids(&self) -> Vec<&NodeId> {
        self.nodes.keys().collect()
    }

    /// Find all healthy nodes with a given capability that also have consent granted.
    pub fn find_capable_with_consent(
        &self,
        capability: &Capability,
        consent_store: &ConsentStore,
    ) -> Vec<&NodeId> {
        self.nodes
            .iter()
            .filter(|(id, node)| {
                node.capabilities().contains(capability)
                    && node.health() == NodeHealth::Healthy
                    && consent_store.is_granted(id, capability)
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// Broadcast a message to all registered nodes. Returns responses from each node.
    pub async fn broadcast_message(&self, msg: &NodeMessage) -> Vec<(NodeId, Option<NodeMessage>)> {
        let mut results = Vec::new();
        for (id, node) in &self.nodes {
            let response = node.handle_message(msg.clone()).await.unwrap_or(None);
            results.push((id.clone(), response));
        }
        results
    }

    /// Aggregate rich capabilities from all registered nodes.
    pub fn node_capabilities_map(&self) -> HashMap<NodeId, Vec<NodeCapability>> {
        self.nodes
            .iter()
            .map(|(id, node)| (id.clone(), node.rich_capabilities()))
            .collect()
    }
}

impl Default for NodeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{NodeInfo, Platform};
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    struct MockNode {
        id: NodeId,
        info: NodeInfo,
        capabilities: Vec<Capability>,
        health: NodeHealth,
    }

    impl MockNode {
        fn new(name: &str, capabilities: Vec<Capability>) -> Self {
            let id = NodeId::new(name);
            Self {
                id: id.clone(),
                info: NodeInfo {
                    node_id: id,
                    name: name.to_string(),
                    platform: Platform::MacOS,
                    hostname: "test".into(),
                    registered_at: Utc::now(),
                    os_version: None,
                    agent_version: "0.1.0".into(),
                    uptime_secs: 0,
                },
                capabilities,
                health: NodeHealth::Healthy,
            }
        }
    }

    #[async_trait]
    impl Node for MockNode {
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
                output: "mock output".into(),
                exit_code: Some(0),
                duration_ms: 1,
            })
        }
        fn health(&self) -> NodeHealth {
            self.health
        }
        async fn heartbeat(&self) -> Result<NodeHealth, RustantError> {
            Ok(self.health)
        }
    }

    #[test]
    fn test_manager_new() {
        let mgr = NodeManager::new();
        assert_eq!(mgr.node_count(), 0);
    }

    #[test]
    fn test_manager_register() {
        let mut mgr = NodeManager::new();
        mgr.register_node(Box::new(MockNode::new("n1", vec![Capability::Shell])));
        assert_eq!(mgr.node_count(), 1);
    }

    #[test]
    fn test_manager_remove() {
        let mut mgr = NodeManager::new();
        let id = NodeId::new("n1");
        mgr.register_node(Box::new(MockNode::new("n1", vec![Capability::Shell])));
        assert!(mgr.remove_node(&id));
        assert_eq!(mgr.node_count(), 0);
    }

    #[test]
    fn test_manager_find_capable() {
        let mut mgr = NodeManager::new();
        mgr.register_node(Box::new(MockNode::new(
            "n1",
            vec![Capability::Shell, Capability::FileSystem],
        )));
        mgr.register_node(Box::new(MockNode::new("n2", vec![Capability::Shell])));
        mgr.register_node(Box::new(MockNode::new("n3", vec![Capability::Screenshot])));

        let shell_nodes = mgr.find_capable(&Capability::Shell);
        assert_eq!(shell_nodes.len(), 2);

        let screenshot_nodes = mgr.find_capable(&Capability::Screenshot);
        assert_eq!(screenshot_nodes.len(), 1);

        let apple_nodes = mgr.find_capable(&Capability::AppleScript);
        assert!(apple_nodes.is_empty());
    }

    #[tokio::test]
    async fn test_manager_execute_on_best() {
        let mut mgr = NodeManager::new();
        let node_id = NodeId::new("n1");
        mgr.register_node(Box::new(MockNode::new("n1", vec![Capability::Shell])));
        mgr.consent_mut().grant(&node_id, Capability::Shell);

        let task = NodeTask::new(Capability::Shell, "echo hello");
        let result = mgr.execute_on_best(task).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_manager_execute_no_capable() {
        let mgr = NodeManager::new();
        let task = NodeTask::new(Capability::Shell, "echo hello");
        let result = mgr.execute_on_best(task).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_execute_no_consent() {
        let mut mgr = NodeManager::new();
        mgr.register_node(Box::new(MockNode::new("n1", vec![Capability::Shell])));
        // No consent granted

        let task = NodeTask::new(Capability::Shell, "echo hello");
        let result = mgr.execute_on_best(task).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_manager_find_with_consent() {
        let mut mgr = NodeManager::new();
        let n1 = NodeId::new("n1");
        let _n2 = NodeId::new("n2");
        mgr.register_node(Box::new(MockNode::new("n1", vec![Capability::Shell])));
        mgr.register_node(Box::new(MockNode::new("n2", vec![Capability::Shell])));

        let mut consent = ConsentStore::new();
        consent.grant(&n1, Capability::Shell);
        // n2 not consented

        let found = mgr.find_capable_with_consent(&Capability::Shell, &consent);
        assert_eq!(found.len(), 1);
        assert_eq!(*found[0], n1);
    }

    #[test]
    fn test_manager_find_without_consent_excluded() {
        let mut mgr = NodeManager::new();
        mgr.register_node(Box::new(MockNode::new("n1", vec![Capability::Shell])));

        let consent = ConsentStore::new(); // No grants at all
        let found = mgr.find_capable_with_consent(&Capability::Shell, &consent);
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn test_manager_broadcast_message() {
        let mut mgr = NodeManager::new();
        mgr.register_node(Box::new(MockNode::new("n1", vec![Capability::Shell])));
        mgr.register_node(Box::new(MockNode::new("n2", vec![Capability::FileSystem])));

        let results = mgr.broadcast_message(&NodeMessage::Ping).await;
        assert_eq!(results.len(), 2);
        for (_, response) in &results {
            match response {
                Some(NodeMessage::Pong { uptime_secs }) => assert_eq!(*uptime_secs, 0),
                _ => panic!("Expected Pong"),
            }
        }
    }

    #[test]
    fn test_manager_capabilities_map() {
        let mut mgr = NodeManager::new();
        mgr.register_node(Box::new(MockNode::new(
            "n1",
            vec![Capability::Shell, Capability::FileSystem],
        )));
        mgr.register_node(Box::new(MockNode::new("n2", vec![Capability::Screenshot])));

        let map = mgr.node_capabilities_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map[&NodeId::new("n1")].len(), 2);
        assert_eq!(map[&NodeId::new("n2")].len(), 1);
    }
}
