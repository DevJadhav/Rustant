//! Project type detection for zero-config initialization.
//!
//! Scans a workspace directory to identify the project's language, framework,
//! and build system. Used by `rustant init` to generate optimal default
//! configurations without requiring manual setup.

use std::path::Path;

/// Detected project type based on workspace analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    Ruby,
    CSharp,
    Cpp,
    /// Multiple languages detected.
    Mixed(Vec<ProjectType>),
    /// No recognized project markers found.
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust"),
            ProjectType::Node => write!(f, "Node.js"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Java => write!(f, "Java"),
            ProjectType::Ruby => write!(f, "Ruby"),
            ProjectType::CSharp => write!(f, "C#"),
            ProjectType::Cpp => write!(f, "C/C++"),
            ProjectType::Mixed(types) => {
                let names: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "Mixed ({})", names.join(", "))
            }
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Result of project detection with rich metadata.
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    /// Primary detected project type.
    pub project_type: ProjectType,
    /// Whether the project has a git repository.
    pub has_git: bool,
    /// Whether the git working tree is clean.
    pub git_clean: bool,
    /// Detected build tool commands (e.g., "cargo", "npm", "make").
    pub build_commands: Vec<String>,
    /// Detected test commands (e.g., "cargo test", "npm test").
    pub test_commands: Vec<String>,
    /// Detected package manager.
    pub package_manager: Option<String>,
    /// Key source directories found.
    pub source_dirs: Vec<String>,
    /// Whether a CI configuration was found.
    pub has_ci: bool,
    /// Detected framework (e.g., "React", "Django", "Actix").
    pub framework: Option<String>,
}

