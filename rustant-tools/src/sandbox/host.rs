//! Host state and function registration for WASM sandbox guests.

use super::config::Capability;
use tracing::debug;

// ---------------------------------------------------------------------------
// HostState
// ---------------------------------------------------------------------------

/// Mutable state shared between the host and WASM guest via host functions.
///
/// Each sandbox execution gets its own `HostState` that accumulates output
/// written by the guest and provides the input buffer for reading.
pub struct HostState {
    /// Bytes written by the guest to stdout.
    pub stdout: Vec<u8>,
    /// Bytes written by the guest to stderr.
    pub stderr: Vec<u8>,
    /// Bytes written by the guest to the structured output channel.
    pub output: Vec<u8>,
    /// Input buffer supplied by the host before execution.
    pub input: Vec<u8>,
    /// Current read position within `input`.
    pub input_pos: usize,
    /// Capabilities granted to this sandbox execution.
    pub capabilities: Vec<Capability>,
    /// Peak linear memory usage observed (in bytes).
    pub memory_peak: usize,
}

impl HostState {
    /// Create a new `HostState` with the given input buffer and capabilities.
    pub fn new(input: Vec<u8>, capabilities: Vec<Capability>) -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
            output: Vec::new(),
            input,
            input_pos: 0,
            capabilities,
            memory_peak: 0,
        }
    }

    /// Check whether this host state includes the given capability.
    ///
    /// For simple capabilities like [`Capability::Stdout`] and
    /// [`Capability::Stderr`], an exact variant match is performed. For
    /// path-based capabilities like [`Capability::FileRead`], the check
    /// succeeds if any allowed path is a prefix of (or equal to) the
    /// requested path.
    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.capabilities.iter().any(|c| match (c, cap) {
            (Capability::Stdout, Capability::Stdout) => true,
            (Capability::Stderr, Capability::Stderr) => true,
            (Capability::FileRead(allowed), Capability::FileRead(requested)) => requested
                .iter()
                .all(|req| allowed.iter().any(|a| req.starts_with(a))),
            (Capability::FileWrite(allowed), Capability::FileWrite(requested)) => requested
                .iter()
                .all(|req| allowed.iter().any(|a| req.starts_with(a))),
            (Capability::NetworkAccess(allowed), Capability::NetworkAccess(requested)) => {
                requested.iter().all(|req| allowed.contains(req))
            }
            (Capability::EnvironmentRead(allowed), Capability::EnvironmentRead(requested)) => {
                requested.iter().all(|req| allowed.contains(req))
            }
            _ => false,
        })
    }

    /// Track peak memory usage. If `current_bytes` exceeds the previous peak
    /// the peak is updated.
    pub fn track_memory(&mut self, current_bytes: usize) {
        if current_bytes > self.memory_peak {
            self.memory_peak = current_bytes;
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Read a UTF-8 string from WASM linear memory at `[ptr .. ptr+len)`.
///
/// Returns `None` if the bounds are invalid or the bytes are not valid UTF-8.
fn read_wasm_string(
    memory: &wasmi::Memory,
    store: &impl wasmi::AsContext,
    ptr: i32,
    len: i32,
) -> Option<String> {
    let bytes = read_wasm_bytes(memory, store, ptr, len)?;
    String::from_utf8(bytes).ok()
}

/// Read raw bytes from WASM linear memory at `[ptr .. ptr+len)`.
///
/// Returns `None` if the requested range falls outside the memory bounds.
fn read_wasm_bytes(
    memory: &wasmi::Memory,
    store: &impl wasmi::AsContext,
    ptr: i32,
    len: i32,
) -> Option<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let start = ptr as usize;
    let size = len as usize;
    let data = memory.data(store);
    if start.checked_add(size)? > data.len() {
        return None;
    }
    Some(data[start..start + size].to_vec())
}

// ---------------------------------------------------------------------------
// Host function registration
// ---------------------------------------------------------------------------

/// Register the standard set of host functions in the `"env"` namespace.
///
/// The registered functions allow a WASM guest to log messages, write to
/// stdout/stderr/output, and read from the input buffer provided by the host.
///
/// # Host functions
///
/// | Name                | Signature                            | Description                          |
/// |---------------------|--------------------------------------|--------------------------------------|
/// | `host_log`          | `(ptr: i32, len: i32)`               | Log a UTF-8 message via `tracing`    |
/// | `host_write_stdout` | `(ptr: i32, len: i32)`               | Append bytes to `HostState::stdout`  |
/// | `host_write_stderr` | `(ptr: i32, len: i32)`               | Append bytes to `HostState::stderr`  |
/// | `host_write_output` | `(ptr: i32, len: i32)`               | Append bytes to `HostState::output`  |
/// | `host_read_input`   | `(buf_ptr: i32, buf_len: i32) -> i32`| Copy input bytes into guest memory   |
/// | `host_get_input_len`| `() -> i32`                          | Return total input buffer length     |
pub fn register_host_functions(linker: &mut wasmi::Linker<HostState>) -> Result<(), wasmi::Error> {
    // -- host_log(ptr, len) ---------------------------------------------------
    linker.func_wrap(
        "env",
        "host_log",
        |caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| {
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return;
            };
            let mem_size = memory.data(&caller).len();
            caller.data().track_memory_peek(mem_size);

            if let Some(msg) = read_wasm_string(&memory, &caller, ptr, len) {
                debug!(target: "sandbox::guest", "{}", msg);
            }
        },
    )?;

    // -- host_write_stdout(ptr, len) ------------------------------------------
    linker.func_wrap(
        "env",
        "host_write_stdout",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| {
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return;
            };
            let mem_size = memory.data(&caller).len();

            // Capability check — read bytes first, then mutate state.
            let has_cap = caller.data().has_capability(&Capability::Stdout);
            if !has_cap {
                return;
            }

            let bytes = match read_wasm_bytes(&memory, &caller, ptr, len) {
                Some(b) => b,
                None => return,
            };

            let host = caller.data_mut();
            host.track_memory(mem_size);
            host.stdout.extend_from_slice(&bytes);
        },
    )?;

    // -- host_write_stderr(ptr, len) ------------------------------------------
    linker.func_wrap(
        "env",
        "host_write_stderr",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| {
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return;
            };
            let mem_size = memory.data(&caller).len();

            let has_cap = caller.data().has_capability(&Capability::Stderr);
            if !has_cap {
                return;
            }

            let bytes = match read_wasm_bytes(&memory, &caller, ptr, len) {
                Some(b) => b,
                None => return,
            };

            let host = caller.data_mut();
            host.track_memory(mem_size);
            host.stderr.extend_from_slice(&bytes);
        },
    )?;

    // -- host_write_output(ptr, len) ------------------------------------------
    linker.func_wrap(
        "env",
        "host_write_output",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| {
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return;
            };
            let mem_size = memory.data(&caller).len();

            let bytes = match read_wasm_bytes(&memory, &caller, ptr, len) {
                Some(b) => b,
                None => return,
            };

            let host = caller.data_mut();
            host.track_memory(mem_size);
            host.output.extend_from_slice(&bytes);
        },
    )?;

    // -- host_read_input(buf_ptr, buf_len) -> i32 -----------------------------
    linker.func_wrap(
        "env",
        "host_read_input",
        |mut caller: wasmi::Caller<'_, HostState>, buf_ptr: i32, buf_len: i32| -> i32 {
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return 0;
            };

            if buf_ptr < 0 || buf_len < 0 {
                return 0;
            }

            let dst_start = buf_ptr as usize;
            let dst_cap = buf_len as usize;

            // Determine how many bytes remain in the input.
            let input_pos = caller.data().input_pos;
            let remaining = caller.data().input.len().saturating_sub(input_pos);
            let to_copy = remaining.min(dst_cap);

            if to_copy == 0 {
                return 0;
            }

            // Validate destination bounds.
            let mem_size = memory.data(&caller).len();
            if dst_start.saturating_add(to_copy) > mem_size {
                return 0;
            }

            // Copy the input slice into a temporary buffer so we can release
            // the shared reference before borrowing mutably.
            let src_bytes: Vec<u8> = caller.data().input[input_pos..input_pos + to_copy].to_vec();

            // Write into WASM memory.
            let data = memory.data_mut(&mut caller);
            data[dst_start..dst_start + to_copy].copy_from_slice(&src_bytes);

            // Advance input position and track memory.
            let host = caller.data_mut();
            host.input_pos += to_copy;
            host.track_memory(mem_size);

            to_copy as i32
        },
    )?;

    // -- host_get_input_len() -> i32 ------------------------------------------
    linker.func_wrap(
        "env",
        "host_get_input_len",
        |caller: wasmi::Caller<'_, HostState>| -> i32 { caller.data().input.len() as i32 },
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers used inside closures that only have shared access
// ---------------------------------------------------------------------------

impl HostState {
    /// Non-mutating peek at memory size for use inside closures that hold a
    /// shared `Caller` reference. The actual peak update happens later via
    /// [`track_memory`].
    fn track_memory_peek(&self, _current_bytes: usize) {
        // Intentional no-op: the host_log function only has a shared reference
        // so it cannot update peak. The next mutable host call will record it.
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // -- HostState creation ---------------------------------------------------

    #[test]
    fn test_host_state_new() {
        let input = b"hello world".to_vec();
        let caps = vec![Capability::Stdout, Capability::Stderr];
        let state = HostState::new(input.clone(), caps.clone());

        assert!(state.stdout.is_empty());
        assert!(state.stderr.is_empty());
        assert!(state.output.is_empty());
        assert_eq!(state.input, input);
        assert_eq!(state.input_pos, 0);
        assert_eq!(state.capabilities, caps);
        assert_eq!(state.memory_peak, 0);
    }

    // -- Capability checks ----------------------------------------------------

    #[test]
    fn test_host_state_has_stdout_capability() {
        let state = HostState::new(Vec::new(), vec![Capability::Stdout]);

        assert!(state.has_capability(&Capability::Stdout));
        assert!(!state.has_capability(&Capability::Stderr));
    }

    #[test]
    fn test_host_state_has_stderr_capability() {
        let state = HostState::new(Vec::new(), vec![Capability::Stderr]);

        assert!(state.has_capability(&Capability::Stderr));
        assert!(!state.has_capability(&Capability::Stdout));
    }

    #[test]
    fn test_host_state_no_capability() {
        let state = HostState::new(Vec::new(), Vec::new());

        assert!(!state.has_capability(&Capability::Stdout));
        assert!(!state.has_capability(&Capability::Stderr));
        assert!(!state.has_capability(&Capability::FileRead(vec![PathBuf::from("/tmp")])));
    }

    // -- Memory tracking ------------------------------------------------------

    #[test]
    fn test_host_state_track_memory() {
        let mut state = HostState::new(Vec::new(), Vec::new());

        assert_eq!(state.memory_peak, 0);
        state.track_memory(1024);
        assert_eq!(state.memory_peak, 1024);
        state.track_memory(4096);
        assert_eq!(state.memory_peak, 4096);
    }

    #[test]
    fn test_host_state_track_memory_no_decrease() {
        let mut state = HostState::new(Vec::new(), Vec::new());

        state.track_memory(8192);
        assert_eq!(state.memory_peak, 8192);

        // A smaller value should not decrease the peak.
        state.track_memory(4096);
        assert_eq!(state.memory_peak, 8192);

        // An equal value should not change it either.
        state.track_memory(8192);
        assert_eq!(state.memory_peak, 8192);
    }

    // -- FileRead capability path matching ------------------------------------

    #[test]
    fn test_host_state_has_file_read_capability() {
        let state = HostState::new(
            Vec::new(),
            vec![Capability::FileRead(vec![
                PathBuf::from("/tmp"),
                PathBuf::from("/home/user/data"),
            ])],
        );

        // Exact match on an allowed prefix.
        assert!(state.has_capability(&Capability::FileRead(vec![PathBuf::from("/tmp")])));

        // Sub-path within an allowed prefix.
        assert!(
            state.has_capability(&Capability::FileRead(vec![PathBuf::from(
                "/tmp/foo/bar.txt"
            )]))
        );

        // Another allowed prefix.
        assert!(
            state.has_capability(&Capability::FileRead(vec![PathBuf::from(
                "/home/user/data/report.csv"
            )]))
        );

        // Path not covered by any prefix.
        assert!(!state.has_capability(&Capability::FileRead(vec![PathBuf::from("/etc/passwd")])));

        // A different capability variant should not match.
        assert!(!state.has_capability(&Capability::FileWrite(vec![PathBuf::from("/tmp")])));
    }

    // -- Registration smoke test ----------------------------------------------

    #[test]
    fn test_register_host_functions() {
        let engine = wasmi::Engine::default();
        let mut linker = wasmi::Linker::<HostState>::new(&engine);

        // Should not panic.
        register_host_functions(&mut linker).expect("registration should succeed");
    }
}
