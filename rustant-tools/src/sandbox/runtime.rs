//! WASM runtime engine using wasmi for sandboxed execution.
//!
//! Manages the `wasmi` engine lifecycle, compiles WASM modules, and executes
//! them with fuel metering and resource limits.

use super::config::SandboxConfig;
use super::host::{register_host_functions, HostState};

/// Errors that can occur during sandbox operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SandboxError {
    /// The WASM binary is malformed or invalid.
    #[error("invalid WASM module: {0}")]
    ModuleInvalid(String),

    /// The module could not be instantiated (e.g. missing imports).
    #[error("instantiation failed: {0}")]
    InstantiationFailed(String),

    /// Execution returned an error.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// The fuel (instruction) budget was exhausted.
    #[error("out of fuel: instruction budget exhausted")]
    OutOfFuel,

    /// The memory limit was exceeded.
    #[error("out of memory: memory limit exceeded")]
    OutOfMemory,

    /// Execution timed out.
    #[error("execution timed out")]
    Timeout,

    /// The guest attempted a forbidden operation.
    #[error("capability denied: {0}")]
    CapabilityDenied(String),

    /// A host function returned an error.
    #[error("host error: {0}")]
    HostError(String),
}

/// The result of a successful WASM module execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Raw output bytes written by the module via `host_write_output`.
    pub output: Vec<u8>,
    /// Number of fuel units consumed during execution.
    pub fuel_consumed: u64,
    /// Peak linear memory usage in bytes.
    pub memory_peak_bytes: usize,
}

/// The WASM runtime engine backed by `wasmi`.
///
/// Maintains a configured `wasmi::Engine` with fuel metering enabled.
/// Use [`WasmRuntime::execute`] to run WASM modules with resource limits.
pub struct WasmRuntime {
    engine: wasmi::Engine,
}

impl WasmRuntime {
    /// Create a new `WasmRuntime` with fuel metering enabled.
    pub fn new() -> Self {
        let mut config = wasmi::Config::default();
        config.consume_fuel(true);
        let engine = wasmi::Engine::new(&config);
        Self { engine }
    }

    /// Validate a WASM module without executing it.
    ///
    /// Accepts both `.wasm` binary and `.wat` text format (when the `wat`
    /// feature is enabled, which it is by default).
    pub fn validate_module(&self, wasm_bytes: &[u8]) -> Result<(), SandboxError> {
        wasmi::Module::new(&self.engine, wasm_bytes)
            .map(|_| ())
            .map_err(|e| SandboxError::ModuleInvalid(e.to_string()))
    }

    /// Execute a WASM module with the given input and configuration.
    ///
    /// # Execution flow
    ///
    /// 1. Compile the module
    /// 2. Create a store with fuel limit from config
    /// 3. Register host functions via the linker
    /// 4. Instantiate the module
    /// 5. Call the exported `_start` function
    /// 6. Collect results (output, fuel consumed, memory peak)
    pub fn execute(
        &self,
        wasm_bytes: &[u8],
        input: &[u8],
        config: &SandboxConfig,
    ) -> Result<ExecutionResult, SandboxError> {
        // 1. Compile module
        let module = wasmi::Module::new(&self.engine, wasm_bytes)
            .map_err(|e| SandboxError::ModuleInvalid(e.to_string()))?;

        // 2. Create store with host state and fuel
        let host_state = HostState::new(input.to_vec(), config.capabilities.clone());
        let mut store = wasmi::Store::new(&self.engine, host_state);
        store
            .set_fuel(config.resource_limits.max_fuel)
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        // 3. Create linker and register host functions
        let mut linker = wasmi::Linker::new(&self.engine);
        register_host_functions(&mut linker)
            .map_err(|e| SandboxError::InstantiationFailed(e.to_string()))?;

        // 4. Instantiate and start module
        let instance =
            linker
                .instantiate_and_start(&mut store, &module)
                .map_err(|e: wasmi::Error| {
                    let msg = e.to_string();
                    if msg.contains("fuel") {
                        SandboxError::OutOfFuel
                    } else {
                        SandboxError::InstantiationFailed(msg)
                    }
                })?;

        // 5. Call _start export
        let start_func = instance
            .get_typed_func::<(), ()>(&store, "_start")
            .map_err(|e: wasmi::Error| {
                SandboxError::ExecutionFailed(format!("missing _start export: {}", e))
            })?;

        match start_func.call(&mut store, ()) {
            Ok(()) => {}
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("fuel") {
                    return Err(SandboxError::OutOfFuel);
                }
                return Err(SandboxError::ExecutionFailed(msg));
            }
        }

        // 6. Collect results
        let fuel_remaining = store.get_fuel().unwrap_or(0);
        let fuel_consumed = config
            .resource_limits
            .max_fuel
            .saturating_sub(fuel_remaining);

        // Track final memory size
        if let Some(memory) = instance.get_memory(&store, "memory") {
            let mem_bytes = memory.data(&store as &wasmi::Store<HostState>).len();
            store.data_mut().track_memory(mem_bytes);
        }

