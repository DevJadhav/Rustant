//! Inter-agent messaging — message bus and envelope types.
//!
//! Provides an in-process message bus for agents to communicate asynchronously.
//! Each agent has a mailbox (bounded queue) to prevent memory exhaustion.

use std::collections::HashMap;
use uuid::Uuid;

/// Payload types for inter-agent communication.
#[derive(Debug, Clone)]
pub enum AgentPayload {
    /// Request to execute a task.
    TaskRequest {
        description: String,
        args: HashMap<String, String>,
    },
    /// Result of a completed task.
    TaskResult {
        success: bool,
        output: String,
    },
    /// Share a fact with another agent.
    FactShare {
        key: String,
        value: String,
    },
    /// Query another agent's status.
    StatusQuery,
    /// Response to a status query.
    StatusResponse {
        agent_name: String,
        active: bool,
        pending_tasks: usize,
    },
    /// Request an agent to shut down.
    Shutdown,
    /// Progress update on a task.
    Progress {
        task_id: String,
        percent: f32,
        message: String,
    },
    /// Error report.
    Error {
        code: String,
        message: String,
        recoverable: bool,
    },
    /// Query another agent about a topic.
    Query {
        topic: String,
        context: HashMap<String, String>,
    },
    /// Response to a query.
    Response {
        topic: String,
        answer: String,
    },
}

/// Priority levels for inter-agent messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// An envelope wrapping a payload with routing information.
#[derive(Debug, Clone)]
pub struct AgentEnvelope {
    /// Unique message ID.
    pub id: Uuid,
    /// Sender agent ID.
    pub from: Uuid,
    /// Recipient agent ID.
    pub to: Uuid,
    /// The payload.
    pub payload: AgentPayload,
    /// Timestamp of creation.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional correlation ID for request/response pairing.
    pub correlation_id: Option<Uuid>,
    /// Message priority.
    pub priority: MessagePriority,
}

impl AgentEnvelope {
    /// Create a new envelope with default priority (Normal) and no correlation ID.
    pub fn new(from: Uuid, to: Uuid, payload: AgentPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            payload,
            created_at: chrono::Utc::now(),
            correlation_id: None,
            priority: MessagePriority::Normal,
        }
    }

    /// Set a correlation ID for request/response pairing.
    pub fn with_correlation(mut self, id: Uuid) -> Self {
        self.correlation_id = Some(id);
        self
    }

    /// Set the message priority.
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }
}

/// In-process message bus for inter-agent communication.
/// Messages are stored in priority order (highest priority first, FIFO within same priority).
pub struct MessageBus {
    mailboxes: HashMap<Uuid, Vec<AgentEnvelope>>,
    max_mailbox_size: usize,
}

impl MessageBus {
    /// Create a new message bus with a maximum mailbox size per agent.
    pub fn new(max_mailbox_size: usize) -> Self {
        Self {
            mailboxes: HashMap::new(),
            max_mailbox_size,
        }
    }

    /// Register a mailbox for an agent.
    pub fn register(&mut self, agent_id: Uuid) {
        self.mailboxes.entry(agent_id).or_default();
    }

    /// Remove a mailbox for an agent, discarding pending messages.
    pub fn unregister(&mut self, agent_id: &Uuid) {
        self.mailboxes.remove(agent_id);
    }

    /// Send a message to an agent's mailbox. Returns Err if the mailbox is full
    /// or the recipient is not registered. Messages are inserted in priority order.
    pub fn send(&mut self, envelope: AgentEnvelope) -> Result<(), String> {
        let mailbox = self
            .mailboxes
            .get_mut(&envelope.to)
            .ok_or_else(|| format!("Agent {} not registered", envelope.to))?;

        if mailbox.len() >= self.max_mailbox_size {
            return Err(format!(
                "Mailbox for agent {} is full (max {})",
                envelope.to, self.max_mailbox_size
            ));
        }

        // Insert in sorted position: higher priority first, FIFO within same priority.
        // We find the first position where the existing message has lower priority.
        let pos = mailbox
            .iter()
            .position(|e| e.priority < envelope.priority)
            .unwrap_or(mailbox.len());
        mailbox.insert(pos, envelope);
        Ok(())
    }

    /// Receive the highest-priority message from an agent's mailbox.
    pub fn receive(&mut self, agent_id: &Uuid) -> Option<AgentEnvelope> {
        self.mailboxes
            .get_mut(agent_id)
            .and_then(|mb| if mb.is_empty() { None } else { Some(mb.remove(0)) })
    }

    /// Peek at the highest-priority message without removing it.
    pub fn peek(&self, agent_id: &Uuid) -> Option<&AgentEnvelope> {
        self.mailboxes
            .get(agent_id)
            .and_then(|mb| mb.first())
    }

    /// Number of pending messages for a specific agent.
    pub fn pending_count(&self, agent_id: &Uuid) -> usize {
        self.mailboxes
            .get(agent_id)
            .map_or(0, |mb| mb.len())
    }

