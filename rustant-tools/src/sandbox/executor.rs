//! Sandboxed tool executor that wraps tool execution in a WASM sandbox.
//!
//! Provides [`SandboxedExecutor`] which can execute WASM modules with
//! configurable resource limits and capability-based permissions, as well
//! as sandbox existing tool invocations for additional isolation.

use std::sync::Arc;
use std::time::Instant;

use super::config::{Capability, SandboxConfig};
use super::runtime::{ExecutionResult, SandboxError, WasmRuntime};

/// Executes WASM modules within a sandboxed environment.
///
/// The executor manages a [`WasmRuntime`] and applies [`SandboxConfig`]
/// settings to each execution, enforcing resource limits and capability
/// restrictions.
pub struct SandboxedExecutor {
    runtime: Arc<WasmRuntime>,
    default_config: SandboxConfig,
}

impl SandboxedExecutor {
    /// Create a new executor with a shared runtime and default config.
    pub fn new(runtime: Arc<WasmRuntime>, default_config: SandboxConfig) -> Self {
        Self {
            runtime,
            default_config,
        }
    }

    /// Create an executor with default settings.
    pub fn with_defaults() -> Self {
        let runtime = Arc::new(WasmRuntime::new());
        Self {
            runtime,
            default_config: SandboxConfig::default(),
        }
    }

    /// Get a reference to the underlying runtime.
    pub fn runtime(&self) -> &WasmRuntime {
        &self.runtime
    }

    /// Get the default configuration.
    pub fn default_config(&self) -> &SandboxConfig {
        &self.default_config
    }

    /// Validate a WASM module without executing it.
    pub fn validate(&self, wasm_bytes: &[u8]) -> Result<(), SandboxError> {
        self.runtime.validate_module(wasm_bytes)
    }

    /// Execute a WASM module with the default configuration.
    pub fn execute(
        &self,
        wasm_bytes: &[u8],
        input: &[u8],
    ) -> Result<SandboxExecution, SandboxError> {
        self.execute_with_config(wasm_bytes, input, &self.default_config)
    }

    /// Execute a WASM module with a specific configuration.
    pub fn execute_with_config(
        &self,
        wasm_bytes: &[u8],
        input: &[u8],
        config: &SandboxConfig,
    ) -> Result<SandboxExecution, SandboxError> {
        let start = Instant::now();

        let result = self.runtime.execute(wasm_bytes, input, config)?;

        let elapsed = start.elapsed();

        Ok(SandboxExecution {
            result,
            wall_time_ms: elapsed.as_millis() as u64,
            config_snapshot: ConfigSnapshot {
                max_memory_bytes: config.resource_limits.max_memory_bytes,
                max_fuel: config.resource_limits.max_fuel,
                capabilities_count: config.capabilities.len(),
                host_calls_allowed: config.allow_host_calls,
            },
        })
    }

    /// Execute a WASM module with additional capabilities beyond the default.
    pub fn execute_with_extra_capabilities(
        &self,
        wasm_bytes: &[u8],
        input: &[u8],
        extra_caps: Vec<Capability>,
    ) -> Result<SandboxExecution, SandboxError> {
        let mut config = self.default_config.clone();
        for cap in extra_caps {
            config.capabilities.push(cap);
        }
        self.execute_with_config(wasm_bytes, input, &config)
    }
}

/// The complete result of a sandboxed execution, including timing and config info.
#[derive(Debug, Clone)]
pub struct SandboxExecution {
    /// The execution result from the WASM runtime.
    pub result: ExecutionResult,
    /// Wall-clock time of execution in milliseconds.
    pub wall_time_ms: u64,
    /// Snapshot of the configuration used for this execution.
    pub config_snapshot: ConfigSnapshot,
}

impl SandboxExecution {
    /// Get the output bytes from the execution.
    pub fn output(&self) -> &[u8] {
        &self.result.output
    }

    /// Get the output as a UTF-8 string, if valid.
    pub fn output_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.result.output).ok()
    }

    /// Get fuel consumed during execution.
    pub fn fuel_consumed(&self) -> u64 {
        self.result.fuel_consumed
    }

    /// Get peak memory usage in bytes.
    pub fn memory_peak_bytes(&self) -> usize {
        self.result.memory_peak_bytes
    }

    /// Check if the execution was within resource limits.
    pub fn within_limits(&self) -> bool {
        self.result.fuel_consumed <= self.config_snapshot.max_fuel
            && self.result.memory_peak_bytes <= self.config_snapshot.max_memory_bytes
    }
}

