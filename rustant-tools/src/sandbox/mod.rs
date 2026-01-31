//! Universal WASM-Based Sandboxing for Rustant.
//!
//! Provides a sandboxed execution environment using WebAssembly (via the `wasmi`
//! interpreter) with capability-based permissions and resource limits. WASM modules
//! execute in a fully isolated environment with controlled access to host resources.
//!
//! ## Architecture
//!
//! ```text
//! SandboxedExecutor
//!     │
//!     ├── WasmRuntime (wasmi Engine)
//!     │       ├── Module compilation & validation
//!     │       ├── Fuel metering (CPU budget)
//!     │       └── Memory limits
//!     │
//!     ├── HostState (guest ↔ host bridge)
//!     │       ├── Input/output buffers
//!     │       ├── Stdout/stderr capture
//!     │       └── Capability checks
//!     │
//!     └── SandboxConfig
//!             ├── ResourceLimits (memory, fuel, time)
//!             └── Capabilities (FileRead, FileWrite, Network, etc.)
//! ```

pub mod config;
pub mod executor;
pub mod host;
pub mod runtime;

// Re-export primary types for convenient access.
pub use config::{Capability, ResourceLimits, SandboxConfig};
pub use executor::{SandboxExecution, SandboxedExecutor};
pub use runtime::{ExecutionResult, SandboxError, WasmRuntime};

use std::sync::Arc;

/// Create a [`SandboxedExecutor`] with default settings.
///
/// This is a convenience function that creates a shared [`WasmRuntime`] and
/// wraps it in a [`SandboxedExecutor`] with default resource limits.
pub fn create_sandbox() -> SandboxedExecutor {
    SandboxedExecutor::with_defaults()
}

/// Create a [`SandboxedExecutor`] with a custom configuration.
pub fn create_sandbox_with_config(config: SandboxConfig) -> SandboxedExecutor {
    let runtime = Arc::new(WasmRuntime::new());
    SandboxedExecutor::new(runtime, config)
}

/// Validate a WASM module without executing it.
///
/// Returns `Ok(())` if the bytes represent a valid WASM module,
/// or `Err(SandboxError::ModuleInvalid)` otherwise.
pub fn validate_module(wasm_bytes: &[u8]) -> Result<(), SandboxError> {
    let runtime = WasmRuntime::new();
    runtime.validate_module(wasm_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_sandbox() {
        let executor = create_sandbox();
        assert_eq!(
            executor.default_config().resource_limits.max_fuel,
            ResourceLimits::default().max_fuel
        );
    }

    #[test]
    fn test_create_sandbox_with_config() {
        let config = SandboxConfig::new().with_fuel_limit(42);
        let executor = create_sandbox_with_config(config);
        assert_eq!(executor.default_config().resource_limits.max_fuel, 42);
    }

    #[test]
    fn test_validate_valid_module() {
        let wat = br#"(module (func (export "_start")))"#;
        assert!(validate_module(wat).is_ok());
    }

    #[test]
    fn test_validate_invalid_module() {
        assert!(validate_module(b"garbage data").is_err());
    }

    #[test]
    fn test_end_to_end_simple_execution() {
        let executor = create_sandbox();
        let wat = br#"(module (func (export "_start")))"#;
        let result = executor.execute(wat, b"").unwrap();
        assert!(result.output().is_empty());
        assert!(result.fuel_consumed() > 0);
    }

    #[test]
    fn test_end_to_end_with_output() {
        let executor = create_sandbox();
        let wat = br#"
            (module
                (import "env" "host_write_output" (func $write (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "sandboxed")
                (func (export "_start")
                    i32.const 0
                    i32.const 9
                    call $write
                )
            )
        "#;
        let result = executor.execute(wat, b"").unwrap();
        assert_eq!(result.output_str(), Some("sandboxed"));
    }

    #[test]
    fn test_end_to_end_fuel_exhaustion() {
        let config = SandboxConfig::new().with_fuel_limit(50);
        let executor = create_sandbox_with_config(config);
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
    fn test_end_to_end_input_output() {
        let executor = create_sandbox();
        let wat = br#"
            (module
                (import "env" "host_get_input_len" (func $input_len (result i32)))
                (import "env" "host_read_input" (func $read (param i32 i32) (result i32)))
                (import "env" "host_write_output" (func $write (param i32 i32)))
                (memory (export "memory") 1)
                (func (export "_start")
                    (local $len i32)
                    (local.set $len (call $input_len))
                    (drop (call $read (i32.const 0) (local.get $len)))
                    (call $write (i32.const 0) (local.get $len))
                )
            )
        "#;
        let result = executor.execute(wat, b"echo-test").unwrap();
        assert_eq!(result.output_str(), Some("echo-test"));
    }

    #[test]
    fn test_capabilities_in_config() {
        let config = SandboxConfig::new()
            .with_capability(Capability::Stdout)
            .with_capability(Capability::Stderr)
            .with_capability(Capability::FileRead(vec!["/tmp".into()]));

        let executor = create_sandbox_with_config(config);
        assert_eq!(executor.default_config().capabilities.len(), 3);
    }

    #[test]
    fn test_resource_limits_applied() {
        let config = SandboxConfig::new()
            .with_fuel_limit(100_000)
            .with_memory_limit(4 * 1024 * 1024);

        let executor = create_sandbox_with_config(config);

        let wat = br#"(module (func (export "_start")))"#;
        let result = executor.execute(wat, b"").unwrap();
        assert!(result.within_limits());
    }
}