        let host_state = store.into_data();
        Ok(ExecutionResult {
            output: host_state.output,
            fuel_consumed,
            memory_peak_bytes: host_state.memory_peak,
        })
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let _runtime = WasmRuntime::new();
    }

    #[test]
    fn test_runtime_default() {
        let _runtime = WasmRuntime::default();
    }

    #[test]
    fn test_validate_valid_module() {
        let runtime = WasmRuntime::new();
        let wat = br#"(module (func (export "_start")))"#;
        assert!(runtime.validate_module(wat).is_ok());
    }

    #[test]
    fn test_validate_module_with_imports() {
        let runtime = WasmRuntime::new();
        let wat = br#"
            (module
                (import "env" "host_write_output" (func (param i32 i32)))
                (memory (export "memory") 1)
                (func (export "_start"))
            )
        "#;
        assert!(runtime.validate_module(wat).is_ok());
    }

    #[test]
    fn test_validate_invalid_module() {
        let runtime = WasmRuntime::new();
        assert!(runtime.validate_module(b"not valid wasm").is_err());
    }

    #[test]
    fn test_validate_empty_bytes() {
        let runtime = WasmRuntime::new();
        assert!(runtime.validate_module(b"").is_err());
    }

    #[test]
    fn test_execute_simple_module() {
        let runtime = WasmRuntime::new();
        let config = SandboxConfig::default();
        let wat = br#"(module (func (export "_start")))"#;

        let result = runtime.execute(wat, b"", &config).unwrap();
        assert!(result.output.is_empty());
        assert!(result.fuel_consumed > 0);
    }

    #[test]
    fn test_execute_module_with_output() {
        let runtime = WasmRuntime::new();
        let config = SandboxConfig::default();
        let wat = br#"
            (module
                (import "env" "host_write_output" (func $write (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "hello")
                (func (export "_start")
                    i32.const 0
                    i32.const 5
                    call $write
                )
            )
        "#;

        let result = runtime.execute(wat, b"", &config).unwrap();
        assert_eq!(result.output, b"hello");
    }

    #[test]
    fn test_execute_fuel_limit() {
        let runtime = WasmRuntime::new();
        let config = SandboxConfig::new().with_fuel_limit(100);
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

        let err = runtime.execute(wat, b"", &config).unwrap_err();
        assert!(matches!(err, SandboxError::OutOfFuel));
    }

    #[test]
    fn test_execute_reads_input() {
        let runtime = WasmRuntime::new();
        let config = SandboxConfig::default();
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

        let result = runtime.execute(wat, b"test-input", &config).unwrap();
        assert_eq!(result.output, b"test-input");
    }

    #[test]
    fn test_fuel_consumption_tracked() {
        let runtime = WasmRuntime::new();
        let config = SandboxConfig::default();
        let wat = br#"
            (module
                (func (export "_start")
                    (local $i i32)
                    (loop $loop
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br_if $loop (i32.lt_u (local.get $i) (i32.const 10)))
                    )
                )
            )
        "#;

        let result = runtime.execute(wat, b"", &config).unwrap();
        assert!(result.fuel_consumed > 0, "fuel_consumed should be > 0");
    }

    #[test]
    fn test_memory_peak_tracked() {
        let runtime = WasmRuntime::new();
        let config = SandboxConfig::default();
        let wat = br#"
            (module
                (import "env" "host_write_output" (func $write (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "x")
                (func (export "_start")
                    i32.const 0
                    i32.const 1
                    call $write
                )
            )
        "#;

        let result = runtime.execute(wat, b"", &config).unwrap();
        // One page = 65536 bytes
        assert!(
            result.memory_peak_bytes >= 65536,
            "peak should be >= 1 WASM page (65536), got {}",
            result.memory_peak_bytes
        );
    }

    #[test]
    fn test_execute_missing_start_export() {
        let runtime = WasmRuntime::new();
        let config = SandboxConfig::default();
        let wat = br#"(module (func $internal (nop)))"#;

        let err = runtime.execute(wat, b"", &config).unwrap_err();
        assert!(matches!(err, SandboxError::ExecutionFailed(_)));
    }

    #[test]
    fn test_sandbox_error_display() {
        assert_eq!(
            SandboxError::OutOfFuel.to_string(),
            "out of fuel: instruction budget exhausted"
        );
        assert_eq!(
            SandboxError::ModuleInvalid("bad".to_string()).to_string(),
            "invalid WASM module: bad"
        );
        assert_eq!(SandboxError::Timeout.to_string(), "execution timed out");
        assert_eq!(
            SandboxError::OutOfMemory.to_string(),
            "out of memory: memory limit exceeded"
        );
    }

    #[test]
    fn test_execution_result_clone() {
        let result = ExecutionResult {
            output: vec![1, 2, 3],
            fuel_consumed: 42,
            memory_peak_bytes: 65536,
        };
        let cloned = result.clone();
        assert_eq!(cloned.output, vec![1, 2, 3]);
        assert_eq!(cloned.fuel_consumed, 42);
        assert_eq!(cloned.memory_peak_bytes, 65536);
    }
}
