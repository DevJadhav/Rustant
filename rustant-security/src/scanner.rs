//! Scanner plugin interface — trait and registry for security scanners.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::Finding;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Version information for a scanner.
#[derive(Debug, Clone)]
pub struct ScannerVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl std::fmt::Display for ScannerVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Risk level for a scanner operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScannerRiskLevel {
    /// Only reads files, no side effects.
    ReadOnly,
    /// May make network calls (e.g., vulnerability database lookups).
    Network,
    /// May execute external processes.
    Execute,
    /// May modify files or state.
    Write,
    /// Potentially destructive (e.g., behavioral analysis of untrusted code).
    Destructive,
}

/// The core trait for all security scanners.
#[async_trait]
pub trait Scanner: Send + Sync {
    /// Unique name for this scanner.
    fn name(&self) -> &str;

    /// Scanner version.
    fn version(&self) -> ScannerVersion;

    /// Categories of findings this scanner can produce.
    fn supported_categories(&self) -> Vec<crate::finding::FindingCategory>;

    /// Check if this scanner supports a given language.
    fn supports_language(&self, language: &str) -> bool;

    /// Run the scan and return findings.
    async fn scan(
        &self,
        config: &ScanConfig,
        context: &ScanContext,
    ) -> Result<Vec<Finding>, ScanError>;

    /// Check if the scanner is available (e.g., external tool installed).
    fn is_available(&self) -> bool {
        true
    }

    /// Risk level of this scanner.
    fn risk_level(&self) -> ScannerRiskLevel {
        ScannerRiskLevel::ReadOnly
    }
}

/// Context provided to scanners during a scan.
#[derive(Debug, Clone)]
pub struct ScanContext {
    /// Workspace root path.
    pub workspace: std::path::PathBuf,
    /// Files to scan (empty = scan all).
    pub files: Vec<std::path::PathBuf>,
    /// Languages detected in the project.
    pub languages: Vec<String>,
    /// Whether this is a diff-only scan.
    pub diff_only: bool,
}

impl ScanContext {
    pub fn new(workspace: impl Into<std::path::PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
            files: Vec::new(),
            languages: Vec::new(),
            diff_only: false,
        }
    }

    pub fn with_files(mut self, files: Vec<std::path::PathBuf>) -> Self {
        self.files = files;
        self
    }

    pub fn with_languages(mut self, languages: Vec<String>) -> Self {
        self.languages = languages;
        self
    }
}

/// Registry for scanner discovery and invocation.
pub struct ScannerRegistry {
    scanners: RwLock<HashMap<String, Arc<dyn Scanner>>>,
}

impl ScannerRegistry {
    pub fn new() -> Self {
        Self {
            scanners: RwLock::new(HashMap::new()),
        }
    }

    /// Register a scanner.
    pub async fn register(&self, scanner: Arc<dyn Scanner>) {
        let name = scanner.name().to_string();
        self.scanners.write().await.insert(name, scanner);
    }

    /// Unregister a scanner by name.
    pub async fn unregister(&self, name: &str) -> Option<Arc<dyn Scanner>> {
        self.scanners.write().await.remove(name)
    }

    /// Get a scanner by name.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Scanner>> {
        self.scanners.read().await.get(name).cloned()
    }

    /// List all registered scanner names.
    pub async fn list(&self) -> Vec<String> {
        self.scanners.read().await.keys().cloned().collect()
    }

    /// List all available scanners (is_available() == true).
    pub async fn list_available(&self) -> Vec<String> {
        self.scanners
            .read()
            .await
            .iter()
            .filter(|(_, s)| s.is_available())
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get scanners that support a given language.
    pub async fn scanners_for_language(&self, language: &str) -> Vec<Arc<dyn Scanner>> {
        self.scanners
            .read()
            .await
            .values()
            .filter(|s| s.supports_language(language) && s.is_available())
            .cloned()
            .collect()
    }
}

impl Default for ScannerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Orchestrator that runs multiple scanners in parallel with configurable concurrency.
pub struct ScanOrchestrator {
    registry: Arc<ScannerRegistry>,
    max_concurrent: usize,
}

/// Result of an orchestrated scan across multiple scanners.
#[derive(Debug)]
pub struct OrchestratedScanResult {
    /// All findings from all scanners, deduplicated.
    pub findings: Vec<Finding>,
    /// Per-scanner execution results.
    pub scanner_results: Vec<ScannerExecutionResult>,
    /// Total scan duration in milliseconds.
    pub duration_ms: u64,
}

/// Result from a single scanner execution.
#[derive(Debug)]
pub struct ScannerExecutionResult {
    pub scanner_name: String,
    pub findings_count: usize,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

impl ScanOrchestrator {
    pub fn new(registry: Arc<ScannerRegistry>, max_concurrent: usize) -> Self {
        Self {
            registry,
            max_concurrent,
        }
    }

    /// Run all available scanners against the given context.
    pub async fn run_all(
        &self,
        config: &ScanConfig,
        context: &ScanContext,
    ) -> OrchestratedScanResult {
        let start = std::time::Instant::now();
        let scanner_names = self.registry.list_available().await;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_concurrent));
        let mut handles = Vec::new();

