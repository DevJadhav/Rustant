//! Event-driven hooks system for agent lifecycle extensibility.
//!
//! Users can register shell commands that execute on specific agent lifecycle events.
//! Hooks can be blocking (prevent the action) or non-blocking (fire and forget).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Events that can trigger hooks during the agent lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    TaskStart {
        goal: String,
    },
    TaskComplete {
        goal: String,
        success: bool,
    },
    PreToolUse {
        tool_name: String,
        args: serde_json::Value,
    },
    PostToolUse {
        tool_name: String,
        result: String,
        success: bool,
    },
    PreThink {
        message_count: usize,
    },
    PostThink {
        has_tool_call: bool,
    },
    PreCompact {
        message_count: usize,
    },
    ContextOverflow {
        usage_percent: f32,
    },
    SafetyDenial {
        tool_name: String,
        reason: String,
    },
    PersonaSwitch {
        from: String,
        to: String,
    },
    CacheHit {
        provider: String,
        tokens_saved: usize,
    },
    ErrorOccurred {
        error: String,
    },
}

impl HookEvent {
    /// Return the event name for matching against hook definitions.
    pub fn event_name(&self) -> &str {
        match self {
            HookEvent::SessionStart => "session_start",
            HookEvent::SessionEnd => "session_end",
            HookEvent::TaskStart { .. } => "task_start",
            HookEvent::TaskComplete { .. } => "task_complete",
            HookEvent::PreToolUse { .. } => "pre_tool_use",
            HookEvent::PostToolUse { .. } => "post_tool_use",
            HookEvent::PreThink { .. } => "pre_think",
            HookEvent::PostThink { .. } => "post_think",
            HookEvent::PreCompact { .. } => "pre_compact",
            HookEvent::ContextOverflow { .. } => "context_overflow",
            HookEvent::SafetyDenial { .. } => "safety_denial",
            HookEvent::PersonaSwitch { .. } => "persona_switch",
            HookEvent::CacheHit { .. } => "cache_hit",
            HookEvent::ErrorOccurred { .. } => "error_occurred",
        }
    }

    /// Serialize event data to environment variables for shell hooks.
    pub fn to_env_vars(&self) -> Vec<(String, String)> {
        let mut vars = vec![("RUSTANT_HOOK_EVENT".into(), self.event_name().into())];
        match self {
            HookEvent::TaskStart { goal } => {
                vars.push(("RUSTANT_HOOK_GOAL".into(), goal.clone()));
            }
            HookEvent::TaskComplete { goal, success } => {
                vars.push(("RUSTANT_HOOK_GOAL".into(), goal.clone()));
                vars.push(("RUSTANT_HOOK_SUCCESS".into(), success.to_string()));
            }
            HookEvent::PreToolUse { tool_name, args } => {
                vars.push(("RUSTANT_HOOK_TOOL".into(), tool_name.clone()));
                vars.push(("RUSTANT_HOOK_ARGS".into(), args.to_string()));
            }
            HookEvent::PostToolUse {
                tool_name,
                result,
                success,
            } => {
                vars.push(("RUSTANT_HOOK_TOOL".into(), tool_name.clone()));
                vars.push((
                    "RUSTANT_HOOK_RESULT".into(),
                    result[..result.len().min(1000)].to_string(),
                ));
                vars.push(("RUSTANT_HOOK_SUCCESS".into(), success.to_string()));
            }
            HookEvent::SafetyDenial { tool_name, reason } => {
                vars.push(("RUSTANT_HOOK_TOOL".into(), tool_name.clone()));
                vars.push(("RUSTANT_HOOK_REASON".into(), reason.clone()));
            }
            HookEvent::PersonaSwitch { from, to } => {
                vars.push(("RUSTANT_HOOK_FROM".into(), from.clone()));
                vars.push(("RUSTANT_HOOK_TO".into(), to.clone()));
            }
            HookEvent::ContextOverflow { usage_percent } => {
                vars.push((
                    "RUSTANT_HOOK_USAGE_PERCENT".into(),
                    format!("{usage_percent:.1}"),
                ));
            }
            HookEvent::ErrorOccurred { error } => {
                vars.push(("RUSTANT_HOOK_ERROR".into(), error.clone()));
            }
            _ => {}
        }
        vars
    }
}

/// The result of executing a hook.
#[derive(Debug, Clone)]
pub enum HookResult {
    /// Proceed normally.
    Continue,
    /// Modify the event data (e.g., change tool args). Contains JSON patch.
    Modify(serde_json::Value),
    /// Block the action with a reason.
    Block(String),
}

/// A registered hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    /// Which event triggers this hook.
    pub event: String,
    /// Shell command to execute.
    pub command: String,
    /// Timeout in milliseconds (default: 10_000).
    #[serde(default = "default_hook_timeout")]
    pub timeout_ms: u64,
    /// Whether this hook blocks the agent action (default: false).
    #[serde(default)]
    pub blocking: bool,
    /// Whether this hook is currently enabled (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_hook_timeout() -> u64 {
    10_000
}

fn default_true() -> bool {
    true
}

