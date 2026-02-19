//! Agent Teams — coordinated multi-agent task execution.
//!
//! Teams define groups of agents with specific roles and coordination strategies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A team of agents that coordinate on tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTeam {
    /// Team name.
    pub name: String,
    /// Team members.
    pub members: Vec<TeamMember>,
    /// Shared context for the team.
    #[serde(default)]
    pub shared_context: SharedContext,
    /// How team members coordinate.
    #[serde(default)]
    pub coordination: CoordinationStrategy,
}

/// A member of an agent team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    /// Unique identifier for this member.
    pub agent_id: String,
    /// Role within the team (e.g., "lead", "reviewer", "implementer").
    pub role: String,
    /// Persona to use for this member.
    #[serde(default)]
    pub persona: Option<String>,
    /// LLM provider to use (can differ per member).
    #[serde(default)]
    pub provider: Option<String>,
}

/// Shared context accessible by all team members.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharedContext {
    /// Known facts shared across the team.
    #[serde(default)]
    pub facts: Vec<String>,
    /// Decisions made during task execution.
    #[serde(default)]
    pub decisions: Vec<TeamDecision>,
    /// Task board for tracking team work.
    #[serde(default)]
    pub tasks: Vec<TeamTask>,
}

/// A decision recorded in the shared context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDecision {
    /// Who made the decision.
    pub made_by: String,
    /// What was decided.
    pub description: String,
    /// Rationale for the decision.
    pub rationale: String,
}

/// A task on the team's task board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    /// Task identifier.
    pub id: String,
    /// Task description.
    pub description: String,
    /// Assigned team member.
    #[serde(default)]
    pub assigned_to: Option<String>,
    /// Task status.
    #[serde(default)]
    pub status: TeamTaskStatus,
    /// Result from completing the task.
    #[serde(default)]
    pub result: Option<String>,
}

/// Status of a team task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamTaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// How team members coordinate their work.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationStrategy {
    /// One agent at a time, passing results forward.
    #[default]
    Sequential,
    /// All agents work simultaneously on the same task.
    Parallel,
    /// Implementer → Reviewer → Lead approves.
    ReviewChain,
    /// Planner → Executor → Verifier.
    PlanExecuteVerify,
}

/// Registry of defined agent teams.
#[derive(Debug, Default)]
pub struct TeamRegistry {
    teams: HashMap<String, AgentTeam>,
}

impl TeamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a team.
    pub fn register(&mut self, team: AgentTeam) {
        self.teams.insert(team.name.clone(), team);
    }

    /// Get a team by name.
    pub fn get(&self, name: &str) -> Option<&AgentTeam> {
        self.teams.get(name)
    }

    /// Get a mutable team by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut AgentTeam> {
        self.teams.get_mut(name)
    }

    /// Remove a team.
    pub fn remove(&mut self, name: &str) -> Option<AgentTeam> {
        self.teams.remove(name)
    }

    /// List all team names.
    pub fn list_names(&self) -> Vec<&str> {
        self.teams.keys().map(|k| k.as_str()).collect()
    }

    /// Number of registered teams.
    pub fn len(&self) -> usize {
        self.teams.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.teams.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_creation() {
        let team = AgentTeam {
            name: "review-team".into(),
            members: vec![
                TeamMember {
                    agent_id: "architect".into(),
                    role: "lead".into(),
                    persona: Some("Architect".into()),
                    provider: Some("anthropic".into()),
                },
                TeamMember {
                    agent_id: "security".into(),
                    role: "reviewer".into(),
                    persona: Some("SecurityGuardian".into()),
                    provider: Some("openai".into()),
                },
            ],
            shared_context: SharedContext::default(),
            coordination: CoordinationStrategy::ReviewChain,
        };

        assert_eq!(team.members.len(), 2);
        assert!(matches!(
            team.coordination,
            CoordinationStrategy::ReviewChain
        ));
    }

    #[test]
    fn test_team_registry() {
        let mut registry = TeamRegistry::new();
        assert!(registry.is_empty());

        registry.register(AgentTeam {
            name: "test-team".into(),
            members: vec![TeamMember {
                agent_id: "agent1".into(),
                role: "lead".into(),
                persona: None,
                provider: None,
            }],
            shared_context: SharedContext::default(),
            coordination: CoordinationStrategy::Sequential,
        });

        assert_eq!(registry.len(), 1);
        assert!(registry.get("test-team").is_some());
        assert_eq!(registry.list_names().len(), 1);

        registry.remove("test-team");
        assert!(registry.is_empty());
    }

    #[test]
    fn test_shared_context() {
        let mut ctx = SharedContext::default();
        ctx.facts.push("Project uses Rust 2024 edition".into());
        ctx.decisions.push(TeamDecision {
            made_by: "architect".into(),
            description: "Use async-trait for LLM provider".into(),
            rationale: "Trait objects need Send+Sync".into(),
        });
        ctx.tasks.push(TeamTask {
            id: "1".into(),
            description: "Implement thinking support".into(),
            assigned_to: Some("implementer".into()),
            status: TeamTaskStatus::InProgress,
            result: None,
        });

        assert_eq!(ctx.facts.len(), 1);
        assert_eq!(ctx.decisions.len(), 1);
        assert_eq!(ctx.tasks.len(), 1);
    }

    #[test]
    fn test_coordination_strategy_serde() {
        let strats = vec![
            CoordinationStrategy::Sequential,
            CoordinationStrategy::Parallel,
            CoordinationStrategy::ReviewChain,
            CoordinationStrategy::PlanExecuteVerify,
        ];
        for strat in strats {
            let json = serde_json::to_string(&strat).unwrap();
            let restored: CoordinationStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{strat:?}"), format!("{:?}", restored));
        }
    }

    #[test]
    fn test_team_task_status_default() {
        let status = TeamTaskStatus::default();
        assert_eq!(status, TeamTaskStatus::Pending);
    }

    #[test]
    fn test_team_serde_roundtrip() {
        let team = AgentTeam {
            name: "serde-test".into(),
            members: vec![TeamMember {
                agent_id: "a1".into(),
                role: "lead".into(),
                persona: Some("General".into()),
                provider: None,
            }],
            shared_context: SharedContext::default(),
            coordination: CoordinationStrategy::PlanExecuteVerify,
        };
        let json = serde_json::to_string(&team).unwrap();
        let restored: AgentTeam = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "serde-test");
        assert_eq!(restored.members.len(), 1);
    }
}