/// Detect the project type and metadata from a workspace directory.
pub fn detect_project(workspace: &Path) -> ProjectInfo {
    let mut types = Vec::new();
    let mut build_commands = Vec::new();
    let mut test_commands = Vec::new();
    let mut package_manager = None;
    let mut source_dirs = Vec::new();
    let mut framework = None;

    // Rust detection
    if workspace.join("Cargo.toml").exists() {
        types.push(ProjectType::Rust);
        build_commands.push("cargo build".to_string());
        test_commands.push("cargo test".to_string());
        package_manager = Some("cargo".to_string());
        if workspace.join("src").exists() {
            source_dirs.push("src".to_string());
        }
        // Detect Rust frameworks
        if let Ok(content) = std::fs::read_to_string(workspace.join("Cargo.toml")) {
            if content.contains("actix-web") {
                framework = Some("Actix Web".to_string());
            } else if content.contains("axum") {
                framework = Some("Axum".to_string());
            } else if content.contains("rocket") {
                framework = Some("Rocket".to_string());
            } else if content.contains("tauri") {
                framework = Some("Tauri".to_string());
            }
        }
    }

    // Node.js detection
    if workspace.join("package.json").exists() {
        types.push(ProjectType::Node);
        if workspace.join("pnpm-lock.yaml").exists() {
            build_commands.push("pnpm build".to_string());
            test_commands.push("pnpm test".to_string());
            package_manager = Some("pnpm".to_string());
        } else if workspace.join("yarn.lock").exists() {
            build_commands.push("yarn build".to_string());
            test_commands.push("yarn test".to_string());
            package_manager = Some("yarn".to_string());
        } else if workspace.join("bun.lockb").exists() {
            build_commands.push("bun build".to_string());
            test_commands.push("bun test".to_string());
            package_manager = Some("bun".to_string());
        } else {
            build_commands.push("npm run build".to_string());
            test_commands.push("npm test".to_string());
            package_manager = Some("npm".to_string());
        }
        if workspace.join("src").exists() {
            source_dirs.push("src".to_string());
        }
        // Detect Node frameworks
        if let Ok(content) = std::fs::read_to_string(workspace.join("package.json")) {
            if content.contains("\"next\"") {
                framework = Some("Next.js".to_string());
            } else if content.contains("\"react\"") {
                framework = Some("React".to_string());
            } else if content.contains("\"vue\"") {
                framework = Some("Vue.js".to_string());
            } else if content.contains("\"svelte\"") {
                framework = Some("Svelte".to_string());
            } else if content.contains("\"express\"") {
                framework = Some("Express".to_string());
            } else if content.contains("\"nestjs\"") || content.contains("\"@nestjs/core\"") {
                framework = Some("NestJS".to_string());
            }
        }
    }

    // Python detection
    if workspace.join("pyproject.toml").exists()
        || workspace.join("setup.py").exists()
        || workspace.join("requirements.txt").exists()
        || workspace.join("Pipfile").exists()
    {
        types.push(ProjectType::Python);
        if workspace.join("pyproject.toml").exists() {
            if workspace.join("poetry.lock").exists() {
                build_commands.push("poetry build".to_string());
                test_commands.push("poetry run pytest".to_string());
                package_manager = Some("poetry".to_string());
            } else if workspace.join("uv.lock").exists() {
                test_commands.push("uv run pytest".to_string());
                package_manager = Some("uv".to_string());
            } else {
                test_commands.push("python -m pytest".to_string());
                package_manager = Some("pip".to_string());
            }
        } else if workspace.join("Pipfile").exists() {
            test_commands.push("pipenv run pytest".to_string());
            package_manager = Some("pipenv".to_string());
        } else {
            test_commands.push("python -m pytest".to_string());
            package_manager = Some("pip".to_string());
        }
        // Detect Python frameworks
        let py_files = ["pyproject.toml", "requirements.txt", "setup.py"];
        for file in &py_files {
            if let Ok(content) = std::fs::read_to_string(workspace.join(file)) {
                if content.contains("django") {
                    framework = Some("Django".to_string());
                    break;
                } else if content.contains("fastapi") {
                    framework = Some("FastAPI".to_string());
                    break;
                } else if content.contains("flask") {
                    framework = Some("Flask".to_string());
                    break;
                }
            }
        }
    }

    // Go detection
    if workspace.join("go.mod").exists() {
        types.push(ProjectType::Go);
        build_commands.push("go build ./...".to_string());
        test_commands.push("go test ./...".to_string());
        package_manager = Some("go".to_string());
    }

    // Java detection
    if workspace.join("pom.xml").exists() {
        types.push(ProjectType::Java);
        build_commands.push("mvn compile".to_string());
        test_commands.push("mvn test".to_string());
        package_manager = Some("maven".to_string());
    } else if workspace.join("build.gradle").exists() || workspace.join("build.gradle.kts").exists()
    {
        types.push(ProjectType::Java);
        build_commands.push("./gradlew build".to_string());
        test_commands.push("./gradlew test".to_string());
        package_manager = Some("gradle".to_string());
    }

    // Ruby detection
    if workspace.join("Gemfile").exists() {
        types.push(ProjectType::Ruby);
        test_commands.push("bundle exec rspec".to_string());
        package_manager = Some("bundler".to_string());
        if workspace.join("config").join("routes.rb").exists() {
            framework = Some("Rails".to_string());
        }
    }

    // C# detection
    let has_csharp = workspace.join("*.csproj").exists()
        || std::fs::read_dir(workspace)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.path().extension().is_some_and(|ext| ext == "csproj"))
            })
            .unwrap_or(false);
    if has_csharp || workspace.join("*.sln").exists() {
        types.push(ProjectType::CSharp);
        build_commands.push("dotnet build".to_string());
        test_commands.push("dotnet test".to_string());
        package_manager = Some("nuget".to_string());
    }

    // C/C++ detection
    if workspace.join("CMakeLists.txt").exists() || workspace.join("Makefile").exists() {
        types.push(ProjectType::Cpp);
        if workspace.join("CMakeLists.txt").exists() {
            build_commands.push("cmake --build build".to_string());
            package_manager = Some("cmake".to_string());
        } else {
            build_commands.push("make".to_string());
        }
    }

    // Git detection
    let has_git = workspace.join(".git").exists();
    let git_clean = if has_git {
        std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(workspace)
            .output()
            .map(|o| o.stdout.is_empty())
            .unwrap_or(false)
    } else {
        false
    };

    // CI detection
    let has_ci = workspace.join(".github").join("workflows").exists()
        || workspace.join(".gitlab-ci.yml").exists()
        || workspace.join(".circleci").exists()
        || workspace.join("Jenkinsfile").exists();

    // Determine primary project type
    let project_type = match types.len() {
        0 => ProjectType::Unknown,
        1 => types.into_iter().next().unwrap(),
        _ => ProjectType::Mixed(types),
    };

    ProjectInfo {
        project_type,
        has_git,
        git_clean,
        build_commands,
        test_commands,
        package_manager,
        source_dirs,
        has_ci,
        framework,
    }
}

