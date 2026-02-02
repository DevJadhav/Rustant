//! Agent isolation — each agent gets its own memory and safety context.
//!
//! `AgentContext` bundles a unique ID, name, memory system, safety guardian,
//! and optional parent reference, ensuring agents cannot interfere with
//! each other's state.

use crate::config::SafetyConfig;
use crate::memory::MemorySystem;
use crate::safety::SafetyGuardian;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Resource limits for an agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_tokens_per_turn: Option<u64>,
    pub max_tool_calls: Option<u32>,
    pub max_runtime_secs: Option<u64>,
}

/// Status of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Running,
    Waiting,
    Terminated,
}

/// Isolated execution context for a single agent.
pub struct AgentContext {
    /// Unique identifier for this agent.
    pub agent_id: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Dedicated memory system (not shared with other agents).
    pub memory: MemorySystem,
    /// Dedicated safety guardian.
    pub safety: SafetyGuardian,
    /// Parent agent ID, if this agent was spawned by another.
    pub parent_id: Option<Uuid>,
    /// Per-agent sandbox working directory.
    pub workspace_dir: Option<PathBuf>,
    /// Per-agent LLM model override.
    pub llm_override: Option<String>,
    /// Per-agent resource constraints.
    pub resource_limits: ResourceLimits,
    /// When this agent was created.
    pub created_at: DateTime<Utc>,
    /// Current status.
    pub status: AgentStatus,
}

impl AgentContext {
    /// Create a new agent context with the given name.
    pub fn new(name: impl Into<String>, window_size: usize, safety_config: SafetyConfig) -> Self {
        Self {
            agent_id: Uuid::new_v4(),
            name: name.into(),
            memory: MemorySystem::new(window_size),
            safety: SafetyGuardian::new(safety_config),
            parent_id: None,
            workspace_dir: None,
            llm_override: None,
            resource_limits: ResourceLimits::default(),
            created_at: Utc::now(),
            status: AgentStatus::Idle,
        }
    }

    /// Create a child context, linking back to a parent agent.
    pub fn new_child(
        name: impl Into<String>,
        parent_id: Uuid,
        window_size: usize,
        safety_config: SafetyConfig,
    ) -> Self {
        Self {
            agent_id: Uuid::new_v4(),
            name: name.into(),
            memory: MemorySystem::new(window_size),
            safety: SafetyGuardian::new(safety_config),
            parent_id: Some(parent_id),
            workspace_dir: None,
            llm_override: None,
            resource_limits: ResourceLimits::default(),
            created_at: Utc::now(),
            status: AgentStatus::Idle,
        }
    }

    /// Whether this context belongs to a child agent.
    pub fn is_child(&self) -> bool {
        self.parent_id.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SafetyConfig;

    #[test]
    fn test_agent_context_new() {
        let ctx = AgentContext::new("test-agent", 10, SafetyConfig::default());
        assert_eq!(ctx.name, "test-agent");
        assert!(!ctx.is_child());
        assert!(ctx.parent_id.is_none());
    }

    #[test]
    fn test_agent_context_child() {
        let parent_id = Uuid::new_v4();
        let ctx = AgentContext::new_child("child-agent", parent_id, 10, SafetyConfig::default());
        assert!(ctx.is_child());
        assert_eq!(ctx.parent_id, Some(parent_id));
    }

    #[test]
    fn test_agent_context_isolation() {
        // Two agents should have completely separate memory systems.
        let mut ctx1 = AgentContext::new("agent-1", 10, SafetyConfig::default());
        let mut ctx2 = AgentContext::new("agent-2", 10, SafetyConfig::default());

        ctx1.memory.working.set_goal("Goal A");
        ctx2.memory.working.set_goal("Goal B");

        assert_eq!(
            ctx1.memory.working.current_goal.as_deref(),
            Some("Goal A")
        );
        assert_eq!(
            ctx2.memory.working.current_goal.as_deref(),
            Some("Goal B")
        );
        assert_ne!(ctx1.agent_id, ctx2.agent_id);
    }

    #[test]
    fn test_agent_context_unique_ids() {
        let a = AgentContext::new("a", 5, SafetyConfig::default());
        let b = AgentContext::new("b", 5, SafetyConfig::default());
        assert_ne!(a.agent_id, b.agent_id);
    }

    #[test]
    fn test_agent_context_with_workspace() {
        let mut ctx = AgentContext::new("test", 10, SafetyConfig::default());
        assert!(ctx.workspace_dir.is_none());
        ctx.workspace_dir = Some(PathBuf::from("/tmp/agent-workspace"));
        assert_eq!(ctx.workspace_dir.as_deref(), Some(std::path::Path::new("/tmp/agent-workspace")));
    }

    #[test]
    fn test_agent_context_with_llm_override() {
        let mut ctx = AgentContext::new("test", 10, SafetyConfig::default());
        assert!(ctx.llm_override.is_none());
        ctx.llm_override = Some("claude-3-opus".into());
        assert_eq!(ctx.llm_override.as_deref(), Some("claude-3-opus"));
    }

    #[test]
    fn test_resource_limits_default_unbounded() {
        let limits = ResourceLimits::default();
        assert!(limits.max_memory_mb.is_none());
        assert!(limits.max_tokens_per_turn.is_none());
        assert!(limits.max_tool_calls.is_none());
        assert!(limits.max_runtime_secs.is_none());
    }

    #[test]
    fn test_resource_limits_custom() {
        let limits = ResourceLimits {
            max_memory_mb: Some(512),
            max_tokens_per_turn: Some(4096),
            max_tool_calls: Some(50),
            max_runtime_secs: Some(300),
        };
        assert_eq!(limits.max_memory_mb, Some(512));
        assert_eq!(limits.max_tool_calls, Some(50));
    }

    #[test]
    fn test_agent_status_transitions() {
        let mut ctx = AgentContext::new("test", 10, SafetyConfig::default());
        assert_eq!(ctx.status, AgentStatus::Idle);
        ctx.status = AgentStatus::Running;
        assert_eq!(ctx.status, AgentStatus::Running);
        ctx.status = AgentStatus::Waiting;
        assert_eq!(ctx.status, AgentStatus::Waiting);
        ctx.status = AgentStatus::Terminated;
        assert_eq!(ctx.status, AgentStatus::Terminated);
    }

    #[test]
    fn test_agent_context_created_at() {
        let before = chrono::Utc::now();
        let ctx = AgentContext::new("test", 10, SafetyConfig::default());
        let after = chrono::Utc::now();
        assert!(ctx.created_at >= before);
        assert!(ctx.created_at <= after);
    }

    #[test]
    fn test_new_child_inherits_defaults() {
        let parent_id = Uuid::new_v4();
        let ctx = AgentContext::new_child("child", parent_id, 10, SafetyConfig::default());
        assert!(ctx.workspace_dir.is_none());
        assert!(ctx.llm_override.is_none());
        assert!(ctx.resource_limits.max_memory_mb.is_none());
        assert_eq!(ctx.status, AgentStatus::Idle);
    }

    #[test]
    fn test_resource_limits_none_means_unlimited() {
        let limits = ResourceLimits {
            max_memory_mb: None,
            max_tokens_per_turn: None,
            max_tool_calls: None,
            max_runtime_secs: None,
        };
        // All None means no limits applied
        assert!(limits.max_memory_mb.is_none());
        assert!(limits.max_tokens_per_turn.is_none());
        assert!(limits.max_tool_calls.is_none());
        assert!(limits.max_runtime_secs.is_none());
    }
}
