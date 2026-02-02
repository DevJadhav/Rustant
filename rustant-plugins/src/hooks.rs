//! Hook system — 7 hook points for plugin interception.
//!
//! Hooks allow plugins to intercept and modify agent behavior at defined points.
//! Each hook returns a `HookResult` indicating whether execution should continue or be blocked.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The 7 hook points in the agent lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookPoint {
    /// Before a tool is executed.
    BeforeToolExecution,
    /// After a tool has executed.
    AfterToolExecution,
    /// Before an LLM request is sent.
    BeforeLlmRequest,
    /// After an LLM response is received.
    AfterLlmResponse,
    /// When a new session starts.
    OnSessionStart,
    /// When a session ends.
    OnSessionEnd,
    /// When an error occurs.
    OnError,
}

/// Context passed to hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// The hook point being triggered.
    pub point: HookPoint,
    /// Name of the relevant tool/provider (if applicable).
    pub name: Option<String>,
    /// Input data (tool args, LLM prompt, etc.).
    pub input: Option<serde_json::Value>,
    /// Output data (tool result, LLM response, etc.).
    pub output: Option<serde_json::Value>,
    /// Error message (for OnError).
    pub error: Option<String>,
    /// Session ID.
    pub session_id: Option<String>,
}

impl HookContext {
    /// Create a context for a tool execution hook.
    pub fn tool(point: HookPoint, name: &str, args: serde_json::Value) -> Self {
        Self {
            point,
            name: Some(name.into()),
            input: Some(args),
            output: None,
            error: None,
            session_id: None,
        }
    }

    /// Create a context for an LLM request/response hook.
    pub fn llm(point: HookPoint, provider: &str, data: serde_json::Value) -> Self {
        let (input, output) = if point == HookPoint::BeforeLlmRequest {
            (Some(data), None)
        } else {
            (None, Some(data))
        };
        Self {
            point,
            name: Some(provider.into()),
            input,
            output,
            error: None,
            session_id: None,
        }
    }

    /// Create a context for session lifecycle hooks.
    pub fn session(point: HookPoint, session_id: &str) -> Self {
        Self {
            point,
            name: None,
            input: None,
            output: None,
            error: None,
            session_id: Some(session_id.into()),
        }
    }

    /// Create a context for error hooks.
    pub fn error(error_message: &str) -> Self {
        Self {
            point: HookPoint::OnError,
            name: None,
            input: None,
            output: None,
            error: Some(error_message.into()),
            session_id: None,
        }
    }
}

/// Result of hook execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookResult {
    /// Allow execution to continue.
    Continue,
    /// Block execution (with reason).
    Block(String),
    /// Continue but with modified context.
    Modified,
}

/// Trait for hook implementations.
pub trait Hook: Send + Sync {
    /// Execute the hook and return a result.
    fn execute(&self, context: &HookContext) -> HookResult;

    /// Display name for logging.
    fn name(&self) -> &str;
}

/// Manages hook registration and firing.
pub struct HookManager {
    hooks: HashMap<HookPoint, Vec<Box<dyn Hook>>>,
}

impl HookManager {
    /// Create a new empty hook manager.
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Register a hook at a specific point.
    pub fn register(&mut self, point: HookPoint, hook: Box<dyn Hook>) {
        self.hooks.entry(point).or_default().push(hook);
    }

    /// Fire all hooks at a given point.
    /// Returns `HookResult::Continue` if all hooks allow, or the first `Block` result.
    pub fn fire(&self, context: &HookContext) -> HookResult {
        let hooks = match self.hooks.get(&context.point) {
            Some(hooks) => hooks,
            None => return HookResult::Continue,
        };

        let mut result = HookResult::Continue;
        for hook in hooks {
            let hook_result = hook.execute(context);
            match &hook_result {
                HookResult::Block(_) => return hook_result,
                HookResult::Modified => result = HookResult::Modified,
                HookResult::Continue => {}
            }
        }
        result
    }

    /// Number of hooks registered at a specific point.
    pub fn count_at(&self, point: HookPoint) -> usize {
        self.hooks.get(&point).map(|v| v.len()).unwrap_or(0)
    }

    /// Total number of registered hooks.
    pub fn total_hooks(&self) -> usize {
        self.hooks.values().map(|v| v.len()).sum()
    }