/// Registry of all hooks, keyed by event name.
pub struct HookRegistry {
    hooks: HashMap<String, Vec<HookDefinition>>,
}

impl HookRegistry {
    /// Create an empty hook registry.
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Load hooks from configuration.
    pub fn from_config(hooks: Vec<HookDefinition>) -> Self {
        let mut registry = Self::new();
        for hook in hooks {
            registry.register(hook);
        }
        registry
    }

    /// Register a hook definition.
    pub fn register(&mut self, hook: HookDefinition) {
        self.hooks.entry(hook.event.clone()).or_default().push(hook);
    }

    /// Remove all hooks for a specific event.
    pub fn remove_event(&mut self, event: &str) {
        self.hooks.remove(event);
    }

    /// List all registered hooks.
    pub fn list(&self) -> Vec<&HookDefinition> {
        self.hooks.values().flat_map(|v| v.iter()).collect()
    }

    /// Get the number of registered hooks.
    pub fn count(&self) -> usize {
        self.hooks.values().map(|v| v.len()).sum()
    }

    /// Fire all hooks registered for the given event.
    ///
    /// Returns a list of results. If any blocking hook returns `Block`,
    /// subsequent hooks are not executed and the block result is included.
    pub async fn fire(&self, event: &HookEvent) -> Vec<HookResult> {
        let event_name = event.event_name();
        let hooks = match self.hooks.get(event_name) {
            Some(hooks) => hooks,
            None => return vec![HookResult::Continue],
        };

        let env_vars = event.to_env_vars();
        let mut results = Vec::new();

        for hook in hooks {
            if !hook.enabled {
                continue;
            }

            let timeout = Duration::from_millis(hook.timeout_ms);
            let result = execute_shell_hook(&hook.command, &env_vars, timeout).await;

            match &result {
                HookResult::Block(reason) => {
                    info!(
                        event = event_name,
                        command = hook.command,
                        reason = reason.as_str(),
                        "Hook blocked action"
                    );
                    results.push(result);
                    return results; // Stop processing further hooks
                }
                HookResult::Modify(_) => {
                    debug!(
                        event = event_name,
                        command = hook.command,
                        "Hook modified event data"
                    );
                    results.push(result);
                }
                HookResult::Continue => {
                    debug!(event = event_name, command = hook.command, "Hook completed");
                    results.push(result);
                }
            }
        }

        if results.is_empty() {
            results.push(HookResult::Continue);
        }
        results
    }

