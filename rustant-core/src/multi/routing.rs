//! Agent routing — directs tasks and channel messages to the appropriate agent.
//!
//! The `AgentRouter` evaluates routing rules in priority order and returns
//! the ID of the agent that should handle a given task or message.

use crate::channels::ChannelType;
use uuid::Uuid;

/// A routing rule that maps conditions to a target agent.
#[derive(Debug, Clone)]
pub struct AgentRoute {
    /// Priority (lower = higher priority).
    pub priority: u32,
    /// The agent to route to if all conditions match.
    pub target_agent_id: Uuid,
    /// All conditions must match for this route to apply.
    pub conditions: Vec<RouteCondition>,
}

/// Conditions that can be evaluated for routing decisions.
#[derive(Debug, Clone)]
pub enum RouteCondition {
    /// Match on the channel type.
    ChannelType(ChannelType),
    /// Match on the user ID (platform-specific).
    UserId(String),
    /// Match if the message text contains a substring.
    MessageContains(String),
    /// Match if the task name/command starts with a prefix.
    TaskPrefix(String),
    /// Match a specific capability name.
    CapabilityName(String),
}

/// A routing request containing the information needed to pick an agent.
#[derive(Debug, Clone, Default)]
pub struct RouteRequest {
    pub channel_type: Option<ChannelType>,
    pub user_id: Option<String>,
    pub message_text: Option<String>,
    pub task_name: Option<String>,
    pub capability: Option<String>,
}

impl RouteRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_channel(mut self, ct: ChannelType) -> Self {
        self.channel_type = Some(ct);
        self
    }

    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_message(mut self, text: impl Into<String>) -> Self {
        self.message_text = Some(text.into());
        self
    }

    pub fn with_task(mut self, name: impl Into<String>) -> Self {
        self.task_name = Some(name.into());
        self
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.capability = Some(cap.into());
        self
    }
}

/// Routes tasks and messages to agents based on rules.
pub struct AgentRouter {
    routes: Vec<AgentRoute>,
    /// Default agent for unmatched requests.
    default_agent_id: Option<Uuid>,
}

impl AgentRouter {
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            default_agent_id: None,
        }
    }

    /// Set the default agent that handles unmatched requests.
    pub fn with_default(mut self, agent_id: Uuid) -> Self {
        self.default_agent_id = Some(agent_id);
        self
    }

    /// Add a routing rule.
    pub fn add_route(&mut self, route: AgentRoute) {
        self.routes.push(route);
        self.routes.sort_by_key(|r| r.priority);
    }

    /// Find the best-matching agent for a given request.
    pub fn route(&self, request: &RouteRequest) -> Option<Uuid> {
        for route in &self.routes {
            if self.matches_all(&route.conditions, request) {
                return Some(route.target_agent_id);
            }
        }
        self.default_agent_id
    }

    /// Number of registered routes.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    fn matches_all(&self, conditions: &[RouteCondition], request: &RouteRequest) -> bool {
        conditions.iter().all(|c| self.matches(c, request))
    }

    fn matches(&self, condition: &RouteCondition, request: &RouteRequest) -> bool {
        match condition {
            RouteCondition::ChannelType(ct) => request.channel_type.as_ref() == Some(ct),
            RouteCondition::UserId(uid) => request.user_id.as_deref() == Some(uid.as_str()),
            RouteCondition::MessageContains(sub) => request
                .message_text
                .as_ref()
                .is_some_and(|t| t.contains(sub.as_str())),
            RouteCondition::TaskPrefix(prefix) => request
                .task_name
                .as_ref()
                .is_some_and(|t| t.starts_with(prefix.as_str())),
            RouteCondition::CapabilityName(cap) => {
                request.capability.as_deref() == Some(cap.as_str())
            }
        }
    }
}

impl Default for AgentRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_no_routes_returns_default() {
        let default_id = Uuid::new_v4();
        let router = AgentRouter::new().with_default(default_id);
        let req = RouteRequest::new().with_message("hello");
        assert_eq!(router.route(&req), Some(default_id));
    }

    #[test]
    fn test_router_no_routes_no_default_returns_none() {
        let router = AgentRouter::new();
        let req = RouteRequest::new().with_message("hello");
        assert_eq!(router.route(&req), None);
    }

    #[test]
    fn test_router_matches_channel_type() {
        let agent_id = Uuid::new_v4();
        let mut router = AgentRouter::new();
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: agent_id,
            conditions: vec![RouteCondition::ChannelType(ChannelType::Telegram)],
        });

        let req = RouteRequest::new().with_channel(ChannelType::Telegram);
        assert_eq!(router.route(&req), Some(agent_id));

        let req2 = RouteRequest::new().with_channel(ChannelType::Discord);
        assert_eq!(router.route(&req2), None);
    }

    #[test]
    fn test_router_priority_ordering() {
        let low_prio_agent = Uuid::new_v4();
        let high_prio_agent = Uuid::new_v4();
        let mut router = AgentRouter::new();

        // Add low-priority first
        router.add_route(AgentRoute {
            priority: 10,
            target_agent_id: low_prio_agent,
            conditions: vec![RouteCondition::MessageContains("help".into())],
        });
        // Add high-priority second
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: high_prio_agent,
            conditions: vec![RouteCondition::MessageContains("help".into())],
        });

        let req = RouteRequest::new().with_message("I need help");
        // Should match higher priority (lower number) first
        assert_eq!(router.route(&req), Some(high_prio_agent));
    }

    #[test]
    fn test_router_multiple_conditions_all_must_match() {
        let agent_id = Uuid::new_v4();
        let mut router = AgentRouter::new();
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: agent_id,
            conditions: vec![
                RouteCondition::ChannelType(ChannelType::Discord),
                RouteCondition::UserId("user-42".into()),
            ],
        });

        // Both conditions met
        let req = RouteRequest::new()
            .with_channel(ChannelType::Discord)
            .with_user("user-42");
        assert_eq!(router.route(&req), Some(agent_id));

        // Only one condition met
        let req2 = RouteRequest::new()
            .with_channel(ChannelType::Discord)
            .with_user("user-99");
        assert_eq!(router.route(&req2), None);
    }
}
