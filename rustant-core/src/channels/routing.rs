//! Channel routing — rule-based routing of incoming messages to agents.

use super::{ChannelMessage, ChannelType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A routing condition used to match messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingCondition {
    /// Match by channel type.
    ChannelType(ChannelType),
    /// Match by sender user ID.
    UserId(String),
    /// Match if the text content contains a substring.
    MessageContains(String),
    /// Match by command prefix (e.g., "/agent2").
    CommandPrefix(String),
}

/// A routing rule: conditions + target agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub priority: u32,
    pub conditions: Vec<RoutingCondition>,
    pub target_agent: Uuid,
}

/// Routes incoming channel messages to the appropriate agent.
#[derive(Debug, Clone, Default)]
pub struct ChannelRouter {
    rules: Vec<RoutingRule>,
    default_agent: Option<Uuid>,
}

impl ChannelRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the default agent for unmatched messages.
    pub fn with_default_agent(mut self, agent_id: Uuid) -> Self {
        self.default_agent = Some(agent_id);
        self
    }

    /// Add a routing rule.
    pub fn add_rule(&mut self, rule: RoutingRule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| a.priority.cmp(&b.priority));
    }

    /// Number of rules configured.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Route a message to the appropriate agent. Returns the target agent ID.
    pub fn route(&self, msg: &ChannelMessage) -> Option<Uuid> {
        for rule in &self.rules {
            if self.matches_rule(rule, msg) {
                return Some(rule.target_agent);
            }
        }
        self.default_agent
    }

    fn matches_rule(&self, rule: &RoutingRule, msg: &ChannelMessage) -> bool {
        rule.conditions
            .iter()
            .all(|cond| self.matches_condition(cond, msg))
    }

    fn matches_condition(&self, cond: &RoutingCondition, msg: &ChannelMessage) -> bool {
        match cond {
            RoutingCondition::ChannelType(ct) => msg.channel_type == *ct,
            RoutingCondition::UserId(id) => msg.sender.id == *id,
            RoutingCondition::MessageContains(sub) => msg
                .content
                .as_text()
                .map(|t| t.contains(sub.as_str()))
                .unwrap_or(false),
            RoutingCondition::CommandPrefix(prefix) => msg
                .content
                .as_text()
                .map(|t| t.starts_with(prefix.as_str()))
                .unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelUser;

    fn make_msg(channel_type: ChannelType, user_id: &str, text: &str) -> ChannelMessage {
        let sender = ChannelUser::new(user_id, channel_type);
        ChannelMessage::text(channel_type, "ch1", sender, text)
    }

    #[test]
    fn test_router_no_rules_no_default() {
        let router = ChannelRouter::new();
        let msg = make_msg(ChannelType::Telegram, "u1", "hello");
        assert!(router.route(&msg).is_none());
    }

    #[test]
    fn test_router_default_agent() {
        let default_id = Uuid::new_v4();
        let router = ChannelRouter::new().with_default_agent(default_id);
        let msg = make_msg(ChannelType::Slack, "u1", "hello");
        assert_eq!(router.route(&msg), Some(default_id));
    }

    #[test]
    fn test_router_channel_type_rule() {
        let agent_tg = Uuid::new_v4();
        let agent_sl = Uuid::new_v4();

        let mut router = ChannelRouter::new();
        router.add_rule(RoutingRule {
            priority: 1,
            conditions: vec![RoutingCondition::ChannelType(ChannelType::Telegram)],
            target_agent: agent_tg,
        });
        router.add_rule(RoutingRule {
            priority: 2,
            conditions: vec![RoutingCondition::ChannelType(ChannelType::Slack)],
            target_agent: agent_sl,
        });

        let tg_msg = make_msg(ChannelType::Telegram, "u1", "hi");
        assert_eq!(router.route(&tg_msg), Some(agent_tg));

        let sl_msg = make_msg(ChannelType::Slack, "u1", "hi");
        assert_eq!(router.route(&sl_msg), Some(agent_sl));
    }

    #[test]
    fn test_router_command_prefix_rule() {
        let special_agent = Uuid::new_v4();
        let default_agent = Uuid::new_v4();

        let mut router = ChannelRouter::new().with_default_agent(default_agent);
        router.add_rule(RoutingRule {
            priority: 1,
            conditions: vec![RoutingCondition::CommandPrefix("/admin".into())],
            target_agent: special_agent,
        });

        let admin_msg = make_msg(ChannelType::Telegram, "u1", "/admin status");
        assert_eq!(router.route(&admin_msg), Some(special_agent));

        let normal_msg = make_msg(ChannelType::Telegram, "u1", "hello");
        assert_eq!(router.route(&normal_msg), Some(default_agent));
    }
}