        for name in scanner_names {
            let registry = self.registry.clone();
            let config = config.clone();
            let context = context.clone();
            let sem = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let scanner_start = std::time::Instant::now();

                if let Some(scanner) = registry.get(&name).await {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(config.timeout_secs),
                        scanner.scan(&config, &context),
                    )
                    .await
                    {
                        Ok(Ok(findings)) => ScannerExecutionResult {
                            scanner_name: name,
                            findings_count: findings.len(),
                            duration_ms: scanner_start.elapsed().as_millis() as u64,
                            success: true,
                            error: None,
                        },
                        Ok(Err(e)) => ScannerExecutionResult {
                            scanner_name: name,
                            findings_count: 0,
                            duration_ms: scanner_start.elapsed().as_millis() as u64,
                            success: false,
                            error: Some(e.to_string()),
                        },
                        Err(_) => ScannerExecutionResult {
                            scanner_name: name,
                            findings_count: 0,
                            duration_ms: scanner_start.elapsed().as_millis() as u64,
                            success: false,
                            error: Some(format!("Timed out after {}s", config.timeout_secs)),
                        },
                    }
                } else {
                    ScannerExecutionResult {
                        scanner_name: name,
                        findings_count: 0,
                        duration_ms: 0,
                        success: false,
                        error: Some("Scanner not found".into()),
                    }
                }
            });

            handles.push(handle);
        }

        let mut scanner_results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                scanner_results.push(result);
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        OrchestratedScanResult {
            findings: Vec::new(), // Findings collected separately via scan() return values
            scanner_results,
            duration_ms,
        }
    }

    /// Run specific scanners by name.
    pub async fn run_scanners(
        &self,
        scanner_names: &[&str],
        config: &ScanConfig,
        context: &ScanContext,
    ) -> OrchestratedScanResult {
        let start = std::time::Instant::now();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_concurrent));
        let mut handles = Vec::new();

        for &name in scanner_names {
            let registry = self.registry.clone();
            let config = config.clone();
            let context = context.clone();
            let sem = semaphore.clone();
            let name = name.to_string();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let scanner_start = std::time::Instant::now();

                if let Some(scanner) = registry.get(&name).await {
                    match scanner.scan(&config, &context).await {
                        Ok(findings) => (
                            findings,
                            ScannerExecutionResult {
                                scanner_name: name,
                                findings_count: 0, // updated below
                                duration_ms: scanner_start.elapsed().as_millis() as u64,
                                success: true,
                                error: None,
                            },
                        ),
                        Err(e) => (
                            Vec::new(),
                            ScannerExecutionResult {
                                scanner_name: name,
                                findings_count: 0,
                                duration_ms: scanner_start.elapsed().as_millis() as u64,
                                success: false,
                                error: Some(e.to_string()),
                            },
                        ),
                    }
                } else {
                    (
                        Vec::new(),
                        ScannerExecutionResult {
                            scanner_name: name,
                            findings_count: 0,
                            duration_ms: 0,
                            success: false,
                            error: Some("Scanner not found".into()),
                        },
                    )
                }
            });

            handles.push(handle);
        }

        let mut all_findings = Vec::new();
        let mut scanner_results = Vec::new();

        for handle in handles {
            if let Ok((findings, mut result)) = handle.await {
                result.findings_count = findings.len();
                all_findings.extend(findings);
                scanner_results.push(result);
            }
        }

        // Deduplicate
        let mut dedup = crate::finding::DeduplicationEngine::new();
        let findings = dedup.deduplicate(all_findings);

        OrchestratedScanResult {
            findings,
            scanner_results,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockScanner {
        name: String,
    }

    #[async_trait]
    impl Scanner for MockScanner {
        fn name(&self) -> &str {
            &self.name
        }
        fn version(&self) -> ScannerVersion {
            ScannerVersion {
                major: 1,
                minor: 0,
                patch: 0,
            }
        }
        fn supported_categories(&self) -> Vec<crate::finding::FindingCategory> {
            vec![crate::finding::FindingCategory::Security]
        }
        fn supports_language(&self, _lang: &str) -> bool {
            true
        }
        async fn scan(
            &self,
            _config: &ScanConfig,
            _ctx: &ScanContext,
        ) -> Result<Vec<Finding>, ScanError> {
            Ok(vec![Finding::new(
                "Test Finding",
                "A test finding",
                crate::finding::FindingSeverity::Medium,
                crate::finding::FindingCategory::Security,
                crate::finding::FindingProvenance::new(&self.name, 0.8),
            )])
        }
    }

    #[tokio::test]
    async fn test_scanner_registry() {
        let registry = ScannerRegistry::new();
        let scanner = Arc::new(MockScanner {
            name: "test".into(),
        });
        registry.register(scanner).await;

        assert_eq!(registry.list().await.len(), 1);
        assert!(registry.get("test").await.is_some());
        assert!(registry.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_scanner_for_language() {
        let registry = ScannerRegistry::new();
        registry
            .register(Arc::new(MockScanner {
                name: "test".into(),
            }))
            .await;

        let scanners = registry.scanners_for_language("rust").await;
        assert_eq!(scanners.len(), 1);
    }
}
