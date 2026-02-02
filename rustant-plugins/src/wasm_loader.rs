//! WASM plugin loader — loads sandboxed plugins via wasmi.
//!
//! WASM plugins run in a sandboxed environment with capability-based permissions.

use crate::{Plugin, PluginError, PluginMetadata, PluginToolDef};
use async_trait::async_trait;
use std::path::Path;

/// Loader for WASM plugins.
pub struct WasmPluginLoader;

impl WasmPluginLoader {
    /// Create a new WASM plugin loader.
    pub fn new() -> Self {
        Self
    }

    /// Load a WASM plugin from bytes.
    pub fn load_from_bytes(
        &self,
        name: &str,
        wasm_bytes: &[u8],
    ) -> Result<Box<dyn Plugin>, PluginError> {
        // Validate the WASM module
        let engine = wasmi::Engine::default();
        let module = wasmi::Module::new(&engine, wasm_bytes).map_err(|e| {
            PluginError::LoadFailed(format!("Invalid WASM module '{}': {}", name, e))
        })?;

        Ok(Box::new(WasmPlugin {
            name: name.into(),
            engine,
            module,
            store: None,
        }))
    }

    /// Load a WASM plugin from a file.
    pub fn load_from_file(&self, path: &Path) -> Result<Box<dyn Plugin>, PluginError> {
        let bytes = std::fs::read(path).map_err(|e| {
            PluginError::LoadFailed(format!("Failed to read '{}': {}", path.display(), e))
        })?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        self.load_from_bytes(name, &bytes)
    }
}

impl Default for WasmPluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// A WASM plugin loaded via wasmi.
struct WasmPlugin {
    name: String,
    #[allow(dead_code)]
    engine: wasmi::Engine,
    #[allow(dead_code)]
    module: wasmi::Module,
    #[allow(dead_code)]
    store: Option<wasmi::Store<()>>,
}

// Safety: WasmPlugin is single-threaded internally but we only access it under a mutex
unsafe impl Send for WasmPlugin {}
unsafe impl Sync for WasmPlugin {}

#[async_trait]
impl Plugin for WasmPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: self.name.clone(),
            version: "0.1.0".into(),
            description: format!("WASM plugin: {}", self.name),
            author: None,
            min_core_version: None,
            capabilities: vec![],
        }
    }

    async fn on_load(&mut self) -> Result<(), PluginError> {
        // Create a store for the module
        self.store = Some(wasmi::Store::new(&self.engine, ()));
        tracing::info!(plugin = %self.name, "WASM plugin loaded");
        Ok(())
    }

    async fn on_unload(&mut self) -> Result<(), PluginError> {
        self.store = None;
        tracing::info!(plugin = %self.name, "WASM plugin unloaded");
        Ok(())
    }

    fn tools(&self) -> Vec<PluginToolDef> {
        // WASM plugins can export tool definitions
        // For now, return empty — full implementation would call exported functions
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal valid WASM module (empty module)
    const MINIMAL_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, // magic: \0asm
        0x01, 0x00, 0x00, 0x00, // version: 1
    ];

    #[test]
    fn test_wasm_loader_from_bytes() {
        let loader = WasmPluginLoader::new();
        let result = loader.load_from_bytes("test", MINIMAL_WASM);
        assert!(result.is_ok());

        let plugin = result.unwrap();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "test");
    }

    #[test]
    fn test_wasm_loader_invalid_bytes() {
        let loader = WasmPluginLoader::new();
        let result = loader.load_from_bytes("bad", &[0x00, 0x01, 0x02]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wasm_plugin_lifecycle() {
        let loader = WasmPluginLoader::new();
        let mut plugin = loader.load_from_bytes("lifecycle", MINIMAL_WASM).unwrap();

        plugin.on_load().await.unwrap();
        assert_eq!(plugin.metadata().name, "lifecycle");

        let tools = plugin.tools();
        assert!(tools.is_empty());

        plugin.on_unload().await.unwrap();
    }

    #[test]
    fn test_wasm_loader_from_file_not_found() {
        let loader = WasmPluginLoader::new();
        let result = loader.load_from_file(Path::new("/nonexistent/plugin.wasm"));
        assert!(result.is_err());
    }

    #[test]
    fn test_wasm_loader_from_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let wasm_path = dir.path().join("test.wasm");
        std::fs::write(&wasm_path, MINIMAL_WASM).unwrap();

        let loader = WasmPluginLoader::new();
        let result = loader.load_from_file(&wasm_path);
        assert!(result.is_ok());
    }
}
