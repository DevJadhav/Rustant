//! Runtime Registry — stub for pre-compiled WASM language runtimes.
//!
//! Designed for future extension with QuickJS, RustPython, etc.
//! Currently provides the registry interface without shipping actual runtimes.

use std::collections::HashMap;

/// A pre-compiled WASM runtime for a specific language.
#[derive(Debug, Clone)]
pub struct WasmRuntime {
    /// Runtime identifier (e.g., "quickjs", "rustpython").
    pub name: String,
    /// Language this runtime supports.
    pub language: String,
    /// Version string.
    pub version: String,
    /// Whether the runtime is available (compiled and loadable).
    pub available: bool,
}

/// Registry of available WASM language runtimes.
pub struct RuntimeRegistry {
    runtimes: HashMap<String, WasmRuntime>,
}

impl RuntimeRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            runtimes: HashMap::new(),
        }
    }

    /// Create a registry with stub entries for known runtimes.
    pub fn with_stubs() -> Self {
        let mut registry = Self::new();

        // Stub entries — not yet available
        registry.register(WasmRuntime {
            name: "quickjs".into(),
            language: "javascript".into(),
            version: "0.0.0".into(),
            available: false,
        });

        registry.register(WasmRuntime {
            name: "rustpython".into(),
            language: "python".into(),
            version: "0.0.0".into(),
            available: false,
        });

        registry
    }

    /// Register a runtime.
    pub fn register(&mut self, runtime: WasmRuntime) {
        self.runtimes.insert(runtime.name.clone(), runtime);
    }

    /// Get a runtime by name.
    pub fn get(&self, name: &str) -> Option<&WasmRuntime> {
        self.runtimes.get(name)
    }

    /// List all registered runtimes.
    pub fn list(&self) -> Vec<&WasmRuntime> {
        self.runtimes.values().collect()
    }

    /// List available (loadable) runtimes.
    pub fn available(&self) -> Vec<&WasmRuntime> {
        self.runtimes
            .values()
            .filter(|r| r.available)
            .collect()
    }
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self::with_stubs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new_empty() {
        let registry = RuntimeRegistry::new();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_registry_with_stubs() {
        let registry = RuntimeRegistry::with_stubs();
        assert_eq!(registry.list().len(), 2);
        assert!(registry.available().is_empty()); // All stubs are unavailable
    }

    #[test]
    fn test_get_runtime() {
        let registry = RuntimeRegistry::with_stubs();
        let qjs = registry.get("quickjs");
        assert!(qjs.is_some());
        assert_eq!(qjs.unwrap().language, "javascript");
        assert!(!qjs.unwrap().available);
    }

    #[test]
    fn test_register_custom() {
        let mut registry = RuntimeRegistry::new();
        registry.register(WasmRuntime {
            name: "lua".into(),
            language: "lua".into(),
            version: "5.4.0".into(),
            available: true,
        });
        assert_eq!(registry.available().len(), 1);
    }
}