/// Generate recommended safety allowed_commands based on project type.
pub fn recommended_allowed_commands(info: &ProjectInfo) -> Vec<String> {
    let mut commands = vec!["git".to_string(), "echo".to_string(), "cat".to_string()];

    match &info.project_type {
        ProjectType::Rust => {
            commands.extend([
                "cargo".to_string(),
                "rustfmt".to_string(),
                "clippy-driver".to_string(),
            ]);
        }
        ProjectType::Node => {
            commands.extend(["node".to_string(), "npx".to_string()]);
            if let Some(pm) = &info.package_manager {
                commands.push(pm.clone());
            }
        }
        ProjectType::Python => {
            commands.extend([
                "python".to_string(),
                "python3".to_string(),
                "pytest".to_string(),
            ]);
            if let Some(pm) = &info.package_manager {
                commands.push(pm.clone());
            }
        }
        ProjectType::Go => {
            commands.extend(["go".to_string(), "gofmt".to_string()]);
        }
        ProjectType::Java => {
            if let Some(pm) = &info.package_manager {
                match pm.as_str() {
                    "maven" => commands.push("mvn".to_string()),
                    "gradle" => commands.push("./gradlew".to_string()),
                    _ => {}
                }
            }
        }
        ProjectType::Ruby => {
            commands.extend(["ruby".to_string(), "bundle".to_string(), "rake".to_string()]);
        }
        ProjectType::CSharp => {
            commands.push("dotnet".to_string());
        }
        ProjectType::Cpp => {
            commands.extend(["cmake".to_string(), "make".to_string()]);
        }
        ProjectType::Mixed(types) => {
            for t in types {
                let sub_info = ProjectInfo {
                    project_type: t.clone(),
                    has_git: false,
                    git_clean: false,
                    build_commands: vec![],
                    test_commands: vec![],
                    package_manager: info.package_manager.clone(),
                    source_dirs: vec![],
                    has_ci: false,
                    framework: None,
                };
                commands.extend(recommended_allowed_commands(&sub_info));
            }
            commands.sort();
            commands.dedup();
        }
        ProjectType::Unknown => {}
    }

    commands
}