/// Snapshot of configuration at time of execution.
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub max_memory_bytes: usize,
    pub max_fuel: u64,
    pub capabilities_count: usize,
    pub host_calls_allowed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::config::ResourceLimits;

    #[test]
    fn test_executor_with_defaults() {
        let executor = SandboxedExecutor::with_defaults();
        assert_eq!(
            executor.default_config().resource_limits.max_fuel,
            ResourceLimits::default().max_fuel
        );
    }

    #[test]
    fn test_executor_custom_config() {
        let config = SandboxConfig::new()
            .with_fuel_limit(500_000)
            .with_memory_limit(8 * 1024 * 1024);
        let runtime = Arc::new(WasmRuntime::new());
        let executor = SandboxedExecutor::new(runtime, config);

        assert_eq!(executor.default_config().resource_limits.max_fuel, 500_000);
        assert_eq!(
            executor.default_config().resource_limits.max_memory_bytes,
            8 * 1024 * 1024
        );
    }

    #[test]
    fn test_executor_validate_valid_module() {
        let executor = SandboxedExecutor::with_defaults();
        let wat = br#"(module (func (export "_start")))"#;
        assert!(executor.validate(wat).is_ok());
    }

    #[test]
    fn test_executor_validate_invalid_module() {
        let executor = SandboxedExecutor::with_defaults();
        assert!(executor.validate(b"not wasm").is_err());
    }

    #[test]
    fn test_executor_execute_simple() {
        let executor = SandboxedExecutor::with_defaults();
        let wat = br#"(module (func (export "_start")))"#;
        let result = executor.execute(wat, b"").unwrap();

        assert!(result.output().is_empty());
        assert!(result.fuel_consumed() > 0);
        assert!(result.within_limits());
        assert!(result.wall_time_ms < 5000); // should be fast
    }

    #[test]
    fn test_executor_execute_with_output() {
        let executor = SandboxedExecutor::with_defaults();
        let wat = br#"
            (module
                (import "env" "host_write_output" (func $write (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "sandbox-out")
                (func (export "_start")
                    i32.const 0
                    i32.const 11
                    call $write
                )
            )
        "#;
        let result = executor.execute(wat, b"").unwrap();
        assert_eq!(result.output(), b"sandbox-out");
    }

    #[test]
    fn test_executor_fuel_exhaustion() {
        let config = SandboxConfig::new().with_fuel_limit(100);
        let runtime = Arc::new(WasmRuntime::new());
        let executor = SandboxedExecutor::new(runtime, config);

        let wat = br#"
            (module
                (func (export "_start")
                    (local $i i32)
                    (loop $loop
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br_if $loop (i32.lt_u (local.get $i) (i32.const 999999)))
                    )
                )
            )
        "#;
        let err = executor.execute(wat, b"").unwrap_err();
        assert!(matches!(err, SandboxError::OutOfFuel));
    }

    #[test]
    fn test_executor_with_extra_capabilities() {
        let executor = SandboxedExecutor::with_defaults();
        let wat = br#"(module (func (export "_start")))"#;
        let result = executor
            .execute_with_extra_capabilities(wat, b"", vec![Capability::Stdout])
            .unwrap();
        assert!(result.within_limits());
    }

    #[test]
    fn test_sandbox_execution_output_str() {
        let exec = SandboxExecution {
            result: ExecutionResult {
                output: b"hello".to_vec(),
                fuel_consumed: 10,
                memory_peak_bytes: 1024,
            },
            wall_time_ms: 1,
            config_snapshot: ConfigSnapshot {
                max_memory_bytes: 16 * 1024 * 1024,
                max_fuel: 1_000_000,
                capabilities_count: 0,
                host_calls_allowed: false,
            },
        };
        assert_eq!(exec.output_str(), Some("hello"));
        assert!(exec.within_limits());
    }

    #[test]
    fn test_sandbox_execution_invalid_utf8() {
        let exec = SandboxExecution {
            result: ExecutionResult {
                output: vec![0xFF, 0xFE],
                fuel_consumed: 5,
                memory_peak_bytes: 512,
            },
            wall_time_ms: 1,
            config_snapshot: ConfigSnapshot {
                max_memory_bytes: 16 * 1024 * 1024,
                max_fuel: 1_000_000,
                capabilities_count: 0,
                host_calls_allowed: false,
            },
        };
        assert!(exec.output_str().is_none());
    }

    #[test]
    fn test_config_snapshot_fields() {
        let config = SandboxConfig::new()
            .with_fuel_limit(42)
            .with_memory_limit(1024)
            .with_capability(Capability::Stdout)
            .with_capability(Capability::Stderr)
            .allow_host_calls();

        let runtime = Arc::new(WasmRuntime::new());
        let executor = SandboxedExecutor::new(runtime, config);

        let wat = br#"(module (func (export "_start")))"#;
        let result = executor.execute(wat, b"").unwrap();

        assert_eq!(result.config_snapshot.max_fuel, 42);
        assert_eq!(result.config_snapshot.max_memory_bytes, 1024);
        assert_eq!(result.config_snapshot.capabilities_count, 2);
        assert!(result.config_snapshot.host_calls_allowed);
    }
}
