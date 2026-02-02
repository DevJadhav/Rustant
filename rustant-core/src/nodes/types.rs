//! Node system types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn random() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Information about a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub name: String,
    pub platform: Platform,
    pub hostname: String,
    pub registered_at: DateTime<Utc>,
    #[serde(default)]
    pub os_version: Option<String>,
    #[serde(default = "default_agent_version")]
    pub agent_version: String,
    #[serde(default)]
    pub uptime_secs: u64,
}

fn default_agent_version() -> String {
    "0.1.0".to_string()
}

/// Platform a node runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
    Ios,
    Android,
    Unknown,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MacOS => write!(f, "macos"),
            Self::Linux => write!(f, "linux"),
            Self::Windows => write!(f, "windows"),
            Self::Ios => write!(f, "ios"),
            Self::Android => write!(f, "android"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// A capability that a node can provide.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    FileSystem,
    Shell,
    AppleScript,
    Automator,
    Screenshot,
    Clipboard,
    Notifications,
    Browser,
    Camera,
    ScreenRecord,
    Location,
    AppControl(String),
    Custom(String),
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileSystem => write!(f, "filesystem"),
            Self::Shell => write!(f, "shell"),
            Self::AppleScript => write!(f, "applescript"),
            Self::Automator => write!(f, "automator"),
            Self::Screenshot => write!(f, "screenshot"),
            Self::Clipboard => write!(f, "clipboard"),
            Self::Notifications => write!(f, "notifications"),
            Self::Browser => write!(f, "browser"),
            Self::Camera => write!(f, "camera"),
            Self::ScreenRecord => write!(f, "screen_record"),
            Self::Location => write!(f, "location"),
            Self::AppControl(app) => write!(f, "app_control:{}", app),
            Self::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

/// Health status of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeHealth {
    Healthy,
    Degraded,
    Unreachable,
    Unknown,
}

/// A task to execute on a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTask {
    pub task_id: Uuid,
    pub capability: Capability,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_secs: u64,
}

impl NodeTask {
    pub fn new(capability: Capability, command: impl Into<String>) -> Self {
        Self {
            task_id: Uuid::new_v4(),
            capability,
            command: command.into(),
            args: Vec::new(),
            timeout_secs: 30,
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// Result of a node task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub task_id: Uuid,
    pub success: bool,
    pub output: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// Rate limit for a node capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    pub max_calls: u32,
    pub window_secs: u64,
}

/// Rich capability description for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapability {
    pub capability: Capability,
    pub description: String,
    pub requires_consent: bool,
    pub rate_limit: Option<RateLimit>,
    pub parameters: std::collections::HashMap<String, String>,
}

impl NodeCapability {
    /// Create a basic NodeCapability from a Capability.
    pub fn basic(capability: Capability) -> Self {
        Self {
            description: capability.to_string(),
            capability,
            requires_consent: true,
            rate_limit: None,
            parameters: std::collections::HashMap::new(),
        }
    }
}

/// Inter-node protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeMessage {
    Ping,
    Pong { uptime_secs: u64 },
    TaskForward { task: NodeTask, from_node: NodeId },
    TaskResponse { result: NodeResult, from_node: NodeId },
    CapabilityQuery,
    CapabilityResponse { capabilities: Vec<NodeCapability> },
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id() {
        let id = NodeId::new("node-1");
        assert_eq!(id.to_string(), "node-1");
        let random = NodeId::random();
        assert!(!random.0.is_empty());
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::MacOS.to_string(), "macos");
        assert_eq!(Platform::Linux.to_string(), "linux");
        assert_eq!(Platform::Windows.to_string(), "windows");
    }

    #[test]
    fn test_capability_display() {
        assert_eq!(Capability::Shell.to_string(), "shell");
        assert_eq!(Capability::AppleScript.to_string(), "applescript");
        assert_eq!(Capability::Custom("gpu".into()).to_string(), "custom:gpu");
    }

    #[test]
    fn test_node_task_construction() {
        let task = NodeTask::new(Capability::Shell, "ls")
            .with_args(vec!["-la".into()])
            .with_timeout(10);
        assert_eq!(task.command, "ls");
        assert_eq!(task.args, vec!["-la"]);
        assert_eq!(task.timeout_secs, 10);
    }

    #[test]
    fn test_node_types_serialization() {
        let health = NodeHealth::Healthy;
        let json = serde_json::to_string(&health).unwrap();
        let restored: NodeHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, NodeHealth::Healthy);

        let cap = Capability::FileSystem;
        let json = serde_json::to_string(&cap).unwrap();
        let restored: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, Capability::FileSystem);
    }

    #[test]
    fn test_platform_ios_android() {
        assert_eq!(Platform::Ios.to_string(), "ios");
        assert_eq!(Platform::Android.to_string(), "android");
        assert_ne!(Platform::Ios, Platform::Android);
    }

    #[test]
    fn test_capability_camera_screenrecord_location() {
        assert_eq!(Capability::Camera.to_string(), "camera");
        assert_eq!(Capability::ScreenRecord.to_string(), "screen_record");
        assert_eq!(Capability::Location.to_string(), "location");
    }

    #[test]
    fn test_capability_app_control() {
        let cap = Capability::AppControl("Safari".into());
        assert_eq!(cap.to_string(), "app_control:Safari");
        assert_ne!(cap, Capability::AppControl("Chrome".into()));
    }

    #[test]
    fn test_node_capability_struct() {
        let nc = NodeCapability::basic(Capability::Shell);
        assert_eq!(nc.capability, Capability::Shell);
        assert!(nc.requires_consent);
        assert!(nc.rate_limit.is_none());
        assert!(nc.parameters.is_empty());
        assert_eq!(nc.description, "shell");
    }

    #[test]
    fn test_rate_limit() {
        let rl = RateLimit {
            max_calls: 10,
            window_secs: 60,
        };
        assert_eq!(rl.max_calls, 10);
        assert_eq!(rl.window_secs, 60);

        let json = serde_json::to_string(&rl).unwrap();
        let restored: RateLimit = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, rl);
    }

    #[test]
    fn test_node_message_ping_pong() {
        let ping = NodeMessage::Ping;
        let pong = NodeMessage::Pong { uptime_secs: 3600 };
        let json = serde_json::to_string(&ping).unwrap();
        assert!(json.contains("Ping"));
        let json = serde_json::to_string(&pong).unwrap();
        assert!(json.contains("3600"));
    }

    #[test]
    fn test_node_message_task_forward() {
        let task = NodeTask::new(Capability::Shell, "ls");
        let msg = NodeMessage::TaskForward {
            task,
            from_node: NodeId::new("sender"),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("sender"));
    }

    #[test]
    fn test_node_message_capability_query() {
        let query = NodeMessage::CapabilityQuery;
        let response = NodeMessage::CapabilityResponse {
            capabilities: vec![NodeCapability::basic(Capability::Shell)],
        };
        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("CapabilityQuery"));
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("shell"));
    }

    #[test]
    fn test_node_message_shutdown() {
        let msg = NodeMessage::Shutdown;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Shutdown"));
    }

    #[test]
    fn test_node_info_expanded() {
        let info = NodeInfo {
            node_id: NodeId::new("n1"),
            name: "test".into(),
            platform: Platform::MacOS,
            hostname: "host".into(),
            registered_at: Utc::now(),
            os_version: Some("14.2".into()),
            agent_version: "0.2.0".into(),
            uptime_secs: 7200,
        };
        assert_eq!(info.os_version.as_deref(), Some("14.2"));
        assert_eq!(info.agent_version, "0.2.0");
        assert_eq!(info.uptime_secs, 7200);
    }
}