    /// Total pending messages across all mailboxes.
    pub fn pending_count_all(&self) -> usize {
        self.mailboxes.values().map(|mb| mb.len()).sum()
    }

    /// Number of registered mailboxes.
    pub fn mailbox_count(&self) -> usize {
        self.mailboxes.len()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_bus_register_and_unregister() {
        let mut bus = MessageBus::new(10);
        let id = Uuid::new_v4();
        bus.register(id);
        assert_eq!(bus.mailbox_count(), 1);

        bus.unregister(&id);
        assert_eq!(bus.mailbox_count(), 0);
    }

    #[test]
    fn test_message_bus_send_and_receive() {
        let mut bus = MessageBus::new(10);
        let sender = Uuid::new_v4();
        let receiver = Uuid::new_v4();
        bus.register(sender);
        bus.register(receiver);

        let envelope = AgentEnvelope::new(
            sender,
            receiver,
            AgentPayload::StatusQuery,
        );
        bus.send(envelope).unwrap();

        assert_eq!(bus.pending_count(&receiver), 1);

        let msg = bus.receive(&receiver).unwrap();
        assert_eq!(msg.from, sender);
        assert!(matches!(msg.payload, AgentPayload::StatusQuery));
        assert_eq!(bus.pending_count(&receiver), 0);
    }

    #[test]
    fn test_message_bus_send_to_unregistered() {
        let mut bus = MessageBus::new(10);
        let sender = Uuid::new_v4();
        let ghost = Uuid::new_v4();
        bus.register(sender);

        let envelope = AgentEnvelope::new(sender, ghost, AgentPayload::Shutdown);
        let result = bus.send(envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }

    #[test]
    fn test_message_bus_mailbox_full() {
        let mut bus = MessageBus::new(2);
        let sender = Uuid::new_v4();
        let receiver = Uuid::new_v4();
        bus.register(sender);
        bus.register(receiver);

        let e1 = AgentEnvelope::new(sender, receiver, AgentPayload::StatusQuery);
        let e2 = AgentEnvelope::new(sender, receiver, AgentPayload::StatusQuery);
        let e3 = AgentEnvelope::new(sender, receiver, AgentPayload::StatusQuery);

        bus.send(e1).unwrap();
        bus.send(e2).unwrap();
        let result = bus.send(e3);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("full"));
    }

    #[test]
    fn test_message_bus_pending_count_all() {
        let mut bus = MessageBus::new(10);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        bus.register(a);
        bus.register(b);

        bus.send(AgentEnvelope::new(a, b, AgentPayload::StatusQuery)).unwrap();
        bus.send(AgentEnvelope::new(b, a, AgentPayload::Shutdown)).unwrap();

        assert_eq!(bus.pending_count_all(), 2);
    }

    #[test]
    fn test_message_bus_peek() {
        let mut bus = MessageBus::new(10);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        bus.register(a);
        bus.register(b);

        bus.send(AgentEnvelope::new(a, b, AgentPayload::StatusQuery)).unwrap();

        // Peek should not remove
        let peeked = bus.peek(&b);
        assert!(peeked.is_some());
        assert_eq!(bus.pending_count(&b), 1);

        // Receive should remove
        let received = bus.receive(&b);
        assert!(received.is_some());
        assert_eq!(bus.pending_count(&b), 0);
    }

    #[test]
    fn test_envelope_creation() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let envelope = AgentEnvelope::new(
            from,
            to,
            AgentPayload::FactShare {
                key: "test-key".into(),
                value: "test-value".into(),
            },
        );
        assert_eq!(envelope.from, from);
        assert_eq!(envelope.to, to);
        if let AgentPayload::FactShare { key, value } = &envelope.payload {
            assert_eq!(key, "test-key");
            assert_eq!(value, "test-value");
        } else {
            panic!("Expected FactShare payload");
        }
    }

    // --- Sprint 4B: New payload variants + MessagePriority ---

    #[test]
    fn test_payload_progress() {
        let payload = AgentPayload::Progress {
            task_id: "task-1".into(),
            percent: 0.75,
            message: "Almost done".into(),
        };
        if let AgentPayload::Progress {
            task_id,
            percent,
            message,
        } = &payload
        {
            assert_eq!(task_id, "task-1");
            assert!((percent - 0.75).abs() < f32::EPSILON);
            assert_eq!(message, "Almost done");
        } else {
            panic!("Expected Progress");
        }
    }

    #[test]
    fn test_payload_error_recoverable() {
        let payload = AgentPayload::Error {
            code: "E001".into(),
            message: "Something went wrong".into(),
            recoverable: true,
        };
        if let AgentPayload::Error {
            code,
            recoverable,
            ..
        } = &payload
        {
            assert_eq!(code, "E001");
            assert!(recoverable);
        } else {
            panic!("Expected Error");
        }
    }