/// Generate example tasks tailored to the detected project type.
pub fn example_tasks(info: &ProjectInfo) -> Vec<String> {
    let mut tasks = Vec::new();

    match &info.project_type {
        ProjectType::Rust => {
            tasks.push("\"Fix the compiler warnings in src/main.rs\"".to_string());
            tasks.push("\"Add error handling to the database module\"".to_string());
            tasks.push("\"Write tests for the authentication logic\"".to_string());
        }
        ProjectType::Node => {
            tasks.push("\"Add input validation to the API endpoints\"".to_string());
            tasks.push("\"Fix the failing test in auth.test.ts\"".to_string());
            tasks.push("\"Refactor the user service to use async/await\"".to_string());
        }
        ProjectType::Python => {
            tasks.push("\"Add type hints to the data processing module\"".to_string());
            tasks.push("\"Write unit tests for the API handlers\"".to_string());
            tasks.push("\"Fix the race condition in the worker pool\"".to_string());
        }
        ProjectType::Go => {
            tasks.push("\"Add error wrapping to the HTTP handlers\"".to_string());
            tasks.push("\"Write table-driven tests for the parser\"".to_string());
            tasks.push("\"Implement graceful shutdown for the server\"".to_string());
        }
        ProjectType::Java => {
            tasks.push("\"Add null safety checks to the service layer\"".to_string());
            tasks.push("\"Write integration tests for the REST controllers\"".to_string());
            tasks.push("\"Refactor the DAO layer to use the repository pattern\"".to_string());
        }
        _ => {
            tasks.push("\"Find and fix bugs in the codebase\"".to_string());
            tasks.push("\"Add tests for the main module\"".to_string());
            tasks.push("\"Explain the architecture of this project\"".to_string());
        }
    }

    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_rust_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let info = detect_project(dir.path());
        assert_eq!(info.project_type, ProjectType::Rust);
        assert!(info.build_commands.contains(&"cargo build".to_string()));
        assert!(info.test_commands.contains(&"cargo test".to_string()));
        assert_eq!(info.package_manager, Some("cargo".to_string()));
        assert!(info.source_dirs.contains(&"src".to_string()));
    }

    #[test]
    fn test_detect_node_project_npm() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "version": "1.0.0"}"#,
        )
        .unwrap();

        let info = detect_project(dir.path());
        assert_eq!(info.project_type, ProjectType::Node);
        assert_eq!(info.package_manager, Some("npm".to_string()));
    }

    #[test]
    fn test_detect_node_project_pnpm() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "version": "1.0.0"}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "lockfileVersion: 9\n").unwrap();

        let info = detect_project(dir.path());
        assert_eq!(info.project_type, ProjectType::Node);
        assert_eq!(info.package_manager, Some("pnpm".to_string()));
    }

    #[test]
    fn test_detect_python_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"test\"",
        )
        .unwrap();

        let info = detect_project(dir.path());
        assert_eq!(info.project_type, ProjectType::Python);
    }

    #[test]
    fn test_detect_go_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("go.mod"),
            "module example.com/test\n\ngo 1.21\n",
        )
        .unwrap();

        let info = detect_project(dir.path());
        assert_eq!(info.project_type, ProjectType::Go);
        assert!(info.build_commands.contains(&"go build ./...".to_string()));
    }

    #[test]
    fn test_detect_mixed_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

        let info = detect_project(dir.path());
        match &info.project_type {
            ProjectType::Mixed(types) => {
                assert!(types.contains(&ProjectType::Rust));
                assert!(types.contains(&ProjectType::Node));
            }
            _ => panic!("Expected Mixed project type"),
        }
    }

    #[test]
    fn test_detect_unknown_project() {
        let dir = TempDir::new().unwrap();
        let info = detect_project(dir.path());
        assert_eq!(info.project_type, ProjectType::Unknown);
    }

    #[test]
    fn test_detect_git_status() {
        let dir = TempDir::new().unwrap();
        // No .git directory
        let info = detect_project(dir.path());
        assert!(!info.has_git);
    }

    #[test]
    fn test_detect_ci() {
        let dir = TempDir::new().unwrap();
        let gh_dir = dir.path().join(".github").join("workflows");
        std::fs::create_dir_all(&gh_dir).unwrap();
        std::fs::write(gh_dir.join("ci.yml"), "name: CI").unwrap();

        let info = detect_project(dir.path());
        assert!(info.has_ci);
    }

    #[test]
    fn test_recommended_commands_rust() {
        let info = ProjectInfo {
            project_type: ProjectType::Rust,
            has_git: true,
            git_clean: true,
            build_commands: vec!["cargo build".to_string()],
            test_commands: vec!["cargo test".to_string()],
            package_manager: Some("cargo".to_string()),
            source_dirs: vec!["src".to_string()],
            has_ci: false,
            framework: None,
        };
        let cmds = recommended_allowed_commands(&info);
        assert!(cmds.contains(&"cargo".to_string()));
        assert!(cmds.contains(&"git".to_string()));
    }

    #[test]
    fn test_example_tasks_rust() {
        let info = ProjectInfo {
            project_type: ProjectType::Rust,
            has_git: true,
            git_clean: true,
            build_commands: vec![],
            test_commands: vec![],
            package_manager: None,
            source_dirs: vec![],
            has_ci: false,
            framework: None,
        };
        let tasks = example_tasks(&info);
        assert!(!tasks.is_empty());
    }

    #[test]
    fn test_detect_rust_framework_axum() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[dependencies]\naxum = \"0.7\"",
        )
        .unwrap();

        let info = detect_project(dir.path());
        assert_eq!(info.framework, Some("Axum".to_string()));
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(ProjectType::Rust.to_string(), "Rust");
        assert_eq!(ProjectType::Node.to_string(), "Node.js");
        assert_eq!(ProjectType::Unknown.to_string(), "Unknown");
        assert_eq!(
            ProjectType::Mixed(vec![ProjectType::Rust, ProjectType::Node]).to_string(),
            "Mixed (Rust, Node.js)"
        );
    }
}
