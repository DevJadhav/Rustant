//! WASM runtime engine using wasmi for sandboxed execution.
//!
//! Manages the `wasmi` engine lifecycle, compiles WASM modules, and executes
//! them with fuel metering and resource limits.  Host functions are registered
//! in the `"env"` namespace so that guests can log messages, read input, and
//! write output through a controlled interface.

use super::config::{Capability, SandboxConfig};
#[allow(unused_imports)]
use wasmi::{Caller, Engine, Extern, Func, Instance, Linker, Memory, Module, Store};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Error type for sandbox operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SandboxError {
    /// The WASM binary is invalid or could not be parsed.
    #[error("invalid WASM module: {0}")]
    ModuleInvalid(String),

    /// The module could not be instantiated (e.g. missing imports).
    #[error("module instantiation failed: {0}")]
    InstantiationFailed(String),

    /// Execution returned an error or trapped.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// Fuel / instruction budget exhausted.
    #[error("fuel/instruction budget exhausted")]
    OutOfFuel,

    /// Memory limit exceeded.
    #[error("memory limit exceeded")]
    OutOfMemory,

    /// Execution timed out.
    #[error("execution timed out")]
    Timeout,

    /// Guest tried a forbidden operation.
    #[error("capability denied: {0}")]
    CapabilityDenied(String),

    /// A host function returned an error.
    #[error("host function error: {0}")]
    HostError(String),
}

// ---------------------------------------------------------------------------
// Execution result
// ---------------------------------------------------------------------------

/// The result of a successful WASM module execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Raw output bytes produced by the module via `host_write_output`.
    pub output: Vec<u8>,
    /// Number of fuel units consumed during execution.
    pub fuel_consumed: u64,
    /// Peak linear memory usage in bytes.
    pub memory_peak_bytes: usize,
}

// ---------------------------------------------------------------------------
// Host state
// ---------------------------------------------------------------------------

/// Per-execution state shared with host functions via the wasmi [`Store`].
///
/// Each sandbox invocation receives a fresh `HostState` that accumulates
/// guest output and provides the input buffer for reading.
pub struct HostState {
    /// Captured stdout from the guest.
    pub stdout: Vec<u8>,
    /// Captured stderr from the guest.
    pub stderr: Vec<u8>,
    /// Module output buffer (populated by `host_write_output`).
    pub output: Vec<u8>,
    /// Module input buffer (read by `host_read_input`).
    pub input: Vec<u8>,
    /// Capabilities granted to this execution.
    pub capabilities: Vec<Capability>,
    /// Peak memory usage tracked across host calls.
    pub memory_peak: usize,
}

// ---------------------------------------------------------------------------
// WASM runtime
// ---------------------------------------------------------------------------