    #[test]
    fn test_payload_query_response() {
        let query = AgentPayload::Query {
            topic: "weather".into(),
            context: HashMap::from([("city".into(), "SF".into())]),
        };
        let response = AgentPayload::Response {
            topic: "weather".into(),
            answer: "Sunny".into(),
        };
        if let AgentPayload::Query { topic, context } = &query {
            assert_eq!(topic, "weather");
            assert_eq!(context.get("city").unwrap(), "SF");
        } else {
            panic!("Expected Query");
        }
        if let AgentPayload::Response { topic, answer } = &response {
            assert_eq!(topic, "weather");
            assert_eq!(answer, "Sunny");
        } else {
            panic!("Expected Response");
        }
    }

    #[test]
    fn test_envelope_correlation_id() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let corr = Uuid::new_v4();
        let envelope = AgentEnvelope::new(from, to, AgentPayload::StatusQuery)
            .with_correlation(corr);
        assert_eq!(envelope.correlation_id, Some(corr));
    }

    #[test]
    fn test_envelope_priority_default_normal() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let envelope = AgentEnvelope::new(from, to, AgentPayload::StatusQuery);
        assert_eq!(envelope.priority, MessagePriority::Normal);
    }

    #[test]
    fn test_envelope_with_priority_critical() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let envelope = AgentEnvelope::new(from, to, AgentPayload::Shutdown)
            .with_priority(MessagePriority::Critical);
        assert_eq!(envelope.priority, MessagePriority::Critical);
    }

    #[test]
    fn test_envelope_builder_chain() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let corr = Uuid::new_v4();
        let envelope = AgentEnvelope::new(from, to, AgentPayload::StatusQuery)
            .with_correlation(corr)
            .with_priority(MessagePriority::High);
        assert_eq!(envelope.correlation_id, Some(corr));
        assert_eq!(envelope.priority, MessagePriority::High);
    }

    // --- Sprint 4C: Priority queue tests ---

    #[test]
    fn test_priority_queue_critical_first() {
        let mut bus = MessageBus::new(10);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        bus.register(b);

        // Send Normal first, then Critical
        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::StatusQuery)
                .with_priority(MessagePriority::Normal),
        )
        .unwrap();
        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::Shutdown)
                .with_priority(MessagePriority::Critical),
        )
        .unwrap();

        // Critical should come out first
        let first = bus.receive(&b).unwrap();
        assert_eq!(first.priority, MessagePriority::Critical);
        let second = bus.receive(&b).unwrap();
        assert_eq!(second.priority, MessagePriority::Normal);
    }

    #[test]
    fn test_priority_queue_fifo_same_priority() {
        let mut bus = MessageBus::new(10);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        bus.register(b);

        let e1 = AgentEnvelope::new(
            a,
            b,
            AgentPayload::FactShare {
                key: "first".into(),
                value: "1".into(),
            },
        );
        let e2 = AgentEnvelope::new(
            a,
            b,
            AgentPayload::FactShare {
                key: "second".into(),
                value: "2".into(),
            },
        );
        let id1 = e1.id;
        let id2 = e2.id;

        bus.send(e1).unwrap();
        bus.send(e2).unwrap();

        // Both Normal priority — should come out in FIFO order
        let first = bus.receive(&b).unwrap();
        assert_eq!(first.id, id1);
        let second = bus.receive(&b).unwrap();
        assert_eq!(second.id, id2);
    }

    #[test]
    fn test_priority_queue_mixed_priorities() {
        let mut bus = MessageBus::new(10);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        bus.register(b);

        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::StatusQuery)
                .with_priority(MessagePriority::Low),
        )
        .unwrap();
        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::StatusQuery)
                .with_priority(MessagePriority::High),
        )
        .unwrap();
        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::StatusQuery)
                .with_priority(MessagePriority::Normal),
        )
        .unwrap();
        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::Shutdown)
                .with_priority(MessagePriority::Critical),
        )
        .unwrap();

        assert_eq!(bus.receive(&b).unwrap().priority, MessagePriority::Critical);
        assert_eq!(bus.receive(&b).unwrap().priority, MessagePriority::High);
        assert_eq!(bus.receive(&b).unwrap().priority, MessagePriority::Normal);
        assert_eq!(bus.receive(&b).unwrap().priority, MessagePriority::Low);
    }

    #[test]
    fn test_priority_queue_empty() {
        let mut bus = MessageBus::new(10);
        let a = Uuid::new_v4();
        bus.register(a);
        assert!(bus.receive(&a).is_none());
        assert!(bus.peek(&a).is_none());
    }

    #[test]
    fn test_message_bus_priority_ordering() {
        let mut bus = MessageBus::new(10);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        bus.register(b);

        // Send Normal, then Critical
        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::StatusQuery)
                .with_priority(MessagePriority::Normal),
        )
        .unwrap();
        bus.send(
            AgentEnvelope::new(a, b, AgentPayload::Shutdown)
                .with_priority(MessagePriority::Critical),
        )
        .unwrap();

        // Peek should show Critical
        let peeked = bus.peek(&b).unwrap();
        assert_eq!(peeked.priority, MessagePriority::Critical);
    }
}