    /// Check if any hook would block for the given event (checks blocking hooks only).
    pub fn has_blocking_hooks(&self, event_name: &str) -> bool {
        self.hooks
            .get(event_name)
            .map(|hooks| hooks.iter().any(|h| h.blocking && h.enabled))
            .unwrap_or(false)
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a shell command as a hook and interpret the result.
///
/// - Exit code 0 → `Continue`
/// - Exit code 1 → `Block` (stderr is the reason)
/// - Exit code 2 → `Modify` (stdout is JSON patch)
/// - Timeout → `Continue` (non-blocking hooks) or `Block("hook timed out")`
async fn execute_shell_hook(
    command: &str,
    env_vars: &[(String, String)],
    timeout: Duration,
) -> HookResult {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let result = tokio::time::timeout(timeout, cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            match exit_code {
                0 => HookResult::Continue,
                1 => {
                    let reason = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    HookResult::Block(if reason.is_empty() {
                        "Hook blocked action".into()
                    } else {
                        reason
                    })
                }
                2 => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    match serde_json::from_str(&stdout) {
                        Ok(value) => HookResult::Modify(value),
                        Err(_) => {
                            warn!(
                                command = command,
                                "Hook returned exit code 2 but stdout is not valid JSON"
                            );
                            HookResult::Continue
                        }
                    }
                }
                _ => {
                    warn!(
                        command = command,
                        exit_code, "Hook exited with unexpected code"
                    );
                    HookResult::Continue
                }
            }
        }
        Ok(Err(e)) => {
            warn!(command = command, error = %e, "Hook command failed to execute");
            HookResult::Continue
        }
        Err(_) => {
            warn!(command = command, "Hook timed out");
            HookResult::Block("Hook timed out".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_names() {
        assert_eq!(HookEvent::SessionStart.event_name(), "session_start");
        assert_eq!(HookEvent::SessionEnd.event_name(), "session_end");
        assert_eq!(
            HookEvent::TaskStart {
                goal: "test".into()
            }
            .event_name(),
            "task_start"
        );
        assert_eq!(
            HookEvent::PreToolUse {
                tool_name: "file_read".into(),
                args: serde_json::json!({})
            }
            .event_name(),
            "pre_tool_use"
        );
        assert_eq!(
            HookEvent::SafetyDenial {
                tool_name: "shell_exec".into(),
                reason: "too risky".into()
            }
            .event_name(),
            "safety_denial"
        );
    }

    #[test]
    fn test_hook_event_env_vars() {
        let event = HookEvent::PreToolUse {
            tool_name: "file_read".into(),
            args: serde_json::json!({"path": "/tmp/test"}),
        };
        let vars = event.to_env_vars();
        assert!(
            vars.iter()
                .any(|(k, v)| k == "RUSTANT_HOOK_EVENT" && v == "pre_tool_use")
        );
        assert!(
            vars.iter()
                .any(|(k, v)| k == "RUSTANT_HOOK_TOOL" && v == "file_read")
        );
        assert!(vars.iter().any(|(k, _)| k == "RUSTANT_HOOK_ARGS"));
    }

    #[test]
    fn test_hook_registry_basic() {
        let mut registry = HookRegistry::new();
        assert_eq!(registry.count(), 0);
        assert!(registry.list().is_empty());

        registry.register(HookDefinition {
            event: "pre_tool_use".into(),
            command: "echo pre-hook".into(),
            timeout_ms: 5000,
            blocking: false,
            enabled: true,
        });

        assert_eq!(registry.count(), 1);
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.list()[0].event, "pre_tool_use");
    }

    #[test]
    fn test_hook_registry_from_config() {
        let hooks = vec![
            HookDefinition {
                event: "session_start".into(),
                command: "echo start".into(),
                timeout_ms: 5000,
                blocking: false,
                enabled: true,
            },
            HookDefinition {
                event: "session_end".into(),
                command: "echo end".into(),
                timeout_ms: 5000,
                blocking: false,
                enabled: true,
            },
            HookDefinition {
                event: "pre_tool_use".into(),
                command: "echo pre".into(),
                timeout_ms: 5000,
                blocking: true,
                enabled: true,
            },
        ];
        let registry = HookRegistry::from_config(hooks);
        assert_eq!(registry.count(), 3);
        assert!(registry.has_blocking_hooks("pre_tool_use"));
        assert!(!registry.has_blocking_hooks("session_start"));
    }

    #[test]
    fn test_hook_registry_remove() {
        let mut registry = HookRegistry::new();
        registry.register(HookDefinition {
            event: "session_start".into(),
            command: "echo test".into(),
            timeout_ms: 5000,
            blocking: false,
            enabled: true,
        });
        assert_eq!(registry.count(), 1);

        registry.remove_event("session_start");
        assert_eq!(registry.count(), 0);
    }

    #[tokio::test]
    async fn test_hook_fire_continue() {
        let mut registry = HookRegistry::new();
        registry.register(HookDefinition {
            event: "session_start".into(),
            command: "true".into(), // exit 0
            timeout_ms: 5000,
            blocking: false,
            enabled: true,
        });

        let results = registry.fire(&HookEvent::SessionStart).await;
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], HookResult::Continue));
    }

    #[tokio::test]
    async fn test_hook_fire_block() {
        let mut registry = HookRegistry::new();
        registry.register(HookDefinition {
            event: "pre_tool_use".into(),
            command: "echo 'blocked!' >&2; exit 1".into(),
            timeout_ms: 5000,
            blocking: true,
            enabled: true,
        });

        let event = HookEvent::PreToolUse {
            tool_name: "shell_exec".into(),
            args: serde_json::json!({"command": "rm -rf /"}),
        };
        let results = registry.fire(&event).await;
        assert!(
            matches!(results.last(), Some(HookResult::Block(reason)) if reason.contains("blocked"))
        );
    }

    #[tokio::test]
    async fn test_hook_disabled_skipped() {
        let mut registry = HookRegistry::new();
        registry.register(HookDefinition {
            event: "session_start".into(),
            command: "exit 1".into(), // Would block if enabled
            timeout_ms: 5000,
            blocking: true,
            enabled: false, // Disabled!
        });

        let results = registry.fire(&HookEvent::SessionStart).await;
        assert!(matches!(results[0], HookResult::Continue));
    }

    #[tokio::test]
    async fn test_hook_timeout() {
        let mut registry = HookRegistry::new();
        registry.register(HookDefinition {
            event: "session_start".into(),
            command: "sleep 10".into(),
            timeout_ms: 100, // 100ms timeout
            blocking: true,
            enabled: true,
        });

        let results = registry.fire(&HookEvent::SessionStart).await;
        assert!(matches!(
            results.last(),
            Some(HookResult::Block(reason)) if reason.contains("timed out")
        ));
    }

    #[tokio::test]
    async fn test_hook_no_matching_event() {
        let registry = HookRegistry::new();
        let results = registry.fire(&HookEvent::SessionStart).await;
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], HookResult::Continue));
    }

    #[test]
    fn test_hook_event_serde_roundtrip() {
        let event = HookEvent::TaskComplete {
            goal: "refactor auth".into(),
            success: true,
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: HookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn test_hook_definition_serde() {
        let hook = HookDefinition {
            event: "pre_tool_use".into(),
            command: "echo test".into(),
            timeout_ms: 5000,
            blocking: true,
            enabled: true,
        };
        let json = serde_json::to_string(&hook).unwrap();
        let restored: HookDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.event, "pre_tool_use");
        assert!(restored.blocking);
    }

    #[test]
    fn test_hook_definition_defaults() {
        let json = r#"{"event": "session_start", "command": "echo hi"}"#;
        let hook: HookDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(hook.timeout_ms, 10_000);
        assert!(!hook.blocking);
        assert!(hook.enabled);
    }
}