/// Manages the wasmi [`Engine`] and provides module validation and execution.
///
/// The engine is created once with fuel metering enabled.  Each call to
/// [`execute`](Self::execute) compiles, instantiates, and runs a WASM module
/// inside a fresh [`Store`] with its own fuel budget and host state.
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    /// Create a new `WasmRuntime` with fuel metering enabled.
    pub fn new() -> Self {
        let mut config = wasmi::Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        Self { engine }
    }

    /// Validate that `wasm_bytes` contains a well-formed WASM (or WAT) module.
    ///
    /// Accepts both `.wasm` binary and `.wat` text format (when the `wat`
    /// crate feature is enabled, which it is by default in wasmi).
    pub fn validate_module(&self, wasm_bytes: &[u8]) -> Result<(), SandboxError> {
        Module::new(&self.engine, wasm_bytes)
            .map(|_| ())
            .map_err(|e| SandboxError::ModuleInvalid(e.to_string()))
    }

    /// Compile, instantiate, and execute a WASM module.
    ///
    /// # Execution flow
    ///
    /// 1. Compile the WASM (or WAT) bytes into a [`Module`].
    /// 2. Create a [`Store`] seeded with fuel from `config`.
    /// 3. Register host functions (`host_log`, `host_write_output`,
    ///    `host_read_input`, `host_get_input_len`) under the `"env"` namespace.
    /// 4. Instantiate the module (runs the WASM start section if present).
    /// 5. Call the exported `execute(ptr, len) -> i32` function, or fall back
    ///    to `_start()` if `execute` is not exported.
    /// 6. Collect and return the [`ExecutionResult`].
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::OutOfFuel`] when the fuel budget is exhausted,
    /// [`SandboxError::ModuleInvalid`] for malformed WASM, and other variants
    /// for instantiation or execution failures.
    pub fn execute(
        &self,
        wasm_bytes: &[u8],
        input: &[u8],
        config: &SandboxConfig,
    ) -> Result<ExecutionResult, SandboxError> {
        // 1. Compile module
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| SandboxError::ModuleInvalid(e.to_string()))?;

        // 2. Create store with host state and fuel budget
        let host_state = HostState {
            stdout: Vec::new(),
            stderr: Vec::new(),
            output: Vec::new(),
            input: input.to_vec(),
            capabilities: config.capabilities.clone(),
            memory_peak: 0,
        };
        let mut store = Store::new(&self.engine, host_state);
        store
            .set_fuel(config.resource_limits.max_fuel)
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        // 3. Create linker and register host functions
        let mut linker = <Linker<HostState>>::new(&self.engine);
        Self::register_host_functions(&mut linker)?;

        // 4. Instantiate the module (and run its WASM start section, if any)
        let instance: Instance =
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

        // 5. Call the entry-point export.
        //    Prefer `execute(ptr, len) -> i32`; fall back to `_start()`.
        if let Ok(func) = instance.get_typed_func::<(i32, i32), i32>(&store, "execute") {
            func.call(&mut store, (0, input.len() as i32))
                .map_err(map_wasmi_error)?;
        } else if let Ok(func) = instance.get_typed_func::<(), ()>(&store, "_start") {
            func.call(&mut store, ()).map_err(map_wasmi_error)?;
        } else {
            return Err(SandboxError::ExecutionFailed(
                "module exports neither '_start' nor 'execute' function".to_string(),
            ));
        }

        // 6. Collect results
        let fuel_remaining = store.get_fuel().unwrap_or(0);
        let fuel_consumed = config
            .resource_limits
            .max_fuel
            .saturating_sub(fuel_remaining);

        let memory_peak_bytes: usize = instance
            .get_export(&store, "memory")
            .and_then(Extern::into_memory)
            .map(|m: Memory| m.data(&store).len())
            .unwrap_or(0);

        let host_peak = store.data().memory_peak;
        let output = std::mem::take(&mut store.data_mut().output);

        Ok(ExecutionResult {
            output,
            fuel_consumed,
            memory_peak_bytes: std::cmp::max(memory_peak_bytes, host_peak),
        })
    }

    // -- Host function registration -----------------------------------------

    /// Register the four standard host functions in the `"env"` namespace.
    ///
    /// | Function             | Signature                      | Purpose                        |
    /// |----------------------|--------------------------------|--------------------------------|
    /// | `host_log`           | `(ptr: i32, len: i32)`         | Log UTF-8 message from guest   |
    /// | `host_write_output`  | `(ptr: i32, len: i32)`         | Append bytes to output buffer  |
    /// | `host_read_input`    | `(ptr: i32, len: i32) -> i32`  | Copy input into guest memory   |
    /// | `host_get_input_len` | `() -> i32`                    | Return input buffer length     |
    fn register_host_functions(linker: &mut Linker<HostState>) -> Result<(), SandboxError> {
        // -- env::host_log(ptr, len) ------------------------------------------
        linker
            .func_wrap(
                "env",
                "host_log",
                |caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return,
                    };
                    let (ptr, len) = (ptr as usize, len as usize);
                    let data = mem.data(&caller);
                    if let Some(end) = ptr.checked_add(len)
                        && end <= data.len()
                        && let Ok(msg) = std::str::from_utf8(&data[ptr..end])
                    {
                        tracing::debug!(target: "wasm_guest", "{}", msg);
                    }
                },
            )
            .map_err(|e| SandboxError::InstantiationFailed(e.to_string()))?;

        // -- env::host_write_output(ptr, len) ---------------------------------
        linker
            .func_wrap(
                "env",
                "host_write_output",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return,
                    };
                    let (ptr, len) = (ptr as usize, len as usize);
                    let data = mem.data(&caller);
                    let bytes = match ptr.checked_add(len) {
                        Some(end) if end <= data.len() => data[ptr..end].to_vec(),
                        _ => return,
                    };
                    caller.data_mut().output.extend_from_slice(&bytes);
                },
            )
            .map_err(|e| SandboxError::InstantiationFailed(e.to_string()))?;

        // -- env::host_read_input(ptr, len) -> i32 ----------------------------
        linker
            .func_wrap(
                "env",
                "host_read_input",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                    let input_bytes = caller.data().input.clone();
                    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return 0,
                    };
                    let (ptr, len) = (ptr as usize, len as usize);
                    let to_copy = std::cmp::min(len, input_bytes.len());
                    let mem_data = mem.data_mut(&mut caller);
                    match ptr.checked_add(to_copy) {
                        Some(end) if end <= mem_data.len() => {
                            mem_data[ptr..end].copy_from_slice(&input_bytes[..to_copy]);
                            to_copy as i32
                        }
                        _ => 0,
                    }
                },
            )
            .map_err(|e| SandboxError::InstantiationFailed(e.to_string()))?;

        // -- env::host_get_input_len() -> i32 ---------------------------------
        linker
            .func_wrap(
                "env",
                "host_get_input_len",
                |caller: Caller<'_, HostState>| -> i32 { caller.data().input.len() as i32 },
            )
            .map_err(|e| SandboxError::InstantiationFailed(e.to_string()))?;

        Ok(())
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Map a [`wasmi::Error`] into a [`SandboxError`], detecting out-of-fuel traps.
fn map_wasmi_error(err: wasmi::Error) -> SandboxError {
    let msg = err.to_string();
    if msg.contains("fuel") {
        SandboxError::OutOfFuel
    } else {
        SandboxError::ExecutionFailed(msg)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: a default config with generous fuel and host calls enabled.
    fn default_config() -> SandboxConfig {
        SandboxConfig::new()
            .with_fuel_limit(1_000_000)
            .allow_host_calls()
    }

    // -- Runtime creation ----------------------------------------------------

    #[test]
    fn test_runtime_creation() {
        let _runtime = WasmRuntime::new();
    }

    // -- Module validation ---------------------------------------------------

    #[test]
    fn test_validate_valid_module() {
        let runtime = WasmRuntime::new();
        let wat = b"(module (func (export \"_start\")))";
        assert!(runtime.validate_module(wat).is_ok());
    }

    #[test]
    fn test_validate_invalid_module() {
        let runtime = WasmRuntime::new();
        let invalid = b"this is definitely not valid wasm";
        let result = runtime.validate_module(invalid);
        assert!(result.is_err());
        match result {
            Err(SandboxError::ModuleInvalid(_)) => {}
            other => panic!("expected ModuleInvalid, got {:?}", other),
        }
    }

    // -- Execution -----------------------------------------------------------

    #[test]
    fn test_execute_simple_module() {
        let runtime = WasmRuntime::new();
        let wat = b"(module (func (export \"_start\")))";
        let config = default_config();
        let result = runtime.execute(wat, &[], &config).unwrap();
        assert!(result.output.is_empty());
    }

    #[test]
    fn test_execute_module_with_output() {
        let runtime = WasmRuntime::new();
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
        let config = default_config();
        let result = runtime.execute(wat, &[], &config).unwrap();
        assert_eq!(result.output, b"hello");
    }

    #[test]
    fn test_execute_fuel_limit() {
        let runtime = WasmRuntime::new();
        let wat = br#"
            (module
                (func (export "_start")
                    (local $i i32)
                    (loop $loop
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br_if $loop (i32.lt_u (local.get $i) (i32.const 999999999)))
                    )
                )
            )
        "#;
        let config = SandboxConfig::new()
            .with_fuel_limit(1_000)
            .allow_host_calls();
        let result = runtime.execute(wat, &[], &config);
        assert!(
            matches!(result, Err(SandboxError::OutOfFuel)),
            "expected OutOfFuel, got {:?}",
            result,
        );
    }

    #[test]
    fn test_execute_reads_input() {
        let runtime = WasmRuntime::new();
        let wat = br#"
            (module
                (import "env" "host_get_input_len" (func $get_len (result i32)))
                (import "env" "host_read_input" (func $read (param i32 i32) (result i32)))
                (import "env" "host_write_output" (func $write (param i32 i32)))
                (memory (export "memory") 1)
                (func (export "_start")
                    (local $len i32)
                    (local $read_len i32)
                    (local.set $len (call $get_len))
                    (local.set $read_len (call $read (i32.const 0) (local.get $len)))
                    (call $write (i32.const 0) (local.get $read_len))
                )
            )
        "#;
        let config = default_config();
        let input = b"world";
        let result = runtime.execute(wat, input, &config).unwrap();
        assert_eq!(result.output, b"world");
    }

    #[test]
    fn test_fuel_consumption_tracked() {
        let runtime = WasmRuntime::new();
        let wat = b"(module (func (export \"_start\") nop))";
        let config = default_config();
        let result = runtime.execute(wat, &[], &config).unwrap();
        assert!(
            result.fuel_consumed > 0,
            "fuel_consumed should be non-zero after execution",
        );
    }
}
