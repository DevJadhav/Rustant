//! Native plugin loader — loads .so/.dll/.dylib plugins via libloading.
//!
//! Native plugins expose a `rustant_plugin_create` symbol that returns a boxed Plugin trait object.

use crate::{Plugin, PluginError};
use std::path::{Path, PathBuf};

/// Loader for native dynamic library plugins.
pub struct NativePluginLoader {
    search_dirs: Vec<PathBuf>,
}

impl NativePluginLoader {
    /// Create a new native plugin loader.
    pub fn new() -> Self {
        Self {
            search_dirs: Vec::new(),
        }
    }

    /// Add a directory to search for plugin libraries.
    pub fn add_search_dir(&mut self, dir: impl Into<PathBuf>) {
        self.search_dirs.push(dir.into());
    }

    /// List available plugin libraries in search directories.
    pub fn discover(&self) -> Vec<PathBuf> {
        let mut plugins = Vec::new();
        for dir in &self.search_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if is_plugin_library(&path) {
                        plugins.push(path);
                    }
                }
            }
        }
        plugins
    }

    /// Load a plugin from a dynamic library path.
    ///
    /// # Safety
    ///
    /// Loading native plugins executes arbitrary code. Only load trusted plugins.
    pub unsafe fn load(&self, path: &Path) -> Result<Box<dyn Plugin>, PluginError> {
        let lib = libloading::Library::new(path)
            .map_err(|e| PluginError::LoadFailed(format!("{}: {}", path.display(), e)))?;

        // Look for the plugin creation function
        let create_fn: libloading::Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> =
            lib.get(b"rustant_plugin_create").map_err(|e| {
                PluginError::LoadFailed(format!(
                    "Symbol 'rustant_plugin_create' not found in {}: {}",
                    path.display(),
                    e
                ))
            })?;

        let raw = create_fn();
        if raw.is_null() {
            return Err(PluginError::LoadFailed(
                "Plugin creation function returned null".into(),
            ));
        }

        let plugin = Box::from_raw(raw);

        // Keep the library alive by leaking it (plugin owns the code)
        std::mem::forget(lib);

        Ok(plugin)
    }
}

impl Default for NativePluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a file path looks like a plugin shared library.
fn is_plugin_library(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "so" | "dll" | "dylib")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_plugin_library() {
        assert!(is_plugin_library(Path::new("libfoo.so")));
        assert!(is_plugin_library(Path::new("foo.dll")));
        assert!(is_plugin_library(Path::new("libfoo.dylib")));
        assert!(!is_plugin_library(Path::new("foo.rs")));
        assert!(!is_plugin_library(Path::new("foo.toml")));
        assert!(!is_plugin_library(Path::new("foo")));
    }

    #[test]
    fn test_native_loader_discover_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut loader = NativePluginLoader::new();
        loader.add_search_dir(dir.path());
        let plugins = loader.discover();
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_native_loader_discover_finds_libs() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create fake library files
        std::fs::write(dir.path().join("libplugin.so"), b"fake").unwrap();
        std::fs::write(dir.path().join("plugin.dll"), b"fake").unwrap();
        std::fs::write(dir.path().join("README.md"), b"docs").unwrap();

        let mut loader = NativePluginLoader::new();
        loader.add_search_dir(dir.path());
        let plugins = loader.discover();
        assert_eq!(plugins.len(), 2);
    }

    #[test]
    fn test_native_loader_discover_nonexistent_dir() {
        let mut loader = NativePluginLoader::new();
        loader.add_search_dir("/nonexistent/path");
        let plugins = loader.discover();
        assert!(plugins.is_empty());
    }
}
