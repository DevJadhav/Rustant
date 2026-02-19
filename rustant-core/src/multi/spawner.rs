//! Agent spawner — lifecycle management for agents.
//!
//! Manages creating and terminating agents, enforces limits, and tracks
//! parent-child relationships for hierarchical agent spawning.

use super::isolation::{AgentContext, AgentStatus, ResourceLimits};
use crate::config::SafetyConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Configuration for multi-agent spawning.
#[derive(Debug, Clone)]
pub struct SpawnerConfig {
    /// Maximum number of concurrent agents.
    pub max_agents: usize,
    /// Default window size for agent memory.
    pub default_window_size: usize,
    /// Default safety config applied to spawned agents.
    pub default_safety: SafetyConfig,
}

impl Default for SpawnerConfig {
    fn default() -> Self {
        Self {
            max_agents: 8,
            default_window_size: 10,
            default_safety: SafetyConfig::default(),
        }
    }
}

/// Manages agent lifecycle — spawn, terminate, query.
pub struct AgentSpawner {
    config: SpawnerConfig,
    contexts: HashMap<Uuid, AgentContext>,
}

impl AgentSpawner {
    pub fn new(config: SpawnerConfig) -> Self {
        Self {
            config,
            contexts: HashMap::new(),
        }
    }

    /// Spawn a new top-level agent. Returns the agent ID, or an error if limit reached.
    pub fn spawn(&mut self, name: impl Into<String>) -> Result<Uuid, String> {
        if self.contexts.len() >= self.config.max_agents {
            return Err(format!(
                "Agent limit reached (max {})",
                self.config.max_agents
            ));
        }

        let ctx = AgentContext::new(
            name,
            self.config.default_window_size,
            self.config.default_safety.clone(),
        );
        let id = ctx.agent_id;
        self.contexts.insert(id, ctx);
        Ok(id)
    }

    /// Spawn a child agent under a parent. Returns the child's ID.
    pub fn spawn_child(
        &mut self,
        name: impl Into<String>,
        parent_id: Uuid,
    ) -> Result<Uuid, String> {
        if !self.contexts.contains_key(&parent_id) {
            return Err(format!("Parent agent {parent_id} not found"));
        }
        if self.contexts.len() >= self.config.max_agents {
            return Err(format!(
                "Agent limit reached (max {})",
                self.config.max_agents
            ));
        }

        let ctx = AgentContext::new_child(
            name,
            parent_id,
            self.config.default_window_size,
            self.config.default_safety.clone(),
        );
        let id = ctx.agent_id;
        self.contexts.insert(id, ctx);
        Ok(id)
    }

    /// Terminate an agent and all its children. Returns number of agents removed.
    pub fn terminate(&mut self, agent_id: Uuid) -> usize {
        let children = self.children_of(agent_id);
        let mut count = 0;

        // Recursively terminate children first
        for child_id in children {
            count += self.terminate(child_id);
        }

        if self.contexts.remove(&agent_id).is_some() {
            count += 1;
        }
        count
    }

    /// Get a reference to an agent's context.
    pub fn get(&self, agent_id: &Uuid) -> Option<&AgentContext> {
        self.contexts.get(agent_id)
    }

    /// Get a mutable reference to an agent's context.
    pub fn get_mut(&mut self, agent_id: &Uuid) -> Option<&mut AgentContext> {
        self.contexts.get_mut(agent_id)
    }

