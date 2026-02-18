//! Multi-agent system — isolation, routing, spawning, and inter-agent messaging.
//!
//! Provides the building blocks for running multiple agents within a single
//! Rustant instance, each with its own isolated memory and safety context.

pub mod isolation;
pub mod messaging;
pub mod orchestrator;
pub mod routing;
pub mod spawner;
pub mod teams;

pub use isolation::{AgentContext, AgentStatus, ResourceLimits};
pub use messaging::{AgentEnvelope, AgentPayload, MessageBus, MessagePriority};
pub use orchestrator::{AgentOrchestrator, TaskHandler};
pub use routing::{AgentRoute, AgentRouter};
pub use spawner::AgentSpawner;
pub use teams::{
    AgentTeam, CoordinationStrategy, SharedContext, TeamDecision, TeamMember, TeamRegistry,
    TeamTask, TeamTaskStatus,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelType;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn test_multi_module_exports() {
        // Verify that key types are accessible via the module re-exports.
        let bus = MessageBus::new(100);
        assert_eq!(bus.pending_count_all(), 0);
    }

    #[test]
    fn test_channel_to_agent_to_node_flow() {
        // ChannelMessage → AgentRouter → MessageBus → AgentPayload::TaskRequest
        let agent_id = Uuid::new_v4();
        let mut router = AgentRouter::new();
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: agent_id,
            conditions: vec![routing::RouteCondition::ChannelType(ChannelType::Telegram)],
        });

        // Route a channel message
        let req = routing::RouteRequest::new()
            .with_channel(ChannelType::Telegram)
            .with_message("run shell ls");
        let target = router.route(&req).unwrap();
        assert_eq!(target, agent_id);

        // Deliver as a TaskRequest via MessageBus
        let mut bus = MessageBus::new(100);
        bus.register(agent_id);

        let from = Uuid::new_v4();
        let mut args = HashMap::new();
        args.insert("channel_type".into(), "Telegram".into());
        let envelope = AgentEnvelope::new(
            from,
            agent_id,
            AgentPayload::TaskRequest {
                description: "run shell ls".into(),
                args,
            },
        );
        bus.send(envelope).unwrap();

        // Agent receives the message
        let received = bus.receive(&agent_id).unwrap();
        match &received.payload {
            AgentPayload::TaskRequest { description, args } => {
                assert_eq!(description, "run shell ls");
                assert_eq!(args.get("channel_type").unwrap(), "Telegram");
            }
            _ => panic!("Expected TaskRequest"),
        }
    }

    #[test]
    fn test_multi_agent_delegation() {
        // Parent spawns child, sends TaskRequest, child responds with TaskResult
        let mut spawner = AgentSpawner::default();
        let parent = spawner.spawn("parent").unwrap();
        let child = spawner.spawn_child("child", parent).unwrap();

        let mut bus = MessageBus::new(100);
        bus.register(parent);
        bus.register(child);

        // Parent delegates a task to child
        let task = AgentEnvelope::new(
            parent,
            child,
            AgentPayload::TaskRequest {
                description: "analyze code".into(),
                args: HashMap::new(),
            },
        );
        let correlation = task.id;
        bus.send(task).unwrap();

        // Child receives
        let received = bus.receive(&child).unwrap();
        assert_eq!(received.from, parent);

        // Child responds with a result using the correlation ID
        let response = AgentEnvelope::new(
            child,
            parent,
            AgentPayload::TaskResult {
                success: true,
                output: "Analysis complete".into(),
            },
        )
        .with_correlation(correlation);
        bus.send(response).unwrap();

        // Parent receives correlated response
        let result = bus.receive(&parent).unwrap();
        assert_eq!(result.correlation_id, Some(correlation));
        match &result.payload {
            AgentPayload::TaskResult { success, output } => {
                assert!(success);
                assert_eq!(output, "Analysis complete");
            }
            _ => panic!("Expected TaskResult"),
        }
    }

    #[test]
    fn test_agent_fact_sharing() {
        // Agent A sends FactShare to Agent B, B receives and processes
        let mut spawner = AgentSpawner::default();
        let agent_a = spawner.spawn("agent-a").unwrap();
        let agent_b = spawner.spawn("agent-b").unwrap();

        let mut bus = MessageBus::new(100);
        bus.register(agent_a);
        bus.register(agent_b);

        // Agent A shares a fact with Agent B
        let fact = AgentEnvelope::new(
            agent_a,
            agent_b,
            AgentPayload::FactShare {
                key: "project.language".into(),
                value: "Rust".into(),
            },
        );
        bus.send(fact).unwrap();

        // Agent B receives the fact
        let received = bus.receive(&agent_b).unwrap();
        assert_eq!(received.from, agent_a);
        match &received.payload {
            AgentPayload::FactShare { key, value } => {
                assert_eq!(key, "project.language");
                assert_eq!(value, "Rust");
            }
            _ => panic!("Expected FactShare"),
        }
    }

    #[test]
    fn test_full_lifecycle() {
        // Spawn agents, register routes, send messages, terminate cascading
        let mut spawner = AgentSpawner::default();
        let supervisor = spawner.spawn("supervisor").unwrap();
        let worker1 = spawner.spawn_child("worker-1", supervisor).unwrap();
        let worker2 = spawner.spawn_child("worker-2", supervisor).unwrap();
        assert_eq!(spawner.agent_count(), 3);

        // Set up routing
        let mut router = AgentRouter::new().with_default(supervisor);
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: worker1,
            conditions: vec![routing::RouteCondition::TaskPrefix("code:".into())],
        });
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: worker2,
            conditions: vec![routing::RouteCondition::TaskPrefix("test:".into())],
        });

        // Route tasks
        let req1 = routing::RouteRequest::new().with_task("code:refactor");
        let req2 = routing::RouteRequest::new().with_task("test:unit");
        let req3 = routing::RouteRequest::new().with_task("deploy:prod");

        assert_eq!(router.route(&req1), Some(worker1));
        assert_eq!(router.route(&req2), Some(worker2));
        assert_eq!(router.route(&req3), Some(supervisor)); // default

        // Set up bus and send messages
        let mut bus = MessageBus::new(100);
        bus.register(supervisor);
        bus.register(worker1);
        bus.register(worker2);

        let task = AgentEnvelope::new(
            supervisor,
            worker1,
            AgentPayload::TaskRequest {
                description: "code:refactor main.rs".into(),
                args: HashMap::new(),
            },
        );
        bus.send(task).unwrap();
        assert_eq!(bus.pending_count(&worker1), 1);

        // Terminate supervisor cascades to workers
        let removed = spawner.terminate(supervisor);
        assert_eq!(removed, 3);
        assert_eq!(spawner.agent_count(), 0);

        // Bus still has the message (bus and spawner are independent)
        assert_eq!(bus.pending_count(&worker1), 1);
    }
}
