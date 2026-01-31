//! Sandbox configuration types and capability definitions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Capability
// ---------------------------------------------------------------------------

/// A capability that can be granted to sandboxed code.
///
/// Each variant restricts what the WASM module may access. Paths, hosts, and
/// environment variable names are provided as allow-lists so only explicitly
/// permitted resources are reachable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Read access to specific file-system paths.
    FileRead(Vec<PathBuf>),
    /// Write access to specific file-system paths.
    FileWrite(Vec<PathBuf>),
    /// Network access to specific hosts or URLs.
    NetworkAccess(Vec<String>),
    /// Read access to specific environment variables.
    EnvironmentRead(Vec<String>),
    /// Permission to write to stdout.
    Stdout,
    /// Permission to write to stderr.
    Stderr,
}

// ---------------------------------------------------------------------------
// ResourceLimits
// ---------------------------------------------------------------------------

/// Resource constraints applied to a sandbox execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum linear memory in bytes (default: 16 MiB).
    pub max_memory_bytes: usize,
    /// Instruction fuel budget (default: 1,000,000).
    pub max_fuel: u64,
    /// Wall-clock execution time limit (default: 30 s).
    #[serde(with = "duration_serde")]
    pub max_execution_time: Duration,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 16 * 1024 * 1024, // 16 MiB
            max_fuel: 1_000_000,
            max_execution_time: Duration::from_secs(30),
        }
    }
}

// ---------------------------------------------------------------------------
// SandboxConfig
// ---------------------------------------------------------------------------

/// Configuration for a WASM sandbox execution environment.
///
/// Use the builder methods to customise limits and capabilities:
///
/// ```rust
/// use rustant_tools::sandbox::config::{SandboxConfig, Capability};
/// use std::time::Duration;
///
/// let config = SandboxConfig::new()
///     .with_memory_limit(32 * 1024 * 1024)
///     .with_fuel_limit(2_000_000)
///     .with_timeout(Duration::from_secs(60))
///     .with_capability(Capability::Stdout)
///     .allow_host_calls();
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Resource limits governing memory, fuel, and wall-clock time.
    pub resource_limits: ResourceLimits,
    /// Capabilities granted to the sandboxed module.
    pub capabilities: Vec<Capability>,
    /// Whether the module may invoke host functions.
    pub allow_host_calls: bool,
}

impl SandboxConfig {
    /// Create a new `SandboxConfig` with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum linear memory in bytes.
    pub fn with_memory_limit(mut self, bytes: usize) -> Self {
        self.resource_limits.max_memory_bytes = bytes;
        self
    }

    /// Set the instruction fuel budget.
    pub fn with_fuel_limit(mut self, fuel: u64) -> Self {
        self.resource_limits.max_fuel = fuel;
        self
    }

    /// Set the wall-clock execution timeout.
    pub fn with_timeout(mut self, duration: Duration) -> Self {
        self.resource_limits.max_execution_time = duration;
        self
    }

    /// Add a single capability.
    pub fn with_capability(mut self, cap: Capability) -> Self {
        self.capabilities.push(cap);
        self
    }

    /// Add multiple capabilities at once.
    pub fn with_capabilities(mut self, caps: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities.extend(caps);
        self
    }

    /// Enable host function calls from within the sandbox.
    pub fn allow_host_calls(mut self) -> Self {
        self.allow_host_calls = true;
        self
    }
}

// ---------------------------------------------------------------------------
// Serde helper for `Duration`
// ---------------------------------------------------------------------------

