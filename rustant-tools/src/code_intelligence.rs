//! Code intelligence tool — cross-language codebase analysis, pattern detection,
//! tech debt scanning, API surface extraction, and dependency mapping.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::registry::Tool;

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LanguageStats {
    language: String,
    files: usize,
    lines: usize,
    extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DirectoryInfo {
    path: String,
    classification: String,
    file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArchitectureSnapshot {
    project_root: String,
    languages: Vec<LanguageStats>,
    directories: Vec<DirectoryInfo>,
    entry_points: Vec<String>,
    config_files: Vec<String>,
    total_files: usize,
    total_lines: usize,
    analyzed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PatternMatch {
    pattern_name: String,
    file_path: String,
    line_number: usize,
    snippet: String,
    confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TechDebtItem {
    file_path: String,
    line_number: usize,
    category: String,
    description: String,
    severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiEntry {
    name: String,
    kind: String,
    file_path: String,
    line_number: usize,
    signature: String,
    visibility: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DependencyEntry {
    name: String,
    version: String,
    dep_type: String,
    source_file: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CodeIntelCache {
    last_snapshot: Option<ArchitectureSnapshot>,
}

// ---------------------------------------------------------------------------
// Tool struct
// ---------------------------------------------------------------------------

pub struct CodeIntelligenceTool {
    workspace: PathBuf,
}

impl CodeIntelligenceTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("code_intel")
            .join("cache.json")
    }

    fn load_cache(&self) -> CodeIntelCache {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            CodeIntelCache::default()
        }
    }

    fn save_cache(&self, cache: &CodeIntelCache) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Failed to create cache dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(cache).map_err(|e| ToolError::ExecutionFailed {
            name: "code_intelligence".to_string(),
            message: format!("Failed to serialize cache: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "code_intelligence".to_string(),
            message: format!("Failed to write cache: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "code_intelligence".to_string(),
            message: format!("Failed to rename cache file: {}", e),
        })?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Resolve a path argument relative to the workspace.
    fn resolve_path(&self, args: &Value) -> PathBuf {
        args.get("path")
            .and_then(|v| v.as_str())
            .map(|p| {
                let pb = PathBuf::from(p);
                if pb.is_absolute() {
                    pb
                } else {
                    self.workspace.join(pb)
                }
            })
            .unwrap_or_else(|| self.workspace.clone())
    }

    /// Map a file extension to a language name.
    fn ext_to_language(ext: &str) -> Option<&'static str> {
        match ext {
            "rs" => Some("Rust"),
            "py" | "pyi" => Some("Python"),
            "js" | "mjs" | "cjs" => Some("JavaScript"),
            "ts" | "mts" | "cts" => Some("TypeScript"),
            "jsx" => Some("JavaScript (JSX)"),
            "tsx" => Some("TypeScript (TSX)"),
            "go" => Some("Go"),
            "java" => Some("Java"),
            "rb" => Some("Ruby"),
            "c" | "h" => Some("C"),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("C++"),
            "cs" => Some("C#"),
            "swift" => Some("Swift"),
            "kt" | "kts" => Some("Kotlin"),
            "sh" | "bash" | "zsh" => Some("Shell"),
            "html" | "htm" => Some("HTML"),
            "css" | "scss" | "sass" => Some("CSS"),
            "json" => Some("JSON"),
            "toml" => Some("TOML"),
            "yaml" | "yml" => Some("YAML"),
            "xml" => Some("XML"),
            "md" | "markdown" => Some("Markdown"),
            "sql" => Some("SQL"),
            _ => None,
        }
    }

    /// Check whether an extension is likely binary.
    fn is_binary_ext(ext: &str) -> bool {
        matches!(
            ext,
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "bmp"
                | "ico"
                | "svg"
                | "woff"
                | "woff2"
                | "ttf"
                | "otf"
                | "eot"
                | "pdf"
                | "zip"
                | "tar"
                | "gz"
                | "bz2"
                | "xz"
                | "7z"
                | "rar"
                | "exe"
                | "dll"
                | "so"
                | "dylib"
                | "o"
                | "a"
                | "class"
                | "jar"
                | "war"
                | "pyc"
                | "pyo"
                | "wasm"
                | "db"
                | "sqlite"
                | "lock"
        )
    }

    /// Classify a directory name.
    fn classify_dir(name: &str) -> &'static str {
        match name {
            "src" | "lib" | "app" | "pkg" | "internal" | "cmd" => "source",
            "test" | "tests" | "spec" | "specs" | "__tests__" | "test_data" | "testdata" => "test",
            "doc" | "docs" | "documentation" => "docs",
            "build" | "target" | "dist" | "out" | "output" | "bin" | "obj" => "build",
            "vendor" | "node_modules" | "third_party" | "external" | "deps" => "vendor",
            "config" | "configs" | "conf" | "etc" | "settings" | ".github" | ".vscode" => "config",
            _ => "source",
        }
    }

    /// Check if a filename is an entry point.
    fn is_entry_point(name: &str) -> bool {
        matches!(
            name,
            "main.rs"
                | "main.py"
                | "__main__.py"
                | "index.js"
                | "index.ts"
                | "index.tsx"
                | "index.jsx"
                | "main.go"
                | "Main.java"
                | "Program.cs"
                | "main.c"
                | "main.cpp"
                | "main.rb"
                | "app.py"
                | "app.js"
                | "app.ts"
                | "server.js"
                | "server.ts"
                | "manage.py"
        )
    }

    /// Check if a filename is a config file.
    fn is_config_file(name: &str) -> bool {
        matches!(
            name,
            "Cargo.toml"
                | "package.json"
                | "tsconfig.json"
                | "pyproject.toml"
                | "setup.py"
                | "setup.cfg"
                | "requirements.txt"
                | "go.mod"
                | "go.sum"
                | "Gemfile"
                | "Makefile"
                | "CMakeLists.txt"
                | "Dockerfile"
                | "docker-compose.yml"
                | "docker-compose.yaml"
                | ".gitignore"
                | ".editorconfig"
                | "jest.config.js"
                | "jest.config.ts"
                | "webpack.config.js"
                | "vite.config.ts"
                | "vite.config.js"
                | "babel.config.js"
                | ".eslintrc.json"
                | ".eslintrc.js"
                | ".prettierrc"
                | "tox.ini"
                | "Pipfile"
                | "poetry.lock"
                | ".env.example"
        )
    }

    /// Count lines in a string.
    fn count_lines(content: &str) -> usize {
        if content.is_empty() {
            0
        } else {
            content.lines().count()
        }
    }

    // -----------------------------------------------------------------------
    // Actions
    // -----------------------------------------------------------------------

    fn analyze_architecture(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

        // Return cached if not forcing and cache exists.
        if !force {
            let cache = self.load_cache();
            if let Some(snapshot) = &cache.last_snapshot {
                let out = serde_json::to_string_pretty(snapshot).unwrap_or_default();
                return Ok(ToolOutput::text(format!(
                    "Architecture snapshot (cached):\n{}",
                    out
                )));
            }
        }

        let root = self.resolve_path(args);
        if !root.exists() {
            return Err(ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Path does not exist: {}", root.display()),
            });
        }

        // language -> (files, lines, extensions set)
        let mut lang_map: HashMap<String, (usize, usize, std::collections::HashSet<String>)> =
            HashMap::new();
        // dir relative path -> (classification, file count)
        let mut dir_map: HashMap<String, (String, usize)> = HashMap::new();
        let mut entry_points: Vec<String> = Vec::new();
        let mut config_files: Vec<String> = Vec::new();
        let mut total_files: usize = 0;
        let mut total_lines: usize = 0;
        let max_files: usize = 5000;

        let walker = ignore::WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            if total_files >= max_files {
                break;
            }

            let path = entry.path();
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            // Skip binary files.
            if Self::is_binary_ext(&ext) {
                continue;
            }

            total_files += 1;

            // Entry points.
            if Self::is_entry_point(&file_name) {
                entry_points.push(rel.clone());
            }

            // Config files.
            if Self::is_config_file(&file_name) {
                config_files.push(rel.clone());
            }

            // Count lines.
            let lines = std::fs::read_to_string(path)
                .map(|c| Self::count_lines(&c))
                .unwrap_or(0);
            total_lines += lines;

            // Language stats.
            if let Some(lang) = Self::ext_to_language(&ext) {
                let entry = lang_map
                    .entry(lang.to_string())
                    .or_insert_with(|| (0, 0, std::collections::HashSet::new()));
                entry.0 += 1;
                entry.1 += lines;
                entry.2.insert(ext.clone());
            }

            // Directory classification.
            if let Some(parent) = path.parent() {
                let dir_rel = parent
                    .strip_prefix(&root)
                    .unwrap_or(parent)
                    .to_string_lossy()
                    .to_string();
                let dir_name = parent
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| dir_rel.clone());
                let classification = Self::classify_dir(&dir_name).to_string();
                let dir_entry = dir_map
                    .entry(dir_rel)
                    .or_insert_with(|| (classification, 0));
                dir_entry.1 += 1;
            }
        }

        // Build language stats, sorted by file count descending.
        let mut languages: Vec<LanguageStats> = lang_map
            .into_iter()
            .map(|(lang, (files, lines, exts))| {
                let mut ext_vec: Vec<String> = exts.into_iter().collect();
                ext_vec.sort();
                LanguageStats {
                    language: lang,
                    files,
                    lines,
                    extensions: ext_vec,
                }
            })
            .collect();
        languages.sort_by(|a, b| b.files.cmp(&a.files));

        // Build directory list, sorted by file count descending.
        let mut directories: Vec<DirectoryInfo> = dir_map
            .into_iter()
            .map(|(path, (classification, file_count))| DirectoryInfo {
                path,
                classification,
                file_count,
            })
            .collect();
        directories.sort_by(|a, b| b.file_count.cmp(&a.file_count));

        entry_points.sort();
        config_files.sort();

        let snapshot = ArchitectureSnapshot {
            project_root: root.to_string_lossy().to_string(),
            languages,
            directories,
            entry_points,
            config_files,
            total_files,
            total_lines,
            analyzed_at: Utc::now(),
        };

        // Cache the result.
        let cache = CodeIntelCache {
            last_snapshot: Some(snapshot.clone()),
        };
        self.save_cache(&cache)?;

        let out = serde_json::to_string_pretty(&snapshot).unwrap_or_default();
        Ok(ToolOutput::text(format!("Architecture snapshot:\n{}", out)))
    }

    fn detect_patterns(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let root = self.resolve_path(args);
        if !root.exists() {
            return Err(ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Path does not exist: {}", root.display()),
            });
        }

        let pattern_filter = args.get("pattern").and_then(|v| v.as_str());
        let mut matches: Vec<PatternMatch> = Vec::new();
        let max_files: usize = 1000;
        let mut file_count: usize = 0;

        let walker = ignore::WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            if file_count >= max_files {
                break;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            if Self::is_binary_ext(&ext) {
                continue;
            }

            // Only scan source code files.
            if Self::ext_to_language(&ext).is_none() {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            file_count += 1;

            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();

                // Always scan for TODO/FIXME/HACK comments.
                if pattern_filter.is_none()
                    || pattern_filter == Some("todo")
                    || pattern_filter == Some("fixme")
                    || pattern_filter == Some("hack")
                {
                    if trimmed.contains("TODO") || trimmed.contains("FIXME") {
                        let pname = if trimmed.contains("TODO") {
                            "TODO"
                        } else {
                            "FIXME"
                        };
                        matches.push(PatternMatch {
                            pattern_name: pname.to_string(),
                            file_path: rel.clone(),
                            line_number: line_num + 1,
                            snippet: trimmed.to_string(),
                            confidence: 1.0,
                        });
                    }
                    if trimmed.contains("HACK") {
                        matches.push(PatternMatch {
                            pattern_name: "HACK".to_string(),
                            file_path: rel.clone(),
                            line_number: line_num + 1,
                            snippet: trimmed.to_string(),
                            confidence: 1.0,
                        });
                    }
                }

                // Pattern-specific detection.
                if let Some(filter) = pattern_filter {
                    match filter {
                        "singleton" => {
                            if (trimmed.contains("static")
                                && (trimmed.contains("instance") || trimmed.contains("INSTANCE")))
                                || trimmed.contains("get_instance")
                                || trimmed.contains("getInstance")
                            {
                                matches.push(PatternMatch {
                                    pattern_name: "Singleton".to_string(),
                                    file_path: rel.clone(),
                                    line_number: line_num + 1,
                                    snippet: trimmed.to_string(),
                                    confidence: 0.8,
                                });
                            }
                        }
                        "factory" => {
                            if trimmed.contains("fn create_")
                                || trimmed.contains("fn new_")
                                || trimmed.contains("def create_")
                                || trimmed.contains("function create")
                                || trimmed.contains("Factory")
                            {
                                matches.push(PatternMatch {
                                    pattern_name: "Factory".to_string(),
                                    file_path: rel.clone(),
                                    line_number: line_num + 1,
                                    snippet: trimmed.to_string(),
                                    confidence: 0.7,
                                });
                            }
                        }
                        "builder" => {
                            if trimmed.contains("fn builder(")
                                || trimmed.contains(".builder()")
                                || trimmed.contains(".build()")
                                || trimmed.contains("Builder")
                            {
                                matches.push(PatternMatch {
                                    pattern_name: "Builder".to_string(),
                                    file_path: rel.clone(),
                                    line_number: line_num + 1,
                                    snippet: trimmed.to_string(),
                                    confidence: 0.7,
                                });
                            }
                        }
                        "observer" => {
                            if trimmed.contains("on_event")
                                || trimmed.contains("addEventListener")
                                || trimmed.contains("subscribe")
                                || trimmed.contains("notify")
                                || trimmed.contains("Observer")
                            {
                                matches.push(PatternMatch {
                                    pattern_name: "Observer".to_string(),
                                    file_path: rel.clone(),
                                    line_number: line_num + 1,
                                    snippet: trimmed.to_string(),
                                    confidence: 0.6,
                                });
                            }
                        }
                        "repository" => {
                            if trimmed.contains("find_by")
                                || trimmed.contains("findBy")
                                || trimmed.contains("get_all")
                                || trimmed.contains("getAll")
                                || trimmed.contains("Repository")
                            {
                                matches.push(PatternMatch {
                                    pattern_name: "Repository".to_string(),
                                    file_path: rel.clone(),
                                    line_number: line_num + 1,
                                    snippet: trimmed.to_string(),
                                    confidence: 0.7,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let out = serde_json::to_string_pretty(&matches).unwrap_or_default();
        Ok(ToolOutput::text(format!(
            "Detected {} pattern matches:\n{}",
            matches.len(),
            out
        )))
    }

    fn translate_snippet(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let code = args.get("code").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "code_intelligence".to_string(),
                reason: "Missing required parameter 'code'".to_string(),
            }
        })?;
        let from_lang = args
            .get("from_language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "code_intelligence".to_string(),
                reason: "Missing required parameter 'from_language'".to_string(),
            })?;
        let to_lang = args
            .get("to_language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "code_intelligence".to_string(),
                reason: "Missing required parameter 'to_language'".to_string(),
            })?;

        let semantics_from = Self::language_semantics_notes(from_lang);
        let semantics_to = Self::language_semantics_notes(to_lang);

        let prompt = format!(
            "Translate the following {from_lang} code to {to_lang}.\n\n\
             ## Source Code ({from_lang})\n```{from_ext}\n{code}\n```\n\n\
             ## {from_lang} Semantics\n{semantics_from}\n\n\
             ## {to_lang} Semantics\n{semantics_to}\n\n\
             ## Instructions\n\
             - Produce idiomatic {to_lang} code.\n\
             - Preserve the original logic and behavior.\n\
             - Use {to_lang} conventions for naming, error handling, and structure.\n\
             - Add brief comments where the translation involves non-obvious choices.",
            from_lang = from_lang,
            to_lang = to_lang,
            from_ext = from_lang.to_lowercase(),
            code = code,
            semantics_from = semantics_from,
            semantics_to = semantics_to,
        );

        Ok(ToolOutput::text(prompt))
    }

    fn language_semantics_notes(lang: &str) -> &'static str {
        match lang.to_lowercase().as_str() {
            "rust" => {
                "Ownership & borrowing, no GC, Result/Option for errors, pattern matching, traits for polymorphism, lifetimes."
            }
            "python" => {
                "Dynamic typing, GC, exceptions for errors, duck typing, indentation-based blocks, list comprehensions."
            }
            "javascript" | "js" => {
                "Dynamic typing, prototype-based OOP, async/await with Promises, closures, event loop concurrency."
            }
            "typescript" | "ts" => {
                "Structural typing over JavaScript, interfaces, generics, union/intersection types, async/await."
            }
            "go" => {
                "Static typing, GC, error values (not exceptions), goroutines/channels for concurrency, interfaces (implicit), no generics (pre-1.18)."
            }
            "java" => {
                "Static typing, GC, checked exceptions, class-based OOP, interfaces, generics with type erasure."
            }
            "c" => "Manual memory management, pointers, no OOP, preprocessor macros, header files.",
            "c++" | "cpp" => {
                "Manual memory + RAII/smart pointers, templates, OOP with multiple inheritance, operator overloading."
            }
            "ruby" => {
                "Dynamic typing, GC, everything is an object, blocks/procs/lambdas, mixins via modules."
            }
            _ => "General-purpose programming language.",
        }
    }

    fn compare_implementations(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let file_a = args.get("file_a").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "code_intelligence".to_string(),
                reason: "Missing required parameter 'file_a'".to_string(),
            }
        })?;
        let file_b = args.get("file_b").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "code_intelligence".to_string(),
                reason: "Missing required parameter 'file_b'".to_string(),
            }
        })?;

        let path_a = {
            let p = PathBuf::from(file_a);
            if p.is_absolute() {
                p
            } else {
                self.workspace.join(p)
            }
        };
        let path_b = {
            let p = PathBuf::from(file_b);
            if p.is_absolute() {
                p
            } else {
                self.workspace.join(p)
            }
        };

        let content_a =
            std::fs::read_to_string(&path_a).map_err(|e| ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Failed to read file_a '{}': {}", path_a.display(), e),
            })?;
        let content_b =
            std::fs::read_to_string(&path_b).map_err(|e| ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Failed to read file_b '{}': {}", path_b.display(), e),
            })?;

        let lang_a = path_a
            .extension()
            .and_then(|e| Self::ext_to_language(&e.to_string_lossy()))
            .unwrap_or("Unknown");
        let lang_b = path_b
            .extension()
            .and_then(|e| Self::ext_to_language(&e.to_string_lossy()))
            .unwrap_or("Unknown");

        let lines_a = Self::count_lines(&content_a);
        let lines_b = Self::count_lines(&content_b);

        // Count functions/methods heuristically.
        let fn_count_a = Self::count_functions(&content_a, lang_a);
        let fn_count_b = Self::count_functions(&content_b, lang_b);

        let output = format!(
            "## Implementation Comparison\n\n\
             | Metric | File A | File B |\n\
             |--------|--------|--------|\n\
             | Path | {file_a} | {file_b} |\n\
             | Language | {lang_a} | {lang_b} |\n\
             | Lines | {lines_a} | {lines_b} |\n\
             | Functions | {fn_count_a} | {fn_count_b} |\n\n\
             ### File A: {file_a}\n```{ext_a}\n{preview_a}\n```\n\n\
             ### File B: {file_b}\n```{ext_b}\n{preview_b}\n```",
            file_a = file_a,
            file_b = file_b,
            lang_a = lang_a,
            lang_b = lang_b,
            lines_a = lines_a,
            lines_b = lines_b,
            fn_count_a = fn_count_a,
            fn_count_b = fn_count_b,
            ext_a = lang_a.to_lowercase(),
            ext_b = lang_b.to_lowercase(),
            preview_a = Self::preview_content(&content_a, 50),
            preview_b = Self::preview_content(&content_b, 50),
        );

        Ok(ToolOutput::text(output))
    }

    /// Count function-like definitions heuristically.
    fn count_functions(content: &str, language: &str) -> usize {
        let mut count = 0;
        for line in content.lines() {
            let trimmed = line.trim();
            match language {
                "Rust" => {
                    if (trimmed.starts_with("fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("pub(crate) fn ")
                        || trimmed.starts_with("async fn ")
                        || trimmed.starts_with("pub async fn "))
                        && trimmed.contains('(')
                    {
                        count += 1;
                    }
                }
                "Python" => {
                    if trimmed.starts_with("def ") && trimmed.contains('(') {
                        count += 1;
                    }
                }
                "JavaScript" | "JavaScript (JSX)" | "TypeScript" | "TypeScript (TSX)" => {
                    if (trimmed.starts_with("function ")
                        || trimmed.starts_with("async function ")
                        || trimmed.starts_with("export function ")
                        || trimmed.starts_with("export async function "))
                        && trimmed.contains('(')
                    {
                        count += 1;
                    }
                }
                "Go" => {
                    if trimmed.starts_with("func ") && trimmed.contains('(') {
                        count += 1;
                    }
                }
                "Java" | "C#" => {
                    if (trimmed.contains("public ")
                        || trimmed.contains("private ")
                        || trimmed.contains("protected "))
                        && trimmed.contains('(')
                        && !trimmed.contains("class ")
                        && !trimmed.contains("interface ")
                    {
                        count += 1;
                    }
                }
                "Ruby" => {
                    if trimmed.starts_with("def ") {
                        count += 1;
                    }
                }
                "C" | "C++" => {
                    if trimmed.contains('(')
                        && trimmed.contains(')')
                        && (trimmed.ends_with('{') || trimmed.ends_with(") {"))
                        && !trimmed.starts_with("if ")
                        && !trimmed.starts_with("for ")
                        && !trimmed.starts_with("while ")
                        && !trimmed.starts_with("switch ")
                        && !trimmed.starts_with("//")
                        && !trimmed.starts_with('#')
                    {
                        count += 1;
                    }
                }
                _ => {}
            }
        }
        count
    }

    /// Truncate content to the first N lines for preview.
    fn preview_content(content: &str, max_lines: usize) -> String {
        let lines: Vec<&str> = content.lines().take(max_lines).collect();
        let preview = lines.join("\n");
        let total = content.lines().count();
        if total > max_lines {
            format!("{}\n\n... ({} more lines)", preview, total - max_lines)
        } else {
            preview
        }
    }

    fn tech_debt_report(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let root = self.resolve_path(args);
        if !root.exists() {
            return Err(ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Path does not exist: {}", root.display()),
            });
        }

        let severity_filter = args.get("severity").and_then(|v| v.as_str());

        let mut items: Vec<TechDebtItem> = Vec::new();
        let max_files: usize = 1000;
        let mut file_count: usize = 0;

        let walker = ignore::WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            if file_count >= max_files {
                break;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            if Self::is_binary_ext(&ext) || Self::ext_to_language(&ext).is_none() {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            file_count += 1;

            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            // Track function length for long-function detection.
            let mut fn_start_line: Option<usize> = 0_usize.into();
            let mut fn_name = String::new();
            let mut brace_depth: i32 = 0;
            let mut in_function = false;

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();

                // TODO / FIXME / HACK detection.
                if trimmed.contains("TODO") {
                    let item = TechDebtItem {
                        file_path: rel.clone(),
                        line_number: line_num + 1,
                        category: "todo".to_string(),
                        description: trimmed.to_string(),
                        severity: "medium".to_string(),
                    };
                    if severity_filter.is_none() || severity_filter == Some("medium") {
                        items.push(item);
                    }
                }
                if trimmed.contains("FIXME") {
                    let item = TechDebtItem {
                        file_path: rel.clone(),
                        line_number: line_num + 1,
                        category: "fixme".to_string(),
                        description: trimmed.to_string(),
                        severity: "medium".to_string(),
                    };
                    if severity_filter.is_none() || severity_filter == Some("medium") {
                        items.push(item);
                    }
                }
                if trimmed.contains("HACK") {
                    let item = TechDebtItem {
                        file_path: rel.clone(),
                        line_number: line_num + 1,
                        category: "hack".to_string(),
                        description: trimmed.to_string(),
                        severity: "medium".to_string(),
                    };
                    if severity_filter.is_none() || severity_filter == Some("medium") {
                        items.push(item);
                    }
                }

                // Deep nesting detection (>4 levels of indentation).
                let indent_level = Self::measure_indent(line);
                if indent_level > 4 {
                    let item = TechDebtItem {
                        file_path: rel.clone(),
                        line_number: line_num + 1,
                        category: "deep_nesting".to_string(),
                        description: format!(
                            "Deeply nested code ({} levels): {}",
                            indent_level,
                            Self::truncate_str(trimmed, 80)
                        ),
                        severity: "medium".to_string(),
                    };
                    if severity_filter.is_none() || severity_filter == Some("medium") {
                        items.push(item);
                    }
                }

                // Long function detection (>100 lines) using brace tracking.
                let is_fn_start = trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("pub(crate) fn ")
                    || trimmed.starts_with("async fn ")
                    || trimmed.starts_with("pub async fn ")
                    || trimmed.starts_with("def ")
                    || trimmed.starts_with("function ")
                    || trimmed.starts_with("async function ")
                    || trimmed.starts_with("export function ")
                    || trimmed.starts_with("export async function ")
                    || trimmed.starts_with("func ");

                if is_fn_start && trimmed.contains('(') {
                    // If already tracking a function, check its length before starting a new one.
                    if in_function && let Some(start) = fn_start_line {
                        let length = line_num - start;
                        if length > 100 {
                            let item = TechDebtItem {
                                file_path: rel.clone(),
                                line_number: start + 1,
                                category: "long_function".to_string(),
                                description: format!(
                                    "Function '{}' is {} lines long (>100)",
                                    fn_name, length
                                ),
                                severity: "high".to_string(),
                            };
                            if severity_filter.is_none() || severity_filter == Some("high") {
                                items.push(item);
                            }
                        }
                    }

                    fn_start_line = Some(line_num);
                    fn_name = Self::extract_fn_name(trimmed);
                    brace_depth = 0;
                    in_function = true;
                }

                if in_function {
                    for ch in trimmed.chars() {
                        if ch == '{' {
                            brace_depth += 1;
                        } else if ch == '}' {
                            brace_depth -= 1;
                        }
                    }

                    if brace_depth <= 0
                        && fn_start_line.is_some()
                        && line_num > fn_start_line.unwrap_or(0)
                    {
                        if let Some(start) = fn_start_line {
                            let length = line_num - start + 1;
                            if length > 100 {
                                let item = TechDebtItem {
                                    file_path: rel.clone(),
                                    line_number: start + 1,
                                    category: "long_function".to_string(),
                                    description: format!(
                                        "Function '{}' is {} lines long (>100)",
                                        fn_name, length
                                    ),
                                    severity: "high".to_string(),
                                };
                                if severity_filter.is_none() || severity_filter == Some("high") {
                                    items.push(item);
                                }
                            }
                        }
                        in_function = false;
                        fn_start_line = None;
                    }
                }
            }
        }

        // Summarize by category.
        let mut by_category: HashMap<String, usize> = HashMap::new();
        for item in &items {
            *by_category.entry(item.category.clone()).or_insert(0) += 1;
        }

        let summary = by_category
            .iter()
            .map(|(k, v)| format!("  {}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        let detail = serde_json::to_string_pretty(&items).unwrap_or_default();
        Ok(ToolOutput::text(format!(
            "Tech debt report: {} items found\n\nSummary:\n{}\n\nDetails:\n{}",
            items.len(),
            summary,
            detail
        )))
    }

    /// Measure indentation level (number of indentation units).
    fn measure_indent(line: &str) -> usize {
        let leading_spaces = line.len() - line.trim_start().len();
        // Treat 4 spaces or 1 tab as one indentation level.
        let tab_count = line.chars().take_while(|c| *c == '\t').count();
        if tab_count > 0 {
            tab_count
        } else {
            leading_spaces / 4
        }
    }

    /// Extract function name from a line that starts a function definition.
    fn extract_fn_name(line: &str) -> String {
        // Try to extract the name between "fn "/"def "/"function " and "(".
        let prefixes = [
            "pub async fn ",
            "pub(crate) fn ",
            "pub fn ",
            "async fn ",
            "fn ",
            "export async function ",
            "export function ",
            "async function ",
            "function ",
            "func ",
            "def ",
        ];
        for prefix in &prefixes {
            if let Some(rest) = line.trim().strip_prefix(prefix)
                && let Some(paren_pos) = rest.find('(')
            {
                let name = rest[..paren_pos].trim();
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
        "<anonymous>".to_string()
    }

    fn truncate_str(s: &str, max: usize) -> String {
        if s.len() <= max {
            s.to_string()
        } else {
            format!("{}...", &s[..max])
        }
    }

    fn api_surface(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let root = self.resolve_path(args);
        if !root.exists() {
            return Err(ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Path does not exist: {}", root.display()),
            });
        }

        let lang_filter = args.get("language").and_then(|v| v.as_str());
        let mut entries: Vec<ApiEntry> = Vec::new();

        let walker = ignore::WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            if Self::is_binary_ext(&ext) {
                continue;
            }

            let language = match Self::ext_to_language(&ext) {
                Some(l) => l,
                None => continue,
            };

            // If a language filter is specified, skip non-matching files.
            if let Some(filter) = lang_filter {
                let filter_lower = filter.to_lowercase();
                if !language.to_lowercase().contains(&filter_lower) {
                    continue;
                }
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();

                match language {
                    "Rust" => {
                        // pub fn
                        if (trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn "))
                            && trimmed.contains('(')
                        {
                            let name = Self::extract_fn_name(trimmed);
                            entries.push(ApiEntry {
                                name,
                                kind: "function".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                        // pub struct
                        if trimmed.starts_with("pub struct ") {
                            let name = trimmed
                                .strip_prefix("pub struct ")
                                .and_then(|r| {
                                    r.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                                })
                                .unwrap_or("")
                                .to_string();
                            entries.push(ApiEntry {
                                name,
                                kind: "struct".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                        // pub trait
                        if trimmed.starts_with("pub trait ") {
                            let name = trimmed
                                .strip_prefix("pub trait ")
                                .and_then(|r| {
                                    r.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                                })
                                .unwrap_or("")
                                .to_string();
                            entries.push(ApiEntry {
                                name,
                                kind: "trait".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                        // pub enum
                        if trimmed.starts_with("pub enum ") {
                            let name = trimmed
                                .strip_prefix("pub enum ")
                                .and_then(|r| {
                                    r.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                                })
                                .unwrap_or("")
                                .to_string();
                            entries.push(ApiEntry {
                                name,
                                kind: "enum".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                    }
                    "Python" => {
                        // Module-level def (no leading whitespace).
                        if line.starts_with("def ") && trimmed.contains('(') {
                            let name = Self::extract_fn_name(trimmed);
                            entries.push(ApiEntry {
                                name,
                                kind: "function".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                        // Module-level class.
                        if line.starts_with("class ") {
                            let name = trimmed
                                .strip_prefix("class ")
                                .and_then(|r| {
                                    r.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                                })
                                .unwrap_or("")
                                .to_string();
                            entries.push(ApiEntry {
                                name,
                                kind: "class".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                    }
                    "JavaScript" | "JavaScript (JSX)" | "TypeScript" | "TypeScript (TSX)" => {
                        // export function
                        if (trimmed.starts_with("export function ")
                            || trimmed.starts_with("export async function "))
                            && trimmed.contains('(')
                        {
                            let name = Self::extract_fn_name(trimmed);
                            entries.push(ApiEntry {
                                name,
                                kind: "function".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                        // export class
                        if trimmed.starts_with("export class ") {
                            let name = trimmed
                                .strip_prefix("export class ")
                                .and_then(|r| {
                                    r.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                                })
                                .unwrap_or("")
                                .to_string();
                            entries.push(ApiEntry {
                                name,
                                kind: "class".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                        // export const
                        if trimmed.starts_with("export const ") {
                            let name = trimmed
                                .strip_prefix("export const ")
                                .and_then(|r| {
                                    r.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                                })
                                .unwrap_or("")
                                .to_string();
                            entries.push(ApiEntry {
                                name,
                                kind: "module".to_string(),
                                file_path: rel.clone(),
                                line_number: line_num + 1,
                                signature: trimmed.to_string(),
                                visibility: "public".to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        let out = serde_json::to_string_pretty(&entries).unwrap_or_default();
        Ok(ToolOutput::text(format!(
            "API surface: {} public entries\n{}",
            entries.len(),
            out
        )))
    }

    fn dependency_map(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let root = self.resolve_path(args);
        if !root.exists() {
            return Err(ToolError::ExecutionFailed {
                name: "code_intelligence".to_string(),
                message: format!("Path does not exist: {}", root.display()),
            });
        }

        let mut deps: Vec<DependencyEntry> = Vec::new();

        // Parse Cargo.toml files.
        Self::find_and_parse_files(&root, "Cargo.toml", |path, content| {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            deps.extend(Self::parse_cargo_toml(&content, &rel));
        });

        // Parse package.json files.
        Self::find_and_parse_files(&root, "package.json", |path, content| {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            deps.extend(Self::parse_package_json(&content, &rel));
        });

        // Parse requirements.txt files.
        Self::find_and_parse_files(&root, "requirements.txt", |path, content| {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            deps.extend(Self::parse_requirements_txt(&content, &rel));
        });

        // Parse go.mod files.
        Self::find_and_parse_files(&root, "go.mod", |path, content| {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            deps.extend(Self::parse_go_mod(&content, &rel));
        });

        // Parse Gemfile files.
        Self::find_and_parse_files(&root, "Gemfile", |path, content| {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            deps.extend(Self::parse_gemfile(&content, &rel));
        });

        let out = serde_json::to_string_pretty(&deps).unwrap_or_default();
        Ok(ToolOutput::text(format!(
            "Dependency map: {} dependencies\n{}",
            deps.len(),
            out
        )))
    }

    // -----------------------------------------------------------------------
    // Dependency parsers
    // -----------------------------------------------------------------------

    /// Walk directory to find files with a specific name and call the handler.
    fn find_and_parse_files<F>(root: &PathBuf, filename: &str, mut handler: F)
    where
        F: FnMut(&std::path::Path, String),
    {
        let walker = ignore::WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            if entry.file_name().to_string_lossy() == filename
                && let Ok(content) = std::fs::read_to_string(entry.path())
            {
                handler(entry.path(), content);
            }
        }
    }

    /// Parse dependencies from Cargo.toml content.
    fn parse_cargo_toml(content: &str, source_file: &str) -> Vec<DependencyEntry> {
        let mut deps = Vec::new();
        let mut current_section = String::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Detect section headers.
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                current_section = trimmed[1..trimmed.len() - 1].to_string();
                continue;
            }

            // Also handle dotted section headers like [workspace.dependencies].
            if trimmed.starts_with('[') {
                if let Some(end) = trimmed.find(']') {
                    current_section = trimmed[1..end].to_string();
                }
                continue;
            }

            let dep_type = match current_section.as_str() {
                "dependencies" | "workspace.dependencies" => "runtime",
                "dev-dependencies" => "dev",
                "build-dependencies" => "build",
                s if s.ends_with(".dependencies") && !s.contains("dev") && !s.contains("build") => {
                    "runtime"
                }
                _ => continue,
            };

            // Parse lines like: name = "version" or name = { version = "..." ... }
            if let Some(eq_pos) = trimmed.find('=') {
                let name = trimmed[..eq_pos].trim().to_string();
                if name.is_empty() || name.starts_with('#') {
                    continue;
                }
                let value_part = trimmed[eq_pos + 1..].trim();

                let version = if value_part.starts_with('"') {
                    // Simple version string.
                    value_part.trim_matches('"').trim_matches('\'').to_string()
                } else if value_part.starts_with('{') {
                    // Inline table — extract version field.
                    Self::extract_toml_inline_version(value_part)
                } else {
                    value_part.to_string()
                };

                deps.push(DependencyEntry {
                    name,
                    version,
                    dep_type: dep_type.to_string(),
                    source_file: source_file.to_string(),
                });
            }
        }

        deps
    }

    /// Extract the `version` field from a TOML inline table like `{ version = "1.0", ... }`.
    fn extract_toml_inline_version(inline: &str) -> String {
        // Look for version = "..." within the inline table.
        if let Some(ver_pos) = inline.find("version") {
            let after_key = &inline[ver_pos + 7..];
            if let Some(eq_pos) = after_key.find('=') {
                let after_eq = after_key[eq_pos + 1..].trim();
                if let Some(stripped) = after_eq.strip_prefix('"')
                    && let Some(end_quote) = stripped.find('"')
                {
                    return stripped[..end_quote].to_string();
                }
            }
        }
        // Fallback: look for `workspace = true`.
        if inline.contains("workspace") {
            return "workspace".to_string();
        }
        "*".to_string()
    }

    /// Parse dependencies from package.json content.
    fn parse_package_json(content: &str, source_file: &str) -> Vec<DependencyEntry> {
        let mut deps = Vec::new();

        let parsed: Value = match serde_json::from_str(content) {
            Ok(v) => v,
            Err(_) => return deps,
        };

        let sections = [
            ("dependencies", "runtime"),
            ("devDependencies", "dev"),
            ("peerDependencies", "runtime"),
            ("optionalDependencies", "optional"),
        ];

        for (key, dep_type) in &sections {
            if let Some(obj) = parsed.get(key).and_then(|v| v.as_object()) {
                for (name, version) in obj {
                    deps.push(DependencyEntry {
                        name: name.clone(),
                        version: version.as_str().unwrap_or("*").to_string(),
                        dep_type: dep_type.to_string(),
                        source_file: source_file.to_string(),
                    });
                }
            }
        }

        deps
    }

    /// Parse dependencies from requirements.txt content.
    fn parse_requirements_txt(content: &str, source_file: &str) -> Vec<DependencyEntry> {
        let mut deps = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
                continue;
            }

            // Lines like: package==1.0.0, package>=1.0.0, package~=1.0, or just package
            let (name, version) = if let Some(pos) = trimmed.find("==") {
                (trimmed[..pos].trim(), trimmed[pos + 2..].trim())
            } else if let Some(pos) = trimmed.find(">=") {
                (trimmed[..pos].trim(), trimmed[pos..].trim())
            } else if let Some(pos) = trimmed.find("~=") {
                (trimmed[..pos].trim(), trimmed[pos..].trim())
            } else if let Some(pos) = trimmed.find("<=") {
                (trimmed[..pos].trim(), trimmed[pos..].trim())
            } else if let Some(pos) = trimmed.find("!=") {
                (trimmed[..pos].trim(), trimmed[pos..].trim())
            } else {
                (trimmed, "*")
            };

            if !name.is_empty() {
                deps.push(DependencyEntry {
                    name: name.to_string(),
                    version: version.to_string(),
                    dep_type: "runtime".to_string(),
                    source_file: source_file.to_string(),
                });
            }
        }

        deps
    }

    /// Parse dependencies from go.mod content.
    fn parse_go_mod(content: &str, source_file: &str) -> Vec<DependencyEntry> {
        let mut deps = Vec::new();
        let mut in_require = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed == "require (" {
                in_require = true;
                continue;
            }
            if trimmed == ")" {
                in_require = false;
                continue;
            }

            // Single-line require.
            if trimmed.starts_with("require ") && !trimmed.contains('(') {
                let rest = trimmed.strip_prefix("require ").unwrap_or("").trim();
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 2 {
                    deps.push(DependencyEntry {
                        name: parts[0].to_string(),
                        version: parts[1].to_string(),
                        dep_type: "runtime".to_string(),
                        source_file: source_file.to_string(),
                    });
                }
                continue;
            }

            // Inside require block.
            if in_require && !trimmed.is_empty() && !trimmed.starts_with("//") {
                let clean = if let Some(pos) = trimmed.find("//") {
                    trimmed[..pos].trim()
                } else {
                    trimmed
                };
                let parts: Vec<&str> = clean.split_whitespace().collect();
                if parts.len() >= 2 {
                    let dep_type = if parts.len() > 2 && parts[2] == "// indirect" {
                        "optional"
                    } else {
                        "runtime"
                    };
                    deps.push(DependencyEntry {
                        name: parts[0].to_string(),
                        version: parts[1].to_string(),
                        dep_type: dep_type.to_string(),
                        source_file: source_file.to_string(),
                    });
                }
            }
        }

        deps
    }

    /// Parse dependencies from Gemfile content.
    fn parse_gemfile(content: &str, source_file: &str) -> Vec<DependencyEntry> {
        let mut deps = Vec::new();
        let mut in_group: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("group ") {
                if trimmed.contains(":development") || trimmed.contains(":test") {
                    in_group = Some("dev".to_string());
                } else {
                    in_group = Some("runtime".to_string());
                }
                continue;
            }
            if trimmed == "end" {
                in_group = None;
                continue;
            }

            if trimmed.starts_with("gem ") {
                let rest = trimmed.strip_prefix("gem ").unwrap_or("").trim();
                // Parse: gem 'name', '~> version' or gem "name", "version"
                let parts: Vec<&str> = rest.split(',').collect();
                if let Some(name_part) = parts.first() {
                    let name = name_part
                        .trim()
                        .trim_matches('\'')
                        .trim_matches('"')
                        .to_string();
                    let version = if parts.len() > 1 {
                        parts[1]
                            .trim()
                            .trim_matches('\'')
                            .trim_matches('"')
                            .to_string()
                    } else {
                        "*".to_string()
                    };
                    let dep_type = in_group.as_deref().unwrap_or("runtime").to_string();
                    deps.push(DependencyEntry {
                        name,
                        version,
                        dep_type,
                        source_file: source_file.to_string(),
                    });
                }
            }
        }

        deps
    }
}

// ---------------------------------------------------------------------------
// Tool trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Tool for CodeIntelligenceTool {
    fn name(&self) -> &str {
        "code_intelligence"
    }

    fn description(&self) -> &str {
        "Cross-language codebase analysis: architecture detection, pattern recognition, \
         tech debt scanning, API surface extraction. Actions: analyze_architecture, \
         detect_patterns, translate_snippet, compare_implementations, tech_debt_report, \
         api_surface, dependency_map."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "analyze_architecture",
                        "detect_patterns",
                        "translate_snippet",
                        "compare_implementations",
                        "tech_debt_report",
                        "api_surface",
                        "dependency_map"
                    ],
                    "description": "Action to perform"
                },
                "path": {
                    "type": "string",
                    "description": "Target path (defaults to workspace root)"
                },
                "force": {
                    "type": "boolean",
                    "description": "Force re-analysis ignoring cache (for analyze_architecture)"
                },
                "pattern": {
                    "type": "string",
                    "enum": ["singleton", "factory", "observer", "builder", "repository"],
                    "description": "Design pattern to detect (for detect_patterns)"
                },
                "code": {
                    "type": "string",
                    "description": "Source code snippet (for translate_snippet)"
                },
                "from_language": {
                    "type": "string",
                    "description": "Source language (for translate_snippet)"
                },
                "to_language": {
                    "type": "string",
                    "description": "Target language (for translate_snippet)"
                },
                "file_a": {
                    "type": "string",
                    "description": "First file path (for compare_implementations)"
                },
                "file_b": {
                    "type": "string",
                    "description": "Second file path (for compare_implementations)"
                },
                "severity": {
                    "type": "string",
                    "enum": ["low", "medium", "high"],
                    "description": "Filter by severity (for tech_debt_report)"
                },
                "language": {
                    "type": "string",
                    "description": "Filter by language (for api_surface)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(120)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "code_intelligence".to_string(),
                reason: "Missing required parameter 'action'".to_string(),
            }
        })?;

        match action {
            "analyze_architecture" => self.analyze_architecture(&args),
            "detect_patterns" => self.detect_patterns(&args),
            "translate_snippet" => self.translate_snippet(&args),
            "compare_implementations" => self.compare_implementations(&args),
            "tech_debt_report" => self.tech_debt_report(&args),
            "api_surface" => self.api_surface(&args),
            "dependency_map" => self.dependency_map(&args),
            other => Err(ToolError::InvalidArguments {
                name: "code_intelligence".to_string(),
                reason: format!(
                    "Unknown action '{}'. Valid actions: analyze_architecture, detect_patterns, translate_snippet, compare_implementations, tech_debt_report, api_surface, dependency_map",
                    other
                ),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool(dir: &std::path::Path) -> CodeIntelligenceTool {
        let workspace = dir.canonicalize().unwrap();
        CodeIntelligenceTool::new(workspace)
    }

    #[test]
    fn test_tool_properties() {
        let dir = TempDir::new().unwrap();
        let tool = make_tool(dir.path());
        assert_eq!(tool.name(), "code_intelligence");
        assert!(!tool.description().is_empty());
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(tool.timeout(), Duration::from_secs(120));
    }

    #[test]
    fn test_schema_validation() {
        let dir = TempDir::new().unwrap();
        let tool = make_tool(dir.path());
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        let props = schema.get("properties").unwrap();
        assert!(props.get("action").is_some());
        assert!(props.get("path").is_some());
        assert!(props.get("code").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[tokio::test]
    async fn test_analyze_architecture_basic() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        // Create some source files.
        let src_dir = workspace.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("main.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            src_dir.join("lib.rs"),
            "pub fn greet() -> String {\n    \"hello\".to_string()\n}\n",
        )
        .unwrap();
        std::fs::write(workspace.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();

        let tool = CodeIntelligenceTool::new(workspace);
        let result = tool
            .execute(json!({"action": "analyze_architecture"}))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("Architecture snapshot"));
        assert!(text.contains("Rust"));
        assert!(text.contains("main.rs"));
        assert!(text.contains("Cargo.toml"));
    }

    #[tokio::test]
    async fn test_analyze_caching() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        let src_dir = workspace.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();

        let tool = CodeIntelligenceTool::new(workspace);

        // First call: fresh analysis.
        let result1 = tool
            .execute(json!({"action": "analyze_architecture"}))
            .await
            .unwrap();
        assert!(result1.content.contains("Architecture snapshot:"));
        assert!(!result1.content.contains("(cached)"));

        // Second call: should return cached.
        let result2 = tool
            .execute(json!({"action": "analyze_architecture"}))
            .await
            .unwrap();
        assert!(result2.content.contains("(cached)"));

        // Force re-scan.
        let result3 = tool
            .execute(json!({"action": "analyze_architecture", "force": true}))
            .await
            .unwrap();
        assert!(!result3.content.contains("(cached)"));
    }

    #[tokio::test]
    async fn test_detect_patterns_todo() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        std::fs::write(
            workspace.join("example.rs"),
            "fn main() {\n    // TODO: fix this later\n    // FIXME: broken\n    // HACK: workaround\n    println!(\"ok\");\n}\n",
        )
        .unwrap();

        let tool = CodeIntelligenceTool::new(workspace);
        let result = tool
            .execute(json!({"action": "detect_patterns"}))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("TODO"));
        assert!(text.contains("FIXME"));
        assert!(text.contains("HACK"));
        assert!(text.contains("fix this later"));
    }

    #[tokio::test]
    async fn test_translate_returns_prompt() {
        let dir = TempDir::new().unwrap();
        let tool = make_tool(dir.path());

        let result = tool
            .execute(json!({
                "action": "translate_snippet",
                "code": "fn add(a: i32, b: i32) -> i32 { a + b }",
                "from_language": "Rust",
                "to_language": "Python"
            }))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("Translate the following Rust code to Python"));
        assert!(text.contains("fn add"));
        assert!(text.contains("Rust Semantics"));
        assert!(text.contains("Python Semantics"));
        assert!(text.contains("Ownership"));
        assert!(text.contains("Dynamic typing"));
    }

    #[tokio::test]
    async fn test_compare_implementations() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        std::fs::write(
            workspace.join("sort_a.rs"),
            "pub fn bubble_sort(arr: &mut Vec<i32>) {\n    let n = arr.len();\n    for i in 0..n {\n        for j in 0..n-1-i {\n            if arr[j] > arr[j+1] {\n                arr.swap(j, j+1);\n            }\n        }\n    }\n}\n",
        )
        .unwrap();
        std::fs::write(
            workspace.join("sort_b.py"),
            "def quick_sort(arr):\n    if len(arr) <= 1:\n        return arr\n    pivot = arr[0]\n    left = [x for x in arr[1:] if x <= pivot]\n    right = [x for x in arr[1:] if x > pivot]\n    return quick_sort(left) + [pivot] + quick_sort(right)\n",
        )
        .unwrap();

        let tool = CodeIntelligenceTool::new(workspace);
        let result = tool
            .execute(json!({
                "action": "compare_implementations",
                "file_a": "sort_a.rs",
                "file_b": "sort_b.py"
            }))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("Implementation Comparison"));
        assert!(text.contains("sort_a.rs"));
        assert!(text.contains("sort_b.py"));
        assert!(text.contains("Rust"));
        assert!(text.contains("Python"));
        assert!(text.contains("Lines"));
        assert!(text.contains("Functions"));
    }

    #[tokio::test]
    async fn test_tech_debt_todo_fixme() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        std::fs::write(
            workspace.join("messy.rs"),
            "fn main() {\n    // TODO: refactor this\n    // FIXME: memory leak\n    println!(\"ok\");\n}\n",
        )
        .unwrap();

        let tool = CodeIntelligenceTool::new(workspace);
        let result = tool
            .execute(json!({"action": "tech_debt_report"}))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("Tech debt report"));
        assert!(text.contains("todo"));
        assert!(text.contains("fixme"));
        assert!(text.contains("refactor this"));
        assert!(text.contains("memory leak"));
    }

    #[tokio::test]
    async fn test_api_surface_rust() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        std::fs::write(
            workspace.join("api.rs"),
            "pub fn create_user(name: &str) -> User {\n    User { name: name.to_string() }\n}\n\n\
             pub struct User {\n    pub name: String,\n}\n\n\
             pub trait Greet {\n    fn greet(&self) -> String;\n}\n\n\
             pub enum Color {\n    Red,\n    Blue,\n}\n\n\
             fn private_helper() {}\n",
        )
        .unwrap();

        let tool = CodeIntelligenceTool::new(workspace);
        let result = tool
            .execute(json!({"action": "api_surface", "language": "rust"}))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("create_user"));
        assert!(text.contains("User"));
        assert!(text.contains("Greet"));
        assert!(text.contains("Color"));
        // private_helper should NOT appear since it is not pub.
        assert!(!text.contains("private_helper"));
    }

    #[tokio::test]
    async fn test_dependency_map_cargo() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        std::fs::write(
            workspace.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nserde = \"1.0\"\ntokio = { version = \"1.47\", features = [\"full\"] }\n\n\
             [dev-dependencies]\ntempfile = \"3.14\"\n\n\
             [build-dependencies]\ncc = \"1.0\"\n",
        )
        .unwrap();

        let tool = CodeIntelligenceTool::new(workspace);
        let result = tool
            .execute(json!({"action": "dependency_map"}))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("serde"));
        assert!(text.contains("tokio"));
        assert!(text.contains("tempfile"));
        assert!(text.contains("cc"));
        assert!(text.contains("runtime"));
        assert!(text.contains("dev"));
        assert!(text.contains("build"));
    }

    #[tokio::test]
    async fn test_dependency_map_npm() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        std::fs::write(
            workspace.join("package.json"),
            r#"{
                "name": "demo",
                "dependencies": {
                    "express": "^4.18.0",
                    "lodash": "^4.17.21"
                },
                "devDependencies": {
                    "jest": "^29.0.0"
                }
            }"#,
        )
        .unwrap();

        let tool = CodeIntelligenceTool::new(workspace);
        let result = tool
            .execute(json!({"action": "dependency_map"}))
            .await
            .unwrap();

        let text = &result.content;
        assert!(text.contains("express"));
        assert!(text.contains("lodash"));
        assert!(text.contains("jest"));
        assert!(text.contains("runtime"));
        assert!(text.contains("dev"));
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = CodeIntelligenceTool::new(workspace);

        // Save a cache with a snapshot.
        let snapshot = ArchitectureSnapshot {
            project_root: "/test".to_string(),
            languages: vec![LanguageStats {
                language: "Rust".to_string(),
                files: 10,
                lines: 500,
                extensions: vec!["rs".to_string()],
            }],
            directories: vec![],
            entry_points: vec!["src/main.rs".to_string()],
            config_files: vec!["Cargo.toml".to_string()],
            total_files: 10,
            total_lines: 500,
            analyzed_at: Utc::now(),
        };
        let cache = CodeIntelCache {
            last_snapshot: Some(snapshot),
        };
        tool.save_cache(&cache).unwrap();

        // Load it back.
        let loaded = tool.load_cache();
        assert!(loaded.last_snapshot.is_some());
        let loaded_snap = loaded.last_snapshot.unwrap();
        assert_eq!(loaded_snap.project_root, "/test");
        assert_eq!(loaded_snap.languages.len(), 1);
        assert_eq!(loaded_snap.languages[0].language, "Rust");
        assert_eq!(loaded_snap.total_files, 10);
        assert_eq!(loaded_snap.total_lines, 500);
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let dir = TempDir::new().unwrap();
        let tool = make_tool(dir.path());

        let result = tool.execute(json!({"action": "nonexistent_action"})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "code_intelligence");
                assert!(reason.contains("Unknown action"));
                assert!(reason.contains("nonexistent_action"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }
}
