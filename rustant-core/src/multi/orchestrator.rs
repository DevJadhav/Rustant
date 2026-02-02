//! Agent orchestrator — the async run loop that connects message bus, spawner,
//! routing, and task handlers into a cohesive multi-agent execution engine.
//!
//! The orchestrator receives messages from the `MessageBus`, dispatches them
//! to registered `TaskHandler` implementations, and returns results via the bus.
//! It enforces `ResourceLimits` on each agent.

use super::messaging::{AgentEnvelope, AgentPayload, MessageBus, MessagePriority};
use super::routing::AgentRouter;
use super::spawner::AgentSpawner;
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

/// Trait for handling tasks dispatched by the orchestrator.
///
/// Implementations receive a task description and arguments, execute the work,
/// and return a string result or an error message.
#[async_trait]
pub trait TaskHandler: Send + Sync {
    async fn handle_task(
        &self,
        description: &str,
        args: &HashMap<String, String>,
    ) -> Result<String, String>;
}

/// The agent orchestrator ties together the spawner, message bus, router,
/// and task handlers into a cohesive execution engine.
pub struct AgentOrchestrator {
    spawner: AgentSpawner,
    bus: MessageBus,
    router: AgentRouter,
    handlers: HashMap<Uuid, Box<dyn TaskHandler>>,
    tool_call_counts: HashMap<Uuid, u32>,
}

impl AgentOrchestrator {
    /// Create a new orchestrator with the given components.
    pub fn new(spawner: AgentSpawner, bus: MessageBus, router: AgentRouter) -> Self {
        Self {
            spawner,
            bus,
            router,
            handlers: HashMap::new(),
            tool_call_counts: HashMap::new(),
        }
    }

    /// Register a task handler for a specific agent.
    pub fn register_handler(&mut self, agent_id: Uuid, handler: Box<dyn TaskHandler>) {
        self.handlers.insert(agent_id, handler);
    }

    /// Access the spawner.
    pub fn spawner(&self) -> &AgentSpawner {
        &self.spawner
    }

    /// Mutably access the spawner.
    pub fn spawner_mut(&mut self) -> &mut AgentSpawner {
        &mut self.spawner
    }

    /// Access the message bus.
    pub fn bus(&self) -> &MessageBus {
        &self.bus
    }

    /// Mutably access the message bus.
    pub fn bus_mut(&mut self) -> &mut MessageBus {
        &mut self.bus
    }

    /// Access the router.
    pub fn router(&self) -> &AgentRouter {
        &self.router
    }

    /// Mutably access the router.
    pub fn router_mut(&mut self) -> &mut AgentRouter {
        &mut self.router
    }

    /// Get the current tool call count for an agent.
    pub fn tool_call_count(&self, agent_id: &Uuid) -> u32 {
        self.tool_call_counts.get(agent_id).copied().unwrap_or(0)
    }

    /// Reset tool call counts for an agent (e.g., at the start of a new turn).
    pub fn reset_tool_counts(&mut self, agent_id: &Uuid) {
        self.tool_call_counts.insert(*agent_id, 0);
    }

    /// Check whether processing a task would violate the agent's resource limits.
    ///
    /// Returns `Ok(())` if within limits, or `Err(reason)` if a limit would be exceeded.
    pub fn check_resource_limits(&self, agent_id: &Uuid) -> Result<(), String> {
        let limits = self
            .spawner
            .get(agent_id)
            .map(|ctx| &ctx.resource_limits)
            .cloned()
            .unwrap_or_default();

        // Check tool call limit
        if let Some(max_calls) = limits.max_tool_calls {
            let current = self.tool_call_count(agent_id);
            if current >= max_calls {
                return Err(format!(
                    "Agent {} exceeded max_tool_calls limit ({}/{})",
                    agent_id, current, max_calls
                ));
            }
        }

        Ok(())
    }

