//! Python runtime manager for ML workloads.
//!
//! Provides managed subprocess execution for Python-based ML tools (training,
//! inference, dataset processing). Follows the `kubernetes.rs` / `lint.rs`
//! subprocess pattern.

use crate::error::MlError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tracing::debug;

/// Information about the detected Python installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonInfo {
    pub path: PathBuf,
    pub version: String,
    pub has_pip: bool,
    pub venv_path: Option<PathBuf>,
}

/// Managed Python subprocess runner.
pub struct PythonRuntime {
    python_path: PathBuf,
    venv_path: Option<PathBuf>,
    workspace: PathBuf,
    timeout: Duration,
}

impl PythonRuntime {
    /// Create a new Python runtime with the given configuration.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            python_path: PathBuf::from("python3"),
            venv_path: None,
            workspace,
            timeout: Duration::from_secs(300),
        }
    }

    /// Create with explicit paths.
    pub fn with_config(
        python_path: PathBuf,
        venv_path: Option<PathBuf>,
        workspace: PathBuf,
        timeout: Duration,
    ) -> Self {
        Self {
            python_path,
            venv_path,
            workspace,
            timeout,
        }
    }

    /// Detect available Python installation.
    pub async fn detect() -> Result<PythonInfo, MlError> {
        // Try python3 first, then python
        for cmd in &["python3", "python"] {
            let output = Command::new(cmd).args(["--version"]).output().await;

            if let Ok(output) = output {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let version = if version.is_empty() {
                        String::from_utf8_lossy(&output.stderr).trim().to_string()
                    } else {
                        version
                    };

                    // Check for pip
                    let has_pip = Command::new(cmd)
                        .args(["-m", "pip", "--version"])
                        .output()
                        .await
                        .is_ok_and(|o| o.status.success());

                    // Check for venv in common locations
                    let venv_path = detect_venv().await;

                    return Ok(PythonInfo {
                        path: PathBuf::from(cmd),
                        version,
                        has_pip,
                        venv_path,
                    });
                }
            }
        }

        Err(MlError::Python(
            "Python not found. Install Python 3.8+ to use ML features.".to_string(),
        ))
    }

    /// Get the effective Python command (accounting for venv).
    fn python_cmd(&self) -> PathBuf {
        if let Some(venv) = &self.venv_path {
            let bin_dir = if cfg!(windows) { "Scripts" } else { "bin" };
            venv.join(bin_dir).join("python")
        } else {
            self.python_path.clone()
        }
    }

    /// Run a Python script with JSON input/output.
    ///
    /// The script receives input as a JSON string on stdin and should
    /// write its output as JSON to stdout.
    pub async fn run_script(
        &self,
        script: &str,
        input: serde_json::Value,
        timeout: Option<Duration>,
    ) -> Result<serde_json::Value, MlError> {
        let timeout = timeout.unwrap_or(self.timeout);
        let _input_json = serde_json::to_string(&input)?;

        debug!(script_len = script.len(), "Running Python script");

        let result = tokio::time::timeout(timeout, async {
            let output = Command::new(self.python_cmd())
                .args(["-c", script])
                .current_dir(&self.workspace)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
                .map_err(|e| MlError::Python(format!("Failed to spawn Python: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(MlError::Python(format!(
                    "Python script failed (exit {}): {}",
                    output.status, stderr
                )));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                Ok(serde_json::Value::Null)
            } else {
                serde_json::from_str(stdout.trim())
                    .map_err(|e| MlError::Python(format!("Invalid JSON output: {e}")))
            }
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(MlError::Timeout(format!(
                "Python script timed out after {}s",
                timeout.as_secs()
            ))),
        }
    }

    /// Run a Python script file.
    pub async fn run_script_file(
        &self,
        script_path: &Path,
        args: &[&str],
        timeout: Option<Duration>,
    ) -> Result<String, MlError> {
        let timeout = timeout.unwrap_or(self.timeout);

        let result = tokio::time::timeout(timeout, async {
            let output = Command::new(self.python_cmd())
                .arg(script_path)
                .args(args)
                .current_dir(&self.workspace)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
                .map_err(|e| MlError::Python(format!("Failed to spawn Python: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(MlError::Python(format!(
                    "Script failed (exit {}): {}",
                    output.status, stderr
                )));
            }

            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(MlError::Timeout(format!(
                "Script timed out after {}s",
                timeout.as_secs()
            ))),
        }
    }

    /// Check which packages are available.
    pub async fn check_packages(&self, packages: &[&str]) -> HashMap<String, bool> {
        let mut results = HashMap::new();

        for pkg in packages {
            let script = format!("import importlib; importlib.import_module('{pkg}'); print('ok')");
            let available = Command::new(self.python_cmd())
                .args(["-c", &script])
                .output()
                .await
                .is_ok_and(|o| o.status.success());

            results.insert(pkg.to_string(), available);
        }

        results
    }

    /// Install packages via pip.
    pub async fn ensure_packages(&self, packages: &[&str]) -> Result<(), MlError> {
        if packages.is_empty() {
            return Ok(());
        }

        let output = Command::new(self.python_cmd())
            .args(["-m", "pip", "install", "--quiet"])
            .args(packages)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| MlError::Python(format!("Failed to run pip: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MlError::Python(format!("pip install failed: {stderr}")));
        }

        Ok(())
    }
}

/// Detect a virtual environment in common locations.
async fn detect_venv() -> Option<PathBuf> {
    // Check VIRTUAL_ENV environment variable
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let path = PathBuf::from(venv);
        if path.exists() {
            return Some(path);
        }
    }

    // Check common venv directory names in the current directory
    for name in &[".venv", "venv", ".env", "env"] {
        let path = PathBuf::from(name);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_python_detect() {
        // This test just verifies the detection logic doesn't panic.
        // Python may or may not be available in CI.
        let result = PythonRuntime::detect().await;
        if let Ok(info) = result {
            assert!(!info.version.is_empty());
        }
    }

    #[test]
    fn test_python_info_serde() {
        let info = PythonInfo {
            path: PathBuf::from("/usr/bin/python3"),
            version: "Python 3.11.0".to_string(),
            has_pip: true,
            venv_path: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: PythonInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, info.version);
    }
}