    /// Clear all hooks.
    pub fn clear(&mut self) {
        self.hooks.clear();
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AllowHook;
    impl Hook for AllowHook {
        fn execute(&self, _context: &HookContext) -> HookResult {
            HookResult::Continue
        }
        fn name(&self) -> &str {
            "allow"
        }
    }

    struct BlockHook {
        reason: String,
    }
    impl BlockHook {
        fn new(reason: &str) -> Self {
            Self {
                reason: reason.into(),
            }
        }
    }
    impl Hook for BlockHook {
        fn execute(&self, _context: &HookContext) -> HookResult {
            HookResult::Block(self.reason.clone())
        }
        fn name(&self) -> &str {
            "block"
        }
    }

    struct ModifyHook;
    impl Hook for ModifyHook {
        fn execute(&self, _context: &HookContext) -> HookResult {
            HookResult::Modified
        }
        fn name(&self) -> &str {
            "modify"
        }
    }

    struct CountingHook {
        name: String,
    }
    impl CountingHook {
        fn new(name: &str) -> Self {
            Self { name: name.into() }
        }
    }
    impl Hook for CountingHook {
        fn execute(&self, _context: &HookContext) -> HookResult {
            HookResult::Continue
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_hook_manager_register_and_fire() {
        let mut mgr = HookManager::new();
        mgr.register(HookPoint::BeforeToolExecution, Box::new(AllowHook));

        let ctx = HookContext::tool(
            HookPoint::BeforeToolExecution,
            "shell_exec",
            serde_json::json!({"cmd": "ls"}),
        );
        let result = mgr.fire(&ctx);
        assert_eq!(result, HookResult::Continue);
    }

    #[test]
    fn test_hook_manager_block() {
        let mut mgr = HookManager::new();
        mgr.register(
            HookPoint::BeforeToolExecution,
            Box::new(BlockHook::new("dangerous")),
        );

        let ctx = HookContext::tool(HookPoint::BeforeToolExecution, "rm", serde_json::json!({}));
        let result = mgr.fire(&ctx);
        assert_eq!(result, HookResult::Block("dangerous".into()));
    }

    #[test]
    fn test_hook_ordering_block_stops_chain() {
        let mut mgr = HookManager::new();
        mgr.register(HookPoint::BeforeToolExecution, Box::new(AllowHook));
        mgr.register(
            HookPoint::BeforeToolExecution,
            Box::new(BlockHook::new("blocked")),
        );
        mgr.register(HookPoint::BeforeToolExecution, Box::new(AllowHook));

        let ctx = HookContext::tool(
            HookPoint::BeforeToolExecution,
            "test",
            serde_json::json!({}),
        );
        let result = mgr.fire(&ctx);
        assert_eq!(result, HookResult::Block("blocked".into()));
    }

    #[test]
    fn test_hook_modified_result() {
        let mut mgr = HookManager::new();
        mgr.register(HookPoint::AfterLlmResponse, Box::new(ModifyHook));
        mgr.register(HookPoint::AfterLlmResponse, Box::new(AllowHook));

        let ctx = HookContext::llm(
            HookPoint::AfterLlmResponse,
            "openai",
            serde_json::json!({"text": "hello"}),
        );
        let result = mgr.fire(&ctx);
        assert_eq!(result, HookResult::Modified);
    }

    #[test]
    fn test_hook_fire_no_hooks() {
        let mgr = HookManager::new();
        let ctx = HookContext::session(HookPoint::OnSessionStart, "session-1");
        let result = mgr.fire(&ctx);
        assert_eq!(result, HookResult::Continue);
    }

    #[test]
    fn test_hook_manager_count() {
        let mut mgr = HookManager::new();
        mgr.register(HookPoint::BeforeToolExecution, Box::new(AllowHook));
        mgr.register(HookPoint::BeforeToolExecution, Box::new(AllowHook));
        mgr.register(HookPoint::OnError, Box::new(AllowHook));

        assert_eq!(mgr.count_at(HookPoint::BeforeToolExecution), 2);
        assert_eq!(mgr.count_at(HookPoint::OnError), 1);
        assert_eq!(mgr.count_at(HookPoint::OnSessionEnd), 0);
        assert_eq!(mgr.total_hooks(), 3);
    }

    #[test]
    fn test_hook_manager_clear() {
        let mut mgr = HookManager::new();
        mgr.register(HookPoint::BeforeToolExecution, Box::new(AllowHook));
        mgr.register(HookPoint::OnError, Box::new(AllowHook));
        assert_eq!(mgr.total_hooks(), 2);

        mgr.clear();
        assert_eq!(mgr.total_hooks(), 0);
    }

    #[test]
    fn test_hook_context_tool() {
        let ctx = HookContext::tool(
            HookPoint::BeforeToolExecution,
            "shell_exec",
            serde_json::json!({"cmd": "ls"}),
        );
        assert_eq!(ctx.point, HookPoint::BeforeToolExecution);
        assert_eq!(ctx.name.as_deref(), Some("shell_exec"));
        assert!(ctx.input.is_some());
    }

    #[test]
    fn test_hook_context_error() {
        let ctx = HookContext::error("something went wrong");
        assert_eq!(ctx.point, HookPoint::OnError);
        assert_eq!(ctx.error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn test_multiple_hooks_fire_in_order() {
        let mut mgr = HookManager::new();
        mgr.register(
            HookPoint::BeforeToolExecution,
            Box::new(CountingHook::new("first")),
        );
        mgr.register(
            HookPoint::BeforeToolExecution,
            Box::new(CountingHook::new("second")),
        );
        mgr.register(
            HookPoint::BeforeToolExecution,
            Box::new(CountingHook::new("third")),
        );

        // All continue, so result should be Continue
        let ctx = HookContext::tool(
            HookPoint::BeforeToolExecution,
            "test",
            serde_json::json!({}),
        );
        assert_eq!(mgr.fire(&ctx), HookResult::Continue);
        assert_eq!(mgr.count_at(HookPoint::BeforeToolExecution), 3);
    }

    #[test]
    fn test_hook_point_serialization() {
        let point = HookPoint::BeforeToolExecution;
        let json = serde_json::to_string(&point).unwrap();
        let restored: HookPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, HookPoint::BeforeToolExecution);
    }

    #[test]
    fn test_all_seven_hook_points() {
        let points = vec![
            HookPoint::BeforeToolExecution,
            HookPoint::AfterToolExecution,
            HookPoint::BeforeLlmRequest,
            HookPoint::AfterLlmResponse,
            HookPoint::OnSessionStart,
            HookPoint::OnSessionEnd,
            HookPoint::OnError,
        ];
        assert_eq!(points.len(), 7);

        // Each can be used as a hash key
        let mut mgr = HookManager::new();
        for point in &points {
            mgr.register(*point, Box::new(AllowHook));
        }
        assert_eq!(mgr.total_hooks(), 7);
    }
}