    /// Process all pending messages for all registered agents.
    ///
    /// For each agent with pending messages:
    /// 1. Check resource limits
    /// 2. Receive the message
    /// 3. Dispatch to the registered handler (for TaskRequest)
    /// 4. Send the result back via the bus
    ///
    /// Returns the number of messages processed.
    pub async fn process_pending(&mut self) -> usize {
        // Collect agent IDs that have pending messages and handlers
        let agent_ids: Vec<Uuid> = self
            .handlers
            .keys()
            .filter(|id| self.bus.pending_count(id) > 0)
            .copied()
            .collect();

        let mut processed = 0;

        for agent_id in agent_ids {
            // Check resource limits before processing
            if let Err(reason) = self.check_resource_limits(&agent_id) {
                // Send an error back if there's a pending message
                if let Some(envelope) = self.bus.receive(&agent_id) {
                    let error_response = AgentEnvelope::new(
                        agent_id,
                        envelope.from,
                        AgentPayload::Error {
                            code: "RESOURCE_LIMIT".into(),
                            message: reason,
                            recoverable: false,
                        },
                    )
                    .with_priority(MessagePriority::High);
                    if let Some(corr) = envelope.correlation_id {
                        let error_response = error_response.with_correlation(corr);
                        let _ = self.bus.send(error_response);
                    } else {
                        let _ = self.bus.send(error_response);
                    }
                    processed += 1;
                }
                continue;
            }

            // Receive the next message
            let envelope = match self.bus.receive(&agent_id) {
                Some(e) => e,
                None => continue,
            };

            match &envelope.payload {
                AgentPayload::TaskRequest { description, args } => {
                    // Increment tool call count
                    *self.tool_call_counts.entry(agent_id).or_insert(0) += 1;

                    let handler = match self.handlers.get(&agent_id) {
                        Some(h) => h,
                        None => continue,
                    };

                    let result = handler.handle_task(description, args).await;

                    let response_payload = match result {
                        Ok(output) => AgentPayload::TaskResult {
                            success: true,
                            output,
                        },
                        Err(err) => AgentPayload::TaskResult {
                            success: false,
                            output: err,
                        },
                    };

                    let mut response = AgentEnvelope::new(agent_id, envelope.from, response_payload);
                    if let Some(corr) = envelope.correlation_id {
                        response = response.with_correlation(corr);
                    }
                    let _ = self.bus.send(response);
                    processed += 1;
                }
                AgentPayload::Shutdown => {
                    // Terminate the agent and its children
                    self.spawner.terminate(agent_id);
                    self.handlers.remove(&agent_id);
                    self.tool_call_counts.remove(&agent_id);
                    processed += 1;
                }
                AgentPayload::StatusQuery => {
                    let pending = self.bus.pending_count(&agent_id);
                    let agent_name = self
                        .spawner
                        .get(&agent_id)
                        .map(|ctx| ctx.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    let response = AgentEnvelope::new(
                        agent_id,
                        envelope.from,
                        AgentPayload::StatusResponse {
                            agent_name,
                            active: true,
                            pending_tasks: pending,
                        },
                    );
                    let _ = self.bus.send(response);
                    processed += 1;
                }
                _ => {
                    // Other payload types are forwarded as-is (no special handling)
                    processed += 1;
                }
            }
        }

        processed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi::spawner::SpawnerConfig;

    struct EchoHandler;

    #[async_trait]
    impl TaskHandler for EchoHandler {
        async fn handle_task(
            &self,
            description: &str,
            _args: &HashMap<String, String>,
        ) -> Result<String, String> {
            Ok(format!("echo: {}", description))
        }
    }

    struct FailHandler;

    #[async_trait]
    impl TaskHandler for FailHandler {
        async fn handle_task(
            &self,
            _description: &str,
            _args: &HashMap<String, String>,
        ) -> Result<String, String> {
            Err("task failed".to_string())
        }
    }

    fn setup_orchestrator() -> (AgentOrchestrator, Uuid) {
        let mut spawner = AgentSpawner::default();
        let agent_id = spawner.spawn("test-agent").unwrap();

        let mut bus = MessageBus::new(100);
        bus.register(agent_id);

        let router = AgentRouter::new();
        let mut orch = AgentOrchestrator::new(spawner, bus, router);
        orch.register_handler(agent_id, Box::new(EchoHandler));

        (orch, agent_id)
    }

    #[tokio::test]
    async fn test_orchestrator_processes_task_request() {
        let (mut orch, agent_id) = setup_orchestrator();

        // Also register a "sender" so we can receive the response
        let sender_id = orch.spawner_mut().spawn("sender").unwrap();
        orch.bus_mut().register(sender_id);

        let task = AgentEnvelope::new(
            sender_id,
            agent_id,
            AgentPayload::TaskRequest {
                description: "hello world".into(),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task).unwrap();

        let processed = orch.process_pending().await;
        assert_eq!(processed, 1);

        // Check the response was sent back
        let response = orch.bus_mut().receive(&sender_id).unwrap();
        match &response.payload {
            AgentPayload::TaskResult { success, output } => {
                assert!(success);
                assert_eq!(output, "echo: hello world");
            }
            _ => panic!("Expected TaskResult"),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_handles_task_failure() {
        let mut spawner = AgentSpawner::default();
        let agent_id = spawner.spawn("fail-agent").unwrap();
        let sender_id = spawner.spawn("sender").unwrap();

        let mut bus = MessageBus::new(100);
        bus.register(agent_id);
        bus.register(sender_id);

        let router = AgentRouter::new();
        let mut orch = AgentOrchestrator::new(spawner, bus, router);
        orch.register_handler(agent_id, Box::new(FailHandler));

        let task = AgentEnvelope::new(
            sender_id,
            agent_id,
            AgentPayload::TaskRequest {
                description: "will fail".into(),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task).unwrap();

        orch.process_pending().await;

        let response = orch.bus_mut().receive(&sender_id).unwrap();
        match &response.payload {
            AgentPayload::TaskResult { success, output } => {
                assert!(!success);
                assert_eq!(output, "task failed");
            }
            _ => panic!("Expected TaskResult"),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_correlation_id_preserved() {
        let (mut orch, agent_id) = setup_orchestrator();
        let sender_id = orch.spawner_mut().spawn("sender").unwrap();
        orch.bus_mut().register(sender_id);

        let corr_id = Uuid::new_v4();
        let task = AgentEnvelope::new(
            sender_id,
            agent_id,
            AgentPayload::TaskRequest {
                description: "correlated".into(),
                args: HashMap::new(),
            },
        )
        .with_correlation(corr_id);
        orch.bus_mut().send(task).unwrap();

        orch.process_pending().await;

        let response = orch.bus_mut().receive(&sender_id).unwrap();
        assert_eq!(response.correlation_id, Some(corr_id));
    }

    #[tokio::test]
    async fn test_orchestrator_handles_shutdown() {
        let (mut orch, agent_id) = setup_orchestrator();
        let sender_id = orch.spawner_mut().spawn("sender").unwrap();
        orch.bus_mut().register(sender_id);

        let shutdown = AgentEnvelope::new(sender_id, agent_id, AgentPayload::Shutdown);
        orch.bus_mut().send(shutdown).unwrap();

        let processed = orch.process_pending().await;
        assert_eq!(processed, 1);

        // Agent should be terminated
        assert!(orch.spawner().get(&agent_id).is_none());
    }

    #[tokio::test]
    async fn test_orchestrator_handles_status_query() {
        let (mut orch, agent_id) = setup_orchestrator();
        let sender_id = orch.spawner_mut().spawn("sender").unwrap();
        orch.bus_mut().register(sender_id);

        let query = AgentEnvelope::new(sender_id, agent_id, AgentPayload::StatusQuery);
        orch.bus_mut().send(query).unwrap();

        orch.process_pending().await;

        let response = orch.bus_mut().receive(&sender_id).unwrap();
        match &response.payload {
            AgentPayload::StatusResponse {
                agent_name,
                active,
                pending_tasks,
            } => {
                assert_eq!(agent_name, "test-agent");
                assert!(active);
                assert_eq!(*pending_tasks, 0);
            }
            _ => panic!("Expected StatusResponse"),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_respects_tool_call_limit() {
        let mut spawner = AgentSpawner::new(SpawnerConfig::default());
        let agent_id = spawner.spawn("limited-agent").unwrap();
        let sender_id = spawner.spawn("sender").unwrap();

        // Set resource limits on the agent
        if let Some(ctx) = spawner.get_mut(&agent_id) {
            ctx.resource_limits.max_tool_calls = Some(2);
        }

        let mut bus = MessageBus::new(100);
        bus.register(agent_id);
        bus.register(sender_id);

        let router = AgentRouter::new();
        let mut orch = AgentOrchestrator::new(spawner, bus, router);
        orch.register_handler(agent_id, Box::new(EchoHandler));

        // Send and process tasks one at a time to verify limit enforcement
        // Task 1 — should succeed
        let task1 = AgentEnvelope::new(
            sender_id,
            agent_id,
            AgentPayload::TaskRequest {
                description: "task-0".into(),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task1).unwrap();
        orch.process_pending().await;

        let r1 = orch.bus_mut().receive(&sender_id).unwrap();
        match &r1.payload {
            AgentPayload::TaskResult { success, .. } => assert!(success),
            other => panic!("Expected TaskResult, got {:?}", std::mem::discriminant(other)),
        }

        // Task 2 — should succeed (count = 2 now)
        let task2 = AgentEnvelope::new(
            sender_id,
            agent_id,
            AgentPayload::TaskRequest {
                description: "task-1".into(),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task2).unwrap();
        orch.process_pending().await;

        let r2 = orch.bus_mut().receive(&sender_id).unwrap();
        match &r2.payload {
            AgentPayload::TaskResult { success, .. } => assert!(success),
            other => panic!("Expected TaskResult, got {:?}", std::mem::discriminant(other)),
        }

        // Task 3 — should hit resource limit (count = 2, max = 2)
        let task3 = AgentEnvelope::new(
            sender_id,
            agent_id,
            AgentPayload::TaskRequest {
                description: "task-2".into(),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task3).unwrap();
        orch.process_pending().await;

        let r3 = orch.bus_mut().receive(&sender_id).unwrap();
        match &r3.payload {
            AgentPayload::Error {
                code,
                recoverable,
                ..
            } => {
                assert_eq!(code, "RESOURCE_LIMIT");
                assert!(!recoverable);
            }
            other => panic!("Expected Error for third task, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn test_tool_call_count_tracking() {
        let spawner = AgentSpawner::default();
        let bus = MessageBus::new(100);
        let router = AgentRouter::new();
        let mut orch = AgentOrchestrator::new(spawner, bus, router);

        let agent_id = Uuid::new_v4();
        assert_eq!(orch.tool_call_count(&agent_id), 0);

        orch.tool_call_counts.insert(agent_id, 5);
        assert_eq!(orch.tool_call_count(&agent_id), 5);

        orch.reset_tool_counts(&agent_id);
        assert_eq!(orch.tool_call_count(&agent_id), 0);
    }

    #[tokio::test]
    async fn test_orchestrator_no_pending_returns_zero() {
        let (mut orch, _) = setup_orchestrator();
        let processed = orch.process_pending().await;
        assert_eq!(processed, 0);
    }

    #[tokio::test]
    async fn test_orchestrator_parent_delegates_to_child() {
        let mut spawner = AgentSpawner::default();
        let parent_id = spawner.spawn("parent").unwrap();
        let child_id = spawner.spawn_child("child", parent_id).unwrap();

        let mut bus = MessageBus::new(100);
        bus.register(parent_id);
        bus.register(child_id);

        let router = AgentRouter::new();
        let mut orch = AgentOrchestrator::new(spawner, bus, router);
        orch.register_handler(child_id, Box::new(EchoHandler));

        // Parent sends task to child
        let task = AgentEnvelope::new(
            parent_id,
            child_id,
            AgentPayload::TaskRequest {
                description: "delegated task".into(),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task).unwrap();

        orch.process_pending().await;

        // Parent receives the result
        let response = orch.bus_mut().receive(&parent_id).unwrap();
        match &response.payload {
            AgentPayload::TaskResult { success, output } => {
                assert!(success);
                assert_eq!(output, "echo: delegated task");
            }
            _ => panic!("Expected TaskResult"),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_multiple_agents() {
        let mut spawner = AgentSpawner::default();
        let agent_a = spawner.spawn("agent-a").unwrap();
        let agent_b = spawner.spawn("agent-b").unwrap();
        let coordinator = spawner.spawn("coordinator").unwrap();

        let mut bus = MessageBus::new(100);
        bus.register(agent_a);
        bus.register(agent_b);
        bus.register(coordinator);

        let router = AgentRouter::new();
        let mut orch = AgentOrchestrator::new(spawner, bus, router);
        orch.register_handler(agent_a, Box::new(EchoHandler));
        orch.register_handler(agent_b, Box::new(EchoHandler));

        // Coordinator sends tasks to both agents
        let task_a = AgentEnvelope::new(
            coordinator,
            agent_a,
            AgentPayload::TaskRequest {
                description: "task-for-a".into(),
                args: HashMap::new(),
            },
        );
        let task_b = AgentEnvelope::new(
            coordinator,
            agent_b,
            AgentPayload::TaskRequest {
                description: "task-for-b".into(),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task_a).unwrap();
        orch.bus_mut().send(task_b).unwrap();

        let processed = orch.process_pending().await;
        assert_eq!(processed, 2);

        // Coordinator should have 2 responses
        let r1 = orch.bus_mut().receive(&coordinator).unwrap();
        let r2 = orch.bus_mut().receive(&coordinator).unwrap();

        let mut outputs: Vec<String> = Vec::new();
        for r in [&r1, &r2] {
            if let AgentPayload::TaskResult { output, .. } = &r.payload {
                outputs.push(output.clone());
            }
        }
        outputs.sort();
        assert_eq!(outputs, vec!["echo: task-for-a", "echo: task-for-b"]);
    }

    #[test]
    fn test_check_resource_limits_no_limits() {
        let mut spawner = AgentSpawner::default();
        let agent_id = spawner.spawn("no-limits").unwrap();
        let bus = MessageBus::new(100);
        let router = AgentRouter::new();
        let orch = AgentOrchestrator::new(spawner, bus, router);
        assert!(orch.check_resource_limits(&agent_id).is_ok());
    }

    #[test]
    fn test_check_resource_limits_exceeded() {
        let mut spawner = AgentSpawner::new(SpawnerConfig::default());
        let agent_id = spawner.spawn("limited").unwrap();
        if let Some(ctx) = spawner.get_mut(&agent_id) {
            ctx.resource_limits.max_tool_calls = Some(3);
        }

        let bus = MessageBus::new(100);
        let router = AgentRouter::new();
        let mut orch = AgentOrchestrator::new(spawner, bus, router);

        // Simulate 3 tool calls
        orch.tool_call_counts.insert(agent_id, 3);

        let result = orch.check_resource_limits(&agent_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("max_tool_calls"));
    }
}