/// Custom serde module for `std::time::Duration`, serialised as a
/// `{ secs, nanos }` pair so it round-trips through JSON cleanly.
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    struct DurationRepr {
        secs: u64,
        nanos: u32,
    }

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let repr = DurationRepr {
            secs: duration.as_secs(),
            nanos: duration.subsec_nanos(),
        };
        repr.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = DurationRepr::deserialize(deserializer)?;
        Ok(Duration::new(repr.secs, repr.nanos))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Default values ------------------------------------------------------

    #[test]
    fn test_sandbox_config_default_values() {
        let config = SandboxConfig::default();

        assert_eq!(config.resource_limits.max_memory_bytes, 16 * 1024 * 1024,);
        assert_eq!(config.resource_limits.max_fuel, 1_000_000);
        assert_eq!(
            config.resource_limits.max_execution_time,
            Duration::from_secs(30),
        );
        assert!(config.capabilities.is_empty());
        assert!(!config.allow_host_calls);
    }

    #[test]
    fn test_sandbox_config_new_equals_default() {
        assert_eq!(SandboxConfig::new(), SandboxConfig::default());
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();

        assert_eq!(limits.max_memory_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 1_000_000);
        assert_eq!(limits.max_execution_time, Duration::from_secs(30));
    }

    // -- Builder pattern -----------------------------------------------------

    #[test]
    fn test_builder_chain() {
        let config = SandboxConfig::new()
            .with_memory_limit(32 * 1024 * 1024)
            .with_fuel_limit(2_000_000)
            .with_timeout(Duration::from_secs(60))
            .with_capability(Capability::Stdout)
            .with_capability(Capability::Stderr)
            .allow_host_calls();

        assert_eq!(config.resource_limits.max_memory_bytes, 32 * 1024 * 1024);
        assert_eq!(config.resource_limits.max_fuel, 2_000_000);
        assert_eq!(
            config.resource_limits.max_execution_time,
            Duration::from_secs(60),
        );
        assert_eq!(config.capabilities.len(), 2);
        assert!(config.allow_host_calls);
    }

    #[test]
    fn test_builder_with_capabilities_batch() {
        let caps = vec![
            Capability::Stdout,
            Capability::Stderr,
            Capability::NetworkAccess(vec!["localhost".to_string()]),
        ];

        let config = SandboxConfig::new().with_capabilities(caps);

        assert_eq!(config.capabilities.len(), 3);
    }

    #[test]
    fn test_builder_with_memory_limit() {
        let config = SandboxConfig::new().with_memory_limit(64 * 1024 * 1024);
        assert_eq!(config.resource_limits.max_memory_bytes, 64 * 1024 * 1024);
        // Other limits remain at defaults.
        assert_eq!(config.resource_limits.max_fuel, 1_000_000);
    }

    #[test]
    fn test_builder_allow_host_calls() {
        let config = SandboxConfig::new();
        assert!(!config.allow_host_calls);

        let config = config.allow_host_calls();
        assert!(config.allow_host_calls);
    }

    // -- Capabilities --------------------------------------------------------

    #[test]
    fn test_capability_file_read() {
        let cap = Capability::FileRead(vec![
            PathBuf::from("/tmp/data"),
            PathBuf::from("/home/user/docs"),
        ]);

        if let Capability::FileRead(paths) = &cap {
            assert_eq!(paths.len(), 2);
            assert_eq!(paths[0], PathBuf::from("/tmp/data"));
        } else {
            panic!("expected FileRead variant");
        }
    }

    #[test]
    fn test_config_with_various_capabilities() {
        let config = SandboxConfig::new()
            .with_capability(Capability::FileRead(vec![PathBuf::from("/data")]))
            .with_capability(Capability::FileWrite(vec![PathBuf::from("/output")]))
            .with_capability(Capability::NetworkAccess(vec![
                "api.example.com".to_string()
            ]))
            .with_capability(Capability::EnvironmentRead(vec![
                "HOME".to_string(),
                "PATH".to_string(),
            ]))
            .with_capability(Capability::Stdout)
            .with_capability(Capability::Stderr);

        assert_eq!(config.capabilities.len(), 6);
    }

    // -- Serialization round-trips -------------------------------------------

    #[test]
    fn test_sandbox_config_serde_round_trip() {
        let config = SandboxConfig::new()
            .with_memory_limit(8 * 1024 * 1024)
            .with_fuel_limit(500_000)
            .with_timeout(Duration::from_secs(10))
            .with_capability(Capability::Stdout)
            .with_capability(Capability::FileRead(vec![PathBuf::from("/tmp")]))
            .allow_host_calls();

        let json = serde_json::to_string(&config).unwrap();
        let decoded: SandboxConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, decoded);
    }

    #[test]
    fn test_resource_limits_serde_round_trip() {
        let limits = ResourceLimits {
            max_memory_bytes: 4 * 1024 * 1024,
            max_fuel: 250_000,
            max_execution_time: Duration::from_millis(1500),
        };

        let json = serde_json::to_string(&limits).unwrap();
        let decoded: ResourceLimits = serde_json::from_str(&json).unwrap();

        assert_eq!(limits, decoded);
    }

    #[test]
    fn test_capability_serde_round_trip() {
        let caps = vec![
            Capability::FileRead(vec![PathBuf::from("/a"), PathBuf::from("/b")]),
            Capability::FileWrite(vec![PathBuf::from("/c")]),
            Capability::NetworkAccess(vec!["example.com".to_string()]),
            Capability::EnvironmentRead(vec!["HOME".to_string()]),
            Capability::Stdout,
            Capability::Stderr,
        ];

        let json = serde_json::to_string(&caps).unwrap();
        let decoded: Vec<Capability> = serde_json::from_str(&json).unwrap();

        assert_eq!(caps, decoded);
    }

    #[test]
    fn test_default_config_serde_round_trip() {
        let config = SandboxConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: SandboxConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, decoded);
    }

    #[test]
    fn test_duration_json_shape() {
        let limits = ResourceLimits::default();
        let value: serde_json::Value = serde_json::to_value(&limits).unwrap();

        // max_execution_time should serialise as { secs, nanos }
        let time = value.get("max_execution_time").unwrap();
        assert_eq!(time.get("secs").unwrap(), 30);
        assert_eq!(time.get("nanos").unwrap(), 0);
    }
}