    /// Find all direct children of a given agent.
    pub fn children_of(&self, parent_id: Uuid) -> Vec<Uuid> {
        self.contexts
            .iter()
            .filter(|(_, ctx)| ctx.parent_id == Some(parent_id))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Total number of active agents.
    pub fn agent_count(&self) -> usize {
        self.contexts.len()
    }

    /// List all active agent IDs.
    pub fn agent_ids(&self) -> Vec<Uuid> {
        self.contexts.keys().copied().collect()
    }

    /// Spawn an agent with custom configuration.
    pub fn spawn_with_config(
        &mut self,
        name: impl Into<String>,
        workspace_dir: Option<PathBuf>,
        llm_override: Option<String>,
        resource_limits: ResourceLimits,
    ) -> Result<Uuid, String> {
        if self.contexts.len() >= self.config.max_agents {
            return Err(format!(
                "Agent limit reached (max {})",
                self.config.max_agents
            ));
        }

        let mut ctx = AgentContext::new(
            name,
            self.config.default_window_size,
            self.config.default_safety.clone(),
        );
        ctx.workspace_dir = workspace_dir;
        ctx.llm_override = llm_override;
        ctx.resource_limits = resource_limits;
        let id = ctx.agent_id;
        self.contexts.insert(id, ctx);
        Ok(id)
    }

    /// Get the status of an agent.
    pub fn get_status(&self, agent_id: &Uuid) -> Option<AgentStatus> {
        self.contexts.get(agent_id).map(|ctx| ctx.status)
    }

    /// Set the status of an agent.
    pub fn set_status(&mut self, agent_id: &Uuid, status: AgentStatus) -> Result<(), String> {
        let ctx = self
            .contexts
            .get_mut(agent_id)
            .ok_or_else(|| format!("Agent {agent_id} not found"))?;
        ctx.status = status;
        Ok(())
    }

    /// List all agents with a given status.
    pub fn list_by_status(&self, status: AgentStatus) -> Vec<Uuid> {
        self.contexts
            .iter()
            .filter(|(_, ctx)| ctx.status == status)
            .map(|(id, _)| *id)
            .collect()
    }
}

impl Default for AgentSpawner {
    fn default() -> Self {
        Self::new(SpawnerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawner_new_empty() {
        let spawner = AgentSpawner::default();
        assert_eq!(spawner.agent_count(), 0);
    }

    #[test]
    fn test_spawner_spawn() {
        let mut spawner = AgentSpawner::default();
        let id = spawner.spawn("agent-1").unwrap();
        assert_eq!(spawner.agent_count(), 1);
        assert!(spawner.get(&id).is_some());
        assert_eq!(spawner.get(&id).unwrap().name, "agent-1");
    }

    #[test]
    fn test_spawner_max_limit() {
        let config = SpawnerConfig {
            max_agents: 2,
            ..Default::default()
        };
        let mut spawner = AgentSpawner::new(config);
        spawner.spawn("a1").unwrap();
        spawner.spawn("a2").unwrap();
        let result = spawner.spawn("a3");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("limit reached"));
    }

    #[test]
    fn test_spawner_spawn_child() {
        let mut spawner = AgentSpawner::default();
        let parent = spawner.spawn("parent").unwrap();
        let child = spawner.spawn_child("child", parent).unwrap();

        assert_eq!(spawner.agent_count(), 2);
        assert!(spawner.get(&child).unwrap().is_child());
        assert_eq!(spawner.get(&child).unwrap().parent_id, Some(parent));
    }

    #[test]
    fn test_spawner_spawn_child_missing_parent() {
        let mut spawner = AgentSpawner::default();
        let fake_parent = Uuid::new_v4();
        let result = spawner.spawn_child("orphan", fake_parent);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_spawner_terminate() {
        let mut spawner = AgentSpawner::default();
        let id = spawner.spawn("agent").unwrap();
        assert_eq!(spawner.agent_count(), 1);

        let removed = spawner.terminate(id);
        assert_eq!(removed, 1);
        assert_eq!(spawner.agent_count(), 0);
    }

    #[test]
    fn test_spawner_terminate_cascades_to_children() {
        let mut spawner = AgentSpawner::default();
        let parent = spawner.spawn("parent").unwrap();
        let _child1 = spawner.spawn_child("child1", parent).unwrap();
        let _child2 = spawner.spawn_child("child2", parent).unwrap();
        assert_eq!(spawner.agent_count(), 3);

        let removed = spawner.terminate(parent);
        assert_eq!(removed, 3);
        assert_eq!(spawner.agent_count(), 0);
    }

    #[test]
    fn test_spawner_children_of() {
        let mut spawner = AgentSpawner::default();
        let parent = spawner.spawn("parent").unwrap();
        let c1 = spawner.spawn_child("c1", parent).unwrap();
        let c2 = spawner.spawn_child("c2", parent).unwrap();
        let _other = spawner.spawn("other").unwrap();

        let children = spawner.children_of(parent);
        assert_eq!(children.len(), 2);
        assert!(children.contains(&c1));
        assert!(children.contains(&c2));
    }

    #[test]
    fn test_spawn_with_custom_config() {
        let mut spawner = AgentSpawner::default();
        let limits = ResourceLimits {
            max_memory_mb: Some(256),
            max_tokens_per_turn: Some(2048),
            max_tool_calls: Some(20),
            max_runtime_secs: Some(120),
        };
        let id = spawner
            .spawn_with_config(
                "custom",
                Some(PathBuf::from("/tmp/workspace")),
                Some("claude-3-sonnet".into()),
                limits,
            )
            .unwrap();

        let ctx = spawner.get(&id).unwrap();
        assert_eq!(
            ctx.workspace_dir.as_deref(),
            Some(std::path::Path::new("/tmp/workspace"))
        );
        assert_eq!(ctx.llm_override.as_deref(), Some("claude-3-sonnet"));
        assert_eq!(ctx.resource_limits.max_memory_mb, Some(256));
    }

    #[test]
    fn test_get_set_status() {
        let mut spawner = AgentSpawner::default();
        let id = spawner.spawn("agent").unwrap();

        assert_eq!(spawner.get_status(&id), Some(AgentStatus::Idle));

        spawner.set_status(&id, AgentStatus::Running).unwrap();
        assert_eq!(spawner.get_status(&id), Some(AgentStatus::Running));

        spawner.set_status(&id, AgentStatus::Terminated).unwrap();
        assert_eq!(spawner.get_status(&id), Some(AgentStatus::Terminated));
    }

    #[test]
    fn test_list_by_status() {
        let mut spawner = AgentSpawner::default();
        let a1 = spawner.spawn("a1").unwrap();
        let a2 = spawner.spawn("a2").unwrap();
        let _a3 = spawner.spawn("a3").unwrap();

        spawner.set_status(&a1, AgentStatus::Running).unwrap();
        spawner.set_status(&a2, AgentStatus::Running).unwrap();

        let running = spawner.list_by_status(AgentStatus::Running);
        assert_eq!(running.len(), 2);
        assert!(running.contains(&a1));
        assert!(running.contains(&a2));

        let idle = spawner.list_by_status(AgentStatus::Idle);
        assert_eq!(idle.len(), 1);
    }

    #[test]
    fn test_set_status_unknown_agent() {
        let mut spawner = AgentSpawner::default();
        let fake = Uuid::new_v4();
        let result = spawner.set_status(&fake, AgentStatus::Running);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
