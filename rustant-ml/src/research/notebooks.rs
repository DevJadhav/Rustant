//! Jupyter notebook execution and management.

use crate::error::MlError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Notebook execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookResult {
    pub path: PathBuf,
    pub cells_executed: usize,
    pub cells_failed: usize,
    pub execution_time_secs: f64,
    pub outputs: Vec<CellOutput>,
}

/// Output from a notebook cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellOutput {
    pub cell_index: usize,
    pub output_type: String,
    pub text: Option<String>,
    pub error: Option<String>,
}

/// Notebook executor.
pub struct NotebookExecutor {
    workspace: PathBuf,
}

impl NotebookExecutor {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    pub async fn execute(&self, path: &Path) -> Result<NotebookResult, MlError> {
        tracing::info!(path = %path.display(), "Notebook execution (stub)");
        Ok(NotebookResult {
            path: path.to_path_buf(),
            cells_executed: 0,
            cells_failed: 0,
            execution_time_secs: 0.0,
            outputs: Vec::new(),
        })
    }

    /// Convenience alias for [`execute`](Self::execute).
    pub async fn run(&self, notebook_path: &Path) -> Result<NotebookResult, MlError> {
        self.execute(notebook_path).await
    }

    /// Convert notebook to specified output format (html, pdf, py).
    pub async fn convert(
        &self,
        notebook_path: &Path,
        output_format: &str,
    ) -> Result<PathBuf, MlError> {
        tracing::info!(path = %notebook_path.display(), format = output_format, "Notebook conversion (stub)");
        let stem = notebook_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("notebook");
        let output_path = self.workspace.join(format!("{stem}.{output_format}"));
        Ok(output_path)
    }

    /// Validate notebook structure.
    pub async fn validate(&self, notebook_path: &Path) -> Result<NotebookValidation, MlError> {
        tracing::info!(path = %notebook_path.display(), "Notebook validation (stub)");
        Ok(NotebookValidation {
            is_valid: true,
            total_cells: 0,
            code_cells: 0,
            markdown_cells: 0,
            issues: Vec::new(),
        })
    }
}

/// Result of notebook structural validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookValidation {
    pub is_valid: bool,
    pub total_cells: usize,
    pub code_cells: usize,
    pub markdown_cells: usize,
    pub issues: Vec<String>,
}
